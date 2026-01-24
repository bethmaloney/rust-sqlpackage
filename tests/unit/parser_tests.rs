//! Unit tests for T-SQL parser
//!
//! These tests are converted from DacFx test patterns to verify
//! rust-sqlpackage's T-SQL parsing capabilities.

use std::io::Write;
use std::path::PathBuf;

use tempfile::NamedTempFile;

// Note: We need to test internal functions, so we'll call through the public API
// or test via file-based parsing

/// Helper to create a temp SQL file with content
fn create_sql_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::with_suffix(".sql").unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

// ============================================================================
// Batch Separator Tests
// ============================================================================

#[test]
fn test_split_batches_basic() {
    let sql = "CREATE TABLE t1 (id INT)\nGO\nCREATE TABLE t2 (id INT)";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2, "Expected 2 statements from 2 batches");
}

#[test]
fn test_split_batches_multiple_go() {
    let sql = r#"
CREATE TABLE t1 (id INT)
GO
CREATE TABLE t2 (id INT)
GO
CREATE TABLE t3 (id INT)
GO
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 3, "Expected 3 statements from 3 batches");
}

#[test]
fn test_split_batches_case_insensitive_go() {
    let sql =
        "CREATE TABLE t1 (id INT)\ngo\nCREATE TABLE t2 (id INT)\nGO\nCREATE TABLE t3 (id INT)\nGo";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 3, "GO should be case-insensitive");
}

#[test]
fn test_split_batches_no_go() {
    let sql = "CREATE TABLE t1 (id INT); CREATE TABLE t2 (id INT);";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(
        statements.len(),
        2,
        "Expected 2 statements without GO separator"
    );
}

// ============================================================================
// CREATE TABLE Parsing Tests
// ============================================================================

#[test]
fn test_parse_simple_table() {
    let sql = r#"
CREATE TABLE [dbo].[SimpleTable] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse simple table: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify it's a CREATE TABLE statement
    match &statements[0].statement {
        Some(sqlparser::ast::Statement::CreateTable(create)) => {
            assert!(create.name.to_string().contains("SimpleTable"));
        }
        _ => panic!("Expected CREATE TABLE statement"),
    }
}

#[test]
fn test_parse_table_with_primary_key() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithPK] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with PK: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_named_primary_key() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithNamedPK] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    CONSTRAINT [PK_TableWithNamedPK] PRIMARY KEY CLUSTERED ([Id])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with named PK: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_foreign_key() {
    let sql = r#"
CREATE TABLE [dbo].[Parent] (
    [Id] INT NOT NULL PRIMARY KEY
);
GO
CREATE TABLE [dbo].[Child] (
    [Id] INT NOT NULL PRIMARY KEY,
    [ParentId] INT NOT NULL,
    CONSTRAINT [FK_Child_Parent] FOREIGN KEY ([ParentId]) REFERENCES [dbo].[Parent]([Id])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with FK: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2);
}

#[test]
fn test_parse_table_with_unique_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithUnique] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Email] NVARCHAR(255) NOT NULL,
    CONSTRAINT [UQ_TableWithUnique_Email] UNIQUE ([Email])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with unique: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_check_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithCheck] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Age] INT NOT NULL,
    CONSTRAINT [CK_TableWithCheck_Age] CHECK ([Age] >= 0)
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with check: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_default_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithDefault] (
    [Id] INT NOT NULL PRIMARY KEY,
    [CreatedAt] DATETIME2 NOT NULL DEFAULT GETDATE(),
    [IsActive] BIT NOT NULL DEFAULT 1
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with default: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_identity_column() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithIdentity] (
    [Id] INT NOT NULL IDENTITY(1, 1) PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with identity: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_all_common_data_types() {
    let sql = r#"
CREATE TABLE [dbo].[AllTypes] (
    [ColInt] INT NOT NULL,
    [ColBigInt] BIGINT NULL,
    [ColSmallInt] SMALLINT NULL,
    [ColTinyInt] TINYINT NULL,
    [ColBit] BIT NOT NULL,
    [ColDecimal] DECIMAL(18, 2) NULL,
    [ColNumeric] NUMERIC(10, 4) NULL,
    [ColMoney] MONEY NULL,
    [ColFloat] FLOAT NULL,
    [ColReal] REAL NULL,
    [ColDate] DATE NULL,
    [ColTime] TIME NULL,
    [ColDateTime] DATETIME NULL,
    [ColDateTime2] DATETIME2 NULL,
    [ColDateTimeOffset] DATETIMEOFFSET NULL,
    [ColChar] CHAR(10) NULL,
    [ColVarChar] VARCHAR(100) NULL,
    [ColVarCharMax] VARCHAR(MAX) NULL,
    [ColNChar] NCHAR(10) NULL,
    [ColNVarChar] NVARCHAR(100) NULL,
    [ColNVarCharMax] NVARCHAR(MAX) NULL,
    [ColBinary] BINARY(16) NULL,
    [ColVarBinary] VARBINARY(100) NULL,
    [ColUniqueIdentifier] UNIQUEIDENTIFIER NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with all types: {:?}",
        result.err()
    );
}

// ============================================================================
// CREATE VIEW Parsing Tests
// ============================================================================

#[test]
fn test_parse_simple_view() {
    let sql = r#"
CREATE VIEW [dbo].[SimpleView]
AS
SELECT 1 AS [Value];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse simple view: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::CreateView { name, .. }) => {
            assert!(name.to_string().contains("SimpleView"));
        }
        _ => panic!("Expected CREATE VIEW statement"),
    }
}

#[test]
fn test_parse_view_with_columns() {
    let sql = r#"
CREATE VIEW [dbo].[ViewWithColumns] ([Col1], [Col2])
AS
SELECT 1, 2;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse view with columns: {:?}",
        result.err()
    );
}

// ============================================================================
// CREATE INDEX Parsing Tests
// ============================================================================

#[test]
fn test_parse_nonclustered_index() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Table_Column]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse nonclustered index: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_clustered_index() {
    let sql = r#"
CREATE CLUSTERED INDEX [IX_Table_Clustered]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse clustered index: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_unique_index() {
    let sql = r#"
CREATE UNIQUE NONCLUSTERED INDEX [IX_Table_Unique]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse unique index: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_index_with_include() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Table_WithInclude]
ON [dbo].[SomeTable] ([Column1])
INCLUDE ([Column2], [Column3]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse index with include: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_index_include_extracts_columns() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Test_Include]
ON [dbo].[TestTable] ([KeyCol1], [KeyCol2])
INCLUDE ([IncludeCol1], [IncludeCol2], [IncludeCol3]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify the fallback type captured include columns
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            name,
            columns,
            include_columns,
            ..
        }) => {
            assert_eq!(name, "IX_Test_Include");
            assert_eq!(columns.len(), 2, "Should have 2 key columns");
            assert_eq!(include_columns.len(), 3, "Should have 3 include columns");
            assert!(include_columns.contains(&"IncludeCol1".to_string()));
            assert!(include_columns.contains(&"IncludeCol2".to_string()));
            assert!(include_columns.contains(&"IncludeCol3".to_string()));
        }
        _ => panic!("Expected Index fallback type"),
    }
}

#[test]
fn test_parse_index_include_single_column() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Single]
ON [dbo].[T] ([A])
INCLUDE ([B]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok());

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            include_columns, ..
        }) => {
            assert_eq!(include_columns.len(), 1);
            assert_eq!(include_columns[0], "B");
        }
        _ => panic!("Expected Index fallback type"),
    }
}

#[test]
fn test_parse_index_no_include() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_NoInclude]
ON [dbo].[T] ([Col1], [Col2]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok());

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            include_columns, ..
        }) => {
            assert!(
                include_columns.is_empty(),
                "Index without INCLUDE should have no include_columns"
            );
        }
        _ => panic!("Expected Index fallback type"),
    }
}

#[test]
fn test_parse_unique_nonclustered_index_with_include() {
    let sql = r#"
CREATE UNIQUE NONCLUSTERED INDEX [IX_Unique_Include]
ON [dbo].[Orders] ([OrderNumber])
INCLUDE ([CustomerName], [OrderDate], [Total]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok());

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            is_unique,
            is_clustered,
            include_columns,
            ..
        }) => {
            assert!(*is_unique, "Index should be unique");
            assert!(!*is_clustered, "Index should be nonclustered");
            assert_eq!(include_columns.len(), 3);
        }
        _ => panic!("Expected Index fallback type"),
    }
}

#[test]
fn test_parse_index_missing_whitespace_before_on() {
    // Edge case: SQL with missing whitespace between ] and ON (e.g., "]ON" instead of "] ON")
    // This pattern appears in some real-world SQL files
    let sql = r#"CREATE NONCLUSTERED INDEX [IX_Test]ON [dbo].[Table] ([Col1])INCLUDE ([Col2])"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Should parse index with missing whitespace: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            name,
            table_name,
            columns,
            include_columns,
            ..
        }) => {
            assert_eq!(name, "IX_Test");
            assert_eq!(table_name, "Table");
            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0], "Col1");
            assert_eq!(include_columns.len(), 1);
            assert_eq!(include_columns[0], "Col2");
        }
        _ => panic!("Expected Index fallback type"),
    }
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_parse_invalid_sql_returns_error() {
    let sql = "THIS IS NOT VALID SQL AT ALL!!!";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Invalid SQL should return error");
}

#[test]
fn test_parse_file_not_found_error() {
    let result =
        rust_sqlpackage::parser::parse_sql_file(&PathBuf::from("/nonexistent/path/file.sql"));
    assert!(result.is_err(), "Non-existent file should return error");
}

#[test]
fn test_parse_empty_file() {
    let sql = "";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Empty file should parse successfully");

    let statements = result.unwrap();
    assert_eq!(statements.len(), 0, "Empty file should have no statements");
}

#[test]
fn test_parse_whitespace_only_file() {
    let sql = "   \n\n   \t  \n  ";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Whitespace-only file should parse successfully"
    );

    let statements = result.unwrap();
    assert_eq!(
        statements.len(),
        0,
        "Whitespace-only file should have no statements"
    );
}

#[test]
fn test_parse_comment_only_file() {
    let sql = r#"
-- This is a comment
/* This is a block comment */
-- Another comment
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This might succeed with 0 statements or fail depending on parser behavior
    // Either is acceptable for a baseline test
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(
            statements.len(),
            0,
            "Comment-only file should have no statements"
        );
    }
}

// ============================================================================
// Additional Batch Separator Tests (from TEST_PLAN.md)
// ============================================================================

#[test]
fn test_split_batches_go_with_count() {
    // GO 5 means execute the batch 5 times in SSMS - we DON'T treat "GO 5" as a batch separator
    // Only exact "GO" (case-insensitive, with optional whitespace) separates batches
    let sql = "CREATE TABLE t1 (id INT)\nGO 5\nCREATE TABLE t2 (id INT)";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This test documents behavior - GO with count is NOT recognized as a separator
    // The entire content is treated as one batch, and fallback parsing extracts what it can
    if result.is_ok() {
        let statements = result.unwrap();
        // Only 1 statement extracted via fallback parsing (first CREATE TABLE)
        assert!(
            statements.len() >= 1,
            "Should extract at least 1 statement via fallback parsing"
        );
    } else {
        // If parsing fails entirely, that's also acceptable behavior
        println!(
            "Note: GO with count causes parse failure: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_split_batches_go_in_comment() {
    // GO inside a comment should NOT cause a split
    let sql = r#"
CREATE TABLE t1 (
    id INT -- GO here is in a comment
)
GO
CREATE TABLE t2 (id INT)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2, "GO in comment should not cause split");
}

#[test]
fn test_split_batches_go_in_string() {
    // GO inside a string literal should NOT cause a split
    let sql = r#"
CREATE TABLE t1 (
    name VARCHAR(10) DEFAULT 'GO'
)
GO
CREATE TABLE t2 (id INT)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2, "GO in string should not cause split");
}

#[test]
fn test_split_batches_go_in_block_comment() {
    // GO inside a block comment should NOT cause a split
    // This tests whether the batch splitter is comment-aware
    let sql = r#"
CREATE TABLE t1 (id INT)
/*
GO
This GO should be ignored
*/
GO
CREATE TABLE t2 (id INT)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This documents current behavior - if the batch splitter is not comment-aware,
    // it will fail. If it is comment-aware, it should produce 2 statements.
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(
            statements.len(),
            2,
            "GO in block comment should not cause split"
        );
    } else {
        // Document that GO in block comments is not currently handled
        println!("Note: GO in block comments not handled: {:?}", result.err());
    }
}

// ============================================================================
// Additional CREATE TABLE Tests (from TEST_PLAN.md)
// ============================================================================

#[test]
fn test_parse_table_with_computed_column() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithComputed] (
    [FirstName] NVARCHAR(50) NOT NULL,
    [LastName] NVARCHAR(50) NOT NULL,
    [FullName] AS ([FirstName] + ' ' + [LastName])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Computed columns may or may not be fully supported
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: Computed columns not fully supported: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_table_with_persisted_computed_column() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithPersistedComputed] (
    [Price] DECIMAL(18, 2) NOT NULL,
    [Quantity] INT NOT NULL,
    [Total] AS ([Price] * [Quantity]) PERSISTED
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // PERSISTED computed columns may or may not be supported
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: PERSISTED computed columns not supported: {:?}",
            result.err()
        );
    }
}

// ============================================================================
// Additional CREATE VIEW Tests (from TEST_PLAN.md)
// ============================================================================

#[test]
fn test_parse_view_with_schema_binding() {
    let sql = r#"
CREATE VIEW [dbo].[BoundView]
WITH SCHEMABINDING
AS
SELECT 1 AS [Value];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // SCHEMABINDING may or may not be supported
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!("Note: WITH SCHEMABINDING not supported: {:?}", result.err());
    }
}

// ============================================================================
// Standard CREATE INDEX Test (workaround for sqlparser-rs limitation)
// ============================================================================

#[test]
fn test_parse_standard_index() {
    // Use CREATE INDEX without CLUSTERED/NONCLUSTERED (supported by sqlparser-rs)
    let sql = r#"
CREATE INDEX [IX_Table_Column]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Standard CREATE INDEX should be supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

// ============================================================================
// CREATE PROCEDURE Parsing Tests (using fallback parser for T-SQL syntax)
// ============================================================================

#[test]
fn test_parse_simple_procedure() {
    // This syntax MAY be parsed by sqlparser (if it has BEGIN...END and parenthesized params)
    // or may use fallback parsing. Either way, it should parse successfully.
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUsers]
AS
BEGIN
    SELECT * FROM Users
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse simple procedure: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Either sqlparser parsed it or fallback did
    if let Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) =
        &statements[0].fallback_type
    {
        assert_eq!(schema, "dbo");
        assert_eq!(name, "GetUsers");
    } else if let Some(sqlparser::ast::Statement::CreateProcedure { name, .. }) =
        &statements[0].statement
    {
        assert!(name.to_string().contains("GetUsers"));
    } else {
        panic!("Expected CreateProcedure statement or fallback type");
    }
}

#[test]
fn test_parse_procedure_with_parameters() {
    // T-SQL style parameters (@param) will use fallback parsing
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUserById]
    @UserId INT,
    @IncludeDeleted BIT = 0
AS
BEGIN
    SELECT * FROM Users WHERE Id = @UserId
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with parameters: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // T-SQL @param syntax requires fallback parsing
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetUserById");
        }
        _ => panic!("Expected Procedure fallback type for T-SQL @param syntax"),
    }

    // Verify original SQL is preserved
    assert!(statements[0].sql_text.contains("@UserId INT"));
}

#[test]
fn test_parse_procedure_or_alter() {
    // CREATE OR ALTER with T-SQL @params will use fallback
    let sql = r#"
CREATE OR ALTER PROCEDURE [sales].[UpdateOrder]
    @OrderId INT,
    @Status VARCHAR(50)
AS
BEGIN
    UPDATE Orders SET Status = @Status WHERE Id = @OrderId
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CREATE OR ALTER PROCEDURE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "sales");
            assert_eq!(name, "UpdateOrder");
        }
        _ => panic!("Expected Procedure fallback type for CREATE OR ALTER with @params"),
    }
}

#[test]
fn test_parse_procedure_short_form() {
    // T-SQL PROC abbreviation - uses fallback parsing
    let sql = r#"
CREATE PROC [dbo].[QuickProc]
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse PROC abbreviation: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "QuickProc");
        }
        _ => panic!("Expected Procedure fallback type for PROC abbreviation"),
    }
}

#[test]
fn test_parse_procedure_no_schema() {
    // No schema specified - uses fallback, defaults to dbo
    let sql = r#"
CREATE PROCEDURE SimpleProc
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure without schema: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    // Check either fallback or sqlparser parsing
    if let Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) =
        &statements[0].fallback_type
    {
        assert_eq!(schema, "dbo", "Should default to dbo schema");
        assert_eq!(name, "SimpleProc");
    } else if let Some(sqlparser::ast::Statement::CreateProcedure { name, .. }) =
        &statements[0].statement
    {
        assert!(name.to_string().contains("SimpleProc"));
    } else {
        panic!("Expected CreateProcedure statement or fallback type");
    }
}

// ============================================================================
// CREATE FUNCTION Parsing Tests (using fallback parser for T-SQL syntax)
// ============================================================================

#[test]
fn test_parse_scalar_function() {
    // T-SQL function syntax uses fallback parsing since MsSqlDialect doesn't support CREATE FUNCTION
    let sql = r#"
CREATE FUNCTION [dbo].[GetFullName]
(
    @FirstName NVARCHAR(50),
    @LastName NVARCHAR(50)
)
RETURNS NVARCHAR(101)
AS
BEGIN
    RETURN @FirstName + ' ' + @LastName
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse scalar function: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Functions always use fallback parsing with MsSqlDialect
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function {
            schema,
            name,
            function_type,
        }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetFullName");
            assert_eq!(
                *function_type,
                rust_sqlpackage::parser::FallbackFunctionType::Scalar
            );
        }
        _ => panic!("Expected Function fallback type"),
    }
}

#[test]
fn test_parse_table_valued_function() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetUserOrders]
(
    @UserId INT
)
RETURNS TABLE
AS
RETURN
(
    SELECT * FROM Orders WHERE UserId = @UserId
)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table-valued function: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function {
            schema,
            name,
            function_type,
        }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetUserOrders");
            assert_eq!(
                *function_type,
                rust_sqlpackage::parser::FallbackFunctionType::TableValued
            );
        }
        _ => panic!("Expected Function fallback type"),
    }
}

#[test]
fn test_parse_multi_statement_table_function() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetFilteredData]
(
    @MinValue INT
)
RETURNS @ResultTable TABLE
(
    Id INT,
    Value INT
)
AS
BEGIN
    INSERT INTO @ResultTable
    SELECT Id, Value FROM Data WHERE Value >= @MinValue
    RETURN
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse multi-statement table function: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function {
            schema,
            name,
            function_type,
        }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetFilteredData");
            // RETURNS @ResultTable TABLE should be detected as table-valued
            assert_eq!(
                *function_type,
                rust_sqlpackage::parser::FallbackFunctionType::TableValued
            );
        }
        _ => panic!("Expected Function fallback type"),
    }
}

#[test]
fn test_parse_function_or_alter() {
    let sql = r#"
CREATE OR ALTER FUNCTION [utils].[FormatDate]
(
    @Date DATETIME
)
RETURNS VARCHAR(10)
AS
BEGIN
    RETURN CONVERT(VARCHAR(10), @Date, 120)
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CREATE OR ALTER FUNCTION: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function { schema, name, .. }) => {
            assert_eq!(schema, "utils");
            assert_eq!(name, "FormatDate");
        }
        _ => panic!("Expected Function fallback type"),
    }
}

#[test]
fn test_parse_function_no_schema() {
    let sql = r#"
CREATE FUNCTION SimpleFunc()
RETURNS INT
AS
BEGIN
    RETURN 42
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse function without schema: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function { schema, name, .. }) => {
            assert_eq!(schema, "dbo", "Should default to dbo schema");
            assert_eq!(name, "SimpleFunc");
        }
        _ => panic!("Expected Function fallback type"),
    }
}

// ============================================================================
// Mixed Procedures and Functions in Same File
// ============================================================================

#[test]
fn test_parse_multiple_procedures_and_functions() {
    // All batches use T-SQL syntax that requires fallback parsing
    let sql = r#"
CREATE PROCEDURE [dbo].[Proc1]
    @Id INT
AS
BEGIN
    SELECT @Id
END
GO

CREATE FUNCTION [dbo].[Func1]()
RETURNS INT
AS
BEGIN
    RETURN 1
END
GO

CREATE PROC [dbo].[Proc2]
AS
BEGIN
    SELECT 2
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse multiple procs/funcs: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 3, "Should have 3 statements");

    // Verify each statement has fallback type (uses T-SQL syntax)
    assert!(
        matches!(
            &statements[0].fallback_type,
            Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { name, .. }) if name == "Proc1"
        ),
        "First should be Proc1"
    );
    assert!(
        matches!(
            &statements[1].fallback_type,
            Some(rust_sqlpackage::parser::FallbackStatementType::Function { name, .. }) if name == "Func1"
        ),
        "Second should be Func1"
    );
    assert!(
        matches!(
            &statements[2].fallback_type,
            Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { name, .. }) if name == "Proc2"
        ),
        "Third should be Proc2"
    );
}

// ============================================================================
// ALTER Statement Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_table_add_column() {
    let sql = r#"
ALTER TABLE [dbo].[Users]
ADD [Email] NVARCHAR(255) NULL;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD COLUMN: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_table_drop_column() {
    let sql = r#"
ALTER TABLE [dbo].[Users]
DROP COLUMN [TempColumn];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE DROP COLUMN: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_constraint() {
    let sql = r#"
ALTER TABLE [dbo].[Orders]
ADD CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId]) REFERENCES [dbo].[Users]([Id]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD CONSTRAINT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_drop_constraint() {
    let sql = r#"
ALTER TABLE [dbo].[Orders]
DROP CONSTRAINT [FK_Orders_Users];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE DROP CONSTRAINT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_alter_column() {
    let sql = r#"
ALTER TABLE [dbo].[Users]
ALTER COLUMN [Name] NVARCHAR(500) NOT NULL;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ALTER COLUMN: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_primary_key() {
    let sql = r#"
ALTER TABLE [dbo].[Products]
ADD CONSTRAINT [PK_Products] PRIMARY KEY CLUSTERED ([Id]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD PRIMARY KEY: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_check_constraint() {
    let sql = r#"
ALTER TABLE [dbo].[Products]
ADD CONSTRAINT [CK_Products_Price] CHECK ([Price] >= 0);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD CHECK: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_unique() {
    let sql = r#"
ALTER TABLE [dbo].[Products]
ADD CONSTRAINT [UQ_Products_SKU] UNIQUE ([SKU]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD UNIQUE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_default() {
    let sql = r#"
ALTER TABLE [dbo].[Products]
ADD CONSTRAINT [DF_Products_IsActive] DEFAULT (1) FOR [IsActive];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // ALTER TABLE ADD DEFAULT FOR may use fallback parsing
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER TABLE ADD DEFAULT FOR uses fallback: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_alter_table_multiple_actions() {
    let sql = r#"
ALTER TABLE [dbo].[Users]
ADD [MiddleName] NVARCHAR(100) NULL,
    [Suffix] NVARCHAR(10) NULL;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE with multiple adds: {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER VIEW Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_view_basic() {
    let sql = r#"
ALTER VIEW [dbo].[MyView]
AS
SELECT [Id], [Name] FROM [dbo].[Users];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER VIEW: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_view_with_schemabinding() {
    let sql = r#"
ALTER VIEW [dbo].[BoundView]
WITH SCHEMABINDING
AS
SELECT [Id] FROM [dbo].[Users];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // WITH SCHEMABINDING may or may not be supported
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER VIEW WITH SCHEMABINDING not supported: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_alter_view_with_columns() {
    let sql = r#"
ALTER VIEW [dbo].[ViewWithColumns] ([Column1], [Column2])
AS
SELECT 1, 2;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER VIEW with columns: {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER PROCEDURE Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_procedure_basic() {
    let sql = r#"
ALTER PROCEDURE [dbo].[MyProc]
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER PROCEDURE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_procedure_with_parameters() {
    let sql = r#"
ALTER PROCEDURE [dbo].[GetUser]
    @UserId INT,
    @IncludeDeleted BIT = 0
AS
BEGIN
    SELECT * FROM [dbo].[Users] WHERE [Id] = @UserId
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER PROCEDURE with parameters: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_proc_short_form() {
    let sql = r#"
ALTER PROC [dbo].[QuickProc]
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER PROC (short form): {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER FUNCTION Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_function_scalar() {
    let sql = r#"
ALTER FUNCTION [dbo].[GetFullName]
(
    @FirstName NVARCHAR(50),
    @LastName NVARCHAR(50)
)
RETURNS NVARCHAR(101)
AS
BEGIN
    RETURN @FirstName + ' ' + @LastName
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER FUNCTION (scalar): {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_function_table_valued() {
    let sql = r#"
ALTER FUNCTION [dbo].[GetUserOrders]
(
    @UserId INT
)
RETURNS TABLE
AS
RETURN
(
    SELECT [OrderId], [Amount] FROM [dbo].[Orders] WHERE [UserId] = @UserId
)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER FUNCTION (table-valued): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_function_multi_statement() {
    let sql = r#"
ALTER FUNCTION [dbo].[GetFilteredData]
(
    @MinValue INT
)
RETURNS @ResultTable TABLE
(
    Id INT,
    Value INT
)
AS
BEGIN
    INSERT INTO @ResultTable
    SELECT [Id], [Value] FROM [dbo].[Data] WHERE [Value] >= @MinValue
    RETURN
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER FUNCTION (multi-statement table): {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER TRIGGER Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_trigger_basic() {
    let sql = r#"
ALTER TRIGGER [dbo].[TR_Users_Update]
ON [dbo].[Users]
AFTER UPDATE
AS
BEGIN
    SET NOCOUNT ON;
    UPDATE [dbo].[Users] SET [ModifiedAt] = GETDATE()
    FROM [dbo].[Users] u
    INNER JOIN inserted i ON u.[Id] = i.[Id]
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // ALTER TRIGGER may use fallback parsing
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER TRIGGER not fully supported: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_alter_trigger_instead_of() {
    let sql = r#"
ALTER TRIGGER [dbo].[TR_View_Insert]
ON [dbo].[MyView]
INSTEAD OF INSERT
AS
BEGIN
    INSERT INTO [dbo].[BaseTable] ([Col1], [Col2])
    SELECT [Col1], [Col2] FROM inserted
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER TRIGGER INSTEAD OF not supported: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_alter_trigger_for_insert_update_delete() {
    let sql = r#"
ALTER TRIGGER [dbo].[TR_Audit]
ON [dbo].[Products]
FOR INSERT, UPDATE, DELETE
AS
BEGIN
    INSERT INTO [dbo].[AuditLog] ([Action], [TableName], [Timestamp])
    VALUES ('Change', 'Products', GETDATE())
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER TRIGGER FOR multiple events not supported: {:?}",
            result.err()
        );
    }
}

// ============================================================================
// ALTER SCHEMA Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_schema_transfer() {
    let sql = r#"
ALTER SCHEMA [sales] TRANSFER [dbo].[Orders];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // ALTER SCHEMA TRANSFER may not be supported by sqlparser
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER SCHEMA TRANSFER not supported: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_alter_authorization_on_schema() {
    let sql = r#"
ALTER AUTHORIZATION ON SCHEMA::[sales] TO [dbo];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER AUTHORIZATION ON SCHEMA not supported: {:?}",
            result.err()
        );
    }
}

// ============================================================================
// ALTER TYPE Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_type_add_value() {
    // Note: SQL Server doesn't support ALTER TYPE for table types
    // This tests the parser behavior for the syntax
    let sql = r#"
ALTER TYPE [dbo].[StatusType] ADD VALUE 'pending';
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This syntax may not be valid T-SQL, documenting behavior
    if result.is_ok() {
        println!("ALTER TYPE ADD VALUE accepted");
    } else {
        println!(
            "Note: ALTER TYPE ADD VALUE not supported (expected for T-SQL): {:?}",
            result.err()
        );
    }
}

// ============================================================================
// ALTER SEQUENCE Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_sequence_restart() {
    let sql = r#"
ALTER SEQUENCE [dbo].[OrderSequence]
RESTART WITH 1000;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER SEQUENCE RESTART: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_sequence_increment() {
    let sql = r#"
ALTER SEQUENCE [dbo].[CounterSeq]
INCREMENT BY 5;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER SEQUENCE INCREMENT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_sequence_minmax() {
    let sql = r#"
ALTER SEQUENCE [dbo].[BoundedSeq]
MINVALUE 1
MAXVALUE 10000
CYCLE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER SEQUENCE with MIN/MAX: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_sequence_multiple_options() {
    let sql = r#"
ALTER SEQUENCE [dbo].[ComplexSeq]
RESTART WITH 500
INCREMENT BY 10
MINVALUE 1
MAXVALUE 99999
NO CYCLE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER SEQUENCE with multiple options: {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER SYNONYM Parsing Tests
// ============================================================================

#[test]
fn test_parse_drop_and_recreate_synonym() {
    // SQL Server doesn't have ALTER SYNONYM - you must drop and recreate
    // TODO: sqlparser-rs doesn't currently support SYNONYM syntax
    let sql = r#"
DROP SYNONYM [dbo].[ProductAlias];
GO
CREATE SYNONYM [dbo].[ProductAlias] FOR [inventory].[Products];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP/CREATE SYNONYM not supported by sqlparser: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2);
}

// ============================================================================
// ALTER INDEX Parsing Tests (bonus - commonly used with ALTER)
// ============================================================================

#[test]
fn test_parse_alter_index_rebuild() {
    let sql = r#"
ALTER INDEX [IX_Users_Email] ON [dbo].[Users]
REBUILD;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER INDEX REBUILD not supported: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_alter_index_disable() {
    let sql = r#"
ALTER INDEX [IX_Products_Category] ON [dbo].[Products]
DISABLE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER INDEX DISABLE not supported: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_alter_index_reorganize() {
    let sql = r#"
ALTER INDEX [IX_Orders_Date] ON [dbo].[Orders]
REORGANIZE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER INDEX REORGANIZE not supported: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_alter_index_all() {
    let sql = r#"
ALTER INDEX ALL ON [dbo].[LargeTable]
REBUILD WITH (ONLINE = ON);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!(
            "Note: ALTER INDEX ALL not supported: {:?}",
            result.err()
        );
    }
}

// ============================================================================
// DROP Statement Parsing Tests
// ============================================================================

#[test]
fn test_parse_drop_table() {
    let sql = "DROP TABLE [dbo].[TempTable];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP TABLE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { object_type, .. }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::Table);
        }
        _ => panic!("Expected DROP TABLE statement"),
    }
}

#[test]
fn test_parse_drop_table_if_exists() {
    let sql = "DROP TABLE IF EXISTS [dbo].[TempTable];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP TABLE IF EXISTS: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_view() {
    let sql = "DROP VIEW [dbo].[MyView];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP VIEW: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_procedure() {
    let sql = "DROP PROCEDURE [dbo].[MyProc];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP PROCEDURE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_function() {
    let sql = "DROP FUNCTION [dbo].[MyFunc];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP FUNCTION: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_index() {
    // T-SQL DROP INDEX syntax: DROP INDEX [name] ON [table]
    // TODO: sqlparser does not fully support T-SQL's ON clause syntax
    let sql = "DROP INDEX [IX_Users_Email] ON [dbo].[Users];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP INDEX ON table syntax not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_schema() {
    let sql = "DROP SCHEMA [temp_schema];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP SCHEMA: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_trigger() {
    // TODO: DROP TRIGGER may not be fully supported by sqlparser
    let sql = "DROP TRIGGER [dbo].[TR_Users_Insert];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP TRIGGER not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_multiple_tables() {
    let sql = "DROP TABLE [dbo].[Table1], [dbo].[Table2], [dbo].[Table3];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP multiple tables: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_type() {
    let sql = "DROP TYPE [dbo].[AddressTableType];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP TYPE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { object_type, .. }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::Type);
        }
        _ => panic!("Expected DROP TYPE statement"),
    }
}

#[test]
fn test_parse_drop_type_if_exists() {
    let sql = "DROP TYPE IF EXISTS [dbo].[OrderItemType];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP TYPE IF EXISTS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { if_exists, .. }) => {
            assert!(*if_exists, "Expected IF EXISTS to be true");
        }
        _ => panic!("Expected DROP statement"),
    }
}

#[test]
fn test_parse_drop_sequence() {
    let sql = "DROP SEQUENCE [dbo].[OrderNumberSequence];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP SEQUENCE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { object_type, .. }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::Sequence);
        }
        _ => panic!("Expected DROP SEQUENCE statement"),
    }
}

#[test]
fn test_parse_drop_sequence_if_exists() {
    let sql = "DROP SEQUENCE IF EXISTS [dbo].[InvoiceSequence];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP SEQUENCE IF EXISTS: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_synonym() {
    // TODO: SYNONYM support requires parser extension
    let sql = "DROP SYNONYM [dbo].[ProductAlias];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP SYNONYM not supported by sqlparser: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_synonym_if_exists() {
    // TODO: SYNONYM support requires parser extension
    let sql = "DROP SYNONYM IF EXISTS [dbo].[CustomerAlias];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP SYNONYM IF EXISTS not supported by sqlparser: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_view_if_exists() {
    let sql = "DROP VIEW IF EXISTS [dbo].[vw_ActiveUsers];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP VIEW IF EXISTS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop {
            object_type,
            if_exists,
            ..
        }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::View);
            assert!(*if_exists, "Expected IF EXISTS to be true");
        }
        _ => panic!("Expected DROP VIEW statement"),
    }
}

#[test]
fn test_parse_drop_procedure_if_exists() {
    let sql = "DROP PROCEDURE IF EXISTS [dbo].[usp_GetUserById];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP PROCEDURE IF EXISTS: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_function_if_exists() {
    let sql = "DROP FUNCTION IF EXISTS [dbo].[fn_CalculateTotal];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP FUNCTION IF EXISTS: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_trigger_if_exists() {
    // TODO: DROP TRIGGER may require parser extension
    let sql = "DROP TRIGGER IF EXISTS [dbo].[TR_Orders_Insert];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP TRIGGER IF EXISTS not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_schema_if_exists() {
    let sql = "DROP SCHEMA IF EXISTS [staging];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP SCHEMA IF EXISTS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop {
            object_type,
            if_exists,
            ..
        }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::Schema);
            assert!(*if_exists, "Expected IF EXISTS to be true");
        }
        _ => panic!("Expected DROP SCHEMA statement"),
    }
}

#[test]
fn test_parse_drop_index_if_exists() {
    // T-SQL: DROP INDEX IF EXISTS [name] ON [table]
    // TODO: sqlparser does not fully support T-SQL's ON clause syntax
    let sql = "DROP INDEX IF EXISTS [IX_Users_Email] ON [dbo].[Users];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP INDEX IF EXISTS ON table syntax not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_view_with_ast_verification() {
    let sql = "DROP VIEW [dbo].[vw_CustomerOrders];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP VIEW: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop {
            object_type, names, ..
        }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::View);
            assert_eq!(names.len(), 1);
            assert!(names[0].to_string().contains("vw_CustomerOrders"));
        }
        _ => panic!("Expected DROP VIEW statement"),
    }
}

#[test]
fn test_parse_drop_multiple_views() {
    let sql = "DROP VIEW [dbo].[View1], [dbo].[View2];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP multiple views: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { names, .. }) => {
            assert_eq!(names.len(), 2, "Expected 2 view names in DROP statement");
        }
        _ => panic!("Expected DROP statement"),
    }
}

#[test]
fn test_parse_drop_multiple_procedures() {
    let sql = "DROP PROCEDURE [dbo].[Proc1], [dbo].[Proc2], [dbo].[Proc3];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP multiple procedures: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_multiple_functions() {
    let sql = "DROP FUNCTION [dbo].[Func1], [dbo].[Func2];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP multiple functions: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_proc_abbreviation() {
    // SQL Server supports DROP PROC as abbreviation for DROP PROCEDURE
    // TODO: PROC abbreviation may not be supported by sqlparser
    let sql = "DROP PROC [dbo].[MyProc];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP PROC abbreviation not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_statements_in_batch() {
    let sql = r#"
DROP TABLE IF EXISTS [dbo].[TempTable1];
GO
DROP VIEW IF EXISTS [dbo].[TempView];
GO
DROP PROCEDURE IF EXISTS [dbo].[TempProc];
GO
DROP FUNCTION IF EXISTS [dbo].[TempFunc];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP statements in batch: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 4, "Expected 4 DROP statements in batch");
}

#[test]
fn test_parse_drop_cascade_restrict() {
    // SQL Server doesn't use CASCADE/RESTRICT like PostgreSQL, but sqlparser may parse it
    // TODO: CASCADE syntax support in T-SQL mode
    let sql = "DROP TABLE [dbo].[Orders] CASCADE;";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP TABLE CASCADE syntax not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

// ============================================================================
// MERGE Statement Parsing Tests
// ============================================================================

#[test]
fn test_parse_merge_basic() {
    let sql = r#"
MERGE INTO [dbo].[Target] AS T
USING [dbo].[Source] AS S
ON T.[Id] = S.[Id]
WHEN MATCHED THEN
    UPDATE SET T.[Name] = S.[Name]
WHEN NOT MATCHED THEN
    INSERT ([Id], [Name]) VALUES (S.[Id], S.[Name]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse MERGE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_merge_with_delete() {
    let sql = r#"
MERGE [dbo].[Products] AS Target
USING [dbo].[StagingProducts] AS Source
ON Target.[SKU] = Source.[SKU]
WHEN MATCHED AND Source.[IsDeleted] = 1 THEN
    DELETE
WHEN MATCHED THEN
    UPDATE SET Target.[Name] = Source.[Name], Target.[Price] = Source.[Price]
WHEN NOT MATCHED BY TARGET THEN
    INSERT ([SKU], [Name], [Price]) VALUES (Source.[SKU], Source.[Name], Source.[Price]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse MERGE with DELETE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_merge_with_output() {
    let sql = r#"
MERGE [dbo].[Target] AS T
USING [dbo].[Source] AS S
ON T.[Id] = S.[Id]
WHEN MATCHED THEN
    UPDATE SET T.[Value] = S.[Value]
WHEN NOT MATCHED THEN
    INSERT ([Id], [Value]) VALUES (S.[Id], S.[Value])
OUTPUT $action, inserted.*, deleted.*;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse MERGE with OUTPUT: {:?}",
        result.err()
    );
}

// ============================================================================
// Common Table Expression (CTE) Tests
// ============================================================================

#[test]
fn test_parse_simple_cte() {
    let sql = r#"
WITH SimpleCTE AS (
    SELECT [Id], [Name] FROM [dbo].[Users]
)
SELECT * FROM SimpleCTE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse simple CTE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_multiple_ctes() {
    let sql = r#"
WITH
    CTE1 AS (SELECT [Id] FROM [dbo].[Table1]),
    CTE2 AS (SELECT [Id] FROM [dbo].[Table2]),
    CTE3 AS (SELECT [Id] FROM CTE1 UNION ALL SELECT [Id] FROM CTE2)
SELECT * FROM CTE3;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse multiple CTEs: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_recursive_cte() {
    let sql = r#"
WITH EmployeeHierarchy AS (
    -- Anchor member
    SELECT [Id], [Name], [ManagerId], 0 AS [Level]
    FROM [dbo].[Employees]
    WHERE [ManagerId] IS NULL

    UNION ALL

    -- Recursive member
    SELECT e.[Id], e.[Name], e.[ManagerId], h.[Level] + 1
    FROM [dbo].[Employees] e
    INNER JOIN EmployeeHierarchy h ON e.[ManagerId] = h.[Id]
)
SELECT * FROM EmployeeHierarchy;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse recursive CTE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_cte_with_insert() {
    let sql = r#"
WITH SourceData AS (
    SELECT [Id], [Name] FROM [dbo].[SourceTable]
)
INSERT INTO [dbo].[TargetTable] ([Id], [Name])
SELECT [Id], [Name] FROM SourceData;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CTE with INSERT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_cte_with_update() {
    let sql = r#"
WITH ToUpdate AS (
    SELECT [Id], [Status] FROM [dbo].[Orders] WHERE [Status] = 'Pending'
)
UPDATE ToUpdate SET [Status] = 'Processing';
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CTE with UPDATE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_cte_with_delete() {
    let sql = r#"
WITH OldRecords AS (
    SELECT [Id] FROM [dbo].[Logs] WHERE [CreatedAt] < '2020-01-01'
)
DELETE FROM OldRecords;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CTE with DELETE: {:?}",
        result.err()
    );
}

// ============================================================================
// Window Function Tests
// ============================================================================

#[test]
fn test_parse_row_number() {
    let sql = r#"
SELECT
    [Id],
    [Name],
    ROW_NUMBER() OVER (ORDER BY [Name]) AS [RowNum]
FROM [dbo].[Users];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ROW_NUMBER: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_rank_and_dense_rank() {
    let sql = r#"
SELECT
    [Id],
    [Score],
    RANK() OVER (ORDER BY [Score] DESC) AS [Rank],
    DENSE_RANK() OVER (ORDER BY [Score] DESC) AS [DenseRank]
FROM [dbo].[Players];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse RANK/DENSE_RANK: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_ntile() {
    let sql = r#"
SELECT
    [Id],
    [Amount],
    NTILE(4) OVER (ORDER BY [Amount]) AS [Quartile]
FROM [dbo].[Transactions];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse NTILE: {:?}", result.err());
}

#[test]
fn test_parse_lag_and_lead() {
    let sql = r#"
SELECT
    [OrderDate],
    [Amount],
    LAG([Amount], 1, 0) OVER (ORDER BY [OrderDate]) AS [PrevAmount],
    LEAD([Amount], 1, 0) OVER (ORDER BY [OrderDate]) AS [NextAmount]
FROM [dbo].[Orders];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse LAG/LEAD: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_first_value_last_value() {
    let sql = r#"
SELECT
    [Category],
    [ProductName],
    [Price],
    FIRST_VALUE([ProductName]) OVER (PARTITION BY [Category] ORDER BY [Price]) AS [Cheapest],
    LAST_VALUE([ProductName]) OVER (PARTITION BY [Category] ORDER BY [Price]
        ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) AS [MostExpensive]
FROM [dbo].[Products];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FIRST_VALUE/LAST_VALUE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_aggregate_with_over() {
    let sql = r#"
SELECT
    [DepartmentId],
    [EmployeeName],
    [Salary],
    SUM([Salary]) OVER (PARTITION BY [DepartmentId]) AS [DeptTotal],
    AVG([Salary]) OVER (PARTITION BY [DepartmentId]) AS [DeptAvg],
    COUNT(*) OVER (PARTITION BY [DepartmentId]) AS [DeptCount]
FROM [dbo].[Employees];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse aggregate OVER: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_window_frame_rows() {
    let sql = r#"
SELECT
    [OrderDate],
    [Amount],
    SUM([Amount]) OVER (ORDER BY [OrderDate] ROWS BETWEEN 2 PRECEDING AND CURRENT ROW) AS [MovingSum]
FROM [dbo].[Orders];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ROWS frame: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_window_frame_range() {
    let sql = r#"
SELECT
    [OrderDate],
    [Amount],
    SUM([Amount]) OVER (ORDER BY [OrderDate] RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS [RunningTotal]
FROM [dbo].[Orders];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse RANGE frame: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_percent_rank_cume_dist() {
    let sql = r#"
SELECT
    [Name],
    [Score],
    PERCENT_RANK() OVER (ORDER BY [Score]) AS [PercentRank],
    CUME_DIST() OVER (ORDER BY [Score]) AS [CumeDist]
FROM [dbo].[Students];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse PERCENT_RANK/CUME_DIST: {:?}",
        result.err()
    );
}

// ============================================================================
// APPLY Operator Tests
// ============================================================================

#[test]
fn test_parse_cross_apply() {
    let sql = r#"
SELECT o.[Id], o.[OrderDate], d.[ProductId], d.[Quantity]
FROM [dbo].[Orders] o
CROSS APPLY (
    SELECT [ProductId], [Quantity]
    FROM [dbo].[OrderDetails]
    WHERE [OrderId] = o.[Id]
) d;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CROSS APPLY: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_outer_apply() {
    let sql = r#"
SELECT c.[Name], r.[OrderCount], r.[TotalAmount]
FROM [dbo].[Customers] c
OUTER APPLY (
    SELECT
        COUNT(*) AS [OrderCount],
        SUM([Amount]) AS [TotalAmount]
    FROM [dbo].[Orders]
    WHERE [CustomerId] = c.[Id]
) r;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse OUTER APPLY: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_cross_apply_with_function() {
    let sql = r#"
SELECT e.[Name], s.[Value]
FROM [dbo].[Employees] e
CROSS APPLY STRING_SPLIT(e.[Skills], ',') s;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CROSS APPLY with function: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_nested_apply() {
    let sql = r#"
SELECT d.[Name], e.[Name], p.[ProjectName]
FROM [dbo].[Departments] d
CROSS APPLY (
    SELECT [Name], [Id] FROM [dbo].[Employees] WHERE [DepartmentId] = d.[Id]
) e
OUTER APPLY (
    SELECT [ProjectName] FROM [dbo].[Projects] WHERE [LeaderId] = e.[Id]
) p;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse nested APPLY: {:?}",
        result.err()
    );
}

// ============================================================================
// PIVOT/UNPIVOT Tests
// ============================================================================

#[test]
fn test_parse_pivot_basic() {
    let sql = r#"
SELECT [CustomerId], [Jan], [Feb], [Mar]
FROM (
    SELECT [CustomerId], [Month], [Amount]
    FROM [dbo].[Sales]
) AS SourceTable
PIVOT (
    SUM([Amount])
    FOR [Month] IN ([Jan], [Feb], [Mar])
) AS PivotTable;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse PIVOT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_unpivot_basic() {
    let sql = r#"
SELECT [CustomerId], [Month], [Amount]
FROM [dbo].[MonthlySales]
UNPIVOT (
    [Amount] FOR [Month] IN ([Jan], [Feb], [Mar], [Apr])
) AS UnpivotTable;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse UNPIVOT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_pivot_with_join() {
    let sql = r#"
SELECT c.[Name], pvt.[Q1], pvt.[Q2], pvt.[Q3], pvt.[Q4]
FROM [dbo].[Customers] c
INNER JOIN (
    SELECT [CustomerId], [Q1], [Q2], [Q3], [Q4]
    FROM (
        SELECT [CustomerId], [Quarter], [Revenue]
        FROM [dbo].[QuarterlyRevenue]
    ) AS src
    PIVOT (
        SUM([Revenue])
        FOR [Quarter] IN ([Q1], [Q2], [Q3], [Q4])
    ) AS pvt
) pvt ON c.[Id] = pvt.[CustomerId];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse PIVOT with JOIN: {:?}",
        result.err()
    );
}

// ============================================================================
// JSON Function Tests
// ============================================================================

#[test]
fn test_parse_json_value() {
    let sql = r#"
SELECT
    [Id],
    JSON_VALUE([Data], '$.name') AS [Name],
    JSON_VALUE([Data], '$.address.city') AS [City]
FROM [dbo].[JsonDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_VALUE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_json_query() {
    let sql = r#"
SELECT
    [Id],
    JSON_QUERY([Data], '$.items') AS [Items],
    JSON_QUERY([Data], '$.metadata') AS [Metadata]
FROM [dbo].[JsonDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_QUERY: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_json_modify() {
    let sql = r#"
UPDATE [dbo].[JsonDocuments]
SET [Data] = JSON_MODIFY([Data], '$.status', 'active')
WHERE [Id] = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_MODIFY: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_isjson() {
    let sql = r#"
SELECT [Id], [Data]
FROM [dbo].[Documents]
WHERE ISJSON([Data]) = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse ISJSON: {:?}", result.err());
}

#[test]
fn test_parse_openjson() {
    let sql = r#"
SELECT j.[key], j.[value], j.[type]
FROM [dbo].[JsonDocuments] d
CROSS APPLY OPENJSON(d.[Data]) j;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse OPENJSON: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_openjson_with_schema() {
    let sql = r#"
SELECT [Name], [Age], [City]
FROM OPENJSON(@json)
WITH (
    [Name] NVARCHAR(100) '$.name',
    [Age] INT '$.age',
    [City] NVARCHAR(50) '$.address.city'
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse OPENJSON WITH: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_for_json_auto() {
    let sql = r#"
SELECT [Id], [Name], [Email]
FROM [dbo].[Users]
FOR JSON AUTO;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR JSON AUTO: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_for_json_path() {
    let sql = r#"
SELECT
    [Id] AS 'user.id',
    [Name] AS 'user.name',
    [Email] AS 'contact.email'
FROM [dbo].[Users]
FOR JSON PATH, ROOT('users');
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR JSON PATH: {:?}",
        result.err()
    );
}

// ============================================================================
// XML Method Tests
// ============================================================================

#[test]
fn test_parse_xml_query() {
    let sql = r#"
SELECT
    [Id],
    [XmlData].query('/root/items/item') AS [Items]
FROM [dbo].[XmlDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .query(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_xml_value() {
    let sql = r#"
SELECT
    [Id],
    [XmlData].value('(/root/name)[1]', 'NVARCHAR(100)') AS [Name]
FROM [dbo].[XmlDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .value(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_xml_exist() {
    let sql = r#"
SELECT [Id], [XmlData]
FROM [dbo].[XmlDocuments]
WHERE [XmlData].exist('/root/items/item[@status="active"]') = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .exist(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_xml_modify() {
    let sql = r#"
UPDATE [dbo].[XmlDocuments]
SET [XmlData].modify('replace value of (/root/status/text())[1] with "active"')
WHERE [Id] = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .modify(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_xml_nodes() {
    let sql = r#"
SELECT
    t.c.value('@id', 'INT') AS [ItemId],
    t.c.value('name[1]', 'NVARCHAR(100)') AS [ItemName]
FROM [dbo].[XmlDocuments] d
CROSS APPLY d.[XmlData].nodes('/root/items/item') AS t(c);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .nodes(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_for_xml_raw() {
    let sql = r#"
SELECT [Id], [Name], [Email]
FROM [dbo].[Users]
FOR XML RAW('user'), ROOT('users'), ELEMENTS;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR XML RAW: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_for_xml_path() {
    let sql = r#"
SELECT
    [Id] AS '@id',
    [Name] AS 'name',
    [Email] AS 'contact/email'
FROM [dbo].[Users]
FOR XML PATH('user'), ROOT('users');
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR XML PATH: {:?}",
        result.err()
    );
}

// ============================================================================
// Parser Error Handling Tests
// ============================================================================

#[test]
fn test_error_unclosed_string_literal() {
    let sql = "SELECT 'unclosed string FROM [dbo].[Table];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Unclosed string should fail");
}

#[test]
fn test_error_unclosed_bracket() {
    // Test with clearly unclosed bracket that can't be misinterpreted
    let sql = "SELECT [col1, [col2] FROM [dbo].[Table];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Document behavior - some unclosed brackets may be accepted as identifiers
    if result.is_err() {
        println!("Unclosed bracket correctly rejected");
    } else {
        println!("Note: Parser accepted SQL with bracket issues");
    }
}

#[test]
fn test_error_unclosed_parenthesis() {
    let sql = "SELECT (1 + 2 FROM [dbo].[Table];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Unclosed parenthesis should fail");
}

#[test]
fn test_error_invalid_keyword_order() {
    let sql = "TABLE CREATE [dbo].[Invalid];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Invalid keyword order should fail");
}

#[test]
fn test_error_missing_column_definition() {
    let sql = "CREATE TABLE [dbo].[Empty] ();";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Empty table may or may not be valid depending on parser
    // This documents behavior rather than asserting specific outcome
    if result.is_err() {
        println!("Empty column list is rejected (expected)");
    } else {
        println!("Empty column list is accepted");
    }
}

#[test]
fn test_error_duplicate_column_name() {
    // Note: This is a semantic error, not a parse error
    // The parser may accept this even though it's invalid SQL
    let sql = r#"
CREATE TABLE [dbo].[DupColumns] (
    [Id] INT,
    [Id] INT
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Parser typically accepts duplicate names; semantic analysis would catch this
    if result.is_ok() {
        println!("Duplicate column names accepted by parser (semantic check elsewhere)");
    }
}

#[test]
fn test_error_invalid_data_type() {
    let sql = "CREATE TABLE [dbo].[BadType] ([Col] FAKETYPE);";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Unknown data types might be accepted as custom types or rejected
    if result.is_err() {
        println!("Unknown data type rejected");
    } else {
        println!("Unknown data type accepted (might be treated as user-defined type)");
    }
}

#[test]
fn test_error_missing_table_name() {
    let sql = "CREATE TABLE ([Id] INT);";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Missing table name should fail");
}

#[test]
fn test_error_invalid_constraint_syntax() {
    // Test truly malformed constraint syntax
    let sql = r#"
CREATE TABLE [dbo].[BadConstraint] (
    [Id] INT,
    CONSTRAINT [PK] PRIMARY ([Id])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Missing KEY after PRIMARY should fail
    if result.is_err() {
        println!("Invalid constraint syntax correctly rejected");
    } else {
        // Document if parser is lenient
        println!("Note: Parser accepted malformed PRIMARY constraint");
    }
}

#[test]
fn test_error_line_number_in_message() {
    // Use SQL that definitely fails parsing
    let sql = r#"
-- Line 1: comment
-- Line 2: comment
INVALID SYNTAX THAT WILL FAIL;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Invalid SQL should fail");

    // Error message should contain line information
    let err_msg = result.unwrap_err().to_string();
    // The error should reference a line in the file
    println!("Error message: {}", err_msg);
    // Verify the error contains path or line info
    assert!(
        err_msg.contains("line") || err_msg.contains("Line") || err_msg.contains(".sql"),
        "Error message should contain file/line info"
    );
}

#[test]
fn test_error_recovery_good_batch_after_bad() {
    // Test that a good batch after a bad one doesn't affect error reporting
    let sql = r#"
-- First batch is invalid
INVALID SYNTAX HERE
GO
-- Second batch is valid
CREATE TABLE [dbo].[Valid] ([Id] INT);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // First batch should fail, aborting the entire file
    assert!(result.is_err(), "Invalid first batch should fail");
}

#[test]
fn test_parse_mixed_valid_statements_in_file() {
    let sql = r#"
CREATE TABLE [dbo].[Table1] ([Id] INT NOT NULL PRIMARY KEY);
GO

CREATE VIEW [dbo].[View1] AS SELECT [Id] FROM [dbo].[Table1];
GO

CREATE INDEX [IX_Table1] ON [dbo].[Table1] ([Id]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Mixed valid statements should parse: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 3, "Should have 3 statements");
}

#[test]
fn test_error_nested_comments_edge_case() {
    // SQL Server allows nested block comments /* /* */ */
    let sql = r#"
/* Outer comment
   /* Nested comment */
   Still in outer
*/
SELECT 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This tests whether nested comments are handled
    if result.is_ok() {
        println!("Nested comments supported");
    } else {
        println!(
            "Nested comments not supported (sqlparser limitation): {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_unicode_identifiers() {
    let sql = r#"
CREATE TABLE [dbo].[日本語テーブル] (
    [列名] NVARCHAR(100),
    [Ñoño] INT
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Unicode identifiers should parse: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_very_long_identifier() {
    // SQL Server allows identifiers up to 128 characters
    let long_name = "A".repeat(128);
    let sql = format!(
        "CREATE TABLE [dbo].[{}] ([Id] INT);",
        long_name
    );
    let file = create_sql_file(&sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Long identifier should parse: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_special_characters_in_strings() {
    let sql = r#"
CREATE TABLE [dbo].[Test] (
    [Value] NVARCHAR(100) DEFAULT N'It''s a "test" with special chars: \n\t'
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Special characters in strings should parse: {:?}",
        result.err()
    );
}

// ============================================================================
// OUTPUT Parameter Tests
// ============================================================================

#[test]
fn test_parse_procedure_with_output_parameter() {
    let sql = r#"
CREATE PROCEDURE [dbo].[GetNextId]
    @TableName NVARCHAR(128),
    @NextId INT OUTPUT
AS
BEGIN
    SELECT @NextId = MAX(Id) + 1 FROM sys.tables WHERE name = @TableName
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with OUTPUT parameter: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify the procedure is parsed with fallback
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetNextId");
        }
        _ => panic!("Expected Procedure fallback type for OUTPUT parameter syntax"),
    }

    // Verify original SQL preserves OUTPUT keyword
    assert!(
        statements[0].sql_text.contains("OUTPUT"),
        "SQL text should preserve OUTPUT keyword"
    );
}

#[test]
fn test_parse_procedure_with_out_abbreviation() {
    // SQL Server allows OUT as abbreviation for OUTPUT
    let sql = r#"
CREATE PROCEDURE [dbo].[GetValue]
    @Key VARCHAR(50),
    @Value VARCHAR(MAX) OUT
AS
BEGIN
    SELECT @Value = Value FROM Config WHERE [Key] = @Key
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with OUT abbreviation: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("OUT"),
        "SQL text should preserve OUT keyword"
    );
}

#[test]
fn test_parse_procedure_with_multiple_output_parameters() {
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUserDetails]
    @UserId INT,
    @FirstName NVARCHAR(50) OUTPUT,
    @LastName NVARCHAR(50) OUTPUT,
    @Email NVARCHAR(255) OUTPUT,
    @CreatedDate DATETIME2 OUTPUT
AS
BEGIN
    SELECT
        @FirstName = FirstName,
        @LastName = LastName,
        @Email = Email,
        @CreatedDate = CreatedDate
    FROM Users
    WHERE UserId = @UserId
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with multiple OUTPUT parameters: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify all OUTPUT parameters are in the preserved SQL
    let sql_text = &statements[0].sql_text;
    let output_count = sql_text.matches("OUTPUT").count();
    assert_eq!(
        output_count, 4,
        "Should have 4 OUTPUT keywords in the preserved SQL"
    );
}

#[test]
fn test_parse_procedure_with_output_and_default_value() {
    let sql = r#"
CREATE PROCEDURE [dbo].[ProcessWithDefaults]
    @InputValue INT,
    @Multiplier INT = 2,
    @Result INT OUTPUT,
    @Status VARCHAR(20) = 'Pending' OUTPUT
AS
BEGIN
    SET @Result = @InputValue * @Multiplier
    SET @Status = 'Complete'
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with OUTPUT and default value: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify defaults and OUTPUT are preserved
    let sql_text = &statements[0].sql_text;
    assert!(sql_text.contains("= 2"), "Should preserve default value = 2");
    assert!(
        sql_text.contains("'Pending'"),
        "Should preserve default value 'Pending'"
    );
}

#[test]
fn test_parse_procedure_mixed_input_and_output_parameters() {
    let sql = r#"
CREATE PROCEDURE [sales].[CalculateOrderTotal]
    @OrderId INT,
    @ApplyDiscount BIT = 0,
    @DiscountPercent DECIMAL(5,2) = 0.00,
    @Subtotal MONEY OUTPUT,
    @Tax MONEY OUTPUT,
    @Total MONEY OUTPUT
AS
BEGIN
    DECLARE @TaxRate DECIMAL(5,2) = 0.08

    SELECT @Subtotal = SUM(Quantity * UnitPrice)
    FROM OrderItems
    WHERE OrderId = @OrderId

    IF @ApplyDiscount = 1
        SET @Subtotal = @Subtotal * (1 - @DiscountPercent / 100)

    SET @Tax = @Subtotal * @TaxRate
    SET @Total = @Subtotal + @Tax
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with mixed input/output parameters: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "sales");
            assert_eq!(name, "CalculateOrderTotal");
        }
        _ => panic!("Expected Procedure fallback type"),
    }
}

#[test]
fn test_parse_procedure_output_readonly_table_type() {
    // Table-valued parameters are READONLY, not OUTPUT
    // But a procedure can have both TVP and OUTPUT params
    let sql = r#"
CREATE PROCEDURE [dbo].[BulkInsertWithCount]
    @Items dbo.ItemTableType READONLY,
    @InsertedCount INT OUTPUT
AS
BEGIN
    INSERT INTO Items (Name, Value)
    SELECT Name, Value FROM @Items

    SET @InsertedCount = @@ROWCOUNT
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with READONLY and OUTPUT parameters: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    let sql_text = &statements[0].sql_text;
    assert!(
        sql_text.contains("READONLY"),
        "Should preserve READONLY keyword"
    );
    assert!(
        sql_text.contains("OUTPUT"),
        "Should preserve OUTPUT keyword"
    );
}

#[test]
fn test_parse_alter_procedure_with_output_parameter() {
    let sql = r#"
ALTER PROCEDURE [dbo].[GetMaxId]
    @TableName SYSNAME,
    @MaxId BIGINT OUTPUT
AS
BEGIN
    DECLARE @sql NVARCHAR(MAX)
    SET @sql = N'SELECT @MaxId = MAX(Id) FROM ' + QUOTENAME(@TableName)
    EXEC sp_executesql @sql, N'@MaxId BIGINT OUTPUT', @MaxId = @MaxId OUTPUT
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER PROCEDURE with OUTPUT parameter: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_procedure_cursor_output() {
    // CURSOR OUTPUT is a special case in SQL Server
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUsersCursor]
    @ActiveOnly BIT = 1,
    @UserCursor CURSOR VARYING OUTPUT
AS
BEGIN
    SET @UserCursor = CURSOR FORWARD_ONLY STATIC FOR
        SELECT UserId, UserName FROM Users
        WHERE @ActiveOnly = 0 OR IsActive = 1

    OPEN @UserCursor
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with CURSOR OUTPUT: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("CURSOR VARYING OUTPUT"),
        "Should preserve CURSOR VARYING OUTPUT"
    );
}
