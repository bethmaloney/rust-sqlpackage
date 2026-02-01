//! TVF (Table-Valued Function) column property tests
//!
//! Tests for multi-statement TVF column type specifiers, including MAX types.
//! Verifies that NVARCHAR(MAX), VARCHAR(MAX), VARBINARY(MAX) columns use IsMax=True
//! instead of Length=4294967295 in the generated model.xml.

use crate::common::TestContext;

use super::{find_elements_by_type, get_property_value, parse_dacpac_model, parse_model_xml};

/// Helper to find a TVF column by name within a multi-statement TVF
fn find_tvf_column<'a>(
    doc: &'a roxmltree::Document,
    func_name: &str,
    column_name: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
    // TVF columns are nested as: SqlMultiStatementTableValuedFunction -> Columns relationship -> SqlSimpleColumn
    // The column name format is: [dbo].[FuncName].[ColumnName]
    let expected_suffix = format!("].[{}].[{}]", func_name, column_name);

    doc.descendants().find(|n| {
        n.tag_name().name() == "Element"
            && n.attribute("Type") == Some("SqlSimpleColumn")
            && n.attribute("Name")
                .is_some_and(|name| name.ends_with(&expected_suffix))
    })
}

/// Helper to get a property from a TVF column's TypeSpecifier
fn get_tvf_type_specifier_property(
    column: &roxmltree::Node,
    property_name: &str,
) -> Option<String> {
    // Navigate: Column -> TypeSpecifier relationship -> Entry -> SqlTypeSpecifier element -> Property
    column
        .children()
        .find(|c| {
            c.tag_name().name() == "Relationship" && c.attribute("Name") == Some("TypeSpecifier")
        })
        .and_then(|rel| rel.children().find(|c| c.tag_name().name() == "Entry"))
        .and_then(|entry| entry.children().find(|c| c.tag_name().name() == "Element"))
        .and_then(|elem| get_property_value(&elem, property_name))
}

// ============================================================================
// TVF Column MAX Type Tests
// ============================================================================

/// Test that NVARCHAR(MAX) columns in multi-statement TVFs have IsMax=True.
/// This was a bug where u32::MAX was being written as Length instead of IsMax.
#[test]
fn test_tvf_column_nvarchar_max_property() {
    let ctx = TestContext::with_fixture("tvf_max_columns");
    let dacpac_path = ctx.build_successfully();

    let (_info, model_xml) = parse_dacpac_model(&dacpac_path);
    let doc = parse_model_xml(&model_xml);

    // Verify we have the TVF
    let tvfs = find_elements_by_type(&doc, "SqlMultiStatementTableValuedFunction");
    assert!(
        !tvfs.is_empty(),
        "Should have SqlMultiStatementTableValuedFunction"
    );

    // Check NVARCHAR(MAX) column - Content
    let content_col = find_tvf_column(&doc, "GetDocuments", "Content")
        .expect("Should find Content column in TVF");

    let is_max = get_tvf_type_specifier_property(&content_col, "IsMax");
    assert_eq!(
        is_max,
        Some("True".to_string()),
        "Content (nvarchar(max)) TVF column should have IsMax=True"
    );

    // Should NOT have Length property (that was the bug)
    let length = get_tvf_type_specifier_property(&content_col, "Length");
    assert!(
        length.is_none(),
        "Content (nvarchar(max)) TVF column should NOT have Length property, got: {:?}",
        length
    );
}

/// Test that VARCHAR(MAX) columns in multi-statement TVFs have IsMax=True.
#[test]
fn test_tvf_column_varchar_max_property() {
    let ctx = TestContext::with_fixture("tvf_max_columns");
    let dacpac_path = ctx.build_successfully();

    let (_info, model_xml) = parse_dacpac_model(&dacpac_path);
    let doc = parse_model_xml(&model_xml);

    // Check VARCHAR(MAX) column - Description
    let desc_col = find_tvf_column(&doc, "GetDocuments", "Description")
        .expect("Should find Description column in TVF");

    let is_max = get_tvf_type_specifier_property(&desc_col, "IsMax");
    assert_eq!(
        is_max,
        Some("True".to_string()),
        "Description (varchar(max)) TVF column should have IsMax=True"
    );

    // Should NOT have Length property
    let length = get_tvf_type_specifier_property(&desc_col, "Length");
    assert!(
        length.is_none(),
        "Description (varchar(max)) TVF column should NOT have Length property, got: {:?}",
        length
    );
}

/// Test that VARBINARY(MAX) columns in multi-statement TVFs have IsMax=True.
#[test]
fn test_tvf_column_varbinary_max_property() {
    let ctx = TestContext::with_fixture("tvf_max_columns");
    let dacpac_path = ctx.build_successfully();

    let (_info, model_xml) = parse_dacpac_model(&dacpac_path);
    let doc = parse_model_xml(&model_xml);

    // Check VARBINARY(MAX) column - BinaryData
    let binary_col = find_tvf_column(&doc, "GetDocuments", "BinaryData")
        .expect("Should find BinaryData column in TVF");

    let is_max = get_tvf_type_specifier_property(&binary_col, "IsMax");
    assert_eq!(
        is_max,
        Some("True".to_string()),
        "BinaryData (varbinary(max)) TVF column should have IsMax=True"
    );

    // Should NOT have Length property
    let length = get_tvf_type_specifier_property(&binary_col, "Length");
    assert!(
        length.is_none(),
        "BinaryData (varbinary(max)) TVF column should NOT have Length property, got: {:?}",
        length
    );
}

// ============================================================================
// TVF Column Regular Length Tests (for comparison)
// ============================================================================

/// Test that regular length columns in TVFs have the correct Length property.
/// This ensures the MAX fix doesn't break normal length handling.
#[test]
fn test_tvf_column_regular_length_property() {
    let ctx = TestContext::with_fixture("tvf_max_columns");
    let dacpac_path = ctx.build_successfully();

    let (_info, model_xml) = parse_dacpac_model(&dacpac_path);
    let doc = parse_model_xml(&model_xml);

    // Check NVARCHAR(200) column - Title
    let title_col =
        find_tvf_column(&doc, "GetDocuments", "Title").expect("Should find Title column in TVF");

    let length = get_tvf_type_specifier_property(&title_col, "Length");
    assert_eq!(
        length,
        Some("200".to_string()),
        "Title (nvarchar(200)) TVF column should have Length=200"
    );

    // Should NOT have IsMax property
    let is_max = get_tvf_type_specifier_property(&title_col, "IsMax");
    assert!(
        is_max.is_none(),
        "Title (nvarchar(200)) TVF column should NOT have IsMax property"
    );

    // Check VARCHAR(10) column - ShortCode
    let code_col = find_tvf_column(&doc, "GetDocuments", "ShortCode")
        .expect("Should find ShortCode column in TVF");

    let length = get_tvf_type_specifier_property(&code_col, "Length");
    assert_eq!(
        length,
        Some("10".to_string()),
        "ShortCode (varchar(10)) TVF column should have Length=10"
    );
}
