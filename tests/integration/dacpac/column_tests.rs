//! Column property tests
//!
//! Tests for nullable, identity, type specifier, length, precision/scale, and max properties.

use crate::common::{DacpacInfo, TestContext};

use super::{
    find_column_by_name, get_property_value, get_type_specifier_property, parse_model_xml,
};

// ============================================================================
// Column Property Tests (Medium Priority)
// ============================================================================

#[test]
fn test_column_nullable_property() {
    let ctx = TestContext::with_fixture("column_properties");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check required (NOT NULL) column
    let required_col =
        find_column_by_name(&doc, "RequiredName").expect("Should find RequiredName column");
    let required_nullable = get_property_value(&required_col, "IsNullable");
    assert_eq!(
        required_nullable,
        Some("False".to_string()),
        "RequiredName should have IsNullable=False"
    );

    // Check optional (NULL) column
    let optional_col =
        find_column_by_name(&doc, "OptionalName").expect("Should find OptionalName column");
    let optional_nullable = get_property_value(&optional_col, "IsNullable");
    assert_eq!(
        optional_nullable,
        Some("True".to_string()),
        "OptionalName should have IsNullable=True"
    );
}

/// Test that IDENTITY columns have the IsIdentity property set.
/// This test uses a dedicated fixture with IDENTITY syntax.
/// NOTE: Currently fails because sqlparser falls back to RawStatement for IDENTITY columns.
#[test]
fn test_column_identity_property() {
    let ctx = TestContext::with_fixture("identity_column");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check identity column - should find it as SqlSimpleColumn with IsIdentity=True
    let id_col = find_column_by_name(&doc, "IdentityTable].[Id");
    assert!(
        id_col.is_some(),
        "Should find Id column as SqlSimpleColumn (parser currently falls back to RawStatement for IDENTITY)"
    );

    let id_col = id_col.unwrap();
    let is_identity = get_property_value(&id_col, "IsIdentity");
    assert_eq!(
        is_identity,
        Some("True".to_string()),
        "Id column should have IsIdentity=True"
    );
}

#[test]
fn test_column_type_specifier() {
    let ctx = TestContext::with_fixture("column_properties");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Find a column and verify it has TypeSpecifier relationship
    let code_col = find_column_by_name(&doc, "Code").expect("Should find Code column");

    let has_type_specifier = code_col.children().any(|c| {
        c.tag_name().name() == "Relationship" && c.attribute("Name") == Some("TypeSpecifier")
    });

    assert!(
        has_type_specifier,
        "Column should have TypeSpecifier relationship"
    );

    // Verify the type reference
    let type_specifier_rel = code_col
        .children()
        .find(|c| {
            c.tag_name().name() == "Relationship" && c.attribute("Name") == Some("TypeSpecifier")
        })
        .expect("Should have TypeSpecifier relationship");

    let has_type_ref = type_specifier_rel.descendants().any(|d| {
        d.tag_name().name() == "References"
            && d.attribute("ExternalSource") == Some("BuiltIns")
            && d.attribute("Name").is_some_and(|n| n.contains("varchar"))
    });

    assert!(
        has_type_ref,
        "TypeSpecifier should reference [varchar] built-in type"
    );
}

#[test]
fn test_column_length_property() {
    let ctx = TestContext::with_fixture("column_properties");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check varchar(10) column
    let code_col = find_column_by_name(&doc, "Code").expect("Should find Code column");
    let length = get_type_specifier_property(&code_col, "Length");
    assert_eq!(
        length,
        Some("10".to_string()),
        "Code column should have Length=10"
    );

    // Check varchar(50) column
    let short_desc_col =
        find_column_by_name(&doc, "ShortDescription").expect("Should find ShortDescription column");
    let length = get_type_specifier_property(&short_desc_col, "Length");
    assert_eq!(
        length,
        Some("50".to_string()),
        "ShortDescription column should have Length=50"
    );

    // Check char(2) column
    let country_col =
        find_column_by_name(&doc, "CountryCode").expect("Should find CountryCode column");
    let length = get_type_specifier_property(&country_col, "Length");
    assert_eq!(
        length,
        Some("2".to_string()),
        "CountryCode column should have Length=2"
    );
}

#[test]
fn test_column_precision_scale_properties() {
    let ctx = TestContext::with_fixture("column_properties");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check decimal(18, 2) column - Price
    let price_col = find_column_by_name(&doc, "Price").expect("Should find Price column");
    let precision = get_type_specifier_property(&price_col, "Precision");
    let scale = get_type_specifier_property(&price_col, "Scale");
    assert_eq!(
        precision,
        Some("18".to_string()),
        "Price column should have Precision=18"
    );
    assert_eq!(
        scale,
        Some("2".to_string()),
        "Price column should have Scale=2"
    );

    // Check decimal(5, 4) column - TaxRate
    let tax_col = find_column_by_name(&doc, "TaxRate").expect("Should find TaxRate column");
    let precision = get_type_specifier_property(&tax_col, "Precision");
    let scale = get_type_specifier_property(&tax_col, "Scale");
    assert_eq!(
        precision,
        Some("5".to_string()),
        "TaxRate column should have Precision=5"
    );
    assert_eq!(
        scale,
        Some("4".to_string()),
        "TaxRate column should have Scale=4"
    );
}

#[test]
fn test_column_max_property() {
    let ctx = TestContext::with_fixture("column_properties");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check varchar(max) column - LongDescription
    let long_desc_col =
        find_column_by_name(&doc, "LongDescription").expect("Should find LongDescription column");
    let is_max = get_type_specifier_property(&long_desc_col, "IsMax");
    assert_eq!(
        is_max,
        Some("True".to_string()),
        "LongDescription (varchar(max)) column should have IsMax=True"
    );

    // Check nvarchar(max) column - Notes
    let notes_col = find_column_by_name(&doc, "Notes").expect("Should find Notes column");
    let is_max = get_type_specifier_property(&notes_col, "IsMax");
    assert_eq!(
        is_max,
        Some("True".to_string()),
        "Notes (nvarchar(max)) column should have IsMax=True"
    );
}

/// Test that varbinary(max) columns have the IsMax property set.
/// Uses a dedicated fixture to isolate the varbinary(max) type.
/// NOTE: Currently fails due to parser limitations with certain column type combinations.
#[test]
fn test_column_varbinary_max_property() {
    let ctx = TestContext::with_fixture("varbinary_max");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check varbinary(max) column - LargeData
    let large_data_col = find_column_by_name(&doc, "LargeData");
    assert!(
        large_data_col.is_some(),
        "Should find LargeData column as SqlSimpleColumn (parser may fall back for varbinary(max))"
    );

    let large_data_col = large_data_col.unwrap();
    let is_max = get_type_specifier_property(&large_data_col, "IsMax");
    assert_eq!(
        is_max,
        Some("True".to_string()),
        "LargeData (varbinary(max)) column should have IsMax=True"
    );
}
