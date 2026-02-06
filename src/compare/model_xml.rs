//! Semantic comparison of model.xml files

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::types::{ElementKey, HeaderResult, ModelElementsResult, RelEntry};

const NS: &str = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02";

/// Check if a node is an element with the given local name in the DAC namespace.
fn is_ns_element(node: &roxmltree::Node, local_name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == local_name
        && node.tag_name().namespace() == Some(NS)
}

/// Find the first child element with the given local name in the DAC namespace.
fn find_child<'a>(
    parent: &'a roxmltree::Node<'a, 'a>,
    local_name: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
    parent.children().find(|c| is_ns_element(c, local_name))
}

/// Find all child elements with the given local name in the DAC namespace.
fn find_children<'a>(
    parent: &'a roxmltree::Node<'a, 'a>,
    local_name: &str,
) -> Vec<roxmltree::Node<'a, 'a>> {
    parent
        .children()
        .filter(|c| is_ns_element(c, local_name))
        .collect()
}

/// Get the Name attribute of the first References in a named Relationship.
/// Matches Python's `get_ref_name()`.
fn get_ref_name(elem: &roxmltree::Node, rel_name: &str) -> Option<String> {
    let rel = elem
        .children()
        .find(|c| is_ns_element(c, "Relationship") && c.attribute("Name") == Some(rel_name))?;

    let entry = find_child(&rel, "Entry")?;
    let refs = find_child(&entry, "References")?;
    refs.attribute("Name").map(|s| s.to_string())
}

/// Generate a unique key for a model Element.
/// Matches Python's `element_key()`.
pub fn element_key(elem: &roxmltree::Node) -> ElementKey {
    let elem_type = elem.attribute("Type").unwrap_or("").to_string();
    let name = elem.attribute("Name");

    if let Some(name) = name {
        return ElementKey::Named {
            element_type: elem_type,
            name: name.to_string(),
        };
    }

    // Try DefiningTable + ForColumn (e.g. SqlDefaultConstraint)
    let defining_table = get_ref_name(elem, "DefiningTable");
    let for_column = get_ref_name(elem, "ForColumn");
    let defining_column = get_ref_name(elem, "DefiningColumn");

    if let Some(dt) = &defining_table {
        if let Some(fc) = &for_column {
            return ElementKey::Composite {
                element_type: elem_type,
                composite: format!("DefiningTable={},ForColumn={}", dt, fc),
            };
        }
        if let Some(dc) = &defining_column {
            return ElementKey::Composite {
                element_type: elem_type,
                composite: format!("DefiningTable={},DefiningColumn={}", dt, dc),
            };
        }
        return ElementKey::Composite {
            element_type: elem_type,
            composite: format!("DefiningTable={}", dt),
        };
    }

    // Singleton type (e.g. SqlDatabaseOptions)
    ElementKey::Singleton {
        element_type: elem_type,
    }
}

/// Extract properties as BTreeMap: name -> value.
/// Matches Python's `get_properties()`.
pub fn get_properties(elem: &roxmltree::Node) -> BTreeMap<String, String> {
    let mut props = BTreeMap::new();
    for prop in find_children(elem, "Property") {
        let name = prop.attribute("Name").unwrap_or("").to_string();
        let value = if let Some(v) = prop.attribute("Value") {
            v.to_string()
        } else {
            // Check for child Value element with text
            find_child(&prop, "Value")
                .and_then(|v| v.text())
                .unwrap_or("")
                .trim()
                .to_string()
        };
        props.insert(name, value);
    }
    props
}

/// Create a fingerprint of an inline element for comparison (order-independent).
/// Matches Python's `inline_element_fingerprint()`.
fn inline_element_fingerprint(elem: &roxmltree::Node) -> String {
    let type_part = elem.attribute("Type").unwrap_or("");

    // Properties sorted by Name
    let mut prop_parts: Vec<String> = Vec::new();
    for prop in find_children(elem, "Property") {
        let name = prop.attribute("Name").unwrap_or("");
        let value = if let Some(v) = prop.attribute("Value") {
            v.to_string()
        } else {
            find_child(&prop, "Value")
                .and_then(|v| v.text())
                .unwrap_or("")
                .trim()
                .to_string()
        };
        prop_parts.push(format!("P:{}={}", name, value));
    }
    prop_parts.sort();

    // Nested relationships sorted
    let mut rel_parts: Vec<String> = Vec::new();
    for rel in find_children(elem, "Relationship") {
        let rel_name = rel.attribute("Name").unwrap_or("");
        for entry in find_children(&rel, "Entry") {
            if let Some(refs) = find_child(&entry, "References") {
                let mut ref_name = refs.attribute("Name").unwrap_or("").to_string();
                if let Some(ext) = refs.attribute("ExternalSource") {
                    ref_name = format!("{}@{}", ref_name, ext);
                }
                rel_parts.push(format!("R:{}={}", rel_name, ref_name));
            } else if let Some(inner) = find_child(&entry, "Element") {
                rel_parts.push(format!(
                    "R:{}=({})",
                    rel_name,
                    inline_element_fingerprint(&inner)
                ));
            }
        }
    }
    rel_parts.sort();

    // Annotations sorted
    let mut ann_parts: Vec<String> = Vec::new();
    for ann in find_children(elem, "AttachedAnnotation") {
        let ann_type = ann.attribute("Type").unwrap_or("");
        let mut ann_props = get_properties(&ann);
        ann_props.remove("Disambiguator");
        ann_parts.push(format!("A:{}={:?}", ann_type, ann_props));
    }
    ann_parts.sort();

    let mut all_parts = vec![type_part.to_string()];
    all_parts.extend(prop_parts);
    all_parts.extend(rel_parts);
    all_parts.extend(ann_parts);
    all_parts.join("|")
}

/// Extract relationships as BTreeMap: name -> Vec<RelEntry>.
/// Matches Python's `get_relationships()`.
pub fn get_relationships(elem: &roxmltree::Node) -> BTreeMap<String, Vec<RelEntry>> {
    let mut rels = BTreeMap::new();
    for rel in find_children(elem, "Relationship") {
        let rel_name = rel.attribute("Name").unwrap_or("").to_string();
        let mut entries = Vec::new();
        for entry in find_children(&rel, "Entry") {
            if let Some(refs) = find_child(&entry, "References") {
                let mut ref_key = refs.attribute("Name").unwrap_or("").to_string();
                if let Some(ext) = refs.attribute("ExternalSource") {
                    ref_key = format!("{}@{}", ref_key, ext);
                }
                entries.push(RelEntry::Ref(ref_key));
            } else if let Some(inline) = find_child(&entry, "Element") {
                entries.push(RelEntry::Inline(inline_element_fingerprint(&inline)));
            }
        }
        rels.insert(rel_name, entries);
    }
    rels
}

/// Extract annotations as sorted list of (type, sorted properties) tuples.
/// Matches Python's `get_annotations()`.
fn get_annotations(elem: &roxmltree::Node) -> Vec<(String, Vec<(String, String)>)> {
    let mut anns = Vec::new();
    for ann in find_children(elem, "AttachedAnnotation") {
        let ann_type = ann.attribute("Type").unwrap_or("").to_string();
        let mut props = get_properties(&ann);
        props.remove("Disambiguator");
        let sorted_props: Vec<(String, String)> = props.into_iter().collect();
        anns.push((ann_type, sorted_props));
    }
    anns.sort();
    anns
}

/// Compare two elements and return list of difference descriptions.
/// Matches Python's `diff_element()`.
fn diff_element(elem_a: &roxmltree::Node, elem_b: &roxmltree::Node) -> Vec<String> {
    let mut diffs = Vec::new();

    // Compare properties
    let props_a = get_properties(elem_a);
    let props_b = get_properties(elem_b);
    let all_prop_names: BTreeSet<&String> = props_a.keys().chain(props_b.keys()).collect();

    for name in all_prop_names {
        let val_a = props_a.get(name);
        let val_b = props_b.get(name);
        if val_a != val_b {
            match (val_a, val_b) {
                (Some(va), None) => {
                    diffs.push(format!(
                        "    Property \"{}\": missing in dotnet, rust=\"{}\"",
                        name, va
                    ));
                }
                (None, Some(vb)) => {
                    diffs.push(format!(
                        "    Property \"{}\": dotnet=\"{}\", missing in rust",
                        name, vb
                    ));
                }
                (Some(va), Some(vb)) => {
                    diffs.push(format!(
                        "    Property \"{}\": dotnet=\"{}\", rust=\"{}\"",
                        name, vb, va
                    ));
                }
                _ => {}
            }
        }
    }

    // Compare relationships
    let rels_a = get_relationships(elem_a);
    let rels_b = get_relationships(elem_b);
    let all_rel_names: BTreeSet<&String> = rels_a.keys().chain(rels_b.keys()).collect();

    for name in all_rel_names {
        let entries_a = rels_a.get(name);
        let entries_b = rels_b.get(name);
        if entries_a != entries_b {
            match (entries_a, entries_b) {
                (Some(ea), None) => {
                    diffs.push(format!(
                        "    Relationship \"{}\": missing in dotnet, rust has {} entries",
                        name,
                        ea.len()
                    ));
                }
                (None, Some(eb)) => {
                    diffs.push(format!(
                        "    Relationship \"{}\": dotnet has {} entries, missing in rust",
                        name,
                        eb.len()
                    ));
                }
                (Some(ea), Some(eb)) => {
                    let set_a: HashSet<String> = ea.iter().map(|e| e.to_string()).collect();
                    let set_b: HashSet<String> = eb.iter().map(|e| e.to_string()).collect();
                    let only_rust: Vec<&String> = set_a.difference(&set_b).collect();
                    let only_dotnet: Vec<&String> = set_b.difference(&set_a).collect();
                    if !only_rust.is_empty() || !only_dotnet.is_empty() {
                        diffs.push(format!(
                            "    Relationship \"{}\": {} only in dotnet, {} only in rust",
                            name,
                            only_dotnet.len(),
                            only_rust.len()
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    // Compare annotations
    let anns_a = get_annotations(elem_a);
    let anns_b = get_annotations(elem_b);
    if anns_a != anns_b {
        let set_a: HashSet<String> = anns_a.iter().map(|a| format!("{:?}", a)).collect();
        let set_b: HashSet<String> = anns_b.iter().map(|a| format!("{:?}", a)).collect();
        let only_rust: HashSet<&String> = set_a.difference(&set_b).collect();
        let only_dotnet: HashSet<&String> = set_b.difference(&set_a).collect();
        if !only_rust.is_empty() || !only_dotnet.is_empty() {
            // Collect affected annotation types
            let mut types_affected: BTreeSet<&str> = BTreeSet::new();
            for a in anns_a.iter().chain(anns_b.iter()) {
                types_affected.insert(&a.0);
            }
            for ann_type in types_affected {
                let rust_of_type: Vec<_> = anns_a.iter().filter(|a| a.0 == ann_type).collect();
                let dotnet_of_type: Vec<_> = anns_b.iter().filter(|a| a.0 == ann_type).collect();
                if rust_of_type != dotnet_of_type {
                    let count_info = if rust_of_type.len() != dotnet_of_type.len() {
                        format!(
                            " (rust={}, dotnet={})",
                            rust_of_type.len(),
                            dotnet_of_type.len()
                        )
                    } else {
                        String::new()
                    };
                    diffs.push(format!(
                        "    Annotation \"{}\": differs{}",
                        ann_type, count_info
                    ));
                }
            }
        }
    }

    diffs
}

/// Compare Header/CustomData sections.
/// Matches Python's `compare_header()`.
pub fn compare_header(
    header_a: Option<roxmltree::Node>,
    header_b: Option<roxmltree::Node>,
) -> HeaderResult {
    fn index_custom_data(
        header: &Option<roxmltree::Node>,
    ) -> BTreeMap<(String, String), BTreeMap<String, String>> {
        let mut result = BTreeMap::new();
        let header = match header {
            Some(h) => h,
            None => return result,
        };
        for cd in header.children().filter(|c| is_ns_element(c, "CustomData")) {
            let category = cd.attribute("Category").unwrap_or("").to_string();
            let type_attr = cd.attribute("Type").unwrap_or("").to_string();
            let mut metas = BTreeMap::new();
            for m in cd.children().filter(|c| is_ns_element(c, "Metadata")) {
                let name = m.attribute("Name").unwrap_or("").to_string();
                let value = m.attribute("Value").unwrap_or("").to_string();
                metas.insert(name, value);
            }
            result.insert((category, type_attr), metas);
        }
        result
    }

    let cd_a = index_custom_data(&header_a);
    let cd_b = index_custom_data(&header_b);
    let all_keys: BTreeSet<&(String, String)> = cd_a.keys().chain(cd_b.keys()).collect();

    let mut diffs = Vec::new();
    for key in all_keys {
        let label = if key.1.is_empty() {
            format!("CustomData({})", key.0)
        } else {
            format!("CustomData({}, {})", key.0, key.1)
        };

        match (cd_a.get(key), cd_b.get(key)) {
            (Some(_), None) => {
                diffs.push(format!("  {}: missing in dotnet", label));
            }
            (None, Some(_)) => {
                diffs.push(format!("  {}: missing in rust", label));
            }
            (Some(ma), Some(mb)) if ma != mb => {
                diffs.push(format!("  {}:", label));
                let all_meta: BTreeSet<&String> = ma.keys().chain(mb.keys()).collect();
                for mk in all_meta {
                    let va = ma.get(mk);
                    let vb = mb.get(mk);
                    if va != vb {
                        diffs.push(format!(
                            "    {}: dotnet=\"{}\", rust=\"{}\"",
                            mk,
                            vb.map(|s| s.as_str()).unwrap_or(""),
                            va.map(|s| s.as_str()).unwrap_or("")
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    HeaderResult {
        is_ok: diffs.is_empty(),
        diffs,
    }
}

/// Compare two model.xml documents semantically.
/// Matches Python's `compare_model_xml()`.
pub fn compare_model_xml(
    xml_a: &str,
    xml_b: &str,
) -> (
    HeaderResult,
    ModelElementsResult,
    Vec<(String, Vec<ElementKey>)>,
) {
    let doc_a = roxmltree::Document::parse(xml_a).expect("Failed to parse rust model.xml");
    let doc_b = roxmltree::Document::parse(xml_b).expect("Failed to parse dotnet model.xml");
    let root_a = doc_a.root_element();
    let root_b = doc_b.root_element();

    // Compare headers
    let header_a = find_child(&root_a, "Header");
    let header_b = find_child(&root_b, "Header");
    let header_result = compare_header(header_a, header_b);

    // Compare model elements
    let model_a = find_child(&root_a, "Model").expect("No Model element in rust model.xml");
    let model_b = find_child(&root_b, "Model").expect("No Model element in dotnet model.xml");

    // Index elements by key
    fn index_elements<'a>(
        model: &roxmltree::Node<'a, 'a>,
    ) -> (
        HashMap<ElementKey, roxmltree::Node<'a, 'a>>,
        Vec<ElementKey>,
    ) {
        let mut index = HashMap::new();
        let mut duplicates = Vec::new();
        for elem in model.children().filter(|c| is_ns_element(c, "Element")) {
            let key = element_key(&elem);
            if index.contains_key(&key) {
                duplicates.push(key.clone());
            }
            index.insert(key, elem);
        }
        (index, duplicates)
    }

    let (elems_a, dupes_a) = index_elements(&model_a);
    let (elems_b, dupes_b) = index_elements(&model_b);

    let mut duplicate_warnings = Vec::new();
    if !dupes_a.is_empty() {
        duplicate_warnings.push(("rust".to_string(), dupes_a));
    }
    if !dupes_b.is_empty() {
        duplicate_warnings.push(("dotnet".to_string(), dupes_b));
    }

    let keys_a: HashSet<&ElementKey> = elems_a.keys().collect();
    let keys_b: HashSet<&ElementKey> = elems_b.keys().collect();

    let mut missing_in_rust: Vec<ElementKey> =
        keys_b.difference(&keys_a).map(|k| (*k).clone()).collect();
    missing_in_rust.sort_by(|a, b| a.to_string().cmp(&b.to_string()));

    let mut extra_in_rust: Vec<ElementKey> =
        keys_a.difference(&keys_b).map(|k| (*k).clone()).collect();
    extra_in_rust.sort_by(|a, b| a.to_string().cmp(&b.to_string()));

    let mut common: Vec<&ElementKey> = keys_a.intersection(&keys_b).copied().collect();
    common.sort_by(|a, b| a.to_string().cmp(&b.to_string()));

    let mut differences = Vec::new();
    for key in common {
        let elem_a = &elems_a[key];
        let elem_b = &elems_b[key];
        let diffs = diff_element(elem_a, elem_b);
        if !diffs.is_empty() {
            differences.push((key.clone(), diffs));
        }
    }

    let elements_result = ModelElementsResult {
        total_rust: elems_a.len(),
        total_dotnet: elems_b.len(),
        missing_in_rust,
        extra_in_rust,
        differences,
    };

    (header_result, elements_result, duplicate_warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_MODEL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Header>
    <CustomData Category="Reference" Type="System">
      <Metadata Name="FileName" Value="master.dacpac" />
    </CustomData>
  </Header>
  <Model>
    <Element Type="SqlSchema" Name="[dbo]">
      <Property Name="IsDefault" Value="True" />
    </Element>
  </Model>
</DataSchemaModel>"#;

    #[test]
    fn test_identical_model() {
        let (header, elems, dupes) = compare_model_xml(MINIMAL_MODEL, MINIMAL_MODEL);
        assert!(header.is_ok);
        assert!(elems.missing_in_rust.is_empty());
        assert!(elems.extra_in_rust.is_empty());
        assert!(elems.differences.is_empty());
        assert!(dupes.is_empty());
    }

    #[test]
    fn test_element_key_named() {
        let doc = roxmltree::Document::parse(MINIMAL_MODEL).unwrap();
        let root = doc.root_element();
        let model = find_child(&root, "Model").unwrap();
        let elem = model
            .children()
            .find(|c| is_ns_element(c, "Element"))
            .unwrap();
        let key = element_key(&elem);
        assert_eq!(
            key,
            ElementKey::Named {
                element_type: "SqlSchema".to_string(),
                name: "[dbo]".to_string(),
            }
        );
    }

    #[test]
    fn test_element_key_singleton() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Model>
    <Element Type="SqlDatabaseOptions">
      <Property Name="Collation" Value="SQL_Latin1_General_CP1_CI_AS" />
    </Element>
  </Model>
</DataSchemaModel>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let root = doc.root_element();
        let model = find_child(&root, "Model").unwrap();
        let elem = model
            .children()
            .find(|c| is_ns_element(c, "Element"))
            .unwrap();
        let key = element_key(&elem);
        assert_eq!(
            key,
            ElementKey::Singleton {
                element_type: "SqlDatabaseOptions".to_string(),
            }
        );
    }

    #[test]
    fn test_element_key_composite() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Model>
    <Element Type="SqlDefaultConstraint">
      <Relationship Name="DefiningTable">
        <Entry>
          <References Name="[dbo].[MyTable]" />
        </Entry>
      </Relationship>
      <Relationship Name="ForColumn">
        <Entry>
          <References Name="[dbo].[MyTable].[Col1]" />
        </Entry>
      </Relationship>
    </Element>
  </Model>
</DataSchemaModel>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let root = doc.root_element();
        let model = find_child(&root, "Model").unwrap();
        let elem = model
            .children()
            .find(|c| is_ns_element(c, "Element"))
            .unwrap();
        let key = element_key(&elem);
        assert_eq!(
            key,
            ElementKey::Composite {
                element_type: "SqlDefaultConstraint".to_string(),
                composite: "DefiningTable=[dbo].[MyTable],ForColumn=[dbo].[MyTable].[Col1]"
                    .to_string(),
            }
        );
    }

    #[test]
    fn test_missing_element() {
        let xml_a = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Header />
  <Model>
    <Element Type="SqlSchema" Name="[dbo]" />
  </Model>
</DataSchemaModel>"#;

        let xml_b = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Header />
  <Model>
    <Element Type="SqlSchema" Name="[dbo]" />
    <Element Type="SqlTable" Name="[dbo].[Users]" />
  </Model>
</DataSchemaModel>"#;

        let (_, elems, _) = compare_model_xml(xml_a, xml_b);
        assert_eq!(elems.missing_in_rust.len(), 1);
        assert_eq!(
            elems.missing_in_rust[0],
            ElementKey::Named {
                element_type: "SqlTable".to_string(),
                name: "[dbo].[Users]".to_string(),
            }
        );
        assert!(elems.extra_in_rust.is_empty());
    }

    #[test]
    fn test_property_difference() {
        let xml_a = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Header />
  <Model>
    <Element Type="SqlSchema" Name="[dbo]">
      <Property Name="IsDefault" Value="True" />
    </Element>
  </Model>
</DataSchemaModel>"#;

        let xml_b = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Header />
  <Model>
    <Element Type="SqlSchema" Name="[dbo]">
      <Property Name="IsDefault" Value="False" />
    </Element>
  </Model>
</DataSchemaModel>"#;

        let (_, elems, _) = compare_model_xml(xml_a, xml_b);
        assert_eq!(elems.differences.len(), 1);
        assert!(elems.differences[0].1[0].contains("IsDefault"));
    }

    #[test]
    fn test_header_comparison() {
        let xml_a = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Header>
    <CustomData Category="Reference" Type="System">
      <Metadata Name="FileName" Value="master.dacpac" />
    </CustomData>
  </Header>
  <Model />
</DataSchemaModel>"#;

        let xml_b = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Header>
    <CustomData Category="Reference" Type="System">
      <Metadata Name="FileName" Value="different.dacpac" />
    </CustomData>
  </Header>
  <Model />
</DataSchemaModel>"#;

        let (header, _, _) = compare_model_xml(xml_a, xml_b);
        assert!(!header.is_ok);
        assert!(!header.diffs.is_empty());
    }

    #[test]
    fn test_get_properties_with_value_child() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Element xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02" Type="SqlProcedure" Name="[dbo].[MyProc]">
  <Property Name="BodyScript">
    <Value>SELECT 1</Value>
  </Property>
</Element>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let elem = doc.root_element();
        let props = get_properties(&elem);
        assert_eq!(props.get("BodyScript").unwrap(), "SELECT 1");
    }

    #[test]
    fn test_get_relationships() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Element xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02" Type="SqlTable" Name="[dbo].[T]">
  <Relationship Name="Schema">
    <Entry>
      <References Name="[dbo]" />
    </Entry>
  </Relationship>
</Element>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let elem = doc.root_element();
        let rels = get_relationships(&elem);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels["Schema"], vec![RelEntry::Ref("[dbo]".to_string())]);
    }
}
