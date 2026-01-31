//! Token-based extended property parsing for T-SQL
//!
//! This module provides token-based parsing for sp_addextendedproperty calls, replacing
//! the previous regex-based approach. Part of Phase 15.6 (G1) of the implementation plan.
//!
//! ## Supported Syntax
//!
//! ```sql
//! EXEC sp_addextendedproperty
//!     @name = N'MS_Description',
//!     @value = N'Description text',
//!     @level0type = N'SCHEMA', @level0name = N'dbo',
//!     @level1type = N'TABLE',  @level1name = N'TableName',
//!     @level2type = N'COLUMN', @level2name = N'ColumnName';
//! ```
//!
//! Also supports:
//! - With or without EXEC/EXECUTE keyword
//! - Schema-qualified procedure name: [sys].sp_addextendedproperty
//! - NULL values for @value parameter
//! - Mixed case parameter names

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

/// Result of parsing an extended property call using tokens
#[derive(Debug, Clone, Default)]
pub struct TokenParsedExtendedProperty {
    /// Property name (e.g., "MS_Description")
    pub property_name: String,
    /// Property value (e.g., "Description text")
    pub property_value: String,
    /// Level 0 name (schema, e.g., "dbo")
    pub level0name: String,
    /// Level 1 type (e.g., "TABLE", "VIEW")
    pub level1type: Option<String>,
    /// Level 1 name (e.g., "DocumentedTable")
    pub level1name: Option<String>,
    /// Level 2 type (e.g., "COLUMN", "INDEX")
    pub level2type: Option<String>,
    /// Level 2 name (e.g., "Id")
    pub level2name: Option<String>,
}

/// Token-based extended property parser
pub struct ExtendedPropertyTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl ExtendedPropertyTokenParser {
    /// Create a new parser for an extended property SQL string
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Parse sp_addextendedproperty call and return property info
    pub fn parse_extended_property(&mut self) -> Option<TokenParsedExtendedProperty> {
        self.skip_whitespace();

        // Skip optional EXEC or EXECUTE keyword
        if self.check_keyword(Keyword::EXEC) || self.check_keyword(Keyword::EXECUTE) {
            self.advance();
            self.skip_whitespace();
        }

        // Skip optional schema prefix (e.g., [sys]. or sys.)
        if self.check_word_ci("sys") || self.check_word_ci("dbo") {
            self.advance();
            self.skip_whitespace();
            if self.check_token(&Token::Period) {
                self.advance();
                self.skip_whitespace();
            }
        }

        // Expect sp_addextendedproperty identifier
        if !self.check_word_ci("sp_addextendedproperty") {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Initialize with defaults
        let mut result = TokenParsedExtendedProperty {
            level0name: "dbo".to_string(),
            ..Default::default()
        };

        // Parse parameters until end of statement
        self.parse_parameters(&mut result)?;

        // Require at minimum a property name
        if result.property_name.is_empty() {
            return None;
        }

        Some(result)
    }

    /// Parse the parameters of sp_addextendedproperty
    fn parse_parameters(&mut self, result: &mut TokenParsedExtendedProperty) -> Option<()> {
        // Parse all @param = value pairs
        // Note: MsSqlDialect tokenizes @name as a single Word token, not @ + name
        while !self.is_at_end() {
            self.skip_whitespace();

            // Check for semicolon or end of statement
            if self.check_token(&Token::SemiColon) {
                break;
            }

            // Check for parameter word starting with @
            let param_name = match self.try_parse_param_name() {
                Some(name) => name,
                None => {
                    // Skip any commas
                    if self.check_token(&Token::Comma) {
                        self.advance();
                        continue;
                    }
                    // Skip unknown tokens
                    self.advance();
                    continue;
                }
            };
            self.skip_whitespace();

            // Expect =
            if !self.check_token(&Token::Eq) {
                continue;
            }
            self.advance();
            self.skip_whitespace();

            // Parse value - could be N'string', 'string', NULL, or identifier
            let value = self.parse_parameter_value();

            // Store value based on parameter name
            match param_name.as_str() {
                "NAME" => {
                    result.property_name = value.unwrap_or_default();
                }
                "VALUE" => {
                    result.property_value = value.unwrap_or_default();
                }
                "LEVEL0NAME" => {
                    if let Some(v) = value {
                        result.level0name = v;
                    }
                }
                "LEVEL0TYPE" => {
                    // Skip level0type - it's always SCHEMA
                }
                "LEVEL1TYPE" => {
                    result.level1type = value;
                }
                "LEVEL1NAME" => {
                    result.level1name = value;
                }
                "LEVEL2TYPE" => {
                    result.level2type = value;
                }
                "LEVEL2NAME" => {
                    result.level2name = value;
                }
                _ => {
                    // Unknown parameter, ignore
                }
            }
        }

        Some(())
    }

    /// Try to parse a parameter name (word starting with @)
    /// MsSqlDialect tokenizes @name as a single Word token
    fn try_parse_param_name(&mut self) -> Option<String> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                if w.value.starts_with('@') {
                    // Extract parameter name (without @)
                    let param_name = w.value[1..].to_uppercase();
                    self.advance();
                    return Some(param_name);
                }
            }
        }
        None
    }

    /// Parse a parameter value: N'string', 'string', NULL, or identifier
    /// Note: MsSqlDialect tokenizes N'string' as NationalStringLiteral directly
    fn parse_parameter_value(&mut self) -> Option<String> {
        self.skip_whitespace();

        if self.is_at_end() {
            return None;
        }

        // Check for NULL keyword
        if self.check_keyword(Keyword::NULL) {
            self.advance();
            return None;
        }

        // Check for string literal (including NationalStringLiteral which handles N'...')
        if let Some(token) = self.current_token() {
            match &token.token {
                Token::SingleQuotedString(s) | Token::NationalStringLiteral(s) => {
                    let value = s.clone();
                    self.advance();
                    return Some(value);
                }
                Token::Word(w) => {
                    // Could be an identifier value or NULL
                    let value = w.value.clone();
                    self.advance();
                    // Check for NULL as a word
                    if value.eq_ignore_ascii_case("NULL") {
                        return None;
                    }
                    return Some(value);
                }
                _ => {}
            }
        }

        None
    }

    // ========================================================================
    // Helper methods
    // ========================================================================

    /// Skip whitespace tokens
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                if matches!(&token.token, Token::Whitespace(_)) {
                    self.advance();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    /// Check if at end of tokens
    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
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

    /// Check if current token matches a word (case-insensitive)
    fn check_word_ci(&self, word: &str) -> bool {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                return w.value.eq_ignore_ascii_case(word);
            }
        }
        false
    }

    /// Check if current token matches a specific token type
    fn check_token(&self, expected: &Token) -> bool {
        if let Some(token) = self.current_token() {
            std::mem::discriminant(&token.token) == std::mem::discriminant(expected)
        } else {
            false
        }
    }
}

/// Parse an extended property from SQL using token-based parsing
pub fn parse_extended_property_tokens(sql: &str) -> Option<TokenParsedExtendedProperty> {
    let mut parser = ExtendedPropertyTokenParser::new(sql)?;
    parser.parse_extended_property()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_table_property() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Description text',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'TABLE', @level1name = N'Users';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.property_name, "MS_Description");
        assert_eq!(result.property_value, "Description text");
        assert_eq!(result.level0name, "dbo");
        assert_eq!(result.level1type, Some("TABLE".to_string()));
        assert_eq!(result.level1name, Some("Users".to_string()));
        assert_eq!(result.level2type, None);
        assert_eq!(result.level2name, None);
    }

    #[test]
    fn test_column_property() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Column description',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'TABLE', @level1name = N'Users',
                @level2type = N'COLUMN', @level2name = N'Id';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.property_name, "MS_Description");
        assert_eq!(result.property_value, "Column description");
        assert_eq!(result.level0name, "dbo");
        assert_eq!(result.level1type, Some("TABLE".to_string()));
        assert_eq!(result.level1name, Some("Users".to_string()));
        assert_eq!(result.level2type, Some("COLUMN".to_string()));
        assert_eq!(result.level2name, Some("Id".to_string()));
    }

    #[test]
    fn test_without_exec() {
        let sql = r#"
            sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Description',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'TABLE', @level1name = N'Products';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.property_name, "MS_Description");
        assert_eq!(result.level1name, Some("Products".to_string()));
    }

    #[test]
    fn test_execute_keyword() {
        let sql = r#"
            EXECUTE sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Description',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'VIEW', @level1name = N'CustomerView';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.level1type, Some("VIEW".to_string()));
        assert_eq!(result.level1name, Some("CustomerView".to_string()));
    }

    #[test]
    fn test_schema_qualified_procedure() {
        let sql = r#"
            EXEC sys.sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Test',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'TABLE', @level1name = N'Test';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.property_name, "MS_Description");
    }

    #[test]
    fn test_null_value() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = NULL,
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'TABLE', @level1name = N'Test';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.property_name, "MS_Description");
        assert_eq!(result.property_value, ""); // NULL becomes empty string
    }

    #[test]
    fn test_single_quotes_without_n() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = 'MS_Description',
                @value = 'Simple string',
                @level0type = 'SCHEMA', @level0name = 'sales',
                @level1type = 'TABLE', @level1name = 'Orders';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.property_name, "MS_Description");
        assert_eq!(result.property_value, "Simple string");
        assert_eq!(result.level0name, "sales");
        assert_eq!(result.level1name, Some("Orders".to_string()));
    }

    #[test]
    fn test_index_property() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Index description',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'TABLE', @level1name = N'Users',
                @level2type = N'INDEX', @level2name = N'IX_Users_Email';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.level2type, Some("INDEX".to_string()));
        assert_eq!(result.level2name, Some("IX_Users_Email".to_string()));
    }

    #[test]
    fn test_custom_schema() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Table in custom schema',
                @level0type = N'SCHEMA', @level0name = N'sales',
                @level1type = N'TABLE', @level1name = N'Orders';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.level0name, "sales");
    }

    #[test]
    fn test_schema_property_only() {
        // Property on schema itself, no level1
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Sales schema',
                @level0type = N'SCHEMA', @level0name = N'sales';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.property_name, "MS_Description");
        assert_eq!(result.property_value, "Sales schema");
        assert_eq!(result.level0name, "sales");
        assert_eq!(result.level1type, None);
        assert_eq!(result.level1name, None);
    }

    #[test]
    fn test_default_schema() {
        // No explicit level0name, should default to dbo
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Test',
                @level0type = N'SCHEMA',
                @level1type = N'TABLE', @level1name = N'Test';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.level0name, "dbo"); // Default
    }

    #[test]
    fn test_mixed_case_parameters() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @NAME = N'MS_Description',
                @VALUE = N'Test',
                @Level0Type = N'SCHEMA', @Level0Name = N'dbo',
                @level1type = N'TABLE', @LEVEL1NAME = N'Test';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.property_name, "MS_Description");
        assert_eq!(result.property_value, "Test");
        assert_eq!(result.level1name, Some("Test".to_string()));
    }

    #[test]
    fn test_value_with_special_characters() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Description with "quotes" and special chars: <>',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'TABLE', @level1name = N'Test';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert!(result.property_value.contains("quotes"));
    }

    #[test]
    fn test_invalid_no_name() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @value = N'Description',
                @level0type = N'SCHEMA', @level0name = N'dbo';
        "#;

        let result = parse_extended_property_tokens(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_invalid_not_extended_property() {
        let sql = r#"
            CREATE TABLE dbo.Test (Id INT);
        "#;

        let result = parse_extended_property_tokens(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_procedure_property() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Stored procedure description',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'PROCEDURE', @level1name = N'usp_GetUsers';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.level1type, Some("PROCEDURE".to_string()));
        assert_eq!(result.level1name, Some("usp_GetUsers".to_string()));
    }

    #[test]
    fn test_function_property() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Function description',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'FUNCTION', @level1name = N'fn_CalculateTotal';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.level1type, Some("FUNCTION".to_string()));
        assert_eq!(result.level1name, Some("fn_CalculateTotal".to_string()));
    }

    #[test]
    fn test_constraint_property() {
        let sql = r#"
            EXEC sp_addextendedproperty
                @name = N'MS_Description',
                @value = N'Primary key constraint',
                @level0type = N'SCHEMA', @level0name = N'dbo',
                @level1type = N'TABLE', @level1name = N'Users',
                @level2type = N'CONSTRAINT', @level2name = N'PK_Users';
        "#;

        let result = parse_extended_property_tokens(sql).unwrap();
        assert_eq!(result.level2type, Some("CONSTRAINT".to_string()));
        assert_eq!(result.level2name, Some("PK_Users".to_string()));
    }
}
