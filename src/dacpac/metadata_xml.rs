//! Generate DacMetadata.xml for dacpac

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

use crate::project::SqlProject;

const NAMESPACE: &str = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02";

pub fn generate_metadata_xml<W: Write>(
    writer: W,
    project: &SqlProject,
    version: &str,
) -> anyhow::Result<()> {
    let mut xml_writer = Writer::new_with_indent(writer, b' ', 2);
    // Add space before /> in self-closing tags to match DotNet DacFx output
    xml_writer
        .config_mut()
        .add_space_before_slash_in_empty_elements = true;

    // XML declaration
    xml_writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    // Root element - DacType per MS schema
    let mut root = BytesStart::new("DacType");
    root.push_attribute(("xmlns", NAMESPACE));
    xml_writer.write_event(Event::Start(root))?;

    // Name
    write_element(&mut xml_writer, "Name", &project.name)?;

    // Version
    write_element(&mut xml_writer, "Version", version)?;

    // Description - emit only if DacDescription is specified in sqlproj (matches DacFx behavior)
    if let Some(ref description) = project.dac_description {
        write_element(&mut xml_writer, "Description", description)?;
    }

    // Close root
    xml_writer.write_event(Event::End(BytesEnd::new("DacType")))?;

    Ok(())
}

fn write_element<W: Write>(writer: &mut Writer<W>, name: &str, value: &str) -> anyhow::Result<()> {
    writer.write_event(Event::Start(BytesStart::new(name)))?;
    writer.write_event(Event::Text(quick_xml::events::BytesText::new(value)))?;
    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}
