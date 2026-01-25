//! Dacpac generation

mod metadata_xml;
mod model_xml;
mod origin_xml;
mod packager;

pub use metadata_xml::generate_metadata_xml;
pub use model_xml::generate_model_xml;
pub use origin_xml::generate_origin_xml;
pub use packager::create_dacpac;

use crate::model::DatabaseModel;
use crate::project::SqlServerVersion;

/// Generate model.xml as a string (for testing)
pub fn generate_model_xml_string(
    model: &DatabaseModel,
    version: SqlServerVersion,
    collation_lcid: u32,
    _case_sensitive: bool,
) -> String {
    use crate::project::SqlProject;
    use std::path::PathBuf;

    // Create a minimal SqlProject for XML generation
    let project = SqlProject {
        name: "TestProject".to_string(),
        target_platform: version,
        default_schema: "dbo".to_string(),
        collation_lcid,
        sql_files: vec![],
        dacpac_references: vec![],
        package_references: vec![],
        project_dir: PathBuf::new(),
        pre_deploy_script: None,
        post_deploy_script: None,
        ansi_nulls: true,
        quoted_identifier: true,
        database_options: crate::project::DatabaseOptions::default(),
    };

    let mut buffer = Vec::new();
    generate_model_xml(&mut buffer, model, &project).unwrap();
    String::from_utf8(buffer).unwrap()
}

/// Generate DacMetadata.xml as a string (for testing)
pub fn generate_dac_metadata_xml(name: &str, version: &str) -> String {
    use crate::project::SqlProject;
    use std::path::PathBuf;

    let project = SqlProject {
        name: name.to_string(),
        target_platform: SqlServerVersion::Sql160,
        default_schema: "dbo".to_string(),
        collation_lcid: 1033,
        sql_files: vec![],
        dacpac_references: vec![],
        package_references: vec![],
        project_dir: PathBuf::new(),
        pre_deploy_script: None,
        post_deploy_script: None,
        ansi_nulls: true,
        quoted_identifier: true,
        database_options: crate::project::DatabaseOptions::default(),
    };

    let mut buffer = Vec::new();
    generate_metadata_xml(&mut buffer, &project, version).unwrap();
    String::from_utf8(buffer).unwrap()
}

/// Generate Origin.xml as a string (for testing)
pub fn generate_origin_xml_string(checksum: &str) -> String {
    let mut buffer = Vec::new();
    generate_origin_xml(&mut buffer, checksum).unwrap();
    String::from_utf8(buffer).unwrap()
}

/// Generate [Content_Types].xml as a string (for testing)
pub fn generate_content_types_xml() -> String {
    packager::generate_content_types_xml(false)
}
