//! Generate model.xml for dacpac

use quick_xml::events::{BytesCData, BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

use crate::model::{
    ColumnElement, ConstraintElement, ConstraintType, DatabaseModel, FunctionElement, IndexElement,
    ModelElement, ProcedureElement, RawElement, SchemaElement, SequenceElement, TableElement,
    UserDefinedTypeElement, ViewElement,
};
use crate::project::SqlProject;

const NAMESPACE: &str = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02";

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
        ModelElement::Raw(r) => write_raw(writer, r),
    }
}

fn write_schema<W: Write>(writer: &mut Writer<W>, schema: &SchemaElement) -> anyhow::Result<()> {
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
    write_relationship(writer, "Schema", &[&format!("[{}]", table.schema)])?;

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
    let col_name = format!("{}.[{}]", table_name, column.name);

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlSimpleColumn"));
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

    // Write QueryScript property with CDATA containing the view definition
    write_script_property(writer, "QueryScript", &view.definition)?;

    write_relationship(writer, "Schema", &[&format!("[{}]", view.schema)])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
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

    // Write BodyScript property with CDATA containing the procedure definition
    write_script_property(writer, "BodyScript", &proc.definition)?;

    write_relationship(writer, "Schema", &[&format!("[{}]", proc.schema)])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_function<W: Write>(writer: &mut Writer<W>, func: &FunctionElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", func.schema, func.name);
    let type_name = match func.function_type {
        crate::model::FunctionType::Scalar => "SqlScalarFunction",
        crate::model::FunctionType::TableValued => "SqlTableValuedFunction",
        crate::model::FunctionType::InlineTableValued => "SqlInlineTableValuedFunction",
    };

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", type_name));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write BodyScript property with CDATA containing the function definition
    write_script_property(writer, "BodyScript", &func.definition)?;

    write_relationship(writer, "Schema", &[&format!("[{}]", func.schema)])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
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

    // Reference to table
    let table_ref = format!("[{}].[{}]", constraint.table_schema, constraint.table_name);
    write_relationship(writer, "DefiningTable", &[&table_ref])?;

    // For foreign keys, add reference to foreign table
    if constraint.constraint_type == ConstraintType::ForeignKey {
        if let Some(ref foreign_table) = constraint.referenced_table {
            write_relationship(writer, "ForeignTable", &[foreign_table])?;
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

fn write_sequence<W: Write>(writer: &mut Writer<W>, seq: &SequenceElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", seq.schema, seq.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlSequence"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write BodyScript property with CDATA containing the sequence definition
    write_script_property(writer, "BodyScript", &seq.definition)?;

    // Relationship to schema
    write_relationship(writer, "Schema", &[&format!("[{}]", seq.schema)])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_user_defined_type<W: Write>(
    writer: &mut Writer<W>,
    udt: &UserDefinedTypeElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", udt.schema, udt.name);

    let mut elem = BytesStart::new("Element");
    elem.push_attribute(("Type", "SqlUserDefinedTableType"));
    elem.push_attribute(("Name", full_name.as_str()));
    writer.write_event(Event::Start(elem))?;

    // Write BodyScript property with CDATA containing the type definition
    write_script_property(writer, "BodyScript", &udt.definition)?;

    // Relationship to schema
    write_relationship(writer, "Schema", &[&format!("[{}]", udt.schema)])?;

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
    write_relationship(writer, "Schema", &[&format!("[{}]", raw.schema)])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}
