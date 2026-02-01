//! Unit tests for database model builder
//!
//! These tests verify the transformation from SQL AST to internal database model.

use std::io::Write;
use std::path::PathBuf;

use tempfile::NamedTempFile;

mod column_type_tests;
mod constraint_tests;
mod element_tests;
mod execute_as_tests;
mod graph_table_tests;
mod index_tests;
mod routine_tests;
mod schema_tests;
mod table_tests;
mod view_tests;

/// Helper to create a temp SQL file with content
pub fn create_sql_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::with_suffix(".sql").unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

/// Helper to create a test SqlProject
pub fn create_test_project() -> rust_sqlpackage::project::SqlProject {
    rust_sqlpackage::project::SqlProject {
        name: "TestProject".to_string(),
        target_platform: rust_sqlpackage::project::SqlServerVersion::Sql160,
        default_schema: "dbo".to_string(),
        collation_lcid: 1033,
        sql_files: vec![],
        dacpac_references: vec![],
        package_references: vec![],
        sqlcmd_variables: vec![],
        project_dir: PathBuf::new(),
        pre_deploy_script: None,
        post_deploy_script: None,
        ansi_nulls: true,
        quoted_identifier: true,
        database_options: rust_sqlpackage::project::DatabaseOptions::default(),
        dac_version: "1.0.0.0".to_string(),
        dac_description: None,
    }
}

/// Helper to parse SQL and build model
pub fn parse_and_build_model(sql: &str) -> rust_sqlpackage::model::DatabaseModel {
    let file = create_sql_file(sql);
    let statements = rust_sqlpackage::parser::parse_sql_file(file.path()).unwrap();
    let project = create_test_project();
    rust_sqlpackage::model::build_model(&statements, &project).unwrap()
}
