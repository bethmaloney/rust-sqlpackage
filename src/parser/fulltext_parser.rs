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

use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan};

use super::token_parser_base::TokenParser;

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
    base: TokenParser,
}

impl FullTextTokenParser {
    /// Create a new parser for a fulltext definition string
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

    /// Parse CREATE FULLTEXT INDEX and return index info
    pub fn parse_fulltext_index(&mut self) -> Option<TokenParsedFullTextIndex> {
        self.base.skip_whitespace();

        // Expect CREATE keyword
        if !self.base.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect FULLTEXT keyword
        if !self.base.check_word_ci("FULLTEXT") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect INDEX keyword
        if !self.base.check_keyword(Keyword::INDEX) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect ON keyword
        if !self.base.check_keyword(Keyword::ON) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse table name (schema-qualified)
        let (table_schema, table_name) = self.base.parse_schema_qualified_name()?;
        self.base.skip_whitespace();

        // Parse column list with optional LANGUAGE specifiers
        let columns = self.parse_fulltext_column_list()?;
        self.base.skip_whitespace();

        // Expect KEY keyword
        if !self.base.check_keyword(Keyword::KEY) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect INDEX keyword
        if !self.base.check_keyword(Keyword::INDEX) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse key index name
        let key_index = self.base.parse_identifier()?;
        self.base.skip_whitespace();

        // Parse optional ON [catalog]
        let catalog = if self.base.check_keyword(Keyword::ON) {
            self.base.advance();
            self.base.skip_whitespace();
            self.base.parse_identifier()
        } else {
            None
        };
        self.base.skip_whitespace();

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
        self.base.skip_whitespace();

        // Expect CREATE keyword
        if !self.base.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect FULLTEXT keyword
        if !self.base.check_word_ci("FULLTEXT") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect CATALOG keyword
        if !self.base.check_word_ci("CATALOG") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse catalog name
        let name = self.base.parse_identifier()?;
        self.base.skip_whitespace();

        // Check for AS DEFAULT
        let is_default = self.check_as_default();

        Some(TokenParsedFullTextCatalog { name, is_default })
    }

    /// Check for "AS DEFAULT" clause
    fn check_as_default(&mut self) -> bool {
        if self.base.check_keyword(Keyword::AS) {
            self.base.advance();
            self.base.skip_whitespace();
            if self.base.check_keyword(Keyword::DEFAULT) {
                self.base.advance();
                return true;
            }
        }
        false
    }

    /// Parse fulltext column list: ([col1] LANGUAGE 1033, [col2], [col3] LANGUAGE 1041)
    fn parse_fulltext_column_list(&mut self) -> Option<Vec<TokenParsedFullTextColumn>> {
        // Expect opening parenthesis
        if !self.base.check_token(&Token::LParen) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        let mut columns = Vec::new();

        while !self.base.is_at_end() && !self.base.check_token(&Token::RParen) {
            self.base.skip_whitespace();

            // Parse column name
            if let Some(col_name) = self.base.parse_identifier() {
                let mut col = TokenParsedFullTextColumn {
                    name: col_name,
                    language_id: None,
                };

                self.base.skip_whitespace();

                // Check for optional LANGUAGE specifier
                if self.base.check_word_ci("LANGUAGE") {
                    self.base.advance();
                    self.base.skip_whitespace();
                    if let Some(lang_id) = self.base.parse_positive_integer() {
                        col.language_id = Some(lang_id as u32);
                    }
                    self.base.skip_whitespace();
                }

                // Check for TYPE COLUMN (advanced feature, skip it)
                if self.base.check_keyword(Keyword::TYPE) {
                    self.base.advance();
                    self.base.skip_whitespace();
                    if self.base.check_word_ci("COLUMN") {
                        self.base.advance();
                        self.base.skip_whitespace();
                        // Skip the type column name
                        let _ = self.base.parse_identifier();
                        self.base.skip_whitespace();
                    }
                }

                // Check for STATISTICAL_SEMANTICS (advanced feature, skip it)
                if self.base.check_word_ci("STATISTICAL_SEMANTICS") {
                    self.base.advance();
                    self.base.skip_whitespace();
                }

                columns.push(col);
            } else {
                // No valid identifier, break
                break;
            }

            // Check for comma (more columns) or right paren (end)
            if self.base.check_token(&Token::Comma) {
                self.base.advance();
                self.base.skip_whitespace();
            } else if self.base.check_token(&Token::RParen) {
                break;
            } else {
                // Unexpected token, advance to avoid infinite loop
                self.base.advance();
            }
        }

        // Consume closing parenthesis
        if self.base.check_token(&Token::RParen) {
            self.base.advance();
        }

        if columns.is_empty() {
            None
        } else {
            Some(columns)
        }
    }

    /// Parse WITH clause for fulltext index: WITH CHANGE_TRACKING AUTO|MANUAL|OFF
    fn parse_fulltext_with_options(&mut self) -> Option<String> {
        if !self.base.check_keyword(Keyword::WITH) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Check for CHANGE_TRACKING keyword
        if self.base.check_word_ci("CHANGE_TRACKING") {
            self.base.advance();
            self.base.skip_whitespace();

            // Parse the mode: AUTO, MANUAL, OFF, or NO POPULATION
            if self.base.check_word_ci("AUTO") {
                self.base.advance();
                return Some("AUTO".to_string());
            } else if self.base.check_word_ci("MANUAL") {
                self.base.advance();
                return Some("MANUAL".to_string());
            } else if self.base.check_word_ci("OFF") {
                self.base.advance();
                self.base.skip_whitespace();
                // Check for optional ", NO POPULATION" after OFF
                if self.base.check_token(&Token::Comma) {
                    self.base.advance();
                    self.base.skip_whitespace();
                }
                if self.base.check_word_ci("NO") {
                    self.base.advance();
                    self.base.skip_whitespace();
                    if self.base.check_word_ci("POPULATION") {
                        self.base.advance();
                    }
                }
                return Some("OFF".to_string());
            }
        }

        // Handle STOPLIST or other WITH options we don't specifically track
        // Just skip to end
        None
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

/// Parse CREATE FULLTEXT INDEX from pre-tokenized tokens (Phase 76)
pub fn parse_fulltext_index_tokens_with_tokens(
    tokens: Vec<TokenWithSpan>,
) -> Option<TokenParsedFullTextIndex> {
    let mut parser = FullTextTokenParser::from_tokens(tokens);
    parser.parse_fulltext_index()
}

/// Parse CREATE FULLTEXT CATALOG from pre-tokenized tokens (Phase 76)
pub fn parse_fulltext_catalog_tokens_with_tokens(
    tokens: Vec<TokenWithSpan>,
) -> Option<TokenParsedFullTextCatalog> {
    let mut parser = FullTextTokenParser::from_tokens(tokens);
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
