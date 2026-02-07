//! Order-independent XML comparison for DacMetadata.xml, [Content_Types].xml

use super::types::FileStatus;

/// Convert an XML node to a canonical string: "tag|attr1=val1|attr2=val2|text=content"
fn xml_to_canonical(node: &roxmltree::Node) -> String {
    let mut parts = Vec::new();

    // Tag name without namespace
    parts.push(node.tag_name().name().to_string());

    // Sorted attributes (skip namespace declarations)
    let mut attrs: Vec<(&str, &str)> = node
        .attributes()
        .map(|a: roxmltree::Attribute| (a.name(), a.value()))
        .collect();
    attrs.sort_by_key(|(k, _)| *k);
    for (k, v) in &attrs {
        parts.push(format!("{}={}", k, v));
    }

    // Text content
    let text = node.text().unwrap_or("").trim();
    if !text.is_empty() {
        parts.push(format!("text={}", text));
    }

    parts.join("|")
}

/// Flatten an XML tree to sorted (path, canonical_string) tuples.
fn flatten(node: &roxmltree::Node, path: &str) -> Vec<(String, String)> {
    let tag = node.tag_name().name();
    let current = format!("{}/{}", path, tag);
    let canonical = xml_to_canonical(node);

    let mut result = vec![(current.clone(), canonical)];

    // Collect children, sort by their canonical strings for order-independence
    let mut children: Vec<roxmltree::Node> = node.children().filter(|c| c.is_element()).collect();
    children.sort_by_key(|a| xml_to_canonical(a));

    for child in &children {
        result.extend(flatten(child, &current));
    }

    result
}

/// Compare two XML strings in an order-independent way.
pub fn compare_simple_xml(xml_a: &str, xml_b: &str) -> FileStatus {
    let doc_a = match roxmltree::Document::parse(xml_a) {
        Ok(d) => d,
        Err(e) => return FileStatus::Different(vec![format!("Failed to parse rust XML: {}", e)]),
    };
    let doc_b = match roxmltree::Document::parse(xml_b) {
        Ok(d) => d,
        Err(e) => return FileStatus::Different(vec![format!("Failed to parse dotnet XML: {}", e)]),
    };

    let flat_a = flatten(&doc_a.root_element(), "");
    let flat_b = flatten(&doc_b.root_element(), "");

    if flat_a == flat_b {
        return FileStatus::Ok;
    }

    // Show differences
    let lines_a: Vec<String> = flat_a
        .iter()
        .map(|(p, c)| format!("{}: {}", p, c))
        .collect();
    let lines_b: Vec<String> = flat_b
        .iter()
        .map(|(p, c)| format!("{}: {}", p, c))
        .collect();

    let mut diff_lines = vec!["--- dotnet".to_string(), "+++ rust".to_string()];

    // Simple set difference
    let set_a: std::collections::HashSet<&String> = lines_a.iter().collect();
    let set_b: std::collections::HashSet<&String> = lines_b.iter().collect();

    for line in &lines_b {
        if !set_a.contains(line) {
            diff_lines.push(format!("-{}", line));
        }
    }
    for line in &lines_a {
        if !set_b.contains(line) {
            diff_lines.push(format!("+{}", line));
        }
    }

    FileStatus::Different(diff_lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_xml() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Root xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Child Name="a" />
  <Child Name="b" />
</Root>"#;
        assert!(compare_simple_xml(xml, xml).is_ok());
    }

    #[test]
    fn test_reordered_children() {
        let xml_a = r#"<?xml version="1.0" encoding="utf-8"?>
<Root xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Child Name="b" />
  <Child Name="a" />
</Root>"#;
        let xml_b = r#"<?xml version="1.0" encoding="utf-8"?>
<Root xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Child Name="a" />
  <Child Name="b" />
</Root>"#;
        assert!(compare_simple_xml(xml_a, xml_b).is_ok());
    }

    #[test]
    fn test_different_attributes() {
        let xml_a = r#"<?xml version="1.0" encoding="utf-8"?>
<Root xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Child Name="a" Value="1" />
</Root>"#;
        let xml_b = r#"<?xml version="1.0" encoding="utf-8"?>
<Root xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Child Name="a" Value="2" />
</Root>"#;
        assert!(!compare_simple_xml(xml_a, xml_b).is_ok());
    }

    #[test]
    fn test_different_text_content() {
        let xml_a = r#"<?xml version="1.0" encoding="utf-8"?>
<Root xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Name>Hello</Name>
</Root>"#;
        let xml_b = r#"<?xml version="1.0" encoding="utf-8"?>
<Root xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Name>World</Name>
</Root>"#;
        assert!(!compare_simple_xml(xml_a, xml_b).is_ok());
    }
}
