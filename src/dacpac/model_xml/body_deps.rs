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
// CTE (Common Table Expression) Extraction (Phase 24.1.2)
// =============================================================================

/// Represents a column extracted from a CTE definition
#[derive(Debug, Clone)]
pub(crate) struct CteColumn {
    /// Column name (output alias or original column name)
    pub name: String,
    /// Expression dependencies - source columns this CTE column references
    /// e.g., ["[dbo].[Account].[Id]", "[dbo].[Account].[Name]"]
    pub expression_dependencies: Vec<String>,
}

/// Represents a CTE extracted from a WITH clause
#[derive(Debug, Clone)]
pub(crate) struct CteDefinition {
    /// CTE name (e.g., "AccountCte")
    pub name: String,
    /// CTE sequence number within the procedure/view (CTE1, CTE2, etc.)
    /// This is based on the WITH block number, not the CTE position within a WITH block
    pub cte_number: u32,
    /// Columns in this CTE with their expression dependencies
    pub columns: Vec<CteColumn>,
}

// =============================================================================
// Temp Table Extraction (Phase 24.2)
// =============================================================================

/// Represents a column extracted from a CREATE TABLE #temp definition
#[derive(Debug, Clone)]
pub(crate) struct TempTableColumn {
    /// Column name
    pub name: String,
    /// Data type (e.g., "int", "varchar(50)")
    pub data_type: String,
    /// Whether the column is nullable (defaults to true)
    pub is_nullable: bool,
}

/// Represents a temp table extracted from a CREATE TABLE #name statement
#[derive(Debug, Clone)]
pub(crate) struct TempTableDefinition {
    /// Temp table name (including the # prefix, e.g., "#TempOrders")
    pub name: String,
    /// Temp table sequence number within the procedure (TempTable1, TempTable2, etc.)
    pub temp_table_number: u32,
    /// Columns in this temp table
    pub columns: Vec<TempTableColumn>,
}

// =============================================================================
// Table Variable Extraction (Phase 24.3)
// =============================================================================

/// Represents a column extracted from a DECLARE @name TABLE definition
#[derive(Debug, Clone)]
pub(crate) struct TableVariableColumn {
    /// Column name
    pub name: String,
    /// Data type (e.g., "int", "varchar(50)")
    pub data_type: String,
    /// Whether the column is nullable (defaults to true)
    pub is_nullable: bool,
}

/// Represents a table variable extracted from a DECLARE @name TABLE statement
#[derive(Debug, Clone)]
pub(crate) struct TableVariableDefinition {
    /// Table variable name (including the @ prefix, e.g., "@OrderItems")
    pub name: String,
    /// Table variable sequence number within the procedure (TableVariable1, TableVariable2, etc.)
    pub table_variable_number: u32,
    /// Columns in this table variable
    pub columns: Vec<TableVariableColumn>,
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

/// A token with its byte position in the original SQL text.
/// Used for scope-aware column resolution in APPLY subqueries.
#[derive(Debug, Clone)]
pub(crate) struct BodyDepTokenWithPos {
    pub token: BodyDepToken,
    pub byte_pos: usize,
}

/// Represents a subquery scope with its byte range, internal tables, and aliases.
/// Used to resolve columns and aliases to the correct table based on position.
/// Phase 43: Extended to track aliases per scope for scope-aware alias resolution.
#[derive(Debug, Clone)]
pub(crate) struct ApplySubqueryScope {
    /// Byte position where the subquery starts (after opening paren)
    pub start_pos: usize,
    /// Byte position where the subquery ends (at closing paren)
    pub end_pos: usize,
    /// Tables referenced inside this subquery (in order of appearance)
    pub tables: Vec<String>,
    /// Aliases defined within this scope: alias (lowercase) -> table reference
    /// Phase 43.1.2: Per-scope alias tracking for scope conflicts
    pub aliases: HashMap<String, String>,
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

    /// Get the byte offset of the current token in the original SQL text.
    /// Uses line/column info from tokenizer to compute byte offset.
    fn current_byte_offset(&self, line_offsets: &[usize]) -> usize {
        if let Some(token) = self.current_token() {
            let loc = &token.span.start;
            location_to_byte_offset(line_offsets, loc.line, loc.column)
        } else {
            0
        }
    }

    /// Scan the body and return all matched tokens with their byte positions.
    /// This is used for scope-aware column resolution in APPLY subqueries.
    pub fn scan_with_positions(&mut self, sql: &str) -> Vec<BodyDepTokenWithPos> {
        let line_offsets = compute_line_offsets(sql);
        let mut results = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace();
            if self.is_at_end() {
                break;
            }

            // Get byte position before scanning the token
            let byte_pos = self.current_byte_offset(&line_offsets);

            // Try to match patterns in order of specificity
            if let Some(token) = self.try_scan_token() {
                results.push(BodyDepTokenWithPos { token, byte_pos });
            } else {
                // No pattern matched, advance to next token
                self.advance();
            }
        }

        results
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
    subquery_aliases: &HashSet<String>,
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
                    let first_lower = first.to_lowercase();
                    // Skip if first part is a subquery alias (APPLY alias)
                    if subquery_aliases.contains(&first_lower) {
                        continue;
                    }
                    // Skip if first part is a table alias - this is alias.column, not schema.table
                    if table_aliases.contains_key(&first_lower) {
                        continue;
                    }
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
                // Skip if first part is a subquery alias (APPLY alias)
                if subquery_aliases.contains(&first.to_lowercase()) {
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
                let alias_lower = alias.to_lowercase();
                if !table_aliases.contains_key(&alias_lower)
                    && !subquery_aliases.contains(&alias_lower)
                {
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
                    let alias_lower = alias.to_lowercase();
                    if !table_aliases.contains_key(&alias_lower)
                        && !subquery_aliases.contains(&alias_lower)
                    {
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
// Function Call Detection
// =============================================================================

/// Extract function call references from SQL body.
/// Detects patterns like `dbo.f_split(` or `[schema].[func](` after FROM/JOIN/APPLY keywords.
/// Returns a set of lowercase `[schema].[function]` references.
fn extract_function_call_refs(body: &str) -> HashSet<String> {
    let mut function_refs = HashSet::new();

    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, body).tokenize_with_location() {
        Ok(t) => t,
        Err(_) => return function_refs,
    };

    let mut pos = 0;
    while pos < tokens.len() {
        // Look for FROM, JOIN, or APPLY keywords
        if let Token::Word(w) = &tokens[pos].token {
            let keyword_upper = w.value.to_uppercase();
            if matches!(
                keyword_upper.as_str(),
                "FROM" | "JOIN" | "APPLY" | "INNER" | "LEFT" | "RIGHT" | "OUTER" | "CROSS" | "FULL"
            ) {
                pos += 1;
                // Skip whitespace
                while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                    pos += 1;
                }
                // Skip additional join keywords (INNER JOIN, LEFT OUTER JOIN, etc.)
                while pos < tokens.len() {
                    if let Token::Word(w2) = &tokens[pos].token {
                        let w2_upper = w2.value.to_uppercase();
                        if matches!(
                            w2_upper.as_str(),
                            "JOIN" | "APPLY" | "OUTER" | "INNER" | "LEFT" | "RIGHT" | "FULL"
                        ) {
                            pos += 1;
                            while pos < tokens.len()
                                && matches!(tokens[pos].token, Token::Whitespace(_))
                            {
                                pos += 1;
                            }
                            continue;
                        }
                    }
                    break;
                }
                // Skip subqueries starting with (
                if pos < tokens.len() && matches!(tokens[pos].token, Token::LParen) {
                    pos += 1;
                    continue;
                }
                // Try to parse a qualified name followed by (
                if let Some((schema, name, next_pos)) =
                    try_parse_qualified_name_for_function(&tokens, pos)
                {
                    // Check if followed by ( after optional whitespace
                    let mut check_pos = next_pos;
                    while check_pos < tokens.len()
                        && matches!(tokens[check_pos].token, Token::Whitespace(_))
                    {
                        check_pos += 1;
                    }
                    if check_pos < tokens.len() && matches!(tokens[check_pos].token, Token::LParen)
                    {
                        // This is a function call - add to set
                        let func_ref = format!("[{}].[{}]", schema, name).to_lowercase();
                        function_refs.insert(func_ref);
                    }
                    pos = check_pos;
                } else {
                    pos += 1;
                }
                continue;
            }
        }
        pos += 1;
    }

    function_refs
}

/// Try to parse a qualified name (schema.name or [schema].[name]) from tokens.
/// Returns (schema, name, next_position) if successful.
fn try_parse_qualified_name_for_function(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
    start_pos: usize,
) -> Option<(String, String, usize)> {
    let mut pos = start_pos;

    // Parse first identifier
    let first = match tokens.get(pos) {
        Some(t) => match &t.token {
            Token::Word(w) => w.value.clone(),
            _ => return None,
        },
        None => return None,
    };
    pos += 1;

    // Skip whitespace
    while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
        pos += 1;
    }

    // Check for dot
    if pos >= tokens.len() || !matches!(tokens[pos].token, Token::Period) {
        // Unqualified name - use default schema
        return Some(("dbo".to_string(), first, pos));
    }
    pos += 1; // Skip dot

    // Skip whitespace
    while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
        pos += 1;
    }

    // Parse second identifier
    let second = match tokens.get(pos) {
        Some(t) => match &t.token {
            Token::Word(w) => w.value.clone(),
            _ => return None,
        },
        None => return None,
    };
    pos += 1;

    Some((first, second, pos))
}

// =============================================================================
// Body Dependency Extraction
// =============================================================================

use super::column_registry::ColumnRegistry;

/// Extract body dependencies from a procedure/function body
/// This extracts dependencies in order of appearance:
/// 1. Built-in types from DECLARE statements
/// 2. Table references, columns, and parameters in the order they appear
///
/// Phase 49: Accepts a ColumnRegistry for schema-aware unqualified column resolution.
/// When multiple tables are in scope, the registry is used to determine which table
/// actually has the column, eliminating false positive dependencies.
pub(crate) fn extract_body_dependencies(
    body: &str,
    full_name: &str,
    params: &[String],
    column_registry: &ColumnRegistry,
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
    // Track table variable column names - these should not be resolved against other tables
    let mut table_var_columns: HashSet<String> = HashSet::new();

    // Extract aliases from FROM/JOIN clauses with proper alias tracking
    extract_table_aliases_for_body_deps(body, &mut table_aliases, &mut subquery_aliases);

    // Extract column aliases (SELECT expr AS alias patterns)
    extract_column_aliases_for_body_deps(body, &mut column_aliases);

    // Extract table variable column names to exclude from resolution
    // Pattern: DECLARE @name TABLE ([col1] type, [col2] type, ...)
    for table_var in extract_table_variable_definitions(body) {
        for col in &table_var.columns {
            table_var_columns.insert(col.name.to_lowercase());
        }
    }

    // Extract function call names to exclude from column resolution targets
    // Pattern: FROM dbo.f_split(...) or CROSS APPLY dbo.func(...)
    // These shouldn't be used as default tables for unqualified column resolution
    let function_refs: HashSet<String> = extract_function_call_refs(body);

    // First pass: collect all table references using token-based extraction
    // Phase 20.4.3: Replaced BRACKETED_TABLE_RE and UNBRACKETED_TABLE_RE with tokenization
    // This handles whitespace (tabs, multiple spaces, newlines) correctly and is more robust
    let all_table_refs = extract_table_refs_tokenized(body, &table_aliases, &subquery_aliases);

    // Filter out function calls from table refs used for column resolution
    // Function refs are still valid as table refs (for dependency tracking), but shouldn't
    // be used as default tables for unqualified column resolution
    let table_refs: Vec<String> = all_table_refs
        .iter()
        .filter(|r| !function_refs.contains(&r.to_lowercase()))
        .cloned()
        .collect();

    // Phase 34+43: Extract ALL subquery scopes for scope-aware column and alias resolution
    // Phase 43: Extended to include derived tables and per-scope alias tracking
    let all_scopes = extract_all_subquery_scopes(body);

    // Scan body sequentially for all references in order of appearance using token-based scanner
    // Note: DotNet has a complex ordering that depends on SQL clause structure (FROM first, etc.)
    // We process in textual order which may differ from DotNet's order but contains the same refs
    // Phase 20.2.1: Replaced TOKEN_RE regex with BodyDependencyTokenScanner for robust whitespace handling

    if let Some(mut scanner) = BodyDependencyTokenScanner::new(body) {
        // Phase 34: Use position-aware scanning for scope-aware column resolution
        for token_with_pos in scanner.scan_with_positions(body) {
            let token = token_with_pos.token;
            let byte_pos = token_with_pos.byte_pos;
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

                    // Phase 43: Use position-aware alias resolution for scope conflicts
                    // Check if first_part is a table alias that should be resolved
                    if let Some(resolved_table) = resolve_alias_for_position(
                        &first_lower,
                        byte_pos,
                        &all_scopes,
                        &table_aliases,
                    ) {
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

                    // Phase 43: Use position-aware alias resolution for scope conflicts
                    // Check if alias is a table alias that should be resolved
                    if let Some(resolved_table) = resolve_alias_for_position(
                        &alias_lower,
                        byte_pos,
                        &all_scopes,
                        &table_aliases,
                    ) {
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

                    // Phase 43: Use position-aware alias resolution for scope conflicts
                    // Check if alias is a table alias that should be resolved
                    if let Some(resolved_table) = resolve_alias_for_position(
                        &alias_lower,
                        byte_pos,
                        &all_scopes,
                        &table_aliases,
                    ) {
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

                    // Skip if this is a known table alias, subquery alias, column alias, or table variable column
                    if table_aliases.contains_key(&ident_lower)
                        || subquery_aliases.contains(&ident_lower)
                        || column_aliases.contains(&ident_lower)
                        || table_var_columns.contains(&ident_lower)
                    {
                        continue;
                    }

                    // Skip if this is part of a table reference (schema or table name)
                    let is_table_or_schema = table_refs.iter().any(|t| {
                        t.ends_with(&format!("].[{}]", ident))
                            || t.starts_with(&format!("[{}].", ident))
                    });

                    // If not a table/schema, treat as unqualified column -> resolve against scope table
                    // Phase 49: Use schema-aware resolution to find unique table with this column
                    if !is_table_or_schema {
                        let resolve_table = find_scope_table_for_column(
                            &ident,
                            byte_pos,
                            &all_scopes,
                            &table_refs,
                            column_registry,
                        );
                        if let Some(target_table) = resolve_table {
                            // First emit the table reference if not seen (DotNet deduplicates tables)
                            if !seen_tables.contains(target_table) {
                                seen_tables.insert(target_table.clone());
                                deps.push(BodyDependency::ObjectRef(target_table.clone()));
                            }

                            // Direct column refs (single bracketed) ARE deduplicated by DotNet
                            let col_ref = format!("{}.[{}]", target_table, ident);
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

                    // Phase 43: Use position-aware alias resolution for scope conflicts
                    // Check if first_part is a table alias that should be resolved
                    if let Some(resolved_table) = resolve_alias_for_position(
                        &first_lower,
                        byte_pos,
                        &all_scopes,
                        &table_aliases,
                    ) {
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

                    // Skip if this is a known table alias, subquery alias, column alias, or table variable column
                    if table_aliases.contains_key(&ident_lower)
                        || subquery_aliases.contains(&ident_lower)
                        || column_aliases.contains(&ident_lower)
                        || table_var_columns.contains(&ident_lower)
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

                    // If not a table/schema, treat as unqualified column -> resolve against scope table
                    // Phase 49: Use schema-aware resolution to find unique table with this column
                    if !is_table_or_schema {
                        let resolve_table = find_scope_table_for_column(
                            &ident,
                            byte_pos,
                            &all_scopes,
                            &table_refs,
                            column_registry,
                        );
                        if let Some(target_table) = resolve_table {
                            // First emit the table reference if not seen (DotNet deduplicates tables)
                            if !seen_tables.contains(target_table) {
                                seen_tables.insert(target_table.clone());
                                deps.push(BodyDependency::ObjectRef(target_table.clone()));
                            }

                            // Direct column refs (single unbracketed) ARE deduplicated by DotNet
                            let col_ref = format!("{}.[{}]", target_table, ident);
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

/// Phase 43: Extract ALL subquery scopes (APPLY + derived tables) with their aliases.
/// This is the primary scope extraction function for body dependency analysis.
/// Each scope contains byte range, tables, and aliases defined within.
pub(crate) fn extract_all_subquery_scopes(body: &str) -> Vec<ApplySubqueryScope> {
    let mut parser = match TableAliasTokenParser::new(body) {
        Some(p) => p,
        None => return Vec::new(),
    };
    parser.extract_all_scopes(body)
}

/// Find the appropriate table to resolve an unqualified column against.
/// Phase 49: Schema-aware table resolution for unqualified columns.
///
/// Given an unqualified column name, find the table that has this column.
/// - First, determine the tables in scope (APPLY subquery tables if inside one, otherwise all table_refs)
/// - Then, use the ColumnRegistry to find which table(s) have this column
/// - If exactly one table has the column, return it
/// - If 0 tables have the column (none known in registry), fall back to first table (backward compat)
/// - If >1 tables have the column, return None (ambiguous - skip dependency emission)
///
/// This eliminates false positive dependencies like `[dbo].[EntityTypeDefaults].[Name]`
/// when `Name` actually belongs to a different table in scope.
fn find_scope_table_for_column<'a>(
    column_name: &str,
    byte_pos: usize,
    apply_scopes: &'a [ApplySubqueryScope],
    table_refs: &'a [String],
    column_registry: &ColumnRegistry,
) -> Option<&'a String> {
    // First, determine which tables are in scope based on position
    let tables_in_scope: &[String] = {
        // Check if position falls within any APPLY subquery scope
        let mut scope_tables: Option<&[String]> = None;
        for scope in apply_scopes {
            if byte_pos >= scope.start_pos && byte_pos <= scope.end_pos {
                // Position is inside this APPLY subquery - use its tables
                if !scope.tables.is_empty() {
                    scope_tables = Some(&scope.tables);
                    break;
                }
            }
        }
        scope_tables.unwrap_or(table_refs)
    };

    // Phase 49: Use schema-aware resolution to find the unique table with this column
    let matches = column_registry.find_tables_with_column(column_name, tables_in_scope);

    match matches.len() {
        // Exactly one table has this column - use it
        1 => Some(matches[0]),
        // No table has this column in the registry.
        // This could mean:
        // a) The registry has no info about any tables in scope (empty registry or tables not tracked)
        // b) The column doesn't exist in any known table
        // Fall back to first table for backward compatibility with existing behavior.
        0 => tables_in_scope.first(),
        // Multiple tables have this column - ambiguous, skip resolution
        _ => None,
    }
}

/// Phase 43.2: Resolve an alias to its table reference, considering position within scopes.
/// When multiple scopes define the same alias, uses the innermost scope that contains the position.
/// Falls back to global table_aliases if no scope-specific alias is found.
///
/// For nested scopes, the smallest byte range (innermost) wins.
fn resolve_alias_for_position<'a>(
    alias: &str,
    byte_pos: usize,
    scopes: &'a [ApplySubqueryScope],
    global_aliases: &'a HashMap<String, String>,
) -> Option<&'a String> {
    let alias_lower = alias.to_lowercase();

    // Find the innermost (smallest) scope that contains this position and has this alias
    let mut best_scope: Option<&ApplySubqueryScope> = None;
    let mut best_scope_size = usize::MAX;

    for scope in scopes {
        if byte_pos >= scope.start_pos && byte_pos <= scope.end_pos {
            // Position is inside this scope
            if scope.aliases.contains_key(&alias_lower) {
                let scope_size = scope.end_pos - scope.start_pos;
                if scope_size < best_scope_size {
                    best_scope = Some(scope);
                    best_scope_size = scope_size;
                }
            }
        }
    }

    // If found in a scope, use that scope's alias definition
    if let Some(scope) = best_scope {
        return scope.aliases.get(&alias_lower);
    }

    // Fall back to global aliases
    global_aliases.get(&alias_lower)
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
        // CTEs are mapped to their underlying tables so references like AccountCte.Id
        // get resolved to [dbo].[Account].[Id]
        self.extract_cte_aliases_with_tables(table_aliases, subquery_aliases);

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
                // Check for APPLY - first save position after APPLY to scan subquery contents,
                // then skip to end and capture the APPLY alias
                let saved_pos = self.pos;
                self.advance();
                self.skip_whitespace();
                if self.check_keyword(Keyword::APPLY) || self.check_word_ci("APPLY") {
                    self.advance();
                    self.skip_whitespace();

                    // Check if followed by opening paren (subquery)
                    if self.check_token(&Token::LParen) {
                        // Save position after LParen to scan subquery contents
                        let subquery_start = self.pos;

                        // Skip to end of balanced parens to find APPLY alias
                        self.skip_balanced_parens();
                        self.skip_whitespace();

                        // Check for AS keyword (optional)
                        if self.check_keyword(Keyword::AS) {
                            self.advance();
                            self.skip_whitespace();
                        }

                        // Capture the APPLY alias (e.g., "d" in CROSS APPLY (...) d)
                        if let Some(alias) = self.try_parse_subquery_alias() {
                            let alias_lower = alias.to_lowercase();
                            if !Self::is_alias_keyword(&alias_lower) {
                                subquery_aliases.insert(alias_lower);
                            }
                        }

                        // Now go back and scan the subquery contents for table aliases
                        // Advance past the opening paren
                        self.pos = subquery_start + 1;
                        // The main loop will continue scanning FROM/JOIN inside the subquery
                    }
                    // If not followed by paren, it might be OUTER JOIN, handle in else branch
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

    /// This is the main scope extraction function that handles:
    /// - CROSS/OUTER APPLY subqueries
    /// - Derived tables in JOIN (SELECT...) pattern
    ///
    /// Each scope contains the byte range, tables, and aliases defined within.
    pub fn extract_all_scopes(&mut self, sql: &str) -> Vec<ApplySubqueryScope> {
        let line_offsets = compute_line_offsets(sql);
        let mut scopes = Vec::new();

        self.pos = 0;

        while !self.is_at_end() {
            self.skip_whitespace();

            // Look for CROSS/OUTER APPLY patterns
            if self.check_word_ci("CROSS") || self.check_word_ci("OUTER") {
                let saved_pos = self.pos;
                self.advance();
                self.skip_whitespace();

                if self.check_keyword(Keyword::APPLY) || self.check_word_ci("APPLY") {
                    self.advance();
                    self.skip_whitespace();

                    // Check if followed by opening paren (subquery)
                    if self.check_token(&Token::LParen) {
                        if let Some(scope) = self.extract_subquery_scope(&line_offsets) {
                            scopes.push(scope);
                        }
                    }
                } else {
                    // Not an APPLY, restore and continue
                    self.pos = saved_pos;
                    self.advance();
                }
            }
            // Look for JOIN (SELECT...) derived table patterns
            else if self.is_join_keyword() {
                self.skip_join_keywords();
                self.skip_whitespace();

                // Check if followed by opening paren (derived table)
                if self.check_token(&Token::LParen) {
                    if let Some(scope) = self.extract_subquery_scope(&line_offsets) {
                        scopes.push(scope);
                    }
                }
            } else {
                self.advance();
            }
        }

        scopes
    }

    /// Extract a subquery scope starting at the current position (which should be at LParen).
    /// Collects tables and aliases defined within the subquery.
    fn extract_subquery_scope(&mut self, line_offsets: &[usize]) -> Option<ApplySubqueryScope> {
        if !self.check_token(&Token::LParen) {
            return None;
        }

        // Get byte position of opening paren
        let start_byte_pos = self.get_current_byte_offset(line_offsets);

        self.advance(); // Move past opening paren

        // Collect tables and aliases inside the subquery
        let mut tables_in_scope = Vec::new();
        let mut aliases_in_scope: HashMap<String, String> = HashMap::new();
        let mut depth = 1;

        while !self.is_at_end() && depth > 0 {
            if self.check_token(&Token::LParen) {
                depth += 1;
                self.advance();
            } else if self.check_token(&Token::RParen) {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                self.advance();
            } else if self.check_keyword(Keyword::FROM) && depth == 1 {
                // Extract table ref and alias after FROM at top level
                self.advance();
                self.skip_whitespace();
                if let Some((schema, table_name, alias_opt)) = self.parse_table_name_with_alias() {
                    let table_ref = format!("[{}].[{}]", schema, table_name);
                    if !tables_in_scope.contains(&table_ref) {
                        tables_in_scope.push(table_ref.clone());
                    }
                    // Add alias if present
                    if let Some(alias) = alias_opt {
                        let alias_lower = alias.to_lowercase();
                        if !Self::is_alias_keyword(&alias_lower) {
                            aliases_in_scope.insert(alias_lower, table_ref.clone());
                        }
                    }
                    // Also add table name as self-alias
                    let table_name_lower = table_name.to_lowercase();
                    aliases_in_scope
                        .entry(table_name_lower)
                        .or_insert(table_ref);
                }
            } else if self.is_join_keyword() && depth == 1 {
                // Extract table ref and alias after JOIN at top level
                self.skip_join_keywords();
                self.skip_whitespace();
                if !self.check_token(&Token::LParen) {
                    if let Some((schema, table_name, alias_opt)) =
                        self.parse_table_name_with_alias()
                    {
                        let table_ref = format!("[{}].[{}]", schema, table_name);
                        if !tables_in_scope.contains(&table_ref) {
                            tables_in_scope.push(table_ref.clone());
                        }
                        // Add alias if present
                        if let Some(alias) = alias_opt {
                            let alias_lower = alias.to_lowercase();
                            if !Self::is_alias_keyword(&alias_lower) {
                                aliases_in_scope.insert(alias_lower, table_ref.clone());
                            }
                        }
                        // Also add table name as self-alias
                        let table_name_lower = table_name.to_lowercase();
                        aliases_in_scope
                            .entry(table_name_lower)
                            .or_insert(table_ref);
                    }
                }
            } else {
                self.advance();
            }
        }

        // Get byte position of closing paren
        let end_byte_pos = self.get_current_byte_offset(line_offsets);

        // Continue past the closing paren
        if !self.is_at_end() && self.check_token(&Token::RParen) {
            self.advance();
        }

        // Only return scope if it has tables or aliases
        if !tables_in_scope.is_empty() || !aliases_in_scope.is_empty() {
            Some(ApplySubqueryScope {
                start_pos: start_byte_pos,
                end_pos: end_byte_pos,
                tables: tables_in_scope,
                aliases: aliases_in_scope,
            })
        } else {
            None
        }
    }

    /// Parse table name with optional alias, returning (schema, table, Option<alias>)
    fn parse_table_name_with_alias(&mut self) -> Option<(String, String, Option<String>)> {
        let (schema, table_name) = self.parse_table_name()?;
        self.skip_whitespace();

        // Check for AS keyword (optional)
        if self.check_keyword(Keyword::AS) {
            self.advance();
            self.skip_whitespace();
        }

        // Try to get alias
        let alias = self.try_parse_table_alias();
        Some((schema, table_name, alias))
    }

    /// Get the byte offset of the current token position
    fn get_current_byte_offset(&self, line_offsets: &[usize]) -> usize {
        if let Some(token) = self.tokens.get(self.pos) {
            let loc = &token.span.start;
            location_to_byte_offset(line_offsets, loc.line, loc.column)
        } else if let Some(last_token) = self.tokens.last() {
            // If past end, return position after last token
            let loc = &last_token.span.end;
            location_to_byte_offset(line_offsets, loc.line, loc.column)
        } else {
            0
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
    /// Extract CTE aliases and map them to their underlying tables.
    /// CTEs are treated like table aliases - when code references `AccountCte.Id`,
    /// it gets resolved to `[dbo].[Account].[Id]` because AccountCte selects FROM Account.
    fn extract_cte_aliases_with_tables(
        &mut self,
        table_aliases: &mut HashMap<String, String>,
        subquery_aliases: &mut HashSet<String>,
    ) {
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

                        // Skip optional column list: CTE_name (col1, col2, ...) AS (...)
                        if self.check_token(&Token::LParen) {
                            // This might be column list or AS body - peek ahead
                            let saved_pos = self.pos;
                            self.skip_balanced_parens();
                            self.skip_whitespace();

                            // If followed by AS, that was column list; otherwise restore
                            if !self.check_keyword(Keyword::AS) {
                                self.pos = saved_pos;
                            }
                        }

                        // Expect AS keyword
                        if self.check_keyword(Keyword::AS) {
                            self.advance();
                            self.skip_whitespace();

                            // Expect opening paren for CTE body
                            if self.check_token(&Token::LParen) {
                                // Save position to scan CTE body for FROM table
                                let cte_body_start = self.pos;
                                self.advance(); // Skip opening paren

                                // Find first FROM clause in CTE body
                                let mut paren_depth = 1;
                                let mut found_table: Option<String> = None;

                                while !self.is_at_end() && paren_depth > 0 {
                                    if self.check_token(&Token::LParen) {
                                        paren_depth += 1;
                                        self.advance();
                                    } else if self.check_token(&Token::RParen) {
                                        paren_depth -= 1;
                                        if paren_depth == 0 {
                                            break;
                                        }
                                        self.advance();
                                    } else if paren_depth == 1
                                        && self.check_keyword(Keyword::FROM)
                                        && found_table.is_none()
                                    {
                                        // Found FROM at top level of CTE body
                                        self.advance();
                                        self.skip_whitespace();

                                        // Parse the table name
                                        if let Some((schema, table_name)) = self.parse_table_name()
                                        {
                                            found_table =
                                                Some(format!("[{}].[{}]", schema, table_name));
                                        }
                                    } else {
                                        self.advance();
                                    }
                                }

                                // Skip to end of balanced parens
                                self.pos = cte_body_start;
                                self.skip_balanced_parens();

                                // Add CTE to appropriate map
                                if !Self::is_alias_keyword(&cte_name_lower) {
                                    if let Some(table_ref) = found_table {
                                        // CTE maps to its underlying table
                                        table_aliases.insert(cte_name_lower, table_ref);
                                    } else {
                                        // CTE doesn't have a simple FROM table (e.g., VALUES, recursive)
                                        // Treat as subquery alias (skip column refs)
                                        subquery_aliases.insert(cte_name_lower);
                                    }
                                }

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

    /// Legacy version that only extracts CTE names as subquery aliases (for backward compatibility)
    fn extract_cte_aliases(&mut self, subquery_aliases: &mut HashSet<String>) {
        let mut dummy_table_aliases = HashMap::new();
        self.extract_cte_aliases_with_tables(&mut dummy_table_aliases, subquery_aliases);
        // Move any entries from table_aliases to subquery_aliases for legacy behavior
        for (cte_name, _) in dummy_table_aliases {
            subquery_aliases.insert(cte_name);
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

        // Handle table-valued function calls: dbo.f_split(@args, ',') [Alias]
        // Skip over the function arguments in parentheses to find the alias
        if self.check_token(&Token::LParen) {
            self.skip_balanced_parens();
            self.skip_whitespace();
        }

        // Check for AS keyword (optional)
        if self.check_keyword(Keyword::AS) {
            self.advance();
            self.skip_whitespace();
        }

        // Check for alias - must be an identifier that's not a keyword like ON, WHERE, etc.
        let table_ref = format!("[{}].[{}]", schema, table_name);
        let table_name_lower = table_name.to_lowercase();

        if let Some(alias) = self.try_parse_table_alias() {
            let alias_lower = alias.to_lowercase();

            // Skip if alias is a SQL keyword
            if Self::is_alias_keyword(&alias_lower) {
                return;
            }

            // Don't overwrite if already captured by a more specific pattern
            // Note: This can cause issues with same-named aliases in different scopes,
            // but scope-tracking is complex. The first definition usually wins.
            table_aliases
                .entry(alias_lower)
                .or_insert_with(|| table_ref.clone());
        }

        // Always add the table name itself as a "self-alias", even when there's an explicit alias.
        // This handles patterns like "FROM FundsTransfer ft" where the scanner later encounters
        // "FundsTransfer" as a single identifier (e.g., after FROM keyword) and needs to recognize
        // it as a table name rather than an unqualified column reference.
        // Only add if not already present (don't overwrite explicit aliases that might shadow it).
        if !Self::is_alias_keyword(&table_name_lower)
            && !table_aliases.contains_key(&table_name_lower)
        {
            table_aliases.insert(table_name_lower, table_ref);
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
pub(crate) fn strip_sql_comments_for_body_deps(body: &str) -> String {
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
            // SQL Server scalar functions that might appear unqualified
            | "IIF"
            | "NULLIF"
            | "CHOOSE"
            | "ABS"
            | "CEILING"
            | "FLOOR"
            | "ROUND"
            | "POWER"
            | "SQRT"
            | "SIGN"
            | "RAND"
            | "NEWID"
            | "ROW_NUMBER"
            | "RANK"
            | "DENSE_RANK"
            | "NTILE"
            | "LAG"
            | "LEAD"
            | "FIRST_VALUE"
            | "LAST_VALUE"
            | "OVER"
            | "PARTITION"
            | "WITHIN"
            | "PERCENT"
            | "PERCENTILE_CONT"
            | "PERCENTILE_DISC"
            | "CUME_DIST"
            | "PERCENT_RANK"
            | "STRING_SPLIT"
            | "OPENJSON"
            | "JSON_VALUE"
            | "JSON_QUERY"
            | "JSON_MODIFY"
            | "FORMATMESSAGE"
            | "FORMAT"
            | "TRY_CAST"
            | "TRY_CONVERT"
            | "TRY_PARSE"
            | "PARSE"
            | "EOMONTH"
            | "DATEFROMPARTS"
            | "TIMEFROMPARTS"
            | "SYSDATETIME"
            | "SYSUTCDATETIME"
            | "SYSDATETIMEOFFSET"
    )
    // Intentionally excludes: TIMESTAMP, ACTION, ID, TEXT, IMAGE, DATE, TIME, etc.
    // as these are commonly used as column names
}

// =============================================================================
// CTE Definition Extraction (Phase 24.1.2)
// =============================================================================

/// Extract CTE definitions from a SQL body (procedure or view).
/// Returns a list of CTE definitions with their columns and expression dependencies.
///
/// # Arguments
/// * `sql` - The SQL body text containing WITH clauses
/// * `default_schema` - Default schema for unqualified table references
///
/// # Returns
/// Vector of CteDefinition structs, one per CTE found in the body
pub(crate) fn extract_cte_definitions(sql: &str, default_schema: &str) -> Vec<CteDefinition> {
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, sql).tokenize_with_location() {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let mut cte_defs = Vec::new();
    let mut with_block_number = 0u32; // Tracks which WITH block we're in
    let mut pos = 0;

    // First, extract table aliases for the entire body so we can resolve column references
    let mut table_aliases = HashMap::new();
    let mut subquery_aliases = HashSet::new();
    if let Some(mut parser) = TableAliasTokenParser::with_default_schema(sql, default_schema) {
        parser.extract_all_aliases(&mut table_aliases, &mut subquery_aliases);
    }

    while pos < tokens.len() {
        // Skip whitespace
        while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
            pos += 1;
        }
        if pos >= tokens.len() {
            break;
        }

        // Look for WITH keyword
        if matches!(tokens[pos].token, Token::Word(ref w) if w.keyword == Keyword::WITH) {
            pos += 1;
            with_block_number += 1;

            // Skip whitespace
            while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                pos += 1;
            }

            // Skip RECURSIVE if present
            if pos < tokens.len() {
                if let Token::Word(ref w) = tokens[pos].token {
                    if w.value.to_uppercase() == "RECURSIVE" {
                        pos += 1;
                        while pos < tokens.len()
                            && matches!(tokens[pos].token, Token::Whitespace(_))
                        {
                            pos += 1;
                        }
                    }
                }
            }

            // Parse CTE definitions in this WITH block
            loop {
                // Get CTE name
                let cte_name = match &tokens.get(pos).map(|t| &t.token) {
                    Some(Token::Word(w)) => {
                        pos += 1;
                        w.value.clone()
                    }
                    Some(Token::SingleQuotedString(s) | Token::DoubleQuotedString(s)) => {
                        pos += 1;
                        s.clone()
                    }
                    _ => break,
                };

                // Skip whitespace
                while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                    pos += 1;
                }

                // Check for optional column list: cte_name (col1, col2, ...) AS (...)
                // We skip this for now - column names come from the SELECT
                if pos < tokens.len() && matches!(tokens[pos].token, Token::LParen) {
                    // Check if next token after LParen is AS - if so, this is the query body, not a column list
                    let mut check_pos = pos + 1;
                    while check_pos < tokens.len()
                        && matches!(tokens[check_pos].token, Token::Whitespace(_))
                    {
                        check_pos += 1;
                    }
                    if check_pos < tokens.len() {
                        if let Token::Word(ref w) = tokens[check_pos].token {
                            if w.keyword != Keyword::AS && w.value.to_uppercase() != "SELECT" {
                                // This is an optional column list, skip it
                                let mut depth = 1;
                                pos += 1;
                                while pos < tokens.len() && depth > 0 {
                                    match tokens[pos].token {
                                        Token::LParen => depth += 1,
                                        Token::RParen => depth -= 1,
                                        _ => {}
                                    }
                                    pos += 1;
                                }
                            }
                        }
                    }
                }

                // Skip whitespace
                while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                    pos += 1;
                }

                // Expect AS keyword
                let is_as = pos < tokens.len()
                    && matches!(&tokens[pos].token, Token::Word(w) if w.keyword == Keyword::AS);
                if !is_as {
                    break;
                }
                pos += 1;

                // Skip whitespace
                while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                    pos += 1;
                }

                // Expect opening paren
                if pos >= tokens.len() || !matches!(tokens[pos].token, Token::LParen) {
                    break;
                }
                let cte_body_start = pos + 1;

                // Find matching closing paren to extract CTE body
                let mut depth = 1;
                pos += 1;
                while pos < tokens.len() && depth > 0 {
                    match tokens[pos].token {
                        Token::LParen => depth += 1,
                        Token::RParen => depth -= 1,
                        _ => {}
                    }
                    pos += 1;
                }
                let cte_body_end = pos - 1;

                // Extract CTE body SQL
                if cte_body_start < cte_body_end {
                    let cte_body_tokens = &tokens[cte_body_start..cte_body_end];
                    let columns = extract_cte_columns_from_tokens(
                        cte_body_tokens,
                        &table_aliases,
                        default_schema,
                    );

                    cte_defs.push(CteDefinition {
                        name: cte_name,
                        cte_number: with_block_number,
                        columns,
                    });
                }

                // Skip whitespace
                while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                    pos += 1;
                }

                // Check for comma (more CTEs in this WITH block)
                if pos < tokens.len() && matches!(tokens[pos].token, Token::Comma) {
                    pos += 1;
                    while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                        pos += 1;
                    }
                    continue;
                }

                // End of this WITH block
                break;
            }
        } else {
            pos += 1;
        }
    }

    cte_defs
}

/// Extract columns and their expression dependencies from CTE body tokens.
/// Parses the SELECT clause to find column aliases/names and their source references.
fn extract_cte_columns_from_tokens(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
    table_aliases: &HashMap<String, String>,
    default_schema: &str,
) -> Vec<CteColumn> {
    let mut columns = Vec::new();
    let mut pos = 0;

    // Skip to SELECT keyword
    while pos < tokens.len() {
        if let Token::Word(ref w) = tokens[pos].token {
            if w.keyword == Keyword::SELECT {
                pos += 1;
                break;
            }
        }
        pos += 1;
    }

    // Skip DISTINCT, TOP, etc.
    while pos < tokens.len() {
        match &tokens[pos].token {
            Token::Whitespace(_) => pos += 1,
            Token::Word(w) if w.keyword == Keyword::DISTINCT || w.keyword == Keyword::ALL => {
                pos += 1
            }
            Token::Word(w) if w.keyword == Keyword::TOP => {
                pos += 1;
                // Skip the TOP number/expression
                while pos < tokens.len() {
                    match &tokens[pos].token {
                        Token::Whitespace(_) | Token::Number(_, _) => pos += 1,
                        Token::LParen => {
                            let mut depth = 1;
                            pos += 1;
                            while pos < tokens.len() && depth > 0 {
                                match tokens[pos].token {
                                    Token::LParen => depth += 1,
                                    Token::RParen => depth -= 1,
                                    _ => {}
                                }
                                pos += 1;
                            }
                        }
                        _ => break,
                    }
                }
            }
            _ => break,
        }
    }

    // Parse column expressions until FROM
    while pos < tokens.len() {
        // Skip whitespace
        while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
            pos += 1;
        }
        if pos >= tokens.len() {
            break;
        }

        // Check for FROM (end of SELECT list)
        if let Token::Word(ref w) = tokens[pos].token {
            if w.keyword == Keyword::FROM {
                break;
            }
        }

        // Parse one column expression
        let (column_name, dependencies, new_pos) =
            parse_cte_column_expression(&tokens[pos..], table_aliases, default_schema);

        if !column_name.is_empty() {
            columns.push(CteColumn {
                name: column_name,
                expression_dependencies: dependencies,
            });
        }

        pos += new_pos;

        // Skip whitespace
        while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
            pos += 1;
        }

        // Check for comma (more columns)
        if pos < tokens.len() && matches!(tokens[pos].token, Token::Comma) {
            pos += 1;
            continue;
        }

        // Not a comma - exit the loop (no more columns)
        break;
    }

    columns
}

/// Parse a single column expression from the SELECT clause.
/// Returns (column_name, expression_dependencies, tokens_consumed)
fn parse_cte_column_expression(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
    table_aliases: &HashMap<String, String>,
    default_schema: &str,
) -> (String, Vec<String>, usize) {
    let mut pos = 0;
    let mut column_name = String::new();
    let mut dependencies = Vec::new();
    let mut current_ref_parts: Vec<String> = Vec::new();
    let mut paren_depth = 0;

    // Track potential column references as we go
    while pos < tokens.len() {
        match &tokens[pos].token {
            Token::Whitespace(_) => {
                // Flush any pending reference before whitespace
                if !current_ref_parts.is_empty() && paren_depth == 0 {
                    // Extract column name from the last part if no AS alias was used yet
                    if column_name.is_empty() {
                        column_name = current_ref_parts.last().unwrap().clone();
                    }
                    if let Some(dep) =
                        resolve_cte_column_ref(&current_ref_parts, table_aliases, default_schema)
                    {
                        if !dependencies.contains(&dep) {
                            dependencies.push(dep);
                        }
                    }
                    current_ref_parts.clear();
                }
                pos += 1;
            }
            Token::Comma => {
                // End of this column expression
                if !current_ref_parts.is_empty() {
                    // Extract column name from the last part if no AS alias was used
                    if column_name.is_empty() {
                        // Use the last part as the column name
                        column_name = current_ref_parts.last().unwrap().clone();
                    }
                    // Try to resolve as a dependency
                    if let Some(dep) =
                        resolve_cte_column_ref(&current_ref_parts, table_aliases, default_schema)
                    {
                        if !dependencies.contains(&dep) {
                            dependencies.push(dep);
                        }
                    }
                }
                break;
            }
            Token::LParen => {
                // Flush pending reference
                if !current_ref_parts.is_empty() {
                    if let Some(dep) =
                        resolve_cte_column_ref(&current_ref_parts, table_aliases, default_schema)
                    {
                        if !dependencies.contains(&dep) {
                            dependencies.push(dep);
                        }
                    }
                    current_ref_parts.clear();
                }
                paren_depth += 1;
                pos += 1;
            }
            Token::RParen => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                pos += 1;
            }
            Token::Period => {
                // Part of a qualified name - don't flush yet
                pos += 1;
            }
            Token::Word(w) => {
                if w.keyword == Keyword::FROM {
                    // End of SELECT list - flush pending
                    if !current_ref_parts.is_empty() {
                        // Extract column name from the last part if no AS alias was used
                        if column_name.is_empty() {
                            column_name = current_ref_parts.last().unwrap().clone();
                        }
                        if let Some(dep) = resolve_cte_column_ref(
                            &current_ref_parts,
                            table_aliases,
                            default_schema,
                        ) {
                            if !dependencies.contains(&dep) {
                                dependencies.push(dep);
                            }
                        }
                    }
                    break;
                } else if w.keyword == Keyword::AS && paren_depth == 0 {
                    // AS indicates the next identifier is the column alias
                    // First flush any pending reference as a dependency
                    if !current_ref_parts.is_empty() {
                        if let Some(dep) = resolve_cte_column_ref(
                            &current_ref_parts,
                            table_aliases,
                            default_schema,
                        ) {
                            if !dependencies.contains(&dep) {
                                dependencies.push(dep);
                            }
                        }
                        current_ref_parts.clear();
                    }
                    pos += 1;
                    // Skip whitespace
                    while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                        pos += 1;
                    }
                    // Get the alias name
                    if pos < tokens.len() {
                        match &tokens[pos].token {
                            Token::Word(alias_w) => {
                                column_name = alias_w.value.clone();
                                pos += 1;
                            }
                            Token::SingleQuotedString(s) | Token::DoubleQuotedString(s) => {
                                column_name = s.clone();
                                pos += 1;
                            }
                            _ => {}
                        }
                    }
                } else if is_sql_reserved_word(&w.value) && paren_depth == 0 {
                    // SQL function or keyword - flush pending reference
                    if !current_ref_parts.is_empty() {
                        if let Some(dep) = resolve_cte_column_ref(
                            &current_ref_parts,
                            table_aliases,
                            default_schema,
                        ) {
                            if !dependencies.contains(&dep) {
                                dependencies.push(dep);
                            }
                        }
                        current_ref_parts.clear();
                    }
                    pos += 1;
                } else {
                    // Regular identifier - add to current reference
                    current_ref_parts.push(w.value.clone());
                    pos += 1;
                }
            }
            Token::SingleQuotedString(_) | Token::DoubleQuotedString(_) => {
                // String literal - could be alias if after expression, or just a literal
                pos += 1;
            }
            Token::Number(_, _) => {
                // Numeric literal
                pos += 1;
            }
            _ => {
                // Operator or other token - flush pending reference
                if !current_ref_parts.is_empty() {
                    if let Some(dep) =
                        resolve_cte_column_ref(&current_ref_parts, table_aliases, default_schema)
                    {
                        if !dependencies.contains(&dep) {
                            dependencies.push(dep);
                        }
                    }
                    current_ref_parts.clear();
                }
                pos += 1;
            }
        }
    }

    // Flush any remaining reference
    if !current_ref_parts.is_empty() {
        // Extract column name from the last part if no AS alias was used
        if column_name.is_empty() {
            column_name = current_ref_parts.last().unwrap().clone();
        }
        if let Some(dep) = resolve_cte_column_ref(&current_ref_parts, table_aliases, default_schema)
        {
            if !dependencies.contains(&dep) {
                dependencies.push(dep);
            }
        }
    }

    (column_name, dependencies, pos)
}

/// Resolve a column reference to its fully qualified form.
/// Handles alias resolution and schema qualification.
fn resolve_cte_column_ref(
    parts: &[String],
    table_aliases: &HashMap<String, String>,
    default_schema: &str,
) -> Option<String> {
    if parts.is_empty() {
        return None;
    }

    match parts.len() {
        1 => {
            // Single identifier - could be a column name without table prefix
            // We can't resolve this to a specific table without more context
            None
        }
        2 => {
            // alias.column or table.column
            let alias_or_table = &parts[0];
            let column = &parts[1];
            let alias_lower = alias_or_table.to_lowercase();

            // Check if first part is a known table alias
            if let Some(table_ref) = table_aliases.get(&alias_lower) {
                // table_ref is like "[dbo].[Account]"
                Some(format!("{}.[{}]", table_ref, column))
            } else {
                // Assume it's schema.table (unlikely in CTE select) or just table.column
                // Use default schema if it looks like an unqualified table
                Some(format!(
                    "[{}].[{}].[{}]",
                    default_schema, alias_or_table, column
                ))
            }
        }
        3 => {
            // schema.table.column
            let schema = &parts[0];
            let table = &parts[1];
            let column = &parts[2];
            Some(format!("[{}].[{}].[{}]", schema, table, column))
        }
        _ => {
            // More than 3 parts - take last 3
            let len = parts.len();
            let schema = &parts[len - 3];
            let table = &parts[len - 2];
            let column = &parts[len - 1];
            Some(format!("[{}].[{}].[{}]", schema, table, column))
        }
    }
}

/// Check if a word is a SQL reserved word that should not be treated as a column reference
fn is_sql_reserved_word(word: &str) -> bool {
    let upper = word.to_uppercase();
    matches!(
        upper.as_str(),
        "SELECT"
            | "FROM"
            | "WHERE"
            | "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "IS"
            | "IN"
            | "BETWEEN"
            | "LIKE"
            | "EXISTS"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
            | "CAST"
            | "CONVERT"
            | "COALESCE"
            | "NULLIF"
            | "IIF"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "ROW_NUMBER"
            | "RANK"
            | "DENSE_RANK"
            | "OVER"
            | "PARTITION"
            | "ORDER"
            | "BY"
            | "ASC"
            | "DESC"
            | "INNER"
            | "LEFT"
            | "RIGHT"
            | "FULL"
            | "OUTER"
            | "CROSS"
            | "JOIN"
            | "ON"
            | "GROUP"
            | "HAVING"
            | "UNION"
            | "ALL"
            | "DISTINCT"
            | "TOP"
            | "STUFF"
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
    )
}

// =============================================================================
// Temp Table Definition Extraction (Phase 24.2.1)
// =============================================================================

/// Extract temp table definitions from a SQL body (procedure or function).
/// Returns a list of temp table definitions with their columns.
///
/// # Arguments
/// * `sql` - The SQL body text containing CREATE TABLE #... statements
///
/// # Returns
/// Vector of TempTableDefinition structs, one per temp table found in the body
pub(crate) fn extract_temp_table_definitions(sql: &str) -> Vec<TempTableDefinition> {
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, sql).tokenize_with_location() {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let mut temp_tables = Vec::new();
    let mut temp_table_number = 0u32;
    let mut pos = 0;

    while pos < tokens.len() {
        // Skip whitespace
        while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
            pos += 1;
        }
        if pos >= tokens.len() {
            break;
        }

        // Look for CREATE keyword
        if matches!(tokens[pos].token, Token::Word(ref w) if w.keyword == Keyword::CREATE) {
            pos += 1;

            // Skip whitespace
            while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                pos += 1;
            }

            // Look for TABLE keyword
            if pos < tokens.len()
                && matches!(tokens[pos].token, Token::Word(ref w) if w.keyword == Keyword::TABLE)
            {
                pos += 1;

                // Skip whitespace
                while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                    pos += 1;
                }

                // Check for temp table name starting with # (using Hashtag token or Word starting with #)
                let temp_name = extract_temp_table_name(&tokens, &mut pos);
                if let Some(name) = temp_name {
                    // Skip whitespace
                    while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                        pos += 1;
                    }

                    // Expect opening paren for column definitions
                    if pos < tokens.len() && matches!(tokens[pos].token, Token::LParen) {
                        let columns_start = pos + 1;

                        // Find matching closing paren
                        let mut depth = 1;
                        pos += 1;
                        while pos < tokens.len() && depth > 0 {
                            match tokens[pos].token {
                                Token::LParen => depth += 1,
                                Token::RParen => depth -= 1,
                                _ => {}
                            }
                            pos += 1;
                        }
                        let columns_end = pos - 1;

                        // Extract columns from the definition
                        if columns_start < columns_end {
                            let column_tokens = &tokens[columns_start..columns_end];
                            let columns = extract_temp_table_columns(column_tokens);

                            if !columns.is_empty() {
                                temp_table_number += 1;
                                temp_tables.push(TempTableDefinition {
                                    name,
                                    temp_table_number,
                                    columns,
                                });
                            }
                        }
                    }
                }
            }
        } else {
            pos += 1;
        }
    }

    temp_tables
}

/// Extract temp table name from tokens (handles #name and ##name patterns)
fn extract_temp_table_name(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
    pos: &mut usize,
) -> Option<String> {
    if *pos >= tokens.len() {
        return None;
    }

    // MsSqlDialect tokenizes temp table names like #TempOrders as a single Word token
    // with the # included in the value
    if let Token::Word(w) = &tokens[*pos].token {
        if w.value.starts_with('#') {
            *pos += 1;
            return Some(w.value.clone());
        }
    }

    None
}

/// Extract column definitions from temp table CREATE TABLE tokens
fn extract_temp_table_columns(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
) -> Vec<TempTableColumn> {
    let mut columns = Vec::new();
    let mut pos = 0;

    while pos < tokens.len() {
        // Skip whitespace
        while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
            pos += 1;
        }
        if pos >= tokens.len() {
            break;
        }

        // Check for CONSTRAINT keyword (skip constraint definitions)
        if matches!(&tokens[pos].token, Token::Word(w) if w.keyword == Keyword::CONSTRAINT) {
            // Skip to next comma or end
            while pos < tokens.len() && !matches!(tokens[pos].token, Token::Comma) {
                // Handle nested parentheses in constraint definitions
                if matches!(tokens[pos].token, Token::LParen) {
                    let mut depth = 1;
                    pos += 1;
                    while pos < tokens.len() && depth > 0 {
                        match tokens[pos].token {
                            Token::LParen => depth += 1,
                            Token::RParen => depth -= 1,
                            _ => {}
                        }
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
            // Skip comma
            if pos < tokens.len() && matches!(tokens[pos].token, Token::Comma) {
                pos += 1;
            }
            continue;
        }

        // Check for PRIMARY, FOREIGN, UNIQUE, CHECK keywords (table-level constraints)
        if matches!(&tokens[pos].token, Token::Word(w) if matches!(w.keyword,
            Keyword::PRIMARY | Keyword::FOREIGN | Keyword::UNIQUE | Keyword::CHECK))
        {
            // Skip to next comma or end
            while pos < tokens.len() && !matches!(tokens[pos].token, Token::Comma) {
                if matches!(tokens[pos].token, Token::LParen) {
                    let mut depth = 1;
                    pos += 1;
                    while pos < tokens.len() && depth > 0 {
                        match tokens[pos].token {
                            Token::LParen => depth += 1,
                            Token::RParen => depth -= 1,
                            _ => {}
                        }
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
            if pos < tokens.len() && matches!(tokens[pos].token, Token::Comma) {
                pos += 1;
            }
            continue;
        }

        // Try to extract column name
        let column_name = match &tokens[pos].token {
            Token::Word(w) => {
                // Skip if it's a keyword that starts a constraint
                if matches!(
                    w.keyword,
                    Keyword::PRIMARY
                        | Keyword::FOREIGN
                        | Keyword::UNIQUE
                        | Keyword::CHECK
                        | Keyword::INDEX
                        | Keyword::CONSTRAINT
                ) {
                    pos += 1;
                    continue;
                }
                pos += 1;
                w.value.clone()
            }
            Token::SingleQuotedString(s) | Token::DoubleQuotedString(s) => {
                pos += 1;
                s.clone()
            }
            _ => {
                pos += 1;
                continue;
            }
        };

        // Skip whitespace
        while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
            pos += 1;
        }

        // Extract data type
        let (data_type, new_pos) = extract_column_data_type(tokens, pos);
        pos = new_pos;

        // Look for NULL/NOT NULL and skip to end of column definition
        let mut is_nullable = true;
        while pos < tokens.len() && !matches!(tokens[pos].token, Token::Comma) {
            match &tokens[pos].token {
                Token::Word(w) if w.keyword == Keyword::NOT => {
                    // Check if followed by NULL
                    let mut check_pos = pos + 1;
                    while check_pos < tokens.len()
                        && matches!(tokens[check_pos].token, Token::Whitespace(_))
                    {
                        check_pos += 1;
                    }
                    if check_pos < tokens.len() {
                        if let Token::Word(w2) = &tokens[check_pos].token {
                            if w2.keyword == Keyword::NULL {
                                is_nullable = false;
                                pos = check_pos + 1;
                                continue;
                            }
                        }
                    }
                }
                Token::Word(w) if w.keyword == Keyword::NULL => {
                    is_nullable = true;
                }
                Token::LParen => {
                    // Skip nested parentheses (e.g., DEFAULT expressions, constraints)
                    let mut depth = 1;
                    pos += 1;
                    while pos < tokens.len() && depth > 0 {
                        match tokens[pos].token {
                            Token::LParen => depth += 1,
                            Token::RParen => depth -= 1,
                            _ => {}
                        }
                        pos += 1;
                    }
                    continue;
                }
                _ => {}
            }
            pos += 1;
        }

        // Add column if we have a data type
        if !data_type.is_empty() {
            columns.push(TempTableColumn {
                name: column_name,
                data_type,
                is_nullable,
            });
        }

        // Skip comma
        if pos < tokens.len() && matches!(tokens[pos].token, Token::Comma) {
            pos += 1;
        }
    }

    columns
}

/// Extract data type from column definition tokens
fn extract_column_data_type(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
    start_pos: usize,
) -> (String, usize) {
    let mut pos = start_pos;
    let mut type_parts = Vec::new();

    // Get base type name (could be multi-word like "NATIONAL CHARACTER VARYING")
    while pos < tokens.len() {
        match &tokens[pos].token {
            Token::Whitespace(_) => {
                pos += 1;
            }
            Token::Word(w) => {
                // Check if this is a type name or a modifier
                let upper = w.value.to_uppercase();
                if is_data_type_name(&upper) || type_parts.is_empty() {
                    type_parts.push(w.value.clone());
                    pos += 1;
                } else {
                    // Not a type name, stop here
                    break;
                }
            }
            Token::LParen => {
                // Type parameters like varchar(50), decimal(18,2)
                let mut params = String::from("(");
                pos += 1;
                let mut depth = 1;
                while pos < tokens.len() && depth > 0 {
                    match &tokens[pos].token {
                        Token::LParen => {
                            depth += 1;
                            params.push('(');
                        }
                        Token::RParen => {
                            depth -= 1;
                            if depth > 0 {
                                params.push(')');
                            }
                        }
                        Token::Comma => params.push(','),
                        Token::Number(n, _) => params.push_str(n),
                        Token::Word(w) => {
                            // Handle MAX keyword
                            if w.value.to_uppercase() == "MAX" {
                                params.push_str("MAX");
                            } else {
                                params.push_str(&w.value);
                            }
                        }
                        Token::Whitespace(_) => {}
                        _ => {}
                    }
                    pos += 1;
                }
                params.push(')');
                if let Some(last) = type_parts.last_mut() {
                    last.push_str(&params);
                }
                break;
            }
            Token::Period => {
                // Schema-qualified type like [dbo].[MyType]
                pos += 1;
            }
            _ => break,
        }
    }

    (type_parts.join(" "), pos)
}

/// Check if a word is a SQL data type name
fn is_data_type_name(word: &str) -> bool {
    matches!(
        word,
        "INT"
            | "INTEGER"
            | "BIGINT"
            | "SMALLINT"
            | "TINYINT"
            | "BIT"
            | "DECIMAL"
            | "NUMERIC"
            | "MONEY"
            | "SMALLMONEY"
            | "FLOAT"
            | "REAL"
            | "CHAR"
            | "VARCHAR"
            | "NCHAR"
            | "NVARCHAR"
            | "TEXT"
            | "NTEXT"
            | "BINARY"
            | "VARBINARY"
            | "IMAGE"
            | "DATE"
            | "TIME"
            | "DATETIME"
            | "DATETIME2"
            | "DATETIMEOFFSET"
            | "SMALLDATETIME"
            | "TIMESTAMP"
            | "UNIQUEIDENTIFIER"
            | "XML"
            | "SQL_VARIANT"
            | "GEOGRAPHY"
            | "GEOMETRY"
            | "HIERARCHYID"
            | "SYSNAME"
            | "NATIONAL"
            | "CHARACTER"
            | "VARYING"
    )
}

// =============================================================================
// Table Variable Definition Extraction (Phase 24.3.1)
// =============================================================================

/// Extract table variable definitions from a SQL body (procedure or function).
/// Returns a list of table variable definitions with their columns.
///
/// Pattern: DECLARE @name TABLE (column definitions)
///
/// # Arguments
/// * `sql` - The SQL body text containing DECLARE @name TABLE statements
///
/// # Returns
/// Vector of TableVariableDefinition structs, one per table variable found in the body
pub(crate) fn extract_table_variable_definitions(sql: &str) -> Vec<TableVariableDefinition> {
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, sql).tokenize_with_location() {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let mut table_variables = Vec::new();
    let mut table_variable_number = 0u32;
    let mut pos = 0;

    while pos < tokens.len() {
        // Skip whitespace
        while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
            pos += 1;
        }
        if pos >= tokens.len() {
            break;
        }

        // Look for DECLARE keyword
        if matches!(tokens[pos].token, Token::Word(ref w) if w.keyword == Keyword::DECLARE) {
            pos += 1;

            // Skip whitespace
            while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                pos += 1;
            }

            // Look for @name pattern (variable name starting with @)
            let var_name = extract_table_variable_name(&tokens, &mut pos);
            if let Some(name) = var_name {
                // Skip whitespace
                while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                    pos += 1;
                }

                // Check for TABLE keyword
                if pos < tokens.len()
                    && matches!(tokens[pos].token, Token::Word(ref w) if w.keyword == Keyword::TABLE)
                {
                    pos += 1;

                    // Skip whitespace
                    while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
                        pos += 1;
                    }

                    // Expect opening paren for column definitions
                    if pos < tokens.len() && matches!(tokens[pos].token, Token::LParen) {
                        let columns_start = pos + 1;

                        // Find matching closing paren
                        let mut depth = 1;
                        pos += 1;
                        while pos < tokens.len() && depth > 0 {
                            match tokens[pos].token {
                                Token::LParen => depth += 1,
                                Token::RParen => depth -= 1,
                                _ => {}
                            }
                            pos += 1;
                        }
                        let columns_end = pos - 1;

                        // Extract columns from the definition
                        if columns_start < columns_end {
                            let column_tokens = &tokens[columns_start..columns_end];
                            let columns = extract_table_variable_columns(column_tokens);

                            if !columns.is_empty() {
                                table_variable_number += 1;
                                table_variables.push(TableVariableDefinition {
                                    name,
                                    table_variable_number,
                                    columns,
                                });
                            }
                        }
                    }
                }
            }
        } else {
            pos += 1;
        }
    }

    table_variables
}

/// Extract table variable name from tokens (handles @name pattern)
fn extract_table_variable_name(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
    pos: &mut usize,
) -> Option<String> {
    if *pos >= tokens.len() {
        return None;
    }

    // MsSqlDialect tokenizes @name as a single Word token with @ included
    // But sometimes it can be tokenized as separate tokens
    if let Token::Word(w) = &tokens[*pos].token {
        if w.value.starts_with('@') {
            *pos += 1;
            return Some(w.value.clone());
        }
    }

    // Handle case where @ might be a separate token (Placeholder token)
    if matches!(tokens[*pos].token, Token::Placeholder(_)) {
        // Get the placeholder value
        if let Token::Placeholder(ref p) = tokens[*pos].token {
            if p.starts_with('@') {
                *pos += 1;
                return Some(p.clone());
            }
        }
    }

    None
}

/// Extract column definitions from table variable DECLARE TABLE tokens
/// This follows the same pattern as extract_temp_table_columns
fn extract_table_variable_columns(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
) -> Vec<TableVariableColumn> {
    let mut columns = Vec::new();
    let mut pos = 0;

    while pos < tokens.len() {
        // Skip whitespace
        while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
            pos += 1;
        }
        if pos >= tokens.len() {
            break;
        }

        // Check for CONSTRAINT keyword (skip constraint definitions)
        if matches!(&tokens[pos].token, Token::Word(w) if w.keyword == Keyword::CONSTRAINT) {
            // Skip to next comma or end
            while pos < tokens.len() && !matches!(tokens[pos].token, Token::Comma) {
                // Handle nested parentheses in constraint definitions
                if matches!(tokens[pos].token, Token::LParen) {
                    let mut depth = 1;
                    pos += 1;
                    while pos < tokens.len() && depth > 0 {
                        match tokens[pos].token {
                            Token::LParen => depth += 1,
                            Token::RParen => depth -= 1,
                            _ => {}
                        }
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
            // Skip comma
            if pos < tokens.len() && matches!(tokens[pos].token, Token::Comma) {
                pos += 1;
            }
            continue;
        }

        // Check for PRIMARY, FOREIGN, UNIQUE, CHECK keywords (table-level constraints)
        if matches!(&tokens[pos].token, Token::Word(w) if matches!(w.keyword,
            Keyword::PRIMARY | Keyword::FOREIGN | Keyword::UNIQUE | Keyword::CHECK))
        {
            // Skip to next comma or end
            while pos < tokens.len() && !matches!(tokens[pos].token, Token::Comma) {
                if matches!(tokens[pos].token, Token::LParen) {
                    let mut depth = 1;
                    pos += 1;
                    while pos < tokens.len() && depth > 0 {
                        match tokens[pos].token {
                            Token::LParen => depth += 1,
                            Token::RParen => depth -= 1,
                            _ => {}
                        }
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
            if pos < tokens.len() && matches!(tokens[pos].token, Token::Comma) {
                pos += 1;
            }
            continue;
        }

        // Try to extract column name
        let column_name = match &tokens[pos].token {
            Token::Word(w) => {
                // Skip if it's a keyword that starts a constraint
                if matches!(
                    w.keyword,
                    Keyword::PRIMARY
                        | Keyword::FOREIGN
                        | Keyword::UNIQUE
                        | Keyword::CHECK
                        | Keyword::INDEX
                        | Keyword::CONSTRAINT
                ) {
                    pos += 1;
                    continue;
                }
                pos += 1;
                w.value.clone()
            }
            Token::SingleQuotedString(s) | Token::DoubleQuotedString(s) => {
                pos += 1;
                s.clone()
            }
            _ => {
                pos += 1;
                continue;
            }
        };

        // Skip whitespace
        while pos < tokens.len() && matches!(tokens[pos].token, Token::Whitespace(_)) {
            pos += 1;
        }

        // Extract data type (reuse the same function as temp tables)
        let (data_type, new_pos) = extract_column_data_type(tokens, pos);
        pos = new_pos;

        // Look for NULL/NOT NULL and skip to end of column definition
        let mut is_nullable = true;
        while pos < tokens.len() && !matches!(tokens[pos].token, Token::Comma) {
            match &tokens[pos].token {
                Token::Word(w) if w.keyword == Keyword::NOT => {
                    // Check if followed by NULL
                    let mut check_pos = pos + 1;
                    while check_pos < tokens.len()
                        && matches!(tokens[check_pos].token, Token::Whitespace(_))
                    {
                        check_pos += 1;
                    }
                    if check_pos < tokens.len() {
                        if let Token::Word(w2) = &tokens[check_pos].token {
                            if w2.keyword == Keyword::NULL {
                                is_nullable = false;
                                pos = check_pos + 1;
                                continue;
                            }
                        }
                    }
                }
                Token::Word(w) if w.keyword == Keyword::NULL => {
                    is_nullable = true;
                }
                Token::LParen => {
                    // Skip nested parentheses (e.g., DEFAULT expressions, constraints)
                    let mut depth = 1;
                    pos += 1;
                    while pos < tokens.len() && depth > 0 {
                        match tokens[pos].token {
                            Token::LParen => depth += 1,
                            Token::RParen => depth -= 1,
                            _ => {}
                        }
                        pos += 1;
                    }
                    continue;
                }
                _ => {}
            }
            pos += 1;
        }

        // Add column if we have a data type
        if !data_type.is_empty() {
            columns.push(TableVariableColumn {
                name: column_name,
                data_type,
                is_nullable,
            });
        }

        // Skip comma
        if pos < tokens.len() && matches!(tokens[pos].token, Token::Comma) {
            pos += 1;
        }
    }

    columns
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create an empty ColumnRegistry for tests that don't need schema-aware resolution
    fn empty_registry() -> ColumnRegistry {
        ColumnRegistry::new()
    }

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

    // ============================================================================
    // CTE extraction tests (Phase 24.1.2)
    // ============================================================================

    #[test]
    fn test_extract_cte_definitions_single_cte() {
        let sql = r#"
            WITH AccountCte AS (
                SELECT A.Id, A.Name
                FROM [dbo].[Account] A
            )
            SELECT * FROM AccountCte
        "#;
        let cte_defs = extract_cte_definitions(sql, "dbo");
        assert_eq!(cte_defs.len(), 1, "Expected 1 CTE, got {:?}", cte_defs);
        assert_eq!(cte_defs[0].name, "AccountCte");
        assert_eq!(cte_defs[0].cte_number, 1);
        assert_eq!(
            cte_defs[0].columns.len(),
            2,
            "Expected 2 columns, got {:?}",
            cte_defs[0].columns
        );
        assert_eq!(cte_defs[0].columns[0].name, "Id");
        assert_eq!(cte_defs[0].columns[1].name, "Name");
    }

    #[test]
    fn test_extract_cte_definitions_multiple_ctes_same_with() {
        let sql = r#"
            WITH TagCte AS (
                SELECT T.Id, T.Name FROM [dbo].[Tag] T
            ),
            AccountTagCte AS (
                SELECT AT.AccountId, AT.TagId FROM [dbo].[AccountTag] AT
            )
            SELECT * FROM TagCte
        "#;
        let cte_defs = extract_cte_definitions(sql, "dbo");
        assert_eq!(cte_defs.len(), 2);
        assert_eq!(cte_defs[0].name, "TagCte");
        assert_eq!(cte_defs[0].cte_number, 1);
        assert_eq!(cte_defs[1].name, "AccountTagCte");
        assert_eq!(cte_defs[1].cte_number, 1); // Same WITH block
    }

    #[test]
    fn test_extract_cte_definitions_multiple_with_blocks() {
        let sql = r#"
            WITH Cte1 AS (SELECT 1 AS A)
            SELECT * FROM Cte1;

            WITH Cte2 AS (SELECT 2 AS B),
            Cte3 AS (SELECT 3 AS C)
            SELECT * FROM Cte2;
        "#;
        let cte_defs = extract_cte_definitions(sql, "dbo");
        assert_eq!(cte_defs.len(), 3);
        assert_eq!(cte_defs[0].name, "Cte1");
        assert_eq!(cte_defs[0].cte_number, 1);
        assert_eq!(cte_defs[1].name, "Cte2");
        assert_eq!(cte_defs[1].cte_number, 2); // Second WITH block
        assert_eq!(cte_defs[2].name, "Cte3");
        assert_eq!(cte_defs[2].cte_number, 2); // Same second WITH block
    }

    #[test]
    fn test_extract_cte_definitions_no_cte() {
        let sql = "SELECT * FROM [dbo].[Account]";
        let cte_defs = extract_cte_definitions(sql, "dbo");
        assert!(cte_defs.is_empty());
    }

    #[test]
    fn test_extract_cte_definitions_column_with_alias() {
        let sql = r#"
            WITH Cte AS (
                SELECT A.Id AS AccountId, A.Name AS AccountName
                FROM [dbo].[Account] A
            )
            SELECT * FROM Cte
        "#;
        let cte_defs = extract_cte_definitions(sql, "dbo");
        assert_eq!(cte_defs.len(), 1);
        // Aliased columns should use the alias name
        let col_names: Vec<_> = cte_defs[0].columns.iter().map(|c| &c.name).collect();
        assert!(col_names.contains(&&"AccountId".to_string()));
        assert!(col_names.contains(&&"AccountName".to_string()));
    }

    // ============================================================================
    // Temp Table Extraction tests (Phase 24.2)
    // ============================================================================

    #[test]
    fn test_extract_temp_table_single_table() {
        let sql = r#"
            CREATE TABLE #TempOrders (
                OrderId INT NOT NULL,
                CustomerId INT,
                OrderDate DATETIME
            )
        "#;
        let temp_tables = extract_temp_table_definitions(sql);
        assert_eq!(temp_tables.len(), 1);
        assert_eq!(temp_tables[0].name, "#TempOrders");
        assert_eq!(temp_tables[0].temp_table_number, 1);
        assert_eq!(temp_tables[0].columns.len(), 3);

        assert_eq!(temp_tables[0].columns[0].name, "OrderId");
        assert_eq!(temp_tables[0].columns[0].data_type, "INT");
        assert!(!temp_tables[0].columns[0].is_nullable);

        assert_eq!(temp_tables[0].columns[1].name, "CustomerId");
        assert_eq!(temp_tables[0].columns[1].data_type, "INT");
        assert!(temp_tables[0].columns[1].is_nullable);

        assert_eq!(temp_tables[0].columns[2].name, "OrderDate");
        assert_eq!(temp_tables[0].columns[2].data_type, "DATETIME");
        assert!(temp_tables[0].columns[2].is_nullable);
    }

    #[test]
    fn test_extract_temp_table_with_varchar_lengths() {
        let sql = r#"
            CREATE TABLE #TempCustomers (
                Id INT,
                Name VARCHAR(100),
                Email NVARCHAR(255) NOT NULL,
                Description NVARCHAR(MAX)
            )
        "#;
        let temp_tables = extract_temp_table_definitions(sql);
        assert_eq!(temp_tables.len(), 1);
        assert_eq!(temp_tables[0].columns.len(), 4);

        assert_eq!(temp_tables[0].columns[1].name, "Name");
        assert_eq!(temp_tables[0].columns[1].data_type, "VARCHAR(100)");

        assert_eq!(temp_tables[0].columns[2].name, "Email");
        assert_eq!(temp_tables[0].columns[2].data_type, "NVARCHAR(255)");
        assert!(!temp_tables[0].columns[2].is_nullable);

        assert_eq!(temp_tables[0].columns[3].name, "Description");
        assert_eq!(temp_tables[0].columns[3].data_type, "NVARCHAR(MAX)");
    }

    #[test]
    fn test_extract_temp_table_with_decimal() {
        let sql = r#"
            CREATE TABLE #TempAmounts (
                Id INT,
                Amount DECIMAL(18,2) NOT NULL,
                Quantity NUMERIC(10,4)
            )
        "#;
        let temp_tables = extract_temp_table_definitions(sql);
        assert_eq!(temp_tables.len(), 1);
        assert_eq!(temp_tables[0].columns.len(), 3);

        assert_eq!(temp_tables[0].columns[1].name, "Amount");
        assert_eq!(temp_tables[0].columns[1].data_type, "DECIMAL(18,2)");
        assert!(!temp_tables[0].columns[1].is_nullable);

        assert_eq!(temp_tables[0].columns[2].name, "Quantity");
        assert_eq!(temp_tables[0].columns[2].data_type, "NUMERIC(10,4)");
    }

    #[test]
    fn test_extract_temp_table_multiple_tables() {
        let sql = r#"
            CREATE TABLE #TempA (
                Id INT
            )

            SELECT * FROM #TempA

            CREATE TABLE #TempB (
                Name VARCHAR(50)
            )
        "#;
        let temp_tables = extract_temp_table_definitions(sql);
        assert_eq!(temp_tables.len(), 2);
        assert_eq!(temp_tables[0].name, "#TempA");
        assert_eq!(temp_tables[0].temp_table_number, 1);
        assert_eq!(temp_tables[1].name, "#TempB");
        assert_eq!(temp_tables[1].temp_table_number, 2);
    }

    #[test]
    fn test_extract_temp_table_global_temp() {
        let sql = r#"
            CREATE TABLE ##GlobalTemp (
                Id INT,
                Value FLOAT
            )
        "#;
        let temp_tables = extract_temp_table_definitions(sql);
        assert_eq!(temp_tables.len(), 1);
        assert_eq!(temp_tables[0].name, "##GlobalTemp");
        assert_eq!(temp_tables[0].columns.len(), 2);
    }

    #[test]
    fn test_extract_temp_table_no_temp_table() {
        let sql = r#"
            SELECT * FROM [dbo].[Orders]
            WHERE OrderDate > GETDATE()
        "#;
        let temp_tables = extract_temp_table_definitions(sql);
        assert!(temp_tables.is_empty());
    }

    #[test]
    fn test_extract_temp_table_with_constraint() {
        let sql = r#"
            CREATE TABLE #TempWithPK (
                Id INT NOT NULL,
                Name VARCHAR(50),
                CONSTRAINT PK_Temp PRIMARY KEY (Id)
            )
        "#;
        let temp_tables = extract_temp_table_definitions(sql);
        assert_eq!(temp_tables.len(), 1);
        // Should only have 2 columns, not the constraint
        assert_eq!(temp_tables[0].columns.len(), 2);
        assert_eq!(temp_tables[0].columns[0].name, "Id");
        assert_eq!(temp_tables[0].columns[1].name, "Name");
    }

    #[test]
    fn test_extract_temp_table_with_primary_key_inline() {
        let sql = r#"
            CREATE TABLE #TempWithPK (
                Id INT NOT NULL PRIMARY KEY,
                Name VARCHAR(50)
            )
        "#;
        let temp_tables = extract_temp_table_definitions(sql);
        assert_eq!(temp_tables.len(), 1);
        assert_eq!(temp_tables[0].columns.len(), 2);
        // First column should still be extracted even with inline PRIMARY KEY
        assert_eq!(temp_tables[0].columns[0].name, "Id");
        assert!(!temp_tables[0].columns[0].is_nullable);
    }

    // ============================================================================
    // Table Variable Extraction tests (Phase 24.3)
    // ============================================================================

    #[test]
    fn test_extract_table_variable_single_table() {
        let sql = r#"
            DECLARE @OrderItems TABLE (
                ItemId INT NOT NULL,
                ProductId INT,
                Quantity INT
            )
        "#;
        let table_vars = extract_table_variable_definitions(sql);
        assert_eq!(table_vars.len(), 1);
        assert_eq!(table_vars[0].name, "@OrderItems");
        assert_eq!(table_vars[0].table_variable_number, 1);
        assert_eq!(table_vars[0].columns.len(), 3);

        assert_eq!(table_vars[0].columns[0].name, "ItemId");
        assert_eq!(table_vars[0].columns[0].data_type, "INT");
        assert!(!table_vars[0].columns[0].is_nullable);

        assert_eq!(table_vars[0].columns[1].name, "ProductId");
        assert_eq!(table_vars[0].columns[1].data_type, "INT");
        assert!(table_vars[0].columns[1].is_nullable);

        assert_eq!(table_vars[0].columns[2].name, "Quantity");
        assert_eq!(table_vars[0].columns[2].data_type, "INT");
        assert!(table_vars[0].columns[2].is_nullable);
    }

    #[test]
    fn test_extract_table_variable_with_varchar_lengths() {
        let sql = r#"
            DECLARE @Results TABLE (
                Id INT,
                Name VARCHAR(100),
                Email NVARCHAR(255) NOT NULL,
                Notes NVARCHAR(MAX)
            )
        "#;
        let table_vars = extract_table_variable_definitions(sql);
        assert_eq!(table_vars.len(), 1);
        assert_eq!(table_vars[0].columns.len(), 4);

        assert_eq!(table_vars[0].columns[1].name, "Name");
        assert_eq!(table_vars[0].columns[1].data_type, "VARCHAR(100)");

        assert_eq!(table_vars[0].columns[2].name, "Email");
        assert_eq!(table_vars[0].columns[2].data_type, "NVARCHAR(255)");
        assert!(!table_vars[0].columns[2].is_nullable);

        assert_eq!(table_vars[0].columns[3].name, "Notes");
        assert_eq!(table_vars[0].columns[3].data_type, "NVARCHAR(MAX)");
    }

    #[test]
    fn test_extract_table_variable_with_decimal() {
        let sql = r#"
            DECLARE @Amounts TABLE (
                Id INT,
                Amount DECIMAL(18,2) NOT NULL,
                Rate NUMERIC(10,4)
            )
        "#;
        let table_vars = extract_table_variable_definitions(sql);
        assert_eq!(table_vars.len(), 1);
        assert_eq!(table_vars[0].columns.len(), 3);

        assert_eq!(table_vars[0].columns[1].name, "Amount");
        assert_eq!(table_vars[0].columns[1].data_type, "DECIMAL(18,2)");
        assert!(!table_vars[0].columns[1].is_nullable);

        assert_eq!(table_vars[0].columns[2].name, "Rate");
        assert_eq!(table_vars[0].columns[2].data_type, "NUMERIC(10,4)");
    }

    #[test]
    fn test_extract_table_variable_multiple_variables() {
        let sql = r#"
            DECLARE @First TABLE (
                Id INT
            )

            SELECT * FROM @First

            DECLARE @Second TABLE (
                Name VARCHAR(50)
            )
        "#;
        let table_vars = extract_table_variable_definitions(sql);
        assert_eq!(table_vars.len(), 2);
        assert_eq!(table_vars[0].name, "@First");
        assert_eq!(table_vars[0].table_variable_number, 1);
        assert_eq!(table_vars[1].name, "@Second");
        assert_eq!(table_vars[1].table_variable_number, 2);
    }

    #[test]
    fn test_extract_table_variable_no_table_variable() {
        let sql = r#"
            SELECT * FROM [dbo].[Orders]
            WHERE OrderDate > GETDATE()
        "#;
        let table_vars = extract_table_variable_definitions(sql);
        assert!(table_vars.is_empty());
    }

    #[test]
    fn test_extract_table_variable_with_constraint() {
        let sql = r#"
            DECLARE @Items TABLE (
                Id INT NOT NULL,
                Name VARCHAR(50),
                CONSTRAINT PK_Items PRIMARY KEY (Id)
            )
        "#;
        let table_vars = extract_table_variable_definitions(sql);
        assert_eq!(table_vars.len(), 1);
        // Should only have 2 columns, not the constraint
        assert_eq!(table_vars[0].columns.len(), 2);
        assert_eq!(table_vars[0].columns[0].name, "Id");
        assert_eq!(table_vars[0].columns[1].name, "Name");
    }

    #[test]
    fn test_extract_table_variable_with_primary_key_inline() {
        let sql = r#"
            DECLARE @Items TABLE (
                Id INT NOT NULL PRIMARY KEY,
                Name VARCHAR(50)
            )
        "#;
        let table_vars = extract_table_variable_definitions(sql);
        assert_eq!(table_vars.len(), 1);
        assert_eq!(table_vars[0].columns.len(), 2);
        // First column should still be extracted even with inline PRIMARY KEY
        assert_eq!(table_vars[0].columns[0].name, "Id");
        assert!(!table_vars[0].columns[0].is_nullable);
    }

    #[test]
    fn test_extract_table_variable_mixed_with_regular_declare() {
        // Ensure we don't confuse regular DECLARE statements with table variables
        let sql = r#"
            DECLARE @MyInt INT
            DECLARE @MyTable TABLE (
                Id INT,
                Value VARCHAR(50)
            )
            DECLARE @MyString NVARCHAR(100)
        "#;
        let table_vars = extract_table_variable_definitions(sql);
        assert_eq!(table_vars.len(), 1);
        assert_eq!(table_vars[0].name, "@MyTable");
        assert_eq!(table_vars[0].columns.len(), 2);
    }

    // ============================================================================
    // Phase 34: APPLY subquery scope tests
    // ============================================================================

    #[test]
    fn test_extract_apply_scopes_cross_apply() {
        let sql = r#"
            SELECT a.Id
            FROM [dbo].[Account] a
            CROSS APPLY (
                SELECT COUNT(*) AS TagCount
                FROM [dbo].[AccountTag]
                WHERE AccountId = a.Id
            ) d
        "#;
        let scopes = extract_all_subquery_scopes(sql);
        assert_eq!(scopes.len(), 1, "Should find 1 APPLY scope");
        assert_eq!(
            scopes[0].tables,
            vec!["[dbo].[AccountTag]".to_string()],
            "APPLY scope should contain AccountTag table"
        );
    }

    #[test]
    fn test_extract_apply_scopes_outer_apply() {
        let sql = r#"
            SELECT a.Id
            FROM [dbo].[Account] a
            OUTER APPLY (
                SELECT TOP 1 tag.[Name]
                FROM [dbo].[AccountTag] at
                INNER JOIN [dbo].[Tag] tag ON at.TagId = tag.Id
                WHERE at.AccountId = a.Id
            ) t
        "#;
        let scopes = extract_all_subquery_scopes(sql);
        assert_eq!(scopes.len(), 1, "Should find 1 APPLY scope");
        assert!(
            scopes[0].tables.contains(&"[dbo].[AccountTag]".to_string()),
            "APPLY scope should contain AccountTag"
        );
        assert!(
            scopes[0].tables.contains(&"[dbo].[Tag]".to_string()),
            "APPLY scope should contain Tag"
        );
    }

    #[test]
    fn test_apply_subquery_unqualified_column_resolution() {
        // Test that unqualified columns inside APPLY subqueries resolve to inner table
        let sql = r#"
            SELECT a.Id
            FROM [dbo].[Account] a
            CROSS APPLY (
                SELECT COUNT(*) AS TagCount
                FROM [dbo].[AccountTag]
                WHERE AccountId = a.Id
            ) d
        "#;
        let deps = extract_body_dependencies(sql, "[dbo].[TestProc]", &[], &empty_registry());

        // Should have [dbo].[AccountTag].[AccountId] (AccountId resolves to inner table)
        let has_accounttag_accountid = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[AccountTag].[AccountId]",
            _ => false,
        });
        assert!(
            has_accounttag_accountid,
            "AccountId should resolve to [dbo].[AccountTag].[AccountId], not [dbo].[Account].[AccountId]. Got deps: {:?}",
            deps
        );

        // Should NOT have [dbo].[Account].[AccountId] (Account doesn't have AccountId column)
        let has_account_accountid = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Account].[AccountId]",
            _ => false,
        });
        assert!(
            !has_account_accountid,
            "Should NOT have [dbo].[Account].[AccountId] - that's a wrong resolution. Got deps: {:?}",
            deps
        );
    }

    #[test]
    fn test_apply_subquery_qualified_column_still_resolves_outer() {
        // Test that qualified columns like a.Id still resolve to outer table
        let sql = r#"
            SELECT a.Id
            FROM [dbo].[Account] a
            CROSS APPLY (
                SELECT COUNT(*) AS TagCount
                FROM [dbo].[AccountTag]
                WHERE AccountId = a.Id
            ) d
        "#;
        let deps = extract_body_dependencies(sql, "[dbo].[TestProc]", &[], &empty_registry());

        // Should have [dbo].[Account].[Id] (qualified ref a.Id resolves to outer alias)
        let has_account_id = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Account].[Id]",
            _ => false,
        });
        assert!(
            has_account_id,
            "Qualified a.Id should resolve to [dbo].[Account].[Id]. Got deps: {:?}",
            deps
        );
    }

    // ============================================================================
    // Phase 43: Scope-aware alias tracking tests
    // ============================================================================

    #[test]
    fn test_extract_all_scopes_derived_table() {
        // Test that derived tables in JOIN are detected as scopes
        let sql = r#"
            SELECT o.Id
            FROM [dbo].[Order] o
            LEFT JOIN (
                SELECT i.OrderId, COUNT(*) AS ItemCount
                FROM [dbo].[OrderItem] i
                GROUP BY i.OrderId
            ) OrderItems ON OrderItems.OrderId = o.Id
        "#;
        let scopes = extract_all_subquery_scopes(sql);
        assert_eq!(scopes.len(), 1, "Should find 1 derived table scope");
        assert!(
            scopes[0].tables.contains(&"[dbo].[OrderItem]".to_string()),
            "Derived table scope should contain OrderItem"
        );
        assert!(
            scopes[0].aliases.contains_key("i"),
            "Derived table scope should have alias 'i'"
        );
        assert_eq!(
            scopes[0].aliases.get("i"),
            Some(&"[dbo].[OrderItem]".to_string()),
            "Alias 'i' should map to [dbo].[OrderItem]"
        );
    }

    #[test]
    fn test_scope_conflict_same_alias_different_scopes() {
        // Test the example from the implementation plan: 't' used for both Tag and Task
        let sql = r#"
            SELECT
                OrderTags.TagList,
                OrderItems.ItemCount
            FROM [dbo].[Order] o
            -- First derived table: 't' aliases [dbo].[Tag]
            LEFT JOIN (
                SELECT ot.OrderId, t.[Name]
                FROM [dbo].[OrderTag] ot
                INNER JOIN [dbo].[Tag] t ON ot.TagId = t.Id
            ) OrderTags ON OrderTags.OrderId = o.Id
            -- Second derived table: 't' aliases [dbo].[Task]
            LEFT JOIN (
                SELECT i.OrderId, COUNT(*) AS ItemCount
                FROM [dbo].[OrderItem] i
                INNER JOIN [dbo].[Task] t ON t.Id = i.TaskId
                WHERE t.IsActive = 1
                GROUP BY i.OrderId
            ) OrderItems ON OrderItems.OrderId = o.Id
        "#;
        let scopes = extract_all_subquery_scopes(sql);
        assert_eq!(scopes.len(), 2, "Should find 2 derived table scopes");

        // First scope should have 't' -> Tag
        assert_eq!(
            scopes[0].aliases.get("t"),
            Some(&"[dbo].[Tag]".to_string()),
            "First scope alias 't' should map to Tag"
        );

        // Second scope should have 't' -> Task
        assert_eq!(
            scopes[1].aliases.get("t"),
            Some(&"[dbo].[Task]".to_string()),
            "Second scope alias 't' should map to Task"
        );
    }

    #[test]
    fn test_resolve_alias_for_position_innermost_scope() {
        // Test that innermost scope wins for nested scopes
        let scopes = vec![
            ApplySubqueryScope {
                start_pos: 100,
                end_pos: 500,
                tables: vec!["[dbo].[Outer]".to_string()],
                aliases: {
                    let mut m = HashMap::new();
                    m.insert("t".to_string(), "[dbo].[Outer]".to_string());
                    m
                },
            },
            ApplySubqueryScope {
                start_pos: 200,
                end_pos: 400,
                tables: vec!["[dbo].[Inner]".to_string()],
                aliases: {
                    let mut m = HashMap::new();
                    m.insert("t".to_string(), "[dbo].[Inner]".to_string());
                    m
                },
            },
        ];

        let global_aliases: HashMap<String, String> = HashMap::new();

        // Position 150 is in outer scope only
        let result = resolve_alias_for_position("t", 150, &scopes, &global_aliases);
        assert_eq!(
            result,
            Some(&"[dbo].[Outer]".to_string()),
            "Position 150 should resolve to outer scope"
        );

        // Position 300 is in both scopes - inner (smaller) wins
        let result = resolve_alias_for_position("t", 300, &scopes, &global_aliases);
        assert_eq!(
            result,
            Some(&"[dbo].[Inner]".to_string()),
            "Position 300 should resolve to inner (smaller) scope"
        );

        // Position 450 is in outer scope only
        let result = resolve_alias_for_position("t", 450, &scopes, &global_aliases);
        assert_eq!(
            result,
            Some(&"[dbo].[Outer]".to_string()),
            "Position 450 should resolve to outer scope"
        );

        // Position 600 is outside all scopes - falls back to global
        let result = resolve_alias_for_position("t", 600, &scopes, &global_aliases);
        assert_eq!(
            result, None,
            "Position 600 should fall back to global (which is empty)"
        );
    }

    #[test]
    fn test_resolve_alias_falls_back_to_global() {
        // Test that global aliases are used when not inside a scope
        let scopes = vec![ApplySubqueryScope {
            start_pos: 100,
            end_pos: 200,
            tables: vec!["[dbo].[ScopeTable]".to_string()],
            aliases: {
                let mut m = HashMap::new();
                m.insert("s".to_string(), "[dbo].[ScopeTable]".to_string());
                m
            },
        }];

        let mut global_aliases: HashMap<String, String> = HashMap::new();
        global_aliases.insert("g".to_string(), "[dbo].[GlobalTable]".to_string());

        // Position 50 is outside scope - should use global
        let result = resolve_alias_for_position("g", 50, &scopes, &global_aliases);
        assert_eq!(
            result,
            Some(&"[dbo].[GlobalTable]".to_string()),
            "Should resolve 'g' from global aliases"
        );

        // Position 150 is inside scope but 'g' not in scope - should still use global
        let result = resolve_alias_for_position("g", 150, &scopes, &global_aliases);
        assert_eq!(
            result,
            Some(&"[dbo].[GlobalTable]".to_string()),
            "Should resolve 'g' from global even when inside scope (alias not in scope)"
        );
    }

    #[test]
    fn test_body_deps_scope_conflict_resolution() {
        // Full integration test: verify that t.Id resolves to correct table in each scope
        let sql = r#"
            SELECT
                OrderTags.TagName,
                OrderItems.ItemCount
            FROM [dbo].[Order] o
            LEFT JOIN (
                SELECT ot.OrderId, t.Name AS TagName
                FROM [dbo].[OrderTag] ot
                INNER JOIN [dbo].[Tag] t ON ot.TagId = t.Id
            ) OrderTags ON OrderTags.OrderId = o.Id
            LEFT JOIN (
                SELECT i.OrderId, COUNT(*) AS ItemCount
                FROM [dbo].[OrderItem] i
                INNER JOIN [dbo].[Task] t ON t.Id = i.TaskId
                WHERE t.IsActive = 1
                GROUP BY i.OrderId
            ) OrderItems ON OrderItems.OrderId = o.Id
        "#;
        let deps = extract_body_dependencies(sql, "[dbo].[TestProc]", &[], &empty_registry());

        // Should have [dbo].[Tag].[Name] from first derived table
        let has_tag_name = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Tag].[Name]",
            _ => false,
        });
        assert!(
            has_tag_name,
            "First derived table t.Name should resolve to [dbo].[Tag].[Name]. Got deps: {:?}",
            deps
        );

        // Should have [dbo].[Task].[IsActive] from second derived table
        let has_task_isactive = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Task].[IsActive]",
            _ => false,
        });
        assert!(
            has_task_isactive,
            "Second derived table t.IsActive should resolve to [dbo].[Task].[IsActive]. Got deps: {:?}",
            deps
        );

        // Should have [dbo].[Tag].[Id] and [dbo].[Task].[Id] - both 't.Id' references
        let has_tag_id = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Tag].[Id]",
            _ => false,
        });
        let has_task_id = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Task].[Id]",
            _ => false,
        });
        assert!(
            has_tag_id,
            "Should have [dbo].[Tag].[Id] from first derived table. Got deps: {:?}",
            deps
        );
        assert!(
            has_task_id,
            "Should have [dbo].[Task].[Id] from second derived table. Got deps: {:?}",
            deps
        );
    }
}
