//! Parser for .sqlproj files

use std::path::{Path, PathBuf};

use anyhow::Result;
use roxmltree::Document;

use crate::error::SqlPackageError;

/// SQL Server version target
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SqlServerVersion {
    Sql130, // SQL Server 2016
    Sql140, // SQL Server 2017
    Sql150, // SQL Server 2019
    #[default]
    Sql160, // SQL Server 2022
}

impl std::str::FromStr for SqlServerVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sql130" | "130" => Ok(SqlServerVersion::Sql130),
            "sql140" | "140" => Ok(SqlServerVersion::Sql140),
            "sql150" | "150" => Ok(SqlServerVersion::Sql150),
            "sql160" | "160" => Ok(SqlServerVersion::Sql160),
            _ => Err(format!("Unknown SQL Server version: {}", s)),
        }
    }
}

impl SqlServerVersion {
    pub fn dsp_name(&self) -> &'static str {
        match self {
            SqlServerVersion::Sql130 => {
                "Microsoft.Data.Tools.Schema.Sql.Sql130DatabaseSchemaProvider"
            }
            SqlServerVersion::Sql140 => {
                "Microsoft.Data.Tools.Schema.Sql.Sql140DatabaseSchemaProvider"
            }
            SqlServerVersion::Sql150 => {
                "Microsoft.Data.Tools.Schema.Sql.Sql150DatabaseSchemaProvider"
            }
            SqlServerVersion::Sql160 => {
                "Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider"
            }
        }
    }

    /// Get the compatibility mode number for the Header section
    pub fn compatibility_mode(&self) -> u16 {
        match self {
            SqlServerVersion::Sql130 => 130,
            SqlServerVersion::Sql140 => 140,
            SqlServerVersion::Sql150 => 150,
            SqlServerVersion::Sql160 => 160,
        }
    }
}

/// Reference to another dacpac
#[derive(Debug, Clone)]
pub struct DacpacReference {
    pub path: PathBuf,
    pub database_variable: Option<String>,
    pub server_variable: Option<String>,
    pub suppress_missing_dependencies: bool,
}

/// NuGet package reference (e.g., Microsoft.SqlServer.Dacpacs.Master)
#[derive(Debug, Clone)]
pub struct PackageReference {
    /// Package name (e.g., "Microsoft.SqlServer.Dacpacs.Master")
    pub name: String,
    /// Package version (e.g., "150.0.0")
    pub version: String,
}

/// SQLCMD variable definition from sqlproj
#[derive(Debug, Clone)]
pub struct SqlCmdVariable {
    /// Variable name (e.g., "Environment")
    pub name: String,
    /// Variable value expression (e.g., "$(SqlCmdVar__1)")
    pub value: String,
    /// Default value (e.g., "Development")
    pub default_value: String,
}

/// Database options from sqlproj PropertyGroup
#[derive(Debug, Clone)]
pub struct DatabaseOptions {
    /// Default collation (e.g., "Latin1_General_CI_AS")
    pub collation: Option<String>,
    /// Page verify mode (e.g., "CHECKSUM", "TORN_PAGE_DETECTION", "NONE")
    pub page_verify: Option<String>,
    /// Default filegroup (e.g., "PRIMARY")
    pub default_filegroup: Option<String>,
    /// ANSI_NULL_DEFAULT ON/OFF setting
    pub ansi_null_default_on: bool,
    /// ANSI_NULLS ON/OFF setting
    pub ansi_nulls_on: bool,
    /// ANSI_WARNINGS ON/OFF setting
    pub ansi_warnings_on: bool,
    /// ARITHABORT ON/OFF setting
    pub arith_abort_on: bool,
    /// CONCAT_NULL_YIELDS_NULL ON/OFF setting
    pub concat_null_yields_null_on: bool,
    /// Full-text enabled
    pub full_text_enabled: bool,
    /// Torn page protection enabled
    pub torn_page_protection_on: bool,
    /// Default language (empty string if not specified)
    pub default_language: String,
    /// Default full-text language (empty string if not specified)
    pub default_full_text_language: String,
    /// Query store stale query threshold in days
    pub query_store_stale_query_threshold: u32,
}

impl Default for DatabaseOptions {
    fn default() -> Self {
        Self {
            // Default collation when not specified in sqlproj
            collation: Some("SQL_Latin1_General_CP1_CI_AS".to_string()),
            page_verify: Some("CHECKSUM".to_string()),
            default_filegroup: Some("PRIMARY".to_string()),
            ansi_null_default_on: true,
            ansi_nulls_on: true,
            ansi_warnings_on: true,
            arith_abort_on: true,
            concat_null_yields_null_on: true,
            // DotNet defaults to True for full-text enabled
            full_text_enabled: true,
            // DotNet defaults to False for torn page protection
            torn_page_protection_on: false,
            default_language: String::new(),
            default_full_text_language: String::new(),
            // DotNet default: 367 days
            query_store_stale_query_threshold: 367,
        }
    }
}

/// Parsed SQL project
#[derive(Debug, Clone)]
pub struct SqlProject {
    /// Project name
    pub name: String,
    /// Target SQL Server version
    pub target_platform: SqlServerVersion,
    /// Default schema
    pub default_schema: String,
    /// Collation LCID
    pub collation_lcid: u32,
    /// SQL files to compile
    pub sql_files: Vec<PathBuf>,
    /// Dacpac references
    pub dacpac_references: Vec<DacpacReference>,
    /// Package references (NuGet packages like Microsoft.SqlServer.Dacpacs.Master)
    pub package_references: Vec<PackageReference>,
    /// SQLCMD variables from sqlproj
    pub sqlcmd_variables: Vec<SqlCmdVariable>,
    /// Project directory
    pub project_dir: PathBuf,
    /// Pre-deployment script file (optional, at most one)
    pub pre_deploy_script: Option<PathBuf>,
    /// Post-deployment script file (optional, at most one)
    pub post_deploy_script: Option<PathBuf>,
    /// ANSI_NULLS setting (default: true)
    pub ansi_nulls: bool,
    /// QUOTED_IDENTIFIER setting (default: true)
    pub quoted_identifier: bool,
    /// Database options from sqlproj
    pub database_options: DatabaseOptions,
    /// DAC version for metadata (default: "1.0.0.0")
    pub dac_version: String,
    /// DAC description for metadata (optional)
    pub dac_description: Option<String>,
}

/// Parse a .sqlproj file
pub fn parse_sqlproj(path: &Path) -> Result<SqlProject> {
    let content = std::fs::read_to_string(path).map_err(|e| SqlPackageError::ProjectReadError {
        path: path.to_path_buf(),
        source: e,
    })?;

    let doc = Document::parse(&content).map_err(|e| SqlPackageError::ProjectParseError {
        path: path.to_path_buf(),
        source: e,
    })?;

    let project_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();

    let project_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Database")
        .to_string();

    let root = doc.root_element();

    // Parse target platform
    let target_platform = find_property_value(&root, "DSP")
        .and_then(|dsp| extract_version_from_dsp(&dsp))
        .unwrap_or_default();

    // Parse default schema
    let default_schema =
        find_property_value(&root, "DefaultSchema").unwrap_or_else(|| "dbo".to_string());

    // Collation LCID is always 1033 (US English) - actual collation name is in database_options
    let collation_lcid = 1033;

    // Parse ANSI_NULLS setting (default: true)
    let ansi_nulls = parse_bool_property(&root, "AnsiNulls", true);

    // Parse QUOTED_IDENTIFIER setting (default: true)
    let quoted_identifier = parse_bool_property(&root, "QuotedIdentifier", true);

    // Parse database options
    let database_options = parse_database_options(&root);

    // Parse DAC version (default: "1.0.0.0" per DacFx behavior)
    let dac_version =
        find_property_value(&root, "DacVersion").unwrap_or_else(|| "1.0.0.0".to_string());

    // Parse DAC description (optional, omit if not specified)
    let dac_description = find_property_value(&root, "DacDescription");

    // Find all SQL files
    let sql_files = find_sql_files(&root, &project_dir)?;

    // Find dacpac references
    let dacpac_references = find_dacpac_references(&root, &project_dir);

    // Find package references (NuGet packages)
    let package_references = find_package_references(&root);

    // Find SQLCMD variables
    let sqlcmd_variables = find_sqlcmd_variables(&root);

    // Find pre/post deployment scripts
    let (pre_deploy_script, post_deploy_script) = find_deployment_scripts(&root, &project_dir);

    Ok(SqlProject {
        name: project_name,
        target_platform,
        default_schema,
        collation_lcid,
        sql_files,
        dacpac_references,
        package_references,
        sqlcmd_variables,
        project_dir,
        pre_deploy_script,
        post_deploy_script,
        ansi_nulls,
        quoted_identifier,
        database_options,
        dac_version,
        dac_description,
    })
}

/// Parse database options from sqlproj PropertyGroup
fn parse_database_options(root: &roxmltree::Node) -> DatabaseOptions {
    let mut options = DatabaseOptions::default();

    // String properties - override defaults if specified
    if let Some(collation) = find_property_value(root, "DefaultCollation") {
        options.collation = Some(collation);
    }
    if let Some(page_verify) = find_property_value(root, "PageVerify") {
        options.page_verify = Some(page_verify);
    }
    if let Some(filegroup) = find_property_value(root, "DefaultFilegroup") {
        options.default_filegroup = Some(filegroup);
    }

    // Boolean properties - use helper to reduce boilerplate
    options.ansi_null_default_on = parse_bool_property(root, "AnsiNullDefaultOn", true);
    options.ansi_nulls_on = parse_bool_property(root, "AnsiNullsOn", true);
    options.ansi_warnings_on = parse_bool_property(root, "AnsiWarningsOn", true);
    options.arith_abort_on = parse_bool_property(root, "ArithAbortOn", true);
    options.concat_null_yields_null_on = parse_bool_property(root, "ConcatNullYieldsNullOn", true);
    options.full_text_enabled = parse_bool_property(root, "FullTextEnabled", true);

    options
}

fn find_property_value(root: &roxmltree::Node, property_name: &str) -> Option<String> {
    for node in root.descendants() {
        if node.tag_name().name() == property_name {
            return node.text().map(|s| s.to_string());
        }
    }
    None
}

/// Parse a boolean property from the project file, returning `default` if not found.
fn parse_bool_property(root: &roxmltree::Node, property_name: &str, default: bool) -> bool {
    find_property_value(root, property_name)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(default)
}

/// Find the text content of a child element by tag name.
fn find_child_text(node: &roxmltree::Node, tag_name: &str) -> Option<String> {
    node.children()
        .find(|n| n.tag_name().name() == tag_name)
        .and_then(|n| n.text())
        .map(|s| s.to_string())
}

fn extract_version_from_dsp(dsp: &str) -> Option<SqlServerVersion> {
    const VERSION_MAP: &[(&str, SqlServerVersion)] = &[
        ("Sql160", SqlServerVersion::Sql160),
        ("Sql150", SqlServerVersion::Sql150),
        ("Sql140", SqlServerVersion::Sql140),
        ("Sql130", SqlServerVersion::Sql130),
    ];
    VERSION_MAP
        .iter()
        .find(|(pattern, _)| dsp.contains(pattern))
        .map(|(_, version)| *version)
}

fn find_sql_files(root: &roxmltree::Node, project_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut sql_files = Vec::new();
    let mut include_patterns: Vec<String> = Vec::new();
    let mut exclude_patterns: Vec<String> = Vec::new();

    // Collect Build Include and Remove patterns
    for node in root.descendants() {
        if node.tag_name().name() == "Build" {
            if let Some(include) = node.attribute("Include") {
                include_patterns.push(include.replace('\\', "/"));
            }
            if let Some(remove) = node.attribute("Remove") {
                exclude_patterns.push(remove.replace('\\', "/"));
            }
        }
    }

    // Process include patterns
    for pattern in &include_patterns {
        if pattern.contains('*') {
            // Glob pattern - expand it
            let glob_pattern = project_dir.join(pattern);
            let glob_str = glob_pattern.to_string_lossy();
            if let Ok(paths) = glob::glob(&glob_str) {
                for entry in paths.filter_map(|p| p.ok()) {
                    if entry.extension().is_some_and(|ext| ext == "sql") {
                        sql_files.push(entry);
                    }
                }
            }
        } else if pattern.to_lowercase().ends_with(".sql") {
            // Direct file path
            let sql_path = project_dir.join(pattern);
            if sql_path.exists() {
                sql_files.push(sql_path);
            }
        }
    }

    // If no explicit Build items, glob for .sql files in project directory (SDK-style default)
    if sql_files.is_empty() && include_patterns.is_empty() {
        for entry in walkdir::WalkDir::new(project_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "sql") {
                // Skip bin and obj directories
                let path_str = path.to_string_lossy();
                if !path_str.contains("/bin/")
                    && !path_str.contains("/obj/")
                    && !path_str.contains("\\bin\\")
                    && !path_str.contains("\\obj\\")
                {
                    sql_files.push(path.to_path_buf());
                }
            }
        }
    }

    // Apply exclusion patterns (after SDK-style glob so Remove works for both styles)
    if !exclude_patterns.is_empty() {
        sql_files.retain(|file| {
            for pattern in &exclude_patterns {
                if pattern.contains('*') {
                    let glob_pattern = project_dir.join(pattern);
                    let glob_str = glob_pattern.to_string_lossy();
                    if let Ok(matcher) = glob::Pattern::new(&glob_str) {
                        if matcher.matches_path(file) {
                            return false;
                        }
                    }
                } else {
                    let exclude_path = project_dir.join(pattern);
                    if file == &exclude_path {
                        return false;
                    }
                }
            }
            true
        });
    }

    Ok(sql_files)
}

fn find_dacpac_references(root: &roxmltree::Node, project_dir: &Path) -> Vec<DacpacReference> {
    let mut references = Vec::new();

    for node in root.descendants() {
        if node.tag_name().name() == "ArtifactReference" {
            if let Some(include) = node.attribute("Include") {
                let path = project_dir.join(include.replace('\\', "/"));
                let database_variable = find_child_text(&node, "DatabaseVariableLiteralValue");
                let server_variable = find_child_text(&node, "ServerVariableLiteralValue");
                let suppress = find_child_text(&node, "SuppressMissingDependenciesErrors")
                    .map(|s| s.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);

                references.push(DacpacReference {
                    path,
                    database_variable,
                    server_variable,
                    suppress_missing_dependencies: suppress,
                });
            }
        }
    }

    references
}

/// Find PackageReference items in the project file
/// Format: <PackageReference Include="Microsoft.SqlServer.Dacpacs.Master" Version="150.0.0" />
fn find_package_references(root: &roxmltree::Node) -> Vec<PackageReference> {
    let mut references = Vec::new();

    for node in root.descendants() {
        if node.tag_name().name() == "PackageReference" {
            if let Some(include) = node.attribute("Include") {
                // Version can be an attribute or a child element
                let version = node
                    .attribute("Version")
                    .map(|s| s.to_string())
                    .or_else(|| find_child_text(&node, "Version"))
                    .unwrap_or_else(|| "0.0.0".to_string());

                references.push(PackageReference {
                    name: include.to_string(),
                    version,
                });
            }
        }
    }

    references
}

/// Find SqlCmdVariable items in the project file
/// Format:
/// ```xml
/// <SqlCmdVariable Include="Environment">
///   <Value>$(SqlCmdVar__1)</Value>
///   <DefaultValue>Development</DefaultValue>
/// </SqlCmdVariable>
/// ```
fn find_sqlcmd_variables(root: &roxmltree::Node) -> Vec<SqlCmdVariable> {
    let mut variables = Vec::new();

    for node in root.descendants() {
        if node.tag_name().name() == "SqlCmdVariable" {
            if let Some(name) = node.attribute("Include") {
                variables.push(SqlCmdVariable {
                    name: name.to_string(),
                    value: find_child_text(&node, "Value").unwrap_or_default(),
                    default_value: find_child_text(&node, "DefaultValue").unwrap_or_default(),
                });
            }
        }
    }

    variables
}

fn find_deployment_scripts(
    root: &roxmltree::Node,
    project_dir: &Path,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut pre_deploy: Option<PathBuf> = None;
    let mut post_deploy: Option<PathBuf> = None;

    for node in root.descendants() {
        match node.tag_name().name() {
            "PreDeploy" => {
                if let Some(include) = node.attribute("Include") {
                    let script_path = project_dir.join(include.replace('\\', "/"));
                    if script_path.exists() {
                        if pre_deploy.is_some() {
                            eprintln!(
                                "Warning: Multiple PreDeploy scripts specified, using first one"
                            );
                        } else {
                            pre_deploy = Some(script_path);
                        }
                    }
                }
            }
            "PostDeploy" => {
                if let Some(include) = node.attribute("Include") {
                    let script_path = project_dir.join(include.replace('\\', "/"));
                    if script_path.exists() {
                        if post_deploy.is_some() {
                            eprintln!(
                                "Warning: Multiple PostDeploy scripts specified, using first one"
                            );
                        } else {
                            post_deploy = Some(script_path);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    (pre_deploy, post_deploy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_server_version_from_str() {
        assert_eq!(
            "Sql160".parse::<SqlServerVersion>().unwrap(),
            SqlServerVersion::Sql160
        );
        assert_eq!(
            "sql150".parse::<SqlServerVersion>().unwrap(),
            SqlServerVersion::Sql150
        );
    }

    #[test]
    fn test_dsp_name() {
        assert!(SqlServerVersion::Sql160.dsp_name().contains("Sql160"));
    }
}
