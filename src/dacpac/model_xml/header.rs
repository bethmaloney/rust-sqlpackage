//! Header and metadata XML writing utilities for model.xml generation.
//!
//! This module provides functions for writing the Header section and
//! SqlDatabaseOptions element in the model.xml output. The Header contains
//! CustomData entries for AnsiNulls, QuotedIdentifier, CompatibilityMode,
//! package references, and SQLCMD variables.

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

use crate::project::SqlProject;

use super::xml_helpers::write_property;

/// Write the Header section with CustomData entries for AnsiNulls, QuotedIdentifier,
/// CompatibilityMode, References, and SqlCmdVariables.
pub(crate) fn write_header<W: Write>(
    writer: &mut Writer<W>,
    project: &SqlProject,
) -> anyhow::Result<()> {
    writer.write_event(Event::Start(BytesStart::new("Header")))?;

    // AnsiNulls
    write_custom_data(
        writer,
        "AnsiNulls",
        "AnsiNulls",
        if project.ansi_nulls { "True" } else { "False" },
    )?;

    // QuotedIdentifier
    write_custom_data(
        writer,
        "QuotedIdentifier",
        "QuotedIdentifier",
        if project.quoted_identifier {
            "True"
        } else {
            "False"
        },
    )?;

    // CompatibilityMode
    let compat_mode = project.target_platform.compatibility_mode().to_string();
    write_custom_data(
        writer,
        "CompatibilityMode",
        "CompatibilityMode",
        &compat_mode,
    )?;

    // Package references (e.g., Microsoft.SqlServer.Dacpacs.Master)
    for pkg_ref in &project.package_references {
        write_package_reference(writer, pkg_ref)?;
    }

    // SQLCMD variables (all in one CustomData element)
    if !project.sqlcmd_variables.is_empty() {
        write_sqlcmd_variables(writer, &project.sqlcmd_variables)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Header")))?;
    Ok(())
}

/// Write a CustomData element for a package reference
/// Format:
/// ```xml
/// <CustomData Category="Reference" Type="SqlSchema">
///   <Metadata Name="FileName" Value="master.dacpac" />
///   <Metadata Name="LogicalName" Value="master.dacpac" />
///   <Metadata Name="SuppressMissingDependenciesErrors" Value="False" />
/// </CustomData>
/// ```
fn write_package_reference<W: Write>(
    writer: &mut Writer<W>,
    pkg_ref: &crate::project::PackageReference,
) -> anyhow::Result<()> {
    // Extract dacpac name from package name
    // e.g., "Microsoft.SqlServer.Dacpacs.Master" -> "master.dacpac"
    let dacpac_name = extract_dacpac_name(&pkg_ref.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let custom_data = BytesStart::new("CustomData")
        .with_attributes([("Category", "Reference"), ("Type", "SqlSchema")]);
    writer.write_event(Event::Start(custom_data))?;

    // FileName metadata - batch attributes
    let filename = BytesStart::new("Metadata")
        .with_attributes([("Name", "FileName"), ("Value", dacpac_name.as_str())]);
    writer.write_event(Event::Empty(filename))?;

    // LogicalName metadata - batch attributes
    let logical_name = BytesStart::new("Metadata")
        .with_attributes([("Name", "LogicalName"), ("Value", dacpac_name.as_str())]);
    writer.write_event(Event::Empty(logical_name))?;

    // SuppressMissingDependenciesErrors metadata - batch attributes
    let suppress = BytesStart::new("Metadata").with_attributes([
        ("Name", "SuppressMissingDependenciesErrors"),
        ("Value", "False"),
    ]);
    writer.write_event(Event::Empty(suppress))?;

    writer.write_event(Event::End(BytesEnd::new("CustomData")))?;
    Ok(())
}

/// Write a CustomData element for all SQLCMD variables
/// Format (matches .NET DacFx):
/// ```xml
/// <CustomData Category="SqlCmdVariables" Type="SqlCmdVariable">
///   <Metadata Name="Environment" Value="" />
///   <Metadata Name="ServerName" Value="" />
///   <Metadata Name="MaxConnections" Value="" />
/// </CustomData>
/// ```
fn write_sqlcmd_variables<W: Write>(
    writer: &mut Writer<W>,
    sqlcmd_vars: &[crate::project::SqlCmdVariable],
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let custom_data = BytesStart::new("CustomData")
        .with_attributes([("Category", "SqlCmdVariables"), ("Type", "SqlCmdVariable")]);
    writer.write_event(Event::Start(custom_data))?;

    // Write each variable as a Metadata element with the variable name as Name attribute
    for sqlcmd_var in sqlcmd_vars {
        let metadata = BytesStart::new("Metadata")
            .with_attributes([("Name", sqlcmd_var.name.as_str()), ("Value", "")]);
        writer.write_event(Event::Empty(metadata))?;
    }

    writer.write_event(Event::End(BytesEnd::new("CustomData")))?;
    Ok(())
}

/// Extract dacpac name from a package reference name
/// e.g., "Microsoft.SqlServer.Dacpacs.Master" -> "master.dacpac"
/// e.g., "Microsoft.SqlServer.Dacpacs.Msdb" -> "msdb.dacpac"
fn extract_dacpac_name(package_name: &str) -> String {
    // Common pattern: Microsoft.SqlServer.Dacpacs.<DatabaseName>
    if let Some(last_part) = package_name.split('.').next_back() {
        format!("{}.dacpac", last_part.to_lowercase())
    } else {
        format!("{}.dacpac", package_name.to_lowercase())
    }
}

/// Write a CustomData element with a single Metadata child
/// Format: <CustomData Category="category"><Metadata Name="name" Value="value"/></CustomData>
fn write_custom_data<W: Write>(
    writer: &mut Writer<W>,
    category: &str,
    name: &str,
    value: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let custom_data = BytesStart::new("CustomData").with_attributes([("Category", category)]);
    writer.write_event(Event::Start(custom_data))?;

    let metadata = BytesStart::new("Metadata").with_attributes([("Name", name), ("Value", value)]);
    writer.write_event(Event::Empty(metadata))?;

    writer.write_event(Event::End(BytesEnd::new("CustomData")))?;
    Ok(())
}

/// Write the SqlDatabaseOptions element
/// Format:
/// ```xml
/// <Element Type="SqlDatabaseOptions">
///   <Property Name="Collation" Value="Latin1_General_CI_AS"/>
///   <Property Name="IsAnsiNullDefaultOn" Value="True"/>
///   <Property Name="IsAnsiNullsOn" Value="True"/>
///   <Property Name="IsAnsiWarningsOn" Value="True"/>
///   <Property Name="IsArithAbortOn" Value="True"/>
///   <Property Name="IsConcatNullYieldsNullOn" Value="True"/>
///   <Property Name="IsFullTextEnabled" Value="False"/>
///   <Property Name="PageVerifyMode" Value="3"/>
/// </Element>
/// ```
pub(crate) fn write_database_options<W: Write>(
    writer: &mut Writer<W>,
    project: &SqlProject,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlDatabaseOptions")]);
    writer.write_event(Event::Start(elem))?;

    let db_options = &project.database_options;

    // Collation (always emit - use default if not specified)
    if let Some(ref collation) = db_options.collation {
        write_property(writer, "Collation", collation)?;
    }

    // IsAnsiNullDefaultOn
    write_property(
        writer,
        "IsAnsiNullDefaultOn",
        if db_options.ansi_null_default_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsAnsiNullsOn
    write_property(
        writer,
        "IsAnsiNullsOn",
        if db_options.ansi_nulls_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsAnsiWarningsOn
    write_property(
        writer,
        "IsAnsiWarningsOn",
        if db_options.ansi_warnings_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsArithAbortOn
    write_property(
        writer,
        "IsArithAbortOn",
        if db_options.arith_abort_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsConcatNullYieldsNullOn
    write_property(
        writer,
        "IsConcatNullYieldsNullOn",
        if db_options.concat_null_yields_null_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsTornPageProtectionOn
    write_property(
        writer,
        "IsTornPageProtectionOn",
        if db_options.torn_page_protection_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsFullTextEnabled
    write_property(
        writer,
        "IsFullTextEnabled",
        if db_options.full_text_enabled {
            "True"
        } else {
            "False"
        },
    )?;

    // PageVerifyMode (convert string to numeric value for DacFx compatibility)
    // NONE = 0, TORN_PAGE_DETECTION = 1, CHECKSUM = 3
    if let Some(ref page_verify) = db_options.page_verify {
        let mode_value = match page_verify.to_uppercase().as_str() {
            "NONE" => "0",
            "TORN_PAGE_DETECTION" => "1",
            "CHECKSUM" => "3",
            _ => "3", // Default to CHECKSUM
        };
        write_property(writer, "PageVerifyMode", mode_value)?;
    }

    // DefaultLanguage (always emit, even if empty)
    write_property(writer, "DefaultLanguage", &db_options.default_language)?;

    // DefaultFullTextLanguage (always emit, even if empty)
    write_property(
        writer,
        "DefaultFullTextLanguage",
        &db_options.default_full_text_language,
    )?;

    // QueryStoreStaleQueryThreshold
    write_property(
        writer,
        "QueryStoreStaleQueryThreshold",
        &db_options.query_store_stale_query_threshold.to_string(),
    )?;

    // DefaultFilegroup - write as a Relationship with ExternalSource="BuiltIns"
    if let Some(ref filegroup) = db_options.default_filegroup {
        writer.write_event(Event::Start(
            BytesStart::new("Relationship").with_attributes([("Name", "DefaultFilegroup")]),
        ))?;
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let filegroup_name = format!("[{}]", filegroup);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let refs = BytesStart::new("References").with_attributes([
            ("ExternalSource", "BuiltIns"),
            ("Name", filegroup_name.as_str()),
        ]);
        writer.write_event(Event::Empty(refs))?;

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{
        DatabaseOptions, PackageReference, SqlCmdVariable, SqlProject, SqlServerVersion,
    };
    use std::io::Cursor;
    use std::path::PathBuf;

    fn create_test_writer() -> Writer<Cursor<Vec<u8>>> {
        Writer::new(Cursor::new(Vec::new()))
    }

    fn get_output(writer: Writer<Cursor<Vec<u8>>>) -> String {
        let inner = writer.into_inner();
        String::from_utf8(inner.into_inner()).unwrap()
    }

    /// Create a test SqlProject with default values
    fn create_test_project() -> SqlProject {
        SqlProject {
            name: "TestDatabase".to_string(),
            target_platform: SqlServerVersion::Sql160,
            default_schema: "dbo".to_string(),
            collation_lcid: 1033,
            sql_files: Vec::new(),
            dacpac_references: Vec::new(),
            package_references: Vec::new(),
            sqlcmd_variables: Vec::new(),
            project_dir: PathBuf::new(),
            pre_deploy_script: None,
            post_deploy_script: None,
            ansi_nulls: true,
            quoted_identifier: true,
            database_options: DatabaseOptions::default(),
        }
    }

    #[test]
    fn test_extract_dacpac_name() {
        assert_eq!(
            extract_dacpac_name("Microsoft.SqlServer.Dacpacs.Master"),
            "master.dacpac"
        );
        assert_eq!(
            extract_dacpac_name("Microsoft.SqlServer.Dacpacs.Msdb"),
            "msdb.dacpac"
        );
        assert_eq!(extract_dacpac_name("CustomPackage"), "custompackage.dacpac");
    }

    #[test]
    fn test_write_custom_data() {
        let mut writer = create_test_writer();
        write_custom_data(&mut writer, "AnsiNulls", "AnsiNulls", "True").unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<CustomData Category="AnsiNulls">"#));
        assert!(output.contains(r#"<Metadata Name="AnsiNulls" Value="True"/>"#));
        assert!(output.contains("</CustomData>"));
    }

    #[test]
    fn test_write_package_reference() {
        let mut writer = create_test_writer();
        let pkg_ref = PackageReference {
            name: "Microsoft.SqlServer.Dacpacs.Master".to_string(),
            version: "160.0.0".to_string(),
        };
        write_package_reference(&mut writer, &pkg_ref).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<CustomData Category="Reference" Type="SqlSchema">"#));
        assert!(output.contains(r#"<Metadata Name="FileName" Value="master.dacpac"/>"#));
        assert!(output.contains(r#"<Metadata Name="LogicalName" Value="master.dacpac"/>"#));
        assert!(output
            .contains(r#"<Metadata Name="SuppressMissingDependenciesErrors" Value="False"/>"#));
    }

    #[test]
    fn test_write_sqlcmd_variables() {
        let mut writer = create_test_writer();
        let vars = vec![
            SqlCmdVariable {
                name: "Environment".to_string(),
                value: String::new(),
                default_value: String::new(),
            },
            SqlCmdVariable {
                name: "ServerName".to_string(),
                value: String::new(),
                default_value: String::new(),
            },
        ];
        write_sqlcmd_variables(&mut writer, &vars).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<CustomData Category="SqlCmdVariables" Type="SqlCmdVariable">"#));
        assert!(output.contains(r#"<Metadata Name="Environment" Value=""/>"#));
        assert!(output.contains(r#"<Metadata Name="ServerName" Value=""/>"#));
    }

    #[test]
    fn test_write_header() {
        let mut writer = create_test_writer();
        let project = create_test_project();
        write_header(&mut writer, &project).unwrap();
        let output = get_output(writer);
        assert!(output.contains("<Header>"));
        assert!(output.contains(r#"<CustomData Category="AnsiNulls">"#));
        assert!(output.contains(r#"<Metadata Name="AnsiNulls" Value="True"/>"#));
        assert!(output.contains(r#"<CustomData Category="QuotedIdentifier">"#));
        assert!(output.contains(r#"<Metadata Name="QuotedIdentifier" Value="True"/>"#));
        assert!(output.contains(r#"<CustomData Category="CompatibilityMode">"#));
        assert!(output.contains("</Header>"));
    }

    #[test]
    fn test_write_header_with_package_references() {
        let mut writer = create_test_writer();
        let mut project = create_test_project();
        project.package_references = vec![PackageReference {
            name: "Microsoft.SqlServer.Dacpacs.Master".to_string(),
            version: "160.0.0".to_string(),
        }];
        write_header(&mut writer, &project).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<CustomData Category="Reference" Type="SqlSchema">"#));
        assert!(output.contains(r#"<Metadata Name="FileName" Value="master.dacpac"/>"#));
    }

    #[test]
    fn test_write_database_options() {
        let mut writer = create_test_writer();
        let mut project = create_test_project();
        project.database_options = DatabaseOptions {
            collation: Some("Latin1_General_CI_AS".to_string()),
            ansi_null_default_on: true,
            ansi_nulls_on: true,
            ansi_warnings_on: true,
            arith_abort_on: true,
            concat_null_yields_null_on: true,
            torn_page_protection_on: false,
            full_text_enabled: false,
            page_verify: Some("CHECKSUM".to_string()),
            default_language: String::new(),
            default_full_text_language: String::new(),
            query_store_stale_query_threshold: 367,
            default_filegroup: None,
        };
        write_database_options(&mut writer, &project).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Element Type="SqlDatabaseOptions">"#));
        assert!(output.contains(r#"<Property Name="Collation" Value="Latin1_General_CI_AS"/>"#));
        assert!(output.contains(r#"<Property Name="IsAnsiNullDefaultOn" Value="True"/>"#));
        assert!(output.contains(r#"<Property Name="IsAnsiNullsOn" Value="True"/>"#));
        assert!(output.contains(r#"<Property Name="PageVerifyMode" Value="3"/>"#));
        assert!(output.contains("</Element>"));
    }

    #[test]
    fn test_write_database_options_with_filegroup() {
        let mut writer = create_test_writer();
        let mut project = create_test_project();
        project.database_options.default_filegroup = Some("PRIMARY".to_string());
        write_database_options(&mut writer, &project).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Relationship Name="DefaultFilegroup">"#));
        assert!(output.contains(r#"ExternalSource="BuiltIns""#));
        assert!(output.contains(r#"Name="[PRIMARY]""#));
    }

    #[test]
    fn test_page_verify_mode_values() {
        // Test NONE
        let mut writer = create_test_writer();
        let mut project = create_test_project();
        project.database_options.page_verify = Some("NONE".to_string());
        write_database_options(&mut writer, &project).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Property Name="PageVerifyMode" Value="0"/>"#));

        // Test TORN_PAGE_DETECTION
        let mut writer = create_test_writer();
        let mut project = create_test_project();
        project.database_options.page_verify = Some("TORN_PAGE_DETECTION".to_string());
        write_database_options(&mut writer, &project).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Property Name="PageVerifyMode" Value="1"/>"#));
    }
}
