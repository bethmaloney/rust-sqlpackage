//! Layered dacpac comparison utilities for E2E testing
//!
//! Provides multiple layers of comparison:
//! 1. Element inventory - verify all elements exist with correct names
//! 2. Property comparison - verify element properties match
//! 3. SqlPackage DeployReport - verify deployment equivalence
//! 4. Element ordering - verify element order matches DotNet output
//! 5. Metadata files - verify Content_Types.xml, DacMetadata.xml, Origin.xml match

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use zip::ZipArchive;

// =============================================================================
// Data Structures
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

/// Layer 2: Property comparison errors
#[derive(Debug)]
pub struct Layer2Error {
    pub element_type: String,
    pub element_name: String,
    pub property_name: String,
    pub rust_value: Option<String>,
    pub dotnet_value: Option<String>,
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

// =============================================================================
// XML Parsing
// =============================================================================

impl DacpacModel {
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
// Layer 1: Element Inventory Comparison
// =============================================================================

/// Compare element inventories between two models
pub fn compare_element_inventory(
    rust_model: &DacpacModel,
    dotnet_model: &DacpacModel,
) -> Vec<Layer1Error> {
    let mut errors = Vec::new();

    let rust_elements = rust_model.named_elements();
    let dotnet_elements = dotnet_model.named_elements();

    // Find elements missing in Rust
    for (elem_type, name) in dotnet_elements.difference(&rust_elements) {
        errors.push(Layer1Error::MissingInRust {
            element_type: elem_type.clone(),
            name: name.clone(),
        });
    }

    // Find extra elements in Rust
    for (elem_type, name) in rust_elements.difference(&dotnet_elements) {
        errors.push(Layer1Error::ExtraInRust {
            element_type: elem_type.clone(),
            name: name.clone(),
        });
    }

    // Compare counts by type
    let rust_types = rust_model.element_types();
    let dotnet_types = dotnet_model.element_types();
    let all_types: BTreeSet<_> = rust_types.union(&dotnet_types).collect();

    for elem_type in all_types {
        let rust_count = rust_model.elements_of_type(elem_type).len();
        let dotnet_count = dotnet_model.elements_of_type(elem_type).len();

        if rust_count != dotnet_count {
            errors.push(Layer1Error::CountMismatch {
                element_type: elem_type.clone(),
                rust_count,
                dotnet_count,
            });
        }
    }

    errors
}

// =============================================================================
// Layer 2: Property Comparison
// =============================================================================

/// Key properties to compare for each element type (subset for backward compatibility)
fn get_key_properties(element_type: &str) -> &'static [&'static str] {
    match element_type {
        "SqlTable" => &["IsAnsiNullsOn"],
        "SqlSimpleColumn" => &["IsNullable", "IsIdentity", "IsRowGuidCol", "IsPersisted"],
        "SqlComputedColumn" => &["IsPersisted", "ExpressionScript"],
        "SqlPrimaryKeyConstraint" => &["IsClustered"],
        "SqlIndex" => &["IsClustered", "IsUnique", "FilterPredicate"],
        "SqlForeignKeyConstraint" => &["DeleteAction", "UpdateAction"],
        "SqlDefaultConstraint" => &["Expression"],
        "SqlCheckConstraint" => &["Expression", "IsNotForReplication"],
        "SqlProcedure" => &["BodyScript"],
        "SqlScalarFunction" => &["BodyScript"],
        "SqlView" => &["SelectScript", "IsAnsiNullsOn", "IsQuotedIdentifierOn"],
        "SqlSubroutineParameter" => &["IsOutput", "IsReadOnly"],
        "SqlTypeSpecifier" => &["Length", "Precision", "Scale", "IsMax"],
        _ => &[],
    }
}

/// Complete set of properties for each element type based on DotNet DacFx output.
/// This documents all known properties that DotNet generates for parity testing.
///
/// Property Documentation by Element Type:
///
/// SqlDatabaseOptions - Database-level settings
///   - Collation: Database collation (e.g., "SQL_Latin1_General_CP1_CI_AS")
///   - IsAnsiNullDefaultOn: ANSI NULL default setting
///   - IsAnsiNullsOn: ANSI nulls setting
///   - IsAnsiWarningsOn: ANSI warnings setting
///   - IsArithAbortOn: Arithmetic abort setting
///   - IsConcatNullYieldsNullOn: Concat null behavior
///   - IsTornPageProtectionOn: Torn page detection
///   - IsFullTextEnabled: Full-text search enabled
///   - PageVerifyMode: Page verification mode (0=NONE, 1=TORN_PAGE, 3=CHECKSUM)
///   - DefaultLanguage: Default language setting
///   - DefaultFullTextLanguage: Default full-text language
///   - QueryStoreStaleQueryThreshold: Query store threshold
///
/// SqlTable - Table definitions
///   - IsAnsiNullsOn: ANSI nulls setting for table creation context
///
/// SqlSimpleColumn - Regular table columns
///   - IsNullable: Whether column allows NULL values
///   - IsIdentity: Whether column is an identity column
///   - IsRowGuidCol: Whether column is a ROWGUIDCOL
///   - IsSparse: Whether column is sparse
///   - IsColumnSet: Whether column is a column set
///
/// SqlComputedColumn - Computed columns
///   - IsPersisted: Whether computed value is stored
///   - ExpressionScript: The computation expression
///
/// SqlTypeSpecifier - Type information for columns/parameters
///   - Length: Character/binary length
///   - Precision: Numeric precision
///   - Scale: Numeric scale
///   - IsMax: Whether MAX length (varchar(max), etc.)
///
/// SqlIndex - Index definitions
///   - IsClustered: Whether index is clustered
///   - IsUnique: Whether index enforces uniqueness
///   - IsDisabled: Whether index is disabled
///   - FillFactor: Index fill factor (0-100)
///   - FilterPredicate/FilterDefinition: Filtered index predicate
///   - IgnoreDuplicateKeys: Ignore duplicate key behavior
///   - DisallowPageLocks: Page lock behavior
///   - DisallowRowLocks: Row lock behavior
///   - PadIndex: Pad index setting
///
/// SqlIndexedColumnSpecification - Index column details
///   - IsAscending: Sort order (True=ASC, False=DESC)
///
/// SqlPrimaryKeyConstraint - Primary key constraints
///   - IsClustered: Whether PK is clustered
///
/// SqlUniqueConstraint - Unique constraints
///   - IsClustered: Whether unique constraint is clustered
///
/// SqlForeignKeyConstraint - Foreign key constraints
///   - DeleteAction: ON DELETE action (NO ACTION, CASCADE, SET NULL, SET DEFAULT)
///   - UpdateAction: ON UPDATE action
///   - IsNotForReplication: NOT FOR REPLICATION setting
///
/// SqlCheckConstraint - Check constraints
///   - CheckExpressionScript: The check expression (CDATA)
///   - IsNotForReplication: NOT FOR REPLICATION setting
///
/// SqlDefaultConstraint - Default constraints
///   - DefaultExpressionScript: The default value expression (CDATA)
///
/// SqlProcedure - Stored procedures
///   - BodyScript: Procedure body (CDATA)
///   - IsNativelyCompiled: Native compilation setting
///
/// SqlScalarFunction / SqlMultiStatementTableValuedFunction - Functions
///   - BodyScript: Function body (CDATA)
///   - HeaderContents: Function header for parsing
///
/// SqlView - View definitions
///   - QueryScript: View SELECT statement (CDATA)
///   - IsAnsiNullsOn: ANSI nulls context
///   - IsQuotedIdentifierOn: Quoted identifier context
///
/// SqlSubroutineParameter - Procedure/function parameters
///   - IsOutput: Whether parameter is OUTPUT
///   - IsReadOnly: Whether parameter is READONLY (for TVPs)
///
/// SqlExtendedProperty - Extended properties
///   - Value: The extended property value (CDATA)
///
/// SqlSequence - Sequence objects
///   - StartValue: Starting value
///   - IncrementValue: Increment
///   - MinValue: Minimum value
///   - MaxValue: Maximum value
///   - IsCycling: Whether sequence cycles
///   - CacheSize: Cache size
fn get_all_properties(element_type: &str) -> &'static [&'static str] {
    match element_type {
        "SqlDatabaseOptions" => &[
            "Collation",
            "IsAnsiNullDefaultOn",
            "IsAnsiNullsOn",
            "IsAnsiWarningsOn",
            "IsArithAbortOn",
            "IsConcatNullYieldsNullOn",
            "IsTornPageProtectionOn",
            "IsFullTextEnabled",
            "PageVerifyMode",
            "DefaultLanguage",
            "DefaultFullTextLanguage",
            "QueryStoreStaleQueryThreshold",
        ],
        "SqlTable" => &["IsAnsiNullsOn"],
        "SqlSimpleColumn" => &[
            "IsNullable",
            "IsIdentity",
            "IsRowGuidCol",
            "IsSparse",
            "IsColumnSet",
        ],
        "SqlTableTypeSimpleColumn" => &["IsNullable", "IsIdentity", "IsRowGuidCol"],
        "SqlComputedColumn" => &["IsPersisted", "ExpressionScript"],
        "SqlTypeSpecifier" => &["Length", "Precision", "Scale", "IsMax"],
        "SqlIndex" => &[
            "IsClustered",
            "IsUnique",
            "IsDisabled",
            "FillFactor",
            "FilterPredicate",
            "FilterDefinition",
            "IgnoreDuplicateKeys",
            "DisallowPageLocks",
            "DisallowRowLocks",
            "PadIndex",
        ],
        "SqlIndexedColumnSpecification" => &["IsAscending"],
        "SqlPrimaryKeyConstraint" => &["IsClustered"],
        "SqlUniqueConstraint" => &["IsClustered"],
        "SqlForeignKeyConstraint" => &["DeleteAction", "UpdateAction", "IsNotForReplication"],
        "SqlCheckConstraint" => &["CheckExpressionScript", "IsNotForReplication"],
        "SqlDefaultConstraint" => &["DefaultExpressionScript"],
        "SqlProcedure" => &["BodyScript", "IsNativelyCompiled"],
        "SqlScalarFunction" => &["BodyScript", "HeaderContents"],
        "SqlMultiStatementTableValuedFunction" => &["BodyScript", "HeaderContents"],
        "SqlInlineTableValuedFunction" => &["BodyScript", "HeaderContents"],
        "SqlView" => &["QueryScript", "IsAnsiNullsOn", "IsQuotedIdentifierOn"],
        "SqlSubroutineParameter" => &["IsOutput", "IsReadOnly"],
        "SqlExtendedProperty" => &["Value"],
        "SqlSequence" => &[
            "StartValue",
            "IncrementValue",
            "MinValue",
            "MaxValue",
            "IsCycling",
            "CacheSize",
        ],
        "SqlTableType" => &["IsAnsiNullsOn"],
        "SqlSchema" | "SqlCmdVariable" | "SqlScriptFunctionImplementation" => &[],
        _ => &[],
    }
}

/// Compare properties of matching elements
pub fn compare_element_properties(
    rust_model: &DacpacModel,
    dotnet_model: &DacpacModel,
) -> Vec<Layer2Error> {
    let mut errors = Vec::new();

    // Compare properties of all matching named elements
    for (elem_type, name) in rust_model.named_elements() {
        if let (Some(rust_elem), Some(dotnet_elem)) = (
            rust_model.get_element(&elem_type, &name),
            dotnet_model.get_element(&elem_type, &name),
        ) {
            errors.extend(compare_element_pair(rust_elem, dotnet_elem));
        }
    }

    errors
}

fn compare_element_pair(rust_elem: &ModelElement, dotnet_elem: &ModelElement) -> Vec<Layer2Error> {
    compare_element_pair_internal(rust_elem, dotnet_elem, false)
}

fn compare_element_pair_strict(
    rust_elem: &ModelElement,
    dotnet_elem: &ModelElement,
) -> Vec<Layer2Error> {
    compare_element_pair_internal(rust_elem, dotnet_elem, true)
}

fn compare_element_pair_internal(
    rust_elem: &ModelElement,
    dotnet_elem: &ModelElement,
    strict: bool,
) -> Vec<Layer2Error> {
    let mut errors = Vec::new();

    let props_to_check = if strict {
        get_all_properties(&rust_elem.element_type)
    } else {
        get_key_properties(&rust_elem.element_type)
    };

    for &prop_name in props_to_check {
        let rust_val = rust_elem.properties.get(prop_name);
        let dotnet_val = dotnet_elem.properties.get(prop_name);

        if rust_val != dotnet_val {
            errors.push(Layer2Error {
                element_type: rust_elem.element_type.clone(),
                element_name: rust_elem.name.clone().unwrap_or_default(),
                property_name: prop_name.to_string(),
                rust_value: rust_val.cloned(),
                dotnet_value: dotnet_val.cloned(),
            });
        }
    }

    // Recursively compare children (columns, parameters, etc.)
    // Match children by name if available, otherwise by index
    let rust_named: HashMap<_, _> = rust_elem
        .children
        .iter()
        .filter_map(|c| c.name.as_ref().map(|n| (n.clone(), c)))
        .collect();

    for dotnet_child in &dotnet_elem.children {
        if let Some(ref child_name) = dotnet_child.name {
            if let Some(rust_child) = rust_named.get(child_name) {
                errors.extend(compare_element_pair_internal(
                    rust_child,
                    dotnet_child,
                    strict,
                ));
            }
        }
    }

    errors
}

/// Compare ALL properties of matching elements (strict mode).
/// This compares the complete set of properties defined in `get_all_properties()`
/// rather than just the key properties. Used for exact parity testing.
pub fn compare_all_properties(
    rust_model: &DacpacModel,
    dotnet_model: &DacpacModel,
) -> Vec<Layer2Error> {
    let mut errors = Vec::new();

    // Compare properties of all matching named elements
    for (elem_type, name) in rust_model.named_elements() {
        if let (Some(rust_elem), Some(dotnet_elem)) = (
            rust_model.get_element(&elem_type, &name),
            dotnet_model.get_element(&elem_type, &name),
        ) {
            errors.extend(compare_element_pair_strict(rust_elem, dotnet_elem));
        }
    }

    errors
}

// =============================================================================
// Relationship Comparison (Phase 3)
// =============================================================================

/// Compare relationships of all matching elements between two models
pub fn compare_element_relationships(
    rust_model: &DacpacModel,
    dotnet_model: &DacpacModel,
) -> Vec<RelationshipError> {
    let mut errors = Vec::new();

    // Compare relationships of all matching named elements
    for (elem_type, name) in rust_model.named_elements() {
        if let (Some(rust_elem), Some(dotnet_elem)) = (
            rust_model.get_element(&elem_type, &name),
            dotnet_model.get_element(&elem_type, &name),
        ) {
            errors.extend(compare_relationships(rust_elem, dotnet_elem));
        }
    }

    errors
}

/// Compare relationships between two matching elements
pub fn compare_relationships(
    rust_elem: &ModelElement,
    dotnet_elem: &ModelElement,
) -> Vec<RelationshipError> {
    let mut errors = Vec::new();
    let elem_type = rust_elem.element_type.clone();
    let elem_name = rust_elem.name.clone().unwrap_or_default();

    // Build maps of relationships by name for both elements
    let rust_rels: HashMap<_, _> = rust_elem
        .relationships
        .iter()
        .map(|r| (r.name.clone(), r))
        .collect();

    let dotnet_rels: HashMap<_, _> = dotnet_elem
        .relationships
        .iter()
        .map(|r| (r.name.clone(), r))
        .collect();

    // Find relationships missing in Rust (present in DotNet but not Rust)
    for rel_name in dotnet_rels.keys() {
        if !rust_rels.contains_key(rel_name) {
            errors.push(RelationshipError::MissingRelationship {
                element_type: elem_type.clone(),
                element_name: elem_name.clone(),
                relationship_name: rel_name.clone(),
            });
        }
    }

    // Find extra relationships in Rust (present in Rust but not DotNet)
    for rel_name in rust_rels.keys() {
        if !dotnet_rels.contains_key(rel_name) {
            errors.push(RelationshipError::ExtraRelationship {
                element_type: elem_type.clone(),
                element_name: elem_name.clone(),
                relationship_name: rel_name.clone(),
            });
        }
    }

    // Compare matching relationships
    for (rel_name, rust_rel) in &rust_rels {
        if let Some(dotnet_rel) = dotnet_rels.get(rel_name) {
            errors.extend(compare_relationship_pair(
                &elem_type, &elem_name, rust_rel, dotnet_rel,
            ));
        }
    }

    // Recursively compare relationships in nested elements (children)
    let rust_named_children: HashMap<_, _> = rust_elem
        .children
        .iter()
        .filter_map(|c| c.name.as_ref().map(|n| (n.clone(), c)))
        .collect();

    for dotnet_child in &dotnet_elem.children {
        if let Some(ref child_name) = dotnet_child.name {
            if let Some(rust_child) = rust_named_children.get(child_name) {
                errors.extend(compare_relationships(rust_child, dotnet_child));
            }
        }
    }

    errors
}

/// Compare a pair of relationships with the same name
fn compare_relationship_pair(
    elem_type: &str,
    elem_name: &str,
    rust_rel: &Relationship,
    dotnet_rel: &Relationship,
) -> Vec<RelationshipError> {
    let mut errors = Vec::new();

    // Compare reference counts
    if rust_rel.references.len() != dotnet_rel.references.len() {
        errors.push(RelationshipError::ReferenceCountMismatch {
            element_type: elem_type.to_string(),
            element_name: elem_name.to_string(),
            relationship_name: rust_rel.name.clone(),
            rust_count: rust_rel.references.len(),
            dotnet_count: dotnet_rel.references.len(),
        });
    }

    // Compare reference names (order matters for some relationships)
    let rust_ref_names: Vec<_> = rust_rel.references.iter().map(|r| r.name.clone()).collect();
    let dotnet_ref_names: Vec<_> = dotnet_rel
        .references
        .iter()
        .map(|r| r.name.clone())
        .collect();

    if rust_ref_names != dotnet_ref_names {
        errors.push(RelationshipError::ReferenceMismatch {
            element_type: elem_type.to_string(),
            element_name: elem_name.to_string(),
            relationship_name: rust_rel.name.clone(),
            rust_refs: rust_ref_names,
            dotnet_refs: dotnet_ref_names,
        });
    }

    // Compare nested element counts
    if rust_rel.entries.len() != dotnet_rel.entries.len() {
        errors.push(RelationshipError::EntryCountMismatch {
            element_type: elem_type.to_string(),
            element_name: elem_name.to_string(),
            relationship_name: rust_rel.name.clone(),
            rust_count: rust_rel.entries.len(),
            dotnet_count: dotnet_rel.entries.len(),
        });
    }

    // Recursively compare nested element relationships
    let rust_named_entries: HashMap<_, _> = rust_rel
        .entries
        .iter()
        .filter_map(|e| e.name.as_ref().map(|n| (n.clone(), e)))
        .collect();

    for dotnet_entry in &dotnet_rel.entries {
        if let Some(ref entry_name) = dotnet_entry.name {
            if let Some(rust_entry) = rust_named_entries.get(entry_name) {
                errors.extend(compare_relationships(rust_entry, dotnet_entry));
            }
        }
    }

    errors
}

// =============================================================================
// Layer 4: Element Order Comparison (Phase 4)
// =============================================================================

/// Compare the ordering of elements between two models.
///
/// DotNet DacFx generates elements in a specific, deterministic order:
/// 1. Elements are typically grouped by type (schemas, tables, views, procedures, etc.)
/// 2. Within each type, elements may be ordered alphabetically or by dependency
///
/// This function compares:
/// - Type ordering: Which element types appear first
/// - Element ordering: Position of individual elements within the model
///
/// Returns errors for any ordering mismatches found.
pub fn compare_element_order(
    rust_model: &DacpacModel,
    dotnet_model: &DacpacModel,
) -> Vec<Layer4Error> {
    let mut errors = Vec::new();

    // Build position maps for named elements
    // Position is the index in the elements vector (order in XML)
    let rust_positions = build_element_position_map(rust_model);
    let dotnet_positions = build_element_position_map(dotnet_model);

    // Compare type ordering - which types appear first in each model
    errors.extend(compare_type_ordering(rust_model, dotnet_model));

    // Compare individual element positions
    // Only compare elements that exist in both models (Layer 1 catches missing/extra)
    for ((elem_type, elem_name), &rust_pos) in &rust_positions {
        if let Some(&dotnet_pos) = dotnet_positions.get(&(elem_type.clone(), elem_name.clone())) {
            if rust_pos != dotnet_pos {
                errors.push(Layer4Error::ElementOrderMismatch {
                    element_type: elem_type.clone(),
                    element_name: elem_name.clone(),
                    rust_position: rust_pos,
                    dotnet_position: dotnet_pos,
                });
            }
        }
    }

    errors
}

/// Build a map of (element_type, element_name) -> position index
fn build_element_position_map(model: &DacpacModel) -> HashMap<(String, String), usize> {
    let mut positions = HashMap::new();

    for (idx, element) in model.elements.iter().enumerate() {
        if let Some(ref name) = element.name {
            positions.insert((element.element_type.clone(), name.clone()), idx);
        }
    }

    positions
}

/// Compare the ordering of element types between models.
///
/// For example, if DotNet outputs schemas at position 0-2, tables at 3-10,
/// and views at 11-15, Rust should follow the same pattern.
fn compare_type_ordering(rust_model: &DacpacModel, dotnet_model: &DacpacModel) -> Vec<Layer4Error> {
    let mut errors = Vec::new();

    // Find first occurrence of each type
    let rust_type_first_pos = find_type_first_positions(rust_model);
    let dotnet_type_first_pos = find_type_first_positions(dotnet_model);

    // Get all types that appear in both models
    let all_types: BTreeSet<_> = rust_type_first_pos
        .keys()
        .chain(dotnet_type_first_pos.keys())
        .cloned()
        .collect();

    for elem_type in all_types {
        if let (Some(&rust_first), Some(&dotnet_first)) = (
            rust_type_first_pos.get(&elem_type),
            dotnet_type_first_pos.get(&elem_type),
        ) {
            // Compare relative ordering of types
            // We don't require exact positions, but check if the relative order differs significantly
            if rust_first != dotnet_first {
                errors.push(Layer4Error::TypeOrderMismatch {
                    element_type: elem_type,
                    rust_first_position: rust_first,
                    dotnet_first_position: dotnet_first,
                });
            }
        }
    }

    errors
}

/// Find the first position where each element type appears in the model
fn find_type_first_positions(model: &DacpacModel) -> HashMap<String, usize> {
    let mut first_positions = HashMap::new();

    for (idx, element) in model.elements.iter().enumerate() {
        first_positions
            .entry(element.element_type.clone())
            .or_insert(idx);
    }

    first_positions
}

// =============================================================================
// Phase 5: Metadata File Comparison
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
                    .map_or(false, |p| p.has_tag_name("PackageProperties"))
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
// Phase 5.4: Pre/Post Deploy Script Comparison
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
fn normalize_script_whitespace(content: &str) -> String {
    // Convert CRLF to LF
    let content = content.replace("\r\n", "\n");

    // Process each line: trim trailing whitespace
    let lines: Vec<&str> = content.lines().map(|line| line.trim_end()).collect();

    // Remove trailing empty lines
    let mut result: Vec<&str> = lines;
    while result.last().map_or(false, |line| line.is_empty()) {
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
// Phase 5.5: Unified Metadata File Comparison
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
///
/// # Example
/// ```ignore
/// let errors = compare_dacpac_files(&rust_path, &dotnet_path);
/// if errors.is_empty() {
///     println!("All metadata files match!");
/// } else {
///     for error in &errors {
///         println!("Metadata difference: {}", error);
///     }
/// }
/// ```
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

// =============================================================================
// Phase 7: Canonical XML Comparison
// =============================================================================

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

/// Represents a canonicalized element for sorting and serialization
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalElement {
    /// Element type (e.g., "SqlTable", "SqlView")
    element_type: String,
    /// Element name (e.g., "[dbo].[Products]") - empty string for unnamed elements
    name: String,
    /// Sorted properties as (name, value) pairs
    properties: Vec<(String, String)>,
    /// Sorted relationships
    relationships: Vec<CanonicalRelationship>,
    /// Nested child elements (recursively canonicalized)
    children: Vec<CanonicalElement>,
}

/// Represents a canonicalized relationship for sorting
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalRelationship {
    /// Relationship name (e.g., "Schema", "Columns")
    name: String,
    /// Sorted references as (name, external_source) pairs
    references: Vec<(String, Option<String>)>,
    /// Nested entry elements (recursively canonicalized)
    entries: Vec<CanonicalElement>,
}

/// Canonicalize a model.xml string for deterministic comparison.
///
/// Canonicalization normalizes the XML structure by:
/// 1. Parsing the XML into a structured representation
/// 2. Sorting top-level elements by (Type, Name) - preserving semantic grouping
/// 3. Sorting properties alphabetically by Name within each element
/// 4. Sorting relationships alphabetically by Name within each element
/// 5. Recursively canonicalizing nested elements within relationships
/// 6. Normalizing whitespace (consistent indentation, no trailing whitespace)
/// 7. Re-serializing to a deterministic XML string
///
/// This allows byte-level comparison of semantically equivalent model.xml files
/// that may have different element ordering or formatting.
///
/// # Arguments
/// * `xml` - The raw model.xml content string
///
/// # Returns
/// A canonicalized XML string suitable for comparison, or an error message.
///
/// # Example
/// ```ignore
/// let rust_canonical = canonicalize_model_xml(&rust_xml)?;
/// let dotnet_canonical = canonicalize_model_xml(&dotnet_xml)?;
/// assert_eq!(rust_canonical, dotnet_canonical);
/// ```
pub fn canonicalize_model_xml(xml: &str) -> Result<String, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("Failed to parse XML: {}", e))?;

    let root = doc.root_element();

    // Extract root attributes
    let file_format_version = root.attribute("FileFormatVersion").unwrap_or("1.2");
    let schema_version = root.attribute("SchemaVersion").unwrap_or("2.9");
    let dsp_name = root
        .attribute("DspName")
        .unwrap_or("Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider");
    let collation_lcid = root.attribute("CollationLcid").unwrap_or("1033");
    let collation_case_sensitive = root.attribute("CollationCaseSensitive").unwrap_or("False");

    // Find Model element
    let model_node = root
        .children()
        .find(|n| n.has_tag_name("Model"))
        .ok_or("No Model element found")?;

    // Parse and canonicalize all elements
    let mut canonical_elements: Vec<CanonicalElement> = model_node
        .children()
        .filter(|n| n.has_tag_name("Element"))
        .map(|node| canonicalize_element(&node))
        .collect();

    // Sort elements by (type, name) for deterministic ordering
    canonical_elements.sort();

    // Find Header element (if present)
    let header_node = root.children().find(|n| n.has_tag_name("Header"));

    // Serialize to canonical XML
    let mut output = String::new();
    output.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    output.push_str(&format!(
        "<DataSchemaModel FileFormatVersion=\"{}\" SchemaVersion=\"{}\" DspName=\"{}\" CollationLcid=\"{}\" CollationCaseSensitive=\"{}\" xmlns=\"http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02\">\n",
        file_format_version, schema_version, dsp_name, collation_lcid, collation_case_sensitive
    ));

    // Serialize Header if present
    if let Some(header) = header_node {
        serialize_header(&header, &mut output, 1);
    }

    // Serialize Model with canonicalized elements
    output.push_str("  <Model>\n");
    for element in &canonical_elements {
        serialize_canonical_element(element, &mut output, 2);
    }
    output.push_str("  </Model>\n");

    output.push_str("</DataSchemaModel>\n");

    Ok(output)
}

/// Convert a parsed element node into a canonical representation
fn canonicalize_element(node: &roxmltree::Node) -> CanonicalElement {
    let element_type = node.attribute("Type").unwrap_or("Unknown").to_string();
    let name = node.attribute("Name").unwrap_or("").to_string();

    // Collect and sort properties
    let mut properties: Vec<(String, String)> = node
        .children()
        .filter(|n| n.has_tag_name("Property"))
        .filter_map(|n| {
            let prop_name = n.attribute("Name")?;
            // Handle both inline Value attribute and nested <Value> element
            let prop_value = if let Some(val) = n.attribute("Value") {
                val.to_string()
            } else {
                // Look for nested <Value> element (for CDATA content)
                n.children()
                    .find(|c| c.has_tag_name("Value"))
                    .map(|v| v.text().unwrap_or("").to_string())
                    .unwrap_or_default()
            };
            Some((prop_name.to_string(), prop_value))
        })
        .collect();
    properties.sort();

    // Collect and sort relationships
    let mut relationships: Vec<CanonicalRelationship> = node
        .children()
        .filter(|n| n.has_tag_name("Relationship"))
        .map(|rel_node| canonicalize_relationship(&rel_node))
        .collect();
    relationships.sort();

    // Collect and canonicalize direct child elements (for Annotation elements)
    let mut children: Vec<CanonicalElement> = node
        .children()
        .filter(|n| n.has_tag_name("Annotation"))
        .map(|n| canonicalize_annotation(&n))
        .collect();
    children.sort();

    CanonicalElement {
        element_type,
        name,
        properties,
        relationships,
        children,
    }
}

/// Convert a relationship node into a canonical representation
fn canonicalize_relationship(node: &roxmltree::Node) -> CanonicalRelationship {
    let name = node.attribute("Name").unwrap_or("Unknown").to_string();

    let mut references: Vec<(String, Option<String>)> = Vec::new();
    let mut entries: Vec<CanonicalElement> = Vec::new();

    for entry in node.children().filter(|n| n.has_tag_name("Entry")) {
        for child in entry.children() {
            if child.has_tag_name("References") {
                if let Some(ref_name) = child.attribute("Name") {
                    let external_source = child.attribute("ExternalSource").map(|s| s.to_string());
                    references.push((ref_name.to_string(), external_source));
                }
            } else if child.has_tag_name("Element") {
                entries.push(canonicalize_element(&child));
            }
        }
    }

    // Sort references and entries for deterministic ordering
    references.sort();
    entries.sort();

    CanonicalRelationship {
        name,
        references,
        entries,
    }
}

/// Convert an Annotation element into a canonical representation
fn canonicalize_annotation(node: &roxmltree::Node) -> CanonicalElement {
    let element_type = node.attribute("Type").unwrap_or("Annotation").to_string();
    let disambiguator = node.attribute("Disambiguator").unwrap_or("").to_string();

    // Collect and sort properties
    let mut properties: Vec<(String, String)> = node
        .children()
        .filter(|n| n.has_tag_name("Property"))
        .filter_map(|n| {
            let prop_name = n.attribute("Name")?;
            let prop_value = if let Some(val) = n.attribute("Value") {
                val.to_string()
            } else {
                n.children()
                    .find(|c| c.has_tag_name("Value"))
                    .map(|v| v.text().unwrap_or("").to_string())
                    .unwrap_or_default()
            };
            Some((prop_name.to_string(), prop_value))
        })
        .collect();
    properties.sort();

    // Add disambiguator as a property if present
    if !disambiguator.is_empty() {
        properties.push(("Disambiguator".to_string(), disambiguator));
        properties.sort();
    }

    CanonicalElement {
        element_type,
        name: String::new(),
        properties,
        relationships: Vec::new(),
        children: Vec::new(),
    }
}

/// Serialize the Header element to canonical XML
fn serialize_header(header: &roxmltree::Node, output: &mut String, indent: usize) {
    let indent_str = "  ".repeat(indent);
    output.push_str(&format!("{}<Header>\n", indent_str));

    // Collect CustomData elements and sort by Category
    let mut custom_data: Vec<_> = header
        .children()
        .filter(|n| n.has_tag_name("CustomData"))
        .map(|n| {
            let category = n.attribute("Category").unwrap_or("");
            let type_attr = n.attribute("Type");
            let metadata: Vec<_> = n
                .children()
                .filter(|m| m.has_tag_name("Metadata"))
                .map(|m| {
                    (
                        m.attribute("Name").unwrap_or("").to_string(),
                        m.attribute("Value").unwrap_or("").to_string(),
                    )
                })
                .collect();
            (
                category.to_string(),
                type_attr.map(|s| s.to_string()),
                metadata,
            )
        })
        .collect();

    custom_data.sort_by(|a, b| a.0.cmp(&b.0));

    for (category, type_attr, metadata) in custom_data {
        let type_str = type_attr
            .map(|t| format!(" Type=\"{}\"", t))
            .unwrap_or_default();

        if metadata.is_empty() {
            output.push_str(&format!(
                "{}  <CustomData Category=\"{}\"{} />\n",
                indent_str, category, type_str
            ));
        } else {
            output.push_str(&format!(
                "{}  <CustomData Category=\"{}\"{}>\n",
                indent_str, category, type_str
            ));
            for (name, value) in metadata {
                output.push_str(&format!(
                    "{}    <Metadata Name=\"{}\" Value=\"{}\" />\n",
                    indent_str, name, value
                ));
            }
            output.push_str(&format!("{}  </CustomData>\n", indent_str));
        }
    }

    output.push_str(&format!("{}</Header>\n", indent_str));
}

/// Serialize a canonical element to XML
fn serialize_canonical_element(element: &CanonicalElement, output: &mut String, indent: usize) {
    let indent_str = "  ".repeat(indent);

    // Build opening tag
    let name_attr = if element.name.is_empty() {
        String::new()
    } else {
        format!(" Name=\"{}\"", escape_xml_attr(&element.name))
    };

    // Check if element is empty (no properties, relationships, or children)
    let is_empty = element.properties.is_empty()
        && element.relationships.is_empty()
        && element.children.is_empty();

    if is_empty {
        output.push_str(&format!(
            "{}<Element Type=\"{}\"{} />\n",
            indent_str, element.element_type, name_attr
        ));
        return;
    }

    output.push_str(&format!(
        "{}<Element Type=\"{}\"{}>\n",
        indent_str, element.element_type, name_attr
    ));

    // Serialize properties
    for (prop_name, prop_value) in &element.properties {
        // Check if value needs CDATA (contains special chars or is multi-line)
        if needs_cdata(prop_value) {
            output.push_str(&format!(
                "{}  <Property Name=\"{}\">\n",
                indent_str, prop_name
            ));
            output.push_str(&format!(
                "{}    <Value><![CDATA[{}]]></Value>\n",
                indent_str, prop_value
            ));
            output.push_str(&format!("{}  </Property>\n", indent_str));
        } else {
            output.push_str(&format!(
                "{}  <Property Name=\"{}\" Value=\"{}\" />\n",
                indent_str,
                prop_name,
                escape_xml_attr(prop_value)
            ));
        }
    }

    // Serialize relationships
    for rel in &element.relationships {
        serialize_canonical_relationship(rel, output, indent + 1);
    }

    // Serialize child annotations
    for child in &element.children {
        serialize_canonical_annotation(child, output, indent + 1);
    }

    output.push_str(&format!("{}</Element>\n", indent_str));
}

/// Serialize a canonical relationship to XML
fn serialize_canonical_relationship(
    rel: &CanonicalRelationship,
    output: &mut String,
    indent: usize,
) {
    let indent_str = "  ".repeat(indent);

    // Check if relationship is empty
    if rel.references.is_empty() && rel.entries.is_empty() {
        output.push_str(&format!(
            "{}<Relationship Name=\"{}\" />\n",
            indent_str, rel.name
        ));
        return;
    }

    output.push_str(&format!(
        "{}<Relationship Name=\"{}\">\n",
        indent_str, rel.name
    ));

    // Serialize references
    for (ref_name, external_source) in &rel.references {
        let ext_attr = external_source
            .as_ref()
            .map(|s| format!(" ExternalSource=\"{}\"", s))
            .unwrap_or_default();
        output.push_str(&format!("{}  <Entry>\n", indent_str));
        output.push_str(&format!(
            "{}    <References{} Name=\"{}\" />\n",
            indent_str,
            ext_attr,
            escape_xml_attr(ref_name)
        ));
        output.push_str(&format!("{}  </Entry>\n", indent_str));
    }

    // Serialize entry elements
    for entry in &rel.entries {
        output.push_str(&format!("{}  <Entry>\n", indent_str));
        serialize_canonical_element(entry, output, indent + 2);
        output.push_str(&format!("{}  </Entry>\n", indent_str));
    }

    output.push_str(&format!("{}</Relationship>\n", indent_str));
}

/// Serialize a canonical annotation to XML
fn serialize_canonical_annotation(element: &CanonicalElement, output: &mut String, indent: usize) {
    let indent_str = "  ".repeat(indent);

    // Check for Disambiguator in properties
    let disambiguator = element
        .properties
        .iter()
        .find(|(k, _)| k == "Disambiguator")
        .map(|(_, v)| v.clone());

    // Filter out Disambiguator from serialized properties
    let props_without_disambiguator: Vec<_> = element
        .properties
        .iter()
        .filter(|(k, _)| k != "Disambiguator")
        .collect();

    let disamb_attr = disambiguator
        .map(|d| format!(" Disambiguator=\"{}\"", d))
        .unwrap_or_default();

    if props_without_disambiguator.is_empty() {
        output.push_str(&format!(
            "{}<Annotation Type=\"{}\"{} />\n",
            indent_str, element.element_type, disamb_attr
        ));
    } else {
        output.push_str(&format!(
            "{}<Annotation Type=\"{}\"{}>\n",
            indent_str, element.element_type, disamb_attr
        ));
        for (prop_name, prop_value) in props_without_disambiguator {
            if needs_cdata(prop_value) {
                output.push_str(&format!(
                    "{}  <Property Name=\"{}\">\n",
                    indent_str, prop_name
                ));
                output.push_str(&format!(
                    "{}    <Value><![CDATA[{}]]></Value>\n",
                    indent_str, prop_value
                ));
                output.push_str(&format!("{}  </Property>\n", indent_str));
            } else {
                output.push_str(&format!(
                    "{}  <Property Name=\"{}\" Value=\"{}\" />\n",
                    indent_str,
                    prop_name,
                    escape_xml_attr(prop_value)
                ));
            }
        }
        output.push_str(&format!("{}</Annotation>\n", indent_str));
    }
}

/// Check if a value needs CDATA encoding
fn needs_cdata(value: &str) -> bool {
    value.contains('\n') || value.contains('<') || value.contains('>') || value.contains("]]>")
}

/// Escape special characters for XML attribute values
fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Compare two canonicalized XML strings and return differences.
///
/// This performs a line-by-line comparison of canonicalized XML, reporting:
/// - The first line that differs between the two outputs
/// - Line count mismatches if the outputs have different lengths
///
/// # Arguments
/// * `rust_canonical` - Canonicalized XML from Rust implementation
/// * `dotnet_canonical` - Canonicalized XML from DotNet implementation
///
/// # Returns
/// A vector of `CanonicalXmlError` describing the differences.
pub fn compare_canonical_xml(
    rust_canonical: &str,
    dotnet_canonical: &str,
) -> Vec<CanonicalXmlError> {
    let mut errors = Vec::new();

    let rust_lines: Vec<&str> = rust_canonical.lines().collect();
    let dotnet_lines: Vec<&str> = dotnet_canonical.lines().collect();

    // Check line counts
    if rust_lines.len() != dotnet_lines.len() {
        errors.push(CanonicalXmlError::LineCountMismatch {
            rust_lines: rust_lines.len(),
            dotnet_lines: dotnet_lines.len(),
        });
    }

    // Compare line by line
    let min_len = rust_lines.len().min(dotnet_lines.len());
    for i in 0..min_len {
        if rust_lines[i] != dotnet_lines[i] {
            errors.push(CanonicalXmlError::ContentMismatch {
                line_number: i + 1,
                rust_line: rust_lines[i].to_string(),
                dotnet_line: dotnet_lines[i].to_string(),
            });
            // Only report first 10 differences to avoid overwhelming output
            if errors.len() >= 10 {
                break;
            }
        }
    }

    errors
}

/// Generate a unified diff between two strings.
///
/// Returns a string showing additions (+), deletions (-), and context lines.
/// This is useful for debugging canonical XML comparison failures.
///
/// # Arguments
/// * `rust_content` - Content from Rust implementation
/// * `dotnet_content` - Content from DotNet implementation
/// * `context_lines` - Number of context lines to show around differences (default: 3)
///
/// # Returns
/// A unified diff string, or empty string if contents are identical.
pub fn generate_diff(rust_content: &str, dotnet_content: &str, context_lines: usize) -> String {
    let rust_lines: Vec<&str> = rust_content.lines().collect();
    let dotnet_lines: Vec<&str> = dotnet_content.lines().collect();

    if rust_lines == dotnet_lines {
        return String::new();
    }

    let mut output = String::new();
    output.push_str("--- Rust (actual)\n");
    output.push_str("+++ DotNet (expected)\n");

    // Find differences using a simple LCS-based approach
    let diff_ranges = find_diff_ranges(&rust_lines, &dotnet_lines);

    for (rust_start, rust_end, dotnet_start, dotnet_end) in diff_ranges {
        // Calculate context range
        let ctx_start_rust = rust_start.saturating_sub(context_lines);
        let ctx_end_rust = (rust_end + context_lines).min(rust_lines.len());
        let ctx_start_dotnet = dotnet_start.saturating_sub(context_lines);
        let ctx_end_dotnet = (dotnet_end + context_lines).min(dotnet_lines.len());

        output.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            ctx_start_rust + 1,
            ctx_end_rust - ctx_start_rust,
            ctx_start_dotnet + 1,
            ctx_end_dotnet - ctx_start_dotnet
        ));

        // Output context before
        for i in ctx_start_rust..rust_start {
            output.push_str(&format!(" {}\n", rust_lines[i]));
        }

        // Output deleted lines (from Rust)
        for i in rust_start..rust_end {
            output.push_str(&format!("-{}\n", rust_lines[i]));
        }

        // Output added lines (from DotNet)
        for i in dotnet_start..dotnet_end {
            output.push_str(&format!("+{}\n", dotnet_lines[i]));
        }

        // Output context after
        for i in rust_end..ctx_end_rust {
            output.push_str(&format!(" {}\n", rust_lines[i]));
        }
    }

    output
}

/// Find ranges of differences between two line vectors.
/// Returns vector of (rust_start, rust_end, dotnet_start, dotnet_end) tuples.
fn find_diff_ranges(
    rust_lines: &[&str],
    dotnet_lines: &[&str],
) -> Vec<(usize, usize, usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    let mut j = 0;

    while i < rust_lines.len() || j < dotnet_lines.len() {
        // Skip matching lines
        while i < rust_lines.len() && j < dotnet_lines.len() && rust_lines[i] == dotnet_lines[j] {
            i += 1;
            j += 1;
        }

        if i >= rust_lines.len() && j >= dotnet_lines.len() {
            break;
        }

        // Found a difference - find the extent
        let rust_diff_start = i;
        let dotnet_diff_start = j;

        // Try to find where they sync up again
        let mut found_sync = false;
        for look_ahead in 1..=20 {
            // Check if Rust catches up to DotNet
            if i + look_ahead < rust_lines.len() && j < dotnet_lines.len() {
                if rust_lines[i + look_ahead] == dotnet_lines[j] {
                    ranges.push((rust_diff_start, i + look_ahead, dotnet_diff_start, j));
                    i += look_ahead;
                    found_sync = true;
                    break;
                }
            }
            // Check if DotNet catches up to Rust
            if j + look_ahead < dotnet_lines.len() && i < rust_lines.len() {
                if dotnet_lines[j + look_ahead] == rust_lines[i] {
                    ranges.push((rust_diff_start, i, dotnet_diff_start, j + look_ahead));
                    j += look_ahead;
                    found_sync = true;
                    break;
                }
            }
        }

        if !found_sync {
            // Can't find sync point - treat rest as different
            ranges.push((
                rust_diff_start,
                rust_lines.len(),
                dotnet_diff_start,
                dotnet_lines.len(),
            ));
            break;
        }
    }

    ranges
}

/// Compute SHA256 checksum of a string.
///
/// # Arguments
/// * `content` - The content to hash
///
/// # Returns
/// A lowercase hex string representation of the SHA256 hash.
pub fn compute_sha256(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Compare two dacpacs using canonical XML comparison.
///
/// This function:
/// 1. Extracts model.xml from both dacpacs
/// 2. Canonicalizes both XML documents
/// 3. Compares the canonicalized output
/// 4. Optionally computes and compares SHA256 checksums
///
/// # Arguments
/// * `rust_dacpac` - Path to the Rust-generated dacpac
/// * `dotnet_dacpac` - Path to the DotNet-generated dacpac
/// * `include_checksum` - Whether to include SHA256 checksum comparison
///
/// # Returns
/// A vector of `CanonicalXmlError` describing any differences found.
pub fn compare_canonical_dacpacs(
    rust_dacpac: &Path,
    dotnet_dacpac: &Path,
    include_checksum: bool,
) -> Result<Vec<CanonicalXmlError>, String> {
    // Extract model.xml from both dacpacs
    let rust_xml = extract_model_xml(rust_dacpac)?;
    let dotnet_xml = extract_model_xml(dotnet_dacpac)?;

    // Canonicalize both
    let rust_canonical = canonicalize_model_xml(&rust_xml)?;
    let dotnet_canonical = canonicalize_model_xml(&dotnet_xml)?;

    // Compare canonicalized output
    let mut errors = compare_canonical_xml(&rust_canonical, &dotnet_canonical);

    // Add checksum comparison if requested
    if include_checksum && errors.is_empty() {
        let rust_checksum = compute_sha256(&rust_canonical);
        let dotnet_checksum = compute_sha256(&dotnet_canonical);

        if rust_checksum != dotnet_checksum {
            errors.push(CanonicalXmlError::ChecksumMismatch {
                rust_checksum,
                dotnet_checksum,
            });
        }
    }

    Ok(errors)
}

// =============================================================================
// Layer 3: SqlPackage DeployReport
// =============================================================================

/// Check if SqlPackage is available
pub fn sqlpackage_available() -> bool {
    Command::new("sqlpackage")
        .arg("/Version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Compare dacpacs using SqlPackage DeployReport
/// This generates a deployment script from source to target - if empty, they're equivalent
pub fn compare_with_sqlpackage(source_dacpac: &Path, target_dacpac: &Path) -> Layer3Result {
    if !sqlpackage_available() {
        return Layer3Result {
            has_differences: false,
            deploy_script: String::new(),
            error: Some("SqlPackage not available".to_string()),
        };
    }

    // Generate deploy report: what changes would be needed to go from target to source?
    let output = Command::new("sqlpackage")
        .arg("/Action:Script")
        .arg(format!("/SourceFile:{}", source_dacpac.display()))
        .arg(format!("/TargetFile:{}", target_dacpac.display()))
        .arg("/OutputPath:/dev/stdout")
        .arg("/p:IncludeTransactionalScripts=false")
        .arg("/p:CommentOutSetVarDeclarations=true")
        .output();

    match output {
        Ok(result) => {
            let script = String::from_utf8_lossy(&result.stdout).to_string();
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();

            if !result.status.success() {
                return Layer3Result {
                    has_differences: true,
                    deploy_script: script,
                    error: Some(stderr),
                };
            }

            // Check if script contains actual schema changes
            let has_changes = script_has_schema_changes(&script);

            Layer3Result {
                has_differences: has_changes,
                deploy_script: script,
                error: None,
            }
        }
        Err(e) => Layer3Result {
            has_differences: false,
            deploy_script: String::new(),
            error: Some(format!("Failed to run SqlPackage: {}", e)),
        },
    }
}

/// Check if a deployment script contains actual schema changes
fn script_has_schema_changes(script: &str) -> bool {
    // Filter out comments and empty lines
    let significant_lines: Vec<_> = script
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with("--"))
        .filter(|l| !l.starts_with("/*"))
        .filter(|l| !l.starts_with("PRINT"))
        .filter(|l| !l.starts_with("GO"))
        .filter(|l| !l.starts_with(":")) // SQLCMD variables
        .filter(|l| !l.starts_with("SET "))
        .filter(|l| !l.starts_with("USE "))
        .collect();

    // Look for actual DDL statements
    significant_lines.iter().any(|l| {
        let upper = l.to_uppercase();
        upper.starts_with("CREATE ")
            || upper.starts_with("ALTER ")
            || upper.starts_with("DROP ")
            || upper.starts_with("EXEC ")
    })
}

// =============================================================================
// Full Comparison
// =============================================================================

/// Perform full layered comparison of two dacpacs
pub fn compare_dacpacs(
    rust_dacpac: &Path,
    dotnet_dacpac: &Path,
    include_layer3: bool,
) -> Result<ComparisonResult, String> {
    let options = ComparisonOptions {
        include_layer3,
        strict_properties: false,
        check_relationships: false,
        check_element_order: false,
        check_metadata_files: false,
        check_deploy_scripts: false,
    };
    compare_dacpacs_with_options(rust_dacpac, dotnet_dacpac, &options)
}

/// Perform full layered comparison of two dacpacs with configurable options.
///
/// Options:
/// - `include_layer3`: Run SqlPackage DeployReport comparison
/// - `strict_properties`: Compare ALL properties (not just key properties)
/// - `check_relationships`: Validate all relationships between elements
/// - `check_element_order`: Validate element ordering matches DotNet output (Phase 4)
/// - `check_metadata_files`: Compare metadata files like [Content_Types].xml (Phase 5)
/// - `check_deploy_scripts`: Compare pre/post-deploy scripts (Phase 5.4)
pub fn compare_dacpacs_with_options(
    rust_dacpac: &Path,
    dotnet_dacpac: &Path,
    options: &ComparisonOptions,
) -> Result<ComparisonResult, String> {
    let rust_model = DacpacModel::from_dacpac(rust_dacpac)?;
    let dotnet_model = DacpacModel::from_dacpac(dotnet_dacpac)?;

    let layer1_errors = compare_element_inventory(&rust_model, &dotnet_model);

    let layer2_errors = if options.strict_properties {
        compare_all_properties(&rust_model, &dotnet_model)
    } else {
        compare_element_properties(&rust_model, &dotnet_model)
    };

    let relationship_errors = if options.check_relationships {
        compare_element_relationships(&rust_model, &dotnet_model)
    } else {
        Vec::new()
    };

    let layer4_errors = if options.check_element_order {
        compare_element_order(&rust_model, &dotnet_model)
    } else {
        Vec::new()
    };

    // Phase 5: Metadata file comparison
    // Use unified compare_dacpac_files() when both options are enabled,
    // otherwise use individual comparison functions as needed
    let metadata_errors = if options.check_metadata_files && options.check_deploy_scripts {
        // Use unified function for complete metadata comparison
        compare_dacpac_files(rust_dacpac, dotnet_dacpac)
    } else {
        let mut errors = if options.check_metadata_files {
            let mut errs = compare_content_types(rust_dacpac, dotnet_dacpac);
            errs.extend(compare_dac_metadata(rust_dacpac, dotnet_dacpac));
            errs.extend(compare_origin_xml(rust_dacpac, dotnet_dacpac));
            errs
        } else {
            Vec::new()
        };

        // Add deploy script comparison if enabled (Phase 5.4)
        if options.check_deploy_scripts {
            errors.extend(compare_deploy_scripts(rust_dacpac, dotnet_dacpac));
        }
        errors
    };

    let layer3_result = if options.include_layer3 {
        Some(compare_with_sqlpackage(rust_dacpac, dotnet_dacpac))
    } else {
        None
    };

    Ok(ComparisonResult {
        layer1_errors,
        layer2_errors,
        relationship_errors,
        layer4_errors,
        metadata_errors,
        layer3_result,
    })
}

// =============================================================================
// Display / Reporting
// =============================================================================

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
