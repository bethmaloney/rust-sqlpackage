//! Generate Origin.xml for dacpac

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

const NAMESPACE: &str = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02";

/// The product schema URL used by dotnet DacFx
const PRODUCT_SCHEMA: &str = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02";

/// Product name for rust-sqlpackage
const PRODUCT_NAME: &str = "rust-sqlpackage";

/// Product version for rust-sqlpackage
const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

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

    // Operation (before Checksums per XSD schema order)
    xml_writer.write_event(Event::Start(BytesStart::new("Operation")))?;

    write_element(&mut xml_writer, "Identity", "rust-sqlpackage")?;
    write_element(&mut xml_writer, "Start", &chrono::Utc::now().to_rfc3339())?;
    write_element(&mut xml_writer, "End", &chrono::Utc::now().to_rfc3339())?;

    // ProductName (matches dotnet behavior)
    write_element(&mut xml_writer, "ProductName", PRODUCT_NAME)?;

    // ProductVersion (matches dotnet behavior)
    write_element(&mut xml_writer, "ProductVersion", PRODUCT_VERSION)?;

    // ProductSchema as simple URL string (matches dotnet behavior and XSD schema)
    write_element(&mut xml_writer, "ProductSchema", PRODUCT_SCHEMA)?;

    xml_writer.write_event(Event::End(BytesEnd::new("Operation")))?;

    // Checksums (after Operation per XSD schema order)
    xml_writer.write_event(Event::Start(BytesStart::new("Checksums")))?;
    let mut checksum = BytesStart::new("Checksum");
    checksum.push_attribute(("Uri", "/model.xml"));
    xml_writer.write_event(Event::Start(checksum))?;
    xml_writer.write_event(Event::Text(quick_xml::events::BytesText::new(
        model_xml_checksum,
    )))?;
    xml_writer.write_event(Event::End(BytesEnd::new("Checksum")))?;
    xml_writer.write_event(Event::End(BytesEnd::new("Checksums")))?;

    // ModelSchemaVersion (after Checksums, matches DotNet behavior)
    write_element(&mut xml_writer, "ModelSchemaVersion", "2.9")?;

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
