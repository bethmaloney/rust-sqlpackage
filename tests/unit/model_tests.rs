//! Unit tests for database model builder
//!
//! These tests verify the transformation from SQL AST to internal database model.

use std::io::Write;
use std::path::PathBuf;

use tempfile::NamedTempFile;

/// Helper to create a temp SQL file with content
fn create_sql_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::with_suffix(".sql").unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

/// Helper to create a test SqlProject
fn create_test_project() -> rust_sqlpackage::project::SqlProject {
    rust_sqlpackage::project::SqlProject {
        name: "TestProject".to_string(),
        target_platform: rust_sqlpackage::project::SqlServerVersion::Sql160,
        default_schema: "dbo".to_string(),
        collation_lcid: 1033,
        sql_files: vec![],
        dacpac_references: vec![],
        project_dir: PathBuf::new(),
        pre_deploy_script: None,
        post_deploy_script: None,
    }
}

/// Helper to parse SQL and build model
fn parse_and_build_model(sql: &str) -> rust_sqlpackage::model::DatabaseModel {
    let file = create_sql_file(sql);
    let statements = rust_sqlpackage::parser::parse_sql_file(file.path()).unwrap();
    let project = create_test_project();
    rust_sqlpackage::model::build_model(&statements, &project).unwrap()
}

// ============================================================================
// Schema Handling Tests
// ============================================================================

#[test]
fn test_extract_dbo_schema() {
    let sql = "CREATE TABLE [dbo].[TestTable] ([Id] INT NOT NULL PRIMARY KEY);";
    let model = parse_and_build_model(sql);

    // Model should contain dbo schema (stored without brackets)
    let has_dbo = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Schema(s) if s.name == "dbo")
    });
    assert!(has_dbo, "Model should contain dbo schema");
}

#[test]
fn test_extract_custom_schema() {
    let sql = "CREATE TABLE [sales].[Orders] ([Id] INT NOT NULL PRIMARY KEY);";
    let model = parse_and_build_model(sql);

    // Model should contain sales schema
    let has_sales = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Schema(s) if s.name.contains("sales"))
    });
    assert!(has_sales, "Model should contain sales schema");
}

#[test]
fn test_default_schema_when_unspecified() {
    let sql = "CREATE TABLE TestTable ([Id] INT NOT NULL PRIMARY KEY);";
    let model = parse_and_build_model(sql);

    // Table should be in default schema (dbo)
    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");
    let table = table.unwrap();
    // Schema should be dbo
    assert!(
        table.name.contains("dbo") || table.schema.contains("dbo"),
        "Table should be in dbo schema"
    );
}

// ============================================================================
// Table Building Tests
// ============================================================================

#[test]
fn test_build_table_element() {
    let sql = "CREATE TABLE [dbo].[Users] ([Id] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");
    let table = table.unwrap();
    assert!(table.name.contains("Users"), "Table name should be Users");
}

#[test]
fn test_build_table_with_columns() {
    let sql = r#"
CREATE TABLE [dbo].[Users] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NULL,
    [Email] VARCHAR(255) NOT NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();
    assert_eq!(table.columns.len(), 3, "Table should have 3 columns");
}

#[test]
fn test_build_column_types_int() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(
        col.data_type.to_uppercase().contains("INT"),
        "Column type should be INT, got: {}",
        col.data_type
    );
}

#[test]
fn test_build_column_types_varchar() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] VARCHAR(100) NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(
        col.data_type.to_uppercase().contains("VARCHAR"),
        "Column type should contain VARCHAR, got: {}",
        col.data_type
    );
}

#[test]
fn test_build_column_types_decimal() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] DECIMAL(18, 2) NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(
        col.data_type.to_uppercase().contains("DECIMAL"),
        "Column type should contain DECIMAL, got: {}",
        col.data_type
    );
}

#[test]
fn test_build_column_types_datetime() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] DATETIME2 NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(
        col.data_type.to_uppercase().contains("DATETIME"),
        "Column type should contain DATETIME, got: {}",
        col.data_type
    );
}

#[test]
fn test_build_column_nullable() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] INT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(col.is_nullable, "Column should be nullable");
}

#[test]
fn test_build_column_not_nullable() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(!col.is_nullable, "Column should not be nullable");
}

// ============================================================================
// Constraint Building Tests
// ============================================================================

#[test]
fn test_build_primary_key_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL,
    CONSTRAINT [PK_T] PRIMARY KEY CLUSTERED ([Id])
);
"#;
    let model = parse_and_build_model(sql);

    let has_pk = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Constraint(c) if c.name.contains("PK_"))
    });
    assert!(has_pk, "Model should contain primary key constraint");
}

#[test]
fn test_build_foreign_key_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[Parent] ([Id] INT NOT NULL PRIMARY KEY);
GO
CREATE TABLE [dbo].[Child] (
    [Id] INT NOT NULL PRIMARY KEY,
    [ParentId] INT NOT NULL,
    CONSTRAINT [FK_Child_Parent] FOREIGN KEY ([ParentId]) REFERENCES [dbo].[Parent]([Id])
);
"#;
    let model = parse_and_build_model(sql);

    let has_fk = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Constraint(c) if c.name.contains("FK_"))
    });
    assert!(has_fk, "Model should contain foreign key constraint");
}

#[test]
fn test_build_unique_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Email] NVARCHAR(255) NOT NULL,
    CONSTRAINT [UQ_T_Email] UNIQUE ([Email])
);
"#;
    let model = parse_and_build_model(sql);

    let has_uq = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Constraint(c) if c.name.contains("UQ_"))
    });
    assert!(has_uq, "Model should contain unique constraint");
}

#[test]
fn test_build_check_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Age] INT NOT NULL,
    CONSTRAINT [CK_T_Age] CHECK ([Age] >= 0)
);
"#;
    let model = parse_and_build_model(sql);

    let has_ck = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Constraint(c) if c.name.contains("CK_"))
    });
    assert!(has_ck, "Model should contain check constraint");
}

// ============================================================================
// View Building Tests
// ============================================================================

#[test]
fn test_build_view_element() {
    let sql = r#"
CREATE VIEW [dbo].[TestView]
AS
SELECT 1 AS [Value];
"#;
    let model = parse_and_build_model(sql);

    let view = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::View(v) = e {
            Some(v)
        } else {
            None
        }
    });

    assert!(view.is_some(), "Model should contain a view");
    let view = view.unwrap();
    assert!(view.name.contains("TestView"), "View name should be TestView");
}

#[test]
fn test_build_view_with_select_statement() {
    let sql = r#"
CREATE VIEW [dbo].[DetailedView]
AS
SELECT [Id], [Name], [CreatedAt]
FROM [dbo].[SomeTable]
WHERE [IsActive] = 1;
"#;
    let model = parse_and_build_model(sql);

    let view = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::View(v) = e {
            Some(v)
        } else {
            None
        }
    });

    assert!(view.is_some(), "Model should contain a view");
    let view = view.unwrap();
    // View should have the definition stored
    assert!(
        !view.definition.is_empty(),
        "View should have a definition"
    );
}

// ============================================================================
// Index Building Tests
// ============================================================================

#[test]
fn test_build_index_element() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_Column]
ON [dbo].[T] ([Column1]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(
        index.name.contains("IX_"),
        "Index name should contain IX_, got: {}",
        index.name
    );
}

#[test]
fn test_build_clustered_index() {
    let sql = r#"
CREATE CLUSTERED INDEX [IX_T_Clustered]
ON [dbo].[T] ([Column1]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(index.is_clustered, "Index should be clustered");
}

#[test]
fn test_build_index_with_included_columns() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_WithInclude]
ON [dbo].[T] ([Column1])
INCLUDE ([Column2], [Column3]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(
        index.name.contains("IX_T_WithInclude"),
        "Index should be named correctly"
    );

    // Verify include columns are captured
    assert_eq!(index.include_columns.len(), 2, "Index should have 2 include columns");
    assert!(
        index.include_columns.contains(&"Column2".to_string()),
        "Include columns should contain Column2"
    );
    assert!(
        index.include_columns.contains(&"Column3".to_string()),
        "Include columns should contain Column3"
    );
}

#[test]
fn test_build_index_with_single_include_column() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_SingleInclude]
ON [dbo].[T] ([Col1], [Col2])
INCLUDE ([Col3]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert_eq!(index.columns.len(), 2, "Index should have 2 key columns");
    assert_eq!(index.include_columns.len(), 1, "Index should have 1 include column");
    assert_eq!(index.include_columns[0], "Col3");
}

#[test]
fn test_build_index_without_include_clause() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_NoInclude]
ON [dbo].[T] ([Column1], [Column2]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(index.include_columns.is_empty(), "Index without INCLUDE should have empty include_columns");
}

#[test]
fn test_build_unique_clustered_index_with_include() {
    let sql = r#"
CREATE UNIQUE NONCLUSTERED INDEX [IX_T_UniqueWithInclude]
ON [dbo].[T] ([KeyCol])
INCLUDE ([Data1], [Data2], [Data3]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(index.is_unique, "Index should be unique");
    assert!(!index.is_clustered, "Index should be nonclustered");
    assert_eq!(index.columns.len(), 1, "Index should have 1 key column");
    assert_eq!(index.include_columns.len(), 3, "Index should have 3 include columns");
}

// ============================================================================
// Multiple Elements Tests
// ============================================================================

#[test]
fn test_build_model_with_multiple_tables() {
    let sql = r#"
CREATE TABLE [dbo].[Table1] ([Id] INT NOT NULL PRIMARY KEY);
GO
CREATE TABLE [dbo].[Table2] ([Id] INT NOT NULL PRIMARY KEY);
GO
CREATE TABLE [dbo].[Table3] ([Id] INT NOT NULL PRIMARY KEY);
"#;
    let model = parse_and_build_model(sql);

    let table_count = model
        .elements
        .iter()
        .filter(|e| matches!(e, rust_sqlpackage::model::ModelElement::Table(_)))
        .count();

    assert_eq!(table_count, 3, "Model should contain 3 tables");
}

#[test]
fn test_build_model_with_mixed_elements() {
    let sql = r#"
CREATE TABLE [dbo].[Users] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL
);
GO
CREATE VIEW [dbo].[ActiveUsers]
AS
SELECT * FROM [dbo].[Users];
GO
CREATE INDEX [IX_Users_Name]
ON [dbo].[Users] ([Name]);
"#;
    let model = parse_and_build_model(sql);

    let has_table = model
        .elements
        .iter()
        .any(|e| matches!(e, rust_sqlpackage::model::ModelElement::Table(_)));
    let has_view = model
        .elements
        .iter()
        .any(|e| matches!(e, rust_sqlpackage::model::ModelElement::View(_)));
    let has_index = model
        .elements
        .iter()
        .any(|e| matches!(e, rust_sqlpackage::model::ModelElement::Index(_)));

    assert!(has_table, "Model should contain a table");
    assert!(has_view, "Model should contain a view");
    assert!(has_index, "Model should contain an index");
}

// ============================================================================
// Additional Constraint Tests (from TEST_PLAN.md)
// ============================================================================

#[test]
fn test_build_default_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [CreatedAt] DATETIME2 NOT NULL DEFAULT GETDATE(),
    [IsActive] BIT NOT NULL DEFAULT 1
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");
    let table = table.unwrap();

    // Check that default values are captured (if supported)
    // The column should still be present regardless of default handling
    assert_eq!(table.columns.len(), 3, "Table should have 3 columns");
}

#[test]
fn test_build_model_preserves_schema_from_qualified_name() {
    let sql = "CREATE TABLE [custom_schema].[MyTable] ([Id] INT NOT NULL PRIMARY KEY);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");
    let table = table.unwrap();

    // Table name should include schema qualification
    assert!(
        table.name.contains("custom_schema") || table.schema.contains("custom_schema"),
        "Table should preserve custom_schema"
    );
}

#[test]
fn test_build_model_with_standard_index() {
    // Use standard CREATE INDEX (without CLUSTERED/NONCLUSTERED) which sqlparser-rs supports
    let sql = r#"
CREATE TABLE [dbo].[T] ([Col1] INT NOT NULL);
GO
CREATE INDEX [IX_T_Col1]
ON [dbo].[T] ([Col1]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(
        index.name.contains("IX_T_Col1"),
        "Index should be named IX_T_Col1, got: {}",
        index.name
    );
}

// ============================================================================
// Procedure Building Tests
// ============================================================================

#[test]
fn test_build_procedure_element_simple() {
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUsers]
AS
BEGIN
    SELECT * FROM Users
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some(), "Model should contain a procedure");
    let proc = proc.unwrap();
    assert_eq!(proc.name, "GetUsers", "Procedure name should be GetUsers");
    assert_eq!(proc.schema, "dbo", "Procedure schema should be dbo");
}

#[test]
fn test_build_procedure_element_with_parameters() {
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUserById]
    @UserId INT,
    @IncludeDeleted BIT = 0
AS
BEGIN
    SELECT * FROM Users WHERE Id = @UserId
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some(), "Model should contain a procedure");
    let proc = proc.unwrap();
    assert_eq!(proc.name, "GetUserById");
    // Definition should contain the full T-SQL including parameters
    assert!(
        proc.definition.contains("@UserId INT"),
        "Definition should contain parameter declaration"
    );
}

#[test]
fn test_build_procedure_element_custom_schema() {
    let sql = r#"
CREATE PROCEDURE [sales].[ProcessOrder]
    @OrderId INT
AS
BEGIN
    UPDATE Orders SET Status = 'Processed' WHERE Id = @OrderId
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some(), "Model should contain a procedure");
    let proc = proc.unwrap();
    assert_eq!(proc.schema, "sales", "Procedure should be in sales schema");
    assert_eq!(proc.name, "ProcessOrder");

    // Verify sales schema was added to model
    let has_sales_schema = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Schema(s) if s.name.contains("sales"))
    });
    assert!(has_sales_schema, "Model should contain sales schema");
}

#[test]
fn test_build_procedure_preserves_definition() {
    let sql = r#"
CREATE PROCEDURE [dbo].[ComplexProc]
    @Param1 INT,
    @Param2 VARCHAR(100)
AS
BEGIN
    DECLARE @Result INT
    SET @Result = @Param1 * 2
    SELECT @Result, @Param2
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some());
    let proc = proc.unwrap();
    // Full T-SQL definition should be preserved
    assert!(proc.definition.contains("DECLARE @Result"));
    assert!(proc.definition.contains("SET @Result"));
}

// ============================================================================
// Function Building Tests
// ============================================================================

#[test]
fn test_build_scalar_function_element() {
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
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Function(f) = e {
            Some(f)
        } else {
            None
        }
    });

    assert!(func.is_some(), "Model should contain a function");
    let func = func.unwrap();
    assert_eq!(func.name, "GetFullName");
    assert_eq!(func.schema, "dbo");
    assert_eq!(
        func.function_type,
        rust_sqlpackage::model::FunctionType::Scalar,
        "Should be scalar function"
    );
}

#[test]
fn test_build_table_valued_function_element() {
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
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Function(f) = e {
            Some(f)
        } else {
            None
        }
    });

    assert!(func.is_some(), "Model should contain a function");
    let func = func.unwrap();
    assert_eq!(func.name, "GetUserOrders");
    assert_eq!(
        func.function_type,
        rust_sqlpackage::model::FunctionType::TableValued,
        "Should be table-valued function"
    );
}

#[test]
fn test_build_function_element_custom_schema() {
    let sql = r#"
CREATE FUNCTION [utils].[FormatDate]
(
    @Date DATETIME
)
RETURNS VARCHAR(10)
AS
BEGIN
    RETURN CONVERT(VARCHAR(10), @Date, 120)
END
"#;
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Function(f) = e {
            Some(f)
        } else {
            None
        }
    });

    assert!(func.is_some());
    let func = func.unwrap();
    assert_eq!(func.schema, "utils", "Function should be in utils schema");

    // Verify utils schema was added
    let has_utils_schema = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Schema(s) if s.name.contains("utils"))
    });
    assert!(has_utils_schema, "Model should contain utils schema");
}

#[test]
fn test_build_function_preserves_definition() {
    let sql = r#"
CREATE FUNCTION [dbo].[CalculateTax]
(
    @Amount DECIMAL(18, 2),
    @Rate DECIMAL(5, 2)
)
RETURNS DECIMAL(18, 2)
AS
BEGIN
    RETURN @Amount * @Rate / 100
END
"#;
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Function(f) = e {
            Some(f)
        } else {
            None
        }
    });

    assert!(func.is_some());
    let func = func.unwrap();
    assert!(func.definition.contains("@Amount * @Rate / 100"));
}

// ============================================================================
// Column Type Parameter Tests
// ============================================================================

#[test]
fn test_build_column_varchar_max_length() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] VARCHAR(100) NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert_eq!(col.max_length, Some(100), "VARCHAR(100) should have max_length 100");
}

#[test]
fn test_build_column_nvarchar_max_length() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] NVARCHAR(255) NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert_eq!(col.max_length, Some(255), "NVARCHAR(255) should have max_length 255");
}

#[test]
fn test_build_column_varchar_max() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] VARCHAR(MAX) NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert_eq!(col.max_length, Some(-1), "VARCHAR(MAX) should have max_length -1");
}

#[test]
fn test_build_column_decimal_precision_scale() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] DECIMAL(18, 2) NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert_eq!(col.precision, Some(18), "DECIMAL(18,2) should have precision 18");
    assert_eq!(col.scale, Some(2), "DECIMAL(18,2) should have scale 2");
}

#[test]
fn test_build_column_numeric_precision_scale() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] NUMERIC(10, 4) NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert_eq!(col.precision, Some(10), "NUMERIC(10,4) should have precision 10");
    assert_eq!(col.scale, Some(4), "NUMERIC(10,4) should have scale 4");
}

#[test]
fn test_build_column_char_max_length() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] CHAR(10) NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert_eq!(col.max_length, Some(10), "CHAR(10) should have max_length 10");
}

// ============================================================================
// Default Value Tests
// ============================================================================

#[test]
fn test_build_column_default_value_literal() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] INT NOT NULL DEFAULT 0);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(
        col.default_value.is_some(),
        "Column should have default value"
    );
    assert!(
        col.default_value.as_ref().unwrap().contains("0"),
        "Default value should be 0"
    );
}

#[test]
fn test_build_column_default_value_string() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] VARCHAR(50) NOT NULL DEFAULT 'unknown');";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(col.default_value.is_some());
    assert!(
        col.default_value.as_ref().unwrap().contains("unknown"),
        "Default value should contain 'unknown'"
    );
}

#[test]
fn test_build_column_default_value_function() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] DATETIME2 NOT NULL DEFAULT GETDATE());";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(col.default_value.is_some());
    // GETDATE() should be captured in some form
    let default = col.default_value.as_ref().unwrap().to_uppercase();
    assert!(
        default.contains("GETDATE"),
        "Default value should contain GETDATE, got: {}",
        default
    );
}

#[test]
fn test_build_column_default_value_bit() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] BIT NOT NULL DEFAULT 1);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(col.default_value.is_some());
}

// ============================================================================
// Index Property Tests
// ============================================================================

#[test]
fn test_build_unique_index() {
    let sql = r#"
CREATE TABLE [dbo].[T] ([Col1] INT NOT NULL);
GO
CREATE UNIQUE INDEX [IX_T_Col1_Unique]
ON [dbo].[T] ([Col1]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(index.is_unique, "Index should be marked as unique");
}

#[test]
fn test_build_index_columns() {
    let sql = r#"
CREATE TABLE [dbo].[T] ([Col1] INT, [Col2] INT, [Col3] INT);
GO
CREATE INDEX [IX_T_Multi]
ON [dbo].[T] ([Col1], [Col2]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some());
    let index = index.unwrap();
    assert_eq!(index.columns.len(), 2, "Index should have 2 columns");
}

#[test]
fn test_build_index_table_reference() {
    let sql = r#"
CREATE TABLE [sales].[Orders] ([Id] INT NOT NULL);
GO
CREATE INDEX [IX_Orders_Id]
ON [sales].[Orders] ([Id]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some());
    let index = index.unwrap();
    assert!(
        index.table_schema.contains("sales"),
        "Index should reference sales schema"
    );
    assert!(
        index.table_name.contains("Orders"),
        "Index should reference Orders table"
    );
}

// ============================================================================
// Constraint Detail Tests
// ============================================================================

#[test]
fn test_build_primary_key_columns() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id1] INT NOT NULL,
    [Id2] INT NOT NULL,
    CONSTRAINT [PK_T] PRIMARY KEY CLUSTERED ([Id1], [Id2])
);
"#;
    let model = parse_and_build_model(sql);

    let pk = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            if matches!(c.constraint_type, rust_sqlpackage::model::ConstraintType::PrimaryKey) {
                Some(c)
            } else {
                None
            }
        } else {
            None
        }
    });

    assert!(pk.is_some(), "Model should contain PK constraint");
    let pk = pk.unwrap();
    assert_eq!(pk.columns.len(), 2, "PK should have 2 columns");
}

#[test]
fn test_build_foreign_key_referenced_table() {
    let sql = r#"
CREATE TABLE [dbo].[Parent] ([Id] INT NOT NULL PRIMARY KEY);
GO
CREATE TABLE [dbo].[Child] (
    [Id] INT NOT NULL PRIMARY KEY,
    [ParentId] INT NOT NULL,
    CONSTRAINT [FK_Child_Parent] FOREIGN KEY ([ParentId]) REFERENCES [dbo].[Parent]([Id])
);
"#;
    let model = parse_and_build_model(sql);

    let fk = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            if matches!(c.constraint_type, rust_sqlpackage::model::ConstraintType::ForeignKey) {
                Some(c)
            } else {
                None
            }
        } else {
            None
        }
    });

    assert!(fk.is_some(), "Model should contain FK constraint");
    let fk = fk.unwrap();
    assert!(fk.referenced_table.is_some(), "FK should have referenced table");
    assert!(
        fk.referenced_table.as_ref().unwrap().contains("Parent"),
        "FK should reference Parent table"
    );
    assert!(fk.referenced_columns.is_some(), "FK should have referenced columns");
}

#[test]
fn test_build_check_constraint_definition() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Age] INT NOT NULL,
    CONSTRAINT [CK_T_Age] CHECK ([Age] >= 0 AND [Age] <= 150)
);
"#;
    let model = parse_and_build_model(sql);

    let ck = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            if matches!(c.constraint_type, rust_sqlpackage::model::ConstraintType::Check) {
                Some(c)
            } else {
                None
            }
        } else {
            None
        }
    });

    assert!(ck.is_some(), "Model should contain CHECK constraint");
    let ck = ck.unwrap();
    assert!(ck.definition.is_some(), "CHECK should have definition");
    let def = ck.definition.as_ref().unwrap();
    assert!(def.contains("Age"), "CHECK definition should reference Age column");
}

// ============================================================================
// Schema Deduplication Tests
// ============================================================================

#[test]
fn test_schemas_are_deduplicated() {
    let sql = r#"
CREATE TABLE [dbo].[T1] ([Id] INT NOT NULL);
GO
CREATE TABLE [dbo].[T2] ([Id] INT NOT NULL);
GO
CREATE TABLE [dbo].[T3] ([Id] INT NOT NULL);
"#;
    let model = parse_and_build_model(sql);

    let schema_count = model
        .elements
        .iter()
        .filter(|e| {
            matches!(e, rust_sqlpackage::model::ModelElement::Schema(s) if s.name == "dbo")
        })
        .count();

    assert_eq!(schema_count, 1, "dbo schema should appear exactly once");
}

#[test]
fn test_multiple_schemas_all_present() {
    let sql = r#"
CREATE TABLE [dbo].[T1] ([Id] INT NOT NULL);
GO
CREATE TABLE [sales].[T2] ([Id] INT NOT NULL);
GO
CREATE TABLE [hr].[T3] ([Id] INT NOT NULL);
"#;
    let model = parse_and_build_model(sql);

    let schema_names: Vec<_> = model
        .elements
        .iter()
        .filter_map(|e| {
            if let rust_sqlpackage::model::ModelElement::Schema(s) = e {
                Some(s.name.as_str())
            } else {
                None
            }
        })
        .collect();

    assert!(
        schema_names.iter().any(|s| s.contains("dbo")),
        "Should have dbo schema"
    );
    assert!(
        schema_names.iter().any(|s| s.contains("sales")),
        "Should have sales schema"
    );
    assert!(
        schema_names.iter().any(|s| s.contains("hr")),
        "Should have hr schema"
    );
}

// ============================================================================
// ModelElement Method Tests
// ============================================================================

#[test]
fn test_model_element_type_name_table() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Table(_))
    });

    assert!(table.is_some());
    assert_eq!(table.unwrap().type_name(), "SqlTable");
}

#[test]
fn test_model_element_type_name_view() {
    let sql = "CREATE VIEW [dbo].[V] AS SELECT 1 AS [Val];";
    let model = parse_and_build_model(sql);

    let view = model.elements.iter().find(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::View(_))
    });

    assert!(view.is_some());
    assert_eq!(view.unwrap().type_name(), "SqlView");
}

#[test]
fn test_model_element_type_name_procedure() {
    let sql = r#"
CREATE PROCEDURE [dbo].[P]
AS
BEGIN
    SELECT 1
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Procedure(_))
    });

    assert!(proc.is_some());
    assert_eq!(proc.unwrap().type_name(), "SqlProcedure");
}

#[test]
fn test_model_element_type_name_scalar_function() {
    let sql = r#"
CREATE FUNCTION [dbo].[F]()
RETURNS INT
AS
BEGIN
    RETURN 1
END
"#;
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Function(_))
    });

    assert!(func.is_some());
    assert_eq!(func.unwrap().type_name(), "SqlScalarFunction");
}

#[test]
fn test_model_element_type_name_table_valued_function() {
    let sql = r#"
CREATE FUNCTION [dbo].[TVF]()
RETURNS TABLE
AS
RETURN (SELECT 1 AS [Val])
"#;
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Function(_))
    });

    assert!(func.is_some());
    assert_eq!(func.unwrap().type_name(), "SqlTableValuedFunction");
}

#[test]
fn test_model_element_full_name_table() {
    let sql = "CREATE TABLE [sales].[Orders] ([Id] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Table(_))
    });

    assert!(table.is_some());
    let full_name = table.unwrap().full_name();
    assert!(
        full_name.contains("sales") && full_name.contains("Orders"),
        "Full name should be [sales].[Orders], got: {}",
        full_name
    );
}

#[test]
fn test_model_element_full_name_schema() {
    let sql = "CREATE TABLE [myschema].[T] ([Id] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let schema = model.elements.iter().find(|e| {
        if let rust_sqlpackage::model::ModelElement::Schema(s) = e {
            s.name.contains("myschema")
        } else {
            false
        }
    });

    assert!(schema.is_some());
    let full_name = schema.unwrap().full_name();
    assert!(
        full_name.contains("myschema"),
        "Full name should contain myschema, got: {}",
        full_name
    );
}
