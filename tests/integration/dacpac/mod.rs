//! Integration tests for dacpac file validation
//!
//! These tests verify the structure and content of generated dacpac files.

mod alias_resolution_tests;
mod column_tests;
mod constraint_tests;
mod element_tests;
mod index_tests;
mod metadata_tests;
mod model_xml_tests;
mod structure_tests;

// Re-export helper functions used by multiple test modules
pub(crate) fn parse_model_xml(model_xml: &str) -> roxmltree::Document<'_> {
    roxmltree::Document::parse(model_xml).expect("Model XML should be valid XML")
}

pub(crate) fn find_elements_by_type<'a>(
    doc: &'a roxmltree::Document,
    type_name: &str,
) -> Vec<roxmltree::Node<'a, 'a>> {
    doc.descendants()
        .filter(|n| n.tag_name().name() == "Element" && n.attribute("Type") == Some(type_name))
        .collect()
}

pub(crate) fn has_relationship(element: &roxmltree::Node, rel_name: &str) -> bool {
    element
        .children()
        .any(|c| c.tag_name().name() == "Relationship" && c.attribute("Name") == Some(rel_name))
}

pub(crate) fn find_column_by_name<'a>(
    doc: &'a roxmltree::Document,
    column_name: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
    doc.descendants().find(|n| {
        n.tag_name().name() == "Element"
            && n.attribute("Type") == Some("SqlSimpleColumn")
            && n.attribute("Name")
                .is_some_and(|name| name.contains(column_name))
    })
}

pub(crate) fn get_property_value(element: &roxmltree::Node, property_name: &str) -> Option<String> {
    element
        .children()
        .find(|c| c.tag_name().name() == "Property" && c.attribute("Name") == Some(property_name))
        .and_then(|p| p.attribute("Value").map(String::from))
}

pub(crate) fn get_type_specifier_property(
    column: &roxmltree::Node,
    property_name: &str,
) -> Option<String> {
    // Navigate: Column -> TypeSpecifier relationship -> Entry -> SqlTypeSpecifier element -> Property
    column
        .children()
        .find(|c| {
            c.tag_name().name() == "Relationship" && c.attribute("Name") == Some("TypeSpecifier")
        })
        .and_then(|rel| rel.children().find(|c| c.tag_name().name() == "Entry"))
        .and_then(|entry| entry.children().find(|c| c.tag_name().name() == "Element"))
        .and_then(|elem| get_property_value(&elem, property_name))
}

pub(crate) fn find_index_by_name<'a>(
    doc: &'a roxmltree::Document,
    index_name: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
    // Match exact index name at end of full name (e.g., [dbo].[Table].[IndexName])
    let suffix = format!("].[{}]", index_name);
    doc.descendants().find(|n| {
        n.tag_name().name() == "Element"
            && n.attribute("Type") == Some("SqlIndex")
            && n.attribute("Name")
                .is_some_and(|name| name.ends_with(&suffix))
    })
}
