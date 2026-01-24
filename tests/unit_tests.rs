//! Unit tests for rust-sqlpackage
//!
//! This file serves as the entry point for all unit tests.

#[path = "unit/parser/mod.rs"]
mod parser;

#[path = "unit/sqlproj_tests.rs"]
mod sqlproj_tests;

#[path = "unit/model/mod.rs"]
mod model;

#[path = "unit/xml_tests.rs"]
mod xml_tests;

#[path = "unit/dacpac_comparison_tests.rs"]
mod dacpac_comparison_tests;
