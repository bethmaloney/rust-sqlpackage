//! Parser for .sqlproj files

use std::path::{Path, PathBuf};

use anyhow::Result;
use roxmltree::Document;

use crate::error::SqlPackageError;

/// SQL Server version target
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlServerVersion {
    Sql130, // SQL Server 2016
    Sql140, // SQL Server 2017
    Sql150, // SQL Server 2019
    Sql160, // SQL Server 2022
}

impl Default for SqlServerVersion {
    fn default() -> Self {
        SqlServerVersion::Sql160
    }
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
}

impl Default for DatabaseOptions {
    fn default() -> Self {
        Self {
            collation: None,
            page_verify: Some("CHECKSUM".to_string()),
            ansi_null_default_on: true,
            ansi_nulls_on: true,
            ansi_warnings_on: true,
            arith_abort_on: true,
            concat_null_yields_null_on: true,
            full_text_enabled: false,
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

    // Parse collation LCID
    let collation_lcid = find_property_value(&root, "DefaultCollation")
        .and_then(|c| extract_lcid_from_collation(&c))
        .unwrap_or(1033); // Default to US English

    // Parse ANSI_NULLS setting (default: true)
    let ansi_nulls = find_property_value(&root, "AnsiNulls")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);

    // Parse QUOTED_IDENTIFIER setting (default: true)
    let quoted_identifier = find_property_value(&root, "QuotedIdentifier")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);

    // Parse database options
    let database_options = parse_database_options(&root);

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
    })
}

/// Parse database options from sqlproj PropertyGroup
fn parse_database_options(root: &roxmltree::Node) -> DatabaseOptions {
    let mut options = DatabaseOptions::default();

    // DefaultCollation (e.g., "Latin1_General_CI_AS")
    if let Some(collation) = find_property_value(root, "DefaultCollation") {
        options.collation = Some(collation);
    }

    // PageVerify (e.g., "CHECKSUM", "TORN_PAGE_DETECTION", "NONE")
    if let Some(page_verify) = find_property_value(root, "PageVerify") {
        options.page_verify = Some(page_verify);
    }

    // AnsiNullDefaultOn (default: true)
    if let Some(val) = find_property_value(root, "AnsiNullDefaultOn") {
        options.ansi_null_default_on = val.eq_ignore_ascii_case("true");
    }

    // AnsiNullsOn (default: true)
    if let Some(val) = find_property_value(root, "AnsiNullsOn") {
        options.ansi_nulls_on = val.eq_ignore_ascii_case("true");
    }

    // AnsiWarningsOn (default: true)
    if let Some(val) = find_property_value(root, "AnsiWarningsOn") {
        options.ansi_warnings_on = val.eq_ignore_ascii_case("true");
    }

    // ArithAbortOn (default: true)
    if let Some(val) = find_property_value(root, "ArithAbortOn") {
        options.arith_abort_on = val.eq_ignore_ascii_case("true");
    }

    // ConcatNullYieldsNullOn (default: true)
    if let Some(val) = find_property_value(root, "ConcatNullYieldsNullOn") {
        options.concat_null_yields_null_on = val.eq_ignore_ascii_case("true");
    }

    // FullTextEnabled (default: false)
    if let Some(val) = find_property_value(root, "FullTextEnabled") {
        options.full_text_enabled = val.eq_ignore_ascii_case("true");
    }

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

fn extract_version_from_dsp(dsp: &str) -> Option<SqlServerVersion> {
    if dsp.contains("Sql160") {
        Some(SqlServerVersion::Sql160)
    } else if dsp.contains("Sql150") {
        Some(SqlServerVersion::Sql150)
    } else if dsp.contains("Sql140") {
        Some(SqlServerVersion::Sql140)
    } else if dsp.contains("Sql130") {
        Some(SqlServerVersion::Sql130)
    } else {
        None
    }
}

fn extract_lcid_from_collation(collation: &str) -> Option<u32> {
    // Common collations and their LCIDs
    if collation.starts_with("SQL_Latin1_General") || collation.starts_with("Latin1_General") {
        Some(1033) // US English
    } else {
        Some(1033) // Default to US English
    }
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
                    if entry.extension().map_or(false, |ext| ext == "sql") {
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

    // Apply exclusion patterns
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

    // If no explicit Build items, glob for .sql files in project directory (SDK-style default)
    if sql_files.is_empty() && include_patterns.is_empty() {
        for entry in walkdir::WalkDir::new(project_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "sql") {
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

    Ok(sql_files)
}

fn find_dacpac_references(root: &roxmltree::Node, project_dir: &Path) -> Vec<DacpacReference> {
    let mut references = Vec::new();

    for node in root.descendants() {
        if node.tag_name().name() == "ArtifactReference" {
            if let Some(include) = node.attribute("Include") {
                let path = project_dir.join(include.replace('\\', "/"));

                let database_variable = node
                    .children()
                    .find(|n| n.tag_name().name() == "DatabaseVariableLiteralValue")
                    .and_then(|n| n.text())
                    .map(|s| s.to_string());

                let server_variable = node
                    .children()
                    .find(|n| n.tag_name().name() == "ServerVariableLiteralValue")
                    .and_then(|n| n.text())
                    .map(|s| s.to_string());

                let suppress = node
                    .children()
                    .find(|n| n.tag_name().name() == "SuppressMissingDependenciesErrors")
                    .and_then(|n| n.text())
                    .map(|s| s.to_lowercase() == "true")
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
                    .or_else(|| {
                        node.children()
                            .find(|n| n.tag_name().name() == "Version")
                            .and_then(|n| n.text())
                            .map(|s| s.to_string())
                    })
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
                // Get the Value child element
                let value = node
                    .children()
                    .find(|n| n.tag_name().name() == "Value")
                    .and_then(|n| n.text())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                // Get the DefaultValue child element
                let default_value = node
                    .children()
                    .find(|n| n.tag_name().name() == "DefaultValue")
                    .and_then(|n| n.text())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                variables.push(SqlCmdVariable {
                    name: name.to_string(),
                    value,
                    default_value,
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
