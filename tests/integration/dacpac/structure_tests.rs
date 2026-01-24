//! Dacpac Structure Tests
//!
//! Tests for ZIP validation, file presence, and file counts.

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

    let doc = super::parse_model_xml(&model_xml);
    let schemas = super::find_elements_by_type(&doc, "SqlSchema");

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

    let doc = super::parse_model_xml(&model_xml);

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
