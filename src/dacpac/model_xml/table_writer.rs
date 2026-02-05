//! Table and column XML writing utilities for model.xml generation.
//!
//! This module provides functions for writing table and column elements
//! to the model.xml output. It handles regular tables, computed columns,
//! table type columns, and type specifiers.

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

use crate::model::{ColumnElement, TableElement, TableTypeColumnElement};
use crate::parser::identifier_utils::normalize_identifier;

use super::xml_helpers::{
    write_builtin_type_relationship, write_property, write_schema_relationship,
    write_script_property,
};
use super::{extract_computed_expression_columns, parse_data_type, parse_qualified_name_tokenized};

/// Write a table element to XML.
///
/// Generates the SqlTable Element with Columns relationship and Schema relationship.
pub(crate) fn write_table<W: Write>(
    writer: &mut Writer<W>,
    table: &TableElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", table.schema, table.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlTable"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Write IsAnsiNullsOn property (always true for tables - ANSI_NULLS ON is default)
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Relationship to columns
    if !table.columns.is_empty() {
        let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
        writer.write_event(Event::Start(rel))?;

        for col in &table.columns {
            write_column(writer, col, &full_name)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    // Relationship to schema (comes after Columns in DotNet output)
    write_schema_relationship(writer, &table.schema)?;

    // Write AttachedAnnotation elements that come BEFORE the Annotation
    // DotNet outputs AttachedAnnotations for constraints appearing after the annotated one first
    for disambiguator in &table.attached_annotations_before_annotation {
        let disamb_str = disambiguator.to_string();
        let annotation = BytesStart::new("AttachedAnnotation")
            .with_attributes([("Disambiguator", disamb_str.as_str())]);
        writer.write_event(Event::Empty(annotation))?;
    }

    // Write SqlInlineConstraintAnnotation elements
    // Tables get Annotation for each constraint that uses AttachedAnnotation
    for disambiguator in &table.inline_constraint_disambiguators {
        let disamb_str = disambiguator.to_string();
        let annotation = BytesStart::new("Annotation").with_attributes([
            ("Type", "SqlInlineConstraintAnnotation"),
            ("Disambiguator", disamb_str.as_str()),
        ]);
        writer.write_event(Event::Empty(annotation))?;
    }

    // Write AttachedAnnotation elements that come AFTER the Annotation
    // DotNet outputs AttachedAnnotations for constraints appearing before the annotated one last
    for disambiguator in &table.attached_annotations_after_annotation {
        let disamb_str = disambiguator.to_string();
        let annotation = BytesStart::new("AttachedAnnotation")
            .with_attributes([("Disambiguator", disamb_str.as_str())]);
        writer.write_event(Event::Empty(annotation))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write a column element, dispatching to computed or regular column writer.
pub(crate) fn write_column<W: Write>(
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

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlComputedColumn"), ("Name", col_name.as_str())]);
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
///
/// Uses token-based parsing (Phase 20.4.3) to handle whitespace and various quote styles correctly.
pub(crate) fn parse_qualified_table_name(qualified_name: &str) -> Option<(String, String)> {
    // Use token-based parsing instead of regex (replaces QUALIFIED_TABLE_NAME_RE)
    let qn = parse_qualified_name_tokenized(qualified_name)?;
    qn.schema_and_table()
        .map(|(s, t)| (s.to_string(), t.to_string()))
}

/// Check if a reference string represents a built-in SQL type (e.g., "[nvarchar]", "[int]")
pub(crate) fn is_builtin_type_reference(dep: &str) -> bool {
    // Built-in types are single-part references like "[nvarchar]", not qualified like "[dbo].[Table].[Column]"
    // They have exactly one set of brackets
    let bracket_count = dep.matches('[').count();
    if bracket_count != 1 {
        return false;
    }

    // Extract the type name without brackets using centralized identifier normalization
    let type_name = normalize_identifier(dep).to_lowercase();

    matches!(
        type_name.as_str(),
        "int"
            | "bigint"
            | "smallint"
            | "tinyint"
            | "bit"
            | "decimal"
            | "numeric"
            | "money"
            | "smallmoney"
            | "float"
            | "real"
            | "datetime"
            | "datetime2"
            | "date"
            | "time"
            | "datetimeoffset"
            | "smalldatetime"
            | "char"
            | "varchar"
            | "text"
            | "nchar"
            | "nvarchar"
            | "ntext"
            | "binary"
            | "varbinary"
            | "image"
            | "uniqueidentifier"
            | "xml"
            | "sql_variant"
            | "geography"
            | "geometry"
            | "hierarchyid"
            | "sysname"
    )
}

/// Write ExpressionDependencies relationship for computed columns
fn write_expression_dependencies<W: Write>(
    writer: &mut Writer<W>,
    dependencies: &[String],
) -> anyhow::Result<()> {
    if dependencies.is_empty() {
        return Ok(());
    }

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "ExpressionDependencies")]);
    writer.write_event(Event::Start(rel))?;

    for dep in dependencies {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Conditional attribute - use with_attributes with appropriate attributes
        let refs = if is_builtin_type_reference(dep) {
            BytesStart::new("References")
                .with_attributes([("ExternalSource", "BuiltIns"), ("Name", dep.as_str())])
        } else {
            BytesStart::new("References").with_attributes([("Name", dep.as_str())])
        };
        writer.write_event(Event::Empty(refs))?;

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Write a table type column (uses SqlTableTypeSimpleColumn for user-defined table types)
/// Note: DotNet emits IsNullable="True" for nullable columns (explicit NULL or no nullability specified)
pub(crate) fn write_table_type_column_with_annotation<W: Write>(
    writer: &mut Writer<W>,
    column: &TableTypeColumnElement,
    type_name: &str,
    disambiguator: Option<u32>,
) -> anyhow::Result<()> {
    let col_name = format!("{}.[{}]", type_name, column.name);

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([
        ("Type", "SqlTableTypeSimpleColumn"),
        ("Name", col_name.as_str()),
    ]);
    writer.write_event(Event::Start(elem))?;

    // DotNet emits IsNullable="True" for nullable columns (explicit NULL or no nullability specified)
    // For NOT NULL columns, IsNullable is omitted (defaults to False)
    if column.nullability != Some(false) {
        write_property(writer, "IsNullable", "True")?;
    }

    // Data type relationship
    write_type_specifier(
        writer,
        &column.data_type,
        column.max_length,
        column.precision,
        column.scale,
    )?;

    // SqlInlineConstraintAnnotation for columns with default values
    if let Some(disam) = disambiguator {
        let disamb_str = disam.to_string();
        let annotation = BytesStart::new("Annotation").with_attributes([
            ("Type", "SqlInlineConstraintAnnotation"),
            ("Disambiguator", disamb_str.as_str()),
        ]);
        writer.write_event(Event::Empty(annotation))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write a column with its type information (SqlSimpleColumn)
pub(crate) fn write_column_with_type<W: Write>(
    writer: &mut Writer<W>,
    column: &ColumnElement,
    parent_name: &str,
    column_type: &str,
) -> anyhow::Result<()> {
    let col_name = format!("{}.[{}]", parent_name, column.name);

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", column_type), ("Name", col_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Properties - DotNet property order: Collation, IsNullable, IsIdentity, IsFileStream
    // Collation property for string columns with explicit COLLATE clause (must come first)
    if let Some(ref collation) = column.collation {
        write_property(writer, "Collation", collation)?;
    }

    // IsNullable - only emit IsNullable="False" for NOT NULL columns
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
        let disamb_str = disambiguator.to_string();
        let annotation = BytesStart::new("AttachedAnnotation")
            .with_attributes([("Disambiguator", disamb_str.as_str())]);
        writer.write_event(Event::Empty(annotation))?;
    }

    // Write Annotation element for single named inline constraint
    // DotNet pattern: when a table has exactly one named inline constraint,
    // the column (not the table) carries the Annotation element
    if let Some(disambiguator) = column.inline_constraint_annotation {
        let disamb_str = disambiguator.to_string();
        let annotation = BytesStart::new("Annotation").with_attributes([
            ("Type", "SqlInlineConstraintAnnotation"),
            ("Disambiguator", disamb_str.as_str()),
        ]);
        writer.write_event(Event::Empty(annotation))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write TypeSpecifier relationship for a column
pub(crate) fn write_type_specifier<W: Write>(
    writer: &mut Writer<W>,
    data_type: &str,
    max_length: Option<i32>,
    precision: Option<u8>,
    scale: Option<u8>,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "TypeSpecifier")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
    writer.write_event(Event::Start(elem))?;

    // DotNet order: Properties first, then Type relationship
    // Properties order: Scale, Precision, Length/IsMax
    // Note: Scale=0 is omitted (DotNet behavior)
    if let Some(s) = scale {
        if s > 0 {
            write_property(writer, "Scale", &s.to_string())?;
        }
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

    // Check if this is a User-Defined Type (qualified name with schema)
    // UDTs look like: "[dbo].[PhoneNumber]" or "dbo.PhoneNumber"
    if is_user_defined_type(data_type) {
        // For UDTs, reference the type without ExternalSource
        write_udt_type_relationship(writer, data_type)?;
    } else {
        // Write type reference based on data type (with ExternalSource for built-ins)
        let type_ref = sql_type_to_reference(data_type);
        write_builtin_type_relationship(writer, "Type", &type_ref)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Convert a SQL data type to its bracketed reference form
pub(crate) fn sql_type_to_reference(data_type: &str) -> String {
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
        "decimal" => "[decimal]",
        "numeric" => "[numeric]",
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

/// Check if a data type is a User-Defined Type (qualified name with schema).
/// UDTs look like: "[dbo].[PhoneNumber]" or "dbo.PhoneNumber"
/// Built-in types are single identifiers like "int", "varchar(50)", etc.
fn is_user_defined_type(data_type: &str) -> bool {
    let trimmed = data_type.trim();
    // Extract base type (before any parenthesis for parameters)
    let base_type = trimmed.split('(').next().unwrap_or(trimmed).trim();

    // Check for qualified name patterns:
    // 1. [schema].[name] format
    // 2. schema.name format (with or without brackets)
    if base_type.contains("].[") {
        return true;
    }

    // Check for unbracketed qualified name (contains a dot but not just "[type]")
    if base_type.contains('.') {
        // Filter out single-part types that don't contain a schema separator
        let parts: Vec<&str> = base_type.split('.').collect();
        if parts.len() >= 2 {
            // Has at least schema.name
            return true;
        }
    }

    false
}

/// Write Type relationship for a User-Defined Type (no ExternalSource attribute).
/// UDTs are referenced by their qualified name like [dbo].[PhoneNumber].
fn write_udt_type_relationship<W: Write>(
    writer: &mut Writer<W>,
    data_type: &str,
) -> anyhow::Result<()> {
    use super::normalize_type_name;

    // Use with_attributes for batched attribute setting
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Normalize the UDT name to [schema].[name] format
    // Extract base type (before any parenthesis)
    let base_type = data_type.split('(').next().unwrap_or(data_type).trim();
    let type_ref = normalize_type_name(base_type);

    // No ExternalSource for user-defined types
    let refs = BytesStart::new("References").with_attributes([("Name", type_ref.as_str())]);
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Write TypeSpecifier relationship for table type columns and procedure parameters.
/// Uses precision and scale if available.
pub(crate) fn write_column_type_specifier<W: Write>(
    writer: &mut Writer<W>,
    data_type: &str,
    precision: Option<u8>,
    scale: Option<u8>,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "TypeSpecifier")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let type_spec = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
    writer.write_event(Event::Start(type_spec))?;

    // Write Scale before Precision (DotNet order)
    // Note: Scale=0 is omitted (DotNet behavior)
    if let Some(sc) = scale {
        if sc > 0 {
            write_property(writer, "Scale", &sc.to_string())?;
        }
    }
    if let Some(prec) = precision {
        write_property(writer, "Precision", &prec.to_string())?;
    }

    // Write Type relationship
    let (base_type, _, _, _) = parse_data_type(data_type);
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

/// Write Type relationship for a table type parameter (no ExternalSource attribute)
pub(crate) fn write_table_type_relationship<W: Write>(
    writer: &mut Writer<W>,
    data_type: &str,
) -> anyhow::Result<()> {
    use super::normalize_type_name;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
    writer.write_event(Event::Start(elem))?;

    // Write the type reference (no ExternalSource for user-defined types)
    let type_ref = normalize_type_name(data_type);
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let type_rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(type_rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    // No ExternalSource for user-defined table types
    let refs = BytesStart::new("References").with_attributes([("Name", type_ref.as_str())]);
    writer.write_event(Event::Empty(refs))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_test_writer() -> Writer<Cursor<Vec<u8>>> {
        Writer::new(Cursor::new(Vec::new()))
    }

    fn get_output(writer: Writer<Cursor<Vec<u8>>>) -> String {
        let inner = writer.into_inner();
        String::from_utf8(inner.into_inner()).unwrap()
    }

    #[test]
    fn test_is_builtin_type_reference() {
        assert!(is_builtin_type_reference("[int]"));
        assert!(is_builtin_type_reference("[nvarchar]"));
        assert!(is_builtin_type_reference("[datetime2]"));
        assert!(!is_builtin_type_reference("[dbo].[MyTable]"));
        assert!(!is_builtin_type_reference("[dbo].[MyTable].[Column]"));
        assert!(!is_builtin_type_reference("[MyCustomType]"));
    }

    #[test]
    fn test_sql_type_to_reference() {
        assert_eq!(sql_type_to_reference("int"), "[int]");
        assert_eq!(sql_type_to_reference("INT"), "[int]");
        assert_eq!(sql_type_to_reference("varchar(50)"), "[varchar]");
        assert_eq!(sql_type_to_reference("DECIMAL(10,2)"), "[decimal]");
        assert_eq!(sql_type_to_reference("numeric(18,4)"), "[numeric]");
        assert_eq!(sql_type_to_reference("unknown_type"), "[sql_variant]");
    }

    #[test]
    fn test_write_type_specifier() {
        let mut writer = create_test_writer();
        write_type_specifier(&mut writer, "varchar", Some(50), None, None).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Relationship Name="TypeSpecifier">"#));
        assert!(output.contains(r#"<Element Type="SqlTypeSpecifier">"#));
        assert!(output.contains(r#"<Property Name="Length" Value="50"/>"#));
        assert!(output.contains(r#"Name="[varchar]""#));
    }

    #[test]
    fn test_write_type_specifier_with_precision_scale() {
        let mut writer = create_test_writer();
        write_type_specifier(&mut writer, "decimal", None, Some(18), Some(2)).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Property Name="Scale" Value="2"/>"#));
        assert!(output.contains(r#"<Property Name="Precision" Value="18"/>"#));
        assert!(output.contains(r#"Name="[decimal]""#));
    }

    #[test]
    fn test_write_type_specifier_max() {
        let mut writer = create_test_writer();
        write_type_specifier(&mut writer, "varchar", Some(-1), None, None).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Property Name="IsMax" Value="True"/>"#));
    }

    #[test]
    fn test_write_column_with_type() {
        let column = ColumnElement {
            name: "TestCol".to_string(),
            data_type: "int".to_string(),
            nullability: Some(false),
            max_length: None,
            precision: None,
            scale: None,
            computed_expression: None,
            is_persisted: false,
            is_identity: false,
            is_rowguidcol: false,
            is_sparse: false,
            is_filestream: false,
            default_value: None,
            attached_annotations: vec![],
            inline_constraint_annotation: None,
            collation: None,
        };
        let mut writer = create_test_writer();
        write_column_with_type(&mut writer, &column, "[dbo].[TestTable]", "SqlSimpleColumn")
            .unwrap();
        let output = get_output(writer);
        assert!(output
            .contains(r#"<Element Type="SqlSimpleColumn" Name="[dbo].[TestTable].[TestCol]">"#));
        assert!(output.contains(r#"<Property Name="IsNullable" Value="False"/>"#));
    }

    #[test]
    fn test_write_column_identity() {
        let column = ColumnElement {
            name: "Id".to_string(),
            data_type: "int".to_string(),
            nullability: Some(false),
            max_length: None,
            precision: None,
            scale: None,
            computed_expression: None,
            is_persisted: false,
            is_identity: true,
            is_rowguidcol: false,
            is_sparse: false,
            is_filestream: false,
            default_value: None,
            attached_annotations: vec![],
            inline_constraint_annotation: None,
            collation: None,
        };
        let mut writer = create_test_writer();
        write_column_with_type(&mut writer, &column, "[dbo].[TestTable]", "SqlSimpleColumn")
            .unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Property Name="IsIdentity" Value="True"/>"#));
    }

    #[test]
    fn test_write_table() {
        let table = TableElement {
            schema: "dbo".to_string(),
            name: "TestTable".to_string(),
            columns: vec![ColumnElement {
                name: "Id".to_string(),
                data_type: "int".to_string(),
                nullability: Some(false),
                max_length: None,
                precision: None,
                scale: None,
                computed_expression: None,
                is_persisted: false,
                is_identity: true,
                is_rowguidcol: false,
                is_sparse: false,
                is_filestream: false,
                default_value: None,
                attached_annotations: vec![],
                inline_constraint_annotation: None,
                collation: None,
            }],
            is_node: false,
            is_edge: false,
            inline_constraint_disambiguators: vec![],
            attached_annotations_before_annotation: vec![],
            attached_annotations_after_annotation: vec![],
        };
        let mut writer = create_test_writer();
        write_table(&mut writer, &table).unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Element Type="SqlTable" Name="[dbo].[TestTable]">"#));
        assert!(output.contains(r#"<Property Name="IsAnsiNullsOn" Value="True"/>"#));
        assert!(output.contains(r#"<Relationship Name="Columns">"#));
        assert!(output.contains(r#"<Relationship Name="Schema">"#));
    }

    #[test]
    fn test_write_table_with_disambiguator() {
        let table = TableElement {
            schema: "dbo".to_string(),
            name: "TestTable".to_string(),
            columns: vec![],
            is_node: false,
            is_edge: false,
            inline_constraint_disambiguators: vec![1],
            attached_annotations_before_annotation: vec![],
            attached_annotations_after_annotation: vec![],
        };
        let mut writer = create_test_writer();
        write_table(&mut writer, &table).unwrap();
        let output = get_output(writer);
        assert!(output
            .contains(r#"<Annotation Type="SqlInlineConstraintAnnotation" Disambiguator="1"/>"#));
    }

    #[test]
    fn test_is_user_defined_type() {
        // UDTs (qualified names with schema)
        assert!(is_user_defined_type("[dbo].[PhoneNumber]"));
        assert!(is_user_defined_type("[dbo].[EmailAddress]"));
        assert!(is_user_defined_type("dbo.PhoneNumber"));
        assert!(is_user_defined_type("[Sales].[CustomType]"));

        // Built-in types (no schema qualifier)
        assert!(!is_user_defined_type("int"));
        assert!(!is_user_defined_type("varchar"));
        assert!(!is_user_defined_type("varchar(50)"));
        assert!(!is_user_defined_type("decimal(18,4)"));
        assert!(!is_user_defined_type("[int]"));
        assert!(!is_user_defined_type("nvarchar(max)"));
    }

    #[test]
    fn test_write_type_specifier_with_udt() {
        let mut writer = create_test_writer();
        write_type_specifier(&mut writer, "[dbo].[PhoneNumber]", None, None, None).unwrap();
        let output = get_output(writer);
        // Should NOT contain ExternalSource="BuiltIns" for UDTs
        assert!(!output.contains("ExternalSource"));
        // Should reference the UDT name
        assert!(output.contains(r#"Name="[dbo].[PhoneNumber]""#));
    }

    #[test]
    fn test_write_type_specifier_builtin_still_works() {
        let mut writer = create_test_writer();
        write_type_specifier(&mut writer, "int", None, None, None).unwrap();
        let output = get_output(writer);
        // Should contain ExternalSource="BuiltIns" for built-in types
        assert!(output.contains(r#"ExternalSource="BuiltIns""#));
        assert!(output.contains(r#"Name="[int]""#));
    }
}
