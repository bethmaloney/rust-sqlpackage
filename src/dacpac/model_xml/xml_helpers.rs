//! Low-level XML writing utilities for model.xml generation.
//!
//! This module provides basic XML element writing functions used throughout
//! the model_xml generation code. These are the building blocks for writing
//! properties, relationships, and other common XML patterns.

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesCData, BytesEnd, BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::Writer;
use std::borrow::Cow;
use std::io::Write;

/// Built-in schemas that exist by default in SQL Server
pub(crate) const BUILTIN_SCHEMAS: &[&str] = &[
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
pub(crate) fn is_builtin_schema(schema: &str) -> bool {
    BUILTIN_SCHEMAS
        .iter()
        .any(|&s| s.eq_ignore_ascii_case(schema))
}

/// Write a simple Property element with Name and Value attributes.
///
/// Generates: `<Property Name="name" Value="value"/>`
pub(crate) fn write_property<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    value: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let prop = BytesStart::new("Property").with_attributes([("Name", name), ("Value", value)]);
    writer.write_event(Event::Empty(prop))?;
    Ok(())
}

/// Normalize script content for consistent output.
///
/// DotNet DacFx normalizes line endings in script content to LF (Unix-style).
/// This ensures consistent output regardless of the source file's line endings.
pub(crate) fn normalize_script_content(script: &str) -> String {
    // Convert CRLF to LF for consistent line endings
    script.replace("\r\n", "\n")
}

/// Escape a string for use in XML attribute values, including newlines.
///
/// This function performs full XML attribute escaping:
/// - `&` becomes `&amp;`
/// - `<` becomes `&lt;`
/// - `>` becomes `&gt;`
/// - `"` becomes `&quot;`
/// - LF (\\n, 0x0A) becomes `&#xA;`
/// - CR (\\r, 0x0D) becomes `&#xD;`
///
/// DotNet DacFx uses XML numeric character references for newlines in attribute values.
/// This function is used with `write_property_raw` to write pre-escaped values.
pub(crate) fn escape_newlines_for_attr(s: &str) -> String {
    // First escape XML special characters (order matters: & must be first)
    let escaped = s
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    // Then escape newlines
    escaped.replace('\r', "&#xD;").replace('\n', "&#xA;")
}

/// Write a Property element with a pre-escaped (raw) value that won't be double-escaped.
///
/// This is needed for values containing XML entity references like `&#xA;` (newline)
/// which should be preserved as-is in the output. Using the standard `write_property`
/// would cause `&` to be escaped to `&amp;`, resulting in `&amp;#xA;`.
///
/// Generates: `<Property Name="name" Value="value"/>`
pub(crate) fn write_property_raw<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    raw_value: &str,
) -> anyhow::Result<()> {
    let mut prop = BytesStart::new("Property");
    prop.push_attribute(("Name", name));
    // Use Attribute struct with raw bytes to avoid escaping
    prop.push_attribute(Attribute {
        key: QName(b"Value"),
        value: Cow::Borrowed(raw_value.as_bytes()),
    });
    writer.write_event(Event::Empty(prop))?;
    Ok(())
}

/// Write a property with a CDATA value (for script content like QueryScript, BodyScript).
///
/// Generates:
/// ```xml
/// <Property Name="name">
///   <Value><![CDATA[script]]></Value>
/// </Property>
/// ```
pub(crate) fn write_script_property<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    script: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let prop = BytesStart::new("Property").with_attributes([("Name", name)]);
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

/// Write a Relationship element with multiple entries.
///
/// Generates:
/// ```xml
/// <Relationship Name="name">
///   <Entry>
///     <References Name="ref1"/>
///   </Entry>
///   <Entry>
///     <References Name="ref2"/>
///   </Entry>
/// </Relationship>
/// ```
pub(crate) fn write_relationship<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    references: &[&str],
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", name)]);
    writer.write_event(Event::Start(rel))?;

    for reference in references {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let refs = BytesStart::new("References").with_attributes([("Name", *reference)]);
        writer.write_event(Event::Empty(refs))?;

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write a Relationship referencing a built-in type.
///
/// Generates:
/// ```xml
/// <Relationship Name="name">
///   <Entry>
///     <References ExternalSource="BuiltIns" Name="type_ref"/>
///   </Entry>
/// </Relationship>
/// ```
pub(crate) fn write_builtin_type_relationship<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    type_ref: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", name)]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Batch both attributes in a single with_attributes call
    let refs = BytesStart::new("References")
        .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref)]);
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write a Schema relationship, using ExternalSource="BuiltIns" for built-in schemas.
///
/// For built-in schemas (dbo, sys, etc.), generates:
/// ```xml
/// <Relationship Name="Schema">
///   <Entry>
///     <References ExternalSource="BuiltIns" Name="[schema]"/>
///   </Entry>
/// </Relationship>
/// ```
///
/// For user-defined schemas, omits ExternalSource.
pub(crate) fn write_schema_relationship<W: Write>(
    writer: &mut Writer<W>,
    schema: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Schema")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let schema_ref = format!("[{}]", schema);
    // Conditional attribute - use with_attributes with appropriate attributes
    let refs = if is_builtin_schema(schema) {
        BytesStart::new("References").with_attributes([
            ("ExternalSource", "BuiltIns"),
            ("Name", schema_ref.as_str()),
        ])
    } else {
        BytesStart::new("References").with_attributes([("Name", schema_ref.as_str())])
    };
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write TypeSpecifier relationship for sequences referencing a built-in type.
///
/// Generates:
/// ```xml
/// <Relationship Name="TypeSpecifier">
///   <Entry>
///     <Element Type="SqlTypeSpecifier">
///       <Relationship Name="Type">
///         <Entry>
///           <References ExternalSource="BuiltIns" Name="[type_name]"/>
///         </Entry>
///       </Relationship>
///     </Element>
///   </Entry>
/// </Relationship>
/// ```
pub(crate) fn write_type_specifier_builtin<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "TypeSpecifier")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
    writer.write_event(Event::Start(elem))?;

    // Nested Type relationship referencing the built-in type
    let inner_rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(inner_rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let refs = BytesStart::new("References")
        .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_name)]);
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
    use quick_xml::events::attributes::Attribute;
    use quick_xml::name::QName;
    use std::borrow::Cow;
    use std::io::Cursor;

    fn create_test_writer() -> Writer<Cursor<Vec<u8>>> {
        Writer::new(Cursor::new(Vec::new()))
    }

    fn get_output(writer: Writer<Cursor<Vec<u8>>>) -> String {
        let inner = writer.into_inner();
        String::from_utf8(inner.into_inner()).unwrap()
    }

    #[test]
    fn test_raw_attribute_with_entity() {
        // Test that using push_attribute with Attribute struct preserves entity references
        let mut writer = create_test_writer();
        let mut elem = BytesStart::new("Property");
        elem.push_attribute(("Name", "Test"));
        // Use Attribute struct with raw bytes
        elem.push_attribute(Attribute {
            key: QName(b"Value"),
            value: Cow::Borrowed(b"line1&#xA;line2"),
        });
        writer.write_event(Event::Empty(elem)).unwrap();
        let output = get_output(writer);
        println!("Raw attribute output: {}", output);
        // Check if entity reference is preserved
        assert!(
            output.contains("&#xA;"),
            "Should contain raw &#xA; entity, got: {}",
            output
        );
    }

    #[test]
    fn test_is_builtin_schema() {
        assert!(is_builtin_schema("dbo"));
        assert!(is_builtin_schema("DBO"));
        assert!(is_builtin_schema("sys"));
        assert!(is_builtin_schema("INFORMATION_SCHEMA"));
        assert!(!is_builtin_schema("custom"));
        assert!(!is_builtin_schema("myschema"));
    }

    #[test]
    fn test_write_property() {
        let mut writer = create_test_writer();
        write_property(&mut writer, "TestName", "TestValue").unwrap();
        let output = get_output(writer);
        assert_eq!(output, r#"<Property Name="TestName" Value="TestValue"/>"#);
    }

    #[test]
    fn test_normalize_script_content() {
        assert_eq!(normalize_script_content("line1\r\nline2"), "line1\nline2");
        assert_eq!(normalize_script_content("line1\nline2"), "line1\nline2");
        assert_eq!(normalize_script_content("a\r\nb\r\nc"), "a\nb\nc");
    }

    #[test]
    fn test_write_script_property() {
        let mut writer = create_test_writer();
        write_script_property(&mut writer, "BodyScript", "SELECT 1").unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Property Name="BodyScript">"#));
        assert!(output.contains("<Value>"));
        assert!(output.contains("<![CDATA[SELECT 1]]>"));
        assert!(output.contains("</Value>"));
        assert!(output.contains("</Property>"));
    }

    #[test]
    fn test_write_relationship() {
        let mut writer = create_test_writer();
        write_relationship(
            &mut writer,
            "Columns",
            &["[dbo].[T1].[Col1]", "[dbo].[T1].[Col2]"],
        )
        .unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Relationship Name="Columns">"#));
        assert!(output.contains("<Entry>"));
        assert!(output.contains(r#"<References Name="[dbo].[T1].[Col1]"/>"#));
        assert!(output.contains(r#"<References Name="[dbo].[T1].[Col2]"/>"#));
        assert!(output.contains("</Relationship>"));
    }

    #[test]
    fn test_write_builtin_type_relationship() {
        let mut writer = create_test_writer();
        write_builtin_type_relationship(&mut writer, "Type", "[int]").unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Relationship Name="Type">"#));
        assert!(output.contains(r#"ExternalSource="BuiltIns""#));
        assert!(output.contains(r#"Name="[int]""#));
    }

    #[test]
    fn test_write_schema_relationship_builtin() {
        let mut writer = create_test_writer();
        write_schema_relationship(&mut writer, "dbo").unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Relationship Name="Schema">"#));
        assert!(output.contains(r#"ExternalSource="BuiltIns""#));
        assert!(output.contains(r#"Name="[dbo]""#));
    }

    #[test]
    fn test_write_schema_relationship_custom() {
        let mut writer = create_test_writer();
        write_schema_relationship(&mut writer, "custom").unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Relationship Name="Schema">"#));
        assert!(!output.contains("ExternalSource"));
        assert!(output.contains(r#"Name="[custom]""#));
    }

    #[test]
    fn test_write_type_specifier_builtin() {
        let mut writer = create_test_writer();
        write_type_specifier_builtin(&mut writer, "[int]").unwrap();
        let output = get_output(writer);
        assert!(output.contains(r#"<Relationship Name="TypeSpecifier">"#));
        assert!(output.contains(r#"<Element Type="SqlTypeSpecifier">"#));
        assert!(output.contains(r#"<Relationship Name="Type">"#));
        assert!(output.contains(r#"ExternalSource="BuiltIns""#));
        assert!(output.contains(r#"Name="[int]""#));
    }
}
