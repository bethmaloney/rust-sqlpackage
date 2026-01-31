//! Token-based sequence definition parsing for T-SQL
//!
//! This module provides token-based parsing for sequence definitions, replacing
//! the previous regex-based approach. Part of Phase 15.3 of the implementation plan.
//!
//! ## Supported Syntax
//!
//! CREATE SEQUENCE:
//! ```sql
//! CREATE SEQUENCE [schema].[name] AS BIGINT START WITH 1 INCREMENT BY 1
//! CREATE SEQUENCE [schema].[name] MINVALUE 0 MAXVALUE 1000000 NO CYCLE
//! CREATE SEQUENCE [schema].[name] AS INT START WITH 100 INCREMENT BY 10 CACHE 50
//! CREATE SEQUENCE [schema].[name] NO MINVALUE NO MAXVALUE CYCLE NO CACHE
//! ```
//!
//! ALTER SEQUENCE:
//! ```sql
//! ALTER SEQUENCE [schema].[name] RESTART WITH 1000
//! ALTER SEQUENCE [schema].[name] INCREMENT BY 5
//! ALTER SEQUENCE [schema].[name] MINVALUE 1 MAXVALUE 10000 CYCLE
//! ```

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

/// Result of parsing a sequence definition using tokens
#[derive(Debug, Clone, Default)]
pub struct TokenParsedSequence {
    /// Schema name of the sequence (defaults to "dbo" if not specified)
    pub schema: String,
    /// Sequence name
    pub name: String,
    /// Data type (e.g., "INT", "BIGINT") - only for CREATE, not ALTER
    pub data_type: Option<String>,
    /// START WITH value (CREATE) or RESTART WITH value (ALTER)
    pub start_value: Option<i64>,
    /// INCREMENT BY value
    pub increment_value: Option<i64>,
    /// MINVALUE value (None means NO MINVALUE or not specified)
    pub min_value: Option<i64>,
    /// MAXVALUE value (None means NO MAXVALUE or not specified)
    pub max_value: Option<i64>,
    /// CYCLE / NO CYCLE (default is NO CYCLE)
    pub is_cycling: bool,
    /// Explicit NO MINVALUE
    pub has_no_min_value: bool,
    /// Explicit NO MAXVALUE
    pub has_no_max_value: bool,
    /// CACHE size (None means default cache, Some(0) means NO CACHE)
    pub cache_size: Option<i64>,
}

/// Token-based sequence definition parser
pub struct SequenceTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl SequenceTokenParser {
    /// Create a new parser for a sequence definition string
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Parse CREATE SEQUENCE and return sequence info
    pub fn parse_create_sequence(&mut self) -> Option<TokenParsedSequence> {
        self.skip_whitespace();

        // Expect CREATE keyword
        if !self.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect SEQUENCE keyword
        if !self.check_keyword(Keyword::SEQUENCE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse sequence name (schema-qualified)
        let (schema, name) = self.parse_schema_qualified_name()?;
        self.skip_whitespace();

        // Parse sequence options
        let mut result = TokenParsedSequence {
            schema,
            name,
            ..Default::default()
        };

        self.parse_sequence_options(&mut result, false);

        Some(result)
    }

    /// Parse ALTER SEQUENCE and return sequence info
    pub fn parse_alter_sequence(&mut self) -> Option<TokenParsedSequence> {
        self.skip_whitespace();

        // Expect ALTER keyword
        if !self.check_keyword(Keyword::ALTER) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect SEQUENCE keyword
        if !self.check_keyword(Keyword::SEQUENCE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse sequence name (schema-qualified)
        let (schema, name) = self.parse_schema_qualified_name()?;
        self.skip_whitespace();

        // Parse sequence options (ALTER uses RESTART instead of START)
        let mut result = TokenParsedSequence {
            schema,
            name,
            data_type: None, // ALTER SEQUENCE doesn't change the data type
            ..Default::default()
        };

        self.parse_sequence_options(&mut result, true);

        Some(result)
    }

    /// Parse sequence options: AS type, START/RESTART WITH, INCREMENT BY, MINVALUE, MAXVALUE, CYCLE, CACHE
    fn parse_sequence_options(&mut self, result: &mut TokenParsedSequence, is_alter: bool) {
        while !self.is_at_end() {
            self.skip_whitespace();

            if self.is_at_end() {
                break;
            }

            // Check for AS <data_type> (only for CREATE)
            if !is_alter && self.check_keyword(Keyword::AS) {
                self.advance();
                self.skip_whitespace();
                if let Some(data_type) = self.parse_data_type() {
                    result.data_type = Some(data_type);
                }
                continue;
            }

            // Check for START WITH <value> (CREATE) or RESTART WITH <value> (ALTER)
            if !is_alter && self.check_word_ci("START") {
                self.advance();
                self.skip_whitespace();
                if self.check_keyword(Keyword::WITH) {
                    self.advance();
                    self.skip_whitespace();
                    if let Some(value) = self.parse_integer() {
                        result.start_value = Some(value);
                    }
                }
                continue;
            }

            if is_alter && self.check_word_ci("RESTART") {
                self.advance();
                self.skip_whitespace();
                if self.check_keyword(Keyword::WITH) {
                    self.advance();
                    self.skip_whitespace();
                    if let Some(value) = self.parse_integer() {
                        result.start_value = Some(value);
                    }
                }
                continue;
            }

            // Check for INCREMENT BY <value>
            if self.check_word_ci("INCREMENT") {
                self.advance();
                self.skip_whitespace();
                if self.check_keyword(Keyword::BY) {
                    self.advance();
                    self.skip_whitespace();
                    if let Some(value) = self.parse_integer() {
                        result.increment_value = Some(value);
                    }
                }
                continue;
            }

            // Check for NO keyword (NO MINVALUE, NO MAXVALUE, NO CYCLE, NO CACHE)
            if self.check_keyword(Keyword::NO) {
                self.advance();
                self.skip_whitespace();

                if self.check_word_ci("MINVALUE") {
                    result.has_no_min_value = true;
                    result.min_value = None;
                    self.advance();
                    continue;
                }
                if self.check_word_ci("MAXVALUE") {
                    result.has_no_max_value = true;
                    result.max_value = None;
                    self.advance();
                    continue;
                }
                if self.check_keyword(Keyword::CYCLE) {
                    result.is_cycling = false;
                    self.advance();
                    continue;
                }
                if self.check_word_ci("CACHE") {
                    result.cache_size = Some(0); // NO CACHE means cache size of 0
                    self.advance();
                    continue;
                }
                // Unknown NO X, skip
                continue;
            }

            // Check for MINVALUE <value>
            if self.check_word_ci("MINVALUE") {
                self.advance();
                self.skip_whitespace();
                if let Some(value) = self.parse_integer() {
                    result.min_value = Some(value);
                }
                continue;
            }

            // Check for MAXVALUE <value>
            if self.check_word_ci("MAXVALUE") {
                self.advance();
                self.skip_whitespace();
                if let Some(value) = self.parse_integer() {
                    result.max_value = Some(value);
                }
                continue;
            }

            // Check for CYCLE (without NO)
            if self.check_keyword(Keyword::CYCLE) {
                result.is_cycling = true;
                self.advance();
                continue;
            }

            // Check for CACHE <size>
            if self.check_word_ci("CACHE") {
                self.advance();
                self.skip_whitespace();
                if let Some(value) = self.parse_integer() {
                    result.cache_size = Some(value);
                }
                continue;
            }

            // Unknown token, advance
            self.advance();
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

    /// Parse a data type (simple identifier like INT, BIGINT, SMALLINT, TINYINT, DECIMAL, NUMERIC)
    fn parse_data_type(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        let token = self.current_token()?;
        match &token.token {
            Token::Word(w) => {
                let type_name = w.value.to_uppercase();
                self.advance();
                Some(type_name)
            }
            _ => None,
        }
    }

    /// Parse an integer (positive or negative)
    fn parse_integer(&mut self) -> Option<i64> {
        if self.is_at_end() {
            return None;
        }

        // Check for optional minus sign
        let is_negative = if self.check_token(&Token::Minus) {
            self.advance();
            self.skip_whitespace();
            true
        } else {
            false
        };

        let token = self.current_token()?;
        match &token.token {
            Token::Number(n, _) => {
                if let Ok(value) = n.parse::<i64>() {
                    self.advance();
                    Some(if is_negative { -value } else { value })
                } else {
                    None
                }
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
}

/// Parse CREATE SEQUENCE using tokens and return sequence info
///
/// This function replaces the regex-based `extract_sequence_info` function.
/// Supports:
/// - CREATE SEQUENCE [dbo].[SeqName] AS BIGINT START WITH 1 INCREMENT BY 1
/// - CREATE SEQUENCE [dbo].[SeqName] MINVALUE 0 MAXVALUE 1000000 NO CYCLE
/// - CREATE SEQUENCE [dbo].[SeqName] AS INT START WITH 100 INCREMENT BY 10 CACHE 50
pub fn parse_create_sequence_tokens(sql: &str) -> Option<TokenParsedSequence> {
    let mut parser = SequenceTokenParser::new(sql)?;
    parser.parse_create_sequence()
}

/// Parse ALTER SEQUENCE using tokens and return sequence info
///
/// Supports:
/// - ALTER SEQUENCE [dbo].[SeqName] RESTART WITH 1000
/// - ALTER SEQUENCE [dbo].[SeqName] INCREMENT BY 5
/// - ALTER SEQUENCE [dbo].[SeqName] MINVALUE 1 MAXVALUE 10000 CYCLE
pub fn parse_alter_sequence_tokens(sql: &str) -> Option<TokenParsedSequence> {
    let mut parser = SequenceTokenParser::new(sql)?;
    parser.parse_alter_sequence()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // CREATE SEQUENCE tests
    // ========================================================================

    #[test]
    fn test_create_sequence_basic() {
        let sql = "CREATE SEQUENCE [dbo].[OrderSequence]";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "OrderSequence");
        assert!(result.data_type.is_none());
        assert!(result.start_value.is_none());
        assert!(result.increment_value.is_none());
    }

    #[test]
    fn test_create_sequence_with_data_type() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] AS BIGINT";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "Counter");
        assert_eq!(result.data_type, Some("BIGINT".to_string()));
    }

    #[test]
    fn test_create_sequence_start_with() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] START WITH 100";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.start_value, Some(100));
    }

    #[test]
    fn test_create_sequence_negative_start() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] START WITH -50";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.start_value, Some(-50));
    }

    #[test]
    fn test_create_sequence_increment_by() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] INCREMENT BY 5";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.increment_value, Some(5));
    }

    #[test]
    fn test_create_sequence_negative_increment() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] INCREMENT BY -1";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.increment_value, Some(-1));
    }

    #[test]
    fn test_create_sequence_minvalue() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] MINVALUE 1";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.min_value, Some(1));
        assert!(!result.has_no_min_value);
    }

    #[test]
    fn test_create_sequence_negative_minvalue() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] MINVALUE -100";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.min_value, Some(-100));
    }

    #[test]
    fn test_create_sequence_no_minvalue() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] NO MINVALUE";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert!(result.min_value.is_none());
        assert!(result.has_no_min_value);
    }

    #[test]
    fn test_create_sequence_maxvalue() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] MAXVALUE 1000000";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.max_value, Some(1000000));
        assert!(!result.has_no_max_value);
    }

    #[test]
    fn test_create_sequence_no_maxvalue() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] NO MAXVALUE";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert!(result.max_value.is_none());
        assert!(result.has_no_max_value);
    }

    #[test]
    fn test_create_sequence_cycle() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] CYCLE";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert!(result.is_cycling);
    }

    #[test]
    fn test_create_sequence_no_cycle() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] NO CYCLE";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert!(!result.is_cycling);
    }

    #[test]
    fn test_create_sequence_cache() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] CACHE 50";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.cache_size, Some(50));
    }

    #[test]
    fn test_create_sequence_no_cache() {
        let sql = "CREATE SEQUENCE [dbo].[Counter] NO CACHE";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.cache_size, Some(0));
    }

    #[test]
    fn test_create_sequence_all_options() {
        let sql = "CREATE SEQUENCE [dbo].[OrderSequence] AS BIGINT START WITH 1 INCREMENT BY 1 MINVALUE 1 MAXVALUE 9999999999 NO CYCLE CACHE 100";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "OrderSequence");
        assert_eq!(result.data_type, Some("BIGINT".to_string()));
        assert_eq!(result.start_value, Some(1));
        assert_eq!(result.increment_value, Some(1));
        assert_eq!(result.min_value, Some(1));
        assert_eq!(result.max_value, Some(9999999999));
        assert!(!result.is_cycling);
        assert_eq!(result.cache_size, Some(100));
    }

    #[test]
    fn test_create_sequence_multiline() {
        let sql = r#"
CREATE SEQUENCE [dbo].[OrderSequence]
AS BIGINT
START WITH 1
INCREMENT BY 1
MINVALUE 1
MAXVALUE 9999999999
NO CYCLE
CACHE 100
"#;
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "OrderSequence");
        assert_eq!(result.data_type, Some("BIGINT".to_string()));
        assert_eq!(result.start_value, Some(1));
        assert_eq!(result.increment_value, Some(1));
        assert_eq!(result.min_value, Some(1));
        assert_eq!(result.max_value, Some(9999999999));
        assert!(!result.is_cycling);
        assert_eq!(result.cache_size, Some(100));
    }

    #[test]
    fn test_create_sequence_custom_schema() {
        let sql = "CREATE SEQUENCE [sales].[InvoiceSequence] AS INT";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "sales");
        assert_eq!(result.name, "InvoiceSequence");
        assert_eq!(result.data_type, Some("INT".to_string()));
    }

    #[test]
    fn test_create_sequence_unbracketed() {
        let sql = "CREATE SEQUENCE dbo.Counter AS BIGINT START WITH 1";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "Counter");
        assert_eq!(result.data_type, Some("BIGINT".to_string()));
        assert_eq!(result.start_value, Some(1));
    }

    #[test]
    fn test_create_sequence_no_schema() {
        let sql = "CREATE SEQUENCE [MySequence] AS INT";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MySequence");
    }

    #[test]
    fn test_create_sequence_case_insensitive() {
        let sql = "create sequence [dbo].[Counter] as bigint start with 1 increment by 1";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.name, "Counter");
        assert_eq!(result.data_type, Some("BIGINT".to_string()));
        assert_eq!(result.start_value, Some(1));
        assert_eq!(result.increment_value, Some(1));
    }

    #[test]
    fn test_create_sequence_mixed_case() {
        let sql = "Create Sequence [dbo].[Counter] As Int Start With 100";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.name, "Counter");
        assert_eq!(result.data_type, Some("INT".to_string()));
        assert_eq!(result.start_value, Some(100));
    }

    #[test]
    fn test_create_sequence_cycle_with_bounds() {
        let sql = "CREATE SEQUENCE [dbo].[CyclingSeq] MINVALUE 1 MAXVALUE 100 CYCLE";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.min_value, Some(1));
        assert_eq!(result.max_value, Some(100));
        assert!(result.is_cycling);
    }

    #[test]
    fn test_create_sequence_no_minmax_values() {
        let sql = "CREATE SEQUENCE [dbo].[UnboundedSeq] NO MINVALUE NO MAXVALUE";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert!(result.min_value.is_none());
        assert!(result.has_no_min_value);
        assert!(result.max_value.is_none());
        assert!(result.has_no_max_value);
    }

    // ========================================================================
    // ALTER SEQUENCE tests
    // ========================================================================

    #[test]
    fn test_alter_sequence_restart() {
        let sql = "ALTER SEQUENCE [dbo].[OrderSequence] RESTART WITH 1000";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "OrderSequence");
        assert_eq!(result.start_value, Some(1000));
        assert!(result.data_type.is_none()); // ALTER cannot change data type
    }

    #[test]
    fn test_alter_sequence_increment() {
        let sql = "ALTER SEQUENCE [dbo].[CounterSeq] INCREMENT BY 5";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "CounterSeq");
        assert_eq!(result.increment_value, Some(5));
    }

    #[test]
    fn test_alter_sequence_minmax() {
        let sql = "ALTER SEQUENCE [dbo].[BoundedSeq] MINVALUE 1 MAXVALUE 10000 CYCLE";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.min_value, Some(1));
        assert_eq!(result.max_value, Some(10000));
        assert!(result.is_cycling);
    }

    #[test]
    fn test_alter_sequence_multiple_options() {
        let sql = "ALTER SEQUENCE [dbo].[ComplexSeq] RESTART WITH 500 INCREMENT BY 10 MINVALUE 1 MAXVALUE 99999 NO CYCLE";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.start_value, Some(500));
        assert_eq!(result.increment_value, Some(10));
        assert_eq!(result.min_value, Some(1));
        assert_eq!(result.max_value, Some(99999));
        assert!(!result.is_cycling);
    }

    #[test]
    fn test_alter_sequence_multiline() {
        let sql = r#"
ALTER SEQUENCE [dbo].[OrderSequence]
RESTART WITH 1000
INCREMENT BY 10
MINVALUE 1
MAXVALUE 999999
NO CYCLE
"#;
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "OrderSequence");
        assert_eq!(result.start_value, Some(1000));
        assert_eq!(result.increment_value, Some(10));
        assert_eq!(result.min_value, Some(1));
        assert_eq!(result.max_value, Some(999999));
        assert!(!result.is_cycling);
    }

    #[test]
    fn test_alter_sequence_custom_schema() {
        let sql = "ALTER SEQUENCE [sales].[InvoiceSeq] RESTART WITH 5000";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "sales");
        assert_eq!(result.name, "InvoiceSeq");
        assert_eq!(result.start_value, Some(5000));
    }

    #[test]
    fn test_alter_sequence_unbracketed() {
        let sql = "ALTER SEQUENCE dbo.Counter RESTART WITH 100";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "Counter");
        assert_eq!(result.start_value, Some(100));
    }

    #[test]
    fn test_alter_sequence_cache() {
        let sql = "ALTER SEQUENCE [dbo].[CachedSeq] CACHE 200";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.cache_size, Some(200));
    }

    #[test]
    fn test_alter_sequence_no_cache() {
        let sql = "ALTER SEQUENCE [dbo].[UncachedSeq] NO CACHE";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.cache_size, Some(0));
    }

    #[test]
    fn test_alter_sequence_case_insensitive() {
        let sql = "alter sequence [dbo].[Counter] restart with 50 increment by 2";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert_eq!(result.name, "Counter");
        assert_eq!(result.start_value, Some(50));
        assert_eq!(result.increment_value, Some(2));
    }

    // ========================================================================
    // Edge cases and negative tests
    // ========================================================================

    #[test]
    fn test_not_a_sequence() {
        let result = parse_create_sequence_tokens("CREATE TABLE [dbo].[Users] (Id INT)");
        assert!(result.is_none());
    }

    #[test]
    fn test_create_on_alter() {
        // CREATE SEQUENCE should not match ALTER SEQUENCE parser
        let result =
            parse_alter_sequence_tokens("CREATE SEQUENCE [dbo].[Counter] AS BIGINT START WITH 1");
        assert!(result.is_none());
    }

    #[test]
    fn test_alter_on_create() {
        // ALTER SEQUENCE should not match CREATE SEQUENCE parser
        let result =
            parse_create_sequence_tokens("ALTER SEQUENCE [dbo].[Counter] RESTART WITH 100");
        assert!(result.is_none());
    }

    #[test]
    fn test_create_procedure_not_sequence() {
        let result = parse_create_sequence_tokens(
            "CREATE PROCEDURE [dbo].[GetUsers] AS SELECT * FROM Users",
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_alter_does_not_change_data_type() {
        // Even if AS keyword appears in ALTER, it should not set data_type
        let sql = "ALTER SEQUENCE [dbo].[Seq] RESTART WITH 1";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        assert!(result.data_type.is_none());
    }

    #[test]
    fn test_alter_start_with_not_recognized() {
        // ALTER uses RESTART, not START
        let sql = "ALTER SEQUENCE [dbo].[Seq] START WITH 100";
        let result = parse_alter_sequence_tokens(sql).unwrap();
        // START WITH should NOT be recognized by ALTER
        assert!(result.start_value.is_none());
    }

    #[test]
    fn test_create_restart_not_recognized() {
        // CREATE uses START, not RESTART
        let sql = "CREATE SEQUENCE [dbo].[Seq] RESTART WITH 100";
        let result = parse_create_sequence_tokens(sql).unwrap();
        // RESTART WITH should NOT be recognized by CREATE
        assert!(result.start_value.is_none());
    }

    #[test]
    fn test_negative_minvalue() {
        let sql = "CREATE SEQUENCE [dbo].[Seq] MINVALUE -9999";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.min_value, Some(-9999));
    }

    #[test]
    fn test_negative_maxvalue() {
        let sql = "CREATE SEQUENCE [dbo].[Seq] MAXVALUE -1";
        let result = parse_create_sequence_tokens(sql).unwrap();
        assert_eq!(result.max_value, Some(-1));
    }
}
