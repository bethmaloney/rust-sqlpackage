//! Token-based synonym definition parsing for T-SQL
//!
//! This module provides token-based parsing for CREATE SYNONYM statements.
//! Part of Phase 56.1 of the implementation plan.
//!
//! ## Supported Syntax
//!
//! ```sql
//! CREATE SYNONYM [schema].[name] FOR [target_schema].[target_name]
//! CREATE SYNONYM [schema].[name] FOR [database].[target_schema].[target_name]
//! CREATE SYNONYM [schema].[name] FOR [server].[database].[target_schema].[target_name]
//! ```

use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::Token;

use super::token_parser_base::TokenParser;

/// Result of parsing a CREATE SYNONYM statement
#[derive(Debug, Clone)]
pub struct TokenParsedSynonym {
    /// Schema name of the synonym (defaults to "dbo" if not specified)
    pub schema: String,
    /// Synonym name
    pub name: String,
    /// Target schema (the schema of the referenced object)
    pub target_schema: String,
    /// Target name (the name of the referenced object)
    pub target_name: String,
    /// Target database (for cross-database synonyms)
    pub target_database: Option<String>,
    /// Target server (for cross-server synonyms)
    pub target_server: Option<String>,
}

/// Token-based synonym definition parser
pub struct SynonymTokenParser {
    base: TokenParser,
}

impl SynonymTokenParser {
    /// Create a new parser for a synonym definition string
    pub fn new(sql: &str) -> Option<Self> {
        Some(Self {
            base: TokenParser::new(sql)?,
        })
    }

    /// Parse CREATE SYNONYM and return synonym info
    pub fn parse_create_synonym(&mut self) -> Option<TokenParsedSynonym> {
        self.base.skip_whitespace();

        // Expect CREATE keyword
        if !self.base.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect SYNONYM keyword (not a sqlparser keyword, check as word)
        if !self.base.check_word_ci("SYNONYM") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse synonym name (schema-qualified)
        let (schema, name) = self.base.parse_schema_qualified_name()?;
        self.base.skip_whitespace();

        // Expect FOR keyword
        if !self.base.check_keyword(Keyword::FOR) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse the target multi-part name
        // Could be 1-part, 2-part, 3-part, or 4-part:
        //   target_name
        //   target_schema.target_name
        //   database.target_schema.target_name
        //   server.database.target_schema.target_name
        let parts = self.parse_multi_part_name()?;

        let (target_server, target_database, target_schema, target_name) = match parts.len() {
            1 => (None, None, "dbo".to_string(), parts[0].clone()),
            2 => (None, None, parts[0].clone(), parts[1].clone()),
            3 => (
                None,
                Some(parts[0].clone()),
                parts[1].clone(),
                parts[2].clone(),
            ),
            4 => (
                Some(parts[0].clone()),
                Some(parts[1].clone()),
                parts[2].clone(),
                parts[3].clone(),
            ),
            _ => return None, // More than 4 parts is invalid
        };

        Some(TokenParsedSynonym {
            schema,
            name,
            target_schema,
            target_name,
            target_database,
            target_server,
        })
    }

    /// Parse a multi-part name separated by dots (1 to 4 parts).
    /// Returns a vector of identifier strings.
    fn parse_multi_part_name(&mut self) -> Option<Vec<String>> {
        let mut parts = Vec::new();

        let first = self.base.parse_identifier()?;
        parts.push(first);

        // Keep reading .identifier parts
        loop {
            self.base.skip_whitespace();
            if !self.base.check_token(&Token::Period) {
                break;
            }
            self.base.advance();
            self.base.skip_whitespace();

            if let Some(ident) = self.base.parse_identifier() {
                parts.push(ident);
            } else {
                break;
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts)
        }
    }
}

/// Parse CREATE SYNONYM using tokens and return synonym info
pub fn parse_create_synonym_tokens(sql: &str) -> Option<TokenParsedSynonym> {
    let mut parser = SynonymTokenParser::new(sql)?;
    parser.parse_create_synonym()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Basic CREATE SYNONYM tests
    // ========================================================================

    #[test]
    fn test_create_synonym_basic_2part_target() {
        let sql = "CREATE SYNONYM [dbo].[MySynonym] FOR [dbo].[TargetTable]";
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MySynonym");
        assert_eq!(result.target_schema, "dbo");
        assert_eq!(result.target_name, "TargetTable");
        assert!(result.target_database.is_none());
        assert!(result.target_server.is_none());
    }

    #[test]
    fn test_create_synonym_1part_target() {
        let sql = "CREATE SYNONYM [dbo].[MySynonym] FOR [TargetTable]";
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MySynonym");
        assert_eq!(result.target_schema, "dbo");
        assert_eq!(result.target_name, "TargetTable");
        assert!(result.target_database.is_none());
        assert!(result.target_server.is_none());
    }

    #[test]
    fn test_create_synonym_3part_target() {
        let sql = "CREATE SYNONYM [dbo].[MySynonym] FOR [OtherDB].[dbo].[TargetTable]";
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MySynonym");
        assert_eq!(result.target_database, Some("OtherDB".to_string()));
        assert_eq!(result.target_schema, "dbo");
        assert_eq!(result.target_name, "TargetTable");
        assert!(result.target_server.is_none());
    }

    #[test]
    fn test_create_synonym_4part_target() {
        let sql =
            "CREATE SYNONYM [dbo].[MySynonym] FOR [RemoteServer].[OtherDB].[dbo].[TargetTable]";
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MySynonym");
        assert_eq!(result.target_server, Some("RemoteServer".to_string()));
        assert_eq!(result.target_database, Some("OtherDB".to_string()));
        assert_eq!(result.target_schema, "dbo");
        assert_eq!(result.target_name, "TargetTable");
    }

    #[test]
    fn test_create_synonym_cross_schema() {
        let sql = "CREATE SYNONYM [sales].[MySynonym] FOR [hr].[Employees]";
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "sales");
        assert_eq!(result.name, "MySynonym");
        assert_eq!(result.target_schema, "hr");
        assert_eq!(result.target_name, "Employees");
    }

    #[test]
    fn test_create_synonym_unbracketed() {
        let sql = "CREATE SYNONYM dbo.MySynonym FOR dbo.TargetTable";
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MySynonym");
        assert_eq!(result.target_schema, "dbo");
        assert_eq!(result.target_name, "TargetTable");
    }

    #[test]
    fn test_create_synonym_no_schema() {
        let sql = "CREATE SYNONYM [MySynonym] FOR [dbo].[TargetTable]";
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MySynonym");
        assert_eq!(result.target_schema, "dbo");
        assert_eq!(result.target_name, "TargetTable");
    }

    #[test]
    fn test_create_synonym_case_insensitive() {
        let sql = "create synonym [dbo].[MySynonym] for [dbo].[TargetTable]";
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MySynonym");
        assert_eq!(result.target_schema, "dbo");
        assert_eq!(result.target_name, "TargetTable");
    }

    #[test]
    fn test_create_synonym_with_semicolon() {
        let sql = "CREATE SYNONYM [dbo].[MySynonym] FOR [dbo].[TargetTable];";
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "MySynonym");
        assert_eq!(result.target_schema, "dbo");
        assert_eq!(result.target_name, "TargetTable");
    }

    #[test]
    fn test_create_synonym_multiline() {
        let sql = r#"
CREATE SYNONYM [dbo].[ExternalTable]
FOR [OtherDatabase].[dbo].[SomeTable];
"#;
        let result = parse_create_synonym_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "ExternalTable");
        assert_eq!(result.target_database, Some("OtherDatabase".to_string()));
        assert_eq!(result.target_schema, "dbo");
        assert_eq!(result.target_name, "SomeTable");
    }

    // ========================================================================
    // Negative tests
    // ========================================================================

    #[test]
    fn test_not_a_synonym() {
        let result = parse_create_synonym_tokens("CREATE TABLE [dbo].[Users] (Id INT)");
        assert!(result.is_none());
    }

    #[test]
    fn test_drop_synonym_not_create() {
        let result = parse_create_synonym_tokens("DROP SYNONYM [dbo].[MySynonym]");
        assert!(result.is_none());
    }

    #[test]
    fn test_create_synonym_missing_for() {
        let result =
            parse_create_synonym_tokens("CREATE SYNONYM [dbo].[MySynonym] [dbo].[TargetTable]");
        assert!(result.is_none());
    }
}
