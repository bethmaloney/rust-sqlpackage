//! Unit tests for T-SQL parser
//!
//! These tests are converted from DacFx test patterns to verify
//! rust-sqlpackage's T-SQL parsing capabilities.
//!
//! The tests are organized into the following modules:
//! - batch_tests: Tests for GO batch splitting
//! - table_tests: CREATE TABLE parsing tests
//! - view_tests: CREATE VIEW parsing tests
//! - index_tests: CREATE INDEX parsing tests
//! - procedure_tests: CREATE PROCEDURE parsing tests
//! - function_tests: CREATE FUNCTION parsing tests
//! - alter_tests: ALTER statement parsing tests
//! - drop_tests: DROP statement parsing tests

mod alter_tests;
mod batch_tests;
mod drop_tests;
mod execute_as_tests;
mod function_tests;
mod graph_table_tests;
mod index_tests;
mod procedure_tests;
mod table_tests;
mod view_tests;
