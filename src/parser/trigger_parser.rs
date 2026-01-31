//! Token-based trigger definition parsing for T-SQL
//!
//! This module provides token-based parsing for trigger definitions, replacing
//! the previous regex-based approach. Part of Phase 15.3 of the implementation plan.
//!
//! ## Supported Syntax
//!
//! CREATE TRIGGER:
//! ```sql
//! CREATE TRIGGER [schema].[name] ON [schema].[table] FOR INSERT, UPDATE AS ...
//! CREATE TRIGGER [schema].[name] ON [schema].[table] AFTER DELETE AS ...
//! CREATE TRIGGER [schema].[name] ON [schema].[view] INSTEAD OF INSERT AS ...
//! CREATE OR ALTER TRIGGER [schema].[name] ON [schema].[table] AFTER INSERT, UPDATE, DELETE AS ...
//! ```
//!
//! ALTER TRIGGER:
//! ```sql
//! ALTER TRIGGER [schema].[name] ON [schema].[table] AFTER UPDATE AS ...
//! ```

use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

/// Result of parsing a trigger definition using tokens
#[derive(Debug, Clone, Default)]
pub struct TokenParsedTrigger {
    /// Schema name of the trigger (defaults to "dbo" if not specified)
    pub schema: String,
    /// Trigger name
    pub name: String,
    /// Schema name of the parent table/view (defaults to "dbo" if not specified)
    pub parent_schema: String,
    /// Name of the parent table/view
    pub parent_name: String,
    /// True if trigger fires on INSERT
    pub is_insert: bool,
    /// True if trigger fires on UPDATE
    pub is_update: bool,
    /// True if trigger fires on DELETE
    pub is_delete: bool,
    /// Trigger type: 2 = AFTER/FOR, 3 = INSTEAD OF
    pub trigger_type: u8,
}

/// Token-based trigger definition parser
pub struct TriggerTokenParser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl TriggerTokenParser {
    /// Create a new parser for a trigger definition string
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;

        Some(Self { tokens, pos: 0 })
    }

    /// Parse CREATE TRIGGER and return trigger info
    pub fn parse_create_trigger(&mut self) -> Option<TokenParsedTrigger> {
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

        // Expect TRIGGER keyword
        if !self.check_keyword(Keyword::TRIGGER) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse trigger name (schema-qualified)
        let (trigger_schema, trigger_name) = self.parse_schema_qualified_name()?;
        self.skip_whitespace();

        // Expect ON keyword
        if !self.check_keyword(Keyword::ON) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse parent table/view name (schema-qualified)
        let (parent_schema, parent_name) = self.parse_schema_qualified_name()?;
        self.skip_whitespace();

        // Parse trigger type and events
        let (trigger_type, is_insert, is_update, is_delete) = self.parse_trigger_clause()?;

        Some(TokenParsedTrigger {
            schema: trigger_schema,
            name: trigger_name,
            parent_schema,
            parent_name,
            is_insert,
            is_update,
            is_delete,
            trigger_type,
        })
    }

    /// Parse trigger clause: (INSTEAD OF | AFTER | FOR) (INSERT|UPDATE|DELETE)[,...]
    /// Returns (trigger_type, is_insert, is_update, is_delete)
    fn parse_trigger_clause(&mut self) -> Option<(u8, bool, bool, bool)> {
        // Determine trigger type based on keyword
        let trigger_type = if self.check_word_ci("INSTEAD") {
            // INSTEAD OF
            self.advance();
            self.skip_whitespace();
            if !self.check_keyword(Keyword::OF) {
                return None;
            }
            self.advance();
            self.skip_whitespace();
            3u8 // INSTEAD OF
        } else if self.check_keyword(Keyword::AFTER) || self.check_keyword(Keyword::FOR) {
            // AFTER and FOR are equivalent trigger types in T-SQL
            self.advance();
            self.skip_whitespace();
            2u8 // AFTER or FOR
        } else {
            return None;
        };

        // Parse events: INSERT, UPDATE, DELETE (comma-separated)
        let mut is_insert = false;
        let mut is_update = false;
        let mut is_delete = false;

        loop {
            if self.check_keyword(Keyword::INSERT) {
                is_insert = true;
                self.advance();
            } else if self.check_keyword(Keyword::UPDATE) {
                is_update = true;
                self.advance();
            } else if self.check_keyword(Keyword::DELETE) {
                is_delete = true;
                self.advance();
            } else {
                // Not an event keyword - either end or unknown
                break;
            }

            self.skip_whitespace();

            // Check for comma (more events)
            if self.check_token(&Token::Comma) {
                self.advance();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        // Must have at least one event
        if !is_insert && !is_update && !is_delete {
            return None;
        }

        Some((trigger_type, is_insert, is_update, is_delete))
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

    // ========================================================================
    // Helper methods (similar to ProcedureTokenParser)
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

/// Parse CREATE TRIGGER using tokens and return trigger info
///
/// This function replaces the regex-based `extract_trigger_info` function.
/// Supports:
/// - CREATE TRIGGER [dbo].[TriggerName] ON [dbo].[TableName] FOR INSERT, UPDATE
/// - CREATE TRIGGER [dbo].[TriggerName] ON [dbo].[ViewName] INSTEAD OF DELETE
/// - CREATE OR ALTER TRIGGER [dbo].[TriggerName] ON [dbo].[TableName] AFTER INSERT, UPDATE, DELETE
pub fn parse_create_trigger_tokens(sql: &str) -> Option<TokenParsedTrigger> {
    let mut parser = TriggerTokenParser::new(sql)?;
    parser.parse_create_trigger()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // CREATE TRIGGER tests
    // ========================================================================

    #[test]
    fn test_create_trigger_basic_for() {
        let sql = "CREATE TRIGGER [dbo].[TR_Users_Insert] ON [dbo].[Users] FOR INSERT AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TR_Users_Insert");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "Users");
        assert!(result.is_insert);
        assert!(!result.is_update);
        assert!(!result.is_delete);
        assert_eq!(result.trigger_type, 2); // FOR = AFTER
    }

    #[test]
    fn test_create_trigger_after() {
        let sql = "CREATE TRIGGER [dbo].[TR_Users_Update] ON [dbo].[Users] AFTER UPDATE AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TR_Users_Update");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "Users");
        assert!(!result.is_insert);
        assert!(result.is_update);
        assert!(!result.is_delete);
        assert_eq!(result.trigger_type, 2); // AFTER
    }

    #[test]
    fn test_create_trigger_instead_of() {
        let sql = "CREATE TRIGGER [dbo].[TR_View_Delete] ON [dbo].[MyView] INSTEAD OF DELETE AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TR_View_Delete");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "MyView");
        assert!(!result.is_insert);
        assert!(!result.is_update);
        assert!(result.is_delete);
        assert_eq!(result.trigger_type, 3); // INSTEAD OF
    }

    #[test]
    fn test_create_trigger_multiple_events() {
        let sql = "CREATE TRIGGER [dbo].[TR_Audit] ON [dbo].[Products] AFTER INSERT, UPDATE, DELETE AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TR_Audit");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "Products");
        assert!(result.is_insert);
        assert!(result.is_update);
        assert!(result.is_delete);
        assert_eq!(result.trigger_type, 2);
    }

    #[test]
    fn test_create_trigger_two_events() {
        let sql = "CREATE TRIGGER [dbo].[TR_Log] ON [dbo].[Orders] FOR INSERT, UPDATE AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert!(result.is_insert);
        assert!(result.is_update);
        assert!(!result.is_delete);
    }

    #[test]
    fn test_create_or_alter_trigger() {
        let sql = "CREATE OR ALTER TRIGGER [dbo].[TR_Test] ON [dbo].[TestTable] AFTER INSERT AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TR_Test");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "TestTable");
        assert!(result.is_insert);
        assert_eq!(result.trigger_type, 2);
    }

    #[test]
    fn test_create_trigger_custom_schema() {
        let sql = "CREATE TRIGGER [sales].[TR_OrderAudit] ON [sales].[Orders] AFTER UPDATE AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "sales");
        assert_eq!(result.name, "TR_OrderAudit");
        assert_eq!(result.parent_schema, "sales");
        assert_eq!(result.parent_name, "Orders");
    }

    #[test]
    fn test_create_trigger_different_schemas() {
        let sql =
            "CREATE TRIGGER [audit].[TR_Log] ON [dbo].[Users] AFTER INSERT AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "audit");
        assert_eq!(result.name, "TR_Log");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "Users");
    }

    #[test]
    fn test_create_trigger_unbracketed() {
        let sql = "CREATE TRIGGER dbo.TR_Test ON dbo.TestTable AFTER INSERT AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TR_Test");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "TestTable");
    }

    #[test]
    fn test_create_trigger_no_schema() {
        let sql = "CREATE TRIGGER [TR_NoSchema] ON [Users] AFTER INSERT AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TR_NoSchema");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "Users");
    }

    #[test]
    fn test_create_trigger_multiline() {
        let sql = r#"
CREATE TRIGGER [dbo].[TR_Users_Audit]
ON [dbo].[Users]
AFTER INSERT, UPDATE, DELETE
AS
BEGIN
    SET NOCOUNT ON;
    -- Audit trigger placeholder
END
"#;
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TR_Users_Audit");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "Users");
        assert!(result.is_insert);
        assert!(result.is_update);
        assert!(result.is_delete);
        assert_eq!(result.trigger_type, 2);
    }

    #[test]
    fn test_create_trigger_instead_of_multiline() {
        let sql = r#"
CREATE TRIGGER [dbo].[TR_ProductsView_Insert]
ON [dbo].[ProductsView]
INSTEAD OF INSERT
AS
BEGIN
    SET NOCOUNT ON;
    INSERT INTO [dbo].[Products] ([Id], [Name], [Price], [IsActive], [CreatedAt])
    SELECT [Id], [Name], [Price], 1, GETDATE()
    FROM inserted;
END
"#;
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.schema, "dbo");
        assert_eq!(result.name, "TR_ProductsView_Insert");
        assert_eq!(result.parent_schema, "dbo");
        assert_eq!(result.parent_name, "ProductsView");
        assert!(result.is_insert);
        assert!(!result.is_update);
        assert!(!result.is_delete);
        assert_eq!(result.trigger_type, 3); // INSTEAD OF
    }

    #[test]
    fn test_create_trigger_case_insensitive() {
        let sql = "create trigger [dbo].[TR_Test] on [dbo].[TestTable] after insert as begin select 1 end";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.name, "TR_Test");
        assert!(result.is_insert);
    }

    #[test]
    fn test_create_trigger_mixed_case() {
        let sql = "Create Trigger [dbo].[TR_Test] On [dbo].[TestTable] After Insert As Begin Select 1 End";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.name, "TR_Test");
        assert!(result.is_insert);
    }

    #[test]
    fn test_create_trigger_with_special_characters_in_name() {
        let sql = "CREATE TRIGGER [dbo].[TR_Users&Orders] ON [dbo].[Data&Table] AFTER INSERT AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert_eq!(result.name, "TR_Users&Orders");
        assert_eq!(result.parent_name, "Data&Table");
    }

    // ========================================================================
    // Edge cases and negative tests
    // ========================================================================

    #[test]
    fn test_not_a_trigger() {
        let result = parse_create_trigger_tokens("CREATE TABLE [dbo].[Users] (Id INT)");
        assert!(result.is_none());
    }

    #[test]
    fn test_alter_on_create() {
        // ALTER TRIGGER should not match CREATE TRIGGER parser
        let result = parse_create_trigger_tokens(
            "ALTER TRIGGER [dbo].[TR_Test] ON [dbo].[Test] AFTER INSERT AS BEGIN SELECT 1 END",
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_missing_on_keyword() {
        let result = parse_create_trigger_tokens(
            "CREATE TRIGGER [dbo].[TR_Test] [dbo].[Test] AFTER INSERT AS BEGIN SELECT 1 END",
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_missing_event() {
        let result = parse_create_trigger_tokens(
            "CREATE TRIGGER [dbo].[TR_Test] ON [dbo].[Test] AFTER AS BEGIN SELECT 1 END",
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_missing_trigger_type() {
        let result = parse_create_trigger_tokens(
            "CREATE TRIGGER [dbo].[TR_Test] ON [dbo].[Test] INSERT AS BEGIN SELECT 1 END",
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_create_procedure_not_trigger() {
        let result =
            parse_create_trigger_tokens("CREATE PROCEDURE [dbo].[GetUsers] AS SELECT * FROM Users");
        assert!(result.is_none());
    }

    #[test]
    fn test_events_order_insert_delete() {
        let sql = "CREATE TRIGGER [dbo].[TR_Test] ON [dbo].[Test] AFTER INSERT, DELETE AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert!(result.is_insert);
        assert!(!result.is_update);
        assert!(result.is_delete);
    }

    #[test]
    fn test_events_order_update_delete() {
        let sql = "CREATE TRIGGER [dbo].[TR_Test] ON [dbo].[Test] AFTER UPDATE, DELETE AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert!(!result.is_insert);
        assert!(result.is_update);
        assert!(result.is_delete);
    }

    #[test]
    fn test_events_order_delete_update_insert() {
        let sql = "CREATE TRIGGER [dbo].[TR_Test] ON [dbo].[Test] AFTER DELETE, UPDATE, INSERT AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert!(result.is_insert);
        assert!(result.is_update);
        assert!(result.is_delete);
    }

    #[test]
    fn test_instead_of_insert_update() {
        let sql = "CREATE TRIGGER [dbo].[TR_View] ON [dbo].[MyView] INSTEAD OF INSERT, UPDATE AS BEGIN SELECT 1 END";
        let result = parse_create_trigger_tokens(sql).unwrap();
        assert!(result.is_insert);
        assert!(result.is_update);
        assert!(!result.is_delete);
        assert_eq!(result.trigger_type, 3);
    }
}
