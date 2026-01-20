//! T-SQL parser using sqlparser-rs

use std::path::{Path, PathBuf};

use anyhow::Result;
use sqlparser::ast::Statement;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::parser::Parser;

use crate::error::SqlPackageError;

/// A parsed SQL statement with source information
#[derive(Debug, Clone)]
pub struct ParsedStatement {
    /// The parsed AST statement (None for fallback-parsed statements)
    pub statement: Option<Statement>,
    /// Source file path
    pub source_file: PathBuf,
    /// Original SQL text
    pub sql_text: String,
    /// Fallback-parsed statement type (for procedures/functions that sqlparser can't handle)
    pub fallback_type: Option<FallbackStatementType>,
}

/// Statement types that require fallback parsing due to sqlparser limitations
#[derive(Debug, Clone)]
pub enum FallbackStatementType {
    Procedure {
        schema: String,
        name: String,
    },
    Function {
        schema: String,
        name: String,
        function_type: FallbackFunctionType,
    },
}

/// Function type detected from SQL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackFunctionType {
    Scalar,
    TableValued,
}

impl ParsedStatement {
    /// Create a new ParsedStatement from a sqlparser Statement
    pub fn from_statement(statement: Statement, source_file: PathBuf, sql_text: String) -> Self {
        Self {
            statement: Some(statement),
            source_file,
            sql_text,
            fallback_type: None,
        }
    }

    /// Create a new ParsedStatement from fallback parsing
    pub fn from_fallback(
        fallback_type: FallbackStatementType,
        source_file: PathBuf,
        sql_text: String,
    ) -> Self {
        Self {
            statement: None,
            source_file,
            sql_text,
            fallback_type: Some(fallback_type),
        }
    }
}

/// Parse multiple SQL files
pub fn parse_sql_files(files: &[PathBuf]) -> Result<Vec<ParsedStatement>> {
    let mut all_statements = Vec::new();

    for file in files {
        let statements = parse_sql_file(file)?;
        all_statements.extend(statements);
    }

    Ok(all_statements)
}

/// Parse a single SQL file
pub fn parse_sql_file(path: &Path) -> Result<Vec<ParsedStatement>> {
    let content = std::fs::read_to_string(path).map_err(|e| SqlPackageError::SqlFileReadError {
        path: path.to_path_buf(),
        source: e,
    })?;

    // Split on GO statements (batch separator)
    let batches = split_batches(&content);

    let dialect = MsSqlDialect {};
    let mut statements = Vec::new();

    for batch in batches {
        let trimmed = batch.trim();
        if trimmed.is_empty() {
            continue;
        }

        match Parser::parse_sql(&dialect, trimmed) {
            Ok(parsed) => {
                for stmt in parsed {
                    statements.push(ParsedStatement::from_statement(
                        stmt,
                        path.to_path_buf(),
                        trimmed.to_string(),
                    ));
                }
            }
            Err(e) => {
                // Try fallback parsing for procedures and functions
                // sqlparser has limited T-SQL support for these statement types
                if let Some(fallback) = try_fallback_parse(trimmed) {
                    statements.push(ParsedStatement::from_fallback(
                        fallback,
                        path.to_path_buf(),
                        trimmed.to_string(),
                    ));
                } else {
                    // Calculate line number for error
                    let line = content[..content.find(trimmed).unwrap_or(0)]
                        .lines()
                        .count()
                        + 1;

                    return Err(SqlPackageError::SqlParseError {
                        path: path.to_path_buf(),
                        line,
                        message: e.to_string(),
                    }
                    .into());
                }
            }
        }
    }

    Ok(statements)
}

/// Try to parse a statement using fallback regex-based parsing
/// Returns Some(FallbackStatementType) if the statement is a procedure or function
fn try_fallback_parse(sql: &str) -> Option<FallbackStatementType> {
    let sql_upper = sql.to_uppercase();

    // Check for CREATE PROCEDURE or CREATE PROC (T-SQL shorthand)
    if sql_upper.contains("CREATE PROCEDURE")
        || sql_upper.contains("CREATE OR ALTER PROCEDURE")
        || sql_upper.contains("CREATE PROC")
        || sql_upper.contains("CREATE OR ALTER PROC")
    {
        if let Some((schema, name)) = extract_procedure_name(sql) {
            return Some(FallbackStatementType::Procedure { schema, name });
        }
    }

    // Check for CREATE FUNCTION
    if sql_upper.contains("CREATE FUNCTION") || sql_upper.contains("CREATE OR ALTER FUNCTION") {
        if let Some((schema, name)) = extract_function_name(sql) {
            let function_type = detect_function_type(sql);
            return Some(FallbackStatementType::Function {
                schema,
                name,
                function_type,
            });
        }
    }

    None
}

/// Extract schema and name from CREATE PROCEDURE statement
fn extract_procedure_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // CREATE PROCEDURE [dbo].[ProcName]
    // CREATE PROCEDURE dbo.ProcName
    // CREATE OR ALTER PROCEDURE [schema].[name]
    // CREATE PROC [dbo].[name]
    let re = regex::Regex::new(
        r"(?i)CREATE\s+(?:OR\s+ALTER\s+)?(?:PROCEDURE|PROC)\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?"
    ).ok()?;

    let caps = re.captures(sql)?;
    let schema = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| "dbo".to_string());
    let name = caps.get(2)?.as_str().to_string();

    Some((schema, name))
}

/// Extract schema and name from CREATE FUNCTION statement
fn extract_function_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // CREATE FUNCTION [dbo].[FuncName]
    // CREATE FUNCTION dbo.FuncName
    // CREATE OR ALTER FUNCTION [schema].[name]
    let re = regex::Regex::new(
        r"(?i)CREATE\s+(?:OR\s+ALTER\s+)?FUNCTION\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?"
    ).ok()?;

    let caps = re.captures(sql)?;
    let schema = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| "dbo".to_string());
    let name = caps.get(2)?.as_str().to_string();

    Some((schema, name))
}

/// Detect function type from SQL definition
fn detect_function_type(sql: &str) -> FallbackFunctionType {
    let sql_upper = sql.to_uppercase();

    // Table-valued functions return TABLE
    if sql_upper.contains("RETURNS TABLE") || sql_upper.contains("RETURNS @") {
        FallbackFunctionType::TableValued
    } else {
        FallbackFunctionType::Scalar
    }
}

/// Split SQL content into batches by GO statement
fn split_batches(content: &str) -> Vec<&str> {
    let mut batches = Vec::new();
    let mut start = 0;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        // GO must be on its own line (optionally with whitespace)
        if trimmed.eq_ignore_ascii_case("go") {
            let line_start = content
                .lines()
                .take(i)
                .map(|l| l.len() + 1) // +1 for newline
                .sum::<usize>();

            if line_start > start {
                batches.push(&content[start..line_start]);
            }
            start = line_start + line.len() + 1; // Skip past GO and newline
        }
    }

    // Add remaining content
    if start < content.len() {
        batches.push(&content[start..]);
    }

    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_batches() {
        let sql = "CREATE TABLE t1 (id INT)\nGO\nCREATE TABLE t2 (id INT)";
        let batches = split_batches(sql);
        assert_eq!(batches.len(), 2);
    }

    #[test]
    fn test_split_batches_no_go() {
        let sql = "CREATE TABLE t1 (id INT)";
        let batches = split_batches(sql);
        assert_eq!(batches.len(), 1);
    }
}
