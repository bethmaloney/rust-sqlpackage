//! Body dependency extraction for procedures, functions, and triggers.
//!
//! This module handles tokenizer-based extraction of dependencies from SQL body text,
//! including table references, column references, parameter references, and type references.
//!
//! Key components:
//! - `BodyDependencyTokenScanner`: Token-based scanner for body dependency extraction
//! - `TableAliasTokenParser`: Parser for extracting table aliases from FROM/JOIN clauses
//! - `QualifiedName`: Parsed qualified name with 1-3 parts
//! - `extract_body_dependencies`: Main function to extract all dependencies from SQL body

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer};
use std::collections::{HashMap, HashSet};

/// Represents dependencies extracted from a procedure/function body.
/// These map to XML `<Relationship>` elements with specific types.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BodyDependency {
    /// Reference to a built-in type (e.g., `[int]`, `[nvarchar]`)
    BuiltInType(String),
    /// Reference to an object (table, column, parameter, etc.)
    ObjectRef(String),
    /// Reference to a TVP parameter with its disambiguator
    TvpParameter(String, u32),
}

// =============================================================================
// Body Dependency Token Scanner (Phase 20.2.1)
// =============================================================================
// Replaces TOKEN_RE regex with tokenizer-based scanning for body dependency extraction.
// Handles 8 token patterns:
// 1. @param - parameter references
// 2. [a].[b].[c] - three-part bracketed reference (schema.table.column)
// 3. [a].[b] - two-part bracketed reference (schema.table or alias.column)
// 4. alias.[column] - unbracketed alias with bracketed column
// 5. [alias].column - bracketed alias with unbracketed column
// 6. [ident] - single bracketed identifier
// 7. schema.table - unbracketed two-part reference
// 8. ident - unbracketed single identifier

/// Represents a token pattern matched by the body dependency scanner
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BodyDepToken {
    /// @param - parameter reference
    Parameter(String),
    /// [schema].[table].[column] - three-part bracketed
    ThreePartBracketed {
        schema: String,
        table: String,
        column: String,
    },
    /// [first].[second] - two-part bracketed (schema.table or alias.column)
    TwoPartBracketed { first: String, second: String },
    /// alias.[column] - unbracketed alias with bracketed column
    AliasDotBracketedColumn { alias: String, column: String },
    /// [alias].column - bracketed alias with unbracketed column
    BracketedAliasDotColumn { alias: String, column: String },
    /// [ident] - single bracketed identifier
    SingleBracketed(String),
    /// schema.table - unbracketed two-part reference
    TwoPartUnbracketed { first: String, second: String },
    /// ident - single unbracketed identifier
    SingleUnbracketed(String),
}

/// Token-based scanner for body dependency extraction.
/// Replaces TOKEN_RE regex with proper tokenization for handling whitespace, comments,
/// and SQL syntax correctly.
pub(crate) struct BodyDependencyTokenScanner {
    tokens: Vec<sqlparser::tokenizer::TokenWithSpan>,
    pos: usize,
}

impl BodyDependencyTokenScanner {
    /// Create a new scanner for SQL body text
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;
        Some(Self { tokens, pos: 0 })
    }

    /// Scan the body and return all matched tokens in order of appearance
    pub fn scan(&mut self) -> Vec<BodyDepToken> {
        let mut results = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace();
            if self.is_at_end() {
                break;
            }

            // Try to match patterns in order of specificity
            if let Some(token) = self.try_scan_token() {
                results.push(token);
            } else {
                // No pattern matched, advance to next token
                self.advance();
            }
        }

        results
    }

    /// Try to scan a single token pattern at the current position
    pub fn try_scan_token(&mut self) -> Option<BodyDepToken> {
        // Pattern 1: @param - parameter reference
        // MsSqlDialect tokenizes @param as a single Word token with @ prefix
        if self.is_parameter_word() {
            return self.try_scan_parameter();
        }

        // Patterns 2-6: Start with a bracketed identifier [ident]
        if self.is_bracketed_word() {
            return self.try_scan_bracketed_pattern();
        }

        // Patterns 7-8: Unbracketed identifiers (not starting with @)
        if self.is_unbracketed_word() {
            return self.try_scan_unbracketed_pattern();
        }

        None
    }

    /// Try to scan a parameter reference: @param
    /// MsSqlDialect tokenizes @param as a single Word with "@param" as value
    fn try_scan_parameter(&mut self) -> Option<BodyDepToken> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                if w.quote_style.is_none() && w.value.starts_with('@') {
                    // Extract parameter name without @ prefix
                    let param_name = w.value[1..].to_string();
                    self.advance();
                    return Some(BodyDepToken::Parameter(param_name));
                }
            }
        }
        None
    }

    /// Check if current token is a parameter word (starts with @)
    fn is_parameter_word(&self) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.quote_style.is_none() && w.value.starts_with('@'))
        } else {
            false
        }
    }

    /// Try to scan patterns starting with a bracketed identifier
    fn try_scan_bracketed_pattern(&mut self) -> Option<BodyDepToken> {
        let first_ident = self.parse_bracketed_identifier()?;
        self.skip_whitespace();

        // Check for dot separator
        if self.check_token(&Token::Period) {
            self.advance(); // consume .
            self.skip_whitespace();

            // Could be: [a].[b], [a].[b].[c], or [alias].column
            if self.is_bracketed_word() {
                // [a].[b] or [a].[b].[c]
                let second_ident = self.parse_bracketed_identifier()?;
                self.skip_whitespace();

                // Check for third part
                if self.check_token(&Token::Period) {
                    self.advance(); // consume .
                    self.skip_whitespace();

                    if self.is_bracketed_word() {
                        // [a].[b].[c] - three-part bracketed
                        let third_ident = self.parse_bracketed_identifier()?;
                        return Some(BodyDepToken::ThreePartBracketed {
                            schema: first_ident,
                            table: second_ident,
                            column: third_ident,
                        });
                    }
                }

                // [a].[b] - two-part bracketed
                return Some(BodyDepToken::TwoPartBracketed {
                    first: first_ident,
                    second: second_ident,
                });
            } else if self.is_unbracketed_word() {
                // [alias].column - bracketed alias with unbracketed column
                let column = self.parse_unbracketed_identifier()?;
                return Some(BodyDepToken::BracketedAliasDotColumn {
                    alias: first_ident,
                    column,
                });
            }
        }

        // Just [ident] - single bracketed identifier
        Some(BodyDepToken::SingleBracketed(first_ident))
    }

    /// Try to scan patterns starting with an unbracketed identifier
    fn try_scan_unbracketed_pattern(&mut self) -> Option<BodyDepToken> {
        // Check word boundary - we need to make sure we're not continuing from another token
        // This is handled by checking the previous token isn't a word character

        let first_ident = self.parse_unbracketed_identifier()?;
        self.skip_whitespace();

        // Check for dot separator
        if self.check_token(&Token::Period) {
            self.advance(); // consume .
            self.skip_whitespace();

            if self.is_bracketed_word() {
                // alias.[column] - unbracketed alias with bracketed column
                let column = self.parse_bracketed_identifier()?;
                return Some(BodyDepToken::AliasDotBracketedColumn {
                    alias: first_ident,
                    column,
                });
            } else if self.is_unbracketed_word() {
                // schema.table - unbracketed two-part
                let second_ident = self.parse_unbracketed_identifier()?;
                return Some(BodyDepToken::TwoPartUnbracketed {
                    first: first_ident,
                    second: second_ident,
                });
            }
        }

        // Just ident - single unbracketed identifier
        Some(BodyDepToken::SingleUnbracketed(first_ident))
    }

    /// Parse a bracketed identifier and return the inner value
    fn parse_bracketed_identifier(&mut self) -> Option<String> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                // Check if it's actually bracketed (quote_style shows the quote type)
                if w.quote_style.is_some() {
                    let value = w.value.clone();
                    self.advance();
                    return Some(value);
                }
            }
        }
        None
    }

    /// Parse an unbracketed identifier
    fn parse_unbracketed_identifier(&mut self) -> Option<String> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                // Check if it's unbracketed (no quote_style)
                if w.quote_style.is_none() {
                    let value = w.value.clone();
                    self.advance();
                    return Some(value);
                }
            }
        }
        None
    }

    /// Check if current token is a bracketed word (identifier with quote_style)
    fn is_bracketed_word(&self) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.quote_style.is_some())
        } else {
            false
        }
    }

    /// Check if current token is an unbracketed word (identifier without quote_style, not starting with @)
    fn is_unbracketed_word(&self) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.quote_style.is_none() && !w.value.starts_with('@'))
        } else {
            false
        }
    }

    /// Skip whitespace tokens
    pub fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                if matches!(&token.token, Token::Whitespace(_)) {
                    self.advance();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    /// Check if at end of tokens
    pub fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Get current token without consuming
    fn current_token(&self) -> Option<&sqlparser::tokenizer::TokenWithSpan> {
        self.tokens.get(self.pos)
    }

    /// Advance to next token
    fn advance(&mut self) {
        if !self.is_at_end() {
            self.pos += 1;
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

// =============================================================================
// Qualified Name Tokenization (Phase 20.2.8)
// =============================================================================
// Token-based parsing for qualified SQL names like [schema].[table].[column].
// Replaces split('.') string operations with proper tokenization that handles
// whitespace, comments, and various bracket/quote styles correctly.

/// Represents a parsed qualified name with 1-3 parts.
/// Used for schema.table or schema.table.column references.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QualifiedName {
    /// The first part (schema for 2+ parts, or name for single part)
    pub first: String,
    /// The second part (table name for 2+ parts)
    pub second: Option<String>,
    /// The third part (column name for 3 parts)
    pub third: Option<String>,
}

impl QualifiedName {
    /// Creates a single-part name
    pub fn single(name: String) -> Self {
        Self {
            first: name,
            second: None,
            third: None,
        }
    }

    /// Creates a two-part name (schema.table)
    pub fn two_part(first: String, second: String) -> Self {
        Self {
            first,
            second: Some(second),
            third: None,
        }
    }

    /// Creates a three-part name (schema.table.column)
    pub fn three_part(first: String, second: String, third: String) -> Self {
        Self {
            first,
            second: Some(second),
            third: Some(third),
        }
    }

    /// Returns the number of parts in this qualified name
    pub fn part_count(&self) -> usize {
        if self.third.is_some() {
            3
        } else if self.second.is_some() {
            2
        } else {
            1
        }
    }

    /// Returns the last part of the name (column for 3-part, table for 2-part, name for 1-part)
    pub fn last_part(&self) -> &str {
        self.third
            .as_deref()
            .or(self.second.as_deref())
            .unwrap_or(&self.first)
    }

    /// Returns the schema and table as a tuple if this is a 2+ part name
    pub fn schema_and_table(&self) -> Option<(&str, &str)> {
        self.second
            .as_ref()
            .map(|table| (self.first.as_str(), table.as_str()))
    }

    /// Formats as a bracketed reference: [first].[second] or [first].[second].[third]
    #[cfg(test)]
    pub fn to_bracketed(&self) -> String {
        match (&self.second, &self.third) {
            (Some(second), Some(third)) => {
                format!("[{}].[{}].[{}]", self.first, second, third)
            }
            (Some(second), None) => format!("[{}].[{}]", self.first, second),
            (None, _) => format!("[{}]", self.first),
        }
    }
}

/// Parse a qualified name from a string using tokenization.
///
/// Handles all combinations of bracketed and unbracketed identifiers:
/// - `[schema].[table].[column]` -> 3-part
/// - `[schema].[table]` -> 2-part
/// - `schema.table` -> 2-part (unbracketed)
/// - `alias.[column]` -> 2-part (mixed)
/// - `[alias].column` -> 2-part (mixed)
/// - `[name]` or `name` -> 1-part
///
/// This replaces split('.') operations with proper tokenization that handles
/// whitespace and SQL syntax correctly.
pub(crate) fn parse_qualified_name_tokenized(sql: &str) -> Option<QualifiedName> {
    let mut scanner = BodyDependencyTokenScanner::new(sql)?;
    scanner.skip_whitespace();

    if scanner.is_at_end() {
        return None;
    }

    // Try to parse a token pattern - this will give us the qualified name structure
    let token = scanner.try_scan_token()?;

    // Convert BodyDepToken to QualifiedName
    match token {
        BodyDepToken::ThreePartBracketed {
            schema,
            table,
            column,
        } => Some(QualifiedName::three_part(schema, table, column)),

        BodyDepToken::TwoPartBracketed { first, second } => {
            Some(QualifiedName::two_part(first, second))
        }

        BodyDepToken::AliasDotBracketedColumn { alias, column } => {
            Some(QualifiedName::two_part(alias, column))
        }

        BodyDepToken::BracketedAliasDotColumn { alias, column } => {
            Some(QualifiedName::two_part(alias, column))
        }

        BodyDepToken::TwoPartUnbracketed { first, second } => {
            Some(QualifiedName::two_part(first, second))
        }

        BodyDepToken::SingleBracketed(name) => Some(QualifiedName::single(name)),

        BodyDepToken::SingleUnbracketed(name) => Some(QualifiedName::single(name)),

        BodyDepToken::Parameter(_) => None, // Parameters are not qualified names
    }
}

// =============================================================================
// Bracketed Identifier Extraction (Phase 20.2.4)
// =============================================================================
// Token-based extraction of single bracketed identifiers from SQL expressions.
// Used for CHECK constraints, computed columns, and filter predicates.

/// Represents a bracketed identifier with its position in the source text.
/// Used for extracting `[ColumnName]` patterns from SQL expressions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BracketedIdentWithPos {
    /// The identifier name without brackets
    pub name: String,
    /// The byte position where the identifier starts (position of the '[')
    pub position: usize,
}

/// Extract single bracketed identifiers from SQL text using tokenization.
///
/// This function uses the sqlparser tokenizer to find all `[identifier]` patterns
/// in the SQL text, returning them with their positions. This replaces the
/// `BRACKETED_IDENT_RE` regex for more robust parsing.
///
/// Only single bracketed identifiers are returned; multi-part references like
/// `[schema].[table]` are not included as individual components.
pub(crate) fn extract_bracketed_identifiers_tokenized(sql: &str) -> Vec<BracketedIdentWithPos> {
    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, sql).tokenize_with_location() else {
        return Vec::new();
    };

    // Build a line/column to byte offset map for position calculation
    // This allows us to convert token Location (line, column) to byte offset
    let line_offsets = compute_line_offsets(sql);

    let mut results = Vec::new();
    let mut i = 0;
    let len = tokens.len();

    while i < len {
        let token = &tokens[i];

        // Look for bracketed Word tokens (quote_style is Some('[') for bracketed identifiers)
        if let Token::Word(w) = &token.token {
            if w.quote_style == Some('[') {
                // Check if this is a standalone bracketed identifier
                // (not followed by a dot, which would make it part of a multi-part name)
                let is_standalone = {
                    // Look ahead for Period token (skip whitespace)
                    let mut j = i + 1;
                    while j < len {
                        match &tokens[j].token {
                            Token::Whitespace(_) => j += 1,
                            Token::Period => break,
                            _ => break,
                        }
                    }
                    // If followed by period, it's not standalone
                    j >= len || !matches!(&tokens[j].token, Token::Period)
                };

                // Also check if this is preceded by a dot (meaning it's the second/third part)
                let not_preceded_by_dot = {
                    if i == 0 {
                        true
                    } else {
                        // Look back for Period token (skip whitespace)
                        let mut j = i as isize - 1;
                        while j >= 0 {
                            match &tokens[j as usize].token {
                                Token::Whitespace(_) => j -= 1,
                                Token::Period => break,
                                _ => break,
                            }
                        }
                        j < 0 || !matches!(&tokens[j as usize].token, Token::Period)
                    }
                };

                if is_standalone && not_preceded_by_dot {
                    // Convert (line, column) to byte offset
                    let location = &token.span.start;
                    let byte_pos =
                        location_to_byte_offset(&line_offsets, location.line, location.column);
                    results.push(BracketedIdentWithPos {
                        name: w.value.clone(),
                        position: byte_pos,
                    });
                }
            }
        }

        i += 1;
    }

    results
}

// =============================================================================
// Table Reference Extraction (Phase 20.4.3)
// =============================================================================
// Token-based extraction of table references from SQL body text.
// Replaces BRACKETED_TABLE_RE and UNBRACKETED_TABLE_RE regex patterns.

/// Extract all two-part table references from SQL body text using tokenization.
///
/// This function scans the body and extracts references in both formats:
/// - Bracketed: `[schema].[table]`
/// - Unbracketed: `schema.table`
///
/// It filters out:
/// - Parameter references (starting with @)
/// - SQL keywords as schema names (FROM.something)
/// - Table alias references (alias.column)
///
/// Returns a deduplicated list of table references in `[schema].[table]` format.
///
/// This replaces the BRACKETED_TABLE_RE and UNBRACKETED_TABLE_RE regex patterns
/// for more robust parsing that handles whitespace, comments, and edge cases correctly.
pub(crate) fn extract_table_refs_tokenized(
    body: &str,
    table_aliases: &HashMap<String, String>,
) -> Vec<String> {
    let mut table_refs: Vec<String> = Vec::with_capacity(5);

    let Some(mut scanner) = BodyDependencyTokenScanner::new(body) else {
        return table_refs;
    };

    for token in scanner.scan() {
        match token {
            BodyDepToken::TwoPartBracketed { first, second } => {
                // [schema].[table] pattern - equivalent to BRACKETED_TABLE_RE
                // Skip if either part starts with @ (parameter)
                if !first.starts_with('@') && !second.starts_with('@') {
                    let table_ref = format!("[{}].[{}]", first, second);
                    if !table_refs.contains(&table_ref) {
                        table_refs.push(table_ref);
                    }
                }
            }
            BodyDepToken::TwoPartUnbracketed { first, second } => {
                // schema.table pattern - equivalent to UNBRACKETED_TABLE_RE
                // Skip if first part is a SQL keyword (like FROM.something)
                if is_sql_keyword(&first.to_uppercase()) {
                    continue;
                }
                // Skip if first part is a table alias (alias.column reference)
                if table_aliases.contains_key(&first.to_lowercase()) {
                    continue;
                }
                let table_ref = format!("[{}].[{}]", first, second);
                if !table_refs.contains(&table_ref) {
                    table_refs.push(table_ref);
                }
            }
            BodyDepToken::AliasDotBracketedColumn { alias, column } => {
                // alias.[column] pattern - could be schema.[table] if alias is not a known alias
                if is_sql_keyword(&alias.to_uppercase()) {
                    continue;
                }
                // If not a known alias, treat as potential schema.table reference
                if !table_aliases.contains_key(&alias.to_lowercase()) {
                    let table_ref = format!("[{}].[{}]", alias, column);
                    if !table_refs.contains(&table_ref) {
                        table_refs.push(table_ref);
                    }
                }
            }
            BodyDepToken::BracketedAliasDotColumn { alias, column } => {
                // [alias].column pattern - could be [schema].table if alias is not a known alias
                if !alias.starts_with('@') {
                    if is_sql_keyword(&alias.to_uppercase()) {
                        continue;
                    }
                    // If not a known alias, treat as potential schema.table reference
                    if !table_aliases.contains_key(&alias.to_lowercase()) {
                        let table_ref = format!("[{}].[{}]", alias, column);
                        if !table_refs.contains(&table_ref) {
                            table_refs.push(table_ref);
                        }
                    }
                }
            }
            BodyDepToken::ThreePartBracketed { schema, table, .. } => {
                // [schema].[table].[column] - extract the table part
                if !schema.starts_with('@') && !table.starts_with('@') {
                    let table_ref = format!("[{}].[{}]", schema, table);
                    if !table_refs.contains(&table_ref) {
                        table_refs.push(table_ref);
                    }
                }
            }
            // Skip single identifiers and parameters - they're not table references
            BodyDepToken::SingleBracketed(_)
            | BodyDepToken::SingleUnbracketed(_)
            | BodyDepToken::Parameter(_) => {}
        }
    }

    table_refs
}

// =============================================================================
// Body Dependency Extraction
// =============================================================================

/// Extract body dependencies from a procedure/function body
/// This extracts dependencies in order of appearance:
/// 1. Built-in types from DECLARE statements
/// 2. Table references, columns, and parameters in the order they appear
pub(crate) fn extract_body_dependencies(
    body: &str,
    full_name: &str,
    params: &[String],
) -> Vec<BodyDependency> {
    // Estimate ~10 dependencies typical for a procedure/function body
    let mut deps = Vec::with_capacity(10);
    // Track seen items for deduplication:
    // - DotNet deduplicates built-in types
    // - DotNet deduplicates table references (2-part refs like [schema].[table])
    // - DotNet deduplicates parameter references
    // - DotNet deduplicates DIRECT column references (columns matched without alias resolution)
    // - DotNet does NOT deduplicate ALIAS-RESOLVED column references (alias.column patterns)
    let mut seen_types: HashSet<String> = HashSet::with_capacity(5);
    let mut seen_tables: HashSet<String> = HashSet::with_capacity(10);
    let mut seen_params: HashSet<String> = HashSet::with_capacity(5);
    let mut seen_direct_columns: HashSet<String> = HashSet::with_capacity(10);

    // Extract DECLARE type dependencies first (for scalar functions)
    // Uses token-based extraction (Phase 20.3.1) for proper whitespace handling
    for type_name in extract_declare_types_tokenized(body) {
        let type_ref = format!("[{}]", type_name);
        // Only deduplicate built-in types
        if !seen_types.contains(&type_ref) {
            seen_types.insert(type_ref.clone());
            deps.push(BodyDependency::BuiltInType(type_ref));
        }
    }

    // Strip SQL comments from body to prevent words in comments being treated as references
    let body_no_comments = strip_sql_comments_for_body_deps(body);
    let body = body_no_comments.as_str();

    // Phase 18: Extract table aliases for resolution
    // Maps alias (lowercase) -> table reference (e.g., "a" -> "[dbo].[Account]")
    let mut table_aliases: HashMap<String, String> = HashMap::new();
    // Track subquery/derived table aliases - these should be skipped, not resolved
    let mut subquery_aliases: HashSet<String> = HashSet::new();
    // Track column aliases (AS identifier) - these should not be treated as column references
    let mut column_aliases: HashSet<String> = HashSet::new();

    // Extract aliases from FROM/JOIN clauses with proper alias tracking
    extract_table_aliases_for_body_deps(body, &mut table_aliases, &mut subquery_aliases);

    // Extract column aliases (SELECT expr AS alias patterns)
    extract_column_aliases_for_body_deps(body, &mut column_aliases);

    // First pass: collect all table references using token-based extraction
    // Phase 20.4.3: Replaced BRACKETED_TABLE_RE and UNBRACKETED_TABLE_RE with tokenization
    // This handles whitespace (tabs, multiple spaces, newlines) correctly and is more robust
    let table_refs = extract_table_refs_tokenized(body, &table_aliases);

    // Scan body sequentially for all references in order of appearance using token-based scanner
    // Note: DotNet has a complex ordering that depends on SQL clause structure (FROM first, etc.)
    // We process in textual order which may differ from DotNet's order but contains the same refs
    // Phase 20.2.1: Replaced TOKEN_RE regex with BodyDependencyTokenScanner for robust whitespace handling

    if let Some(mut scanner) = BodyDependencyTokenScanner::new(body) {
        for token in scanner.scan() {
            match token {
                BodyDepToken::Parameter(param_name) => {
                    // Pattern 1: Parameter reference: @param
                    // Check if this is a declared parameter (not a local variable)
                    // Note: params contains parameter names WITHOUT @ prefix (Phase 20.1.3)
                    if params.iter().any(|p| p.eq_ignore_ascii_case(&param_name)) {
                        let param_ref = format!("{}.[@{}]", full_name, param_name);
                        // DotNet deduplicates parameter references
                        if !seen_params.contains(&param_ref) {
                            seen_params.insert(param_ref.clone());
                            deps.push(BodyDependency::ObjectRef(param_ref));
                        }
                    }
                }
                BodyDepToken::ThreePartBracketed {
                    schema,
                    table,
                    column,
                } => {
                    // Pattern 2: Three-part bracketed reference: [schema].[table].[column]
                    if !schema.starts_with('@') && !table.starts_with('@') {
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        let table_ref = format!("[{}].[{}]", schema, table);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }

                        // Direct three-part column refs ARE deduplicated by DotNet
                        let col_ref = format!("[{}].[{}].[{}]", schema, table, column);
                        if !seen_direct_columns.contains(&col_ref) {
                            seen_direct_columns.insert(col_ref.clone());
                            deps.push(BodyDependency::ObjectRef(col_ref));
                        }
                    }
                }
                BodyDepToken::TwoPartBracketed { first, second } => {
                    // Pattern 3: Two-part bracketed reference: [schema].[table] or [alias].[column]
                    if first.starts_with('@') || second.starts_with('@') {
                        continue;
                    }

                    let first_lower = first.to_lowercase();

                    // Check if first_part is a subquery/derived table alias - skip entirely
                    if subquery_aliases.contains(&first_lower) {
                        continue;
                    }

                    // Check if first_part is a table alias that should be resolved
                    if let Some(resolved_table) = table_aliases.get(&first_lower) {
                        // This is alias.column - resolve to [schema].[table].[column]
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        if !seen_tables.contains(resolved_table) {
                            seen_tables.insert(resolved_table.clone());
                            deps.push(BodyDependency::ObjectRef(resolved_table.clone()));
                        }

                        // Then emit the column reference (DotNet does NOT deduplicate columns)
                        let col_ref = format!("{}.[{}]", resolved_table, second);
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    } else {
                        // Not an alias - treat as [schema].[table] (DotNet deduplicates tables)
                        let table_ref = format!("[{}].[{}]", first, second);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }
                    }
                }
                BodyDepToken::AliasDotBracketedColumn { alias, column } => {
                    // Pattern 4: Unbracketed alias with bracketed column: alias.[column]
                    let alias_lower = alias.to_lowercase();

                    // Check if alias is a subquery/derived table alias - skip entirely
                    if subquery_aliases.contains(&alias_lower) {
                        continue;
                    }

                    // Check if alias is a table alias that should be resolved
                    if let Some(resolved_table) = table_aliases.get(&alias_lower) {
                        // This is alias.[column] - resolve to [schema].[table].[column]
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        if !seen_tables.contains(resolved_table) {
                            seen_tables.insert(resolved_table.clone());
                            deps.push(BodyDependency::ObjectRef(resolved_table.clone()));
                        }

                        // Then emit the column reference (DotNet does NOT deduplicate columns)
                        let col_ref = format!("{}.[{}]", resolved_table, column);
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    } else {
                        // Not a known alias - treat as [alias].[column] (might be schema.table)
                        let table_ref = format!("[{}].[{}]", alias, column);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }
                    }
                }
                BodyDepToken::BracketedAliasDotColumn { alias, column } => {
                    // Pattern 5: Bracketed alias with unbracketed column: [alias].column
                    let alias_lower = alias.to_lowercase();

                    // Check if alias is a subquery/derived table alias - skip entirely
                    if subquery_aliases.contains(&alias_lower) {
                        continue;
                    }

                    // Check if alias is a table alias that should be resolved
                    if let Some(resolved_table) = table_aliases.get(&alias_lower) {
                        // This is [alias].column - resolve to [schema].[table].[column]
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        if !seen_tables.contains(resolved_table) {
                            seen_tables.insert(resolved_table.clone());
                            deps.push(BodyDependency::ObjectRef(resolved_table.clone()));
                        }

                        // Then emit the column reference (DotNet does NOT deduplicate columns)
                        let col_ref = format!("{}.[{}]", resolved_table, column);
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    } else {
                        // Not a known alias - treat as [alias].[column] (might be schema.table)
                        let table_ref = format!("[{}].[{}]", alias, column);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }
                    }
                }
                BodyDepToken::SingleBracketed(ident) => {
                    // Pattern 6: Single bracketed identifier: [ident]
                    let ident_lower = ident.to_lowercase();
                    let upper_ident = ident.to_uppercase();

                    // Skip SQL keywords (but allow column names that happen to match type names)
                    if is_sql_keyword_not_column(&upper_ident) {
                        continue;
                    }

                    // Skip if this is a known table alias, subquery alias, or column alias
                    if table_aliases.contains_key(&ident_lower)
                        || subquery_aliases.contains(&ident_lower)
                        || column_aliases.contains(&ident_lower)
                    {
                        continue;
                    }

                    // Skip if this is part of a table reference (schema or table name)
                    let is_table_or_schema = table_refs.iter().any(|t| {
                        t.ends_with(&format!("].[{}]", ident))
                            || t.starts_with(&format!("[{}].", ident))
                    });

                    // If not a table/schema, treat as unqualified column -> resolve against first table
                    if !is_table_or_schema {
                        if let Some(first_table) = table_refs.first() {
                            // First emit the table reference if not seen (DotNet deduplicates tables)
                            if !seen_tables.contains(first_table) {
                                seen_tables.insert(first_table.clone());
                                deps.push(BodyDependency::ObjectRef(first_table.clone()));
                            }

                            // Direct column refs (single bracketed) ARE deduplicated by DotNet
                            let col_ref = format!("{}.[{}]", first_table, ident);
                            if !seen_direct_columns.contains(&col_ref) {
                                seen_direct_columns.insert(col_ref.clone());
                                deps.push(BodyDependency::ObjectRef(col_ref));
                            }
                        }
                    }
                }
                BodyDepToken::TwoPartUnbracketed { first, second } => {
                    // Pattern 7: Unbracketed two-part reference: schema.table or alias.column
                    let first_lower = first.to_lowercase();
                    let first_upper = first.to_uppercase();

                    // Skip if first part is a keyword
                    if is_sql_keyword(&first_upper) {
                        continue;
                    }

                    // Check if first_part is a subquery/derived table alias - skip entirely
                    if subquery_aliases.contains(&first_lower) {
                        continue;
                    }

                    // Check if first_part is a table alias that should be resolved
                    if let Some(resolved_table) = table_aliases.get(&first_lower) {
                        // This is alias.column - resolve to [schema].[table].[column]
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        if !seen_tables.contains(resolved_table) {
                            seen_tables.insert(resolved_table.clone());
                            deps.push(BodyDependency::ObjectRef(resolved_table.clone()));
                        }

                        // Then emit the column reference (DotNet does NOT deduplicate columns)
                        let col_ref = format!("{}.[{}]", resolved_table, second);
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    } else {
                        // Not an alias - treat as schema.table (DotNet deduplicates tables)
                        let table_ref = format!("[{}].[{}]", first, second);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }
                    }
                }
                BodyDepToken::SingleUnbracketed(ident) => {
                    // Pattern 8: Unbracketed single identifier: might be a column name
                    let ident_lower = ident.to_lowercase();
                    let upper_ident = ident.to_uppercase();

                    // Skip SQL keywords
                    if is_sql_keyword_not_column(&upper_ident) {
                        continue;
                    }

                    // Skip if this is a known table alias, subquery alias, or column alias
                    if table_aliases.contains_key(&ident_lower)
                        || subquery_aliases.contains(&ident_lower)
                        || column_aliases.contains(&ident_lower)
                    {
                        continue;
                    }

                    // Skip if this is part of a table reference (schema or table name)
                    let is_table_or_schema = table_refs.iter().any(|t| {
                        // Check case-insensitive match for unbracketed identifiers
                        let t_lower = t.to_lowercase();
                        t_lower.ends_with(&format!("].[{}]", ident_lower))
                            || t_lower.starts_with(&format!("[{}].", ident_lower))
                    });

                    // If not a table/schema, treat as unqualified column -> resolve against first table
                    if !is_table_or_schema {
                        if let Some(first_table) = table_refs.first() {
                            // First emit the table reference if not seen (DotNet deduplicates tables)
                            if !seen_tables.contains(first_table) {
                                seen_tables.insert(first_table.clone());
                                deps.push(BodyDependency::ObjectRef(first_table.clone()));
                            }

                            // Direct column refs (single unbracketed) ARE deduplicated by DotNet
                            let col_ref = format!("{}.[{}]", first_table, ident);
                            if !seen_direct_columns.contains(&col_ref) {
                                seen_direct_columns.insert(col_ref.clone());
                                deps.push(BodyDependency::ObjectRef(col_ref));
                            }
                        }
                    }
                }
            }
        }
    }

    deps
}

/// Extract table aliases from FROM/JOIN clauses for body dependency resolution.
/// Populates two maps:
/// - table_aliases: maps alias (lowercase) -> full table reference (e.g., "a" -> "[dbo].[Account]")
/// - subquery_aliases: set of aliases that refer to subqueries/derived tables (should be skipped)
///
/// Handles:
/// - FROM [schema].[table] alias
/// - FROM [schema].[table] AS alias
/// - JOIN [schema].[table] alias ON ...
/// - LEFT JOIN (...) AS SubqueryAlias ON ...
/// - CROSS APPLY (...) AS ApplyAlias
///
/// This implementation uses sqlparser-rs tokenizer instead of regex for more robust parsing.
pub(crate) fn extract_table_aliases_for_body_deps(
    body: &str,
    table_aliases: &mut HashMap<String, String>,
    subquery_aliases: &mut HashSet<String>,
) {
    let mut parser = match TableAliasTokenParser::new(body) {
        Some(p) => p,
        None => return,
    };
    parser.extract_all_aliases(table_aliases, subquery_aliases);
}

/// Token-based parser for extracting table aliases from SQL body text.
/// Replaces 6 regex patterns with a single tokenizer-based implementation.
pub(crate) struct TableAliasTokenParser {
    tokens: Vec<sqlparser::tokenizer::TokenWithSpan>,
    pos: usize,
    default_schema: String,
}

impl TableAliasTokenParser {
    /// Create a new parser for SQL body text
    pub fn new(sql: &str) -> Option<Self> {
        Self::with_default_schema(sql, "dbo")
    }

    /// Create a new parser with a custom default schema
    pub fn with_default_schema(sql: &str, default_schema: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;
        Some(Self {
            tokens,
            pos: 0,
            default_schema: default_schema.to_string(),
        })
    }

    /// Extract all aliases from the SQL body
    pub fn extract_all_aliases(
        &mut self,
        table_aliases: &mut HashMap<String, String>,
        subquery_aliases: &mut HashSet<String>,
    ) {
        // First pass: extract CTE aliases from WITH clauses
        self.extract_cte_aliases(subquery_aliases);

        // Reset position for second pass
        self.pos = 0;

        // Second pass: extract table aliases and subquery aliases from FROM/JOIN/APPLY
        // We scan the entire token stream without skipping nested parens for table aliases,
        // because table aliases inside subqueries are still valid and need to be captured.
        while !self.is_at_end() {
            self.skip_whitespace();

            // Look for FROM, JOIN variants, or APPLY keywords
            if self.check_keyword(Keyword::FROM) {
                self.advance();
                self.extract_table_reference_after_from_join(table_aliases, subquery_aliases);
            } else if self.is_join_keyword() {
                self.skip_join_keywords();
                self.extract_table_reference_after_from_join(table_aliases, subquery_aliases);
            } else if self.check_word_ci("CROSS") || self.check_word_ci("OUTER") {
                // Check for APPLY - just skip past the APPLY keyword and let the loop
                // continue to find FROM/JOIN inside the APPLY subquery
                // The subquery alias will be captured via the ) AS/alias pattern
                let saved_pos = self.pos;
                self.advance();
                self.skip_whitespace();
                if self.check_keyword(Keyword::APPLY) || self.check_word_ci("APPLY") {
                    self.advance();
                    // Don't extract alias here - let the loop continue to scan content
                    // The ) alias pattern will capture the APPLY alias
                } else {
                    // Not an APPLY, restore position and continue
                    self.pos = saved_pos;
                    self.advance();
                }
            } else if self.check_keyword(Keyword::MERGE) || self.check_word_ci("MERGE") {
                // Handle MERGE INTO [table] AS [alias] pattern
                // This extracts the TARGET alias which is the alias for the target table
                self.advance();
                self.skip_whitespace();

                // Skip optional INTO keyword
                if self.check_keyword(Keyword::INTO) || self.check_word_ci("INTO") {
                    self.advance();
                    self.skip_whitespace();
                }

                // Parse the target table name
                if let Some((schema, table_name)) = self.parse_table_name() {
                    self.skip_whitespace();

                    // Check for AS keyword (optional)
                    if self.check_keyword(Keyword::AS) {
                        self.advance();
                        self.skip_whitespace();
                    }

                    // Try to get alias
                    if let Some(alias) = self.try_parse_table_alias() {
                        let alias_lower = alias.to_lowercase();
                        if !Self::is_alias_keyword(&alias_lower)
                            && !table_aliases.contains_key(&alias_lower)
                        {
                            let table_ref = format!("[{}].[{}]", schema, table_name);
                            table_aliases.insert(alias_lower, table_ref);
                        }
                    }
                }
            } else if self.check_word_ci("USING") {
                // Handle USING ... pattern in MERGE statements
                // For USING (subquery) AS alias - the subquery alias will be captured by
                // the RParen handler when it sees ) AS alias pattern.
                // For USING table AS alias - we handle it here as a table alias.
                self.advance();
                self.skip_whitespace();

                // Check if followed by opening paren (subquery)
                if self.check_token(&Token::LParen) {
                    // Don't skip balanced parens - let the main loop continue scanning
                    // the subquery contents. FROM/JOIN inside will be captured.
                    // The ) AS alias pattern will be captured by the RParen handler.
                    self.advance();
                } else {
                    // USING references a table directly (less common but valid)
                    // Parse table name and optional alias
                    if let Some((schema, table_name)) = self.parse_table_name() {
                        self.skip_whitespace();

                        // Check for AS keyword (optional)
                        if self.check_keyword(Keyword::AS) {
                            self.advance();
                            self.skip_whitespace();
                        }

                        // Try to get alias
                        if let Some(alias) = self.try_parse_table_alias() {
                            let alias_lower = alias.to_lowercase();
                            if !Self::is_alias_keyword(&alias_lower)
                                && !table_aliases.contains_key(&alias_lower)
                            {
                                let table_ref = format!("[{}].[{}]", schema, table_name);
                                table_aliases.insert(alias_lower, table_ref);
                            }
                        }
                    }
                }
            } else if self.check_token(&Token::RParen) {
                // After closing paren, check for subquery alias pattern: ) AS alias or ) alias
                self.advance();
                self.skip_whitespace();

                // Check for AS keyword (optional)
                if self.check_keyword(Keyword::AS) {
                    self.advance();
                    self.skip_whitespace();
                }

                // Try to get an alias - but only if it's a valid identifier
                if let Some(alias) = self.try_parse_subquery_alias() {
                    let alias_lower = alias.to_lowercase();
                    if !Self::is_alias_keyword(&alias_lower) {
                        subquery_aliases.insert(alias_lower);
                    }
                }
            } else {
                self.advance();
            }
        }
    }

    /// Extract aliases with table names for view column resolution.
    /// Returns Vec of (alias/table_name, full_table_ref) pairs.
    /// Unlike `extract_all_aliases`, this also includes the table name itself as a lookup key.
    pub fn extract_aliases_with_table_names(&mut self) -> Vec<(String, String)> {
        let mut result = Vec::new();
        let mut seen_tables: HashSet<String> = HashSet::new();

        // First pass: extract CTE aliases into a set (to exclude them from table references)
        let mut cte_names: HashSet<String> = HashSet::new();
        self.extract_cte_aliases(&mut cte_names);

        // Reset position for second pass
        self.pos = 0;

        // Second pass: extract table aliases and table names
        while !self.is_at_end() {
            self.skip_whitespace();

            // Look for FROM, JOIN variants, or APPLY keywords
            if self.check_keyword(Keyword::FROM) {
                self.advance();
                self.extract_table_with_alias(&mut result, &mut seen_tables, &cte_names);
            } else if self.is_join_keyword() {
                self.skip_join_keywords();
                self.extract_table_with_alias(&mut result, &mut seen_tables, &cte_names);
            } else if self.check_word_ci("CROSS") || self.check_word_ci("OUTER") {
                // Check for APPLY
                let saved_pos = self.pos;
                self.advance();
                self.skip_whitespace();
                if self.check_keyword(Keyword::APPLY) || self.check_word_ci("APPLY") {
                    self.advance();
                    // APPLY subquery - don't extract here, continue scanning
                } else {
                    self.pos = saved_pos;
                    self.advance();
                }
            } else {
                self.advance();
            }
        }

        result
    }

    /// Extract table reference and alias after FROM/JOIN, adding both to result.
    fn extract_table_with_alias(
        &mut self,
        result: &mut Vec<(String, String)>,
        seen_tables: &mut HashSet<String>,
        cte_names: &HashSet<String>,
    ) {
        self.skip_whitespace();

        // Check if it's a subquery (starts with paren)
        if self.check_token(&Token::LParen) {
            return;
        }

        // Parse table name (could be qualified or unqualified)
        let (schema, table_name) = match self.parse_table_name() {
            Some(t) => t,
            None => return,
        };

        let table_ref = format!("[{}].[{}]", schema, table_name);

        // Skip if this is a CTE name (not a real table)
        let table_name_lower = table_name.to_lowercase();
        if cte_names.contains(&table_name_lower) {
            return;
        }

        self.skip_whitespace();

        // Check for AS keyword (optional)
        if self.check_keyword(Keyword::AS) {
            self.advance();
            self.skip_whitespace();
        }

        // Check for alias
        if let Some(alias) = self.try_parse_table_alias() {
            let alias_lower = alias.to_lowercase();

            // Skip if alias is a SQL keyword
            if !Self::is_alias_keyword(&alias_lower) {
                result.push((alias, table_ref.clone()));
            }
        }

        // Always add the table name itself as an alias (for unaliased references like Products.Name)
        if !seen_tables.contains(&table_name_lower) {
            seen_tables.insert(table_name_lower);
            result.push((table_name, table_ref));
        }
    }

    /// Extract CTE aliases from WITH clause
    fn extract_cte_aliases(&mut self, subquery_aliases: &mut HashSet<String>) {
        while !self.is_at_end() {
            self.skip_whitespace();

            // Look for WITH keyword (start of CTE)
            if self.check_keyword(Keyword::WITH) {
                self.advance();
                self.skip_whitespace();

                // Skip RECURSIVE if present
                if self.check_word_ci("RECURSIVE") {
                    self.advance();
                    self.skip_whitespace();
                }

                // Parse CTE definitions: name AS (...), name AS (...), ...
                loop {
                    // Get CTE name
                    if let Some(cte_name) = self.parse_identifier() {
                        let cte_name_lower = cte_name.to_lowercase();

                        self.skip_whitespace();

                        // Expect AS keyword
                        if self.check_keyword(Keyword::AS) {
                            self.advance();
                            self.skip_whitespace();

                            // Expect opening paren
                            if self.check_token(&Token::LParen) {
                                // This is a valid CTE - add to subquery aliases
                                if !Self::is_alias_keyword(&cte_name_lower) {
                                    subquery_aliases.insert(cte_name_lower);
                                }

                                // Skip past the balanced parens
                                self.skip_balanced_parens();

                                self.skip_whitespace();

                                // Check for comma (more CTEs) or end of WITH clause
                                if self.check_token(&Token::Comma) {
                                    self.advance();
                                    self.skip_whitespace();
                                    continue; // Parse next CTE
                                }
                            }
                        }
                    }
                    break; // End of CTEs
                }
            } else {
                self.advance();
            }
        }
    }

    /// Extract table reference after FROM or JOIN keyword
    fn extract_table_reference_after_from_join(
        &mut self,
        table_aliases: &mut HashMap<String, String>,
        _subquery_aliases: &mut HashSet<String>,
    ) {
        self.skip_whitespace();

        // Check if it's a subquery (starts with paren)
        if self.check_token(&Token::LParen) {
            // This is a subquery - don't skip it, let the main loop continue scanning
            // The subquery alias will be captured when we hit the closing paren + AS pattern
            return;
        }

        // Parse table name (could be qualified or unqualified)
        let (schema, table_name) = match self.parse_table_name() {
            Some(t) => t,
            None => return,
        };

        self.skip_whitespace();

        // Check for AS keyword (optional)
        if self.check_keyword(Keyword::AS) {
            self.advance();
            self.skip_whitespace();
        }

        // Check for alias - must be an identifier that's not a keyword like ON, WHERE, etc.
        if let Some(alias) = self.try_parse_table_alias() {
            let alias_lower = alias.to_lowercase();

            // Skip if alias is a SQL keyword
            if Self::is_alias_keyword(&alias_lower) {
                return;
            }

            // Don't overwrite if already captured by a more specific pattern
            if table_aliases.contains_key(&alias_lower) {
                return;
            }

            let table_ref = format!("[{}].[{}]", schema, table_name);
            table_aliases.insert(alias_lower, table_ref);
        }
    }

    /// Parse a table name (qualified or unqualified)
    /// Returns (schema, table_name)
    fn parse_table_name(&mut self) -> Option<(String, String)> {
        let first_ident = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for dot (schema.table pattern)
        if self.check_token(&Token::Period) {
            self.advance();
            self.skip_whitespace();

            let second_ident = self.parse_identifier()?;

            // Skip if schema is a SQL keyword (would make this not a valid schema.table)
            if is_sql_keyword(&first_ident.to_uppercase()) {
                return None;
            }

            Some((first_ident, second_ident))
        } else {
            // Unqualified table - use default schema
            // Skip if table name is a SQL keyword
            if is_sql_keyword(&first_ident.to_uppercase()) {
                return None;
            }
            Some((self.default_schema.clone(), first_ident))
        }
    }

    /// Try to parse a table alias (identifier that's not a reserved keyword for clause structure)
    fn try_parse_table_alias(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        // Check if current token is a word that could be an alias
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                let value_upper = w.value.to_uppercase();

                // These keywords indicate end of table reference, not an alias
                if matches!(
                    value_upper.as_str(),
                    "ON" | "WHERE"
                        | "INNER"
                        | "LEFT"
                        | "RIGHT"
                        | "OUTER"
                        | "CROSS"
                        | "FULL"
                        | "JOIN"
                        | "GROUP"
                        | "ORDER"
                        | "HAVING"
                        | "UNION"
                        | "WITH"
                        | "AND"
                        | "OR"
                        | "NOT"
                        | "SET"
                        | "FROM"
                        | "SELECT"
                        | "INTO"
                        | "WHEN"
                        | "THEN"
                        | "ELSE"
                        | "END"
                        | "CASE"
                        | "FOR"
                ) {
                    return None;
                }

                // Also check if it's a sqlparser keyword that indicates clause structure
                if matches!(
                    w.keyword,
                    Keyword::ON
                        | Keyword::WHERE
                        | Keyword::INNER
                        | Keyword::LEFT
                        | Keyword::RIGHT
                        | Keyword::OUTER
                        | Keyword::CROSS
                        | Keyword::FULL
                        | Keyword::JOIN
                        | Keyword::GROUP
                        | Keyword::ORDER
                        | Keyword::HAVING
                        | Keyword::UNION
                        | Keyword::WITH
                        | Keyword::AND
                        | Keyword::OR
                        | Keyword::NOT
                        | Keyword::SET
                        | Keyword::FROM
                        | Keyword::SELECT
                        | Keyword::INTO
                        | Keyword::WHEN
                        | Keyword::THEN
                        | Keyword::ELSE
                        | Keyword::END
                        | Keyword::CASE
                        | Keyword::FOR
                ) {
                    return None;
                }

                // This is a valid alias
                let alias = w.value.clone();
                self.advance();
                return Some(alias);
            }
        }

        None
    }

    /// Try to parse a subquery alias after closing paren
    /// This is similar to try_parse_table_alias but handles the ) AS alias or ) alias pattern
    fn try_parse_subquery_alias(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        // Check if current token is a word that could be a subquery alias
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                let value_upper = w.value.to_uppercase();

                // These keywords indicate something other than a subquery alias
                if matches!(
                    value_upper.as_str(),
                    "ON" | "WHERE"
                        | "INNER"
                        | "LEFT"
                        | "RIGHT"
                        | "OUTER"
                        | "CROSS"
                        | "FULL"
                        | "JOIN"
                        | "GROUP"
                        | "ORDER"
                        | "HAVING"
                        | "UNION"
                        | "WITH"
                        | "AND"
                        | "OR"
                        | "NOT"
                        | "SET"
                        | "FROM"
                        | "SELECT"
                        | "INTO"
                        | "WHEN"
                        | "THEN"
                        | "ELSE"
                        | "END"
                        | "CASE"
                        | "FOR"
                        | "AS" // Don't consume AS here - it's handled by caller
                ) {
                    return None;
                }

                // Also check if it's a sqlparser keyword that indicates clause structure
                if matches!(
                    w.keyword,
                    Keyword::ON
                        | Keyword::WHERE
                        | Keyword::INNER
                        | Keyword::LEFT
                        | Keyword::RIGHT
                        | Keyword::OUTER
                        | Keyword::CROSS
                        | Keyword::FULL
                        | Keyword::JOIN
                        | Keyword::GROUP
                        | Keyword::ORDER
                        | Keyword::HAVING
                        | Keyword::UNION
                        | Keyword::WITH
                        | Keyword::AND
                        | Keyword::OR
                        | Keyword::NOT
                        | Keyword::SET
                        | Keyword::FROM
                        | Keyword::SELECT
                        | Keyword::INTO
                        | Keyword::WHEN
                        | Keyword::THEN
                        | Keyword::ELSE
                        | Keyword::END
                        | Keyword::CASE
                        | Keyword::FOR
                        | Keyword::AS
                ) {
                    return None;
                }

                // This is a valid subquery alias
                let alias = w.value.clone();
                self.advance();
                return Some(alias);
            }
        }

        None
    }

    /// Check if a word is a SQL keyword that should not be treated as an alias
    fn is_alias_keyword(word: &str) -> bool {
        matches!(
            word.to_uppercase().as_str(),
            "ON" | "WHERE"
                | "INNER"
                | "LEFT"
                | "RIGHT"
                | "OUTER"
                | "CROSS"
                | "JOIN"
                | "GROUP"
                | "ORDER"
                | "HAVING"
                | "UNION"
                | "WITH"
                | "AS"
                | "AND"
                | "OR"
                | "NOT"
                | "SET"
                | "FROM"
                | "SELECT"
                | "INTO"
        )
    }

    /// Check if current position is at a JOIN keyword (INNER, LEFT, RIGHT, FULL, CROSS, JOIN)
    fn is_join_keyword(&self) -> bool {
        self.check_keyword(Keyword::INNER)
            || self.check_keyword(Keyword::LEFT)
            || self.check_keyword(Keyword::RIGHT)
            || self.check_keyword(Keyword::FULL)
            || self.check_keyword(Keyword::JOIN)
    }

    /// Skip past JOIN keyword variants (INNER JOIN, LEFT OUTER JOIN, etc.)
    fn skip_join_keywords(&mut self) {
        // Skip INNER/LEFT/RIGHT/FULL/CROSS
        if self.check_keyword(Keyword::INNER)
            || self.check_keyword(Keyword::LEFT)
            || self.check_keyword(Keyword::RIGHT)
            || self.check_keyword(Keyword::FULL)
            || self.check_keyword(Keyword::CROSS)
        {
            self.advance();
            self.skip_whitespace();
        }

        // Skip OUTER (for LEFT OUTER JOIN, etc.)
        if self.check_keyword(Keyword::OUTER) {
            self.advance();
            self.skip_whitespace();
        }

        // Skip JOIN
        if self.check_keyword(Keyword::JOIN) {
            self.advance();
            self.skip_whitespace();
        }
    }

    /// Skip balanced parentheses
    fn skip_balanced_parens(&mut self) {
        if !self.check_token(&Token::LParen) {
            return;
        }

        let mut depth = 0;
        while !self.is_at_end() {
            if self.check_token(&Token::LParen) {
                depth += 1;
            } else if self.check_token(&Token::RParen) {
                depth -= 1;
                if depth == 0 {
                    self.advance();
                    return;
                }
            }
            self.advance();
        }
    }

    /// Parse an identifier (bracketed or unbracketed)
    fn parse_identifier(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        let token = self.current_token()?;
        if let Token::Word(w) = &token.token {
            let name = w.value.clone();
            self.advance();
            Some(name)
        } else {
            None
        }
    }

    /// Skip whitespace tokens
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                if matches!(&token.token, Token::Whitespace(_)) {
                    self.advance();
                } else {
                    break;
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
    fn current_token(&self) -> Option<&sqlparser::tokenizer::TokenWithSpan> {
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

// =============================================================================
// Helper Functions
// =============================================================================

/// Compute byte offsets for each line in the source text.
/// Returns a vector where index i contains the byte offset where line (i+1) starts.
pub(crate) fn compute_line_offsets(sql: &str) -> Vec<usize> {
    let mut offsets = vec![0]; // Line 1 starts at offset 0
    for (i, ch) in sql.char_indices() {
        if ch == '\n' {
            // Next line starts after this newline
            offsets.push(i + 1);
        }
    }
    offsets
}

/// Convert a (1-based line, 1-based column) Location to a byte offset.
pub(crate) fn location_to_byte_offset(line_offsets: &[usize], line: u64, column: u64) -> usize {
    if line == 0 || line as usize > line_offsets.len() {
        return 0;
    }
    let line_start = line_offsets[(line - 1) as usize];
    // Column is 1-based, so subtract 1 to get offset within line
    line_start + (column.saturating_sub(1) as usize)
}

/// Strip SQL comments from body text for dependency extraction.
/// Removes both line comments (-- ...) and block comments (/* ... */).
/// This prevents words in comments from being treated as column/table references.
fn strip_sql_comments_for_body_deps(body: &str) -> String {
    let mut result = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    let mut in_string = false;
    let mut string_delimiter = ' ';

    while let Some(c) = chars.next() {
        // Handle string literals - don't strip comments inside strings
        if (c == '\'' || c == '"') && !in_string {
            in_string = true;
            string_delimiter = c;
            result.push(c);
            continue;
        }
        if c == string_delimiter && in_string {
            in_string = false;
            result.push(c);
            continue;
        }
        if in_string {
            result.push(c);
            continue;
        }

        // Check for line comment: --
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next(); // consume second -
                          // Skip until end of line
            while let Some(&ch) = chars.peek() {
                chars.next();
                if ch == '\n' {
                    result.push('\n'); // preserve line structure
                    break;
                }
            }
            continue;
        }

        // Check for block comment: /* ... */
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume *
                          // Skip until */
            while let Some(ch) = chars.next() {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next(); // consume /
                    result.push(' '); // replace comment with space to preserve word boundaries
                    break;
                }
            }
            continue;
        }

        result.push(c);
    }

    result
}

/// Extract column aliases from SELECT expressions (expr AS alias patterns).
/// These are output column names that should not be treated as column references.
fn extract_column_aliases_for_body_deps(body: &str, column_aliases: &mut HashSet<String>) {
    // Use tokenizer-based extraction (replaces COLUMN_ALIAS_RE regex)
    for alias in extract_column_aliases_tokenized(body) {
        column_aliases.insert(alias);
    }
}

/// Extract column aliases from SQL text using tokenization.
///
/// This function scans SQL and extracts identifiers that follow the AS keyword.
/// Pattern: `expr AS alias` or `expr AS [alias]`
///
/// Used in `extract_column_aliases_for_body_deps()` to find output column names
/// that should not be treated as column references.
///
/// # Arguments
/// * `sql` - SQL text to scan (e.g., SELECT clause with aliases)
///
/// # Returns
/// A vector of alias names (without brackets, lowercase) in order of appearance.
pub(crate) fn extract_column_aliases_tokenized(sql: &str) -> Vec<String> {
    let mut results = Vec::new();

    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, sql).tokenize_with_location() else {
        return results;
    };

    // SQL keywords that should not be treated as aliases
    let alias_keywords = [
        "ON", "WHERE", "INNER", "LEFT", "RIGHT", "OUTER", "CROSS", "JOIN", "GROUP", "ORDER",
        "HAVING", "UNION", "WITH", "AND", "OR", "NOT", "SET", "FROM", "SELECT", "INTO", "BEGIN",
        "END", "NULL", "INT", "VARCHAR", "NVARCHAR", "DATETIME", "BIT", "DECIMAL",
    ];

    let mut i = 0;
    while i < tokens.len() {
        // Skip whitespace
        while i < tokens.len() {
            if matches!(&tokens[i].token, Token::Whitespace(_)) {
                i += 1;
            } else {
                break;
            }
        }
        if i >= tokens.len() {
            break;
        }

        // Look for the AS keyword
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("AS") {
                i += 1;

                // Skip whitespace after AS
                while i < tokens.len() {
                    if matches!(&tokens[i].token, Token::Whitespace(_)) {
                        i += 1;
                    } else {
                        break;
                    }
                }

                // Extract the alias (next identifier, bracketed or unbracketed)
                if i < tokens.len() {
                    if let Token::Word(alias_word) = &tokens[i].token {
                        let alias_name = &alias_word.value;
                        let alias_upper = alias_name.to_uppercase();

                        // Skip if alias is a SQL keyword
                        if !alias_keywords.iter().any(|&k| k == alias_upper)
                            && !alias_name.is_empty()
                        {
                            results.push(alias_name.to_lowercase());
                        }
                        i += 1;
                        continue;
                    }
                }
            }
        }

        i += 1;
    }

    results
}

/// Extract DECLARE types from SQL body using tokenization.
///
/// This function scans SQL and extracts type names from DECLARE statements.
/// Pattern: `DECLARE @varname typename` or `DECLARE @varname typename(precision)`
///
/// Used in `extract_body_dependencies()` to find built-in type dependencies
/// from DECLARE statements in function/procedure bodies.
///
/// # Arguments
/// * `sql` - SQL text to scan (e.g., function or procedure body)
///
/// # Returns
/// A vector of type names (lowercase) in order of appearance.
/// Types include base names without precision/scale (e.g., "nvarchar" not "nvarchar(50)").
pub(crate) fn extract_declare_types_tokenized(sql: &str) -> Vec<String> {
    let mut results = Vec::new();

    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, sql).tokenize_with_location() else {
        return results;
    };

    let mut i = 0;
    while i < tokens.len() {
        // Skip whitespace
        while i < tokens.len() && matches!(&tokens[i].token, Token::Whitespace(_)) {
            i += 1;
        }
        if i >= tokens.len() {
            break;
        }

        // Look for DECLARE keyword
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("DECLARE") {
                i += 1;

                // Skip whitespace after DECLARE
                while i < tokens.len() && matches!(&tokens[i].token, Token::Whitespace(_)) {
                    i += 1;
                }
                if i >= tokens.len() {
                    break;
                }

                // Expect variable name (@name) - MsSqlDialect tokenizes as a single Word
                if let Token::Word(var_word) = &tokens[i].token {
                    if var_word.value.starts_with('@') {
                        i += 1;

                        // Skip whitespace after variable name
                        while i < tokens.len() && matches!(&tokens[i].token, Token::Whitespace(_)) {
                            i += 1;
                        }
                        if i >= tokens.len() {
                            break;
                        }

                        // Extract type name (next identifier)
                        if let Token::Word(type_word) = &tokens[i].token {
                            // Get the base type name (without any precision/scale)
                            let type_name = type_word.value.to_lowercase();
                            results.push(type_name);
                            i += 1;
                            continue;
                        }
                    }
                }
            }
        }

        i += 1;
    }

    results
}

/// Check if a word is a SQL keyword (to filter out from column detection)
pub(crate) fn is_sql_keyword(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "FROM"
            | "WHERE"
            | "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "IS"
            | "IN"
            | "AS"
            | "ON"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "INNER"
            | "OUTER"
            | "CROSS"
            | "FULL"
            | "INSERT"
            | "INTO"
            | "VALUES"
            | "UPDATE"
            | "SET"
            | "DELETE"
            | "CREATE"
            | "ALTER"
            | "DROP"
            | "TABLE"
            | "VIEW"
            | "INDEX"
            | "PROCEDURE"
            | "FUNCTION"
            | "TRIGGER"
            | "BEGIN"
            | "END"
            | "IF"
            | "ELSE"
            | "WHILE"
            | "RETURN"
            | "DECLARE"
            | "INT"
            | "VARCHAR"
            | "NVARCHAR"
            | "CHAR"
            | "NCHAR"
            | "TEXT"
            | "NTEXT"
            | "BIT"
            | "TINYINT"
            | "SMALLINT"
            | "BIGINT"
            | "DECIMAL"
            | "NUMERIC"
            | "FLOAT"
            | "REAL"
            | "MONEY"
            | "SMALLMONEY"
            | "DATE"
            | "TIME"
            | "DATETIME"
            | "DATETIME2"
            | "SMALLDATETIME"
            | "DATETIMEOFFSET"
            | "UNIQUEIDENTIFIER"
            | "BINARY"
            | "VARBINARY"
            | "IMAGE"
            | "XML"
            | "SQL_VARIANT"
            | "TIMESTAMP"
            | "ROWVERSION"
            | "GEOGRAPHY"
            | "GEOMETRY"
            | "HIERARCHYID"
            | "PRIMARY"
            | "KEY"
            | "FOREIGN"
            | "REFERENCES"
            | "UNIQUE"
            | "CHECK"
            | "DEFAULT"
            | "CONSTRAINT"
            | "IDENTITY"
            | "NOCOUNT"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "ISNULL"
            | "COALESCE"
            | "CAST"
            | "CONVERT"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "EXEC"
            | "EXECUTE"
            | "GO"
            | "USE"
            | "DATABASE"
            | "SCHEMA"
            | "GRANT"
            | "REVOKE"
            | "DENY"
            | "ORDER"
            | "BY"
            | "GROUP"
            | "HAVING"
            | "DISTINCT"
            | "TOP"
            | "OFFSET"
            | "FETCH"
            | "NEXT"
            | "ROWS"
            | "ONLY"
            | "UNION"
            | "ALL"
            | "EXCEPT"
            | "INTERSECT"
            | "EXISTS"
            | "ANY"
            | "SOME"
            | "LIKE"
            | "BETWEEN"
            | "ASC"
            | "DESC"
            | "CLUSTERED"
            | "NONCLUSTERED"
            | "OUTPUT"
            | "SCOPE_IDENTITY"
    )
}

/// Check if a word is a SQL keyword that should be filtered from column detection in procedure bodies.
/// This is a more permissive filter than `is_sql_keyword` - it allows words that are commonly
/// used as column names (like TIMESTAMP, ACTION, ID, etc.) even though they're also SQL keywords/types.
pub(crate) fn is_sql_keyword_not_column(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "FROM"
            | "WHERE"
            | "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "IS"
            | "IN"
            | "AS"
            | "ON"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "INNER"
            | "OUTER"
            | "CROSS"
            | "FULL"
            | "INSERT"
            | "INTO"
            | "VALUES"
            | "UPDATE"
            | "SET"
            | "DELETE"
            | "CREATE"
            | "ALTER"
            | "DROP"
            | "TABLE"
            | "VIEW"
            | "INDEX"
            | "PROCEDURE"
            | "FUNCTION"
            | "TRIGGER"
            | "BEGIN"
            | "END"
            | "IF"
            | "ELSE"
            | "WHILE"
            | "RETURN"
            | "DECLARE"
            | "PRIMARY"
            | "KEY"
            | "FOREIGN"
            | "REFERENCES"
            | "UNIQUE"
            | "CHECK"
            | "DEFAULT"
            | "CONSTRAINT"
            | "IDENTITY"
            | "NOCOUNT"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "ISNULL"
            | "COALESCE"
            | "CAST"
            | "CONVERT"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "EXEC"
            | "EXECUTE"
            | "GO"
            | "USE"
            | "DATABASE"
            | "SCHEMA"
            | "GRANT"
            | "REVOKE"
            | "DENY"
            | "ORDER"
            | "BY"
            | "GROUP"
            | "HAVING"
            | "DISTINCT"
            | "TOP"
            | "OFFSET"
            | "FETCH"
            | "NEXT"
            | "ROWS"
            | "ONLY"
            | "UNION"
            | "ALL"
            | "EXCEPT"
            | "INTERSECT"
            | "EXISTS"
            | "ANY"
            | "SOME"
            | "LIKE"
            | "BETWEEN"
            | "ASC"
            | "DESC"
            | "CLUSTERED"
            | "NONCLUSTERED"
            | "OUTPUT"
            | "SCOPE_IDENTITY"
            // Core data types that are rarely used as column names
            | "INT"
            | "INTEGER"
            | "VARCHAR"
            | "NVARCHAR"
            | "CHAR"
            | "NCHAR"
            | "BIT"
            | "TINYINT"
            | "SMALLINT"
            | "BIGINT"
            | "DECIMAL"
            | "NUMERIC"
            | "FLOAT"
            | "REAL"
            | "MONEY"
            | "SMALLMONEY"
            | "DATETIME"
            | "DATETIME2"
            | "SMALLDATETIME"
            | "DATETIMEOFFSET"
            | "UNIQUEIDENTIFIER"
            | "BINARY"
            | "VARBINARY"
            | "XML"
            | "SQL_VARIANT"
            | "ROWVERSION"
            | "GEOGRAPHY"
            | "GEOMETRY"
            | "HIERARCHYID"
            | "NTEXT"
            // SQL Server specific functions and keywords commonly found in queries
            | "STUFF"
            | "FOR"
            | "PATH"
            | "STRING_AGG"
            | "CONCAT"
            | "LEN"
            | "CHARINDEX"
            | "SUBSTRING"
            | "REPLACE"
            | "LTRIM"
            | "RTRIM"
            | "TRIM"
            | "UPPER"
            | "LOWER"
            | "GETDATE"
            | "GETUTCDATE"
            | "DATEADD"
            | "DATEDIFF"
            | "DATENAME"
            | "DATEPART"
            | "YEAR"
            | "MONTH"
            | "DAY"
            | "HOUR"
            | "MINUTE"
            | "SECOND"
            | "APPLY"
            | "WITH"
    )
    // Intentionally excludes: TIMESTAMP, ACTION, ID, TEXT, IMAGE, DATE, TIME, etc.
    // as these are commonly used as column names
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // BodyDependencyTokenScanner tests
    // ============================================================================

    #[test]
    fn test_body_dep_scanner_parameter() {
        let sql = "@MyParam";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], BodyDepToken::Parameter("MyParam".to_string()));
    }

    #[test]
    fn test_body_dep_scanner_parameter_with_whitespace() {
        let sql = "  @MyParam  ";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], BodyDepToken::Parameter("MyParam".to_string()));
    }

    #[test]
    fn test_body_dep_scanner_three_part_bracketed() {
        let sql = "[dbo].[Table].[Column]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::ThreePartBracketed {
                schema: "dbo".to_string(),
                table: "Table".to_string(),
                column: "Column".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_three_part_with_whitespace() {
        let sql = "[dbo] . [Table] . [Column]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::ThreePartBracketed {
                schema: "dbo".to_string(),
                table: "Table".to_string(),
                column: "Column".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_three_part_with_tabs() {
        let sql = "[dbo]\t.\t[Table]\t.\t[Column]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::ThreePartBracketed {
                schema: "dbo".to_string(),
                table: "Table".to_string(),
                column: "Column".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_two_part_bracketed() {
        let sql = "[dbo].[Table]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::TwoPartBracketed {
                first: "dbo".to_string(),
                second: "Table".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_two_part_with_whitespace() {
        let sql = "[dbo] . [Table]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::TwoPartBracketed {
                first: "dbo".to_string(),
                second: "Table".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_alias_dot_bracketed_column() {
        let sql = "t.[Column]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::AliasDotBracketedColumn {
                alias: "t".to_string(),
                column: "Column".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_alias_dot_bracketed_with_whitespace() {
        let sql = "t . [Column]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::AliasDotBracketedColumn {
                alias: "t".to_string(),
                column: "Column".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_bracketed_alias_dot_column() {
        let sql = "[t].Column";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::BracketedAliasDotColumn {
                alias: "t".to_string(),
                column: "Column".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_bracketed_alias_dot_column_with_whitespace() {
        let sql = "[t] . Column";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::BracketedAliasDotColumn {
                alias: "t".to_string(),
                column: "Column".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_single_bracketed() {
        let sql = "[Column]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::SingleBracketed("Column".to_string())
        );
    }

    #[test]
    fn test_body_dep_scanner_two_part_unbracketed() {
        let sql = "dbo.Table";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::TwoPartUnbracketed {
                first: "dbo".to_string(),
                second: "Table".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_two_part_unbracketed_with_whitespace() {
        let sql = "dbo . Table";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::TwoPartUnbracketed {
                first: "dbo".to_string(),
                second: "Table".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_single_unbracketed() {
        let sql = "Column";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::SingleUnbracketed("Column".to_string())
        );
    }

    #[test]
    fn test_body_dep_scanner_multiple_tokens() {
        let sql = "@Param [dbo].[Table] t.[Col]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], BodyDepToken::Parameter("Param".to_string()));
        assert_eq!(
            tokens[1],
            BodyDepToken::TwoPartBracketed {
                first: "dbo".to_string(),
                second: "Table".to_string(),
            }
        );
        assert_eq!(
            tokens[2],
            BodyDepToken::AliasDotBracketedColumn {
                alias: "t".to_string(),
                column: "Col".to_string(),
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_empty_input() {
        let sql = "";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_body_dep_scanner_whitespace_only() {
        let sql = "   \t\n   ";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert!(tokens.is_empty());
    }

    // ============================================================================
    // QualifiedName tests
    // ============================================================================

    #[test]
    fn test_qualified_name_single_bracketed() {
        let qn = parse_qualified_name_tokenized("[Column]");
        assert!(qn.is_some());
        let qn = qn.unwrap();
        assert_eq!(qn.first, "Column");
        assert!(qn.second.is_none());
        assert!(qn.third.is_none());
        assert_eq!(qn.part_count(), 1);
        assert_eq!(qn.last_part(), "Column");
    }

    #[test]
    fn test_qualified_name_single_unbracketed() {
        let qn = parse_qualified_name_tokenized("Column");
        assert!(qn.is_some());
        let qn = qn.unwrap();
        assert_eq!(qn.first, "Column");
        assert_eq!(qn.part_count(), 1);
    }

    #[test]
    fn test_qualified_name_two_part_bracketed() {
        let qn = parse_qualified_name_tokenized("[dbo].[Table]");
        assert!(qn.is_some());
        let qn = qn.unwrap();
        assert_eq!(qn.first, "dbo");
        assert_eq!(qn.second, Some("Table".to_string()));
        assert!(qn.third.is_none());
        assert_eq!(qn.part_count(), 2);
        assert_eq!(qn.last_part(), "Table");
        assert_eq!(qn.schema_and_table(), Some(("dbo", "Table")));
    }

    #[test]
    fn test_qualified_name_two_part_unbracketed() {
        let qn = parse_qualified_name_tokenized("dbo.Table");
        assert!(qn.is_some());
        let qn = qn.unwrap();
        assert_eq!(qn.first, "dbo");
        assert_eq!(qn.second, Some("Table".to_string()));
        assert_eq!(qn.part_count(), 2);
    }

    #[test]
    fn test_qualified_name_three_part_bracketed() {
        let qn = parse_qualified_name_tokenized("[dbo].[Table].[Column]");
        assert!(qn.is_some());
        let qn = qn.unwrap();
        assert_eq!(qn.first, "dbo");
        assert_eq!(qn.second, Some("Table".to_string()));
        assert_eq!(qn.third, Some("Column".to_string()));
        assert_eq!(qn.part_count(), 3);
        assert_eq!(qn.last_part(), "Column");
    }

    #[test]
    fn test_qualified_name_empty() {
        let qn = parse_qualified_name_tokenized("");
        assert!(qn.is_none());
    }

    #[test]
    fn test_qualified_name_whitespace_only() {
        let qn = parse_qualified_name_tokenized("   ");
        assert!(qn.is_none());
    }

    #[test]
    fn test_qualified_name_parameter_returns_none() {
        let qn = parse_qualified_name_tokenized("@Param");
        assert!(qn.is_none());
    }

    // ============================================================================
    // extract_declare_types_tokenized tests
    // ============================================================================

    #[test]
    fn test_declare_type_simple_int() {
        let types = extract_declare_types_tokenized("DECLARE @Count INT");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_varchar_with_size() {
        let types = extract_declare_types_tokenized("DECLARE @Name NVARCHAR(50)");
        assert_eq!(types, vec!["nvarchar"]);
    }

    #[test]
    fn test_declare_type_with_tabs() {
        let types = extract_declare_types_tokenized("DECLARE\t@Count\tINT");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_with_multiple_spaces() {
        let types = extract_declare_types_tokenized("DECLARE   @Count   INT");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_case_insensitive() {
        let types = extract_declare_types_tokenized("declare @count int");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_empty() {
        let types = extract_declare_types_tokenized("");
        assert!(types.is_empty());
    }

    #[test]
    fn test_declare_type_no_declare() {
        let types = extract_declare_types_tokenized("SELECT * FROM Table");
        assert!(types.is_empty());
    }

    // ============================================================================
    // strip_sql_comments_for_body_deps tests
    // ============================================================================

    #[test]
    fn test_strip_line_comment() {
        let result = strip_sql_comments_for_body_deps("SELECT * -- comment\nFROM Table");
        assert_eq!(result, "SELECT * \nFROM Table");
    }

    #[test]
    fn test_strip_block_comment() {
        let result = strip_sql_comments_for_body_deps("SELECT /* comment */ * FROM Table");
        assert_eq!(result, "SELECT   * FROM Table");
    }

    #[test]
    fn test_preserve_string_literal() {
        let result = strip_sql_comments_for_body_deps("SELECT 'text -- not a comment'");
        assert_eq!(result, "SELECT 'text -- not a comment'");
    }

    // ============================================================================
    // extract_column_aliases_tokenized tests
    // ============================================================================

    #[test]
    fn test_extract_column_alias_simple() {
        let aliases = extract_column_aliases_tokenized("SELECT col AS alias");
        assert_eq!(aliases, vec!["alias"]);
    }

    #[test]
    fn test_extract_column_alias_with_whitespace() {
        let aliases = extract_column_aliases_tokenized("SELECT col   AS   alias");
        assert_eq!(aliases, vec!["alias"]);
    }

    #[test]
    fn test_extract_column_alias_multiple() {
        let aliases = extract_column_aliases_tokenized("SELECT a AS x, b AS y");
        assert_eq!(aliases, vec!["x", "y"]);
    }

    #[test]
    fn test_extract_column_alias_filters_keywords() {
        let aliases = extract_column_aliases_tokenized("SELECT a AS FROM, b AS alias");
        assert_eq!(aliases, vec!["alias"]);
    }
}
