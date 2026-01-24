//! Model XML tests
//!
//! Tests for namespace, format, schema version, collation, and XML well-formedness.

use crate::common::{DacpacInfo, TestContext};

use super::{find_elements_by_type, has_relationship, parse_model_xml};

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

// ============================================================================
// DataSchemaModel Root Element Tests (XSD Compliance)
// ============================================================================

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
