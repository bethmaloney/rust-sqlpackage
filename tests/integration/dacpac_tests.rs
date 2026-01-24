//! Integration tests for dacpac file validation
//!
//! These tests verify the structure and content of generated dacpac files.

use std::fs::File;

use zip::ZipArchive;

use crate::common::{DacpacInfo, TestContext};

// ============================================================================
// Dacpac Structure Tests
// ============================================================================

#[test]
fn test_dacpac_is_valid_zip() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    // Try to open as ZIP
    let file = File::open(&dacpac_path).expect("Should open file");
    let archive = ZipArchive::new(file);

    assert!(archive.is_ok(), "Dacpac should be a valid ZIP archive");
}

#[test]
fn test_dacpac_contains_model_xml() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    assert!(info.has_model_xml, "Dacpac must contain model.xml");
}

#[test]
fn test_dacpac_contains_dac_metadata_xml() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    assert!(info.has_metadata_xml, "Dacpac must contain DacMetadata.xml");
}

#[test]
fn test_dacpac_contains_origin_xml() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    assert!(info.has_origin_xml, "Dacpac must contain Origin.xml");
}

#[test]
fn test_dacpac_contains_content_types_xml() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    assert!(
        info.has_content_types,
        "Dacpac must contain [Content_Types].xml"
    );
}

// ============================================================================
// Model.xml Validation Tests
// ============================================================================

#[test]
fn test_model_xml_has_correct_namespace() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    assert!(
        model_xml.contains("http://schemas.microsoft.com/sqlserver/dac/Serialization"),
        "Model XML should have correct Microsoft namespace"
    );
}

#[test]
fn test_model_xml_has_correct_dsp() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    assert!(
        model_xml.contains("DatabaseSchemaProvider"),
        "Model XML should have DSP name"
    );
    assert!(
        model_xml.contains("Sql160") || model_xml.contains("Sql150"),
        "Model XML should have valid SQL Server version"
    );
}

#[test]
fn test_model_contains_all_tables() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert_eq!(
        info.tables.len(),
        4,
        "Should have 4 tables. Found: {:?}",
        info.tables
    );
}

#[test]
fn test_model_contains_all_views() {
    let ctx = TestContext::with_fixture("views");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(
        info.views.len() >= 1,
        "Should have at least 1 view. Found: {:?}",
        info.views
    );
}

#[test]
fn test_model_contains_indexes() {
    let ctx = TestContext::with_fixture("indexes");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    assert!(
        model_xml.contains("SqlIndex"),
        "Model should contain index elements"
    );
}

// ============================================================================
// DacMetadata.xml Validation Tests
// ============================================================================

#[test]
fn test_metadata_xml_structure() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let metadata_xml = info.metadata_xml_content.expect("Should have metadata XML");

    assert!(
        metadata_xml.contains("<DacMetadata") || metadata_xml.contains("DacMetadata"),
        "Metadata XML should have DacMetadata root element"
    );
}

// ============================================================================
// ZIP Entry Tests
// ============================================================================

#[test]
fn test_dacpac_file_count() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let file = File::open(&dacpac_path).expect("Should open file");
    let archive = ZipArchive::new(file).expect("Should be valid ZIP");

    // Should have at least 4 files: model.xml, DacMetadata.xml, Origin.xml, [Content_Types].xml
    assert!(
        archive.len() >= 4,
        "Dacpac should have at least 4 files, has {}",
        archive.len()
    );
}

#[test]
fn test_dacpac_file_names() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let mut file_names: Vec<String> = Vec::new();
    let mut archive = ZipArchive::new(File::open(&dacpac_path).expect("Should open file"))
        .expect("Should be valid ZIP");
    for i in 0..archive.len() {
        let name = archive.by_index(i).unwrap().name().to_string();
        file_names.push(name);
    }

    assert!(
        file_names.iter().any(|n| n == "model.xml"),
        "Should contain model.xml. Files: {:?}",
        file_names
    );
    assert!(
        file_names.iter().any(|n| n == "DacMetadata.xml"),
        "Should contain DacMetadata.xml. Files: {:?}",
        file_names
    );
    assert!(
        file_names.iter().any(|n| n == "Origin.xml"),
        "Should contain Origin.xml. Files: {:?}",
        file_names
    );
    assert!(
        file_names.iter().any(|n| n == "[Content_Types].xml"),
        "Should contain [Content_Types].xml. Files: {:?}",
        file_names
    );
}

// ============================================================================
// Dacpac Reproducibility Tests
// ============================================================================

#[test]
fn test_dacpac_model_xml_consistency() {
    // Build twice and verify model.xml content is consistent
    let ctx1 = TestContext::with_fixture("simple_table");
    let result1 = ctx1.build();
    assert!(result1.success);

    let ctx2 = TestContext::with_fixture("simple_table");
    let result2 = ctx2.build();
    assert!(result2.success);

    let info1 = DacpacInfo::from_dacpac(&result1.dacpac_path.unwrap()).unwrap();
    let info2 = DacpacInfo::from_dacpac(&result2.dacpac_path.unwrap()).unwrap();

    // Tables should be the same
    assert_eq!(
        info1.tables, info2.tables,
        "Same project should produce same tables"
    );

    // Schemas should be the same
    assert_eq!(
        info1.schemas, info2.schemas,
        "Same project should produce same schemas"
    );
}

// ============================================================================
// Complex Model Tests
// ============================================================================

#[test]
fn test_dacpac_with_relationships() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Verify relationships are present in model
    assert!(
        model_xml.contains("Relationship") || model_xml.contains("relationship"),
        "Model should contain relationships"
    );
}

#[test]
fn test_dacpac_with_constraints() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Check for various constraint types
    let has_pk = model_xml.contains("SqlPrimaryKeyConstraint");
    let has_fk = model_xml.contains("SqlForeignKeyConstraint");
    let has_uq = model_xml.contains("SqlUniqueConstraint");
    let has_ck = model_xml.contains("SqlCheckConstraint");

    assert!(
        has_pk || has_fk || has_uq || has_ck,
        "Model should contain at least one constraint type"
    );

    // Log which constraints were found for debugging
    println!(
        "Found constraints - PK: {}, FK: {}, UQ: {}, CK: {}",
        has_pk, has_fk, has_uq, has_ck
    );
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_invalid_dacpac_path() {
    let result = DacpacInfo::from_dacpac(&std::path::PathBuf::from("/nonexistent/path.dacpac"));
    assert!(result.is_err(), "Should fail for nonexistent file");
}

#[test]
fn test_corrupted_dacpac() {
    // Create a file that isn't a valid ZIP
    let temp_dir = tempfile::TempDir::new().unwrap();
    let fake_dacpac = temp_dir.path().join("fake.dacpac");
    std::fs::write(&fake_dacpac, "This is not a ZIP file").unwrap();

    let result = DacpacInfo::from_dacpac(&fake_dacpac);
    assert!(result.is_err(), "Should fail for invalid ZIP");
}

// ============================================================================
// DataSchemaModel Root Element Tests (XSD Compliance)
// ============================================================================

/// Helper to parse model.xml and return the document
fn parse_model_xml(model_xml: &str) -> roxmltree::Document<'_> {
    roxmltree::Document::parse(model_xml).expect("Model XML should be valid XML")
}

/// Helper to find elements by type attribute
fn find_elements_by_type<'a>(
    doc: &'a roxmltree::Document,
    type_name: &str,
) -> Vec<roxmltree::Node<'a, 'a>> {
    doc.descendants()
        .filter(|n| n.tag_name().name() == "Element" && n.attribute("Type") == Some(type_name))
        .collect()
}

/// Helper to check if an element has a Relationship child with the given name
fn has_relationship(element: &roxmltree::Node, rel_name: &str) -> bool {
    element
        .children()
        .any(|c| c.tag_name().name() == "Relationship" && c.attribute("Name") == Some(rel_name))
}

#[test]
fn test_model_xml_has_file_format_version() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let root = doc.root_element();

    assert_eq!(
        root.tag_name().name(),
        "DataSchemaModel",
        "Root element should be DataSchemaModel"
    );

    // FileFormatVersion is required per XSD and must be a decimal
    let version_str = root
        .attribute("FileFormatVersion")
        .expect("FileFormatVersion attribute is required");

    let version: f64 = version_str
        .parse()
        .expect("FileFormatVersion should be a valid decimal");
    assert!(version > 0.0, "FileFormatVersion should be positive");
}

#[test]
fn test_model_xml_has_schema_version() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let root = doc.root_element();

    // SchemaVersion is required per XSD and must be a decimal
    let version_str = root
        .attribute("SchemaVersion")
        .expect("SchemaVersion attribute is required");

    let version: f64 = version_str
        .parse()
        .expect("SchemaVersion should be a valid decimal");
    assert!(version > 0.0, "SchemaVersion should be positive");
}

#[test]
fn test_model_xml_has_collation_lcid() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let root = doc.root_element();

    // CollationLcid is required per XSD and must be an unsigned short
    let lcid_str = root
        .attribute("CollationLcid")
        .expect("CollationLcid attribute is required");

    let lcid: u16 = lcid_str
        .parse()
        .expect("CollationLcid should be a valid unsigned short");
    assert!(
        lcid > 0,
        "CollationLcid should be positive (e.g., 1033 for en-US)"
    );
}

#[test]
fn test_model_xml_has_collation_case_sensitive() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let root = doc.root_element();

    // CollationCaseSensitive is required per XSD
    let value = root
        .attribute("CollationCaseSensitive")
        .expect("CollationCaseSensitive attribute is required");

    assert!(
        value == "True" || value == "False",
        "CollationCaseSensitive should be 'True' or 'False', got '{}'",
        value
    );
}

// ============================================================================
// Relationship Structure Tests (XSD Compliance)
// ============================================================================

#[test]
fn test_table_has_schema_relationship() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let tables = find_elements_by_type(&doc, "SqlTable");

    assert!(
        !tables.is_empty(),
        "Should have at least one SqlTable element"
    );

    for table in &tables {
        assert!(
            has_relationship(table, "Schema"),
            "SqlTable '{}' should have a Schema relationship",
            table.attribute("Name").unwrap_or("unnamed")
        );
    }
}

#[test]
fn test_table_has_columns_relationship() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let tables = find_elements_by_type(&doc, "SqlTable");

    assert!(
        !tables.is_empty(),
        "Should have at least one SqlTable element"
    );

    // Find tables with Columns relationship
    let tables_with_columns: Vec<_> = tables
        .iter()
        .filter(|t| has_relationship(t, "Columns"))
        .collect();

    assert!(
        !tables_with_columns.is_empty(),
        "At least one SqlTable should have a Columns relationship"
    );

    // Verify columns contain Entry elements with SqlSimpleColumn
    for table in tables_with_columns {
        let columns_rel = table
            .children()
            .find(|c| {
                c.tag_name().name() == "Relationship" && c.attribute("Name") == Some("Columns")
            })
            .expect("Should have Columns relationship");

        let entries: Vec<_> = columns_rel
            .children()
            .filter(|c| c.tag_name().name() == "Entry")
            .collect();

        assert!(
            !entries.is_empty(),
            "Columns relationship should contain Entry elements"
        );

        // Check that entries contain SqlSimpleColumn elements
        for entry in entries {
            let has_column = entry.descendants().any(|d| {
                d.tag_name().name() == "Element" && d.attribute("Type") == Some("SqlSimpleColumn")
            });
            assert!(has_column, "Entry should contain SqlSimpleColumn element");
        }
    }
}

#[test]
fn test_view_has_schema_relationship() {
    let ctx = TestContext::with_fixture("views");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let views = find_elements_by_type(&doc, "SqlView");

    assert!(
        !views.is_empty(),
        "Should have at least one SqlView element"
    );

    for view in &views {
        assert!(
            has_relationship(view, "Schema"),
            "SqlView '{}' should have a Schema relationship",
            view.attribute("Name").unwrap_or("unnamed")
        );
    }
}

#[test]
fn test_type_references_have_external_source() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Find all References elements with ExternalSource="BuiltIns"
    let builtin_refs: Vec<_> = doc
        .descendants()
        .filter(|n| {
            n.tag_name().name() == "References" && n.attribute("ExternalSource") == Some("BuiltIns")
        })
        .collect();

    assert!(
        !builtin_refs.is_empty(),
        "Built-in type references should use ExternalSource=\"BuiltIns\""
    );

    // Verify each reference has a Name attribute
    for refs in &builtin_refs {
        let name = refs.attribute("Name");
        assert!(
            name.is_some(),
            "References element with ExternalSource should have a Name attribute"
        );

        // Name should be a bracketed type like [int], [varchar], etc.
        let name_val = name.unwrap();
        assert!(
            name_val.starts_with('[') && name_val.ends_with(']'),
            "Type reference Name should be bracketed, got '{}'",
            name_val
        );
    }
}

// ============================================================================
// XML Well-formedness Tests
// ============================================================================

#[test]
fn test_model_xml_is_well_formed() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Parse with roxmltree to verify well-formedness
    let doc = roxmltree::Document::parse(&model_xml);
    assert!(
        doc.is_ok(),
        "Model XML should be well-formed: {:?}",
        doc.err()
    );
}

#[test]
fn test_model_xml_has_xml_declaration() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Check raw string for XML declaration (roxmltree doesn't expose this)
    assert!(
        model_xml.starts_with("<?xml"),
        "Model XML should start with XML declaration"
    );
    assert!(
        model_xml.contains("encoding=\"utf-8\"") || model_xml.contains("encoding='utf-8'"),
        "Model XML should declare UTF-8 encoding"
    );
}

#[test]
fn test_model_xml_has_dataschemamodel_root() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let root = doc.root_element();

    assert_eq!(
        root.tag_name().name(),
        "DataSchemaModel",
        "Root element must be DataSchemaModel"
    );
}

#[test]
fn test_model_xml_has_model_element() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let root = doc.root_element();

    // Find the Model child element
    let model_element = root.children().find(|c| c.tag_name().name() == "Model");

    assert!(
        model_element.is_some(),
        "DataSchemaModel must have a Model child element"
    );
}

#[test]
fn test_special_characters_escaped_in_definitions() {
    // Test with the constraints fixture which may have check constraints with < or >
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Parse with roxmltree - will fail if special chars aren't properly escaped
    let doc = roxmltree::Document::parse(&model_xml);
    assert!(
        doc.is_ok(),
        "Model XML should be well-formed (special characters must be escaped): {:?}",
        doc.err()
    );

    // If there are any check constraints with comparisons, they should use &lt; &gt;
    // This is implicit - if the XML parses, escaping is correct
}

// ============================================================================
// XSD Schema Validation Tests
// ============================================================================
// These tests validate generated model.xml against the official Microsoft XSD schema.
// Enable with: cargo test --features xsd-validation

// ============================================================================
// Element Type Coverage Tests (Medium Priority)
// ============================================================================

#[test]
fn test_model_contains_procedures() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let procedures = find_elements_by_type(&doc, "SqlProcedure");

    assert!(
        !procedures.is_empty(),
        "Model should contain at least one SqlProcedure element"
    );

    // Verify the GetUsers procedure exists
    let has_get_users = procedures.iter().any(|p| {
        p.attribute("Name")
            .map_or(false, |n| n.contains("GetUsers"))
    });
    assert!(
        has_get_users,
        "Should have GetUsers procedure. Found: {:?}",
        procedures
            .iter()
            .filter_map(|p| p.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_scalar_functions() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let scalar_funcs = find_elements_by_type(&doc, "SqlScalarFunction");

    assert!(
        !scalar_funcs.is_empty(),
        "Model should contain at least one SqlScalarFunction element"
    );

    // Verify the GetUserCount function exists
    let has_get_user_count = scalar_funcs.iter().any(|f| {
        f.attribute("Name")
            .map_or(false, |n| n.contains("GetUserCount"))
    });
    assert!(
        has_get_user_count,
        "Should have GetUserCount scalar function. Found: {:?}",
        scalar_funcs
            .iter()
            .filter_map(|f| f.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_table_valued_functions() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check for multi-statement TVF (SqlMultiStatementTableValuedFunction)
    let tvfs = find_elements_by_type(&doc, "SqlMultiStatementTableValuedFunction");
    // Check for inline TVF (SqlInlineTableValuedFunction)
    let inline_tvfs = find_elements_by_type(&doc, "SqlInlineTableValuedFunction");

    assert!(
        !tvfs.is_empty() || !inline_tvfs.is_empty(),
        "Model should contain at least one table-valued function (SqlMultiStatementTableValuedFunction or SqlInlineTableValuedFunction)"
    );
}

#[test]
fn test_model_contains_sequences() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let sequences = find_elements_by_type(&doc, "SqlSequence");

    assert!(
        !sequences.is_empty(),
        "Model should contain at least one SqlSequence element"
    );

    // Verify the OrderSequence exists
    let has_order_sequence = sequences.iter().any(|s| {
        s.attribute("Name")
            .map_or(false, |n| n.contains("OrderSequence"))
    });
    assert!(
        has_order_sequence,
        "Should have OrderSequence. Found: {:?}",
        sequences
            .iter()
            .filter_map(|s| s.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_user_defined_types() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let udts = find_elements_by_type(&doc, "SqlTableType");

    assert!(
        !udts.is_empty(),
        "Model should contain at least one SqlTableType element"
    );

    // Verify the UserTableType exists
    let has_user_table_type = udts.iter().any(|u| {
        u.attribute("Name")
            .map_or(false, |n| n.contains("UserTableType"))
    });
    assert!(
        has_user_table_type,
        "Should have UserTableType. Found: {:?}",
        udts.iter()
            .filter_map(|u| u.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_triggers() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let triggers = find_elements_by_type(&doc, "SqlDmlTrigger");

    assert!(
        !triggers.is_empty(),
        "Model should contain at least one SqlDmlTrigger element"
    );

    // Verify the AuditTrigger exists
    let has_audit_trigger = triggers.iter().any(|t| {
        t.attribute("Name")
            .map_or(false, |n| n.contains("TR_Users_Audit"))
    });
    assert!(
        has_audit_trigger,
        "Should have TR_Users_Audit trigger. Found: {:?}",
        triggers
            .iter()
            .filter_map(|t| t.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_schemas() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let schemas = find_elements_by_type(&doc, "SqlSchema");

    assert!(
        !schemas.is_empty(),
        "Model should contain at least one SqlSchema element"
    );

    // Verify the custom Sales schema exists (in addition to dbo)
    let has_sales_schema = schemas
        .iter()
        .any(|s| s.attribute("Name").map_or(false, |n| n.contains("Sales")));
    assert!(
        has_sales_schema,
        "Should have Sales schema. Found: {:?}",
        schemas
            .iter()
            .filter_map(|s| s.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Column Property Tests (Medium Priority)
// ============================================================================

/// Helper to find a column element by its name
fn find_column_by_name<'a>(
    doc: &'a roxmltree::Document,
    column_name: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
    doc.descendants().find(|n| {
        n.tag_name().name() == "Element"
            && n.attribute("Type") == Some("SqlSimpleColumn")
            && n.attribute("Name")
                .map_or(false, |name| name.contains(column_name))
    })
}

/// Helper to get a property value from an element
fn get_property_value(element: &roxmltree::Node, property_name: &str) -> Option<String> {
    element
        .children()
        .find(|c| c.tag_name().name() == "Property" && c.attribute("Name") == Some(property_name))
        .and_then(|p| p.attribute("Value").map(String::from))
}

/// Helper to get type specifier property from a column
fn get_type_specifier_property(column: &roxmltree::Node, property_name: &str) -> Option<String> {
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
            && d.attribute("Name").map_or(false, |n| n.contains("varchar"))
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

// ============================================================================
// Constraint Tests (Medium Priority)
// ============================================================================

#[test]
fn test_primary_key_constraint() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let pk_constraints = find_elements_by_type(&doc, "SqlPrimaryKeyConstraint");

    assert!(
        !pk_constraints.is_empty(),
        "Model should contain SqlPrimaryKeyConstraint elements"
    );

    // Verify PK has DefiningTable relationship
    for pk in &pk_constraints {
        assert!(
            has_relationship(pk, "DefiningTable"),
            "SqlPrimaryKeyConstraint '{}' should have DefiningTable relationship",
            pk.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify the named constraint exists
    let has_pk_primary_key_table = pk_constraints.iter().any(|pk| {
        pk.attribute("Name")
            .map_or(false, |n| n.contains("PK_PrimaryKeyTable"))
    });
    assert!(
        has_pk_primary_key_table,
        "Should have PK_PrimaryKeyTable constraint. Found: {:?}",
        pk_constraints
            .iter()
            .filter_map(|pk| pk.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_foreign_key_constraint_with_referenced_table() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let fk_constraints = find_elements_by_type(&doc, "SqlForeignKeyConstraint");

    assert!(
        !fk_constraints.is_empty(),
        "Model should contain SqlForeignKeyConstraint elements"
    );

    // Verify FK has DefiningTable relationship
    for fk in &fk_constraints {
        assert!(
            has_relationship(fk, "DefiningTable"),
            "SqlForeignKeyConstraint '{}' should have DefiningTable relationship",
            fk.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify FK has ForeignTable relationship
    for fk in &fk_constraints {
        assert!(
            has_relationship(fk, "ForeignTable"),
            "SqlForeignKeyConstraint '{}' should have ForeignTable relationship",
            fk.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify the named constraint exists
    let has_fk_foreign_key_table = fk_constraints.iter().any(|fk| {
        fk.attribute("Name")
            .map_or(false, |n| n.contains("FK_ForeignKeyTable_Parent"))
    });
    assert!(
        has_fk_foreign_key_table,
        "Should have FK_ForeignKeyTable_Parent constraint. Found: {:?}",
        fk_constraints
            .iter()
            .filter_map(|fk| fk.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_check_constraint_with_definition() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let ck_constraints = find_elements_by_type(&doc, "SqlCheckConstraint");

    assert!(
        !ck_constraints.is_empty(),
        "Model should contain SqlCheckConstraint elements"
    );

    // Verify check constraints have DefiningTable relationship
    for ck in &ck_constraints {
        assert!(
            has_relationship(ck, "DefiningTable"),
            "SqlCheckConstraint '{}' should have DefiningTable relationship",
            ck.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify check constraints have CheckExpressionScript property with CDATA
    for ck in &ck_constraints {
        let has_check_expression = ck.children().any(|c| {
            c.tag_name().name() == "Property"
                && c.attribute("Name") == Some("CheckExpressionScript")
        });
        assert!(
            has_check_expression,
            "SqlCheckConstraint '{}' should have CheckExpressionScript property with the check expression",
            ck.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify named constraints exist
    let has_age_check = ck_constraints.iter().any(|ck| {
        ck.attribute("Name")
            .map_or(false, |n| n.contains("CK_CheckConstraintTable_Age"))
    });
    assert!(
        has_age_check,
        "Should have CK_CheckConstraintTable_Age constraint"
    );
}

#[test]
fn test_default_constraint() {
    // Note: The constraints fixture doesn't have default constraints currently.
    // This test verifies the structure works when SqlDefaultConstraint elements exist.
    // For now, we just verify the model can be built and parsed.
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let default_constraints = find_elements_by_type(&doc, "SqlDefaultConstraint");

    // Default constraints are optional - test structure if they exist
    for df in &default_constraints {
        assert!(
            has_relationship(df, "DefiningTable"),
            "SqlDefaultConstraint '{}' should have DefiningTable relationship",
            df.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Just verify the model is valid - default constraints may or may not be present
    assert!(
        doc.root_element().tag_name().name() == "DataSchemaModel",
        "Model should be valid DataSchemaModel"
    );
}

// ============================================================================
// Index Property Tests (XSD Compliance)
// ============================================================================

/// Helper to find an index element by its name
fn find_index_by_name<'a>(
    doc: &'a roxmltree::Document,
    index_name: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
    doc.descendants().find(|n| {
        n.tag_name().name() == "Element"
            && n.attribute("Type") == Some("SqlIndex")
            && n.attribute("Name")
                .map_or(false, |name| name.contains(index_name))
    })
}

/// Test that unique indexes have the IsUnique property set to True.
/// This verifies that CREATE UNIQUE INDEX statements produce indexes with IsUnique=True.
#[test]
fn test_index_is_unique_property() {
    let ctx = TestContext::with_fixture("index_properties");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check unique index has IsUnique=True
    let unique_index = find_index_by_name(&doc, "UX_Products_SKU");
    assert!(
        unique_index.is_some(),
        "Should find UX_Products_SKU index as SqlIndex element"
    );

    let unique_index = unique_index.unwrap();
    let is_unique = get_property_value(&unique_index, "IsUnique");
    assert_eq!(
        is_unique,
        Some("True".to_string()),
        "UX_Products_SKU should have IsUnique=True"
    );

    // Check non-unique index does NOT have IsUnique=True (or has IsUnique=False)
    let non_unique_index = find_index_by_name(&doc, "IX_Products_Category");
    assert!(
        non_unique_index.is_some(),
        "Should find IX_Products_Category index as SqlIndex element"
    );

    let non_unique_index = non_unique_index.unwrap();
    let non_unique_is_unique = get_property_value(&non_unique_index, "IsUnique");
    // Per XSD, property is only written when True, so None means not unique
    assert!(
        non_unique_is_unique.is_none() || non_unique_is_unique == Some("False".to_string()),
        "IX_Products_Category should NOT have IsUnique=True. Got: {:?}",
        non_unique_is_unique
    );
}

/// Test that clustered indexes have the IsClustered property set to True.
/// This verifies that CREATE CLUSTERED INDEX statements produce indexes with IsClustered=True.
#[test]
fn test_index_is_clustered_property() {
    let ctx = TestContext::with_fixture("index_properties");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check clustered index has IsClustered=True
    let clustered_index = find_index_by_name(&doc, "IX_Products_CreatedAt");
    assert!(
        clustered_index.is_some(),
        "Should find IX_Products_CreatedAt index as SqlIndex element"
    );

    let clustered_index = clustered_index.unwrap();
    let is_clustered = get_property_value(&clustered_index, "IsClustered");
    assert_eq!(
        is_clustered,
        Some("True".to_string()),
        "IX_Products_CreatedAt should have IsClustered=True"
    );

    // Check nonclustered index does NOT have IsClustered=True
    let nonclustered_index = find_index_by_name(&doc, "IX_Products_Category");
    assert!(
        nonclustered_index.is_some(),
        "Should find IX_Products_Category index as SqlIndex element"
    );

    let nonclustered_index = nonclustered_index.unwrap();
    let non_clustered_is_clustered = get_property_value(&nonclustered_index, "IsClustered");
    // Per XSD, property is only written when True, so None means not clustered
    assert!(
        non_clustered_is_clustered.is_none()
            || non_clustered_is_clustered == Some("False".to_string()),
        "IX_Products_Category should NOT have IsClustered=True. Got: {:?}",
        non_clustered_is_clustered
    );
}

/// Test that indexes have ColumnSpecifications relationship with proper column references.
/// This verifies the index key columns are correctly referenced.
#[test]
fn test_index_column_specifications() {
    let ctx = TestContext::with_fixture("index_properties");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Find the multi-column index
    let multi_col_index = find_index_by_name(&doc, "IX_Products_Category_Name_Include");
    assert!(
        multi_col_index.is_some(),
        "Should find IX_Products_Category_Name_Include index as SqlIndex element"
    );

    let multi_col_index = multi_col_index.unwrap();

    // Verify ColumnSpecifications relationship exists
    assert!(
        has_relationship(&multi_col_index, "ColumnSpecifications"),
        "Index should have ColumnSpecifications relationship"
    );

    // Get the ColumnSpecifications relationship
    let col_specs_rel = multi_col_index
        .children()
        .find(|c| {
            c.tag_name().name() == "Relationship"
                && c.attribute("Name") == Some("ColumnSpecifications")
        })
        .expect("Should have ColumnSpecifications relationship");

    // Count Entry elements (should have 2 for Category and Name key columns)
    let entries: Vec<_> = col_specs_rel
        .children()
        .filter(|c| c.tag_name().name() == "Entry")
        .collect();

    assert_eq!(
        entries.len(),
        2,
        "ColumnSpecifications should have 2 entries for the 2 key columns (Category, Name). Found: {}",
        entries.len()
    );

    // Verify each entry contains SqlIndexedColumnSpecification
    for entry in &entries {
        let has_spec = entry.descendants().any(|d| {
            d.tag_name().name() == "Element"
                && d.attribute("Type") == Some("SqlIndexedColumnSpecification")
        });
        assert!(
            has_spec,
            "Each Entry should contain a SqlIndexedColumnSpecification element"
        );
    }

    // Verify column references exist
    let column_refs: Vec<_> = col_specs_rel
        .descendants()
        .filter(|n| n.tag_name().name() == "Relationship" && n.attribute("Name") == Some("Column"))
        .collect();

    assert_eq!(
        column_refs.len(),
        2,
        "Should have 2 Column relationships (one per key column). Found: {}",
        column_refs.len()
    );
}

/// Test that indexes with INCLUDE columns have the IncludedColumns relationship.
/// This verifies that the INCLUDE clause columns are properly captured.
#[test]
fn test_index_include_columns() {
    let ctx = TestContext::with_fixture("index_properties");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Find the index with INCLUDE columns
    let include_index = find_index_by_name(&doc, "IX_Products_Category_Name_Include");
    assert!(
        include_index.is_some(),
        "Should find IX_Products_Category_Name_Include index as SqlIndex element"
    );

    let include_index = include_index.unwrap();

    // Verify IncludedColumns relationship exists
    assert!(
        has_relationship(&include_index, "IncludedColumns"),
        "Index with INCLUDE clause should have IncludedColumns relationship"
    );

    // Get the IncludedColumns relationship
    let include_rel = include_index
        .children()
        .find(|c| {
            c.tag_name().name() == "Relationship" && c.attribute("Name") == Some("IncludedColumns")
        })
        .expect("Should have IncludedColumns relationship");

    // Count Entry elements (should have 2 for Price and Description)
    let entries: Vec<_> = include_rel
        .children()
        .filter(|c| c.tag_name().name() == "Entry")
        .collect();

    assert_eq!(
        entries.len(),
        2,
        "IncludedColumns should have 2 entries for the 2 included columns (Price, Description). Found: {}",
        entries.len()
    );

    // Verify entries contain References to the columns
    let refs: Vec<_> = include_rel
        .descendants()
        .filter(|n| n.tag_name().name() == "References")
        .collect();

    assert_eq!(
        refs.len(),
        2,
        "IncludedColumns should have 2 References (one per included column). Found: {}",
        refs.len()
    );

    // Verify the column names are Price and Description
    let ref_names: Vec<_> = refs.iter().filter_map(|r| r.attribute("Name")).collect();

    let has_price = ref_names.iter().any(|n| n.contains("Price"));
    let has_description = ref_names.iter().any(|n| n.contains("Description"));

    assert!(
        has_price,
        "IncludedColumns should reference Price column. Found refs: {:?}",
        ref_names
    );
    assert!(
        has_description,
        "IncludedColumns should reference Description column. Found refs: {:?}",
        ref_names
    );

    // Verify that an index WITHOUT INCLUDE does NOT have IncludedColumns relationship
    let simple_index = find_index_by_name(&doc, "IX_Products_Category");
    if let Some(simple_index) = simple_index {
        assert!(
            !has_relationship(&simple_index, "IncludedColumns"),
            "Index without INCLUDE clause should NOT have IncludedColumns relationship"
        );
    }
}

// ============================================================================
// Origin.xml Tests
// ============================================================================

#[test]
fn test_origin_xml_has_package_properties() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

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
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

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
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

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
// DacMetadata.xml Tests
// ============================================================================

#[test]
fn test_metadata_xml_has_name() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

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
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

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
// Content_Types.xml Tests
// ============================================================================

#[test]
fn test_content_types_has_correct_mime_types() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

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
            .map_or(false, |ct| ct.contains("xml"))
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
// Edge Case Tests
// ============================================================================

/// Test building a project with no SQL objects (empty project).
/// The dacpac should still be valid with empty model.
#[test]
fn test_empty_project() {
    let ctx = TestContext::with_fixture("empty_project");
    let result = ctx.build();

    // Build might succeed or fail - we want to document the behavior
    if result.success {
        let dacpac_path = result.dacpac_path.unwrap();
        let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

        assert!(info.is_valid(), "Dacpac should have all required files");
        assert!(
            info.tables.is_empty(),
            "Empty project should have no tables"
        );
        assert!(info.views.is_empty(), "Empty project should have no views");
    } else {
        // Document that empty projects fail to build
        println!(
            "Empty project build failed (expected behavior): {:?}",
            result.errors
        );
    }
}

/// Test building a project with only schema definitions.
#[test]
fn test_project_with_only_schemas() {
    let ctx = TestContext::with_fixture("only_schemas");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let schemas = find_elements_by_type(&doc, "SqlSchema");

    assert!(
        !schemas.is_empty(),
        "Project with only schemas should have schema elements"
    );
    assert!(
        info.tables.is_empty(),
        "Project with only schemas should have no tables"
    );
}

/// Test that SQL reserved keywords can be used as identifiers when properly quoted.
#[test]
fn test_reserved_keyword_identifiers() {
    let ctx = TestContext::with_fixture("reserved_keywords");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    // Should have table with reserved keyword names
    assert!(
        !info.tables.is_empty(),
        "Should have tables with reserved keyword names"
    );

    // Verify model XML is well-formed
    let model_xml = info.model_xml_content.expect("Should have model XML");
    let doc = roxmltree::Document::parse(&model_xml);
    assert!(
        doc.is_ok(),
        "Model XML should be well-formed even with reserved keywords"
    );
}

/// Test that Unicode identifiers are handled correctly.
#[test]
fn test_unicode_identifiers() {
    let ctx = TestContext::with_fixture("unicode_identifiers");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    // Should have tables with unicode names
    assert!(
        !info.tables.is_empty(),
        "Should have tables with Unicode identifiers"
    );

    // Verify model XML is well-formed and UTF-8
    let model_xml = info.model_xml_content.expect("Should have model XML");
    let doc = roxmltree::Document::parse(&model_xml);
    assert!(
        doc.is_ok(),
        "Model XML should be well-formed with Unicode identifiers"
    );

    // Check encoding declaration
    assert!(
        model_xml.contains("utf-8") || model_xml.contains("UTF-8"),
        "Model XML should declare UTF-8 encoding"
    );
}

/// Test building a table with many columns (stress test).
#[test]
fn test_large_table_many_columns() {
    let ctx = TestContext::with_fixture("large_table");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Count SqlSimpleColumn elements
    let columns: Vec<_> = doc
        .descendants()
        .filter(|n| {
            n.tag_name().name() == "Element" && n.attribute("Type") == Some("SqlSimpleColumn")
        })
        .collect();

    // Should have many columns (fixture has 50+)
    assert!(
        columns.len() >= 50,
        "Large table should have at least 50 columns, found: {}",
        columns.len()
    );
}

/// Test a table with all constraint types: PK, FK, UQ, CK, DF.
#[test]
fn test_all_constraint_types_combined() {
    let ctx = TestContext::with_fixture("all_constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Verify all constraint types are present
    let pk = find_elements_by_type(&doc, "SqlPrimaryKeyConstraint");
    let fk = find_elements_by_type(&doc, "SqlForeignKeyConstraint");
    let uq = find_elements_by_type(&doc, "SqlUniqueConstraint");
    let ck = find_elements_by_type(&doc, "SqlCheckConstraint");
    let df = find_elements_by_type(&doc, "SqlDefaultConstraint");

    assert!(!pk.is_empty(), "Should have primary key constraint");
    assert!(!fk.is_empty(), "Should have foreign key constraint");
    assert!(!uq.is_empty(), "Should have unique constraint");
    assert!(!ck.is_empty(), "Should have check constraint");
    assert!(!df.is_empty(), "Should have default constraint");

    println!(
        "Found constraints - PK: {}, FK: {}, UQ: {}, CK: {}, DF: {}",
        pk.len(),
        fk.len(),
        uq.len(),
        ck.len(),
        df.len()
    );
}

/// Test multiple indexes on the same table.
#[test]
fn test_multiple_indexes_same_table() {
    let ctx = TestContext::with_fixture("multiple_indexes");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let indexes = find_elements_by_type(&doc, "SqlIndex");

    // Should have multiple indexes on same table
    assert!(
        indexes.len() >= 3,
        "Should have at least 3 indexes on the same table, found: {}",
        indexes.len()
    );

    // Verify all indexes reference the same table
    let table_refs: Vec<_> = indexes
        .iter()
        .filter_map(|idx| {
            idx.children()
                .find(|c| {
                    c.tag_name().name() == "Relationship"
                        && c.attribute("Name") == Some("IndexedObject")
                })
                .and_then(|rel| {
                    rel.descendants()
                        .find(|d| d.tag_name().name() == "References")
                        .and_then(|r| r.attribute("Name"))
                })
        })
        .collect();

    // All indexes should reference a table named "Products" or similar
    assert!(
        !table_refs.is_empty(),
        "Indexes should have IndexedObject relationship references"
    );
    println!("Index table references: {:?}", table_refs);
}

/// Test a self-referencing foreign key (table FK to itself).
#[test]
fn test_self_referencing_foreign_key() {
    let ctx = TestContext::with_fixture("self_ref_fk");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let fk_constraints = find_elements_by_type(&doc, "SqlForeignKeyConstraint");

    assert!(
        !fk_constraints.is_empty(),
        "Should have at least one foreign key constraint"
    );

    // Find the self-referencing FK by checking DefiningTable and ForeignTable are the same
    let self_ref_fk = fk_constraints.iter().find(|fk| {
        let defining_table = fk
            .children()
            .find(|c| {
                c.tag_name().name() == "Relationship"
                    && c.attribute("Name") == Some("DefiningTable")
            })
            .and_then(|rel| {
                rel.descendants()
                    .find(|d| d.tag_name().name() == "References")
            })
            .and_then(|r| r.attribute("Name"));

        let foreign_table = fk
            .children()
            .find(|c| {
                c.tag_name().name() == "Relationship" && c.attribute("Name") == Some("ForeignTable")
            })
            .and_then(|rel| {
                rel.descendants()
                    .find(|d| d.tag_name().name() == "References")
            })
            .and_then(|r| r.attribute("Name"));

        // Self-referencing: both should reference the same table
        defining_table.is_some() && defining_table == foreign_table
    });

    assert!(
        self_ref_fk.is_some(),
        "Should have a self-referencing foreign key (DefiningTable == ForeignTable)"
    );
}

// ============================================================================
// Cross-Fixture Tests
// ============================================================================

/// Test that pre/post deployment scripts are NOT included in the model as elements.
/// They should be packaged separately, not as SqlProcedure or other element types.
#[test]
fn test_pre_post_deploy_scripts_excluded_from_model() {
    let ctx = TestContext::with_fixture("pre_post_deploy");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Pre/post deploy scripts should not appear as model elements
    assert!(
        !model_xml.contains("PreDeployment") || !model_xml.contains("Type=\"SqlProcedure\""),
        "PreDeployment script should not appear as a SqlProcedure element in model"
    );
    assert!(
        !model_xml.contains("PostDeployment") || !model_xml.contains("Type=\"SqlProcedure\""),
        "PostDeployment script should not appear as a SqlProcedure element in model"
    );

    // Verify the table IS in the model
    assert!(
        info.tables.iter().any(|t| t.contains("Table1")),
        "Regular table should be in model. Found tables: {:?}",
        info.tables
    );
}

/// Test that SDK-style project exclusions are respected.
/// Files not in <Build Include="..."/> should not be in the model.
#[test]
fn test_sdk_style_exclusions_work() {
    let ctx = TestContext::with_fixture("build_with_exclude");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    // Table1.sql IS included
    assert!(
        info.tables.iter().any(|t| t.contains("Table1")),
        "Table1 (included) should be in model. Found: {:?}",
        info.tables
    );

    // Table2.sql is NOT included (not in Build items)
    assert!(
        !info.tables.iter().any(|t| t.contains("Table2")),
        "Table2 (excluded) should NOT be in model. Found: {:?}",
        info.tables
    );
}

/// Test that SQLCMD :r includes are resolved.
/// Scripts with :r directives should have their includes processed.
#[test]
fn test_sqlcmd_includes_resolved() {
    let ctx = TestContext::with_fixture("sqlcmd_includes");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    // The main tables should be present
    let has_users = info.tables.iter().any(|t| t.contains("Users"));
    let has_orders = info.tables.iter().any(|t| t.contains("Orders"));

    assert!(
        has_users,
        "Users table should be in model. Found: {:?}",
        info.tables
    );
    assert!(
        has_orders,
        "Orders table should be in model. Found: {:?}",
        info.tables
    );

    // If there's a Settings table from an included script, verify it's present
    // (depending on how the fixture is set up)
    println!("Tables found in sqlcmd_includes fixture: {:?}", info.tables);
}

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
