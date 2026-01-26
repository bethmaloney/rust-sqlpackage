//! Parity Testing Infrastructure
//!
//! This module provides a comprehensive framework for comparing dacpac output
//! between rust-sqlpackage and DotNet DacFx. The comparison is organized into
//! multiple layers, each providing increasingly detailed validation.
//!
//! ## Layer Architecture
//!
//! | Layer | Module | Purpose |
//! |-------|--------|---------|
//! | 1 | `layer1_inventory` | Element inventory comparison (names and counts) |
//! | 2 | `layer2_properties` | Property comparison (key or all properties) |
//! | 3 | `layer3_sqlpackage` | SqlPackage DeployReport validation |
//! | 4 | `layer4_structure` | Element ordering comparison |
//! | 5 | `layer5_relationships` | Relationship comparison |
//! | 6 | `layer6_metadata` | Metadata file comparison ([Content_Types].xml, etc.) |
//! | 7 | `layer7_canonical` | Canonical XML comparison for byte-level matching |
//!
//! ## Usage
//!
//! Import the types and functions you need from the re-exported modules:
//!
//! ```ignore
//! use crate::parity::{
//!     DacpacModel, ComparisonOptions, ComparisonResult,
//!     compare_element_inventory, compare_all_properties,
//!     compare_canonical_dacpacs,
//! };
//! ```
//!
//! Or use the full comparison function:
//!
//! ```ignore
//! let result = compare_dacpacs_with_options(&rust_dacpac, &dotnet_dacpac, &options)?;
//! if result.is_success() {
//!     println!("Full parity achieved!");
//! }
//! ```

// Core types module - foundational data structures used by all layers
pub mod types;

// Layer modules - each implements a specific comparison strategy
pub mod layer1_inventory;
pub mod layer2_properties;
pub mod layer3_sqlpackage;
pub mod layer4_structure;
pub mod layer5_relationships;
pub mod layer6_metadata;
pub mod layer7_canonical;

// Re-export commonly used types from types module
pub use types::{
    CanonicalXmlError, ComparisonOptions, ComparisonResult, ContentTypesXml, DacMetadataXml,
    DacpacModel, DetailedFixtureResult, FixtureMetrics, Layer1Error, Layer2Error, Layer3Result,
    Layer4Error, MetadataFileError, ModelElement, OriginXml, ParityMetrics, ParityReport,
    ReferenceEntry, Relationship, RelationshipError,
};

// Re-export extract_model_xml for convenience
pub use types::extract_model_xml;

// Re-export layer functions
pub use layer1_inventory::compare_element_inventory;
pub use layer2_properties::{
    compare_all_properties, compare_element_properties, get_all_properties,
};
pub use layer3_sqlpackage::{compare_with_sqlpackage, sqlpackage_available};
pub use layer4_structure::compare_element_order;
pub use layer5_relationships::{compare_element_relationships, compare_relationships};
pub use layer6_metadata::{
    compare_content_types, compare_dac_metadata, compare_dacpac_files, compare_deploy_scripts,
    compare_origin_xml, extract_content_types_xml, extract_dac_metadata_xml, extract_deploy_script,
    extract_origin_xml, normalize_script_whitespace,
};
pub use layer7_canonical::{
    canonicalize_model_xml, compare_canonical_dacpacs, compare_canonical_xml, compute_sha256,
    generate_diff,
};

use std::path::Path;

/// Perform full layered comparison of two dacpacs.
///
/// This is a convenience function that uses default options (no Layer 3).
pub fn compare_dacpacs(
    rust_dacpac: &Path,
    dotnet_dacpac: &Path,
    include_layer3: bool,
) -> Result<ComparisonResult, String> {
    let options = ComparisonOptions {
        include_layer3,
        strict_properties: false,
        check_relationships: false,
        check_element_order: false,
        check_metadata_files: false,
        check_deploy_scripts: false,
    };
    compare_dacpacs_with_options(rust_dacpac, dotnet_dacpac, &options)
}

/// Perform full layered comparison of two dacpacs with configurable options.
///
/// Options:
/// - `include_layer3`: Run SqlPackage DeployReport comparison
/// - `strict_properties`: Compare ALL properties (not just key properties)
/// - `check_relationships`: Validate all relationships between elements
/// - `check_element_order`: Validate element ordering matches DotNet output (Phase 4)
/// - `check_metadata_files`: Compare metadata files like [Content_Types].xml (Phase 5)
/// - `check_deploy_scripts`: Compare pre/post-deploy scripts (Phase 5.4)
pub fn compare_dacpacs_with_options(
    rust_dacpac: &Path,
    dotnet_dacpac: &Path,
    options: &ComparisonOptions,
) -> Result<ComparisonResult, String> {
    let rust_model = DacpacModel::from_dacpac(rust_dacpac)?;
    let dotnet_model = DacpacModel::from_dacpac(dotnet_dacpac)?;

    let layer1_errors = compare_element_inventory(&rust_model, &dotnet_model);

    let layer2_errors = if options.strict_properties {
        compare_all_properties(&rust_model, &dotnet_model)
    } else {
        compare_element_properties(&rust_model, &dotnet_model)
    };

    let relationship_errors = if options.check_relationships {
        compare_element_relationships(&rust_model, &dotnet_model)
    } else {
        Vec::new()
    };

    let layer4_errors = if options.check_element_order {
        compare_element_order(&rust_model, &dotnet_model)
    } else {
        Vec::new()
    };

    // Phase 5: Metadata file comparison
    // Use unified compare_dacpac_files() when both options are enabled,
    // otherwise use individual comparison functions as needed
    let metadata_errors = if options.check_metadata_files && options.check_deploy_scripts {
        // Use unified function for complete metadata comparison
        compare_dacpac_files(rust_dacpac, dotnet_dacpac)
    } else {
        let mut errors = if options.check_metadata_files {
            let mut errs = compare_content_types(rust_dacpac, dotnet_dacpac);
            errs.extend(compare_dac_metadata(rust_dacpac, dotnet_dacpac));
            errs.extend(compare_origin_xml(rust_dacpac, dotnet_dacpac));
            errs
        } else {
            Vec::new()
        };

        // Add deploy script comparison if enabled (Phase 5.4)
        if options.check_deploy_scripts {
            errors.extend(compare_deploy_scripts(rust_dacpac, dotnet_dacpac));
        }
        errors
    };

    let layer3_result = if options.include_layer3 {
        Some(compare_with_sqlpackage(rust_dacpac, dotnet_dacpac))
    } else {
        None
    };

    Ok(ComparisonResult {
        layer1_errors,
        layer2_errors,
        relationship_errors,
        layer4_errors,
        metadata_errors,
        layer3_result,
    })
}
