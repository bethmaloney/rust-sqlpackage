//! Token-based procedure definition parsing for T-SQL
//!
//! This module provides token-based parsing for procedure definitions, replacing
//! the previous regex-based approach. Part of Phase 15.3 of the implementation plan.
//!
//! ## Supported Syntax
//!
//! CREATE PROCEDURE:
//! ```sql
//! CREATE PROCEDURE [schema].[name] AS ...
//! CREATE PROC [schema].[name] AS ...
//! CREATE OR ALTER PROCEDURE [schema].[name] AS ...
//! CREATE OR ALTER PROC [schema].[name] AS ...
//! ```
//!
//! ALTER PROCEDURE:
//! ```sql
//! ALTER PROCEDURE [schema].[name] AS ...
//! ALTER PROC [schema].[name] AS ...
//! ```

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

/// Result of parsing a procedure definition using tokens
#[derive(Debug, Clone, Default)]
pub struct TokenParsedProcedure {
    /// Schema name (defaults to "dbo" if not specified)
    pub schema: String,
    /// Procedure name
    pub name: String,
}

/// Token-based procedure definition parser
pub struct ProcedureTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl ProcedureTokenParser {
    /// Create a new parser for a procedure definition string
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Parse CREATE PROCEDURE and return schema/name
    pub fn parse_create_procedure(&mut self) -> Option<TokenParsedProcedure> {
        self.skip_whitespace();

        // Expect CREATE keyword
        if !self.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Check for optional OR ALTER
        if self.check_keyword(Keyword::OR) {
            self.advance();
            self.skip_whitespace();

            if !self.check_keyword(Keyword::ALTER) {
                return None;
            }
            self.advance();
            self.skip_whitespace();
        }

        // Expect PROCEDURE or PROC keyword
        if !self.check_keyword(Keyword::PROCEDURE) && !self.check_word_ci("PROC") {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse the schema-qualified name
        self.parse_schema_qualified_name()
    }

    /// Parse ALTER PROCEDURE and return schema/name
    pub fn parse_alter_procedure(&mut self) -> Option<TokenParsedProcedure> {
        self.skip_whitespace();

        // Expect ALTER keyword
        if !self.check_keyword(Keyword::ALTER) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect PROCEDURE or PROC keyword
        if !self.check_keyword(Keyword::PROCEDURE) && !self.check_word_ci("PROC") {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse the schema-qualified name
        self.parse_schema_qualified_name()
    }

    /// Parse a schema-qualified name: [schema].[name] or schema.name or [name] or name
    fn parse_schema_qualified_name(&mut self) -> Option<TokenParsedProcedure> {
        let first_ident = self.parse_identifier()?;
        self.skip_whitespace();

        // Check if there's a dot (schema.name pattern)
        if self.check_token(&Token::Period) {
            self.advance();
            self.skip_whitespace();

            let second_ident = self.parse_identifier()?;

            Some(TokenParsedProcedure {
                schema: first_ident,
                name: second_ident,
            })
        } else {
            // No dot - just a name, default schema to "dbo"
            Some(TokenParsedProcedure {
                schema: "dbo".to_string(),
                name: first_ident,
            })
        }
    }

    // ========================================================================
    // Helper methods (similar to ColumnTokenParser)
    // ========================================================================

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

/// Parse CREATE PROCEDURE using tokens and return (schema, name)
///
/// This function replaces the regex-based `extract_procedure_name` function.
/// Supports:
/// - CREATE PROCEDURE [dbo].[ProcName]
/// - CREATE PROCEDURE dbo.ProcName
/// - CREATE OR ALTER PROCEDURE [schema].[name]
/// - CREATE PROC [dbo].[name]
/// - CREATE PROCEDURE [dbo].[Name&With&Special]
pub fn parse_create_procedure_tokens(sql: &str) -> Option<(String, String)> {
    let mut parser = ProcedureTokenParser::new(sql)?;
    let result = parser.parse_create_procedure()?;
    Some((result.schema, result.name))
}

/// Parse ALTER PROCEDURE using tokens and return (schema, name)
///
/// This function replaces the regex-based `extract_alter_procedure_name` function.
/// Supports:
/// - ALTER PROCEDURE [dbo].[ProcName]
/// - ALTER PROCEDURE dbo.ProcName
/// - ALTER PROC [dbo].[name]
pub fn parse_alter_procedure_tokens(sql: &str) -> Option<(String, String)> {
    let mut parser = ProcedureTokenParser::new(sql)?;
    let result = parser.parse_alter_procedure()?;
    Some((result.schema, result.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // CREATE PROCEDURE tests
    // ========================================================================

    #[test]
    fn test_create_procedure_bracketed_schema_and_name() {
        let result = parse_create_procedure_tokens(
            "CREATE PROCEDURE [dbo].[GetUsers] AS SELECT * FROM Users",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_create_procedure_unbracketed() {
        let result =
            parse_create_procedure_tokens("CREATE PROCEDURE dbo.GetUsers AS SELECT * FROM Users")
                .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_create_procedure_no_schema() {
        let result =
            parse_create_procedure_tokens("CREATE PROCEDURE [GetUsers] AS SELECT * FROM Users")
                .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_create_procedure_no_schema_unbracketed() {
        let result =
            parse_create_procedure_tokens("CREATE PROCEDURE GetUsers AS SELECT * FROM Users")
                .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_create_or_alter_procedure() {
        let result = parse_create_procedure_tokens(
            "CREATE OR ALTER PROCEDURE [dbo].[UpdateUser] AS UPDATE Users SET Name = @Name",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "UpdateUser");
    }

    #[test]
    fn test_create_proc_shorthand() {
        let result =
            parse_create_procedure_tokens("CREATE PROC [dbo].[GetUsers] AS SELECT * FROM Users")
                .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_create_or_alter_proc_shorthand() {
        let result = parse_create_procedure_tokens(
            "CREATE OR ALTER PROC [dbo].[GetUsers] AS SELECT * FROM Users",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_create_procedure_with_special_characters_in_name() {
        let result = parse_create_procedure_tokens(
            "CREATE PROCEDURE [dbo].[Get&Update&Users] AS SELECT * FROM Users",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "Get&Update&Users");
    }

    #[test]
    fn test_create_procedure_with_parameters() {
        let result = parse_create_procedure_tokens(
            "CREATE PROCEDURE [dbo].[GetUserById] @UserId INT AS SELECT * FROM Users WHERE Id = @UserId",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUserById");
    }

    #[test]
    fn test_create_procedure_multiline() {
        let sql = r#"
CREATE PROCEDURE [dbo].[ComplexProc]
    @Param1 INT,
    @Param2 VARCHAR(100)
AS
BEGIN
    SELECT * FROM Users
END
"#;
        let result = parse_create_procedure_tokens(sql).unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "ComplexProc");
    }

    #[test]
    fn test_create_procedure_custom_schema() {
        let result = parse_create_procedure_tokens(
            "CREATE PROCEDURE [sales].[GetOrders] AS SELECT * FROM Orders",
        )
        .unwrap();
        assert_eq!(result.0, "sales");
        assert_eq!(result.1, "GetOrders");
    }

    // ========================================================================
    // ALTER PROCEDURE tests
    // ========================================================================

    #[test]
    fn test_alter_procedure_bracketed() {
        let result = parse_alter_procedure_tokens(
            "ALTER PROCEDURE [dbo].[GetUsers] AS SELECT * FROM Users WHERE Active = 1",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_alter_procedure_unbracketed() {
        let result = parse_alter_procedure_tokens(
            "ALTER PROCEDURE dbo.GetUsers AS SELECT * FROM Users WHERE Active = 1",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_alter_procedure_no_schema() {
        let result = parse_alter_procedure_tokens(
            "ALTER PROCEDURE [GetUsers] AS SELECT * FROM Users WHERE Active = 1",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_alter_proc_shorthand() {
        let result = parse_alter_procedure_tokens(
            "ALTER PROC [dbo].[GetUsers] AS SELECT * FROM Users WHERE Active = 1",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_alter_procedure_custom_schema() {
        let result = parse_alter_procedure_tokens(
            "ALTER PROCEDURE [sales].[GetOrders] AS SELECT * FROM Orders WHERE Status = 'Active'",
        )
        .unwrap();
        assert_eq!(result.0, "sales");
        assert_eq!(result.1, "GetOrders");
    }

    #[test]
    fn test_alter_procedure_with_special_characters() {
        let result =
            parse_alter_procedure_tokens("ALTER PROCEDURE [dbo].[Name&With&Special] AS SELECT 1")
                .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "Name&With&Special");
    }

    // ========================================================================
    // Edge cases and negative tests
    // ========================================================================

    #[test]
    fn test_create_procedure_case_insensitive() {
        let result =
            parse_create_procedure_tokens("create procedure [dbo].[GetUsers] AS SELECT 1").unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_create_procedure_mixed_case() {
        let result =
            parse_create_procedure_tokens("Create Procedure [dbo].[GetUsers] AS SELECT 1").unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUsers");
    }

    #[test]
    fn test_not_a_procedure() {
        let result = parse_create_procedure_tokens("CREATE TABLE [dbo].[Users] (Id INT)");
        assert!(result.is_none());
    }

    #[test]
    fn test_alter_on_create() {
        // ALTER PROCEDURE should not match CREATE PROCEDURE parser
        let result = parse_create_procedure_tokens("ALTER PROCEDURE [dbo].[GetUsers] AS SELECT 1");
        assert!(result.is_none());
    }

    #[test]
    fn test_create_on_alter() {
        // CREATE PROCEDURE should not match ALTER PROCEDURE parser
        let result = parse_alter_procedure_tokens("CREATE PROCEDURE [dbo].[GetUsers] AS SELECT 1");
        assert!(result.is_none());
    }
}
