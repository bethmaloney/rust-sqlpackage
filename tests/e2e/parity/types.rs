//! Shared types and data structures for dacpac comparison
//!
//! This module contains all the core data structures used across the
//! comparison layers, including:
//! - Model representation types (ModelElement, DacpacModel)
//! - Error types for each layer
//! - Options and result types

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

// =============================================================================
// Core Model Types
// =============================================================================

/// Represents a parsed element from model.xml
#[derive(Debug, Clone)]
pub struct ModelElement {
    pub element_type: String,
    pub name: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub children: Vec<ModelElement>,
    pub relationships: Vec<Relationship>,
}

/// Represents a relationship within an element.
/// Relationships link elements to other objects (tables, columns, types, etc.)
#[derive(Debug, Clone)]
pub struct Relationship {
    /// The relationship name (e.g., "Schema", "Columns", "DefiningTable", "Type")
    pub name: String,
    /// External references by name (e.g., "[dbo].[Products]", "[int]")
    pub references: Vec<ReferenceEntry>,
    /// Nested elements within the relationship (e.g., SqlSimpleColumn, SqlTypeSpecifier)
    pub entries: Vec<ModelElement>,
}

/// Represents a reference entry within a relationship
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceEntry {
    /// The referenced object name (e.g., "[dbo].[Products]", "[int]")
    pub name: String,
    /// Optional external source (e.g., "BuiltIns" for SQL Server built-in types)
    pub external_source: Option<String>,
}

/// Represents the parsed model.xml structure
#[derive(Debug)]
pub struct DacpacModel {
    pub elements: Vec<ModelElement>,
    /// Index of top-level elements by (type, name)
    pub element_index: HashMap<(String, String), usize>,
}

impl DacpacModel {
    /// Create an empty model (for testing)
    pub fn empty() -> Self {
        Self {
            elements: Vec::new(),
            element_index: HashMap::new(),
        }
    }

    /// Extract and parse model.xml from a dacpac file
    pub fn from_dacpac(dacpac_path: &Path) -> Result<Self, String> {
        let xml = extract_model_xml(dacpac_path)?;
        Self::from_xml(&xml)
    }

    /// Parse model.xml content
    pub fn from_xml(xml: &str) -> Result<Self, String> {
        let doc =
            roxmltree::Document::parse(xml).map_err(|e| format!("Failed to parse XML: {}", e))?;

        let root = doc.root_element();
        let model_node = root
            .children()
            .find(|n| n.has_tag_name("Model"))
            .ok_or("No Model element found")?;

        let mut elements = Vec::new();
        let mut element_index = HashMap::new();

        for (idx, node) in model_node
            .children()
            .filter(|n| n.has_tag_name("Element"))
            .enumerate()
        {
            let element = parse_element(&node);
            if let Some(ref name) = element.name {
                element_index.insert((element.element_type.clone(), name.clone()), idx);
            }
            elements.push(element);
        }

        Ok(Self {
            elements,
            element_index,
        })
    }

    /// Get all elements of a specific type
    pub fn elements_of_type(&self, element_type: &str) -> Vec<&ModelElement> {
        self.elements
            .iter()
            .filter(|e| e.element_type == element_type)
            .collect()
    }

    /// Get element by type and name
    pub fn get_element(&self, element_type: &str, name: &str) -> Option<&ModelElement> {
        self.element_index
            .get(&(element_type.to_string(), name.to_string()))
            .map(|&idx| &self.elements[idx])
    }

    /// Get all unique element types
    pub fn element_types(&self) -> BTreeSet<String> {
        self.elements
            .iter()
            .map(|e| e.element_type.clone())
            .collect()
    }

    /// Get all named elements as (type, name) pairs
    pub fn named_elements(&self) -> BTreeSet<(String, String)> {
        self.elements
            .iter()
            .filter_map(|e| e.name.as_ref().map(|n| (e.element_type.clone(), n.clone())))
            .collect()
    }
}

// =============================================================================
// XML Parsing Helpers
// =============================================================================

fn parse_element(node: &roxmltree::Node) -> ModelElement {
    let element_type = node.attribute("Type").unwrap_or("Unknown").to_string();
    let name = node.attribute("Name").map(|s| s.to_string());

    let mut properties = BTreeMap::new();
    let mut children = Vec::new();
    let mut relationships = Vec::new();

    for child in node.children() {
        if child.has_tag_name("Property") {
            if let (Some(prop_name), Some(prop_value)) =
                (child.attribute("Name"), child.attribute("Value"))
            {
                properties.insert(prop_name.to_string(), prop_value.to_string());
            }
        } else if child.has_tag_name("Relationship") {
            let relationship = parse_relationship(&child);

            // For backward compatibility, also add nested elements to children
            for entry in &relationship.entries {
                children.push(entry.clone());
            }

            relationships.push(relationship);
        }
    }

    ModelElement {
        element_type,
        name,
        properties,
        children,
        relationships,
    }
}

/// Parse a Relationship element and its contents
fn parse_relationship(node: &roxmltree::Node) -> Relationship {
    let name = node.attribute("Name").unwrap_or("Unknown").to_string();
    let mut references = Vec::new();
    let mut entries = Vec::new();

    for entry in node.children().filter(|n| n.has_tag_name("Entry")) {
        for entry_child in entry.children() {
            if entry_child.has_tag_name("References") {
                // Capture reference with optional ExternalSource
                if let Some(ref_name) = entry_child.attribute("Name") {
                    references.push(ReferenceEntry {
                        name: ref_name.to_string(),
                        external_source: entry_child
                            .attribute("ExternalSource")
                            .map(|s| s.to_string()),
                    });
                }
            } else if entry_child.has_tag_name("Element") {
                // Recursively parse nested elements
                entries.push(parse_element(&entry_child));
            }
        }
    }

    Relationship {
        name,
        references,
        entries,
    }
}

/// Extract model.xml from a dacpac
pub fn extract_model_xml(dacpac_path: &Path) -> Result<String, String> {
    let file = fs::File::open(dacpac_path).map_err(|e| format!("Failed to open dacpac: {}", e))?;

    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

        if file.name() == "model.xml" {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| format!("Failed to read model.xml: {}", e))?;
            return Ok(content);
        }
    }

    Err("model.xml not found in dacpac".to_string())
}

// =============================================================================
// Error Types
// =============================================================================

/// Layer 1: Element inventory errors
#[derive(Debug)]
pub enum Layer1Error {
    MissingInRust {
        element_type: String,
        name: String,
    },
    ExtraInRust {
        element_type: String,
        name: String,
    },
    CountMismatch {
        element_type: String,
        rust_count: usize,
        dotnet_count: usize,
    },
}

impl fmt::Display for Layer1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Layer1Error::MissingInRust { element_type, name } => {
                write!(f, "MISSING in Rust: {} {}", element_type, name)
            }
            Layer1Error::ExtraInRust { element_type, name } => {
                write!(f, "EXTRA in Rust: {} {}", element_type, name)
            }
            Layer1Error::CountMismatch {
                element_type,
                rust_count,
                dotnet_count,
            } => {
                write!(
                    f,
                    "COUNT MISMATCH: {} (Rust: {}, DotNet: {})",
                    element_type, rust_count, dotnet_count
                )
            }
        }
    }
}

/// Layer 2: Property comparison errors
#[derive(Debug)]
pub struct Layer2Error {
    pub element_type: String,
    pub element_name: String,
    pub property_name: String,
    pub rust_value: Option<String>,
    pub dotnet_value: Option<String>,
}

impl fmt::Display for Layer2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PROPERTY MISMATCH: {}.{} - {} (Rust: {:?}, DotNet: {:?})",
            self.element_type,
            self.element_name,
            self.property_name,
            self.rust_value,
            self.dotnet_value
        )
    }
}

/// Relationship comparison errors
#[derive(Debug)]
pub enum RelationshipError {
    /// Relationship exists in DotNet but not in Rust
    MissingRelationship {
        element_type: String,
        element_name: String,
        relationship_name: String,
    },
    /// Relationship exists in Rust but not in DotNet
    ExtraRelationship {
        element_type: String,
        element_name: String,
        relationship_name: String,
    },
    /// Reference count differs between implementations
    ReferenceCountMismatch {
        element_type: String,
        element_name: String,
        relationship_name: String,
        rust_count: usize,
        dotnet_count: usize,
    },
    /// References differ between implementations
    ReferenceMismatch {
        element_type: String,
        element_name: String,
        relationship_name: String,
        rust_refs: Vec<String>,
        dotnet_refs: Vec<String>,
    },
    /// Nested element count differs
    EntryCountMismatch {
        element_type: String,
        element_name: String,
        relationship_name: String,
        rust_count: usize,
        dotnet_count: usize,
    },
}

impl fmt::Display for RelationshipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelationshipError::MissingRelationship {
                element_type,
                element_name,
                relationship_name,
            } => {
                write!(
                    f,
                    "MISSING RELATIONSHIP: {}.{} - {} (not in Rust)",
                    element_type, element_name, relationship_name
                )
            }
            RelationshipError::ExtraRelationship {
                element_type,
                element_name,
                relationship_name,
            } => {
                write!(
                    f,
                    "EXTRA RELATIONSHIP: {}.{} - {} (not in DotNet)",
                    element_type, element_name, relationship_name
                )
            }
            RelationshipError::ReferenceCountMismatch {
                element_type,
                element_name,
                relationship_name,
                rust_count,
                dotnet_count,
            } => {
                write!(
                    f,
                    "REFERENCE COUNT MISMATCH: {}.{}.{} (Rust: {}, DotNet: {})",
                    element_type, element_name, relationship_name, rust_count, dotnet_count
                )
            }
            RelationshipError::ReferenceMismatch {
                element_type,
                element_name,
                relationship_name,
                rust_refs,
                dotnet_refs,
            } => {
                write!(
                    f,
                    "REFERENCE MISMATCH: {}.{}.{} (Rust: {:?}, DotNet: {:?})",
                    element_type, element_name, relationship_name, rust_refs, dotnet_refs
                )
            }
            RelationshipError::EntryCountMismatch {
                element_type,
                element_name,
                relationship_name,
                rust_count,
                dotnet_count,
            } => {
                write!(
                    f,
                    "ENTRY COUNT MISMATCH: {}.{}.{} (Rust: {}, DotNet: {})",
                    element_type, element_name, relationship_name, rust_count, dotnet_count
                )
            }
        }
    }
}

/// Layer 4: Element ordering errors (Phase 4)
///
/// DotNet DacFx generates elements in a specific, deterministic order within model.xml.
/// This error type captures ordering mismatches between Rust and DotNet output.
/// Element ordering may affect certain DAC tools and operations, so matching
/// the exact order is important for true 1-1 parity.
#[derive(Debug)]
pub enum Layer4Error {
    /// Element appears at different position in output
    /// DotNet has specific ordering rules - typically schemas first, then tables,
    /// views, procedures, etc. Within a type, elements may be ordered alphabetically
    /// or by dependency.
    ElementOrderMismatch {
        /// The type of element (e.g., "SqlTable", "SqlView")
        element_type: String,
        /// The element name (e.g., "[dbo].[Products]")
        element_name: String,
        /// Position in Rust output (0-indexed)
        rust_position: usize,
        /// Position in DotNet output (0-indexed)
        dotnet_position: usize,
    },
    /// Element types appear in different order
    /// For example, DotNet might output all schemas, then all tables, then all views,
    /// while Rust might interleave them.
    TypeOrderMismatch {
        /// The element type that's out of order
        element_type: String,
        /// First position this type appears in Rust output
        rust_first_position: usize,
        /// First position this type appears in DotNet output
        dotnet_first_position: usize,
    },
}

impl fmt::Display for Layer4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Layer4Error::ElementOrderMismatch {
                element_type,
                element_name,
                rust_position,
                dotnet_position,
            } => {
                write!(
                    f,
                    "ELEMENT ORDER MISMATCH: {} {} (Rust pos: {}, DotNet pos: {})",
                    element_type, element_name, rust_position, dotnet_position
                )
            }
            Layer4Error::TypeOrderMismatch {
                element_type,
                rust_first_position,
                dotnet_first_position,
            } => {
                write!(
                    f,
                    "TYPE ORDER MISMATCH: {} first appears at (Rust pos: {}, DotNet pos: {})",
                    element_type, rust_first_position, dotnet_first_position
                )
            }
        }
    }
}

/// Phase 5: Metadata file comparison errors
///
/// Beyond model.xml, dacpacs contain metadata files that should match between
/// Rust and DotNet implementations for true 1-1 parity.
#[derive(Debug)]
pub enum MetadataFileError {
    /// [Content_Types].xml MIME type definition mismatch
    ContentTypeMismatch {
        /// File extension (e.g., "xml", "sql")
        extension: String,
        /// Content type in Rust output
        rust_content_type: Option<String>,
        /// Content type in DotNet output
        dotnet_content_type: Option<String>,
    },
    /// [Content_Types].xml has different number of type definitions
    ContentTypeCountMismatch {
        rust_count: usize,
        dotnet_count: usize,
    },
    /// File exists in one dacpac but not the other
    FileMissing {
        /// Name of the file (e.g., "[Content_Types].xml", "DacMetadata.xml")
        file_name: String,
        /// True if missing in Rust dacpac, false if missing in DotNet dacpac
        missing_in_rust: bool,
    },
    /// DacMetadata.xml field value mismatch
    DacMetadataMismatch {
        /// Field name (e.g., "Name", "Version", "Description")
        field_name: String,
        /// Value in Rust output
        rust_value: Option<String>,
        /// Value in DotNet output
        dotnet_value: Option<String>,
    },
    /// Origin.xml field value mismatch
    OriginXmlMismatch {
        /// Field name (e.g., "ProductName", "ProductVersion", "ProductSchema")
        field_name: String,
        /// Value in Rust output
        rust_value: Option<String>,
        /// Value in DotNet output
        dotnet_value: Option<String>,
    },
    /// Pre/post-deploy script content mismatch (after whitespace normalization)
    DeployScriptMismatch {
        /// Script name (e.g., "predeploy.sql", "postdeploy.sql")
        script_name: String,
        /// Content in Rust dacpac (normalized)
        rust_content: Option<String>,
        /// Content in DotNet dacpac (normalized)
        dotnet_content: Option<String>,
    },
    /// Pre/post-deploy script exists in one dacpac but not the other
    DeployScriptMissing {
        /// Script name (e.g., "predeploy.sql", "postdeploy.sql")
        script_name: String,
        /// True if missing in Rust dacpac, false if missing in DotNet dacpac
        missing_in_rust: bool,
    },
}

impl fmt::Display for MetadataFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataFileError::ContentTypeMismatch {
                extension,
                rust_content_type,
                dotnet_content_type,
            } => {
                write!(
                    f,
                    "CONTENT TYPE MISMATCH: .{} extension (Rust: {:?}, DotNet: {:?})",
                    extension, rust_content_type, dotnet_content_type
                )
            }
            MetadataFileError::ContentTypeCountMismatch {
                rust_count,
                dotnet_count,
            } => {
                write!(
                    f,
                    "CONTENT TYPE COUNT MISMATCH: Rust has {} types, DotNet has {} types",
                    rust_count, dotnet_count
                )
            }
            MetadataFileError::FileMissing {
                file_name,
                missing_in_rust,
            } => {
                if *missing_in_rust {
                    write!(f, "FILE MISSING IN RUST: {}", file_name)
                } else {
                    write!(f, "FILE MISSING IN DOTNET: {}", file_name)
                }
            }
            MetadataFileError::DacMetadataMismatch {
                field_name,
                rust_value,
                dotnet_value,
            } => {
                write!(
                    f,
                    "DACMETADATA MISMATCH: {} (Rust: {:?}, DotNet: {:?})",
                    field_name, rust_value, dotnet_value
                )
            }
            MetadataFileError::OriginXmlMismatch {
                field_name,
                rust_value,
                dotnet_value,
            } => {
                write!(
                    f,
                    "ORIGIN.XML MISMATCH: {} (Rust: {:?}, DotNet: {:?})",
                    field_name, rust_value, dotnet_value
                )
            }
            MetadataFileError::DeployScriptMismatch {
                script_name,
                rust_content,
                dotnet_content,
            } => {
                write!(
                    f,
                    "DEPLOY SCRIPT MISMATCH: {} (Rust len: {}, DotNet len: {})",
                    script_name,
                    rust_content.as_ref().map_or(0, |s| s.len()),
                    dotnet_content.as_ref().map_or(0, |s| s.len())
                )
            }
            MetadataFileError::DeployScriptMissing {
                script_name,
                missing_in_rust,
            } => {
                if *missing_in_rust {
                    write!(
                        f,
                        "DEPLOY SCRIPT MISSING IN RUST: {} (present in DotNet)",
                        script_name
                    )
                } else {
                    write!(
                        f,
                        "DEPLOY SCRIPT MISSING IN DOTNET: {} (present in Rust)",
                        script_name
                    )
                }
            }
        }
    }
}

/// Phase 7 error types for canonical XML comparison
///
/// These errors indicate differences between canonicalized XML representations,
/// providing the final validation layer for true byte-level matching.
#[derive(Debug)]
pub enum CanonicalXmlError {
    /// Canonicalized XML content differs
    ContentMismatch {
        /// First differing line number (1-indexed)
        line_number: usize,
        /// Content of the line in Rust output
        rust_line: String,
        /// Content of the line in DotNet output
        dotnet_line: String,
    },
    /// Line count differs between outputs
    LineCountMismatch {
        rust_lines: usize,
        dotnet_lines: usize,
    },
    /// SHA256 checksums differ
    ChecksumMismatch {
        rust_checksum: String,
        dotnet_checksum: String,
    },
}

impl fmt::Display for CanonicalXmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanonicalXmlError::ContentMismatch {
                line_number,
                rust_line,
                dotnet_line,
            } => {
                write!(
                    f,
                    "CANONICAL XML MISMATCH at line {}: Rust='{}', DotNet='{}'",
                    line_number, rust_line, dotnet_line
                )
            }
            CanonicalXmlError::LineCountMismatch {
                rust_lines,
                dotnet_lines,
            } => {
                write!(
                    f,
                    "CANONICAL XML LINE COUNT MISMATCH: Rust has {} lines, DotNet has {} lines",
                    rust_lines, dotnet_lines
                )
            }
            CanonicalXmlError::ChecksumMismatch {
                rust_checksum,
                dotnet_checksum,
            } => {
                write!(
                    f,
                    "CANONICAL XML CHECKSUM MISMATCH: Rust={}, DotNet={}",
                    rust_checksum, dotnet_checksum
                )
            }
        }
    }
}

// =============================================================================
// Options and Result Types
// =============================================================================

/// Options for controlling comparison behavior
#[derive(Debug, Clone, Default)]
pub struct ComparisonOptions {
    /// Include Layer 3 (SqlPackage DeployReport) comparison
    pub include_layer3: bool,
    /// Compare ALL properties instead of just key properties
    pub strict_properties: bool,
    /// Validate all relationships between elements
    pub check_relationships: bool,
    /// Validate element ordering matches DotNet output (Phase 4)
    pub check_element_order: bool,
    /// Compare metadata files ([Content_Types].xml, DacMetadata.xml, etc.) (Phase 5)
    pub check_metadata_files: bool,
    /// Compare pre/post-deploy scripts (predeploy.sql, postdeploy.sql) (Phase 5.4)
    pub check_deploy_scripts: bool,
}

/// Layer 3: SqlPackage DeployReport result
#[derive(Debug)]
pub struct Layer3Result {
    pub has_differences: bool,
    pub deploy_script: String,
    pub error: Option<String>,
}

/// Result of comparing two dacpacs
#[derive(Debug, Default)]
pub struct ComparisonResult {
    pub layer1_errors: Vec<Layer1Error>,
    pub layer2_errors: Vec<Layer2Error>,
    pub relationship_errors: Vec<RelationshipError>,
    pub layer4_errors: Vec<Layer4Error>,
    pub metadata_errors: Vec<MetadataFileError>,
    pub layer3_result: Option<Layer3Result>,
}

impl ComparisonResult {
    pub fn is_success(&self) -> bool {
        self.layer1_errors.is_empty()
            && self.layer2_errors.is_empty()
            && self.relationship_errors.is_empty()
            && self.layer4_errors.is_empty()
            && self.metadata_errors.is_empty()
            && self
                .layer3_result
                .as_ref()
                .map_or(true, |r| !r.has_differences)
    }

    pub fn print_report(&self) {
        println!("\n{}", "=".repeat(60));
        println!("DACPAC COMPARISON REPORT");
        println!("{}\n", "=".repeat(60));

        // Layer 1
        println!("Layer 1: Element Inventory");
        println!("{}", "-".repeat(40));
        if self.layer1_errors.is_empty() {
            println!("  All elements match.");
        } else {
            for err in &self.layer1_errors {
                println!("  {}", err);
            }
        }
        println!();

        // Layer 2
        println!("Layer 2: Property Comparison");
        println!("{}", "-".repeat(40));
        if self.layer2_errors.is_empty() {
            println!("  All properties match.");
        } else {
            for err in &self.layer2_errors {
                println!("  {}", err);
            }
        }
        println!();

        // Relationship Comparison
        if !self.relationship_errors.is_empty() {
            println!("Relationship Comparison");
            println!("{}", "-".repeat(40));
            for err in &self.relationship_errors {
                println!("  {}", err);
            }
            println!();
        }

        // Layer 4: Element Ordering
        if !self.layer4_errors.is_empty() {
            println!("Layer 4: Element Ordering");
            println!("{}", "-".repeat(40));
            for err in &self.layer4_errors {
                println!("  {}", err);
            }
            println!();
        }

        // Phase 5: Metadata Files & Deploy Scripts Comparison
        if !self.metadata_errors.is_empty() {
            println!("Phase 5: Metadata Files & Deploy Scripts");
            println!("{}", "-".repeat(40));
            for err in &self.metadata_errors {
                println!("  {}", err);
            }
            println!();
        }

        // Layer 3
        if let Some(ref l3) = self.layer3_result {
            println!("Layer 3: SqlPackage DeployReport");
            println!("{}", "-".repeat(40));
            if let Some(ref err) = l3.error {
                println!("  Error: {}", err);
            } else if l3.has_differences {
                println!("  Schema differences detected!");
                println!("  Deploy script:\n{}", l3.deploy_script);
            } else {
                println!("  No schema differences (dacpacs are equivalent).");
            }
        }

        println!();
        println!(
            "Result: {}",
            if self.is_success() { "PASS" } else { "FAIL" }
        );
    }
}

// =============================================================================
// Metadata File Types
// =============================================================================

/// Parsed [Content_Types].xml structure
#[derive(Debug, Default)]
pub struct ContentTypesXml {
    /// Map of file extension to MIME content type
    /// e.g., "xml" -> "text/xml", "sql" -> "text/plain"
    pub types: HashMap<String, String>,
}

/// Parsed DacMetadata.xml structure
///
/// DacMetadata.xml contains package metadata for the dacpac:
/// - Name: The database/project name
/// - Version: Package version (e.g., "1.0.0.0")
/// - Description: Optional package description (omitted when empty)
///
/// Root element is `DacType` (per MS XSD schema).
#[derive(Debug, Default)]
pub struct DacMetadataXml {
    /// Package name (database name)
    pub name: Option<String>,
    /// Package version (e.g., "1.0.0.0")
    pub version: Option<String>,
    /// Optional description (typically empty/omitted)
    pub description: Option<String>,
}

/// Parsed Origin.xml structure
///
/// Origin.xml contains build/package origin metadata:
/// - PackageProperties: Version (package format), ContainsExportedData, StreamVersions
/// - Operation: Identity, Start/End timestamps, ProductName, ProductVersion, ProductSchema
/// - Checksums: SHA256 checksum of model.xml
///
/// For comparison purposes, we ignore timestamps (Start/End) as they differ between builds.
/// We also ignore checksums since model.xml content may legitimately differ.
/// We focus on ProductName, ProductVersion, ProductSchema for parity testing.
///
/// Root element is `DacOrigin` (per MS XSD schema).
#[derive(Debug, Default)]
pub struct OriginXml {
    /// Package format version (e.g., "3.1.0.0")
    pub package_version: Option<String>,
    /// ContainsExportedData flag (typically "false")
    pub contains_exported_data: Option<String>,
    /// Data stream version (e.g., "2.0.0.0")
    pub data_stream_version: Option<String>,
    /// DeploymentContributors stream version (e.g., "1.0.0.0")
    pub deployment_contributors_version: Option<String>,
    /// Product name (e.g., "Microsoft.Data.Tools.Schema.Sql, Version=...")
    pub product_name: Option<String>,
    /// Product version (e.g., "17.0" or "0.1.0")
    pub product_version: Option<String>,
    /// Product schema URL
    pub product_schema: Option<String>,
}

// =============================================================================
// Phase 8.2: Parity Metrics for CI Progress Tracking
// =============================================================================

/// Per-fixture metrics for tracking parity test results.
///
/// This struct captures the outcome of a parity test for a single fixture,
/// including error counts per layer and overall status.
#[derive(Debug, Clone)]
pub struct FixtureMetrics {
    /// Fixture name (directory name in tests/fixtures/)
    pub name: String,
    /// Overall status: "PASS", "PARTIAL", "FAIL", or "ERROR"
    pub status: String,
    /// Number of Layer 1 errors (element inventory)
    pub layer1_errors: usize,
    /// Number of Layer 2 errors (property comparison)
    pub layer2_errors: usize,
    /// Number of relationship errors
    pub relationship_errors: usize,
    /// Number of Layer 4 errors (element ordering)
    pub layer4_errors: usize,
    /// Number of metadata errors
    pub metadata_errors: usize,
    /// Error message if test setup failed
    pub error_message: Option<String>,
}

/// Aggregate metrics for all parity tests.
///
/// This struct collects metrics from multiple fixture tests to provide
/// an overview of parity status. Designed for CI reporting and progress
/// tracking over time.
#[derive(Debug, Clone)]
pub struct ParityMetrics {
    /// ISO 8601 timestamp when metrics were collected
    pub timestamp: String,
    /// Git commit hash (if available)
    pub commit: Option<String>,
    /// Total number of fixtures tested
    pub total_fixtures: usize,
    /// Number of fixtures passing Layer 1 (inventory)
    pub layer1_pass: usize,
    /// Number of fixtures passing Layer 2 (properties)
    pub layer2_pass: usize,
    /// Number of fixtures passing relationship comparison
    pub relationship_pass: usize,
    /// Number of fixtures passing Layer 4 (element ordering)
    pub layer4_pass: usize,
    /// Number of fixtures passing metadata comparison
    pub metadata_pass: usize,
    /// Number of fixtures with full parity (all layers pass)
    pub full_parity: usize,
    /// Number of fixtures that failed to build/compare (errors)
    pub error_count: usize,
    /// Per-fixture detailed results
    pub fixtures: Vec<FixtureMetrics>,
}

impl ParityMetrics {
    /// Create a new empty ParityMetrics with current timestamp.
    pub fn new() -> Self {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let commit = get_git_commit_hash();

        Self {
            timestamp,
            commit,
            total_fixtures: 0,
            layer1_pass: 0,
            layer2_pass: 0,
            relationship_pass: 0,
            layer4_pass: 0,
            metadata_pass: 0,
            full_parity: 0,
            error_count: 0,
            fixtures: Vec::new(),
        }
    }

    /// Add a successful comparison result for a fixture.
    pub fn add_result(&mut self, fixture_name: &str, result: &ComparisonResult) {
        self.total_fixtures += 1;

        let l1_ok = result.layer1_errors.is_empty();
        let l2_ok = result.layer2_errors.is_empty();
        let rel_ok = result.relationship_errors.is_empty();
        let l4_ok = result.layer4_errors.is_empty();
        let meta_ok = result.metadata_errors.is_empty();

        if l1_ok {
            self.layer1_pass += 1;
        }
        if l2_ok {
            self.layer2_pass += 1;
        }
        if rel_ok {
            self.relationship_pass += 1;
        }
        if l4_ok {
            self.layer4_pass += 1;
        }
        if meta_ok {
            self.metadata_pass += 1;
        }

        let all_pass = l1_ok && l2_ok && rel_ok && l4_ok && meta_ok;
        if all_pass {
            self.full_parity += 1;
        }

        let status = if all_pass {
            "PASS".to_string()
        } else if l1_ok {
            "PARTIAL".to_string()
        } else {
            "FAIL".to_string()
        };

        self.fixtures.push(FixtureMetrics {
            name: fixture_name.to_string(),
            status,
            layer1_errors: result.layer1_errors.len(),
            layer2_errors: result.layer2_errors.len(),
            relationship_errors: result.relationship_errors.len(),
            layer4_errors: result.layer4_errors.len(),
            metadata_errors: result.metadata_errors.len(),
            error_message: None,
        });
    }

    /// Add an error result for a fixture that failed to build/compare.
    pub fn add_error(&mut self, fixture_name: &str, error: &str) {
        self.total_fixtures += 1;
        self.error_count += 1;

        self.fixtures.push(FixtureMetrics {
            name: fixture_name.to_string(),
            status: "ERROR".to_string(),
            layer1_errors: 0,
            layer2_errors: 0,
            relationship_errors: 0,
            layer4_errors: 0,
            metadata_errors: 0,
            error_message: Some(error.to_string()),
        });
    }

    /// Calculate pass rate as a percentage.
    pub fn pass_rate(&self, pass_count: usize) -> f64 {
        if self.total_fixtures == 0 {
            0.0
        } else {
            100.0 * pass_count as f64 / self.total_fixtures as f64
        }
    }

    /// Serialize metrics to JSON format.
    ///
    /// The JSON format is designed for CI reporting and can be parsed
    /// by tools that track metrics over time.
    pub fn to_json(&self) -> String {
        let mut json = String::new();
        json.push_str("{\n");
        json.push_str(&format!("  \"timestamp\": \"{}\",\n", self.timestamp));

        if let Some(ref commit) = self.commit {
            json.push_str(&format!("  \"commit\": \"{}\",\n", commit));
        } else {
            json.push_str("  \"commit\": null,\n");
        }

        json.push_str(&format!("  \"total_fixtures\": {},\n", self.total_fixtures));
        json.push_str(&format!("  \"layer1_pass\": {},\n", self.layer1_pass));
        json.push_str(&format!("  \"layer2_pass\": {},\n", self.layer2_pass));
        json.push_str(&format!(
            "  \"relationship_pass\": {},\n",
            self.relationship_pass
        ));
        json.push_str(&format!("  \"layer4_pass\": {},\n", self.layer4_pass));
        json.push_str(&format!("  \"metadata_pass\": {},\n", self.metadata_pass));
        json.push_str(&format!("  \"full_parity\": {},\n", self.full_parity));
        json.push_str(&format!("  \"error_count\": {},\n", self.error_count));

        // Add pass rates
        json.push_str("  \"pass_rates\": {\n");
        json.push_str(&format!(
            "    \"layer1\": {:.1},\n",
            self.pass_rate(self.layer1_pass)
        ));
        json.push_str(&format!(
            "    \"layer2\": {:.1},\n",
            self.pass_rate(self.layer2_pass)
        ));
        json.push_str(&format!(
            "    \"relationships\": {:.1},\n",
            self.pass_rate(self.relationship_pass)
        ));
        json.push_str(&format!(
            "    \"layer4\": {:.1},\n",
            self.pass_rate(self.layer4_pass)
        ));
        json.push_str(&format!(
            "    \"metadata\": {:.1},\n",
            self.pass_rate(self.metadata_pass)
        ));
        json.push_str(&format!(
            "    \"full_parity\": {:.1}\n",
            self.pass_rate(self.full_parity)
        ));
        json.push_str("  },\n");

        // Add fixtures array
        json.push_str("  \"fixtures\": [\n");
        for (i, fixture) in self.fixtures.iter().enumerate() {
            json.push_str("    {\n");
            json.push_str(&format!("      \"name\": \"{}\",\n", fixture.name));
            json.push_str(&format!("      \"status\": \"{}\",\n", fixture.status));
            json.push_str(&format!(
                "      \"layer1_errors\": {},\n",
                fixture.layer1_errors
            ));
            json.push_str(&format!(
                "      \"layer2_errors\": {},\n",
                fixture.layer2_errors
            ));
            json.push_str(&format!(
                "      \"relationship_errors\": {},\n",
                fixture.relationship_errors
            ));
            json.push_str(&format!(
                "      \"layer4_errors\": {},\n",
                fixture.layer4_errors
            ));
            json.push_str(&format!(
                "      \"metadata_errors\": {},\n",
                fixture.metadata_errors
            ));
            if let Some(ref err) = fixture.error_message {
                // Escape JSON special characters in error message
                let escaped = err
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n");
                json.push_str(&format!("      \"error_message\": \"{}\"\n", escaped));
            } else {
                json.push_str("      \"error_message\": null\n");
            }
            if i < self.fixtures.len() - 1 {
                json.push_str("    },\n");
            } else {
                json.push_str("    }\n");
            }
        }
        json.push_str("  ]\n");
        json.push_str("}\n");

        json
    }

    /// Print a summary report to stdout in a format suitable for CI logs.
    pub fn print_summary(&self) {
        println!("\n{}", "=".repeat(60));
        println!("PARITY METRICS SUMMARY");
        println!("{}\n", "=".repeat(60));

        if let Some(ref commit) = self.commit {
            println!("Commit: {}", commit);
        }
        println!("Timestamp: {}", self.timestamp);
        println!();

        println!("Layer Pass Rates:");
        println!(
            "  Layer 1 (inventory):    {:3}/{} ({:.1}%)",
            self.layer1_pass,
            self.total_fixtures,
            self.pass_rate(self.layer1_pass)
        );
        println!(
            "  Layer 2 (properties):   {:3}/{} ({:.1}%)",
            self.layer2_pass,
            self.total_fixtures,
            self.pass_rate(self.layer2_pass)
        );
        println!(
            "  Relationships:          {:3}/{} ({:.1}%)",
            self.relationship_pass,
            self.total_fixtures,
            self.pass_rate(self.relationship_pass)
        );
        println!(
            "  Layer 4 (ordering):     {:3}/{} ({:.1}%)",
            self.layer4_pass,
            self.total_fixtures,
            self.pass_rate(self.layer4_pass)
        );
        println!(
            "  Metadata files:         {:3}/{} ({:.1}%)",
            self.metadata_pass,
            self.total_fixtures,
            self.pass_rate(self.metadata_pass)
        );
        println!(
            "  Full parity:            {:3}/{} ({:.1}%)",
            self.full_parity,
            self.total_fixtures,
            self.pass_rate(self.full_parity)
        );

        if self.error_count > 0 {
            println!("  Errors:                 {:3}", self.error_count);
        }

        println!();

        // Print per-fixture summary
        println!("Per-Fixture Results:");
        println!("{:-<60}", "");
        for fixture in &self.fixtures {
            let status_symbol = match fixture.status.as_str() {
                "PASS" => "✓",
                "PARTIAL" => "~",
                "FAIL" => "✗",
                "ERROR" => "!",
                _ => "?",
            };

            if fixture.status == "ERROR" {
                println!("{} {:40} ERROR", status_symbol, fixture.name);
            } else {
                println!(
                    "{} {:40} L1:{:<2} L2:{:<2} Rel:{:<2} L4:{:<2} Meta:{:<2}",
                    status_symbol,
                    fixture.name,
                    fixture.layer1_errors,
                    fixture.layer2_errors,
                    fixture.relationship_errors,
                    fixture.layer4_errors,
                    fixture.metadata_errors
                );
            }
        }
        println!();
    }
}

impl Default for ParityMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Phase 8.3: Detailed Parity Report Generation
// =============================================================================

/// Detailed per-fixture result with actual error messages for report generation.
///
/// Unlike `FixtureMetrics` which only tracks error counts, this struct captures
/// the actual error messages for generating detailed Markdown reports.
#[derive(Debug, Clone)]
pub struct DetailedFixtureResult {
    /// Fixture name (directory name in tests/fixtures/)
    pub name: String,
    /// Overall status: "PASS", "PARTIAL", "FAIL", or "ERROR"
    pub status: String,
    /// Layer 1 error messages (element inventory)
    pub layer1_errors: Vec<String>,
    /// Layer 2 error messages (property comparison)
    pub layer2_errors: Vec<String>,
    /// Relationship error messages
    pub relationship_errors: Vec<String>,
    /// Layer 4 error messages (element ordering)
    pub layer4_errors: Vec<String>,
    /// Metadata error messages
    pub metadata_errors: Vec<String>,
    /// Setup error message if test failed to run
    pub setup_error: Option<String>,
}

impl DetailedFixtureResult {
    /// Create from a successful ComparisonResult.
    pub fn from_result(name: &str, result: &ComparisonResult) -> Self {
        let l1_ok = result.layer1_errors.is_empty();
        let l2_ok = result.layer2_errors.is_empty();
        let rel_ok = result.relationship_errors.is_empty();
        let l4_ok = result.layer4_errors.is_empty();
        let meta_ok = result.metadata_errors.is_empty();

        let all_pass = l1_ok && l2_ok && rel_ok && l4_ok && meta_ok;

        let status = if all_pass {
            "PASS".to_string()
        } else if l1_ok {
            "PARTIAL".to_string()
        } else {
            "FAIL".to_string()
        };

        Self {
            name: name.to_string(),
            status,
            layer1_errors: result.layer1_errors.iter().map(|e| e.to_string()).collect(),
            layer2_errors: result.layer2_errors.iter().map(|e| e.to_string()).collect(),
            relationship_errors: result
                .relationship_errors
                .iter()
                .map(|e| e.to_string())
                .collect(),
            layer4_errors: result.layer4_errors.iter().map(|e| e.to_string()).collect(),
            metadata_errors: result
                .metadata_errors
                .iter()
                .map(|e| e.to_string())
                .collect(),
            setup_error: None,
        }
    }

    /// Create from a setup/build error.
    pub fn from_error(name: &str, error: &str) -> Self {
        Self {
            name: name.to_string(),
            status: "ERROR".to_string(),
            layer1_errors: Vec::new(),
            layer2_errors: Vec::new(),
            relationship_errors: Vec::new(),
            layer4_errors: Vec::new(),
            metadata_errors: Vec::new(),
            setup_error: Some(error.to_string()),
        }
    }

    /// Check if this fixture has full parity.
    pub fn is_pass(&self) -> bool {
        self.status == "PASS"
    }

    /// Get total number of errors across all layers.
    pub fn total_errors(&self) -> usize {
        self.layer1_errors.len()
            + self.layer2_errors.len()
            + self.relationship_errors.len()
            + self.layer4_errors.len()
            + self.metadata_errors.len()
    }
}

/// Detailed parity report with full error messages for all fixtures.
///
/// This struct extends `ParityMetrics` with actual error messages, enabling
/// generation of detailed Markdown reports that show exactly what differs
/// between Rust and DotNet output.
#[derive(Debug, Clone)]
pub struct ParityReport {
    /// ISO 8601 timestamp when report was generated
    pub timestamp: String,
    /// Git commit hash (if available)
    pub commit: Option<String>,
    /// Per-fixture detailed results with error messages
    pub fixtures: Vec<DetailedFixtureResult>,
}

impl ParityReport {
    /// Create a new empty ParityReport with current timestamp.
    pub fn new() -> Self {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let commit = get_git_commit_hash();

        Self {
            timestamp,
            commit,
            fixtures: Vec::new(),
        }
    }

    /// Add a successful comparison result for a fixture.
    pub fn add_result(&mut self, fixture_name: &str, result: &ComparisonResult) {
        self.fixtures
            .push(DetailedFixtureResult::from_result(fixture_name, result));
    }

    /// Add an error result for a fixture that failed to build/compare.
    pub fn add_error(&mut self, fixture_name: &str, error: &str) {
        self.fixtures
            .push(DetailedFixtureResult::from_error(fixture_name, error));
    }

    /// Get total number of fixtures.
    pub fn total_fixtures(&self) -> usize {
        self.fixtures.len()
    }

    /// Get number of fixtures with full parity.
    pub fn full_parity_count(&self) -> usize {
        self.fixtures.iter().filter(|f| f.is_pass()).count()
    }

    /// Get number of fixtures passing Layer 1 (element inventory).
    pub fn layer1_pass_count(&self) -> usize {
        self.fixtures
            .iter()
            .filter(|f| f.layer1_errors.is_empty() && f.setup_error.is_none())
            .count()
    }

    /// Get number of fixtures passing Layer 2 (properties).
    pub fn layer2_pass_count(&self) -> usize {
        self.fixtures
            .iter()
            .filter(|f| f.layer2_errors.is_empty() && f.setup_error.is_none())
            .count()
    }

    /// Get number of fixtures passing relationships.
    pub fn relationship_pass_count(&self) -> usize {
        self.fixtures
            .iter()
            .filter(|f| f.relationship_errors.is_empty() && f.setup_error.is_none())
            .count()
    }

    /// Get number of fixtures passing Layer 4 (ordering).
    pub fn layer4_pass_count(&self) -> usize {
        self.fixtures
            .iter()
            .filter(|f| f.layer4_errors.is_empty() && f.setup_error.is_none())
            .count()
    }

    /// Get number of fixtures passing metadata comparison.
    pub fn metadata_pass_count(&self) -> usize {
        self.fixtures
            .iter()
            .filter(|f| f.metadata_errors.is_empty() && f.setup_error.is_none())
            .count()
    }

    /// Get number of fixtures with setup errors.
    pub fn error_count(&self) -> usize {
        self.fixtures
            .iter()
            .filter(|f| f.setup_error.is_some())
            .count()
    }

    /// Calculate pass rate as a percentage.
    fn pass_rate(&self, pass_count: usize) -> f64 {
        if self.fixtures.is_empty() {
            0.0
        } else {
            100.0 * pass_count as f64 / self.fixtures.len() as f64
        }
    }

    /// Generate a Markdown report of all parity test results.
    ///
    /// The report includes:
    /// - Summary table with pass rates per layer
    /// - Per-fixture results table
    /// - Detailed error breakdown for fixtures with failures
    ///
    /// This format is suitable for:
    /// - Viewing in GitHub Actions artifacts
    /// - Including in pull request comments
    /// - Archiving for historical comparison
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        // Header
        md.push_str("# Dacpac Parity Test Report\n\n");

        // Metadata
        if let Some(ref commit) = self.commit {
            md.push_str(&format!("**Commit:** `{}`  \n", commit));
        }
        md.push_str(&format!("**Generated:** {}  \n\n", self.timestamp));

        // Summary Section
        md.push_str("## Summary\n\n");
        md.push_str("| Layer | Pass | Total | Rate |\n");
        md.push_str("|-------|------|-------|------|\n");

        let total = self.total_fixtures();
        md.push_str(&format!(
            "| Layer 1 (Element Inventory) | {} | {} | {:.1}% |\n",
            self.layer1_pass_count(),
            total,
            self.pass_rate(self.layer1_pass_count())
        ));
        md.push_str(&format!(
            "| Layer 2 (Properties) | {} | {} | {:.1}% |\n",
            self.layer2_pass_count(),
            total,
            self.pass_rate(self.layer2_pass_count())
        ));
        md.push_str(&format!(
            "| Relationships | {} | {} | {:.1}% |\n",
            self.relationship_pass_count(),
            total,
            self.pass_rate(self.relationship_pass_count())
        ));
        md.push_str(&format!(
            "| Layer 4 (Ordering) | {} | {} | {:.1}% |\n",
            self.layer4_pass_count(),
            total,
            self.pass_rate(self.layer4_pass_count())
        ));
        md.push_str(&format!(
            "| Metadata Files | {} | {} | {:.1}% |\n",
            self.metadata_pass_count(),
            total,
            self.pass_rate(self.metadata_pass_count())
        ));
        md.push_str(&format!(
            "| **Full Parity** | **{}** | **{}** | **{:.1}%** |\n",
            self.full_parity_count(),
            total,
            self.pass_rate(self.full_parity_count())
        ));

        if self.error_count() > 0 {
            md.push_str(&format!(
                "\n⚠️ **{} fixture(s) failed to build/compare**\n",
                self.error_count()
            ));
        }

        md.push('\n');

        // Per-Fixture Results Table
        md.push_str("## Per-Fixture Results\n\n");
        md.push_str("| Fixture | Status | L1 | L2 | Rel | L4 | Meta | Total |\n");
        md.push_str("|---------|--------|----|----|-----|----|----- |-------|\n");

        for fixture in &self.fixtures {
            let status_emoji = match fixture.status.as_str() {
                "PASS" => "✅",
                "PARTIAL" => "🟡",
                "FAIL" => "❌",
                "ERROR" => "⚠️",
                _ => "❓",
            };

            if fixture.setup_error.is_some() {
                md.push_str(&format!(
                    "| {} | {} ERROR | - | - | - | - | - | - |\n",
                    fixture.name, status_emoji
                ));
            } else {
                md.push_str(&format!(
                    "| {} | {} {} | {} | {} | {} | {} | {} | {} |\n",
                    fixture.name,
                    status_emoji,
                    fixture.status,
                    fixture.layer1_errors.len(),
                    fixture.layer2_errors.len(),
                    fixture.relationship_errors.len(),
                    fixture.layer4_errors.len(),
                    fixture.metadata_errors.len(),
                    fixture.total_errors()
                ));
            }
        }

        md.push('\n');

        // Detailed Errors Section (for non-passing fixtures)
        let fixtures_with_errors: Vec<_> = self.fixtures.iter().filter(|f| !f.is_pass()).collect();

        if !fixtures_with_errors.is_empty() {
            md.push_str("## Detailed Errors\n\n");

            for fixture in fixtures_with_errors {
                md.push_str(&format!("### {}\n\n", fixture.name));

                if let Some(ref err) = fixture.setup_error {
                    md.push_str("**Setup Error:**\n");
                    md.push_str("```\n");
                    md.push_str(err);
                    md.push_str("\n```\n\n");
                    continue;
                }

                // Layer 1 Errors
                if !fixture.layer1_errors.is_empty() {
                    md.push_str(&format!(
                        "**Layer 1 - Element Inventory ({} errors):**\n",
                        fixture.layer1_errors.len()
                    ));
                    for (i, err) in fixture.layer1_errors.iter().take(10).enumerate() {
                        md.push_str(&format!("{}. {}\n", i + 1, err));
                    }
                    if fixture.layer1_errors.len() > 10 {
                        md.push_str(&format!(
                            "\n*...and {} more errors*\n",
                            fixture.layer1_errors.len() - 10
                        ));
                    }
                    md.push('\n');
                }

                // Layer 2 Errors
                if !fixture.layer2_errors.is_empty() {
                    md.push_str(&format!(
                        "**Layer 2 - Properties ({} errors):**\n",
                        fixture.layer2_errors.len()
                    ));
                    for (i, err) in fixture.layer2_errors.iter().take(10).enumerate() {
                        md.push_str(&format!("{}. {}\n", i + 1, err));
                    }
                    if fixture.layer2_errors.len() > 10 {
                        md.push_str(&format!(
                            "\n*...and {} more errors*\n",
                            fixture.layer2_errors.len() - 10
                        ));
                    }
                    md.push('\n');
                }

                // Relationship Errors
                if !fixture.relationship_errors.is_empty() {
                    md.push_str(&format!(
                        "**Relationships ({} errors):**\n",
                        fixture.relationship_errors.len()
                    ));
                    for (i, err) in fixture.relationship_errors.iter().take(10).enumerate() {
                        md.push_str(&format!("{}. {}\n", i + 1, err));
                    }
                    if fixture.relationship_errors.len() > 10 {
                        md.push_str(&format!(
                            "\n*...and {} more errors*\n",
                            fixture.relationship_errors.len() - 10
                        ));
                    }
                    md.push('\n');
                }

                // Layer 4 Errors
                if !fixture.layer4_errors.is_empty() {
                    md.push_str(&format!(
                        "**Layer 4 - Ordering ({} errors):**\n",
                        fixture.layer4_errors.len()
                    ));
                    for (i, err) in fixture.layer4_errors.iter().take(10).enumerate() {
                        md.push_str(&format!("{}. {}\n", i + 1, err));
                    }
                    if fixture.layer4_errors.len() > 10 {
                        md.push_str(&format!(
                            "\n*...and {} more errors*\n",
                            fixture.layer4_errors.len() - 10
                        ));
                    }
                    md.push('\n');
                }

                // Metadata Errors
                if !fixture.metadata_errors.is_empty() {
                    md.push_str(&format!(
                        "**Metadata Files ({} errors):**\n",
                        fixture.metadata_errors.len()
                    ));
                    for (i, err) in fixture.metadata_errors.iter().take(10).enumerate() {
                        md.push_str(&format!("{}. {}\n", i + 1, err));
                    }
                    if fixture.metadata_errors.len() > 10 {
                        md.push_str(&format!(
                            "\n*...and {} more errors*\n",
                            fixture.metadata_errors.len() - 10
                        ));
                    }
                    md.push('\n');
                }
            }
        }

        // Footer
        md.push_str("---\n\n");
        md.push_str("*Report generated by rust-sqlpackage parity test infrastructure*\n");

        md
    }
}

impl Default for ParityReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the current git commit hash, if available.
fn get_git_commit_hash() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

// =============================================================================
// Phase 8.4: Regression Detection
// =============================================================================

/// Per-fixture baseline state capturing which layers pass.
///
/// This represents the expected state of a fixture as recorded in the baseline.
/// When running regression tests, current results are compared against this baseline
/// to detect regressions (previously passing layers now failing).
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureBaseline {
    /// Fixture name (directory name in tests/fixtures/)
    pub name: String,
    /// Whether Layer 1 (inventory) passes
    pub layer1_pass: bool,
    /// Whether Layer 2 (properties) passes
    pub layer2_pass: bool,
    /// Whether relationship comparison passes
    pub relationship_pass: bool,
    /// Whether Layer 4 (ordering) passes
    pub layer4_pass: bool,
    /// Whether metadata comparison passes
    pub metadata_pass: bool,
}

impl FixtureBaseline {
    /// Create a new FixtureBaseline from a fixture name with all layers failing.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            layer1_pass: false,
            layer2_pass: false,
            relationship_pass: false,
            layer4_pass: false,
            metadata_pass: false,
        }
    }

    /// Create a FixtureBaseline from FixtureMetrics (current test results).
    pub fn from_metrics(metrics: &FixtureMetrics) -> Self {
        Self {
            name: metrics.name.clone(),
            layer1_pass: metrics.layer1_errors == 0 && metrics.status != "ERROR",
            layer2_pass: metrics.layer2_errors == 0 && metrics.status != "ERROR",
            relationship_pass: metrics.relationship_errors == 0 && metrics.status != "ERROR",
            layer4_pass: metrics.layer4_errors == 0 && metrics.status != "ERROR",
            metadata_pass: metrics.metadata_errors == 0 && metrics.status != "ERROR",
        }
    }

    /// Parse a FixtureBaseline from a JSON object string.
    ///
    /// Expected format:
    /// ```json
    /// {
    ///   "name": "fixture_name",
    ///   "layer1_pass": true,
    ///   "layer2_pass": false,
    ///   ...
    /// }
    /// ```
    pub fn from_json(json: &str) -> Result<Self, String> {
        // Simple JSON parsing without external dependencies
        let name = extract_json_string(json, "name")?;
        let layer1_pass = extract_json_bool(json, "layer1_pass")?;
        let layer2_pass = extract_json_bool(json, "layer2_pass")?;
        let relationship_pass = extract_json_bool(json, "relationship_pass")?;
        let layer4_pass = extract_json_bool(json, "layer4_pass")?;
        let metadata_pass = extract_json_bool(json, "metadata_pass")?;

        Ok(Self {
            name,
            layer1_pass,
            layer2_pass,
            relationship_pass,
            layer4_pass,
            metadata_pass,
        })
    }

    /// Serialize to JSON format.
    pub fn to_json(&self) -> String {
        format!(
            r#"    {{
      "name": "{}",
      "layer1_pass": {},
      "layer2_pass": {},
      "relationship_pass": {},
      "layer4_pass": {},
      "metadata_pass": {}
    }}"#,
            self.name,
            self.layer1_pass,
            self.layer2_pass,
            self.relationship_pass,
            self.layer4_pass,
            self.metadata_pass
        )
    }
}

/// A regression detected when a previously passing layer now fails.
#[derive(Debug, Clone)]
pub struct Regression {
    /// Fixture name
    pub fixture: String,
    /// Which layer regressed
    pub layer: String,
    /// Description of the regression
    pub message: String,
}

impl std::fmt::Display for Regression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.fixture, self.layer, self.message)
    }
}

/// Parity baseline containing expected states for all fixtures.
///
/// The baseline is stored as a JSON file (`tests/e2e/parity-baseline.json`) and
/// tracks which fixtures pass at each layer. This enables:
/// - Detecting regressions: previously passing tests now failing
/// - Tracking progress: new fixtures passing that were previously failing
/// - CI enforcement: fail the build if regressions are detected
#[derive(Debug, Clone)]
pub struct ParityBaseline {
    /// Baseline format version for future compatibility
    pub version: u32,
    /// ISO 8601 timestamp when baseline was last updated
    pub updated: String,
    /// Git commit hash when baseline was established
    pub commit: Option<String>,
    /// Per-fixture baseline states
    pub fixtures: Vec<FixtureBaseline>,
}

impl ParityBaseline {
    /// Create a new empty baseline.
    pub fn new() -> Self {
        Self {
            version: 1,
            updated: chrono::Utc::now().to_rfc3339(),
            commit: get_git_commit_hash(),
            fixtures: Vec::new(),
        }
    }

    /// Create a baseline from current ParityMetrics (captures current state as baseline).
    pub fn from_metrics(metrics: &ParityMetrics) -> Self {
        let fixtures = metrics
            .fixtures
            .iter()
            .map(FixtureBaseline::from_metrics)
            .collect();

        Self {
            version: 1,
            updated: chrono::Utc::now().to_rfc3339(),
            commit: get_git_commit_hash(),
            fixtures,
        }
    }

    /// Load baseline from JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        // Parse version
        let version = extract_json_number(json, "version")? as u32;

        // Parse updated timestamp
        let updated = extract_json_string(json, "updated")?;

        // Parse commit (may be null)
        let commit = extract_json_string_optional(json, "commit");

        // Parse fixtures array
        let fixtures = parse_fixtures_array(json)?;

        Ok(Self {
            version,
            updated,
            commit,
            fixtures,
        })
    }

    /// Load baseline from a file path.
    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read baseline file: {}", e))?;
        Self::from_json(&content)
    }

    /// Serialize baseline to JSON format.
    pub fn to_json(&self) -> String {
        let mut json = String::new();
        json.push_str("{\n");
        json.push_str(&format!("  \"version\": {},\n", self.version));
        json.push_str(&format!("  \"updated\": \"{}\",\n", self.updated));

        if let Some(ref commit) = self.commit {
            json.push_str(&format!("  \"commit\": \"{}\",\n", commit));
        } else {
            json.push_str("  \"commit\": null,\n");
        }

        json.push_str("  \"fixtures\": [\n");
        for (i, fixture) in self.fixtures.iter().enumerate() {
            json.push_str(&fixture.to_json());
            if i < self.fixtures.len() - 1 {
                json.push_str(",\n");
            } else {
                json.push('\n');
            }
        }
        json.push_str("  ]\n");
        json.push_str("}\n");

        json
    }

    /// Save baseline to a file.
    pub fn to_file(&self, path: &std::path::Path) -> Result<(), String> {
        std::fs::write(path, self.to_json())
            .map_err(|e| format!("Failed to write baseline file: {}", e))
    }

    /// Get baseline for a specific fixture by name.
    pub fn get_fixture(&self, name: &str) -> Option<&FixtureBaseline> {
        self.fixtures.iter().find(|f| f.name == name)
    }

    /// Compare current metrics against this baseline to detect regressions.
    ///
    /// A regression occurs when a layer that passed in the baseline now fails.
    /// Returns a list of all detected regressions.
    pub fn detect_regressions(&self, current: &ParityMetrics) -> Vec<Regression> {
        let mut regressions = Vec::new();

        for current_fixture in &current.fixtures {
            // Skip fixtures that had errors (build failures, etc.)
            if current_fixture.status == "ERROR" {
                // If the fixture exists in baseline and had passing layers,
                // treat ERROR as regression for those layers
                if let Some(baseline) = self.get_fixture(&current_fixture.name) {
                    if baseline.layer1_pass {
                        regressions.push(Regression {
                            fixture: current_fixture.name.clone(),
                            layer: "Layer 1".to_string(),
                            message: format!(
                                "Build error (was passing): {}",
                                current_fixture
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("unknown error")
                            ),
                        });
                    }
                }
                continue;
            }

            // Find the baseline for this fixture
            let Some(baseline) = self.get_fixture(&current_fixture.name) else {
                // New fixture not in baseline - not a regression
                continue;
            };

            // Check each layer for regression
            if baseline.layer1_pass && current_fixture.layer1_errors > 0 {
                regressions.push(Regression {
                    fixture: current_fixture.name.clone(),
                    layer: "Layer 1 (inventory)".to_string(),
                    message: format!(
                        "was passing, now has {} errors",
                        current_fixture.layer1_errors
                    ),
                });
            }

            if baseline.layer2_pass && current_fixture.layer2_errors > 0 {
                regressions.push(Regression {
                    fixture: current_fixture.name.clone(),
                    layer: "Layer 2 (properties)".to_string(),
                    message: format!(
                        "was passing, now has {} errors",
                        current_fixture.layer2_errors
                    ),
                });
            }

            if baseline.relationship_pass && current_fixture.relationship_errors > 0 {
                regressions.push(Regression {
                    fixture: current_fixture.name.clone(),
                    layer: "Relationships".to_string(),
                    message: format!(
                        "was passing, now has {} errors",
                        current_fixture.relationship_errors
                    ),
                });
            }

            if baseline.layer4_pass && current_fixture.layer4_errors > 0 {
                regressions.push(Regression {
                    fixture: current_fixture.name.clone(),
                    layer: "Layer 4 (ordering)".to_string(),
                    message: format!(
                        "was passing, now has {} errors",
                        current_fixture.layer4_errors
                    ),
                });
            }

            if baseline.metadata_pass && current_fixture.metadata_errors > 0 {
                regressions.push(Regression {
                    fixture: current_fixture.name.clone(),
                    layer: "Metadata".to_string(),
                    message: format!(
                        "was passing, now has {} errors",
                        current_fixture.metadata_errors
                    ),
                });
            }
        }

        regressions
    }

    /// Find improvements: layers that were failing but now pass.
    pub fn detect_improvements(&self, current: &ParityMetrics) -> Vec<String> {
        let mut improvements = Vec::new();

        for current_fixture in &current.fixtures {
            // Skip fixtures with errors
            if current_fixture.status == "ERROR" {
                continue;
            }

            let Some(baseline) = self.get_fixture(&current_fixture.name) else {
                // New fixture - if it has any passing layers, note that
                if current_fixture.layer1_errors == 0 {
                    improvements.push(format!(
                        "[{}] New fixture with passing Layer 1",
                        current_fixture.name
                    ));
                }
                continue;
            };

            // Check each layer for improvement
            if !baseline.layer1_pass && current_fixture.layer1_errors == 0 {
                improvements.push(format!(
                    "[{}] Layer 1 (inventory) now passes!",
                    current_fixture.name
                ));
            }

            if !baseline.layer2_pass && current_fixture.layer2_errors == 0 {
                improvements.push(format!(
                    "[{}] Layer 2 (properties) now passes!",
                    current_fixture.name
                ));
            }

            if !baseline.relationship_pass && current_fixture.relationship_errors == 0 {
                improvements.push(format!(
                    "[{}] Relationships now pass!",
                    current_fixture.name
                ));
            }

            if !baseline.layer4_pass && current_fixture.layer4_errors == 0 {
                improvements.push(format!(
                    "[{}] Layer 4 (ordering) now passes!",
                    current_fixture.name
                ));
            }

            if !baseline.metadata_pass && current_fixture.metadata_errors == 0 {
                improvements.push(format!("[{}] Metadata now passes!", current_fixture.name));
            }
        }

        improvements
    }

    /// Generate a summary of regression check results.
    pub fn print_regression_summary(&self, current: &ParityMetrics) {
        let regressions = self.detect_regressions(current);
        let improvements = self.detect_improvements(current);

        println!("\n{}", "=".repeat(60));
        println!("REGRESSION CHECK RESULTS");
        println!("{}\n", "=".repeat(60));

        if regressions.is_empty() {
            println!("✓ No regressions detected!");
        } else {
            println!("✗ {} REGRESSIONS DETECTED:\n", regressions.len());
            for regression in &regressions {
                println!("  - {}", regression);
            }
        }

        if !improvements.is_empty() {
            println!("\n✓ {} improvements detected:\n", improvements.len());
            for improvement in &improvements {
                println!("  + {}", improvement);
            }
        }

        println!();
    }
}

impl Default for ParityBaseline {
    fn default() -> Self {
        Self::new()
    }
}

// Helper functions for simple JSON parsing without external dependencies

fn extract_json_string(json: &str, key: &str) -> Result<String, String> {
    let pattern = format!(r#""{}"\s*:\s*""#, key);
    let re = regex::Regex::new(&pattern).map_err(|e| e.to_string())?;

    if let Some(m) = re.find(json) {
        let start = m.end();
        let remaining = &json[start..];
        if let Some(end) = remaining.find('"') {
            return Ok(remaining[..end].to_string());
        }
    }
    Err(format!("Failed to parse JSON key: {}", key))
}

fn extract_json_string_optional(json: &str, key: &str) -> Option<String> {
    // Check for null value
    let null_pattern = format!(r#""{}"\s*:\s*null"#, key);
    if regex::Regex::new(&null_pattern).ok()?.is_match(json) {
        return None;
    }
    extract_json_string(json, key).ok()
}

fn extract_json_bool(json: &str, key: &str) -> Result<bool, String> {
    let pattern = format!(r#""{}"\s*:\s*(true|false)"#, key);
    let re = regex::Regex::new(&pattern).map_err(|e| e.to_string())?;

    if let Some(caps) = re.captures(json) {
        if let Some(m) = caps.get(1) {
            return Ok(m.as_str() == "true");
        }
    }
    Err(format!("Failed to parse JSON boolean: {}", key))
}

fn extract_json_number(json: &str, key: &str) -> Result<i64, String> {
    let pattern = format!(r#""{}"\s*:\s*(\d+)"#, key);
    let re = regex::Regex::new(&pattern).map_err(|e| e.to_string())?;

    if let Some(caps) = re.captures(json) {
        if let Some(m) = caps.get(1) {
            return m
                .as_str()
                .parse()
                .map_err(|e: std::num::ParseIntError| e.to_string());
        }
    }
    Err(format!("Failed to parse JSON number: {}", key))
}

fn parse_fixtures_array(json: &str) -> Result<Vec<FixtureBaseline>, String> {
    // Find the fixtures array
    let fixtures_start = json.find(r#""fixtures""#).ok_or("Missing fixtures array")?;
    let array_start = json[fixtures_start..]
        .find('[')
        .ok_or("Missing array start")?
        + fixtures_start;

    // Find matching closing bracket
    let mut depth = 0;
    let mut array_end = array_start;
    for (i, c) in json[array_start..].chars().enumerate() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    array_end = array_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    let array_content = &json[array_start + 1..array_end];

    // Parse each fixture object
    let mut fixtures = Vec::new();
    let mut depth = 0;
    let mut obj_start = None;

    for (i, c) in array_content.chars().enumerate() {
        match c {
            '{' => {
                if depth == 0 {
                    obj_start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = obj_start {
                        let obj = &array_content[start..=i];
                        fixtures.push(FixtureBaseline::from_json(obj)?);
                    }
                    obj_start = None;
                }
            }
            _ => {}
        }
    }

    Ok(fixtures)
}
