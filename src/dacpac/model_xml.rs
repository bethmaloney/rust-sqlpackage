//! Generate model.xml for dacpac

use quick_xml::events::{BytesCData, BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

use crate::model::{
    ColumnElement, ConstraintColumn, ConstraintElement, ConstraintType, DatabaseModel,
    ExtendedPropertyElement, FullTextCatalogElement, FullTextIndexElement, FunctionElement,
    IndexElement, ModelElement, ProcedureElement, RawElement, ScalarTypeElement, SchemaElement,
    SequenceElement, SortDirection, TableElement, TableTypeColumnElement, TableTypeConstraint,
    TriggerElement, UserDefinedTypeElement, ViewElement,
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

    // Header element with CustomData entries
    write_header(&mut xml_writer, project)?;

    // Model element
    xml_writer.write_event(Event::Start(BytesStart::new("Model")))?;

    // Write SqlDatabaseOptions element first
    write_database_options(&mut xml_writer, project)?;

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

/// Write the Header section with CustomData entries for AnsiNulls, QuotedIdentifier, CompatibilityMode, References, and SqlCmdVariables
fn write_header<W: Write>(writer: &mut Writer<W>, project: &SqlProject) -> anyhow::Result<()> {
    writer.write_event(Event::Start(BytesStart::new("Header")))?;

    // AnsiNulls
    write_custom_data(
        writer,
        "AnsiNulls",
        "AnsiNulls",
        if project.ansi_nulls { "True" } else { "False" },
    )?;

    // QuotedIdentifier
    write_custom_data(
        writer,
        "QuotedIdentifier",
        "QuotedIdentifier",
        if project.quoted_identifier {
            "True"
        } else {
            "False"
        },
    )?;

    // CompatibilityMode
    let compat_mode = project.target_platform.compatibility_mode().to_string();
    write_custom_data(
        writer,
        "CompatibilityMode",
        "CompatibilityMode",
        &compat_mode,
    )?;

    // Package references (e.g., Microsoft.SqlServer.Dacpacs.Master)
    for pkg_ref in &project.package_references {
        write_package_reference(writer, pkg_ref)?;
    }

    // SQLCMD variables
    for sqlcmd_var in &project.sqlcmd_variables {
        write_sqlcmd_variable(writer, sqlcmd_var)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Header")))?;
    Ok(())
}

/// Write a CustomData element for a package reference
/// Format:
/// ```xml
/// <CustomData Category="Reference" Type="SqlSchema">
///   <Metadata Name="FileName" Value="master.dacpac" />
///   <Metadata Name="LogicalName" Value="master.dacpac" />
///   <Metadata Name="SuppressMissingDependenciesErrors" Value="False" />
/// </CustomData>
/// ```
fn write_package_reference<W: Write>(
    writer: &mut Writer<W>,
    pkg_ref: &crate::project::PackageReference,
) -> anyhow::Result<()> {
    // Extract dacpac name from package name
    // e.g., "Microsoft.SqlServer.Dacpacs.Master" -> "master.dacpac"
    let dacpac_name = extract_dacpac_name(&pkg_ref.name);

    let mut custom_data = BytesStart::new("CustomData");
    custom_data.push_attribute(("Category", "Reference"));
    custom_data.push_attribute(("Type", "SqlSchema"));
    writer.write_event(Event::Start(custom_data))?;

    // FileName metadata
    let mut filename = BytesStart::new("Metadata");
    filename.push_attribute(("Name", "FileName"));
    filename.push_attribute(("Value", dacpac_name.as_str()));
    writer.write_event(Event::Empty(filename))?;

    // LogicalName metadata
    let mut logical_name = BytesStart::new("Metadata");
    logical_name.push_attribute(("Name", "LogicalName"));
    logical_name.push_attribute(("Value", dacpac_name.as_str()));
    writer.write_event(Event::Empty(logical_name))?;

    // SuppressMissingDependenciesErrors metadata
    let mut suppress = BytesStart::new("Metadata");
    suppress.push_attribute(("Name", "SuppressMissingDependenciesErrors"));
    suppress.push_attribute(("Value", "False"));
    writer.write_event(Event::Empty(suppress))?;

    writer.write_event(Event::End(BytesEnd::new("CustomData")))?;
    Ok(())
}

/// Write a CustomData element for a SQLCMD variable
/// Format:
/// ```xml
/// <CustomData Category="SqlCmdVariable">
///   <Metadata Name="SqlCmdVariable" Value="Environment" />
///   <Metadata Name="DefaultValue" Value="Development" />
/// </CustomData>
/// ```
fn write_sqlcmd_variable<W: Write>(
    writer: &mut Writer<W>,
    sqlcmd_var: &crate::project::SqlCmdVariable,
) -> anyhow::Result<()> {
    let mut custom_data = BytesStart::new("CustomData");
    custom_data.push_attribute(("Category", "SqlCmdVariable"));
    writer.write_event(Event::Start(custom_data))?;

    // SqlCmdVariable metadata (the variable name)
    let mut var_name = BytesStart::new("Metadata");
    var_name.push_attribute(("Name", "SqlCmdVariable"));
    var_name.push_attribute(("Value", sqlcmd_var.name.as_str()));
    writer.write_event(Event::Empty(var_name))?;

    // DefaultValue metadata
    let mut default_val = BytesStart::new("Metadata");
    default_val.push_attribute(("Name", "DefaultValue"));
    default_val.push_attribute(("Value", sqlcmd_var.default_value.as_str()));
    writer.write_event(Event::Empty(default_val))?;

    writer.write_event(Event::End(BytesEnd::new("CustomData")))?;
    Ok(())
}

/// Extract dacpac name from a package reference name
/// e.g., "Microsoft.SqlServer.Dacpacs.Master" -> "master.dacpac"
/// e.g., "Microsoft.SqlServer.Dacpacs.Msdb" -> "msdb.dacpac"
fn extract_dacpac_name(package_name: &str) -> String {
    // Common pattern: Microsoft.SqlServer.Dacpacs.<DatabaseName>
    if let Some(last_part) = package_name.split('.').next_back() {
        format!("{}.dacpac", last_part.to_lowercase())
    } else {
        format!("{}.dacpac", package_name.to_lowercase())
    }
}

/// Write a CustomData element with a single Metadata child
/// Format: <CustomData Category="category"><Metadata Name="name" Value="value"/></CustomData>
fn write_custom_data<W: Write>(
    writer: &mut Writer<W>,
    category: &str,
    name: &str,
    value: &str,
) -> anyhow::Result<()> {
    let mut custom_data = BytesStart::new("CustomData");
    custom_data.push_attribute(("Category", category));
    writer.write_event(Event::Start(custom_data))?;

    let mut metadata = BytesStart::new("Metadata");
    metadata.push_attribute(("Name", name));
    metadata.push_attribute(("Value", value));
    writer.write_event(Event::Empty(metadata))?;

    writer.write_event(Event::End(BytesEnd::new("CustomData")))?;
    Ok(())
}

/// Write the SqlDatabaseOptions element
/// Format:
/// ```xml
/// <Element Type="SqlDatabaseOptions">
///   <Property Name="Collation" Value="Latin1_General_CI_AS"/>
///   <Property Name="IsAnsiNullDefaultOn" Value="True"/>
///   <Property Name="IsAnsiNullsOn" Value="True"/>
///   <Property Name="IsAnsiWarningsOn" Value="True"/>
///   <Property Name="IsArithAbortOn" Value="True"/>
///   <Property Name="IsConcatNullYieldsNullOn" Value="True"/>
///   <Property Name="IsFullTextEnabled" Value="False"/>
///   <Property Name="PageVerifyMode" Value="3"/>
/// </Element>
/// ```
fn write_database_options<W: Write>(
    writer: &mut Writer<W>,
    project: &SqlProject,
) -> anyhow::Result<()> {
    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlDatabaseOptions"));
    writer.write_event(Event::Start(elem))?;

    let db_options = &project.database_options;

    // Collation (if specified)
    if let Some(ref collation) = db_options.collation {
        write_property(writer, "Collation", collation)?;
    }

    // IsAnsiNullDefaultOn
    write_property(
        writer,
        "IsAnsiNullDefaultOn",
        if db_options.ansi_null_default_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsAnsiNullsOn
    write_property(
        writer,
        "IsAnsiNullsOn",
        if db_options.ansi_nulls_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsAnsiWarningsOn
    write_property(
        writer,
        "IsAnsiWarningsOn",
        if db_options.ansi_warnings_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsArithAbortOn
    write_property(
        writer,
        "IsArithAbortOn",
        if db_options.arith_abort_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsConcatNullYieldsNullOn
    write_property(
        writer,
        "IsConcatNullYieldsNullOn",
        if db_options.concat_null_yields_null_on {
            "True"
        } else {
            "False"
        },
    )?;

    // IsFullTextEnabled
    write_property(
        writer,
        "IsFullTextEnabled",
        if db_options.full_text_enabled {
            "True"
        } else {
            "False"
        },
    )?;

    // PageVerifyMode (convert string to numeric value for DacFx compatibility)
    // NONE = 0, TORN_PAGE_DETECTION = 1, CHECKSUM = 3
    if let Some(ref page_verify) = db_options.page_verify {
        let mode_value = match page_verify.to_uppercase().as_str() {
            "NONE" => "0",
            "TORN_PAGE_DETECTION" => "1",
            "CHECKSUM" => "3",
            _ => "3", // Default to CHECKSUM
        };
        write_property(writer, "PageVerifyMode", mode_value)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
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
        ModelElement::FullTextIndex(f) => write_fulltext_index(writer, f),
        ModelElement::FullTextCatalog(c) => write_fulltext_catalog(writer, c),
        ModelElement::Constraint(c) => write_constraint(writer, c),
        ModelElement::Sequence(s) => write_sequence(writer, s),
        ModelElement::UserDefinedType(u) => write_user_defined_type(writer, u),
        ModelElement::ScalarType(s) => write_scalar_type(writer, s),
        ModelElement::ExtendedProperty(e) => write_extended_property(writer, e),
        ModelElement::Trigger(t) => write_trigger(writer, t),
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

    // If no authorization, use empty element; otherwise write with relationship
    if schema.authorization.is_none() {
        writer.write_event(Event::Empty(elem))?;
    } else {
        writer.write_event(Event::Start(elem))?;

        // Write Authorizer relationship
        if let Some(ref auth) = schema.authorization {
            write_authorizer_relationship(writer, auth)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
    }
    Ok(())
}

/// Write an Authorizer relationship for schema authorization
fn write_authorizer_relationship<W: Write>(
    writer: &mut Writer<W>,
    owner: &str,
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "Authorizer"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let owner_ref = format!("[{}]", owner);
    let mut refs = BytesStart::new("References");
    // Built-in principals (like dbo) use ExternalSource="BuiltIns"
    if is_builtin_schema(owner) {
        refs.push_attribute(("ExternalSource", "BuiltIns"));
    }
    refs.push_attribute(("Name", owner_ref.as_str()));
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

fn write_table<W: Write>(writer: &mut Writer<W>, table: &TableElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", table.schema, table.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTable"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write IsAnsiNullsOn property (always true for tables - ANSI_NULLS ON is default)
    write_property(writer, "IsAnsiNullsOn", "True")?;

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

    // Relationship to schema (comes after Columns in DotNet output)
    write_schema_relationship(writer, &table.schema)?;

    // Write SqlInlineConstraintAnnotation if table has inline constraints
    // DotNet assigns a disambiguator to tables with inline constraints
    if let Some(disambiguator) = table.inline_constraint_disambiguator {
        let mut annotation = BytesStart::new("Annotation");
        annotation.push_attribute(("Type", "SqlInlineConstraintAnnotation"));
        annotation.push_attribute(("Disambiguator", disambiguator.to_string().as_str()));
        writer.write_event(Event::Empty(annotation))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_column<W: Write>(
    writer: &mut Writer<W>,
    column: &ColumnElement,
    table_name: &str,
) -> anyhow::Result<()> {
    // Check if this is a computed column
    if column.computed_expression.is_some() {
        write_computed_column(writer, column, table_name)
    } else {
        write_column_with_type(writer, column, table_name, "SqlSimpleColumn")
    }
}

/// Write a computed column element (SqlComputedColumn)
fn write_computed_column<W: Write>(
    writer: &mut Writer<W>,
    column: &ColumnElement,
    table_name: &str,
) -> anyhow::Result<()> {
    let col_name = format!("{}.[{}]", table_name, column.name);

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlComputedColumn"));
    elem.push_attribute(("Name", col_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // SqlComputedColumn does NOT support IsNullable property (unlike SqlSimpleColumn)
    // DotNet property order: ExpressionScript, IsPersisted (if true)

    // Write expression script first (DotNet order)
    if let Some(ref expr) = column.computed_expression {
        write_script_property(writer, "ExpressionScript", expr)?;
    }

    if column.is_persisted {
        write_property(writer, "IsPersisted", "True")?;
    }

    // Write ExpressionDependencies relationship for column references in the expression
    if let Some(ref expr) = column.computed_expression {
        // Parse schema and table name from qualified table_name like "[dbo].[Employees]"
        if let Some((schema, tbl)) = parse_qualified_table_name(table_name) {
            let deps = extract_computed_expression_columns(expr, &schema, &tbl);
            if !deps.is_empty() {
                write_expression_dependencies(writer, &deps)?;
            }
        }
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Parse a qualified table name like "[dbo].[Employees]" into schema and table components.
/// Returns (schema, table) without brackets.
fn parse_qualified_table_name(qualified_name: &str) -> Option<(String, String)> {
    // Match pattern: [schema].[table]
    let re = regex::Regex::new(r"^\[([^\]]+)\]\.\[([^\]]+)\]$").ok()?;
    let caps = re.captures(qualified_name)?;
    Some((caps[1].to_string(), caps[2].to_string()))
}

/// Write ExpressionDependencies relationship for computed columns
fn write_expression_dependencies<W: Write>(
    writer: &mut Writer<W>,
    dependencies: &[String],
) -> anyhow::Result<()> {
    if dependencies.is_empty() {
        return Ok(());
    }

    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "ExpressionDependencies"));
    writer.write_event(Event::Start(rel))?;

    for dep in dependencies {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let mut refs = BytesStart::new("References");
        refs.push_attribute(("Name", dep.as_str()));
        writer.write_event(Event::Empty(refs))?;

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Write a table type column (uses SqlTableTypeSimpleColumn for user-defined table types)
/// Note: DotNet never emits IsNullable for SqlTableTypeSimpleColumn, so we don't either
fn write_table_type_column<W: Write>(
    writer: &mut Writer<W>,
    column: &TableTypeColumnElement,
    type_name: &str,
) -> anyhow::Result<()> {
    let col_name = format!("{}.[{}]", type_name, column.name);

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTableTypeSimpleColumn"));
    elem.push_attribute(("Name", col_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Note: DotNet never emits IsNullable for SqlTableTypeSimpleColumn
    // regardless of whether the column is nullable or not, so we omit it

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

    // Properties - only emit IsNullable="False" for NOT NULL columns
    // DotNet never emits IsNullable="True" for nullable columns (explicit or implicit)
    if matches!(column.nullability, Some(false)) {
        write_property(writer, "IsNullable", "False")?;
    }

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

    // Write AttachedAnnotation elements linking column to inline constraints
    // DotNet uses <AttachedAnnotation Disambiguator="X" /> (no Type attribute)
    for disambiguator in &column.attached_annotations {
        let mut annotation = BytesStart::new("AttachedAnnotation");
        annotation.push_attribute(("Disambiguator", disambiguator.to_string().as_str()));
        writer.write_event(Event::Empty(annotation))?;
    }

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

    // DotNet order: Properties first, then Type relationship
    // Properties order: Scale, Precision, Length/IsMax
    if let Some(s) = scale {
        write_property(writer, "Scale", &s.to_string())?;
    }

    if let Some(p) = precision {
        write_property(writer, "Precision", &p.to_string())?;
    }

    if let Some(len) = max_length {
        if len == -1 {
            write_property(writer, "IsMax", "True")?;
        } else {
            write_property(writer, "Length", &len.to_string())?;
        }
    }

    // Write type reference based on data type (with ExternalSource for built-ins)
    let type_ref = sql_type_to_reference(data_type);
    write_builtin_type_relationship(writer, "Type", &type_ref)?;

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

    // Write properties in DotNet order:
    // 1. IsSchemaBound (if true)
    if view.is_schema_bound {
        write_property(writer, "IsSchemaBound", "True")?;
    }

    // 2. IsMetadataReported (if true)
    if view.is_metadata_reported {
        write_property(writer, "IsMetadataReported", "True")?;
    }

    // 3. QueryScript
    let query_script = extract_view_query(&view.definition);
    write_script_property(writer, "QueryScript", &query_script)?;

    // 4. IsWithCheckOption (if true)
    if view.is_with_check_option {
        write_property(writer, "IsWithCheckOption", "True")?;
    }

    // 5. IsAnsiNullsOn - always emit for views (current DotNet behavior)
    // Modern .NET DacFx emits this property for all views
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Only emit Columns and QueryDependencies for schema-bound or with-check-option views
    // DotNet only analyzes dependencies for these types of views
    if view.is_schema_bound || view.is_with_check_option {
        // Extract view columns and dependencies from the query
        let (columns, query_deps) = extract_view_columns_and_deps(&query_script, &view.schema);

        // 6. Write Columns relationship with SqlComputedColumn elements
        if !columns.is_empty() {
            write_view_columns(writer, &full_name, &columns)?;
        }

        // 7. Write QueryDependencies relationship
        if !query_deps.is_empty() {
            write_query_dependencies(writer, &query_deps)?;
        }
    }

    // 8. Schema relationship
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

/// Represents a view column with its name and optional source dependency
#[derive(Debug, Clone)]
struct ViewColumn {
    /// The output column name (alias or original name)
    name: String,
    /// The source column reference (if direct column reference), e.g., "[dbo].[Products].[Id]"
    source_ref: Option<String>,
}

/// Extract view columns and query dependencies from a SELECT statement
/// Returns: (columns, query_dependencies)
/// - columns: List of output columns with their source references
/// - query_dependencies: All tables and columns referenced in the query
fn extract_view_columns_and_deps(
    query: &str,
    default_schema: &str,
) -> (Vec<ViewColumn>, Vec<String>) {
    let mut columns = Vec::new();
    let mut query_deps = Vec::new();

    // Parse table aliases from FROM clause and JOINs
    let table_aliases = extract_table_aliases(query, default_schema);

    // Extract SELECT column list
    let select_columns = extract_select_columns(query);

    for col_expr in select_columns {
        let (col_name, source_ref) =
            parse_column_expression(&col_expr, &table_aliases, default_schema);
        columns.push(ViewColumn {
            name: col_name,
            source_ref,
        });
    }

    // Build QueryDependencies: tables first, then columns
    // Add all referenced tables (unique)
    for (_alias, table_ref) in &table_aliases {
        if !query_deps.contains(table_ref) {
            query_deps.push(table_ref.clone());
        }
    }

    // Add column references from the SELECT columns (the ones we already parsed)
    for col in &columns {
        if let Some(ref source_ref) = col.source_ref {
            if !query_deps.contains(source_ref) {
                query_deps.push(source_ref.clone());
            }
        }
    }

    // Add all column references from the rest of the query (WHERE, ON, GROUP BY, etc.)
    let all_column_refs = extract_all_column_references(query, &table_aliases, default_schema);
    for col_ref in all_column_refs {
        if !query_deps.contains(&col_ref) {
            query_deps.push(col_ref);
        }
    }

    (columns, query_deps)
}

/// Extract table aliases from FROM and JOIN clauses
/// Returns a map of alias -> full table reference (e.g., "p" -> "[dbo].[Products]")
fn extract_table_aliases(query: &str, default_schema: &str) -> Vec<(String, String)> {
    let mut aliases = Vec::new();

    // Regex to find table references with optional aliases
    // Matches patterns like:
    // - FROM [dbo].[Products] p
    // - FROM [dbo].[Products] AS p
    // - JOIN [dbo].[Categories] c ON
    // - FROM Products (without schema)
    // - FROM [dbo].[Products]; (with trailing semicolon)
    let table_pattern = regex::Regex::new(
        r"(?i)(?:FROM|(?:INNER|LEFT|RIGHT|OUTER|CROSS)?\s*JOIN)\s+(\[?[^\]\s]+\]?\.\[?[^\]\s]+\]?|\[?[^\]\s;]+\]?)\s*(?:AS\s+)?(\w+)?",
    )
    .unwrap();

    for cap in table_pattern.captures_iter(query) {
        let table_name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let alias = cap.get(2).map(|m| m.as_str()).unwrap_or("");

        // Clean up any trailing semicolons or whitespace
        let table_name_cleaned = table_name.trim_end_matches([';', ' ']);

        let full_table_ref = normalize_table_reference(table_name_cleaned, default_schema);

        if !alias.is_empty() {
            let alias_upper = alias.to_uppercase();
            // Skip if alias is actually a keyword
            if !matches!(
                alias_upper.as_str(),
                "ON" | "WHERE"
                    | "INNER"
                    | "LEFT"
                    | "RIGHT"
                    | "OUTER"
                    | "CROSS"
                    | "GROUP"
                    | "ORDER"
                    | "HAVING"
                    | "UNION"
                    | "WITH"
                    | "AS"
            ) {
                aliases.push((alias.to_string(), full_table_ref.clone()));
            }
        }

        // Also add the table name itself as an alias (for unaliased references)
        let simple_name = extract_simple_table_name(&full_table_ref);
        if !simple_name.is_empty() {
            aliases.push((simple_name, full_table_ref));
        }
    }

    aliases
}

/// Extract simple table name from full reference like "[dbo].[Products]" -> "Products"
fn extract_simple_table_name(full_ref: &str) -> String {
    let parts: Vec<&str> = full_ref.split('.').collect();
    if parts.len() >= 2 {
        parts[1].trim_matches(|c| c == '[' || c == ']').to_string()
    } else if !parts.is_empty() {
        parts[0].trim_matches(|c| c == '[' || c == ']').to_string()
    } else {
        String::new()
    }
}

/// Normalize a table reference to [schema].[table] format
fn normalize_table_reference(table_name: &str, default_schema: &str) -> String {
    let cleaned = table_name.trim();

    // Check if already has schema
    if cleaned.contains('.') {
        // Split and normalize
        let parts: Vec<&str> = cleaned.split('.').collect();
        if parts.len() >= 2 {
            let schema = parts[0].trim_matches(|c| c == '[' || c == ']');
            let table = parts[1].trim_matches(|c| c == '[' || c == ']');
            return format!("[{}].[{}]", schema, table);
        }
    }

    // No schema, add default
    let table = cleaned.trim_matches(|c| c == '[' || c == ']');
    format!("[{}].[{}]", default_schema, table)
}

/// Extract SELECT column expressions from the query
fn extract_select_columns(query: &str) -> Vec<String> {
    let mut columns = Vec::new();

    // Find the SELECT ... FROM section
    let upper = query.to_uppercase();
    let select_pos = upper.find("SELECT");
    let from_pos = upper.find("FROM");

    if let (Some(start), Some(end)) = (select_pos, from_pos) {
        let select_section = &query[start + 6..end].trim();

        // Split by comma, but handle nested parentheses
        let mut current = String::new();
        let mut paren_depth = 0;

        for ch in select_section.chars() {
            match ch {
                '(' => {
                    paren_depth += 1;
                    current.push(ch);
                }
                ')' => {
                    paren_depth -= 1;
                    current.push(ch);
                }
                ',' if paren_depth == 0 => {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        columns.push(trimmed);
                    }
                    current = String::new();
                }
                _ => current.push(ch),
            }
        }

        // Add the last column
        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() {
            columns.push(trimmed);
        }
    }

    columns
}

/// Parse a column expression and return (output_name, source_reference)
fn parse_column_expression(
    expr: &str,
    table_aliases: &[(String, String)],
    default_schema: &str,
) -> (String, Option<String>) {
    let trimmed = expr.trim();
    let upper = trimmed.to_uppercase();

    // Check for AS alias
    // Handle both "column AS alias" and "column alias" forms
    let (col_expr, alias) = if let Some(as_pos) = upper.rfind(" AS ") {
        let alias = trimmed[as_pos + 4..]
            .trim()
            .trim_matches(|c| c == '[' || c == ']');
        let col = trimmed[..as_pos].trim();
        (col.to_string(), Some(alias.to_string()))
    } else {
        // No AS keyword - check for space-separated alias (but not function calls)
        // Only treat last word as alias if it's a simple identifier
        (trimmed.to_string(), None)
    };

    // Determine the output column name
    let output_name = alias.unwrap_or_else(|| {
        // Extract the column name from the expression
        extract_column_name_from_expr(&col_expr)
    });

    // Determine the source reference (for simple column references)
    let source_ref = resolve_column_reference(&col_expr, table_aliases, default_schema);

    (output_name, source_ref)
}

/// Extract the column name from an expression like "[Id]", "t.[Name]", "COUNT(*)"
fn extract_column_name_from_expr(expr: &str) -> String {
    let trimmed = expr.trim();

    // If it's a function call (contains parentheses), return the expression as-is
    if trimmed.contains('(') {
        return trimmed.to_string();
    }

    // If it's a qualified reference like "t.[Name]" or "[dbo].[Products].[Name]"
    let parts: Vec<&str> = trimmed.split('.').collect();
    if let Some(last) = parts.last() {
        return last.trim_matches(|c| c == '[' || c == ']').to_string();
    }

    trimmed.trim_matches(|c| c == '[' || c == ']').to_string()
}

/// Resolve a column reference to its full [schema].[table].[column] form
/// Returns None for aggregate/function expressions
fn resolve_column_reference(
    expr: &str,
    table_aliases: &[(String, String)],
    _default_schema: &str,
) -> Option<String> {
    let trimmed = expr.trim();

    // If it's a function call (contains parentheses), no direct reference
    if trimmed.contains('(') {
        return None;
    }

    // Parse the column reference
    let parts: Vec<&str> = trimmed.split('.').collect();

    match parts.len() {
        1 => {
            // Just column name, try to resolve using first table alias
            let col_name = parts[0].trim_matches(|c| c == '[' || c == ']');
            if let Some((_, table_ref)) = table_aliases.first() {
                return Some(format!("{}.[{}]", table_ref, col_name));
            }
            None
        }
        2 => {
            // alias.column or schema.table
            let alias_or_schema = parts[0].trim_matches(|c| c == '[' || c == ']');
            let col_or_table = parts[1].trim_matches(|c| c == '[' || c == ']');

            // Try to find matching alias
            for (alias, table_ref) in table_aliases {
                if alias.eq_ignore_ascii_case(alias_or_schema) {
                    return Some(format!("{}.[{}]", table_ref, col_or_table));
                }
            }

            // If not found as alias, assume it's schema.table (unusual for column ref)
            None
        }
        3 => {
            // schema.table.column
            let schema = parts[0].trim_matches(|c| c == '[' || c == ']');
            let table = parts[1].trim_matches(|c| c == '[' || c == ']');
            let column = parts[2].trim_matches(|c| c == '[' || c == ']');
            Some(format!("[{}].[{}].[{}]", schema, table, column))
        }
        _ => None,
    }
}

/// Extract all column references from the entire query (SELECT, WHERE, ON, GROUP BY, etc.)
fn extract_all_column_references(
    query: &str,
    table_aliases: &[(String, String)],
    default_schema: &str,
) -> Vec<String> {
    let mut refs = Vec::new();

    // Find all column-like references: alias.column or [schema].[table].[column]
    // Pattern matches: word.word, [word].[word], word.[word], etc.
    let col_pattern = regex::Regex::new(r"(\[?\w+\]?)\.(\[?\w+\]?)(?:\.(\[?\w+\]?))?").unwrap();

    for cap in col_pattern.captures_iter(query) {
        let full_match = cap.get(0).map(|m| m.as_str()).unwrap_or("");

        // Skip if it looks like a function call argument position
        if full_match.contains("(") || full_match.contains(")") {
            continue;
        }

        // Try to resolve to full column reference
        if let Some(resolved) = resolve_column_reference(full_match, table_aliases, default_schema)
        {
            if !refs.contains(&resolved) {
                refs.push(resolved);
            }
        }
    }

    refs
}

/// Write view columns as SqlComputedColumn elements
fn write_view_columns<W: Write>(
    writer: &mut Writer<W>,
    view_full_name: &str,
    columns: &[ViewColumn],
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "Columns"));
    writer.write_event(Event::Start(rel))?;

    for col in columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", view_full_name, col.name);
        let mut elem = BytesStart::new("Element");
        elem.push_attribute(("Type", "SqlComputedColumn"));
        elem.push_attribute(("Name", col_full_name.as_str()));
        writer.write_event(Event::Start(elem))?;

        // Write ExpressionDependencies if this column has a source reference
        if let Some(source_ref) = &col.source_ref {
            let mut dep_rel = BytesStart::new("Relationship");
            dep_rel.push_attribute(("Name", "ExpressionDependencies"));
            writer.write_event(Event::Start(dep_rel))?;

            writer.write_event(Event::Start(BytesStart::new("Entry")))?;

            let mut refs = BytesStart::new("References");
            refs.push_attribute(("Name", source_ref.as_str()));
            writer.write_event(Event::Empty(refs))?;

            writer.write_event(Event::End(BytesEnd::new("Entry")))?;
            writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write QueryDependencies relationship
fn write_query_dependencies<W: Write>(
    writer: &mut Writer<W>,
    deps: &[String],
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "QueryDependencies"));
    writer.write_event(Event::Start(rel))?;

    for dep in deps {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let mut refs = BytesStart::new("References");
        refs.push_attribute(("Name", dep.as_str()));
        writer.write_event(Event::Empty(refs))?;

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
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

    // Extract parameters for both writing and dependency extraction
    let params = extract_procedure_parameters(&proc.definition);

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
    let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    let body_deps = extract_body_dependencies(&body, &full_name, &param_names);
    write_body_dependencies(writer, &body_deps)?;

    // Write Parameters relationship
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

            // Write default value if present
            if let Some(ref default_val) = param.default_value {
                write_script_property(writer, "DefaultExpressionScript", default_val)?;
            }

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

/// Represents an extracted function parameter with full details
#[derive(Debug)]
struct FunctionParameter {
    name: String,
    data_type: String,
    default_value: Option<String>,
}

/// Extract parameters from a CREATE FUNCTION definition
fn extract_function_parameters(definition: &str) -> Vec<FunctionParameter> {
    let mut params = Vec::new();

    // Find the function name and the parameters that follow
    let def_upper = definition.to_uppercase();
    let func_start = def_upper.find("CREATE FUNCTION");

    if func_start.is_none() {
        return params;
    }

    let after_create = &definition[func_start.unwrap()..];

    // Function parameters are in parentheses after the function name
    // Find opening paren after the function name
    if let Some(open_paren) = after_create.find('(') {
        // Find matching close paren - need to handle nested parens for types like DECIMAL(18,2)
        let rest = &after_create[open_paren + 1..];
        let mut paren_depth = 1;
        let mut close_pos = None;
        for (i, ch) in rest.char_indices() {
            match ch {
                '(' => paren_depth += 1,
                ')' => {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        close_pos = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }

        if let Some(close_paren) = close_pos {
            let param_section = &rest[..close_paren];

            // Extract parameters with full details - same regex as procedure parameters
            // but without OUTPUT since function parameters are always input
            let param_regex = regex::Regex::new(
                r"@(\w+)\s+([A-Za-z0-9_\(\),\s]+?)(?:\s*=\s*([^,@]+?))?(?:,|$|\s*\n)",
            )
            .unwrap();

            for cap in param_regex.captures_iter(param_section) {
                let name = cap
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let data_type = cap
                    .get(2)
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();
                let default_value = cap.get(3).map(|m| m.as_str().trim().to_string());

                if !name.is_empty() && !data_type.is_empty() {
                    // Clean up data type
                    let clean_type = clean_data_type(&data_type);
                    params.push(FunctionParameter {
                        name,
                        data_type: clean_type,
                        default_value,
                    });
                }
            }
        }
    }

    params
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

/// Represents a dependency extracted from a procedure/function body
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum BodyDependency {
    /// Reference to a built-in type (e.g., [int], [decimal])
    BuiltInType(String),
    /// Reference to a table or other object (e.g., [dbo].[Products])
    ObjectRef(String),
}

/// Extract body dependencies from a procedure/function body
/// This extracts dependencies in order of appearance:
/// 1. Built-in types from DECLARE statements
/// 2. Table references, columns, and parameters in the order they appear
fn extract_body_dependencies(
    body: &str,
    full_name: &str,
    params: &[String],
) -> Vec<BodyDependency> {
    use std::collections::HashSet;
    let mut deps = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Regex to match DECLARE @var type patterns for built-in types
    let declare_regex =
        regex::Regex::new(r"(?i)DECLARE\s+@\w+\s+([A-Za-z][A-Za-z0-9_]*(?:\s*\([^)]*\))?)")
            .unwrap();

    // Extract DECLARE type dependencies first (for scalar functions)
    for cap in declare_regex.captures_iter(body) {
        if let Some(type_match) = cap.get(1) {
            let type_str = type_match.as_str().trim();
            let base_type = if let Some(paren_pos) = type_str.find('(') {
                &type_str[..paren_pos]
            } else {
                type_str
            };
            let type_ref = format!("[{}]", base_type.to_lowercase());
            if !seen.contains(&type_ref) {
                seen.insert(type_ref.clone());
                deps.push(BodyDependency::BuiltInType(type_ref));
            }
        }
    }

    // First pass: collect all table references - both bracketed and unbracketed
    // Patterns: [schema].[table] or schema.table
    // But don't add them to deps yet - we'll process everything in order of appearance
    let mut table_refs: Vec<String> = Vec::new();

    // Match bracketed table refs: [schema].[table]
    let bracketed_table_regex = regex::Regex::new(r"\[([^\]]+)\]\s*\.\s*\[([^\]]+)\]").unwrap();
    for cap in bracketed_table_regex.captures_iter(body) {
        let schema = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        if !schema.starts_with('@') && !name.starts_with('@') {
            let table_ref = format!("[{}].[{}]", schema, name);
            if !table_refs.contains(&table_ref) {
                table_refs.push(table_ref);
            }
        }
    }

    // Match unbracketed table refs: schema.table (identifier.identifier not preceded by @)
    // This must be a word boundary followed by identifier.identifier
    let unbracketed_table_regex =
        regex::Regex::new(r"(?:^|[^@\w\]])([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)")
            .unwrap();
    for cap in unbracketed_table_regex.captures_iter(body) {
        let schema = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        // Skip if schema is a keyword (like FROM.something)
        if is_sql_keyword(&schema.to_uppercase()) {
            continue;
        }
        let table_ref = format!("[{}].[{}]", schema, name);
        if !table_refs.contains(&table_ref) {
            table_refs.push(table_ref);
        }
    }

    // Second pass: scan body sequentially for all references in order of appearance
    // This complex regex matches (in order of priority):
    // 1. @param - parameter references
    // 2. [a].[b].[c] - three-part bracketed reference
    // 3. [a].[b] - two-part bracketed reference
    // 4. [ident] - single bracketed identifier
    // 5. schema.table - unbracketed two-part reference
    // 6. ident - unbracketed identifier (column name)
    let token_regex = regex::Regex::new(
        r"(@([A-Za-z_]\w*))|(\[([^\]]+)\]\s*\.\s*\[([^\]]+)\]\s*\.\s*\[([^\]]+)\])|(\[([^\]]+)\]\s*\.\s*\[([^\]]+)\])|(\[([A-Za-z_][A-Za-z0-9_]*)\])|(?:^|[^@\w\]])([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)|(?:^|[^@\w\.\]])([A-Za-z_][A-Za-z0-9_]*)",
    )
    .unwrap();

    for cap in token_regex.captures_iter(body) {
        if cap.get(1).is_some() {
            // Parameter reference: @param
            let param_name = cap.get(2).map(|m| m.as_str()).unwrap_or("");

            // Check if this is a declared parameter (not a local variable)
            if params.iter().any(|p| {
                let p_name = p.trim_start_matches('@');
                p_name.eq_ignore_ascii_case(param_name)
            }) {
                let param_ref = format!("{}.[@{}]", full_name, param_name);
                if !seen.contains(&param_ref) {
                    seen.insert(param_ref.clone());
                    deps.push(BodyDependency::ObjectRef(param_ref));
                }
            }
        } else if cap.get(3).is_some() {
            // Three-part bracketed reference: [schema].[table].[column]
            let schema = cap.get(4).map(|m| m.as_str()).unwrap_or("");
            let table = cap.get(5).map(|m| m.as_str()).unwrap_or("");
            let column = cap.get(6).map(|m| m.as_str()).unwrap_or("");

            if !schema.starts_with('@') && !table.starts_with('@') {
                // First emit the table reference if not seen
                let table_ref = format!("[{}].[{}]", schema, table);
                if !seen.contains(&table_ref) {
                    seen.insert(table_ref.clone());
                    deps.push(BodyDependency::ObjectRef(table_ref));
                }

                // Then emit the column reference
                let col_ref = format!("[{}].[{}].[{}]", schema, table, column);
                if !seen.contains(&col_ref) {
                    seen.insert(col_ref.clone());
                    deps.push(BodyDependency::ObjectRef(col_ref));
                }
            }
        } else if cap.get(7).is_some() {
            // Two-part bracketed reference: [schema].[table]
            let schema = cap.get(8).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(9).map(|m| m.as_str()).unwrap_or("");

            if !schema.starts_with('@') && !name.starts_with('@') {
                let table_ref = format!("[{}].[{}]", schema, name);
                if !seen.contains(&table_ref) {
                    seen.insert(table_ref.clone());
                    deps.push(BodyDependency::ObjectRef(table_ref));
                }
            }
        } else if cap.get(10).is_some() {
            // Single bracketed identifier: [ident]
            let ident = cap.get(11).map(|m| m.as_str()).unwrap_or("");
            let upper_ident = ident.to_uppercase();

            // Skip SQL keywords (but allow column names that happen to match type names)
            if is_sql_keyword_not_column(&upper_ident) {
                continue;
            }

            // Skip if this is part of a table reference (schema or table name)
            let is_table_or_schema = table_refs.iter().any(|t| {
                t.ends_with(&format!("].[{}]", ident)) || t.starts_with(&format!("[{}].", ident))
            });

            // If not a table/schema, treat as unqualified column -> resolve against first table
            if !is_table_or_schema {
                if let Some(first_table) = table_refs.first() {
                    // First emit the table reference if not seen (DotNet orders table before columns)
                    if !seen.contains(first_table) {
                        seen.insert(first_table.clone());
                        deps.push(BodyDependency::ObjectRef(first_table.clone()));
                    }

                    // Then emit the column reference
                    let col_ref = format!("{}.[{}]", first_table, ident);
                    if !seen.contains(&col_ref) {
                        seen.insert(col_ref.clone());
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    }
                }
            }
        } else if cap.get(12).is_some() {
            // Unbracketed two-part reference: schema.table
            let schema = cap.get(12).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(13).map(|m| m.as_str()).unwrap_or("");

            // Skip if schema is a keyword
            if is_sql_keyword(&schema.to_uppercase()) {
                continue;
            }

            let table_ref = format!("[{}].[{}]", schema, name);
            if !seen.contains(&table_ref) {
                seen.insert(table_ref.clone());
                deps.push(BodyDependency::ObjectRef(table_ref));
            }
        } else if cap.get(14).is_some() {
            // Unbracketed single identifier: might be a column name
            let ident = cap.get(14).map(|m| m.as_str()).unwrap_or("");
            let upper_ident = ident.to_uppercase();

            // Skip SQL keywords
            if is_sql_keyword_not_column(&upper_ident) {
                continue;
            }

            // Skip if this is part of a table reference (schema or table name)
            let is_table_or_schema = table_refs.iter().any(|t| {
                // Check case-insensitive match for unbracketed identifiers
                let t_lower = t.to_lowercase();
                let ident_lower = ident.to_lowercase();
                t_lower.ends_with(&format!("].[{}]", ident_lower))
                    || t_lower.starts_with(&format!("[{}].", ident_lower))
            });

            // If not a table/schema, treat as unqualified column -> resolve against first table
            if !is_table_or_schema {
                if let Some(first_table) = table_refs.first() {
                    // First emit the table reference if not seen (DotNet orders table before columns)
                    if !seen.contains(first_table) {
                        seen.insert(first_table.clone());
                        deps.push(BodyDependency::ObjectRef(first_table.clone()));
                    }

                    // Then emit the column reference
                    let col_ref = format!("{}.[{}]", first_table, ident);
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

/// Check if a word is a SQL keyword (to filter out from column detection)
fn is_sql_keyword(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "FROM"
            | "WHERE"
            | "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "IS"
            | "IN"
            | "AS"
            | "ON"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "INNER"
            | "OUTER"
            | "CROSS"
            | "FULL"
            | "INSERT"
            | "INTO"
            | "VALUES"
            | "UPDATE"
            | "SET"
            | "DELETE"
            | "CREATE"
            | "ALTER"
            | "DROP"
            | "TABLE"
            | "VIEW"
            | "INDEX"
            | "PROCEDURE"
            | "FUNCTION"
            | "TRIGGER"
            | "BEGIN"
            | "END"
            | "IF"
            | "ELSE"
            | "WHILE"
            | "RETURN"
            | "DECLARE"
            | "INT"
            | "VARCHAR"
            | "NVARCHAR"
            | "CHAR"
            | "NCHAR"
            | "TEXT"
            | "NTEXT"
            | "BIT"
            | "TINYINT"
            | "SMALLINT"
            | "BIGINT"
            | "DECIMAL"
            | "NUMERIC"
            | "FLOAT"
            | "REAL"
            | "MONEY"
            | "SMALLMONEY"
            | "DATE"
            | "TIME"
            | "DATETIME"
            | "DATETIME2"
            | "SMALLDATETIME"
            | "DATETIMEOFFSET"
            | "UNIQUEIDENTIFIER"
            | "BINARY"
            | "VARBINARY"
            | "IMAGE"
            | "XML"
            | "SQL_VARIANT"
            | "TIMESTAMP"
            | "ROWVERSION"
            | "GEOGRAPHY"
            | "GEOMETRY"
            | "HIERARCHYID"
            | "PRIMARY"
            | "KEY"
            | "FOREIGN"
            | "REFERENCES"
            | "UNIQUE"
            | "CHECK"
            | "DEFAULT"
            | "CONSTRAINT"
            | "IDENTITY"
            | "NOCOUNT"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "ISNULL"
            | "COALESCE"
            | "CAST"
            | "CONVERT"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "EXEC"
            | "EXECUTE"
            | "GO"
            | "USE"
            | "DATABASE"
            | "SCHEMA"
            | "GRANT"
            | "REVOKE"
            | "DENY"
            | "ORDER"
            | "BY"
            | "GROUP"
            | "HAVING"
            | "DISTINCT"
            | "TOP"
            | "OFFSET"
            | "FETCH"
            | "NEXT"
            | "ROWS"
            | "ONLY"
            | "UNION"
            | "ALL"
            | "EXCEPT"
            | "INTERSECT"
            | "EXISTS"
            | "ANY"
            | "SOME"
            | "LIKE"
            | "BETWEEN"
            | "ASC"
            | "DESC"
            | "CLUSTERED"
            | "NONCLUSTERED"
            | "OUTPUT"
            | "SCOPE_IDENTITY"
    )
}

/// Check if a word is a SQL keyword that should be filtered from column detection in procedure bodies.
/// This is a more permissive filter than `is_sql_keyword` - it allows words that are commonly
/// used as column names (like TIMESTAMP, ACTION, ID, etc.) even though they're also SQL keywords/types.
fn is_sql_keyword_not_column(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "FROM"
            | "WHERE"
            | "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "IS"
            | "IN"
            | "AS"
            | "ON"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "INNER"
            | "OUTER"
            | "CROSS"
            | "FULL"
            | "INSERT"
            | "INTO"
            | "VALUES"
            | "UPDATE"
            | "SET"
            | "DELETE"
            | "CREATE"
            | "ALTER"
            | "DROP"
            | "TABLE"
            | "VIEW"
            | "INDEX"
            | "PROCEDURE"
            | "FUNCTION"
            | "TRIGGER"
            | "BEGIN"
            | "END"
            | "IF"
            | "ELSE"
            | "WHILE"
            | "RETURN"
            | "DECLARE"
            | "PRIMARY"
            | "KEY"
            | "FOREIGN"
            | "REFERENCES"
            | "UNIQUE"
            | "CHECK"
            | "DEFAULT"
            | "CONSTRAINT"
            | "IDENTITY"
            | "NOCOUNT"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "ISNULL"
            | "COALESCE"
            | "CAST"
            | "CONVERT"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "EXEC"
            | "EXECUTE"
            | "GO"
            | "USE"
            | "DATABASE"
            | "SCHEMA"
            | "GRANT"
            | "REVOKE"
            | "DENY"
            | "ORDER"
            | "BY"
            | "GROUP"
            | "HAVING"
            | "DISTINCT"
            | "TOP"
            | "OFFSET"
            | "FETCH"
            | "NEXT"
            | "ROWS"
            | "ONLY"
            | "UNION"
            | "ALL"
            | "EXCEPT"
            | "INTERSECT"
            | "EXISTS"
            | "ANY"
            | "SOME"
            | "LIKE"
            | "BETWEEN"
            | "ASC"
            | "DESC"
            | "CLUSTERED"
            | "NONCLUSTERED"
            | "OUTPUT"
            | "SCOPE_IDENTITY"
            // Core data types that are rarely used as column names
            | "INT"
            | "INTEGER"
            | "VARCHAR"
            | "NVARCHAR"
            | "CHAR"
            | "NCHAR"
            | "BIT"
            | "TINYINT"
            | "SMALLINT"
            | "BIGINT"
            | "DECIMAL"
            | "NUMERIC"
            | "FLOAT"
            | "REAL"
            | "MONEY"
            | "SMALLMONEY"
            | "DATETIME"
            | "DATETIME2"
            | "SMALLDATETIME"
            | "DATETIMEOFFSET"
            | "UNIQUEIDENTIFIER"
            | "BINARY"
            | "VARBINARY"
            | "XML"
            | "SQL_VARIANT"
            | "ROWVERSION"
            | "GEOGRAPHY"
            | "GEOMETRY"
            | "HIERARCHYID"
            | "NTEXT"
    )
    // Intentionally excludes: TIMESTAMP, ACTION, ID, TEXT, IMAGE, DATE, TIME, etc.
    // as these are commonly used as column names
}

/// Extract column references from a CHECK constraint expression.
///
/// CHECK expressions reference columns by their unqualified names (e.g., `[Price] >= 0`).
/// This function extracts those column names and returns them as fully-qualified references
/// in the format `[schema].[table].[column]`.
///
/// DotNet emits these as the `CheckExpressionDependencies` relationship.
fn extract_check_expression_columns(
    expression: &str,
    table_schema: &str,
    table_name: &str,
) -> Vec<String> {
    extract_expression_column_references(expression, table_schema, table_name)
}

/// Extract column references from a computed column expression.
///
/// Computed column expressions reference columns by their unqualified names
/// (e.g., `[Quantity] * [UnitPrice]`). This function extracts those column names
/// and returns them as fully-qualified references in the format `[schema].[table].[column]`.
///
/// DotNet emits these as the `ExpressionDependencies` relationship.
fn extract_computed_expression_columns(
    expression: &str,
    table_schema: &str,
    table_name: &str,
) -> Vec<String> {
    extract_expression_column_references(expression, table_schema, table_name)
}

/// Extract column references from an expression.
///
/// Expressions reference columns by their unqualified names (e.g., `[ColumnName]`).
/// This function extracts those column names and returns them as fully-qualified references
/// in the format `[schema].[table].[column]`.
///
/// Used by both CHECK constraints and computed columns.
fn extract_expression_column_references(
    expression: &str,
    table_schema: &str,
    table_name: &str,
) -> Vec<String> {
    use std::collections::HashSet;
    let mut columns = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Match bracketed identifiers: [ColumnName]
    // These are column references in the expression
    let column_regex = regex::Regex::new(r"\[([A-Za-z_][A-Za-z0-9_]*)\]").unwrap();

    for cap in column_regex.captures_iter(expression) {
        if let Some(col_match) = cap.get(1) {
            let col_name = col_match.as_str();
            let upper_name = col_name.to_uppercase();

            // Skip SQL keywords
            if is_sql_keyword(&upper_name) {
                continue;
            }

            // Build fully-qualified column reference
            let col_ref = format!("[{}].[{}].[{}]", table_schema, table_name, col_name);

            // Only add each column once, but preserve order of first appearance
            if !seen.contains(&col_ref) {
                seen.insert(col_ref.clone());
                columns.push(col_ref);
            }
        }
    }

    columns
}

/// Write BodyDependencies relationship for procedures and functions
fn write_body_dependencies<W: Write>(
    writer: &mut Writer<W>,
    deps: &[BodyDependency],
) -> anyhow::Result<()> {
    if deps.is_empty() {
        return Ok(());
    }

    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "BodyDependencies"));
    writer.write_event(Event::Start(rel))?;

    for dep in deps {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        match dep {
            BodyDependency::BuiltInType(type_ref) => {
                let mut refs = BytesStart::new("References");
                refs.push_attribute(("ExternalSource", "BuiltIns"));
                refs.push_attribute(("Name", type_ref.as_str()));
                writer.write_event(Event::Empty(refs))?;
            }
            BodyDependency::ObjectRef(obj_ref) => {
                let mut refs = BytesStart::new("References");
                refs.push_attribute(("Name", obj_ref.as_str()));
                writer.write_event(Event::Empty(refs))?;
            }
        }

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Extract just the body after AS from a procedure definition
fn extract_procedure_body_only(definition: &str) -> String {
    // Find the standalone AS keyword that separates header from body
    // This AS must be followed by whitespace/newline and then BEGIN or a statement
    // We look for AS that's at the end of a line (followed by newline) or followed by BEGIN
    let as_pos = find_body_separator_as(definition);
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

/// Find the AS keyword that separates procedure header from body
/// This is the AS that's:
/// 1. At the end of a line (AS\n or AS\r\n) followed by BEGIN or other body content
/// 2. Or followed directly by BEGIN (AS BEGIN)
/// We avoid matching "AS alias" patterns in SELECT statements
fn find_body_separator_as(s: &str) -> Option<usize> {
    let upper = s.to_uppercase();
    let chars: Vec<char> = upper.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Look for AS preceded by whitespace/newline
        if i + 2 <= chars.len() && chars[i] == 'A' && chars[i + 1] == 'S' {
            let prev_ok = i == 0 || chars[i - 1].is_whitespace();
            if !prev_ok {
                i += 1;
                continue;
            }

            // Check what comes after AS
            if i + 2 < chars.len() {
                let after_as = &upper[i + 2..];
                let after_as_trimmed = after_as.trim_start();

                // AS must be followed by:
                // 1. Newline only (AS at end of line, body on next line)
                // 2. BEGIN (AS BEGIN or AS\nBEGIN)
                // 3. RETURN (for functions: AS RETURN ...)
                // 4. SET, SELECT, IF, WHILE, etc. (direct statement after AS)

                // Check if followed by newline then BEGIN/body
                if chars[i + 2] == '\n' || chars[i + 2] == '\r' {
                    // AS is at end of line - this is likely the body separator
                    return Some(i);
                }

                // Check if followed by whitespace then BEGIN/RETURN/statement keyword
                if after_as_trimmed.starts_with("BEGIN")
                    || after_as_trimmed.starts_with("RETURN")
                    || after_as_trimmed.starts_with("SET")
                    || after_as_trimmed.starts_with("SELECT")
                    || after_as_trimmed.starts_with("IF")
                    || after_as_trimmed.starts_with("WHILE")
                    || after_as_trimmed.starts_with("DECLARE")
                    || after_as_trimmed.starts_with("WITH")
                    || after_as_trimmed.starts_with("INSERT")
                    || after_as_trimmed.starts_with("UPDATE")
                    || after_as_trimmed.starts_with("DELETE")
                    || after_as_trimmed.starts_with("EXEC")
                {
                    return Some(i);
                }
            } else if i + 2 == chars.len() {
                // AS is at the very end - return it
                return Some(i);
            }
        }
        i += 1;
    }
    None
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

    // Extract function body for dependency analysis
    let body = extract_function_body(&func.definition);
    let header = extract_function_header(&func.definition);

    // Extract function parameters for dependency analysis
    let func_params = extract_function_parameters(&func.definition);
    let param_names: Vec<String> = func_params.iter().map(|p| p.name.clone()).collect();

    // Extract and write BodyDependencies
    let body_deps = extract_body_dependencies(&body, &full_name, &param_names);
    write_body_dependencies(writer, &body_deps)?;

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
fn write_function_return_type<W: Write>(
    writer: &mut Writer<W>,
    return_type: &str,
) -> anyhow::Result<()> {
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

    if let Some(fill_factor) = index.fill_factor {
        write_property(writer, "FillFactor", &fill_factor.to_string())?;
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

fn write_fulltext_index<W: Write>(
    writer: &mut Writer<W>,
    fulltext: &FullTextIndexElement,
) -> anyhow::Result<()> {
    // Full-text index name format: [schema].[table] (same as table name)
    let full_name = format!("[{}].[{}]", fulltext.table_schema, fulltext.table_name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlFullTextIndex"));
    elem.push_attribute(("Name", full_name.as_str()));
    // Disambiguator needed since fulltext index shares name with table
    if let Some(disambiguator) = fulltext.disambiguator {
        elem.push_attribute(("Disambiguator", disambiguator.to_string().as_str()));
    }
    writer.write_event(Event::Start(elem))?;

    // Reference to full-text catalog if specified
    if let Some(catalog) = &fulltext.catalog {
        let catalog_ref = format!("[{}]", catalog);
        write_relationship(writer, "Catalog", &[&catalog_ref])?;
    }

    // Write Columns for full-text columns
    let table_ref = format!("[{}].[{}]", fulltext.table_schema, fulltext.table_name);
    if !fulltext.columns.is_empty() {
        write_fulltext_column_specifications(writer, fulltext, &table_ref)?;
    }

    // Reference to table (IndexedObject)
    write_relationship(writer, "IndexedObject", &[&table_ref])?;

    // Reference to the unique key index (KeyName)
    // Key reference format: [schema].[constraint_name]
    let key_index_ref = format!("[{}].[{}]", fulltext.table_schema, fulltext.key_index);
    write_relationship(writer, "KeyName", &[&key_index_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_fulltext_column_specifications<W: Write>(
    writer: &mut Writer<W>,
    fulltext: &FullTextIndexElement,
    table_ref: &str,
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "Columns"));
    writer.write_event(Event::Start(rel))?;

    for col in fulltext.columns.iter() {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // DotNet uses anonymous elements (no Name attribute) for column specifiers
        let mut elem = BytesStart::new("Element");
        elem.push_attribute(("Type", "SqlFullTextIndexColumnSpecifier"));
        writer.write_event(Event::Start(elem))?;

        // Add LanguageId property if specified
        if let Some(lang_id) = col.language_id {
            write_property(writer, "LanguageId", &lang_id.to_string())?;
        }

        // Reference to the column
        let col_ref = format!("{}.[{}]", table_ref, col.name);
        write_relationship(writer, "Column", &[&col_ref])?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

fn write_fulltext_catalog<W: Write>(
    writer: &mut Writer<W>,
    catalog: &FullTextCatalogElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}]", catalog.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlFullTextCatalog"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Add IsDefault property if this is the default catalog
    if catalog.is_default {
        write_property(writer, "IsDefault", "True")?;
    }

    // Fulltext catalogs have an Authorizer relationship (defaults to dbo)
    write_authorizer_relationship(writer, "dbo")?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_constraint<W: Write>(
    writer: &mut Writer<W>,
    constraint: &ConstraintElement,
) -> anyhow::Result<()> {
    // DotNet uses two-part names for constraints: [schema].[constraint_name]
    // But inline constraints (without CONSTRAINT keyword) have no Name attribute
    let full_name = format!("[{}].[{}]", constraint.table_schema, constraint.name);

    let type_name = match constraint.constraint_type {
        ConstraintType::PrimaryKey => "SqlPrimaryKeyConstraint",
        ConstraintType::ForeignKey => "SqlForeignKeyConstraint",
        ConstraintType::Unique => "SqlUniqueConstraint",
        ConstraintType::Check => "SqlCheckConstraint",
        ConstraintType::Default => "SqlDefaultConstraint",
    };

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", type_name));
    // Inline constraints have no Name attribute - only named constraints do
    if !constraint.is_inline {
        elem.push_attribute(("Name", full_name.as_str()));
    }
    writer.write_event(Event::Start(elem))?;

    // Write IsClustered property for primary keys and unique constraints
    // DotNet only emits IsClustered when it differs from the default:
    // - Primary Key: default is CLUSTERED, so only emit when NONCLUSTERED (False)
    // - Unique: default is NONCLUSTERED, so only emit when CLUSTERED (True)
    if let Some(is_clustered) = constraint.is_clustered {
        match constraint.constraint_type {
            ConstraintType::PrimaryKey if !is_clustered => {
                // PK is nonclustered (non-default), emit IsClustered=False
                write_property(writer, "IsClustered", "False")?;
            }
            ConstraintType::Unique if is_clustered => {
                // Unique is clustered (non-default), emit IsClustered=True
                write_property(writer, "IsClustered", "True")?;
            }
            _ => {
                // Default values - don't emit
            }
        }
    }

    // Reference to table
    let table_ref = format!("[{}].[{}]", constraint.table_schema, constraint.table_name);

    // Handle CHECK constraints with special ordering:
    // DotNet order for CHECK: CheckExpressionScript, CheckExpressionDependencies, DefiningTable
    if constraint.constraint_type == ConstraintType::Check {
        // Write CheckExpressionScript property first
        if let Some(ref definition) = constraint.definition {
            write_script_property(writer, "CheckExpressionScript", definition)?;

            // Extract and write CheckExpressionDependencies relationship
            let col_refs = extract_check_expression_columns(
                definition,
                &constraint.table_schema,
                &constraint.table_name,
            );
            if !col_refs.is_empty() {
                let col_refs_str: Vec<&str> = col_refs.iter().map(|s| s.as_str()).collect();
                write_relationship(writer, "CheckExpressionDependencies", &col_refs_str)?;
            }
        }

        // DefiningTable comes after CheckExpressionDependencies
        write_relationship(writer, "DefiningTable", &[&table_ref])?;
    } else {
        // Write column relationships and DefiningTable based on constraint type
        // DotNet ordering for foreign keys: Columns, DefiningTable, ForeignColumns, ForeignTable
        // DotNet ordering for PK/Unique: DefiningTable, ColumnSpecifications
        if !constraint.columns.is_empty() {
            match constraint.constraint_type {
                ConstraintType::PrimaryKey | ConstraintType::Unique => {
                    // PK/Unique: DefiningTable first, then ColumnSpecifications
                    write_relationship(writer, "DefiningTable", &[&table_ref])?;

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
                    // Foreign keys: Columns, DefiningTable, ForeignColumns, ForeignTable (DotNet order)
                    let column_refs: Vec<String> = constraint
                        .columns
                        .iter()
                        .map(|c| format!("{}.[{}]", table_ref, c.name))
                        .collect();
                    let column_refs_str: Vec<&str> =
                        column_refs.iter().map(|s| s.as_str()).collect();
                    write_relationship(writer, "Columns", &column_refs_str)?;

                    write_relationship(writer, "DefiningTable", &[&table_ref])?;

                    // Add ForeignColumns and ForeignTable relationships
                    if let Some(ref foreign_table) = constraint.referenced_table {
                        // ForeignColumns comes before ForeignTable in DotNet
                        if let Some(ref foreign_columns) = constraint.referenced_columns {
                            if !foreign_columns.is_empty() {
                                let foreign_col_refs: Vec<String> = foreign_columns
                                    .iter()
                                    .map(|c| format!("{}.[{}]", foreign_table, c))
                                    .collect();
                                let foreign_col_refs_str: Vec<&str> =
                                    foreign_col_refs.iter().map(|s| s.as_str()).collect();
                                write_relationship(
                                    writer,
                                    "ForeignColumns",
                                    &foreign_col_refs_str,
                                )?;
                            }
                        }
                        write_relationship(writer, "ForeignTable", &[foreign_table])?;
                    }
                }
                _ => {
                    // Other constraint types: DefiningTable only
                    write_relationship(writer, "DefiningTable", &[&table_ref])?;
                }
            }
        } else {
            // No columns - still write DefiningTable for constraints that need it
            write_relationship(writer, "DefiningTable", &[&table_ref])?;
        }
    }

    // Default constraint expression - handled separately since it has different structure
    if constraint.constraint_type == ConstraintType::Default {
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

    // Write annotation at the end of the constraint element
    // - Inline constraints: <Annotation Type="SqlInlineConstraintAnnotation" Disambiguator="X" />
    // - Named constraints: <AttachedAnnotation Disambiguator="X" /> (referencing table's disambiguator)
    if let Some(disambiguator) = constraint.inline_constraint_disambiguator {
        if constraint.is_inline {
            // Inline constraint gets its own SqlInlineConstraintAnnotation
            let mut annotation = BytesStart::new("Annotation");
            annotation.push_attribute(("Type", "SqlInlineConstraintAnnotation"));
            annotation.push_attribute(("Disambiguator", disambiguator.to_string().as_str()));
            writer.write_event(Event::Empty(annotation))?;
        } else {
            // Named constraint references the table's disambiguator via AttachedAnnotation
            let mut annotation = BytesStart::new("AttachedAnnotation");
            annotation.push_attribute(("Disambiguator", disambiguator.to_string().as_str()));
            writer.write_event(Event::Empty(annotation))?;
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

/// Normalize script content for consistent output.
///
/// DotNet DacFx normalizes line endings in script content to LF (Unix-style).
/// This ensures consistent output regardless of the source file's line endings.
fn normalize_script_content(script: &str) -> String {
    // Convert CRLF to LF for consistent line endings
    script.replace("\r\n", "\n")
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

    // Normalize line endings before writing
    let normalized_script = normalize_script_content(script);

    // Write Value element with CDATA content
    writer.write_event(Event::Start(BytesStart::new("Value")))?;
    writer.write_event(Event::CData(BytesCData::new(&normalized_script)))?;
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

/// Write TypeSpecifier relationship for sequences referencing a built-in type
/// Format: <Relationship Name="TypeSpecifier"><Entry><Element Type="SqlTypeSpecifier">
///           <Relationship Name="Type"><Entry><References ExternalSource="BuiltIns" Name="[int]"/></Entry></Relationship>
///         </Element></Entry></Relationship>
fn write_type_specifier_builtin<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
) -> anyhow::Result<()> {
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "TypeSpecifier"));
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
    refs.push_attribute(("Name", type_name));
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
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

    // Properties in DotNet order: IsCycling, HasNoMaxValue, HasNoMinValue, MinValue, MaxValue, Increment, StartValue
    if seq.is_cycling {
        write_property(writer, "IsCycling", "True")?;
    }

    // HasNoMaxValue and HasNoMinValue
    let has_no_max = seq.has_no_max_value || seq.max_value.is_none();
    let has_no_min = seq.has_no_min_value || seq.min_value.is_none();
    write_property(
        writer,
        "HasNoMaxValue",
        if has_no_max { "True" } else { "False" },
    )?;
    write_property(
        writer,
        "HasNoMinValue",
        if has_no_min { "True" } else { "False" },
    )?;

    // MinValue and MaxValue
    if let Some(min) = seq.min_value {
        write_property(writer, "MinValue", &min.to_string())?;
    }
    if let Some(max) = seq.max_value {
        write_property(writer, "MaxValue", &max.to_string())?;
    }

    // Increment
    if let Some(inc) = seq.increment_value {
        write_property(writer, "Increment", &inc.to_string())?;
    }

    // StartValue
    if let Some(start) = seq.start_value {
        write_property(writer, "StartValue", &start.to_string())?;
    }

    // Relationship to schema
    write_schema_relationship(writer, &seq.schema)?;

    // TypeSpecifier relationship for data type
    if let Some(ref data_type) = seq.data_type {
        let type_name = format!("[{}]", data_type.to_lowercase());
        write_type_specifier_builtin(writer, &type_name)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write SqlUserDefinedDataType element for scalar types (alias types)
/// e.g., CREATE TYPE [dbo].[PhoneNumber] FROM VARCHAR(20) NOT NULL
fn write_scalar_type<W: Write>(
    writer: &mut Writer<W>,
    scalar: &ScalarTypeElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", scalar.schema, scalar.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlUserDefinedDataType"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Properties - IsNullable only if explicitly false (NOT NULL)
    if !scalar.is_nullable {
        write_property(writer, "IsNullable", "False")?;
    }

    // Scale (appears before Precision in DotNet output for decimal types)
    if let Some(scale) = scalar.scale {
        write_property(writer, "Scale", &scale.to_string())?;
    }

    // Precision for decimal types
    if let Some(precision) = scalar.precision {
        write_property(writer, "Precision", &precision.to_string())?;
    }

    // Length for string types
    if let Some(length) = scalar.length {
        write_property(writer, "Length", &length.to_string())?;
    }

    // Relationship to schema
    write_schema_relationship(writer, &scalar.schema)?;

    // Relationship to base type (Type relationship points to built-in type)
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "Type"));
    writer.write_event(Event::Start(rel))?;

    let entry = BytesStart::new("Entry");
    writer.write_event(Event::Start(entry.clone()))?;

    let mut refs = BytesStart::new("References");
    refs.push_attribute(("ExternalSource", "BuiltIns"));
    refs.push_attribute(("Name", format!("[{}]", scalar.base_type).as_str()));
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

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

    // Write constraints (PRIMARY KEY, UNIQUE, CHECK, INDEX)
    for (idx, constraint) in udt.constraints.iter().enumerate() {
        write_table_type_constraint(writer, constraint, &full_name, idx, &udt.columns)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write a table type constraint (PrimaryKey, Unique, Check, Index)
fn write_table_type_constraint<W: Write>(
    writer: &mut Writer<W>,
    constraint: &TableTypeConstraint,
    type_name: &str,
    idx: usize,
    columns: &[TableTypeColumnElement],
) -> anyhow::Result<()> {
    match constraint {
        TableTypeConstraint::PrimaryKey {
            columns: pk_cols,
            is_clustered,
        } => {
            write_table_type_pk_constraint(writer, type_name, pk_cols, *is_clustered, columns)?;
        }
        TableTypeConstraint::Unique {
            columns: uq_cols,
            is_clustered,
        } => {
            write_table_type_unique_constraint(
                writer,
                type_name,
                uq_cols,
                *is_clustered,
                idx,
                columns,
            )?;
        }
        TableTypeConstraint::Check { expression } => {
            write_table_type_check_constraint(writer, type_name, expression, idx)?;
        }
        TableTypeConstraint::Index {
            name,
            columns: idx_cols,
            is_unique,
            is_clustered,
        } => {
            write_table_type_index(writer, type_name, name, idx_cols, *is_unique, *is_clustered)?;
        }
    }
    Ok(())
}

/// Write SqlTableTypePrimaryKeyConstraint element
fn write_table_type_pk_constraint<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    pk_columns: &[ConstraintColumn],
    is_clustered: bool,
    all_columns: &[TableTypeColumnElement],
) -> anyhow::Result<()> {
    // Relationship for PrimaryKey
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "PrimaryKey"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTableTypePrimaryKeyConstraint"));
    writer.write_event(Event::Start(elem))?;

    // IsClustered property
    if is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    // ColumnSpecifications relationship
    if !pk_columns.is_empty() {
        let mut col_rel = BytesStart::new("Relationship");
        col_rel.push_attribute(("Name", "ColumnSpecifications"));
        writer.write_event(Event::Start(col_rel))?;

        for pk_col in pk_columns {
            let is_descending = pk_col.sort_direction == SortDirection::Descending;
            write_table_type_indexed_column_spec(
                writer,
                type_name,
                &pk_col.name,
                is_descending,
                all_columns,
            )?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write SqlTableTypeUniqueConstraint element
fn write_table_type_unique_constraint<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    uq_columns: &[ConstraintColumn],
    is_clustered: bool,
    _idx: usize,
    all_columns: &[TableTypeColumnElement],
) -> anyhow::Result<()> {
    // Relationship for UniqueConstraints
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "UniqueConstraints"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTableTypeUniqueConstraint"));
    writer.write_event(Event::Start(elem))?;

    // IsClustered property
    if is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    // ColumnSpecifications relationship
    if !uq_columns.is_empty() {
        let mut col_rel = BytesStart::new("Relationship");
        col_rel.push_attribute(("Name", "ColumnSpecifications"));
        writer.write_event(Event::Start(col_rel))?;

        for uq_col in uq_columns {
            let is_descending = uq_col.sort_direction == SortDirection::Descending;
            write_table_type_indexed_column_spec(
                writer,
                type_name,
                &uq_col.name,
                is_descending,
                all_columns,
            )?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write SqlTableTypeCheckConstraint element
fn write_table_type_check_constraint<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    expression: &str,
    idx: usize,
) -> anyhow::Result<()> {
    // Relationship for CheckConstraints
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "CheckConstraints"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Generate a disambiguator for unnamed check constraints
    let disambiguator = format!("{}_CK{}", type_name, idx);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTableTypeCheckConstraint"));
    elem.push_attribute(("Disambiguator", disambiguator.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Expression property
    write_script_property(writer, "Expression", expression)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write table type index element
fn write_table_type_index<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    name: &str,
    idx_columns: &[String],
    is_unique: bool,
    is_clustered: bool,
) -> anyhow::Result<()> {
    // Relationship for Indexes
    let mut rel = BytesStart::new("Relationship");
    rel.push_attribute(("Name", "Indexes"));
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let idx_name = format!("{}.[{}]", type_name, name);
    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTableTypeIndex"));
    elem.push_attribute(("Name", idx_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Properties
    if is_unique {
        write_property(writer, "IsUnique", "True")?;
    }
    if is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    // Columns relationship
    if !idx_columns.is_empty() {
        let mut col_rel = BytesStart::new("Relationship");
        col_rel.push_attribute(("Name", "Columns"));
        writer.write_event(Event::Start(col_rel))?;

        for col_name in idx_columns {
            writer.write_event(Event::Start(BytesStart::new("Entry")))?;
            let col_ref = format!("{}.[{}]", type_name, col_name);
            let mut refs = BytesStart::new("References");
            refs.push_attribute(("Name", col_ref.as_str()));
            writer.write_event(Event::Empty(refs))?;
            writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write SqlTableTypeIndexedColumnSpecification element
fn write_table_type_indexed_column_spec<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    column_name: &str,
    is_descending: bool,
    _all_columns: &[TableTypeColumnElement],
) -> anyhow::Result<()> {
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlTableTypeIndexedColumnSpecification"));
    writer.write_event(Event::Start(elem))?;

    // IsAscending property (true by default, false if descending)
    if is_descending {
        write_property(writer, "IsAscending", "False")?;
    }

    // Column relationship
    let col_ref = format!("{}.[{}]", type_name, column_name);
    write_relationship(writer, "Column", &[&col_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write a DML trigger element to model.xml
/// DotNet format:
/// - Properties: IsInsertTrigger, IsUpdateTrigger, IsDeleteTrigger, SqlTriggerType, BodyScript, IsAnsiNullsOn
/// - Relationships: Parent (the table/view), no Schema relationship
fn write_trigger<W: Write>(writer: &mut Writer<W>, trigger: &TriggerElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", trigger.schema, trigger.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlDmlTrigger"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write properties in DotNet order:
    // 1. IsInsertTrigger (only if true)
    if trigger.is_insert_trigger {
        write_property(writer, "IsInsertTrigger", "True")?;
    }

    // 2. IsUpdateTrigger (only if true)
    if trigger.is_update_trigger {
        write_property(writer, "IsUpdateTrigger", "True")?;
    }

    // 3. IsDeleteTrigger (only if true)
    if trigger.is_delete_trigger {
        write_property(writer, "IsDeleteTrigger", "True")?;
    }

    // 4. SqlTriggerType: 2 = AFTER/FOR, 3 = INSTEAD OF
    write_property(writer, "SqlTriggerType", &trigger.trigger_type.to_string())?;

    // 5. BodyScript - extract just the trigger body (after AS)
    let body_script = extract_trigger_body(&trigger.definition);
    write_script_property(writer, "BodyScript", &body_script)?;

    // 6. IsAnsiNullsOn - always True for now (matches typical SQL Server defaults)
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Write Parent relationship (the table or view the trigger is on)
    let parent_ref = format!("[{}].[{}]", trigger.parent_schema, trigger.parent_name);
    write_relationship(writer, "Parent", &[&parent_ref])?;

    // Note: DotNet does NOT emit a Schema relationship for triggers

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Extract the trigger body (everything after AS keyword) from the full trigger definition
fn extract_trigger_body(definition: &str) -> String {
    // Find the AS keyword that starts the trigger body
    // The pattern is: CREATE TRIGGER ... ON ... (FOR|AFTER|INSTEAD OF) ... AS <body>
    // We need to find AS that's after the trigger events (INSERT/UPDATE/DELETE)

    let upper = definition.to_uppercase();

    // Find position of trigger action keywords to ensure we find AS after them
    let action_pos = upper
        .find("INSTEAD OF")
        .or_else(|| upper.find("AFTER"))
        .or_else(|| upper.find(" FOR ")) // Use " FOR " to avoid matching substrings
        .unwrap_or(0);

    // Find AS after the trigger action
    if let Some(as_pos) = upper[action_pos..].find("\nAS\n") {
        let actual_pos = action_pos + as_pos + 4; // Skip past "\nAS\n"
        return definition[actual_pos..].trim().to_string();
    }

    if let Some(as_pos) = upper[action_pos..].find("\nAS ") {
        let actual_pos = action_pos + as_pos + 4; // Skip past "\nAS "
        return definition[actual_pos..].trim().to_string();
    }

    if let Some(as_pos) = upper[action_pos..].find(" AS\n") {
        let actual_pos = action_pos + as_pos + 4; // Skip past " AS\n"
        return definition[actual_pos..].trim().to_string();
    }

    if let Some(as_pos) = upper[action_pos..].find("\rAS\r") {
        let actual_pos = action_pos + as_pos + 4;
        return definition[actual_pos..].trim().to_string();
    }

    // Try a more flexible pattern - AS followed by whitespace
    let re = regex::Regex::new(r"(?i)\bAS\s+").ok();
    if let Some(re) = re {
        if let Some(m) = re.find(&definition[action_pos..]) {
            let actual_pos = action_pos + m.end();
            return definition[actual_pos..].trim().to_string();
        }
    }

    // Fallback - return entire definition if we can't parse it
    definition.to_string()
}

fn write_raw<W: Write>(writer: &mut Writer<W>, raw: &RawElement) -> anyhow::Result<()> {
    // Handle SqlView specially to get full property/relationship support
    if raw.sql_type == "SqlView" {
        return write_raw_view(writer, raw);
    }

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

/// Write a view from a RawElement (for views parsed via fallback)
/// Mirrors the write_view function but works with raw definition text
fn write_raw_view<W: Write>(writer: &mut Writer<W>, raw: &RawElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", raw.schema, raw.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlView"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Extract view options from raw SQL text
    let upper = raw.definition.to_uppercase();

    // WITH SCHEMABINDING appears before AS in the view definition
    let is_schema_bound = upper.contains("WITH SCHEMABINDING")
        || upper.contains("WITH SCHEMABINDING,")
        || upper.contains(", SCHEMABINDING")
        || upper.contains(",SCHEMABINDING");

    // WITH CHECK OPTION appears at the end of the view definition
    let is_with_check_option = upper.contains("WITH CHECK OPTION");

    // VIEW_METADATA appears in WITH clause before AS
    let is_metadata_reported = upper.contains("VIEW_METADATA");

    // Write properties in DotNet order:
    // 1. IsSchemaBound (if true)
    if is_schema_bound {
        write_property(writer, "IsSchemaBound", "True")?;
    }

    // 2. IsMetadataReported (if true)
    if is_metadata_reported {
        write_property(writer, "IsMetadataReported", "True")?;
    }

    // 3. QueryScript
    let query_script = extract_view_query(&raw.definition);
    write_script_property(writer, "QueryScript", &query_script)?;

    // 4. IsWithCheckOption (if true)
    if is_with_check_option {
        write_property(writer, "IsWithCheckOption", "True")?;
    }

    // 5. IsAnsiNullsOn - always emit for views (current DotNet behavior)
    // Modern .NET DacFx emits this property for all views
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Only emit Columns and QueryDependencies for schema-bound or with-check-option views
    if is_schema_bound || is_with_check_option {
        // Extract view columns and dependencies from the query
        let (columns, query_deps) = extract_view_columns_and_deps(&query_script, &raw.schema);

        // 6. Write Columns relationship with SqlComputedColumn elements
        if !columns.is_empty() {
            write_view_columns(writer, &full_name, &columns)?;
        }

        // 7. Write QueryDependencies relationship
        if !query_deps.is_empty() {
            write_query_dependencies(writer, &query_deps)?;
        }
    }

    // 8. Schema relationship
    write_schema_relationship(writer, &raw.schema)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write an extended property element
/// Format:
/// ```xml
/// <Element Type="SqlExtendedProperty" Name="[dbo].[Table].[MS_Description]">
///   <Property Name="Value">
///     <Value><![CDATA[Description text]]></Value>
///   </Property>
///   <Relationship Name="Host">
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

    // Write Value property with CDATA (SqlScriptProperty format)
    // The value must be wrapped with N'...' for proper SQL string literal escaping
    // Any single quotes in the value must be doubled for SQL escaping
    let escaped_value = ext_prop.property_value.replace('\'', "''");
    let quoted_value = format!("N'{}'", escaped_value);
    write_script_property(writer, "Value", &quoted_value)?;

    // Write Host relationship pointing to the target object (table or column)
    let extends_ref = ext_prop.extends_object_ref();
    write_relationship(writer, "Host", &[&extends_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}
