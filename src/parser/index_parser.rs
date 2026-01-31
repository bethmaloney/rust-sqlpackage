//! Token-based index definition parsing for T-SQL
//!
//! This module provides token-based parsing for CREATE INDEX statements, replacing
//! the previous regex-based approach. Part of Phase 15.3 (B6) of the implementation plan.
//!
//! ## Supported Syntax
//!
//! ```sql
//! CREATE [UNIQUE] [CLUSTERED | NONCLUSTERED] INDEX [name] ON [schema].[table] (columns)
//! CREATE NONCLUSTERED INDEX [IX_Name] ON [dbo].[Table] ([Col1], [Col2] DESC)
//! CREATE UNIQUE CLUSTERED INDEX [IX_Name] ON [dbo].[Table] ([Col]) INCLUDE ([Col2])
//! CREATE NONCLUSTERED INDEX [IX_Name] ON [dbo].[Table] ([Col]) WHERE [Status] = 'Active'
//! CREATE NONCLUSTERED INDEX [IX_Name] ON [dbo].[Table] ([Col]) WITH (FILLFACTOR = 80)
//! ```

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

/// Result of parsing an index definition using tokens
#[derive(Debug, Clone, Default)]
pub struct TokenParsedIndex {
    /// Index name
    pub name: String,
    /// Schema of the table (defaults to "dbo" if not specified)
    pub table_schema: String,
    /// Table name the index is on
    pub table_name: String,
    /// Key columns in the index (column names only, stripped of ASC/DESC)
    pub columns: Vec<String>,
    /// Columns included in the index leaf level (INCLUDE clause)
    pub include_columns: Vec<String>,
    /// Whether the index is UNIQUE
    pub is_unique: bool,
    /// Whether the index is CLUSTERED (false = NONCLUSTERED)
    pub is_clustered: bool,
    /// Fill factor percentage (0-100)
    pub fill_factor: Option<u8>,
    /// Filter predicate for filtered indexes (WHERE clause condition)
    pub filter_predicate: Option<String>,
    /// Data compression type (NONE, ROW, PAGE, etc.)
    pub data_compression: Option<String>,
}

/// Token-based index definition parser
pub struct IndexTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl IndexTokenParser {
    /// Create a new parser for an index definition string
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Parse CREATE INDEX and return index info
    pub fn parse_create_index(&mut self) -> Option<TokenParsedIndex> {
        self.skip_whitespace();

        // Expect CREATE keyword
        if !self.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        let mut is_unique = false;
        let mut is_clustered = false;
        let mut has_clustering_spec = false;

        // Parse optional UNIQUE
        if self.check_keyword(Keyword::UNIQUE) {
            is_unique = true;
            self.advance();
            self.skip_whitespace();
        }

        // Parse optional CLUSTERED or NONCLUSTERED
        if self.check_word_ci("CLUSTERED") {
            is_clustered = true;
            has_clustering_spec = true;
            self.advance();
            self.skip_whitespace();
        } else if self.check_word_ci("NONCLUSTERED") {
            is_clustered = false;
            has_clustering_spec = true;
            self.advance();
            self.skip_whitespace();
        }

        // Expect INDEX keyword
        if !self.check_keyword(Keyword::INDEX) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // This parser only handles T-SQL specific indexes with CLUSTERED/NONCLUSTERED
        // Standard CREATE INDEX statements are handled by sqlparser directly
        if !has_clustering_spec {
            return None;
        }

        // Parse index name
        let name = self.parse_identifier()?;
        self.skip_whitespace();

        // Expect ON keyword
        if !self.check_keyword(Keyword::ON) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse table name (schema-qualified)
        let (table_schema, table_name) = self.parse_schema_qualified_name()?;
        self.skip_whitespace();

        // Parse column list
        let columns = self.parse_column_list()?;
        self.skip_whitespace();

        // Initialize result
        let mut result = TokenParsedIndex {
            name,
            table_schema,
            table_name,
            columns,
            include_columns: Vec::new(),
            is_unique,
            is_clustered,
            fill_factor: None,
            filter_predicate: None,
            data_compression: None,
        };

        // Parse optional clauses: INCLUDE, WHERE, WITH
        self.parse_index_options(&mut result);

        Some(result)
    }

    /// Parse optional index clauses: INCLUDE, WHERE, WITH
    fn parse_index_options(&mut self, result: &mut TokenParsedIndex) {
        while !self.is_at_end() {
            self.skip_whitespace();

            if self.is_at_end() {
                break;
            }

            // Check for INCLUDE clause
            if self.check_keyword(Keyword::INCLUDE) {
                self.advance();
                self.skip_whitespace();
                if let Some(cols) = self.parse_column_list() {
                    result.include_columns = cols;
                }
                continue;
            }

            // Check for WHERE clause (filtered index)
            if self.check_keyword(Keyword::WHERE) {
                self.advance();
                self.skip_whitespace();
                if let Some(predicate) = self.parse_filter_predicate() {
                    result.filter_predicate = Some(predicate);
                }
                continue;
            }

            // Check for WITH clause (index options)
            if self.check_keyword(Keyword::WITH) {
                self.advance();
                self.skip_whitespace();
                self.parse_with_options(result);
                continue;
            }

            // Check for semicolon (end of statement)
            if self.check_token(&Token::SemiColon) {
                break;
            }

            // Unknown token, advance
            self.advance();
        }
    }

    /// Parse WITH clause options: FILLFACTOR, DATA_COMPRESSION, etc.
    fn parse_with_options(&mut self, result: &mut TokenParsedIndex) {
        // Expect opening parenthesis
        if !self.check_token(&Token::LParen) {
            return;
        }
        self.advance();
        self.skip_whitespace();

        // Parse options until closing parenthesis
        while !self.is_at_end() && !self.check_token(&Token::RParen) {
            self.skip_whitespace();

            // Check for FILLFACTOR
            if self.check_word_ci("FILLFACTOR") {
                self.advance();
                self.skip_whitespace();
                if self.check_token(&Token::Eq) {
                    self.advance();
                    self.skip_whitespace();
                    if let Some(value) = self.parse_positive_integer() {
                        if value <= 100 {
                            result.fill_factor = Some(value as u8);
                        }
                    }
                }
                self.skip_to_comma_or_paren();
                continue;
            }

            // Check for DATA_COMPRESSION
            if self.check_word_ci("DATA_COMPRESSION") {
                self.advance();
                self.skip_whitespace();
                if self.check_token(&Token::Eq) {
                    self.advance();
                    self.skip_whitespace();
                    if let Some(compression) = self.parse_identifier() {
                        result.data_compression = Some(compression.to_uppercase());
                    }
                }
                self.skip_to_comma_or_paren();
                continue;
            }

            // Skip other options we don't care about (PAD_INDEX, SORT_IN_TEMPDB, etc.)
            self.skip_to_comma_or_paren();
        }

        // Consume closing parenthesis
        if self.check_token(&Token::RParen) {
            self.advance();
        }
    }

    /// Skip tokens until we hit a comma or right parenthesis
    fn skip_to_comma_or_paren(&mut self) {
        while !self.is_at_end() {
            if self.check_token(&Token::Comma) {
                self.advance();
                break;
            }
            if self.check_token(&Token::RParen) {
                break;
            }
            self.advance();
        }
    }

    /// Parse the filter predicate from a WHERE clause
    /// This captures everything until WITH, semicolon, or end of statement
    fn parse_filter_predicate(&mut self) -> Option<String> {
        let start_pos = self.pos;
        let mut end_pos = self.pos;

        // Collect tokens until we hit WITH, semicolon, or end
        while !self.is_at_end() {
            if self.check_keyword(Keyword::WITH) {
                break;
            }
            if self.check_token(&Token::SemiColon) {
                break;
            }
            end_pos = self.pos + 1;
            self.advance();
        }

        if end_pos <= start_pos {
            return None;
        }

        // Reconstruct the predicate from tokens
        let predicate = self.tokens_to_string(start_pos, end_pos);
        let predicate = predicate.trim().to_string();

        if predicate.is_empty() {
            None
        } else {
            Some(predicate)
        }
    }

    /// Convert a range of tokens back to a string
    fn tokens_to_string(&self, start: usize, end: usize) -> String {
        let mut result = String::new();

        for i in start..end.min(self.tokens.len()) {
            let token = &self.tokens[i];
            match &token.token {
                Token::Word(w) => {
                    if w.quote_style.is_some() {
                        result.push_str(&format!("[{}]", w.value));
                    } else {
                        result.push_str(&w.value);
                    }
                }
                Token::Number(n, _) => result.push_str(n),
                Token::SingleQuotedString(s) => result.push_str(&format!("'{}'", s)),
                Token::DoubleQuotedString(s) => result.push_str(&format!("\"{}\"", s)),
                Token::Whitespace(_) => result.push(' '),
                Token::Eq => result.push('='),
                Token::Neq => result.push_str("<>"),
                Token::Lt => result.push('<'),
                Token::Gt => result.push('>'),
                Token::LtEq => result.push_str("<="),
                Token::GtEq => result.push_str(">="),
                Token::LParen => result.push('('),
                Token::RParen => result.push(')'),
                Token::Comma => result.push(','),
                Token::Period => result.push('.'),
                Token::Plus => result.push('+'),
                Token::Minus => result.push('-'),
                Token::Mul => result.push('*'),
                Token::Div => result.push('/'),
                _ => {
                    // For other tokens, use debug format and extract the value
                    result.push_str(&format!("{}", token.token));
                }
            }
        }

        result
    }

    /// Parse a parenthesized column list: ([Col1], [Col2] DESC, [Col3] ASC)
    /// Returns just the column names, stripping ASC/DESC
    fn parse_column_list(&mut self) -> Option<Vec<String>> {
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
                // No valid identifier, break
                break;
            }

            self.skip_whitespace();

            // Skip optional ASC/DESC
            if self.check_keyword(Keyword::ASC) || self.check_keyword(Keyword::DESC) {
                self.advance();
                self.skip_whitespace();
            }

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

    /// Parse a positive integer
    fn parse_positive_integer(&mut self) -> Option<i64> {
        if self.is_at_end() {
            return None;
        }

        let token = self.current_token()?;
        match &token.token {
            Token::Number(n, _) => {
                if let Ok(value) = n.parse::<i64>() {
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

/// Parse CREATE INDEX using tokens and return index info
///
/// This function replaces the regex-based `extract_index_info` function.
/// Only handles T-SQL specific CREATE [UNIQUE] CLUSTERED/NONCLUSTERED INDEX syntax.
/// Standard CREATE INDEX statements are handled by sqlparser directly.
pub fn parse_create_index_tokens(sql: &str) -> Option<TokenParsedIndex> {
    let mut parser = IndexTokenParser::new(sql)?;
    parser.parse_create_index()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Basic CREATE INDEX tests
    // ========================================================================

    #[test]
    fn test_create_clustered_index_basic() {
        let sql = "CREATE CLUSTERED INDEX [IX_Table_Col] ON [dbo].[MyTable] ([Col1])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Table_Col");
        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "MyTable");
        assert_eq!(result.columns, vec!["Col1"]);
        assert!(!result.is_unique);
        assert!(result.is_clustered);
    }

    #[test]
    fn test_create_nonclustered_index_basic() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Table_Col] ON [dbo].[MyTable] ([Col1])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Table_Col");
        assert!(!result.is_unique);
        assert!(!result.is_clustered);
    }

    #[test]
    fn test_create_unique_clustered_index() {
        let sql = "CREATE UNIQUE CLUSTERED INDEX [IX_PK] ON [dbo].[Users] ([UserId])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_PK");
        assert!(result.is_unique);
        assert!(result.is_clustered);
    }

    #[test]
    fn test_create_unique_nonclustered_index() {
        let sql = "CREATE UNIQUE NONCLUSTERED INDEX [IX_Email] ON [dbo].[Users] ([Email])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert!(result.is_unique);
        assert!(!result.is_clustered);
    }

    #[test]
    fn test_standard_index_returns_none() {
        // Standard CREATE INDEX (without CLUSTERED/NONCLUSTERED) should return None
        // because sqlparser handles these directly
        let sql = "CREATE INDEX [IX_Test] ON [dbo].[Table] ([Col])";
        let result = parse_create_index_tokens(sql);
        assert!(result.is_none());
    }

    // ========================================================================
    // Multiple columns tests
    // ========================================================================

    #[test]
    fn test_index_multiple_columns() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Multi] ON [dbo].[Orders] ([CustomerId], [OrderDate], [Status])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.columns, vec!["CustomerId", "OrderDate", "Status"]);
    }

    #[test]
    fn test_index_columns_with_asc() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Asc] ON [dbo].[Table] ([Col1] ASC, [Col2] ASC)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.columns, vec!["Col1", "Col2"]);
    }

    #[test]
    fn test_index_columns_with_desc() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Desc] ON [dbo].[Table] ([Col1] DESC, [Col2] DESC)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.columns, vec!["Col1", "Col2"]);
    }

    #[test]
    fn test_index_columns_mixed_order() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Mixed] ON [dbo].[Table] ([Col1] ASC, [Col2] DESC, [Col3])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.columns, vec!["Col1", "Col2", "Col3"]);
    }

    // ========================================================================
    // INCLUDE clause tests
    // ========================================================================

    #[test]
    fn test_index_with_include_single() {
        let sql =
            "CREATE NONCLUSTERED INDEX [IX_Inc] ON [dbo].[Table] ([KeyCol]) INCLUDE ([IncCol])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.columns, vec!["KeyCol"]);
        assert_eq!(result.include_columns, vec!["IncCol"]);
    }

    #[test]
    fn test_index_with_include_multiple() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Inc] ON [dbo].[Orders] ([OrderId]) INCLUDE ([CustomerName], [OrderDate], [Total])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.columns, vec!["OrderId"]);
        assert_eq!(
            result.include_columns,
            vec!["CustomerName", "OrderDate", "Total"]
        );
    }

    #[test]
    fn test_index_no_include() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_NoInc] ON [dbo].[Table] ([Col1])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert!(result.include_columns.is_empty());
    }

    // ========================================================================
    // WITH clause tests (FILLFACTOR, DATA_COMPRESSION)
    // ========================================================================

    #[test]
    fn test_index_with_fillfactor() {
        let sql =
            "CREATE NONCLUSTERED INDEX [IX_Fill] ON [dbo].[Table] ([Col]) WITH (FILLFACTOR = 80)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.fill_factor, Some(80));
    }

    #[test]
    fn test_index_with_fillfactor_100() {
        let sql =
            "CREATE NONCLUSTERED INDEX [IX_Fill] ON [dbo].[Table] ([Col]) WITH (FILLFACTOR = 100)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.fill_factor, Some(100));
    }

    #[test]
    fn test_index_with_data_compression_row() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Comp] ON [dbo].[Table] ([Col]) WITH (DATA_COMPRESSION = ROW)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.data_compression, Some("ROW".to_string()));
    }

    #[test]
    fn test_index_with_data_compression_page() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Comp] ON [dbo].[Table] ([Col]) WITH (DATA_COMPRESSION = PAGE)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.data_compression, Some("PAGE".to_string()));
    }

    #[test]
    fn test_index_with_data_compression_none() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Comp] ON [dbo].[Table] ([Col]) WITH (DATA_COMPRESSION = NONE)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.data_compression, Some("NONE".to_string()));
    }

    #[test]
    fn test_index_with_multiple_options() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Multi] ON [dbo].[Table] ([Col]) WITH (FILLFACTOR = 90, DATA_COMPRESSION = PAGE)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.fill_factor, Some(90));
        assert_eq!(result.data_compression, Some("PAGE".to_string()));
    }

    #[test]
    fn test_index_with_other_options_ignored() {
        // PAD_INDEX, SORT_IN_TEMPDB, etc. should be parsed but ignored
        let sql = "CREATE NONCLUSTERED INDEX [IX_Other] ON [dbo].[Table] ([Col]) WITH (PAD_INDEX = ON, FILLFACTOR = 70, SORT_IN_TEMPDB = OFF)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.fill_factor, Some(70));
    }

    // ========================================================================
    // WHERE clause tests (filtered indexes)
    // ========================================================================

    #[test]
    fn test_index_with_where_simple() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Filter] ON [dbo].[Orders] ([OrderDate]) WHERE [Status] = 'Active'";
        let result = parse_create_index_tokens(sql).unwrap();
        assert!(result.filter_predicate.is_some());
        let pred = result.filter_predicate.unwrap();
        assert!(pred.contains("Status"));
        assert!(pred.contains("Active"));
    }

    #[test]
    fn test_index_with_where_and_with() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Filter] ON [dbo].[Orders] ([OrderDate]) WHERE [Status] = 1 WITH (FILLFACTOR = 80)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert!(result.filter_predicate.is_some());
        assert_eq!(result.fill_factor, Some(80));
        let pred = result.filter_predicate.unwrap();
        assert!(pred.contains("Status"));
        assert!(pred.contains("1"));
    }

    #[test]
    fn test_index_with_where_is_not_null() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_NotNull] ON [dbo].[Users] ([Email]) WHERE [Email] IS NOT NULL";
        let result = parse_create_index_tokens(sql).unwrap();
        assert!(result.filter_predicate.is_some());
        let pred = result.filter_predicate.unwrap();
        assert!(pred.contains("Email"));
        assert!(pred.contains("IS"));
        assert!(pred.contains("NOT"));
        assert!(pred.contains("NULL"));
    }

    // ========================================================================
    // Schema and naming tests
    // ========================================================================

    #[test]
    fn test_index_custom_schema() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Sales] ON [sales].[Orders] ([OrderId])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.table_schema, "sales");
        assert_eq!(result.table_name, "Orders");
    }

    #[test]
    fn test_index_no_schema() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Test] ON [MyTable] ([Col])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "MyTable");
    }

    #[test]
    fn test_index_unbracketed_names() {
        let sql = "CREATE NONCLUSTERED INDEX IX_Test ON dbo.MyTable (Col1, Col2)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Test");
        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "MyTable");
        assert_eq!(result.columns, vec!["Col1", "Col2"]);
    }

    #[test]
    fn test_index_mixed_brackets() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Test] ON dbo.[MyTable] (Col1)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Test");
        assert_eq!(result.table_name, "MyTable");
    }

    // ========================================================================
    // Case insensitivity tests
    // ========================================================================

    #[test]
    fn test_index_lowercase() {
        let sql = "create nonclustered index [IX_Test] on [dbo].[Table] ([Col])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Test");
    }

    #[test]
    fn test_index_mixed_case() {
        let sql = "Create NonClustered Index [IX_Test] On [dbo].[Table] ([Col])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Test");
        assert!(!result.is_clustered);
    }

    #[test]
    fn test_index_uppercase() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_TEST] ON [DBO].[TABLE] ([COL])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_TEST");
    }

    // ========================================================================
    // Multiline and whitespace tests
    // ========================================================================

    #[test]
    fn test_index_multiline() {
        let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Test]
ON [dbo].[MyTable]
([Col1], [Col2] DESC)
INCLUDE ([Col3], [Col4])
WHERE [Active] = 1
WITH (FILLFACTOR = 80)
"#;
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Test");
        assert_eq!(result.columns, vec!["Col1", "Col2"]);
        assert_eq!(result.include_columns, vec!["Col3", "Col4"]);
        assert!(result.filter_predicate.is_some());
        assert_eq!(result.fill_factor, Some(80));
    }

    #[test]
    fn test_index_minimal_whitespace() {
        let sql = "CREATE NONCLUSTERED INDEX[IX_Test]ON[dbo].[Table]([Col])";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Test");
        assert_eq!(result.table_name, "Table");
    }

    #[test]
    fn test_index_with_semicolon() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Test] ON [dbo].[Table] ([Col]);";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Test");
    }

    // ========================================================================
    // Edge cases and error handling
    // ========================================================================

    #[test]
    fn test_not_an_index() {
        let result = parse_create_index_tokens("CREATE TABLE [dbo].[Users] (Id INT)");
        assert!(result.is_none());
    }

    #[test]
    fn test_create_procedure_not_index() {
        let result =
            parse_create_index_tokens("CREATE PROCEDURE [dbo].[GetUsers] AS SELECT * FROM Users");
        assert!(result.is_none());
    }

    #[test]
    fn test_alter_index_not_supported() {
        // ALTER INDEX is not supported by this parser
        let result = parse_create_index_tokens("ALTER INDEX [IX_Test] ON [dbo].[Table] REBUILD");
        assert!(result.is_none());
    }

    #[test]
    fn test_drop_index_not_supported() {
        let result = parse_create_index_tokens("DROP INDEX [IX_Test] ON [dbo].[Table]");
        assert!(result.is_none());
    }

    // ========================================================================
    // Complex real-world examples
    // ========================================================================

    #[test]
    fn test_complex_covering_index() {
        let sql = r#"
CREATE UNIQUE NONCLUSTERED INDEX [IX_Orders_CustomerOrder]
ON [sales].[Orders] ([CustomerId] ASC, [OrderNumber] ASC)
INCLUDE ([OrderDate], [TotalAmount], [Status], [ShippingAddress])
WHERE [IsDeleted] = 0
WITH (FILLFACTOR = 90, DATA_COMPRESSION = PAGE)
"#;
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.name, "IX_Orders_CustomerOrder");
        assert_eq!(result.table_schema, "sales");
        assert_eq!(result.table_name, "Orders");
        assert!(result.is_unique);
        assert!(!result.is_clustered);
        assert_eq!(result.columns, vec!["CustomerId", "OrderNumber"]);
        assert_eq!(result.include_columns.len(), 4);
        assert!(result.filter_predicate.is_some());
        assert_eq!(result.fill_factor, Some(90));
        assert_eq!(result.data_compression, Some("PAGE".to_string()));
    }

    #[test]
    fn test_columnstore_compression() {
        let sql = "CREATE NONCLUSTERED INDEX [IX_Archive] ON [dbo].[Archive] ([Date]) WITH (DATA_COMPRESSION = COLUMNSTORE)";
        let result = parse_create_index_tokens(sql).unwrap();
        assert_eq!(result.data_compression, Some("COLUMNSTORE".to_string()));
    }
}
