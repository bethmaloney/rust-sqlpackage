//! Centralized identifier handling utilities for T-SQL parsing.
//!
//! This module provides consistent functions for handling SQL Server identifier
//! formatting, normalization, and bracket handling. These utilities consolidate
//! the duplicate bracket/quote handling code scattered throughout the codebase.
//!
//! # Examples
//!
//! ```ignore
//! use crate::parser::identifier_utils::*;
//!
//! // Strip brackets/quotes from identifiers
//! assert_eq!(normalize_identifier("[MyTable]"), "MyTable");
//! assert_eq!(normalize_identifier("\"MyTable\""), "MyTable");
//!
//! // Ensure identifiers are bracketed
//! assert_eq!(ensure_bracketed("MyTable"), "[MyTable]");
//! assert_eq!(ensure_bracketed("[MyTable]"), "[MyTable]");
//!
//! // Normalize object names with default schema
//! assert_eq!(normalize_object_name("MyTable", "dbo"), "[dbo].[MyTable]");
//! assert_eq!(normalize_object_name("[schema].[table]", "dbo"), "[schema].[table]");
//! ```

use std::borrow::Cow;

use sqlparser::tokenizer::Token;

/// Strips brackets `[]` and double quotes `""` from an identifier.
///
/// This function removes leading/trailing bracket and quote characters
/// from SQL Server identifiers, returning the bare identifier name.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(normalize_identifier("[MyTable]"), "MyTable");
/// assert_eq!(normalize_identifier("\"MyColumn\""), "MyColumn");
/// assert_eq!(normalize_identifier("dbo"), "dbo");
/// assert_eq!(normalize_identifier("  [Trimmed]  "), "Trimmed");
/// ```
pub fn normalize_identifier(ident: &str) -> String {
    ident
        .trim()
        .trim_matches(|c| c == '[' || c == ']' || c == '"')
        .to_string()
}

/// Ensures an identifier is wrapped in brackets.
///
/// If the identifier is already bracketed, it is returned as-is.
/// Otherwise, brackets are added around it.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(ensure_bracketed("MyTable"), "[MyTable]");
/// assert_eq!(ensure_bracketed("[MyTable]"), "[MyTable]");
/// assert_eq!(ensure_bracketed(""), "[]");
/// ```
pub fn ensure_bracketed(ident: &str) -> String {
    let trimmed = ident.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed.to_string()
    } else {
        format!("[{}]", normalize_identifier(trimmed))
    }
}

/// Converts a sqlparser-rs Word token to a properly quoted string.
///
/// This function examines the `quote_style` of the Word token and formats
/// the identifier accordingly:
/// - `Some('[')` -> `[identifier]`
/// - `Some('"')` -> `"identifier"`
/// - `None` -> `identifier` (unquoted)
///
/// # Examples
///
/// ```ignore
/// use sqlparser::tokenizer::Word;
///
/// let word = Word { value: "MyTable".to_string(), quote_style: Some('['), keyword: Keyword::NoKeyword };
/// assert_eq!(format_word(&word), "[MyTable]");
/// ```
pub fn format_word(word: &sqlparser::tokenizer::Word) -> String {
    match word.quote_style {
        Some('[') => format!("[{}]", word.value),
        Some('"') => format!("\"{}\"", word.value),
        _ => word.value.clone(),
    }
}

/// Converts a Word token to a bracketed string, regardless of original quote style.
///
/// This is useful when all quoted identifiers should be normalized to SQL Server's
/// bracket syntax `[identifier]`.
///
/// # Examples
///
/// ```ignore
/// use sqlparser::tokenizer::Word;
///
/// let word = Word { value: "MyTable".to_string(), quote_style: Some('"'), keyword: Keyword::NoKeyword };
/// assert_eq!(format_word_bracketed(&word), "[MyTable]");
/// ```
pub fn format_word_bracketed(word: &sqlparser::tokenizer::Word) -> String {
    if word.quote_style.is_some() {
        format!("[{}]", word.value)
    } else {
        word.value.clone()
    }
}

/// Converts a sqlparser-rs Token to a string representation.
///
/// This is useful for reconstructing SQL text from a token stream while
/// preserving the original quoting style of identifiers.
///
/// # Supported Token Types
///
/// - `Word` - identifiers with quote style preserved
/// - `Number` - numeric literals
/// - `SingleQuotedString` - 'string' literals
/// - `NationalStringLiteral` - N'string' literals
/// - Punctuation tokens (parens, comma, operators, etc.)
/// - Other tokens fall back to their debug representation
pub fn format_token(token: &Token) -> String {
    match token {
        Token::Word(w) => format_word(w),
        Token::Number(n, _) => n.clone(),
        Token::SingleQuotedString(s) => format!("'{}'", s),
        Token::NationalStringLiteral(s) => format!("N'{}'", s),
        Token::LParen => "(".to_string(),
        Token::RParen => ")".to_string(),
        Token::Comma => ",".to_string(),
        Token::Period => ".".to_string(),
        Token::SemiColon => ";".to_string(),
        Token::Colon => ":".to_string(),
        Token::Plus => "+".to_string(),
        Token::Minus => "-".to_string(),
        Token::Mul => "*".to_string(),
        Token::Div => "/".to_string(),
        Token::Mod => "%".to_string(),
        Token::Eq => "=".to_string(),
        Token::Neq => "<>".to_string(),
        Token::Lt => "<".to_string(),
        Token::Gt => ">".to_string(),
        Token::LtEq => "<=".to_string(),
        Token::GtEq => ">=".to_string(),
        Token::Whitespace(ws) => ws.to_string(),
        _ => format!("{:?}", token),
    }
}

/// Converts a sqlparser-rs Token to a SQL-safe string representation.
///
/// Similar to `format_token`, but escapes single quotes inside string literals
/// by doubling them (SQL standard escaping). This is useful when reconstructing
/// SQL that will be re-parsed or executed.
///
/// # Examples
///
/// ```ignore
/// let token = Token::SingleQuotedString("it's a test".to_string());
/// assert_eq!(format_token_sql(&token), "'it''s a test'");
/// ```
pub fn format_token_sql(token: &Token) -> String {
    match token {
        Token::Word(w) => format_word(w),
        Token::Number(n, _) => n.clone(),
        Token::SingleQuotedString(s) => format!("'{}'", s.replace('\'', "''")),
        Token::NationalStringLiteral(s) => format!("N'{}'", s.replace('\'', "''")),
        Token::DoubleQuotedString(s) => format!("\"{}\"", s),
        Token::HexStringLiteral(s) => format!("0x{}", s),
        Token::LParen => "(".to_string(),
        Token::RParen => ")".to_string(),
        Token::Comma => ",".to_string(),
        Token::Period => ".".to_string(),
        Token::SemiColon => ";".to_string(),
        Token::Colon => ":".to_string(),
        Token::DoubleColon => "::".to_string(),
        Token::Plus => "+".to_string(),
        Token::Minus => "-".to_string(),
        Token::Mul => "*".to_string(),
        Token::Div => "/".to_string(),
        Token::Mod => "%".to_string(),
        Token::Eq => "=".to_string(),
        Token::Neq => "<>".to_string(),
        Token::Lt => "<".to_string(),
        Token::Gt => ">".to_string(),
        Token::LtEq => "<=".to_string(),
        Token::GtEq => ">=".to_string(),
        Token::Whitespace(ws) => ws.to_string(),
        Token::AtSign => "@".to_string(),
        Token::Sharp => "#".to_string(),
        Token::Ampersand => "&".to_string(),
        Token::Pipe => "|".to_string(),
        Token::Caret => "^".to_string(),
        Token::Tilde => "~".to_string(),
        Token::ExclamationMark => "!".to_string(),
        Token::LBracket => "[".to_string(),
        Token::RBracket => "]".to_string(),
        Token::LBrace => "{".to_string(),
        Token::RBrace => "}".to_string(),
        _ => format!("{}", token),
    }
}

/// Converts a sqlparser-rs Token to a SQL-safe string with bracket normalization.
///
/// Similar to `format_token_sql`, but converts any quoted identifier (whether
/// bracket or double-quoted) to bracket-quoted format `[identifier]`.
///
/// # Examples
///
/// ```ignore
/// let word = Word { value: "Table".to_string(), quote_style: Some('"'), .. };
/// let token = Token::Word(word);
/// assert_eq!(format_token_sql_bracketed(&token), "[Table]");
/// ```
pub fn format_token_sql_bracketed(token: &Token) -> String {
    match token {
        Token::Word(w) => format_word_bracketed(w),
        _ => format_token_sql(token),
    }
}

/// Converts a sqlparser-rs Token to a SQL-safe string, returning `Cow<'static, str>`.
///
/// This is a performance-optimized version of `format_token_sql` that returns
/// `Cow::Borrowed` for static tokens (punctuation, operators) and `Cow::Owned`
/// for dynamic content (identifiers, strings, numbers).
///
/// Use this version in hot paths where avoiding allocations for static tokens
/// improves performance.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(format_token_sql_cow(&Token::LParen), Cow::Borrowed("("));
/// ```
pub fn format_token_sql_cow(token: &Token) -> Cow<'static, str> {
    match token {
        Token::Word(w) => Cow::Owned(format_word(w)),
        Token::Number(n, _) => Cow::Owned(n.clone()),
        Token::Char(c) => Cow::Owned(c.to_string()),
        Token::SingleQuotedString(s) => Cow::Owned(format!("'{}'", s.replace('\'', "''"))),
        Token::NationalStringLiteral(s) => Cow::Owned(format!("N'{}'", s.replace('\'', "''"))),
        Token::HexStringLiteral(s) => Cow::Owned(format!("0x{}", s)),
        Token::DoubleQuotedString(s) => Cow::Owned(format!("\"{}\"", s)),
        Token::SingleQuotedByteStringLiteral(s) => Cow::Owned(format!("b'{}'", s)),
        Token::DoubleQuotedByteStringLiteral(s) => Cow::Owned(format!("b\"{}\"", s)),
        Token::DollarQuotedString(s) => Cow::Owned(s.to_string()),
        Token::Whitespace(w) => Cow::Owned(w.to_string()),
        // Static tokens - no allocation needed
        Token::LParen => Cow::Borrowed("("),
        Token::RParen => Cow::Borrowed(")"),
        Token::LBrace => Cow::Borrowed("{"),
        Token::RBrace => Cow::Borrowed("}"),
        Token::LBracket => Cow::Borrowed("["),
        Token::RBracket => Cow::Borrowed("]"),
        Token::Comma => Cow::Borrowed(","),
        Token::Period => Cow::Borrowed("."),
        Token::Colon => Cow::Borrowed(":"),
        Token::DoubleColon => Cow::Borrowed("::"),
        Token::SemiColon => Cow::Borrowed(";"),
        Token::Eq => Cow::Borrowed("="),
        Token::Neq => Cow::Borrowed("<>"),
        Token::Lt => Cow::Borrowed("<"),
        Token::Gt => Cow::Borrowed(">"),
        Token::LtEq => Cow::Borrowed("<="),
        Token::GtEq => Cow::Borrowed(">="),
        Token::Spaceship => Cow::Borrowed("<=>"),
        Token::Plus => Cow::Borrowed("+"),
        Token::Minus => Cow::Borrowed("-"),
        Token::Mul => Cow::Borrowed("*"),
        Token::Div => Cow::Borrowed("/"),
        Token::Mod => Cow::Borrowed("%"),
        Token::StringConcat => Cow::Borrowed("||"),
        Token::LongArrow => Cow::Borrowed("->>"),
        Token::Arrow => Cow::Borrowed("->"),
        Token::HashArrow => Cow::Borrowed("#>"),
        Token::HashLongArrow => Cow::Borrowed("#>>"),
        Token::AtSign => Cow::Borrowed("@"),
        Token::Sharp => Cow::Borrowed("#"),
        Token::Ampersand => Cow::Borrowed("&"),
        Token::Pipe => Cow::Borrowed("|"),
        Token::Caret => Cow::Borrowed("^"),
        Token::Tilde => Cow::Borrowed("~"),
        Token::ExclamationMark => Cow::Borrowed("!"),
        _ => Cow::Borrowed(""), // Handle unknown tokens gracefully
    }
}

/// Normalizes an object name to `[schema].[name]` format.
///
/// This function handles various input formats:
/// - Already formatted: `[schema].[name]` -> returned as-is
/// - Dot-separated: `schema.name` -> `[schema].[name]`
/// - Unqualified: `name` -> `[default_schema].[name]`
/// - Mixed brackets: `[schema].name` -> `[schema].[name]`
///
/// # Arguments
///
/// * `name` - The object name (table, view, type, etc.)
/// * `default_schema` - Schema to use if not specified (typically "dbo")
///
/// # Examples
///
/// ```ignore
/// assert_eq!(normalize_object_name("MyTable", "dbo"), "[dbo].[MyTable]");
/// assert_eq!(normalize_object_name("schema.table", "dbo"), "[schema].[table]");
/// assert_eq!(normalize_object_name("[schema].[table]", "dbo"), "[schema].[table]");
/// assert_eq!(normalize_object_name("[schema].table", "dbo"), "[schema].[table]");
/// ```
pub fn normalize_object_name(name: &str, default_schema: &str) -> String {
    let trimmed = name.trim();

    // Already in [schema].[name] format
    if trimmed.starts_with('[') && trimmed.contains("].[") {
        // Still normalize to ensure consistent formatting
        if let Some((schema_part, name_part)) = trimmed.split_once("].[") {
            let schema = schema_part.trim_start_matches('[');
            let name = name_part.trim_end_matches(']');
            return format!("[{}].[{}]", schema, name);
        }
        return trimmed.to_string();
    }

    // Check if it contains a dot (schema.name format)
    if trimmed.contains('.') {
        let parts: Vec<&str> = trimmed.splitn(2, '.').collect();
        if parts.len() == 2 {
            let schema = normalize_identifier(parts[0]);
            let obj_name = normalize_identifier(parts[1]);
            return format!("[{}].[{}]", schema, obj_name);
        }
    }

    // No schema specified, use default
    let obj_name = normalize_identifier(trimmed);
    format!("[{}].[{}]", default_schema, obj_name)
}

/// Checks if a string is a bracketed identifier (starts with `[` and ends with `]`).
///
/// # Examples
///
/// ```ignore
/// assert!(is_bracketed("[MyTable]"));
/// assert!(!is_bracketed("MyTable"));
/// assert!(!is_bracketed("[MyTable"));
/// ```
pub fn is_bracketed(ident: &str) -> bool {
    let trimmed = ident.trim();
    trimmed.starts_with('[') && trimmed.ends_with(']')
}

/// Checks if a string is a quoted identifier (starts and ends with `"`).
///
/// # Examples
///
/// ```ignore
/// assert!(is_double_quoted("\"MyTable\""));
/// assert!(!is_double_quoted("MyTable"));
/// ```
pub fn is_double_quoted(ident: &str) -> bool {
    let trimmed = ident.trim();
    trimmed.starts_with('"') && trimmed.ends_with('"')
}

/// Checks if a string appears to be a qualified name (contains schema separator).
///
/// This detects patterns like `schema.name` or `[schema].[name]`.
///
/// # Examples
///
/// ```ignore
/// assert!(is_qualified_name("dbo.MyTable"));
/// assert!(is_qualified_name("[dbo].[MyTable]"));
/// assert!(!is_qualified_name("MyTable"));
/// ```
pub fn is_qualified_name(name: &str) -> bool {
    let trimmed = name.trim();
    // Check for [schema].[name] pattern
    if trimmed.contains("].[") {
        return true;
    }
    // Check for schema.name pattern (but not inside brackets which could be [contains.dot])
    if !trimmed.starts_with('[') && trimmed.contains('.') {
        return true;
    }
    // Check for [schema].name pattern
    if trimmed.starts_with('[') && trimmed.contains("].") {
        return true;
    }
    false
}

/// Splits a qualified name into schema and object name parts.
///
/// Returns `(schema, name)` tuple. If no schema is present, returns
/// the default schema with the object name.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(split_qualified_name("[dbo].[MyTable]", "dbo"), ("dbo", "MyTable"));
/// assert_eq!(split_qualified_name("schema.table", "dbo"), ("schema", "table"));
/// assert_eq!(split_qualified_name("MyTable", "dbo"), ("dbo", "MyTable"));
/// ```
pub fn split_qualified_name<'a>(name: &'a str, default_schema: &'a str) -> (String, String) {
    let trimmed = name.trim();

    // Handle [schema].[name] format
    if trimmed.contains("].[") {
        if let Some((schema_part, name_part)) = trimmed.split_once("].[") {
            let schema = schema_part.trim_start_matches('[').to_string();
            let obj_name = name_part.trim_end_matches(']').to_string();
            return (schema, obj_name);
        }
    }

    // Handle [schema].name format
    if trimmed.starts_with('[') && trimmed.contains("].") && !trimmed.contains("].[") {
        if let Some((schema_part, name_part)) = trimmed.split_once("].") {
            let schema = schema_part.trim_start_matches('[').to_string();
            let obj_name = normalize_identifier(name_part);
            return (schema, obj_name);
        }
    }

    // Handle schema.[name] format (check this before generic schema.name)
    if trimmed.contains(".[") {
        if let Some((schema_part, name_part)) = trimmed.split_once(".[") {
            let schema = normalize_identifier(schema_part);
            // Strip the trailing ] from the name part
            let obj_name = normalize_identifier(name_part);
            return (schema, obj_name);
        }
    }

    // Handle schema.name format (unbracketed)
    if trimmed.contains('.') && !trimmed.starts_with('[') && !trimmed.contains('[') {
        let parts: Vec<&str> = trimmed.splitn(2, '.').collect();
        if parts.len() == 2 {
            return (parts[0].to_string(), parts[1].to_string());
        }
    }

    // No schema, use default
    (default_schema.to_string(), normalize_identifier(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::keywords::Keyword;
    use sqlparser::tokenizer::{Token, Word};

    #[test]
    fn test_normalize_identifier_brackets() {
        assert_eq!(normalize_identifier("[MyTable]"), "MyTable");
        assert_eq!(normalize_identifier("[My Table]"), "My Table");
        // Note: trim_matches strips all matching chars from both ends
        // so [[Nested]] becomes Nested, not [Nested]
        assert_eq!(normalize_identifier("[[Nested]]"), "Nested");
    }

    #[test]
    fn test_normalize_identifier_quotes() {
        assert_eq!(normalize_identifier("\"MyColumn\""), "MyColumn");
        // Note: trim_matches strips all matching chars from both ends
        assert_eq!(normalize_identifier("\"\"DoubleQuote\"\""), "DoubleQuote");
    }

    #[test]
    fn test_normalize_identifier_plain() {
        assert_eq!(normalize_identifier("dbo"), "dbo");
        assert_eq!(normalize_identifier("  spaces  "), "spaces");
    }

    #[test]
    fn test_ensure_bracketed() {
        assert_eq!(ensure_bracketed("MyTable"), "[MyTable]");
        assert_eq!(ensure_bracketed("[MyTable]"), "[MyTable]");
        assert_eq!(ensure_bracketed("  MyTable  "), "[MyTable]");
        assert_eq!(
            ensure_bracketed("[Already Bracketed]"),
            "[Already Bracketed]"
        );
    }

    #[test]
    fn test_format_word_bracketed() {
        let word = Word {
            value: "MyTable".to_string(),
            quote_style: Some('['),
            keyword: Keyword::NoKeyword,
        };
        assert_eq!(format_word(&word), "[MyTable]");
    }

    #[test]
    fn test_format_word_quoted() {
        let word = Word {
            value: "MyColumn".to_string(),
            quote_style: Some('"'),
            keyword: Keyword::NoKeyword,
        };
        assert_eq!(format_word(&word), "\"MyColumn\"");
    }

    #[test]
    fn test_format_word_unquoted() {
        let word = Word {
            value: "SELECT".to_string(),
            quote_style: None,
            keyword: Keyword::SELECT,
        };
        assert_eq!(format_word(&word), "SELECT");
    }

    #[test]
    fn test_format_token() {
        assert_eq!(format_token(&Token::LParen), "(");
        assert_eq!(format_token(&Token::RParen), ")");
        assert_eq!(format_token(&Token::Comma), ",");
        assert_eq!(format_token(&Token::Period), ".");
        assert_eq!(format_token(&Token::Eq), "=");
        assert_eq!(format_token(&Token::Number("42".to_string(), false)), "42");
        assert_eq!(
            format_token(&Token::SingleQuotedString("test".to_string())),
            "'test'"
        );
        assert_eq!(
            format_token(&Token::NationalStringLiteral("unicode".to_string())),
            "N'unicode'"
        );
    }

    #[test]
    fn test_normalize_object_name_unqualified() {
        assert_eq!(normalize_object_name("MyTable", "dbo"), "[dbo].[MyTable]");
        assert_eq!(normalize_object_name("[MyTable]", "dbo"), "[dbo].[MyTable]");
    }

    #[test]
    fn test_normalize_object_name_qualified() {
        assert_eq!(
            normalize_object_name("schema.table", "dbo"),
            "[schema].[table]"
        );
        assert_eq!(
            normalize_object_name("[schema].[table]", "dbo"),
            "[schema].[table]"
        );
        assert_eq!(
            normalize_object_name("[schema].table", "dbo"),
            "[schema].[table]"
        );
        assert_eq!(
            normalize_object_name("schema.[table]", "dbo"),
            "[schema].[table]"
        );
    }

    #[test]
    fn test_normalize_object_name_with_spaces() {
        assert_eq!(
            normalize_object_name("  MyTable  ", "dbo"),
            "[dbo].[MyTable]"
        );
        assert_eq!(
            normalize_object_name("  [schema].[table]  ", "dbo"),
            "[schema].[table]"
        );
    }

    #[test]
    fn test_is_bracketed() {
        assert!(is_bracketed("[MyTable]"));
        assert!(is_bracketed("  [MyTable]  "));
        assert!(!is_bracketed("MyTable"));
        assert!(!is_bracketed("[MyTable"));
        assert!(!is_bracketed("MyTable]"));
    }

    #[test]
    fn test_is_double_quoted() {
        assert!(is_double_quoted("\"MyTable\""));
        assert!(is_double_quoted("  \"MyTable\"  "));
        assert!(!is_double_quoted("MyTable"));
        assert!(!is_double_quoted("'MyTable'"));
    }

    #[test]
    fn test_is_qualified_name() {
        assert!(is_qualified_name("dbo.MyTable"));
        assert!(is_qualified_name("[dbo].[MyTable]"));
        assert!(is_qualified_name("[dbo].MyTable"));
        assert!(!is_qualified_name("MyTable"));
        assert!(!is_qualified_name("[MyTable]"));
    }

    #[test]
    fn test_split_qualified_name_fully_bracketed() {
        let (schema, name) = split_qualified_name("[dbo].[MyTable]", "default");
        assert_eq!(schema, "dbo");
        assert_eq!(name, "MyTable");
    }

    #[test]
    fn test_split_qualified_name_unbracketed() {
        let (schema, name) = split_qualified_name("schema.table", "default");
        assert_eq!(schema, "schema");
        assert_eq!(name, "table");
    }

    #[test]
    fn test_split_qualified_name_unqualified() {
        let (schema, name) = split_qualified_name("MyTable", "dbo");
        assert_eq!(schema, "dbo");
        assert_eq!(name, "MyTable");
    }

    #[test]
    fn test_split_qualified_name_mixed_brackets() {
        let (schema, name) = split_qualified_name("[schema].table", "dbo");
        assert_eq!(schema, "schema");
        assert_eq!(name, "table");

        let (schema2, name2) = split_qualified_name("schema.[table]", "dbo");
        assert_eq!(schema2, "schema");
        assert_eq!(name2, "table");
    }

    #[test]
    fn test_format_word_bracketed_converts_double_quote() {
        // Double-quoted should become bracketed
        let word = Word {
            value: "MyColumn".to_string(),
            quote_style: Some('"'),
            keyword: Keyword::NoKeyword,
        };
        assert_eq!(format_word_bracketed(&word), "[MyColumn]");
    }

    #[test]
    fn test_format_word_bracketed_preserves_bracket() {
        let word = Word {
            value: "MyTable".to_string(),
            quote_style: Some('['),
            keyword: Keyword::NoKeyword,
        };
        assert_eq!(format_word_bracketed(&word), "[MyTable]");
    }

    #[test]
    fn test_format_word_bracketed_unquoted() {
        let word = Word {
            value: "SELECT".to_string(),
            quote_style: None,
            keyword: Keyword::SELECT,
        };
        assert_eq!(format_word_bracketed(&word), "SELECT");
    }

    #[test]
    fn test_format_token_sql_escapes_quotes() {
        // Single quoted string with embedded quote
        assert_eq!(
            format_token_sql(&Token::SingleQuotedString("it's a test".to_string())),
            "'it''s a test'"
        );

        // National string literal with embedded quote
        assert_eq!(
            format_token_sql(&Token::NationalStringLiteral("it's unicode".to_string())),
            "N'it''s unicode'"
        );
    }

    #[test]
    fn test_format_token_sql_operators() {
        assert_eq!(format_token_sql(&Token::AtSign), "@");
        assert_eq!(format_token_sql(&Token::Sharp), "#");
        assert_eq!(format_token_sql(&Token::DoubleColon), "::");
        assert_eq!(format_token_sql(&Token::LBracket), "[");
        assert_eq!(format_token_sql(&Token::RBracket), "]");
    }

    #[test]
    fn test_format_token_sql_bracketed() {
        // Double-quoted word becomes bracketed
        let word = Word {
            value: "MyColumn".to_string(),
            quote_style: Some('"'),
            keyword: Keyword::NoKeyword,
        };
        assert_eq!(format_token_sql_bracketed(&Token::Word(word)), "[MyColumn]");

        // Regular tokens pass through unchanged
        assert_eq!(
            format_token_sql_bracketed(&Token::SingleQuotedString("it's".to_string())),
            "'it''s'"
        );
    }
}
