//! Token-based SQL preprocessing for T-SQL
//!
//! This module provides token-aware preprocessing that correctly handles
//! string literals and comments, replacing the previous regex-based approach.
//! Part of Phase 15.7 of the implementation plan.
//!
//! ## Preprocessing Operations
//!
//! 1. **H1: BINARY/VARBINARY(MAX) Sentinel Replacement**
//!    - Replaces `BINARY(MAX)` and `VARBINARY(MAX)` with sentinel values
//!    - sqlparser expects numeric literals, not MAX for these types
//!    - Correctly ignores occurrences in string literals and comments
//!
//! 2. **H2: DEFAULT FOR Constraint Extraction**
//!    - Extracts `CONSTRAINT [name] DEFAULT (value) FOR [column]` patterns
//!    - This T-SQL syntax isn't supported by sqlparser
//!    - Correctly ignores occurrences in string literals and comments
//!
//! 3. **H3: Trailing Comma Cleanup**
//!    - Removes trailing commas before closing parentheses
//!    - Handles commas followed by whitespace or comments

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

use super::tsql_parser::ExtractedDefaultConstraint;

/// Sentinel value for BINARY(MAX) and VARBINARY(MAX) types
/// Using max i32 value to represent MAX size
pub const BINARY_MAX_SENTINEL: u64 = 2_147_483_647;

/// Result of preprocessing T-SQL using token-based parsing
#[derive(Debug, Clone)]
pub struct TokenPreprocessResult {
    /// SQL with T-SQL-specific syntax converted for sqlparser
    pub sql: String,
    /// Default constraints extracted from the SQL
    pub extracted_defaults: Vec<ExtractedDefaultConstraint>,
}

/// Token-based preprocessor for T-SQL
///
/// This preprocessor works by rebuilding the SQL from tokens, which allows
/// us to selectively modify certain patterns while preserving string literals
/// and comments exactly as they were.
pub struct PreprocessTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl PreprocessTokenParser {
    /// Create a new preprocessor for SQL content
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Preprocess the SQL and return the result
    pub fn preprocess(&mut self) -> TokenPreprocessResult {
        let mut extracted_defaults = Vec::new();
        let mut output_tokens: Vec<String> = Vec::new();

        self.pos = 0;
        while !self.is_at_end() {
            // Check for BINARY(MAX) or VARBINARY(MAX)
            if let Some(replacement) = self.try_parse_binary_max() {
                output_tokens.push(replacement);
                continue;
            }

            // Check for CONSTRAINT [name] DEFAULT (value) FOR [column]
            if let Some(extracted) = self.try_parse_default_for() {
                extracted_defaults.push(extracted);
                // Don't output anything - remove the constraint from SQL
                continue;
            }

            // Check for trailing comma before closing paren (H3)
            if self.is_trailing_comma() {
                // Skip the comma - don't add to output
                self.advance();
                continue;
            }

            // Output the token unchanged
            output_tokens.push(self.token_to_string(&self.tokens[self.pos].token));
            self.advance();
        }

        TokenPreprocessResult {
            sql: output_tokens.join(""),
            extracted_defaults,
        }
    }

    /// Try to parse BINARY(MAX) or VARBINARY(MAX) and return replacement string
    fn try_parse_binary_max(&mut self) -> Option<String> {
        // Don't modify content inside string literals
        if self.is_string_literal() {
            return None;
        }

        let start_pos = self.pos;

        // Check for BINARY or VARBINARY keyword
        if !self.check_word_ci("BINARY") && !self.check_word_ci("VARBINARY") {
            return None;
        }

        let type_name = self.get_word_value()?.to_uppercase();
        self.advance();

        // Collect any whitespace
        let mut whitespace = String::new();
        while !self.is_at_end() && matches!(&self.tokens[self.pos].token, Token::Whitespace(_)) {
            whitespace.push_str(&self.token_to_string(&self.tokens[self.pos].token));
            self.advance();
        }

        // Check for opening paren
        if !self.check_token(&Token::LParen) {
            self.pos = start_pos;
            return None;
        }
        self.advance();

        // Skip whitespace inside parens
        while !self.is_at_end() && matches!(&self.tokens[self.pos].token, Token::Whitespace(_)) {
            self.advance();
        }

        // Check for MAX keyword
        if !self.check_keyword(Keyword::MAX) {
            self.pos = start_pos;
            return None;
        }
        self.advance();

        // Skip whitespace inside parens
        while !self.is_at_end() && matches!(&self.tokens[self.pos].token, Token::Whitespace(_)) {
            self.advance();
        }

        // Check for closing paren
        if !self.check_token(&Token::RParen) {
            self.pos = start_pos;
            return None;
        }
        self.advance();

        // Return replacement with sentinel value
        Some(format!(
            "{}{}({})",
            type_name, whitespace, BINARY_MAX_SENTINEL
        ))
    }

    /// Try to parse CONSTRAINT [name] DEFAULT (value) FOR [column]
    fn try_parse_default_for(&mut self) -> Option<ExtractedDefaultConstraint> {
        // Don't process content inside string literals
        if self.is_string_literal() {
            return None;
        }

        let start_pos = self.pos;

        // Check for optional leading comma
        if self.check_token(&Token::Comma) {
            self.advance();
            self.skip_whitespace();
        }

        // Check for CONSTRAINT keyword
        if !self.check_keyword(Keyword::CONSTRAINT) {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse constraint name
        let constraint_name = self.parse_identifier()?;
        if constraint_name.is_empty() {
            self.pos = start_pos;
            return None;
        }
        self.skip_whitespace();

        // Check for DEFAULT keyword
        if !self.check_keyword(Keyword::DEFAULT) {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse default expression (parenthesized)
        if !self.check_token(&Token::LParen) {
            self.pos = start_pos;
            return None;
        }

        let expression = self.parse_parenthesized_expression()?;
        self.skip_whitespace();

        // Check for FOR keyword - this is what distinguishes DEFAULT FOR from inline DEFAULT
        if !self.check_keyword(Keyword::FOR) {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse column name
        let column_name = self.parse_identifier()?;
        if column_name.is_empty() {
            self.pos = start_pos;
            return None;
        }

        Some(ExtractedDefaultConstraint {
            name: constraint_name,
            column: column_name,
            expression: format!("({})", expression),
        })
    }

    /// Check if current position has a trailing comma (comma followed only by whitespace and close paren)
    fn is_trailing_comma(&self) -> bool {
        if !self.check_token(&Token::Comma) {
            return false;
        }

        // Look ahead, skipping whitespace, for closing paren
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            match &self.tokens[i].token {
                Token::Whitespace(_) => {
                    i += 1;
                    continue;
                }
                Token::RParen => return true,
                _ => return false,
            }
        }
        false
    }

    /// Check if current token is a string literal
    fn is_string_literal(&self) -> bool {
        if self.is_at_end() {
            return false;
        }
        matches!(
            &self.tokens[self.pos].token,
            Token::SingleQuotedString(_)
                | Token::NationalStringLiteral(_)
                | Token::HexStringLiteral(_)
                | Token::DoubleQuotedString(_)
                | Token::SingleQuotedByteStringLiteral(_)
                | Token::DoubleQuotedByteStringLiteral(_)
        )
    }

    // === Helper methods ===

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.pos += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            if matches!(&self.tokens[self.pos].token, Token::Whitespace(_)) {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn check_token(&self, expected: &Token) -> bool {
        if self.is_at_end() {
            return false;
        }
        std::mem::discriminant(&self.tokens[self.pos].token) == std::mem::discriminant(expected)
    }

    fn check_keyword(&self, keyword: Keyword) -> bool {
        if self.is_at_end() {
            return false;
        }
        matches!(
            &self.tokens[self.pos].token,
            Token::Word(w) if w.keyword == keyword
        )
    }

    fn check_word_ci(&self, word: &str) -> bool {
        if self.is_at_end() {
            return false;
        }
        matches!(
            &self.tokens[self.pos].token,
            Token::Word(w) if w.value.eq_ignore_ascii_case(word)
        )
    }

    fn get_word_value(&self) -> Option<&str> {
        if self.is_at_end() {
            return None;
        }
        match &self.tokens[self.pos].token {
            Token::Word(w) => Some(&w.value),
            _ => None,
        }
    }

    /// Parse an identifier (quoted or unquoted)
    fn parse_identifier(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        match &self.tokens[self.pos].token {
            Token::Word(w) => {
                let name = w.value.clone();
                self.advance();
                Some(name)
            }
            _ => None,
        }
    }

    /// Parse a parenthesized expression, handling nested parentheses
    /// Returns the expression content (without outer parentheses)
    fn parse_parenthesized_expression(&mut self) -> Option<String> {
        if !self.check_token(&Token::LParen) {
            return None;
        }

        self.advance(); // Skip opening paren

        let mut result = String::new();
        let mut depth = 1;

        while !self.is_at_end() && depth > 0 {
            match &self.tokens[self.pos].token {
                Token::LParen => {
                    depth += 1;
                    result.push_str(&self.token_to_string(&self.tokens[self.pos].token));
                }
                Token::RParen => {
                    depth -= 1;
                    if depth > 0 {
                        result.push_str(&self.token_to_string(&self.tokens[self.pos].token));
                    }
                }
                _ => {
                    result.push_str(&self.token_to_string(&self.tokens[self.pos].token));
                }
            }
            self.advance();
        }

        if depth != 0 {
            return None;
        }

        Some(result)
    }

    /// Convert a token back to its string representation
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
            Token::Char(c) => c.to_string(),
            Token::SingleQuotedString(s) => format!("'{}'", s.replace('\'', "''")),
            Token::NationalStringLiteral(s) => format!("N'{}'", s.replace('\'', "''")),
            Token::HexStringLiteral(s) => format!("0x{}", s),
            Token::DoubleQuotedString(s) => format!("\"{}\"", s),
            Token::SingleQuotedByteStringLiteral(s) => format!("b'{}'", s),
            Token::DoubleQuotedByteStringLiteral(s) => format!("b\"{}\"", s),
            Token::LParen => "(".to_string(),
            Token::RParen => ")".to_string(),
            Token::LBrace => "{".to_string(),
            Token::RBrace => "}".to_string(),
            Token::LBracket => "[".to_string(),
            Token::RBracket => "]".to_string(),
            Token::Comma => ",".to_string(),
            Token::Period => ".".to_string(),
            Token::Colon => ":".to_string(),
            Token::DoubleColon => "::".to_string(),
            Token::SemiColon => ";".to_string(),
            Token::Whitespace(w) => w.to_string(),
            Token::Eq => "=".to_string(),
            Token::Neq => "<>".to_string(),
            Token::Lt => "<".to_string(),
            Token::Gt => ">".to_string(),
            Token::LtEq => "<=".to_string(),
            Token::GtEq => ">=".to_string(),
            Token::Spaceship => "<=>".to_string(),
            Token::Plus => "+".to_string(),
            Token::Minus => "-".to_string(),
            Token::Mul => "*".to_string(),
            Token::Div => "/".to_string(),
            Token::Mod => "%".to_string(),
            Token::StringConcat => "||".to_string(),
            Token::LongArrow => "->>".to_string(),
            Token::Arrow => "->".to_string(),
            Token::HashArrow => "#>".to_string(),
            Token::HashLongArrow => "#>>".to_string(),
            Token::AtSign => "@".to_string(),
            Token::Sharp => "#".to_string(),
            Token::Ampersand => "&".to_string(),
            Token::Pipe => "|".to_string(),
            Token::Caret => "^".to_string(),
            Token::Tilde => "~".to_string(),
            Token::ExclamationMark => "!".to_string(),
            Token::DollarQuotedString(s) => s.to_string(),
            _ => String::new(), // Handle unknown tokens gracefully
        }
    }
}

/// Preprocess T-SQL using token-based parsing
///
/// This is the main entry point that replaces the regex-based preprocessing.
/// It correctly handles string literals and comments.
pub fn preprocess_tsql_tokens(sql: &str) -> TokenPreprocessResult {
    match PreprocessTokenParser::new(sql) {
        Some(mut parser) => parser.preprocess(),
        None => {
            // If tokenization fails, return unchanged
            TokenPreprocessResult {
                sql: sql.to_string(),
                extracted_defaults: Vec::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === H1: BINARY/VARBINARY(MAX) Tests ===

    #[test]
    fn test_binary_max_replacement() {
        let sql = "CREATE TABLE T ([Data] BINARY(MAX))";
        let result = preprocess_tsql_tokens(sql);
        assert!(result
            .sql
            .contains(&format!("BINARY({})", BINARY_MAX_SENTINEL)));
        assert!(!result.sql.contains("BINARY(MAX)"));
    }

    #[test]
    fn test_varbinary_max_replacement() {
        let sql = "CREATE TABLE T ([Data] VARBINARY(MAX))";
        let result = preprocess_tsql_tokens(sql);
        assert!(result
            .sql
            .contains(&format!("VARBINARY({})", BINARY_MAX_SENTINEL)));
        assert!(!result.sql.contains("VARBINARY(MAX)"));
    }

    #[test]
    fn test_binary_max_case_insensitive() {
        let sql = "CREATE TABLE T ([Data] varbinary ( max ))";
        let result = preprocess_tsql_tokens(sql);
        // Should contain the sentinel value (whitespace between type and paren may vary)
        assert!(result.sql.contains(&BINARY_MAX_SENTINEL.to_string()));
        assert!(!result.sql.contains("max"));
        assert!(!result.sql.contains("MAX"));
    }

    #[test]
    fn test_binary_max_in_string_not_replaced() {
        let sql = "SELECT 'Use VARBINARY(MAX) for files' AS hint";
        let result = preprocess_tsql_tokens(sql);
        assert!(result.sql.contains("VARBINARY(MAX)"));
        assert!(!result.sql.contains(&BINARY_MAX_SENTINEL.to_string()));
    }

    #[test]
    fn test_binary_max_in_national_string_not_replaced() {
        let sql = "SELECT N'Use BINARY(MAX) for data' AS hint";
        let result = preprocess_tsql_tokens(sql);
        assert!(result.sql.contains("BINARY(MAX)"));
    }

    #[test]
    fn test_multiple_binary_max_replacements() {
        let sql = "CREATE TABLE T ([A] BINARY(MAX), [B] VARBINARY(MAX))";
        let result = preprocess_tsql_tokens(sql);
        // Both should be replaced
        assert!(!result.sql.contains("BINARY(MAX)"));
        assert!(!result.sql.contains("VARBINARY(MAX)"));
        // Count sentinel occurrences
        let count = result.sql.matches(&BINARY_MAX_SENTINEL.to_string()).count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_binary_with_regular_size_not_replaced() {
        let sql = "CREATE TABLE T ([Data] BINARY(100))";
        let result = preprocess_tsql_tokens(sql);
        assert!(result.sql.contains("BINARY(100)"));
        assert!(!result.sql.contains(&BINARY_MAX_SENTINEL.to_string()));
    }

    // === H2: DEFAULT FOR Tests ===

    #[test]
    fn test_default_for_extraction() {
        let sql = "CREATE TABLE T ([Id] INT, CONSTRAINT [DF_Active] DEFAULT (1) FOR [Active])";
        let result = preprocess_tsql_tokens(sql);

        assert_eq!(result.extracted_defaults.len(), 1);
        assert_eq!(result.extracted_defaults[0].name, "DF_Active");
        assert_eq!(result.extracted_defaults[0].column, "Active");
        assert_eq!(result.extracted_defaults[0].expression, "(1)");

        // Should be removed from SQL
        assert!(!result.sql.contains("CONSTRAINT [DF_Active]"));
        assert!(!result.sql.contains("DEFAULT (1) FOR"));
    }

    #[test]
    fn test_default_for_with_complex_expression() {
        let sql =
            "CREATE TABLE T ([Id] INT, CONSTRAINT [DF_Date] DEFAULT (GETDATE()) FOR [Created])";
        let result = preprocess_tsql_tokens(sql);

        assert_eq!(result.extracted_defaults.len(), 1);
        assert_eq!(result.extracted_defaults[0].expression, "(GETDATE())");
    }

    #[test]
    fn test_default_for_with_nested_parens() {
        let sql = "CREATE TABLE T ([Id] INT, CONSTRAINT [DF_Val] DEFAULT ((1)+(2)) FOR [Value])";
        let result = preprocess_tsql_tokens(sql);

        assert_eq!(result.extracted_defaults.len(), 1);
        assert_eq!(result.extracted_defaults[0].expression, "((1)+(2))");
    }

    #[test]
    fn test_multiple_default_for_extraction() {
        let sql = "CREATE TABLE T (
            [Id] INT,
            CONSTRAINT [DF_A] DEFAULT (1) FOR [A],
            CONSTRAINT [DF_B] DEFAULT (0) FOR [B]
        )";
        let result = preprocess_tsql_tokens(sql);

        assert_eq!(result.extracted_defaults.len(), 2);
        assert_eq!(result.extracted_defaults[0].name, "DF_A");
        assert_eq!(result.extracted_defaults[1].name, "DF_B");
    }

    #[test]
    fn test_default_for_in_string_not_extracted() {
        let sql = "SELECT 'CONSTRAINT [DF_Test] DEFAULT (1) FOR [Col]' AS note";
        let result = preprocess_tsql_tokens(sql);

        assert_eq!(result.extracted_defaults.len(), 0);
        // Original string should be preserved
        assert!(result
            .sql
            .contains("CONSTRAINT [DF_Test] DEFAULT (1) FOR [Col]"));
    }

    #[test]
    fn test_inline_default_not_extracted() {
        // Inline DEFAULT (without FOR) should not be extracted
        let sql = "CREATE TABLE T ([Active] BIT CONSTRAINT [DF_Active] DEFAULT (1))";
        let result = preprocess_tsql_tokens(sql);

        // No extraction - this is inline DEFAULT, not DEFAULT FOR
        assert_eq!(result.extracted_defaults.len(), 0);
        // SQL should be unchanged
        assert!(result.sql.contains("CONSTRAINT [DF_Active] DEFAULT (1)"));
    }

    // === H3: Trailing Comma Tests ===

    #[test]
    fn test_trailing_comma_removal() {
        // After removing DEFAULT FOR, there might be a trailing comma
        let sql = "CREATE TABLE T ([Id] INT, CONSTRAINT [DF_A] DEFAULT (1) FOR [A])";
        let result = preprocess_tsql_tokens(sql);

        // The comma before CONSTRAINT should be removed along with the constraint
        assert!(!result.sql.contains(", )"));
        assert!(!result.sql.contains(",)"));
    }

    #[test]
    fn test_trailing_comma_with_whitespace() {
        let sql = "CREATE TABLE T ([Id] INT ,  )";
        let result = preprocess_tsql_tokens(sql);

        // Trailing comma should be removed
        assert!(!result.sql.contains(","));
        assert!(result.sql.contains("[Id] INT"));
    }

    #[test]
    fn test_no_trailing_comma_in_string() {
        let sql = "SELECT 'value, )' AS test";
        let result = preprocess_tsql_tokens(sql);

        // String content should be preserved
        assert!(result.sql.contains("value, )"));
    }

    // === Integration Tests ===

    #[test]
    fn test_combined_preprocessing() {
        let sql = "CREATE TABLE Products (
            [Id] INT,
            [Data] VARBINARY(MAX),
            [Active] BIT,
            CONSTRAINT [DF_Active] DEFAULT (1) FOR [Active]
        )";
        let result = preprocess_tsql_tokens(sql);

        // VARBINARY(MAX) replaced
        assert!(!result.sql.contains("VARBINARY(MAX)"));
        assert!(result.sql.contains(&BINARY_MAX_SENTINEL.to_string()));

        // DEFAULT FOR extracted
        assert_eq!(result.extracted_defaults.len(), 1);
        assert!(!result.sql.contains("DEFAULT (1) FOR"));
    }

    #[test]
    fn test_preserve_sql_structure() {
        let sql = "CREATE TABLE T ([Id] INT NOT NULL PRIMARY KEY)";
        let result = preprocess_tsql_tokens(sql);

        // No transformations needed, SQL should be essentially identical
        // (might have minor whitespace differences due to token reconstruction)
        assert!(result.extracted_defaults.is_empty());
        assert!(result.sql.contains("[Id]"));
        assert!(result.sql.contains("INT"));
        assert!(result.sql.contains("NOT NULL"));
        assert!(result.sql.contains("PRIMARY KEY"));
    }
}
