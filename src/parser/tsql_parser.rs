//! T-SQL parser using sqlparser-rs

use std::path::{Path, PathBuf};

use anyhow::Result;
use regex::Regex;
use sqlparser::ast::Statement;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::parser::Parser;

use crate::error::SqlPackageError;

/// A SQL batch with its content and source location
struct Batch<'a> {
    content: &'a str,
    start_line: usize, // 1-based line number
}

/// Extract line number from sqlparser error message (format: "... at Line: X, Column: Y")
fn extract_line_from_error(error_msg: &str) -> Option<usize> {
    let re = Regex::new(r"Line:\s*(\d+)").ok()?;
    let caps = re.captures(error_msg)?;
    caps.get(1)?.as_str().parse().ok()
}

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
    Index {
        name: String,
        table_schema: String,
        table_name: String,
        columns: Vec<String>,
        /// Columns included in the index leaf level (INCLUDE clause)
        include_columns: Vec<String>,
        is_unique: bool,
        is_clustered: bool,
    },
    Sequence {
        schema: String,
        name: String,
    },
    UserDefinedType {
        schema: String,
        name: String,
    },
    /// Generic fallback for any statement that can't be parsed
    RawStatement {
        object_type: String,
        schema: String,
        name: String,
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

    // Strip UTF-8 BOM if present
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(&content);

    // Split on GO statements (batch separator)
    let batches = split_batches(content);

    let dialect = MsSqlDialect {};
    let mut statements = Vec::new();

    for batch in batches {
        let trimmed = batch.content.trim();
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
                    // Calculate absolute line number from batch offset and error line
                    let error_msg = e.to_string();
                    let relative_line = extract_line_from_error(&error_msg).unwrap_or(1);
                    let absolute_line = batch.start_line + relative_line - 1;

                    return Err(SqlPackageError::SqlParseError {
                        path: path.to_path_buf(),
                        line: absolute_line,
                        message: error_msg,
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

    // Check for CREATE CLUSTERED/NONCLUSTERED INDEX (T-SQL specific syntax)
    if sql_upper.contains("CREATE CLUSTERED INDEX")
        || sql_upper.contains("CREATE NONCLUSTERED INDEX")
        || sql_upper.contains("CREATE UNIQUE CLUSTERED INDEX")
        || sql_upper.contains("CREATE UNIQUE NONCLUSTERED INDEX")
    {
        if let Some(index_info) = extract_index_info(sql) {
            return Some(index_info);
        }
    }

    // Check for CREATE SEQUENCE (T-SQL multiline syntax not fully supported by sqlparser)
    if sql_upper.contains("CREATE SEQUENCE") {
        if let Some((schema, name)) = extract_sequence_name(sql) {
            return Some(FallbackStatementType::Sequence { schema, name });
        }
    }

    // Check for CREATE TYPE (user-defined table types)
    if sql_upper.contains("CREATE TYPE") {
        if let Some((schema, name)) = extract_type_name(sql) {
            return Some(FallbackStatementType::UserDefinedType { schema, name });
        }
    }

    // Generic fallback for CREATE TABLE statements that fail parsing
    if sql_upper.contains("CREATE TABLE") {
        if let Some((schema, name)) = extract_generic_object_name(sql, "TABLE") {
            return Some(FallbackStatementType::RawStatement {
                object_type: "Table".to_string(),
                schema,
                name,
            });
        }
    }

    // Generic fallback for any other CREATE statements
    if let Some(fallback) = try_generic_create_fallback(sql) {
        return Some(fallback);
    }

    // Generic fallback for ALTER TABLE statements that can't be parsed
    if sql_upper.contains("ALTER TABLE") {
        if let Some((schema, name)) = extract_alter_table_name(sql) {
            return Some(FallbackStatementType::RawStatement {
                object_type: "AlterTable".to_string(),
                schema,
                name,
            });
        }
    }

    None
}

/// Extract schema and name from ALTER TABLE statement
fn extract_alter_table_name(sql: &str) -> Option<(String, String)> {
    let re = regex::Regex::new(
        r"(?i)ALTER\s+TABLE\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?"
    ).ok()?;

    let caps = re.captures(sql)?;
    let schema = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| "dbo".to_string());
    let name = caps.get(2)?.as_str().to_string();

    Some((schema, name))
}

/// Try to extract any CREATE statement as a generic fallback
fn try_generic_create_fallback(sql: &str) -> Option<FallbackStatementType> {
    let re = regex::Regex::new(
        r"(?i)CREATE\s+(?:OR\s+ALTER\s+)?(\w+)\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?"
    ).ok()?;

    let caps = re.captures(sql)?;
    let object_type = caps.get(1)?.as_str().to_string();
    let schema = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_else(|| "dbo".to_string());
    let name = caps.get(3)?.as_str().to_string();

    Some(FallbackStatementType::RawStatement {
        object_type,
        schema,
        name,
    })
}

/// Extract schema and name for a specific object type
fn extract_generic_object_name(sql: &str, object_type: &str) -> Option<(String, String)> {
    let pattern = format!(
        r"(?i)CREATE\s+(?:OR\s+ALTER\s+)?{}\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?",
        object_type
    );
    let re = regex::Regex::new(&pattern).ok()?;

    let caps = re.captures(sql)?;
    let schema = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| "dbo".to_string());
    let name = caps.get(2)?.as_str().to_string();

    Some((schema, name))
}

/// Extract schema and name from CREATE SEQUENCE statement
fn extract_sequence_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // CREATE SEQUENCE [dbo].[SeqName]
    // CREATE SEQUENCE dbo.SeqName
    let re = regex::Regex::new(
        r"(?i)CREATE\s+SEQUENCE\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?"
    ).ok()?;

    let caps = re.captures(sql)?;
    let schema = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| "dbo".to_string());
    let name = caps.get(2)?.as_str().to_string();

    Some((schema, name))
}

/// Extract schema and name from CREATE TYPE statement
fn extract_type_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // CREATE TYPE [dbo].[TypeName] AS TABLE
    // CREATE TYPE dbo.TypeName AS TABLE
    let re = regex::Regex::new(
        r"(?i)CREATE\s+TYPE\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?"
    ).ok()?;

    let caps = re.captures(sql)?;
    let schema = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| "dbo".to_string());
    let name = caps.get(2)?.as_str().to_string();

    Some((schema, name))
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

/// Extract index information from CREATE CLUSTERED/NONCLUSTERED INDEX statement
fn extract_index_info(sql: &str) -> Option<FallbackStatementType> {
    // Match patterns like:
    // CREATE CLUSTERED INDEX [IX_Name] ON [dbo].[Table] ([Col1], [Col2] DESC)
    // CREATE NONCLUSTERED INDEX [IX_Name] ON [schema].[Table] ([Col]) INCLUDE ([Col2])
    // CREATE UNIQUE CLUSTERED INDEX IX_Name ON dbo.Table (Col)
    // Also handles malformed SQL with missing whitespace (e.g., "]ON" instead of "] ON")
    let re = regex::Regex::new(
        r"(?i)CREATE\s+(UNIQUE\s+)?(CLUSTERED|NONCLUSTERED)\s+INDEX\s+\[?(\w+)\]?\s*ON\s*(?:\[?(\w+)\]?\.)?\[?(\w+)\]?\s*\(([^)]+)\)"
    ).ok()?;

    let caps = re.captures(sql)?;

    let is_unique = caps.get(1).is_some();
    let is_clustered = caps.get(2)
        .map(|m| m.as_str().to_uppercase() == "CLUSTERED")
        .unwrap_or(false);
    let name = caps.get(3)?.as_str().to_string();
    let table_schema = caps.get(4)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dbo".to_string());
    let table_name = caps.get(5)?.as_str().to_string();

    // Parse column list, handling sort direction (ASC/DESC)
    let columns_str = caps.get(6)?.as_str();
    let columns: Vec<String> = parse_column_list(columns_str);

    // Extract INCLUDE columns if present
    let include_columns = extract_include_columns(sql);

    Some(FallbackStatementType::Index {
        name,
        table_schema,
        table_name,
        columns,
        include_columns,
        is_unique,
        is_clustered,
    })
}

/// Parse a comma-separated column list, stripping brackets and sort direction
fn parse_column_list(columns_str: &str) -> Vec<String> {
    columns_str
        .split(',')
        .map(|col| {
            // Extract column name, stripping brackets and sort direction
            let col = col.trim();
            let re_col = regex::Regex::new(r"(?i)\[?(\w+)\]?(?:\s+(?:ASC|DESC))?").ok();
            re_col
                .and_then(|r| r.captures(col))
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| col.to_string())
        })
        .collect()
}

/// Extract columns from INCLUDE clause if present
fn extract_include_columns(sql: &str) -> Vec<String> {
    // Match INCLUDE ([Col1], [Col2], ...)
    let re = regex::Regex::new(r"(?i)INCLUDE\s*\(([^)]+)\)").ok();

    re.and_then(|r| r.captures(sql))
        .and_then(|caps| caps.get(1))
        .map(|m| parse_column_list(m.as_str()))
        .unwrap_or_default()
}

/// Split SQL content into batches by GO statement, tracking line numbers
fn split_batches(content: &str) -> Vec<Batch<'_>> {
    let mut batches = Vec::new();
    let mut current_pos = 0;
    let mut batch_start = 0;
    let mut current_line = 1; // 1-based line numbers
    let mut batch_start_line = 1;

    for line in content.lines() {
        let trimmed = line.trim();
        // Calculate actual line length in the original content (including line ending)
        let line_end = current_pos + line.len();
        let next_pos = if content[line_end..].starts_with("\r\n") {
            line_end + 2
        } else if content[line_end..].starts_with('\n') {
            line_end + 1
        } else {
            line_end // End of file, no newline
        };

        // GO must be on its own line (optionally with whitespace)
        if trimmed.eq_ignore_ascii_case("go") {
            if current_pos > batch_start {
                batches.push(Batch {
                    content: &content[batch_start..current_pos],
                    start_line: batch_start_line,
                });
            }
            batch_start = next_pos;
            batch_start_line = current_line + 1; // Next line after GO
        }

        current_pos = next_pos;
        current_line += 1;
    }

    // Add remaining content
    if batch_start < content.len() {
        batches.push(Batch {
            content: &content[batch_start..],
            start_line: batch_start_line,
        });
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
        assert_eq!(batches[0].start_line, 1);
        assert_eq!(batches[1].start_line, 3);
    }

    #[test]
    fn test_split_batches_no_go() {
        let sql = "CREATE TABLE t1 (id INT)";
        let batches = split_batches(sql);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].start_line, 1);
    }

    #[test]
    fn test_split_batches_line_numbers_multiline() {
        let sql = "-- Comment line 1\n-- Comment line 2\nCREATE TABLE t1 (id INT)\nGO\n-- Comment line 5\nCREATE TABLE t2 (id INT)";
        let batches = split_batches(sql);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].start_line, 1);
        assert!(batches[0].content.contains("CREATE TABLE t1"));
        assert_eq!(batches[1].start_line, 5);
        assert!(batches[1].content.contains("CREATE TABLE t2"));
    }

    #[test]
    fn test_split_batches_multiple_go() {
        let sql = "SELECT 1\nGO\nSELECT 2\nGO\nSELECT 3";
        let batches = split_batches(sql);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].start_line, 1);
        assert_eq!(batches[1].start_line, 3);
        assert_eq!(batches[2].start_line, 5);
    }

    #[test]
    fn test_extract_line_from_error() {
        assert_eq!(extract_line_from_error("Error at Line: 5, Column: 10"), Some(5));
        assert_eq!(extract_line_from_error("Parse error at Line: 123, Column: 1"), Some(123));
        assert_eq!(extract_line_from_error("No line info here"), None);
        assert_eq!(extract_line_from_error("Line:42, Column: 1"), Some(42));
    }
}
