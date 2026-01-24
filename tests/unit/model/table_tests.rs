//! Table Building Tests

use super::parse_and_build_model;

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
