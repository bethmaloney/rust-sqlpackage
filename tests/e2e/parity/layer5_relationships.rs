//! Layer 5: Relationship Comparison
//!
//! Compares relationships between elements in Rust and DotNet dacpac output.
//! Relationships link elements to other objects (tables, columns, types, etc.)
//! and are critical for maintaining referential integrity in the model.

use std::collections::HashMap;

use super::types::{DacpacModel, ModelElement, Relationship, RelationshipError};

/// Compare relationships of all matching elements between two models.
///
/// This function iterates through all named elements that exist in both models
/// and compares their relationships.
pub fn compare_element_relationships(
    rust_model: &DacpacModel,
    dotnet_model: &DacpacModel,
) -> Vec<RelationshipError> {
    let mut errors = Vec::new();

    // Compare relationships of all matching named elements
    for (elem_type, name) in rust_model.named_elements() {
        if let (Some(rust_elem), Some(dotnet_elem)) = (
            rust_model.get_element(&elem_type, &name),
            dotnet_model.get_element(&elem_type, &name),
        ) {
            errors.extend(compare_relationships(rust_elem, dotnet_elem));
        }
    }

    errors
}

/// Compare relationships between two matching elements.
///
/// Compares:
/// 1. Relationship names (finds missing/extra relationships)
/// 2. Reference counts within each relationship
/// 3. Reference names and external sources
/// 4. Nested element counts within relationships
/// 5. Recursively compares child element relationships
pub fn compare_relationships(
    rust_elem: &ModelElement,
    dotnet_elem: &ModelElement,
) -> Vec<RelationshipError> {
    let mut errors = Vec::new();
    let elem_type = rust_elem.element_type.clone();
    let elem_name = rust_elem.name.clone().unwrap_or_default();

    // Build maps of relationships by name for both elements
    let rust_rels: HashMap<_, _> = rust_elem
        .relationships
        .iter()
        .map(|r| (r.name.clone(), r))
        .collect();

    let dotnet_rels: HashMap<_, _> = dotnet_elem
        .relationships
        .iter()
        .map(|r| (r.name.clone(), r))
        .collect();

    // Find relationships missing in Rust (present in DotNet but not Rust)
    for rel_name in dotnet_rels.keys() {
        if !rust_rels.contains_key(rel_name) {
            errors.push(RelationshipError::MissingRelationship {
                element_type: elem_type.clone(),
                element_name: elem_name.clone(),
                relationship_name: rel_name.clone(),
            });
        }
    }

    // Find extra relationships in Rust (present in Rust but not DotNet)
    for rel_name in rust_rels.keys() {
        if !dotnet_rels.contains_key(rel_name) {
            errors.push(RelationshipError::ExtraRelationship {
                element_type: elem_type.clone(),
                element_name: elem_name.clone(),
                relationship_name: rel_name.clone(),
            });
        }
    }

    // Compare matching relationships
    for (rel_name, rust_rel) in &rust_rels {
        if let Some(dotnet_rel) = dotnet_rels.get(rel_name) {
            errors.extend(compare_relationship_pair(
                &elem_type, &elem_name, rust_rel, dotnet_rel,
            ));
        }
    }

    // Recursively compare relationships in nested elements (children)
    // Use (type, name) as key to handle elements with same name but different types
    // (e.g., SqlDynamicColumnSource and SqlSubroutineParameter can have the same name)
    let rust_named_children: HashMap<_, _> = rust_elem
        .children
        .iter()
        .filter_map(|c| {
            c.name
                .as_ref()
                .map(|n| ((c.element_type.clone(), n.clone()), c))
        })
        .collect();

    for dotnet_child in &dotnet_elem.children {
        if let Some(ref child_name) = dotnet_child.name {
            let key = (dotnet_child.element_type.clone(), child_name.clone());
            if let Some(rust_child) = rust_named_children.get(&key) {
                errors.extend(compare_relationships(rust_child, dotnet_child));
            }
        }
    }

    errors
}

/// Compare a pair of relationships with the same name.
fn compare_relationship_pair(
    elem_type: &str,
    elem_name: &str,
    rust_rel: &Relationship,
    dotnet_rel: &Relationship,
) -> Vec<RelationshipError> {
    let mut errors = Vec::new();

    // Compare reference counts
    if rust_rel.references.len() != dotnet_rel.references.len() {
        errors.push(RelationshipError::ReferenceCountMismatch {
            element_type: elem_type.to_string(),
            element_name: elem_name.to_string(),
            relationship_name: rust_rel.name.clone(),
            rust_count: rust_rel.references.len(),
            dotnet_count: dotnet_rel.references.len(),
        });
    }

    // Compare reference names (order matters for some relationships)
    let rust_ref_names: Vec<_> = rust_rel.references.iter().map(|r| r.name.clone()).collect();
    let dotnet_ref_names: Vec<_> = dotnet_rel
        .references
        .iter()
        .map(|r| r.name.clone())
        .collect();

    if rust_ref_names != dotnet_ref_names {
        errors.push(RelationshipError::ReferenceMismatch {
            element_type: elem_type.to_string(),
            element_name: elem_name.to_string(),
            relationship_name: rust_rel.name.clone(),
            rust_refs: rust_ref_names,
            dotnet_refs: dotnet_ref_names,
        });
    }

    // Compare nested element counts
    if rust_rel.entries.len() != dotnet_rel.entries.len() {
        errors.push(RelationshipError::EntryCountMismatch {
            element_type: elem_type.to_string(),
            element_name: elem_name.to_string(),
            relationship_name: rust_rel.name.clone(),
            rust_count: rust_rel.entries.len(),
            dotnet_count: dotnet_rel.entries.len(),
        });
    }

    // Recursively compare nested element relationships
    let rust_named_entries: HashMap<_, _> = rust_rel
        .entries
        .iter()
        .filter_map(|e| e.name.as_ref().map(|n| (n.clone(), e)))
        .collect();

    for dotnet_entry in &dotnet_rel.entries {
        if let Some(ref entry_name) = dotnet_entry.name {
            if let Some(rust_entry) = rust_named_entries.get(entry_name) {
                errors.extend(compare_relationships(rust_entry, dotnet_entry));
            }
        }
    }

    errors
}
