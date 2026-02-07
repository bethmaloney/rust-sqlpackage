//! Token-based column definition parsing for T-SQL
//!
//! This module provides token-based parsing for column definitions, replacing
//! the previous regex-based approach. Part of Phase 15.2 of the implementation plan.
//!
//! ## Supported Syntax
//!
//! Regular columns:
//! ```sql
//! [Name] TYPE [IDENTITY(seed, increment)] [NOT NULL|NULL]
//!     [CONSTRAINT name DEFAULT (value)|DEFAULT (value)]
//!     [CONSTRAINT name CHECK (expr)|CHECK (expr)]
//!     [ROWGUIDCOL] [SPARSE] [FILESTREAM]
//! ```
//!
//! Computed columns:
//! ```sql
//! [Name] AS (expression) [PERSISTED] [NOT NULL]
//! ```

use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::Token;

use super::token_parser_base::TokenParser;

/// Result of parsing a column definition using tokens
#[derive(Debug, Clone, Default)]
pub struct TokenParsedColumn {
    /// Column name
    pub name: String,
    /// Data type (e.g., "NVARCHAR(50)", "INT", "DECIMAL(18, 2)")
    /// For computed columns, this will be empty
    pub data_type: String,
    /// Column nullability: Some(true) = explicit NULL, Some(false) = explicit NOT NULL, None = implicit
    pub nullability: Option<bool>,
    /// Whether the column has IDENTITY
    pub is_identity: bool,
    /// Whether the column has ROWGUIDCOL
    pub is_rowguidcol: bool,
    /// Whether the column has SPARSE attribute
    pub is_sparse: bool,
    /// Whether the column has FILESTREAM attribute
    pub is_filestream: bool,
    /// Default constraint name (if any)
    pub default_constraint_name: Option<String>,
    /// Default value expression (if any)
    pub default_value: Option<String>,
    /// Whether the default constraint name should be emitted (true = "NOT NULL CONSTRAINT [name] DEFAULT" pattern)
    /// DotNet only emits the Name attribute when CONSTRAINT appears AFTER NOT NULL.
    pub emit_default_constraint_name: bool,
    /// Inline CHECK constraint name (if any)
    pub check_constraint_name: Option<String>,
    /// Inline CHECK constraint expression (if any)
    pub check_expression: Option<String>,
    /// Whether the check constraint name should be emitted (true = appears after NOT NULL)
    pub emit_check_constraint_name: bool,
    /// Computed column expression (e.g., "([Qty] * [Price])")
    /// If Some, this is a computed column with no explicit data type
    pub computed_expression: Option<String>,
    /// Whether the computed column is PERSISTED (stored physically)
    pub is_persisted: bool,
    /// Collation name (e.g., "Latin1_General_CS_AS")
    /// Only populated for string columns with explicit COLLATE clause
    pub collation: Option<String>,
    /// Whether this column is GENERATED ALWAYS AS ROW START (temporal table period start column)
    pub is_generated_always_start: bool,
    /// Whether this column is GENERATED ALWAYS AS ROW END (temporal table period end column)
    pub is_generated_always_end: bool,
    /// Whether this column has the HIDDEN attribute (temporal table hidden period columns)
    pub is_hidden: bool,
}

/// Token-based column definition parser
pub struct ColumnTokenParser {
    base: TokenParser,
}

impl ColumnTokenParser {
    /// Create a new parser for a column definition string
    pub fn new(col_def: &str) -> Option<Self> {
        Some(Self {
            base: TokenParser::new(col_def)?,
        })
    }

    /// Parse the column definition and return the result
    pub fn parse(&mut self) -> Option<TokenParsedColumn> {
        // Skip leading whitespace tokens
        self.base.skip_whitespace();

        // Check if empty
        if self.base.is_at_end() {
            return None;
        }

        // First token should be the column name (identifier)
        let name = self.base.parse_identifier()?;

        self.base.skip_whitespace();

        // Check if this is a computed column: [Name] AS (expression)
        if self.base.check_keyword(Keyword::AS) {
            return self.parse_computed_column(name);
        }

        // Regular column: parse data type
        let data_type = self.parse_data_type()?;

        // Check for COLLATE clause immediately after data type
        let collation = self.parse_collation();

        // Now parse optional column modifiers in any order
        let mut result = TokenParsedColumn {
            name,
            data_type,
            collation,
            ..Default::default()
        };

        self.parse_column_modifiers(&mut result);

        Some(result)
    }

    /// Parse a computed column: [Name] AS (expression) [PERSISTED] [NOT NULL]
    fn parse_computed_column(&mut self, name: String) -> Option<TokenParsedColumn> {
        // Consume AS
        self.base.expect_keyword(Keyword::AS)?;
        self.base.skip_whitespace();

        // Parse the expression (everything in parentheses)
        let expression = self.parse_parenthesized_expression()?;

        let mut result = TokenParsedColumn {
            name,
            computed_expression: Some(format!("({})", expression)),
            ..Default::default()
        };

        // Parse optional PERSISTED and nullability
        loop {
            self.base.skip_whitespace();
            if self.base.is_at_end() {
                break;
            }

            if self.base.check_word_ci("PERSISTED") {
                self.base.advance();
                result.is_persisted = true;
            } else if self.base.check_keyword(Keyword::NOT) {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_keyword(Keyword::NULL) {
                    self.base.advance();
                    result.nullability = Some(false);
                }
            } else if self.base.check_keyword(Keyword::NULL) {
                self.base.advance();
                result.nullability = Some(true);
            } else {
                break;
            }
        }

        Some(result)
    }

    /// Parse column modifiers (IDENTITY, NOT NULL, DEFAULT, CHECK, etc.)
    fn parse_column_modifiers(&mut self, result: &mut TokenParsedColumn) {
        // Track CONSTRAINT [name] that immediately precedes DEFAULT/CHECK
        // This is cleared when other keywords appear between CONSTRAINT and DEFAULT/CHECK
        let mut pending_constraint_name: Option<String> = None;
        // Track whether pending_constraint_name immediately precedes the constraint
        // (i.e., no intervening keywords like NOT NULL between CONSTRAINT and DEFAULT)
        let mut constraint_immediately_precedes = false;

        loop {
            self.base.skip_whitespace();
            if self.base.is_at_end() {
                break;
            }

            // Check for IDENTITY
            if self.base.check_keyword(Keyword::IDENTITY) {
                self.base.advance();
                result.is_identity = true;
                // Skip optional (seed, increment)
                self.base.skip_whitespace();
                if self.base.check_token(&Token::LParen) {
                    self.base.skip_parenthesized();
                }
                // IDENTITY separates CONSTRAINT from DEFAULT
                constraint_immediately_precedes = false;
                continue;
            }

            // Check for NOT NULL
            if self.base.check_keyword(Keyword::NOT) {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_keyword(Keyword::NULL) {
                    self.base.advance();
                    result.nullability = Some(false);
                    // NOT NULL separates CONSTRAINT from DEFAULT
                    constraint_immediately_precedes = false;
                }
                continue;
            }

            // Check for NULL (explicit nullable)
            if self.base.check_keyword(Keyword::NULL) {
                // Make sure this isn't part of NOT NULL
                self.base.advance();
                if result.nullability.is_none() {
                    result.nullability = Some(true);
                    // NULL separates CONSTRAINT from DEFAULT
                    constraint_immediately_precedes = false;
                }
                continue;
            }

            // Check for CONSTRAINT keyword (names the next constraint)
            if self.base.check_keyword(Keyword::CONSTRAINT) {
                self.base.advance();
                self.base.skip_whitespace();
                let constraint_name = self.base.parse_identifier();
                pending_constraint_name = constraint_name;
                // Mark that CONSTRAINT immediately precedes what comes next
                constraint_immediately_precedes = true;
                continue;
            }

            // Check for DEFAULT
            if self.base.check_keyword(Keyword::DEFAULT) {
                self.base.advance();
                self.base.skip_whitespace();

                // Parse default value
                let default_value = self.parse_default_value();
                result.default_value = default_value;
                result.default_constraint_name = pending_constraint_name.take();
                // .NET DacFx emits Name attribute only when CONSTRAINT [name] immediately
                // precedes DEFAULT (e.g., "CONSTRAINT [name] DEFAULT value").
                // When NOT NULL appears between CONSTRAINT and DEFAULT, Name is NOT emitted.
                // Pattern "CONSTRAINT [name] NOT NULL DEFAULT" → Name NOT emitted
                // Pattern "NOT NULL CONSTRAINT [name] DEFAULT" → Name emitted
                // Pattern "CONSTRAINT [name] DEFAULT value NOT NULL" → Name emitted
                result.emit_default_constraint_name =
                    result.default_constraint_name.is_some() && constraint_immediately_precedes;
                constraint_immediately_precedes = false;
                continue;
            }

            // Check for CHECK
            if self.base.check_keyword(Keyword::CHECK) {
                self.base.advance();
                self.base.skip_whitespace();

                // Parse check expression
                if self.base.check_token(&Token::LParen) {
                    let expr = self.parse_parenthesized_expression();
                    result.check_expression = expr;
                    result.check_constraint_name = pending_constraint_name.take();
                    // Same logic as DEFAULT - emit Name only if CONSTRAINT immediately precedes CHECK
                    result.emit_check_constraint_name =
                        result.check_constraint_name.is_some() && constraint_immediately_precedes;
                    constraint_immediately_precedes = false;
                }
                continue;
            }

            // Check for ROWGUIDCOL
            if self.base.check_word_ci("ROWGUIDCOL") {
                self.base.advance();
                result.is_rowguidcol = true;
                continue;
            }

            // Check for SPARSE
            if self.base.check_word_ci("SPARSE") {
                self.base.advance();
                result.is_sparse = true;
                continue;
            }

            // Check for FILESTREAM
            if self.base.check_word_ci("FILESTREAM") {
                self.base.advance();
                result.is_filestream = true;
                continue;
            }

            // Check for GENERATED ALWAYS AS ROW START/END (temporal table period columns)
            if self.base.check_word_ci("GENERATED") {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_word_ci("ALWAYS") {
                    self.base.advance();
                    self.base.skip_whitespace();
                    if self.base.check_keyword(Keyword::AS) {
                        self.base.advance();
                        self.base.skip_whitespace();
                        if self.base.check_word_ci("ROW") {
                            self.base.advance();
                            self.base.skip_whitespace();
                            if self.base.check_word_ci("START") {
                                self.base.advance();
                                result.is_generated_always_start = true;
                            } else if self.base.check_word_ci("END") {
                                self.base.advance();
                                result.is_generated_always_end = true;
                            }
                        }
                    }
                }
                // GENERATED ALWAYS separates CONSTRAINT from DEFAULT
                constraint_immediately_precedes = false;
                continue;
            }

            // Check for HIDDEN (temporal table hidden period columns)
            if self.base.check_word_ci("HIDDEN") {
                self.base.advance();
                result.is_hidden = true;
                constraint_immediately_precedes = false;
                continue;
            }

            // Check for PERSISTED (for computed columns, but can appear in regular column context)
            if self.base.check_word_ci("PERSISTED") {
                self.base.advance();
                result.is_persisted = true;
                continue;
            }

            // Check for PRIMARY KEY (inline) - skip it
            if self.base.check_keyword(Keyword::PRIMARY) {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_keyword(Keyword::KEY) {
                    self.base.advance();
                    // Skip optional CLUSTERED/NONCLUSTERED
                    self.base.skip_whitespace();
                    if self.base.check_keyword(Keyword::CLUSTERED)
                        || self.base.check_word_ci("NONCLUSTERED")
                    {
                        self.base.advance();
                    }
                }
                continue;
            }

            // Check for UNIQUE (inline) - skip it
            if self.base.check_keyword(Keyword::UNIQUE) {
                self.base.advance();
                // Skip optional CLUSTERED/NONCLUSTERED
                self.base.skip_whitespace();
                if self.base.check_keyword(Keyword::CLUSTERED)
                    || self.base.check_word_ci("NONCLUSTERED")
                {
                    self.base.advance();
                }
                continue;
            }

            // Check for FOREIGN KEY (inline) - skip it
            if self.base.check_keyword(Keyword::FOREIGN) {
                self.base.advance();
                self.base.skip_whitespace();
                if self.base.check_keyword(Keyword::KEY) {
                    self.base.advance();
                }
                // Skip REFERENCES clause
                self.base.skip_whitespace();
                if self.base.check_keyword(Keyword::REFERENCES) {
                    // Skip until end or next keyword
                    while !self.base.is_at_end() {
                        if self.base.check_keyword(Keyword::CONSTRAINT)
                            || self.base.check_keyword(Keyword::DEFAULT)
                            || self.base.check_keyword(Keyword::CHECK)
                        {
                            break;
                        }
                        self.base.advance();
                    }
                }
                continue;
            }

            // Unknown token - break to avoid infinite loop
            break;
        }
    }

    /// Parse a data type (e.g., INT, NVARCHAR(50), DECIMAL(18, 2))
    fn parse_data_type(&mut self) -> Option<String> {
        self.base.skip_whitespace();

        let mut data_type = String::new();

        // Get the base type name
        let token = self.base.current_token()?;
        match &token.token {
            Token::Word(word) => {
                data_type.push_str(&word.value.to_uppercase());
                self.base.advance();
            }
            _ => return None,
        }

        // Check for type parameters in parentheses
        self.base.skip_whitespace();
        if self.base.check_token(&Token::LParen) {
            let params = self.consume_parenthesized_raw()?;
            data_type.push('(');
            data_type.push_str(&params);
            data_type.push(')');
        }

        Some(data_type)
    }

    /// Parse optional COLLATE clause (e.g., COLLATE Latin1_General_CS_AS)
    /// Returns the collation name if present
    fn parse_collation(&mut self) -> Option<String> {
        self.base.skip_whitespace();

        // Check for COLLATE keyword
        if !self.base.check_word_ci("COLLATE") {
            return None;
        }

        self.base.advance(); // consume COLLATE
        self.base.skip_whitespace();

        // The collation name is a Word token (e.g., Latin1_General_CS_AS)
        if let Some(token) = self.base.current_token() {
            if let Token::Word(word) = &token.token {
                let collation = word.value.clone();
                self.base.advance();
                return Some(collation);
            }
        }

        None
    }

    /// Parse a DEFAULT value (handles various forms: function calls, literals, parenthesized expressions)
    fn parse_default_value(&mut self) -> Option<String> {
        self.base.skip_whitespace();

        // Check what kind of default value we have
        if self.base.check_token(&Token::LParen) {
            // Parenthesized expression like ((0)) or (GETDATE())
            let expr = self.parse_parenthesized_expression()?;
            return Some(format!("({})", expr));
        }

        // Check for function call like GETDATE(), NEWID(), etc.
        if let Some(token) = self.base.current_token() {
            if let Token::Word(word) = &token.token {
                let func_name = word.value.clone();
                self.base.advance();
                self.base.skip_whitespace();

                // Check if followed by ()
                if self.base.check_token(&Token::LParen) {
                    self.base.advance(); // consume (
                    self.base.skip_whitespace();
                    if self.base.check_token(&Token::RParen) {
                        self.base.advance(); // consume )
                        return Some(format!("{}()", func_name));
                    }
                    // Function with args - reconstruct
                    let mut args = String::new();
                    let mut depth = 1;
                    while !self.base.is_at_end() && depth > 0 {
                        if let Some(t) = self.base.current_token() {
                            match &t.token {
                                Token::LParen => depth += 1,
                                Token::RParen => depth -= 1,
                                _ => {}
                            }
                            if depth > 0 {
                                args.push_str(&TokenParser::token_to_string(&t.token));
                            }
                            self.base.advance();
                        }
                    }
                    return Some(format!("{}({})", func_name, args.trim()));
                }

                // Just a word - could be a keyword like NULL
                return Some(func_name);
            }
        }

        // Check for string literal
        if let Some(token) = self.base.current_token() {
            match &token.token {
                Token::SingleQuotedString(s) => {
                    let value = format!("'{}'", s.replace('\'', "''"));
                    self.base.advance();
                    return Some(value);
                }
                Token::NationalStringLiteral(s) => {
                    let value = format!("N'{}'", s.replace('\'', "''"));
                    self.base.advance();
                    return Some(value);
                }
                Token::Number(n, _) => {
                    let value = n.clone();
                    self.base.advance();
                    return Some(value);
                }
                Token::Minus => {
                    // Negative number
                    self.base.advance();
                    if let Some(next) = self.base.current_token() {
                        if let Token::Number(n, _) = &next.token {
                            let value = format!("-{}", n);
                            self.base.advance();
                            return Some(value);
                        }
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Parse a parenthesized expression and return its contents
    /// Reconstructs the content from tokens
    fn parse_parenthesized_expression(&mut self) -> Option<String> {
        if !self.base.check_token(&Token::LParen) {
            return None;
        }

        self.base.advance(); // consume (

        let mut depth = 1;
        let mut content = String::new();

        while !self.base.is_at_end() && depth > 0 {
            if let Some(token) = self.base.current_token() {
                match &token.token {
                    Token::LParen => {
                        depth += 1;
                        content.push('(');
                    }
                    Token::RParen => {
                        depth -= 1;
                        if depth > 0 {
                            content.push(')');
                        }
                        // depth == 0 means we hit the closing paren, don't add it
                    }
                    _ => {
                        content.push_str(&TokenParser::token_to_string(&token.token));
                    }
                }
                self.base.advance();
            }
        }

        Some(content.trim().to_string())
    }

    /// Consume a parenthesized section and return the raw content
    fn consume_parenthesized_raw(&mut self) -> Option<String> {
        if !self.base.check_token(&Token::LParen) {
            return None;
        }

        self.base.advance(); // consume (

        let mut depth = 1;
        let mut content = String::new();

        while !self.base.is_at_end() && depth > 0 {
            if let Some(token) = self.base.current_token() {
                match &token.token {
                    Token::LParen => {
                        depth += 1;
                        content.push('(');
                    }
                    Token::RParen => {
                        depth -= 1;
                        if depth > 0 {
                            content.push(')');
                        }
                    }
                    _ => {
                        content.push_str(&TokenParser::token_to_string(&token.token));
                    }
                }
                self.base.advance();
            }
        }

        Some(content.trim().to_string())
    }
}

/// Parse a column definition using token-based parsing
///
/// This function replaces the regex-based `parse_column_definition` function
/// with a token-based approach for better maintainability and error handling.
pub fn parse_column_definition_tokens(col_def: &str) -> Option<TokenParsedColumn> {
    // Strip leading SQL comments (-- style) before parsing
    let col_def = strip_leading_comments(col_def);
    let col_def = col_def.trim();
    if col_def.is_empty() {
        return None;
    }

    let mut parser = ColumnTokenParser::new(col_def)?;
    parser.parse()
}

/// Strip leading SQL line comments (-- style)
fn strip_leading_comments(sql: &str) -> String {
    let mut result = String::new();
    let mut in_comment = false;

    for line in sql.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("--") {
            in_comment = true;
            continue;
        }
        if in_comment && trimmed.is_empty() {
            continue;
        }
        in_comment = false;
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(line);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_column() {
        let result = parse_column_definition_tokens("[Id] INT").unwrap();
        assert_eq!(result.name, "Id");
        assert_eq!(result.data_type, "INT");
        assert!(result.nullability.is_none());
    }

    #[test]
    fn test_column_with_not_null() {
        let result = parse_column_definition_tokens("[Name] NVARCHAR(100) NOT NULL").unwrap();
        assert_eq!(result.name, "Name");
        assert_eq!(result.data_type, "NVARCHAR(100)");
        assert_eq!(result.nullability, Some(false));
    }

    #[test]
    fn test_column_with_null() {
        let result = parse_column_definition_tokens("[Description] NVARCHAR(MAX) NULL").unwrap();
        assert_eq!(result.name, "Description");
        assert_eq!(result.data_type, "NVARCHAR(MAX)");
        assert_eq!(result.nullability, Some(true));
    }

    #[test]
    fn test_column_with_identity() {
        let result = parse_column_definition_tokens("[Id] INT NOT NULL IDENTITY(1, 1)").unwrap();
        assert_eq!(result.name, "Id");
        assert_eq!(result.data_type, "INT");
        assert!(result.is_identity);
        assert_eq!(result.nullability, Some(false));
    }

    #[test]
    fn test_column_with_default_function() {
        let result =
            parse_column_definition_tokens("[CreatedAt] DATETIME NOT NULL DEFAULT GETDATE()")
                .unwrap();
        assert_eq!(result.name, "CreatedAt");
        assert_eq!(result.data_type, "DATETIME");
        assert_eq!(result.default_value, Some("GETDATE()".to_string()));
        assert!(result.default_constraint_name.is_none());
    }

    #[test]
    fn test_column_with_named_default() {
        let result = parse_column_definition_tokens(
            "[Status] INT NOT NULL CONSTRAINT [DF_Status] DEFAULT ((0))",
        )
        .unwrap();
        assert_eq!(result.name, "Status");
        assert_eq!(result.data_type, "INT");
        assert_eq!(
            result.default_constraint_name,
            Some("DF_Status".to_string())
        );
        assert_eq!(result.default_value, Some("((0))".to_string()));
    }

    #[test]
    fn test_column_with_named_default_number() {
        let result = parse_column_definition_tokens(
            "[Amount] DECIMAL(18, 2) NOT NULL CONSTRAINT [DF_Amount] DEFAULT 0.00",
        )
        .unwrap();
        assert_eq!(result.name, "Amount");
        assert_eq!(result.data_type, "DECIMAL(18, 2)");
        assert_eq!(
            result.default_constraint_name,
            Some("DF_Amount".to_string())
        );
        assert_eq!(result.default_value, Some("0.00".to_string()));
    }

    #[test]
    fn test_column_with_constraint_default_not_null_pattern() {
        // Pattern: TYPE CONSTRAINT [name] DEFAULT (value) NOT NULL
        // This is the pattern used in TableWithMultipleCommalessConstraints
        let result = parse_column_definition_tokens(
            "[Version] INT CONSTRAINT [DF_MultiCommaless_Version] DEFAULT ((0)) NOT NULL",
        )
        .unwrap();
        assert_eq!(result.name, "Version");
        assert_eq!(result.data_type, "INT");
        assert_eq!(
            result.default_constraint_name,
            Some("DF_MultiCommaless_Version".to_string())
        );
        assert_eq!(result.default_value, Some("((0))".to_string()));
        assert_eq!(result.nullability, Some(false)); // NOT NULL
    }

    #[test]
    fn test_column_with_default_string() {
        let result =
            parse_column_definition_tokens("[Status] VARCHAR(20) NOT NULL DEFAULT 'Pending'")
                .unwrap();
        assert_eq!(result.name, "Status");
        assert_eq!(result.default_value, Some("'Pending'".to_string()));
    }

    #[test]
    fn test_column_with_check_constraint() {
        let result = parse_column_definition_tokens(
            "[Age] INT NOT NULL CONSTRAINT [CK_Age] CHECK ([Age] >= 0)",
        )
        .unwrap();
        assert_eq!(result.name, "Age");
        assert_eq!(result.check_constraint_name, Some("CK_Age".to_string()));
        assert_eq!(result.check_expression, Some("[Age] >= 0".to_string()));
    }

    #[test]
    fn test_column_with_unnamed_check() {
        let result =
            parse_column_definition_tokens("[Quantity] INT NOT NULL CHECK ([Quantity] > 0)")
                .unwrap();
        assert_eq!(result.name, "Quantity");
        assert!(result.check_constraint_name.is_none());
        assert_eq!(result.check_expression, Some("[Quantity] > 0".to_string()));
    }

    #[test]
    fn test_computed_column() {
        let result = parse_column_definition_tokens("[Total] AS ([Qty] * [Price])").unwrap();
        assert_eq!(result.name, "Total");
        assert!(result.data_type.is_empty());
        assert_eq!(
            result.computed_expression,
            Some("([Qty] * [Price])".to_string())
        );
        assert!(!result.is_persisted);
    }

    #[test]
    fn test_computed_column_persisted() {
        let result =
            parse_column_definition_tokens("[Total] AS ([Qty] * [Price]) PERSISTED").unwrap();
        assert_eq!(result.name, "Total");
        assert_eq!(
            result.computed_expression,
            Some("([Qty] * [Price])".to_string())
        );
        assert!(result.is_persisted);
    }

    #[test]
    fn test_computed_column_with_nullability() {
        let result =
            parse_column_definition_tokens("[Total] AS ([Qty] * [Price]) PERSISTED NOT NULL")
                .unwrap();
        assert_eq!(result.name, "Total");
        assert!(result.is_persisted);
        assert_eq!(result.nullability, Some(false));
    }

    #[test]
    fn test_column_with_rowguidcol() {
        let result = parse_column_definition_tokens(
            "[RowGuid] UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID() ROWGUIDCOL",
        )
        .unwrap();
        assert_eq!(result.name, "RowGuid");
        assert!(result.is_rowguidcol);
    }

    #[test]
    fn test_column_with_sparse() {
        let result =
            parse_column_definition_tokens("[OptionalField] NVARCHAR(100) SPARSE NULL").unwrap();
        assert_eq!(result.name, "OptionalField");
        assert!(result.is_sparse);
        assert_eq!(result.nullability, Some(true));
    }

    #[test]
    fn test_column_without_brackets() {
        let result = parse_column_definition_tokens("Id INT NOT NULL").unwrap();
        assert_eq!(result.name, "Id");
        assert_eq!(result.data_type, "INT");
    }

    #[test]
    fn test_column_with_constraint_before_not_null() {
        // T-SQL allows: CONSTRAINT name NOT NULL DEFAULT value
        let result = parse_column_definition_tokens(
            "[Guid] UNIQUEIDENTIFIER CONSTRAINT [DF_Documents_Guid] NOT NULL DEFAULT NEWSEQUENTIALID()",
        )
        .unwrap();
        assert_eq!(result.name, "Guid");
        assert_eq!(
            result.default_constraint_name,
            Some("DF_Documents_Guid".to_string())
        );
        // Note: The constraint name applies to the DEFAULT, not NOT NULL
        assert_eq!(result.nullability, Some(false));
    }

    #[test]
    fn test_column_with_default_and_check() {
        let result = parse_column_definition_tokens(
            "[Score] INT NOT NULL CONSTRAINT [DF_Score] DEFAULT ((0)) CONSTRAINT [CK_Score] CHECK ([Score] >= 0 AND [Score] <= 100)",
        )
        .unwrap();
        assert_eq!(result.name, "Score");
        assert_eq!(result.default_constraint_name, Some("DF_Score".to_string()));
        assert_eq!(result.default_value, Some("((0))".to_string()));
        assert_eq!(result.check_constraint_name, Some("CK_Score".to_string()));
        assert!(result.check_expression.is_some());
    }

    #[test]
    fn test_column_with_negative_default() {
        let result = parse_column_definition_tokens("[Offset] INT NOT NULL DEFAULT -1").unwrap();
        assert_eq!(result.name, "Offset");
        assert_eq!(result.default_value, Some("-1".to_string()));
    }

    #[test]
    fn test_column_definition_with_leading_comment() {
        let result = parse_column_definition_tokens("-- This is a comment\n[Id] INT").unwrap();
        assert_eq!(result.name, "Id");
        assert_eq!(result.data_type, "INT");
    }

    #[test]
    fn test_unquoted_column_name() {
        let result = parse_column_definition_tokens("UserName VARCHAR(50) NOT NULL").unwrap();
        assert_eq!(result.name, "UserName");
        assert_eq!(result.data_type, "VARCHAR(50)");
    }

    #[test]
    fn test_constraint_null_default_pattern() {
        // Pattern: CONSTRAINT [name] NULL DEFAULT value
        let result = parse_column_definition_tokens(
            "[Active] BIT CONSTRAINT [DF_Active] NULL DEFAULT ((1))",
        )
        .unwrap();
        assert_eq!(result.name, "Active");
        assert_eq!(
            result.default_constraint_name,
            Some("DF_Active".to_string())
        );
        assert_eq!(result.default_value, Some("((1))".to_string()));
        assert_eq!(result.nullability, Some(true)); // explicit NULL
    }

    #[test]
    fn test_unnamed_check_emit_name_is_false() {
        // Unnamed CHECK constraint should have emit_check_constraint_name = false
        // This is the pattern used in AuditLog: CHECK ([Action] IN ('Insert', 'Update', 'Delete'))
        let result = parse_column_definition_tokens(
            "[Action] NVARCHAR(50) NOT NULL CHECK ([Action] IN ('Insert', 'Update', 'Delete'))",
        )
        .unwrap();
        assert_eq!(result.name, "Action");
        assert!(result.check_constraint_name.is_none());
        assert!(result.check_expression.is_some());
        // Key assertion: emit_check_constraint_name should be false for unnamed CHECK
        assert!(
            !result.emit_check_constraint_name,
            "emit_check_constraint_name should be false for unnamed CHECK constraint"
        );
    }

    #[test]
    fn test_named_check_after_not_null_emit_name_is_true() {
        // Named CHECK constraint AFTER NOT NULL should have emit_check_constraint_name = true
        // Pattern: NOT NULL CONSTRAINT [name] CHECK (...)
        let result = parse_column_definition_tokens(
            "[Age] INT NOT NULL CONSTRAINT [CK_Age] CHECK ([Age] >= 0)",
        )
        .unwrap();
        assert_eq!(result.name, "Age");
        assert_eq!(result.check_constraint_name, Some("CK_Age".to_string()));
        // Key assertion: emit_check_constraint_name should be true when CONSTRAINT is after NOT NULL
        assert!(
            result.emit_check_constraint_name,
            "emit_check_constraint_name should be true for named CHECK after NOT NULL"
        );
    }

    #[test]
    fn test_column_with_collate() {
        let result = parse_column_definition_tokens(
            "[Name] NVARCHAR(100) COLLATE Latin1_General_CS_AS NOT NULL",
        )
        .unwrap();
        assert_eq!(result.name, "Name");
        assert_eq!(result.data_type, "NVARCHAR(100)");
        assert_eq!(result.collation, Some("Latin1_General_CS_AS".to_string()));
        assert_eq!(result.nullability, Some(false));
    }

    #[test]
    fn test_column_with_collate_various() {
        // Test different collation names
        let result = parse_column_definition_tokens(
            "[Text] VARCHAR(50) COLLATE SQL_Latin1_General_CP1_CI_AS NULL",
        )
        .unwrap();
        assert_eq!(
            result.collation,
            Some("SQL_Latin1_General_CP1_CI_AS".to_string())
        );

        let result =
            parse_column_definition_tokens("[JapaneseText] NVARCHAR(200) COLLATE Japanese_CI_AS")
                .unwrap();
        assert_eq!(result.collation, Some("Japanese_CI_AS".to_string()));
    }

    #[test]
    fn test_column_without_collate() {
        // Regular column without COLLATE should have None
        let result = parse_column_definition_tokens("[Id] INT NOT NULL").unwrap();
        assert!(result.collation.is_none());

        let result = parse_column_definition_tokens("[Name] NVARCHAR(100) NOT NULL").unwrap();
        assert!(result.collation.is_none());
    }

    // ========================================================================
    // Temporal column tests (Phase 57)
    // ========================================================================

    #[test]
    fn test_generated_always_as_row_start() {
        let result = parse_column_definition_tokens(
            "[SysStartTime] DATETIME2 GENERATED ALWAYS AS ROW START NOT NULL",
        )
        .unwrap();
        assert_eq!(result.name, "SysStartTime");
        assert_eq!(result.data_type, "DATETIME2");
        assert!(result.is_generated_always_start);
        assert!(!result.is_generated_always_end);
        assert!(!result.is_hidden);
        assert_eq!(result.nullability, Some(false));
    }

    #[test]
    fn test_generated_always_as_row_end() {
        let result = parse_column_definition_tokens(
            "[SysEndTime] DATETIME2 GENERATED ALWAYS AS ROW END NOT NULL",
        )
        .unwrap();
        assert_eq!(result.name, "SysEndTime");
        assert_eq!(result.data_type, "DATETIME2");
        assert!(!result.is_generated_always_start);
        assert!(result.is_generated_always_end);
        assert!(!result.is_hidden);
        assert_eq!(result.nullability, Some(false));
    }

    #[test]
    fn test_generated_always_with_hidden() {
        let result = parse_column_definition_tokens(
            "[ValidFrom] DATETIME2 GENERATED ALWAYS AS ROW START HIDDEN NOT NULL",
        )
        .unwrap();
        assert_eq!(result.name, "ValidFrom");
        assert_eq!(result.data_type, "DATETIME2");
        assert!(result.is_generated_always_start);
        assert!(result.is_hidden);
        assert_eq!(result.nullability, Some(false));
    }

    #[test]
    fn test_generated_always_end_hidden() {
        let result = parse_column_definition_tokens(
            "[ValidTo] DATETIME2 GENERATED ALWAYS AS ROW END HIDDEN NOT NULL",
        )
        .unwrap();
        assert_eq!(result.name, "ValidTo");
        assert!(result.is_generated_always_end);
        assert!(result.is_hidden);
    }

    #[test]
    fn test_regular_column_no_temporal_flags() {
        let result = parse_column_definition_tokens("[Name] NVARCHAR(100) NOT NULL").unwrap();
        assert!(!result.is_generated_always_start);
        assert!(!result.is_generated_always_end);
        assert!(!result.is_hidden);
    }

    #[test]
    fn test_generated_always_with_constraint_default() {
        // ALTER TABLE pattern: GENERATED ALWAYS + HIDDEN + CONSTRAINT DEFAULT
        let result = parse_column_definition_tokens(
            "[ValidFrom] DATETIME2 GENERATED ALWAYS AS ROW START HIDDEN CONSTRAINT [DF_ValidFrom] DEFAULT SYSUTCDATETIME() NOT NULL",
        )
        .unwrap();
        assert_eq!(result.name, "ValidFrom");
        assert!(result.is_generated_always_start);
        assert!(result.is_hidden);
        assert_eq!(
            result.default_constraint_name,
            Some("DF_ValidFrom".to_string())
        );
        assert_eq!(result.default_value, Some("SYSUTCDATETIME()".to_string()));
        assert_eq!(result.nullability, Some(false));
    }
}
