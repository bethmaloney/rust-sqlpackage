# Parser Refactoring Guide: Custom sqlparser-rs Dialect

This document provides guidance for implementing Phase 15 of the implementation plan: replacing regex-based fallback parsing with a custom sqlparser-rs dialect.

## Table of Contents

1. [Background](#background)
2. [Current Architecture](#current-architecture)
3. [sqlparser-rs Extension Points](#sqlparser-rs-extension-points)
4. [Implementation Strategy](#implementation-strategy)
5. [Detailed API Reference](#detailed-api-reference)
6. [Code Examples](#code-examples)
7. [Migration Path](#migration-path)
8. [Testing Strategy](#testing-strategy)

---

## Background

### Why Replace Regex?

The current parser uses sqlparser-rs with the `MsSqlDialect`, falling back to regex patterns when parsing fails. This approach has several issues:

1. **Brittleness**: Regex patterns fail on edge cases (nested parentheses, comments, special characters)
2. **Maintainability**: 75+ regex patterns across 2800+ lines are hard to understand and modify
3. **Error Messages**: Regex provides no context about where parsing failed
4. **Performance**: Compiling regex at runtime adds overhead (though `lazy_static` mitigates this)

### The sqlparser-rs Custom Dialect Pattern

sqlparser-rs provides extension points through the `Dialect` trait. The pattern is:

1. Create a custom dialect that wraps `MsSqlDialect`
2. Override `parse_statement()` to intercept specific token patterns
3. Delegate to base dialect for everything else
4. Use the parser's tokenizer (not regex) to extract information

This approach was recommended by the sqlparser-rs maintainers and is used by [DataFusion](https://github.com/apache/datafusion).

---

## Current Architecture

### File Structure

```
src/parser/
  mod.rs           # Module exports
  tsql_parser.rs   # Main parser (2800+ lines, 70+ regex patterns)
  sqlcmd.rs        # SQLCMD directive preprocessing (3 regex patterns)
```

### Current Parsing Flow

```
                                  ┌─────────────────────┐
                                  │  preprocess_tsql()  │
                                  │  (regex transforms) │
                                  └──────────┬──────────┘
                                             │
┌─────────────┐    ┌─────────────┐    ┌──────▼──────┐    ┌─────────────┐
│ SQL File    │───▶│split_batches│───▶│Parser::     │───▶│ ParsedStmt  │
│ (.sql)      │    │ (on GO)     │    │parse_sql()  │    │             │
└─────────────┘    └─────────────┘    └──────┬──────┘    └─────────────┘
                                             │
                                             │ Err
                                             ▼
                                  ┌─────────────────────┐
                                  │ try_fallback_parse()│
                                  │  (75+ regex fns)    │
                                  └─────────────────────┘
```

### Key Data Structures

```rust
/// Statement types that require fallback parsing
pub enum FallbackStatementType {
    Procedure { schema: String, name: String },
    Function { schema: String, name: String, function_type: FunctionType, ... },
    Trigger { schema: String, name: String, parent_schema: String, ... },
    Sequence { schema: String, name: String, info: SequenceInfo },
    TableType { schema: String, name: String, columns: Vec<...>, ... },
    Index { schema: String, name: String, table_schema: String, ... },
    FullTextIndex { ... },
    FullTextCatalog { name: String, is_default: bool },
    RawStatement { object_type: String, schema: String, name: String },
}
```

---

## sqlparser-rs Extension Points

### The Dialect Trait

The `Dialect` trait provides several override points. Key methods for our use case:

```rust
pub trait Dialect {
    // REQUIRED: Identifier character rules
    fn is_identifier_start(&self, ch: char) -> bool;
    fn is_identifier_part(&self, ch: char) -> bool;

    // EXTENSION POINT: Custom statement parsing
    fn parse_statement(&self, parser: &mut Parser<'_>)
        -> Option<Result<Statement, ParserError>>;

    // EXTENSION POINT: Custom expression parsing
    fn parse_prefix(&self, parser: &mut Parser<'_>)
        -> Option<Result<Expr, ParserError>>;
    fn parse_infix(&self, parser: &mut Parser<'_>, expr: &Expr, precedence: u8)
        -> Option<Result<Expr, ParserError>>;

    // EXTENSION POINT: Operator precedence
    fn get_next_precedence(&self, parser: &Parser<'_>)
        -> Option<Result<u8, ParserError>>;

    // Feature flags (many more available)
    fn supports_create_index_with_clause(&self) -> bool;
    fn supports_named_fn_args_with_eq_operator(&self) -> bool;
    // ... 50+ feature flags
}
```

### How parse_statement() Works

The parser's statement parsing flow:

```rust
// In Parser::parse_statement()
pub fn parse_statement(&mut self) -> Result<Statement, ParserError> {
    // 1. FIRST: Check if dialect wants to handle this
    if let Some(statement) = self.dialect.parse_statement(self) {
        return statement;  // Dialect handled it
    }

    // 2. FALLBACK: Default keyword-based parsing
    let token = self.next_token();
    match token {
        Token::Word(w) => match w.keyword {
            Keyword::SELECT => self.parse_select(),
            Keyword::CREATE => self.parse_create(),
            // ... 60+ keyword handlers
        }
    }
}
```

### Parser Methods for Token Manipulation

```rust
impl Parser<'_> {
    // Peek without consuming
    fn peek_token(&self) -> TokenWithSpan;
    fn peek_nth_token(&self, n: usize) -> TokenWithSpan;

    // Consume and return
    fn next_token(&mut self) -> TokenWithSpan;
    fn advance_token(&mut self);

    // Conditional consumption
    fn consume_token(&mut self, expected: &Token) -> bool;
    fn parse_keyword(&mut self, kw: Keyword) -> bool;
    fn parse_keywords(&mut self, kws: &[Keyword]) -> bool;
    fn parse_one_of_keywords(&mut self, kws: &[Keyword]) -> Option<Keyword>;

    // Required consumption (errors if not matched)
    fn expect_token(&mut self, expected: &Token) -> Result<TokenWithSpan, ParserError>;
    fn expect_keyword(&mut self, kw: Keyword) -> Result<TokenWithSpan, ParserError>;
    fn expect_keywords(&mut self, kws: &[Keyword]) -> Result<(), ParserError>;

    // Identifier parsing
    fn parse_identifier(&mut self) -> Result<Ident, ParserError>;
    fn parse_object_name(&mut self) -> Result<ObjectName, ParserError>;

    // Expression parsing
    fn parse_expr(&mut self) -> Result<Expr, ParserError>;
    fn parse_subexpr(&mut self, precedence: u8) -> Result<Expr, ParserError>;

    // Utility
    fn parse_comma_separated<T, F>(&mut self, f: F) -> Result<Vec<T>, ParserError>
        where F: FnMut(&mut Self) -> Result<T, ParserError>;
}
```

### Token Enum (Key Variants)

```rust
pub enum Token {
    // Identifiers and keywords
    Word(Word),           // Keywords and identifiers

    // Literals
    Number(String, bool), // Numeric literals
    SingleQuotedString(String),
    NationalStringLiteral(String),  // N'...'

    // Delimiters
    LParen, RParen,       // ( )
    LBracket, RBracket,   // [ ]
    LBrace, RBrace,       // { }
    Comma, SemiColon,
    Period, Colon,

    // Operators
    Eq,                   // =
    Neq,                  // <> or !=
    Plus, Minus, Mul, Div,

    // Special
    AtSign,               // @ (for parameters)
    EOF,
}

pub struct Word {
    pub value: String,           // Original text
    pub quote_style: Option<char>, // None, '"', '`', '['
    pub keyword: Keyword,        // Keyword::NoKeyword if not a keyword
}
```

---

## Implementation Strategy

### Phase 15.1: Infrastructure

Create the custom dialect wrapper:

```rust
// src/parser/tsql_dialect.rs

use sqlparser::dialect::{Dialect, MsSqlDialect};
use sqlparser::parser::Parser;
use sqlparser::ast::Statement;

/// Extended T-SQL dialect with full stored procedure/function support
#[derive(Debug, Default)]
pub struct ExtendedTsqlDialect {
    base: MsSqlDialect,
}

impl Dialect for ExtendedTsqlDialect {
    // Delegate identifier rules to base
    fn is_identifier_start(&self, ch: char) -> bool {
        self.base.is_identifier_start(ch)
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        self.base.is_identifier_part(ch)
    }

    // Custom statement parsing
    fn parse_statement(&self, parser: &mut Parser<'_>)
        -> Option<Result<Statement, ParserError>>
    {
        // Check token patterns and dispatch to custom parsers
        let token = parser.peek_token();

        match &token.token {
            Token::Word(w) if w.keyword == Keyword::CREATE => {
                // Peek further to determine statement type
                if self.is_create_procedure(parser) {
                    return Some(self.parse_create_procedure(parser));
                }
                if self.is_create_function(parser) {
                    return Some(self.parse_create_function(parser));
                }
                // ... more CREATE variants
            }
            Token::Word(w) if w.keyword == Keyword::ALTER => {
                // Handle ALTER statements
            }
            _ => {}
        }

        // Delegate to MsSqlDialect's parse_statement
        self.base.parse_statement(parser)
    }

    // Delegate all feature flags to base
    fn supports_outer_join_operator(&self) -> bool {
        self.base.supports_outer_join_operator()
    }
    // ... delegate all other methods
}
```

### Detecting Statement Types (Without Regex)

```rust
impl ExtendedTsqlDialect {
    /// Check if this is CREATE [OR ALTER] PROCEDURE
    fn is_create_procedure(&self, parser: &Parser<'_>) -> bool {
        // Peek at upcoming tokens without consuming
        let t0 = parser.peek_token();      // CREATE
        let t1 = parser.peek_nth_token(1); // OR | PROCEDURE | PROC
        let t2 = parser.peek_nth_token(2); // ALTER | name
        let t3 = parser.peek_nth_token(3); // PROCEDURE | PROC | name

        // CREATE PROCEDURE | CREATE PROC
        if matches!(&t1.token, Token::Word(w)
            if w.keyword == Keyword::PROCEDURE || w.value.eq_ignore_ascii_case("PROC"))
        {
            return true;
        }

        // CREATE OR ALTER PROCEDURE | CREATE OR ALTER PROC
        if matches!(&t1.token, Token::Word(w) if w.keyword == Keyword::OR)
            && matches!(&t2.token, Token::Word(w) if w.keyword == Keyword::ALTER)
            && matches!(&t3.token, Token::Word(w)
                if w.keyword == Keyword::PROCEDURE || w.value.eq_ignore_ascii_case("PROC"))
        {
            return true;
        }

        false
    }
}
```

### Parsing Object Names

```rust
impl ExtendedTsqlDialect {
    /// Parse [schema].[name] or schema.name or [name] or name
    fn parse_schema_qualified_name(&self, parser: &mut Parser<'_>)
        -> Result<(String, String), ParserError>
    {
        let first = parser.parse_identifier()?;

        if parser.consume_token(&Token::Period) {
            let second = parser.parse_identifier()?;
            Ok((first.value, second.value))
        } else {
            Ok(("dbo".to_string(), first.value))
        }
    }
}
```

### Parsing Parameter Lists

```rust
impl ExtendedTsqlDialect {
    /// Parse procedure parameters: (@p1 INT, @p2 VARCHAR(50) = 'default', ...)
    fn parse_procedure_parameters(&self, parser: &mut Parser<'_>)
        -> Result<Vec<ProcedureParam>, ParserError>
    {
        let mut params = Vec::new();

        if !parser.consume_token(&Token::LParen) {
            return Ok(params); // No parameters
        }

        if parser.consume_token(&Token::RParen) {
            return Ok(params); // Empty parameter list
        }

        loop {
            // Expect @name
            parser.expect_token(&Token::AtSign)?;
            let name = parser.parse_identifier()?;

            // Parse data type (may be complex: DECIMAL(18, 2))
            let data_type = parser.parse_data_type()?;

            // Check for default value
            let default = if parser.consume_token(&Token::Eq) {
                Some(parser.parse_expr()?)
            } else {
                None
            };

            // Check for OUTPUT keyword
            let is_output = parser.parse_keyword(Keyword::OUTPUT)
                         || parser.parse_keyword(Keyword::OUT);

            params.push(ProcedureParam {
                name: format!("@{}", name.value),
                data_type,
                default,
                is_output,
            });

            if !parser.consume_token(&Token::Comma) {
                break;
            }
        }

        parser.expect_token(&Token::RParen)?;
        Ok(params)
    }
}
```

### Handling Procedure/Function Bodies

The main challenge is parsing the body, which can contain arbitrary T-SQL. Approach:

```rust
impl ExtendedTsqlDialect {
    /// Consume all tokens until we hit a batch terminator or EOF
    /// Returns the raw SQL text of the body
    fn parse_body_as_raw_sql(&self, parser: &mut Parser<'_>) -> Result<String, ParserError> {
        let start_pos = parser.get_current_position(); // Need to track position
        let mut depth = 0; // Track BEGIN/END nesting

        loop {
            let token = parser.peek_token();

            match &token.token {
                Token::EOF => break,
                Token::Word(w) if w.keyword == Keyword::BEGIN => {
                    depth += 1;
                    parser.advance_token();
                }
                Token::Word(w) if w.keyword == Keyword::END => {
                    if depth == 0 {
                        break; // End of body
                    }
                    depth -= 1;
                    parser.advance_token();
                }
                Token::SemiColon if depth == 0 => {
                    // Statement terminator at top level might end body
                    break;
                }
                _ => {
                    parser.advance_token();
                }
            }
        }

        // Extract original SQL between start and current position
        // (Need access to original input - may need custom approach)
        Ok(self.extract_sql_range(start_pos, parser.get_current_position()))
    }
}
```

**Note**: sqlparser doesn't expose the original SQL easily. Alternatives:

1. Store original SQL alongside parsed AST (current approach)
2. Reconstruct from tokens (lossy - loses formatting)
3. Track token spans and slice original input
4. Use a `Statement::RawSql(String)` variant for bodies

---

## Code Examples

### Example: Parsing CREATE PROCEDURE

```rust
impl ExtendedTsqlDialect {
    fn parse_create_procedure(&self, parser: &mut Parser<'_>)
        -> Result<Statement, ParserError>
    {
        // Consume CREATE
        parser.expect_keyword(Keyword::CREATE)?;

        // Check for OR ALTER
        let or_alter = parser.parse_keywords(&[Keyword::OR, Keyword::ALTER]);

        // Consume PROCEDURE or PROC
        if !parser.parse_keyword(Keyword::PROCEDURE) {
            // Try PROC (not a standard keyword, so check as word)
            let token = parser.next_token();
            if !matches!(&token.token, Token::Word(w)
                if w.value.eq_ignore_ascii_case("PROC"))
            {
                return Err(ParserError::ParserError(
                    "Expected PROCEDURE or PROC".to_string()
                ));
            }
        }

        // Parse name
        let (schema, name) = self.parse_schema_qualified_name(parser)?;

        // Parse parameters
        let params = self.parse_procedure_parameters(parser)?;

        // Consume AS
        parser.expect_keyword(Keyword::AS)?;

        // The body is everything until GO or EOF
        // For now, we can use a custom statement type
        Ok(Statement::CreateProcedure {
            or_alter,
            name: ObjectName(vec![
                Ident::new(schema),
                Ident::new(name),
            ]),
            params,
            body: Box::new(self.parse_procedure_body(parser)?),
        })
    }
}
```

### Example: Parsing CREATE INDEX with T-SQL Extensions

```rust
impl ExtendedTsqlDialect {
    fn parse_create_index(&self, parser: &mut Parser<'_>)
        -> Result<Statement, ParserError>
    {
        parser.expect_keyword(Keyword::CREATE)?;

        let unique = parser.parse_keyword(Keyword::UNIQUE);

        // T-SQL requires CLUSTERED or NONCLUSTERED before INDEX
        let clustered = if parser.parse_keyword(Keyword::CLUSTERED) {
            Some(true)
        } else if parser.parse_keyword(Keyword::NONCLUSTERED) {
            Some(false)
        } else {
            None
        };

        parser.expect_keyword(Keyword::INDEX)?;

        let name = parser.parse_identifier()?;

        parser.expect_keyword(Keyword::ON)?;

        let table_name = parser.parse_object_name()?;

        // Parse column list
        parser.expect_token(&Token::LParen)?;
        let columns = parser.parse_comma_separated(|p| {
            let col = p.parse_identifier()?;
            let descending = p.parse_keyword(Keyword::DESC);
            let _ascending = p.parse_keyword(Keyword::ASC); // consume if present
            Ok(IndexColumn { name: col, descending })
        })?;
        parser.expect_token(&Token::RParen)?;

        // Parse optional INCLUDE clause
        let include = if parser.parse_keyword(Keyword::INCLUDE) {
            parser.expect_token(&Token::LParen)?;
            let cols = parser.parse_comma_separated(|p| p.parse_identifier())?;
            parser.expect_token(&Token::RParen)?;
            Some(cols)
        } else {
            None
        };

        // Parse optional WHERE clause (filtered index)
        let filter = if parser.parse_keyword(Keyword::WHERE) {
            Some(parser.parse_expr()?)
        } else {
            None
        };

        // Parse optional WITH clause
        let options = if parser.parse_keyword(Keyword::WITH) {
            parser.expect_token(&Token::LParen)?;
            let opts = self.parse_index_options(parser)?;
            parser.expect_token(&Token::RParen)?;
            Some(opts)
        } else {
            None
        };

        Ok(Statement::CreateIndex { /* ... */ })
    }
}
```

---

## Migration Path

### Step 1: Create Infrastructure (Week 1)

1. Create `src/parser/tsql_dialect.rs` with `ExtendedTsqlDialect`
2. Add helper methods for token pattern matching
3. Add tests that verify delegation works correctly
4. Switch `parse_sql_file()` to use `ExtendedTsqlDialect`

### Step 2: Migrate One Statement Type (Week 2)

Start with a simpler case like `CREATE SEQUENCE`:

1. Implement `is_create_sequence()` detection
2. Implement `parse_create_sequence()`
3. Verify all sequence tests pass
4. Remove corresponding regex functions

### Step 3: Migrate Procedures/Functions (Weeks 3-4)

These are the most common fallbacks:

1. `CREATE/ALTER PROCEDURE`
2. `CREATE/ALTER FUNCTION` (scalar, table-valued, inline)
3. Verify with comprehensive tests

### Step 4: Migrate Remaining Statement Types (Weeks 5-6)

1. `CREATE TRIGGER`
2. `CREATE TYPE AS TABLE`
3. `CREATE INDEX` (already partially supported)
4. `CREATE FULLTEXT INDEX/CATALOG`

### Step 5: Migrate Column Definition Parsing (Weeks 7-8)

This is the most complex area (inline constraints, defaults, etc.):

1. May need to extend `parse_column_def()`
2. Handle T-SQL-specific column options
3. Migrate all default constraint regex patterns

### Step 6: Cleanup (Week 9)

1. Remove all unused regex functions
2. Update documentation
3. Performance benchmarking

---

## Testing Strategy

### Unit Tests for Dialect

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::parser::Parser;

    fn parse(sql: &str) -> Vec<Statement> {
        let dialect = ExtendedTsqlDialect::default();
        Parser::parse_sql(&dialect, sql).unwrap()
    }

    #[test]
    fn test_create_procedure_simple() {
        let sql = "CREATE PROCEDURE [dbo].[GetUsers] AS SELECT * FROM Users";
        let stmts = parse(sql);
        assert_eq!(stmts.len(), 1);
        // Assert on statement structure
    }

    #[test]
    fn test_create_procedure_with_params() {
        let sql = r#"
            CREATE PROCEDURE [dbo].[GetUser]
                @Id INT,
                @Name VARCHAR(50) = 'default'
            AS
            BEGIN
                SELECT * FROM Users WHERE Id = @Id
            END
        "#;
        let stmts = parse(sql);
        // Assert parameters are parsed correctly
    }
}
```

### Integration Tests

Use existing test fixtures to ensure parity:

```rust
#[test]
fn test_procedures_fixture_parses() {
    let sql = include_str!("../../tests/fixtures/procedures/Procedures/GetUserById.sql");
    let dialect = ExtendedTsqlDialect::default();
    let result = Parser::parse_sql(&dialect, sql);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
}
```

### Regression Tests

Compare output before/after migration:

```rust
#[test]
fn test_fallback_equivalence() {
    let sql = "CREATE PROCEDURE dbo.Test AS SELECT 1";

    // Old regex approach
    let old_result = try_fallback_parse(sql);

    // New dialect approach
    let dialect = ExtendedTsqlDialect::default();
    let new_result = Parser::parse_sql(&dialect, sql);

    // Verify equivalent information extracted
    assert_eq!(
        extract_schema_name(&old_result),
        extract_schema_name(&new_result)
    );
}
```

---

## References

- [sqlparser-rs GitHub](https://github.com/apache/datafusion-sqlparser-rs)
- [Custom SQL Parser Docs](https://github.com/apache/datafusion-sqlparser-rs/blob/main/docs/custom_sql_parser.md)
- [Dialect Trait Documentation](https://docs.rs/sqlparser/latest/sqlparser/dialect/trait.Dialect.html)
- [Parser Struct Documentation](https://docs.rs/sqlparser/latest/sqlparser/parser/struct.Parser.html)
- [MsSqlDialect Source](https://github.com/apache/datafusion-sqlparser-rs/blob/main/src/dialect/mssql.rs)
- [Databend Parser Blog Post](https://www.databend.com/blog/category-engineering/2025-09-10-query-parser/)

---

## Appendix: Current Regex Inventory

See [IMPLEMENTATION_PLAN.md](./IMPLEMENTATION_PLAN.md#phase-15-parser-refactoring---replace-regex-fallbacks-with-custom-sqlparser-rs-dialect) for the complete list of 75+ regex patterns organized by category.
