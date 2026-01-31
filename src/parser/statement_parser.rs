//! Token-based statement detection parsing for T-SQL
//!
//! This module provides token-based parsing for detecting statement types that
//! sqlparser-rs doesn't support natively. Part of Phase 15.5 of the implementation plan.
//!
//! ## Supported Patterns
//!
//! CTE with DML (A2):
//! ```sql
//! WITH cte AS (...) DELETE FROM target WHERE ...
//! WITH cte AS (...) UPDATE target SET ...
//! WITH cte AS (...) INSERT INTO target ...
//! WITH cte AS (...) MERGE INTO target USING ...
//! ```
//!
//! MERGE with OUTPUT (A3):
//! ```sql
//! MERGE INTO target USING source ON ... OUTPUT ...
//! ```
//!
//! UPDATE with XML methods (A4):
//! ```sql
//! UPDATE table SET column.modify('...')
//! UPDATE table SET column.value('...')
//! ```
//!
//! DROP statements (A1):
//! ```sql
//! DROP SYNONYM [schema].[name]
//! DROP TRIGGER [schema].[name]
//! DROP INDEX index_name ON [schema].[table]
//! DROP PROC [schema].[name]
//! ```

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

/// Result of parsing a CTE followed by DML using tokens
#[derive(Debug, Clone)]
pub struct TokenParsedCteDml {
    /// The DML type: DELETE, UPDATE, INSERT, or MERGE
    pub dml_type: DmlType,
}

/// DML operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmlType {
    Delete,
    Update,
    Insert,
    Merge,
}

impl DmlType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DmlType::Delete => "DELETE",
            DmlType::Update => "UPDATE",
            DmlType::Insert => "INSERT",
            DmlType::Merge => "MERGE",
        }
    }
}

/// Result of parsing a MERGE with OUTPUT clause
#[derive(Debug, Clone)]
pub struct TokenParsedMergeOutput {
    /// Schema of the target table
    pub schema: String,
    /// Name of the target table
    pub name: String,
}

/// Result of parsing an UPDATE with XML methods
#[derive(Debug, Clone)]
pub struct TokenParsedXmlUpdate {
    /// Schema of the target table
    pub schema: String,
    /// Name of the target table
    pub name: String,
}

/// Result of parsing a DROP statement
#[derive(Debug, Clone)]
pub struct TokenParsedDrop {
    /// Object type being dropped (Synonym, Trigger, Index, Procedure)
    pub drop_type: DropType,
    /// Schema of the object
    pub schema: String,
    /// Name of the object
    pub name: String,
}

/// Result of parsing a generic CREATE statement (A5)
#[derive(Debug, Clone)]
pub struct TokenParsedGenericCreate {
    /// Object type (e.g., "TABLE", "VIEW", "RULE", "SYNONYM")
    pub object_type: String,
    /// Schema of the object
    pub schema: String,
    /// Name of the object
    pub name: String,
}

/// Type of object being dropped
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropType {
    Synonym,
    Trigger,
    Index,
    Procedure,
}

impl DropType {
    pub fn object_type_str(&self) -> &'static str {
        match self {
            DropType::Synonym => "DropSynonym",
            DropType::Trigger => "DropTrigger",
            DropType::Index => "DropIndex",
            DropType::Procedure => "DropProcedure",
        }
    }
}

/// Token-based statement parser
pub struct StatementTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl StatementTokenParser {
    /// Create a new parser for a SQL statement
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Try to parse a CTE followed by DML (DELETE, UPDATE, INSERT, MERGE)
    ///
    /// This detects patterns like:
    /// - WITH cte AS (...) DELETE FROM ...
    /// - WITH cte AS (...) UPDATE ...
    /// - WITH cte AS (...) INSERT INTO ...
    /// - WITH cte AS (...) MERGE INTO ...
    pub fn try_parse_cte_dml(&mut self) -> Option<TokenParsedCteDml> {
        self.skip_whitespace();

        // Must start with WITH keyword
        if !self.check_keyword(Keyword::WITH) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Skip RECURSIVE if present (for recursive CTEs)
        if self.check_word_ci("RECURSIVE") {
            self.advance();
            self.skip_whitespace();
        }

        // Now we need to find the end of the CTE definition(s) and look for a DML keyword
        // CTEs are: name AS (...), name AS (...), ... followed by the main query
        // We track parenthesis depth to find when we exit the CTE definitions

        let mut paren_depth = 0;
        let mut found_as = false;

        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                match &token.token {
                    Token::LParen => {
                        paren_depth += 1;
                        self.advance();
                    }
                    Token::RParen => {
                        if paren_depth > 0 {
                            paren_depth -= 1;
                        }
                        self.advance();

                        // After closing paren at depth 0, check for DML keyword
                        if paren_depth == 0 && found_as {
                            self.skip_whitespace();

                            // Check for comma (more CTEs) or DML keyword
                            if self.check_token(&Token::Comma) {
                                // More CTEs coming
                                self.advance();
                                self.skip_whitespace();
                                found_as = false;
                                continue;
                            }

                            // Check for DML keywords
                            if let Some(dml_type) = self.check_dml_keyword() {
                                return Some(TokenParsedCteDml { dml_type });
                            }

                            // If SELECT, this is a normal CTE that sqlparser can handle
                            if self.check_keyword(Keyword::SELECT) {
                                return None;
                            }
                        }
                    }
                    Token::Word(w) if w.keyword == Keyword::AS && paren_depth == 0 => {
                        found_as = true;
                        self.advance();
                    }
                    _ => {
                        self.advance();
                    }
                }
            } else {
                break;
            }
        }

        None
    }

    /// Try to parse a MERGE statement with OUTPUT clause
    pub fn try_parse_merge_output(&mut self) -> Option<TokenParsedMergeOutput> {
        self.skip_whitespace();

        // Must start with MERGE keyword
        if !self.check_keyword(Keyword::MERGE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Optional INTO keyword
        if self.check_keyword(Keyword::INTO) {
            self.advance();
            self.skip_whitespace();
        }

        // Parse target table name (schema-qualified)
        let (schema, name) = self.parse_schema_qualified_name()?;

        // Now scan for OUTPUT keyword
        while !self.is_at_end() {
            self.skip_whitespace();

            if self.check_word_ci("OUTPUT") {
                return Some(TokenParsedMergeOutput { schema, name });
            }

            self.advance();
        }

        None
    }

    /// Try to parse an UPDATE statement with XML method calls
    pub fn try_parse_xml_update(&mut self) -> Option<TokenParsedXmlUpdate> {
        self.skip_whitespace();

        // Must start with UPDATE keyword
        if !self.check_keyword(Keyword::UPDATE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse target table name (schema-qualified)
        let (schema, name) = self.parse_schema_qualified_name()?;

        // Now scan for XML method patterns: .modify( or .value(
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                if matches!(&token.token, Token::Period) {
                    self.advance();
                    self.skip_whitespace();

                    // Check for modify or value keywords
                    if self.check_word_ci("MODIFY") || self.check_word_ci("VALUE") {
                        self.advance();
                        self.skip_whitespace();

                        // Must be followed by opening paren for method call
                        if self.check_token(&Token::LParen) {
                            return Some(TokenParsedXmlUpdate { schema, name });
                        }
                    }
                } else {
                    self.advance();
                }
            } else {
                break;
            }
        }

        None
    }

    /// Try to parse a DROP statement (SYNONYM, TRIGGER, INDEX, PROC)
    pub fn try_parse_drop(&mut self) -> Option<TokenParsedDrop> {
        self.skip_whitespace();

        // Must start with DROP keyword
        if !self.check_keyword(Keyword::DROP) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Determine what type of DROP this is
        if self.check_word_ci("SYNONYM") {
            self.advance();
            self.skip_whitespace();
            self.skip_if_exists();
            let (schema, name) = self.parse_schema_qualified_name()?;
            return Some(TokenParsedDrop {
                drop_type: DropType::Synonym,
                schema,
                name,
            });
        }

        if self.check_keyword(Keyword::TRIGGER) {
            self.advance();
            self.skip_whitespace();
            self.skip_if_exists();
            let (schema, name) = self.parse_schema_qualified_name()?;
            return Some(TokenParsedDrop {
                drop_type: DropType::Trigger,
                schema,
                name,
            });
        }

        if self.check_keyword(Keyword::INDEX) {
            // DROP INDEX index_name ON [schema].[table]
            self.advance();
            self.skip_whitespace();
            self.skip_if_exists();

            let index_name = self.parse_identifier()?;
            self.skip_whitespace();

            // Expect ON keyword
            if !self.check_keyword(Keyword::ON) {
                return None;
            }
            self.advance();
            self.skip_whitespace();

            // Parse table name (schema-qualified)
            let (table_schema, table_name) = self.parse_schema_qualified_name()?;

            // Combined name: table_name_index_name (matching existing behavior)
            let combined_name = format!("{}_{}", table_name, index_name);

            return Some(TokenParsedDrop {
                drop_type: DropType::Index,
                schema: table_schema,
                name: combined_name,
            });
        }

        // Check for PROC (but not PROCEDURE - sqlparser handles that)
        if self.check_word_ci("PROC") {
            // Make sure it's not PROCEDURE
            if let Some(token) = self.current_token() {
                if let Token::Word(w) = &token.token {
                    if w.value.eq_ignore_ascii_case("PROC")
                        && !w.value.eq_ignore_ascii_case("PROCEDURE")
                    {
                        self.advance();
                        self.skip_whitespace();
                        self.skip_if_exists();
                        let (schema, name) = self.parse_schema_qualified_name()?;
                        return Some(TokenParsedDrop {
                            drop_type: DropType::Procedure,
                            schema,
                            name,
                        });
                    }
                }
            }
        }

        None
    }

    /// Try to parse any CREATE statement as a generic fallback (A5)
    ///
    /// This is used as a last resort to extract object type, schema, and name
    /// from CREATE statements that aren't handled by more specific parsers.
    ///
    /// Handles patterns like:
    /// - CREATE RULE [schema].[name] ...
    /// - CREATE OR ALTER TYPE [schema].[name] ...
    /// - CREATE SYNONYM [schema].[name] ...
    pub fn try_parse_generic_create(&mut self) -> Option<TokenParsedGenericCreate> {
        self.skip_whitespace();

        // Must start with CREATE keyword
        if !self.check_keyword(Keyword::CREATE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Handle optional "OR ALTER" clause
        if self.check_keyword(Keyword::OR) {
            self.advance();
            self.skip_whitespace();

            if self.check_word_ci("ALTER") {
                self.advance();
                self.skip_whitespace();
            } else {
                // "OR" without "ALTER" is not valid
                return None;
            }
        }

        // Extract the object type (e.g., TABLE, VIEW, PROCEDURE, RULE, SYNONYM)
        let object_type = self.parse_identifier()?;
        self.skip_whitespace();

        // Parse the schema-qualified name
        let (schema, name) = self.parse_schema_qualified_name()?;

        Some(TokenParsedGenericCreate {
            object_type,
            schema,
            name,
        })
    }

    // ========================================================================
    // Helper methods
    // ========================================================================

    /// Check if current token is a DML keyword and return the type
    fn check_dml_keyword(&self) -> Option<DmlType> {
        if self.check_keyword(Keyword::DELETE) {
            Some(DmlType::Delete)
        } else if self.check_keyword(Keyword::UPDATE) {
            Some(DmlType::Update)
        } else if self.check_keyword(Keyword::INSERT) {
            Some(DmlType::Insert)
        } else if self.check_keyword(Keyword::MERGE) {
            Some(DmlType::Merge)
        } else {
            None
        }
    }

    /// Skip optional IF EXISTS clause
    fn skip_if_exists(&mut self) {
        if self.check_keyword(Keyword::IF) {
            self.advance();
            self.skip_whitespace();

            if self.check_keyword(Keyword::EXISTS) {
                self.advance();
                self.skip_whitespace();
            }
        }
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

// ============================================================================
// Public API functions
// ============================================================================

/// Try to parse a CTE followed by DML using tokens
///
/// Returns Some(TokenParsedCteDml) if the SQL is a CTE with DELETE, UPDATE, INSERT, or MERGE.
/// Returns None if it's a regular CTE with SELECT (which sqlparser handles).
pub fn try_parse_cte_dml_tokens(sql: &str) -> Option<TokenParsedCteDml> {
    let mut parser = StatementTokenParser::new(sql)?;
    parser.try_parse_cte_dml()
}

/// Try to parse a MERGE with OUTPUT clause using tokens
///
/// Returns Some(TokenParsedMergeOutput) if the SQL is a MERGE with OUTPUT clause.
pub fn try_parse_merge_output_tokens(sql: &str) -> Option<TokenParsedMergeOutput> {
    let mut parser = StatementTokenParser::new(sql)?;
    parser.try_parse_merge_output()
}

/// Try to parse an UPDATE with XML methods using tokens
///
/// Returns Some(TokenParsedXmlUpdate) if the SQL is an UPDATE with .modify() or .value() calls.
pub fn try_parse_xml_update_tokens(sql: &str) -> Option<TokenParsedXmlUpdate> {
    let mut parser = StatementTokenParser::new(sql)?;
    parser.try_parse_xml_update()
}

/// Try to parse a DROP statement using tokens
///
/// Returns Some(TokenParsedDrop) for DROP SYNONYM, DROP TRIGGER, DROP INDEX ... ON,
/// and DROP PROC (abbreviated form).
pub fn try_parse_drop_tokens(sql: &str) -> Option<TokenParsedDrop> {
    let mut parser = StatementTokenParser::new(sql)?;
    parser.try_parse_drop()
}

/// Try to parse any CREATE statement as a generic fallback (A5)
///
/// This is used as a last resort to extract object type, schema, and name
/// from CREATE statements that aren't handled by more specific parsers.
/// Returns Some(TokenParsedGenericCreate) with the object type, schema, and name.
pub fn try_parse_generic_create_tokens(sql: &str) -> Option<TokenParsedGenericCreate> {
    let mut parser = StatementTokenParser::new(sql)?;
    parser.try_parse_generic_create()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // CTE with DML tests (A2)
    // ========================================================================

    #[test]
    fn test_cte_with_delete() {
        let sql = r#"
            WITH ToDelete AS (
                SELECT Id FROM Products WHERE IsDeleted = 1
            )
            DELETE FROM Products WHERE Id IN (SELECT Id FROM ToDelete)
        "#;

        let result = try_parse_cte_dml_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.dml_type, DmlType::Delete);
    }

    #[test]
    fn test_cte_with_update() {
        let sql = r#"
            WITH ToUpdate AS (
                SELECT Id, Name FROM Products WHERE Category = 'Old'
            )
            UPDATE p SET p.Category = 'New'
            FROM Products p
            INNER JOIN ToUpdate t ON p.Id = t.Id
        "#;

        let result = try_parse_cte_dml_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.dml_type, DmlType::Update);
    }

    #[test]
    fn test_cte_with_insert() {
        let sql = r#"
            WITH SourceData AS (
                SELECT Name, Price FROM OtherTable WHERE Active = 1
            )
            INSERT INTO Products (Name, Price)
            SELECT Name, Price FROM SourceData
        "#;

        let result = try_parse_cte_dml_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.dml_type, DmlType::Insert);
    }

    #[test]
    fn test_cte_with_merge() {
        let sql = r#"
            WITH Source AS (
                SELECT Id, Name FROM SourceTable
            )
            MERGE INTO TargetTable t
            USING Source s ON t.Id = s.Id
            WHEN MATCHED THEN UPDATE SET t.Name = s.Name
            WHEN NOT MATCHED THEN INSERT (Id, Name) VALUES (s.Id, s.Name);
        "#;

        let result = try_parse_cte_dml_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.dml_type, DmlType::Merge);
    }

    #[test]
    fn test_cte_with_select_returns_none() {
        // Regular CTE with SELECT should return None (sqlparser handles these)
        let sql = r#"
            WITH cte AS (
                SELECT Id, Name FROM Products
            )
            SELECT * FROM cte
        "#;

        let result = try_parse_cte_dml_tokens(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_ctes_with_delete() {
        let sql = r#"
            WITH
                First AS (SELECT Id FROM Table1),
                Second AS (SELECT Id FROM Table2 WHERE Id IN (SELECT Id FROM First))
            DELETE FROM Table3 WHERE Id IN (SELECT Id FROM Second)
        "#;

        let result = try_parse_cte_dml_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.dml_type, DmlType::Delete);
    }

    #[test]
    fn test_not_a_cte() {
        let sql = "SELECT * FROM Products";
        let result = try_parse_cte_dml_tokens(sql);
        assert!(result.is_none());
    }

    // ========================================================================
    // MERGE with OUTPUT tests (A3)
    // ========================================================================

    #[test]
    fn test_merge_with_output() {
        let sql = r#"
            MERGE INTO dbo.TargetTable t
            USING dbo.SourceTable s ON t.Id = s.Id
            WHEN MATCHED THEN UPDATE SET t.Name = s.Name
            OUTPUT $action, inserted.Id, deleted.Id;
        "#;

        let result = try_parse_merge_output_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "TargetTable");
    }

    #[test]
    fn test_merge_without_output() {
        let sql = r#"
            MERGE INTO dbo.TargetTable t
            USING dbo.SourceTable s ON t.Id = s.Id
            WHEN MATCHED THEN UPDATE SET t.Name = s.Name;
        "#;

        let result = try_parse_merge_output_tokens(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_merge_with_output_no_into() {
        let sql = r#"
            MERGE dbo.TargetTable t
            USING dbo.SourceTable s ON t.Id = s.Id
            WHEN MATCHED THEN UPDATE SET t.Name = s.Name
            OUTPUT inserted.*, deleted.*;
        "#;

        let result = try_parse_merge_output_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "TargetTable");
    }

    #[test]
    fn test_merge_default_schema() {
        let sql = r#"
            MERGE TargetTable t
            USING SourceTable s ON t.Id = s.Id
            WHEN MATCHED THEN UPDATE SET t.Name = s.Name
            OUTPUT $action;
        "#;

        let result = try_parse_merge_output_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "TargetTable");
    }

    // ========================================================================
    // UPDATE with XML methods tests (A4)
    // ========================================================================

    #[test]
    fn test_update_with_xml_modify() {
        let sql = r#"
            UPDATE dbo.Products
            SET XmlData.modify('replace value of (/root/item/text())[1] with "new value"')
            WHERE Id = 1
        "#;

        let result = try_parse_xml_update_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "Products");
    }

    #[test]
    fn test_update_with_xml_value() {
        let sql = r#"
            UPDATE Products
            SET Name = XmlData.value('(/root/name)[1]', 'nvarchar(100)')
            WHERE Id = 1
        "#;

        let result = try_parse_xml_update_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "Products");
    }

    #[test]
    fn test_update_without_xml_method() {
        let sql = r#"
            UPDATE Products
            SET Name = 'New Name'
            WHERE Id = 1
        "#;

        let result = try_parse_xml_update_tokens(sql);
        assert!(result.is_none());
    }

    // ========================================================================
    // DROP statement tests (A1)
    // ========================================================================

    #[test]
    fn test_drop_synonym() {
        let sql = "DROP SYNONYM [dbo].[MySynonym]";

        let result = try_parse_drop_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.drop_type, DropType::Synonym);
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "MySynonym");
    }

    #[test]
    fn test_drop_synonym_if_exists() {
        let sql = "DROP SYNONYM IF EXISTS [dbo].[MySynonym]";

        let result = try_parse_drop_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.drop_type, DropType::Synonym);
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "MySynonym");
    }

    #[test]
    fn test_drop_trigger() {
        let sql = "DROP TRIGGER [dbo].[TR_Products_Insert]";

        let result = try_parse_drop_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.drop_type, DropType::Trigger);
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "TR_Products_Insert");
    }

    #[test]
    fn test_drop_index_on_table() {
        let sql = "DROP INDEX IX_Products_Name ON [dbo].[Products]";

        let result = try_parse_drop_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.drop_type, DropType::Index);
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "Products_IX_Products_Name");
    }

    #[test]
    fn test_drop_proc_abbreviated() {
        let sql = "DROP PROC [dbo].[usp_GetProducts]";

        let result = try_parse_drop_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.drop_type, DropType::Procedure);
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "usp_GetProducts");
    }

    #[test]
    fn test_drop_proc_default_schema() {
        let sql = "DROP PROC MyProcedure";

        let result = try_parse_drop_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.drop_type, DropType::Procedure);
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "MyProcedure");
    }

    #[test]
    fn test_not_a_drop() {
        let sql = "SELECT * FROM Products";
        let result = try_parse_drop_tokens(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_drop_table_not_handled() {
        // DROP TABLE is handled by sqlparser, so we don't parse it
        let sql = "DROP TABLE Products";
        let result = try_parse_drop_tokens(sql);
        assert!(result.is_none());
    }

    // ========================================================================
    // Generic CREATE fallback tests (A5)
    // ========================================================================

    #[test]
    fn test_generic_create_table() {
        let sql = "CREATE TABLE [dbo].[Products] (Id INT)";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "TABLE");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "Products");
    }

    #[test]
    fn test_generic_create_view() {
        let sql = "CREATE VIEW [dbo].[vw_Products] AS SELECT * FROM Products";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "VIEW");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "vw_Products");
    }

    #[test]
    fn test_generic_create_or_alter() {
        let sql = "CREATE OR ALTER VIEW [dbo].[vw_Products] AS SELECT * FROM Products";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "VIEW");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "vw_Products");
    }

    #[test]
    fn test_generic_create_rule() {
        let sql = "CREATE RULE [dbo].[RuleTest] AS @value > 0";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "RULE");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "RuleTest");
    }

    #[test]
    fn test_generic_create_synonym() {
        let sql = "CREATE SYNONYM [dbo].[MySynonym] FOR [OtherDB].[dbo].[Table]";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "SYNONYM");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "MySynonym");
    }

    #[test]
    fn test_generic_create_default_schema() {
        let sql = "CREATE TABLE Products (Id INT)";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "TABLE");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "Products");
    }

    #[test]
    fn test_generic_create_unbracketed() {
        let sql = "CREATE TABLE dbo.Products (Id INT)";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "TABLE");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "Products");
    }

    #[test]
    fn test_generic_create_procedure() {
        let sql = "CREATE PROCEDURE [dbo].[usp_GetProducts] AS SELECT 1";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "PROCEDURE");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "usp_GetProducts");
    }

    #[test]
    fn test_generic_create_function() {
        let sql = "CREATE FUNCTION [dbo].[fn_GetValue] () RETURNS INT AS BEGIN RETURN 1 END";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "FUNCTION");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "fn_GetValue");
    }

    #[test]
    fn test_generic_create_type() {
        let sql = "CREATE TYPE [dbo].[MyTableType] AS TABLE (Id INT)";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "TYPE");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "MyTableType");
    }

    #[test]
    fn test_not_a_create() {
        let sql = "SELECT * FROM Products";
        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_create_with_whitespace() {
        let sql = "  CREATE   TABLE   [dbo].[Products]  (Id INT)";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.object_type, "TABLE");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "Products");
    }

    #[test]
    fn test_generic_create_lowercase() {
        let sql = "create table dbo.products (id int)";

        let result = try_parse_generic_create_tokens(sql);
        assert!(result.is_some());
        let parsed = result.unwrap();
        // Object type is preserved as-is from SQL (lowercase)
        assert_eq!(parsed.object_type, "table");
        assert_eq!(parsed.schema, "dbo");
        assert_eq!(parsed.name, "products");
    }
}
