//! Schema Handling Tests

use super::parse_and_build_model;

// ============================================================================
// Schema Handling Tests
// ============================================================================

#[test]
fn test_extract_dbo_schema() {
    let sql = "CREATE TABLE [dbo].[TestTable] ([Id] INT NOT NULL PRIMARY KEY);";
    let model = parse_and_build_model(sql);

    // Model should contain dbo schema (stored without brackets)
    let has_dbo = model
        .elements
        .iter()
        .any(|e| matches!(e, rust_sqlpackage::model::ModelElement::Schema(s) if s.name == "dbo"));
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
