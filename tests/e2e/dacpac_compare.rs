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
}

/// Parsed [Content_Types].xml structure
#[derive(Debug, Default)]
pub struct ContentTypesXml {
    /// Map of file extension to MIME content type
    /// e.g., "xml" -> "text/xml", "sql" -> "text/plain"
    pub types: HashMap<String, String>,
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

    let metadata_errors = if options.check_metadata_files {
        compare_content_types(rust_dacpac, dotnet_dacpac)
    } else {
        Vec::new()
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

        // Phase 5: Metadata Files Comparison
        if !self.metadata_errors.is_empty() {
            println!("Phase 5: Metadata Files ([Content_Types].xml)");
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
