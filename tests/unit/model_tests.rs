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

    // Model should contain dbo schema
    let has_dbo = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Schema(s) if s.name == "[dbo]")
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
    // Note: The current IndexElement doesn't track included columns separately,
    // so we just verify the index is created successfully
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
