//! T-SQL parser using sqlparser-rs

use std::path::{Path, PathBuf};

use anyhow::Result;
use rayon::prelude::*;
use regex::Regex;
use sqlparser::ast::Statement;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer};

use super::column_parser::{parse_column_definition_tokens, TokenParsedColumn};
use super::constraint_parser::{
    parse_alter_table_add_constraint_tokens, parse_alter_table_name_tokens,
    parse_table_constraint_tokens, TokenParsedConstraint,
};
use super::fulltext_parser::{parse_fulltext_catalog_tokens, parse_fulltext_index_tokens};
use super::function_parser::{
    detect_function_type_tokens, parse_alter_function_tokens, parse_create_function_full,
    parse_create_function_tokens, TokenParsedFunctionType,
};
use super::identifier_utils::format_token_sql;
use super::index_parser::{extract_index_filter_predicate_tokenized, parse_create_index_tokens};
use super::preprocess_parser::preprocess_tsql_tokens;
use super::procedure_parser::{parse_alter_procedure_tokens, parse_create_procedure_tokens};
use super::sequence_parser::{parse_alter_sequence_tokens, parse_create_sequence_tokens};
use super::statement_parser::{
    try_parse_cte_dml_tokens, try_parse_drop_tokens, try_parse_generic_create_tokens,
    try_parse_merge_output_tokens, try_parse_xml_update_tokens,
};
use super::table_type_parser::parse_create_table_type_tokens;
use super::trigger_parser::parse_create_trigger_tokens;
use super::tsql_dialect::ExtendedTsqlDialect;
use crate::error::SqlPackageError;

/// Sentinel value used to represent MAX in binary types (since sqlparser expects u64)
pub const BINARY_MAX_SENTINEL: u64 = 2_147_483_647;

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

/// A default constraint extracted during preprocessing
#[derive(Debug, Clone)]
pub struct ExtractedDefaultConstraint {
    /// Constraint name (e.g., "DF_Products_IsActive")
    pub name: String,
    /// Column the default applies to
    pub column: String,
    /// Default value expression (e.g., "(1)" or "(GETDATE())")
    pub expression: String,
}

/// An extended property extracted from sp_addextendedproperty call
#[derive(Debug, Clone)]
pub struct ExtractedExtendedProperty {
    /// Property name (e.g., "MS_Description")
    pub property_name: String,
    /// Property value (e.g., "Description text")
    pub property_value: String,
    /// Level 0 name (schema, e.g., "dbo")
    pub level0name: String,
    /// Level 1 type (e.g., "TABLE")
    pub level1type: Option<String>,
    /// Level 1 name (e.g., "DocumentedTable")
    pub level1name: Option<String>,
    /// Level 2 type (e.g., "COLUMN")
    pub level2type: Option<String>,
    /// Level 2 name (e.g., "Id")
    pub level2name: Option<String>,
}

/// A column extracted from a table type definition
#[derive(Debug, Clone)]
pub struct ExtractedTableTypeColumn {
    /// Column name
    pub name: String,
    /// Data type (e.g., "NVARCHAR(50)", "INT", "DECIMAL(18, 2)")
    pub data_type: String,
    /// Column nullability: Some(true) = explicit NULL, Some(false) = explicit NOT NULL, None = implicit
    pub nullability: Option<bool>,
    /// Default value expression (if any)
    pub default_value: Option<String>,
}

/// A constraint extracted from a table type definition
#[derive(Debug, Clone)]
pub enum ExtractedTableTypeConstraint {
    PrimaryKey {
        columns: Vec<ExtractedConstraintColumn>,
        is_clustered: bool,
    },
    Unique {
        columns: Vec<ExtractedConstraintColumn>,
        is_clustered: bool,
    },
    Check {
        expression: String,
    },
    Index {
        name: String,
        columns: Vec<String>,
        is_unique: bool,
        is_clustered: bool,
    },
}

/// A parameter extracted from a function definition
#[derive(Debug, Clone)]
pub struct ExtractedFunctionParameter {
    /// Parameter name (including @ prefix)
    pub name: String,
    /// Data type (e.g., "INT", "DECIMAL(18, 2)")
    pub data_type: String,
}

/// A column extracted from a table definition (with additional properties)
#[derive(Debug, Clone)]
pub struct ExtractedTableColumn {
    /// Column name
    pub name: String,
    /// Data type (e.g., "NVARCHAR(50)", "INT", "DECIMAL(18, 2)")
    /// For computed columns, this will be empty
    pub data_type: String,
    /// Column nullability: Some(true) = explicit NULL, Some(false) = explicit NOT NULL, None = implicit
    pub nullability: Option<bool>,
    /// Whether the column has IDENTITY
    pub is_identity: bool,
    /// Whether the column has ROWGUIDCOL
    pub is_rowguidcol: bool,
    /// Whether the column has SPARSE attribute
    pub is_sparse: bool,
    /// Whether the column has FILESTREAM attribute
    pub is_filestream: bool,
    /// Default constraint name (if any)
    pub default_constraint_name: Option<String>,
    /// Default value expression (if any)
    pub default_value: Option<String>,
    /// Whether to emit the default constraint name in XML output
    /// True if CONSTRAINT [name] appeared AFTER NOT NULL (DotNet compatibility)
    pub emit_default_constraint_name: bool,
    /// Inline CHECK constraint name (if any)
    pub check_constraint_name: Option<String>,
    /// Inline CHECK constraint expression (if any)
    pub check_expression: Option<String>,
    /// Whether to emit the check constraint name in XML output
    /// True if CONSTRAINT [name] appeared AFTER NOT NULL (DotNet compatibility)
    pub emit_check_constraint_name: bool,
    /// Computed column expression (e.g., "[Qty] * [Price]")
    /// If Some, this is a computed column with no explicit data type
    pub computed_expression: Option<String>,
    /// Whether the computed column is PERSISTED (stored physically)
    pub is_persisted: bool,
}

/// A column reference in a constraint with optional sort direction
#[derive(Debug, Clone)]
pub struct ExtractedConstraintColumn {
    pub name: String,
    pub descending: bool,
}

/// Extracted table constraint
#[derive(Debug, Clone)]
pub enum ExtractedTableConstraint {
    PrimaryKey {
        name: String,
        columns: Vec<ExtractedConstraintColumn>,
        is_clustered: bool,
    },
    ForeignKey {
        name: String,
        columns: Vec<String>,
        referenced_table: String,
        referenced_columns: Vec<String>,
    },
    Unique {
        name: String,
        columns: Vec<ExtractedConstraintColumn>,
        is_clustered: bool,
    },
    Check {
        name: String,
        expression: String,
    },
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
    /// Default constraints extracted during preprocessing (T-SQL DEFAULT FOR syntax)
    pub extracted_defaults: Vec<ExtractedDefaultConstraint>,
}

/// A column in a full-text index with optional language specification
#[derive(Debug, Clone)]
pub struct ExtractedFullTextColumn {
    /// Column name
    pub name: String,
    /// Language ID (e.g., 1033 for English)
    pub language_id: Option<u32>,
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
        parameters: Vec<ExtractedFunctionParameter>,
        return_type: Option<String>,
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
        /// Fill factor percentage (0-100)
        fill_factor: Option<u8>,
        /// Filter predicate for filtered indexes (WHERE clause condition)
        filter_predicate: Option<String>,
        /// Data compression type (NONE, ROW, PAGE, etc.)
        data_compression: Option<String>,
    },
    /// Full-text index (CREATE FULLTEXT INDEX ON table ...)
    FullTextIndex {
        table_schema: String,
        table_name: String,
        columns: Vec<ExtractedFullTextColumn>,
        /// Key index name (required for full-text index)
        key_index: String,
        /// Full-text catalog name (optional, defaults to default catalog)
        catalog: Option<String>,
        /// Change tracking mode (AUTO, MANUAL, OFF)
        change_tracking: Option<String>,
    },
    /// Full-text catalog (CREATE FULLTEXT CATALOG ...)
    FullTextCatalog {
        name: String,
        is_default: bool,
    },
    Sequence {
        schema: String,
        name: String,
        /// Data type (e.g., "INT", "BIGINT")
        data_type: Option<String>,
        /// START WITH value
        start_value: Option<i64>,
        /// INCREMENT BY value
        increment_value: Option<i64>,
        /// MINVALUE value (None means NO MINVALUE)
        min_value: Option<i64>,
        /// MAXVALUE value (None means NO MAXVALUE)
        max_value: Option<i64>,
        /// CYCLE / NO CYCLE
        is_cycling: bool,
        /// Explicit NO MINVALUE
        has_no_min_value: bool,
        /// Explicit NO MAXVALUE
        has_no_max_value: bool,
        /// CACHE size (None means default cache)
        cache_size: Option<i64>,
    },
    UserDefinedType {
        schema: String,
        name: String,
        columns: Vec<ExtractedTableTypeColumn>,
        constraints: Vec<ExtractedTableTypeConstraint>,
    },
    /// Scalar type (alias type) - CREATE TYPE x FROM basetype
    ScalarType {
        schema: String,
        name: String,
        /// The base type (e.g., VARCHAR, DECIMAL, NVARCHAR)
        base_type: String,
        /// Whether this type allows NULL values (false if NOT NULL specified)
        is_nullable: bool,
        /// Length for string types
        length: Option<i32>,
        /// Precision for decimal types
        precision: Option<u8>,
        /// Scale for decimal types
        scale: Option<u8>,
    },
    /// Fallback for CREATE TABLE statements with T-SQL syntax not supported by sqlparser
    Table {
        schema: String,
        name: String,
        columns: Vec<ExtractedTableColumn>,
        constraints: Vec<ExtractedTableConstraint>,
        /// Whether this is a graph node table (CREATE TABLE AS NODE)
        is_node: bool,
        /// Whether this is a graph edge table (CREATE TABLE AS EDGE)
        is_edge: bool,
    },
    /// Generic fallback for any statement that can't be parsed
    RawStatement {
        object_type: String,
        schema: String,
        name: String,
    },
    /// DML Trigger (CREATE TRIGGER ... ON table/view FOR/AFTER/INSTEAD OF INSERT/UPDATE/DELETE)
    Trigger {
        schema: String,
        name: String,
        /// Schema of the parent table/view
        parent_schema: String,
        /// Name of the parent table/view
        parent_name: String,
        /// True if trigger fires on INSERT
        is_insert: bool,
        /// True if trigger fires on UPDATE
        is_update: bool,
        /// True if trigger fires on DELETE
        is_delete: bool,
        /// Trigger type: 2 = AFTER/FOR, 3 = INSTEAD OF
        trigger_type: u8,
    },
    /// Extended property from sp_addextendedproperty
    ExtendedProperty {
        property: ExtractedExtendedProperty,
    },
    /// Constraint added via ALTER TABLE ... ADD CONSTRAINT
    AlterTableAddConstraint {
        table_schema: String,
        table_name: String,
        constraint: ExtractedTableConstraint,
    },
}

/// Function type detected from SQL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackFunctionType {
    Scalar,
    TableValued,
    InlineTableValued,
}

impl ParsedStatement {
    /// Create a new ParsedStatement from a sqlparser Statement
    pub fn from_statement(statement: Statement, source_file: PathBuf, sql_text: String) -> Self {
        Self {
            statement: Some(statement),
            source_file,
            sql_text,
            fallback_type: None,
            extracted_defaults: Vec::new(),
        }
    }

    /// Create a new ParsedStatement from a sqlparser Statement with extracted defaults
    pub fn from_statement_with_defaults(
        statement: Statement,
        source_file: PathBuf,
        sql_text: String,
        extracted_defaults: Vec<ExtractedDefaultConstraint>,
    ) -> Self {
        Self {
            statement: Some(statement),
            source_file,
            sql_text,
            fallback_type: None,
            extracted_defaults,
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
            extracted_defaults: Vec::new(),
        }
    }
}

/// Minimum number of files to benefit from parallel processing.
/// Below this threshold, sequential processing is faster due to rayon overhead.
const PARALLEL_THRESHOLD: usize = 8;

/// Parse multiple SQL files, using parallel processing for larger file sets
pub fn parse_sql_files(files: &[PathBuf]) -> Result<Vec<ParsedStatement>> {
    // Pre-allocate with estimate of ~2 statements per file
    let mut all_statements = Vec::with_capacity(files.len() * 2);

    if files.len() >= PARALLEL_THRESHOLD {
        // Parse files in parallel using rayon for larger projects
        let results: Vec<Result<Vec<ParsedStatement>>> =
            files.par_iter().map(|file| parse_sql_file(file)).collect();

        // Combine results, propagating the first error if any
        for result in results {
            all_statements.extend(result?);
        }
    } else {
        // Sequential processing for small projects (avoids rayon overhead)
        for file in files {
            let statements = parse_sql_file(file)?;
            all_statements.extend(statements);
        }
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

    let dialect = ExtendedTsqlDialect::new();
    // Estimate ~1 statement per batch on average
    let mut statements = Vec::with_capacity(batches.len());

    for batch in batches {
        let trimmed = batch.content.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Preprocess T-SQL to handle syntax that sqlparser doesn't support
        let preprocessed = preprocess_tsql(trimmed);

        match Parser::parse_sql(&dialect, &preprocessed.sql) {
            Ok(parsed) => {
                for stmt in parsed {
                    // Use the original SQL text, not preprocessed, for storage
                    // but include any extracted defaults
                    if preprocessed.extracted_defaults.is_empty() {
                        statements.push(ParsedStatement::from_statement(
                            stmt,
                            path.to_path_buf(),
                            trimmed.to_string(),
                        ));
                    } else {
                        statements.push(ParsedStatement::from_statement_with_defaults(
                            stmt,
                            path.to_path_buf(),
                            trimmed.to_string(),
                            preprocessed.extracted_defaults.clone(),
                        ));
                    }
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

    // Check for ALTER PROCEDURE or ALTER PROC (T-SQL shorthand)
    // Note: sqlparser doesn't support ALTER PROCEDURE, so we use fallback
    if sql_upper.contains("ALTER PROCEDURE") || sql_upper.contains("ALTER PROC") {
        if let Some((schema, name)) = extract_alter_procedure_name(sql) {
            return Some(FallbackStatementType::Procedure { schema, name });
        }
    }

    // Check for CREATE FUNCTION
    if sql_upper.contains("CREATE FUNCTION") || sql_upper.contains("CREATE OR ALTER FUNCTION") {
        if let Some((schema, name)) = extract_function_name(sql) {
            let function_type = detect_function_type(sql);
            let parameters = extract_function_parameters(sql);
            let return_type = extract_function_return_type(sql);
            return Some(FallbackStatementType::Function {
                schema,
                name,
                function_type,
                parameters,
                return_type,
            });
        }
    }

    // Check for ALTER FUNCTION
    // Note: sqlparser doesn't support ALTER FUNCTION, so we use fallback
    if sql_upper.contains("ALTER FUNCTION") {
        if let Some((schema, name)) = extract_alter_function_name(sql) {
            let function_type = detect_function_type(sql);
            let parameters = extract_function_parameters(sql);
            let return_type = extract_function_return_type(sql);
            return Some(FallbackStatementType::Function {
                schema,
                name,
                function_type,
                parameters,
                return_type,
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

    // Check for CREATE FULLTEXT INDEX (must check before generic CREATE fallback)
    // Use token-based parser (Phase 15.3 B7)
    if sql_upper.contains("CREATE FULLTEXT INDEX") {
        if let Some(parsed) = parse_fulltext_index_tokens(sql) {
            let columns = parsed
                .columns
                .into_iter()
                .map(|c| ExtractedFullTextColumn {
                    name: c.name,
                    language_id: c.language_id,
                })
                .collect();
            return Some(FallbackStatementType::FullTextIndex {
                table_schema: parsed.table_schema,
                table_name: parsed.table_name,
                columns,
                key_index: parsed.key_index,
                catalog: parsed.catalog,
                change_tracking: parsed.change_tracking,
            });
        }
    }

    // Check for CREATE FULLTEXT CATALOG
    // Use token-based parser (Phase 15.3 B8)
    if sql_upper.contains("CREATE FULLTEXT CATALOG") {
        if let Some(parsed) = parse_fulltext_catalog_tokens(sql) {
            return Some(FallbackStatementType::FullTextCatalog {
                name: parsed.name,
                is_default: parsed.is_default,
            });
        }
    }

    // Check for CREATE SEQUENCE (T-SQL multiline syntax not fully supported by sqlparser)
    if sql_upper.contains("CREATE SEQUENCE") {
        if let Some(seq_info) = extract_sequence_info(sql) {
            return Some(FallbackStatementType::Sequence {
                schema: seq_info.schema,
                name: seq_info.name,
                data_type: seq_info.data_type,
                start_value: seq_info.start_value,
                increment_value: seq_info.increment_value,
                min_value: seq_info.min_value,
                max_value: seq_info.max_value,
                is_cycling: seq_info.is_cycling,
                has_no_min_value: seq_info.has_no_min_value,
                has_no_max_value: seq_info.has_no_max_value,
                cache_size: seq_info.cache_size,
            });
        }
    }

    // Check for ALTER SEQUENCE
    // Note: sqlparser doesn't support ALTER SEQUENCE, so we use fallback
    if sql_upper.contains("ALTER SEQUENCE") {
        if let Some(seq_info) = extract_alter_sequence_info(sql) {
            return Some(FallbackStatementType::Sequence {
                schema: seq_info.schema,
                name: seq_info.name,
                data_type: seq_info.data_type,
                start_value: seq_info.start_value,
                increment_value: seq_info.increment_value,
                min_value: seq_info.min_value,
                max_value: seq_info.max_value,
                is_cycling: seq_info.is_cycling,
                has_no_min_value: seq_info.has_no_min_value,
                has_no_max_value: seq_info.has_no_max_value,
                cache_size: seq_info.cache_size,
            });
        }
    }

    // Check for CREATE TYPE (user-defined types)
    // Scalar types use: CREATE TYPE x FROM basetype
    // Table types use: CREATE TYPE x AS TABLE
    // Uses token-based parsing (Phase 15.8 J6) to handle any whitespace between keywords
    if sql_upper.contains("CREATE TYPE") {
        // Check if this is a scalar type (FROM basetype) or table type (AS TABLE)
        match is_scalar_type_definition(sql) {
            Some(true) => {
                // Scalar type - CREATE TYPE [dbo].[TypeName] FROM basetype [NULL|NOT NULL]
                if let Some((schema, name)) = extract_type_name(sql) {
                    if let Some(scalar_info) = extract_scalar_type_info(sql, &sql_upper) {
                        return Some(FallbackStatementType::ScalarType {
                            schema,
                            name,
                            base_type: scalar_info.base_type,
                            is_nullable: scalar_info.is_nullable,
                            length: scalar_info.length,
                            precision: scalar_info.precision,
                            scale: scalar_info.scale,
                        });
                    }
                }
            }
            Some(false) | None => {
                // Table type - CREATE TYPE x AS TABLE (...)
                // Uses token-based parsing (Phase 15.3) for improved maintainability and edge case handling.
                if let Some(parsed) = parse_create_table_type_tokens(sql) {
                    return Some(FallbackStatementType::UserDefinedType {
                        schema: parsed.schema,
                        name: parsed.name,
                        columns: parsed.columns,
                        constraints: parsed.constraints,
                    });
                }
            }
        }
    }

    // Fallback for CREATE TABLE statements that fail parsing
    if sql_upper.contains("CREATE TABLE") {
        if let Some(table_info) = extract_table_structure(sql, &sql_upper) {
            return Some(table_info);
        }
    }

    // Check for EXEC sp_addextendedproperty
    if sql_upper.contains("SP_ADDEXTENDEDPROPERTY") {
        if let Some(property) = extract_extended_property_from_sql(sql) {
            return Some(FallbackStatementType::ExtendedProperty { property });
        }
    }

    // Check for CREATE TRIGGER
    if sql_upper.contains("CREATE TRIGGER") || sql_upper.contains("CREATE OR ALTER TRIGGER") {
        if let Some(trigger) = extract_trigger_info(sql) {
            return Some(trigger);
        }
    }

    // Generic fallback for any other CREATE statements
    if let Some(fallback) = try_generic_create_fallback(sql) {
        return Some(fallback);
    }

    // Check for ALTER TABLE ... ADD CONSTRAINT
    if sql_upper.contains("ALTER TABLE") && sql_upper.contains("ADD CONSTRAINT") {
        if let Some(fallback) = extract_alter_table_add_constraint(sql) {
            return Some(fallback);
        }
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

    // Fallback for DROP statements that sqlparser doesn't support
    if let Some(fallback) = try_drop_fallback(sql) {
        return Some(fallback);
    }

    // Fallback for CTE with DELETE/UPDATE/INSERT/MERGE
    if let Some(fallback) = try_cte_dml_fallback(sql) {
        return Some(fallback);
    }

    // Fallback for MERGE with OUTPUT clause
    if let Some(fallback) = try_merge_output_fallback(sql) {
        return Some(fallback);
    }

    // Fallback for UPDATE with XML methods (.modify(), .value(), etc.)
    if let Some(fallback) = try_xml_method_fallback(sql) {
        return Some(fallback);
    }

    None
}

/// Fallback for DROP statements that sqlparser doesn't support
/// Handles: DROP SYNONYM, DROP TRIGGER, DROP INDEX ... ON, DROP PROC
/// Phase 15.5: Uses token-based parsing instead of regex
fn try_drop_fallback(sql: &str) -> Option<FallbackStatementType> {
    let parsed = try_parse_drop_tokens(sql)?;
    Some(FallbackStatementType::RawStatement {
        object_type: parsed.drop_type.object_type_str().to_string(),
        schema: parsed.schema,
        name: parsed.name,
    })
}

/// Fallback for CTEs followed by DELETE, UPDATE, INSERT, or MERGE
/// sqlparser only supports CTEs followed by SELECT
/// Phase 15.5: Uses token-based parsing instead of regex
fn try_cte_dml_fallback(sql: &str) -> Option<FallbackStatementType> {
    let parsed = try_parse_cte_dml_tokens(sql)?;
    Some(FallbackStatementType::RawStatement {
        object_type: format!("CteWith{}", parsed.dml_type.as_str()),
        schema: "dbo".to_string(),
        name: "anonymous".to_string(),
    })
}

/// Fallback for MERGE statements with OUTPUT clause
/// sqlparser doesn't support the OUTPUT clause on MERGE
/// Phase 15.5: Uses token-based parsing instead of regex
fn try_merge_output_fallback(sql: &str) -> Option<FallbackStatementType> {
    let parsed = try_parse_merge_output_tokens(sql)?;
    Some(FallbackStatementType::RawStatement {
        object_type: "MergeWithOutput".to_string(),
        schema: parsed.schema,
        name: parsed.name,
    })
}

/// Fallback for UPDATE statements with XML methods (.modify(), .value(), etc.)
/// sqlparser doesn't support XML method call syntax
/// Phase 15.5: Uses token-based parsing instead of regex
fn try_xml_method_fallback(sql: &str) -> Option<FallbackStatementType> {
    let parsed = try_parse_xml_update_tokens(sql)?;
    Some(FallbackStatementType::RawStatement {
        object_type: "UpdateWithXmlMethod".to_string(),
        schema: parsed.schema,
        name: parsed.name,
    })
}

/// Extract schema and name from ALTER TABLE statement
fn extract_alter_table_name(sql: &str) -> Option<(String, String)> {
    // Use token-based parser for better accuracy
    parse_alter_table_name_tokens(sql)
}

/// Extract ALTER TABLE ... ADD CONSTRAINT statement
/// Handles both WITH CHECK and WITH NOCHECK variants:
/// ```sql
/// ALTER TABLE [dbo].[Table] WITH NOCHECK
/// ADD CONSTRAINT [FK_Name] FOREIGN KEY ([Col]) REFERENCES [Other]([Id]);
///
/// ALTER TABLE [dbo].[Table] WITH CHECK
/// ADD CONSTRAINT [CK_Name] CHECK ([Col] > 0);
/// ```
fn extract_alter_table_add_constraint(sql: &str) -> Option<FallbackStatementType> {
    // Use token-based parser for better accuracy
    let parsed = parse_alter_table_add_constraint_tokens(sql)?;

    // Convert token-parsed constraint to ExtractedTableConstraint
    let constraint = convert_token_parsed_constraint(parsed.constraint);

    Some(FallbackStatementType::AlterTableAddConstraint {
        table_schema: parsed.table_schema,
        table_name: parsed.table_name,
        constraint,
    })
}

/// Convert TokenParsedConstraint to ExtractedTableConstraint
fn convert_token_parsed_constraint(parsed: TokenParsedConstraint) -> ExtractedTableConstraint {
    match parsed {
        TokenParsedConstraint::PrimaryKey {
            name,
            columns,
            is_clustered,
        } => ExtractedTableConstraint::PrimaryKey {
            name,
            columns: columns
                .into_iter()
                .map(|c| ExtractedConstraintColumn {
                    name: c.name,
                    descending: c.descending,
                })
                .collect(),
            is_clustered,
        },
        TokenParsedConstraint::Unique {
            name,
            columns,
            is_clustered,
        } => ExtractedTableConstraint::Unique {
            name,
            columns: columns
                .into_iter()
                .map(|c| ExtractedConstraintColumn {
                    name: c.name,
                    descending: c.descending,
                })
                .collect(),
            is_clustered,
        },
        TokenParsedConstraint::ForeignKey {
            name,
            columns,
            referenced_table,
            referenced_columns,
        } => ExtractedTableConstraint::ForeignKey {
            name,
            columns,
            referenced_table,
            referenced_columns,
        },
        TokenParsedConstraint::Check { name, expression } => {
            ExtractedTableConstraint::Check { name, expression }
        }
    }
}

/// Extract extended property from sp_addextendedproperty call
///
/// Uses token-based parsing (Phase 15.6 G1) for improved maintainability and edge case handling.
///
/// Handles multiline syntax like:
/// ```sql
/// EXEC sp_addextendedproperty
///     @name = N'MS_Description',
///     @value = N'Description text',
///     @level0type = N'SCHEMA', @level0name = N'dbo',
///     @level1type = N'TABLE',  @level1name = N'TableName',
///     @level2type = N'COLUMN', @level2name = N'ColumnName';
/// ```
pub fn extract_extended_property_from_sql(sql: &str) -> Option<ExtractedExtendedProperty> {
    use crate::parser::extended_property_parser::parse_extended_property_tokens;

    // Try token-based parsing first (Phase 15.6 G1)
    if let Some(parsed) = parse_extended_property_tokens(sql) {
        return Some(ExtractedExtendedProperty {
            property_name: parsed.property_name,
            property_value: parsed.property_value,
            level0name: parsed.level0name,
            level1type: parsed.level1type,
            level1name: parsed.level1name,
            level2type: parsed.level2type,
            level2name: parsed.level2name,
        });
    }

    // Fallback to regex for edge cases not yet covered by token parser
    fn extract_param(sql: &str, param_name: &str) -> Option<String> {
        // Match @paramname = N'value' or @paramname = 'value'
        let pattern = format!(r"(?i)@{}\s*=\s*N?'([^']*)'", param_name);
        let re = regex::Regex::new(&pattern).ok()?;
        re.captures(sql)
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    }

    // Extract required parameters
    let property_name = extract_param(sql, "name")?;
    let property_value = extract_param(sql, "value").unwrap_or_default();

    // Extract level 0 (always SCHEMA)
    let level0name = extract_param(sql, "level0name").unwrap_or_else(|| "dbo".to_string());

    // Extract level 1 (TABLE, VIEW, etc.)
    let level1type = extract_param(sql, "level1type");
    let level1name = extract_param(sql, "level1name");

    // Extract level 2 (COLUMN, INDEX, etc.)
    let level2type = extract_param(sql, "level2type");
    let level2name = extract_param(sql, "level2name");

    Some(ExtractedExtendedProperty {
        property_name,
        property_value,
        level0name,
        level1type,
        level1name,
        level2type,
        level2name,
    })
}

/// Try to extract any CREATE statement as a generic fallback
/// Phase 15.5: Uses token-based parsing (A5) instead of regex
fn try_generic_create_fallback(sql: &str) -> Option<FallbackStatementType> {
    let parsed = try_parse_generic_create_tokens(sql)?;
    Some(FallbackStatementType::RawStatement {
        object_type: parsed.object_type,
        schema: parsed.schema,
        name: parsed.name,
    })
}

/// Extract schema and name for a specific object type
fn extract_generic_object_name(sql: &str, object_type: &str) -> Option<(String, String)> {
    // Use [^\]]+ for bracketed identifiers to capture special characters like &
    let pattern = format!(
        r"(?i)CREATE\s+(?:OR\s+ALTER\s+)?{}\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))",
        object_type
    );
    let re = regex::Regex::new(&pattern).ok()?;

    let caps = re.captures(sql)?;
    // Schema can be in group 1 (bracketed) or group 2 (unbracketed)
    let schema = caps
        .get(1)
        .or_else(|| caps.get(2))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dbo".to_string());
    // Name can be in group 3 (bracketed) or group 4 (unbracketed)
    let name = caps.get(3).or_else(|| caps.get(4))?.as_str().to_string();

    Some((schema, name))
}

/// Information extracted from a sequence definition
#[derive(Debug)]
struct SequenceInfo {
    schema: String,
    name: String,
    data_type: Option<String>,
    start_value: Option<i64>,
    increment_value: Option<i64>,
    min_value: Option<i64>,
    max_value: Option<i64>,
    is_cycling: bool,
    has_no_min_value: bool,
    has_no_max_value: bool,
    cache_size: Option<i64>,
}

/// Extract complete sequence information from CREATE SEQUENCE statement
///
/// Uses token-based parsing (Phase 15.3 B4) for improved maintainability and edge case handling.
fn extract_sequence_info(sql: &str) -> Option<SequenceInfo> {
    let parsed = parse_create_sequence_tokens(sql)?;
    Some(SequenceInfo {
        schema: parsed.schema,
        name: parsed.name,
        data_type: parsed.data_type,
        start_value: parsed.start_value,
        increment_value: parsed.increment_value,
        min_value: parsed.min_value,
        max_value: parsed.max_value,
        is_cycling: parsed.is_cycling,
        has_no_min_value: parsed.has_no_min_value,
        has_no_max_value: parsed.has_no_max_value,
        cache_size: parsed.cache_size,
    })
}

/// Extract schema and name from CREATE TYPE statement
fn extract_type_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // CREATE TYPE [dbo].[TypeName] AS TABLE
    // CREATE TYPE dbo.TypeName AS TABLE
    // Use [^\]]+ for bracketed identifiers to capture special characters like &
    let re = regex::Regex::new(
        r"(?i)CREATE\s+TYPE\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))",
    )
    .ok()?;

    let caps = re.captures(sql)?;
    // Schema can be in group 1 (bracketed) or group 2 (unbracketed)
    let schema = caps
        .get(1)
        .or_else(|| caps.get(2))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dbo".to_string());
    // Name can be in group 3 (bracketed) or group 4 (unbracketed)
    let name = caps.get(3).or_else(|| caps.get(4))?.as_str().to_string();

    Some((schema, name))
}

/// Information extracted from a scalar type definition
#[derive(Debug)]
struct ScalarTypeInfo {
    base_type: String,
    is_nullable: bool,
    length: Option<i32>,
    precision: Option<u8>,
    scale: Option<u8>,
}

/// Determine if a CREATE TYPE statement is a scalar type (FROM) or table type (AS TABLE)
/// Uses token-based parsing to handle any whitespace between keywords (Phase 15.8 J6)
///
/// Returns Some(true) for scalar types (has FROM, no AS TABLE)
/// Returns Some(false) for table types (has AS TABLE)
/// Returns None if neither pattern is found
fn is_scalar_type_definition(sql: &str) -> Option<bool> {
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, sql).tokenize() {
        Ok(t) => t,
        Err(_) => return None,
    };

    let mut paren_depth: i32 = 0;
    let mut found_type = false;
    let mut has_from = false;
    let mut has_as_table = false;

    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::Word(w) if w.keyword == Keyword::TYPE && paren_depth == 0 => {
                found_type = true;
            }
            Token::Word(w) if w.keyword == Keyword::FROM && paren_depth == 0 && found_type => {
                has_from = true;
            }
            Token::Word(w) if w.keyword == Keyword::AS && paren_depth == 0 && found_type => {
                // Check if next non-whitespace token is TABLE
                let mut j = i + 1;
                while j < tokens.len() {
                    match &tokens[j] {
                        Token::Whitespace(_) => j += 1,
                        Token::Word(w2) if w2.keyword == Keyword::TABLE => {
                            has_as_table = true;
                            break;
                        }
                        _ => break,
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    if !found_type {
        return None;
    }

    // Scalar type: has FROM but not AS TABLE
    // Table type: has AS TABLE
    if has_from && !has_as_table {
        Some(true)
    } else if has_as_table {
        Some(false)
    } else {
        None
    }
}

/// Extract scalar type information from CREATE TYPE ... FROM basetype
/// e.g., CREATE TYPE [dbo].[PhoneNumber] FROM VARCHAR(20) NOT NULL
///
/// Uses token-based parsing to handle any whitespace between keywords (Phase 15.8 J7)
fn extract_scalar_type_info(sql: &str, _sql_upper: &str) -> Option<ScalarTypeInfo> {
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, sql).tokenize() {
        Ok(t) => t,
        Err(_) => return None,
    };

    // Find the FROM keyword at top level (paren depth 0)
    let mut paren_depth: i32 = 0;
    let mut from_token_idx = None;

    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::Word(w) if w.keyword == Keyword::FROM && paren_depth == 0 => {
                from_token_idx = Some(i);
                break;
            }
            _ => {}
        }
    }

    let from_idx = from_token_idx?;

    // Extract tokens after FROM (the base type and modifiers)
    let after_from_tokens: Vec<_> = tokens[from_idx + 1..]
        .iter()
        .filter(|t| !matches!(t, Token::Whitespace(_) | Token::EOF))
        .collect();

    if after_from_tokens.is_empty() {
        return None;
    }

    // Check for NOT NULL at the end of the tokens (at top level, paren depth 0)
    // Look for NOT followed by NULL at the end
    let is_nullable = {
        let mut not_null_found = false;
        let mut pd: i32 = 0;
        let mut i = 0;
        while i < after_from_tokens.len() {
            match after_from_tokens[i] {
                Token::LParen => pd += 1,
                Token::RParen => pd = pd.saturating_sub(1),
                Token::Word(w) if w.keyword == Keyword::NOT && pd == 0 => {
                    // Check if next token is NULL
                    if i + 1 < after_from_tokens.len() {
                        if let Token::Word(w2) = after_from_tokens[i + 1] {
                            if w2.keyword == Keyword::NULL {
                                not_null_found = true;
                            }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
        !not_null_found // NULL is the default for scalar types
    };

    // Extract the base type and optional size specification
    // The first token after FROM should be the type name
    let base_type = match &after_from_tokens[0] {
        Token::Word(w) => w.value.to_lowercase(),
        _ => return None,
    };

    // Check for size specification: (length), (MAX), or (precision, scale)
    let (length, precision, scale) = if after_from_tokens.len() > 1 {
        if matches!(after_from_tokens.get(1), Some(Token::LParen)) {
            // Parse numbers or MAX keyword inside parentheses
            let mut numbers: Vec<i32> = Vec::new();
            let mut is_max = false;
            for token in after_from_tokens.iter().skip(2) {
                match token {
                    Token::Number(n, _) => {
                        if let Ok(num) = n.parse::<i32>() {
                            numbers.push(num);
                        }
                    }
                    Token::Word(w) if w.keyword == Keyword::MAX => {
                        // MAX keyword indicates maximum length for variable-length types
                        is_max = true;
                    }
                    Token::RParen => break,
                    _ => {}
                }
            }

            if is_max {
                // MAX type - use -1 to indicate MAX (matches DotNet convention)
                (Some(-1), None, None)
            } else if numbers.len() == 2 {
                // Two numbers: precision and scale (e.g., DECIMAL(18,4))
                (None, Some(numbers[0] as u8), Some(numbers[1] as u8))
            } else if numbers.len() == 1 {
                // One number: could be length or precision depending on type
                match base_type.as_str() {
                    "decimal" | "numeric" => (None, Some(numbers[0] as u8), Some(0)),
                    _ => (Some(numbers[0]), None, None),
                }
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    Some(ScalarTypeInfo {
        base_type,
        is_nullable,
        length,
        precision,
        scale,
    })
}

/// Extract trigger information from CREATE TRIGGER statement
/// Parses trigger name, parent table/view, events (INSERT/UPDATE/DELETE), and type (AFTER/INSTEAD OF)
///
/// Uses token-based parsing (Phase 15.3) for improved maintainability and edge case handling.
fn extract_trigger_info(sql: &str) -> Option<FallbackStatementType> {
    let parsed = parse_create_trigger_tokens(sql)?;

    Some(FallbackStatementType::Trigger {
        schema: parsed.schema,
        name: parsed.name,
        parent_schema: parsed.parent_schema,
        parent_name: parsed.parent_name,
        is_insert: parsed.is_insert,
        is_update: parsed.is_update,
        is_delete: parsed.is_delete,
        trigger_type: parsed.trigger_type,
    })
}

/// Extract schema and name from CREATE PROCEDURE statement
///
/// Uses token-based parsing (Phase 15.3) for improved maintainability and edge case handling.
fn extract_procedure_name(sql: &str) -> Option<(String, String)> {
    parse_create_procedure_tokens(sql)
}

/// Extract schema and name from ALTER PROCEDURE statement
///
/// Uses token-based parsing (Phase 15.3) for improved maintainability and edge case handling.
fn extract_alter_procedure_name(sql: &str) -> Option<(String, String)> {
    parse_alter_procedure_tokens(sql)
}

/// Extract schema and name from ALTER FUNCTION statement
///
/// Uses token-based parsing (Phase 15.3 B2) for improved maintainability and edge case handling.
fn extract_alter_function_name(sql: &str) -> Option<(String, String)> {
    parse_alter_function_tokens(sql)
}

/// Extract complete sequence information from ALTER SEQUENCE statement
///
/// Uses token-based parsing (Phase 15.3 B4) for improved maintainability and edge case handling.
fn extract_alter_sequence_info(sql: &str) -> Option<SequenceInfo> {
    let parsed = parse_alter_sequence_tokens(sql)?;
    Some(SequenceInfo {
        schema: parsed.schema,
        name: parsed.name,
        data_type: None, // ALTER SEQUENCE doesn't change the data type
        start_value: parsed.start_value,
        increment_value: parsed.increment_value,
        min_value: parsed.min_value,
        max_value: parsed.max_value,
        is_cycling: parsed.is_cycling,
        has_no_min_value: parsed.has_no_min_value,
        has_no_max_value: parsed.has_no_max_value,
        cache_size: parsed.cache_size,
    })
}

/// Extract schema and name from CREATE FUNCTION statement
///
/// Uses token-based parsing (Phase 15.3 B2) for improved maintainability and edge case handling.
fn extract_function_name(sql: &str) -> Option<(String, String)> {
    parse_create_function_tokens(sql)
}

/// Detect function type from SQL definition
///
/// Uses token-based parsing (Phase 15.3 B2) for improved maintainability and edge case handling.
fn detect_function_type(sql: &str) -> FallbackFunctionType {
    match detect_function_type_tokens(sql) {
        TokenParsedFunctionType::Scalar => FallbackFunctionType::Scalar,
        TokenParsedFunctionType::TableValued => FallbackFunctionType::TableValued,
        TokenParsedFunctionType::InlineTableValued => FallbackFunctionType::InlineTableValued,
    }
}

/// Extract parameters from a function definition
///
/// Uses token-based parsing (Phase 15.3 B2) for improved maintainability and edge case handling.
fn extract_function_parameters(sql: &str) -> Vec<ExtractedFunctionParameter> {
    if let Some(func) = parse_create_function_full(sql) {
        func.parameters
            .into_iter()
            .map(|p| ExtractedFunctionParameter {
                name: p.name,
                data_type: p.data_type,
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// Extract the return type from a function definition
///
/// Uses token-based parsing (Phase 15.3 B2) for improved maintainability and edge case handling.
fn extract_function_return_type(sql: &str) -> Option<String> {
    parse_create_function_full(sql).and_then(|f| f.return_type)
}

/// Extract index information from CREATE CLUSTERED/NONCLUSTERED INDEX statement
///
/// Uses token-based parsing (Phase 15.3 B6) for improved maintainability and edge case handling.
fn extract_index_info(sql: &str) -> Option<FallbackStatementType> {
    // Try token-based parsing first (Phase 15.3 B6)
    if let Some(parsed) = parse_create_index_tokens(sql) {
        return Some(FallbackStatementType::Index {
            name: parsed.name,
            table_schema: parsed.table_schema,
            table_name: parsed.table_name,
            columns: parsed.columns,
            include_columns: parsed.include_columns,
            is_unique: parsed.is_unique,
            is_clustered: parsed.is_clustered,
            fill_factor: parsed.fill_factor,
            filter_predicate: parsed.filter_predicate,
            data_compression: parsed.data_compression,
        });
    }

    // Fallback to regex for edge cases not yet covered by token parser
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
    let is_clustered = caps
        .get(2)
        .map(|m| m.as_str().to_uppercase() == "CLUSTERED")
        .unwrap_or(false);
    let name = caps.get(3)?.as_str().to_string();
    let table_schema = caps
        .get(4)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dbo".to_string());
    let table_name = caps.get(5)?.as_str().to_string();

    // Parse column list, handling sort direction (ASC/DESC)
    let columns_str = caps.get(6)?.as_str();
    let columns: Vec<String> = parse_column_list(columns_str);

    // Extract INCLUDE columns if present
    let include_columns = extract_include_columns(sql);

    // Extract FILLFACTOR from WITH clause if present
    let fill_factor = extract_index_fill_factor(sql);

    // Extract DATA_COMPRESSION from WITH clause if present
    let data_compression = extract_index_data_compression(sql);

    // Extract filter predicate from WHERE clause if present (token-based, Phase 20.6.1)
    let filter_predicate = extract_index_filter_predicate_tokenized(sql);

    Some(FallbackStatementType::Index {
        name,
        table_schema,
        table_name,
        columns,
        include_columns,
        is_unique,
        is_clustered,
        fill_factor,
        filter_predicate,
        data_compression,
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

/// Extract FILLFACTOR value from index WITH clause
fn extract_index_fill_factor(sql: &str) -> Option<u8> {
    // Match FILLFACTOR = <number> in WITH clause
    let re = regex::Regex::new(r"(?i)FILLFACTOR\s*=\s*(\d+)").ok()?;

    re.captures(sql)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<u8>().ok())
}

/// Extract DATA_COMPRESSION value from index WITH clause
fn extract_index_data_compression(sql: &str) -> Option<String> {
    // Match DATA_COMPRESSION = <type> in WITH clause
    // Type can be: NONE, ROW, PAGE, COLUMNSTORE, COLUMNSTORE_ARCHIVE
    let re = regex::Regex::new(r"(?i)DATA_COMPRESSION\s*=\s*(\w+)").ok()?;

    re.captures(sql)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_uppercase())
}

// Phase 20.6.1: Removed extract_index_filter_predicate() - replaced with token-based
// extract_index_filter_predicate_tokenized() from index_parser.rs

/// Extract full table structure from CREATE TABLE statement
///
/// Takes a pre-computed uppercase SQL string to avoid redundant `.to_uppercase()` calls
/// when called from `try_fallback_parse()` which already has the uppercase version.
fn extract_table_structure(sql: &str, sql_upper: &str) -> Option<FallbackStatementType> {
    let (schema, name) = extract_generic_object_name(sql, "TABLE")?;

    // Check for graph table syntax (AS NODE or AS EDGE)
    let is_node = sql_upper.contains("AS NODE");
    let is_edge = sql_upper.contains("AS EDGE");

    // Find the opening parenthesis after CREATE TABLE [schema].[name]
    let table_name_pattern = format!(
        r"(?i)CREATE\s+TABLE\s+(?:\[?{}\]?\.)?\[?{}\]?\s*\(",
        regex::escape(&schema),
        regex::escape(&name)
    );
    let table_re = Regex::new(&table_name_pattern).ok()?;
    let table_match = table_re.find(sql)?;
    let paren_start = table_match.end() - 1; // Position of the opening '('

    // Find matching closing parenthesis
    let table_body = extract_balanced_parens(&sql[paren_start..])?;

    // Parse columns and constraints from the table body
    let (columns, constraints) = parse_table_body(&table_body, &name);

    Some(FallbackStatementType::Table {
        schema,
        name,
        columns,
        constraints,
        is_node,
        is_edge,
    })
}

/// Extract content between balanced parentheses (returns content without the outer parens)
fn extract_balanced_parens(sql: &str) -> Option<String> {
    if !sql.starts_with('(') {
        return None;
    }

    let mut depth = 0;
    let mut end_pos = 0;

    for (i, c) in sql.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end_pos = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if end_pos > 1 {
        Some(sql[1..end_pos].to_string())
    } else {
        None
    }
}

/// Parse table body to extract columns and constraints
fn parse_table_body(
    body: &str,
    table_name: &str,
) -> (Vec<ExtractedTableColumn>, Vec<ExtractedTableConstraint>) {
    // Split by top-level commas (not inside parentheses)
    let parts = split_by_top_level_comma(body);

    // Most parts are columns, with a few constraints
    let mut columns = Vec::with_capacity(parts.len());
    let mut constraints = Vec::with_capacity(parts.len().min(4));

    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        let upper = trimmed.to_uppercase();

        // Check if this is a table-level constraint
        if upper.starts_with("CONSTRAINT")
            || upper.starts_with("PRIMARY KEY")
            || upper.starts_with("FOREIGN KEY")
            || upper.starts_with("UNIQUE")
            || upper.starts_with("CHECK")
        {
            if let Some(constraint) = parse_table_constraint(trimmed, table_name) {
                constraints.push(constraint);
            }
        } else {
            // This is a column definition
            if let Some(column) = parse_column_definition(trimmed) {
                columns.push(column);
            }
        }
    }

    (columns, constraints)
}

/// Split string by commas at the top level (not inside parentheses)
/// Also splits on constraint keywords (CONSTRAINT, PRIMARY, FOREIGN, UNIQUE, CHECK)
/// that appear at depth 0 after column definitions, to handle SQL Server's
/// relaxed syntax that allows omitting commas before constraints.
fn split_by_top_level_comma(s: &str) -> Vec<String> {
    use sqlparser::dialect::MsSqlDialect;
    use sqlparser::tokenizer::Tokenizer;

    // Try tokenization first for accurate constraint detection
    let dialect = MsSqlDialect {};
    if let Ok(tokens) = Tokenizer::new(&dialect, s).tokenize_with_location() {
        return split_by_comma_or_constraint_tokens(&tokens);
    }

    // Fallback to simple character-based splitting if tokenization fails
    split_by_top_level_comma_simple(s)
}

/// Token-based splitting that handles both commas and comma-less constraints
/// Reconstructs parts from tokens to avoid needing offset tracking.
fn split_by_comma_or_constraint_tokens(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
) -> Vec<String> {
    use sqlparser::keywords::Keyword;
    use sqlparser::tokenizer::Token;

    let mut parts = Vec::new();
    let mut current_part = String::new();
    let mut depth = 0;
    let mut seen_content_in_part = false; // Track if we've seen actual content (not just whitespace)
    let mut in_constraint = false; // Track if we're inside a constraint definition

    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];

        match &token.token {
            Token::EOF => break,
            Token::LParen => {
                depth += 1;
                current_part.push('(');
                seen_content_in_part = true;
            }
            Token::RParen => {
                if depth > 0 {
                    depth -= 1;
                }
                current_part.push(')');
                seen_content_in_part = true;
            }
            Token::Comma if depth == 0 => {
                // Split at comma
                let trimmed = current_part.trim().to_string();
                if !trimmed.is_empty() {
                    parts.push(trimmed);
                }
                current_part = String::new();
                seen_content_in_part = false;
                in_constraint = false; // Reset constraint tracking
            }
            Token::Word(w) if depth == 0 => {
                // Check for constraint keywords at depth 0
                // We need to split before table-level constraints but NOT before inline constraints.
                //
                // Table-level constraints start with:
                // - CONSTRAINT [name] PRIMARY KEY|FOREIGN KEY|UNIQUE|CHECK
                // - PRIMARY KEY (without CONSTRAINT)
                // - UNIQUE (without CONSTRAINT)
                // - FOREIGN KEY (without CONSTRAINT)
                //
                // Inline constraints are:
                // - CONSTRAINT [name] DEFAULT (value)
                // - DEFAULT (value)
                // - CHECK (expr) <- Standalone CHECK without CONSTRAINT is always inline!
                //
                // Strategy: Only split on CONSTRAINT if followed by PK/FK/UNIQUE/CHECK,
                // and split on standalone PRIMARY/UNIQUE/FOREIGN KEY if not in a constraint.
                // Standalone CHECK (without CONSTRAINT) is always an inline column constraint,
                // never a table-level constraint - so we do NOT split on it.

                let is_table_constraint_start = if w.keyword == Keyword::CONSTRAINT {
                    // Check if this CONSTRAINT is followed by PRIMARY KEY, FOREIGN KEY, UNIQUE, or CHECK
                    is_table_level_constraint_ahead(&tokens[i..])
                } else if !in_constraint {
                    // Standalone PRIMARY KEY, UNIQUE, or FOREIGN KEY (but NOT CHECK!)
                    // Standalone CHECK is always an inline constraint, not table-level
                    matches!(w.keyword, Keyword::PRIMARY | Keyword::UNIQUE)
                        || (w.keyword == Keyword::FOREIGN && is_foreign_key_ahead(&tokens[i..]))
                } else {
                    false
                };

                if is_table_constraint_start && seen_content_in_part {
                    // We've hit a table-level constraint without a preceding comma
                    // Split before this keyword (don't include leading whitespace)
                    let trimmed = current_part.trim().to_string();
                    if !trimmed.is_empty() {
                        parts.push(trimmed);
                    }
                    current_part = String::new();
                    // Reset tracking for the new part (the constraint keyword will set it to true below)
                    #[allow(unused_assignments)]
                    {
                        seen_content_in_part = false;
                    }
                }

                // Mark that we're in a constraint if this is CONSTRAINT keyword
                if w.keyword == Keyword::CONSTRAINT {
                    in_constraint = true;
                }

                // Add the token to current part
                current_part.push_str(&token_to_string_simple(&token.token));
                seen_content_in_part = true;
            }
            Token::Whitespace(ws) => {
                // Add whitespace but don't mark as seen content
                current_part.push_str(&ws.to_string());
            }
            _ => {
                current_part.push_str(&token_to_string_simple(&token.token));
                seen_content_in_part = true;
            }
        }

        i += 1;
    }

    // Don't forget the last part
    let remaining = current_part.trim().to_string();
    if !remaining.is_empty() {
        parts.push(remaining);
    }

    parts
}

/// Check if tokens starting at current position represent FOREIGN KEY
fn is_foreign_key_ahead(tokens: &[sqlparser::tokenizer::TokenWithSpan]) -> bool {
    use sqlparser::keywords::Keyword;
    use sqlparser::tokenizer::Token;

    let mut i = 1; // Skip current FOREIGN token
                   // Skip whitespace
    while i < tokens.len() {
        match &tokens[i].token {
            Token::Whitespace(_) => i += 1,
            Token::Word(w) if w.keyword == Keyword::KEY => return true,
            _ => return false,
        }
    }
    false
}

/// Check if tokens starting at CONSTRAINT keyword represent a table-level constraint
/// (i.e., CONSTRAINT [name] PRIMARY KEY | FOREIGN KEY | UNIQUE | CHECK)
/// Returns false for inline constraints like CONSTRAINT [name] DEFAULT (value)
fn is_table_level_constraint_ahead(tokens: &[sqlparser::tokenizer::TokenWithSpan]) -> bool {
    use sqlparser::keywords::Keyword;
    use sqlparser::tokenizer::Token;

    // Pattern: CONSTRAINT [name] <type>
    // We need to skip: CONSTRAINT, whitespace, identifier, whitespace, then check type

    let mut i = 1; // Skip CONSTRAINT token
                   // Skip whitespace
    while i < tokens.len() {
        match &tokens[i].token {
            Token::Whitespace(_) => i += 1,
            _ => break,
        }
    }

    // Skip the constraint name (identifier)
    if i < tokens.len() {
        if let Token::Word(_) = &tokens[i].token {
            i += 1;
        } else {
            return false;
        }
    }

    // Skip whitespace
    while i < tokens.len() {
        match &tokens[i].token {
            Token::Whitespace(_) => i += 1,
            _ => break,
        }
    }

    // Now check if the next token is a table-level constraint type
    if i < tokens.len() {
        if let Token::Word(w) = &tokens[i].token {
            return matches!(
                w.keyword,
                Keyword::PRIMARY | Keyword::UNIQUE | Keyword::CHECK
            ) || (w.keyword == Keyword::FOREIGN && is_foreign_key_ahead(&tokens[i..]));
        }
    }

    false
}

/// Convert a token back to its string representation
fn token_to_string_simple(token: &sqlparser::tokenizer::Token) -> String {
    format_token_sql(token)
}

/// Simple character-based splitting (fallback)
fn split_by_top_level_comma_simple(s: &str) -> Vec<String> {
    let estimated_parts = (s.len() / 30).max(1);
    let mut parts = Vec::with_capacity(estimated_parts);
    let mut current = String::new();
    let mut depth = 0;

    for c in s.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
}

/// Parse a column definition using token-based parsing (Phase 15.2)
///
/// This function uses the token-based `ColumnTokenParser` for improved maintainability
/// and error handling, replacing the previous regex-based approach.
fn parse_column_definition(col_def: &str) -> Option<ExtractedTableColumn> {
    // Use the new token-based parser
    let parsed = parse_column_definition_tokens(col_def)?;

    // Convert TokenParsedColumn to ExtractedTableColumn
    Some(convert_token_parsed_column(parsed))
}

/// Convert a TokenParsedColumn to ExtractedTableColumn
fn convert_token_parsed_column(parsed: TokenParsedColumn) -> ExtractedTableColumn {
    ExtractedTableColumn {
        name: parsed.name,
        data_type: parsed.data_type,
        nullability: parsed.nullability,
        is_identity: parsed.is_identity,
        is_rowguidcol: parsed.is_rowguidcol,
        is_sparse: parsed.is_sparse,
        is_filestream: parsed.is_filestream,
        default_constraint_name: parsed.default_constraint_name,
        default_value: parsed.default_value,
        emit_default_constraint_name: parsed.emit_default_constraint_name,
        check_constraint_name: parsed.check_constraint_name,
        check_expression: parsed.check_expression,
        emit_check_constraint_name: parsed.emit_check_constraint_name,
        computed_expression: parsed.computed_expression,
        is_persisted: parsed.is_persisted,
    }
}

/// Parse a table-level constraint
fn parse_table_constraint(
    constraint_def: &str,
    table_name: &str,
) -> Option<ExtractedTableConstraint> {
    // Use token-based parser for better accuracy
    let parsed = parse_table_constraint_tokens(constraint_def, table_name)?;
    Some(convert_token_parsed_constraint(parsed))
}

/// Result of preprocessing T-SQL for sqlparser compatibility
struct PreprocessResult {
    /// SQL with T-SQL-specific syntax converted for sqlparser
    sql: String,
    /// Default constraints extracted from the SQL
    extracted_defaults: Vec<ExtractedDefaultConstraint>,
}

/// Preprocess T-SQL to handle syntax that sqlparser doesn't support:
/// 1. Replace VARBINARY(MAX) and BINARY(MAX) with sentinel values
/// 2. Extract and remove CONSTRAINT [name] DEFAULT (value) FOR [column] patterns
/// 3. Clean up trailing commas before closing parentheses
///
/// Uses token-based parsing (Phase 15.7 H1-H3) which correctly handles string
/// literals and comments - patterns inside strings are not modified.
fn preprocess_tsql(sql: &str) -> PreprocessResult {
    // Use the new token-based preprocessor
    let token_result = preprocess_tsql_tokens(sql);

    PreprocessResult {
        sql: token_result.sql,
        extracted_defaults: token_result.extracted_defaults,
    }
}

/// Split SQL content into batches by GO statement, tracking line numbers
fn split_batches(content: &str) -> Vec<Batch<'_>> {
    // Estimate ~1 batch per 20 lines (GO separators are relatively sparse)
    let line_count = content.lines().count();
    let estimated_batches = (line_count / 20).max(1);
    let mut batches = Vec::with_capacity(estimated_batches);
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
        // Also handle GO; with trailing semicolon (common in some SQL scripts)
        if trimmed.eq_ignore_ascii_case("go") || trimmed.eq_ignore_ascii_case("go;") {
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
    fn test_split_batches_with_semicolon() {
        // GO; with trailing semicolon should also be recognized as a batch separator
        let sql = "CREATE TABLE t1 (id INT)\nGO;\nCREATE TABLE t2 (id INT)";
        let batches = split_batches(sql);
        assert_eq!(batches.len(), 2, "GO; should split into 2 batches");
        assert_eq!(batches[0].start_line, 1);
        assert!(batches[0].content.contains("CREATE TABLE t1"));
        assert_eq!(batches[1].start_line, 3);
        assert!(batches[1].content.contains("CREATE TABLE t2"));
    }

    #[test]
    fn test_split_batches_go_semicolon_variations() {
        // Test various GO; variations
        let sql = "SELECT 1\nGO;\nSELECT 2\n  GO;  \nSELECT 3\ngo;\nSELECT 4";
        let batches = split_batches(sql);
        assert_eq!(
            batches.len(),
            4,
            "Should handle GO; with various whitespace and casing"
        );
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
        assert_eq!(
            extract_line_from_error("Error at Line: 5, Column: 10"),
            Some(5)
        );
        assert_eq!(
            extract_line_from_error("Parse error at Line: 123, Column: 1"),
            Some(123)
        );
        assert_eq!(extract_line_from_error("No line info here"), None);
        assert_eq!(extract_line_from_error("Line:42, Column: 1"), Some(42));
    }

    #[test]
    fn test_preprocess_default_for() {
        let sql = r#"CREATE TABLE [dbo].[Products] (
    [Id] INT NOT NULL,
    [IsActive] BIT NOT NULL,
    CONSTRAINT [PK_Products] PRIMARY KEY ([Id]),
    CONSTRAINT [DF_Products_IsActive] DEFAULT (1) FOR [IsActive]
);"#;
        let result = preprocess_tsql(sql);

        // Should extract the default constraint
        assert_eq!(result.extracted_defaults.len(), 1);
        assert_eq!(result.extracted_defaults[0].name, "DF_Products_IsActive");
        assert_eq!(result.extracted_defaults[0].column, "IsActive");
        assert_eq!(result.extracted_defaults[0].expression, "(1)");

        // Should not contain DEFAULT FOR in preprocessed SQL
        assert!(!result.sql.contains("DEFAULT (1) FOR"));

        // Should still be valid SQL (parseable by sqlparser)
        let dialect = ExtendedTsqlDialect::new();
        let parsed = Parser::parse_sql(&dialect, &result.sql);
        assert!(
            parsed.is_ok(),
            "Preprocessed SQL should be parseable: {}",
            result.sql
        );
    }

    #[test]
    fn test_preprocess_varbinary_max() {
        let sql = "CREATE TABLE [dbo].[Test] ([Data] VARBINARY(MAX) NULL);";
        let result = preprocess_tsql(sql);

        // Should replace MAX with sentinel
        assert!(result
            .sql
            .contains(&format!("VARBINARY({})", BINARY_MAX_SENTINEL)));
        assert!(!result.sql.contains("VARBINARY(MAX)"));

        // Should be parseable
        let dialect = ExtendedTsqlDialect::new();
        let parsed = Parser::parse_sql(&dialect, &result.sql);
        assert!(
            parsed.is_ok(),
            "Preprocessed SQL should be parseable: {}",
            result.sql
        );
    }

    #[test]
    fn test_preprocess_full_products_table() {
        let sql = r#"-- Table with ALL constraint types: PK, FK, UQ, CK, DF
CREATE TABLE [dbo].[Products] (
    [Id] INT NOT NULL,
    [SKU] NVARCHAR(50) NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [CategoryId] INT NOT NULL,
    [Price] DECIMAL(18,2) NOT NULL,
    [Quantity] INT NOT NULL,
    [IsActive] BIT NOT NULL,
    [CreatedAt] DATETIME NOT NULL,

    -- PK: Primary Key Constraint
    CONSTRAINT [PK_Products] PRIMARY KEY ([Id]),

    -- FK: Foreign Key Constraint
    CONSTRAINT [FK_Products_Categories] FOREIGN KEY ([CategoryId]) REFERENCES [dbo].[Categories]([Id]),

    -- UQ: Unique Constraint
    CONSTRAINT [UQ_Products_SKU] UNIQUE ([SKU]),

    -- CK: Check Constraint
    CONSTRAINT [CK_Products_Price] CHECK ([Price] >= 0),
    CONSTRAINT [CK_Products_Quantity] CHECK ([Quantity] >= 0),

    -- DF: Default Constraint
    CONSTRAINT [DF_Products_IsActive] DEFAULT (1) FOR [IsActive],
    CONSTRAINT [DF_Products_CreatedAt] DEFAULT (GETDATE()) FOR [CreatedAt]
);"#;
        let result = preprocess_tsql(sql);

        println!("=== Extracted defaults ===");
        for d in &result.extracted_defaults {
            println!(
                "  Name: {}, Column: {}, Expression: {}",
                d.name, d.column, d.expression
            );
        }
        println!("=== Preprocessed SQL ===\n{}", result.sql);

        // Should extract 2 default constraints
        assert_eq!(
            result.extracted_defaults.len(),
            2,
            "Should extract 2 default constraints"
        );

        // Should be parseable by sqlparser
        let dialect = ExtendedTsqlDialect::new();
        let parsed = Parser::parse_sql(&dialect, &result.sql);
        assert!(
            parsed.is_ok(),
            "Preprocessed SQL should be parseable. Error: {:?}\nSQL:\n{}",
            parsed.err(),
            result.sql
        );
    }

    #[test]
    fn test_extract_extended_property() {
        let sql = r#"EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Unique identifier for the documented item',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'DocumentedTable',
    @level2type = N'COLUMN', @level2name = N'Id';"#;

        let prop = extract_extended_property_from_sql(sql);
        assert!(prop.is_some(), "Should extract extended property");
        let prop = prop.unwrap();

        assert_eq!(prop.property_name, "MS_Description");
        assert_eq!(
            prop.property_value,
            "Unique identifier for the documented item"
        );
        assert_eq!(prop.level0name, "dbo");
        assert_eq!(prop.level1type, Some("TABLE".to_string()));
        assert_eq!(prop.level1name, Some("DocumentedTable".to_string()));
        assert_eq!(prop.level2type, Some("COLUMN".to_string()));
        assert_eq!(prop.level2name, Some("Id".to_string()));
    }

    #[test]
    fn test_fallback_parse_extended_property() {
        let sql = r#"EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'This table stores documented items',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'DocumentedTable';"#;

        let fallback = try_fallback_parse(sql);
        assert!(fallback.is_some(), "Should parse extended property");

        match fallback.unwrap() {
            FallbackStatementType::ExtendedProperty { property } => {
                assert_eq!(property.property_name, "MS_Description");
                assert_eq!(property.level1name, Some("DocumentedTable".to_string()));
                assert!(property.level2name.is_none());
            }
            _ => panic!("Expected ExtendedProperty variant"),
        }
    }

    #[test]
    fn test_split_by_top_level_comma_with_commaless_constraints() {
        // Test SQL1: Table with one comma-less constraint
        let sql1 = r#"[Id] UNIQUEIDENTIFIER NOT NULL,
    [Name] NVARCHAR(100) NOT NULL
    CONSTRAINT [PK_Test] PRIMARY KEY CLUSTERED ([Id] ASC),
    CONSTRAINT [FK_Test_Self] FOREIGN KEY ([Id]) REFERENCES [dbo].[Test] ([Id])"#;

        // Test SQL3: Inline constraints should not cause splits
        let sql3 = r#"[Version] INT CONSTRAINT [DF_Test_Version] DEFAULT ((0)) NOT NULL,
    [CreatedOn] DATETIME CONSTRAINT [DF_Test_CreatedOn] DEFAULT (GETDATE()) NOT NULL"#;

        let parts = super::split_by_top_level_comma(sql3);
        println!("SQL3 (inline constraints) parts:");
        for (i, part) in parts.iter().enumerate() {
            println!("  Part {}: {}", i, part.replace('\n', "\\n"));
        }

        assert_eq!(
            parts.len(),
            2,
            "Expected 2 parts: two columns with inline defaults"
        );
        assert!(
            parts[0].contains("CONSTRAINT [DF_Test_Version]"),
            "Part 0 should contain inline default"
        );
        assert!(
            parts[1].contains("CONSTRAINT [DF_Test_CreatedOn]"),
            "Part 1 should contain inline default"
        );

        // Test SQL4: Simulating the actual problem SQL from TableWithMultipleCommalessConstraints
        let sql4 = r#"[Id] UNIQUEIDENTIFIER NOT NULL,
    [Version] INT CONSTRAINT [DF_MultiCommaless_Version] DEFAULT ((0)) NOT NULL,
    [CreatedOn] DATETIME CONSTRAINT [DF_MultiCommaless_CreatedOn] DEFAULT (GETDATE()) NOT NULL,
    [ParentId] UNIQUEIDENTIFIER NOT NULL,
    [Status] NVARCHAR(20) CONSTRAINT [DF_MultiCommaless_Status] DEFAULT ('Active') NOT NULL

    CONSTRAINT [PK_MultiCommaless] PRIMARY KEY ([Id] ASC)
    CONSTRAINT [FK_MultiCommaless_Parent] FOREIGN KEY ([ParentId]) REFERENCES [dbo].[TableWithCommalessConstraints] ([Id]),
    CONSTRAINT [CK_MultiCommaless_Status] CHECK ([Status] IN ('Active', 'Inactive', 'Pending'))"#;

        let parts = super::split_by_top_level_comma(sql4);
        println!("\nSQL4 (actual problem SQL) parts:");
        for (i, part) in parts.iter().enumerate() {
            println!("  Part {}: {}", i, part.replace('\n', "\\n").trim());
        }

        // Should have 8 parts: 5 columns + 3 constraints
        assert_eq!(
            parts.len(),
            8,
            "Expected 8 parts: 5 columns + 3 constraints"
        );
        assert!(
            parts[1].contains("CONSTRAINT [DF_MultiCommaless_Version]"),
            "Part 1 should contain inline default"
        );
        assert!(
            parts[5].starts_with("CONSTRAINT [PK_MultiCommaless]"),
            "Part 5 should be PK constraint"
        );
        assert!(
            parts[6].starts_with("CONSTRAINT [FK_MultiCommaless_Parent]"),
            "Part 6 should be FK constraint"
        );

        let parts = super::split_by_top_level_comma(sql1);
        println!("SQL1 parts:");
        for (i, part) in parts.iter().enumerate() {
            println!("  Part {}: {}", i, part.replace('\n', "\\n"));
        }

        assert_eq!(
            parts.len(),
            4,
            "Expected 4 parts: column, column, PK constraint, FK constraint"
        );
        assert!(
            parts[0].contains("[Id] UNIQUEIDENTIFIER"),
            "Part 0 should be Id column"
        );
        assert!(
            parts[1].contains("[Name] NVARCHAR"),
            "Part 1 should be Name column"
        );
        assert!(
            parts[2].starts_with("CONSTRAINT [PK_Test]"),
            "Part 2 should be PK constraint"
        );
        assert!(
            parts[3].starts_with("CONSTRAINT [FK_Test_Self]"),
            "Part 3 should be FK constraint"
        );

        // Test SQL2: Multiple comma-less constraints
        let sql2 = r#"[Status] NVARCHAR(20) NOT NULL
    CONSTRAINT [PK_Test] PRIMARY KEY ([Id] ASC)
    CONSTRAINT [FK_Test_Parent] FOREIGN KEY ([ParentId]) REFERENCES [dbo].[Other] ([Id]),
    CONSTRAINT [CK_Test_Status] CHECK ([Status] IN ('Active', 'Inactive'))"#;

        let parts = super::split_by_top_level_comma(sql2);
        println!("\nSQL2 parts:");
        for (i, part) in parts.iter().enumerate() {
            println!("  Part {}: {}", i, part.replace('\n', "\\n"));
        }

        assert_eq!(parts.len(), 4, "Expected 4 parts: column, PK, FK, CHECK");
        assert!(
            parts[0].contains("[Status] NVARCHAR"),
            "Part 0 should be Status column"
        );
        assert!(
            parts[1].starts_with("CONSTRAINT [PK_Test]"),
            "Part 1 should be PK constraint"
        );
        assert!(
            parts[2].starts_with("CONSTRAINT [FK_Test_Parent]"),
            "Part 2 should be FK constraint"
        );
        assert!(
            parts[3].starts_with("CONSTRAINT [CK_Test_Status]"),
            "Part 3 should be CHECK constraint"
        );
    }

    #[test]
    fn test_split_inline_check_constraint_not_split() {
        // Inline CHECK constraint (without CONSTRAINT keyword) should NOT be split from column
        // This is the AuditLog case: [Action] NVARCHAR(50) NOT NULL CHECK ([Action] IN ('Insert', 'Update', 'Delete'))
        let sql = r#"[Id] UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID(),
    [Action] NVARCHAR(50) NOT NULL CHECK ([Action] IN ('Insert', 'Update', 'Delete')),
    [Timestamp] DATETIME2 NOT NULL"#;

        let parts = super::split_by_top_level_comma(sql);
        println!("Inline CHECK test parts:");
        for (i, part) in parts.iter().enumerate() {
            println!("  Part {}: {}", i, part.replace('\n', "\\n").trim());
        }

        // Should be 3 parts: 3 column definitions
        // The inline CHECK constraint should NOT cause a split
        assert_eq!(
            parts.len(),
            3,
            "Expected 3 parts: 3 columns (inline CHECK should stay with column)"
        );
        assert!(
            parts[1].contains("CHECK"),
            "Part 1 should contain the inline CHECK constraint"
        );
        assert!(
            parts[1].contains("[Action] NVARCHAR"),
            "Part 1 should be the Action column definition"
        );
    }
}
