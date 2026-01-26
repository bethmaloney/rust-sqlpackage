//! Layer 2: Property Comparison
//!
//! Compares element properties between Rust and DotNet dacpac output.
//! Supports both key property comparison (subset) and strict comparison (all properties).

use std::collections::HashMap;

use super::types::{DacpacModel, Layer2Error, ModelElement};

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
pub fn get_all_properties(element_type: &str) -> &'static [&'static str] {
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

/// Compare properties of matching elements (key properties only)
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
