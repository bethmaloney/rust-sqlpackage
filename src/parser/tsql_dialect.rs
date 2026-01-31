//! Extended T-SQL dialect for sqlparser-rs
//!
//! This module provides a custom dialect that extends MsSqlDialect to handle
//! T-SQL-specific syntax that the base dialect doesn't support. It serves as
//! infrastructure for Phase 15 of the implementation plan.
//!
//! Currently, this dialect delegates all parsing to MsSqlDialect while providing
//! a foundation for future custom statement parsing to replace regex fallbacks.

use std::any::TypeId;

use sqlparser::ast::Statement;
use sqlparser::dialect::{Dialect, MsSqlDialect};
use sqlparser::parser::{Parser, ParserError};

/// Extended T-SQL dialect with support for T-SQL-specific syntax.
///
/// This dialect wraps `MsSqlDialect` and can be extended to handle:
/// - CREATE/ALTER PROCEDURE with full body parsing
/// - CREATE/ALTER FUNCTION (scalar, table-valued, inline)
/// - CREATE TRIGGER with event specifications
/// - CREATE TYPE AS TABLE
/// - CREATE SEQUENCE with T-SQL options
/// - And other T-SQL-specific constructs
///
/// # Example
///
/// ```
/// use sqlparser::parser::Parser;
/// use rust_sqlpackage::parser::ExtendedTsqlDialect;
///
/// let dialect = ExtendedTsqlDialect::new();
/// let statements = Parser::parse_sql(&dialect, "SELECT 1").unwrap();
/// ```
#[derive(Debug)]
pub struct ExtendedTsqlDialect {
    /// The base MsSqlDialect to delegate to
    base: MsSqlDialect,
}

impl Default for ExtendedTsqlDialect {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtendedTsqlDialect {
    /// Create a new ExtendedTsqlDialect instance
    pub fn new() -> Self {
        Self {
            base: MsSqlDialect {},
        }
    }
}

impl Dialect for ExtendedTsqlDialect {
    // ==========================================================================
    // Dialect identity - report as MsSqlDialect for dialect_of!() checks
    //
    // This is critical: sqlparser uses dialect_of!(self is MsSqlDialect) checks
    // internally to enable T-SQL specific parsing (like IDENTITY columns).
    // By returning MsSqlDialect's TypeId, our dialect passes those checks.
    // ==========================================================================

    fn dialect(&self) -> TypeId {
        TypeId::of::<MsSqlDialect>()
    }

    // ==========================================================================
    // Required identifier methods - delegate to MsSqlDialect
    // ==========================================================================

    fn is_identifier_start(&self, ch: char) -> bool {
        self.base.is_identifier_start(ch)
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        self.base.is_identifier_part(ch)
    }

    // ==========================================================================
    // Delimited identifier handling - delegate to MsSqlDialect
    // ==========================================================================

    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        self.base.is_delimited_identifier_start(ch)
    }

    // ==========================================================================
    // Custom statement parsing
    //
    // This is the main extension point for handling T-SQL-specific statements.
    // Currently returns None to use default parsing, but will be extended to
    // handle procedures, functions, triggers, etc.
    // ==========================================================================

    fn parse_statement(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        // Phase 15.1: Infrastructure - just delegate to base dialect for now
        // Future phases will add custom parsing for:
        // - CREATE/ALTER PROCEDURE (Phase 15.3)
        // - CREATE/ALTER FUNCTION (Phase 15.3)
        // - CREATE TRIGGER (Phase 15.3)
        // - CREATE TYPE AS TABLE (Phase 15.3)
        // - CREATE SEQUENCE (Phase 15.3)
        // - And more...
        self.base.parse_statement(parser)
    }

    // ==========================================================================
    // Feature flags - delegate all to MsSqlDialect
    // ==========================================================================

    fn convert_type_before_value(&self) -> bool {
        self.base.convert_type_before_value()
    }

    fn supports_connect_by(&self) -> bool {
        self.base.supports_connect_by()
    }

    fn supports_eq_alias_assignment(&self) -> bool {
        self.base.supports_eq_alias_assignment()
    }

    fn supports_try_convert(&self) -> bool {
        self.base.supports_try_convert()
    }

    fn supports_boolean_literals(&self) -> bool {
        self.base.supports_boolean_literals()
    }

    fn supports_methods(&self) -> bool {
        self.base.supports_methods()
    }

    fn supports_named_fn_args_with_colon_operator(&self) -> bool {
        self.base.supports_named_fn_args_with_colon_operator()
    }

    fn supports_named_fn_args_with_expr_name(&self) -> bool {
        self.base.supports_named_fn_args_with_expr_name()
    }

    fn supports_named_fn_args_with_rarrow_operator(&self) -> bool {
        self.base.supports_named_fn_args_with_rarrow_operator()
    }

    fn supports_start_transaction_modifier(&self) -> bool {
        self.base.supports_start_transaction_modifier()
    }

    fn supports_end_transaction_modifier(&self) -> bool {
        self.base.supports_end_transaction_modifier()
    }

    fn supports_set_stmt_without_operator(&self) -> bool {
        self.base.supports_set_stmt_without_operator()
    }

    fn supports_timestamp_versioning(&self) -> bool {
        self.base.supports_timestamp_versioning()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::parser::Parser;

    /// Test that the dialect can parse basic SELECT statements
    #[test]
    fn test_parse_select() {
        let dialect = ExtendedTsqlDialect::new();
        let result = Parser::parse_sql(&dialect, "SELECT 1");
        assert!(result.is_ok());
        let stmts = result.unwrap();
        assert_eq!(stmts.len(), 1);
    }

    /// Test that the dialect can parse SELECT with schema-qualified table
    #[test]
    fn test_parse_select_from_table() {
        let dialect = ExtendedTsqlDialect::new();
        let result = Parser::parse_sql(&dialect, "SELECT * FROM [dbo].[Users]");
        assert!(result.is_ok());
    }

    /// Test that the dialect handles bracket-quoted identifiers
    #[test]
    fn test_bracket_identifiers() {
        let dialect = ExtendedTsqlDialect::new();
        let result = Parser::parse_sql(&dialect, "SELECT [Column With Spaces] FROM [Table]");
        assert!(result.is_ok());
    }

    /// Test identifier start characters
    #[test]
    fn test_identifier_start() {
        let dialect = ExtendedTsqlDialect::new();
        assert!(dialect.is_identifier_start('a'));
        assert!(dialect.is_identifier_start('A'));
        assert!(dialect.is_identifier_start('_'));
        assert!(dialect.is_identifier_start('#')); // Temp tables
        assert!(dialect.is_identifier_start('@')); // Variables
        assert!(!dialect.is_identifier_start('0'));
        assert!(!dialect.is_identifier_start('-'));
    }

    /// Test identifier part characters
    #[test]
    fn test_identifier_part() {
        let dialect = ExtendedTsqlDialect::new();
        assert!(dialect.is_identifier_part('a'));
        assert!(dialect.is_identifier_part('A'));
        assert!(dialect.is_identifier_part('0'));
        assert!(dialect.is_identifier_part('_'));
        assert!(dialect.is_identifier_part('#'));
        assert!(dialect.is_identifier_part('@'));
        assert!(dialect.is_identifier_part('$'));
        assert!(!dialect.is_identifier_part('-'));
    }

    /// Test that delimited identifiers work
    #[test]
    fn test_delimited_identifier_start() {
        let dialect = ExtendedTsqlDialect::new();
        assert!(dialect.is_delimited_identifier_start('['));
        assert!(dialect.is_delimited_identifier_start('"'));
        assert!(!dialect.is_delimited_identifier_start('`'));
    }

    /// Test feature flags match MsSqlDialect
    #[test]
    fn test_feature_flags() {
        let dialect = ExtendedTsqlDialect::new();

        // These should match MsSqlDialect behavior
        assert!(dialect.convert_type_before_value()); // SQL Server CONVERT(type, value)
        assert!(dialect.supports_connect_by());
        assert!(dialect.supports_try_convert());
        assert!(!dialect.supports_boolean_literals()); // No true/false literals in T-SQL
        assert!(dialect.supports_methods()); // .value(), .modify() etc.
    }

    /// Test parsing CREATE TABLE (should work via delegation)
    #[test]
    fn test_parse_create_table() {
        let dialect = ExtendedTsqlDialect::new();
        let sql = r#"
            CREATE TABLE [dbo].[Users] (
                [Id] INT NOT NULL PRIMARY KEY,
                [Name] NVARCHAR(100) NULL
            )
        "#;
        let result = Parser::parse_sql(&dialect, sql);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
    }

    /// Test that CREATE PROCEDURE fails gracefully (expected - will be fixed in Phase 15.3)
    #[test]
    fn test_create_procedure_needs_fallback() {
        let dialect = ExtendedTsqlDialect::new();
        let sql = "CREATE PROCEDURE [dbo].[GetUsers] AS SELECT * FROM Users";
        // This should fail with current dialect - procedures need fallback parsing
        // This test documents the current behavior that Phase 15.3 will improve
        let result = Parser::parse_sql(&dialect, sql);
        // Note: We don't assert failure here because we want to know if sqlparser
        // starts supporting this natively in future versions
        if let Err(err) = result {
            // Expected behavior - procedure parsing not yet supported
            assert!(err.to_string().contains("Expected:"));
        }
    }

    /// Test inline UNIQUE constraint parsing (mirrors inline_constraints fixture)
    #[test]
    fn test_parse_inline_unique() {
        let sql = r#"
            CREATE TABLE [dbo].[Customer] (
                [Id] INT NOT NULL IDENTITY(1,1),
                [Email] NVARCHAR(255) NOT NULL UNIQUE,
                [Phone] NVARCHAR(20) NULL,
                CONSTRAINT [PK_Customer] PRIMARY KEY ([Id])
            )
        "#;

        // Verify MsSqlDialect works
        let mssql_dialect = MsSqlDialect {};
        let mssql_result = Parser::parse_sql(&mssql_dialect, sql);
        assert!(
            mssql_result.is_ok(),
            "MsSqlDialect failed: {:?}",
            mssql_result.err()
        );

        // Test ExtendedTsqlDialect produces identical results
        let dialect = ExtendedTsqlDialect::new();
        let result = Parser::parse_sql(&dialect, sql);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        // Both should produce equivalent ASTs
        let extended_stmts = result.unwrap();
        let mssql_stmts = mssql_result.unwrap();
        assert_eq!(
            format!("{:?}", extended_stmts),
            format!("{:?}", mssql_stmts),
            "ExtendedTsqlDialect should produce identical AST to MsSqlDialect"
        );
    }

    /// Test that dialect() returns MsSqlDialect's TypeId
    #[test]
    fn test_dialect_typeid() {
        use std::any::TypeId;
        let dialect = ExtendedTsqlDialect::new();
        assert_eq!(
            dialect.dialect(),
            TypeId::of::<MsSqlDialect>(),
            "ExtendedTsqlDialect should report as MsSqlDialect for dialect_of! checks"
        );
    }
}
