//! Token-based fulltext index and catalog parsing for T-SQL
//!
//! This module provides token-based parsing for CREATE FULLTEXT INDEX and
//! CREATE FULLTEXT CATALOG statements, replacing the previous regex-based
//! approach. Part of Phase 15.3 (B7/B8) of the implementation plan.
//!
//! ## Supported Syntax
//!
//! ```sql
//! CREATE FULLTEXT INDEX ON [schema].[table] ([col1] LANGUAGE 1033, [col2])
//!     KEY INDEX [pk_name] ON [catalog] WITH CHANGE_TRACKING AUTO;
//!
//! CREATE FULLTEXT CATALOG [name] AS DEFAULT;
//! ```

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

/// Result of parsing a fulltext index column
#[derive(Debug, Clone)]
pub struct TokenParsedFullTextColumn {
    /// Column name
    pub name: String,
    /// Language ID (e.g., 1033 for English)
    pub language_id: Option<u32>,
}

/// Result of parsing a CREATE FULLTEXT INDEX statement
#[derive(Debug, Clone)]
pub struct TokenParsedFullTextIndex {
    /// Schema of the table (defaults to "dbo" if not specified)
    pub table_schema: String,
    /// Table name the index is on
    pub table_name: String,
    /// Columns in the fulltext index with optional LANGUAGE specifiers
    pub columns: Vec<TokenParsedFullTextColumn>,
    /// Key index name (required for fulltext index)
    pub key_index: String,
    /// Fulltext catalog name (optional, defaults to default catalog)
    pub catalog: Option<String>,
    /// Change tracking mode (AUTO, MANUAL, OFF)
    pub change_tracking: Option<String>,
}

/// Result of parsing a CREATE FULLTEXT CATALOG statement
#[derive(Debug, Clone)]
pub struct TokenParsedFullTextCatalog {
    /// Catalog name
    pub name: String,
    /// Whether this catalog is set as the default
    pub is_default: bool,
}

/// Token-based fulltext definition parser
pub struct FullTextTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl FullTextTokenParser {
    /// Create a new parser for a fulltext definition string
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Parse CREATE FULLTEXT INDEX and return index info
    pub fn parse_fulltext_index(&mut self) -> Option<TokenParsedFullTextIndex> {
        self.skip_whitespace();

        // Expect CREATE keyword
        if !self.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect FULLTEXT keyword
        if !self.check_word_ci("FULLTEXT") {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect INDEX keyword
        if !self.check_keyword(Keyword::INDEX) {
            return None;
        }
        self.advance();
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

        // Parse column list with optional LANGUAGE specifiers
        let columns = self.parse_fulltext_column_list()?;
        self.skip_whitespace();

        // Expect KEY keyword
        if !self.check_keyword(Keyword::KEY) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect INDEX keyword
        if !self.check_keyword(Keyword::INDEX) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse key index name
        let key_index = self.parse_identifier()?;
        self.skip_whitespace();

        // Parse optional ON [catalog]
        let catalog = if self.check_keyword(Keyword::ON) {
            self.advance();
            self.skip_whitespace();
            self.parse_identifier()
        } else {
            None
        };
        self.skip_whitespace();

        // Parse optional WITH clause (CHANGE_TRACKING)
        let change_tracking = self.parse_fulltext_with_options();

        Some(TokenParsedFullTextIndex {
            table_schema,
            table_name,
            columns,
            key_index,
            catalog,
            change_tracking,
        })
    }

    /// Parse CREATE FULLTEXT CATALOG and return catalog info
    pub fn parse_fulltext_catalog(&mut self) -> Option<TokenParsedFullTextCatalog> {
        self.skip_whitespace();

        // Expect CREATE keyword
        if !self.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect FULLTEXT keyword
        if !self.check_word_ci("FULLTEXT") {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect CATALOG keyword
        if !self.check_word_ci("CATALOG") {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse catalog name
        let name = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for AS DEFAULT
        let is_default = self.check_as_default();

        Some(TokenParsedFullTextCatalog { name, is_default })
    }

    /// Check for "AS DEFAULT" clause
    fn check_as_default(&mut self) -> bool {
        if self.check_keyword(Keyword::AS) {
            self.advance();
            self.skip_whitespace();
            if self.check_keyword(Keyword::DEFAULT) {
                self.advance();
                return true;
            }
        }
        false
    }

    /// Parse fulltext column list: ([col1] LANGUAGE 1033, [col2], [col3] LANGUAGE 1041)
    fn parse_fulltext_column_list(&mut self) -> Option<Vec<TokenParsedFullTextColumn>> {
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
                let mut col = TokenParsedFullTextColumn {
                    name: col_name,
                    language_id: None,
                };

                self.skip_whitespace();

                // Check for optional LANGUAGE specifier
                if self.check_word_ci("LANGUAGE") {
                    self.advance();
                    self.skip_whitespace();
                    if let Some(lang_id) = self.parse_positive_integer() {
                        col.language_id = Some(lang_id as u32);
                    }
                    self.skip_whitespace();
                }

                // Check for TYPE COLUMN (advanced feature, skip it)
                if self.check_keyword(Keyword::TYPE) {
                    self.advance();
                    self.skip_whitespace();
                    if self.check_word_ci("COLUMN") {
                        self.advance();
                        self.skip_whitespace();
                        // Skip the type column name
                        let _ = self.parse_identifier();
                        self.skip_whitespace();
                    }
                }

                // Check for STATISTICAL_SEMANTICS (advanced feature, skip it)
                if self.check_word_ci("STATISTICAL_SEMANTICS") {
                    self.advance();
                    self.skip_whitespace();
                }

                columns.push(col);
            } else {
                // No valid identifier, break
                break;
            }

            // Check for comma (more columns) or right paren (end)
            if self.check_token(&Token::Comma) {
                self.advance();
                self.skip_whitespace();
            } else if self.check_token(&Token::RParen) {
                break;
            } else {
                // Unexpected token, advance to avoid infinite loop
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

    /// Parse WITH clause for fulltext index: WITH CHANGE_TRACKING AUTO|MANUAL|OFF
    fn parse_fulltext_with_options(&mut self) -> Option<String> {
        if !self.check_keyword(Keyword::WITH) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Check for CHANGE_TRACKING keyword
        if self.check_word_ci("CHANGE_TRACKING") {
            self.advance();
            self.skip_whitespace();

            // Parse the mode: AUTO, MANUAL, OFF, or NO POPULATION
            if self.check_word_ci("AUTO") {
                self.advance();
                return Some("AUTO".to_string());
            } else if self.check_word_ci("MANUAL") {
                self.advance();
                return Some("MANUAL".to_string());
            } else if self.check_word_ci("OFF") {
                self.advance();
                self.skip_whitespace();
                // Check for optional ", NO POPULATION" after OFF
                if self.check_token(&Token::Comma) {
                    self.advance();
                    self.skip_whitespace();
                }
                if self.check_word_ci("NO") {
                    self.advance();
                    self.skip_whitespace();
                    if self.check_word_ci("POPULATION") {
                        self.advance();
                    }
                }
                return Some("OFF".to_string());
            }
        }

        // Handle STOPLIST or other WITH options we don't specifically track
        // Just skip to end
        None
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

/// Parse CREATE FULLTEXT INDEX using tokens and return index info
///
/// This function replaces the regex-based `extract_fulltext_index_info` function.
pub fn parse_fulltext_index_tokens(sql: &str) -> Option<TokenParsedFullTextIndex> {
    let mut parser = FullTextTokenParser::new(sql)?;
    parser.parse_fulltext_index()
}

/// Parse CREATE FULLTEXT CATALOG using tokens and return catalog info
///
/// This function replaces the regex-based `extract_fulltext_catalog_info` function.
pub fn parse_fulltext_catalog_tokens(sql: &str) -> Option<TokenParsedFullTextCatalog> {
    let mut parser = FullTextTokenParser::new(sql)?;
    parser.parse_fulltext_catalog()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // CREATE FULLTEXT INDEX tests
    // ========================================================================

    #[test]
    fn test_fulltext_index_basic() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] ([Content])
                     KEY INDEX [PK_Documents]"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "Documents");
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0].name, "Content");
        assert!(result.columns[0].language_id.is_none());
        assert_eq!(result.key_index, "PK_Documents");
        assert!(result.catalog.is_none());
        assert!(result.change_tracking.is_none());
    }

    #[test]
    fn test_fulltext_index_with_language() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] ([Content] LANGUAGE 1033)
                     KEY INDEX [PK_Documents]"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0].name, "Content");
        assert_eq!(result.columns[0].language_id, Some(1033));
    }

    #[test]
    fn test_fulltext_index_multiple_columns() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] (
            [Title] LANGUAGE 1033,
            [Content] LANGUAGE 1033,
            [Author] LANGUAGE 1033
        ) KEY INDEX [PK_Documents]"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.columns.len(), 3);
        assert_eq!(result.columns[0].name, "Title");
        assert_eq!(result.columns[0].language_id, Some(1033));
        assert_eq!(result.columns[1].name, "Content");
        assert_eq!(result.columns[2].name, "Author");
    }

    #[test]
    fn test_fulltext_index_mixed_language() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] (
            [Title],
            [Content] LANGUAGE 1033,
            [Notes]
        ) KEY INDEX [PK_Documents]"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.columns.len(), 3);
        assert!(result.columns[0].language_id.is_none());
        assert_eq!(result.columns[1].language_id, Some(1033));
        assert!(result.columns[2].language_id.is_none());
    }

    #[test]
    fn test_fulltext_index_with_catalog() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] ([Content])
                     KEY INDEX [PK_Documents] ON [DocumentCatalog]"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.catalog, Some("DocumentCatalog".to_string()));
    }

    #[test]
    fn test_fulltext_index_with_change_tracking_auto() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] ([Content])
                     KEY INDEX [PK_Documents] WITH CHANGE_TRACKING AUTO"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.change_tracking, Some("AUTO".to_string()));
    }

    #[test]
    fn test_fulltext_index_with_change_tracking_manual() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] ([Content])
                     KEY INDEX [PK_Documents] WITH CHANGE_TRACKING MANUAL"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.change_tracking, Some("MANUAL".to_string()));
    }

    #[test]
    fn test_fulltext_index_with_change_tracking_off() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] ([Content])
                     KEY INDEX [PK_Documents] WITH CHANGE_TRACKING OFF"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.change_tracking, Some("OFF".to_string()));
    }

    #[test]
    fn test_fulltext_index_with_change_tracking_off_no_population() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] ([Content])
                     KEY INDEX [PK_Documents] WITH CHANGE_TRACKING OFF, NO POPULATION"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.change_tracking, Some("OFF".to_string()));
    }

    #[test]
    fn test_fulltext_index_complete() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[Documents] (
            [Title] LANGUAGE 1033,
            [Content] LANGUAGE 1033,
            [Author] LANGUAGE 1033
        )
        KEY INDEX [PK_Documents] ON [DocumentCatalog]
        WITH CHANGE_TRACKING AUTO;"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "Documents");
        assert_eq!(result.columns.len(), 3);
        assert_eq!(result.key_index, "PK_Documents");
        assert_eq!(result.catalog, Some("DocumentCatalog".to_string()));
        assert_eq!(result.change_tracking, Some("AUTO".to_string()));
    }

    #[test]
    fn test_fulltext_index_no_schema() {
        let sql = r#"CREATE FULLTEXT INDEX ON [Documents] ([Content])
                     KEY INDEX [PK_Documents]"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "Documents");
    }

    #[test]
    fn test_fulltext_index_custom_schema() {
        let sql = r#"CREATE FULLTEXT INDEX ON [sales].[Products] ([Description])
                     KEY INDEX [PK_Products]"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.table_schema, "sales");
        assert_eq!(result.table_name, "Products");
    }

    #[test]
    fn test_fulltext_index_unbracketed() {
        let sql = r#"CREATE FULLTEXT INDEX ON dbo.Documents (Content LANGUAGE 1033)
                     KEY INDEX PK_Documents ON DocumentCatalog
                     WITH CHANGE_TRACKING AUTO"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.table_schema, "dbo");
        assert_eq!(result.table_name, "Documents");
        assert_eq!(result.columns[0].name, "Content");
        assert_eq!(result.key_index, "PK_Documents");
        assert_eq!(result.catalog, Some("DocumentCatalog".to_string()));
    }

    #[test]
    fn test_fulltext_index_lowercase() {
        let sql = r#"create fulltext index on [dbo].[Documents] ([Content])
                     key index [PK_Documents]"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.table_name, "Documents");
    }

    // ========================================================================
    // CREATE FULLTEXT CATALOG tests
    // ========================================================================

    #[test]
    fn test_fulltext_catalog_basic() {
        let sql = "CREATE FULLTEXT CATALOG [DocumentCatalog]";
        let result = parse_fulltext_catalog_tokens(sql).unwrap();
        assert_eq!(result.name, "DocumentCatalog");
        assert!(!result.is_default);
    }

    #[test]
    fn test_fulltext_catalog_as_default() {
        let sql = "CREATE FULLTEXT CATALOG [DocumentCatalog] AS DEFAULT";
        let result = parse_fulltext_catalog_tokens(sql).unwrap();
        assert_eq!(result.name, "DocumentCatalog");
        assert!(result.is_default);
    }

    #[test]
    fn test_fulltext_catalog_as_default_with_semicolon() {
        let sql = "CREATE FULLTEXT CATALOG [DocumentCatalog] AS DEFAULT;";
        let result = parse_fulltext_catalog_tokens(sql).unwrap();
        assert_eq!(result.name, "DocumentCatalog");
        assert!(result.is_default);
    }

    #[test]
    fn test_fulltext_catalog_unbracketed() {
        let sql = "CREATE FULLTEXT CATALOG DocumentCatalog AS DEFAULT";
        let result = parse_fulltext_catalog_tokens(sql).unwrap();
        assert_eq!(result.name, "DocumentCatalog");
        assert!(result.is_default);
    }

    #[test]
    fn test_fulltext_catalog_lowercase() {
        let sql = "create fulltext catalog [MyCatalog] as default";
        let result = parse_fulltext_catalog_tokens(sql).unwrap();
        assert_eq!(result.name, "MyCatalog");
        assert!(result.is_default);
    }

    // ========================================================================
    // Edge cases and error handling
    // ========================================================================

    #[test]
    fn test_not_fulltext_index() {
        let result = parse_fulltext_index_tokens("CREATE INDEX [IX_Test] ON [dbo].[Table] ([Col])");
        assert!(result.is_none());
    }

    #[test]
    fn test_not_fulltext_catalog() {
        let result = parse_fulltext_catalog_tokens("CREATE TABLE [dbo].[Users] (Id INT)");
        assert!(result.is_none());
    }

    #[test]
    fn test_fulltext_index_missing_key_index() {
        let sql = "CREATE FULLTEXT INDEX ON [dbo].[Documents] ([Content])";
        let result = parse_fulltext_index_tokens(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_fulltext_index_different_languages() {
        let sql = r#"CREATE FULLTEXT INDEX ON [dbo].[MultiLang] (
            [EnglishText] LANGUAGE 1033,
            [JapaneseText] LANGUAGE 1041,
            [GermanText] LANGUAGE 1031
        ) KEY INDEX [PK_MultiLang]"#;
        let result = parse_fulltext_index_tokens(sql).unwrap();
        assert_eq!(result.columns.len(), 3);
        assert_eq!(result.columns[0].language_id, Some(1033));
        assert_eq!(result.columns[1].language_id, Some(1041));
        assert_eq!(result.columns[2].language_id, Some(1031));
    }
}
