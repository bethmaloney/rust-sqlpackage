//! Token-based constraint parsing for T-SQL
//!
//! This module provides token-based parsing for constraint definitions, replacing
//! the previous regex-based approach. Part of Phase 15.4 (C1-C4) of the implementation plan.
//!
//! ## Supported Syntax
//!
//! ALTER TABLE constraints:
//! ```sql
//! ALTER TABLE [schema].[table] ADD CONSTRAINT [name] PRIMARY KEY (columns)
//! ALTER TABLE [schema].[table] ADD CONSTRAINT [name] UNIQUE (columns)
//! ALTER TABLE [schema].[table] ADD CONSTRAINT [name] FOREIGN KEY (columns) REFERENCES [table](columns)
//! ALTER TABLE [schema].[table] ADD CONSTRAINT [name] CHECK (expression)
//! ALTER TABLE [schema].[table] WITH CHECK ADD CONSTRAINT [name] ...
//! ALTER TABLE [schema].[table] WITH NOCHECK ADD CONSTRAINT [name] ...
//! ```
//!
//! Table-level constraints:
//! ```sql
//! CONSTRAINT [name] PRIMARY KEY CLUSTERED ([Col1], [Col2] DESC)
//! CONSTRAINT [name] UNIQUE NONCLUSTERED ([Col1])
//! CONSTRAINT [name] FOREIGN KEY ([Col]) REFERENCES [Table]([Col])
//! CONSTRAINT [name] CHECK ([expression])
//! PRIMARY KEY ([Col1])  -- unnamed
//! ```

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

/// Constraint column with sort order
#[derive(Debug, Clone)]
pub struct TokenParsedConstraintColumn {
    /// Column name
    pub name: String,
    /// Whether the column is sorted descending (default is ASC)
    pub descending: bool,
}

/// Parsed constraint result
#[derive(Debug, Clone)]
pub enum TokenParsedConstraint {
    PrimaryKey {
        name: String,
        columns: Vec<TokenParsedConstraintColumn>,
        is_clustered: bool,
    },
    Unique {
        name: String,
        columns: Vec<TokenParsedConstraintColumn>,
        is_clustered: bool,
    },
    ForeignKey {
        name: String,
        columns: Vec<String>,
        referenced_table: String,
        referenced_columns: Vec<String>,
    },
    Check {
        name: String,
        expression: String,
    },
}

/// Result of parsing ALTER TABLE ... ADD CONSTRAINT
#[derive(Debug, Clone)]
pub struct TokenParsedAlterTableConstraint {
    /// Schema of the table (defaults to "dbo" if not specified)
    pub table_schema: String,
    /// Table name
    pub table_name: String,
    /// The constraint being added
    pub constraint: TokenParsedConstraint,
}

/// Token-based constraint parser
pub struct ConstraintTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl ConstraintTokenParser {
    /// Create a new parser for a SQL string
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Parse ALTER TABLE ... ADD CONSTRAINT statement
    pub fn parse_alter_table_add_constraint(&mut self) -> Option<TokenParsedAlterTableConstraint> {
        self.skip_whitespace();

        // Expect ALTER keyword
        if !self.check_keyword(Keyword::ALTER) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect TABLE keyword
        if !self.check_keyword(Keyword::TABLE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse table name (schema-qualified)
        let (table_schema, table_name) = self.parse_schema_qualified_name()?;
        self.skip_whitespace();

        // Skip optional WITH CHECK or WITH NOCHECK
        if self.check_keyword(Keyword::WITH) {
            self.advance();
            self.skip_whitespace();
            // Skip CHECK or NOCHECK
            if self.check_keyword(Keyword::CHECK) || self.check_word_ci("NOCHECK") {
                self.advance();
                self.skip_whitespace();
            }
        }

        // Expect ADD keyword
        if !self.check_keyword(Keyword::ADD) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect CONSTRAINT keyword
        if !self.check_keyword(Keyword::CONSTRAINT) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse constraint name
        let constraint_name = self.parse_identifier()?;
        self.skip_whitespace();

        // Determine constraint type
        let constraint = if self.check_keyword(Keyword::PRIMARY) {
            self.parse_primary_key_constraint(constraint_name)?
        } else if self.check_keyword(Keyword::UNIQUE) {
            self.parse_unique_constraint(constraint_name)?
        } else if self.check_keyword(Keyword::FOREIGN) {
            self.parse_foreign_key_constraint(constraint_name)?
        } else if self.check_keyword(Keyword::CHECK) {
            self.parse_check_constraint(constraint_name)?
        } else {
            return None;
        };

        Some(TokenParsedAlterTableConstraint {
            table_schema,
            table_name,
            constraint,
        })
    }

    /// Parse a table-level constraint definition
    /// This handles both named (CONSTRAINT [name] ...) and unnamed constraints
    pub fn parse_table_constraint(
        &mut self,
        default_table_name: &str,
    ) -> Option<TokenParsedConstraint> {
        self.skip_whitespace();

        // Check for optional CONSTRAINT keyword and name
        let constraint_name = if self.check_keyword(Keyword::CONSTRAINT) {
            self.advance();
            self.skip_whitespace();
            Some(self.parse_identifier()?)
        } else {
            None
        };

        self.skip_whitespace();

        // Determine constraint type
        if self.check_keyword(Keyword::PRIMARY) {
            let default_name =
                constraint_name.unwrap_or_else(|| format!("PK_{}", default_table_name));
            self.parse_primary_key_constraint(default_name)
        } else if self.check_keyword(Keyword::UNIQUE) {
            let default_name =
                constraint_name.unwrap_or_else(|| format!("UQ_{}", default_table_name));
            self.parse_unique_constraint(default_name)
        } else if self.check_keyword(Keyword::FOREIGN) {
            let default_name =
                constraint_name.unwrap_or_else(|| format!("FK_{}", default_table_name));
            self.parse_foreign_key_constraint(default_name)
        } else if self.check_keyword(Keyword::CHECK) {
            let default_name =
                constraint_name.unwrap_or_else(|| format!("CK_{}", default_table_name));
            self.parse_check_constraint(default_name)
        } else {
            None
        }
    }

    /// Parse PRIMARY KEY constraint
    fn parse_primary_key_constraint(&mut self, name: String) -> Option<TokenParsedConstraint> {
        // Expect PRIMARY keyword
        if !self.check_keyword(Keyword::PRIMARY) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect KEY keyword
        if !self.check_keyword(Keyword::KEY) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Check for optional CLUSTERED/NONCLUSTERED
        let mut is_clustered = true; // Default is CLUSTERED for PRIMARY KEY
        if self.check_keyword(Keyword::CLUSTERED) {
            self.advance();
            self.skip_whitespace();
        } else if self.check_word_ci("NONCLUSTERED") {
            is_clustered = false;
            self.advance();
            self.skip_whitespace();
        }

        // Parse column list with sort order
        let columns = self.parse_constraint_column_list()?;

        Some(TokenParsedConstraint::PrimaryKey {
            name,
            columns,
            is_clustered,
        })
    }

    /// Parse UNIQUE constraint
    fn parse_unique_constraint(&mut self, name: String) -> Option<TokenParsedConstraint> {
        // Expect UNIQUE keyword
        if !self.check_keyword(Keyword::UNIQUE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Check for optional CLUSTERED/NONCLUSTERED
        let mut is_clustered = false; // Default is NONCLUSTERED for UNIQUE
        if self.check_keyword(Keyword::CLUSTERED) {
            is_clustered = true;
            self.advance();
            self.skip_whitespace();
        } else if self.check_word_ci("NONCLUSTERED") {
            is_clustered = false;
            self.advance();
            self.skip_whitespace();
        }

        // Parse column list with sort order
        let columns = self.parse_constraint_column_list()?;

        Some(TokenParsedConstraint::Unique {
            name,
            columns,
            is_clustered,
        })
    }

    /// Parse FOREIGN KEY constraint
    fn parse_foreign_key_constraint(&mut self, name: String) -> Option<TokenParsedConstraint> {
        // Expect FOREIGN keyword
        if !self.check_keyword(Keyword::FOREIGN) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect KEY keyword
        if !self.check_keyword(Keyword::KEY) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse FK column list
        let columns = self.parse_simple_column_list()?;
        self.skip_whitespace();

        // Expect REFERENCES keyword
        if !self.check_keyword(Keyword::REFERENCES) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse referenced table (schema-qualified)
        let (ref_schema, ref_table) = self.parse_schema_qualified_name()?;
        let referenced_table = format!("[{}].[{}]", ref_schema, ref_table);
        self.skip_whitespace();

        // Parse referenced columns
        let referenced_columns = self.parse_simple_column_list()?;

        Some(TokenParsedConstraint::ForeignKey {
            name,
            columns,
            referenced_table,
            referenced_columns,
        })
    }

    /// Parse CHECK constraint
    fn parse_check_constraint(&mut self, name: String) -> Option<TokenParsedConstraint> {
        // Expect CHECK keyword
        if !self.check_keyword(Keyword::CHECK) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse expression in parentheses
        let expression = self.parse_parenthesized_expression()?;

        Some(TokenParsedConstraint::Check { name, expression })
    }

    /// Parse a column list for PRIMARY KEY or UNIQUE constraint
    /// Format: ([Col1] [ASC|DESC], [Col2] [ASC|DESC], ...)
    fn parse_constraint_column_list(&mut self) -> Option<Vec<TokenParsedConstraintColumn>> {
        // Expect opening parenthesis
        if !self.check_token(&Token::LParen) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        let mut columns = Vec::new();

        while !self.is_at_end() && !self.check_token(&Token::RParen) {
            self.skip_whitespace();

            // Parse column name
            let col_name = self.parse_identifier()?;
            self.skip_whitespace();

            // Check for optional ASC/DESC
            let descending = if self.check_keyword(Keyword::ASC) {
                self.advance();
                self.skip_whitespace();
                false
            } else if self.check_keyword(Keyword::DESC) {
                self.advance();
                self.skip_whitespace();
                true
            } else {
                false // Default is ASC
            };

            columns.push(TokenParsedConstraintColumn {
                name: col_name,
                descending,
            });

            // Check for comma (more columns) or right paren (end)
            if self.check_token(&Token::Comma) {
                self.advance();
                self.skip_whitespace();
            } else if self.check_token(&Token::RParen) {
                break;
            } else {
                // Unexpected token, try to continue
                self.advance();
            }
        }

        // Consume closing parenthesis
        if self.check_token(&Token::RParen) {
            self.advance();
        }

        if columns.is_empty() {
            None
        } else {
            Some(columns)
        }
    }

    /// Parse a simple column list (just names, no ASC/DESC)
    /// Format: ([Col1], [Col2], ...)
    fn parse_simple_column_list(&mut self) -> Option<Vec<String>> {
        // Expect opening parenthesis
        if !self.check_token(&Token::LParen) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        let mut columns = Vec::new();

        while !self.is_at_end() && !self.check_token(&Token::RParen) {
            self.skip_whitespace();

            // Parse column name
            if let Some(col_name) = self.parse_identifier() {
                columns.push(col_name);
            } else {
                break;
            }

            self.skip_whitespace();

            // Check for comma (more columns) or right paren (end)
            if self.check_token(&Token::Comma) {
                self.advance();
                self.skip_whitespace();
            } else if self.check_token(&Token::RParen) {
                break;
            } else {
                // Unexpected token, try to continue
                self.advance();
            }
        }

        // Consume closing parenthesis
        if self.check_token(&Token::RParen) {
            self.advance();
        }

        if columns.is_empty() {
            None
        } else {
            Some(columns)
        }
    }

    /// Parse a parenthesized expression and return its contents
    fn parse_parenthesized_expression(&mut self) -> Option<String> {
        if !self.check_token(&Token::LParen) {
            return None;
        }

        self.advance(); // consume (

        let mut depth = 1;
        let mut content = String::new();

        while !self.is_at_end() && depth > 0 {
            if let Some(token) = self.current_token() {
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
                        content.push_str(&self.token_to_string(&token.token));
                    }
                }
                self.advance();
            }
        }

        let content = content.trim().to_string();
        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    }

    /// Parse a schema-qualified name: [schema].[name] or schema.name or [name] or name
    fn parse_schema_qualified_name(&mut self) -> Option<(String, String)> {
        let first_ident = self.parse_identifier()?;
        self.skip_whitespace();

        // Check if there's a dot (schema.name pattern)
        if self.check_token(&Token::Period) {
            self.advance();
            self.skip_whitespace();

            let second_ident = self.parse_identifier()?;

            Some((first_ident, second_ident))
        } else {
            // No dot - just a name, default schema to "dbo"
            Some(("dbo".to_string(), first_ident))
        }
    }

    /// Parse an identifier (bracketed or unbracketed)
    fn parse_identifier(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        let token = self.current_token()?;
        match &token.token {
            Token::Word(w) => {
                let name = w.value.clone();
                self.advance();
                Some(name)
            }
            _ => None,
        }
    }

    // ========================================================================
    // Helper methods
    // ========================================================================

    /// Skip whitespace tokens
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                match &token.token {
                    Token::Whitespace(_) => {
                        self.advance();
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }
    }

    /// Check if at end of tokens
    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
            || matches!(
                self.tokens.get(self.pos),
                Some(TokenWithSpan {
                    token: Token::EOF,
                    ..
                })
            )
    }

    /// Get current token without consuming
    fn current_token(&self) -> Option<&TokenWithSpan> {
        self.tokens.get(self.pos)
    }

    /// Advance to next token
    fn advance(&mut self) {
        if !self.is_at_end() {
            self.pos += 1;
        }
    }

    /// Check if current token is a specific keyword
    fn check_keyword(&self, keyword: Keyword) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.keyword == keyword)
        } else {
            false
        }
    }

    /// Check if current token is a word matching (case-insensitive)
    fn check_word_ci(&self, word: &str) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.value.eq_ignore_ascii_case(word))
        } else {
            false
        }
    }

    /// Check if current token matches a specific token type
    fn check_token(&self, expected: &Token) -> bool {
        if let Some(token) = self.current_token() {
            std::mem::discriminant(&token.token) == std::mem::discriminant(expected)
        } else {
            false
        }
    }

    /// Convert a token back to string for expression reconstruction
    fn token_to_string(&self, token: &Token) -> String {
        match token {
            Token::Word(w) => {
                if w.quote_style == Some('[') {
                    format!("[{}]", w.value)
                } else if w.quote_style == Some('"') {
                    format!("\"{}\"", w.value)
                } else {
                    w.value.clone()
                }
            }
            Token::Number(n, _) => n.clone(),
            Token::SingleQuotedString(s) => format!("'{}'", s),
            Token::NationalStringLiteral(s) => format!("N'{}'", s),
            Token::LParen => "(".to_string(),
            Token::RParen => ")".to_string(),
            Token::Comma => ",".to_string(),
            Token::Period => ".".to_string(),
            Token::SemiColon => ";".to_string(),
            Token::Eq => "=".to_string(),
            Token::Plus => "+".to_string(),
            Token::Minus => "-".to_string(),
            Token::Mul => "*".to_string(),
            Token::Div => "/".to_string(),
            Token::Whitespace(w) => w.to_string(),
            Token::LBracket => "[".to_string(),
            Token::RBracket => "]".to_string(),
            Token::Lt => "<".to_string(),
            Token::Gt => ">".to_string(),
            Token::LtEq => "<=".to_string(),
            Token::GtEq => ">=".to_string(),
            Token::Neq => "<>".to_string(),
            Token::Ampersand => "&".to_string(),
            Token::Pipe => "|".to_string(),
            Token::Caret => "^".to_string(),
            Token::Mod => "%".to_string(),
            Token::Colon => ":".to_string(),
            Token::ExclamationMark => "!".to_string(),
            Token::AtSign => "@".to_string(),
            _ => format!("{}", token),
        }
    }
}

/// Parse ALTER TABLE ... ADD CONSTRAINT using tokens
pub fn parse_alter_table_add_constraint_tokens(
    sql: &str,
) -> Option<TokenParsedAlterTableConstraint> {
    let mut parser = ConstraintTokenParser::new(sql)?;
    parser.parse_alter_table_add_constraint()
}

/// Parse a table-level constraint using tokens
pub fn parse_table_constraint_tokens(
    constraint_def: &str,
    table_name: &str,
) -> Option<TokenParsedConstraint> {
    let mut parser = ConstraintTokenParser::new(constraint_def)?;
    parser.parse_table_constraint(table_name)
}

/// Extract schema and table name from ALTER TABLE statement using tokens
pub fn parse_alter_table_name_tokens(sql: &str) -> Option<(String, String)> {
    let mut parser = ConstraintTokenParser::new(sql)?;
    parser.skip_whitespace();

    // Expect ALTER keyword
    if !parser.check_keyword(Keyword::ALTER) {
        return None;
    }
    parser.advance();
    parser.skip_whitespace();

    // Expect TABLE keyword
    if !parser.check_keyword(Keyword::TABLE) {
        return None;
    }
    parser.advance();
    parser.skip_whitespace();

    // Parse table name (schema-qualified)
    parser.parse_schema_qualified_name()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // ALTER TABLE name extraction tests (C4)
    // ========================================================================

    #[test]
    fn test_alter_table_name_simple() {
        let sql = "ALTER TABLE [dbo].[Users] ADD CONSTRAINT [PK_Users] PRIMARY KEY ([Id])";
        let result = parse_alter_table_name_tokens(sql).unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "Users");
    }

    #[test]
    fn test_alter_table_name_unbracketed() {
        let sql = "ALTER TABLE dbo.Orders ADD CONSTRAINT PK_Orders PRIMARY KEY (Id)";
        let result = parse_alter_table_name_tokens(sql).unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "Orders");
    }

    #[test]
    fn test_alter_table_name_no_schema() {
        let sql = "ALTER TABLE [Products] ADD CONSTRAINT [PK_Products] PRIMARY KEY ([Id])";
        let result = parse_alter_table_name_tokens(sql).unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "Products");
    }

    #[test]
    fn test_alter_table_name_custom_schema() {
        let sql = "ALTER TABLE [sales].[Invoices] ADD CONSTRAINT [PK_Invoices] PRIMARY KEY ([Id])";
        let result = parse_alter_table_name_tokens(sql).unwrap();
        assert_eq!(result.0, "sales");
        assert_eq!(result.1, "Invoices");
    }

    #[test]
    fn test_alter_table_name_special_chars() {
        let sql = "ALTER TABLE [dbo].[User&Data] ADD CONSTRAINT [PK] PRIMARY KEY ([Id])";
        let result = parse_alter_table_name_tokens(sql).unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "User&Data");
    }

    // ========================================================================
    // ALTER TABLE ADD CONSTRAINT PRIMARY KEY tests (C2)
    // ========================================================================

    #[test]
    fn test_alter_add_pk_basic() {
        let sql = "ALTER TABLE [dbo].[Users] ADD CONSTRAINT [PK_Users] PRIMARY KEY ([Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();
        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "Users");

        if let TokenParsedConstraint::PrimaryKey {
            name,
            columns,
            is_clustered,
        } = result.constraint
        {
            assert_eq!(name, "PK_Users");
            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "Id");
            assert!(!columns[0].descending);
            assert!(is_clustered);
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    #[test]
    fn test_alter_add_pk_clustered() {
        let sql =
            "ALTER TABLE [dbo].[Users] ADD CONSTRAINT [PK_Users] PRIMARY KEY CLUSTERED ([Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::PrimaryKey { is_clustered, .. } = result.constraint {
            assert!(is_clustered);
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    #[test]
    fn test_alter_add_pk_nonclustered() {
        let sql =
            "ALTER TABLE [dbo].[Users] ADD CONSTRAINT [PK_Users] PRIMARY KEY NONCLUSTERED ([Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::PrimaryKey { is_clustered, .. } = result.constraint {
            assert!(!is_clustered);
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    #[test]
    fn test_alter_add_pk_multiple_columns() {
        let sql = "ALTER TABLE [dbo].[OrderItems] ADD CONSTRAINT [PK_OrderItems] PRIMARY KEY ([OrderId], [ProductId])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::PrimaryKey { columns, .. } = result.constraint {
            assert_eq!(columns.len(), 2);
            assert_eq!(columns[0].name, "OrderId");
            assert_eq!(columns[1].name, "ProductId");
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    #[test]
    fn test_alter_add_pk_with_desc() {
        let sql = "ALTER TABLE [dbo].[Logs] ADD CONSTRAINT [PK_Logs] PRIMARY KEY ([Timestamp] DESC, [Id] ASC)";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::PrimaryKey { columns, .. } = result.constraint {
            assert_eq!(columns.len(), 2);
            assert_eq!(columns[0].name, "Timestamp");
            assert!(columns[0].descending);
            assert_eq!(columns[1].name, "Id");
            assert!(!columns[1].descending);
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    // ========================================================================
    // ALTER TABLE ADD CONSTRAINT UNIQUE tests (C2)
    // ========================================================================

    #[test]
    fn test_alter_add_unique_basic() {
        let sql = "ALTER TABLE [dbo].[Users] ADD CONSTRAINT [UQ_Email] UNIQUE ([Email])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::Unique {
            name,
            columns,
            is_clustered,
        } = result.constraint
        {
            assert_eq!(name, "UQ_Email");
            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "Email");
            assert!(!is_clustered); // Default for UNIQUE
        } else {
            panic!("Expected Unique constraint");
        }
    }

    #[test]
    fn test_alter_add_unique_clustered() {
        let sql = "ALTER TABLE [dbo].[Users] ADD CONSTRAINT [UQ_Email] UNIQUE CLUSTERED ([Email])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::Unique { is_clustered, .. } = result.constraint {
            assert!(is_clustered);
        } else {
            panic!("Expected Unique constraint");
        }
    }

    #[test]
    fn test_alter_add_unique_nonclustered() {
        let sql =
            "ALTER TABLE [dbo].[Users] ADD CONSTRAINT [UQ_Email] UNIQUE NONCLUSTERED ([Email])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::Unique { is_clustered, .. } = result.constraint {
            assert!(!is_clustered);
        } else {
            panic!("Expected Unique constraint");
        }
    }

    #[test]
    fn test_alter_add_unique_multiple_columns() {
        let sql = "ALTER TABLE [dbo].[Products] ADD CONSTRAINT [UQ_SKU] UNIQUE ([Category], [SKU])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::Unique { columns, .. } = result.constraint {
            assert_eq!(columns.len(), 2);
            assert_eq!(columns[0].name, "Category");
            assert_eq!(columns[1].name, "SKU");
        } else {
            panic!("Expected Unique constraint");
        }
    }

    // ========================================================================
    // ALTER TABLE ADD CONSTRAINT FOREIGN KEY tests (C1)
    // ========================================================================

    #[test]
    fn test_alter_add_fk_basic() {
        let sql = "ALTER TABLE [dbo].[Orders] ADD CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId]) REFERENCES [dbo].[Users]([Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::ForeignKey {
            name,
            columns,
            referenced_table,
            referenced_columns,
        } = result.constraint
        {
            assert_eq!(name, "FK_Orders_Users");
            assert_eq!(columns, vec!["UserId"]);
            assert_eq!(referenced_table, "[dbo].[Users]");
            assert_eq!(referenced_columns, vec!["Id"]);
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    #[test]
    fn test_alter_add_fk_multiple_columns() {
        let sql = "ALTER TABLE [dbo].[OrderItems] ADD CONSTRAINT [FK_OrderItems_Products] FOREIGN KEY ([CategoryId], [ProductId]) REFERENCES [dbo].[Products]([CategoryId], [Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::ForeignKey {
            columns,
            referenced_columns,
            ..
        } = result.constraint
        {
            assert_eq!(columns, vec!["CategoryId", "ProductId"]);
            assert_eq!(referenced_columns, vec!["CategoryId", "Id"]);
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    #[test]
    fn test_alter_add_fk_no_schema() {
        let sql = "ALTER TABLE [Orders] ADD CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId]) REFERENCES [Users]([Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "Orders");

        if let TokenParsedConstraint::ForeignKey {
            referenced_table, ..
        } = result.constraint
        {
            assert_eq!(referenced_table, "[dbo].[Users]");
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    #[test]
    fn test_alter_add_fk_different_schema() {
        let sql = "ALTER TABLE [sales].[Orders] ADD CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId]) REFERENCES [auth].[Users]([Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        assert_eq!(result.table_schema, "sales");
        assert_eq!(result.table_name, "Orders");

        if let TokenParsedConstraint::ForeignKey {
            referenced_table, ..
        } = result.constraint
        {
            assert_eq!(referenced_table, "[auth].[Users]");
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    #[test]
    fn test_alter_add_fk_with_nocheck() {
        let sql = "ALTER TABLE [dbo].[Orders] WITH NOCHECK ADD CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId]) REFERENCES [dbo].[Users]([Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::ForeignKey { name, .. } = result.constraint {
            assert_eq!(name, "FK_Orders_Users");
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    #[test]
    fn test_alter_add_fk_with_check() {
        let sql = "ALTER TABLE [dbo].[Orders] WITH CHECK ADD CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId]) REFERENCES [dbo].[Users]([Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::ForeignKey { name, .. } = result.constraint {
            assert_eq!(name, "FK_Orders_Users");
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    // ========================================================================
    // ALTER TABLE ADD CONSTRAINT CHECK tests (C1)
    // ========================================================================

    #[test]
    fn test_alter_add_check_basic() {
        let sql = "ALTER TABLE [dbo].[Products] ADD CONSTRAINT [CK_Price] CHECK ([Price] > 0)";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::Check { name, expression } = result.constraint {
            assert_eq!(name, "CK_Price");
            assert_eq!(expression, "[Price] > 0");
        } else {
            panic!("Expected Check constraint");
        }
    }

    #[test]
    fn test_alter_add_check_complex() {
        let sql =
            "ALTER TABLE [dbo].[Users] ADD CONSTRAINT [CK_Age] CHECK ([Age] >= 0 AND [Age] <= 150)";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::Check { name, expression } = result.constraint {
            assert_eq!(name, "CK_Age");
            assert!(expression.contains("Age"));
            assert!(expression.contains(">="));
            assert!(expression.contains("AND"));
        } else {
            panic!("Expected Check constraint");
        }
    }

    #[test]
    fn test_alter_add_check_with_check() {
        let sql =
            "ALTER TABLE [dbo].[Products] WITH CHECK ADD CONSTRAINT [CK_Price] CHECK ([Price] > 0)";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::Check { name, .. } = result.constraint {
            assert_eq!(name, "CK_Price");
        } else {
            panic!("Expected Check constraint");
        }
    }

    #[test]
    fn test_alter_add_check_nested_parens() {
        let sql = "ALTER TABLE [dbo].[Orders] ADD CONSTRAINT [CK_Status] CHECK ([Status] IN ('Pending', 'Active', 'Complete'))";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::Check { expression, .. } = result.constraint {
            assert!(expression.contains("IN"));
            assert!(expression.contains("Pending"));
        } else {
            panic!("Expected Check constraint");
        }
    }

    // ========================================================================
    // Table-level constraint tests (C3)
    // ========================================================================

    #[test]
    fn test_table_pk_named() {
        let sql = "CONSTRAINT [PK_Users] PRIMARY KEY CLUSTERED ([Id])";
        let result = parse_table_constraint_tokens(sql, "Users").unwrap();

        if let TokenParsedConstraint::PrimaryKey {
            name,
            columns,
            is_clustered,
        } = result
        {
            assert_eq!(name, "PK_Users");
            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "Id");
            assert!(is_clustered);
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    #[test]
    fn test_table_pk_unnamed() {
        let sql = "PRIMARY KEY ([Id])";
        let result = parse_table_constraint_tokens(sql, "Users").unwrap();

        if let TokenParsedConstraint::PrimaryKey { name, .. } = result {
            assert_eq!(name, "PK_Users"); // Generated default name
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    #[test]
    fn test_table_unique_named() {
        let sql = "CONSTRAINT [UQ_Email] UNIQUE NONCLUSTERED ([Email])";
        let result = parse_table_constraint_tokens(sql, "Users").unwrap();

        if let TokenParsedConstraint::Unique {
            name,
            columns,
            is_clustered,
        } = result
        {
            assert_eq!(name, "UQ_Email");
            assert_eq!(columns[0].name, "Email");
            assert!(!is_clustered);
        } else {
            panic!("Expected Unique constraint");
        }
    }

    #[test]
    fn test_table_unique_unnamed() {
        let sql = "UNIQUE ([Email])";
        let result = parse_table_constraint_tokens(sql, "Users").unwrap();

        if let TokenParsedConstraint::Unique { name, .. } = result {
            assert_eq!(name, "UQ_Users"); // Generated default name
        } else {
            panic!("Expected Unique constraint");
        }
    }

    #[test]
    fn test_table_fk_named() {
        let sql =
            "CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId]) REFERENCES [dbo].[Users]([Id])";
        let result = parse_table_constraint_tokens(sql, "Orders").unwrap();

        if let TokenParsedConstraint::ForeignKey {
            name,
            columns,
            referenced_table,
            referenced_columns,
        } = result
        {
            assert_eq!(name, "FK_Orders_Users");
            assert_eq!(columns, vec!["UserId"]);
            assert_eq!(referenced_table, "[dbo].[Users]");
            assert_eq!(referenced_columns, vec!["Id"]);
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    #[test]
    fn test_table_fk_unnamed() {
        let sql = "FOREIGN KEY ([UserId]) REFERENCES [Users]([Id])";
        let result = parse_table_constraint_tokens(sql, "Orders").unwrap();

        if let TokenParsedConstraint::ForeignKey {
            name,
            referenced_table,
            ..
        } = result
        {
            assert_eq!(name, "FK_Orders"); // Generated default name
            assert_eq!(referenced_table, "[dbo].[Users]");
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    #[test]
    fn test_table_check_named() {
        let sql = "CONSTRAINT [CK_Age] CHECK ([Age] >= 0)";
        let result = parse_table_constraint_tokens(sql, "Users").unwrap();

        if let TokenParsedConstraint::Check { name, expression } = result {
            assert_eq!(name, "CK_Age");
            assert_eq!(expression, "[Age] >= 0");
        } else {
            panic!("Expected Check constraint");
        }
    }

    #[test]
    fn test_table_check_unnamed() {
        let sql = "CHECK ([Price] > 0)";
        let result = parse_table_constraint_tokens(sql, "Products").unwrap();

        if let TokenParsedConstraint::Check { name, expression } = result {
            assert_eq!(name, "CK_Products"); // Generated default name
            assert_eq!(expression, "[Price] > 0");
        } else {
            panic!("Expected Check constraint");
        }
    }

    // ========================================================================
    // Edge cases and multiline tests
    // ========================================================================

    #[test]
    fn test_multiline_alter_add_constraint() {
        let sql = r#"
ALTER TABLE [dbo].[Orders] WITH NOCHECK
ADD CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId])
REFERENCES [dbo].[Users]([Id])
"#;
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::ForeignKey { name, .. } = result.constraint {
            assert_eq!(name, "FK_Orders_Users");
        } else {
            panic!("Expected ForeignKey constraint");
        }
    }

    #[test]
    fn test_lowercase() {
        let sql = "alter table [dbo].[users] add constraint [pk_users] primary key ([id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::PrimaryKey { name, .. } = result.constraint {
            assert_eq!(name, "pk_users");
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    #[test]
    fn test_mixed_case() {
        let sql = "Alter Table [dbo].[Users] Add Constraint [PK_Users] Primary Key ([Id])";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        if let TokenParsedConstraint::PrimaryKey { name, .. } = result.constraint {
            assert_eq!(name, "PK_Users");
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    #[test]
    fn test_unbracketed_identifiers() {
        let sql = "ALTER TABLE dbo.Users ADD CONSTRAINT PK_Users PRIMARY KEY (Id)";
        let result = parse_alter_table_add_constraint_tokens(sql).unwrap();

        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "Users");

        if let TokenParsedConstraint::PrimaryKey { name, columns, .. } = result.constraint {
            assert_eq!(name, "PK_Users");
            assert_eq!(columns[0].name, "Id");
        } else {
            panic!("Expected PrimaryKey constraint");
        }
    }

    // ========================================================================
    // Negative tests
    // ========================================================================

    #[test]
    fn test_not_alter_statement() {
        let result = parse_alter_table_add_constraint_tokens("CREATE TABLE [dbo].[Users] (Id INT)");
        assert!(result.is_none());
    }

    #[test]
    fn test_alter_without_add() {
        let result =
            parse_alter_table_add_constraint_tokens("ALTER TABLE [dbo].[Users] DROP COLUMN [Name]");
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_input() {
        let result = parse_alter_table_add_constraint_tokens("");
        assert!(result.is_none());
    }

    #[test]
    fn test_invalid_constraint_type() {
        let result = parse_table_constraint_tokens("INVALID CONSTRAINT TYPE", "Table");
        assert!(result.is_none());
    }
}
