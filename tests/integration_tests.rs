//! Integration tests for rust-sqlpackage
//!
//! This file serves as the entry point for all integration tests.

#[path = "common/mod.rs"]
mod common;

#[path = "integration/build_tests.rs"]
mod build_tests;

#[path = "integration/dacpac_tests.rs"]
mod dacpac_tests;

#[path = "integration/dacpac_compatibility_tests.rs"]
mod dacpac_compatibility_tests;
