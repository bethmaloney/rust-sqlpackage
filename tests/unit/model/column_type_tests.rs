//! Column Type Parameter Tests and Default Value Tests

use super::parse_and_build_model;

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
    assert_eq!(
        col.max_length,
        Some(100),
        "VARCHAR(100) should have max_length 100"
    );
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
    assert_eq!(
        col.max_length,
        Some(255),
        "NVARCHAR(255) should have max_length 255"
    );
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
    assert_eq!(
        col.max_length,
        Some(-1),
        "VARCHAR(MAX) should have max_length -1"
    );
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
    assert_eq!(
        col.precision,
        Some(18),
        "DECIMAL(18,2) should have precision 18"
    );
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
    assert_eq!(
        col.precision,
        Some(10),
        "NUMERIC(10,4) should have precision 10"
    );
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
    assert_eq!(
        col.max_length,
        Some(10),
        "CHAR(10) should have max_length 10"
    );
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
