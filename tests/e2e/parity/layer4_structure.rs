//! Layer 4: Element Ordering / Structure Comparison
//!
//! DotNet DacFx generates elements in a specific, deterministic order within model.xml.
//! This layer validates that Rust output follows the same ordering conventions.

use std::collections::{BTreeSet, HashMap};

use super::types::{DacpacModel, Layer4Error};

/// Compare the ordering of elements between two models.
///
/// DotNet DacFx generates elements in a specific, deterministic order:
/// 1. Elements are typically grouped by type (schemas, tables, views, procedures, etc.)
/// 2. Within each type, elements may be ordered alphabetically or by dependency
///
/// This function compares:
/// - Type ordering: Which element types appear first
/// - Element ordering: Position of individual elements within the model
///
/// # Returns
/// A vector of `Layer4Error` for any ordering mismatches found.
pub fn compare_element_order(
    rust_model: &DacpacModel,
    dotnet_model: &DacpacModel,
) -> Vec<Layer4Error> {
    let mut errors = Vec::new();

    // Build position maps for named elements
    // Position is the index in the elements vector (order in XML)
    let rust_positions = build_element_position_map(rust_model);
    let dotnet_positions = build_element_position_map(dotnet_model);

    // Compare type ordering - which types appear first in each model
    errors.extend(compare_type_ordering(rust_model, dotnet_model));

    // Compare individual element positions
    // Only compare elements that exist in both models (Layer 1 catches missing/extra)
    for ((elem_type, elem_name), &rust_pos) in &rust_positions {
        if let Some(&dotnet_pos) = dotnet_positions.get(&(elem_type.clone(), elem_name.clone())) {
            if rust_pos != dotnet_pos {
                errors.push(Layer4Error::ElementOrderMismatch {
                    element_type: elem_type.clone(),
                    element_name: elem_name.clone(),
                    rust_position: rust_pos,
                    dotnet_position: dotnet_pos,
                });
            }
        }
    }

    errors
}

/// Build a map of (element_type, element_name) -> position index
fn build_element_position_map(model: &DacpacModel) -> HashMap<(String, String), usize> {
    let mut positions = HashMap::new();

    for (idx, element) in model.elements.iter().enumerate() {
        if let Some(ref name) = element.name {
            positions.insert((element.element_type.clone(), name.clone()), idx);
        }
    }

    positions
}

/// Compare the ordering of element types between models.
///
/// For example, if DotNet outputs schemas at position 0-2, tables at 3-10,
/// and views at 11-15, Rust should follow the same pattern.
fn compare_type_ordering(rust_model: &DacpacModel, dotnet_model: &DacpacModel) -> Vec<Layer4Error> {
    let mut errors = Vec::new();

    // Find first occurrence of each type
    let rust_type_first_pos = find_type_first_positions(rust_model);
    let dotnet_type_first_pos = find_type_first_positions(dotnet_model);

    // Get all types that appear in both models
    let all_types: BTreeSet<_> = rust_type_first_pos
        .keys()
        .chain(dotnet_type_first_pos.keys())
        .cloned()
        .collect();

    for elem_type in all_types {
        if let (Some(&rust_first), Some(&dotnet_first)) = (
            rust_type_first_pos.get(&elem_type),
            dotnet_type_first_pos.get(&elem_type),
        ) {
            // Compare relative ordering of types
            // We don't require exact positions, but check if the relative order differs significantly
            if rust_first != dotnet_first {
                errors.push(Layer4Error::TypeOrderMismatch {
                    element_type: elem_type,
                    rust_first_position: rust_first,
                    dotnet_first_position: dotnet_first,
                });
            }
        }
    }

    errors
}

/// Find the first position where each element type appears in the model
fn find_type_first_positions(model: &DacpacModel) -> HashMap<String, usize> {
    let mut first_positions = HashMap::new();

    for (idx, element) in model.elements.iter().enumerate() {
        first_positions
            .entry(element.element_type.clone())
            .or_insert(idx);
    }

    first_positions
}
