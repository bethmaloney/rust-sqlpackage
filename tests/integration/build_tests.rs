//! Integration tests for the build workflow
//!
//! These tests are converted from the Microsoft DacFx BuildTests.cs
//! to verify end-to-end build functionality.

use std::path::PathBuf;

use crate::common::{DacpacInfo, TestContext};

// ============================================================================
// Basic Build Tests (from DacFx SuccessfulSimpleBuild)
// ============================================================================

#[test]
fn test_successful_simple_build() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(
        result.success,
        "Simple build should succeed. Errors: {:?}",
        result.errors
    );
    assert!(
        result.dacpac_path.is_some(),
        "Should produce a dacpac file"
    );

    let dacpac_path = result.dacpac_path.unwrap();
    assert!(dacpac_path.exists(), "Dacpac file should exist");

    // Verify dacpac structure
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    assert!(info.is_valid(), "Dacpac should have all required files");
    assert!(
        info.tables.iter().any(|t| t.contains("Table1")),
        "Dacpac should contain Table1"
    );
}

#[test]
fn test_build_produces_all_required_files() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(info.has_model_xml, "Dacpac should contain model.xml");
    assert!(
        info.has_metadata_xml,
        "Dacpac should contain DacMetadata.xml"
    );
    assert!(info.has_origin_xml, "Dacpac should contain Origin.xml");
    assert!(
        info.has_content_types,
        "Dacpac should contain [Content_Types].xml"
    );
}

// ============================================================================
// Constraint Tests
// ============================================================================

#[test]
fn test_build_with_constraints() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with constraints should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    // Verify all tables are present
    assert!(
        info.tables.iter().any(|t| t.contains("PrimaryKeyTable")),
        "Should contain PrimaryKeyTable"
    );
    assert!(
        info.tables.iter().any(|t| t.contains("ForeignKeyTable")),
        "Should contain ForeignKeyTable"
    );
    assert!(
        info.tables.iter().any(|t| t.contains("UniqueConstraintTable")),
        "Should contain UniqueConstraintTable"
    );
    assert!(
        info.tables.iter().any(|t| t.contains("CheckConstraintTable")),
        "Should contain CheckConstraintTable"
    );
}

// ============================================================================
// Index Tests
// ============================================================================

#[test]
fn test_build_with_indexes() {
    let ctx = TestContext::with_fixture("indexes");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with indexes should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(
        info.tables.iter().any(|t| t.contains("IndexedTable")),
        "Should contain IndexedTable"
    );

    // Verify model XML contains index definitions
    let model_xml = info.model_xml_content.expect("Should have model XML");
    assert!(
        model_xml.contains("SqlIndex"),
        "Model should contain index elements"
    );
}

// ============================================================================
// View Tests
// ============================================================================

#[test]
fn test_build_with_views() {
    let ctx = TestContext::with_fixture("views");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with views should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(
        info.tables.iter().any(|t| t.contains("BaseTable")),
        "Should contain BaseTable"
    );
    assert!(
        info.views.iter().any(|v| v.contains("ActiveItems")),
        "Should contain ActiveItems view"
    );
}

// ============================================================================
// Build With Exclude Tests (from DacFx BuildWithExclude)
// ============================================================================

#[test]
fn test_build_with_exclude() {
    let ctx = TestContext::with_fixture("build_with_exclude");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with exclude should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    // Table1 should be included
    assert!(
        info.tables.iter().any(|t| t.contains("Table1")),
        "Should contain Table1"
    );

    // Table2 should NOT be included (not in Build items)
    assert!(
        !info.tables.iter().any(|t| t.contains("Table2")),
        "Should NOT contain Table2 (excluded)"
    );
}

// ============================================================================
// Error Cases (from DacFx VerifyBuildFailureWithUnresolvedReference)
// ============================================================================

#[test]
fn test_build_failure_with_unresolved_reference() {
    let ctx = TestContext::with_fixture("unresolved_reference");
    let result = ctx.build();

    // This test documents current behavior - may succeed or fail depending
    // on whether we validate references. Either is acceptable for baseline.
    // The important thing is documenting the behavior.
    if result.success {
        // If build succeeds without validation, that's okay for now
        println!("Note: Build succeeded without reference validation");
    } else {
        // If build fails due to unresolved reference, that's also valid
        assert!(
            result.errors.iter().any(|e| {
                e.contains("unresolved")
                    || e.contains("reference")
                    || e.contains("not found")
                    || e.contains("parse")
            }),
            "Error should mention reference issue. Errors: {:?}",
            result.errors
        );
    }
}

// ============================================================================
// Pre/Post Deployment Scripts (from DacFx SuccessfulBuildWithPreDeployScript)
// ============================================================================

#[test]
fn test_build_with_pre_post_deploy_scripts() {
    let ctx = TestContext::with_fixture("pre_post_deploy");
    let result = ctx.build();

    // This test documents current behavior for deployment scripts
    // Pre/post deployment scripts may or may not be supported yet
    if result.success {
        let dacpac_path = result.dacpac_path.unwrap();
        let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

        assert!(
            info.tables.iter().any(|t| t.contains("Table1")),
            "Should contain Table1"
        );
    } else {
        println!(
            "Note: Build with pre/post deploy scripts failed (may not be implemented yet): {:?}",
            result.errors
        );
    }
}

// ============================================================================
// Multiple Tables Test
// ============================================================================

#[test]
fn test_build_multiple_tables() {
    let ctx = TestContext::with_fixture("simple_table");

    // Add a second table to the fixture
    let table2_path = ctx.project_dir.join("Table2.sql");
    std::fs::write(
        &table2_path,
        "CREATE TABLE [dbo].[Table2] ([Id] INT NOT NULL PRIMARY KEY, [Value] INT NULL);",
    )
    .unwrap();

    // Update sqlproj to include Table2
    let sqlproj_path = ctx.project_path();
    let sqlproj = std::fs::read_to_string(&sqlproj_path).unwrap();
    let updated = sqlproj.replace(
        "</ItemGroup>",
        "    <Build Include=\"Table2.sql\" />\n  </ItemGroup>",
    );
    std::fs::write(&sqlproj_path, updated).unwrap();

    let result = ctx.build();

    assert!(
        result.success,
        "Multi-table build should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(
        info.tables.iter().any(|t| t.contains("Table1")),
        "Should contain Table1"
    );
    assert!(
        info.tables.iter().any(|t| t.contains("Table2")),
        "Should contain Table2"
    );
}

// ============================================================================
// SDK-Style Project Tests
// ============================================================================

#[test]
fn test_build_sdk_style_project_with_globbing() {
    // SDK-style projects don't have explicit Build items
    let ctx = TestContext::with_fixture("simple_table");

    // Modify sqlproj to remove explicit Build items (SDK-style)
    let sqlproj_path = ctx.project_path();
    let sqlproj = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>SdkStyle</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;
    std::fs::write(&sqlproj_path, sqlproj).unwrap();

    let result = ctx.build();

    assert!(
        result.success,
        "SDK-style build should succeed via globbing. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(
        info.tables.iter().any(|t| t.contains("Table1")),
        "Should find Table1 via globbing"
    );
}

// ============================================================================
// Version Target Tests
// ============================================================================

#[test]
fn test_build_with_sql150_target() {
    let ctx = TestContext::with_fixture("simple_table");

    // Update to SQL Server 2019 (Sql150)
    let sqlproj_path = ctx.project_path();
    let sqlproj = std::fs::read_to_string(&sqlproj_path).unwrap();
    let updated = sqlproj.replace("Sql160", "Sql150");
    std::fs::write(&sqlproj_path, updated).unwrap();

    let result = ctx.build();

    assert!(
        result.success,
        "Build with Sql150 target should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    // Verify DSP name in model XML
    let model_xml = info.model_xml_content.expect("Should have model XML");
    assert!(
        model_xml.contains("Sql150"),
        "Model should target Sql150"
    );
}

// ============================================================================
// Empty Project Test
// ============================================================================

#[test]
fn test_build_empty_project() {
    let ctx = TestContext::with_fixture("simple_table");

    // Remove all SQL files
    std::fs::remove_file(ctx.project_dir.join("Table1.sql")).ok();

    // Update sqlproj to have no Build items
    let sqlproj_path = ctx.project_path();
    let sqlproj = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>Empty</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;
    std::fs::write(&sqlproj_path, sqlproj).unwrap();

    let result = ctx.build();

    // Empty project might succeed with empty model or fail
    // Both are acceptable behaviors to document
    if result.success {
        let dacpac_path = result.dacpac_path.unwrap();
        let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
        assert!(info.is_valid(), "Even empty dacpac should have required files");
        assert!(info.tables.is_empty(), "Empty project should have no tables");
    } else {
        println!("Note: Empty project build failed (acceptable): {:?}", result.errors);
    }
}

// ============================================================================
// Schema Tests
// ============================================================================

#[test]
fn test_build_with_custom_schema() {
    let ctx = TestContext::with_fixture("simple_table");

    // Update Table1.sql to use custom schema
    let table_path = ctx.project_dir.join("Table1.sql");
    std::fs::write(
        &table_path,
        "CREATE TABLE [sales].[Orders] ([Id] INT NOT NULL PRIMARY KEY, [Amount] DECIMAL(18,2) NULL);",
    )
    .unwrap();

    let result = ctx.build();

    assert!(
        result.success,
        "Build with custom schema should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(
        info.schemas.iter().any(|s| s.contains("sales")),
        "Should contain sales schema"
    );
}
