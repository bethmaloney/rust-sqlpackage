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
