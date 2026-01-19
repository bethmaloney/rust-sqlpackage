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
    /// The parsed AST statement
    pub statement: Statement,
    /// Source file path
    pub source_file: PathBuf,
    /// Original SQL text
    pub sql_text: String,
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
                    statements.push(ParsedStatement {
                        statement: stmt,
                        source_file: path.to_path_buf(),
                        sql_text: trimmed.to_string(),
                    });
                }
            }
            Err(e) => {
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

    Ok(statements)
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
