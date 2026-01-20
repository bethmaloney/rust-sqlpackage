//! End-to-end tests for rust-sqlpackage
//!
//! These tests verify the complete workflow of building a dacpac
//! and deploying it to a real SQL Server instance.
//!
//! Prerequisites:
//! - SQL Server 2022 running at localhost:1433 with sa/Password1
//! - SqlPackage CLI available in PATH
//!
//! Run with:
//!   cargo test --test e2e_tests -- --ignored

#[path = "common/mod.rs"]
mod common;

#[path = "e2e/deploy_tests.rs"]
mod deploy_tests;
