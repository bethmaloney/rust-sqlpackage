//! Build database model from parsed SQL statements

use std::collections::BTreeSet;

use anyhow::Result;
use sqlparser::ast::{
    BinaryOperator, ColumnDef, ColumnOption, DataType, Expr, ObjectName, SchemaName, Statement,
    TableConstraint,
};

use crate::parser::{
    ExtractedExtendedProperty, ExtractedFullTextColumn, ExtractedFunctionParameter,
    ExtractedTableColumn, ExtractedTableConstraint, ExtractedTableTypeColumn,
    ExtractedTableTypeConstraint, FallbackFunctionType, FallbackStatementType, ParsedStatement,
    BINARY_MAX_SENTINEL,
};
use crate::project::SqlProject;

use super::{
    ColumnElement, ConstraintColumn, ConstraintElement, ConstraintType, DatabaseModel,
    ExtendedPropertyElement, FullTextCatalogElement, FullTextColumnElement, FullTextIndexElement,
    FunctionElement, FunctionType, IndexElement, ModelElement, ParameterElement, ProcedureElement,
    RawElement, ScalarTypeElement, SchemaElement, SequenceElement, TableElement,
    TableTypeColumnElement, TableTypeConstraint, TriggerElement, UserDefinedTypeElement,
    ViewElement,
};

/// Build a database model from parsed statements
pub fn build_model(statements: &[ParsedStatement], project: &SqlProject) -> Result<DatabaseModel> {
    let mut model = DatabaseModel::new();
    let mut schemas: BTreeSet<String> = BTreeSet::new();

    // Always include dbo schema
    schemas.insert("dbo".to_string());

    for parsed in statements {
        // Handle fallback-parsed statements (procedures and functions with T-SQL syntax)
        if let Some(fallback) = &parsed.fallback_type {
            match fallback {
                FallbackStatementType::Procedure { schema, name } => {
                    schemas.insert(schema.clone());
                    let is_natively_compiled = is_natively_compiled(&parsed.sql_text);
                    model.add_element(ModelElement::Procedure(ProcedureElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        parameters: vec![], // T-SQL params not extracted - stored in definition
                        is_natively_compiled,
                    }));
                }
                FallbackStatementType::Function {
                    schema,
                    name,
                    function_type,
                    parameters,
                    return_type,
                } => {
                    schemas.insert(schema.clone());
                    let func_type = match function_type {
                        FallbackFunctionType::Scalar => FunctionType::Scalar,
                        FallbackFunctionType::TableValued => FunctionType::TableValued,
                        FallbackFunctionType::InlineTableValued => FunctionType::InlineTableValued,
                    };
                    let is_natively_compiled = is_natively_compiled(&parsed.sql_text);
                    let param_elements = parameters.iter().map(param_from_extracted).collect();
                    model.add_element(ModelElement::Function(FunctionElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        function_type: func_type,
                        parameters: param_elements,
                        return_type: return_type.clone(),
                        is_natively_compiled,
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
                } => {
                    model.add_element(ModelElement::Index(IndexElement {
                        name: name.clone(),
                        table_schema: table_schema.clone(),
                        table_name: table_name.clone(),
                        columns: columns.clone(),
                        include_columns: include_columns.clone(),
                        is_unique: *is_unique,
                        is_clustered: *is_clustered,
                        fill_factor: *fill_factor,
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
                    schemas.insert(schema.clone());
                    model.add_element(ModelElement::Sequence(SequenceElement {
                        schema: schema.clone(),
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
                    schemas.insert(schema.clone());
                    let column_elements: Vec<TableTypeColumnElement> = columns
                        .iter()
                        .map(table_type_column_from_extracted)
                        .collect();
                    let constraint_elements: Vec<TableTypeConstraint> = constraints
                        .iter()
                        .map(table_type_constraint_from_extracted)
                        .collect();
                    model.add_element(ModelElement::UserDefinedType(UserDefinedTypeElement {
                        schema: schema.clone(),
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
                    schemas.insert(schema.clone());
                    model.add_element(ModelElement::ScalarType(ScalarTypeElement {
                        schema: schema.clone(),
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
                } => {
                    schemas.insert(schema.clone());

                    // Convert extracted columns to model columns
                    let model_columns: Vec<ColumnElement> = columns
                        .iter()
                        .map(|c| column_from_fallback_table(c, schema, name))
                        .collect();

                    // Add the table element
                    model.add_element(ModelElement::Table(TableElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        columns: model_columns,
                        is_node: *is_node,
                        is_edge: *is_edge,
                        inline_constraint_disambiguator: None, // Set during post-processing
                    }));

                    // Add constraints as separate elements
                    for constraint in constraints {
                        if let Some(constraint_element) =
                            constraint_from_extracted(constraint, schema, name)
                        {
                            model.add_element(ModelElement::Constraint(constraint_element));
                        }
                    }

                    // Add inline default constraints from column definitions
                    for col in columns {
                        if let Some(default_value) = &col.default_value {
                            // Use explicit constraint name if provided, otherwise generate one
                            let constraint_name = col
                                .default_constraint_name
                                .clone()
                                .unwrap_or_else(|| format!("DF_{}_{}", name, col.name));
                            let is_inline = col.default_constraint_name.is_none();
                            model.add_element(ModelElement::Constraint(ConstraintElement {
                                name: constraint_name,
                                table_schema: schema.clone(),
                                table_name: name.clone(),
                                constraint_type: ConstraintType::Default,
                                columns: vec![ConstraintColumn::new(col.name.clone())],
                                definition: Some(default_value.clone()),
                                referenced_table: None,
                                referenced_columns: None,
                                is_clustered: None,
                                is_inline,
                                inline_constraint_disambiguator: None,
                            }));
                        }
                    }

                    // Add inline CHECK constraints from column definitions
                    for col in columns {
                        if let Some(check_expr) = &col.check_expression {
                            let is_inline = col.check_constraint_name.is_none();
                            let constraint_name = col
                                .check_constraint_name
                                .clone()
                                .unwrap_or_else(|| format!("CK_{}_{}", name, col.name));
                            model.add_element(ModelElement::Constraint(ConstraintElement {
                                name: constraint_name,
                                table_schema: schema.clone(),
                                table_name: name.clone(),
                                constraint_type: ConstraintType::Check,
                                columns: vec![ConstraintColumn::new(col.name.clone())],
                                definition: Some(check_expr.clone()),
                                referenced_table: None,
                                referenced_columns: None,
                                is_clustered: None,
                                is_inline,
                                inline_constraint_disambiguator: None,
                            }));
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
                    schemas.insert(schema.clone());
                    let sql_type = match object_type.to_uppercase().as_str() {
                        "TABLE" => Some("SqlTable"),
                        "VIEW" => Some("SqlView"),
                        // Skip other object types - they would cause deployment failures
                        // ALTER TABLE, INDEX, FULLTEXT INDEX, etc. are not supported as raw elements
                        // Note: TRIGGER is now handled by FallbackStatementType::Trigger
                        _ => None,
                    };
                    if let Some(sql_type) = sql_type {
                        model.add_element(ModelElement::Raw(RawElement {
                            schema: schema.clone(),
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
                    schemas.insert(schema.clone());
                    model.add_element(ModelElement::Trigger(TriggerElement {
                        schema: schema.clone(),
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
                schemas.insert(schema.clone());

                let columns = create_table
                    .columns
                    .iter()
                    .map(|c| column_from_def(c, &schema, &name))
                    .collect();

                model.add_element(ModelElement::Table(TableElement {
                    schema: schema.clone(),
                    name: name.clone(),
                    columns,
                    is_node: false,
                    is_edge: false,
                    inline_constraint_disambiguator: None, // Set during post-processing
                }));

                // Extract constraints from table definition (table-level constraints)
                for constraint in &create_table.constraints {
                    if let Some(constraint_element) = constraint_from_table_constraint(
                        constraint,
                        &create_table.name,
                        &project.default_schema,
                        &parsed.sql_text,
                    ) {
                        model.add_element(ModelElement::Constraint(constraint_element));
                    }
                }

                // Extract inline column constraints (PRIMARY KEY, UNIQUE on columns)
                // These can be truly inline (anonymous) or named with CONSTRAINT keyword
                for col in &create_table.columns {
                    for option in &col.options {
                        if let ColumnOption::Unique { is_primary, .. } = &option.option {
                            // Check if constraint has an explicit name via CONSTRAINT keyword
                            let has_explicit_name = option.name.is_some();
                            let constraint_name = option
                                .name
                                .as_ref()
                                .map(|n| n.value.clone())
                                .unwrap_or_else(|| {
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

                            model.add_element(ModelElement::Constraint(ConstraintElement {
                                name: constraint_name,
                                table_schema: schema.clone(),
                                table_name: name.clone(),
                                constraint_type,
                                columns: vec![ConstraintColumn::new(col.name.value.clone())],
                                definition: None,
                                referenced_table: None,
                                referenced_columns: None,
                                is_clustered: None,
                                is_inline: !has_explicit_name, // Only inline if no CONSTRAINT keyword
                                inline_constraint_disambiguator: None,
                            }));
                        }
                    }
                }

                // Extract inline default constraints from column definitions
                // IMPORTANT: In SQL Server, "CONSTRAINT [name] NOT NULL DEFAULT" names the NOT NULL
                // constraint, NOT the DEFAULT constraint. Only "NOT NULL CONSTRAINT [name] DEFAULT"
                // names the default. Since sqlparser associates the name with NOT NULL in the first
                // case, we should NOT use that name for the DEFAULT - it makes the default unnamed.
                for col in &create_table.columns {
                    for option in &col.options {
                        if let ColumnOption::Default(expr) = &option.option {
                            // Only use a name if explicitly on the DEFAULT option itself
                            let has_explicit_name = option.name.is_some();
                            let constraint_name = option
                                .name
                                .as_ref()
                                .map(|n| n.value.clone())
                                .unwrap_or_else(|| format!("DF_{}_{}", name, col.name.value));

                            model.add_element(ModelElement::Constraint(ConstraintElement {
                                name: constraint_name,
                                table_schema: schema.clone(),
                                table_name: name.clone(),
                                constraint_type: ConstraintType::Default,
                                columns: vec![ConstraintColumn::new(col.name.value.clone())],
                                definition: Some(expr.to_string()),
                                referenced_table: None,
                                referenced_columns: None,
                                is_clustered: None,
                                is_inline: !has_explicit_name,
                                inline_constraint_disambiguator: None,
                            }));
                        }
                    }
                }

                // Extract inline CHECK constraints from column definitions
                for col in &create_table.columns {
                    for option in &col.options {
                        if let ColumnOption::Check(expr) = &option.option {
                            let is_inline = option.name.is_none();
                            let constraint_name = option
                                .name
                                .as_ref()
                                .map(|n| n.value.clone())
                                .unwrap_or_else(|| format!("CK_{}_{}", name, col.name.value));
                            model.add_element(ModelElement::Constraint(ConstraintElement {
                                name: constraint_name,
                                table_schema: schema.clone(),
                                table_name: name.clone(),
                                constraint_type: ConstraintType::Check,
                                columns: vec![ConstraintColumn::new(col.name.value.clone())],
                                definition: Some(expr.to_string()),
                                referenced_table: None,
                                referenced_columns: None,
                                is_clustered: None,
                                is_inline,
                                inline_constraint_disambiguator: None,
                            }));
                        }
                    }
                }

                // Add extracted default constraints (from T-SQL DEFAULT FOR syntax)
                // These are always named since they use explicit CONSTRAINT keyword
                for default_constraint in &parsed.extracted_defaults {
                    model.add_element(ModelElement::Constraint(ConstraintElement {
                        name: default_constraint.name.clone(),
                        table_schema: schema.clone(),
                        table_name: name.clone(),
                        constraint_type: ConstraintType::Default,
                        columns: vec![ConstraintColumn::new(default_constraint.column.clone())],
                        definition: Some(default_constraint.expression.clone()),
                        referenced_table: None,
                        referenced_columns: None,
                        is_clustered: None,
                        is_inline: false, // Named constraint (uses CONSTRAINT keyword)
                        inline_constraint_disambiguator: None,
                    }));
                }
            }

            Statement::CreateView { name, .. } => {
                let (schema, view_name) = extract_schema_and_name(name, &project.default_schema);
                schemas.insert(schema.clone());

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
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "unnamed_index".to_string());

                let (table_schema, table_name) =
                    extract_schema_and_name(&create_index.table_name, &project.default_schema);

                let columns: Vec<String> = create_index
                    .columns
                    .iter()
                    .map(|c| c.expr.to_string())
                    .collect();

                // Extract INCLUDE columns if available from sqlparser
                let include_columns: Vec<String> = create_index
                    .include
                    .iter()
                    .map(|c| c.value.clone())
                    .collect();

                // Extract index options from WITH clause
                let fill_factor = extract_fill_factor(&create_index.with);

                model.add_element(ModelElement::Index(IndexElement {
                    name: index_name,
                    table_schema,
                    table_name,
                    columns,
                    include_columns,
                    is_unique: create_index.unique,
                    is_clustered: false, // sqlparser doesn't expose this directly
                    fill_factor,
                }));
            }

            // Handle procedures that sqlparser successfully parsed (generic SQL syntax)
            Statement::CreateProcedure { name, .. } => {
                let (schema, proc_name) = extract_schema_and_name(name, &project.default_schema);
                schemas.insert(schema.clone());
                let is_native = is_natively_compiled(&parsed.sql_text);

                model.add_element(ModelElement::Procedure(ProcedureElement {
                    schema,
                    name: proc_name,
                    definition: parsed.sql_text.clone(),
                    parameters: vec![], // Parameters stored in definition
                    is_natively_compiled: is_native,
                }));
            }

            // Handle functions that sqlparser successfully parsed (generic SQL syntax)
            Statement::CreateFunction(create_func) => {
                let (schema, func_name) =
                    extract_schema_and_name(&create_func.name, &project.default_schema);
                schemas.insert(schema.clone());

                // Detect function type from raw SQL (more reliable than parsed return type)
                // Inline TVF: "RETURNS TABLE" without table variable
                // Multi-statement TVF: "RETURNS @variable TABLE (...)"
                let sql_upper = parsed.sql_text.to_uppercase();
                let function_type = if sql_upper.contains("RETURNS TABLE") {
                    FunctionType::InlineTableValued
                } else if sql_upper.contains("RETURNS @") {
                    FunctionType::TableValued
                } else if create_func
                    .return_type
                    .as_ref()
                    .map(|t| t.to_string().to_uppercase().contains("TABLE"))
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

                // Normalize schema name (remove any remaining brackets)
                let normalized = schema_name_str
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string();

                schemas.insert(normalized.clone());
                model.add_element(ModelElement::Schema(SchemaElement {
                    name: normalized,
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

            // Ignore other statements (DML, etc.)
            _ => {}
        }
    }

    // Add schema elements for any schemas we discovered
    // (Built-in schemas like dbo are included in the model but will be filtered
    // during XML generation - they're written as ExternalSource="BuiltIns" references)
    for schema in schemas {
        if !model
            .elements
            .iter()
            .any(|e| matches!(e, ModelElement::Schema(s) if s.name == schema))
        {
            model.add_element(ModelElement::Schema(SchemaElement {
                name: schema,
                authorization: None,
            }));
        }
    }

    // Sort elements by type (following DotNet order) then by name for deterministic output
    sort_elements(&mut model.elements);

    // Assign disambiguators to inline constraints and link to columns/tables
    // This must happen after sorting because disambiguator values follow element order
    assign_inline_constraint_disambiguators(&mut model.elements);

    Ok(model)
}

/// Get the sort priority for an element type (lower = earlier in output)
/// This matches the DotNet DacFx element ordering:
///   1. SqlSchema
///   2. SqlTable
///   3. SqlView
///   4. SqlProcedure
///   5. SqlScalarFunction / SqlInlineTableValuedFunction / SqlMultiStatementTableValuedFunction
///   6. SqlIndex
///   7. SqlFullTextIndex
///   8. SqlFullTextCatalog
///   9. SqlPrimaryKeyConstraint
///   10. SqlForeignKeyConstraint
///   11. SqlUniqueConstraint
///   12. SqlCheckConstraint
///   13. SqlDefaultConstraint
///   14. SqlSequence
///   15. SqlTableType
///   16. SqlExtendedProperty
///   17. SqlDmlTrigger (Raw elements)
fn element_type_priority(element: &ModelElement) -> u32 {
    match element {
        ModelElement::Schema(_) => 1,
        ModelElement::Table(_) => 2,
        ModelElement::View(_) => 3,
        ModelElement::Procedure(_) => 4,
        ModelElement::Function(_) => 5,
        ModelElement::Index(_) => 6,
        ModelElement::FullTextIndex(_) => 7,
        ModelElement::FullTextCatalog(_) => 8,
        ModelElement::Constraint(c) => match c.constraint_type {
            ConstraintType::PrimaryKey => 9,
            ConstraintType::ForeignKey => 10,
            ConstraintType::Unique => 11,
            ConstraintType::Check => 12,
            ConstraintType::Default => 13,
        },
        ModelElement::Sequence(_) => 14,
        ModelElement::UserDefinedType(_) => 15,
        ModelElement::ScalarType(_) => 15, // Same priority as table types
        ModelElement::ExtendedProperty(_) => 16,
        ModelElement::Trigger(_) => 17,
        ModelElement::Raw(r) => match r.sql_type.as_str() {
            "SqlDmlTrigger" => 17,
            _ => 99, // Unknown raw types go last
        },
    }
}

/// Sort elements by type priority then by name for deterministic output
fn sort_elements(elements: &mut [ModelElement]) {
    elements.sort_by(|a, b| {
        let priority_a = element_type_priority(a);
        let priority_b = element_type_priority(b);

        // First compare by type priority
        match priority_a.cmp(&priority_b) {
            std::cmp::Ordering::Equal => {
                // Same type, sort by full_name for deterministic ordering
                a.full_name().cmp(&b.full_name())
            }
            other => other,
        }
    });
}

/// Assign disambiguator values to inline constraints and build linkages to columns/tables.
///
/// DotNet's DacFx assigns sequential disambiguator values starting at 3 for inline constraints.
/// The order is:
/// 1. Each inline constraint gets a disambiguator in element order
/// 2. Each table with inline constraints gets a disambiguator after all constraints
/// 3. Columns with inline constraints get AttachedAnnotation referencing their constraints
/// 4. Named constraints at table-level get AttachedAnnotation referencing their table
fn assign_inline_constraint_disambiguators(elements: &mut [ModelElement]) {
    use std::collections::HashMap;

    // DotNet starts disambiguator values at 3
    let mut next_disambiguator: u32 = 3;

    // Map: (table_schema, table_name, column_name) -> Vec<disambiguator>
    let mut column_annotations: HashMap<(String, String, String), Vec<u32>> = HashMap::new();

    // Map: (table_schema, table_name) -> table_disambiguator
    let mut table_disambiguators: HashMap<(String, String), u32> = HashMap::new();

    // Tables that have inline constraints
    let mut tables_with_inline_constraints: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    // First pass: Assign disambiguators to inline constraints and track columns
    for element in elements.iter_mut() {
        if let ModelElement::Constraint(constraint) = element {
            if constraint.is_inline {
                let disambiguator = next_disambiguator;
                next_disambiguator += 1;
                constraint.inline_constraint_disambiguator = Some(disambiguator);

                // Track that this table has inline constraints
                tables_with_inline_constraints.insert((
                    constraint.table_schema.clone(),
                    constraint.table_name.clone(),
                ));

                // Link the constraint's column(s) to this disambiguator
                for col in &constraint.columns {
                    let key = (
                        constraint.table_schema.clone(),
                        constraint.table_name.clone(),
                        col.name.clone(),
                    );
                    column_annotations
                        .entry(key)
                        .or_default()
                        .push(disambiguator);
                }
            }
        }
    }

    // Second pass: Assign disambiguators to tables that have inline constraints
    // and link named constraints to their table's disambiguator
    for element in elements.iter_mut() {
        if let ModelElement::Table(table) = element {
            let table_key = (table.schema.clone(), table.name.clone());
            if tables_with_inline_constraints.contains(&table_key) {
                let disambiguator = next_disambiguator;
                next_disambiguator += 1;
                table.inline_constraint_disambiguator = Some(disambiguator);
                table_disambiguators.insert(table_key, disambiguator);
            }
        }
    }

    // Third pass: Link named (non-inline) table-level constraints to their table's disambiguator
    for element in elements.iter_mut() {
        if let ModelElement::Constraint(constraint) = element {
            if !constraint.is_inline {
                // Named constraint - link to table's disambiguator if table has inline constraints
                let table_key = (
                    constraint.table_schema.clone(),
                    constraint.table_name.clone(),
                );
                if let Some(&table_disambiguator) = table_disambiguators.get(&table_key) {
                    constraint.inline_constraint_disambiguator = Some(table_disambiguator);
                }
            }
        }
    }

    // Fourth pass: Assign attached_annotations to columns
    for element in elements.iter_mut() {
        if let ModelElement::Table(table) = element {
            for column in &mut table.columns {
                let key = (
                    table.schema.clone(),
                    table.name.clone(),
                    column.name.clone(),
                );
                if let Some(annotations) = column_annotations.get(&key) {
                    column.attached_annotations = annotations.clone();
                }
            }
        }
    }

    // Fifth pass: Assign disambiguators to fulltext indexes
    // Fulltext indexes share the same name as their table, so they need disambiguators
    for element in elements.iter_mut() {
        if let ModelElement::FullTextIndex(fulltext) = element {
            let disambiguator = next_disambiguator;
            next_disambiguator += 1;
            fulltext.disambiguator = Some(disambiguator);
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

    // Check for ROWGUIDCOL - sqlparser doesn't have native support, so check the column options text
    // This is a T-SQL specific feature that may appear in the original SQL
    let is_rowguidcol = col.options.iter().any(|opt| {
        format!("{:?}", opt.option)
            .to_uppercase()
            .contains("ROWGUIDCOL")
    });

    // Check for SPARSE - T-SQL specific feature
    let is_sparse = col.options.iter().any(|opt| {
        format!("{:?}", opt.option)
            .to_uppercase()
            .contains("SPARSE")
    });

    // Check for FILESTREAM - T-SQL specific feature for VARBINARY(MAX) columns
    let is_filestream = col.options.iter().any(|opt| {
        format!("{:?}", opt.option)
            .to_uppercase()
            .contains("FILESTREAM")
    });

    let (max_length, precision, scale) = extract_type_params(&col.data_type);

    // Note: attached_annotations are populated during post-processing
    // after disambiguators are assigned to inline constraints

    ColumnElement {
        name: col.name.value.clone(),
        data_type: col.data_type.to_string(),
        nullability,
        is_identity,
        is_rowguidcol,
        is_sparse,
        is_filestream,
        default_value,
        max_length,
        precision,
        scale,
        attached_annotations: Vec::new(), // Populated during post-processing
        computed_expression,
        is_persisted,
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
    if data_type.to_uppercase().contains("MAX") {
        return (Some(-1), None, None);
    }

    let base_type = data_type.to_uppercase();

    // Parse parameters from type string like "NVARCHAR(50)" or "DECIMAL(18, 2)"
    let re = regex::Regex::new(r"\((\d+)(?:\s*,\s*(\d+))?\)").unwrap();
    if let Some(caps) = re.captures(data_type) {
        let first: Option<i32> = caps.get(1).and_then(|m| m.as_str().parse().ok());
        let second: Option<u8> = caps.get(2).and_then(|m| m.as_str().parse().ok());

        if base_type.starts_with("DECIMAL") || base_type.starts_with("NUMERIC") {
            // For DECIMAL/NUMERIC: first is precision, second is scale
            return (None, first.map(|v| v as u8), second);
        } else if base_type.starts_with("DATETIME2")
            || base_type.starts_with("TIME")
            || base_type.starts_with("DATETIMEOFFSET")
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
    if base_type.starts_with("DATETIME2")
        || base_type.starts_with("TIME")
        || base_type.starts_with("DATETIMEOFFSET")
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
        computed_expression: col.computed_expression.clone(),
        is_persisted: col.is_persisted,
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
        } => Some(ConstraintElement {
            name: name.clone(),
            table_schema: table_schema.to_string(),
            table_name: table_name.to_string(),
            constraint_type: ConstraintType::PrimaryKey,
            columns: columns
                .iter()
                .map(|c| ConstraintColumn::with_direction(c.name.clone(), c.descending))
                .collect(),
            definition: None,
            referenced_table: None,
            referenced_columns: None,
            is_clustered: Some(*is_clustered),
            is_inline: false, // Table-level constraint (uses CONSTRAINT keyword)
            inline_constraint_disambiguator: None,
        }),
        ExtractedTableConstraint::ForeignKey {
            name,
            columns,
            referenced_table,
            referenced_columns,
        } => Some(ConstraintElement {
            name: name.clone(),
            table_schema: table_schema.to_string(),
            table_name: table_name.to_string(),
            constraint_type: ConstraintType::ForeignKey,
            columns: columns
                .iter()
                .map(|c| ConstraintColumn::new(c.clone()))
                .collect(),
            definition: None,
            referenced_table: Some(referenced_table.clone()),
            referenced_columns: Some(referenced_columns.clone()),
            is_clustered: None,
            is_inline: false, // Table-level constraint (uses CONSTRAINT keyword)
            inline_constraint_disambiguator: None,
        }),
        ExtractedTableConstraint::Unique {
            name,
            columns,
            is_clustered,
        } => Some(ConstraintElement {
            name: name.clone(),
            table_schema: table_schema.to_string(),
            table_name: table_name.to_string(),
            constraint_type: ConstraintType::Unique,
            columns: columns
                .iter()
                .map(|c| ConstraintColumn::with_direction(c.name.clone(), c.descending))
                .collect(),
            definition: None,
            referenced_table: None,
            referenced_columns: None,
            is_clustered: Some(*is_clustered),
            is_inline: false, // Table-level constraint (uses CONSTRAINT keyword)
            inline_constraint_disambiguator: None,
        }),
        ExtractedTableConstraint::Check { name, expression } => Some(ConstraintElement {
            name: name.clone(),
            table_schema: table_schema.to_string(),
            table_name: table_name.to_string(),
            constraint_type: ConstraintType::Check,
            columns: vec![],
            definition: Some(expression.clone()),
            referenced_table: None,
            referenced_columns: None,
            is_clustered: None,
            is_inline: false, // Table-level constraint (uses CONSTRAINT keyword)
            inline_constraint_disambiguator: None,
        }),
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
    let upper_sql = raw_sql.to_uppercase();

    // Try to find the constraint definition in the SQL
    // First, try to find a named constraint
    let constraint_name_upper = constraint_name.to_uppercase();
    let constraint_pattern = format!("CONSTRAINT [{}]", constraint_name_upper);
    let constraint_pattern_bare = format!("CONSTRAINT {}", constraint_name_upper);

    // Find the position of the constraint in the SQL
    let constraint_start = upper_sql
        .find(&constraint_pattern)
        .or_else(|| upper_sql.find(&constraint_pattern_bare));

    if let Some(start_pos) = constraint_start {
        // Look at the text after the constraint name for CLUSTERED/NONCLUSTERED
        let remaining = &upper_sql[start_pos..];

        // Find the end of this constraint definition (next CONSTRAINT or end of CREATE TABLE)
        let end_pos = remaining[20..]
            .find("CONSTRAINT")
            .map(|p| p + 20)
            .unwrap_or(remaining.len());
        let constraint_def = &remaining[..end_pos];

        // Check for NONCLUSTERED before CLUSTERED to avoid matching "NONCLUSTERED" as "CLUSTERED"
        if constraint_def.contains("NONCLUSTERED") {
            return Some(false);
        } else if constraint_def.contains("CLUSTERED") {
            return Some(true);
        }
    }

    // If not found by name, look for PRIMARY KEY or UNIQUE patterns
    let keyword = if is_primary_key {
        "PRIMARY KEY"
    } else {
        "UNIQUE"
    };

    if let Some(keyword_pos) = upper_sql.find(keyword) {
        // Look at the text after PRIMARY KEY or UNIQUE
        let remaining = &upper_sql[keyword_pos..];
        let end_pos = remaining
            .find('(')
            .unwrap_or_else(|| remaining.len().min(100));
        let constraint_def = &remaining[..end_pos];

        if constraint_def.contains("NONCLUSTERED") {
            return Some(false);
        } else if constraint_def.contains("CLUSTERED") {
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

            Some(ConstraintElement {
                name: constraint_name,
                table_schema,
                table_name: table_name_str,
                constraint_type: ConstraintType::PrimaryKey,
                columns: columns
                    .iter()
                    .map(|c| ConstraintColumn::new(c.value.clone()))
                    .collect(),
                definition: None,
                referenced_table: None,
                referenced_columns: None,
                is_clustered,
                is_inline: false, // Table-level constraint (uses CONSTRAINT keyword)
                inline_constraint_disambiguator: None,
            })
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

            Some(ConstraintElement {
                name: constraint_name,
                table_schema: table_schema.clone(),
                table_name: table_name_str,
                constraint_type: ConstraintType::ForeignKey,
                columns: columns
                    .iter()
                    .map(|c| ConstraintColumn::new(c.value.clone()))
                    .collect(),
                definition: None,
                referenced_table: Some(formatted_foreign_table),
                referenced_columns: Some(
                    referred_columns.iter().map(|c| c.value.clone()).collect(),
                ),
                is_clustered: None,
                is_inline: false, // Table-level constraint (uses CONSTRAINT keyword)
                inline_constraint_disambiguator: None,
            })
        }
        TableConstraint::Unique { name, columns, .. } => {
            let constraint_name = name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_else(|| format!("UQ_{}", table_name_str));

            // Determine clustering from raw SQL since sqlparser doesn't expose it
            // Default for UNIQUE is NONCLUSTERED, so only set to true if CLUSTERED is found
            let is_clustered = extract_constraint_clustering(raw_sql, &constraint_name, false);

            Some(ConstraintElement {
                name: constraint_name,
                table_schema,
                table_name: table_name_str,
                constraint_type: ConstraintType::Unique,
                columns: columns
                    .iter()
                    .map(|c| ConstraintColumn::new(c.value.clone()))
                    .collect(),
                definition: None,
                referenced_table: None,
                referenced_columns: None,
                is_clustered,
                is_inline: false, // Table-level constraint (uses CONSTRAINT keyword)
                inline_constraint_disambiguator: None,
            })
        }
        TableConstraint::Check { name, expr } => {
            let constraint_name = name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_else(|| format!("CK_{}", table_name_str));

            Some(ConstraintElement {
                name: constraint_name,
                table_schema,
                table_name: table_name_str,
                constraint_type: ConstraintType::Check,
                columns: vec![],
                definition: Some(expr.to_string()),
                referenced_table: None,
                referenced_columns: None,
                is_clustered: None,
                is_inline: false, // Table-level constraint (uses CONSTRAINT keyword)
                inline_constraint_disambiguator: None,
            })
        }
        _ => None,
    }
}

/// Check if a procedure or function definition uses NATIVE_COMPILATION
fn is_natively_compiled(definition: &str) -> bool {
    let upper = definition.to_uppercase();
    // Look for WITH NATIVE_COMPILATION in the definition
    // It can appear as "WITH NATIVE_COMPILATION" or "WITH NATIVE_COMPILATION, SCHEMABINDING" etc.
    upper.contains("NATIVE_COMPILATION")
}

/// Extract FILLFACTOR from index WITH clause options
fn extract_fill_factor(with_options: &[Expr]) -> Option<u8> {
    for expr in with_options {
        if let Expr::BinaryOp { left, op, right } = expr {
            if *op == BinaryOperator::Eq {
                // Check if the left side is FILLFACTOR identifier
                if let Expr::Identifier(ident) = left.as_ref() {
                    if ident.value.to_uppercase() == "FILLFACTOR" {
                        // Extract the numeric value from the right side
                        if let Expr::Value(value) = right.as_ref() {
                            if let sqlparser::ast::Value::Number(n, _) = value {
                                if let Ok(val) = n.parse::<u8>() {
                                    return Some(val);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

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
    let upper = sql.to_uppercase();

    // WITH SCHEMABINDING appears before AS in the view definition
    let is_schema_bound = upper.contains("WITH SCHEMABINDING")
        || upper.contains("WITH SCHEMABINDING,")
        || upper.contains(", SCHEMABINDING")
        || upper.contains(",SCHEMABINDING");

    // WITH CHECK OPTION appears at the end of the view definition
    let is_with_check_option = upper.contains("WITH CHECK OPTION");

    // VIEW_METADATA appears in WITH clause before AS
    let is_metadata_reported = upper.contains("VIEW_METADATA");

    (is_schema_bound, is_with_check_option, is_metadata_reported)
}
