//! Build database model from parsed SQL statements

use std::collections::HashSet;

use anyhow::Result;
use sqlparser::ast::{
    ColumnDef, ColumnOption, DataType, ObjectName, Statement, TableConstraint,
};

use crate::parser::{FallbackFunctionType, FallbackStatementType, ParsedStatement};
use crate::project::SqlProject;

use super::{
    ColumnElement, ConstraintElement, ConstraintType, DatabaseModel, FunctionElement, FunctionType,
    IndexElement, ModelElement, ProcedureElement, RawElement, SchemaElement, SequenceElement,
    TableElement, UserDefinedTypeElement, ViewElement,
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
                    model.add_element(ModelElement::Procedure(ProcedureElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        parameters: vec![], // T-SQL params not extracted - stored in definition
                    }));
                }
                FallbackStatementType::Function {
                    schema,
                    name,
                    function_type,
                } => {
                    schemas.insert(schema.clone());
                    let func_type = match function_type {
                        FallbackFunctionType::Scalar => FunctionType::Scalar,
                        FallbackFunctionType::TableValued => FunctionType::TableValued,
                    };
                    model.add_element(ModelElement::Function(FunctionElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                        function_type: func_type,
                        parameters: vec![], // T-SQL params not extracted - stored in definition
                        return_type: None,  // Return type is in the definition
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
                FallbackStatementType::Sequence { schema, name } => {
                    schemas.insert(schema.clone());
                    model.add_element(ModelElement::Sequence(SequenceElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                    }));
                }
                FallbackStatementType::UserDefinedType { schema, name } => {
                    schemas.insert(schema.clone());
                    model.add_element(ModelElement::UserDefinedType(UserDefinedTypeElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        definition: parsed.sql_text.clone(),
                    }));
                }
                FallbackStatementType::RawStatement {
                    object_type,
                    schema,
                    name,
                } => {
                    schemas.insert(schema.clone());
                    let sql_type = match object_type.to_uppercase().as_str() {
                        "TABLE" => "SqlTable",
                        "VIEW" => "SqlView",
                        "TRIGGER" => "SqlDmlTrigger",
                        "ALTERTABLE" => "SqlAlterTableStatement",
                        _ => "SqlUnknown",
                    };
                    model.add_element(ModelElement::Raw(RawElement {
                        schema: schema.clone(),
                        name: name.clone(),
                        sql_type: sql_type.to_string(),
                        definition: parsed.sql_text.clone(),
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
                let (schema, name) = extract_schema_and_name(&create_table.name, &project.default_schema);
                schemas.insert(schema.clone());

                let columns = create_table
                    .columns
                    .iter()
                    .map(|c| column_from_def(c))
                    .collect();

                model.add_element(ModelElement::Table(TableElement {
                    schema,
                    name,
                    columns,
                }));

                // Extract constraints from table definition
                for constraint in &create_table.constraints {
                    if let Some(constraint_element) =
                        constraint_from_table_constraint(constraint, &create_table.name, &project.default_schema)
                    {
                        model.add_element(ModelElement::Constraint(constraint_element));
                    }
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

                model.add_element(ModelElement::Procedure(ProcedureElement {
                    schema,
                    name: proc_name,
                    definition: parsed.sql_text.clone(),
                    parameters: vec![], // Parameters stored in definition
                }));
            }

            // Handle functions that sqlparser successfully parsed (generic SQL syntax)
            Statement::CreateFunction(create_func) => {
                let (schema, func_name) = extract_schema_and_name(&create_func.name, &project.default_schema);
                schemas.insert(schema.clone());

                // Detect function type from return type
                let function_type = if create_func.return_type.as_ref().map(|t| t.to_string().to_uppercase().contains("TABLE")).unwrap_or(false) {
                    FunctionType::TableValued
                } else {
                    FunctionType::Scalar
                };

                model.add_element(ModelElement::Function(FunctionElement {
                    schema,
                    name: func_name,
                    definition: parsed.sql_text.clone(),
                    function_type,
                    parameters: vec![], // Parameters stored in definition
                    return_type: create_func.return_type.as_ref().map(|t| t.to_string()),
                }));
            }

            Statement::CreateSchema { schema_name, .. } => {
                let name = schema_name.to_string();
                schemas.insert(name.clone());
                model.add_element(ModelElement::Schema(SchemaElement { name }));
            }

            // Ignore other statements (DML, etc.)
            _ => {}
        }
    }

    // Add schema elements for any schemas we discovered
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
        _ => (default_schema.to_string(), parts.last().cloned().unwrap_or_default()),
    }
}

fn column_from_def(col: &ColumnDef) -> ColumnElement {
    let mut is_nullable = true;
    let is_identity = false;
    let mut default_value = None;

    for option in &col.options {
        match &option.option {
            ColumnOption::NotNull => is_nullable = false,
            ColumnOption::Null => is_nullable = true,
            ColumnOption::Default(expr) => default_value = Some(expr.to_string()),
            _ => {}
        }
    }

    let (max_length, precision, scale) = extract_type_params(&col.data_type);

    ColumnElement {
        name: col.name.value.clone(),
        data_type: col.data_type.to_string(),
        is_nullable,
        is_identity,
        default_value,
        max_length,
        precision,
        scale,
    }
}

fn extract_type_params(data_type: &DataType) -> (Option<i32>, Option<u8>, Option<u8>) {
    match data_type {
        DataType::Varchar(len) | DataType::Char(len) | DataType::Nvarchar(len) => {
            let max_length = len.as_ref().and_then(|l| match l {
                sqlparser::ast::CharacterLength::IntegerLength { length, .. } => Some(*length as i32),
                sqlparser::ast::CharacterLength::Max => Some(-1),
            });
            (max_length, None, None)
        }
        DataType::Decimal(info) | DataType::Numeric(info) => {
            let (precision, scale) = match info {
                sqlparser::ast::ExactNumberInfo::None => (None, None),
                sqlparser::ast::ExactNumberInfo::Precision(p) => (Some(*p as u8), None),
                sqlparser::ast::ExactNumberInfo::PrecisionAndScale(p, s) => (Some(*p as u8), Some(*s as u8)),
            };
            (None, precision, scale)
        }
        _ => (None, None, None),
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
                columns: columns.iter().map(|c| c.value.clone()).collect(),
                definition: None,
                referenced_table: None,
                referenced_columns: None,
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

            Some(ConstraintElement {
                name: constraint_name,
                table_schema: table_schema.clone(),
                table_name: table_name_str,
                constraint_type: ConstraintType::ForeignKey,
                columns: columns.iter().map(|c| c.value.clone()).collect(),
                definition: None,
                referenced_table: Some(foreign_table.to_string()),
                referenced_columns: Some(referred_columns.iter().map(|c| c.value.clone()).collect()),
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
                columns: columns.iter().map(|c| c.value.clone()).collect(),
                definition: None,
                referenced_table: None,
                referenced_columns: None,
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
            })
        }
        _ => None,
    }
}
