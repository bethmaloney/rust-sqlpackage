//! Origin XML, DacMetadata XML, and Content_Types XML tests
//!
//! Tests for package metadata files.

use crate::common::{DacpacInfo, TestContext};

// ============================================================================
// DacMetadata.xml Validation Tests
// ============================================================================

#[test]
fn test_metadata_xml_structure() {
    let ctx = TestContext::with_fixture("simple_table");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let metadata_xml = info.metadata_xml_content.expect("Should have metadata XML");

    // DacMetadata.xml uses DacType as root element (per MS XSD schema)
    assert!(
        metadata_xml.contains("<DacType") || metadata_xml.contains("DacType"),
        "DacMetadata.xml should have DacType root element (per MS schema)"
    );

    // Empty Description should not be emitted (matches dotnet behavior)
    assert!(
        !metadata_xml.contains("<Description></Description>")
            && !metadata_xml.contains("<Description/>"),
        "DacMetadata.xml should not contain empty Description element"
    );
}

#[test]
fn test_metadata_xml_has_name() {
    let ctx = TestContext::with_fixture("simple_table");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let metadata_xml = info
        .metadata_xml_content
        .expect("Should have DacMetadata.xml content");

    let doc =
        roxmltree::Document::parse(&metadata_xml).expect("DacMetadata.xml should be valid XML");

    // Find Name element
    let name = doc.descendants().find(|n| n.tag_name().name() == "Name");

    assert!(
        name.is_some(),
        "DacMetadata.xml should contain Name element"
    );

    // Verify name has content
    let name_node = name.unwrap();
    let name_text = name_node.text().unwrap_or("");
    assert!(!name_text.is_empty(), "Name element should have a value");
}

#[test]
fn test_metadata_xml_has_version() {
    let ctx = TestContext::with_fixture("simple_table");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let metadata_xml = info
        .metadata_xml_content
        .expect("Should have DacMetadata.xml content");

    let doc =
        roxmltree::Document::parse(&metadata_xml).expect("DacMetadata.xml should be valid XML");

    // Find Version element
    let version = doc.descendants().find(|n| n.tag_name().name() == "Version");

    assert!(
        version.is_some(),
        "DacMetadata.xml should contain Version element"
    );

    // Verify version has content
    let version_node = version.unwrap();
    let version_text = version_node.text().unwrap_or("");
    assert!(
        !version_text.is_empty(),
        "Version element should have a value"
    );
}

// ============================================================================
// Origin.xml Tests
// ============================================================================

#[test]
fn test_origin_xml_has_package_properties() {
    let ctx = TestContext::with_fixture("simple_table");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let origin_xml = info
        .origin_xml_content
        .expect("Should have Origin.xml content");

    let doc = roxmltree::Document::parse(&origin_xml).expect("Origin.xml should be valid XML");

    // Find PackageProperties element
    let package_properties = doc
        .descendants()
        .find(|n| n.tag_name().name() == "PackageProperties");

    assert!(
        package_properties.is_some(),
        "Origin.xml should contain PackageProperties element"
    );
}

#[test]
fn test_origin_xml_has_version() {
    let ctx = TestContext::with_fixture("simple_table");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let origin_xml = info
        .origin_xml_content
        .expect("Should have Origin.xml content");

    let doc = roxmltree::Document::parse(&origin_xml).expect("Origin.xml should be valid XML");

    // Find Version element within PackageProperties
    let version = doc.descendants().find(|n| n.tag_name().name() == "Version");

    assert!(
        version.is_some(),
        "Origin.xml should contain Version element in PackageProperties"
    );

    // Verify version has content
    let version_node = version.unwrap();
    let version_text = version_node.text().unwrap_or("");
    assert!(
        !version_text.is_empty(),
        "Version element should have a value"
    );
}

#[test]
fn test_origin_xml_has_contains_exported_data() {
    let ctx = TestContext::with_fixture("simple_table");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let origin_xml = info
        .origin_xml_content
        .expect("Should have Origin.xml content");

    let doc = roxmltree::Document::parse(&origin_xml).expect("Origin.xml should be valid XML");

    // Find ContainsExportedData element
    let contains_data = doc
        .descendants()
        .find(|n| n.tag_name().name() == "ContainsExportedData");

    assert!(
        contains_data.is_some(),
        "Origin.xml should contain ContainsExportedData element"
    );

    // Verify it has a boolean value
    let contains_data_node = contains_data.unwrap();
    let value = contains_data_node.text().unwrap_or("");
    assert!(
        value == "true" || value == "false" || value == "True" || value == "False",
        "ContainsExportedData should be a boolean value, got: '{}'",
        value
    );
}

// ============================================================================
// Content_Types.xml Tests
// ============================================================================

#[test]
fn test_content_types_has_correct_mime_types() {
    let ctx = TestContext::with_fixture("simple_table");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let content_types_xml = info
        .content_types_xml_content
        .expect("Should have [Content_Types].xml content");

    let doc = roxmltree::Document::parse(&content_types_xml)
        .expect("[Content_Types].xml should be valid XML");

    // Find all Override elements with ContentType
    let overrides: Vec<_> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "Override" || n.tag_name().name() == "Default")
        .collect();

    assert!(
        !overrides.is_empty(),
        "[Content_Types].xml should contain Override or Default elements"
    );

    // Verify XML content types are defined
    let has_xml_type = overrides.iter().any(|n| {
        n.attribute("ContentType")
            .is_some_and(|ct| ct.contains("xml"))
    });

    assert!(
        has_xml_type,
        "[Content_Types].xml should define XML content types. Found: {:?}",
        overrides
            .iter()
            .filter_map(|n| n.attribute("ContentType"))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// XSD Schema Validation Tests
// ============================================================================
// These tests validate generated model.xml against the official Microsoft XSD schema.
// Enable with: cargo test --features xsd-validation

#[cfg(feature = "xsd-validation")]
mod xsd_validation {
    use super::*;
    use libxml::parser::Parser;
    use libxml::schemas::{SchemaParserContext, SchemaValidationContext};
    use std::path::Path;

    const XSD_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/dacpac.xsd");

    /// Validates model.xml from all test fixtures against the official Microsoft XSD schema.
    /// Uses a single schema parse to keep test execution fast.
    #[test]
    fn test_model_xml_validates_against_xsd() {
        let xsd_path = Path::new(XSD_PATH);
        assert!(
            xsd_path.exists(),
            "XSD schema file not found at {}",
            XSD_PATH
        );

        // Parse the XSD schema once
        let mut schema_parser = SchemaParserContext::from_file(xsd_path.to_str().unwrap());
        let mut validation_ctx = SchemaValidationContext::from_parser(&mut schema_parser)
            .expect("Failed to create schema validation context");

        let xml_parser = Parser::default();

        // Test fixtures to validate - all fixtures that produce valid dacpacs
        let fixtures = [
            "all_constraints",
            "build_with_exclude",
            "column_properties",
            "constraints",
            "element_types",
            "empty_project",
            "external_reference",
            "identity_column",
            "index_properties",
            "indexes",
            "large_table",
            "multiple_indexes",
            "only_schemas",
            "pre_post_deploy",
            "reserved_keywords",
            "self_ref_fk",
            "simple_table",
            "sqlcmd_includes",
            "unicode_identifiers",
            "unresolved_reference",
            "varbinary_max",
            "views",
        ];
        let mut failures: Vec<String> = Vec::new();

        for fixture in fixtures {
            let ctx = TestContext::with_fixture(fixture);
            let result = ctx.build();

            if !result.success {
                failures.push(format!("{}: build failed", fixture));
                continue;
            }

            let dacpac_path = result.dacpac_path.unwrap();
            let info = match DacpacInfo::from_dacpac(&dacpac_path) {
                Ok(i) => i,
                Err(e) => {
                    failures.push(format!("{}: failed to parse dacpac: {}", fixture, e));
                    continue;
                }
            };

            let model_xml = match info.model_xml_content {
                Some(xml) => xml,
                None => {
                    failures.push(format!("{}: no model.xml in dacpac", fixture));
                    continue;
                }
            };

            let doc = match xml_parser.parse_string(&model_xml) {
                Ok(d) => d,
                Err(e) => {
                    failures.push(format!("{}: failed to parse model.xml: {:?}", fixture, e));
                    continue;
                }
            };

            match validation_ctx.validate_document(&doc) {
                Ok(()) => println!("{}: model.xml validates successfully", fixture),
                Err(errors) => {
                    let error_msgs: Vec<String> = errors
                        .iter()
                        .map(|e| {
                            e.message
                                .clone()
                                .unwrap_or_else(|| "Unknown error".to_string())
                        })
                        .collect();
                    failures.push(format!(
                        "{}: XSD validation failed with {} errors:\n  {}",
                        fixture,
                        error_msgs.len(),
                        error_msgs.join("\n  ")
                    ));
                }
            }
        }

        if !failures.is_empty() {
            panic!(
                "XSD validation failed for {} fixture(s):\n\n{}",
                failures.len(),
                failures.join("\n\n")
            );
        }
    }
}
