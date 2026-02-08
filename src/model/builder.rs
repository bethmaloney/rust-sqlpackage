//! Build database model from parsed SQL statements
//!
//! Performance note (Phase 16.3.1): This module uses borrowed references where possible
//! to reduce String cloning. Schema names are tracked as &str during iteration and only
//! converted to owned Strings at the end when creating SchemaElements.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use sqlparser::ast::{
    Action, AlterRoleOperation, BinaryOperator, ColumnDef, ColumnOption, DataType, Expr,
    GrantObjects, ObjectName, Privileges, SchemaName, Statement, TableConstraint,
};

use crate::parser::{
    ident_extract,
    identifier_utils::normalize_identifier,
    index_parser::{extract_index_filter_predicate_tokenized, extract_index_is_padded},
    ExtractedExtendedProperty, ExtractedFullTextColumn, ExtractedFunctionParameter,
    ExtractedTableColumn, ExtractedTableConstraint, ExtractedTableTypeColumn,
    ExtractedTableTypeConstraint, FallbackFunctionType, FallbackStatementType, ParsedStatement,
    BINARY_MAX_SENTINEL,
};
use crate::project::SqlProject;

use super::{
    ColumnElement, ColumnstoreIndexElement, ConstraintColumn, ConstraintElement, ConstraintType,
    DataCompressionType, DatabaseModel, ExtendedPropertyElement, FilegroupElement,
    FullTextCatalogElement, FullTextColumnElement, FullTextIndexElement, FunctionElement,
    FunctionType, IndexColumn, IndexElement, ModelElement, ParameterElement,
    PartitionFunctionElement, PartitionSchemeElement, PermissionElement, ProcedureElement,
    RawElement, RoleElement, RoleMembershipElement, ScalarTypeElement, SchemaElement,
    SequenceElement, SynonymElement, TableElement, TableTypeColumnElement, TableTypeConstraint,
    TriggerElement, UserDefinedTypeElement, UserElement, ViewElement,
};

/// Static schema name for "dbo" - avoids allocation for the most common schema
const DBO_SCHEMA: &str = "dbo";

/// Case-insensitive substring search without allocating an uppercase copy.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    let needle_bytes = needle.as_bytes();
    let haystack_bytes = haystack.as_bytes();
    if needle_bytes.len() > haystack_bytes.len() {
        return false;
    }
    haystack_bytes
        .windows(needle_bytes.len())
        .any(|window| window.eq_ignore_ascii_case(needle_bytes))
}

/// Case-insensitive starts_with check without allocating.
fn starts_with_ci(haystack: &str, needle: &str) -> bool {
    haystack.as_bytes().len() >= needle.as_bytes().len()
        && haystack.as_bytes()[..needle.len()].eq_ignore_ascii_case(needle.as_bytes())
}

/// Case-insensitive find — returns byte offset of first occurrence of `needle` in `haystack`.
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let needle_bytes = needle.as_bytes();
    let haystack_bytes = haystack.as_bytes();
    if needle_bytes.len() > haystack_bytes.len() {
        return None;
    }
    haystack_bytes
        .windows(needle_bytes.len())
        .position(|window| window.eq_ignore_ascii_case(needle_bytes))
}

/// Parse a data compression string into the corresponding enum type (case-insensitive).
fn parse_data_compression(s: &str) -> Option<DataCompressionType> {
    if s.eq_ignore_ascii_case("NONE") {
        Some(DataCompressionType::None)
    } else if s.eq_ignore_ascii_case("ROW") {
        Some(DataCompressionType::Row)
    } else if s.eq_ignore_ascii_case("PAGE") {
        Some(DataCompressionType::Page)
    } else if s.eq_ignore_ascii_case("COLUMNSTORE") {
        Some(DataCompressionType::Columnstore)
    } else if s.eq_ignore_ascii_case("COLUMNSTORE_ARCHIVE") {
        Some(DataCompressionType::ColumnstoreArchive)
    } else {
        None
    }
}

// Cached regex patterns (Phase 63) — compiled once, reused on every call
static TYPE_PARAMS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\((\d+)(?:\s*,\s*(\d+))?\)").unwrap());
static PERIOD_COLUMNS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)PERIOD\s+FOR\s+SYSTEM_TIME\s*\(\s*\[?(\w+)\]?\s*,\s*\[?(\w+)\]?\s*\)").unwrap()
});
static SYSTEM_VERSIONING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)SYSTEM_VERSIONING\s*=\s*ON").unwrap());
static HISTORY_TABLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)HISTORY_TABLE\s*=\s*\[?(\w+)\]?\.\[?(\w+)\]?").unwrap());
static GENERATED_ALWAYS_START_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\[?(\w+)\]?\s+DATETIME2(?:\(\d+\))?\s+GENERATED\s+ALWAYS\s+AS\s+ROW\s+START")
        .unwrap()
});
static GENERATED_ALWAYS_END_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\[?(\w+)\]?\s+DATETIME2(?:\(\d+\))?\s+GENERATED\s+ALWAYS\s+AS\s+ROW\s+END")
        .unwrap()
});
static HIDDEN_COLUMNS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\[?(\w+)\]?\s+DATETIME2(?:\(\d+\))?\s+GENERATED\s+ALWAYS\s+AS\s+ROW\s+(?:START|END)\s+HIDDEN").unwrap()
});

/// Type alias for constraint tracking: maps table_element_index to Vec<(element_index, is_inline, emit_name, source_order)>
type TableConstraintMap = HashMap<usize, Vec<(usize, bool, bool, u32)>>;

/// Builder for creating `ConstraintElement` instances with common defaults.
///
/// Reduces boilerplate by providing sensible defaults for the many fields that
/// are typically None, empty, or determined post-processing. Type-specific
/// fields (referenced_table, is_clustered, etc.) are set via builder methods.
struct ConstraintBuilder {
    name: String,
    table_schema: String,
    table_name: String,
    constraint_type: ConstraintType,
    columns: Vec<ConstraintColumn>,
    definition: Option<String>,
    referenced_table: Option<String>,
    referenced_columns: Option<Vec<String>>,
    is_clustered: Option<bool>,
    is_inline: bool,
    emit_name: bool,
    source_order: u32,
}

impl ConstraintBuilder {
    /// Create a new constraint builder with the required fields.
    fn new(
        name: String,
        table_schema: String,
        table_name: String,
        constraint_type: ConstraintType,
        columns: Vec<ConstraintColumn>,
    ) -> Self {
        Self {
            name,
            table_schema,
            table_name,
            constraint_type,
            columns,
            definition: None,
            referenced_table: None,
            referenced_columns: None,
            is_clustered: None,
            is_inline: false,
            emit_name: true, // Default for table-level constraints
            source_order: 0, // Set by caller
        }
    }

    /// Set the source order (order constraint appears in CREATE TABLE statement).
    fn source_order(mut self, order: u32) -> Self {
        self.source_order = order;
        self
    }

    /// Create an inline (column-level) constraint.
    fn inline(mut self, emit_name: bool) -> Self {
        self.is_inline = true;
        self.emit_name = emit_name;
        self
    }

    /// Set the constraint definition (for DEFAULT and CHECK constraints).
    fn definition(mut self, def: String) -> Self {
        self.definition = Some(def);
        self
    }

    /// Set the referenced table and columns (for FOREIGN KEY constraints).
    fn foreign_key_refs(mut self, table: String, columns: Vec<String>) -> Self {
        self.referenced_table = Some(table);
        self.referenced_columns = Some(columns);
        self
    }

    /// Set whether the constraint is clustered (for PRIMARY KEY and UNIQUE).
    fn clustered(mut self, is_clustered: bool) -> Self {
        self.is_clustered = Some(is_clustered);
        self
    }

    /// Set optional clustering (for PRIMARY KEY and UNIQUE).
    fn clustered_opt(mut self, is_clustered: Option<bool>) -> Self {
        self.is_clustered = is_clustered;
        self
    }

    /// Build the final `ConstraintElement`.
    fn build(self) -> ConstraintElement {
        ConstraintElement {
            name: self.name,
            table_schema: self.table_schema,
            table_name: self.table_name,
            constraint_type: self.constraint_type,
            columns: self.columns,
            definition: self.definition,
            referenced_table: self.referenced_table,
            referenced_columns: self.referenced_columns,
            is_clustered: self.is_clustered,
            is_inline: self.is_inline,
            inline_constraint_disambiguator: None, // Set by assign_inline_constraint_disambiguators
            uses_annotation: false,                // Set by assign_inline_constraint_disambiguators
            emit_name: self.emit_name,
            source_order: self.source_order,
        }
    }
}

/// Track a schema name, avoiding allocation if it already exists in the set.
/// Returns a clone of the schema name for use in struct fields.
#[inline]
fn track_schema(schemas: &mut BTreeSet<Cow<'static, str>>, schema: &str) -> String {
    // Check if schema is the common "dbo" case - use static reference
    if schema.eq_ignore_ascii_case(DBO_SCHEMA) {
        if !schemas.contains(DBO_SCHEMA) {
            schemas.insert(Cow::Borrowed(DBO_SCHEMA));
        }
        DBO_SCHEMA.to_string()
    } else if !schemas.contains(schema) {
        // Only allocate for schemas we haven't seen
        let owned = schema.to_string();
        schemas.insert(Cow::Owned(owned.clone()));
        owned
    } else {
        // Schema already tracked, just clone for struct
        schema.to_string()
    }
}

/// Build a database model from parsed statements
pub fn build_model(statements: &[ParsedStatement], project: &SqlProject) -> Result<DatabaseModel> {
    let mut model = DatabaseModel::new();
    // Use Cow<str> to avoid cloning for common schema patterns
    let mut schemas: BTreeSet<Cow<'static, str>> = BTreeSet::new();

    // Always include dbo schema - use borrowed static string
    schemas.insert(Cow::Borrowed(DBO_SCHEMA));

    for parsed in statements {
        // Handle fallback-parsed statements (procedures and functions with T-SQL syntax)
        if let Some(fallback) = &parsed.fallback_type {
            match fallback {
                FallbackStatementType::Procedure { schema, name } => {
                    let schema_owned = track_schema(&mut schemas, schema);
                    let is_natively_compiled = is_natively_compiled(&parsed.sql_text);
                    model.add_element(ModelElement::Procedure(ProcedureElement {
                        schema: schema_owned,
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        parameters: vec![], // T-SQL params not extracted - stored in definition
                        is_natively_compiled,
                        dynamic_sources: Vec::new(),
                    }));
                }
                FallbackStatementType::Function {
                    schema,
                    name,
                    function_type,
                    parameters,
                    return_type,
                } => {
                    let schema_owned = track_schema(&mut schemas, schema);
                    let func_type = match function_type {
                        FallbackFunctionType::Scalar => FunctionType::Scalar,
                        FallbackFunctionType::TableValued => FunctionType::TableValued,
                        FallbackFunctionType::InlineTableValued => FunctionType::InlineTableValued,
                    };
                    let is_natively_compiled = is_natively_compiled(&parsed.sql_text);
                    let param_elements = parameters.iter().map(param_from_extracted).collect();
                    model.add_element(ModelElement::Function(FunctionElement {
                        schema: schema_owned,
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        function_type: func_type,
                        parameters: param_elements,
                        return_type: return_type.clone(),
                        is_natively_compiled,
                        dynamic_sources: Vec::new(),
                    }));
                }
                FallbackStatementType::Index {
                    name,
                    table_schema,
                    table_name,
                    columns,
                    include_columns,
                    is_unique,
                    is_clustered,
                    fill_factor,
                    filter_predicate,
                    data_compression,
                    is_padded,
                } => {
                    // Convert string data_compression to DataCompressionType
                    let compression_type = data_compression
                        .as_ref()
                        .and_then(|s| parse_data_compression(s));
                    // Convert ParsedIndexColumn to IndexColumn
                    let index_columns: Vec<IndexColumn> = columns
                        .iter()
                        .map(|c| IndexColumn::with_direction(c.name.clone(), c.is_descending))
                        .collect();
                    model.add_element(ModelElement::Index(IndexElement {
                        name: name.clone(),
                        table_schema: table_schema.clone(),
                        table_name: table_name.clone(),
                        columns: index_columns,
                        include_columns: include_columns.clone(),
                        is_unique: *is_unique,
                        is_clustered: *is_clustered,
                        fill_factor: *fill_factor,
                        filter_predicate: filter_predicate.clone(),
                        data_compression: compression_type,
                        is_padded: *is_padded,
                    }));
                }
                FallbackStatementType::ColumnstoreIndex {
                    name,
                    table_schema,
                    table_name,
                    is_clustered,
                    columns,
                    data_compression,
                    filter_predicate,
                } => {
                    let compression_type = data_compression
                        .as_ref()
                        .and_then(|s| parse_data_compression(s));
                    model.add_element(ModelElement::ColumnstoreIndex(ColumnstoreIndexElement {
                        name: name.clone(),
                        table_schema: table_schema.clone(),
                        table_name: table_name.clone(),
                        is_clustered: *is_clustered,
                        columns: columns.clone(),
                        data_compression: compression_type,
                        filter_predicate: filter_predicate.clone(),
                    }));
                }
                FallbackStatementType::FullTextIndex {
                    table_schema,
                    table_name,
                    columns,
                    key_index,
                    catalog,
                    change_tracking,
                } => {
                    let column_elements: Vec<FullTextColumnElement> =
                        columns.iter().map(fulltext_column_from_extracted).collect();
                    model.add_element(ModelElement::FullTextIndex(FullTextIndexElement {
                        table_schema: table_schema.clone(),
                        table_name: table_name.clone(),
                        columns: column_elements,
                        key_index: key_index.clone(),
                        catalog: catalog.clone(),
                        change_tracking: change_tracking.clone(),
                        disambiguator: None, // Set during post-processing
                    }));
                }
                FallbackStatementType::FullTextCatalog { name, is_default } => {
                    model.add_element(ModelElement::FullTextCatalog(FullTextCatalogElement {
                        name: name.clone(),
                        is_default: *is_default,
                    }));
                }
                FallbackStatementType::Sequence {
                    schema,
                    name,
                    data_type,
                    start_value,
                    increment_value,
                    min_value,
                    max_value,
                    is_cycling,
                    has_no_min_value,
                    has_no_max_value,
                    cache_size,
                } => {
                    let schema_owned = track_schema(&mut schemas, schema);
                    model.add_element(ModelElement::Sequence(SequenceElement {
                        schema: schema_owned,
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        data_type: data_type.clone(),
                        start_value: *start_value,
                        increment_value: *increment_value,
                        min_value: *min_value,
                        max_value: *max_value,
                        is_cycling: *is_cycling,
                        has_no_min_value: *has_no_min_value,
                        has_no_max_value: *has_no_max_value,
                        cache_size: *cache_size,
                    }));
                }
                FallbackStatementType::UserDefinedType {
                    schema,
                    name,
                    columns,
                    constraints,
                } => {
                    let schema_owned = track_schema(&mut schemas, schema);
                    let column_elements: Vec<TableTypeColumnElement> = columns
                        .iter()
                        .map(table_type_column_from_extracted)
                        .collect();
                    let constraint_elements: Vec<TableTypeConstraint> = constraints
                        .iter()
                        .map(table_type_constraint_from_extracted)
                        .collect();
                    model.add_element(ModelElement::UserDefinedType(UserDefinedTypeElement {
                        schema: schema_owned,
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        columns: column_elements,
                        constraints: constraint_elements,
                    }));
                }
                FallbackStatementType::ScalarType {
                    schema,
                    name,
                    base_type,
                    is_nullable,
                    length,
                    precision,
                    scale,
                } => {
                    let schema_owned = track_schema(&mut schemas, schema);
                    model.add_element(ModelElement::ScalarType(ScalarTypeElement {
                        schema: schema_owned,
                        name: name.clone(),
                        base_type: base_type.clone(),
                        is_nullable: *is_nullable,
                        length: *length,
                        precision: *precision,
                        scale: *scale,
                    }));
                }
                FallbackStatementType::Table {
                    schema,
                    name,
                    columns,
                    constraints,
                    is_node,
                    is_edge,
                    system_time_start_column,
                    system_time_end_column,
                    is_system_versioned,
                    history_table_schema,
                    history_table_name,
                } => {
                    let schema_owned = track_schema(&mut schemas, schema);

                    // Convert extracted columns to model columns
                    let model_columns: Vec<ColumnElement> = columns
                        .iter()
                        .map(|c| column_from_fallback_table(c, schema, name))
                        .collect();

                    // Add the table element - clone schema/name for table, keep originals for constraints
                    model.add_element(ModelElement::Table(TableElement {
                        schema: schema_owned.clone(),
                        name: name.clone(),
                        columns: model_columns,
                        is_node: *is_node,
                        is_edge: *is_edge,
                        inline_constraint_disambiguators: Vec::new(), // Set during post-processing
                        attached_annotations_before_annotation: Vec::new(), // Set during post-processing
                        attached_annotations_after_annotation: Vec::new(), // Set during post-processing
                        system_time_start_column: system_time_start_column.clone(),
                        system_time_end_column: system_time_end_column.clone(),
                        is_system_versioned: *is_system_versioned,
                        history_table_schema: history_table_schema.clone(),
                        history_table_name: history_table_name.clone(),
                    }));

                    // Add constraints as separate elements, tracking source order
                    let mut constraint_order: u32 = 0;
                    for constraint in constraints {
                        if let Some(mut constraint_element) =
                            constraint_from_extracted(constraint, &schema_owned, name)
                        {
                            constraint_element.source_order = constraint_order;
                            constraint_order += 1;
                            model.add_element(ModelElement::Constraint(constraint_element));
                        }
                    }

                    // Add inline default constraints from column definitions
                    // DotNet emits Name attribute only when CONSTRAINT [name] appears AFTER NOT NULL.
                    // The emit_default_constraint_name flag tracks this position.
                    for col in columns {
                        if let Some(default_value) = &col.default_value {
                            let constraint_name = col
                                .default_constraint_name
                                .clone()
                                .unwrap_or_else(|| format!("DF_{}_{}", name, col.name));
                            model.add_element(ModelElement::Constraint(
                                ConstraintBuilder::new(
                                    constraint_name,
                                    schema_owned.clone(),
                                    name.clone(),
                                    ConstraintType::Default,
                                    vec![ConstraintColumn::new(col.name.clone())],
                                )
                                .inline(col.emit_default_constraint_name)
                                .definition(default_value.clone())
                                .source_order(constraint_order)
                                .build(),
                            ));
                            constraint_order += 1;
                        }
                    }

                    // Add inline CHECK constraints from column definitions
                    // DotNet emits Name attribute only when CONSTRAINT [name] appears AFTER NOT NULL.
                    for col in columns {
                        if let Some(check_expr) = &col.check_expression {
                            let constraint_name = col
                                .check_constraint_name
                                .clone()
                                .unwrap_or_else(|| format!("CK_{}_{}", name, col.name));
                            model.add_element(ModelElement::Constraint(
                                ConstraintBuilder::new(
                                    constraint_name,
                                    schema_owned.clone(),
                                    name.clone(),
                                    ConstraintType::Check,
                                    vec![ConstraintColumn::new(col.name.clone())],
                                )
                                .inline(col.emit_check_constraint_name)
                                .definition(check_expr.clone())
                                .source_order(constraint_order)
                                .build(),
                            ));
                            constraint_order += 1;
                        }
                    }
                }
                FallbackStatementType::AlterTableAddConstraint {
                    table_schema,
                    table_name,
                    constraint,
                } => {
                    // Add constraint from ALTER TABLE ... ADD CONSTRAINT statement
                    if let Some(constraint_element) =
                        constraint_from_extracted(constraint, table_schema, table_name)
                    {
                        model.add_element(ModelElement::Constraint(constraint_element));
                    }
                }
                FallbackStatementType::RawStatement {
                    object_type,
                    schema,
                    name,
                } => {
                    let sql_type = if object_type.eq_ignore_ascii_case("TABLE") {
                        Some("SqlTable")
                    } else if object_type.eq_ignore_ascii_case("VIEW") {
                        Some("SqlView")
                    } else {
                        // Skip other object types - they would cause deployment failures
                        // ALTER TABLE, INDEX, FULLTEXT INDEX, etc. are not supported as raw elements
                        // Note: TRIGGER is now handled by FallbackStatementType::Trigger
                        None
                    };
                    if let Some(sql_type) = sql_type {
                        let schema_owned = track_schema(&mut schemas, schema);
                        model.add_element(ModelElement::Raw(RawElement {
                            schema: schema_owned,
                            name: name.clone(),
                            sql_type: sql_type.to_string(),
                            definition: parsed.sql_text.clone(),
                        }));
                    }
                }
                FallbackStatementType::ExtendedProperty { property } => {
                    // Extended properties apply to existing objects (tables, columns, etc.)
                    // Only add if level1 is specified (TABLE, VIEW, etc.)
                    if property.level1name.is_some() {
                        let ext_prop = extended_property_from_extracted(property);
                        model.add_element(ModelElement::ExtendedProperty(ext_prop));
                    }
                }
                FallbackStatementType::Trigger {
                    schema,
                    name,
                    parent_schema,
                    parent_name,
                    is_insert,
                    is_update,
                    is_delete,
                    trigger_type,
                } => {
                    let schema_owned = track_schema(&mut schemas, schema);
                    model.add_element(ModelElement::Trigger(TriggerElement {
                        schema: schema_owned,
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        parent_schema: parent_schema.clone(),
                        parent_name: parent_name.clone(),
                        is_insert_trigger: *is_insert,
                        is_update_trigger: *is_update,
                        is_delete_trigger: *is_delete,
                        trigger_type: *trigger_type,
                    }));
                }
                FallbackStatementType::Filegroup {
                    name,
                    contains_memory_optimized_data,
                } => {
                    // Filegroups are NOT schema-qualified
                    model.add_element(ModelElement::Filegroup(FilegroupElement {
                        name: name.clone(),
                        contains_memory_optimized_data: *contains_memory_optimized_data,
                    }));
                }
                FallbackStatementType::PartitionFunction {
                    name,
                    data_type,
                    boundary_values,
                    is_range_right,
                } => {
                    // Partition functions are NOT schema-qualified
                    model.add_element(ModelElement::PartitionFunction(PartitionFunctionElement {
                        name: name.clone(),
                        data_type: data_type.clone(),
                        boundary_values: boundary_values.clone(),
                        is_range_right: *is_range_right,
                    }));
                }
                FallbackStatementType::PartitionScheme {
                    name,
                    partition_function,
                    filegroups,
                } => {
                    // Partition schemes are NOT schema-qualified
                    model.add_element(ModelElement::PartitionScheme(PartitionSchemeElement {
                        name: name.clone(),
                        partition_function: partition_function.clone(),
                        filegroups: filegroups.clone(),
                    }));
                }
                FallbackStatementType::Synonym {
                    schema,
                    name,
                    target_schema,
                    target_name,
                    target_database,
                    target_server,
                } => {
                    let schema_owned = track_schema(&mut schemas, schema);
                    model.add_element(ModelElement::Synonym(SynonymElement {
                        schema: schema_owned,
                        name: name.clone(),
                        target_schema: target_schema.clone(),
                        target_name: target_name.clone(),
                        target_database: target_database.clone(),
                        target_server: target_server.clone(),
                    }));
                }
                FallbackStatementType::CreateUser {
                    name,
                    auth_type,
                    login,
                    default_schema,
                } => {
                    model.add_element(ModelElement::User(UserElement {
                        name: name.clone(),
                        auth_type: auth_type.clone(),
                        login: login.clone(),
                        default_schema: default_schema.clone(),
                    }));
                }
                FallbackStatementType::CreateRole { name, owner } => {
                    model.add_element(ModelElement::Role(RoleElement {
                        name: name.clone(),
                        owner: owner.clone(),
                    }));
                }
                FallbackStatementType::AlterRoleMembership {
                    role,
                    member,
                    is_add,
                } => {
                    if *is_add {
                        model.add_element(ModelElement::RoleMembership(RoleMembershipElement {
                            role: role.clone(),
                            member: member.clone(),
                        }));
                    }
                    // DROP MEMBER doesn't create an element — it would be handled at deployment time
                }
                FallbackStatementType::Permission {
                    action,
                    permission,
                    target_schema,
                    target_name,
                    target_type,
                    principal,
                    with_grant_option,
                    cascade,
                } => {
                    model.add_element(ModelElement::Permission(PermissionElement {
                        action: action.clone(),
                        permission: permission.clone(),
                        target_schema: target_schema.clone(),
                        target_name: target_name.clone(),
                        target_type: target_type.clone(),
                        principal: principal.clone(),
                        with_grant_option: *with_grant_option,
                        cascade: *cascade,
                    }));
                }
                FallbackStatementType::SkippedSecurityStatement { statement_type: _ } => {
                    // Server-level security statements are silently skipped
                    // LOGIN, CERTIFICATE, ASYMMETRIC_KEY, SYMMETRIC_KEY, CREDENTIAL, etc.
                }
            }
            continue;
        }

        // Handle regular sqlparser-parsed statements
        let Some(statement) = &parsed.statement else {
            continue;
        };

        match statement {
            Statement::CreateTable(create_table) => {
                let (schema, name) =
                    extract_schema_and_name(&create_table.name, &project.default_schema);
                // Track schema - extract_schema_and_name returns owned strings,
                // so we check if already tracked and reuse the existing allocation
                let schema = track_schema(&mut schemas, &schema);

                // Extract temporal table metadata from raw SQL text
                // sqlparser-rs parses temporal tables successfully but doesn't expose
                // PERIOD FOR SYSTEM_TIME or SYSTEM_VERSIONING in its AST, so we
                // extract these from the original SQL text.
                let temporal = extract_temporal_metadata_from_sql(&parsed.sql_text);

                let mut columns: Vec<ColumnElement> = create_table
                    .columns
                    .iter()
                    .map(|c| column_from_def(c, &schema, &name))
                    .collect();

                // Apply temporal column attributes from raw SQL extraction
                for col in &mut columns {
                    if temporal
                        .generated_always_start_columns
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(&col.name))
                    {
                        col.is_generated_always_start = true;
                    }
                    if temporal
                        .generated_always_end_columns
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(&col.name))
                    {
                        col.is_generated_always_end = true;
                    }
                    if temporal
                        .hidden_columns
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(&col.name))
                    {
                        col.is_hidden = true;
                    }
                }

                model.add_element(ModelElement::Table(TableElement {
                    schema: schema.clone(),
                    name: name.clone(),
                    columns,
                    is_node: false,
                    is_edge: false,
                    inline_constraint_disambiguators: Vec::new(), // Set during post-processing
                    attached_annotations_before_annotation: Vec::new(), // Set during post-processing
                    attached_annotations_after_annotation: Vec::new(), // Set during post-processing
                    system_time_start_column: temporal.system_time_start_column,
                    system_time_end_column: temporal.system_time_end_column,
                    is_system_versioned: temporal.is_system_versioned,
                    history_table_schema: temporal.history_table_schema,
                    history_table_name: temporal.history_table_name,
                }));

                // Extract constraints from table definition (table-level constraints)
                // Track source order for disambiguator assignment
                let mut constraint_order: u32 = 0;
                for constraint in &create_table.constraints {
                    if let Some(mut constraint_element) = constraint_from_table_constraint(
                        constraint,
                        &create_table.name,
                        &project.default_schema,
                        &parsed.sql_text,
                    ) {
                        constraint_element.source_order = constraint_order;
                        constraint_order += 1;
                        model.add_element(ModelElement::Constraint(constraint_element));
                    }
                }

                // Extract inline column constraints (PRIMARY KEY, UNIQUE on columns)
                // DotNet emits Name attribute only if constraint has explicit CONSTRAINT [name]
                for col in &create_table.columns {
                    for option in &col.options {
                        if let ColumnOption::Unique { is_primary, .. } = &option.option {
                            let explicit_name = option.name.as_ref().map(|n| n.value.clone());
                            let has_explicit_name = explicit_name.is_some();
                            let constraint_name = explicit_name.unwrap_or_else(|| {
                                if *is_primary {
                                    format!("PK_{}", name)
                                } else {
                                    format!("UQ_{}_{}", name, col.name.value)
                                }
                            });

                            let constraint_type = if *is_primary {
                                ConstraintType::PrimaryKey
                            } else {
                                ConstraintType::Unique
                            };

                            model.add_element(ModelElement::Constraint(
                                ConstraintBuilder::new(
                                    constraint_name,
                                    schema.clone(),
                                    name.clone(),
                                    constraint_type,
                                    vec![ConstraintColumn::new(col.name.value.clone())],
                                )
                                .inline(has_explicit_name)
                                .source_order(constraint_order)
                                .build(),
                            ));
                            constraint_order += 1;
                        }
                    }
                }

                // Extract inline default constraints from column definitions
                // DotNet DacFx treats ALL column-level constraints as inline, regardless
                // of whether they have explicit CONSTRAINT names.
                //
                // In SQL Server, CONSTRAINT [name] applies to the DEFAULT that follows it,
                // regardless of whether NOT NULL appears between them. NOT NULL is a column
                // property, not a nameable constraint. The syntax "CONSTRAINT [name] NOT NULL DEFAULT"
                // names the DEFAULT constraint, not the NOT NULL property.
                //
                // sqlparser may associate the constraint name with the NotNull option or with
                // the Default option depending on the exact syntax. We need to find the constraint
                // name that precedes the DEFAULT in the option list.
                for col in &create_table.columns {
                    // Find any constraint name in the column options that should apply to DEFAULT
                    let mut pending_constraint_name: Option<String> = None;
                    let mut default_option_index: Option<usize> = None;

                    // First pass: find the DEFAULT option and any preceding constraint name
                    for (i, option) in col.options.iter().enumerate() {
                        // Track any constraint name we encounter
                        if option.name.is_some() {
                            pending_constraint_name = option.name.as_ref().map(|n| n.value.clone());
                        }
                        // If this is a DEFAULT option, record its index
                        if matches!(option.option, ColumnOption::Default(_)) {
                            default_option_index = Some(i);
                            break; // Stop at the DEFAULT - we want the preceding constraint name
                        }
                    }

                    // Second pass: extract DEFAULT with the correct constraint name
                    for (i, option) in col.options.iter().enumerate() {
                        if let ColumnOption::Default(expr) = &option.option {
                            // Use the constraint name if:
                            // 1. It's directly on the DEFAULT option, OR
                            // 2. We found a constraint name before the DEFAULT option
                            //
                            // DotNet only emits Name attribute when CONSTRAINT keyword is directly
                            // on the DEFAULT option (NOT NULL CONSTRAINT [name] DEFAULT syntax),
                            // not when CONSTRAINT keyword precedes NOT NULL (CONSTRAINT [name] NOT NULL DEFAULT syntax)
                            let name_directly_on_default =
                                option.name.as_ref().map(|n| n.value.clone());
                            let explicit_name = name_directly_on_default.clone().or_else(|| {
                                // Check if there was a preceding constraint name and this is the DEFAULT
                                if default_option_index == Some(i) {
                                    pending_constraint_name.clone()
                                } else {
                                    None
                                }
                            });

                            // Emit Name attribute ONLY if CONSTRAINT keyword is directly on the DEFAULT option
                            // (i.e., "NOT NULL CONSTRAINT [name] DEFAULT" syntax, not "CONSTRAINT [name] NOT NULL DEFAULT")
                            let has_explicit_name = name_directly_on_default.is_some();
                            let constraint_name = explicit_name
                                .unwrap_or_else(|| format!("DF_{}_{}", name, col.name.value));

                            model.add_element(ModelElement::Constraint(
                                ConstraintBuilder::new(
                                    constraint_name,
                                    schema.clone(),
                                    name.clone(),
                                    ConstraintType::Default,
                                    vec![ConstraintColumn::new(col.name.value.clone())],
                                )
                                .inline(has_explicit_name)
                                .definition(expr.to_string())
                                .source_order(constraint_order)
                                .build(),
                            ));
                            constraint_order += 1;
                        }
                    }
                }

                // Extract inline CHECK constraints from column definitions
                // DotNet emits Name attribute only if constraint has explicit CONSTRAINT [name]
                for col in &create_table.columns {
                    for option in &col.options {
                        if let ColumnOption::Check(expr) = &option.option {
                            let explicit_name = option.name.as_ref().map(|n| n.value.clone());
                            let has_explicit_name = explicit_name.is_some();
                            let constraint_name = explicit_name
                                .unwrap_or_else(|| format!("CK_{}_{}", name, col.name.value));
                            model.add_element(ModelElement::Constraint(
                                ConstraintBuilder::new(
                                    constraint_name,
                                    schema.clone(),
                                    name.clone(),
                                    ConstraintType::Check,
                                    vec![ConstraintColumn::new(col.name.value.clone())],
                                )
                                .inline(has_explicit_name)
                                .definition(expr.to_string())
                                .source_order(constraint_order)
                                .build(),
                            ));
                            constraint_order += 1;
                        }
                    }
                }

                // Add extracted default constraints (from T-SQL DEFAULT FOR syntax)
                // These are always named since they use explicit CONSTRAINT keyword
                for default_constraint in &parsed.extracted_defaults {
                    model.add_element(ModelElement::Constraint(
                        ConstraintBuilder::new(
                            default_constraint.name.clone(),
                            schema.clone(),
                            name.clone(),
                            ConstraintType::Default,
                            vec![ConstraintColumn::new(default_constraint.column.clone())],
                        )
                        .definition(default_constraint.expression.clone())
                        .source_order(constraint_order)
                        .build(),
                    ));
                    constraint_order += 1;
                }
            }

            Statement::CreateView { name, .. } | Statement::AlterView { name, .. } => {
                let (schema, view_name) = extract_schema_and_name(name, &project.default_schema);
                let schema = track_schema(&mut schemas, &schema);

                // Extract view options from raw SQL text
                let (is_schema_bound, is_with_check_option, is_metadata_reported) =
                    extract_view_options(&parsed.sql_text);

                model.add_element(ModelElement::View(ViewElement {
                    schema,
                    name: view_name,
                    definition: parsed.sql_text.clone(),
                    is_schema_bound,
                    is_with_check_option,
                    is_metadata_reported,
                }));
            }

            Statement::CreateIndex(create_index) => {
                let index_name = create_index
                    .name
                    .as_ref()
                    .map(ident_extract::from_object_name)
                    .unwrap_or_else(|| "unnamed_index".to_string());

                let (table_schema, table_name) =
                    extract_schema_and_name(&create_index.table_name, &project.default_schema);

                // Extract columns with sort direction from OrderByExpr
                // asc is Some(true) for ASC, Some(false) for DESC, None for unspecified (default ASC)
                let columns: Vec<IndexColumn> = create_index
                    .columns
                    .iter()
                    .map(|c| {
                        // Use ident_extract to get unbracketed column name
                        let name = ident_extract::column_from_expr(&c.expr)
                            .unwrap_or_else(|| c.expr.to_string());
                        // asc: Some(true) = ASC, Some(false) = DESC, None = default (ASC)
                        let is_descending = c.asc == Some(false);
                        IndexColumn::with_direction(name, is_descending)
                    })
                    .collect();

                // Extract INCLUDE columns if available from sqlparser
                let include_columns: Vec<String> = create_index
                    .include
                    .iter()
                    .map(|c| c.value.clone())
                    .collect();

                // Extract index options from WITH clause
                let fill_factor = extract_fill_factor(&create_index.with);
                let data_compression = extract_data_compression(&create_index.with);

                // Extract filter predicate from raw SQL using token-based parsing (Phase 20.6.2)
                // sqlparser doesn't expose filter predicates directly
                let filter_predicate = extract_index_filter_predicate_tokenized(&parsed.sql_text);

                // Extract PAD_INDEX from raw SQL (sqlparser doesn't expose this)
                let is_padded = extract_index_is_padded(&parsed.sql_text);

                model.add_element(ModelElement::Index(IndexElement {
                    name: index_name,
                    table_schema,
                    table_name,
                    columns,
                    include_columns,
                    is_unique: create_index.unique,
                    is_clustered: false, // sqlparser doesn't expose this directly
                    fill_factor,
                    filter_predicate,
                    data_compression,
                    is_padded,
                }));
            }

            // Handle procedures that sqlparser successfully parsed (generic SQL syntax)
            Statement::CreateProcedure { name, .. } => {
                let (schema, proc_name) = extract_schema_and_name(name, &project.default_schema);
                let schema = track_schema(&mut schemas, &schema);
                let is_native = is_natively_compiled(&parsed.sql_text);

                model.add_element(ModelElement::Procedure(ProcedureElement {
                    schema,
                    name: proc_name,
                    definition: parsed.sql_text.clone(),
                    parameters: vec![], // Parameters stored in definition
                    is_natively_compiled: is_native,
                    dynamic_sources: Vec::new(),
                }));
            }

            // Handle functions that sqlparser successfully parsed (generic SQL syntax)
            Statement::CreateFunction(create_func) => {
                let (schema, func_name) =
                    extract_schema_and_name(&create_func.name, &project.default_schema);
                let schema = track_schema(&mut schemas, &schema);

                // Detect function type from raw SQL (more reliable than parsed return type)
                // Inline TVF: "RETURNS TABLE" without table variable
                // Multi-statement TVF: "RETURNS @variable TABLE (...)"
                let function_type = if contains_ci(&parsed.sql_text, "RETURNS TABLE") {
                    FunctionType::InlineTableValued
                } else if contains_ci(&parsed.sql_text, "RETURNS @") {
                    FunctionType::TableValued
                } else if create_func
                    .return_type
                    .as_ref()
                    .map(|t| contains_ci(&t.to_string(), "TABLE"))
                    .unwrap_or(false)
                {
                    // Fallback: if parsed return type contains TABLE but didn't match above patterns
                    FunctionType::TableValued
                } else {
                    FunctionType::Scalar
                };

                let is_native = is_natively_compiled(&parsed.sql_text);

                model.add_element(ModelElement::Function(FunctionElement {
                    schema,
                    name: func_name,
                    definition: parsed.sql_text.clone(),
                    function_type,
                    parameters: vec![], // Parameters stored in definition
                    return_type: create_func.return_type.as_ref().map(|t| t.to_string()),
                    is_natively_compiled: is_native,
                    dynamic_sources: Vec::new(),
                }));
            }

            Statement::CreateSchema { schema_name, .. } => {
                // Extract schema name and authorization from SchemaName enum
                let (name_obj, authorization) = match schema_name {
                    SchemaName::Simple(name) => (Some(name), None),
                    SchemaName::UnnamedAuthorization(auth) => {
                        // Schema created with just AUTHORIZATION, use auth as name
                        (None, Some(auth.value.clone()))
                    }
                    SchemaName::NamedAuthorization(name, auth) => {
                        (Some(name), Some(auth.value.clone()))
                    }
                };

                // Get the schema name string (use first part if ObjectName)
                let schema_name_str = match name_obj {
                    Some(obj_name) => {
                        // ObjectName is a Vec<Ident>, get the first element
                        obj_name
                            .0
                            .first()
                            .map(|ident| ident.value.clone())
                            .unwrap_or_default()
                    }
                    None => {
                        // If no name specified, use the authorization as the schema name
                        authorization.clone().unwrap_or_default()
                    }
                };

                // Normalize schema name using centralized identifier normalization
                let normalized = normalize_identifier(&schema_name_str);

                let schema_name = track_schema(&mut schemas, &normalized);
                model.add_element(ModelElement::Schema(SchemaElement {
                    name: schema_name,
                    authorization,
                }));
            }

            // Handle EXEC statements (for sp_addextendedproperty)
            Statement::Execute { name, .. } => {
                // Check if this is sp_addextendedproperty (with or without sys. prefix)
                let parts: Vec<_> = name.0.iter().map(|p| p.value.to_lowercase()).collect();
                let is_sp_addextendedproperty = match parts.as_slice() {
                    [proc] => proc == "sp_addextendedproperty",
                    [schema, proc] => schema == "sys" && proc == "sp_addextendedproperty",
                    _ => false,
                };
                if is_sp_addextendedproperty {
                    // Use the original SQL text to extract the extended property
                    if let Some(property) =
                        crate::parser::extract_extended_property_from_sql(&parsed.sql_text)
                    {
                        if property.level1name.is_some() {
                            let ext_prop = extended_property_from_extracted(&property);
                            model.add_element(ModelElement::ExtendedProperty(ext_prop));
                        }
                    }
                }
            }

            // CREATE ROLE (parsed by sqlparser)
            Statement::CreateRole {
                names,
                authorization_owner,
                ..
            } => {
                for name in names {
                    let role_name = name.0.last().map(|i| i.value.clone()).unwrap_or_default();
                    let owner = authorization_owner
                        .as_ref()
                        .map(|o| o.0.last().map(|i| i.value.clone()).unwrap_or_default());
                    model.add_element(ModelElement::Role(RoleElement {
                        name: role_name,
                        owner,
                    }));
                }
            }

            // ALTER ROLE ADD/DROP MEMBER (parsed by sqlparser)
            Statement::AlterRole {
                name,
                operation: AlterRoleOperation::AddMember { member_name },
            } => {
                model.add_element(ModelElement::RoleMembership(RoleMembershipElement {
                    role: name.value.clone(),
                    member: member_name.value.clone(),
                }));
            }
            Statement::AlterRole { .. } => {
                // DROP MEMBER, RenameRole, etc. — not modeled
            }

            // GRANT (parsed by sqlparser)
            Statement::Grant {
                privileges,
                objects,
                grantees,
                with_grant_option,
                ..
            } => {
                let permission_names = extract_privilege_names(privileges);
                let (target_schema, target_name, target_type) = extract_grant_target(objects);
                let principal = extract_grantee_name(grantees);

                for perm_name in permission_names {
                    model.add_element(ModelElement::Permission(PermissionElement {
                        action: "Grant".to_string(),
                        permission: perm_name,
                        target_schema: target_schema.clone(),
                        target_name: target_name.clone(),
                        target_type: target_type.clone(),
                        principal: principal.clone(),
                        with_grant_option: *with_grant_option,
                        cascade: false,
                    }));
                }
            }

            // REVOKE (parsed by sqlparser)
            Statement::Revoke {
                privileges,
                objects,
                grantees,
                cascade,
                ..
            } => {
                let permission_names = extract_privilege_names(privileges);
                let (target_schema, target_name, target_type) = extract_grant_target(objects);
                let principal = extract_grantee_name(grantees);
                let is_cascade = cascade.is_some();

                for perm_name in permission_names {
                    model.add_element(ModelElement::Permission(PermissionElement {
                        action: "Revoke".to_string(),
                        permission: perm_name,
                        target_schema: target_schema.clone(),
                        target_name: target_name.clone(),
                        target_type: target_type.clone(),
                        principal: principal.clone(),
                        with_grant_option: false,
                        cascade: is_cascade,
                    }));
                }
            }

            // Ignore other statements (DML, etc.)
            _ => {}
        }
    }

    // Add schema elements for any schemas we discovered
    // (Built-in schemas like dbo are included in the model but will be filtered
    // during XML generation - they're written as ExternalSource="BuiltIns" references)
    for schema in schemas {
        let schema_str: &str = &schema;
        if !model
            .elements
            .iter()
            .any(|e| matches!(e, ModelElement::Schema(s) if s.name == schema_str))
        {
            model.add_element(ModelElement::Schema(SchemaElement {
                name: schema.into_owned(),
                authorization: None,
            }));
        }
    }

    // Resolve UDT nullability for columns
    // Columns that use a user-defined scalar type and don't have explicit NULL/NOT NULL
    // inherit the nullability from the UDT definition (matching DotNet behavior)
    resolve_udt_nullability(&mut model.elements);

    // Pre-compute and cache element names before sorting.
    // This avoids repeated format!() allocations in full_name()/xml_name_attr()
    // during sorting and later during XML generation.
    model.cache_element_names();

    // Sort elements by type (following DotNet order) then by name for deterministic output
    sort_model(&mut model);

    // Assign disambiguators to inline constraints and link to columns/tables
    // This must happen after sorting because DotNet assigns disambiguators in sorted order.
    // Pass package reference count since DotNet reserves disambiguator slots for package references
    assign_inline_constraint_disambiguators(&mut model.elements, project.package_references.len());

    // Re-cache names after disambiguation may have changed constraint names
    // (disambiguation doesn't change names, but re-caching is cheap insurance)
    model.cache_element_names();

    Ok(model)
}

/// Sort the model's elements and their cached names together by (Name, Type, SecondaryKey)
/// to match DotNet DacFx ordering.
///
/// DotNet sorts elements alphabetically (case-insensitive) by:
/// 1. Name attribute value (empty string for elements without Name attribute)
/// 2. Type attribute value (e.g., "SqlCheckConstraint", "SqlTable")
/// 3. Secondary key for disambiguation (DefiningTable reference for inline constraints)
///
/// Uses pre-computed cached_xml_names from `model.cache_element_names()` to avoid
/// allocating new Strings during sort key computation. Sort keys reference the cached
/// names via index, so no per-element allocation occurs during sorting.
fn sort_model(model: &mut DatabaseModel) {
    use std::cmp::Reverse;

    let n = model.elements.len();

    // Build sort keys from cached names (no new allocations — just lowercase + references)
    let sort_keys: Vec<_> = (0..n)
        .map(|i| {
            let name = model.cached_xml_names[i].to_lowercase();
            let type_name = model.elements[i].type_name().to_lowercase();
            let secondary = model.elements[i].secondary_sort_key().to_lowercase();

            let secondary_desc = if name.is_empty() && !secondary.is_empty() {
                Some(Reverse(secondary))
            } else {
                None
            };

            (name, type_name, secondary_desc)
        })
        .collect();

    // Sort indices by the pre-computed keys
    let mut indices: Vec<usize> = (0..n).collect();
    indices.sort_by(|&a, &b| sort_keys[a].cmp(&sort_keys[b]));

    // Apply permutation by rebuilding vecs in sorted order.
    // This is O(n) extra memory but avoids the complexity of in-place cycle permutation.
    apply_permutation(&mut model.elements, &indices);
    apply_permutation(&mut model.cached_full_names, &indices);
    apply_permutation(&mut model.cached_xml_names, &indices);
}

/// Apply a permutation to a vec by rebuilding it in the new order.
/// After this call, `vec[i] = original_vec[indices[i]]` for all i.
fn apply_permutation<T>(vec: &mut Vec<T>, indices: &[usize]) {
    let n = vec.len();
    debug_assert_eq!(n, indices.len());

    // Move elements out of vec into Options for safe arbitrary-order access
    let mut opts: Vec<Option<T>> = vec.drain(..).map(Some).collect();

    // Rebuild vec in sorted order using the index permutation
    for &idx in indices {
        vec.push(opts[idx].take().expect("index used twice in permutation"));
    }
}

/// Resolve UDT nullability for columns.
///
/// When a column uses a user-defined scalar type (UDT) created with `CREATE TYPE ... FROM`,
/// and the column doesn't have an explicit NULL/NOT NULL constraint, it inherits the
/// nullability from the UDT definition. This matches DotNet DacFx behavior.
///
/// For example:
///   CREATE TYPE [dbo].[PhoneNumber] FROM VARCHAR(20) NOT NULL;
///   CREATE TABLE [dbo].[T] ([Phone] [dbo].[PhoneNumber]);  -- Phone inherits NOT NULL
fn resolve_udt_nullability(elements: &mut [ModelElement]) {
    use std::collections::HashMap;

    // Build a map of UDT names to their nullability
    // Store multiple name formats to handle different reference styles
    let mut udt_nullability: HashMap<String, bool> = HashMap::new();

    for element in elements.iter() {
        if let ModelElement::ScalarType(scalar_type) = element {
            // UDT can be referenced in various formats:
            // - [schema].[name]
            // - schema.name
            // - [schema].name
            // - schema.[name]
            let schema = &scalar_type.schema;
            let name = &scalar_type.name;
            let is_nullable = scalar_type.is_nullable;

            // Store all possible reference formats
            // Format: [schema].[name]
            udt_nullability.insert(format!("[{}].[{}]", schema, name), is_nullable);
            // Format: schema.name
            udt_nullability.insert(format!("{}.{}", schema, name), is_nullable);
            // Format: [schema].name
            udt_nullability.insert(format!("[{}].{}", schema, name), is_nullable);
            // Format: schema.[name]
            udt_nullability.insert(format!("{}.[{}]", schema, name), is_nullable);
        }
    }

    // If no UDTs, nothing to resolve
    if udt_nullability.is_empty() {
        return;
    }

    // Update columns that use UDTs and don't have explicit nullability
    for element in elements.iter_mut() {
        if let ModelElement::Table(table) = element {
            for column in &mut table.columns {
                // Only update if column doesn't have explicit nullability
                if column.nullability.is_none() {
                    // Check if the column's data type matches a UDT
                    if let Some(&is_nullable) = udt_nullability.get(&column.data_type) {
                        // Inherit nullability from UDT
                        column.nullability = Some(is_nullable);
                    }
                }
            }
        }
    }
}

/// Assign disambiguator values to ALL constraints and build linkages to columns/tables.
///
/// DotNet DacFx annotation pattern:
/// - Every constraint gets a unique disambiguator (starting from 3)
/// - Every constraint gets either Annotation or AttachedAnnotation
/// - Inline constraints: always use Annotation, columns use AttachedAnnotation
/// - Named constraints: pattern depends on count per table:
///   * Single named constraint: table gets Annotation, constraint gets AttachedAnnotation
///   * Multiple named constraints: constraints get Annotation (except one which gets AttachedAnnotation)
///     and table gets AttachedAnnotation for constraints with Annotation, plus one Annotation
fn assign_inline_constraint_disambiguators(
    elements: &mut [ModelElement],
    package_reference_count: usize,
) {
    use std::collections::HashMap;

    // DotNet assigns disambiguators in XML output order to elements that will carry <Annotation>.
    // The pattern is:
    // - Inline constraints: constraint always gets Annotation
    // - Single named constraint per table: TABLE gets Annotation, constraint gets AttachedAnnotation
    // - Multiple named constraints per table: constraints get Annotation (except last), last gets AttachedAnnotation

    // Pre-phase: Build table index lookup and constraint-to-table mapping.
    // Keys are table element indices instead of cloned (schema, name) strings.
    let mut table_name_to_idx: HashMap<(&str, &str), usize> = HashMap::new();
    for (idx, element) in elements.iter().enumerate() {
        if let ModelElement::Table(table) = element {
            table_name_to_idx.insert((&table.schema, &table.name), idx);
        }
    }

    // Map each constraint element index to its parent table element index
    let mut constraint_to_table: HashMap<usize, usize> = HashMap::new();

    // Phase 1: Collect constraint info per table (before assigning any disambiguators)
    // Map: table_element_index -> Vec<(element_index, is_inline, emit_name, source_order)>
    // DotNet assigns disambiguators in source order (order constraints appear in CREATE TABLE),
    // so we track source_order to sort constraints before assigning disambiguators.
    let mut table_constraints: TableConstraintMap = HashMap::new();

    for (idx, element) in elements.iter().enumerate() {
        if let ModelElement::Constraint(constraint) = element {
            if let Some(&table_idx) = table_name_to_idx.get(&(
                constraint.table_schema.as_str(),
                constraint.table_name.as_str(),
            )) {
                constraint_to_table.insert(idx, table_idx);
                table_constraints.entry(table_idx).or_default().push((
                    idx,
                    constraint.is_inline,
                    constraint.emit_name,
                    constraint.source_order,
                ));
            }
        }
    }

    // table_name_to_idx is no longer needed — all lookups use constraint_to_table
    drop(table_name_to_idx);

    // Sort constraints per table by element index (sorted/alphabetical order) for consistent
    // processing. Source order is used specifically for the 2-named-constraint case later.
    for constraints in table_constraints.values_mut() {
        constraints.sort_by_key(|(idx, _, _, _)| *idx);
    }

    // Phase 2: Determine which elements will carry Annotation vs AttachedAnnotation
    // For single named constraint tables, track who carries the Annotation:
    // - Single named TABLE-LEVEL constraint: table carries Annotation
    // - Single named INLINE constraint: column carries Annotation
    let mut table_carries_annotation: HashMap<usize, bool> = HashMap::new();
    // Track single named inline constraint info: (constraint_index, constraint_columns)
    // Column needs this info to write Annotation element
    let mut single_named_inline_info: HashMap<usize, (usize, Vec<String>)> = HashMap::new();

    for (&table_idx, constraints) in &table_constraints {
        // Count named constraints (emit_name = true).
        // This includes table-level constraints AND inline constraints with CONSTRAINT [name].
        let named_count = constraints
            .iter()
            .filter(|(_, _, emit_name, _)| *emit_name)
            .count();

        if named_count == 1 {
            // Find the single named constraint
            let (idx, is_inline, _, _) = constraints
                .iter()
                .find(|(_, _, emit_name, _)| *emit_name)
                .unwrap();

            if *is_inline {
                // Single named INLINE constraint: column carries Annotation
                // Get the constraint's column info from the elements
                if let Some(ModelElement::Constraint(c)) = elements.get(*idx) {
                    let col_names: Vec<String> =
                        c.columns.iter().map(|col| col.name.clone()).collect();
                    single_named_inline_info.insert(table_idx, (*idx, col_names));
                }
            } else {
                // Single named TABLE-LEVEL constraint: table carries Annotation
                table_carries_annotation.insert(table_idx, true);
            }
        }
        // For 0 or multiple named constraints, table doesn't carry primary Annotation
    }

    // Phase 3: Walk through elements in sorted order and assign disambiguators
    // to elements that will carry Annotation
    // DotNet starts at 3, but reserves slots for package references (1 slot per reference)
    let mut next_disambiguator: u32 = 3 + package_reference_count as u32;

    // Map element index -> assigned disambiguator
    let mut element_disambiguators: HashMap<usize, u32> = HashMap::new();

    // Track which constraints use Annotation (vs AttachedAnnotation)
    let mut constraint_uses_annotation: HashMap<usize, bool> = HashMap::new();

    // For the 2-named-constraint case, we need to pre-assign disambiguators in SOURCE ORDER
    // before iterating through sorted elements. This ensures the first constraint in source
    // order gets the lower disambiguator, matching DotNet behavior.
    // Map: table_element_index -> Vec<(element_index, disambiguator)> sorted by source_order
    let mut preassigned_disambiguators: HashMap<usize, Vec<(usize, u32)>> = HashMap::new();

    for (&table_idx, constraints) in &table_constraints {
        // Count named constraints (emit_name = true)
        let named_count = constraints
            .iter()
            .filter(|(_, _, emit_name, _)| *emit_name)
            .count();
        // Count unnamed inline constraints (emit_name = false)
        let unnamed_inline_count = constraints
            .iter()
            .filter(|(_, _, emit_name, _)| !*emit_name)
            .count();

        if named_count == 2 && unnamed_inline_count == 0 {
            // Pre-assign disambiguators for 2-named-constraint tables in SOURCE ORDER
            let mut named: Vec<_> = constraints
                .iter()
                .filter(|(_, _, emit_name, _)| *emit_name)
                .cloned()
                .collect();
            named.sort_by_key(|(_, _, _, source_order)| *source_order);

            let mut assignments = Vec::new();
            for (idx, _, _, _) in named {
                let disambiguator = next_disambiguator;
                next_disambiguator += 1;
                assignments.push((idx, disambiguator));
            }
            preassigned_disambiguators.insert(table_idx, assignments);
        }
    }

    // Pass A: Assign disambiguators to FullTextIndex elements FIRST.
    // DotNet assigns FullTextIndex disambiguators before inline constraints,
    // regardless of alphabetical sort order. This is needed because FullTextIndex
    // elements have names like "[dbo].[Documents]" which sort AFTER inline
    // constraints with empty names, but DotNet gives them lower disambiguator values.
    for (idx, element) in elements.iter().enumerate() {
        if let ModelElement::FullTextIndex(_) = element {
            let disambiguator = next_disambiguator;
            next_disambiguator += 1;
            element_disambiguators.insert(idx, disambiguator);
        }
    }

    // Pass B: Iterate in sorted order and assign disambiguators to tables and constraints
    for (idx, element) in elements.iter().enumerate() {
        match element {
            ModelElement::Table(_) => {
                // Table gets Annotation if it has exactly one named constraint
                if table_carries_annotation.get(&idx) == Some(&true) {
                    let disambiguator = next_disambiguator;
                    next_disambiguator += 1;
                    element_disambiguators.insert(idx, disambiguator);
                }
            }
            ModelElement::Constraint(constraint) => {
                let table_idx = constraint_to_table.get(&idx).copied();

                // Unnamed inline constraints (no CONSTRAINT name in SQL) always get Annotation.
                // Named constraints (both table-level and inline with CONSTRAINT name) follow
                // the single/multiple named constraint rules.
                let is_unnamed_inline = constraint.is_inline && !constraint.emit_name;

                if is_unnamed_inline {
                    // Unnamed inline constraints always get Annotation
                    let disambiguator = next_disambiguator;
                    next_disambiguator += 1;
                    element_disambiguators.insert(idx, disambiguator);
                    constraint_uses_annotation.insert(idx, true);
                } else if let Some(t_idx) = table_idx {
                    // Named constraint (table-level or inline with explicit CONSTRAINT name)
                    let constraints_for_table = table_constraints.get(&t_idx);
                    // Count named constraints (emit_name = true)
                    let named_count = constraints_for_table
                        .map(|c| c.iter().filter(|(_, _, emit_name, _)| *emit_name).count())
                        .unwrap_or(0);
                    // Count unnamed inline constraints (emit_name = false)
                    let unnamed_inline_count = constraints_for_table
                        .map(|c| c.iter().filter(|(_, _, emit_name, _)| !*emit_name).count())
                        .unwrap_or(0);

                    if named_count == 1 {
                        // Single named constraint: constraint gets AttachedAnnotation
                        // The annotation owner depends on whether this is inline or table-level:
                        // - Table-level: table got the disambiguator already, use same
                        // - Inline: we assign a new disambiguator, column will use it for Annotation
                        if constraint.is_inline {
                            // Single named INLINE constraint: assign new disambiguator
                            // The column will carry the Annotation with this disambiguator
                            let disambiguator = next_disambiguator;
                            next_disambiguator += 1;
                            element_disambiguators.insert(idx, disambiguator);
                        } else {
                            // Single named TABLE-LEVEL constraint: use table's disambiguator
                            if let Some(&disambiguator) = element_disambiguators.get(&t_idx) {
                                element_disambiguators.insert(idx, disambiguator);
                            }
                        }
                        constraint_uses_annotation.insert(idx, false);
                    } else if named_count == 2 && unnamed_inline_count == 0 {
                        // Exactly 2 named constraints with NO unnamed inline constraints:
                        // DotNet special case: both constraints get AttachedAnnotation,
                        // and the table gets 2 Annotation elements.
                        // Use pre-assigned disambiguator (assigned in source order)
                        if let Some(assignments) = preassigned_disambiguators.get(&t_idx) {
                            if let Some((_, disambiguator)) =
                                assignments.iter().find(|(i, _)| *i == idx)
                            {
                                element_disambiguators.insert(idx, *disambiguator);
                            }
                        }
                        constraint_uses_annotation.insert(idx, false);
                    } else {
                        // Multiple named constraints (3+, or 2 with unnamed inline constraints)
                        // Find this constraint's position among named constraints for this table
                        let named_constraints: Vec<_> = table_constraints
                            .get(&t_idx)
                            .unwrap()
                            .iter()
                            .filter(|(_, _, emit_name, _)| *emit_name)
                            .collect();

                        let position = named_constraints
                            .iter()
                            .position(|(c_idx, _, _, _)| *c_idx == idx)
                            .unwrap();

                        if position < named_constraints.len() - 1 {
                            // Not the last: gets Annotation
                            let disambiguator = next_disambiguator;
                            next_disambiguator += 1;
                            element_disambiguators.insert(idx, disambiguator);
                            constraint_uses_annotation.insert(idx, true);
                        } else {
                            // Last one: gets AttachedAnnotation
                            // It shares the table's disambiguator (but table also needs Annotation for this)
                            let disambiguator = next_disambiguator;
                            next_disambiguator += 1;
                            element_disambiguators.insert(idx, disambiguator);
                            constraint_uses_annotation.insert(idx, false);
                        }
                    }
                }
            }
            ModelElement::FullTextIndex(_) => {
                // Already handled in Pass A above
            }
            _ => {}
        }
    }

    // Phase 4: Apply disambiguators to elements and build annotation maps
    // Map: (table_element_index, column_name) -> Vec<disambiguator>
    let mut column_annotations: HashMap<(usize, String), Vec<u32>> = HashMap::new();

    // Map: table_element_index -> Vec<(Annotation disambiguator, index of annotated constraint)>
    // For tables where constraints use AttachedAnnotation, the table gets Annotation elements.
    // This Vec can have multiple entries for the 2-named-constraint case.
    let mut table_annotation: HashMap<usize, Vec<(u32, usize)>> = HashMap::new();

    // Map: table_element_index -> Vec<(disambiguator, constraint_index)>
    // Tracks constraints that use Annotation (table gets AttachedAnnotation for them)
    let mut table_attached: HashMap<usize, Vec<(u32, usize)>> = HashMap::new();

    for (idx, element) in elements.iter_mut().enumerate() {
        if let ModelElement::Constraint(constraint) = element {
            let disambiguator = element_disambiguators.get(&idx).copied();
            constraint.inline_constraint_disambiguator = disambiguator;
            constraint.uses_annotation = constraint_uses_annotation
                .get(&idx)
                .copied()
                .unwrap_or(false);

            let table_idx = constraint_to_table.get(&idx).copied();

            // Concern 1: Column AttachedAnnotations (only for unnamed inline constraints)
            // Named inline constraints (single named constraint case) have the column carry Annotation
            // instead of AttachedAnnotation, which is handled separately via inline_constraint_annotation.
            if constraint.is_inline {
                let is_single_named_inline = table_idx
                    .map(|ti| single_named_inline_info.contains_key(&ti))
                    .unwrap_or(false)
                    && constraint.emit_name;

                if !is_single_named_inline {
                    // Unnamed inline constraints: link to column(s) for AttachedAnnotation
                    if let (Some(d), Some(t_idx)) = (disambiguator, table_idx) {
                        for col in &constraint.columns {
                            column_annotations
                                .entry((t_idx, col.name.clone()))
                                .or_default()
                                .push(d);
                        }
                    }
                }
            }

            // Concern 2: Table annotations (for ANY constraint using AttachedAnnotation)
            // This is INDEPENDENT of is_inline - inline constraints with uses_annotation=false
            // must also add to table_annotation so the table gets the Annotation element.
            // This fixes deployment failures for tables with mixed inline + table-level constraints.
            if let Some(t_idx) = table_idx {
                let is_single_named_inline_table = single_named_inline_info.contains_key(&t_idx);
                if !constraint.is_inline || (constraint.emit_name && !is_single_named_inline_table)
                {
                    // For table-level constraints OR named inline constraints that aren't
                    // the single-named-inline case (i.e., 2+ named constraint tables)
                    if let Some(d) = disambiguator {
                        if constraint.uses_annotation {
                            // This constraint uses Annotation, so table gets AttachedAnnotation for it
                            table_attached.entry(t_idx).or_default().push((d, idx));
                        } else {
                            // This constraint uses AttachedAnnotation, so table gets Annotation for it
                            table_annotation.entry(t_idx).or_default().push((d, idx));
                        }
                    }
                }
            }
        }
    }

    // Phase 5: Apply to tables
    // DotNet outputs AttachedAnnotations in a specific order relative to the Annotation:
    // 1. AttachedAnnotations for constraints that appear AFTER the annotated constraint (descending order)
    // 2. The Annotation itself
    // 3. AttachedAnnotations for constraints that appear BEFORE the annotated constraint (ascending order)
    for (idx, element) in elements.iter_mut().enumerate() {
        if let ModelElement::Table(table) = element {
            // Get Annotation disambiguators for this table
            // - Single-constraint tables: disambiguator assigned in Phase 3
            // - Multi-constraint tables: from table_annotation (can have multiple for 2-constraint case)
            if table_carries_annotation.get(&idx) == Some(&true) {
                if let Some(&d) = element_disambiguators.get(&idx) {
                    table.inline_constraint_disambiguators = vec![d];
                }
            } else if let Some(annotations) = table_annotation.get(&idx) {
                // Multi-constraint table: collect all annotation disambiguators
                let mut disambiguators: Vec<u32> = annotations.iter().map(|(d, _)| *d).collect();
                disambiguators.sort(); // Ascending order
                table.inline_constraint_disambiguators = disambiguators;
            }

            // Split attached annotations based on median disambiguator value
            // DotNet splits AttachedAnnotations around the median: higher values go before
            // the Annotation (descending), lower values go after (ascending).
            if let Some(attached_list) = table_attached.get(&idx) {
                if !attached_list.is_empty() {
                    // Collect all disambiguator values
                    let mut disambiguators: Vec<u32> =
                        attached_list.iter().map(|(d, _)| *d).collect();
                    disambiguators.sort();

                    // Calculate median threshold
                    let median = if disambiguators.len().is_multiple_of(2) {
                        // Even count: average of two middle values
                        let mid = disambiguators.len() / 2;
                        (disambiguators[mid - 1] + disambiguators[mid]) as f64 / 2.0
                    } else {
                        // Odd count: middle value
                        disambiguators[disambiguators.len() / 2] as f64
                    };

                    // Split: disambiguators > median go before Annotation (descending order)
                    let mut before_annotation: Vec<u32> = disambiguators
                        .iter()
                        .filter(|&&d| d as f64 > median)
                        .copied()
                        .collect();
                    before_annotation.sort_by(|a, b| b.cmp(a)); // Descending

                    // Disambiguators <= median go after Annotation (ascending order)
                    let mut after_annotation: Vec<u32> = disambiguators
                        .iter()
                        .filter(|&&d| d as f64 <= median)
                        .copied()
                        .collect();
                    after_annotation.sort(); // Ascending

                    table.attached_annotations_before_annotation = before_annotation;
                    table.attached_annotations_after_annotation = after_annotation;
                }
            }

            // Also assign attached_annotations to columns from inline constraints
            for column in &mut table.columns {
                if let Some(annotations) = column_annotations.get(&(idx, column.name.clone())) {
                    column.attached_annotations = annotations.clone();
                }
            }

            // For single named INLINE constraint, assign Annotation to the first column
            // DotNet puts Annotation on the column, not the table, for inline constraints
            if let Some((constraint_idx, col_names)) = single_named_inline_info.get(&idx) {
                if let Some(&disambiguator) = element_disambiguators.get(constraint_idx) {
                    // Find the first column mentioned in the constraint
                    if let Some(col_name) = col_names.first() {
                        for column in &mut table.columns {
                            if column.name == *col_name {
                                column.inline_constraint_annotation = Some(disambiguator);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 6: Apply to fulltext indexes
    for (idx, element) in elements.iter_mut().enumerate() {
        if let ModelElement::FullTextIndex(fulltext) = element {
            if let Some(&d) = element_disambiguators.get(&idx) {
                fulltext.disambiguator = Some(d);
            }
        }
    }
}

fn extract_schema_and_name(name: &ObjectName, default_schema: &str) -> (String, String) {
    let parts: Vec<_> = name.0.iter().map(|p| p.value.clone()).collect();

    match parts.len() {
        1 => (default_schema.to_string(), parts[0].clone()),
        2 => (parts[0].clone(), parts[1].clone()),
        _ => (
            default_schema.to_string(),
            parts.last().cloned().unwrap_or_default(),
        ),
    }
}

fn column_from_def(col: &ColumnDef, _schema: &str, _table_name: &str) -> ColumnElement {
    // Track explicit nullability: None = not specified, Some(true) = explicit NULL, Some(false) = explicit NOT NULL
    let mut nullability: Option<bool> = None;
    let mut is_identity = false;
    let mut default_value = None;
    let mut computed_expression: Option<String> = None;
    let mut is_persisted = false;

    for option in &col.options {
        match &option.option {
            ColumnOption::NotNull => nullability = Some(false),
            ColumnOption::Null => nullability = Some(true),
            ColumnOption::Default(expr) => {
                default_value = Some(expr.to_string());
            }
            ColumnOption::Identity(_) => is_identity = true,
            ColumnOption::Generated {
                generation_expr,
                generation_expr_mode,
                ..
            } => {
                // This is a computed column
                if let Some(expr) = generation_expr {
                    computed_expression = Some(format!("({})", expr));
                }
                // Check if PERSISTED (STORED in ANSI SQL)
                if let Some(mode) = generation_expr_mode {
                    is_persisted = matches!(mode, sqlparser::ast::GeneratedExpressionMode::Stored);
                }
            }
            _ => {}
        }
    }

    // ROWGUIDCOL, SPARSE, FILESTREAM are T-SQL-specific keywords not recognized by sqlparser-rs.
    // Any CREATE TABLE containing these keywords fails sqlparser parsing and goes through
    // the fallback token-based parser (column_parser.rs), which sets these flags directly.
    // This AST path (column_from_def) is only reached when sqlparser succeeds, so these are always false.

    let (max_length, precision, scale) = extract_type_params(&col.data_type);

    // Extract collation if present (sqlparser stores it directly on ColumnDef)
    let collation = col.collation.as_ref().map(|c| c.to_string());

    // Note: attached_annotations are populated during post-processing
    // after disambiguators are assigned to inline constraints

    ColumnElement {
        name: col.name.value.clone(),
        data_type: col.data_type.to_string(),
        nullability,
        is_identity,
        is_rowguidcol: false,
        is_sparse: false,
        is_filestream: false,
        default_value,
        max_length,
        precision,
        scale,
        attached_annotations: Vec::new(), // Populated during post-processing
        inline_constraint_annotation: None, // Populated during post-processing
        computed_expression,
        is_persisted,
        collation,
        is_generated_always_start: false, // AST-parsed tables don't have temporal syntax
        is_generated_always_end: false,
        is_hidden: false,
        masking_function: None, // AST path: sqlparser-rs doesn't support MASKED WITH syntax
    }
}

/// Convert an extracted table type column to a TableTypeColumnElement
fn table_type_column_from_extracted(col: &ExtractedTableTypeColumn) -> TableTypeColumnElement {
    let (max_length, precision, scale) = extract_type_params_from_string(&col.data_type);

    TableTypeColumnElement {
        name: col.name.clone(),
        data_type: col.data_type.clone(),
        nullability: col.nullability,
        default_value: col.default_value.clone(),
        max_length,
        precision,
        scale,
    }
}

/// Convert an extracted table type constraint to a TableTypeConstraint
fn table_type_constraint_from_extracted(
    constraint: &ExtractedTableTypeConstraint,
) -> TableTypeConstraint {
    match constraint {
        ExtractedTableTypeConstraint::PrimaryKey {
            columns,
            is_clustered,
        } => TableTypeConstraint::PrimaryKey {
            columns: columns
                .iter()
                .map(|c| ConstraintColumn::with_direction(c.name.clone(), c.descending))
                .collect(),
            is_clustered: *is_clustered,
        },
        ExtractedTableTypeConstraint::Unique {
            columns,
            is_clustered,
        } => TableTypeConstraint::Unique {
            columns: columns
                .iter()
                .map(|c| ConstraintColumn::with_direction(c.name.clone(), c.descending))
                .collect(),
            is_clustered: *is_clustered,
        },
        ExtractedTableTypeConstraint::Check { expression } => TableTypeConstraint::Check {
            expression: expression.clone(),
        },
        ExtractedTableTypeConstraint::Index {
            name,
            columns,
            is_unique,
            is_clustered,
        } => TableTypeConstraint::Index {
            name: name.clone(),
            columns: columns.clone(),
            is_unique: *is_unique,
            is_clustered: *is_clustered,
        },
    }
}

/// Convert an extracted function parameter to a ParameterElement
fn param_from_extracted(param: &ExtractedFunctionParameter) -> ParameterElement {
    ParameterElement {
        name: param.name.clone(),
        data_type: param.data_type.clone(),
        is_output: false, // Function parameters are typically input-only
        default_value: None,
    }
}

/// Convert an extracted full-text column to a FullTextColumnElement
fn fulltext_column_from_extracted(col: &ExtractedFullTextColumn) -> FullTextColumnElement {
    FullTextColumnElement {
        name: col.name.clone(),
        language_id: col.language_id,
    }
}

/// Extract type parameters from a string data type (e.g., "NVARCHAR(50)", "DECIMAL(18, 2)")
fn extract_type_params_from_string(data_type: &str) -> (Option<i32>, Option<u8>, Option<u8>) {
    // Check for MAX indicator
    if contains_ci(data_type, "MAX") {
        return (Some(-1), None, None);
    }

    // Parse parameters from type string like "NVARCHAR(50)" or "DECIMAL(18, 2)"
    if let Some(caps) = TYPE_PARAMS_RE.captures(data_type) {
        let first: Option<i32> = caps.get(1).and_then(|m| m.as_str().parse().ok());
        let second: Option<u8> = caps.get(2).and_then(|m| m.as_str().parse().ok());

        if starts_with_ci(data_type, "DECIMAL") || starts_with_ci(data_type, "NUMERIC") {
            // For DECIMAL/NUMERIC: first is precision, second is scale
            return (None, first.map(|v| v as u8), second);
        } else if starts_with_ci(data_type, "DATETIME2")
            || starts_with_ci(data_type, "TIME")
            || starts_with_ci(data_type, "DATETIMEOFFSET")
        {
            // For datetime types with fractional seconds: use Scale property (not Precision)
            // The value is the fractional seconds precision (0-7)
            return (None, None, first.map(|v| v as u8));
        } else {
            // For string/binary types: first is length
            return (first, None, None);
        }
    }

    // Handle datetime types without explicit precision - they default to 7
    if starts_with_ci(data_type, "DATETIME2")
        || starts_with_ci(data_type, "TIME")
        || starts_with_ci(data_type, "DATETIMEOFFSET")
    {
        // DotNet always emits Scale="7" for these types when no explicit precision
        return (None, None, Some(7));
    }

    (None, None, None)
}

fn extract_type_params(data_type: &DataType) -> (Option<i32>, Option<u8>, Option<u8>) {
    match data_type {
        DataType::Varchar(len) | DataType::Char(len) | DataType::Nvarchar(len) => {
            let max_length = len.as_ref().map(|l| match l {
                sqlparser::ast::CharacterLength::IntegerLength { length, .. } => *length as i32,
                sqlparser::ast::CharacterLength::Max => -1,
            });
            (max_length, None, None)
        }
        DataType::Varbinary(len) | DataType::Binary(len) => {
            // Handle VARBINARY(MAX) and BINARY(MAX) - preprocessor replaces MAX with sentinel value
            let max_length = len.map(|l| {
                if l == BINARY_MAX_SENTINEL {
                    -1 // Indicates MAX
                } else {
                    l as i32
                }
            });
            (max_length, None, None)
        }
        DataType::Decimal(info) | DataType::Numeric(info) => {
            let (precision, scale) = match info {
                sqlparser::ast::ExactNumberInfo::None => (None, None),
                sqlparser::ast::ExactNumberInfo::Precision(p) => (Some(*p as u8), None),
                sqlparser::ast::ExactNumberInfo::PrecisionAndScale(p, s) => {
                    (Some(*p as u8), Some(*s as u8))
                }
            };
            (None, precision, scale)
        }
        // For other types (including datetime2, time, datetimeoffset), use string-based extraction
        _ => extract_type_params_from_string(&data_type.to_string()),
    }
}

/// Convert an extracted table column (from fallback parser) to a model column
fn column_from_fallback_table(
    col: &ExtractedTableColumn,
    _schema: &str,
    _table_name: &str,
) -> ColumnElement {
    let (max_length, precision, scale) = extract_type_params_from_string(&col.data_type);

    // Note: attached_annotations are populated during post-processing
    // after disambiguators are assigned to inline constraints

    ColumnElement {
        name: col.name.clone(),
        data_type: col.data_type.clone(),
        nullability: col.nullability,
        is_identity: col.is_identity,
        is_rowguidcol: col.is_rowguidcol,
        is_sparse: col.is_sparse,
        is_filestream: col.is_filestream,
        default_value: col.default_value.clone(),
        max_length,
        precision,
        scale,
        attached_annotations: Vec::new(), // Populated during post-processing
        inline_constraint_annotation: None, // Populated during post-processing
        computed_expression: col.computed_expression.clone(),
        is_persisted: col.is_persisted,
        collation: col.collation.clone(),
        is_generated_always_start: col.is_generated_always_start,
        is_generated_always_end: col.is_generated_always_end,
        is_hidden: col.is_hidden,
        masking_function: col.masking_function.clone(),
    }
}

/// Convert an extracted table constraint (from fallback parser) to a model constraint
/// These are table-level constraints (defined with CONSTRAINT keyword) so they are not inline
fn constraint_from_extracted(
    constraint: &ExtractedTableConstraint,
    table_schema: &str,
    table_name: &str,
) -> Option<ConstraintElement> {
    match constraint {
        ExtractedTableConstraint::PrimaryKey {
            name,
            columns,
            is_clustered,
        } => Some(
            ConstraintBuilder::new(
                name.clone(),
                table_schema.to_string(),
                table_name.to_string(),
                ConstraintType::PrimaryKey,
                columns
                    .iter()
                    .map(|c| ConstraintColumn::with_direction(c.name.clone(), c.descending))
                    .collect(),
            )
            .clustered(*is_clustered)
            .build(),
        ),
        ExtractedTableConstraint::ForeignKey {
            name,
            columns,
            referenced_table,
            referenced_columns,
        } => Some(
            ConstraintBuilder::new(
                name.clone(),
                table_schema.to_string(),
                table_name.to_string(),
                ConstraintType::ForeignKey,
                columns
                    .iter()
                    .map(|c| ConstraintColumn::new(c.clone()))
                    .collect(),
            )
            .foreign_key_refs(referenced_table.clone(), referenced_columns.clone())
            .build(),
        ),
        ExtractedTableConstraint::Unique {
            name,
            columns,
            is_clustered,
        } => Some(
            ConstraintBuilder::new(
                name.clone(),
                table_schema.to_string(),
                table_name.to_string(),
                ConstraintType::Unique,
                columns
                    .iter()
                    .map(|c| ConstraintColumn::with_direction(c.name.clone(), c.descending))
                    .collect(),
            )
            .clustered(*is_clustered)
            .build(),
        ),
        ExtractedTableConstraint::Check { name, expression } => Some(
            ConstraintBuilder::new(
                name.clone(),
                table_schema.to_string(),
                table_name.to_string(),
                ConstraintType::Check,
                vec![],
            )
            .definition(expression.clone())
            .build(),
        ),
    }
}

/// Extract clustering info for a constraint from raw SQL.
///
/// sqlparser doesn't expose CLUSTERED/NONCLUSTERED keywords, so we need to
/// look at the raw SQL to determine the clustering mode.
///
/// - For PRIMARY KEY: default is CLUSTERED, returns Some(false) if NONCLUSTERED found
/// - For UNIQUE: default is NONCLUSTERED, returns Some(true) if CLUSTERED found
///
/// Returns None if the constraint cannot be found in the SQL (shouldn't happen).
fn extract_constraint_clustering(
    raw_sql: &str,
    constraint_name: &str,
    is_primary_key: bool,
) -> Option<bool> {
    // Try to find the constraint definition in the SQL
    // First, try to find a named constraint
    let constraint_pattern = format!("CONSTRAINT [{}]", constraint_name);
    let constraint_pattern_bare = format!("CONSTRAINT {}", constraint_name);

    // Find the position of the constraint in the SQL (case-insensitive)
    let constraint_start = find_ci(raw_sql, &constraint_pattern)
        .or_else(|| find_ci(raw_sql, &constraint_pattern_bare));

    if let Some(start_pos) = constraint_start {
        // Look at the text after the constraint name for CLUSTERED/NONCLUSTERED
        let remaining = &raw_sql[start_pos..];

        // Find the end of this constraint definition (next CONSTRAINT or end of CREATE TABLE)
        let end_pos = if remaining.len() > 20 {
            find_ci(&remaining[20..], "CONSTRAINT")
                .map(|p| p + 20)
                .unwrap_or(remaining.len())
        } else {
            remaining.len()
        };
        let constraint_def = &remaining[..end_pos];

        // Check for NONCLUSTERED before CLUSTERED to avoid matching "NONCLUSTERED" as "CLUSTERED"
        if contains_ci(constraint_def, "NONCLUSTERED") {
            return Some(false);
        } else if contains_ci(constraint_def, "CLUSTERED") {
            return Some(true);
        }
    }

    // If not found by name, look for PRIMARY KEY or UNIQUE patterns
    let keyword = if is_primary_key {
        "PRIMARY KEY"
    } else {
        "UNIQUE"
    };

    if let Some(keyword_pos) = find_ci(raw_sql, keyword) {
        // Look at the text after PRIMARY KEY or UNIQUE
        let remaining = &raw_sql[keyword_pos..];
        let end_pos = remaining
            .find('(')
            .unwrap_or_else(|| remaining.len().min(100));
        let constraint_def = &remaining[..end_pos];

        if contains_ci(constraint_def, "NONCLUSTERED") {
            return Some(false);
        } else if contains_ci(constraint_def, "CLUSTERED") {
            return Some(true);
        }
    }

    // Return the default: PRIMARY KEY is clustered, UNIQUE is not
    Some(is_primary_key)
}

/// Convert a table-level constraint from sqlparser to a ConstraintElement
/// Table-level constraints are defined using CONSTRAINT keyword so they are not inline
fn constraint_from_table_constraint(
    constraint: &TableConstraint,
    table_name: &ObjectName,
    default_schema: &str,
    raw_sql: &str,
) -> Option<ConstraintElement> {
    let (table_schema, table_name_str) = extract_schema_and_name(table_name, default_schema);

    match constraint {
        TableConstraint::PrimaryKey { name, columns, .. } => {
            let constraint_name = name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_else(|| format!("PK_{}", table_name_str));

            // Determine clustering from raw SQL since sqlparser doesn't expose it
            // Default for PRIMARY KEY is CLUSTERED, so only set to false if NONCLUSTERED is found
            let is_clustered = extract_constraint_clustering(raw_sql, &constraint_name, true);

            Some(
                ConstraintBuilder::new(
                    constraint_name,
                    table_schema,
                    table_name_str,
                    ConstraintType::PrimaryKey,
                    columns
                        .iter()
                        .map(|c| ConstraintColumn::new(c.value.clone()))
                        .collect(),
                )
                .clustered_opt(is_clustered)
                .build(),
            )
        }
        TableConstraint::ForeignKey {
            name,
            columns,
            foreign_table,
            referred_columns,
            ..
        } => {
            let constraint_name = name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_else(|| format!("FK_{}_{}", table_name_str, foreign_table));

            // Format the foreign table reference with brackets: [schema].[table]
            let (foreign_schema, foreign_table_name) =
                extract_schema_and_name(foreign_table, default_schema);
            let formatted_foreign_table = format!("[{}].[{}]", foreign_schema, foreign_table_name);

            Some(
                ConstraintBuilder::new(
                    constraint_name,
                    table_schema,
                    table_name_str,
                    ConstraintType::ForeignKey,
                    columns
                        .iter()
                        .map(|c| ConstraintColumn::new(c.value.clone()))
                        .collect(),
                )
                .foreign_key_refs(
                    formatted_foreign_table,
                    referred_columns.iter().map(|c| c.value.clone()).collect(),
                )
                .build(),
            )
        }
        TableConstraint::Unique { name, columns, .. } => {
            let constraint_name = name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_else(|| format!("UQ_{}", table_name_str));

            // Determine clustering from raw SQL since sqlparser doesn't expose it
            // Default for UNIQUE is NONCLUSTERED, so only set to true if CLUSTERED is found
            let is_clustered = extract_constraint_clustering(raw_sql, &constraint_name, false);

            Some(
                ConstraintBuilder::new(
                    constraint_name,
                    table_schema,
                    table_name_str,
                    ConstraintType::Unique,
                    columns
                        .iter()
                        .map(|c| ConstraintColumn::new(c.value.clone()))
                        .collect(),
                )
                .clustered_opt(is_clustered)
                .build(),
            )
        }
        TableConstraint::Check { name, expr } => {
            let constraint_name = name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_else(|| format!("CK_{}", table_name_str));

            Some(
                ConstraintBuilder::new(
                    constraint_name,
                    table_schema,
                    table_name_str,
                    ConstraintType::Check,
                    vec![],
                )
                .definition(expr.to_string())
                .build(),
            )
        }
        _ => None,
    }
}

/// Check if a procedure or function definition uses NATIVE_COMPILATION
fn is_natively_compiled(definition: &str) -> bool {
    contains_ci(definition, "NATIVE_COMPILATION")
}

/// Extract FILLFACTOR from index WITH clause options
fn extract_fill_factor(with_options: &[Expr]) -> Option<u8> {
    for expr in with_options {
        if let Expr::BinaryOp { left, op, right } = expr {
            if *op == BinaryOperator::Eq {
                // Check if the left side is FILLFACTOR identifier
                if let Expr::Identifier(ident) = left.as_ref() {
                    if ident.value.eq_ignore_ascii_case("FILLFACTOR") {
                        // Extract the numeric value from the right side
                        if let Expr::Value(sqlparser::ast::Value::Number(n, _)) = right.as_ref() {
                            if let Ok(val) = n.parse::<u8>() {
                                return Some(val);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract DATA_COMPRESSION from index WITH clause options
fn extract_data_compression(with_options: &[Expr]) -> Option<DataCompressionType> {
    for expr in with_options {
        if let Expr::BinaryOp { left, op, right } = expr {
            if *op == BinaryOperator::Eq {
                // Check if the left side is DATA_COMPRESSION identifier
                if let Expr::Identifier(ident) = left.as_ref() {
                    if ident.value.eq_ignore_ascii_case("DATA_COMPRESSION") {
                        // Extract the compression type from the right side
                        if let Expr::Identifier(value_ident) = right.as_ref() {
                            return parse_data_compression(&value_ident.value);
                        }
                    }
                }
            }
        }
    }
    None
}

// Phase 20.6.2: Removed extract_filter_predicate_from_sql() - replaced with token-based
// extract_index_filter_predicate_tokenized() from index_parser.rs

/// Convert an extracted extended property to a model ExtendedPropertyElement
fn extended_property_from_extracted(
    property: &ExtractedExtendedProperty,
) -> ExtendedPropertyElement {
    ExtendedPropertyElement {
        property_name: property.property_name.clone(),
        property_value: property.property_value.clone(),
        target_schema: property.level0name.clone(),
        target_object: property.level1name.clone().unwrap_or_default(),
        target_column: property.level2name.clone(),
        level1type: property.level1type.clone(),
        level2type: property.level2type.clone(),
    }
}

/// Extract view options (SCHEMABINDING, WITH CHECK OPTION, VIEW_METADATA) from SQL text
/// Returns (is_schema_bound, is_with_check_option, is_metadata_reported)
fn extract_view_options(sql: &str) -> (bool, bool, bool) {
    // WITH SCHEMABINDING appears before AS in the view definition
    let is_schema_bound = contains_ci(sql, "WITH SCHEMABINDING")
        || contains_ci(sql, ", SCHEMABINDING")
        || contains_ci(sql, ",SCHEMABINDING");

    // WITH CHECK OPTION appears at the end of the view definition
    let is_with_check_option = contains_ci(sql, "WITH CHECK OPTION");

    // VIEW_METADATA appears in WITH clause before AS
    let is_metadata_reported = contains_ci(sql, "VIEW_METADATA");

    (is_schema_bound, is_with_check_option, is_metadata_reported)
}

/// Temporal metadata extracted from raw SQL text for AST-parsed CREATE TABLE statements.
/// sqlparser-rs parses temporal tables but doesn't expose PERIOD FOR SYSTEM_TIME,
/// SYSTEM_VERSIONING, or GENERATED ALWAYS in its AST.
struct TemporalMetadata {
    system_time_start_column: Option<String>,
    system_time_end_column: Option<String>,
    is_system_versioned: bool,
    history_table_schema: Option<String>,
    history_table_name: Option<String>,
    /// Column names that are GENERATED ALWAYS AS ROW START
    generated_always_start_columns: Vec<String>,
    /// Column names that are GENERATED ALWAYS AS ROW END
    generated_always_end_columns: Vec<String>,
    /// Column names that have the HIDDEN attribute
    hidden_columns: Vec<String>,
}

/// Extract temporal table metadata from raw SQL text.
/// This is used when sqlparser-rs successfully parses a CREATE TABLE
/// but doesn't expose temporal-specific properties in the AST.
fn extract_temporal_metadata_from_sql(sql: &str) -> TemporalMetadata {
    // Extract PERIOD FOR SYSTEM_TIME columns
    let (start_col, end_col) = extract_period_columns(sql);

    // Extract SYSTEM_VERSIONING options
    let (is_system_versioned, history_schema, history_name) = extract_versioning_options(sql);

    // Extract GENERATED ALWAYS AS ROW START/END column names
    let generated_always_start_columns = extract_generated_always_columns(sql, "START");
    let generated_always_end_columns = extract_generated_always_columns(sql, "END");

    // Extract HIDDEN columns
    let hidden_columns = extract_hidden_columns(sql);

    TemporalMetadata {
        system_time_start_column: start_col,
        system_time_end_column: end_col,
        is_system_versioned,
        history_table_schema: history_schema,
        history_table_name: history_name,
        generated_always_start_columns,
        generated_always_end_columns,
        hidden_columns,
    }
}

/// Extract PERIOD FOR SYSTEM_TIME column names from SQL
fn extract_period_columns(sql: &str) -> (Option<String>, Option<String>) {
    if !contains_ci(sql, "PERIOD") || !contains_ci(sql, "SYSTEM_TIME") {
        return (None, None);
    }

    if let Some(caps) = PERIOD_COLUMNS_RE.captures(sql) {
        return (
            Some(caps.get(1).unwrap().as_str().to_string()),
            Some(caps.get(2).unwrap().as_str().to_string()),
        );
    }
    (None, None)
}

/// Extract SYSTEM_VERSIONING options from SQL
fn extract_versioning_options(sql: &str) -> (bool, Option<String>, Option<String>) {
    if !contains_ci(sql, "SYSTEM_VERSIONING") {
        return (false, None, None);
    }

    if !SYSTEM_VERSIONING_RE.is_match(sql) {
        return (false, None, None);
    }

    let (history_schema, history_name) = HISTORY_TABLE_RE
        .captures(sql)
        .map(|caps| {
            (
                Some(caps.get(1).unwrap().as_str().to_string()),
                Some(caps.get(2).unwrap().as_str().to_string()),
            )
        })
        .unwrap_or((None, None));

    (true, history_schema, history_name)
}

/// Extract column names with GENERATED ALWAYS AS ROW START or END
fn extract_generated_always_columns(sql: &str, start_or_end: &str) -> Vec<String> {
    let re = match start_or_end {
        "START" => &*GENERATED_ALWAYS_START_RE,
        "END" => &*GENERATED_ALWAYS_END_RE,
        _ => return Vec::new(),
    };
    re.captures_iter(sql)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// Extract column names with the HIDDEN attribute
fn extract_hidden_columns(sql: &str) -> Vec<String> {
    HIDDEN_COLUMNS_RE
        .captures_iter(sql)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// Extract permission names from sqlparser Privileges enum
fn extract_privilege_names(privileges: &Privileges) -> Vec<String> {
    match privileges {
        Privileges::All { .. } => vec!["ALL".to_string()],
        Privileges::Actions(actions) => actions
            .iter()
            .map(|action| match action {
                Action::Select { .. } => "SELECT".to_string(),
                Action::Insert { .. } => "INSERT".to_string(),
                Action::Update { .. } => "UPDATE".to_string(),
                Action::Delete => "DELETE".to_string(),
                Action::Execute => "EXECUTE".to_string(),
                Action::References { .. } => "REFERENCES".to_string(),
                Action::Create => "CREATE".to_string(),
                Action::Connect => "CONNECT".to_string(),
                Action::Temporary => "TEMPORARY".to_string(),
                Action::Trigger => "TRIGGER".to_string(),
                Action::Truncate => "TRUNCATE".to_string(),
                Action::Usage => "USAGE".to_string(),
            })
            .collect(),
    }
}

/// Extract target info from sqlparser GrantObjects enum
fn extract_grant_target(objects: &GrantObjects) -> (Option<String>, Option<String>, String) {
    match objects {
        GrantObjects::Tables(tables) => {
            if let Some(table) = tables.first() {
                let parts: Vec<_> = table.0.iter().map(|p| p.value.clone()).collect();
                match parts.len() {
                    1 => (None, Some(parts[0].clone()), "Object".to_string()),
                    2 => (
                        Some(parts[0].clone()),
                        Some(parts[1].clone()),
                        "Object".to_string(),
                    ),
                    _ => (None, None, "Database".to_string()),
                }
            } else {
                (None, None, "Database".to_string())
            }
        }
        GrantObjects::Schemas(schemas) => {
            if let Some(schema) = schemas.first() {
                let name = schema.0.last().map(|i| i.value.clone());
                (name, None, "Schema".to_string())
            } else {
                (None, None, "Database".to_string())
            }
        }
        GrantObjects::Sequences(seqs) => {
            if let Some(seq) = seqs.first() {
                let parts: Vec<_> = seq.0.iter().map(|p| p.value.clone()).collect();
                match parts.len() {
                    1 => (None, Some(parts[0].clone()), "Object".to_string()),
                    2 => (
                        Some(parts[0].clone()),
                        Some(parts[1].clone()),
                        "Object".to_string(),
                    ),
                    _ => (None, None, "Database".to_string()),
                }
            } else {
                (None, None, "Database".to_string())
            }
        }
        _ => (None, None, "Database".to_string()),
    }
}

/// Extract the principal name from sqlparser Grantee list
fn extract_grantee_name(grantees: &[sqlparser::ast::Grantee]) -> String {
    if let Some(grantee) = grantees.first() {
        if let Some(ref name) = grantee.name {
            match name {
                sqlparser::ast::GranteeName::ObjectName(obj) => {
                    obj.0.last().map(|i| i.value.clone()).unwrap_or_default()
                }
                sqlparser::ast::GranteeName::UserHost { user, .. } => user.value.clone(),
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}
