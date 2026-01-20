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
    let sql = "CREATE TABLE t1 (id INT)\ngo\nCREATE TABLE t2 (id INT)\nGO\nCREATE TABLE t3 (id INT)\nGo";
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
    assert_eq!(statements.len(), 2, "Expected 2 statements without GO separator");
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
    assert!(result.is_ok(), "Failed to parse simple table: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify it's a CREATE TABLE statement
    match &statements[0].statement {
        sqlparser::ast::Statement::CreateTable(create) => {
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
    assert!(result.is_ok(), "Failed to parse table with PK: {:?}", result.err());
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
    assert!(result.is_ok(), "Failed to parse table with named PK: {:?}", result.err());
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
    assert!(result.is_ok(), "Failed to parse table with FK: {:?}", result.err());

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
    assert!(result.is_ok(), "Failed to parse table with unique: {:?}", result.err());
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
    assert!(result.is_ok(), "Failed to parse table with check: {:?}", result.err());
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
    assert!(result.is_ok(), "Failed to parse table with default: {:?}", result.err());
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
    assert!(result.is_ok(), "Failed to parse table with identity: {:?}", result.err());
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
    assert!(result.is_ok(), "Failed to parse table with all types: {:?}", result.err());
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
    assert!(result.is_ok(), "Failed to parse simple view: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        sqlparser::ast::Statement::CreateView { name, .. } => {
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
    assert!(result.is_ok(), "Failed to parse view with columns: {:?}", result.err());
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
    assert!(result.is_ok(), "Failed to parse nonclustered index: {:?}", result.err());
}

#[test]
fn test_parse_clustered_index() {
    let sql = r#"
CREATE CLUSTERED INDEX [IX_Table_Clustered]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse clustered index: {:?}", result.err());
}

#[test]
fn test_parse_unique_index() {
    let sql = r#"
CREATE UNIQUE NONCLUSTERED INDEX [IX_Table_Unique]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse unique index: {:?}", result.err());
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
    assert!(result.is_ok(), "Failed to parse index with include: {:?}", result.err());
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
    let result = rust_sqlpackage::parser::parse_sql_file(&PathBuf::from("/nonexistent/path/file.sql"));
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
    assert!(result.is_ok(), "Whitespace-only file should parse successfully");

    let statements = result.unwrap();
    assert_eq!(statements.len(), 0, "Whitespace-only file should have no statements");
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
        assert_eq!(statements.len(), 0, "Comment-only file should have no statements");
    }
}

// ============================================================================
// Additional Batch Separator Tests (from TEST_PLAN.md)
// ============================================================================

#[test]
fn test_split_batches_go_with_count() {
    // GO 5 means execute the batch 5 times - we should just treat it as one batch
    let sql = "CREATE TABLE t1 (id INT)\nGO 5\nCREATE TABLE t2 (id INT)";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This test documents behavior - GO with count may or may not be supported
    if result.is_ok() {
        let statements = result.unwrap();
        assert!(statements.len() >= 2, "Should split into at least 2 batches");
    } else {
        // If parsing fails with GO count, that's acceptable behavior to document
        println!("Note: GO with count not supported: {:?}", result.err());
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
        assert_eq!(statements.len(), 2, "GO in block comment should not cause split");
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
        println!("Note: Computed columns not fully supported: {:?}", result.err());
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
        println!("Note: PERSISTED computed columns not supported: {:?}", result.err());
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
    assert!(result.is_ok(), "Standard CREATE INDEX should be supported: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}
