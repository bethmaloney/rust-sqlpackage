//! Unit tests for rust-sqlpackage
//!
//! This file serves as the entry point for all unit tests.

#[path = "unit/parser_tests.rs"]
mod parser_tests;

#[path = "unit/sqlproj_tests.rs"]
mod sqlproj_tests;

#[path = "unit/model_tests.rs"]
mod model_tests;

#[path = "unit/xml_tests.rs"]
mod xml_tests;

#[path = "unit/dacpac_comparison_tests.rs"]
mod dacpac_comparison_tests;
