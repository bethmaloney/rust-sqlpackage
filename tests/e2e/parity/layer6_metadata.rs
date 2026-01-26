//! Layer 6: Metadata File Comparison
//!
//! Compares metadata files beyond model.xml:
//! - [Content_Types].xml - MIME type definitions
//! - DacMetadata.xml - Package metadata
//! - Origin.xml - Build/origin information
//! - predeploy.sql / postdeploy.sql - Deploy scripts

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use super::types::{ContentTypesXml, DacMetadataXml, MetadataFileError, OriginXml};

// =============================================================================
// [Content_Types].xml Comparison
// =============================================================================

/// Extract [Content_Types].xml from a dacpac file
pub fn extract_content_types_xml(dacpac_path: &Path) -> Result<String, String> {
    let file = fs::File::open(dacpac_path).map_err(|e| format!("Failed to open dacpac: {}", e))?;

    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

        if file.name() == "[Content_Types].xml" {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| format!("Failed to read [Content_Types].xml: {}", e))?;
            return Ok(content);
        }
    }

    Err("[Content_Types].xml not found in dacpac".to_string())
}

impl ContentTypesXml {
    /// Parse [Content_Types].xml content using roxmltree
    pub fn from_xml(xml: &str) -> Result<Self, String> {
        let doc =
            roxmltree::Document::parse(xml).map_err(|e| format!("Failed to parse XML: {}", e))?;

        let mut types = HashMap::new();

        // Find all Default elements with Extension and ContentType attributes
        // Example: <Default Extension="xml" ContentType="text/xml" />
        for node in doc.descendants() {
            if node.has_tag_name("Default") {
                if let (Some(ext), Some(content_type)) =
                    (node.attribute("Extension"), node.attribute("ContentType"))
                {
                    types.insert(ext.to_lowercase(), content_type.to_string());
                }
            }
            // Also check for Override elements which may define specific paths
            // Example: <Override PartName="/model.xml" ContentType="text/xml" />
            if node.has_tag_name("Override") {
                if let (Some(part_name), Some(content_type)) =
                    (node.attribute("PartName"), node.attribute("ContentType"))
                {
                    // Extract extension from part name
                    if let Some(ext) = part_name.rsplit('.').next() {
                        types.insert(ext.to_lowercase(), content_type.to_string());
                    }
                }
            }
        }

        Ok(Self { types })
    }

    /// Parse [Content_Types].xml from a dacpac file
    pub fn from_dacpac(dacpac_path: &Path) -> Result<Self, String> {
        let xml = extract_content_types_xml(dacpac_path)?;
        Self::from_xml(&xml)
    }
}

/// Compare [Content_Types].xml between two dacpacs
///
/// Checks that:
/// 1. Both dacpacs have [Content_Types].xml
/// 2. Same file extensions are defined
/// 3. Same MIME content types are used for each extension
///
/// Note: MIME type differences between `text/xml` and `application/xml` are
/// semantically equivalent but flagged for exact parity tracking.
pub fn compare_content_types(rust_dacpac: &Path, dotnet_dacpac: &Path) -> Vec<MetadataFileError> {
    let mut errors = Vec::new();

    // Extract Content_Types from both dacpacs
    let rust_ct = match ContentTypesXml::from_dacpac(rust_dacpac) {
        Ok(ct) => ct,
        Err(_) => {
            errors.push(MetadataFileError::FileMissing {
                file_name: "[Content_Types].xml".to_string(),
                missing_in_rust: true,
            });
            return errors;
        }
    };

    let dotnet_ct = match ContentTypesXml::from_dacpac(dotnet_dacpac) {
        Ok(ct) => ct,
        Err(_) => {
            errors.push(MetadataFileError::FileMissing {
                file_name: "[Content_Types].xml".to_string(),
                missing_in_rust: false,
            });
            return errors;
        }
    };

    // Compare type counts
    if rust_ct.types.len() != dotnet_ct.types.len() {
        errors.push(MetadataFileError::ContentTypeCountMismatch {
            rust_count: rust_ct.types.len(),
            dotnet_count: dotnet_ct.types.len(),
        });
    }

    // Compare all extensions present in either dacpac
    let all_extensions: BTreeSet<_> = rust_ct
        .types
        .keys()
        .chain(dotnet_ct.types.keys())
        .cloned()
        .collect();

    for ext in all_extensions {
        let rust_type = rust_ct.types.get(&ext);
        let dotnet_type = dotnet_ct.types.get(&ext);

        if rust_type != dotnet_type {
            errors.push(MetadataFileError::ContentTypeMismatch {
                extension: ext,
                rust_content_type: rust_type.cloned(),
                dotnet_content_type: dotnet_type.cloned(),
            });
        }
    }

    errors
}

// =============================================================================
// DacMetadata.xml Comparison
// =============================================================================

/// Extract DacMetadata.xml from a dacpac file
pub fn extract_dac_metadata_xml(dacpac_path: &Path) -> Result<String, String> {
    let file = fs::File::open(dacpac_path).map_err(|e| format!("Failed to open dacpac: {}", e))?;

    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

        if file.name() == "DacMetadata.xml" {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| format!("Failed to read DacMetadata.xml: {}", e))?;
            return Ok(content);
        }
    }

    Err("DacMetadata.xml not found in dacpac".to_string())
}

impl DacMetadataXml {
    /// Parse DacMetadata.xml content using roxmltree
    ///
    /// DacMetadata.xml has the following structure:
    /// ```xml
    /// <?xml version="1.0" encoding="utf-8"?>
    /// <DacType xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
    ///   <Name>project</Name>
    ///   <Version>1.0.0.0</Version>
    ///   <Description>Optional description</Description>
    /// </DacType>
    /// ```
    ///
    /// Note: The root element is `DacType` per the MS XSD schema.
    /// Description is typically omitted when empty.
    pub fn from_xml(xml: &str) -> Result<Self, String> {
        let doc =
            roxmltree::Document::parse(xml).map_err(|e| format!("Failed to parse XML: {}", e))?;

        let mut name = None;
        let mut version = None;
        let mut description = None;

        // Find Name, Version, and Description elements
        for node in doc.descendants() {
            if node.has_tag_name("Name") {
                name = node.text().map(|s| s.to_string());
            } else if node.has_tag_name("Version") {
                version = node.text().map(|s| s.to_string());
            } else if node.has_tag_name("Description") {
                description = node.text().map(|s| s.to_string());
            }
        }

        Ok(Self {
            name,
            version,
            description,
        })
    }

    /// Parse DacMetadata.xml from a dacpac file
    pub fn from_dacpac(dacpac_path: &Path) -> Result<Self, String> {
        let xml = extract_dac_metadata_xml(dacpac_path)?;
        Self::from_xml(&xml)
    }
}

/// Compare DacMetadata.xml between two dacpacs
///
/// Compares the metadata fields between Rust and DotNet output:
/// - Name: Should match the project/database name
/// - Version: Package version (typically "1.0.0.0")
/// - Description: Optional, typically omitted when empty
///
/// Note: Version differences are expected if hardcoded differently.
/// The comparison ignores timestamp/build-specific fields.
pub fn compare_dac_metadata(rust_dacpac: &Path, dotnet_dacpac: &Path) -> Vec<MetadataFileError> {
    let mut errors = Vec::new();

    // Extract DacMetadata from both dacpacs
    let rust_meta = match DacMetadataXml::from_dacpac(rust_dacpac) {
        Ok(meta) => meta,
        Err(_) => {
            errors.push(MetadataFileError::FileMissing {
                file_name: "DacMetadata.xml".to_string(),
                missing_in_rust: true,
            });
            return errors;
        }
    };

    let dotnet_meta = match DacMetadataXml::from_dacpac(dotnet_dacpac) {
        Ok(meta) => meta,
        Err(_) => {
            errors.push(MetadataFileError::FileMissing {
                file_name: "DacMetadata.xml".to_string(),
                missing_in_rust: false,
            });
            return errors;
        }
    };

    // Compare Name field
    if rust_meta.name != dotnet_meta.name {
        errors.push(MetadataFileError::DacMetadataMismatch {
            field_name: "Name".to_string(),
            rust_value: rust_meta.name.clone(),
            dotnet_value: dotnet_meta.name.clone(),
        });
    }

    // Compare Version field
    if rust_meta.version != dotnet_meta.version {
        errors.push(MetadataFileError::DacMetadataMismatch {
            field_name: "Version".to_string(),
            rust_value: rust_meta.version.clone(),
            dotnet_value: dotnet_meta.version.clone(),
        });
    }

    // Compare Description field
    // Both None or both Some with same value are considered matching
    if rust_meta.description != dotnet_meta.description {
        errors.push(MetadataFileError::DacMetadataMismatch {
            field_name: "Description".to_string(),
            rust_value: rust_meta.description.clone(),
            dotnet_value: dotnet_meta.description.clone(),
        });
    }

    errors
}

// =============================================================================
// Origin.xml Comparison
// =============================================================================

/// Extract Origin.xml from a dacpac file
pub fn extract_origin_xml(dacpac_path: &Path) -> Result<String, String> {
    let file = fs::File::open(dacpac_path).map_err(|e| format!("Failed to open dacpac: {}", e))?;

    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

        if file.name() == "Origin.xml" {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| format!("Failed to read Origin.xml: {}", e))?;
            return Ok(content);
        }
    }

    Err("Origin.xml not found in dacpac".to_string())
}

impl OriginXml {
    /// Parse Origin.xml content using roxmltree
    ///
    /// Origin.xml has the following structure:
    /// ```xml
    /// <?xml version="1.0" encoding="utf-8"?>
    /// <DacOrigin xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
    ///   <PackageProperties>
    ///     <Version>3.1.0.0</Version>
    ///     <ContainsExportedData>false</ContainsExportedData>
    ///     <StreamVersions>
    ///       <Version StreamName="Data">2.0.0.0</Version>
    ///       <Version StreamName="DeploymentContributors">1.0.0.0</Version>
    ///     </StreamVersions>
    ///   </PackageProperties>
    ///   <Operation>
    ///     <Identity>...</Identity>
    ///     <Start>...</Start>
    ///     <End>...</End>
    ///     <ProductName>...</ProductName>
    ///     <ProductVersion>...</ProductVersion>
    ///     <ProductSchema>...</ProductSchema>
    ///   </Operation>
    ///   <Checksums>
    ///     <Checksum Uri="/model.xml">...</Checksum>
    ///   </Checksums>
    /// </DacOrigin>
    /// ```
    pub fn from_xml(xml: &str) -> Result<Self, String> {
        let doc =
            roxmltree::Document::parse(xml).map_err(|e| format!("Failed to parse XML: {}", e))?;

        let mut origin = OriginXml::default();

        // Find elements within the document
        for node in doc.descendants() {
            // PackageProperties fields
            if node.has_tag_name("Version") {
                // Check if this is the PackageProperties/Version (direct child of PackageProperties)
                // or StreamVersions/Version (has StreamName attribute)
                if let Some(stream_name) = node.attribute("StreamName") {
                    match stream_name {
                        "Data" => origin.data_stream_version = node.text().map(|s| s.to_string()),
                        "DeploymentContributors" => {
                            origin.deployment_contributors_version =
                                node.text().map(|s| s.to_string())
                        }
                        _ => {}
                    }
                } else if node
                    .parent()
                    .is_some_and(|p| p.has_tag_name("PackageProperties"))
                {
                    origin.package_version = node.text().map(|s| s.to_string());
                }
            } else if node.has_tag_name("ContainsExportedData") {
                origin.contains_exported_data = node.text().map(|s| s.to_string());
            }
            // Operation fields
            else if node.has_tag_name("ProductName") {
                origin.product_name = node.text().map(|s| s.to_string());
            } else if node.has_tag_name("ProductVersion") {
                origin.product_version = node.text().map(|s| s.to_string());
            } else if node.has_tag_name("ProductSchema") {
                origin.product_schema = node.text().map(|s| s.to_string());
            }
        }

        Ok(origin)
    }

    /// Parse Origin.xml from a dacpac file
    pub fn from_dacpac(dacpac_path: &Path) -> Result<Self, String> {
        let xml = extract_origin_xml(dacpac_path)?;
        Self::from_xml(&xml)
    }
}

/// Compare Origin.xml between two dacpacs
///
/// Compares the following fields between Rust and DotNet output:
/// - PackageProperties/Version: Package format version (e.g., "3.1.0.0")
/// - PackageProperties/ContainsExportedData: Boolean flag
/// - PackageProperties/StreamVersions/Version[@StreamName="Data"]
/// - PackageProperties/StreamVersions/Version[@StreamName="DeploymentContributors"]
/// - Operation/ProductName: Product identifier
/// - Operation/ProductVersion: Product version
/// - Operation/ProductSchema: Schema URL
///
/// Note: Timestamps (Start/End) and Checksums are intentionally ignored as they
/// will always differ between builds. ProductName and ProductVersion are expected
/// to differ between rust-sqlpackage and DotNet DacFx - these are informational.
pub fn compare_origin_xml(rust_dacpac: &Path, dotnet_dacpac: &Path) -> Vec<MetadataFileError> {
    let mut errors = Vec::new();

    // Extract Origin.xml from both dacpacs
    let rust_origin = match OriginXml::from_dacpac(rust_dacpac) {
        Ok(origin) => origin,
        Err(_) => {
            errors.push(MetadataFileError::FileMissing {
                file_name: "Origin.xml".to_string(),
                missing_in_rust: true,
            });
            return errors;
        }
    };

    let dotnet_origin = match OriginXml::from_dacpac(dotnet_dacpac) {
        Ok(origin) => origin,
        Err(_) => {
            errors.push(MetadataFileError::FileMissing {
                file_name: "Origin.xml".to_string(),
                missing_in_rust: false,
            });
            return errors;
        }
    };

    // Compare PackageProperties/Version
    if rust_origin.package_version != dotnet_origin.package_version {
        errors.push(MetadataFileError::OriginXmlMismatch {
            field_name: "PackageProperties/Version".to_string(),
            rust_value: rust_origin.package_version.clone(),
            dotnet_value: dotnet_origin.package_version.clone(),
        });
    }

    // Compare ContainsExportedData
    if rust_origin.contains_exported_data != dotnet_origin.contains_exported_data {
        errors.push(MetadataFileError::OriginXmlMismatch {
            field_name: "PackageProperties/ContainsExportedData".to_string(),
            rust_value: rust_origin.contains_exported_data.clone(),
            dotnet_value: dotnet_origin.contains_exported_data.clone(),
        });
    }

    // Compare Data stream version
    if rust_origin.data_stream_version != dotnet_origin.data_stream_version {
        errors.push(MetadataFileError::OriginXmlMismatch {
            field_name: "StreamVersions/Data".to_string(),
            rust_value: rust_origin.data_stream_version.clone(),
            dotnet_value: dotnet_origin.data_stream_version.clone(),
        });
    }

    // Compare DeploymentContributors stream version
    if rust_origin.deployment_contributors_version != dotnet_origin.deployment_contributors_version
    {
        errors.push(MetadataFileError::OriginXmlMismatch {
            field_name: "StreamVersions/DeploymentContributors".to_string(),
            rust_value: rust_origin.deployment_contributors_version.clone(),
            dotnet_value: dotnet_origin.deployment_contributors_version.clone(),
        });
    }

    // Compare ProductName (informational - expected to differ)
    if rust_origin.product_name != dotnet_origin.product_name {
        errors.push(MetadataFileError::OriginXmlMismatch {
            field_name: "Operation/ProductName".to_string(),
            rust_value: rust_origin.product_name.clone(),
            dotnet_value: dotnet_origin.product_name.clone(),
        });
    }

    // Compare ProductVersion (informational - expected to differ)
    if rust_origin.product_version != dotnet_origin.product_version {
        errors.push(MetadataFileError::OriginXmlMismatch {
            field_name: "Operation/ProductVersion".to_string(),
            rust_value: rust_origin.product_version.clone(),
            dotnet_value: dotnet_origin.product_version.clone(),
        });
    }

    // Compare ProductSchema
    if rust_origin.product_schema != dotnet_origin.product_schema {
        errors.push(MetadataFileError::OriginXmlMismatch {
            field_name: "Operation/ProductSchema".to_string(),
            rust_value: rust_origin.product_schema.clone(),
            dotnet_value: dotnet_origin.product_schema.clone(),
        });
    }

    errors
}

// =============================================================================
// Deploy Script Comparison (Phase 5.4)
// =============================================================================

/// Extract a deploy script (predeploy.sql or postdeploy.sql) from a dacpac file.
///
/// Returns Ok(Some(content)) if script exists, Ok(None) if not present, Err on ZIP error.
pub fn extract_deploy_script(
    dacpac_path: &Path,
    script_name: &str,
) -> Result<Option<String>, String> {
    let file = fs::File::open(dacpac_path).map_err(|e| format!("Failed to open dacpac: {}", e))?;

    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

        if file.name() == script_name {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| format!("Failed to read {}: {}", script_name, e))?;
            return Ok(Some(content));
        }
    }

    Ok(None)
}

/// Normalize whitespace in SQL script content for comparison.
///
/// This ensures that minor whitespace differences (trailing spaces, different line endings,
/// trailing newlines) don't cause false positives in script comparison.
///
/// Normalization rules:
/// - Convert CRLF to LF (Windows to Unix line endings)
/// - Trim trailing whitespace from each line
/// - Remove trailing empty lines
/// - Preserve leading whitespace and internal blank lines (significant for readability)
pub fn normalize_script_whitespace(content: &str) -> String {
    // Convert CRLF to LF
    let content = content.replace("\r\n", "\n");

    // Process each line: trim trailing whitespace
    let lines: Vec<&str> = content.lines().map(|line| line.trim_end()).collect();

    // Remove trailing empty lines
    let mut result: Vec<&str> = lines;
    while result.last().is_some_and(|line| line.is_empty()) {
        result.pop();
    }

    result.join("\n")
}

/// Compare pre/post-deploy scripts between two dacpacs.
///
/// Compares both predeploy.sql and postdeploy.sql files if present.
/// Scripts are normalized (whitespace trimmed) before comparison to avoid
/// false positives from minor formatting differences.
///
/// Error scenarios:
/// - Script present in one dacpac but not the other: `DeployScriptMissing`
/// - Script content differs after normalization: `DeployScriptMismatch`
pub fn compare_deploy_scripts(rust_dacpac: &Path, dotnet_dacpac: &Path) -> Vec<MetadataFileError> {
    let mut errors = Vec::new();

    // Compare predeploy.sql
    errors.extend(compare_single_deploy_script(
        rust_dacpac,
        dotnet_dacpac,
        "predeploy.sql",
    ));

    // Compare postdeploy.sql
    errors.extend(compare_single_deploy_script(
        rust_dacpac,
        dotnet_dacpac,
        "postdeploy.sql",
    ));

    errors
}

/// Compare a single deploy script between two dacpacs
fn compare_single_deploy_script(
    rust_dacpac: &Path,
    dotnet_dacpac: &Path,
    script_name: &str,
) -> Vec<MetadataFileError> {
    let mut errors = Vec::new();

    // Extract from both dacpacs
    let rust_script = match extract_deploy_script(rust_dacpac, script_name) {
        Ok(s) => s,
        Err(e) => {
            // Log extraction error but continue - treat as missing
            eprintln!(
                "Warning: Failed to extract {} from Rust dacpac: {}",
                script_name, e
            );
            None
        }
    };

    let dotnet_script = match extract_deploy_script(dotnet_dacpac, script_name) {
        Ok(s) => s,
        Err(e) => {
            // Log extraction error but continue - treat as missing
            eprintln!(
                "Warning: Failed to extract {} from DotNet dacpac: {}",
                script_name, e
            );
            None
        }
    };

    match (&rust_script, &dotnet_script) {
        // Both missing - no error
        (None, None) => {}

        // Present in DotNet only
        (None, Some(_)) => {
            errors.push(MetadataFileError::DeployScriptMissing {
                script_name: script_name.to_string(),
                missing_in_rust: true,
            });
        }

        // Present in Rust only
        (Some(_), None) => {
            errors.push(MetadataFileError::DeployScriptMissing {
                script_name: script_name.to_string(),
                missing_in_rust: false,
            });
        }

        // Both present - compare content
        (Some(rust_content), Some(dotnet_content)) => {
            let rust_normalized = normalize_script_whitespace(rust_content);
            let dotnet_normalized = normalize_script_whitespace(dotnet_content);

            if rust_normalized != dotnet_normalized {
                errors.push(MetadataFileError::DeployScriptMismatch {
                    script_name: script_name.to_string(),
                    rust_content: Some(rust_normalized),
                    dotnet_content: Some(dotnet_normalized),
                });
            }
        }
    }

    errors
}

// =============================================================================
// Unified Metadata File Comparison
// =============================================================================

/// Compare all metadata files between two dacpacs in a single unified call.
///
/// This function consolidates all Phase 5 metadata file comparisons:
/// - [Content_Types].xml - MIME type definitions (Phase 5.1)
/// - DacMetadata.xml - Package metadata (Phase 5.2)
/// - Origin.xml - Build/origin information (Phase 5.3)
/// - predeploy.sql / postdeploy.sql - Deploy scripts (Phase 5.4)
///
/// Use this function for comprehensive metadata parity testing without needing
/// to call each comparison function individually.
///
/// # Arguments
/// * `rust_dacpac` - Path to the Rust-generated dacpac file
/// * `dotnet_dacpac` - Path to the DotNet-generated dacpac file
///
/// # Returns
/// A vector of `MetadataFileError` containing all detected differences across
/// all metadata files. An empty vector indicates full metadata parity.
pub fn compare_dacpac_files(rust_dacpac: &Path, dotnet_dacpac: &Path) -> Vec<MetadataFileError> {
    let mut errors = Vec::new();

    // Phase 5.1: [Content_Types].xml comparison
    errors.extend(compare_content_types(rust_dacpac, dotnet_dacpac));

    // Phase 5.2: DacMetadata.xml comparison
    errors.extend(compare_dac_metadata(rust_dacpac, dotnet_dacpac));

    // Phase 5.3: Origin.xml comparison
    errors.extend(compare_origin_xml(rust_dacpac, dotnet_dacpac));

    // Phase 5.4: Pre/post-deploy script comparison
    errors.extend(compare_deploy_scripts(rust_dacpac, dotnet_dacpac));

    errors
}
