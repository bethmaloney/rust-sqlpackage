//! Procedure and function XML writers for dacpac generation.
//!
//! This module handles the generation of XML elements for SQL procedures and functions,
//! including parameter extraction, TVP (table-valued parameter) handling, and multi-statement
//! table-valued function column extraction.

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use regex::Regex;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::tokenizer::{Token, Tokenizer};
use std::io::Write;
use std::sync::LazyLock;

use crate::model::{
    DatabaseModel, FunctionElement, ModelElement, ProcedureElement, UserDefinedTypeElement,
};
use crate::parser::{extract_function_parameters_tokens, extract_procedure_parameters_tokens};

use super::table_writer::{write_column_type_specifier, write_table_type_relationship};
use super::view_writer::{extract_view_columns_and_deps, write_view_columns, ViewColumn};
use super::xml_helpers::{
    escape_newlines_for_attr, write_property, write_property_raw, write_schema_relationship,
    write_script_property,
};
use super::{
    compute_line_offsets, extract_body_dependencies, extract_expression_before_as,
    extract_select_columns, location_to_byte_offset, normalize_type_name, parse_data_type,
    write_body_dependencies, BodyDependency,
};

/// Multi-statement TVF detection: RETURNS @var TABLE (
static MULTI_STMT_TVF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)RETURNS\s+@\w+\s+TABLE\s*\(").unwrap());

// =============================================================================
// Procedure Writing
// =============================================================================

pub(crate) fn write_procedure<W: Write>(
    writer: &mut Writer<W>,
    proc: &ProcedureElement,
    model: &DatabaseModel,
    default_schema: &str,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", proc.schema, proc.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlProcedure"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Extract parameters for both writing and dependency extraction
    let params = extract_procedure_parameters(&proc.definition);

    // Find table type parameters (TVPs) - these have READONLY modifier or reference a table type
    let tvp_params: Vec<(&ProcedureParameter, Option<&UserDefinedTypeElement>)> = params
        .iter()
        .filter_map(|p| {
            // Check if parameter references a table type in the model
            let table_type = find_table_type_for_parameter(&p.data_type, model);
            if table_type.is_some() || p.is_readonly {
                Some((p, table_type))
            } else {
                None
            }
        })
        .collect();

    // Calculate disambiguator for TVP parameters (DotNet uses specific values)
    // The disambiguator is typically 2 for first TVP, 3 for second, etc.
    let tvp_disambiguator_base = 2u32;

    // Extract just the body part (after final AS)
    let body = extract_procedure_body_only(&proc.definition);

    // Write BodyScript property first
    write_script_property(writer, "BodyScript", &body)?;

    // Write IsAnsiNullsOn property (always true for procedures)
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Write IsNativelyCompiled property if true
    if proc.is_natively_compiled {
        write_property(writer, "IsNativelyCompiled", "True")?;
    }

    // Extract and write BodyDependencies
    // For procedures with TVPs, we need special handling for TVP column references
    // For all procedures, we still need regular body dependencies (table refs, param refs, etc.)
    let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    let body_deps = if tvp_params.is_empty() {
        // No TVPs - use regular body dependency extraction
        extract_body_dependencies(&body, &full_name, &param_names)
    } else {
        // Has TVPs - extract TVP-specific dependencies
        extract_body_dependencies_with_tvp(&body, &full_name, &param_names, &tvp_params)
    };
    write_body_dependencies(writer, &body_deps)?;

    // Write DynamicObjects relationship for TVP parameters and CTEs
    // NOTE: Use project's default_schema for unqualified table resolution, NOT the procedure's schema.
    // DotNet resolves unqualified table names to the project's default schema (typically [dbo]).
    write_all_dynamic_objects(writer, &full_name, &body, default_schema, &tvp_params)?;

    // Write Parameters relationship
    if !params.is_empty() {
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let param_rel = BytesStart::new("Relationship").with_attributes([("Name", "Parameters")]);
        writer.write_event(Event::Start(param_rel))?;

        for param in params.iter() {
            writer.write_event(Event::Start(BytesStart::new("Entry")))?;

            // Parameter name must include @ prefix
            let param_name_with_at = if param.name.starts_with('@') {
                param.name.clone()
            } else {
                format!("@{}", param.name)
            };
            let param_name = format!("{}.[{}]", full_name, param_name_with_at);

            // Check if this is a TVP parameter
            let tvp_idx = tvp_params.iter().position(|(p, _)| std::ptr::eq(*p, param));
            let is_tvp = tvp_idx.is_some();
            let disambiguator = tvp_idx.map(|i| tvp_disambiguator_base + i as u32);

            // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
            // Conditional Disambiguator attribute requires separate handling
            let param_elem = if let Some(disamb) = disambiguator {
                let disamb_str = disamb.to_string();
                BytesStart::new("Element").with_attributes([
                    ("Type", "SqlSubroutineParameter"),
                    ("Name", param_name.as_str()),
                    ("Disambiguator", disamb_str.as_str()),
                ])
            } else {
                BytesStart::new("Element").with_attributes([
                    ("Type", "SqlSubroutineParameter"),
                    ("Name", param_name.as_str()),
                ])
            };
            writer.write_event(Event::Start(param_elem))?;

            // Write default value if present
            if let Some(ref default_val) = param.default_value {
                write_script_property(writer, "DefaultExpressionScript", default_val)?;
            }

            // IsOutput property if applicable
            if param.is_output {
                write_property(writer, "IsOutput", "True")?;
            }

            // IsReadOnly property for TVP parameters
            if param.is_readonly || is_tvp {
                write_property(writer, "IsReadOnly", "True")?;
            }

            // Data type relationship - different handling for TVPs vs built-in types
            if is_tvp {
                write_table_type_relationship(writer, &param.data_type)?;
            } else {
                write_data_type_relationship(writer, &param.data_type)?;
            }

            writer.write_event(Event::End(BytesEnd::new("Element")))?;
            writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    write_schema_relationship(writer, &proc.schema)?;

    // Write SysCommentsObjectAnnotation with header contents (matching DotNet format)
    write_procedure_annotation(writer, &proc.definition)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Extract the procedure header (everything up to and including AS).
/// Returns the header text for use in SysCommentsObjectAnnotation.
fn extract_procedure_header(definition: &str) -> String {
    // Use tokenized parsing to find the AS keyword
    if let Some((_as_start, as_end)) = find_procedure_body_separator_as_tokenized(definition) {
        // Return everything up to the end position (after AS)
        // Trim leading whitespace while preserving trailing content
        return definition[..as_end].trim_start().trim_end().to_string();
    }
    String::new()
}

/// Find the byte offset of CREATE within the definition.
/// This is used for the CreateOffset property in SysCommentsObjectAnnotation.
fn find_create_offset(definition: &str) -> usize {
    let def_upper = definition.to_uppercase();
    def_upper.find("CREATE").unwrap_or(0)
}

/// Write SysCommentsObjectAnnotation for a procedure.
/// DotNet emits this annotation with CreateOffset, Length, StartLine, StartColumn, and HeaderContents.
fn write_procedure_annotation<W: Write>(
    writer: &mut Writer<W>,
    definition: &str,
) -> anyhow::Result<()> {
    // Extract header (CREATE PROCEDURE ... AS)
    let header = extract_procedure_header(definition);
    if header.is_empty() {
        // If we can't extract the header, skip the annotation
        return Ok(());
    }

    // Calculate properties
    let total_length = definition.len();
    let create_offset = find_create_offset(definition);

    // Write the annotation
    let annotation =
        BytesStart::new("Annotation").with_attributes([("Type", "SysCommentsObjectAnnotation")]);
    writer.write_event(Event::Start(annotation))?;

    write_property(writer, "CreateOffset", &create_offset.to_string())?;
    write_property(writer, "Length", &total_length.to_string())?;
    write_property(writer, "StartLine", "1")?;
    write_property(writer, "StartColumn", "1")?;
    // Escape newlines for XML attribute value (DotNet uses &#xA; for newlines)
    // Use write_property_raw to avoid double-escaping the & in &#xA;
    let escaped_header = escape_newlines_for_attr(&header);
    write_property_raw(writer, "HeaderContents", &escaped_header)?;

    writer.write_event(Event::End(BytesEnd::new("Annotation")))?;

    Ok(())
}

// =============================================================================
// Function Writing
// =============================================================================

pub(crate) fn write_function<W: Write>(
    writer: &mut Writer<W>,
    func: &FunctionElement,
    model: &DatabaseModel,
    default_schema: &str,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", func.schema, func.name);
    let type_name = match func.function_type {
        crate::model::FunctionType::Scalar => "SqlScalarFunction",
        crate::model::FunctionType::TableValued => "SqlMultiStatementTableValuedFunction",
        crate::model::FunctionType::InlineTableValued => "SqlInlineTableValuedFunction",
    };

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", type_name), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Write IsAnsiNullsOn property (always true for functions)
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Write IsNativelyCompiled property if true
    if func.is_natively_compiled {
        write_property(writer, "IsNativelyCompiled", "True")?;
    }

    // Extract function body for dependency analysis
    let body = extract_function_body(&func.definition);
    let header = extract_function_header(&func.definition);

    // Extract function parameters for dependency analysis
    let func_params = extract_function_parameters(&func.definition);
    let param_names: Vec<String> = func_params.iter().map(|p| p.name.clone()).collect();

    // Extract and write BodyDependencies
    let body_deps = extract_body_dependencies(&body, &full_name, &param_names);
    write_body_dependencies(writer, &body_deps)?;

    // Write DynamicObjects relationship for CTEs, temp tables, and table variables
    // Functions don't have TVP parameters like procedures, so we pass an empty slice
    // NOTE: Use project's default_schema for unqualified table resolution, NOT the function's schema.
    // DotNet resolves unqualified table names to the project's default schema (typically [dbo]).
    let empty_tvp_params: Vec<(&ProcedureParameter, Option<&UserDefinedTypeElement>)> = Vec::new();
    write_all_dynamic_objects(writer, &full_name, &body, default_schema, &empty_tvp_params)?;

    // For inline TVFs, write Columns relationship (after BodyDependencies, before FunctionBody)
    // NOTE: Use project's default_schema for unqualified table resolution, NOT the function's schema.
    // DotNet resolves unqualified table names to the project's default schema (typically [dbo]).
    if matches!(
        func.function_type,
        crate::model::FunctionType::InlineTableValued
    ) {
        let inline_tvf_columns =
            extract_inline_tvf_columns(&body, &full_name, default_schema, model);
        if !inline_tvf_columns.is_empty() {
            write_view_columns(writer, &full_name, &inline_tvf_columns)?;
        }
    }

    // For multi-statement TVFs, write Columns relationship from RETURNS @Table TABLE definition
    if matches!(func.function_type, crate::model::FunctionType::TableValued) {
        let tvf_columns = extract_multi_statement_tvf_columns(&func.definition);
        if !tvf_columns.is_empty() {
            write_tvf_columns(writer, &full_name, &tvf_columns)?;
        }
    }

    // Write FunctionBody relationship with SqlScriptFunctionImplementation
    // BodyScript contains only the function body (BEGIN...END), not the header
    write_function_body_with_annotation(writer, &body, &header)?;

    // Write Parameters relationship for function parameters
    write_function_parameters(writer, &func_params, &full_name)?;

    write_schema_relationship(writer, &func.schema)?;

    // Write Type relationship for return type (scalar functions only)
    if matches!(func.function_type, crate::model::FunctionType::Scalar) {
        if let Some(ref return_type) = func.return_type {
            write_function_return_type(writer, return_type)?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

// =============================================================================
// Parameter Structs and Extraction
// =============================================================================

/// Represents an extracted procedure parameter
#[derive(Debug)]
pub(crate) struct ProcedureParameter {
    pub name: String,
    pub data_type: String,
    pub is_output: bool,
    /// Whether this is a READONLY table-valued parameter
    pub is_readonly: bool,
    #[allow(dead_code)] // Captured for potential future use
    pub default_value: Option<String>,
}

/// Extract parameters from a CREATE PROCEDURE definition using token-based parsing.
///
/// Phase 20.1.3: Replaced PROC_PARAM_RE regex with token-based parser.
/// Uses the same token-based infrastructure as function parameter parsing.
/// Parameter names are stored WITHOUT the @ prefix for consistency.
fn extract_procedure_parameters(definition: &str) -> Vec<ProcedureParameter> {
    // Use token-based parameter extraction
    let token_params = extract_procedure_parameters_tokens(definition);

    // Convert TokenParsedProcedureParameter to ProcedureParameter
    // Note: TokenParsedProcedureParameter.name does NOT include @ prefix
    token_params
        .into_iter()
        .map(|p| ProcedureParameter {
            name: p.name, // Already without @ prefix
            data_type: p.data_type,
            is_output: p.is_output,
            is_readonly: p.is_readonly,
            default_value: p.default_value,
        })
        .collect()
}

/// Represents an extracted function parameter with full details
#[derive(Debug)]
struct FunctionParameter {
    name: String,
    data_type: String,
    default_value: Option<String>,
}

/// Extract parameters from a CREATE FUNCTION definition using token-based parsing.
///
/// Phase 20.1.2: Replaced FUNC_PARAM_RE regex with token-based parser.
/// Uses the same token-based infrastructure as procedure parameter parsing.
fn extract_function_parameters(definition: &str) -> Vec<FunctionParameter> {
    // Use token-based parameter extraction
    let token_params = extract_function_parameters_tokens(definition);

    // Convert TokenParsedParameter to FunctionParameter
    token_params
        .into_iter()
        .map(|p| {
            // Strip @ prefix from parameter name (TokenParsedParameter includes it)
            let name = p.name.strip_prefix('@').unwrap_or(&p.name).to_string();
            FunctionParameter {
                name,
                data_type: p.data_type,
                default_value: p.default_value,
            }
        })
        .collect()
}

/// Write Parameters relationship for function parameters
fn write_function_parameters<W: Write>(
    writer: &mut Writer<W>,
    params: &[FunctionParameter],
    full_name: &str,
) -> anyhow::Result<()> {
    if params.is_empty() {
        return Ok(());
    }

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let param_rel = BytesStart::new("Relationship").with_attributes([("Name", "Parameters")]);
    writer.write_event(Event::Start(param_rel))?;

    for param in params.iter() {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Parameter name must include @ prefix
        let param_name_with_at = if param.name.starts_with('@') {
            param.name.clone()
        } else {
            format!("@{}", param.name)
        };
        let param_name = format!("{}.[{}]", full_name, param_name_with_at);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let param_elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlSubroutineParameter"),
            ("Name", param_name.as_str()),
        ]);
        writer.write_event(Event::Start(param_elem))?;

        // Write default value if present
        if let Some(ref default_val) = param.default_value {
            write_script_property(writer, "DefaultExpressionScript", default_val)?;
        }

        // Data type relationship
        write_data_type_relationship(writer, &param.data_type)?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

// =============================================================================
// TVP (Table-Valued Parameter) Handling
// =============================================================================

/// Find a table type element in the model matching the parameter data type
fn find_table_type_for_parameter<'a>(
    data_type: &str,
    model: &'a DatabaseModel,
) -> Option<&'a UserDefinedTypeElement> {
    // Normalize the data type for comparison
    // Handle both [dbo].[TypeName] and dbo.TypeName formats
    let normalized = normalize_type_name(data_type);

    for element in &model.elements {
        if let ModelElement::UserDefinedType(udt) = element {
            let type_full_name = format!("[{}].[{}]", udt.schema, udt.name);
            if type_full_name.eq_ignore_ascii_case(&normalized) {
                return Some(udt);
            }
        }
    }
    None
}

/// Write Columns relationship for a SqlDynamicColumnSource
fn write_dynamic_columns<W: Write>(
    writer: &mut Writer<W>,
    dynamic_source_name: &str,
    table_type: &UserDefinedTypeElement,
) -> anyhow::Result<()> {
    if table_type.columns.is_empty() {
        return Ok(());
    }

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in &table_type.columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", dynamic_source_name, col.name);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let col_elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlSimpleColumn"),
            ("Name", col_full_name.as_str()),
        ]);
        writer.write_event(Event::Start(col_elem))?;

        // Write IsNullable property - in DynamicObjects columns, IsNullable is based on
        // the table type column definition
        let is_nullable = col.nullability.unwrap_or(true);
        if !is_nullable {
            write_property(writer, "IsNullable", "False")?;
        }

        // Write TypeSpecifier relationship
        write_column_type_specifier(writer, &col.data_type, col.precision, col.scale)?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

// =============================================================================
// CTE DynamicObjects Writing (Phase 24.1.3)
// =============================================================================

use super::{
    extract_cte_definitions, extract_table_variable_definitions, extract_temp_table_definitions,
    CteColumn, TableVariableColumn, TempTableColumn,
};

/// Write Columns relationship for a CTE SqlDynamicColumnSource.
/// Each column is a SqlComputedColumn with ExpressionDependencies.
fn write_cte_columns<W: Write>(
    writer: &mut Writer<W>,
    cte_source_name: &str,
    columns: &[CteColumn],
) -> anyhow::Result<()> {
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", cte_source_name, col.name);
        let col_elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlComputedColumn"),
            ("Name", col_full_name.as_str()),
        ]);
        writer.write_event(Event::Start(col_elem))?;

        // Write ExpressionDependencies if any
        if !col.expression_dependencies.is_empty() {
            write_expression_dependencies(writer, &col.expression_dependencies)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write ExpressionDependencies relationship for a CTE column.
fn write_expression_dependencies<W: Write>(
    writer: &mut Writer<W>,
    dependencies: &[String],
) -> anyhow::Result<()> {
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "ExpressionDependencies")]);
    writer.write_event(Event::Start(rel))?;

    for dep in dependencies {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;
        let refs_elem = BytesStart::new("References").with_attributes([("Name", dep.as_str())]);
        writer.write_event(Event::Empty(refs_elem))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

// =============================================================================
// Temp Table DynamicObjects Writing (Phase 24.2.2)
// =============================================================================

/// Write Columns relationship for a temp table SqlDynamicColumnSource.
/// Each column is a SqlSimpleColumn with TypeSpecifier.
fn write_temp_table_columns<W: Write>(
    writer: &mut Writer<W>,
    temp_table_source_name: &str,
    columns: &[TempTableColumn],
) -> anyhow::Result<()> {
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", temp_table_source_name, col.name);
        let col_elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlSimpleColumn"),
            ("Name", col_full_name.as_str()),
        ]);
        writer.write_event(Event::Start(col_elem))?;

        // Write IsNullable property
        if !col.is_nullable {
            write_property(writer, "IsNullable", "False")?;
        }

        // Write TypeSpecifier relationship for the column's data type
        write_temp_table_column_type_specifier(writer, &col.data_type)?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write TypeSpecifier for a temp table column
fn write_temp_table_column_type_specifier<W: Write>(
    writer: &mut Writer<W>,
    data_type: &str,
) -> anyhow::Result<()> {
    // Parse the data type to extract precision/scale/length
    let (base_type, precision, scale) = parse_temp_table_data_type(data_type);

    let rel = BytesStart::new("Relationship").with_attributes([("Name", "TypeSpecifier")]);
    writer.write_event(Event::Start(rel))?;
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
    writer.write_event(Event::Start(elem))?;

    // Write type reference
    let type_ref_rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(type_ref_rel))?;
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let type_ref = format!("[{}]", base_type.to_lowercase());
    let refs_elem = BytesStart::new("References")
        .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref.as_str())]);
    writer.write_event(Event::Empty(refs_elem))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    // Write precision/scale/length if present
    if let Some(prec) = precision {
        if let Some(sc) = scale {
            // Decimal/numeric type with precision and scale
            write_property(writer, "Precision", &prec.to_string())?;
            write_property(writer, "Scale", &sc.to_string())?;
        } else if prec == -1 {
            // MAX type
            write_property(writer, "IsMax", "True")?;
        } else {
            // Length-based type (varchar, char, binary, etc.)
            write_property(writer, "Length", &prec.to_string())?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Parse a data type string to extract base type name and precision/scale/length
fn parse_temp_table_data_type(data_type: &str) -> (String, Option<i32>, Option<i32>) {
    // Handle types with parentheses like varchar(50), decimal(18,2), nvarchar(MAX)
    if let Some(paren_idx) = data_type.find('(') {
        let base = data_type[..paren_idx].trim().to_string();
        let params = &data_type[paren_idx + 1..].trim_end_matches(')');

        if params.to_uppercase() == "MAX" {
            return (base, Some(-1), None);
        }

        let parts: Vec<&str> = params.split(',').collect();
        if parts.len() == 2 {
            // Precision and scale (decimal, numeric)
            let prec = parts[0].trim().parse::<i32>().ok();
            let scale = parts[1].trim().parse::<i32>().ok();
            return (base, prec, scale);
        } else if parts.len() == 1 {
            // Length (varchar, char, binary, etc.)
            let len = parts[0].trim().parse::<i32>().ok();
            return (base, len, None);
        }
    }

    // No parameters, just return the base type
    (data_type.to_string(), None, None)
}

// =============================================================================
// Table Variable DynamicObjects Writing (Phase 24.3.2)
// =============================================================================

/// Write Columns relationship for a table variable SqlDynamicColumnSource.
/// Each column is a SqlSimpleColumn with TypeSpecifier.
/// This follows the same pattern as temp table columns.
fn write_table_variable_columns<W: Write>(
    writer: &mut Writer<W>,
    table_var_source_name: &str,
    columns: &[TableVariableColumn],
) -> anyhow::Result<()> {
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", table_var_source_name, col.name);
        let col_elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlSimpleColumn"),
            ("Name", col_full_name.as_str()),
        ]);
        writer.write_event(Event::Start(col_elem))?;

        // Write IsNullable property
        if !col.is_nullable {
            write_property(writer, "IsNullable", "False")?;
        }

        // Write TypeSpecifier relationship for the column's data type
        // Reuse the same type specifier writer as temp tables since they have the same format
        write_temp_table_column_type_specifier(writer, &col.data_type)?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Extract CTE definitions, temp tables, and table variables, write combined DynamicObjects
pub(crate) fn write_all_dynamic_objects<W: Write>(
    writer: &mut Writer<W>,
    full_name: &str,
    body: &str,
    default_schema: &str,
    tvp_params: &[(&ProcedureParameter, Option<&UserDefinedTypeElement>)],
) -> anyhow::Result<()> {
    // Extract CTEs from body
    let cte_defs = extract_cte_definitions(body, default_schema);

    // Extract temp tables from body (Phase 24.2)
    let temp_table_defs = extract_temp_table_definitions(body);

    // Extract table variables from body (Phase 24.3)
    let table_var_defs = extract_table_variable_definitions(body);

    // If no TVPs, CTEs, temp tables, or table variables, nothing to write
    if tvp_params.is_empty()
        && cte_defs.is_empty()
        && temp_table_defs.is_empty()
        && table_var_defs.is_empty()
    {
        return Ok(());
    }

    // Open DynamicObjects relationship
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "DynamicObjects")]);
    writer.write_event(Event::Start(rel))?;

    // Write TVP entries first
    for (param, table_type_opt) in tvp_params {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let param_name_with_at = if param.name.starts_with('@') {
            param.name.clone()
        } else {
            format!("@{}", param.name)
        };
        let dynamic_source_name = format!("{}.[{}]", full_name, param_name_with_at);

        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlDynamicColumnSource"),
            ("Name", dynamic_source_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        // Write Columns relationship if we have the table type definition
        if let Some(table_type) = table_type_opt {
            write_dynamic_columns(writer, &dynamic_source_name, table_type)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    // Write CTE entries
    for cte in &cte_defs {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let cte_source_name = format!("{}.[CTE{}].[{}]", full_name, cte.cte_number, cte.name);

        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlDynamicColumnSource"),
            ("Name", cte_source_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        if !cte.columns.is_empty() {
            write_cte_columns(writer, &cte_source_name, &cte.columns)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    // Write temp table entries (Phase 24.2)
    for temp_table in &temp_table_defs {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Format: [schema].[proc].[TempTable1].[#TempName]
        let temp_table_source_name = format!(
            "{}.[TempTable{}].[{}]",
            full_name, temp_table.temp_table_number, temp_table.name
        );

        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlDynamicColumnSource"),
            ("Name", temp_table_source_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        if !temp_table.columns.is_empty() {
            write_temp_table_columns(writer, &temp_table_source_name, &temp_table.columns)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    // Write table variable entries (Phase 24.3)
    for table_var in &table_var_defs {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Format: [schema].[proc].[TableVariable1].[@VarName]
        let table_var_source_name = format!(
            "{}.[TableVariable{}].[{}]",
            full_name, table_var.table_variable_number, table_var.name
        );

        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlDynamicColumnSource"),
            ("Name", table_var_source_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        if !table_var.columns.is_empty() {
            write_table_variable_columns(writer, &table_var_source_name, &table_var.columns)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Extract body dependencies including TVP column references
fn extract_body_dependencies_with_tvp(
    body: &str,
    full_name: &str,
    _params: &[String],
    tvp_params: &[(&ProcedureParameter, Option<&UserDefinedTypeElement>)],
) -> Vec<BodyDependency> {
    use std::collections::HashSet;
    let mut deps = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Build a map of TVP param names (with @ prefix for body matching) to their table type columns
    // Note: param.name is stored WITHOUT @ prefix (Phase 20.1.3)
    let tvp_columns: std::collections::HashMap<String, Vec<String>> = tvp_params
        .iter()
        .filter_map(|(param, tt_opt)| {
            tt_opt.map(|tt| {
                // Add @ prefix for body pattern matching
                let param_name = format!("@{}", param.name);
                let cols = tt.columns.iter().map(|c| c.name.clone()).collect();
                (param_name, cols)
            })
        })
        .collect();

    // First, add the TVP parameter reference with disambiguator
    // This reference appears first in BodyDependencies with the same disambiguator as in Parameters
    // Note: param.name is stored WITHOUT @ prefix (Phase 20.1.3)
    for (idx, (param, _)) in tvp_params.iter().enumerate() {
        let disambiguator = 2 + idx as u32;
        // Use param.name directly - it's already without @ prefix
        let param_ref = format!("{}.[@{}]", full_name, param.name);
        // Store with disambiguator info - we'll need to emit this specially
        if !seen.contains(&param_ref) {
            seen.insert(param_ref.clone());
            deps.push(BodyDependency::TvpParameter(param_ref, disambiguator));
        }
    }

    // Now scan the body for column references from TVP parameters
    // Pattern: FROM @ParamName or @ParamName.Column or just column names used with TVP
    for (tvp_param_name, columns) in &tvp_columns {
        // Look for column references in SELECT, WHERE, etc. that match TVP columns
        // Pattern: column names that appear after FROM @ParamName
        let param_pattern = format!(r"(?i)FROM\s+{}\b", regex::escape(tvp_param_name));
        if regex::Regex::new(&param_pattern).unwrap().is_match(body) {
            // This TVP is used as a table source - add column references
            for col_name in columns {
                // Check if this column is referenced in the body
                let col_pattern = format!(r"\b{}\b", regex::escape(col_name));
                if regex::Regex::new(&col_pattern).unwrap().is_match(body) {
                    let col_ref = format!(
                        "{}.[@{}].[{}]",
                        full_name,
                        tvp_param_name.trim_start_matches('@'),
                        col_name
                    );
                    if !seen.contains(&col_ref) {
                        seen.insert(col_ref.clone());
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    }
                }
            }
        }
    }

    deps
}

// =============================================================================
// Function Body and Header Extraction
// =============================================================================

/// Extract the body part from a CREATE FUNCTION definition
/// Returns just the body (BEGIN...END or RETURN(...)) without the header
///
/// Uses token-based parsing to find the AS keyword that separates header from body,
/// handling whitespace (tabs, spaces, newlines) correctly.
fn extract_function_body(definition: &str) -> String {
    let def_upper = definition.to_uppercase();

    // Find RETURNS and then AS after it
    // Pattern: CREATE FUNCTION [name](...) RETURNS type AS BEGIN ... END
    if let Some(returns_pos) = def_upper.find("RETURNS") {
        // Use token-based parsing to find the AS keyword
        if let Some((_as_start, as_end)) = find_function_body_as_tokenized(definition, returns_pos)
        {
            // Return everything after AS (as_end is after AS and any trailing whitespace)
            return definition[as_end..].trim().to_string();
        }
    }

    // Fallback: return the original definition
    definition.to_string()
}

/// Extract the header part from a CREATE FUNCTION definition
/// Returns everything up to and including AS (CREATE FUNCTION [name](...) RETURNS type AS\n)
/// Preserves trailing whitespace after AS to ensure proper separation from body
///
/// Uses token-based parsing to find the AS keyword that separates header from body,
/// handling whitespace (tabs, spaces, newlines) correctly.
fn extract_function_header(definition: &str) -> String {
    let def_upper = definition.to_uppercase();

    // Find RETURNS and then AS after it
    if let Some(returns_pos) = def_upper.find("RETURNS") {
        // Use token-based parsing to find the AS keyword
        if let Some((_as_start, as_end)) = find_function_body_as_tokenized(definition, returns_pos)
        {
            // Return everything up to and including AS with trailing whitespace
            // Use trim_start() to only remove leading whitespace, preserving trailing newline
            return definition[..as_end].trim_start().to_string();
        }
    }

    // Fallback: return empty string
    String::new()
}

/// Find the AS keyword that separates function header from body using token-based parsing.
///
/// This function tokenizes the input string and looks for the AS keyword that appears
/// after RETURNS and is followed by BEGIN, RETURN, or other body-starting statements.
/// It returns the byte position where the AS token ends (after any trailing whitespace).
///
/// # Arguments
/// * `definition` - The full CREATE FUNCTION definition
/// * `after_returns_start` - The byte position where RETURNS was found
///
/// # Returns
/// * `Some((as_start, as_end))` - The start and end byte positions of the AS keyword
///   where `as_end` includes any trailing whitespace after AS
/// * `None` - If no valid body separator AS was found
fn find_function_body_as_tokenized(
    definition: &str,
    after_returns_start: usize,
) -> Option<(usize, usize)> {
    let after_returns = &definition[after_returns_start..];

    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, after_returns).tokenize_with_location() else {
        return None;
    };

    if tokens.is_empty() {
        return None;
    }

    // Build line offset map for byte position calculation
    let line_offsets = compute_line_offsets(after_returns);

    let len = tokens.len();
    let mut i = 0;

    // Helper to skip whitespace tokens, returns the next non-whitespace index
    let skip_whitespace =
        |tokens: &[sqlparser::tokenizer::TokenWithSpan], mut idx: usize| -> usize {
            while idx < tokens.len() && matches!(&tokens[idx].token, Token::Whitespace(_)) {
                idx += 1;
            }
            idx
        };

    // Keywords that can start a function body after AS
    let body_start_keywords = [
        "BEGIN", "RETURN", "SET", "SELECT", "IF", "WHILE", "DECLARE", "WITH", "INSERT", "UPDATE",
        "DELETE", "EXEC", "EXECUTE",
    ];

    while i < len {
        // Look for AS keyword (unquoted word)
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("AS") {
                // Calculate byte position of AS keyword
                let as_byte_start = location_to_byte_offset(
                    &line_offsets,
                    tokens[i].span.start.line,
                    tokens[i].span.start.column,
                );

                // Look at what comes after AS
                let j = skip_whitespace(&tokens, i + 1);

                // Calculate end position (after AS and any whitespace)
                let as_byte_end = if j < len {
                    location_to_byte_offset(
                        &line_offsets,
                        tokens[j].span.start.line,
                        tokens[j].span.start.column,
                    )
                } else {
                    // AS is at the end, end is after "AS" (2 chars)
                    as_byte_start + 2
                };

                // Check if followed by a body-starting keyword
                if j < len {
                    if let Token::Word(next_word) = &tokens[j].token {
                        if next_word.quote_style.is_none() {
                            let next_upper = next_word.value.to_uppercase();
                            if body_start_keywords.contains(&next_upper.as_str()) {
                                // This AS is the body separator
                                return Some((
                                    after_returns_start + as_byte_start,
                                    after_returns_start + as_byte_end,
                                ));
                            }
                        }
                    }
                } else {
                    // AS is at the very end - still a valid body separator
                    return Some((
                        after_returns_start + as_byte_start,
                        after_returns_start + as_byte_end,
                    ));
                }
            }
        }
        i += 1;
    }

    None
}

/// Extract just the body after AS from a procedure definition using token-based parsing.
fn extract_procedure_body_only(definition: &str) -> String {
    // Use tokenized parsing to find the AS keyword that separates header from body
    if let Some((_as_start, as_end)) = find_procedure_body_separator_as_tokenized(definition) {
        // as_end points to the first token after AS (after any whitespace)
        // Return the body starting from that position
        return definition[as_end..].to_string();
    }
    definition.to_string()
}

/// Find the AS keyword that separates procedure header from body using token-based parsing.
///
/// This function tokenizes the input string and looks for the AS keyword that is followed
/// by BEGIN, SET, SELECT, or other body-starting statements. Unlike `find_function_body_as_tokenized`,
/// this scans from the beginning of the definition (procedures don't have a RETURNS keyword).
///
/// # Arguments
/// * `definition` - The full CREATE PROCEDURE definition
///
/// # Returns
/// * `Some((as_start, as_end))` - The start and end byte positions of the AS keyword
///   where `as_end` includes any trailing whitespace after AS
/// * `None` - If no valid body separator AS was found
fn find_procedure_body_separator_as_tokenized(definition: &str) -> Option<(usize, usize)> {
    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, definition).tokenize_with_location() else {
        return None;
    };

    if tokens.is_empty() {
        return None;
    }

    // Build line offset map for byte position calculation
    let line_offsets = compute_line_offsets(definition);

    let len = tokens.len();
    let mut i = 0;

    // Helper to skip whitespace tokens, returns the next non-whitespace index
    let skip_whitespace =
        |tokens: &[sqlparser::tokenizer::TokenWithSpan], mut idx: usize| -> usize {
            while idx < tokens.len() && matches!(&tokens[idx].token, Token::Whitespace(_)) {
                idx += 1;
            }
            idx
        };

    // Keywords that can start a procedure body after AS
    let body_start_keywords = [
        "BEGIN", "RETURN", "SET", "SELECT", "IF", "WHILE", "DECLARE", "WITH", "INSERT", "UPDATE",
        "DELETE", "EXEC", "EXECUTE",
    ];

    while i < len {
        // Look for AS keyword (unquoted word)
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("AS") {
                // Calculate byte position of AS keyword
                let as_byte_start = location_to_byte_offset(
                    &line_offsets,
                    tokens[i].span.start.line,
                    tokens[i].span.start.column,
                );

                // Look at what comes after AS
                let j = skip_whitespace(&tokens, i + 1);

                // Calculate end position (after AS and any whitespace)
                let as_byte_end = if j < len {
                    location_to_byte_offset(
                        &line_offsets,
                        tokens[j].span.start.line,
                        tokens[j].span.start.column,
                    )
                } else {
                    // AS is at the end, end is after "AS" (2 chars)
                    as_byte_start + 2
                };

                // Check if followed by a body-starting keyword
                if j < len {
                    if let Token::Word(next_word) = &tokens[j].token {
                        if next_word.quote_style.is_none() {
                            let next_upper = next_word.value.to_uppercase();
                            if body_start_keywords.contains(&next_upper.as_str()) {
                                // This AS is the body separator
                                return Some((as_byte_start, as_byte_end));
                            }
                        }
                    }
                } else {
                    // AS is at the very end - still a valid body separator
                    return Some((as_byte_start, as_byte_end));
                }
            }
        }
        i += 1;
    }

    None
}

/// Write FunctionBody relationship for functions with nested SqlScriptFunctionImplementation
/// Includes SysCommentsObjectAnnotation with HeaderContents for DacFx compatibility
fn write_function_body_with_annotation<W: Write>(
    writer: &mut Writer<W>,
    body: &str,
    header: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "FunctionBody")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem =
        BytesStart::new("Element").with_attributes([("Type", "SqlScriptFunctionImplementation")]);
    writer.write_event(Event::Start(elem))?;

    // Write BodyScript property with the function body only (BEGIN...END)
    write_script_property(writer, "BodyScript", body)?;

    // Write SysCommentsObjectAnnotation with HeaderContents
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let annotation =
        BytesStart::new("Annotation").with_attributes([("Type", "SysCommentsObjectAnnotation")]);
    writer.write_event(Event::Start(annotation))?;

    // Calculate length (header + body)
    let total_length = header.len() + body.len();
    write_property(writer, "Length", &total_length.to_string())?;
    write_property(writer, "StartLine", "1")?;
    write_property(writer, "StartColumn", "1")?;

    // Write HeaderContents with XML-escaped header (newlines encoded as &#xA;)
    // Use write_property_raw to avoid double-escaping the & in &#xA;
    let escaped_header = escape_newlines_for_attr(header);
    write_property_raw(writer, "HeaderContents", &escaped_header)?;

    writer.write_event(Event::End(BytesEnd::new("Annotation")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Write Type relationship for scalar function return type
/// Format: <Relationship Name="Type"><Entry><Element Type="SqlTypeSpecifier">
///           <Relationship Name="Type"><Entry><References ExternalSource="BuiltIns" Name="[type]"/></Entry></Relationship>
///         </Element></Entry></Relationship>
fn write_function_return_type<W: Write>(
    writer: &mut Writer<W>,
    return_type: &str,
) -> anyhow::Result<()> {
    // Extract base type name (e.g., "INT" -> "int", "DECIMAL(18,2)" -> "decimal")
    let base_type = extract_base_type_name(return_type);
    let type_ref = format!("[{}]", base_type.to_lowercase());

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
    writer.write_event(Event::Start(elem))?;

    // Nested Type relationship referencing the built-in type
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let inner_rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(inner_rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let refs = BytesStart::new("References")
        .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref.as_str())]);
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Extract base type name from a type specification
/// e.g., "DECIMAL(18,2)" -> "decimal", "VARCHAR(100)" -> "varchar", "INT" -> "int"
fn extract_base_type_name(type_spec: &str) -> String {
    let type_upper = type_spec.trim().to_uppercase();
    // Remove parentheses and everything after
    if let Some(paren_pos) = type_upper.find('(') {
        type_upper[..paren_pos].trim().to_lowercase()
    } else {
        type_upper.to_lowercase()
    }
}

// =============================================================================
// TVF Column Extraction and Writing
// =============================================================================

/// Column definition for multi-statement TVF (RETURNS @Table TABLE (...))
#[derive(Debug)]
struct TvfColumn {
    name: String,
    data_type: String,
    length: Option<u32>,
    precision: Option<u8>,
    scale: Option<u8>,
}

/// Extract columns from an inline table-valued function's RETURN statement
/// The body contains "RETURN (SELECT [cols] FROM ...)" or "RETURN SELECT [cols] FROM ..."
/// func_full_name is needed to construct parameter references like [dbo].[FuncName].[@Param]
fn extract_inline_tvf_columns(
    body: &str,
    func_full_name: &str,
    default_schema: &str,
    model: &DatabaseModel,
) -> Vec<ViewColumn> {
    // Extract the SELECT statement from RETURN clause
    // Pattern: RETURN followed by optional whitespace, optional parenthesis, then SELECT
    let body_upper = body.to_uppercase();

    // Find RETURN keyword
    if let Some(return_pos) = body_upper.find("RETURN") {
        let after_return = &body[return_pos + 6..]; // Skip "RETURN"

        // Skip whitespace and optional opening parenthesis
        let trimmed = after_return.trim_start();
        let query_start = trimmed.strip_prefix('(').unwrap_or(trimmed);

        // Now we should have the SELECT statement
        // Use the existing extract_view_columns_and_deps logic
        // TVFs don't have SCHEMABINDING affecting GROUP BY, use false
        let (mut columns, _deps) =
            extract_view_columns_and_deps(query_start, default_schema, model, false);

        // For inline TVFs, handle parameter references in the SELECT list
        // When column expression is a parameter reference like @CustomerId,
        // the source_ref should be [schema].[FuncName].[@ParamName]
        let select_columns = extract_select_columns(query_start);
        for (idx, col_expr) in select_columns.iter().enumerate() {
            if idx < columns.len() {
                let trimmed_expr = col_expr.trim();
                // Check if the expression (before AS) is a parameter reference
                // Use token-based parsing to handle any whitespace around AS (tabs, multiple spaces, etc.)
                let expr_part = extract_expression_before_as(trimmed_expr);

                // If it's a parameter reference like @ParamName
                if expr_part.starts_with('@') && !expr_part.contains('(') {
                    let param_name = expr_part.trim_matches(|c| c == '[' || c == ']');
                    // DotNet format: [schema].[FuncName].[@ParamName] (brackets around the @param)
                    columns[idx].source_ref = Some(format!("{}.[{}]", func_full_name, param_name));
                }
            }
        }

        return columns;
    }

    Vec::new()
}

/// Extract columns from a multi-statement TVF's RETURNS @TableVar TABLE (...) clause
fn extract_multi_statement_tvf_columns(definition: &str) -> Vec<TvfColumn> {
    // Find the RETURNS @var TABLE (...) clause
    // We need to handle nested parentheses in column types like NVARCHAR(100)
    // First find "RETURNS @name TABLE ("
    let def_upper = definition.to_uppercase();

    if let Some(table_match) = MULTI_STMT_TVF_RE.find(&def_upper) {
        let start_pos = table_match.end();
        // Now find the matching closing paren, accounting for nested parens
        if let Some(cols_str) = extract_balanced_parens(definition, start_pos) {
            // Split by comma, respecting parentheses for types like NVARCHAR(100)
            let col_defs = split_column_definitions(&cols_str);

            // Pre-allocate based on column count
            let mut columns = Vec::with_capacity(col_defs.len());
            for col_def in col_defs {
                if let Some(col) = parse_tvf_column_definition(&col_def) {
                    columns.push(col);
                }
            }
            return columns;
        }
    }

    Vec::new()
}

/// Extract content inside balanced parentheses, handling nested parens
fn extract_balanced_parens(input: &str, start: usize) -> Option<String> {
    let bytes = input.as_bytes();
    let mut depth = 1; // We start after the opening paren
    let mut end = start;

    while end < bytes.len() && depth > 0 {
        match bytes[end] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth > 0 {
            end += 1;
        }
    }

    if depth == 0 {
        Some(input[start..end].to_string())
    } else {
        None
    }
}

/// Split column definitions by comma, respecting parentheses
fn split_column_definitions(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;

    for c in input.chars() {
        match c {
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth -= 1;
                current.push(c);
            }
            ',' if paren_depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }

    result
}

/// Parse a single column definition like "Id INT" or "Name NVARCHAR(100)"
///
/// Uses token-based parsing via `parse_tvf_column_type_tokenized()` to handle
/// type specifications with optional length/precision/scale parameters.
fn parse_tvf_column_definition(def: &str) -> Option<TvfColumn> {
    let def = def.trim();
    if def.is_empty() {
        return None;
    }

    // Pattern: [col_name] type[(length/precision,scale)] [optional modifiers]
    // Examples: Id INT, Name NVARCHAR(100), Price DECIMAL(18,2)
    let parts: Vec<&str> = def.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let name = parts[0].trim_matches(|c| c == '[' || c == ']').to_string();
    if parts.len() < 2 {
        return None;
    }

    // Join remaining parts and parse type with optional length/precision/scale
    let type_part = parts[1..].join(" ");

    // Use token-based parsing instead of regex (Phase 20.3.2)
    if let Some(type_info) = parse_tvf_column_type_tokenized(&type_part) {
        // Determine if first_num is length or precision based on type
        let (length, precision, scale) = if is_precision_scale_type(&type_info.data_type) {
            (
                None,
                type_info.first_num.map(|n| n as u8),
                type_info.second_num,
            )
        } else {
            (type_info.first_num, None, None)
        };

        Some(TvfColumn {
            name,
            data_type: type_info.data_type,
            length,
            precision,
            scale,
        })
    } else {
        None
    }
}

/// Check if a data type uses precision/scale (like DECIMAL, NUMERIC) vs length (like VARCHAR)
fn is_precision_scale_type(data_type: &str) -> bool {
    matches!(
        data_type.to_lowercase().as_str(),
        "decimal" | "numeric" | "float" | "real" | "money" | "smallmoney"
    )
}

/// Parsed TVF column type result from tokenized parsing.
///
/// Contains the extracted data type name and optional length/precision/scale values.
#[derive(Debug, PartialEq)]
struct TvfColumnTypeInfo {
    data_type: String,
    first_num: Option<u32>,
    second_num: Option<u8>,
}

/// Parse TVF column type definition using tokenization.
///
/// This function replaces TVF_COL_TYPE_RE regex pattern. It parses type strings like:
/// - `INT` -> TvfColumnTypeInfo { data_type: "int", first_num: None, second_num: None }
/// - `NVARCHAR(100)` -> TvfColumnTypeInfo { data_type: "nvarchar", first_num: Some(100), second_num: None }
/// - `DECIMAL(18, 2)` -> TvfColumnTypeInfo { data_type: "decimal", first_num: Some(18), second_num: Some(2) }
///
/// # Arguments
/// * `type_str` - Type specification string (e.g., "NVARCHAR(100)" or "[DECIMAL](18, 2)")
///
/// # Returns
/// `Some(TvfColumnTypeInfo)` with parsed type information, or `None` if parsing fails.
fn parse_tvf_column_type_tokenized(type_str: &str) -> Option<TvfColumnTypeInfo> {
    let type_str = type_str.trim();
    if type_str.is_empty() {
        return None;
    }

    let dialect = MsSqlDialect {};
    let tokens = Tokenizer::new(&dialect, type_str).tokenize().ok()?;
    if tokens.is_empty() {
        return None;
    }

    let mut i = 0;
    // Skip whitespace
    while i < tokens.len() && matches!(&tokens[i], Token::Whitespace(_)) {
        i += 1;
    }
    if i >= tokens.len() {
        return None;
    }

    // First token should be the type name (Word)
    let data_type = match &tokens[i] {
        Token::Word(w) => w.value.to_lowercase(),
        _ => return None,
    };
    i += 1;

    // Skip whitespace after type name
    while i < tokens.len() && matches!(&tokens[i], Token::Whitespace(_)) {
        i += 1;
    }

    // Check for optional parentheses with length/precision/scale
    if i >= tokens.len() || !matches!(&tokens[i], Token::LParen) {
        return Some(TvfColumnTypeInfo {
            data_type,
            first_num: None,
            second_num: None,
        });
    }

    // Skip the opening paren
    i += 1;

    // Skip whitespace
    while i < tokens.len() && matches!(&tokens[i], Token::Whitespace(_)) {
        i += 1;
    }

    // Check for MAX keyword (VARCHAR(MAX), NVARCHAR(MAX))
    if i < tokens.len() {
        if let Token::Word(w) = &tokens[i] {
            if w.value.eq_ignore_ascii_case("MAX") {
                return Some(TvfColumnTypeInfo {
                    data_type,
                    first_num: Some(u32::MAX), // Use MAX_VALUE to indicate MAX
                    second_num: None,
                });
            }
        }
    }

    // Look for first number (length or precision)
    let first_num = if i < tokens.len() {
        if let Token::Number(n, _) = &tokens[i] {
            i += 1;
            n.parse::<u32>().ok()
        } else {
            None
        }
    } else {
        None
    };

    // Skip whitespace
    while i < tokens.len() && matches!(&tokens[i], Token::Whitespace(_)) {
        i += 1;
    }

    // Check for comma (indicates second parameter - scale)
    let second_num = if i < tokens.len() && matches!(&tokens[i], Token::Comma) {
        i += 1;
        // Skip whitespace
        while i < tokens.len() && matches!(&tokens[i], Token::Whitespace(_)) {
            i += 1;
        }
        // Get second number (scale)
        if i < tokens.len() {
            if let Token::Number(n, _) = &tokens[i] {
                n.parse::<u8>().ok()
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Some(TvfColumnTypeInfo {
        data_type,
        first_num,
        second_num,
    })
}

fn write_tvf_columns<W: Write>(
    writer: &mut Writer<W>,
    func_full_name: &str,
    columns: &[TvfColumn],
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", func_full_name, col.name);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlSimpleColumn"),
            ("Name", col_full_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        // Write TypeSpecifier relationship
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let type_rel = BytesStart::new("Relationship").with_attributes([("Name", "TypeSpecifier")]);
        writer.write_event(Event::Start(type_rel))?;

        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let spec_elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
        writer.write_event(Event::Start(spec_elem))?;

        // Write Length or Precision/Scale properties if present
        // For MAX types (u32::MAX), write IsMax=True instead of Length=4294967295
        if let Some(length) = col.length {
            if length == u32::MAX {
                write_property(writer, "IsMax", "True")?;
            } else {
                write_property(writer, "Length", &length.to_string())?;
            }
        }
        if let Some(precision) = col.precision {
            write_property(writer, "Precision", &precision.to_string())?;
        }
        if let Some(scale) = col.scale {
            write_property(writer, "Scale", &scale.to_string())?;
        }

        // Write Type reference to built-in type
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let inner_type_rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
        writer.write_event(Event::Start(inner_type_rel))?;

        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let type_ref = format!("[{}]", col.data_type);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let refs = BytesStart::new("References")
            .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref.as_str())]);
        writer.write_event(Event::Empty(refs))?;

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        writer.write_event(Event::End(BytesEnd::new("Relationship")))?; // Type

        writer.write_event(Event::End(BytesEnd::new("Element")))?; // SqlTypeSpecifier
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        writer.write_event(Event::End(BytesEnd::new("Relationship")))?; // TypeSpecifier

        writer.write_event(Event::End(BytesEnd::new("Element")))?; // SqlSimpleColumn
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

// =============================================================================
// Data Type Relationship Writing
// =============================================================================

fn write_data_type_relationship<W: Write>(
    writer: &mut Writer<W>,
    data_type: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Parse the data type and write an inline SqlTypeSpecifier element
    let (base_type, length, precision, scale) = parse_data_type(data_type);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
    writer.write_event(Event::Start(elem))?;

    // Write length/precision/scale if applicable
    if let Some(len) = length {
        if len == -1 {
            write_property(writer, "IsMax", "True")?;
        } else {
            write_property(writer, "Length", &len.to_string())?;
        }
    }
    if let Some(prec) = precision {
        write_property(writer, "Precision", &prec.to_string())?;
    }
    if let Some(sc) = scale {
        write_property(writer, "Scale", &sc.to_string())?;
    }

    // Write the base type as a reference
    let type_ref = format!("[{}]", base_type.to_lowercase());
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let type_rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(type_rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let refs = BytesStart::new("References")
        .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref.as_str())]);
    writer.write_event(Event::Empty(refs))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =============================================================================
    // TVF Column Type Parsing Tests
    // =============================================================================

    #[test]
    fn test_tvf_type_simple_int() {
        let result = parse_tvf_column_type_tokenized("INT");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "int".to_string(),
                first_num: None,
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_simple_nvarchar() {
        let result = parse_tvf_column_type_tokenized("NVARCHAR");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "nvarchar".to_string(),
                first_num: None,
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_nvarchar_with_length() {
        let result = parse_tvf_column_type_tokenized("NVARCHAR(100)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "nvarchar".to_string(),
                first_num: Some(100),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_varchar_with_length() {
        let result = parse_tvf_column_type_tokenized("VARCHAR(50)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "varchar".to_string(),
                first_num: Some(50),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_decimal_with_precision_scale() {
        let result = parse_tvf_column_type_tokenized("DECIMAL(18, 2)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "decimal".to_string(),
                first_num: Some(18),
                second_num: Some(2),
            })
        );
    }

    #[test]
    fn test_tvf_type_decimal_no_spaces() {
        let result = parse_tvf_column_type_tokenized("DECIMAL(10,4)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "decimal".to_string(),
                first_num: Some(10),
                second_num: Some(4),
            })
        );
    }

    #[test]
    fn test_tvf_type_numeric_with_precision() {
        let result = parse_tvf_column_type_tokenized("NUMERIC(15)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "numeric".to_string(),
                first_num: Some(15),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_varchar_max() {
        let result = parse_tvf_column_type_tokenized("VARCHAR(MAX)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "varchar".to_string(),
                first_num: Some(u32::MAX),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_nvarchar_max() {
        let result = parse_tvf_column_type_tokenized("NVARCHAR(MAX)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "nvarchar".to_string(),
                first_num: Some(u32::MAX),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_case_insensitive() {
        let result = parse_tvf_column_type_tokenized("int");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "int".to_string(),
                first_num: None,
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_mixed_case() {
        let result = parse_tvf_column_type_tokenized("NVarChar(50)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "nvarchar".to_string(),
                first_num: Some(50),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_with_tabs() {
        let result = parse_tvf_column_type_tokenized("DECIMAL\t(\t18\t,\t2\t)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "decimal".to_string(),
                first_num: Some(18),
                second_num: Some(2),
            })
        );
    }

    #[test]
    fn test_tvf_type_with_multiple_spaces() {
        let result = parse_tvf_column_type_tokenized("VARCHAR  (   100   )");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "varchar".to_string(),
                first_num: Some(100),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_empty() {
        let result = parse_tvf_column_type_tokenized("");
        assert_eq!(result, None);
    }

    #[test]
    fn test_tvf_type_whitespace_only() {
        let result = parse_tvf_column_type_tokenized("   ");
        assert_eq!(result, None);
    }

    #[test]
    fn test_tvf_type_datetime() {
        let result = parse_tvf_column_type_tokenized("DATETIME");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "datetime".to_string(),
                first_num: None,
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_bit() {
        let result = parse_tvf_column_type_tokenized("BIT");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "bit".to_string(),
                first_num: None,
                second_num: None,
            })
        );
    }

    // =============================================================================
    // Function Body AS Finding Tests
    // =============================================================================

    #[test]
    fn test_find_function_body_as_basic_begin() {
        let def = "CREATE FUNCTION fn() RETURNS INT AS BEGIN RETURN 1 END";
        let returns_pos = def.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(def, returns_pos);
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert!(def[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_function_body_as_with_newline() {
        let def = "CREATE FUNCTION fn() RETURNS INT AS\nBEGIN RETURN 1 END";
        let returns_pos = def.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(def, returns_pos);
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert!(def[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_function_body_as_with_tabs() {
        let def = "CREATE FUNCTION fn() RETURNS INT AS\t\tBEGIN RETURN 1 END";
        let returns_pos = def.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(def, returns_pos);
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert!(def[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_function_body_as_with_return() {
        let def = "CREATE FUNCTION fn() RETURNS INT AS RETURN 1";
        let returns_pos = def.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(def, returns_pos);
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert!(def[end..].trim().starts_with("RETURN"));
    }

    #[test]
    fn test_find_function_body_as_with_select() {
        let def = "CREATE FUNCTION fn() RETURNS TABLE AS SELECT * FROM t";
        let returns_pos = def.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(def, returns_pos);
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert!(def[end..].trim().starts_with("SELECT"));
    }

    #[test]
    fn test_find_function_body_as_lowercase() {
        let def = "CREATE FUNCTION fn() returns int as begin return 1 end";
        let returns_pos = def.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(def, returns_pos);
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert!(def[end..].trim().to_lowercase().starts_with("begin"));
    }

    #[test]
    fn test_find_function_body_as_mixed_case() {
        let def = "CREATE FUNCTION fn() Returns Int As Begin Return 1 End";
        let returns_pos = def.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(def, returns_pos);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_function_body_as_no_match() {
        let def = "CREATE FUNCTION fn() RETURNS INT";
        let returns_pos = def.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(def, returns_pos);
        // No AS keyword at all
        assert!(result.is_none());
    }

    // =============================================================================
    // Procedure Body AS Finding Tests
    // =============================================================================

    #[test]
    fn test_find_procedure_body_as_basic_begin() {
        let def = "CREATE PROCEDURE sp AS BEGIN SELECT 1 END";
        let result = find_procedure_body_separator_as_tokenized(def);
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert!(def[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_with_newline() {
        let def = "CREATE PROCEDURE sp AS\nBEGIN SELECT 1 END";
        let result = find_procedure_body_separator_as_tokenized(def);
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert!(def[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_with_select() {
        let def = "CREATE PROCEDURE sp AS SELECT 1";
        let result = find_procedure_body_separator_as_tokenized(def);
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert!(def[end..].trim().starts_with("SELECT"));
    }

    // =============================================================================
    // Extract Procedure Header Tests
    // =============================================================================

    #[test]
    fn test_extract_procedure_header_basic() {
        let def = "CREATE PROCEDURE sp AS BEGIN SELECT 1 END";
        let header = extract_procedure_header(def);
        assert!(header.starts_with("CREATE"));
        assert!(header.ends_with("AS"));
        assert!(!header.contains("BEGIN"));
    }

    #[test]
    fn test_extract_procedure_header_with_comment() {
        let def = "-- Comment\nCREATE PROCEDURE sp AS BEGIN SELECT 1 END";
        let header = extract_procedure_header(def);
        assert!(header.starts_with("-- Comment"));
        assert!(header.contains("CREATE"));
        assert!(header.ends_with("AS"));
        assert!(!header.contains("BEGIN"));
    }

    #[test]
    fn test_extract_procedure_header_multiline() {
        let def = "CREATE PROCEDURE sp\nAS\nBEGIN SELECT 1 END";
        let header = extract_procedure_header(def);
        assert!(header.starts_with("CREATE"));
        assert!(header.ends_with("AS"));
        assert!(!header.contains("BEGIN"));
    }

    // =============================================================================
    // Extract Function Body/Header Tests
    // =============================================================================

    #[test]
    fn test_extract_function_body_basic() {
        let def = "CREATE FUNCTION fn() RETURNS INT AS BEGIN RETURN 1 END";
        let body = extract_function_body(def);
        assert!(body.starts_with("BEGIN"));
        assert!(body.ends_with("END"));
    }

    #[test]
    fn test_extract_function_header_basic() {
        let def = "CREATE FUNCTION fn() RETURNS INT AS BEGIN RETURN 1 END";
        let header = extract_function_header(def);
        assert!(header.starts_with("CREATE"));
        assert!(header.contains("RETURNS INT AS"));
        assert!(!header.contains("BEGIN"));
    }

    // =============================================================================
    // Data Type Parsing Tests
    // =============================================================================

    #[test]
    fn test_parse_data_type_simple() {
        let (base, len, prec, scale) = parse_data_type("INT");
        assert_eq!(base, "INT");
        assert_eq!(len, None);
        assert_eq!(prec, None);
        assert_eq!(scale, None);
    }

    #[test]
    fn test_parse_data_type_varchar_length() {
        let (base, len, prec, scale) = parse_data_type("VARCHAR(50)");
        assert_eq!(base, "VARCHAR");
        assert_eq!(len, Some(50));
        assert_eq!(prec, None);
        assert_eq!(scale, None);
    }

    #[test]
    fn test_parse_data_type_varchar_max() {
        let (base, len, prec, scale) = parse_data_type("VARCHAR(MAX)");
        assert_eq!(base, "VARCHAR");
        assert_eq!(len, Some(-1));
        assert_eq!(prec, None);
        assert_eq!(scale, None);
    }

    #[test]
    fn test_parse_data_type_decimal_precision_scale() {
        let (base, len, prec, scale) = parse_data_type("DECIMAL(18,2)");
        assert_eq!(base, "DECIMAL");
        assert_eq!(len, None);
        assert_eq!(prec, Some(18));
        assert_eq!(scale, Some(2));
    }

    #[test]
    fn test_parse_data_type_decimal_precision_only() {
        let (base, len, prec, scale) = parse_data_type("DECIMAL(18)");
        assert_eq!(base, "DECIMAL");
        assert_eq!(len, None);
        assert_eq!(prec, Some(18));
        assert_eq!(scale, Some(0));
    }
}
