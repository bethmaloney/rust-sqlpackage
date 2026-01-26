//! Layer 1: Element Inventory Comparison
//!
//! Verifies all elements exist with correct names between Rust and DotNet dacpac output.
//! This is the foundational comparison layer - if element inventories don't match,
//! higher layers will have incorrect results.

use std::collections::BTreeSet;

use super::types::{DacpacModel, Layer1Error};

/// Compare element inventories between two models.
///
/// This function compares:
/// 1. Named elements present in both models (finds missing/extra)
/// 2. Element counts by type (detects count discrepancies)
///
/// # Returns
/// A vector of `Layer1Error` containing all inventory mismatches.
pub fn compare_element_inventory(
    rust_model: &DacpacModel,
    dotnet_model: &DacpacModel,
) -> Vec<Layer1Error> {
    let mut errors = Vec::new();

    let rust_elements = rust_model.named_elements();
    let dotnet_elements = dotnet_model.named_elements();

    // Find elements missing in Rust
    for (elem_type, name) in dotnet_elements.difference(&rust_elements) {
        errors.push(Layer1Error::MissingInRust {
            element_type: elem_type.clone(),
            name: name.clone(),
        });
    }

    // Find extra elements in Rust
    for (elem_type, name) in rust_elements.difference(&dotnet_elements) {
        errors.push(Layer1Error::ExtraInRust {
            element_type: elem_type.clone(),
            name: name.clone(),
        });
    }

    // Compare counts by type
    let rust_types = rust_model.element_types();
    let dotnet_types = dotnet_model.element_types();
    let all_types: BTreeSet<_> = rust_types.union(&dotnet_types).collect();

    for elem_type in all_types {
        let rust_count = rust_model.elements_of_type(elem_type).len();
        let dotnet_count = dotnet_model.elements_of_type(elem_type).len();

        if rust_count != dotnet_count {
            errors.push(Layer1Error::CountMismatch {
                element_type: elem_type.clone(),
                rust_count,
                dotnet_count,
            });
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_empty_models() {
        let rust_model = DacpacModel::empty();
        let dotnet_model = DacpacModel::empty();

        let errors = compare_element_inventory(&rust_model, &dotnet_model);
        assert!(errors.is_empty());
    }
}
