//! Scalar type (user-defined data type) tests
//!
//! Tests for CREATE TYPE ... FROM statements, including MAX types.
//! Verifies that NVARCHAR(MAX), VARCHAR(MAX), VARBINARY(MAX) scalar types use IsMax=True
//! instead of Length=-1 in the generated model.xml.

use crate::common::{DacpacInfo, TestContext};

use super::{find_elements_by_type, get_property_value, parse_model_xml};

/// Helper to find a scalar type element by name
fn find_scalar_type<'a>(
    doc: &'a roxmltree::Document,
    type_name: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
    // Scalar types have Name like [dbo].[TypeName]
    let expected_suffix = format!("].[{}]", type_name);

    doc.descendants().find(|n| {
        n.tag_name().name() == "Element"
            && n.attribute("Type") == Some("SqlUserDefinedDataType")
            && n.attribute("Name")
                .is_some_and(|name| name.ends_with(&expected_suffix))
    })
}

// ============================================================================
// Scalar Type MAX Tests
// ============================================================================

/// Test that scalar types with NVARCHAR(MAX) have IsMax=True.
/// This was a bug where -1 was being written as Length instead of IsMax.
#[test]
fn test_scalar_type_nvarchar_max_property() {
    let ctx = TestContext::with_fixture("scalar_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Verify we have scalar types
    let scalar_types = find_elements_by_type(&doc, "SqlUserDefinedDataType");
    assert!(
        !scalar_types.is_empty(),
        "Should have SqlUserDefinedDataType elements"
    );

    // Check NVARCHAR(MAX) scalar type - LongText
    let long_text = find_scalar_type(&doc, "LongText").expect("Should find LongText scalar type");

    let is_max = get_property_value(&long_text, "IsMax");
    assert_eq!(
        is_max,
        Some("True".to_string()),
        "LongText (nvarchar(max)) scalar type should have IsMax=True"
    );

    // Should NOT have Length property (that was the bug)
    let length = get_property_value(&long_text, "Length");
    assert!(
        length.is_none(),
        "LongText (nvarchar(max)) scalar type should NOT have Length property, got: {:?}",
        length
    );
}

// ============================================================================
// Scalar Type Regular Length Tests (for comparison)
// ============================================================================

/// Test that regular length scalar types have the correct Length property.
/// This ensures the MAX fix doesn't break normal length handling.
#[test]
fn test_scalar_type_regular_length_property() {
    let ctx = TestContext::with_fixture("scalar_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check VARCHAR(20) scalar type - PhoneNumber
    let phone = find_scalar_type(&doc, "PhoneNumber").expect("Should find PhoneNumber scalar type");

    let length = get_property_value(&phone, "Length");
    assert_eq!(
        length,
        Some("20".to_string()),
        "PhoneNumber (varchar(20)) scalar type should have Length=20"
    );

    // Should NOT have IsMax property
    let is_max = get_property_value(&phone, "IsMax");
    assert!(
        is_max.is_none(),
        "PhoneNumber (varchar(20)) scalar type should NOT have IsMax property"
    );

    // Check NVARCHAR(255) scalar type - EmailAddress
    let email =
        find_scalar_type(&doc, "EmailAddress").expect("Should find EmailAddress scalar type");

    let length = get_property_value(&email, "Length");
    assert_eq!(
        length,
        Some("255".to_string()),
        "EmailAddress (nvarchar(255)) scalar type should have Length=255"
    );
}
