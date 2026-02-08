//! T-SQL parser using sqlparser-rs

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use anyhow::Result;
use rayon::prelude::*;
use regex::Regex;
use sqlparser::ast::Statement;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

use super::column_parser::{parse_column_definition_tokens, TokenParsedColumn};
use super::constraint_parser::{
    parse_alter_table_add_constraint_tokens_with_tokens, parse_alter_table_name_tokens_with_tokens,
    parse_table_constraint_tokens, TokenParsedConstraint,
};
use super::extended_property_parser::parse_extended_property_tokens_with_tokens;
use super::fulltext_parser::{
    parse_fulltext_catalog_tokens_with_tokens, parse_fulltext_index_tokens_with_tokens,
};
use super::function_parser::{
    detect_function_type_tokens_with_tokens, parse_alter_function_tokens_with_tokens,
    parse_create_function_full_with_tokens, parse_create_function_tokens_with_tokens,
    TokenParsedFunctionType,
};
use super::identifier_utils::format_token_sql;
use super::index_parser::{
    extract_index_filter_predicate_tokenized, parse_create_columnstore_index_tokens_with_tokens,
    parse_create_index_tokens_with_tokens, ParsedIndexColumn,
};
use super::preprocess_parser::preprocess_tsql_tokens;
use super::procedure_parser::{
    parse_alter_procedure_tokens_with_tokens, parse_create_procedure_tokens_with_tokens,
};
use super::security_parser::{
    parse_alter_role_membership_tokens_with_tokens, parse_create_role_tokens_with_tokens,
    parse_create_user_tokens_with_tokens, parse_permission_tokens_with_tokens,
    parse_sp_addrolemember_with_tokens, PermissionAction, PermissionTarget,
};
use super::sequence_parser::{
    parse_alter_sequence_tokens_with_tokens, parse_create_sequence_tokens_with_tokens,
};
use super::statement_parser::{
    try_parse_alter_view_tokens_with_tokens, try_parse_cte_dml_tokens_with_tokens,
    try_parse_drop_tokens_with_tokens, try_parse_generic_create_tokens_with_tokens,
    try_parse_merge_output_tokens_with_tokens, try_parse_xml_update_tokens_with_tokens,
};
use super::storage_parser::{
    parse_filegroup_tokens_with_tokens, parse_partition_function_tokens_with_tokens,
    parse_partition_scheme_tokens_with_tokens,
};
use super::synonym_parser::parse_create_synonym_tokens_with_tokens;
use super::table_type_parser::parse_create_table_type_tokens_with_tokens;
use super::trigger_parser::parse_create_trigger_tokens_with_tokens;
use super::tsql_dialect::ExtendedTsqlDialect;
use crate::error::SqlPackageError;
use crate::util::{contains_ci, starts_with_ci};

/// Sentinel value used to represent MAX in binary types (since sqlparser expects u64)
pub const BINARY_MAX_SENTINEL: u64 = 2_147_483_647;

// Cached regex patterns (Phase 63) — compiled once, reused on every call
static ERROR_LINE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Line:\s*(\d+)").unwrap());
static TYPE_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)CREATE\s+TYPE\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))").unwrap()
});
static INDEX_FALLBACK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)CREATE\s+(UNIQUE\s+)?(CLUSTERED|NONCLUSTERED)\s+INDEX\s+\[?(\w+)\]?\s*ON\s*(?:\[?(\w+)\]?\.)?\[?(\w+)\]?\s*\(([^)]+)\)").unwrap()
});
static PAD_INDEX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)PAD_INDEX\s*=\s*ON\b").unwrap());
static COLUMN_WITH_DIR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[?(\w+)\]?(?:\s+(ASC|DESC))?").unwrap());
static INCLUDE_COLUMNS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)INCLUDE\s*\(([^)]+)\)").unwrap());
static SIMPLE_COLUMN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[?(\w+)\]?").unwrap());
static FILL_FACTOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)FILLFACTOR\s*=\s*(\d+)").unwrap());
static DATA_COMPRESSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)DATA_COMPRESSION\s*=\s*(\w+)").unwrap());
static SYSTEM_VERSIONING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)SYSTEM_VERSIONING\s*=\s*ON").unwrap());
static HISTORY_TABLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)HISTORY_TABLE\s*=\s*\[?(\w+)\]?\.\[?(\w+)\]?").unwrap());

/// A SQL batch with its content and source location
struct Batch<'a> {
    content: &'a str,
    start_line: usize, // 1-based line number
}

/// Extract line number from sqlparser error message (format: "... at Line: X, Column: Y")
fn extract_line_from_error(error_msg: &str) -> Option<usize> {
    let caps = ERROR_LINE_RE.captures(error_msg)?;
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
    /// Collation name (e.g., "Latin1_General_CS_AS")
    /// Only populated for string columns with explicit COLLATE clause
    pub collation: Option<String>,
    /// Whether this column is GENERATED ALWAYS AS ROW START (temporal table period start column)
    pub is_generated_always_start: bool,
    /// Whether this column is GENERATED ALWAYS AS ROW END (temporal table period end column)
    pub is_generated_always_end: bool,
    /// Whether this column has the HIDDEN attribute (temporal table hidden period columns)
    pub is_hidden: bool,
    /// Dynamic data masking function (e.g., "default()", "email()", "partial(1,\"XXXX\",0)")
    pub masking_function: Option<String>,
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
    /// Original SQL text (Arc-shared to avoid deep copies into element definitions)
    pub sql_text: Arc<str>,
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
        /// Key columns in the index with sort direction
        columns: Vec<ParsedIndexColumn>,
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
        /// Whether PAD_INDEX is ON (applies fill factor to intermediate pages)
        is_padded: bool,
    },
    /// Columnstore index (CREATE CLUSTERED/NONCLUSTERED COLUMNSTORE INDEX)
    ColumnstoreIndex {
        name: String,
        table_schema: String,
        table_name: String,
        /// Whether this is a CLUSTERED columnstore index
        is_clustered: bool,
        /// Column names (only for NONCLUSTERED; empty for CLUSTERED)
        columns: Vec<String>,
        /// Data compression type (COLUMNSTORE or COLUMNSTORE_ARCHIVE)
        data_compression: Option<String>,
        /// Filter predicate for filtered NONCLUSTERED columnstore indexes
        filter_predicate: Option<String>,
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
        /// PERIOD FOR SYSTEM_TIME start column name (temporal tables)
        system_time_start_column: Option<String>,
        /// PERIOD FOR SYSTEM_TIME end column name (temporal tables)
        system_time_end_column: Option<String>,
        /// Whether SYSTEM_VERSIONING = ON is set (temporal tables)
        is_system_versioned: bool,
        /// History table schema for temporal tables (from HISTORY_TABLE option)
        history_table_schema: Option<String>,
        /// History table name for temporal tables (from HISTORY_TABLE option)
        history_table_name: Option<String>,
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
    /// Filegroup (ALTER DATABASE ... ADD FILEGROUP)
    Filegroup {
        name: String,
        /// Whether this filegroup contains memory-optimized data
        contains_memory_optimized_data: bool,
    },
    /// Partition function (CREATE PARTITION FUNCTION)
    PartitionFunction {
        name: String,
        /// Data type of the partition column (e.g., "INT", "DATETIME", "DATE")
        data_type: String,
        /// Boundary values that define partitions
        boundary_values: Vec<String>,
        /// Whether boundary is RIGHT (true for RANGE RIGHT, false for RANGE LEFT)
        is_range_right: bool,
    },
    /// Partition scheme (CREATE PARTITION SCHEME)
    PartitionScheme {
        name: String,
        /// Name of the partition function this scheme references
        partition_function: String,
        /// List of filegroups to map partitions to
        filegroups: Vec<String>,
    },
    /// Synonym (CREATE SYNONYM ... FOR ...)
    Synonym {
        schema: String,
        name: String,
        /// Target schema (the schema of the referenced object)
        target_schema: String,
        /// Target name (the name of the referenced object)
        target_name: String,
        /// Target database (for cross-database synonyms)
        target_database: Option<String>,
        /// Target server (for cross-server synonyms)
        target_server: Option<String>,
    },
    /// CREATE USER statement
    CreateUser {
        name: String,
        auth_type: String,
        login: Option<String>,
        default_schema: Option<String>,
    },
    /// CREATE ROLE statement
    CreateRole {
        name: String,
        owner: Option<String>,
    },
    /// ALTER ROLE ... ADD/DROP MEMBER statement
    AlterRoleMembership {
        role: String,
        member: String,
        is_add: bool,
    },
    /// GRANT/DENY/REVOKE permission statement
    Permission {
        action: String,
        permission: String,
        target_schema: Option<String>,
        target_name: Option<String>,
        target_type: String,
        principal: String,
        with_grant_option: bool,
        cascade: bool,
    },
    /// Security/deployment statements that should be silently skipped
    /// Server-level objects not included in dacpacs (LOGIN, CERTIFICATE, etc.)
    SkippedSecurityStatement {
        /// Type of statement (for logging/debugging)
        statement_type: String,
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
    pub fn from_statement(statement: Statement, source_file: PathBuf, sql_text: Arc<str>) -> Self {
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
        sql_text: Arc<str>,
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
        sql_text: Arc<str>,
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

        // Allocate the SQL text once as Arc<str> — shared across all statements from this batch
        let sql_arc: Arc<str> = Arc::from(trimmed);

        match Parser::parse_sql(&dialect, &preprocessed.sql) {
            Ok(parsed) => {
                for stmt in parsed {
                    // Use the original SQL text, not preprocessed, for storage
                    // but include any extracted defaults
                    if preprocessed.extracted_defaults.is_empty() {
                        statements.push(ParsedStatement::from_statement(
                            stmt,
                            path.to_path_buf(),
                            Arc::clone(&sql_arc),
                        ));
                    } else {
                        statements.push(ParsedStatement::from_statement_with_defaults(
                            stmt,
                            path.to_path_buf(),
                            Arc::clone(&sql_arc),
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
                        sql_arc,
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

/// Try to parse a statement using fallback token-based parsing.
/// Phase 76: Single tokenization — tokens are produced once and shared across all parser attempts.
/// Returns Some(FallbackStatementType) if the statement matches a known pattern.
fn try_fallback_parse(sql: &str) -> Option<FallbackStatementType> {
    // Phase 76: Tokenize once and reuse across all parser attempts.
    // This eliminates 5-20 redundant tokenizations per fallback-parsed statement.
    let dialect = MsSqlDialect {};
    let shared_tokens = Tokenizer::new(&dialect, sql).tokenize_with_location().ok();

    // Helper to clone shared tokens for each parser attempt
    let tk = || -> Vec<TokenWithSpan> { shared_tokens.clone().unwrap_or_default() };

    // Check for CREATE PROCEDURE or CREATE PROC (T-SQL shorthand)
    if contains_ci(sql, "CREATE PROCEDURE")
        || contains_ci(sql, "CREATE OR ALTER PROCEDURE")
        || contains_ci(sql, "CREATE PROC")
        || contains_ci(sql, "CREATE OR ALTER PROC")
    {
        if let Some((schema, name)) = parse_create_procedure_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::Procedure { schema, name });
        }
    }

    // Check for ALTER PROCEDURE or ALTER PROC (T-SQL shorthand)
    // Note: sqlparser doesn't support ALTER PROCEDURE, so we use fallback
    if contains_ci(sql, "ALTER PROCEDURE") || contains_ci(sql, "ALTER PROC") {
        if let Some((schema, name)) = parse_alter_procedure_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::Procedure { schema, name });
        }
    }

    // Check for CREATE FUNCTION
    if contains_ci(sql, "CREATE FUNCTION") || contains_ci(sql, "CREATE OR ALTER FUNCTION") {
        if let Some((schema, name)) = parse_create_function_tokens_with_tokens(tk()) {
            let function_type = detect_function_type_with_tokens(tk());
            let parameters = extract_function_parameters_with_tokens(tk());
            let return_type = extract_function_return_type_with_tokens(tk());
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
    if contains_ci(sql, "ALTER FUNCTION") {
        if let Some((schema, name)) = parse_alter_function_tokens_with_tokens(tk()) {
            let function_type = detect_function_type_with_tokens(tk());
            let parameters = extract_function_parameters_with_tokens(tk());
            let return_type = extract_function_return_type_with_tokens(tk());
            return Some(FallbackStatementType::Function {
                schema,
                name,
                function_type,
                parameters,
                return_type,
            });
        }
    }

    // Check for CREATE COLUMNSTORE INDEX (must be before regular index check)
    if contains_ci(sql, "COLUMNSTORE INDEX") {
        if let Some(parsed) = parse_create_columnstore_index_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::ColumnstoreIndex {
                name: parsed.name,
                table_schema: parsed.table_schema,
                table_name: parsed.table_name,
                is_clustered: parsed.is_clustered,
                columns: parsed.columns,
                data_compression: parsed.data_compression,
                filter_predicate: parsed.filter_predicate,
            });
        }
    }

    // Check for CREATE CLUSTERED/NONCLUSTERED INDEX (T-SQL specific syntax)
    if contains_ci(sql, "CREATE CLUSTERED INDEX")
        || contains_ci(sql, "CREATE NONCLUSTERED INDEX")
        || contains_ci(sql, "CREATE UNIQUE CLUSTERED INDEX")
        || contains_ci(sql, "CREATE UNIQUE NONCLUSTERED INDEX")
    {
        if let Some(index_info) = extract_index_info_with_tokens(sql, tk()) {
            return Some(index_info);
        }
    }

    // Check for CREATE FULLTEXT INDEX (must check before generic CREATE fallback)
    // Use token-based parser (Phase 15.3 B7)
    if contains_ci(sql, "CREATE FULLTEXT INDEX") {
        if let Some(parsed) = parse_fulltext_index_tokens_with_tokens(tk()) {
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
    if contains_ci(sql, "CREATE FULLTEXT CATALOG") {
        if let Some(parsed) = parse_fulltext_catalog_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::FullTextCatalog {
                name: parsed.name,
                is_default: parsed.is_default,
            });
        }
    }

    // Check for ALTER DATABASE SCOPED CONFIGURATION — silently skip.
    // DacFx does not model these statements (they produce SQL70001 errors in DacFx builds).
    // In real SSDT projects, these go in post-deployment scripts, not regular SQL files.
    if contains_ci(sql, "ALTER DATABASE") && contains_ci(sql, "SCOPED CONFIGURATION") {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "DATABASE_SCOPED_CONFIGURATION".to_string(),
        });
    }

    // Check for ALTER DATABASE ... ADD FILEGROUP
    // Must check before generic ALTER DATABASE handling
    if contains_ci(sql, "ALTER DATABASE") && contains_ci(sql, "ADD FILEGROUP") {
        if let Some(parsed) = parse_filegroup_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::Filegroup {
                name: parsed.name,
                contains_memory_optimized_data: parsed.contains_memory_optimized_data,
            });
        }
    }

    // Check for CREATE PARTITION FUNCTION
    if contains_ci(sql, "CREATE PARTITION FUNCTION") {
        if let Some(parsed) = parse_partition_function_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::PartitionFunction {
                name: parsed.name,
                data_type: parsed.data_type,
                boundary_values: parsed.boundary_values,
                is_range_right: parsed.is_range_right,
            });
        }
    }

    // Check for CREATE PARTITION SCHEME
    if contains_ci(sql, "CREATE PARTITION SCHEME") {
        if let Some(parsed) = parse_partition_scheme_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::PartitionScheme {
                name: parsed.name,
                partition_function: parsed.partition_function,
                filegroups: parsed.filegroups,
            });
        }
    }

    // Check for CREATE SEQUENCE (T-SQL multiline syntax not fully supported by sqlparser)
    if contains_ci(sql, "CREATE SEQUENCE") {
        if let Some(parsed) = parse_create_sequence_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::Sequence {
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
            });
        }
    }

    // Check for ALTER SEQUENCE
    // Note: sqlparser doesn't support ALTER SEQUENCE, so we use fallback
    if contains_ci(sql, "ALTER SEQUENCE") {
        if let Some(parsed) = parse_alter_sequence_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::Sequence {
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
            });
        }
    }

    // Check for CREATE TYPE (user-defined types)
    // Scalar types use: CREATE TYPE x FROM basetype
    // Table types use: CREATE TYPE x AS TABLE
    // Uses token-based parsing (Phase 15.8 J6) to handle any whitespace between keywords
    if contains_ci(sql, "CREATE TYPE") {
        // Check if this is a scalar type (FROM basetype) or table type (AS TABLE)
        match is_scalar_type_definition(sql) {
            Some(true) => {
                // Scalar type - CREATE TYPE [dbo].[TypeName] FROM basetype [NULL|NOT NULL]
                if let Some((schema, name)) = extract_type_name(sql) {
                    if let Some(scalar_info) = extract_scalar_type_info(sql) {
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
                if let Some(parsed) = parse_create_table_type_tokens_with_tokens(tk()) {
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
    if contains_ci(sql, "CREATE TABLE") {
        if let Some(table_info) = extract_table_structure(sql) {
            return Some(table_info);
        }
    }

    // Check for EXEC sp_addextendedproperty
    if contains_ci(sql, "SP_ADDEXTENDEDPROPERTY") {
        if let Some(property) = extract_extended_property_from_sql_with_tokens(sql, tk()) {
            return Some(FallbackStatementType::ExtendedProperty { property });
        }
    }

    // Check for CREATE TRIGGER
    if contains_ci(sql, "CREATE TRIGGER") || contains_ci(sql, "CREATE OR ALTER TRIGGER") {
        if let Some(trigger) = extract_trigger_info_with_tokens(sql, tk()) {
            return Some(trigger);
        }
    }

    // Check for security statements — route USER, ROLE, PERMISSION, ROLE_MEMBERSHIP
    // to actual parsers; remaining categories (LOGIN, CERTIFICATE, etc.) are silently skipped
    if let Some(result) = try_security_statement_dispatch_with_tokens(sql, &tk) {
        return Some(result);
    }

    // Check for CREATE SYNONYM (must be before generic CREATE fallback to avoid being
    // captured as RawStatement with object_type "SYNONYM" which would be silently dropped)
    if contains_ci(sql, "CREATE SYNONYM") {
        if let Some(parsed) = parse_create_synonym_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::Synonym {
                schema: parsed.schema,
                name: parsed.name,
                target_schema: parsed.target_schema,
                target_name: parsed.target_name,
                target_database: parsed.target_database,
                target_server: parsed.target_server,
            });
        }
    }

    // Check for ALTER VIEW (e.g., ALTER VIEW WITH SCHEMABINDING — sqlparser-rs fails on bare WITH keywords)
    // Must be before generic CREATE fallback. Returns RawStatement with object_type "VIEW"
    // which routes to write_raw_view() in the XML writer.
    if contains_ci(sql, "ALTER") && contains_ci(sql, "VIEW") {
        if let Some(parsed) = try_parse_alter_view_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::RawStatement {
                object_type: parsed.object_type,
                schema: parsed.schema,
                name: parsed.name,
            });
        }
    }

    // Generic fallback for any other CREATE statements
    if let Some(parsed) = try_parse_generic_create_tokens_with_tokens(tk()) {
        return Some(FallbackStatementType::RawStatement {
            object_type: parsed.object_type,
            schema: parsed.schema,
            name: parsed.name,
        });
    }

    // Check for ALTER TABLE ... ADD CONSTRAINT
    if contains_ci(sql, "ALTER TABLE") && contains_ci(sql, "ADD CONSTRAINT") {
        if let Some(fallback) = extract_alter_table_add_constraint_with_tokens(tk()) {
            return Some(fallback);
        }
    }

    // Generic fallback for ALTER TABLE statements that can't be parsed
    if contains_ci(sql, "ALTER TABLE") {
        if let Some((schema, name)) = parse_alter_table_name_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::RawStatement {
                object_type: "AlterTable".to_string(),
                schema,
                name,
            });
        }
    }

    // Fallback for DROP statements that sqlparser doesn't support
    if let Some(fallback) = try_drop_fallback_with_tokens(tk()) {
        return Some(fallback);
    }

    // Fallback for CTE with DELETE/UPDATE/INSERT/MERGE
    if let Some(fallback) = try_cte_dml_fallback_with_tokens(tk()) {
        return Some(fallback);
    }

    // Fallback for MERGE with OUTPUT clause
    if let Some(fallback) = try_merge_output_fallback_with_tokens(tk()) {
        return Some(fallback);
    }

    // Fallback for UPDATE with XML methods (.modify(), .value(), etc.)
    if let Some(fallback) = try_xml_method_fallback_with_tokens(tk()) {
        return Some(fallback);
    }

    None
}

/// Fallback for DROP statements that sqlparser doesn't support
/// Handles: DROP SYNONYM, DROP TRIGGER, DROP INDEX ... ON, DROP PROC
/// Phase 15.5: Uses token-based parsing instead of regex
/// Phase 76: DROP fallback using pre-tokenized tokens
fn try_drop_fallback_with_tokens(tokens: Vec<TokenWithSpan>) -> Option<FallbackStatementType> {
    let parsed = try_parse_drop_tokens_with_tokens(tokens)?;
    Some(FallbackStatementType::RawStatement {
        object_type: parsed.drop_type.object_type_str().to_string(),
        schema: parsed.schema,
        name: parsed.name,
    })
}

/// Phase 76: CTE DML fallback using pre-tokenized tokens
fn try_cte_dml_fallback_with_tokens(tokens: Vec<TokenWithSpan>) -> Option<FallbackStatementType> {
    let parsed = try_parse_cte_dml_tokens_with_tokens(tokens)?;
    Some(FallbackStatementType::RawStatement {
        object_type: format!("CteWith{}", parsed.dml_type.as_str()),
        schema: "dbo".to_string(),
        name: "anonymous".to_string(),
    })
}

/// Phase 76: MERGE OUTPUT fallback using pre-tokenized tokens
fn try_merge_output_fallback_with_tokens(
    tokens: Vec<TokenWithSpan>,
) -> Option<FallbackStatementType> {
    let parsed = try_parse_merge_output_tokens_with_tokens(tokens)?;
    Some(FallbackStatementType::RawStatement {
        object_type: "MergeWithOutput".to_string(),
        schema: parsed.schema,
        name: parsed.name,
    })
}

/// Phase 76: XML method fallback using pre-tokenized tokens
fn try_xml_method_fallback_with_tokens(
    tokens: Vec<TokenWithSpan>,
) -> Option<FallbackStatementType> {
    let parsed = try_parse_xml_update_tokens_with_tokens(tokens)?;
    Some(FallbackStatementType::RawStatement {
        object_type: "UpdateWithXmlMethod".to_string(),
        schema: parsed.schema,
        name: parsed.name,
    })
}

/// Dispatch security statements to appropriate parsers.
/// USER, ROLE, ROLE_MEMBERSHIP, and GRANT/DENY/REVOKE are parsed into typed variants.
/// Remaining categories (LOGIN, CERTIFICATE, etc.) are returned as SkippedSecurityStatement.
/// Phase 76: Security statement dispatch using pre-tokenized tokens
fn try_security_statement_dispatch_with_tokens(
    sql: &str,
    tk: &dyn Fn() -> Vec<TokenWithSpan>,
) -> Option<FallbackStatementType> {
    // GRANT/DENY/REVOKE — parse into Permission variant
    if starts_with_ci(sql, "GRANT ")
        || starts_with_ci(sql, "DENY ")
        || starts_with_ci(sql, "REVOKE ")
    {
        if let Some(parsed) = parse_permission_tokens_with_tokens(tk()) {
            let (target_schema, target_name, target_type) = match &parsed.target {
                PermissionTarget::Object { schema, name } => {
                    (schema.clone(), Some(name.clone()), "Object".to_string())
                }
                PermissionTarget::Schema(s) => (Some(s.clone()), None, "Schema".to_string()),
                PermissionTarget::Database => (None, None, "Database".to_string()),
            };
            let action = match parsed.action {
                PermissionAction::Grant => "Grant",
                PermissionAction::Deny => "Deny",
                PermissionAction::Revoke => "Revoke",
            };
            return Some(FallbackStatementType::Permission {
                action: action.to_string(),
                permission: parsed.permission,
                target_schema,
                target_name,
                target_type,
                principal: parsed.principal,
                with_grant_option: parsed.with_grant_option,
                cascade: parsed.cascade,
            });
        }
        // If parsing fails, fall through to skip
        let action = if starts_with_ci(sql, "GRANT ") {
            "GRANT"
        } else if starts_with_ci(sql, "DENY ") {
            "DENY"
        } else {
            "REVOKE"
        };
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: action.to_string(),
        });
    }

    // Role membership — sp_addrolemember / sp_droprolemember / ALTER ROLE ... ADD/DROP MEMBER
    if contains_ci(sql, "SP_ADDROLEMEMBER") || contains_ci(sql, "SP_DROPROLEMEMBER") {
        if let Some(parsed) = parse_sp_addrolemember_with_tokens(tk()) {
            return Some(FallbackStatementType::AlterRoleMembership {
                role: parsed.role,
                member: parsed.member,
                is_add: parsed.is_add,
            });
        }
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "ROLE_MEMBERSHIP".to_string(),
        });
    }
    if contains_ci(sql, "ALTER ROLE") && contains_ci(sql, "MEMBER") {
        if let Some(parsed) = parse_alter_role_membership_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::AlterRoleMembership {
                role: parsed.role,
                member: parsed.member,
                is_add: parsed.is_add,
            });
        }
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "ROLE_MEMBERSHIP".to_string(),
        });
    }

    // Login management (server-level) — always skip
    if contains_ci(sql, "CREATE LOGIN")
        || contains_ci(sql, "ALTER LOGIN")
        || contains_ci(sql, "DROP LOGIN")
    {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "LOGIN".to_string(),
        });
    }

    // CREATE USER — parse into CreateUser variant (ALTER/DROP USER still skipped)
    if contains_ci(sql, "CREATE USER") {
        if let Some(parsed) = parse_create_user_tokens_with_tokens(tk()) {
            let (auth_type_str, login) = match &parsed.auth_type {
                super::security_parser::UserAuthType::Login(l) => {
                    ("Login".to_string(), Some(l.clone()))
                }
                super::security_parser::UserAuthType::WithoutLogin => {
                    ("WithoutLogin".to_string(), None)
                }
                super::security_parser::UserAuthType::ExternalProvider => {
                    ("ExternalProvider".to_string(), None)
                }
                super::security_parser::UserAuthType::Default => ("Default".to_string(), None),
            };
            return Some(FallbackStatementType::CreateUser {
                name: parsed.name,
                auth_type: auth_type_str,
                login,
                default_schema: parsed.default_schema,
            });
        }
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "USER".to_string(),
        });
    }
    if contains_ci(sql, "ALTER USER") || contains_ci(sql, "DROP USER") {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "USER".to_string(),
        });
    }

    // Application role management (must check before generic ROLE)
    if contains_ci(sql, "CREATE APPLICATION ROLE")
        || contains_ci(sql, "ALTER APPLICATION ROLE")
        || contains_ci(sql, "DROP APPLICATION ROLE")
    {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "APPLICATION_ROLE".to_string(),
        });
    }

    // Server role management (must check before generic ROLE)
    if contains_ci(sql, "CREATE SERVER ROLE")
        || contains_ci(sql, "ALTER SERVER ROLE")
        || contains_ci(sql, "DROP SERVER ROLE")
    {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "SERVER_ROLE".to_string(),
        });
    }

    // CREATE ROLE — parse into CreateRole variant (ALTER/DROP ROLE still skipped)
    if contains_ci(sql, "CREATE ROLE") {
        if let Some(parsed) = parse_create_role_tokens_with_tokens(tk()) {
            return Some(FallbackStatementType::CreateRole {
                name: parsed.name,
                owner: parsed.owner,
            });
        }
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "ROLE".to_string(),
        });
    }
    if contains_ci(sql, "ALTER ROLE") || contains_ci(sql, "DROP ROLE") {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "ROLE".to_string(),
        });
    }

    // Certificate management — always skip
    if contains_ci(sql, "CREATE CERTIFICATE")
        || contains_ci(sql, "ALTER CERTIFICATE")
        || contains_ci(sql, "DROP CERTIFICATE")
    {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "CERTIFICATE".to_string(),
        });
    }

    // Asymmetric key management — always skip
    if contains_ci(sql, "CREATE ASYMMETRIC KEY")
        || contains_ci(sql, "ALTER ASYMMETRIC KEY")
        || contains_ci(sql, "DROP ASYMMETRIC KEY")
    {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "ASYMMETRIC_KEY".to_string(),
        });
    }

    // Symmetric key management — always skip
    if contains_ci(sql, "CREATE SYMMETRIC KEY")
        || contains_ci(sql, "ALTER SYMMETRIC KEY")
        || contains_ci(sql, "DROP SYMMETRIC KEY")
    {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "SYMMETRIC_KEY".to_string(),
        });
    }

    // Credential management — always skip
    if contains_ci(sql, "CREATE CREDENTIAL")
        || contains_ci(sql, "ALTER CREDENTIAL")
        || contains_ci(sql, "DROP CREDENTIAL")
    {
        return Some(FallbackStatementType::SkippedSecurityStatement {
            statement_type: "CREDENTIAL".to_string(),
        });
    }

    None
}

/// Extract schema and name from ALTER TABLE statement
/// Phase 76: Extract ALTER TABLE ADD CONSTRAINT using pre-tokenized tokens
fn extract_alter_table_add_constraint_with_tokens(
    tokens: Vec<TokenWithSpan>,
) -> Option<FallbackStatementType> {
    let parsed = parse_alter_table_add_constraint_tokens_with_tokens(tokens)?;
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

/// Phase 76: Extract extended property using pre-tokenized tokens
fn extract_extended_property_from_sql_with_tokens(
    sql: &str,
    tokens: Vec<TokenWithSpan>,
) -> Option<ExtractedExtendedProperty> {
    if let Some(parsed) = parse_extended_property_tokens_with_tokens(tokens) {
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

    // Fallback to the original string-based extractor
    extract_extended_property_from_sql(sql)
}

/// Try to extract any CREATE statement as a generic fallback
/// Phase 15.5: Uses token-based parsing (A5) instead of regex
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

/// Extract schema and name from CREATE TYPE statement
fn extract_type_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // CREATE TYPE [dbo].[TypeName] AS TABLE
    // CREATE TYPE dbo.TypeName AS TABLE
    // Use [^\]]+ for bracketed identifiers to capture special characters like &
    let caps = TYPE_NAME_RE.captures(sql)?;
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
fn extract_scalar_type_info(sql: &str) -> Option<ScalarTypeInfo> {
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
/// Phase 76: Extract trigger info using pre-tokenized tokens
fn extract_trigger_info_with_tokens(
    _sql: &str,
    tokens: Vec<TokenWithSpan>,
) -> Option<FallbackStatementType> {
    let parsed = parse_create_trigger_tokens_with_tokens(tokens)?;
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
/// Phase 76: Detect function type using pre-tokenized tokens
fn detect_function_type_with_tokens(tokens: Vec<TokenWithSpan>) -> FallbackFunctionType {
    match detect_function_type_tokens_with_tokens(tokens) {
        TokenParsedFunctionType::Scalar => FallbackFunctionType::Scalar,
        TokenParsedFunctionType::TableValued => FallbackFunctionType::TableValued,
        TokenParsedFunctionType::InlineTableValued => FallbackFunctionType::InlineTableValued,
    }
}

/// Phase 76: Extract function parameters using pre-tokenized tokens
fn extract_function_parameters_with_tokens(
    tokens: Vec<TokenWithSpan>,
) -> Vec<ExtractedFunctionParameter> {
    if let Some(func) = parse_create_function_full_with_tokens(tokens) {
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

/// Phase 76: Extract function return type using pre-tokenized tokens
fn extract_function_return_type_with_tokens(tokens: Vec<TokenWithSpan>) -> Option<String> {
    parse_create_function_full_with_tokens(tokens).and_then(|f| f.return_type)
}

/// Extract index information from CREATE CLUSTERED/NONCLUSTERED INDEX statement
///
/// Uses token-based parsing (Phase 15.3 B6) for improved maintainability and edge case handling.
/// Phase 76: Extract index info using pre-tokenized tokens
fn extract_index_info_with_tokens(
    sql: &str,
    tokens: Vec<TokenWithSpan>,
) -> Option<FallbackStatementType> {
    if let Some(parsed) = parse_create_index_tokens_with_tokens(tokens) {
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
            is_padded: parsed.is_padded,
        });
    }

    // Fallback to regex for edge cases not yet covered by token parser
    let caps = INDEX_FALLBACK_RE.captures(sql)?;
    let is_unique = caps.get(1).is_some();
    let is_clustered = caps
        .get(2)
        .map(|m| m.as_str().eq_ignore_ascii_case("CLUSTERED"))
        .unwrap_or(false);
    let index_name = caps.get(3)?.as_str().to_string();
    let table_schema = caps
        .get(4)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dbo".to_string());
    let table_name = caps.get(5)?.as_str().to_string();
    let columns_str = caps.get(6)?.as_str();
    let columns: Vec<ParsedIndexColumn> = columns_str
        .split(',')
        .filter_map(|col| {
            let col = col.trim();
            if col.is_empty() {
                return None;
            }
            let caps = COLUMN_WITH_DIR_RE.captures(col)?;
            Some(ParsedIndexColumn {
                name: caps.get(1)?.as_str().to_string(),
                is_descending: caps
                    .get(2)
                    .map(|m| m.as_str().eq_ignore_ascii_case("DESC"))
                    .unwrap_or(false),
            })
        })
        .collect();
    let include_columns = INCLUDE_COLUMNS_RE
        .captures(sql)
        .map(|caps| {
            caps.get(1)
                .unwrap()
                .as_str()
                .split(',')
                .filter_map(|col| {
                    let col = col.trim();
                    SIMPLE_COLUMN_RE
                        .captures(col)
                        .and_then(|c| c.get(1))
                        .map(|m| m.as_str().to_string())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let fill_factor = extract_index_fill_factor(sql);
    let filter_predicate = extract_index_filter_predicate_tokenized(sql);
    let data_compression = extract_index_data_compression(sql);
    let is_padded = extract_index_pad_index(sql);
    Some(FallbackStatementType::Index {
        name: index_name,
        table_schema,
        table_name,
        columns,
        include_columns,
        is_unique,
        is_clustered,
        fill_factor,
        filter_predicate,
        data_compression,
        is_padded,
    })
}

/// Extract PAD_INDEX option from CREATE INDEX WITH clause
fn extract_index_pad_index(sql: &str) -> bool {
    // Match PAD_INDEX = ON in WITH clause (case-insensitive, zero-alloc)
    if let Some(with_pos) = crate::util::find_ci(sql, "WITH") {
        let after_with = &sql[with_pos..];
        if contains_ci(after_with, "PAD_INDEX") {
            return PAD_INDEX_RE.is_match(after_with);
        }
    }
    false
}

/// Parse a comma-separated column list, extracting column names and sort direction
/// Extract FILLFACTOR value from index WITH clause
fn extract_index_fill_factor(sql: &str) -> Option<u8> {
    FILL_FACTOR_RE
        .captures(sql)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<u8>().ok())
}

/// Extract DATA_COMPRESSION value from index WITH clause
fn extract_index_data_compression(sql: &str) -> Option<String> {
    DATA_COMPRESSION_RE
        .captures(sql)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_uppercase())
}

// Phase 20.6.1: Removed extract_index_filter_predicate() - replaced with token-based
// extract_index_filter_predicate_tokenized() from index_parser.rs

/// Extract full table structure from CREATE TABLE statement
///
fn extract_table_structure(sql: &str) -> Option<FallbackStatementType> {
    let (schema, name) = extract_generic_object_name(sql, "TABLE")?;

    // Check for graph table syntax (AS NODE or AS EDGE)
    let is_node = contains_ci(sql, "AS NODE");
    let is_edge = contains_ci(sql, "AS EDGE");

    // Find the opening parenthesis after CREATE TABLE [schema].[name]
    let table_name_pattern = format!(
        r"(?i)CREATE\s+TABLE\s+(?:\[?{}\]?\.)?\[?{}\]?\s*\(",
        regex::escape(&schema),
        regex::escape(&name)
    );
    let table_re = Regex::new(&table_name_pattern).ok()?;
    let table_match = table_re.find(sql)?;
    let paren_start = table_match.end() - 1; // Position of the opening '('

    // Find matching closing parenthesis and get remaining SQL after it
    let remaining_sql = &sql[paren_start..];
    let table_body = extract_balanced_parens(remaining_sql)?;

    // Calculate position after the closing parenthesis of the table body
    // table_body is the content inside parens, so the closing paren is at offset 1 + body_len + 1
    let body_len_with_parens = table_body.len() + 2; // +2 for the outer ( and )
    let after_body = &remaining_sql[body_len_with_parens..];

    // Parse columns, constraints, and PERIOD FOR SYSTEM_TIME from the table body
    let (columns, constraints, period) = parse_table_body(&table_body, &name);

    // Extract temporal table options from WITH clause after the closing parenthesis
    let (is_system_versioned, history_table_schema, history_table_name) =
        extract_system_versioning_options(after_body);

    Some(FallbackStatementType::Table {
        schema,
        name,
        columns,
        constraints,
        is_node,
        is_edge,
        system_time_start_column: period.start_column,
        system_time_end_column: period.end_column,
        is_system_versioned,
        history_table_schema,
        history_table_name,
    })
}

/// Extract SYSTEM_VERSIONING options from the WITH clause after a CREATE TABLE body.
/// Returns (is_system_versioned, history_table_schema, history_table_name).
fn extract_system_versioning_options(after_body: &str) -> (bool, Option<String>, Option<String>) {
    // Check for SYSTEM_VERSIONING = ON (zero-alloc)
    if !contains_ci(after_body, "SYSTEM_VERSIONING") {
        return (false, None, None);
    }

    // Check if it's ON (not OFF)
    if !SYSTEM_VERSIONING_RE.is_match(after_body) {
        return (false, None, None);
    }

    // Extract HISTORY_TABLE = [schema].[name]
    let (history_schema, history_name) = HISTORY_TABLE_RE
        .captures(after_body)
        .map(|caps| {
            (
                Some(caps.get(1).unwrap().as_str().to_string()),
                Some(caps.get(2).unwrap().as_str().to_string()),
            )
        })
        .unwrap_or((None, None));

    (true, history_schema, history_name)
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

/// Result of parsing a PERIOD FOR SYSTEM_TIME clause
#[derive(Debug, Clone, Default)]
struct ParsedSystemTimePeriod {
    start_column: Option<String>,
    end_column: Option<String>,
}

/// Parse table body to extract columns, constraints, and PERIOD FOR SYSTEM_TIME
fn parse_table_body(
    body: &str,
    table_name: &str,
) -> (
    Vec<ExtractedTableColumn>,
    Vec<ExtractedTableConstraint>,
    ParsedSystemTimePeriod,
) {
    // Split by top-level commas (not inside parentheses)
    let parts = split_by_top_level_comma(body);

    // Most parts are columns, with a few constraints
    let mut columns = Vec::with_capacity(parts.len());
    let mut constraints = Vec::with_capacity(parts.len().min(4));
    let mut period = ParsedSystemTimePeriod::default();

    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check for PERIOD FOR SYSTEM_TIME ([col1], [col2])
        if starts_with_ci(trimmed, "PERIOD") && contains_ci(trimmed, "SYSTEM_TIME") {
            if let Some(parsed) = parse_period_for_system_time(trimmed) {
                period = parsed;
            }
            continue;
        }

        // Check if this is a table-level constraint
        if starts_with_ci(trimmed, "CONSTRAINT")
            || starts_with_ci(trimmed, "PRIMARY KEY")
            || starts_with_ci(trimmed, "FOREIGN KEY")
            || starts_with_ci(trimmed, "UNIQUE")
            || starts_with_ci(trimmed, "CHECK")
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

    (columns, constraints, period)
}

/// Parse PERIOD FOR SYSTEM_TIME ([start_col], [end_col])
fn parse_period_for_system_time(def: &str) -> Option<ParsedSystemTimePeriod> {
    // Extract the content within parentheses
    let paren_start = def.find('(')?;
    let paren_end = def.rfind(')')?;
    if paren_end <= paren_start {
        return None;
    }

    let inner = &def[paren_start + 1..paren_end];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 2 {
        return None;
    }

    // Strip brackets and whitespace from column names
    let start_col = parts[0]
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    let end_col = parts[1]
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();

    Some(ParsedSystemTimePeriod {
        start_column: Some(start_col),
        end_column: Some(end_col),
    })
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
                current_part.push_str(&format_token_sql(&token.token));
                seen_content_in_part = true;
            }
            Token::Whitespace(ws) => {
                // Add whitespace but don't mark as seen content
                current_part.push_str(&ws.to_string());
            }
            _ => {
                current_part.push_str(&format_token_sql(&token.token));
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
        collation: parsed.collation,
        is_generated_always_start: parsed.is_generated_always_start,
        is_generated_always_end: parsed.is_generated_always_end,
        is_hidden: parsed.is_hidden,
        masking_function: parsed.masking_function,
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

/// Split SQL content into batches by GO statement, tracking line numbers.
/// This function is comment-aware and ignores GO statements inside block comments.
fn split_batches(content: &str) -> Vec<Batch<'_>> {
    // Estimate ~1 batch per 20 lines (GO separators are relatively sparse)
    let line_count = content.lines().count();
    let estimated_batches = (line_count / 20).max(1);
    let mut batches = Vec::with_capacity(estimated_batches);
    let mut current_pos = 0;
    let mut batch_start = 0;
    let mut current_line = 1; // 1-based line numbers
    let mut batch_start_line = 1;
    let mut in_block_comment = false;

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

        // Track block comment state by scanning this line for /* and */ markers.
        // We need to track comment state character by character to handle:
        // - Multiple /* or */ on same line
        // - Comment markers after code on the same line
        // Note: We don't need to handle string literals here since GO must be
        // on its own line - a line with GO and a string would never match anyway.
        let mut i = 0;
        let line_bytes = line.as_bytes();
        while i < line_bytes.len() {
            if !in_block_comment {
                // Check for /* to enter block comment
                if i + 1 < line_bytes.len() && line_bytes[i] == b'/' && line_bytes[i + 1] == b'*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
                // Check for -- to skip rest of line (line comment)
                if i + 1 < line_bytes.len() && line_bytes[i] == b'-' && line_bytes[i + 1] == b'-' {
                    // Rest of line is a comment, no need to scan further
                    break;
                }
            } else {
                // Inside block comment, look for */ to exit
                if i + 1 < line_bytes.len() && line_bytes[i] == b'*' && line_bytes[i + 1] == b'/' {
                    in_block_comment = false;
                    i += 2;
                    continue;
                }
            }
            i += 1;
        }

        // GO must be on its own line (optionally with whitespace)
        // Also handle GO; with trailing semicolon (common in some SQL scripts)
        // Only treat GO as a batch separator if we're NOT inside a block comment
        if !in_block_comment
            && (trimmed.eq_ignore_ascii_case("go") || trimmed.eq_ignore_ascii_case("go;"))
        {
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

    #[test]
    fn test_fallback_skip_alter_database_scoped_configuration_set() {
        let sql = "ALTER DATABASE SCOPED CONFIGURATION SET MAXDOP = 4;";
        let fallback = try_fallback_parse(sql);
        assert!(fallback.is_some());
        match fallback.unwrap() {
            FallbackStatementType::SkippedSecurityStatement { statement_type } => {
                assert_eq!(statement_type, "DATABASE_SCOPED_CONFIGURATION");
            }
            other => panic!("Expected SkippedSecurityStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_fallback_skip_alter_database_scoped_configuration_on_off() {
        let sql = "ALTER DATABASE SCOPED CONFIGURATION SET LEGACY_CARDINALITY_ESTIMATION = ON;";
        let fallback = try_fallback_parse(sql);
        assert!(fallback.is_some());
        match fallback.unwrap() {
            FallbackStatementType::SkippedSecurityStatement { statement_type } => {
                assert_eq!(statement_type, "DATABASE_SCOPED_CONFIGURATION");
            }
            other => panic!("Expected SkippedSecurityStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_fallback_skip_alter_database_scoped_configuration_for_secondary() {
        let sql = "ALTER DATABASE SCOPED CONFIGURATION FOR SECONDARY SET MAXDOP = 2;";
        let fallback = try_fallback_parse(sql);
        assert!(fallback.is_some());
        match fallback.unwrap() {
            FallbackStatementType::SkippedSecurityStatement { statement_type } => {
                assert_eq!(statement_type, "DATABASE_SCOPED_CONFIGURATION");
            }
            other => panic!("Expected SkippedSecurityStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_fallback_skip_alter_database_scoped_configuration_clear() {
        let sql = "ALTER DATABASE SCOPED CONFIGURATION CLEAR PROCEDURE_CACHE;";
        let fallback = try_fallback_parse(sql);
        assert!(fallback.is_some());
        match fallback.unwrap() {
            FallbackStatementType::SkippedSecurityStatement { statement_type } => {
                assert_eq!(statement_type, "DATABASE_SCOPED_CONFIGURATION");
            }
            other => panic!("Expected SkippedSecurityStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_fallback_skip_alter_database_scoped_configuration_identity_cache() {
        let sql = "ALTER DATABASE SCOPED CONFIGURATION SET IDENTITY_CACHE = OFF;";
        let fallback = try_fallback_parse(sql);
        assert!(fallback.is_some());
        match fallback.unwrap() {
            FallbackStatementType::SkippedSecurityStatement { statement_type } => {
                assert_eq!(statement_type, "DATABASE_SCOPED_CONFIGURATION");
            }
            other => panic!("Expected SkippedSecurityStatement, got {:?}", other),
        }
    }

    // ========================================================================
    // ALTER VIEW fallback tests (Phase 60)
    // ========================================================================

    #[test]
    fn test_fallback_alter_view_with_schemabinding() {
        let sql = r#"ALTER VIEW [dbo].[BoundView]
WITH SCHEMABINDING
AS
SELECT [Id] FROM [dbo].[Users];"#;
        let fallback = try_fallback_parse(sql);
        assert!(
            fallback.is_some(),
            "ALTER VIEW WITH SCHEMABINDING should be handled by fallback"
        );
        match fallback.unwrap() {
            FallbackStatementType::RawStatement {
                object_type,
                schema,
                name,
            } => {
                assert_eq!(object_type, "VIEW");
                assert_eq!(schema, "dbo");
                assert_eq!(name, "BoundView");
            }
            other => panic!("Expected RawStatement for ALTER VIEW, got {:?}", other),
        }
    }

    #[test]
    fn test_fallback_alter_view_with_schemabinding_and_view_metadata() {
        let sql = r#"ALTER VIEW [sales].[OrderSummary]
WITH SCHEMABINDING, VIEW_METADATA
AS
SELECT [OrderId], [Total] FROM [sales].[Orders];"#;
        let fallback = try_fallback_parse(sql);
        assert!(fallback.is_some());
        match fallback.unwrap() {
            FallbackStatementType::RawStatement {
                object_type,
                schema,
                name,
            } => {
                assert_eq!(object_type, "VIEW");
                assert_eq!(schema, "sales");
                assert_eq!(name, "OrderSummary");
            }
            other => panic!("Expected RawStatement for ALTER VIEW, got {:?}", other),
        }
    }

    #[test]
    fn test_fallback_alter_view_unqualified_name() {
        let sql = r#"ALTER VIEW [SimpleView]
WITH SCHEMABINDING
AS
SELECT 1 AS [Val];"#;
        let fallback = try_fallback_parse(sql);
        assert!(fallback.is_some());
        match fallback.unwrap() {
            FallbackStatementType::RawStatement {
                object_type,
                schema,
                name,
            } => {
                assert_eq!(object_type, "VIEW");
                // parse_schema_qualified_name() defaults unqualified names to "dbo"
                assert_eq!(schema, "dbo");
                assert_eq!(name, "SimpleView");
            }
            other => panic!("Expected RawStatement for ALTER VIEW, got {:?}", other),
        }
    }
}
