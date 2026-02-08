//! Token-based table type definition parsing for T-SQL
//!
//! This module provides token-based parsing for table type definitions, replacing
//! the previous regex-based approach. Part of Phase 15.3 of the implementation plan.
//! Refactored in Phase 27 to use base TokenParser.
//!
//! ## Supported Syntax
//!
//! ```sql
//! CREATE TYPE [schema].[name] AS TABLE (
//!     [Col1] INT NOT NULL,
//!     [Col2] NVARCHAR(50) DEFAULT 'value',
//!     PRIMARY KEY CLUSTERED ([Col1]),
//!     UNIQUE NONCLUSTERED ([Col2]),
//!     CHECK ([Col1] > 0),
//!     INDEX [IX_Name] NONCLUSTERED ([Col2])
//! )
//! ```

use crate::parser::column_parser::parse_column_definition_tokens;
use crate::parser::tsql_parser::{
    ExtractedConstraintColumn, ExtractedTableTypeColumn, ExtractedTableTypeConstraint,
};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan};

use super::token_parser_base::TokenParser;

/// Result of parsing a table type definition using tokens
#[derive(Debug, Clone, Default)]
pub struct TokenParsedTableType {
    /// Schema name (defaults to "dbo" if not specified)
    pub schema: String,
    /// Type name
    pub name: String,
    /// Columns defined in the table type
    pub columns: Vec<ExtractedTableTypeColumn>,
    /// Constraints defined in the table type
    pub constraints: Vec<ExtractedTableTypeConstraint>,
}

/// Token-based table type definition parser
pub struct TableTypeTokenParser {
    base: TokenParser,
}

impl TableTypeTokenParser {
    /// Create a new parser for a table type definition string
    pub fn new(sql: &str) -> Option<Self> {
        Some(Self {
            base: TokenParser::new(sql)?,
        })
    }

    /// Create a new parser from pre-tokenized tokens (Phase 76)
    pub fn from_tokens(tokens: Vec<TokenWithSpan>) -> Self {
        Self {
            base: TokenParser::from_tokens(tokens),
        }
    }

    /// Parse CREATE TYPE AS TABLE and return table type info
    pub fn parse_create_table_type(&mut self) -> Option<TokenParsedTableType> {
        self.base.skip_whitespace();

        // Expect CREATE keyword
        if !self.base.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect TYPE keyword
        if !self.base.check_keyword(Keyword::TYPE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse type name (schema-qualified)
        let (schema, name) = self.base.parse_schema_qualified_name()?;
        self.base.skip_whitespace();

        // Expect AS keyword
        if !self.base.check_keyword(Keyword::AS) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect TABLE keyword
        if !self.base.check_keyword(Keyword::TABLE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect opening parenthesis
        if !self.base.check_token(&Token::LParen) {
            return None;
        }
        self.base.advance();

        // Parse columns and constraints
        let (columns, constraints) = self.parse_table_body();

        Some(TokenParsedTableType {
            schema,
            name,
            columns,
            constraints,
        })
    }

    /// Parse the table body (columns and constraints)
    fn parse_table_body(
        &mut self,
    ) -> (
        Vec<ExtractedTableTypeColumn>,
        Vec<ExtractedTableTypeConstraint>,
    ) {
        let mut columns = Vec::new();
        let mut constraints = Vec::new();

        loop {
            self.base.skip_whitespace();

            // Check for end of table body
            if self.base.is_at_end() || self.base.check_token(&Token::RParen) {
                break;
            }

            // Skip comma if present
            if self.base.check_token(&Token::Comma) {
                self.base.advance();
                self.base.skip_whitespace();
                continue;
            }

            // Try to parse a constraint first (PRIMARY KEY, UNIQUE, CHECK, INDEX)
            if let Some(constraint) = self.try_parse_constraint() {
                constraints.push(constraint);
                continue;
            }

            // Otherwise, parse as a column definition
            if let Some(column) = self.parse_column_definition() {
                columns.push(column);
            } else {
                // Skip unknown tokens to avoid infinite loop
                self.base.advance();
            }
        }

        (columns, constraints)
    }

    /// Try to parse a table-level constraint (PRIMARY KEY, UNIQUE, CHECK, INDEX)
    fn try_parse_constraint(&mut self) -> Option<ExtractedTableTypeConstraint> {
        let saved_pos = self.base.pos();

        // Check for PRIMARY KEY
        if self.base.check_keyword(Keyword::PRIMARY) {
            self.base.advance();
            self.base.skip_whitespace();

            if self.base.check_keyword(Keyword::KEY) {
                self.base.advance();
                self.base.skip_whitespace();

                // Parse CLUSTERED/NONCLUSTERED
                let is_clustered = self.parse_clustered_option();
                self.base.skip_whitespace();

                // Parse column list
                let columns = self.parse_constraint_columns();

                return Some(ExtractedTableTypeConstraint::PrimaryKey {
                    columns,
                    is_clustered,
                });
            }
            // Not PRIMARY KEY, restore position
            self.base.set_pos(saved_pos);
        }

        // Check for UNIQUE
        if self.base.check_keyword(Keyword::UNIQUE) {
            self.base.advance();
            self.base.skip_whitespace();

            // Parse CLUSTERED/NONCLUSTERED
            let is_clustered = self.parse_clustered_option();
            self.base.skip_whitespace();

            // Parse column list
            let columns = self.parse_constraint_columns();

            return Some(ExtractedTableTypeConstraint::Unique {
                columns,
                is_clustered,
            });
        }

        // Check for CHECK
        if self.base.check_keyword(Keyword::CHECK) {
            self.base.advance();
            self.base.skip_whitespace();

            // Parse expression in parentheses
            if let Some(expression) = self.parse_parenthesized_expression() {
                return Some(ExtractedTableTypeConstraint::Check { expression });
            }
            self.base.set_pos(saved_pos);
        }

        // Check for INDEX
        if self.base.check_keyword(Keyword::INDEX) {
            self.base.advance();
            self.base.skip_whitespace();

            // Parse index name
            let name = self.base.parse_identifier().unwrap_or_default();
            self.base.skip_whitespace();

            // Parse UNIQUE (optional)
            let is_unique = if self.base.check_keyword(Keyword::UNIQUE) {
                self.base.advance();
                self.base.skip_whitespace();
                true
            } else {
                false
            };

            // Parse CLUSTERED/NONCLUSTERED
            let is_clustered = self.parse_clustered_option();
            self.base.skip_whitespace();

            // Parse column list (simple names only for indexes)
            let columns = self.parse_simple_column_list();

            return Some(ExtractedTableTypeConstraint::Index {
                name,
                columns,
                is_unique,
                is_clustered,
            });
        }

        None
    }

    /// Parse CLUSTERED/NONCLUSTERED option (returns true if clustered, false otherwise)
    fn parse_clustered_option(&mut self) -> bool {
        if self.base.check_word_ci("CLUSTERED") {
            self.base.advance();
            true
        } else if self.base.check_word_ci("NONCLUSTERED") {
            self.base.advance();
            false
        } else {
            // Default: PRIMARY KEY is clustered by default, others are nonclustered
            true
        }
    }

    /// Parse a column list for constraints with ASC/DESC info
    fn parse_constraint_columns(&mut self) -> Vec<ExtractedConstraintColumn> {
        let mut columns = Vec::new();

        if !self.base.check_token(&Token::LParen) {
            return columns;
        }
        self.base.advance();

        loop {
            self.base.skip_whitespace();

            // Check for end
            if self.base.is_at_end() || self.base.check_token(&Token::RParen) {
                self.base.advance(); // consume )
                break;
            }

            // Skip comma
            if self.base.check_token(&Token::Comma) {
                self.base.advance();
                continue;
            }

            // Parse column name
            if let Some(col_name) = self.base.parse_identifier() {
                self.base.skip_whitespace();

                // Check for ASC/DESC
                let descending = if self.base.check_keyword(Keyword::DESC) {
                    self.base.advance();
                    true
                } else if self.base.check_keyword(Keyword::ASC) {
                    self.base.advance();
                    false
                } else {
                    false
                };

                columns.push(ExtractedConstraintColumn {
                    name: col_name,
                    descending,
                });
            } else {
                break;
            }
        }

        columns
    }

    /// Parse a simple column list (just names, no ASC/DESC)
    fn parse_simple_column_list(&mut self) -> Vec<String> {
        let mut columns = Vec::new();

        if !self.base.check_token(&Token::LParen) {
            return columns;
        }
        self.base.advance();

        loop {
            self.base.skip_whitespace();

            // Check for end
            if self.base.is_at_end() || self.base.check_token(&Token::RParen) {
                self.base.advance(); // consume )
                break;
            }

            // Skip comma
            if self.base.check_token(&Token::Comma) {
                self.base.advance();
                continue;
            }

            // Parse column name
            if let Some(col_name) = self.base.parse_identifier() {
                self.base.skip_whitespace();
                // Skip ASC/DESC if present
                if self.base.check_keyword(Keyword::ASC) || self.base.check_keyword(Keyword::DESC) {
                    self.base.advance();
                }
                columns.push(col_name);
            } else {
                break;
            }
        }

        columns
    }

    /// Parse a column definition and return as ExtractedTableTypeColumn
    fn parse_column_definition(&mut self) -> Option<ExtractedTableTypeColumn> {
        // Capture the column definition text from current position to next comma or closing paren
        let col_text = self.capture_column_text();

        if col_text.is_empty() {
            return None;
        }

        // Use the existing token-based column parser
        let parsed = parse_column_definition_tokens(&col_text)?;

        // Skip computed columns - table types don't support them
        if parsed.computed_expression.is_some() {
            return None;
        }

        // Only return if we have valid column name and data type
        if parsed.name.is_empty() || parsed.data_type.is_empty() {
            return None;
        }

        Some(ExtractedTableTypeColumn {
            name: parsed.name,
            data_type: parsed.data_type,
            nullability: parsed.nullability,
            default_value: parsed.default_value,
        })
    }

    /// Capture column definition text until next comma, closing paren, or constraint keyword at depth 0
    fn capture_column_text(&mut self) -> String {
        let mut result = String::new();
        let mut depth = 0;

        while !self.base.is_at_end() {
            let token = match self.base.current_token() {
                Some(t) => t.clone(),
                None => break,
            };

            match &token.token {
                Token::LParen => {
                    depth += 1;
                    result.push_str(&TokenParser::token_to_string(&token.token));
                    self.base.advance();
                }
                Token::RParen => {
                    if depth == 0 {
                        // End of table body
                        break;
                    }
                    depth -= 1;
                    result.push_str(&TokenParser::token_to_string(&token.token));
                    self.base.advance();
                }
                Token::Comma if depth == 0 => {
                    // End of this column definition
                    break;
                }
                // Stop at table-level constraint keywords at depth 0 (comma-less constraint syntax)
                Token::Word(w) if depth == 0 => {
                    // Check for constraint keywords that start table-level constraints
                    match w.keyword {
                        Keyword::PRIMARY | Keyword::UNIQUE | Keyword::CHECK | Keyword::INDEX => {
                            // Stop capturing - this is the start of a table-level constraint
                            break;
                        }
                        _ => {
                            result.push_str(&TokenParser::token_to_string(&token.token));
                            self.base.advance();
                        }
                    }
                }
                _ => {
                    result.push_str(&TokenParser::token_to_string(&token.token));
                    self.base.advance();
                }
            }
        }

        result.trim().to_string()
    }

    /// Parse a parenthesized expression (for CHECK constraints)
    fn parse_parenthesized_expression(&mut self) -> Option<String> {
        if !self.base.check_token(&Token::LParen) {
            return None;
        }
        self.base.advance(); // consume (

        let mut depth = 1;
        let mut content = String::new();

        while !self.base.is_at_end() && depth > 0 {
            if let Some(token) = self.base.current_token() {
                match &token.token {
                    Token::LParen => {
                        depth += 1;
                        content.push('(');
                    }
                    Token::RParen => {
                        depth -= 1;
                        if depth > 0 {
                            content.push(')');
                        }
                    }
                    _ => {
                        content.push_str(&TokenParser::token_to_string(&token.token));
                    }
                }
                self.base.advance();
            }
        }

        Some(content.trim().to_string())
    }
}

/// Parse CREATE TYPE AS TABLE using tokens and return table type info
///
/// This function replaces the regex-based `extract_type_name` and
/// `extract_table_type_structure` functions.
///
/// Supports:
/// - CREATE TYPE [dbo].[TypeName] AS TABLE (columns, constraints)
/// - CREATE TYPE dbo.TypeName AS TABLE (...)
/// - CREATE TYPE TypeName AS TABLE (...)
pub fn parse_create_table_type_tokens(sql: &str) -> Option<TokenParsedTableType> {
    let mut parser = TableTypeTokenParser::new(sql)?;
    parser.parse_create_table_type()
}

/// Parse CREATE TYPE AS TABLE from pre-tokenized tokens (Phase 76)
pub fn parse_create_table_type_tokens_with_tokens(
    tokens: Vec<TokenWithSpan>,
) -> Option<TokenParsedTableType> {
    let mut parser = TableTypeTokenParser::from_tokens(tokens);
    parser.parse_create_table_type()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Basic parsing tests
    // ========================================================================

    #[test]
    fn test_simple_table_type() {
        let sql = "CREATE TYPE [dbo].[SimpleType] AS TABLE ([Id] INT NOT NULL)";
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "SimpleType");
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0].name, "Id");
        assert_eq!(result.columns[0].data_type, "INT");
        assert_eq!(result.columns[0].nullability, Some(false));
    }

    #[test]
    fn test_table_type_without_schema() {
        let sql = "CREATE TYPE [MyType] AS TABLE ([Value] NVARCHAR(50))";
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MyType");
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0].name, "Value");
        assert_eq!(result.columns[0].data_type, "NVARCHAR(50)");
    }

    #[test]
    fn test_table_type_unbracketed_names() {
        let sql = "CREATE TYPE dbo.TestType AS TABLE (Col1 INT)";
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TestType");
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0].name, "Col1");
    }

    // ========================================================================
    // Multiple columns tests
    // ========================================================================

    #[test]
    fn test_table_type_multiple_columns() {
        let sql = r#"CREATE TYPE [dbo].[OrderItemsType] AS TABLE (
            [ProductId] INT NOT NULL,
            [Quantity] INT NOT NULL,
            [UnitPrice] DECIMAL(18, 2) NOT NULL
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "OrderItemsType");
        assert_eq!(result.columns.len(), 3);

        assert_eq!(result.columns[0].name, "ProductId");
        assert_eq!(result.columns[0].data_type, "INT");

        assert_eq!(result.columns[1].name, "Quantity");
        assert_eq!(result.columns[1].data_type, "INT");

        assert_eq!(result.columns[2].name, "UnitPrice");
        assert_eq!(result.columns[2].data_type, "DECIMAL(18, 2)");
    }

    #[test]
    fn test_table_type_with_default_value() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithDefault] AS TABLE (
            [Name] NVARCHAR(50) NOT NULL,
            [Value] INT NOT NULL DEFAULT 0
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.columns[1].name, "Value");
        assert_eq!(result.columns[1].default_value, Some("0".to_string()));
    }

    // ========================================================================
    // PRIMARY KEY constraint tests
    // ========================================================================

    #[test]
    fn test_table_type_with_primary_key() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithPK] AS TABLE (
            [Id] INT NOT NULL,
            PRIMARY KEY CLUSTERED ([Id])
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.constraints.len(), 1);

        match &result.constraints[0] {
            ExtractedTableTypeConstraint::PrimaryKey {
                columns,
                is_clustered,
            } => {
                assert!(is_clustered);
                assert_eq!(columns.len(), 1);
                assert_eq!(columns[0].name, "Id");
                assert!(!columns[0].descending);
            }
            _ => panic!("Expected PrimaryKey constraint"),
        }
    }

    #[test]
    fn test_table_type_with_commaless_primary_key() {
        // Test comma-less constraint syntax where no comma precedes the PRIMARY KEY
        let sql = r#"CREATE TYPE [dbo].[TableTypeWithCommalessPK] AS TABLE
(
    [ElementId] INT NOT NULL,
    [SequenceNo] INT NULL,
    [ParentId] INT,
    [Name] NVARCHAR(200),
    [Value] NVARCHAR(MAX) NOT NULL
    PRIMARY KEY ([ElementId])
)"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TableTypeWithCommalessPK");
        assert_eq!(result.columns.len(), 5);
        assert_eq!(result.constraints.len(), 1);

        // Verify columns
        assert_eq!(result.columns[0].name, "ElementId");
        assert_eq!(result.columns[4].name, "Value");

        // Verify PRIMARY KEY constraint
        match &result.constraints[0] {
            ExtractedTableTypeConstraint::PrimaryKey {
                columns,
                is_clustered,
            } => {
                assert!(is_clustered); // Default is clustered
                assert_eq!(columns.len(), 1);
                assert_eq!(columns[0].name, "ElementId");
            }
            _ => panic!("Expected PrimaryKey constraint"),
        }
    }

    #[test]
    fn test_table_type_with_composite_primary_key() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithCompositePK] AS TABLE (
            [Col1] INT NOT NULL,
            [Col2] INT NOT NULL,
            PRIMARY KEY CLUSTERED ([Col1], [Col2] DESC)
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.constraints.len(), 1);

        match &result.constraints[0] {
            ExtractedTableTypeConstraint::PrimaryKey {
                columns,
                is_clustered,
            } => {
                assert!(is_clustered);
                assert_eq!(columns.len(), 2);
                assert_eq!(columns[0].name, "Col1");
                assert!(!columns[0].descending);
                assert_eq!(columns[1].name, "Col2");
                assert!(columns[1].descending);
            }
            _ => panic!("Expected PrimaryKey constraint"),
        }
    }

    #[test]
    fn test_table_type_with_nonclustered_primary_key() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithNCPK] AS TABLE (
            [Id] INT NOT NULL,
            PRIMARY KEY NONCLUSTERED ([Id])
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        match &result.constraints[0] {
            ExtractedTableTypeConstraint::PrimaryKey {
                columns: _,
                is_clustered,
            } => {
                assert!(!is_clustered);
            }
            _ => panic!("Expected PrimaryKey constraint"),
        }
    }

    // ========================================================================
    // UNIQUE constraint tests
    // ========================================================================

    #[test]
    fn test_table_type_with_unique() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithUnique] AS TABLE (
            [Id] INT NOT NULL,
            [Code] NVARCHAR(10) NOT NULL,
            UNIQUE NONCLUSTERED ([Code])
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.constraints.len(), 1);

        match &result.constraints[0] {
            ExtractedTableTypeConstraint::Unique {
                columns,
                is_clustered,
            } => {
                assert!(!is_clustered);
                assert_eq!(columns.len(), 1);
                assert_eq!(columns[0].name, "Code");
            }
            _ => panic!("Expected Unique constraint"),
        }
    }

    #[test]
    fn test_table_type_with_unique_clustered() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithUniqueC] AS TABLE (
            [Id] INT NOT NULL,
            UNIQUE CLUSTERED ([Id])
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        match &result.constraints[0] {
            ExtractedTableTypeConstraint::Unique {
                columns: _,
                is_clustered,
            } => {
                assert!(is_clustered);
            }
            _ => panic!("Expected Unique constraint"),
        }
    }

    // ========================================================================
    // CHECK constraint tests
    // ========================================================================

    #[test]
    fn test_table_type_with_check() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithCheck] AS TABLE (
            [Value] DECIMAL(5, 2) NOT NULL,
            CHECK ([Value] >= 0 AND [Value] <= 100)
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.constraints.len(), 1);

        match &result.constraints[0] {
            ExtractedTableTypeConstraint::Check { expression } => {
                assert!(expression.contains("[Value] >= 0"));
                assert!(expression.contains("[Value] <= 100"));
            }
            _ => panic!("Expected Check constraint"),
        }
    }

    // ========================================================================
    // INDEX tests
    // ========================================================================

    #[test]
    fn test_table_type_with_index() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithIndex] AS TABLE (
            [Id] INT NOT NULL,
            [SortOrder] INT NOT NULL,
            INDEX [IX_SortOrder] NONCLUSTERED ([SortOrder])
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.constraints.len(), 1);

        match &result.constraints[0] {
            ExtractedTableTypeConstraint::Index {
                name,
                columns,
                is_unique,
                is_clustered,
            } => {
                assert_eq!(name, "IX_SortOrder");
                assert!(!is_unique);
                assert!(!is_clustered);
                assert_eq!(columns.len(), 1);
                assert_eq!(columns[0], "SortOrder");
            }
            _ => panic!("Expected Index constraint"),
        }
    }

    #[test]
    fn test_table_type_with_unique_index() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithUniqueIndex] AS TABLE (
            [Id] INT NOT NULL,
            [Code] NVARCHAR(10) NOT NULL,
            INDEX [IX_Code] UNIQUE NONCLUSTERED ([Code])
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        match &result.constraints[0] {
            ExtractedTableTypeConstraint::Index {
                name,
                columns: _,
                is_unique,
                is_clustered,
            } => {
                assert_eq!(name, "IX_Code");
                assert!(is_unique);
                assert!(!is_clustered);
            }
            _ => panic!("Expected Index constraint"),
        }
    }

    // ========================================================================
    // Complex/combined tests
    // ========================================================================

    #[test]
    fn test_order_items_type() {
        // From test fixture
        let sql = r#"CREATE TYPE [dbo].[OrderItemsType] AS TABLE (
            [ProductId] INT NOT NULL,
            [Quantity] INT NOT NULL,
            [UnitPrice] DECIMAL(18, 2) NOT NULL,
            [Discount] DECIMAL(5, 2) NOT NULL DEFAULT 0,
            PRIMARY KEY CLUSTERED ([ProductId])
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "OrderItemsType");
        assert_eq!(result.columns.len(), 4);
        assert_eq!(result.constraints.len(), 1);

        // Check column with default
        assert_eq!(result.columns[3].name, "Discount");
        assert_eq!(result.columns[3].default_value, Some("0".to_string()));

        // Check PK
        match &result.constraints[0] {
            ExtractedTableTypeConstraint::PrimaryKey {
                columns,
                is_clustered,
            } => {
                assert!(is_clustered);
                assert_eq!(columns[0].name, "ProductId");
            }
            _ => panic!("Expected PrimaryKey constraint"),
        }
    }

    #[test]
    fn test_id_list_type() {
        // From test fixture
        let sql = r#"CREATE TYPE [dbo].[IdListType] AS TABLE (
            [Id] INT NOT NULL,
            [SortOrder] INT NOT NULL DEFAULT 0,
            INDEX [IX_IdList_SortOrder] NONCLUSTERED ([SortOrder])
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.name, "IdListType");
        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.constraints.len(), 1);

        // Check default
        assert_eq!(result.columns[1].default_value, Some("0".to_string()));

        // Check index
        match &result.constraints[0] {
            ExtractedTableTypeConstraint::Index {
                name,
                columns,
                is_unique,
                is_clustered,
            } => {
                assert_eq!(name, "IX_IdList_SortOrder");
                assert!(!is_unique);
                assert!(!is_clustered);
                assert_eq!(columns[0], "SortOrder");
            }
            _ => panic!("Expected Index constraint"),
        }
    }

    #[test]
    fn test_percentage_type() {
        // From test fixture
        let sql = r#"CREATE TYPE [dbo].[PercentageType] AS TABLE (
            [Name] NVARCHAR(50) NOT NULL,
            [Value] DECIMAL(5, 2) NOT NULL,
            CHECK ([Value] >= 0 AND [Value] <= 100)
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.name, "PercentageType");
        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.constraints.len(), 1);

        // Check constraint
        match &result.constraints[0] {
            ExtractedTableTypeConstraint::Check { expression } => {
                assert!(expression.contains(">= 0"));
                assert!(expression.contains("<= 100"));
            }
            _ => panic!("Expected Check constraint"),
        }
    }

    #[test]
    fn test_table_type_with_multiple_constraints() {
        let sql = r#"CREATE TYPE [dbo].[ComplexType] AS TABLE (
            [Id] INT NOT NULL,
            [Code] NVARCHAR(10) NOT NULL,
            [Value] DECIMAL(10, 2) NOT NULL,
            PRIMARY KEY CLUSTERED ([Id]),
            UNIQUE NONCLUSTERED ([Code]),
            CHECK ([Value] > 0)
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.columns.len(), 3);
        assert_eq!(result.constraints.len(), 3);

        // Verify constraint types
        let mut has_pk = false;
        let mut has_unique = false;
        let mut has_check = false;

        for constraint in &result.constraints {
            match constraint {
                ExtractedTableTypeConstraint::PrimaryKey { .. } => has_pk = true,
                ExtractedTableTypeConstraint::Unique { .. } => has_unique = true,
                ExtractedTableTypeConstraint::Check { .. } => has_check = true,
                _ => {}
            }
        }

        assert!(has_pk, "Should have PrimaryKey");
        assert!(has_unique, "Should have Unique");
        assert!(has_check, "Should have Check");
    }

    // ========================================================================
    // Edge case tests
    // ========================================================================

    #[test]
    fn test_table_type_nullable_column() {
        let sql = r#"CREATE TYPE [dbo].[TypeWithNullable] AS TABLE (
            [Id] INT NOT NULL,
            [Description] NVARCHAR(MAX) NULL
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.columns[0].nullability, Some(false));
        assert_eq!(result.columns[1].nullability, Some(true));
    }

    #[test]
    fn test_table_type_implicit_nullability() {
        let sql = r#"CREATE TYPE [dbo].[TypeImplicitNull] AS TABLE (
            [Value] INT
        )"#;
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0].nullability, None);
    }

    #[test]
    fn test_invalid_sql_returns_none() {
        assert!(parse_create_table_type_tokens("SELECT * FROM table").is_none());
        assert!(parse_create_table_type_tokens("CREATE TABLE foo (x INT)").is_none());
        assert!(parse_create_table_type_tokens("CREATE TYPE foo FROM INT").is_none());
    }

    #[test]
    fn test_empty_table_type() {
        // Edge case: no columns
        let sql = "CREATE TYPE [dbo].[EmptyType] AS TABLE ()";
        let result = parse_create_table_type_tokens(sql).unwrap();

        assert_eq!(result.name, "EmptyType");
        assert_eq!(result.columns.len(), 0);
        assert_eq!(result.constraints.len(), 0);
    }
}
