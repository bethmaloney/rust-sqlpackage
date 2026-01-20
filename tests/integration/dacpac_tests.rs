//! Integration tests for dacpac file validation
//!
//! These tests verify the structure and content of generated dacpac files.

use std::fs::File;
use std::io::Read;

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

    assert!(
        archive.is_ok(),
        "Dacpac should be a valid ZIP archive"
    );
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
    let mut archive = ZipArchive::new(File::open(&dacpac_path).expect("Should open file")).expect("Should be valid ZIP");
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
    println!("Found constraints - PK: {}, FK: {}, UQ: {}, CK: {}", has_pk, has_fk, has_uq, has_ck);
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
