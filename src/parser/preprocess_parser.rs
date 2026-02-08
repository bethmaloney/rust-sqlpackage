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
//!
//! ## Performance Optimizations (Phase 16.2.3)
//!
//! - Uses single `String` buffer with pre-allocated capacity instead of `Vec<String>.join()`
//! - Uses `format_token_sql_cow()` from `identifier_utils` to avoid allocations for static tokens

use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::Token;

use super::identifier_utils::format_token_sql_cow;
use super::token_parser_base::TokenParser;
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
    base: TokenParser,
}

impl PreprocessTokenParser {
    /// Create a new preprocessor for SQL content
    pub fn new(sql: &str) -> Option<Self> {
        Some(Self {
            base: TokenParser::new(sql)?,
        })
    }

    /// Preprocess the SQL and return the result
    ///
    /// Optimized to use a single String buffer with pre-allocated capacity
    /// instead of Vec<String>.join("") to reduce allocations.
    pub fn preprocess(&mut self) -> TokenPreprocessResult {
        let mut extracted_defaults = Vec::new();
        // Pre-allocate output buffer - estimate roughly same size as input
        // The actual capacity is based on token count * average token size
        let estimated_capacity = self.base.tokens().len() * 4; // Average ~4 chars per token
        let mut output = String::with_capacity(estimated_capacity);

        self.base.set_pos(0);
        while !self.base.is_at_end() {
            // Check for BINARY(MAX) or VARBINARY(MAX)
            if let Some(replacement) = self.try_parse_binary_max() {
                output.push_str(&replacement);
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
                self.base.advance();
                continue;
            }

            // Output the token unchanged - uses format_token_sql_cow to avoid allocations for static tokens
            if let Some(token) = self.base.current_token() {
                output.push_str(&format_token_sql_cow(&token.token));
            }
            self.base.advance();
        }

        TokenPreprocessResult {
            sql: output,
            extracted_defaults,
        }
    }

    /// Try to parse BINARY(MAX) or VARBINARY(MAX) and return replacement string
    fn try_parse_binary_max(&mut self) -> Option<String> {
        // Don't modify content inside string literals
        if self.is_string_literal() {
            return None;
        }

        let start_pos = self.base.pos();

        // Check for BINARY or VARBINARY keyword
        if !self.base.check_word_ci("BINARY") && !self.base.check_word_ci("VARBINARY") {
            return None;
        }

        let type_name = self.get_word_value()?.to_uppercase();
        self.base.advance();

        // Collect any whitespace - pre-allocate with small capacity since whitespace is typically short
        let mut whitespace = String::with_capacity(4);
        while !self.base.is_at_end() {
            if let Some(token) = self.base.current_token() {
                if matches!(&token.token, Token::Whitespace(_)) {
                    whitespace.push_str(&format_token_sql_cow(&token.token));
                    self.base.advance();
                    continue;
                }
            }
            break;
        }

        // Check for opening paren
        if !self.base.check_token(&Token::LParen) {
            self.base.set_pos(start_pos);
            return None;
        }
        self.base.advance();

        // Skip whitespace inside parens
        self.base.skip_whitespace();

        // Check for MAX keyword
        if !self.base.check_keyword(Keyword::MAX) {
            self.base.set_pos(start_pos);
            return None;
        }
        self.base.advance();

        // Skip whitespace inside parens
        self.base.skip_whitespace();

        // Check for closing paren
        if !self.base.check_token(&Token::RParen) {
            self.base.set_pos(start_pos);
            return None;
        }
        self.base.advance();

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

        let start_pos = self.base.pos();

        // Check for optional leading comma
        if self.base.check_token(&Token::Comma) {
            self.base.advance();
            self.base.skip_whitespace();
        }

        // Check for CONSTRAINT keyword
        if !self.base.check_keyword(Keyword::CONSTRAINT) {
            self.base.set_pos(start_pos);
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse constraint name
        let constraint_name = self.base.parse_identifier()?;
        if constraint_name.is_empty() {
            self.base.set_pos(start_pos);
            return None;
        }
        self.base.skip_whitespace();

        // Check for DEFAULT keyword
        if !self.base.check_keyword(Keyword::DEFAULT) {
            self.base.set_pos(start_pos);
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse default expression (parenthesized)
        if !self.base.check_token(&Token::LParen) {
            self.base.set_pos(start_pos);
            return None;
        }

        let expression = self.parse_parenthesized_expression()?;
        self.base.skip_whitespace();

        // Check for FOR keyword - this is what distinguishes DEFAULT FOR from inline DEFAULT
        if !self.base.check_keyword(Keyword::FOR) {
            self.base.set_pos(start_pos);
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse column name
        let column_name = self.base.parse_identifier()?;
        if column_name.is_empty() {
            self.base.set_pos(start_pos);
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
        if !self.base.check_token(&Token::Comma) {
            return false;
        }

        // Look ahead, skipping whitespace, for closing paren
        let tokens = self.base.tokens();
        let mut i = self.base.pos() + 1;
        while i < tokens.len() {
            match &tokens[i].token {
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
        if let Some(token) = self.base.current_token() {
            matches!(
                &token.token,
                Token::SingleQuotedString(_)
                    | Token::NationalStringLiteral(_)
                    | Token::HexStringLiteral(_)
                    | Token::DoubleQuotedString(_)
                    | Token::SingleQuotedByteStringLiteral(_)
                    | Token::DoubleQuotedByteStringLiteral(_)
            )
        } else {
            false
        }
    }

    // === Helper methods unique to PreprocessTokenParser ===

    /// Get the current word value without consuming it
    fn get_word_value(&self) -> Option<&str> {
        if let Some(token) = self.base.current_token() {
            match &token.token {
                Token::Word(w) => Some(&w.value),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Parse a parenthesized expression, handling nested parentheses
    /// Returns the expression content (without outer parentheses)
    ///
    /// Pre-allocates result buffer based on remaining tokens to reduce reallocations.
    fn parse_parenthesized_expression(&mut self) -> Option<String> {
        if !self.base.check_token(&Token::LParen) {
            return None;
        }

        self.base.advance(); // Skip opening paren

        // Estimate capacity based on remaining tokens (average ~4 chars per token)
        let remaining_tokens = self.base.tokens().len().saturating_sub(self.base.pos());
        let estimated_capacity = remaining_tokens.min(64) * 4; // Cap at 256 bytes initial
        let mut result = String::with_capacity(estimated_capacity);
        let mut depth = 1;

        while !self.base.is_at_end() && depth > 0 {
            if let Some(token) = self.base.current_token() {
                match &token.token {
                    Token::LParen => {
                        depth += 1;
                        result.push_str(&format_token_sql_cow(&token.token));
                    }
                    Token::RParen => {
                        depth -= 1;
                        if depth > 0 {
                            result.push_str(&format_token_sql_cow(&token.token));
                        }
                    }
                    _ => {
                        result.push_str(&format_token_sql_cow(&token.token));
                    }
                }
            }
            self.base.advance();
        }

        if depth != 0 {
            return None;
        }

        Some(result)
    }
}

/// Check if SQL text might need preprocessing transformations.
///
/// This is a fast-path check that scans the raw bytes for trigger patterns:
/// - `BINARY` (case-insensitive) — triggers H1 (BINARY/VARBINARY MAX replacement)
/// - `DEFAULT` (case-insensitive) — triggers H2 (DEFAULT FOR constraint extraction)
/// - `,` followed by optional whitespace then `)` — triggers H3 (trailing comma cleanup)
///
/// If none of these patterns are found, we can skip the expensive tokenize-and-reconstruct
/// entirely and return the input unchanged (zero-alloc fast path).
fn needs_preprocessing(sql: &str) -> bool {
    let bytes = sql.as_bytes();

    // Check for BINARY (covers both BINARY and VARBINARY) — case-insensitive
    // Check for DEFAULT — case-insensitive
    let mut has_binary = false;
    let mut has_default = false;
    for window in bytes.windows(7) {
        if !has_binary && window.len() >= 6 && window[..6].eq_ignore_ascii_case(b"BINARY") {
            has_binary = true;
        }
        if window.eq_ignore_ascii_case(b"DEFAULT") {
            has_default = true;
        }
        if has_binary && has_default {
            return true;
        }
    }
    // Also check for 6-byte BINARY if the last window was too short
    if !has_binary && bytes.len() >= 6 {
        let tail = &bytes[bytes.len().saturating_sub(6)..];
        if tail.eq_ignore_ascii_case(b"BINARY") {
            has_binary = true;
        }
    }

    if has_binary || has_default {
        return true;
    }

    // Check for trailing comma: `,` followed by optional whitespace then `)`
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b',' {
            let mut j = i + 1;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\r' | b'\n') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b')' {
                return true;
            }
        }
        i += 1;
    }

    false
}

/// Preprocess T-SQL using token-based parsing
///
/// This is the main entry point that replaces the regex-based preprocessing.
/// It correctly handles string literals and comments.
///
/// Includes a fast-path bypass (Phase 74): if the SQL contains none of the
/// trigger patterns (BINARY, DEFAULT, trailing comma before `)`) then the
/// input is returned unchanged without tokenization.
pub fn preprocess_tsql_tokens(sql: &str) -> TokenPreprocessResult {
    // Fast path: skip tokenization entirely when no transformations are needed
    if !needs_preprocessing(sql) {
        return TokenPreprocessResult {
            sql: sql.to_string(),
            extracted_defaults: Vec::new(),
        };
    }

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

    // === Fast-path bypass tests (Phase 74) ===

    #[test]
    fn test_needs_preprocessing_binary() {
        assert!(needs_preprocessing("CREATE TABLE T ([Data] BINARY(MAX))"));
        assert!(needs_preprocessing(
            "CREATE TABLE T ([Data] varbinary(MAX))"
        ));
        assert!(needs_preprocessing("CREATE TABLE T ([Data] binary(100))"));
    }

    #[test]
    fn test_needs_preprocessing_default() {
        assert!(needs_preprocessing(
            "CREATE TABLE T ([Id] INT, CONSTRAINT [DF_A] DEFAULT (1) FOR [A])"
        ));
        assert!(needs_preprocessing(
            "CREATE TABLE T ([Active] BIT DEFAULT 1)"
        ));
    }

    #[test]
    fn test_needs_preprocessing_trailing_comma() {
        assert!(needs_preprocessing("CREATE TABLE T ([Id] INT,)"));
        assert!(needs_preprocessing("CREATE TABLE T ([Id] INT, )"));
        assert!(needs_preprocessing("CREATE TABLE T ([Id] INT,\n)"));
    }

    #[test]
    fn test_needs_preprocessing_false_for_simple_sql() {
        // No BINARY, DEFAULT, or trailing comma — fast path should apply
        assert!(!needs_preprocessing(
            "CREATE TABLE T ([Id] INT NOT NULL PRIMARY KEY)"
        ));
        assert!(!needs_preprocessing(
            "CREATE PROCEDURE [dbo].[MyProc] AS SELECT 1"
        ));
        assert!(!needs_preprocessing("SELECT 1"));
        assert!(!needs_preprocessing(
            "CREATE VIEW [dbo].[MyView] AS SELECT [Id] FROM [T]"
        ));
        assert!(!needs_preprocessing("CREATE INDEX IX_T ON [T] ([Id])"));
    }

    #[test]
    fn test_fast_path_returns_identical_sql() {
        // When fast path applies, the returned SQL should be identical to input
        let sql = "CREATE PROCEDURE [dbo].[MyProc] AS SELECT 1";
        let result = preprocess_tsql_tokens(sql);
        assert_eq!(result.sql, sql);
        assert!(result.extracted_defaults.is_empty());
    }

    #[test]
    fn test_fast_path_comma_in_normal_position_not_triggered() {
        // Commas followed by something other than ) should not trigger
        assert!(!needs_preprocessing(
            "CREATE TABLE T ([Id] INT, [Name] NVARCHAR(100))"
        ));
    }
}
