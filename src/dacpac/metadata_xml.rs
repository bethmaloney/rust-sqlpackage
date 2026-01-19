//! Generate DacMetadata.xml for dacpac

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

use crate::project::SqlProject;

const NAMESPACE: &str = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02";

pub fn generate_metadata_xml<W: Write>(writer: W, project: &SqlProject) -> anyhow::Result<()> {
    let mut xml_writer = Writer::new_with_indent(writer, b' ', 2);

    // XML declaration
    xml_writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    // Root element
    let mut root = BytesStart::new("DacType");
    root.push_attribute(("xmlns", NAMESPACE));
    xml_writer.write_event(Event::Start(root))?;

    // Name
    write_element(&mut xml_writer, "Name", &project.name)?;

    // Version
    write_element(&mut xml_writer, "Version", "1.0.0.0")?;

    // Description (optional)
    write_element(&mut xml_writer, "Description", "")?;

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
