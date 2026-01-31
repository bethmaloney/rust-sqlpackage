# Custom sqlparser-rs Dialect Guide

This document provides guidance for extending sqlparser-rs with a custom T-SQL dialect to handle SQL Server-specific syntax.

## Table of Contents

1. [Overview](#overview)
2. [The Dialect Trait](#the-dialect-trait)
3. [Parser Methods](#parser-methods)
4. [Token Types](#token-types)
5. [Implementation Patterns](#implementation-patterns)
6. [Code Examples](#code-examples)
7. [Testing](#testing)

---

## Overview

sqlparser-rs provides extension points through the `Dialect` trait. The pattern is:

1. Create a custom dialect that wraps `MsSqlDialect`
2. Override `parse_statement()` to intercept specific token patterns
3. Delegate to base dialect for everything else
4. Use the parser's tokenizer (not regex) to extract information

This approach is recommended by the sqlparser-rs maintainers and is used by [DataFusion](https://github.com/apache/datafusion).

---

## The Dialect Trait

The `Dialect` trait provides several override points:

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

### Creating a Custom Dialect

```rust
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
        let token = parser.peek_token();

        match &token.token {
            Token::Word(w) if w.keyword == Keyword::CREATE => {
                if self.is_create_procedure(parser) {
                    return Some(self.parse_create_procedure(parser));
                }
                if self.is_create_function(parser) {
                    return Some(self.parse_create_function(parser));
                }
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
}
```

---

## Parser Methods

### Token Manipulation

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

---

## Token Types

### Key Token Variants

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

## Implementation Patterns

### Detecting Statement Types

Use token peeking to identify statement types without consuming:

```rust
impl ExtendedTsqlDialect {
    /// Check if this is CREATE [OR ALTER] PROCEDURE
    fn is_create_procedure(&self, parser: &Parser<'_>) -> bool {
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

### Parsing Schema-Qualified Names

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

The body can contain arbitrary T-SQL. Track BEGIN/END nesting:

```rust
impl ExtendedTsqlDialect {
    /// Consume all tokens until we hit a batch terminator or EOF
    fn parse_body_as_raw_sql(&self, parser: &mut Parser<'_>) -> Result<String, ParserError> {
        let start_pos = parser.get_current_position();
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
                    break; // Statement terminator at top level
                }
                _ => {
                    parser.advance_token();
                }
            }
        }

        // Extract original SQL between start and current position
        Ok(self.extract_sql_range(start_pos, parser.get_current_position()))
    }
}
```

**Note**: sqlparser doesn't expose the original SQL easily. Alternatives:
1. Store original SQL alongside parsed AST
2. Reconstruct from tokens (lossy - loses formatting)
3. Track token spans and slice original input
4. Use a `Statement::RawSql(String)` variant for bodies

---

## Code Examples

### Parsing CREATE PROCEDURE

```rust
impl ExtendedTsqlDialect {
    fn parse_create_procedure(&self, parser: &mut Parser<'_>)
        -> Result<Statement, ParserError>
    {
        parser.expect_keyword(Keyword::CREATE)?;

        let or_alter = parser.parse_keywords(&[Keyword::OR, Keyword::ALTER]);

        // Consume PROCEDURE or PROC
        if !parser.parse_keyword(Keyword::PROCEDURE) {
            let token = parser.next_token();
            if !matches!(&token.token, Token::Word(w)
                if w.value.eq_ignore_ascii_case("PROC"))
            {
                return Err(ParserError::ParserError(
                    "Expected PROCEDURE or PROC".to_string()
                ));
            }
        }

        let (schema, name) = self.parse_schema_qualified_name(parser)?;
        let params = self.parse_procedure_parameters(parser)?;

        parser.expect_keyword(Keyword::AS)?;

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

### Parsing CREATE INDEX with T-SQL Extensions

```rust
impl ExtendedTsqlDialect {
    fn parse_create_index(&self, parser: &mut Parser<'_>)
        -> Result<Statement, ParserError>
    {
        parser.expect_keyword(Keyword::CREATE)?;

        let unique = parser.parse_keyword(Keyword::UNIQUE);

        // T-SQL: CLUSTERED or NONCLUSTERED before INDEX
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
            let _ascending = p.parse_keyword(Keyword::ASC);
            Ok(IndexColumn { name: col, descending })
        })?;
        parser.expect_token(&Token::RParen)?;

        // Optional INCLUDE clause
        let include = if parser.parse_keyword(Keyword::INCLUDE) {
            parser.expect_token(&Token::LParen)?;
            let cols = parser.parse_comma_separated(|p| p.parse_identifier())?;
            parser.expect_token(&Token::RParen)?;
            Some(cols)
        } else {
            None
        };

        // Optional WHERE clause (filtered index)
        let filter = if parser.parse_keyword(Keyword::WHERE) {
            Some(parser.parse_expr()?)
        } else {
            None
        };

        // Optional WITH clause
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

## Testing

### Unit Tests

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

### Integration Tests with Fixtures

```rust
#[test]
fn test_procedures_fixture_parses() {
    let sql = include_str!("../../tests/fixtures/procedures/Procedures/GetUserById.sql");
    let dialect = ExtendedTsqlDialect::default();
    let result = Parser::parse_sql(&dialect, sql);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
}
```

---

## References

- [sqlparser-rs GitHub](https://github.com/apache/datafusion-sqlparser-rs)
- [Custom SQL Parser Docs](https://github.com/apache/datafusion-sqlparser-rs/blob/main/docs/custom_sql_parser.md)
- [Dialect Trait Documentation](https://docs.rs/sqlparser/latest/sqlparser/dialect/trait.Dialect.html)
- [Parser Struct Documentation](https://docs.rs/sqlparser/latest/sqlparser/parser/struct.Parser.html)
- [MsSqlDialect Source](https://github.com/apache/datafusion-sqlparser-rs/blob/main/src/dialect/mssql.rs)
