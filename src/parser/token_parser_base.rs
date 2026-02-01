//! Base token parser providing common helper methods for T-SQL parsing.
//!
//! This module provides a shared `TokenParser` struct that consolidates the
//! common helper methods duplicated across all `*TokenParser` implementations.
//!
//! ## Usage
//!
//! Each specialized parser (e.g., `ProcedureTokenParser`, `TriggerTokenParser`)
//! uses composition to include a `TokenParser` and delegate common operations:
//!
//! ```ignore
//! pub struct ProcedureTokenParser {
//!     base: TokenParser,
//! }
//!
//! impl ProcedureTokenParser {
//!     pub fn new(sql: &str) -> Option<Self> {
//!         Some(Self { base: TokenParser::new(sql)? })
//!     }
//!
//!     pub fn parse_create_procedure(&mut self) -> Option<...> {
//!         self.base.skip_whitespace();
//!         if !self.base.check_keyword(Keyword::CREATE) {
//!             return None;
//!         }
//!         self.base.advance();
//!         // ...
//!     }
//! }
//! ```
//!
//! ## Benefits
//!
//! - **Eliminates ~400-500 lines** of duplicated helper methods across 12 parser files
//! - **Single source of truth** for tokenization and token navigation
//! - **Easier maintenance** - bug fixes apply to all parsers
//! - **Consistent behavior** across all T-SQL parsing

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

use super::identifier_utils::format_token;

/// Base token parser with common helper methods for T-SQL parsing.
///
/// This struct encapsulates the token stream and position, providing
/// all the standard navigation and checking methods needed by specialized
/// parsers.
pub struct TokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl TokenParser {
    /// Create a new TokenParser from a SQL string.
    ///
    /// Uses MsSqlDialect for tokenization. Returns `None` if tokenization fails.
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Create a new TokenParser with pre-tokenized tokens.
    ///
    /// Useful when tokens have already been obtained and re-tokenization is not needed.
    pub fn from_tokens(tokens: Vec<TokenWithSpan>) -> Self {
        Self { tokens, pos: 0 }
    }

    // ========================================================================
    // Position and state
    // ========================================================================

    /// Check if at end of tokens.
    #[inline]
    pub fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Get current position in token stream.
    #[inline]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Set current position in token stream.
    #[inline]
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Get the underlying tokens slice.
    #[inline]
    pub fn tokens(&self) -> &[TokenWithSpan] {
        &self.tokens
    }

    // ========================================================================
    // Token access
    // ========================================================================

    /// Get current token without consuming.
    #[inline]
    pub fn current_token(&self) -> Option<&TokenWithSpan> {
        self.tokens.get(self.pos)
    }

    /// Peek at a token at an offset from current position.
    ///
    /// Returns `None` if the offset position is out of bounds.
    #[inline]
    pub fn peek(&self, offset: usize) -> Option<&TokenWithSpan> {
        self.tokens.get(self.pos + offset)
    }

    /// Advance to next token.
    #[inline]
    pub fn advance(&mut self) {
        if !self.is_at_end() {
            self.pos += 1;
        }
    }

    /// Advance by multiple positions.
    #[inline]
    pub fn advance_by(&mut self, count: usize) {
        self.pos = (self.pos + count).min(self.tokens.len());
    }

    // ========================================================================
    // Whitespace handling
    // ========================================================================

    /// Skip whitespace tokens.
    pub fn skip_whitespace(&mut self) {
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

    // ========================================================================
    // Token type checks
    // ========================================================================

    /// Check if current token is a specific keyword.
    #[inline]
    pub fn check_keyword(&self, keyword: Keyword) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.keyword == keyword)
        } else {
            false
        }
    }

    /// Check if current token is a word matching (case-insensitive).
    ///
    /// This is useful for T-SQL keywords that sqlparser doesn't recognize
    /// as keywords (e.g., "INSTEAD", "MINVALUE", "MAXVALUE").
    #[inline]
    pub fn check_word_ci(&self, word: &str) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.value.eq_ignore_ascii_case(word))
        } else {
            false
        }
    }

    /// Check if current token matches a specific token type (by discriminant).
    ///
    /// This compares token types without comparing the inner value.
    /// For example, `check_token(&Token::LParen)` matches any left parenthesis.
    #[inline]
    pub fn check_token(&self, expected: &Token) -> bool {
        if let Some(token) = self.current_token() {
            std::mem::discriminant(&token.token) == std::mem::discriminant(expected)
        } else {
            false
        }
    }

    // ========================================================================
    // Expect methods (check and advance)
    // ========================================================================

    /// Expect a specific keyword, advancing if found.
    ///
    /// Returns `Some(())` if the keyword was found and position advanced,
    /// `None` otherwise (position unchanged).
    pub fn expect_keyword(&mut self, keyword: Keyword) -> Option<()> {
        if self.check_keyword(keyword) {
            self.advance();
            Some(())
        } else {
            None
        }
    }

    /// Expect a specific word (case-insensitive), advancing if found.
    ///
    /// Returns `Some(())` if the word was found and position advanced,
    /// `None` otherwise (position unchanged).
    pub fn expect_word_ci(&mut self, word: &str) -> Option<()> {
        if self.check_word_ci(word) {
            self.advance();
            Some(())
        } else {
            None
        }
    }

    /// Expect a specific token type, advancing if found.
    ///
    /// Returns `Some(())` if the token type was found and position advanced,
    /// `None` otherwise (position unchanged).
    pub fn expect_token(&mut self, expected: &Token) -> Option<()> {
        if self.check_token(expected) {
            self.advance();
            Some(())
        } else {
            None
        }
    }

    // ========================================================================
    // Identifier parsing
    // ========================================================================

    /// Parse an identifier (bracketed or unbracketed).
    ///
    /// Returns the identifier value without brackets/quotes.
    /// Advances position if successful.
    pub fn parse_identifier(&mut self) -> Option<String> {
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

    /// Parse a schema-qualified name: [schema].[name] or schema.name or [name] or name.
    ///
    /// Returns `(schema, name)` tuple. If no schema is present, defaults to "dbo".
    pub fn parse_schema_qualified_name(&mut self) -> Option<(String, String)> {
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

    // ========================================================================
    // Data type parsing
    // ========================================================================

    /// Parse a simple data type (e.g., INT, BIGINT, VARCHAR).
    ///
    /// Returns the uppercase type name. Does not handle type parameters
    /// like precision/scale or MAX - use `parse_data_type_full()` for that.
    pub fn parse_data_type_simple(&mut self) -> Option<String> {
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

    // ========================================================================
    // Numeric parsing
    // ========================================================================

    /// Parse a signed integer (positive or negative).
    ///
    /// Handles optional minus sign followed by a number token.
    pub fn parse_signed_integer(&mut self) -> Option<i64> {
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

    /// Parse a positive integer only.
    pub fn parse_positive_integer(&mut self) -> Option<u64> {
        if self.is_at_end() {
            return None;
        }

        let token = self.current_token()?;
        match &token.token {
            Token::Number(n, _) => {
                if let Ok(value) = n.parse::<u64>() {
                    self.advance();
                    Some(value)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    // ========================================================================
    // Token string conversion
    // ========================================================================

    /// Convert a token to its string representation.
    ///
    /// Delegates to `identifier_utils::format_token()` for consistent formatting.
    #[inline]
    pub fn token_to_string(token: &Token) -> String {
        format_token(token)
    }

    /// Convert a range of tokens to a string.
    ///
    /// Concatenates tokens from `start_pos` to `end_pos` (exclusive).
    pub fn tokens_to_string(&self, start_pos: usize, end_pos: usize) -> String {
        self.tokens[start_pos..end_pos]
            .iter()
            .map(|t| format_token(&t.token))
            .collect()
    }

    // ========================================================================
    // Utility methods
    // ========================================================================

    /// Skip tokens until a specific token type is found.
    ///
    /// The target token is NOT consumed. Position will be at the target or at end.
    pub fn skip_to_token(&mut self, target: &Token) {
        while !self.is_at_end() && !self.check_token(target) {
            self.advance();
        }
    }

    /// Skip tokens until one of several token types is found.
    ///
    /// Returns which token type was found (index into the targets slice),
    /// or `None` if end was reached.
    pub fn skip_to_any_token(&mut self, targets: &[Token]) -> Option<usize> {
        while !self.is_at_end() {
            for (i, target) in targets.iter().enumerate() {
                if self.check_token(target) {
                    return Some(i);
                }
            }
            self.advance();
        }
        None
    }

    /// Skip a parenthesized expression, handling nested parentheses.
    ///
    /// Position should be at the opening parenthesis. After this call,
    /// position will be after the closing parenthesis.
    pub fn skip_parenthesized(&mut self) {
        if !self.check_token(&Token::LParen) {
            return;
        }

        let mut depth = 0;
        while !self.is_at_end() {
            if self.check_token(&Token::LParen) {
                depth += 1;
            } else if self.check_token(&Token::RParen) {
                depth -= 1;
                if depth == 0 {
                    self.advance();
                    return;
                }
            }
            self.advance();
        }
    }

    /// Consume a parenthesized expression and return its contents as a string.
    ///
    /// Position should be at the opening parenthesis. After this call,
    /// position will be after the closing parenthesis.
    /// Returns `None` if not at a left parenthesis.
    pub fn consume_parenthesized(&mut self) -> Option<String> {
        if !self.check_token(&Token::LParen) {
            return None;
        }

        let start_pos = self.pos;
        let mut depth = 0;

        while !self.is_at_end() {
            if self.check_token(&Token::LParen) {
                depth += 1;
            } else if self.check_token(&Token::RParen) {
                depth -= 1;
                if depth == 0 {
                    // Include the closing paren in the string
                    let end_pos = self.pos + 1;
                    self.advance();
                    return Some(self.tokens_to_string(start_pos, end_pos));
                }
            }
            self.advance();
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_parser() {
        let parser = TokenParser::new("SELECT * FROM Users");
        assert!(parser.is_some());
    }

    #[test]
    fn test_is_at_end() {
        let mut parser = TokenParser::new("A").unwrap();
        assert!(!parser.is_at_end());

        // Skip to end
        while !parser.is_at_end() {
            parser.advance();
        }
        assert!(parser.is_at_end());
    }

    #[test]
    fn test_skip_whitespace() {
        let mut parser = TokenParser::new("   SELECT").unwrap();
        parser.skip_whitespace();
        assert!(parser.check_keyword(Keyword::SELECT));
    }

    #[test]
    fn test_check_keyword() {
        let mut parser = TokenParser::new("CREATE TABLE").unwrap();
        parser.skip_whitespace();
        assert!(parser.check_keyword(Keyword::CREATE));
        assert!(!parser.check_keyword(Keyword::SELECT));
    }

    #[test]
    fn test_check_word_ci() {
        let mut parser = TokenParser::new("MINVALUE 100").unwrap();
        parser.skip_whitespace();
        assert!(parser.check_word_ci("MINVALUE"));
        assert!(parser.check_word_ci("minvalue"));
        assert!(parser.check_word_ci("MinValue"));
        assert!(!parser.check_word_ci("MAXVALUE"));
    }

    #[test]
    fn test_check_token() {
        let mut parser = TokenParser::new("(test)").unwrap();
        parser.skip_whitespace();
        assert!(parser.check_token(&Token::LParen));
        assert!(!parser.check_token(&Token::RParen));
    }

    #[test]
    fn test_expect_keyword() {
        let mut parser = TokenParser::new("CREATE TABLE").unwrap();
        parser.skip_whitespace();

        assert!(parser.expect_keyword(Keyword::CREATE).is_some());
        parser.skip_whitespace();
        assert!(parser.check_keyword(Keyword::TABLE));
    }

    #[test]
    fn test_parse_identifier_simple() {
        let mut parser = TokenParser::new("MyTable").unwrap();
        parser.skip_whitespace();

        let ident = parser.parse_identifier();
        assert_eq!(ident, Some("MyTable".to_string()));
    }

    #[test]
    fn test_parse_identifier_bracketed() {
        let mut parser = TokenParser::new("[MyTable]").unwrap();
        parser.skip_whitespace();

        let ident = parser.parse_identifier();
        assert_eq!(ident, Some("MyTable".to_string()));
    }

    #[test]
    fn test_parse_schema_qualified_name() {
        let mut parser = TokenParser::new("[dbo].[Users]").unwrap();
        parser.skip_whitespace();

        let result = parser.parse_schema_qualified_name();
        assert_eq!(result, Some(("dbo".to_string(), "Users".to_string())));
    }

    #[test]
    fn test_parse_schema_qualified_name_unqualified() {
        let mut parser = TokenParser::new("Users").unwrap();
        parser.skip_whitespace();

        let result = parser.parse_schema_qualified_name();
        assert_eq!(result, Some(("dbo".to_string(), "Users".to_string())));
    }

    #[test]
    fn test_parse_signed_integer_positive() {
        let mut parser = TokenParser::new("42").unwrap();
        parser.skip_whitespace();

        assert_eq!(parser.parse_signed_integer(), Some(42));
    }

    #[test]
    fn test_parse_signed_integer_negative() {
        let mut parser = TokenParser::new("-100").unwrap();
        parser.skip_whitespace();

        assert_eq!(parser.parse_signed_integer(), Some(-100));
    }

    #[test]
    fn test_skip_parenthesized() {
        let mut parser = TokenParser::new("(a, (b, c), d) rest").unwrap();
        parser.skip_whitespace();

        parser.skip_parenthesized();
        parser.skip_whitespace();

        // Should be at "rest"
        assert!(parser.check_word_ci("rest"));
    }

    #[test]
    fn test_consume_parenthesized() {
        let mut parser = TokenParser::new("(a, b) rest").unwrap();
        parser.skip_whitespace();

        let content = parser.consume_parenthesized();
        assert!(content.is_some());
        assert!(content.unwrap().starts_with("("));

        parser.skip_whitespace();
        assert!(parser.check_word_ci("rest"));
    }

    #[test]
    fn test_skip_to_token() {
        let mut parser = TokenParser::new("a b c , d e").unwrap();
        parser.skip_whitespace();

        parser.skip_to_token(&Token::Comma);
        assert!(parser.check_token(&Token::Comma));
    }

    #[test]
    fn test_tokens_to_string() {
        let parser = TokenParser::new("[dbo].[Users]").unwrap();
        let s = parser.tokens_to_string(0, parser.tokens().len());
        assert!(s.contains("dbo"));
        assert!(s.contains("Users"));
    }
}
