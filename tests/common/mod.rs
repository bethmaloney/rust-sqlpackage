//! Common test utilities for rust-sqlpackage tests

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use zip::ZipArchive;

/// Test context with temporary directory for isolated test execution
pub struct TestContext {
    /// Kept to prevent temp directory cleanup until TestContext is dropped
    _temp_dir: TempDir,
    pub project_dir: PathBuf,
    /// Stored for debugging purposes
    _fixture_name: String,
}

impl TestContext {
    /// Create a new test context by copying a fixture to a temp directory
    pub fn with_fixture(fixture_name: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(fixture_name);

        let project_dir = temp_dir.path().to_path_buf();

        // Copy fixture to temp directory
        copy_dir_recursive(&fixture_path, &project_dir).expect("Failed to copy fixture");

        Self {
            _temp_dir: temp_dir,
            project_dir,
            _fixture_name: fixture_name.to_string(),
        }
    }

    /// Get the path to the .sqlproj file
    pub fn project_path(&self) -> PathBuf {
        self.project_dir.join("project.sqlproj")
    }

    /// Build the project using rust-sqlpackage library
    pub fn build(&self) -> BuildResult {
        let project_path = self.project_path();

        match rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
            project_path,
            output_path: None,
            target_platform: "Sql160".to_string(),
            verbose: false,
        }) {
            Ok(dacpac_path) => BuildResult {
                success: true,
                dacpac_path: Some(dacpac_path),
                errors: vec![],
            },
            Err(e) => BuildResult {
                success: false,
                dacpac_path: None,
                errors: vec![e.to_string()],
            },
        }
    }

    /// Build the project and return the dacpac path, panicking if build fails.
    ///
    /// This is a convenience method that combines build + assert + unwrap:
    /// ```rust,ignore
    /// let result = ctx.build();
    /// assert!(result.success, "Build failed: {:?}", result.errors);
    /// let dacpac_path = result.dacpac_path.unwrap();
    /// ```
    pub fn build_successfully(&self) -> PathBuf {
        let result = self.build();
        assert!(
            result.success,
            "Build failed for fixture '{}': {:?}",
            self._fixture_name, result.errors
        );
        result
            .dacpac_path
            .expect("Build succeeded but no dacpac path")
    }
}

/// Result of a build operation
#[derive(Debug)]
pub struct BuildResult {
    pub success: bool,
    pub dacpac_path: Option<PathBuf>,
    pub errors: Vec<String>,
}

/// Information extracted from a dacpac file
#[derive(Debug, Default)]
pub struct DacpacInfo {
    pub has_model_xml: bool,
    pub has_metadata_xml: bool,
    pub has_origin_xml: bool,
    pub has_content_types: bool,
    pub has_predeploy: bool,
    pub has_postdeploy: bool,
    pub model_xml_content: Option<String>,
    pub metadata_xml_content: Option<String>,
    pub origin_xml_content: Option<String>,
    pub content_types_xml_content: Option<String>,
    pub predeploy_content: Option<String>,
    pub postdeploy_content: Option<String>,
    pub tables: Vec<String>,
    pub views: Vec<String>,
    pub schemas: Vec<String>,
}

impl DacpacInfo {
    /// Parse a dacpac file and extract information
    pub fn from_dacpac(path: &Path) -> Result<Self, String> {
        let file = fs::File::open(path).map_err(|e| format!("Failed to open dacpac: {}", e))?;

        let mut archive =
            ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

        let mut info = DacpacInfo::default();

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

            let name = file.name().to_string();

            match name.as_str() {
                "model.xml" => {
                    info.has_model_xml = true;
                    let mut content = String::new();
                    file.read_to_string(&mut content)
                        .map_err(|e| format!("Failed to read model.xml: {}", e))?;
                    info.tables = extract_tables_from_model(&content);
                    info.views = extract_views_from_model(&content);
                    info.schemas = extract_schemas_from_model(&content);
                    info.model_xml_content = Some(content);
                }
                "DacMetadata.xml" => {
                    info.has_metadata_xml = true;
                    let mut content = String::new();
                    file.read_to_string(&mut content)
                        .map_err(|e| format!("Failed to read DacMetadata.xml: {}", e))?;
                    info.metadata_xml_content = Some(content);
                }
                "Origin.xml" => {
                    info.has_origin_xml = true;
                    let mut content = String::new();
                    file.read_to_string(&mut content)
                        .map_err(|e| format!("Failed to read Origin.xml: {}", e))?;
                    info.origin_xml_content = Some(content);
                }
                "[Content_Types].xml" => {
                    info.has_content_types = true;
                    let mut content = String::new();
                    file.read_to_string(&mut content)
                        .map_err(|e| format!("Failed to read [Content_Types].xml: {}", e))?;
                    info.content_types_xml_content = Some(content);
                }
                "predeploy.sql" => {
                    info.has_predeploy = true;
                    let mut content = String::new();
                    file.read_to_string(&mut content)
                        .map_err(|e| format!("Failed to read predeploy.sql: {}", e))?;
                    info.predeploy_content = Some(content);
                }
                "postdeploy.sql" => {
                    info.has_postdeploy = true;
                    let mut content = String::new();
                    file.read_to_string(&mut content)
                        .map_err(|e| format!("Failed to read postdeploy.sql: {}", e))?;
                    info.postdeploy_content = Some(content);
                }
                _ => {}
            }
        }

        Ok(info)
    }

    /// Check if dacpac has all required files
    pub fn is_valid(&self) -> bool {
        self.has_model_xml && self.has_metadata_xml && self.has_origin_xml && self.has_content_types
    }
}

/// Extract table names from model.xml content
fn extract_tables_from_model(content: &str) -> Vec<String> {
    let mut tables = Vec::new();

    // Look for Element Type="SqlTable" Name="..."
    for line in content.lines() {
        if line.contains("Type=\"SqlTable\"") {
            if let Some(start) = line.find("Name=\"") {
                let rest = &line[start + 6..];
                if let Some(end) = rest.find('"') {
                    tables.push(rest[..end].to_string());
                }
            }
        }
    }

    tables
}

/// Extract view names from model.xml content
fn extract_views_from_model(content: &str) -> Vec<String> {
    let mut views = Vec::new();

    // Look for Element Type="SqlView" Name="..."
    for line in content.lines() {
        if line.contains("Type=\"SqlView\"") {
            if let Some(start) = line.find("Name=\"") {
                let rest = &line[start + 6..];
                if let Some(end) = rest.find('"') {
                    views.push(rest[..end].to_string());
                }
            }
        }
    }

    views
}

/// Extract schema names from model.xml content
fn extract_schemas_from_model(content: &str) -> Vec<String> {
    let mut schemas = Vec::new();

    // Look for Element Type="SqlSchema" Name="..."
    for line in content.lines() {
        if line.contains("Type=\"SqlSchema\"") {
            if let Some(start) = line.find("Name=\"") {
                let rest = &line[start + 6..];
                if let Some(end) = rest.find('"') {
                    schemas.push(rest[..end].to_string());
                }
            }
        }
    }

    schemas
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Assert that a dacpac contains a specific table
#[macro_export]
macro_rules! assert_dacpac_contains_table {
    ($info:expr, $table:expr) => {
        assert!(
            $info.tables.iter().any(|t| t.contains($table)),
            "Expected dacpac to contain table '{}', found: {:?}",
            $table,
            $info.tables
        );
    };
}

/// Assert that a dacpac contains a specific view
#[macro_export]
macro_rules! assert_dacpac_contains_view {
    ($info:expr, $view:expr) => {
        assert!(
            $info.views.iter().any(|v| v.contains($view)),
            "Expected dacpac to contain view '{}', found: {:?}",
            $view,
            $info.views
        );
    };
}

/// Assert that a dacpac does not contain a specific table
#[macro_export]
macro_rules! assert_dacpac_not_contains_table {
    ($info:expr, $table:expr) => {
        assert!(
            !$info.tables.iter().any(|t| t.contains($table)),
            "Expected dacpac to NOT contain table '{}', but found: {:?}",
            $table,
            $info.tables
        );
    };
}
