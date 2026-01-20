//! Unit tests for .sqlproj parser
//!
//! These tests verify the parsing of SQL Server project files.

use std::io::Write;
use std::path::PathBuf;

use tempfile::{NamedTempFile, TempDir};

/// Helper to create a temp sqlproj file with content
fn create_sqlproj_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::with_suffix(".sqlproj").unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

/// Helper to create a test project directory with sqlproj and SQL files
fn create_test_project(sqlproj_content: &str, sql_files: &[(&str, &str)]) -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    // Write sqlproj file
    let sqlproj_path = temp_dir.path().join("project.sqlproj");
    std::fs::write(&sqlproj_path, sqlproj_content).unwrap();

    // Write SQL files
    for (name, content) in sql_files {
        let sql_path = temp_dir.path().join(name);
        if let Some(parent) = sql_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&sql_path, content).unwrap();
    }

    temp_dir
}

// ============================================================================
// Version Parsing Tests
// ============================================================================

#[test]
fn test_parse_sql160_version() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let project = result.unwrap();
    assert_eq!(
        project.target_platform,
        rust_sqlpackage::project::SqlServerVersion::Sql160
    );
}

#[test]
fn test_parse_sql150_version() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql150DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    assert_eq!(
        project.target_platform,
        rust_sqlpackage::project::SqlServerVersion::Sql150
    );
}

#[test]
fn test_parse_sql140_version() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql140DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    assert_eq!(
        project.target_platform,
        rust_sqlpackage::project::SqlServerVersion::Sql140
    );
}

#[test]
fn test_parse_sql130_version() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql130DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    assert_eq!(
        project.target_platform,
        rust_sqlpackage::project::SqlServerVersion::Sql130
    );
}

#[test]
fn test_parse_default_version_when_missing() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    // Should default to Sql160
    assert_eq!(
        project.target_platform,
        rust_sqlpackage::project::SqlServerVersion::Sql160
    );
}

// ============================================================================
// Property Parsing Tests
// ============================================================================

#[test]
fn test_parse_project_name() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>MyDatabaseProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    // Name the file explicitly to test name extraction from filename
    let sqlproj_path = temp_dir.path().join("CustomName.sqlproj");
    std::fs::write(&sqlproj_path, content).unwrap();

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    // Project name should come from filename stem
    assert_eq!(project.name, "CustomName");
}

#[test]
fn test_parse_default_schema() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DefaultSchema>custom</DefaultSchema>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    assert_eq!(project.default_schema, "custom");
}

#[test]
fn test_parse_default_schema_when_missing() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    assert_eq!(project.default_schema, "dbo");
}

#[test]
fn test_parse_collation() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DefaultCollation>SQL_Latin1_General_CP1_CI_AS</DefaultCollation>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    // Default LCID for SQL_Latin1_General is 1033
    assert_eq!(project.collation_lcid, 1033);
}

// ============================================================================
// SQL File Discovery Tests
// ============================================================================

#[test]
fn test_find_explicit_build_items() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
  <ItemGroup>
    <Build Include="Table1.sql" />
    <Build Include="Tables/Table2.sql" />
  </ItemGroup>
</Project>"#;

    let temp_dir = create_test_project(
        content,
        &[
            ("Table1.sql", "CREATE TABLE t1 (id INT)"),
            ("Tables/Table2.sql", "CREATE TABLE t2 (id INT)"),
        ],
    );
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let project = result.unwrap();
    assert_eq!(project.sql_files.len(), 2, "Expected 2 SQL files");
}

#[test]
fn test_find_sql_files_sdk_style_globbing() {
    // SDK-style projects don't have explicit Build items - they glob automatically
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(
        content,
        &[
            ("Table1.sql", "CREATE TABLE t1 (id INT)"),
            ("Tables/Table2.sql", "CREATE TABLE t2 (id INT)"),
            ("Views/View1.sql", "CREATE VIEW v1 AS SELECT 1"),
        ],
    );
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let project = result.unwrap();
    // Should find all SQL files via globbing
    assert!(
        project.sql_files.len() >= 3,
        "Expected at least 3 SQL files from globbing, got {}",
        project.sql_files.len()
    );
}

#[test]
fn test_exclude_bin_obj_directories() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
</Project>"#;

    let temp_dir = create_test_project(
        content,
        &[
            ("Table1.sql", "CREATE TABLE t1 (id INT)"),
            ("bin/Debug/Generated.sql", "-- Should be excluded"),
            ("obj/Debug/Temp.sql", "-- Should be excluded"),
        ],
    );
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    // Should only find Table1.sql, not the ones in bin/obj
    assert_eq!(
        project.sql_files.len(),
        1,
        "Should exclude bin/obj directories, found: {:?}",
        project.sql_files
    );
}

#[test]
fn test_none_include_excludes_file() {
    // When a file is marked as None instead of Build, it should be excluded
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
  <ItemGroup>
    <Build Include="Table1.sql" />
    <None Include="Table2.sql" />
  </ItemGroup>
</Project>"#;

    let temp_dir = create_test_project(
        content,
        &[
            ("Table1.sql", "CREATE TABLE t1 (id INT)"),
            ("Table2.sql", "CREATE TABLE t2 (id INT)"),
        ],
    );
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    // Should only include Table1.sql
    assert_eq!(project.sql_files.len(), 1);
    assert!(project.sql_files[0].to_string_lossy().contains("Table1"));
}

// ============================================================================
// Dacpac Reference Tests
// ============================================================================

#[test]
fn test_parse_dacpac_reference() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
  <ItemGroup>
    <ArtifactReference Include="..\..\References\master.dacpac" />
  </ItemGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    assert_eq!(project.dacpac_references.len(), 1);
}

#[test]
fn test_parse_dacpac_reference_with_database_variable() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
  <ItemGroup>
    <ArtifactReference Include="OtherDb.dacpac">
      <DatabaseVariableLiteralValue>OtherDatabase</DatabaseVariableLiteralValue>
    </ArtifactReference>
  </ItemGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    assert_eq!(project.dacpac_references.len(), 1);
    assert_eq!(
        project.dacpac_references[0].database_variable,
        Some("OtherDatabase".to_string())
    );
}

#[test]
fn test_parse_dacpac_reference_with_server_variable() {
    let content = r#"<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <Name>TestProject</Name>
    <DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP>
  </PropertyGroup>
  <ItemGroup>
    <ArtifactReference Include="RemoteDb.dacpac">
      <DatabaseVariableLiteralValue>RemoteDatabase</DatabaseVariableLiteralValue>
      <ServerVariableLiteralValue>RemoteServer</ServerVariableLiteralValue>
    </ArtifactReference>
  </ItemGroup>
</Project>"#;

    let temp_dir = create_test_project(content, &[]);
    let sqlproj_path = temp_dir.path().join("project.sqlproj");

    let result = rust_sqlpackage::project::parse_sqlproj(&sqlproj_path);
    assert!(result.is_ok());

    let project = result.unwrap();
    assert_eq!(project.dacpac_references.len(), 1);
    assert_eq!(
        project.dacpac_references[0].server_variable,
        Some("RemoteServer".to_string())
    );
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_parse_invalid_xml_returns_error() {
    let content = "This is not valid XML!!!";

    let file = create_sqlproj_file(content);
    let result = rust_sqlpackage::project::parse_sqlproj(file.path());
    assert!(result.is_err(), "Invalid XML should return error");
}

#[test]
fn test_parse_missing_file_returns_error() {
    let result = rust_sqlpackage::project::parse_sqlproj(&PathBuf::from("/nonexistent/path.sqlproj"));
    assert!(result.is_err(), "Missing file should return error");
}

#[test]
fn test_parse_empty_file() {
    let content = "";
    let file = create_sqlproj_file(content);

    let result = rust_sqlpackage::project::parse_sqlproj(file.path());
    // Empty file should fail to parse as XML
    assert!(result.is_err(), "Empty file should return error");
}

// ============================================================================
// DSP Name Tests
// ============================================================================

#[test]
fn test_dsp_name_sql160() {
    use rust_sqlpackage::project::SqlServerVersion;
    let dsp = SqlServerVersion::Sql160.dsp_name();
    assert!(dsp.contains("Sql160"));
    assert!(dsp.contains("DatabaseSchemaProvider"));
}

#[test]
fn test_dsp_name_sql150() {
    use rust_sqlpackage::project::SqlServerVersion;
    let dsp = SqlServerVersion::Sql150.dsp_name();
    assert!(dsp.contains("Sql150"));
}

#[test]
fn test_dsp_name_sql140() {
    use rust_sqlpackage::project::SqlServerVersion;
    let dsp = SqlServerVersion::Sql140.dsp_name();
    assert!(dsp.contains("Sql140"));
}

#[test]
fn test_dsp_name_sql130() {
    use rust_sqlpackage::project::SqlServerVersion;
    let dsp = SqlServerVersion::Sql130.dsp_name();
    assert!(dsp.contains("Sql130"));
}
