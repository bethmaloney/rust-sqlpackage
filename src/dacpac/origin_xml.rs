//! Generate Origin.xml for dacpac

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

const NAMESPACE: &str = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02";

pub fn generate_origin_xml<W: Write>(writer: W, model_xml_checksum: &str) -> anyhow::Result<()> {
    let mut xml_writer = Writer::new_with_indent(writer, b' ', 2);

    // XML declaration
    xml_writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    // Root element
    let mut root = BytesStart::new("DacOrigin");
    root.push_attribute(("xmlns", NAMESPACE));
    xml_writer.write_event(Event::Start(root))?;

    // PackageProperties
    xml_writer.write_event(Event::Start(BytesStart::new("PackageProperties")))?;

    write_element(&mut xml_writer, "Version", "3.1.0.0")?;
    write_element(&mut xml_writer, "ContainsExportedData", "false")?;

    // StreamVersions with nested Version elements
    xml_writer.write_event(Event::Start(BytesStart::new("StreamVersions")))?;
    let mut data_version = BytesStart::new("Version");
    data_version.push_attribute(("StreamName", "Data"));
    xml_writer.write_event(Event::Start(data_version))?;
    xml_writer.write_event(Event::Text(quick_xml::events::BytesText::new("2.0.0.0")))?;
    xml_writer.write_event(Event::End(BytesEnd::new("Version")))?;
    let mut contrib_version = BytesStart::new("Version");
    contrib_version.push_attribute(("StreamName", "DeploymentContributors"));
    xml_writer.write_event(Event::Start(contrib_version))?;
    xml_writer.write_event(Event::Text(quick_xml::events::BytesText::new("1.0.0.0")))?;
    xml_writer.write_event(Event::End(BytesEnd::new("Version")))?;
    xml_writer.write_event(Event::End(BytesEnd::new("StreamVersions")))?;

    xml_writer.write_event(Event::End(BytesEnd::new("PackageProperties")))?;

    // Checksums
    xml_writer.write_event(Event::Start(BytesStart::new("Checksums")))?;
    let mut checksum = BytesStart::new("Checksum");
    checksum.push_attribute(("Uri", "/model.xml"));
    xml_writer.write_event(Event::Start(checksum))?;
    xml_writer.write_event(Event::Text(quick_xml::events::BytesText::new(
        model_xml_checksum,
    )))?;
    xml_writer.write_event(Event::End(BytesEnd::new("Checksum")))?;
    xml_writer.write_event(Event::End(BytesEnd::new("Checksums")))?;

    // Operation
    xml_writer.write_event(Event::Start(BytesStart::new("Operation")))?;

    write_element(&mut xml_writer, "Identity", "rust-sqlpackage")?;
    write_element(&mut xml_writer, "Start", &chrono::Utc::now().to_rfc3339())?;
    write_element(&mut xml_writer, "End", &chrono::Utc::now().to_rfc3339())?;

    // ProductSchema (required for compatibility)
    xml_writer.write_event(Event::Start(BytesStart::new("ProductSchema")))?;

    let mut major = BytesStart::new("MajorVersion");
    major.push_attribute(("Value", "160"));
    xml_writer.write_event(Event::Empty(major))?;

    xml_writer.write_event(Event::End(BytesEnd::new("ProductSchema")))?;

    xml_writer.write_event(Event::End(BytesEnd::new("Operation")))?;

    // Close root
    xml_writer.write_event(Event::End(BytesEnd::new("DacOrigin")))?;

    Ok(())
}

fn write_element<W: Write>(writer: &mut Writer<W>, name: &str, value: &str) -> anyhow::Result<()> {
    writer.write_event(Event::Start(BytesStart::new(name)))?;
    writer.write_event(Event::Text(quick_xml::events::BytesText::new(value)))?;
    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}
