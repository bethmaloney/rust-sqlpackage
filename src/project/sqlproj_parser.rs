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
            SqlServerVersion::Sql130 => "Microsoft.Data.Tools.Schema.Sql.Sql130DatabaseSchemaProvider",
            SqlServerVersion::Sql140 => "Microsoft.Data.Tools.Schema.Sql.Sql140DatabaseSchemaProvider",
            SqlServerVersion::Sql150 => "Microsoft.Data.Tools.Schema.Sql.Sql150DatabaseSchemaProvider",
            SqlServerVersion::Sql160 => "Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider",
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
    /// Project directory
    pub project_dir: PathBuf,
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

    let project_dir = path
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

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
    let default_schema = find_property_value(&root, "DefaultSchema").unwrap_or_else(|| "dbo".to_string());

    // Parse collation LCID
    let collation_lcid = find_property_value(&root, "DefaultCollation")
        .and_then(|c| extract_lcid_from_collation(&c))
        .unwrap_or(1033); // Default to US English

    // Find all SQL files
    let sql_files = find_sql_files(&root, &project_dir)?;

    // Find dacpac references
    let dacpac_references = find_dacpac_references(&root, &project_dir);

    Ok(SqlProject {
        name: project_name,
        target_platform,
        default_schema,
        collation_lcid,
        sql_files,
        dacpac_references,
        project_dir,
    })
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
                if !path_str.contains("/bin/") && !path_str.contains("/obj/")
                   && !path_str.contains("\\bin\\") && !path_str.contains("\\obj\\") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_server_version_from_str() {
        assert_eq!("Sql160".parse::<SqlServerVersion>().unwrap(), SqlServerVersion::Sql160);
        assert_eq!("sql150".parse::<SqlServerVersion>().unwrap(), SqlServerVersion::Sql150);
    }

    #[test]
    fn test_dsp_name() {
        assert!(SqlServerVersion::Sql160.dsp_name().contains("Sql160"));
    }
}
