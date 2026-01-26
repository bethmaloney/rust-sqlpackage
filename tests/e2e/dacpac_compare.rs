//! Layered dacpac comparison utilities for E2E testing
//!
//! This module provides a comprehensive framework for comparing dacpac output
//! between rust-sqlpackage and DotNet DacFx. The comparison is organized into
//! multiple layers, each providing increasingly detailed validation:
//!
//! 1. Element inventory - verify all elements exist with correct names
//! 2. Property comparison - verify element properties match
//! 3. SqlPackage DeployReport - verify deployment equivalence
//! 4. Element ordering - verify element order matches DotNet output
//! 5. Metadata files - verify Content_Types.xml, DacMetadata.xml, Origin.xml match
//! 6. Relationship comparison - verify relationships between elements
//! 7. Canonical XML comparison - verify byte-level matching after normalization
//!
//! ## Module Organization
//!
//! The comparison infrastructure is organized into the `parity/` submodule:
//!
//! ```text
//! tests/e2e/
//! ├── dacpac_compare.rs         # This file - re-exports parity module
//! └── parity/
//!     ├── mod.rs                # Module coordinator
//!     ├── types.rs              # Shared data structures and error types
//!     ├── layer1_inventory.rs   # Element inventory comparison
//!     ├── layer2_properties.rs  # Property comparison
//!     ├── layer3_sqlpackage.rs  # SqlPackage DeployReport
//!     ├── layer4_structure.rs   # Element ordering comparison
//!     ├── layer5_relationships.rs # Relationship comparison
//!     ├── layer6_metadata.rs    # Metadata file comparison
//!     └── layer7_canonical.rs   # Canonical XML comparison
//! ```
//!
//! All types and functions are re-exported from this module for backward compatibility.

// The parity module contains the modular implementation
pub mod parity;

// Re-export everything from parity module for backward compatibility
pub use parity::*;
