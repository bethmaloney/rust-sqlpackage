//! Build database model from parsed SQL statements

use std::collections::HashSet;

use anyhow::Result;
use sqlparser::ast::{ColumnDef, ColumnOption, DataType, ObjectName, Statement, TableConstraint};

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
    RawElement, SchemaElement, SequenceElement, TableElement, TableTypeColumnElement,
    TableTypeConstraint, UserDefinedTypeElement, ViewElement,
};

/// Build a database model from parsed statements
pub fn build_model(statements: &[ParsedStatement], project: &SqlProject) -> Result<DatabaseModel> {
    let mut model = DatabaseModel::new();
    let mut schemas: HashSet<String> = HashSet::new();

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
                    };
                    let is_natively_compiled = is_natively_compiled(&parsed.sql_text);
                    let param_elements = parameters
                        .iter()
                        .map(|p| param_from_extracted(p))
                        .collect();
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
                } => {
                    model.add_element(ModelElement::Index(IndexElement {
                        name: name.clone(),
                        table_schema: table_schema.clone(),
                        table_name: table_name.clone(),
                        columns: columns.clone(),
                        include_columns: include_columns.clone(),
                        is_unique: *is_unique,
                        is_clustered: *is_clustered,
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
                    let column_elements: Vec<FullTextColumnElement> = columns
                        .iter()
                        .map(|c| fulltext_column_from_extracted(c))
                        .collect();
                    model.add_element(ModelElement::FullTextIndex(FullTextIndexElement {
                        table_schema: table_schema.clone(),
                        table_name: table_name.clone(),
                        columns: column_elements,
                        key_index: key_index.clone(),
                        catalog: catalog.clone(),
                        change_tracking: change_tracking.clone(),
                    }));
                }
                FallbackStatementType::FullTextCatalog { name, is_default } => {
                    model.add_element(ModelElement::FullTextCatalog(FullTextCatalogElement {
                        name: name.clone(),
                        is_default: *is_default,
                    }));
                }
                FallbackStatementType::Sequence { schema, name } => {
                    schemas.insert(schema.clone());
                    model.add_element(ModelElement::Sequence(SequenceElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                    }));
                }
                FallbackStatementType::UserDefinedType {
                    schema,
                    name,
                    columns,
                    constraints,
                } => {
                    schemas.insert(schema.clone());
                    let column_elements: Vec<TableTypeColumnElement> =
                        columns.iter().map(|c| table_type_column_from_extracted(c)).collect();
                    let constraint_elements: Vec<TableTypeConstraint> =
                        constraints.iter().map(|c| table_type_constraint_from_extracted(c)).collect();
                    model.add_element(ModelElement::UserDefinedType(UserDefinedTypeElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        columns: column_elements,
                        constraints: constraint_elements,
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
                        .map(|c| column_from_fallback_table(c))
                        .collect();

                    // Add the table element
                    model.add_element(ModelElement::Table(TableElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        columns: model_columns,
                        is_node: *is_node,
                        is_edge: *is_edge,
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
                            }));
                        }
                    }

                    // Add inline CHECK constraints from column definitions
                    for col in columns {
                        if let Some(check_expr) = &col.check_expression {
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
                            }));
                        }
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
                        "TRIGGER" => Some("SqlDmlTrigger"),
                        // Skip other object types - they would cause deployment failures
                        // ALTER TABLE, INDEX, FULLTEXT INDEX, etc. are not supported as raw elements
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
                    .map(|c| column_from_def(c))
                    .collect();

                model.add_element(ModelElement::Table(TableElement {
                    schema: schema.clone(),
                    name: name.clone(),
                    columns,
                    is_node: false,
                    is_edge: false,
                }));

                // Extract constraints from table definition (table-level constraints)
                for constraint in &create_table.constraints {
                    if let Some(constraint_element) = constraint_from_table_constraint(
                        constraint,
                        &create_table.name,
                        &project.default_schema,
                    ) {
                        model.add_element(ModelElement::Constraint(constraint_element));
                    }
                }

                // Extract inline column constraints (PRIMARY KEY, UNIQUE on columns)
                for col in &create_table.columns {
                    for option in &col.options {
                        if let ColumnOption::Unique { is_primary, .. } = &option.option {
                            let constraint_name = if *is_primary {
                                format!("PK_{}", name)
                            } else {
                                format!("UQ_{}_{}", name, col.name.value)
                            };

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
                            }));
                        }
                    }
                }

                // Extract inline named default constraints from column definitions
                // Note: sqlparser may associate the constraint name with NOT NULL when SQL is
                // written as "CONSTRAINT [name] NOT NULL DEFAULT (value)" - we need to handle
                // this by looking for a named NOT NULL/NULL followed by an unnamed DEFAULT
                for col in &create_table.columns {
                    let mut pending_constraint_name: Option<String> = None;

                    for option in &col.options {
                        match &option.option {
                            ColumnOption::NotNull | ColumnOption::Null => {
                                // If this null option has a name, save it for potential DEFAULT
                                if let Some(constraint_name) = &option.name {
                                    pending_constraint_name = Some(constraint_name.value.clone());
                                }
                            }
                            ColumnOption::Default(expr) => {
                                // Use explicit name on DEFAULT, pending name from NOT NULL, or generate one
                                let constraint_name = option
                                    .name
                                    .as_ref()
                                    .map(|n| n.value.clone())
                                    .or(pending_constraint_name.take())
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
                                }));
                                // Reset pending name
                                pending_constraint_name = None;
                            }
                            _ => {
                                // Reset pending name for other options
                                pending_constraint_name = None;
                            }
                        }
                    }
                }

                // Extract inline CHECK constraints from column definitions
                for col in &create_table.columns {
                    for option in &col.options {
                        if let ColumnOption::Check(expr) = &option.option {
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
                            }));
                        }
                    }
                }

                // Add extracted default constraints (from T-SQL DEFAULT FOR syntax)
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
                    }));
                }
            }

            Statement::CreateView { name, .. } => {
                let (schema, view_name) = extract_schema_and_name(name, &project.default_schema);
                schemas.insert(schema.clone());

                model.add_element(ModelElement::View(ViewElement {
                    schema,
                    name: view_name,
                    definition: parsed.sql_text.clone(),
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

                model.add_element(ModelElement::Index(IndexElement {
                    name: index_name,
                    table_schema,
                    table_name,
                    columns,
                    include_columns,
                    is_unique: create_index.unique,
                    is_clustered: false, // sqlparser doesn't expose this directly
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

                // Detect function type from return type
                let function_type = if create_func
                    .return_type
                    .as_ref()
                    .map(|t| t.to_string().to_uppercase().contains("TABLE"))
                    .unwrap_or(false)
                {
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
                // Normalize schema name (remove brackets if present)
                let name = schema_name.to_string();
                let normalized = name
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string();
                schemas.insert(normalized.clone());
                model.add_element(ModelElement::Schema(SchemaElement { name: normalized }));
            }

            // Handle EXEC statements (for sp_addextendedproperty)
            Statement::Execute { name, .. } => {
                // Check if this is sp_addextendedproperty
                let proc_name = name.0.iter().map(|p| p.value.as_str()).collect::<Vec<_>>().join(".");
                if proc_name.to_lowercase() == "sp_addextendedproperty" {
                    // Use the original SQL text to extract the extended property
                    if let Some(property) = crate::parser::extract_extended_property_from_sql(&parsed.sql_text) {
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
            model.add_element(ModelElement::Schema(SchemaElement { name: schema }));
        }
    }

    Ok(model)
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

fn column_from_def(col: &ColumnDef) -> ColumnElement {
    let mut is_nullable = true;
    let mut is_identity = false;
    let mut default_value = None;
    let mut has_inline_constraint = false;
    let mut computed_expression: Option<String> = None;
    let mut is_persisted = false;

    for option in &col.options {
        match &option.option {
            ColumnOption::NotNull => is_nullable = false,
            ColumnOption::Null => is_nullable = true,
            ColumnOption::Default(expr) => {
                default_value = Some(expr.to_string());
                has_inline_constraint = true;
            }
            ColumnOption::Identity(_) => is_identity = true,
            ColumnOption::Check(_) => {
                has_inline_constraint = true;
            }
            ColumnOption::Unique { .. } => {
                has_inline_constraint = true;
            }
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
                    is_persisted = matches!(
                        mode,
                        sqlparser::ast::GeneratedExpressionMode::Stored
                    );
                }
            }
            _ => {}
        }
    }

    // Check for ROWGUIDCOL - sqlparser doesn't have native support, so check the column options text
    // This is a T-SQL specific feature that may appear in the original SQL
    let is_rowguidcol = col
        .options
        .iter()
        .any(|opt| format!("{:?}", opt.option).to_uppercase().contains("ROWGUIDCOL"));

    // Check for SPARSE - T-SQL specific feature
    let is_sparse = col
        .options
        .iter()
        .any(|opt| format!("{:?}", opt.option).to_uppercase().contains("SPARSE"));

    // Check for FILESTREAM - T-SQL specific feature for VARBINARY(MAX) columns
    let is_filestream = col
        .options
        .iter()
        .any(|opt| format!("{:?}", opt.option).to_uppercase().contains("FILESTREAM"));

    let (max_length, precision, scale) = extract_type_params(&col.data_type);

    // Generate disambiguator from column name hash if column has inline constraints
    let inline_constraint_disambiguator = if has_inline_constraint {
        Some(generate_disambiguator(&col.name.value))
    } else {
        None
    };

    ColumnElement {
        name: col.name.value.clone(),
        data_type: col.data_type.to_string(),
        is_nullable,
        is_identity,
        is_rowguidcol,
        is_sparse,
        is_filestream,
        default_value,
        max_length,
        precision,
        scale,
        inline_constraint_disambiguator,
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
        is_nullable: col.is_nullable,
        default_value: col.default_value.clone(),
        max_length,
        precision,
        scale,
    }
}

/// Convert an extracted table type constraint to a TableTypeConstraint
fn table_type_constraint_from_extracted(constraint: &ExtractedTableTypeConstraint) -> TableTypeConstraint {
    match constraint {
        ExtractedTableTypeConstraint::PrimaryKey { columns, is_clustered } => {
            TableTypeConstraint::PrimaryKey {
                columns: columns
                    .iter()
                    .map(|c| ConstraintColumn::with_direction(c.name.clone(), c.descending))
                    .collect(),
                is_clustered: *is_clustered,
            }
        }
        ExtractedTableTypeConstraint::Unique { columns, is_clustered } => {
            TableTypeConstraint::Unique {
                columns: columns
                    .iter()
                    .map(|c| ConstraintColumn::with_direction(c.name.clone(), c.descending))
                    .collect(),
                is_clustered: *is_clustered,
            }
        }
        ExtractedTableTypeConstraint::Check { expression } => {
            TableTypeConstraint::Check {
                expression: expression.clone(),
            }
        }
        ExtractedTableTypeConstraint::Index { name, columns, is_unique, is_clustered } => {
            TableTypeConstraint::Index {
                name: name.clone(),
                columns: columns.clone(),
                is_unique: *is_unique,
                is_clustered: *is_clustered,
            }
        }
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

    // Parse parameters from type string like "NVARCHAR(50)" or "DECIMAL(18, 2)"
    let re = regex::Regex::new(r"\((\d+)(?:\s*,\s*(\d+))?\)").unwrap();
    if let Some(caps) = re.captures(data_type) {
        let first: Option<i32> = caps.get(1).and_then(|m| m.as_str().parse().ok());
        let second: Option<u8> = caps.get(2).and_then(|m| m.as_str().parse().ok());

        let base_type = data_type.to_uppercase();
        if base_type.starts_with("DECIMAL") || base_type.starts_with("NUMERIC") {
            // For DECIMAL/NUMERIC: first is precision, second is scale
            return (None, first.map(|v| v as u8), second);
        } else {
            // For string/binary types: first is length
            return (first, None, None);
        }
    }

    (None, None, None)
}

fn extract_type_params(data_type: &DataType) -> (Option<i32>, Option<u8>, Option<u8>) {
    match data_type {
        DataType::Varchar(len) | DataType::Char(len) | DataType::Nvarchar(len) => {
            let max_length = len.as_ref().and_then(|l| match l {
                sqlparser::ast::CharacterLength::IntegerLength { length, .. } => {
                    Some(*length as i32)
                }
                sqlparser::ast::CharacterLength::Max => Some(-1),
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
        _ => (None, None, None),
    }
}

/// Convert an extracted table column (from fallback parser) to a model column
fn column_from_fallback_table(col: &ExtractedTableColumn) -> ColumnElement {
    let (max_length, precision, scale) = extract_type_params_from_string(&col.data_type);

    // Check if column has inline constraints (default or check)
    let has_inline_constraint =
        col.default_value.is_some() || col.check_expression.is_some();

    // Generate disambiguator from column name hash if column has inline constraints
    let inline_constraint_disambiguator = if has_inline_constraint {
        Some(generate_disambiguator(&col.name))
    } else {
        None
    };

    ColumnElement {
        name: col.name.clone(),
        data_type: col.data_type.clone(),
        is_nullable: col.is_nullable,
        is_identity: col.is_identity,
        is_rowguidcol: col.is_rowguidcol,
        is_sparse: col.is_sparse,
        is_filestream: col.is_filestream,
        default_value: col.default_value.clone(),
        max_length,
        precision,
        scale,
        inline_constraint_disambiguator,
        computed_expression: col.computed_expression.clone(),
        is_persisted: col.is_persisted,
    }
}

/// Generate a disambiguator value from a string (e.g., column name)
/// Uses a simple hash to generate a consistent numeric value
fn generate_disambiguator(name: &str) -> u32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    // Use modulo to keep the number in a reasonable range similar to DacFx output
    (hasher.finish() % 1_000_000) as u32
}

/// Convert an extracted table constraint (from fallback parser) to a model constraint
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
        }),
    }
}

fn constraint_from_table_constraint(
    constraint: &TableConstraint,
    table_name: &ObjectName,
    default_schema: &str,
) -> Option<ConstraintElement> {
    let (table_schema, table_name_str) = extract_schema_and_name(table_name, default_schema);

    match constraint {
        TableConstraint::PrimaryKey { name, columns, .. } => {
            let constraint_name = name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_else(|| format!("PK_{}", table_name_str));

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
                is_clustered: None,
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
            })
        }
        TableConstraint::Unique { name, columns, .. } => {
            let constraint_name = name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_else(|| format!("UQ_{}", table_name_str));

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
                is_clustered: None,
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

/// Convert an extracted extended property to a model ExtendedPropertyElement
fn extended_property_from_extracted(property: &ExtractedExtendedProperty) -> ExtendedPropertyElement {
    ExtendedPropertyElement {
        property_name: property.property_name.clone(),
        property_value: property.property_value.clone(),
        target_schema: property.level0name.clone(),
        target_object: property.level1name.clone().unwrap_or_default(),
        target_column: property.level2name.clone(),
    }
}
