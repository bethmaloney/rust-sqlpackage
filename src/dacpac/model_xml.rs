//! Generate model.xml for dacpac

use quick_xml::events::{BytesCData, BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

use crate::model::{
    ColumnElement, ConstraintElement, ConstraintType, DatabaseModel, ExtendedPropertyElement,
    FunctionElement, IndexElement, ModelElement, ProcedureElement, RawElement, SchemaElement,
    SequenceElement, TableElement, UserDefinedTypeElement, ViewElement,
};
use crate::project::SqlProject;

const NAMESPACE: &str = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02";

/// Built-in schemas that exist by default in SQL Server
const BUILTIN_SCHEMAS: &[&str] = &[
    "dbo",
    "guest",
    "INFORMATION_SCHEMA",
    "sys",
    "db_owner",
    "db_accessadmin",
    "db_securityadmin",
    "db_ddladmin",
    "db_backupoperator",
    "db_datareader",
    "db_datawriter",
    "db_denydatareader",
    "db_denydatawriter",
];

/// Check if a schema name is a built-in SQL Server schema
fn is_builtin_schema(schema: &str) -> bool {
    BUILTIN_SCHEMAS
        .iter()
        .any(|&s| s.eq_ignore_ascii_case(schema))
}

pub fn generate_model_xml<W: Write>(
    writer: W,
    model: &DatabaseModel,
    project: &SqlProject,
) -> anyhow::Result<()> {
    let mut xml_writer = Writer::new_with_indent(writer, b' ', 2);

    // XML declaration
    xml_writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    // Root element
    let mut root = BytesStart::new("DataSchemaModel");
    root.push_attribute(("FileFormatVersion", model.file_format_version.as_str()));
    root.push_attribute(("SchemaVersion", model.schema_version.as_str()));
    root.push_attribute(("DspName", project.target_platform.dsp_name()));
    let collation_lcid = project.collation_lcid.to_string();
    root.push_attribute(("CollationLcid", collation_lcid.as_str()));
    root.push_attribute(("CollationCaseSensitive", "False"));
    root.push_attribute(("xmlns", NAMESPACE));
    xml_writer.write_event(Event::Start(root))?;

    // Model element
    xml_writer.write_event(Event::Start(BytesStart::new("Model")))?;

    // Write each element
    for element in &model.elements {
        write_element(&mut xml_writer, element)?;
    }

    // Close Model
    xml_writer.write_event(Event::End(BytesEnd::new("Model")))?;

    // Close root
    xml_writer.write_event(Event::End(BytesEnd::new("DataSchemaModel")))?;

    Ok(())
}

fn write_element<W: Write>(writer: &mut Writer<W>, element: &ModelElement) -> anyhow::Result<()> {
    match element {
        ModelElement::Schema(s) => write_schema(writer, s),
        ModelElement::Table(t) => write_table(writer, t),
        ModelElement::View(v) => write_view(writer, v),
        ModelElement::Procedure(p) => write_procedure(writer, p),
        ModelElement::Function(f) => write_function(writer, f),
        ModelElement::Index(i) => write_index(writer, i),
        ModelElement::Constraint(c) => write_constraint(writer, c),
        ModelElement::Sequence(s) => write_sequence(writer, s),
        ModelElement::UserDefinedType(u) => write_user_defined_type(writer, u),
        ModelElement::ExtendedProperty(e) => write_extended_property(writer, e),
        ModelElement::Raw(r) => write_raw(writer, r),
    }
}

fn write_schema<W: Write>(writer: &mut Writer<W>, schema: &SchemaElement) -> anyhow::Result<()> {
    // Skip built-in schemas - they exist by default in SQL Server and are referenced
    // with ExternalSource="BuiltIns" in relationships
    if is_builtin_schema(&schema.name) {
        return Ok(());
    }

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlSchema"));
    elem.push_attribute(("Name", format!("[{}]", schema.name).as_str()));
    writer.write_event(Event::Empty(elem))?;
    Ok(())
}

fn write_table<W: Write>(writer: &mut Writer<W>, table: &TableElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", table.schema, table.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTable"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Relationship to schema
    write_schema_relationship(writer, &table.schema)?;

    // Relationship to columns
    if !table.columns.is_empty() {
        let mut rel = BytesStart::new("Relationship");
        rel.push_attribute(("Name", "Columns"));
        writer.write_event(Event::Start(rel))?;

        for col in &table.columns {
            write_column(writer, col, &full_name)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_column<W: Write>(
    writer: &mut Writer<W>,
    column: &ColumnElement,
    table_name: &str,
) -> anyhow::Result<()> {
    write_column_with_type(writer, column, table_name, "SqlSimpleColumn")
}

/// Write a table type column (uses SqlTableTypeSimpleColumn for user-defined table types)
fn write_table_type_column<W: Write>(
    writer: &mut Writer<W>,
    column: &ColumnElement,
    type_name: &str,
) -> anyhow::Result<()> {
    write_column_with_type(writer, column, type_name, "SqlTableTypeSimpleColumn")
}

fn write_column_with_type<W: Write>(
    writer: &mut Writer<W>,
    column: &ColumnElement,
    parent_name: &str,
    column_type: &str,
) -> anyhow::Result<()> {
    let col_name = format!("{}.[{}]", parent_name, column.name);

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", column_type));
    elem.push_attribute(("Name", col_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Properties
    write_property(
        writer,
        "IsNullable",
        if column.is_nullable { "True" } else { "False" },
    )?;

    if column.is_identity {
        write_property(writer, "IsIdentity", "True")?;
    }

    if column.is_filestream {
        write_property(writer, "IsFileStream", "True")?;
    }

    // Data type relationship
    write_type_specifier(
        writer,
        &column.data_type,
        column.max_length,
        column.precision,
        column.scale,
    )?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

fn write_type_specifier<W: Write>(
    writer: &mut Writer<W>,
    data_type: &str,
    max_length: Option<i32>,
    precision: Option<u8>,
    scale: Option<u8>,
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "TypeSpecifier"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTypeSpecifier"));
    writer.write_event(Event::Start(elem))?;

    // Write type reference based on data type (with ExternalSource for built-ins)
    let type_ref = sql_type_to_reference(data_type);
    write_builtin_type_relationship(writer, "Type", &type_ref)?;

    // Write length/precision/scale if applicable
    if let Some(len) = max_length {
        if len == -1 {
            write_property(writer, "IsMax", "True")?;
        } else {
            write_property(writer, "Length", &len.to_string())?;
        }
    }

    if let Some(p) = precision {
        write_property(writer, "Precision", &p.to_string())?;
    }

    if let Some(s) = scale {
        write_property(writer, "Scale", &s.to_string())?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

fn sql_type_to_reference(data_type: &str) -> String {
    // Extract base type name
    let base_type = data_type
        .split('(')
        .next()
        .unwrap_or(data_type)
        .trim()
        .to_lowercase();

    match base_type.as_str() {
        "int" => "[int]",
        "bigint" => "[bigint]",
        "smallint" => "[smallint]",
        "tinyint" => "[tinyint]",
        "bit" => "[bit]",
        "decimal" | "numeric" => "[decimal]",
        "money" => "[money]",
        "smallmoney" => "[smallmoney]",
        "float" => "[float]",
        "real" => "[real]",
        "datetime" => "[datetime]",
        "datetime2" => "[datetime2]",
        "date" => "[date]",
        "time" => "[time]",
        "datetimeoffset" => "[datetimeoffset]",
        "smalldatetime" => "[smalldatetime]",
        "char" => "[char]",
        "varchar" => "[varchar]",
        "text" => "[text]",
        "nchar" => "[nchar]",
        "nvarchar" => "[nvarchar]",
        "ntext" => "[ntext]",
        "binary" => "[binary]",
        "varbinary" => "[varbinary]",
        "image" => "[image]",
        "uniqueidentifier" => "[uniqueidentifier]",
        "xml" => "[xml]",
        _ => "[sql_variant]",
    }
    .to_string()
}

fn write_view<W: Write>(writer: &mut Writer<W>, view: &ViewElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", view.schema, view.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlView"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Extract just the query part (after AS) from the view definition
    // QueryScript should not include the CREATE VIEW ... AS prefix
    let query_script = extract_view_query(&view.definition);
    write_script_property(writer, "QueryScript", &query_script)?;

    write_schema_relationship(writer, &view.schema)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Extract the query part from a CREATE VIEW definition
/// Strips the "CREATE VIEW [name] AS" prefix, leaving just the SELECT statement
fn extract_view_query(definition: &str) -> String {
    // Find the AS keyword that separates the view header from the query
    // Pattern: CREATE VIEW [schema].[name] AS SELECT ...
    let def_upper = definition.to_uppercase();
    if let Some(as_pos) = def_upper
        .find("\nAS\n")
        .or_else(|| def_upper.find("\nAS "))
        .or_else(|| def_upper.find(" AS\n"))
        .or_else(|| def_upper.find(" AS "))
    {
        // Find the "AS" in the original string and skip past it
        let after_as = &definition[as_pos..];
        // Skip whitespace and "AS"
        let trimmed = after_as.trim_start();
        if trimmed.to_uppercase().starts_with("AS") {
            let query = trimmed[2..].trim_start();
            return query.to_string();
        }
    }
    // Fallback: return the original definition if we can't find AS
    definition.to_string()
}

fn write_procedure<W: Write>(
    writer: &mut Writer<W>,
    proc: &ProcedureElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", proc.schema, proc.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlProcedure"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write IsNativelyCompiled property if true
    if proc.is_natively_compiled {
        write_property(writer, "IsNativelyCompiled", "True")?;
    }

    // For procedures, BodyScript should contain only the body after the final AS keyword
    // Parameters must be defined as separate SqlSubroutineParameter elements
    // First, extract and write parameters
    let params = extract_procedure_parameters(&proc.definition);
    if !params.is_empty() {
        let mut param_rel = BytesStart::new("Relationship");
        param_rel.push_attribute(("Name", "Parameters"));
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
            let mut param_elem = BytesStart::new("Element");
            param_elem.push_attribute(("Type", "SqlSubroutineParameter"));
            param_elem.push_attribute(("Name", param_name.as_str()));
            writer.write_event(Event::Start(param_elem))?;

            // IsOutput property if applicable
            if param.is_output {
                write_property(writer, "IsOutput", "True")?;
            }

            // Data type relationship
            write_data_type_relationship(writer, &param.data_type)?;

            writer.write_event(Event::End(BytesEnd::new("Element")))?;
            writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    // Extract just the body part (after final AS)
    let body = extract_procedure_body_only(&proc.definition);
    write_script_property(writer, "BodyScript", &body)?;

    write_schema_relationship(writer, &proc.schema)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Represents an extracted procedure parameter
#[derive(Debug)]
struct ProcedureParameter {
    name: String,
    data_type: String,
    is_output: bool,
    #[allow(dead_code)] // Captured for potential future use
    default_value: Option<String>,
}

/// Extract parameters from a CREATE PROCEDURE definition
fn extract_procedure_parameters(definition: &str) -> Vec<ProcedureParameter> {
    let mut params = Vec::new();

    // Find the procedure name and the parameters that follow
    let def_upper = definition.to_uppercase();
    let proc_start = def_upper
        .find("CREATE PROCEDURE")
        .or_else(|| def_upper.find("CREATE PROC"));

    if proc_start.is_none() {
        return params;
    }

    let after_create = &definition[proc_start.unwrap()..];

    // Find the AS keyword that ends the parameter section
    // Parameters are between procedure name and AS
    let as_pos = find_standalone_as(after_create);
    if as_pos.is_none() {
        return params;
    }

    let header = &after_create[..as_pos.unwrap()];

    // Find parameters - they start with @
    // Parameters can be on the same line or multiple lines
    let param_regex = regex::Regex::new(
        r"@(\w+)\s+([A-Za-z0-9_\(\),\s]+?)(?:\s*=\s*([^,@]+?))?(?:\s+(OUTPUT|OUT))?(?:,|$|\s*\n)",
    )
    .unwrap();

    for cap in param_regex.captures_iter(header) {
        let name = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let data_type = cap
            .get(2)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let default_value = cap.get(3).map(|m| m.as_str().trim().to_string());
        let is_output = cap.get(4).is_some();

        if !name.is_empty() && !data_type.is_empty() {
            // Clean up data type (remove trailing keywords like NULL, OUTPUT)
            let clean_type = clean_data_type(&data_type);
            params.push(ProcedureParameter {
                name,
                data_type: clean_type,
                is_output,
                default_value,
            });
        }
    }

    params
}

/// Find the standalone AS keyword that separates procedure header from body
fn find_standalone_as(s: &str) -> Option<usize> {
    let upper = s.to_uppercase();
    let chars: Vec<char> = upper.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Look for AS preceded by whitespace/newline and followed by whitespace/newline
        if i + 2 <= chars.len() && chars[i] == 'A' && chars[i + 1] == 'S' {
            let prev_ok = i == 0 || chars[i - 1].is_whitespace();
            let next_ok = i + 2 >= chars.len() || chars[i + 2].is_whitespace();
            if prev_ok && next_ok {
                // Make sure this isn't part of a longer word
                let next_next_ok = i + 3 >= chars.len() || !chars[i + 2].is_alphanumeric();
                if next_next_ok {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

/// Clean up a data type string removing trailing keywords
fn clean_data_type(dt: &str) -> String {
    let trimmed = dt.trim().to_uppercase();
    // Remove trailing NULL, NOT NULL, etc.
    let cleaned = trimmed
        .trim_end_matches(" NULL")
        .trim_end_matches(" NOT")
        .trim();
    cleaned.to_string()
}

/// Extract just the body after AS from a procedure definition
fn extract_procedure_body_only(definition: &str) -> String {
    // Find the AS keyword that separates header from body
    let as_pos = find_standalone_as(definition);
    if let Some(pos) = as_pos {
        let after_as = &definition[pos..];
        // Skip "AS" and any following whitespace
        let trimmed = after_as.trim_start();
        if trimmed.to_uppercase().starts_with("AS") {
            let body = trimmed[2..].trim_start();
            return body.to_string();
        }
    }
    definition.to_string()
}

/// Write the data type relationship for a parameter with inline type specifier
fn write_data_type_relationship<W: Write>(
    writer: &mut Writer<W>,
    data_type: &str,
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "Type"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Parse the data type and write an inline SqlTypeSpecifier element
    let (base_type, length, precision, scale) = parse_data_type(data_type);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTypeSpecifier"));
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
    let mut type_rel = BytesStart::new("Relationship");
    type_rel.push_attribute(("Name", "Type"));
    writer.write_event(Event::Start(type_rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;
    let mut refs = BytesStart::new("References");
    refs.push_attribute(("ExternalSource", "BuiltIns"));
    refs.push_attribute(("Name", type_ref.as_str()));
    writer.write_event(Event::Empty(refs))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Parse a SQL data type into (base_type, length, precision, scale)
fn parse_data_type(data_type: &str) -> (String, Option<i32>, Option<i32>, Option<i32>) {
    let dt_upper = data_type.to_uppercase().trim().to_string();

    // Handle types with parameters like VARCHAR(50), DECIMAL(10,2), VARCHAR(MAX)
    if let Some(paren_pos) = dt_upper.find('(') {
        let base_type = dt_upper[..paren_pos].to_string();
        let params_end = dt_upper.rfind(')').unwrap_or(dt_upper.len());
        let params = &dt_upper[paren_pos + 1..params_end];

        // Check for MAX
        if params.trim().eq_ignore_ascii_case("MAX") {
            return (base_type, Some(-1), None, None);
        }

        // Parse numeric parameters
        let parts: Vec<&str> = params.split(',').collect();
        if parts.len() == 1 {
            // Single parameter (length or precision)
            let val: i32 = parts[0].trim().parse().unwrap_or(0);
            match base_type.as_str() {
                "DECIMAL" | "NUMERIC" => (base_type, None, Some(val), Some(0)),
                _ => (base_type, Some(val), None, None),
            }
        } else if parts.len() == 2 {
            // Two parameters (precision, scale)
            let prec: i32 = parts[0].trim().parse().unwrap_or(0);
            let scale: i32 = parts[1].trim().parse().unwrap_or(0);
            (base_type, None, Some(prec), Some(scale))
        } else {
            (base_type, None, None, None)
        }
    } else {
        (dt_upper, None, None, None)
    }
}

fn write_function<W: Write>(writer: &mut Writer<W>, func: &FunctionElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", func.schema, func.name);
    let type_name = match func.function_type {
        crate::model::FunctionType::Scalar => "SqlScalarFunction",
        crate::model::FunctionType::TableValued => "SqlMultiStatementTableValuedFunction",
        crate::model::FunctionType::InlineTableValued => "SqlInlineTableValuedFunction",
    };

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", type_name));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write IsAnsiNullsOn property (always true for functions)
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Write IsNativelyCompiled property if true
    if func.is_natively_compiled {
        write_property(writer, "IsNativelyCompiled", "True")?;
    }

    // Write FunctionBody relationship with SqlScriptFunctionImplementation
    // BodyScript contains only the function body (BEGIN...END), not the header
    let body = extract_function_body(&func.definition);
    let header = extract_function_header(&func.definition);
    write_function_body_with_annotation(writer, &body, &header)?;

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

/// Write FunctionBody relationship for functions with nested SqlScriptFunctionImplementation
/// Includes SysCommentsObjectAnnotation with HeaderContents for DacFx compatibility
fn write_function_body_with_annotation<W: Write>(
    writer: &mut Writer<W>,
    body: &str,
    header: &str,
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "FunctionBody"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlScriptFunctionImplementation"));
    writer.write_event(Event::Start(elem))?;

    // Write BodyScript property with the function body only (BEGIN...END)
    write_script_property(writer, "BodyScript", body)?;

    // Write SysCommentsObjectAnnotation with HeaderContents
    let mut annotation = BytesStart::new("Annotation");
    annotation.push_attribute(("Type", "SysCommentsObjectAnnotation"));
    writer.write_event(Event::Start(annotation))?;

    // Calculate length (header + body)
    let total_length = header.len() + body.len();
    write_property(writer, "Length", &total_length.to_string())?;
    write_property(writer, "StartLine", "1")?;
    write_property(writer, "StartColumn", "1")?;

    // Write HeaderContents with XML-escaped header
    write_property(writer, "HeaderContents", header)?;

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
fn write_function_return_type<W: Write>(writer: &mut Writer<W>, return_type: &str) -> anyhow::Result<()> {
    // Extract base type name (e.g., "INT" -> "int", "DECIMAL(18,2)" -> "decimal")
    let base_type = extract_base_type_name(return_type);
    let type_ref = format!("[{}]", base_type.to_lowercase());

    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "Type"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTypeSpecifier"));
    writer.write_event(Event::Start(elem))?;

    // Nested Type relationship referencing the built-in type
    let mut inner_rel = BytesStart::new("Relationship");
    inner_rel.push_attribute(("Name", "Type"));
    writer.write_event(Event::Start(inner_rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut refs = BytesStart::new("References");
    refs.push_attribute(("ExternalSource", "BuiltIns"));
    refs.push_attribute(("Name", type_ref.as_str()));
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

/// Extract the body part from a CREATE FUNCTION definition
/// Returns just the body (BEGIN...END or RETURN(...)) without the header
fn extract_function_body(definition: &str) -> String {
    let def_upper = definition.to_uppercase();

    // Find RETURNS and then AS after it
    // Pattern: CREATE FUNCTION [name](...) RETURNS type AS BEGIN ... END
    // AS can be preceded by space or newline
    if let Some(returns_pos) = def_upper.find("RETURNS") {
        let after_returns = &def_upper[returns_pos..];
        // Find AS (could be "\nAS" or " AS" - with word boundary)
        // We need to find " AS " or "\nAS" to avoid matching within a type like "ALIAS"
        let as_regex = regex::Regex::new(r"(?i)[\s\n]AS[\s\n]").unwrap();
        if let Some(m) = as_regex.find(after_returns) {
            // Calculate absolute position in original string
            let abs_as_pos = returns_pos + m.end();
            // Return everything after AS
            return definition[abs_as_pos..].trim().to_string();
        }
    }

    // Fallback: return the original definition
    definition.to_string()
}

/// Extract the header part from a CREATE FUNCTION definition
/// Returns everything up to and including AS (CREATE FUNCTION [name](...) RETURNS type AS\n)
/// Preserves trailing whitespace after AS to ensure proper separation from body
fn extract_function_header(definition: &str) -> String {
    let def_upper = definition.to_uppercase();

    // Find RETURNS and then AS after it
    if let Some(returns_pos) = def_upper.find("RETURNS") {
        let after_returns = &def_upper[returns_pos..];
        // Find AS keyword
        let as_regex = regex::Regex::new(r"(?i)[\s\n]AS[\s\n]").unwrap();
        if let Some(m) = as_regex.find(after_returns) {
            // Calculate absolute position in original string (include the AS and trailing whitespace)
            let abs_as_end = returns_pos + m.end();
            // Return everything up to and including AS with trailing whitespace
            // Use trim_start() to only remove leading whitespace, preserving trailing newline
            return definition[..abs_as_end].trim_start().to_string();
        }
    }

    // Fallback: return empty string
    String::new()
}

fn write_index<W: Write>(writer: &mut Writer<W>, index: &IndexElement) -> anyhow::Result<()> {
    let full_name = format!(
        "[{}].[{}].[{}]",
        index.table_schema, index.table_name, index.name
    );

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlIndex"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    if index.is_unique {
        write_property(writer, "IsUnique", "True")?;
    }

    if index.is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    // Reference to table
    let table_ref = format!("[{}].[{}]", index.table_schema, index.table_name);
    write_relationship(writer, "IndexedObject", &[&table_ref])?;

    // Write ColumnSpecifications for key columns
    if !index.columns.is_empty() {
        write_index_column_specifications(writer, index, &table_ref)?;
    }

    // Write IncludedColumns relationship if present
    if !index.include_columns.is_empty() {
        let include_refs: Vec<String> = index
            .include_columns
            .iter()
            .map(|col| format!("{}.[{}]", table_ref, col))
            .collect();
        let include_refs: Vec<&str> = include_refs.iter().map(|s| s.as_str()).collect();
        write_relationship(writer, "IncludedColumns", &include_refs)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_index_column_specifications<W: Write>(
    writer: &mut Writer<W>,
    index: &IndexElement,
    table_ref: &str,
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "ColumnSpecifications"));
    writer.write_event(Event::Start(rel))?;

    for (i, col) in index.columns.iter().enumerate() {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let spec_name = format!(
            "[{}].[{}].[{}].[{}]",
            index.table_schema, index.table_name, index.name, i
        );

        let mut elem = BytesStart::new("Element");
        elem.push_attribute(("Type", "SqlIndexedColumnSpecification"));
        elem.push_attribute(("Name", spec_name.as_str()));
        writer.write_event(Event::Start(elem))?;

        // Reference to the column
        let col_ref = format!("{}.[{}]", table_ref, col);
        write_relationship(writer, "Column", &[&col_ref])?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

fn write_constraint<W: Write>(
    writer: &mut Writer<W>,
    constraint: &ConstraintElement,
) -> anyhow::Result<()> {
    let full_name = format!(
        "[{}].[{}].[{}]",
        constraint.table_schema, constraint.table_name, constraint.name
    );

    let type_name = match constraint.constraint_type {
        ConstraintType::PrimaryKey => "SqlPrimaryKeyConstraint",
        ConstraintType::ForeignKey => "SqlForeignKeyConstraint",
        ConstraintType::Unique => "SqlUniqueConstraint",
        ConstraintType::Check => "SqlCheckConstraint",
        ConstraintType::Default => "SqlDefaultConstraint",
    };

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", type_name));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write IsClustered property for primary keys and unique constraints
    if matches!(
        constraint.constraint_type,
        ConstraintType::PrimaryKey | ConstraintType::Unique
    ) {
        if let Some(is_clustered) = constraint.is_clustered {
            write_property(
                writer,
                "IsClustered",
                if is_clustered { "True" } else { "False" },
            )?;
        }
    }

    // Reference to table
    let table_ref = format!("[{}].[{}]", constraint.table_schema, constraint.table_name);
    write_relationship(writer, "DefiningTable", &[&table_ref])?;

    // Write column relationships based on constraint type
    if !constraint.columns.is_empty() {
        let table_ref = format!("[{}].[{}]", constraint.table_schema, constraint.table_name);

        match constraint.constraint_type {
            ConstraintType::PrimaryKey | ConstraintType::Unique => {
                // Primary keys and unique constraints use ColumnSpecifications with inline elements
                let mut rel = BytesStart::new("Relationship");
                rel.push_attribute(("Name", "ColumnSpecifications"));
                writer.write_event(Event::Start(rel))?;

                for col in &constraint.columns {
                    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

                    let mut col_elem = BytesStart::new("Element");
                    col_elem.push_attribute(("Type", "SqlIndexedColumnSpecification"));
                    writer.write_event(Event::Start(col_elem))?;

                    // Note: DacFx SqlIndexedColumnSpecification doesn't have a property for
                    // descending sort order - columns default to ascending. The sort direction
                    // is stored in the model for potential future use.

                    // Reference to the actual column
                    let col_ref = format!("{}.[{}]", table_ref, col.name);
                    write_relationship(writer, "Column", &[&col_ref])?;

                    writer.write_event(Event::End(BytesEnd::new("Element")))?;
                    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
                }

                writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
            }
            ConstraintType::ForeignKey => {
                // Foreign keys use Columns relationship with references
                let column_refs: Vec<String> = constraint
                    .columns
                    .iter()
                    .map(|c| format!("{}.[{}]", table_ref, c.name))
                    .collect();
                let column_refs_str: Vec<&str> = column_refs.iter().map(|s| s.as_str()).collect();
                write_relationship(writer, "Columns", &column_refs_str)?;
            }
            _ => {}
        }
    }

    // For foreign keys, add reference to foreign table and foreign columns
    if constraint.constraint_type == ConstraintType::ForeignKey {
        if let Some(ref foreign_table) = constraint.referenced_table {
            write_relationship(writer, "ForeignTable", &[foreign_table])?;

            // Write ForeignColumns relationship
            if let Some(ref foreign_columns) = constraint.referenced_columns {
                if !foreign_columns.is_empty() {
                    let foreign_col_refs: Vec<String> = foreign_columns
                        .iter()
                        .map(|c| format!("{}.[{}]", foreign_table, c))
                        .collect();
                    let foreign_col_refs_str: Vec<&str> =
                        foreign_col_refs.iter().map(|s| s.as_str()).collect();
                    write_relationship(writer, "ForeignColumns", &foreign_col_refs_str)?;
                }
            }
        }
    }

    // Check constraint expression - use CheckExpressionScript property with CDATA
    if constraint.constraint_type == ConstraintType::Check {
        if let Some(ref definition) = constraint.definition {
            write_script_property(writer, "CheckExpressionScript", definition)?;
        }
    } else if constraint.constraint_type == ConstraintType::Default {
        // Default constraint expression
        if let Some(ref definition) = constraint.definition {
            write_script_property(writer, "DefaultExpressionScript", definition)?;
        }
        // Default constraints need a ForColumn relationship to specify the target column
        if !constraint.columns.is_empty() {
            let table_ref = format!("[{}].[{}]", constraint.table_schema, constraint.table_name);
            let col_ref = format!("{}.[{}]", table_ref, constraint.columns[0].name);
            write_relationship(writer, "ForColumn", &[&col_ref])?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_property<W: Write>(writer: &mut Writer<W>, name: &str, value: &str) -> anyhow::Result<()> {
    let mut prop = BytesStart::new("Property");
    prop.push_attribute(("Name", name));
    prop.push_attribute(("Value", value));
    writer.write_event(Event::Empty(prop))?;
    Ok(())
}

/// Write a property with a CDATA value (for script content like QueryScript, BodyScript)
fn write_script_property<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    script: &str,
) -> anyhow::Result<()> {
    let mut prop = BytesStart::new("Property");
    prop.push_attribute(("Name", name));
    writer.write_event(Event::Start(prop))?;

    // Write Value element with CDATA content
    writer.write_event(Event::Start(BytesStart::new("Value")))?;
    writer.write_event(Event::CData(BytesCData::new(script)))?;
    writer.write_event(Event::End(BytesEnd::new("Value")))?;

    writer.write_event(Event::End(BytesEnd::new("Property")))?;
    Ok(())
}

fn write_relationship<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    references: &[&str],
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", name));
    writer.write_event(Event::Start(rel))?;

    for reference in references {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let mut refs = BytesStart::new("References");
        refs.push_attribute(("Name", *reference));
        writer.write_event(Event::Empty(refs))?;

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

fn write_builtin_type_relationship<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    type_ref: &str,
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", name));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut refs = BytesStart::new("References");
    refs.push_attribute(("ExternalSource", "BuiltIns"));
    refs.push_attribute(("Name", type_ref));
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write a Schema relationship, using ExternalSource="BuiltIns" for built-in schemas
fn write_schema_relationship<W: Write>(writer: &mut Writer<W>, schema: &str) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "Schema"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let schema_ref = format!("[{}]", schema);
    let mut refs = BytesStart::new("References");
    if is_builtin_schema(schema) {
        refs.push_attribute(("ExternalSource", "BuiltIns"));
    }
    refs.push_attribute(("Name", schema_ref.as_str()));
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

fn write_sequence<W: Write>(writer: &mut Writer<W>, seq: &SequenceElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", seq.schema, seq.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlSequence"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Relationship to schema
    write_schema_relationship(writer, &seq.schema)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_user_defined_type<W: Write>(
    writer: &mut Writer<W>,
    udt: &UserDefinedTypeElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", udt.schema, udt.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTableType"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Relationship to schema
    write_schema_relationship(writer, &udt.schema)?;

    // Relationship to columns (table types use SqlTableTypeColumn instead of SqlSimpleColumn)
    if !udt.columns.is_empty() {
        let mut rel = BytesStart::new("Relationship");
        rel.push_attribute(("Name", "Columns"));
        writer.write_event(Event::Start(rel))?;

        for col in &udt.columns {
            write_table_type_column(writer, col, &full_name)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_raw<W: Write>(writer: &mut Writer<W>, raw: &RawElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", raw.schema, raw.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", raw.sql_type.as_str()));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write BodyScript property with CDATA containing the definition
    write_script_property(writer, "BodyScript", &raw.definition)?;

    // Relationship to schema
    write_schema_relationship(writer, &raw.schema)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write an extended property element
/// Format:
/// ```xml
/// <Element Type="SqlExtendedProperty" Name="[dbo].[Table].[MS_Description]">
///   <Property Name="Value"><Value><![CDATA[Description text]]></Value></Property>
///   <Relationship Name="ExtendedObject">
///     <Entry>
///       <References Name="[dbo].[Table]"/>
///     </Entry>
///   </Relationship>
/// </Element>
/// ```
fn write_extended_property<W: Write>(
    writer: &mut Writer<W>,
    ext_prop: &ExtendedPropertyElement,
) -> anyhow::Result<()> {
    let full_name = ext_prop.full_name();

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlExtendedProperty"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write Value property with CDATA containing the property value
    write_script_property(writer, "Value", &ext_prop.property_value)?;

    // Write ExtendedObject relationship pointing to the target object (table or column)
    let extends_ref = ext_prop.extends_object_ref();
    write_relationship(writer, "ExtendedObject", &[&extends_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}
