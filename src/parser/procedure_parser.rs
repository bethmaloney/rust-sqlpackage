//! Token-based procedure definition parsing for T-SQL
//!
//! This module provides token-based parsing for procedure definitions, replacing
//! the previous regex-based approach. Part of Phase 15.3 of the implementation plan.
//! Extended in Phase 20.1.1 to add full parameter parsing.
//! Refactored in Phase 27 to use base TokenParser.
//!
//! ## Supported Syntax
//!
//! CREATE PROCEDURE:
//! ```sql
//! CREATE PROCEDURE [schema].[name] AS ...
//! CREATE PROC [schema].[name] AS ...
//! CREATE OR ALTER PROCEDURE [schema].[name] AS ...
//! CREATE OR ALTER PROC [schema].[name] AS ...
//! CREATE PROCEDURE [schema].[name] @param1 TYPE, @param2 TYPE OUTPUT AS ...
//! CREATE PROCEDURE [schema].[name] @items [dbo].[TableType] READONLY AS ...
//! ```
//!
//! ALTER PROCEDURE:
//! ```sql
//! ALTER PROCEDURE [schema].[name] AS ...
//! ALTER PROC [schema].[name] AS ...
//! ```

use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan};

use super::token_parser_base::TokenParser;

/// Result of parsing a procedure definition using tokens
#[derive(Debug, Clone, Default)]
pub struct TokenParsedProcedure {
    /// Schema name (defaults to "dbo" if not specified)
    pub schema: String,
    /// Procedure name
    pub name: String,
    /// Procedure parameters (populated by full parsing)
    pub parameters: Vec<TokenParsedProcedureParameter>,
}

/// A parameter extracted from a procedure definition
#[derive(Debug, Clone)]
pub struct TokenParsedProcedureParameter {
    /// Parameter name (without @ prefix)
    pub name: String,
    /// Data type (e.g., "INT", "DECIMAL(18, 2)", "[dbo].[TableType]")
    pub data_type: String,
    /// Whether this is an OUTPUT parameter
    pub is_output: bool,
    /// Whether this is a READONLY table-valued parameter
    pub is_readonly: bool,
    /// Default value if specified
    pub default_value: Option<String>,
}

/// Token-based procedure definition parser
pub struct ProcedureTokenParser {
    base: TokenParser,
}

impl ProcedureTokenParser {
    /// Create a new parser for a procedure definition string
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

    /// Parse CREATE PROCEDURE and return schema/name (without parameters)
    pub fn parse_create_procedure(&mut self) -> Option<TokenParsedProcedure> {
        self.base.skip_whitespace();

        // Expect CREATE keyword
        if !self.base.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Check for optional OR ALTER
        if self.base.check_keyword(Keyword::OR) {
            self.base.advance();
            self.base.skip_whitespace();

            if !self.base.check_keyword(Keyword::ALTER) {
                return None;
            }
            self.base.advance();
            self.base.skip_whitespace();
        }

        // Expect PROCEDURE or PROC keyword
        if !self.base.check_keyword(Keyword::PROCEDURE) && !self.base.check_word_ci("PROC") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse the schema-qualified name
        self.parse_schema_qualified_name()
    }

    /// Parse CREATE PROCEDURE with full parameter extraction
    pub fn parse_create_procedure_full(&mut self) -> Option<TokenParsedProcedure> {
        self.base.skip_whitespace();

        // Expect CREATE keyword
        if !self.base.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Check for optional OR ALTER
        if self.base.check_keyword(Keyword::OR) {
            self.base.advance();
            self.base.skip_whitespace();

            if !self.base.check_keyword(Keyword::ALTER) {
                return None;
            }
            self.base.advance();
            self.base.skip_whitespace();
        }

        // Expect PROCEDURE or PROC keyword
        if !self.base.check_keyword(Keyword::PROCEDURE) && !self.base.check_word_ci("PROC") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse the schema-qualified name
        let (schema, name) = self.base.parse_schema_qualified_name()?;
        self.base.skip_whitespace();

        // Parse parameters (between procedure name and AS keyword)
        let parameters = self.parse_parameters();

        Some(TokenParsedProcedure {
            schema,
            name,
            parameters,
        })
    }

    /// Parse ALTER PROCEDURE and return schema/name (without parameters)
    pub fn parse_alter_procedure(&mut self) -> Option<TokenParsedProcedure> {
        self.base.skip_whitespace();

        // Expect ALTER keyword
        if !self.base.check_keyword(Keyword::ALTER) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect PROCEDURE or PROC keyword
        if !self.base.check_keyword(Keyword::PROCEDURE) && !self.base.check_word_ci("PROC") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse the schema-qualified name
        self.parse_schema_qualified_name()
    }

    /// Parse ALTER PROCEDURE with full parameter extraction
    pub fn parse_alter_procedure_full(&mut self) -> Option<TokenParsedProcedure> {
        self.base.skip_whitespace();

        // Expect ALTER keyword
        if !self.base.check_keyword(Keyword::ALTER) {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Expect PROCEDURE or PROC keyword
        if !self.base.check_keyword(Keyword::PROCEDURE) && !self.base.check_word_ci("PROC") {
            return None;
        }
        self.base.advance();
        self.base.skip_whitespace();

        // Parse the schema-qualified name
        let (schema, name) = self.base.parse_schema_qualified_name()?;
        self.base.skip_whitespace();

        // Parse parameters
        let parameters = self.parse_parameters();

        Some(TokenParsedProcedure {
            schema,
            name,
            parameters,
        })
    }

    /// Parse a schema-qualified name: [schema].[name] or schema.name or [name] or name
    fn parse_schema_qualified_name(&mut self) -> Option<TokenParsedProcedure> {
        let (schema, name) = self.base.parse_schema_qualified_name()?;
        Some(TokenParsedProcedure {
            schema,
            name,
            parameters: Vec::new(),
        })
    }

    /// Parse procedure parameters: @param1 TYPE, @param2 TYPE OUTPUT, @items TYPE READONLY
    /// Parameters continue until AS keyword is found
    fn parse_parameters(&mut self) -> Vec<TokenParsedProcedureParameter> {
        let mut params = Vec::new();

        // Parameters are between procedure name and AS keyword
        // They may or may not be wrapped in parentheses
        self.base.skip_whitespace();

        // Check for optional opening parenthesis
        let has_parens = self.base.check_token(&Token::LParen);
        if has_parens {
            self.base.advance();
            self.base.skip_whitespace();
        }

        loop {
            // Check if we've hit the AS keyword or end of tokens
            if self.base.is_at_end() || self.base.check_keyword(Keyword::AS) {
                break;
            }

            // Check for closing paren (if we had opening paren)
            if has_parens && self.base.check_token(&Token::RParen) {
                self.base.advance();
                break;
            }

            // Try to parse a parameter (starts with @)
            if let Some(param) = self.parse_single_parameter() {
                params.push(param);
            } else {
                // Not a parameter - check if it's a comma or advance
                if self.base.check_token(&Token::Comma) {
                    self.base.advance();
                    self.base.skip_whitespace();
                } else if self.base.check_keyword(Keyword::AS) {
                    break;
                } else {
                    self.base.advance();
                }
            }

            self.base.skip_whitespace();

            // Check for comma (more parameters)
            if self.base.check_token(&Token::Comma) {
                self.base.advance();
                self.base.skip_whitespace();
            }
        }

        params
    }

    /// Parse a single parameter: @name TYPE [= default] [READONLY] [OUTPUT|OUT]
    fn parse_single_parameter(&mut self) -> Option<TokenParsedProcedureParameter> {
        // Parameter name should be a Word starting with @
        let name = self.parse_parameter_name()?;
        self.base.skip_whitespace();

        // Parse data type
        let data_type = self.parse_data_type()?;
        self.base.skip_whitespace();

        // Now parse optional modifiers: = default, READONLY, OUTPUT/OUT
        // These can appear in various orders, so we loop until we hit a delimiter
        let mut default_value = None;
        let mut is_readonly = false;
        let mut is_output = false;

        loop {
            // Check for default value: = ...
            if self.base.check_token(&Token::Eq) {
                self.base.advance();
                self.base.skip_whitespace();
                default_value = Some(self.parse_default_value());
                self.base.skip_whitespace();
                continue;
            }

            // Check for READONLY keyword
            if self.base.check_word_ci("READONLY") {
                is_readonly = true;
                self.base.advance();
                self.base.skip_whitespace();
                continue;
            }

            // Check for OUTPUT or OUT keyword (OUTPUT is not a Keyword variant, use word check)
            if self.base.check_word_ci("OUTPUT") || self.base.check_word_ci("OUT") {
                is_output = true;
                self.base.advance();
                self.base.skip_whitespace();
                continue;
            }

            // No more modifiers
            break;
        }

        Some(TokenParsedProcedureParameter {
            name,
            data_type,
            is_output,
            is_readonly,
            default_value,
        })
    }

    /// Parse parameter name (@name) and return name without @ prefix
    fn parse_parameter_name(&mut self) -> Option<String> {
        if self.base.is_at_end() {
            return None;
        }

        let token = self.base.current_token()?;
        match &token.token {
            // MsSqlDialect tokenizes @name as a Word
            Token::Word(w) if w.value.starts_with('@') => {
                let name = w.value[1..].to_string(); // Remove @ prefix
                self.base.advance();
                Some(name)
            }
            _ => None,
        }
    }

    /// Parse data type (e.g., INT, DECIMAL(18, 2), NVARCHAR(100), [dbo].[TableType])
    fn parse_data_type(&mut self) -> Option<String> {
        // Check for schema-qualified type: [schema].[type] or schema.type
        let first_part = self.try_parse_identifier()?;
        self.base.skip_whitespace();

        let mut result = if self.base.check_token(&Token::Period) {
            // Schema-qualified type
            self.base.advance();
            self.base.skip_whitespace();

            if let Some(second_part) = self.try_parse_identifier() {
                // Build [schema].[type] format
                let schema = Self::ensure_bracketed(&first_part);
                let type_name = Self::ensure_bracketed(&second_part);
                format!("{}.{}", schema, type_name)
            } else {
                first_part
            }
        } else {
            // Simple type - uppercase it
            first_part.to_uppercase()
        };

        self.base.skip_whitespace();

        // Check for type parameters in parentheses
        if self.base.check_token(&Token::LParen) {
            result.push('(');
            self.base.advance();

            let mut depth = 1;
            while !self.base.is_at_end() && depth > 0 {
                if let Some(token) = self.base.current_token() {
                    match &token.token {
                        Token::LParen => {
                            depth += 1;
                            result.push('(');
                        }
                        Token::RParen => {
                            depth -= 1;
                            if depth > 0 {
                                result.push(')');
                            }
                        }
                        Token::Whitespace(_) => {
                            // Skip whitespace inside type params
                        }
                        _ => {
                            result.push_str(&TokenParser::token_to_string(&token.token));
                        }
                    }
                    self.base.advance();
                }
            }
            result.push(')');
        }

        Some(result)
    }

    /// Try to parse an identifier without consuming if not found
    fn try_parse_identifier(&mut self) -> Option<String> {
        if self.base.is_at_end() {
            return None;
        }

        let token = self.base.current_token()?;
        match &token.token {
            Token::Word(w) => {
                // Don't consume keywords that aren't type names
                // OUTPUT is not a Keyword variant, so check by string value
                if matches!(
                    w.keyword,
                    Keyword::AS | Keyword::BEGIN | Keyword::WITH | Keyword::FOR
                ) || w.value.eq_ignore_ascii_case("READONLY")
                    || w.value.eq_ignore_ascii_case("OUTPUT")
                    || w.value.eq_ignore_ascii_case("OUT")
                {
                    return None;
                }
                let name = w.value.clone();
                self.base.advance();
                Some(name)
            }
            _ => None,
        }
    }

    /// Ensure an identifier is wrapped in brackets
    fn ensure_bracketed(ident: &str) -> String {
        if ident.starts_with('[') && ident.ends_with(']') {
            ident.to_string()
        } else {
            format!("[{}]", ident)
        }
    }

    /// Parse default value (everything up to comma, READONLY, OUTPUT, OUT, or AS)
    fn parse_default_value(&mut self) -> String {
        let mut result = String::new();
        let mut depth = 0;

        while !self.base.is_at_end() {
            if let Some(token) = self.base.current_token() {
                match &token.token {
                    Token::LParen => {
                        depth += 1;
                        result.push('(');
                        self.base.advance();
                    }
                    Token::RParen => {
                        if depth == 0 {
                            break; // End of parameters
                        }
                        depth -= 1;
                        result.push(')');
                        self.base.advance();
                    }
                    Token::Comma if depth == 0 => {
                        break; // Next parameter
                    }
                    Token::Word(w)
                        if depth == 0
                            && (w.keyword == Keyword::AS
                                || w.value.eq_ignore_ascii_case("OUTPUT")
                                || w.value.eq_ignore_ascii_case("OUT")
                                || w.value.eq_ignore_ascii_case("READONLY")) =>
                    {
                        break; // End of default value
                    }
                    Token::Whitespace(_) => {
                        // Add space if result has content
                        if !result.is_empty() && !result.ends_with(' ') {
                            result.push(' ');
                        }
                        self.base.advance();
                    }
                    _ => {
                        result.push_str(&TokenParser::token_to_string(&token.token));
                        self.base.advance();
                    }
                }
            } else {
                break;
            }
        }

        result.trim().to_string()
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

/// Parse CREATE PROCEDURE from pre-tokenized tokens (Phase 76)
pub fn parse_create_procedure_tokens_with_tokens(
    tokens: Vec<TokenWithSpan>,
) -> Option<(String, String)> {
    let mut parser = ProcedureTokenParser::from_tokens(tokens);
    let result = parser.parse_create_procedure()?;
    Some((result.schema, result.name))
}

/// Parse ALTER PROCEDURE from pre-tokenized tokens (Phase 76)
pub fn parse_alter_procedure_tokens_with_tokens(
    tokens: Vec<TokenWithSpan>,
) -> Option<(String, String)> {
    let mut parser = ProcedureTokenParser::from_tokens(tokens);
    let result = parser.parse_alter_procedure()?;
    Some((result.schema, result.name))
}

/// Parse CREATE PROCEDURE with full parameter extraction
///
/// This function replaces the regex-based `PROC_PARAM_RE` extraction.
/// Supports:
/// - Simple types: INT, VARCHAR(50), DECIMAL(10, 2)
/// - Schema-qualified types: [dbo].[TableType], dbo.TableType
/// - READONLY keyword for table-valued parameters
/// - OUTPUT/OUT modifiers
/// - Default values: @param INT = 5, @name VARCHAR(100) = 'default'
pub fn parse_create_procedure_full(sql: &str) -> Option<TokenParsedProcedure> {
    let mut parser = ProcedureTokenParser::new(sql)?;
    parser.parse_create_procedure_full()
}

/// Parse ALTER PROCEDURE with full parameter extraction
pub fn parse_alter_procedure_full(sql: &str) -> Option<TokenParsedProcedure> {
    let mut parser = ProcedureTokenParser::new(sql)?;
    parser.parse_alter_procedure_full()
}

/// Extract procedure parameters from a SQL definition using token-based parsing
///
/// This function is the main entry point for replacing PROC_PARAM_RE regex.
/// Returns a vector of parameters with all details (name, type, output, readonly, default).
pub fn extract_procedure_parameters_tokens(sql: &str) -> Vec<TokenParsedProcedureParameter> {
    parse_create_procedure_full(sql)
        .or_else(|| parse_alter_procedure_full(sql))
        .map(|p| p.parameters)
        .unwrap_or_default()
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

    // ========================================================================
    // Full parameter parsing tests (Phase 20.1.1)
    // ========================================================================

    #[test]
    fn test_full_parse_no_params() {
        let result =
            parse_create_procedure_full("CREATE PROCEDURE [dbo].[GetUsers] AS SELECT * FROM Users")
                .unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "GetUsers");
        assert!(result.parameters.is_empty());
    }

    #[test]
    fn test_full_parse_single_param() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[GetUserById] @UserId INT AS SELECT * FROM Users WHERE Id = @UserId",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "UserId");
        assert_eq!(result.parameters[0].data_type, "INT");
        assert!(!result.parameters[0].is_output);
        assert!(!result.parameters[0].is_readonly);
        assert!(result.parameters[0].default_value.is_none());
    }

    #[test]
    fn test_full_parse_multiple_params() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[InsertUser] @Name VARCHAR(100), @Age INT AS INSERT INTO Users VALUES (@Name, @Age)",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 2);
        assert_eq!(result.parameters[0].name, "Name");
        assert_eq!(result.parameters[0].data_type, "VARCHAR(100)");
        assert_eq!(result.parameters[1].name, "Age");
        assert_eq!(result.parameters[1].data_type, "INT");
    }

    #[test]
    fn test_full_parse_output_param() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[GetCount] @UserId INT, @Count INT OUTPUT AS SELECT @Count = COUNT(*) FROM Users WHERE Id = @UserId",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 2);
        assert_eq!(result.parameters[0].name, "UserId");
        assert!(!result.parameters[0].is_output);
        assert_eq!(result.parameters[1].name, "Count");
        assert_eq!(result.parameters[1].data_type, "INT");
        assert!(result.parameters[1].is_output);
    }

    #[test]
    fn test_full_parse_out_shorthand() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[GetCount] @Count INT OUT AS SELECT @Count = 5",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "Count");
        assert!(result.parameters[0].is_output);
    }

    #[test]
    fn test_full_parse_readonly_tvp() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[ProcessItems] @Items [dbo].[ItemTableType] READONLY AS SELECT * FROM @Items",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "Items");
        assert_eq!(result.parameters[0].data_type, "[dbo].[ItemTableType]");
        assert!(result.parameters[0].is_readonly);
        assert!(!result.parameters[0].is_output);
    }

    #[test]
    fn test_full_parse_tvp_unbracketed() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[ProcessItems] @Items dbo.ItemTableType READONLY AS SELECT * FROM @Items",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].data_type, "[dbo].[ItemTableType]");
        assert!(result.parameters[0].is_readonly);
    }

    #[test]
    fn test_full_parse_default_value() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[GetUsers] @PageSize INT = 10 AS SELECT TOP (@PageSize) * FROM Users",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "PageSize");
        assert_eq!(result.parameters[0].data_type, "INT");
        assert_eq!(result.parameters[0].default_value, Some("10".to_string()));
    }

    #[test]
    fn test_full_parse_default_null() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[GetUsers] @FilterId INT = NULL AS SELECT * FROM Users",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].default_value, Some("NULL".to_string()));
    }

    #[test]
    fn test_full_parse_default_string() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[GetUsers] @Status VARCHAR(20) = 'Active' AS SELECT * FROM Users",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(
            result.parameters[0].default_value,
            Some("'Active'".to_string())
        );
    }

    #[test]
    fn test_full_parse_default_with_output() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[Test] @Value INT = 5 OUTPUT AS SELECT @Value = @Value + 1",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].default_value, Some("5".to_string()));
        assert!(result.parameters[0].is_output);
    }

    #[test]
    fn test_full_parse_complex_types() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[Test] @Amount DECIMAL(18, 2), @Precision NUMERIC(10, 4) AS SELECT 1",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 2);
        assert_eq!(result.parameters[0].data_type, "DECIMAL(18,2)");
        assert_eq!(result.parameters[1].data_type, "NUMERIC(10,4)");
    }

    #[test]
    fn test_full_parse_multiline() {
        let sql = r#"
CREATE PROCEDURE [dbo].[ComplexProc]
    @UserId INT,
    @Name VARCHAR(100) = 'Unknown',
    @Items [dbo].[ItemType] READONLY,
    @Result INT OUTPUT
AS
BEGIN
    SELECT * FROM Users WHERE Id = @UserId
END
"#;
        let result = parse_create_procedure_full(sql).unwrap();
        assert_eq!(result.parameters.len(), 4);

        assert_eq!(result.parameters[0].name, "UserId");
        assert_eq!(result.parameters[0].data_type, "INT");
        assert!(!result.parameters[0].is_output);
        assert!(!result.parameters[0].is_readonly);
        assert!(result.parameters[0].default_value.is_none());

        assert_eq!(result.parameters[1].name, "Name");
        assert_eq!(result.parameters[1].data_type, "VARCHAR(100)");
        assert_eq!(
            result.parameters[1].default_value,
            Some("'Unknown'".to_string())
        );

        assert_eq!(result.parameters[2].name, "Items");
        assert_eq!(result.parameters[2].data_type, "[dbo].[ItemType]");
        assert!(result.parameters[2].is_readonly);

        assert_eq!(result.parameters[3].name, "Result");
        assert_eq!(result.parameters[3].data_type, "INT");
        assert!(result.parameters[3].is_output);
    }

    #[test]
    fn test_full_parse_with_tabs() {
        let sql =
            "CREATE PROCEDURE [dbo].[Test]\t@Param1\tINT,\t@Param2\tVARCHAR(50)\tAS\tSELECT 1";
        let result = parse_create_procedure_full(sql).unwrap();
        assert_eq!(result.parameters.len(), 2);
        assert_eq!(result.parameters[0].name, "Param1");
        assert_eq!(result.parameters[0].data_type, "INT");
        assert_eq!(result.parameters[1].name, "Param2");
        assert_eq!(result.parameters[1].data_type, "VARCHAR(50)");
    }

    #[test]
    fn test_full_parse_with_multiple_spaces() {
        let sql = "CREATE PROCEDURE [dbo].[Test]    @Param1    INT    READONLY    AS    SELECT 1";
        let result = parse_create_procedure_full(sql).unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "Param1");
        assert!(result.parameters[0].is_readonly);
    }

    #[test]
    fn test_full_parse_varchar_max() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[Test] @Data VARCHAR(MAX) AS SELECT 1",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].data_type, "VARCHAR(MAX)");
    }

    #[test]
    fn test_full_parse_nvarchar() {
        let result = parse_create_procedure_full(
            "CREATE PROCEDURE [dbo].[Test] @Name NVARCHAR(256) AS SELECT 1",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].data_type, "NVARCHAR(256)");
    }

    #[test]
    fn test_extract_parameters_helper() {
        let params = extract_procedure_parameters_tokens(
            "CREATE PROCEDURE [dbo].[Test] @Id INT, @Name VARCHAR(100) OUTPUT AS SELECT 1",
        );
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "Id");
        assert_eq!(params[1].name, "Name");
        assert!(params[1].is_output);
    }

    #[test]
    fn test_extract_parameters_from_alter() {
        let params = extract_procedure_parameters_tokens(
            "ALTER PROCEDURE [dbo].[Test] @Id INT READONLY AS SELECT 1",
        );
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "Id");
        assert!(params[0].is_readonly);
    }

    #[test]
    fn test_full_parse_alter_with_params() {
        let result = parse_alter_procedure_full(
            "ALTER PROCEDURE [dbo].[UpdateUser] @UserId INT, @Name VARCHAR(100) AS UPDATE Users SET Name = @Name WHERE Id = @UserId",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 2);
        assert_eq!(result.parameters[0].name, "UserId");
        assert_eq!(result.parameters[1].name, "Name");
    }
}
