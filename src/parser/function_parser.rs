//! Token-based function definition parsing for T-SQL
//!
//! This module provides token-based parsing for function definitions, replacing
//! the previous regex-based approach. Part of Phase 15.3 (B2) of the implementation plan.
//!
//! ## Supported Syntax
//!
//! CREATE FUNCTION:
//! ```sql
//! CREATE FUNCTION [schema].[name](@param1 TYPE, ...) RETURNS TYPE AS ...
//! CREATE OR ALTER FUNCTION [schema].[name](...) RETURNS TABLE AS ...
//! ```
//!
//! ALTER FUNCTION:
//! ```sql
//! ALTER FUNCTION [schema].[name](@param1 TYPE, ...) RETURNS TYPE AS ...
//! ```
//!
//! Function types supported:
//! - Scalar functions (RETURNS <type>)
//! - Inline table-valued functions (RETURNS TABLE)
//! - Multi-statement table-valued functions (RETURNS @var TABLE)

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

/// Result of parsing a function definition using tokens
#[derive(Debug, Clone, Default)]
pub struct TokenParsedFunction {
    /// Schema name (defaults to "dbo" if not specified)
    pub schema: String,
    /// Function name
    pub name: String,
    /// Function parameters
    pub parameters: Vec<TokenParsedParameter>,
    /// Return type (e.g., "INT", "DECIMAL(18,2)", "TABLE")
    pub return_type: Option<String>,
    /// Function type (Scalar, TableValued, InlineTableValued)
    pub function_type: TokenParsedFunctionType,
}

/// A parameter extracted from a function definition
#[derive(Debug, Clone)]
pub struct TokenParsedParameter {
    /// Parameter name (including @ prefix)
    pub name: String,
    /// Data type (e.g., "INT", "DECIMAL(18, 2)")
    pub data_type: String,
    /// Default value if specified (reserved for future use)
    #[allow(dead_code)]
    pub default_value: Option<String>,
}

/// Function type detected from SQL
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenParsedFunctionType {
    #[default]
    Scalar,
    TableValued,
    InlineTableValued,
}

/// Token-based function definition parser
pub struct FunctionTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl FunctionTokenParser {
    /// Create a new parser for a function definition string
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Parse CREATE FUNCTION and return complete function info
    pub fn parse_create_function(&mut self) -> Option<TokenParsedFunction> {
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

        // Expect FUNCTION keyword
        if !self.check_keyword(Keyword::FUNCTION) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse the schema-qualified name
        let (schema, name) = self.parse_schema_qualified_name()?;
        self.skip_whitespace();

        // Parse parameters (if present)
        let parameters = self.parse_parameters();
        self.skip_whitespace();

        // Parse RETURNS clause
        let (return_type, function_type) = self.parse_returns_clause();

        Some(TokenParsedFunction {
            schema,
            name,
            parameters,
            return_type,
            function_type,
        })
    }

    /// Parse ALTER FUNCTION and return complete function info
    pub fn parse_alter_function(&mut self) -> Option<TokenParsedFunction> {
        self.skip_whitespace();

        // Expect ALTER keyword
        if !self.check_keyword(Keyword::ALTER) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect FUNCTION keyword
        if !self.check_keyword(Keyword::FUNCTION) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse the schema-qualified name
        let (schema, name) = self.parse_schema_qualified_name()?;
        self.skip_whitespace();

        // Parse parameters (if present)
        let parameters = self.parse_parameters();
        self.skip_whitespace();

        // Parse RETURNS clause
        let (return_type, function_type) = self.parse_returns_clause();

        Some(TokenParsedFunction {
            schema,
            name,
            parameters,
            return_type,
            function_type,
        })
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

    /// Parse function parameters: (@param1 TYPE, @param2 TYPE = default, ...)
    fn parse_parameters(&mut self) -> Vec<TokenParsedParameter> {
        let mut params = Vec::new();

        // Expect opening parenthesis
        if !self.check_token(&Token::LParen) {
            return params;
        }
        self.advance();
        self.skip_whitespace();

        // Handle empty parameter list
        if self.check_token(&Token::RParen) {
            self.advance();
            return params;
        }

        loop {
            // Parse parameter name (must start with @)
            if let Some(param) = self.parse_single_parameter() {
                params.push(param);
            } else {
                // Skip to next comma or closing paren
                self.skip_to_param_delimiter();
            }

            self.skip_whitespace();

            // Check for comma (more parameters) or closing paren (end)
            if self.check_token(&Token::Comma) {
                self.advance();
                self.skip_whitespace();
            } else if self.check_token(&Token::RParen) {
                self.advance();
                break;
            } else {
                // Unexpected token, try to recover by finding closing paren
                self.skip_to_token(&Token::RParen);
                if self.check_token(&Token::RParen) {
                    self.advance();
                }
                break;
            }
        }

        params
    }

    /// Parse a single parameter: @name TYPE [= default]
    fn parse_single_parameter(&mut self) -> Option<TokenParsedParameter> {
        // Parameter name should be a Word starting with @
        // MsSqlDialect tokenizes @name as a single Word token
        let name = self.parse_parameter_name()?;
        self.skip_whitespace();

        // Parse data type
        let data_type = self.parse_data_type()?;
        self.skip_whitespace();

        // Check for default value (= ...)
        let default_value = if self.check_token(&Token::Eq) {
            self.advance();
            self.skip_whitespace();
            Some(self.parse_default_value())
        } else {
            None
        };

        Some(TokenParsedParameter {
            name,
            data_type,
            default_value,
        })
    }

    /// Parse parameter name (@name)
    fn parse_parameter_name(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        let token = self.current_token()?;
        match &token.token {
            // MsSqlDialect tokenizes @name as a Word
            Token::Word(w) if w.value.starts_with('@') => {
                let name = w.value.clone();
                self.advance();
                Some(name)
            }
            _ => None,
        }
    }

    /// Parse data type (e.g., INT, DECIMAL(18, 2), NVARCHAR(100))
    fn parse_data_type(&mut self) -> Option<String> {
        let mut result = String::new();

        // Get base type name
        let type_name = self.parse_identifier()?;
        result.push_str(&type_name.to_uppercase());

        self.skip_whitespace();

        // Check for type parameters in parentheses
        if self.check_token(&Token::LParen) {
            result.push('(');
            self.advance();

            let mut depth = 1;
            while !self.is_at_end() && depth > 0 {
                if let Some(token) = self.current_token() {
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
                            // Minimal whitespace handling in type
                        }
                        _ => {
                            result.push_str(&self.token_to_string(&token.token));
                        }
                    }
                    self.advance();
                }
            }
            result.push(')');
        }

        Some(result)
    }

    /// Parse default value (everything up to , or ))
    fn parse_default_value(&mut self) -> String {
        let mut result = String::new();
        let mut depth = 0;

        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                match &token.token {
                    Token::LParen => {
                        depth += 1;
                        result.push('(');
                        self.advance();
                    }
                    Token::RParen => {
                        if depth == 0 {
                            break; // End of parameters
                        }
                        depth -= 1;
                        result.push(')');
                        self.advance();
                    }
                    Token::Comma if depth == 0 => {
                        break; // Next parameter
                    }
                    Token::Whitespace(_) => {
                        self.advance();
                    }
                    _ => {
                        result.push_str(&self.token_to_string(&token.token));
                        self.advance();
                    }
                }
            } else {
                break;
            }
        }

        result.trim().to_string()
    }

    /// Parse RETURNS clause and determine function type
    fn parse_returns_clause(&mut self) -> (Option<String>, TokenParsedFunctionType) {
        // Find RETURNS keyword
        while !self.is_at_end() {
            if self.check_word_ci("RETURNS") {
                self.advance();
                self.skip_whitespace();
                break;
            }
            self.advance();
        }

        if self.is_at_end() {
            return (None, TokenParsedFunctionType::Scalar);
        }

        // Check for TABLE (inline TVF)
        if self.check_keyword(Keyword::TABLE) {
            self.advance();
            return (
                Some("TABLE".to_string()),
                TokenParsedFunctionType::InlineTableValued,
            );
        }

        // Check for @var TABLE (multi-statement TVF)
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                if w.value.starts_with('@') {
                    // Multi-statement TVF: RETURNS @var TABLE (...)
                    return (
                        Some("TABLE".to_string()),
                        TokenParsedFunctionType::TableValued,
                    );
                }
            }
        }

        // Scalar function - parse the return type
        if let Some(return_type) = self.parse_data_type() {
            (Some(return_type), TokenParsedFunctionType::Scalar)
        } else {
            (None, TokenParsedFunctionType::Scalar)
        }
    }

    // ========================================================================
    // Helper methods
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

    /// Skip to next parameter delimiter (comma or closing paren)
    fn skip_to_param_delimiter(&mut self) {
        let mut depth = 0;
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                match &token.token {
                    Token::LParen => depth += 1,
                    Token::RParen if depth > 0 => depth -= 1,
                    Token::RParen if depth == 0 => return,
                    Token::Comma if depth == 0 => return,
                    _ => {}
                }
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Skip to a specific token
    fn skip_to_token(&mut self, target: &Token) {
        while !self.is_at_end() {
            if self.check_token(target) {
                return;
            }
            self.advance();
        }
    }

    /// Convert a token to its string representation
    fn token_to_string(&self, token: &Token) -> String {
        match token {
            Token::Word(w) => w.value.clone(),
            Token::Number(n, _) => n.clone(),
            Token::SingleQuotedString(s) => format!("'{}'", s),
            Token::NationalStringLiteral(s) => format!("N'{}'", s),
            Token::Comma => ",".to_string(),
            Token::Period => ".".to_string(),
            Token::LParen => "(".to_string(),
            Token::RParen => ")".to_string(),
            Token::Eq => "=".to_string(),
            Token::Minus => "-".to_string(),
            Token::Plus => "+".to_string(),
            Token::Mul => "*".to_string(),
            Token::Div => "/".to_string(),
            _ => String::new(),
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

// ============================================================================
// Public API functions
// ============================================================================

/// Parse CREATE FUNCTION using tokens and return (schema, name)
///
/// This function replaces the regex-based `extract_function_name` function.
/// Supports:
/// - CREATE FUNCTION [dbo].[FuncName]
/// - CREATE FUNCTION dbo.FuncName
/// - CREATE OR ALTER FUNCTION [schema].[name]
/// - CREATE FUNCTION [dbo].[Name&With&Special]
pub fn parse_create_function_tokens(sql: &str) -> Option<(String, String)> {
    let mut parser = FunctionTokenParser::new(sql)?;
    let result = parser.parse_create_function()?;
    Some((result.schema, result.name))
}

/// Parse ALTER FUNCTION using tokens and return (schema, name)
///
/// This function replaces the regex-based `extract_alter_function_name` function.
/// Supports:
/// - ALTER FUNCTION [dbo].[FuncName]
/// - ALTER FUNCTION dbo.FuncName
pub fn parse_alter_function_tokens(sql: &str) -> Option<(String, String)> {
    let mut parser = FunctionTokenParser::new(sql)?;
    let result = parser.parse_alter_function()?;
    Some((result.schema, result.name))
}

/// Parse CREATE FUNCTION and extract all information including parameters and return type
pub fn parse_create_function_full(sql: &str) -> Option<TokenParsedFunction> {
    let mut parser = FunctionTokenParser::new(sql)?;
    parser.parse_create_function()
}

/// Parse ALTER FUNCTION and extract all information including parameters and return type
pub fn parse_alter_function_full(sql: &str) -> Option<TokenParsedFunction> {
    let mut parser = FunctionTokenParser::new(sql)?;
    parser.parse_alter_function()
}

/// Detect function type from SQL using token-based parsing
pub fn detect_function_type_tokens(sql: &str) -> TokenParsedFunctionType {
    if let Some(func) = parse_create_function_full(sql).or_else(|| parse_alter_function_full(sql)) {
        func.function_type
    } else {
        // Fallback to simple string matching if tokenization fails
        let sql_upper = sql.to_uppercase();
        if sql_upper.contains("RETURNS TABLE") {
            TokenParsedFunctionType::InlineTableValued
        } else if sql_upper.contains("RETURNS @") {
            TokenParsedFunctionType::TableValued
        } else {
            TokenParsedFunctionType::Scalar
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // CREATE FUNCTION name extraction tests
    // ========================================================================

    #[test]
    fn test_create_function_bracketed_schema_and_name() {
        let result = parse_create_function_tokens(
            "CREATE FUNCTION [dbo].[GetUserCount]() RETURNS INT AS BEGIN RETURN 1 END",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUserCount");
    }

    #[test]
    fn test_create_function_unbracketed() {
        let result = parse_create_function_tokens(
            "CREATE FUNCTION dbo.GetUserCount() RETURNS INT AS BEGIN RETURN 1 END",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUserCount");
    }

    #[test]
    fn test_create_function_no_schema() {
        let result = parse_create_function_tokens(
            "CREATE FUNCTION [GetUserCount]() RETURNS INT AS BEGIN RETURN 1 END",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUserCount");
    }

    #[test]
    fn test_create_function_no_schema_unbracketed() {
        let result = parse_create_function_tokens(
            "CREATE FUNCTION GetUserCount() RETURNS INT AS BEGIN RETURN 1 END",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUserCount");
    }

    #[test]
    fn test_create_or_alter_function() {
        let result = parse_create_function_tokens(
            "CREATE OR ALTER FUNCTION [dbo].[GetUserCount]() RETURNS INT AS BEGIN RETURN 1 END",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUserCount");
    }

    #[test]
    fn test_create_function_with_special_characters() {
        let result = parse_create_function_tokens(
            "CREATE FUNCTION [dbo].[Get&User&Count]() RETURNS INT AS BEGIN RETURN 1 END",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "Get&User&Count");
    }

    #[test]
    fn test_create_function_custom_schema() {
        let result = parse_create_function_tokens(
            "CREATE FUNCTION [Sales].[GetOrderTotal](@OrderId INT) RETURNS DECIMAL(18, 2) AS BEGIN RETURN 0 END",
        )
        .unwrap();
        assert_eq!(result.0, "Sales");
        assert_eq!(result.1, "GetOrderTotal");
    }

    // ========================================================================
    // ALTER FUNCTION name extraction tests
    // ========================================================================

    #[test]
    fn test_alter_function_bracketed() {
        let result = parse_alter_function_tokens(
            "ALTER FUNCTION [dbo].[GetUserCount]() RETURNS INT AS BEGIN RETURN 2 END",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUserCount");
    }

    #[test]
    fn test_alter_function_unbracketed() {
        let result = parse_alter_function_tokens(
            "ALTER FUNCTION dbo.GetUserCount() RETURNS INT AS BEGIN RETURN 2 END",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUserCount");
    }

    #[test]
    fn test_alter_function_no_schema() {
        let result = parse_alter_function_tokens(
            "ALTER FUNCTION [GetUserCount]() RETURNS INT AS BEGIN RETURN 2 END",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetUserCount");
    }

    #[test]
    fn test_alter_function_custom_schema() {
        let result = parse_alter_function_tokens(
            "ALTER FUNCTION [Sales].[GetOrderTotal](@OrderId INT) RETURNS DECIMAL(18, 2) AS BEGIN RETURN 0 END",
        )
        .unwrap();
        assert_eq!(result.0, "Sales");
        assert_eq!(result.1, "GetOrderTotal");
    }

    // ========================================================================
    // Full parsing tests (parameters and return types)
    // ========================================================================

    #[test]
    fn test_parse_function_no_params() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[GetUserCount]() RETURNS INT AS BEGIN RETURN 1 END",
        )
        .unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "GetUserCount");
        assert!(result.parameters.is_empty());
        assert_eq!(result.return_type, Some("INT".to_string()));
        assert_eq!(result.function_type, TokenParsedFunctionType::Scalar);
    }

    #[test]
    fn test_parse_function_single_param() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[GetOrderTotal](@OrderId INT) RETURNS DECIMAL(18, 2) AS BEGIN RETURN 0 END",
        )
        .unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "GetOrderTotal");
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "@OrderId");
        assert_eq!(result.parameters[0].data_type, "INT");
        assert_eq!(result.return_type, Some("DECIMAL(18,2)".to_string()));
        assert_eq!(result.function_type, TokenParsedFunctionType::Scalar);
    }

    #[test]
    fn test_parse_function_multiple_params() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[CalculateTotal](@Quantity INT, @UnitPrice DECIMAL(18, 2)) RETURNS DECIMAL(18, 2) AS BEGIN RETURN @Quantity * @UnitPrice END",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 2);
        assert_eq!(result.parameters[0].name, "@Quantity");
        assert_eq!(result.parameters[0].data_type, "INT");
        assert_eq!(result.parameters[1].name, "@UnitPrice");
        assert_eq!(result.parameters[1].data_type, "DECIMAL(18,2)");
    }

    #[test]
    fn test_parse_function_with_default() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[CalculateTotal](@Quantity INT, @Discount DECIMAL(5, 2) = 0) RETURNS DECIMAL(18, 2) AS BEGIN RETURN @Quantity END",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 2);
        assert_eq!(result.parameters[0].name, "@Quantity");
        assert!(result.parameters[0].default_value.is_none());
        assert_eq!(result.parameters[1].name, "@Discount");
        assert_eq!(result.parameters[1].default_value, Some("0".to_string()));
    }

    #[test]
    fn test_parse_function_with_null_default() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[GetOrders](@CustomerId INT, @StartDate DATE = NULL) RETURNS TABLE AS RETURN (SELECT 1 AS Id)",
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 2);
        assert_eq!(result.parameters[1].name, "@StartDate");
        assert_eq!(result.parameters[1].default_value, Some("NULL".to_string()));
    }

    // ========================================================================
    // Function type detection tests
    // ========================================================================

    #[test]
    fn test_detect_scalar_function() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[GetCount]() RETURNS INT AS BEGIN RETURN 1 END",
        )
        .unwrap();
        assert_eq!(result.function_type, TokenParsedFunctionType::Scalar);
    }

    #[test]
    fn test_detect_inline_tvf() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[GetActiveUsers]() RETURNS TABLE AS RETURN (SELECT Id FROM Users)",
        )
        .unwrap();
        assert_eq!(
            result.function_type,
            TokenParsedFunctionType::InlineTableValued
        );
        assert_eq!(result.return_type, Some("TABLE".to_string()));
    }

    #[test]
    fn test_detect_multistatement_tvf() {
        let sql = r#"
CREATE FUNCTION [dbo].[GetUsersByName](@SearchName NVARCHAR(100))
RETURNS @Results TABLE (
    Id INT,
    Name NVARCHAR(100)
)
AS
BEGIN
    INSERT INTO @Results SELECT Id, Name FROM Users
    RETURN
END
"#;
        let result = parse_create_function_full(sql).unwrap();
        assert_eq!(result.function_type, TokenParsedFunctionType::TableValued);
        assert_eq!(result.return_type, Some("TABLE".to_string()));
    }

    #[test]
    fn test_detect_function_type_standalone() {
        assert_eq!(
            detect_function_type_tokens("CREATE FUNCTION f() RETURNS INT AS BEGIN RETURN 1 END"),
            TokenParsedFunctionType::Scalar
        );
        assert_eq!(
            detect_function_type_tokens("CREATE FUNCTION f() RETURNS TABLE AS RETURN SELECT 1"),
            TokenParsedFunctionType::InlineTableValued
        );
        assert_eq!(
            detect_function_type_tokens(
                "CREATE FUNCTION f() RETURNS @t TABLE (x INT) AS BEGIN RETURN END"
            ),
            TokenParsedFunctionType::TableValued
        );
    }

    // ========================================================================
    // Return type parsing tests
    // ========================================================================

    #[test]
    fn test_return_type_int() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[GetCount]() RETURNS INT AS BEGIN RETURN 1 END",
        )
        .unwrap();
        assert_eq!(result.return_type, Some("INT".to_string()));
    }

    #[test]
    fn test_return_type_decimal_with_precision() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[GetTotal]() RETURNS DECIMAL(18, 2) AS BEGIN RETURN 0 END",
        )
        .unwrap();
        assert_eq!(result.return_type, Some("DECIMAL(18,2)".to_string()));
    }

    #[test]
    fn test_return_type_nvarchar() {
        let result = parse_create_function_full(
            "CREATE FUNCTION [dbo].[GetName]() RETURNS NVARCHAR(100) AS BEGIN RETURN N'' END",
        )
        .unwrap();
        assert_eq!(result.return_type, Some("NVARCHAR(100)".to_string()));
    }

    // ========================================================================
    // Edge cases and error handling tests
    // ========================================================================

    #[test]
    fn test_not_a_function() {
        let result = parse_create_function_tokens("CREATE TABLE [dbo].[Users] (Id INT)");
        assert!(result.is_none());
    }

    #[test]
    fn test_alter_on_create() {
        let result = parse_create_function_tokens(
            "ALTER FUNCTION [dbo].[GetCount]() RETURNS INT AS BEGIN RETURN 1 END",
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_create_on_alter() {
        let result = parse_alter_function_tokens(
            "CREATE FUNCTION [dbo].[GetCount]() RETURNS INT AS BEGIN RETURN 1 END",
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_create_function_case_insensitive() {
        let result = parse_create_function_tokens(
            "create function [dbo].[GetCount]() returns int as begin return 1 end",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetCount");
    }

    #[test]
    fn test_create_function_mixed_case() {
        let result = parse_create_function_tokens(
            "Create Function [dbo].[GetCount]() Returns Int As Begin Return 1 End",
        )
        .unwrap();
        assert_eq!(result.0, "dbo");
        assert_eq!(result.1, "GetCount");
    }

    #[test]
    fn test_multiline_function() {
        let sql = r#"
CREATE FUNCTION [dbo].[GetProductCount]()
RETURNS INT
AS
BEGIN
    DECLARE @Count INT;
    SELECT @Count = COUNT(*) FROM [dbo].[Products];
    RETURN @Count;
END
"#;
        let result = parse_create_function_full(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "GetProductCount");
        assert!(result.parameters.is_empty());
        assert_eq!(result.return_type, Some("INT".to_string()));
        assert_eq!(result.function_type, TokenParsedFunctionType::Scalar);
    }

    #[test]
    fn test_multiline_params() {
        let sql = r#"
CREATE FUNCTION [dbo].[GetProductsInPriceRange]
(
    @MinPrice DECIMAL(18, 2),
    @MaxPrice DECIMAL(18, 2)
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Price]
    FROM [dbo].[Products]
    WHERE [Price] BETWEEN @MinPrice AND @MaxPrice
)
"#;
        let result = parse_create_function_full(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "GetProductsInPriceRange");
        assert_eq!(result.parameters.len(), 2);
        assert_eq!(result.parameters[0].name, "@MinPrice");
        assert_eq!(result.parameters[0].data_type, "DECIMAL(18,2)");
        assert_eq!(result.parameters[1].name, "@MaxPrice");
        assert_eq!(result.parameters[1].data_type, "DECIMAL(18,2)");
        assert_eq!(
            result.function_type,
            TokenParsedFunctionType::InlineTableValued
        );
    }
}
