//! T-SQL parser using sqlparser-rs

use std::path::{Path, PathBuf};

use anyhow::Result;
use regex::Regex;
use sqlparser::ast::Statement;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::parser::Parser;

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
    /// Whether the column is nullable
    pub is_nullable: bool,
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
    /// Whether the column is nullable
    pub is_nullable: bool,
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
    /// Inline CHECK constraint name (if any)
    pub check_constraint_name: Option<String>,
    /// Inline CHECK constraint expression (if any)
    pub check_expression: Option<String>,
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
    },
    UserDefinedType {
        schema: String,
        name: String,
        columns: Vec<ExtractedTableTypeColumn>,
        constraints: Vec<ExtractedTableTypeConstraint>,
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
    /// Extended property from sp_addextendedproperty
    ExtendedProperty {
        property: ExtractedExtendedProperty,
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
    if sql_upper.contains("CREATE FULLTEXT INDEX") {
        if let Some(fulltext_info) = extract_fulltext_index_info(sql) {
            return Some(fulltext_info);
        }
    }

    // Check for CREATE FULLTEXT CATALOG
    if sql_upper.contains("CREATE FULLTEXT CATALOG") {
        if let Some(catalog_info) = extract_fulltext_catalog_info(sql) {
            return Some(catalog_info);
        }
    }

    // Check for CREATE SEQUENCE (T-SQL multiline syntax not fully supported by sqlparser)
    if sql_upper.contains("CREATE SEQUENCE") {
        if let Some((schema, name)) = extract_sequence_name(sql) {
            return Some(FallbackStatementType::Sequence { schema, name });
        }
    }

    // Check for ALTER SEQUENCE
    // Note: sqlparser doesn't support ALTER SEQUENCE, so we use fallback
    if sql_upper.contains("ALTER SEQUENCE") {
        if let Some((schema, name)) = extract_alter_sequence_name(sql) {
            return Some(FallbackStatementType::Sequence { schema, name });
        }
    }

    // Check for CREATE TYPE (user-defined table types)
    if sql_upper.contains("CREATE TYPE") {
        if let Some((schema, name)) = extract_type_name(sql) {
            let (columns, constraints) = extract_table_type_structure(sql);
            return Some(FallbackStatementType::UserDefinedType {
                schema,
                name,
                columns,
                constraints,
            });
        }
    }

    // Fallback for CREATE TABLE statements that fail parsing
    if sql_upper.contains("CREATE TABLE") {
        if let Some(table_info) = extract_table_structure(sql) {
            return Some(table_info);
        }
    }

    // Check for EXEC sp_addextendedproperty
    if sql_upper.contains("SP_ADDEXTENDEDPROPERTY") {
        if let Some(property) = extract_extended_property_from_sql(sql) {
            return Some(FallbackStatementType::ExtendedProperty { property });
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
fn try_drop_fallback(sql: &str) -> Option<FallbackStatementType> {
    let sql_upper = sql.to_uppercase();

    // Check for DROP SYNONYM
    if sql_upper.contains("DROP SYNONYM") {
        let re = regex::Regex::new(
            r"(?i)DROP\s+SYNONYM\s+(?:IF\s+EXISTS\s+)?(?:\[?(\w+)\]?\.)?\[?(\w+)\]?",
        )
        .ok()?;
        let caps = re.captures(sql)?;
        let schema = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "dbo".to_string());
        let name = caps.get(2)?.as_str().to_string();
        return Some(FallbackStatementType::RawStatement {
            object_type: "DropSynonym".to_string(),
            schema,
            name,
        });
    }

    // Check for DROP TRIGGER
    if sql_upper.contains("DROP TRIGGER") {
        let re = regex::Regex::new(
            r"(?i)DROP\s+TRIGGER\s+(?:IF\s+EXISTS\s+)?(?:\[?(\w+)\]?\.)?\[?(\w+)\]?",
        )
        .ok()?;
        let caps = re.captures(sql)?;
        let schema = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "dbo".to_string());
        let name = caps.get(2)?.as_str().to_string();
        return Some(FallbackStatementType::RawStatement {
            object_type: "DropTrigger".to_string(),
            schema,
            name,
        });
    }

    // Check for DROP INDEX ... ON (T-SQL specific syntax)
    if sql_upper.contains("DROP INDEX") && sql_upper.contains(" ON ") {
        let re = regex::Regex::new(
            r"(?i)DROP\s+INDEX\s+(?:IF\s+EXISTS\s+)?\[?(\w+)\]?\s+ON\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?"
        ).ok()?;
        let caps = re.captures(sql)?;
        let index_name = caps.get(1)?.as_str().to_string();
        let schema = caps
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "dbo".to_string());
        let table_name = caps.get(3)?.as_str().to_string();
        return Some(FallbackStatementType::RawStatement {
            object_type: "DropIndex".to_string(),
            schema,
            name: format!("{}_{}", table_name, index_name),
        });
    }

    // Check for DROP PROC (abbreviation for DROP PROCEDURE)
    // Only match PROC that's not followed by EDURE (to avoid matching PROCEDURE)
    if sql_upper.contains("DROP PROC") && !sql_upper.contains("DROP PROCEDURE") {
        let re = regex::Regex::new(
            r"(?i)DROP\s+PROC\s+(?:IF\s+EXISTS\s+)?(?:\[?(\w+)\]?\.)?\[?(\w+)\]?",
        )
        .ok()?;
        let caps = re.captures(sql)?;
        let schema = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "dbo".to_string());
        let name = caps.get(2)?.as_str().to_string();
        return Some(FallbackStatementType::RawStatement {
            object_type: "DropProcedure".to_string(),
            schema,
            name,
        });
    }

    None
}

/// Fallback for CTEs followed by DELETE, UPDATE, INSERT, or MERGE
/// sqlparser only supports CTEs followed by SELECT
fn try_cte_dml_fallback(sql: &str) -> Option<FallbackStatementType> {
    let sql_upper = sql.to_uppercase();

    // Check if this is a CTE (starts with WITH)
    let trimmed_upper = sql_upper.trim();
    if !trimmed_upper.starts_with("WITH ") && !trimmed_upper.starts_with("WITH\n") {
        return None;
    }

    // Check if followed by DELETE, UPDATE, INSERT, or MERGE (after the CTE definition)
    // Look for these keywords after a closing parenthesis
    let dml_pattern = regex::Regex::new(r"(?i)\)\s*(DELETE|UPDATE|INSERT|MERGE)\b").ok()?;
    if dml_pattern.is_match(sql) {
        // Extract the DML type
        let caps = dml_pattern.captures(sql)?;
        let dml_type = caps.get(1)?.as_str().to_uppercase();

        return Some(FallbackStatementType::RawStatement {
            object_type: format!("CteWith{}", dml_type),
            schema: "dbo".to_string(),
            name: "anonymous".to_string(),
        });
    }

    None
}

/// Fallback for MERGE statements with OUTPUT clause
/// sqlparser doesn't support the OUTPUT clause on MERGE
fn try_merge_output_fallback(sql: &str) -> Option<FallbackStatementType> {
    let sql_upper = sql.to_uppercase();

    // Check for MERGE ... OUTPUT
    if sql_upper.contains("MERGE") && sql_upper.contains("OUTPUT") {
        // Extract target table name
        let re =
            regex::Regex::new(r"(?i)MERGE\s+(?:INTO\s+)?(?:\[?(\w+)\]?\.)?\[?(\w+)\]?").ok()?;
        let caps = re.captures(sql)?;
        let schema = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "dbo".to_string());
        let name = caps.get(2)?.as_str().to_string();

        return Some(FallbackStatementType::RawStatement {
            object_type: "MergeWithOutput".to_string(),
            schema,
            name,
        });
    }

    None
}

/// Fallback for UPDATE statements with XML methods (.modify(), .value(), etc.)
/// sqlparser doesn't support XML method call syntax
fn try_xml_method_fallback(sql: &str) -> Option<FallbackStatementType> {
    let sql_upper = sql.to_uppercase();

    // Check for UPDATE with XML method call pattern
    // Pattern: UPDATE ... SET [column].modify(...) or [column].value(...)
    if sql_upper.contains("UPDATE")
        && (sql_upper.contains(".MODIFY(") || sql_upper.contains(".VALUE("))
    {
        // Extract target table name
        let re = regex::Regex::new(r"(?i)UPDATE\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?").ok()?;
        let caps = re.captures(sql)?;
        let schema = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "dbo".to_string());
        let name = caps.get(2)?.as_str().to_string();

        return Some(FallbackStatementType::RawStatement {
            object_type: "UpdateWithXmlMethod".to_string(),
            schema,
            name,
        });
    }

    None
}

/// Extract schema and name from ALTER TABLE statement
fn extract_alter_table_name(sql: &str) -> Option<(String, String)> {
    // Use [^\]]+ for bracketed identifiers to capture special characters like &
    let re = regex::Regex::new(
        r"(?i)ALTER\s+TABLE\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))",
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

/// Extract extended property from sp_addextendedproperty call
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
    // Helper to extract a parameter value from @paramname = N'value' pattern
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
fn try_generic_create_fallback(sql: &str) -> Option<FallbackStatementType> {
    let re =
        regex::Regex::new(r"(?i)CREATE\s+(?:OR\s+ALTER\s+)?(\w+)\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?")
            .ok()?;

    let caps = re.captures(sql)?;
    let object_type = caps.get(1)?.as_str().to_string();
    let schema = caps
        .get(2)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dbo".to_string());
    let name = caps.get(3)?.as_str().to_string();

    Some(FallbackStatementType::RawStatement {
        object_type,
        schema,
        name,
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

/// Extract schema and name from CREATE SEQUENCE statement
fn extract_sequence_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // CREATE SEQUENCE [dbo].[SeqName]
    // CREATE SEQUENCE dbo.SeqName
    // Use [^\]]+ for bracketed identifiers to capture special characters like &
    let re = regex::Regex::new(
        r"(?i)CREATE\s+SEQUENCE\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))",
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

/// Extract columns and constraints from a table type definition
fn extract_table_type_structure(
    sql: &str,
) -> (
    Vec<ExtractedTableTypeColumn>,
    Vec<ExtractedTableTypeConstraint>,
) {
    let mut columns = Vec::new();
    let mut constraints = Vec::new();

    // Find the content between AS TABLE ( and the closing )
    let sql_upper = sql.to_uppercase();
    let start = match sql_upper.find("AS TABLE") {
        Some(idx) => idx + "AS TABLE".len(),
        None => return (columns, constraints),
    };

    // Find the opening paren after AS TABLE
    let remaining = &sql[start..];
    let paren_start = match remaining.find('(') {
        Some(idx) => start + idx + 1,
        None => return (columns, constraints),
    };

    // Find the matching closing paren (handle nested parens for types like DECIMAL(18,2))
    let mut depth = 1;
    let mut paren_end = paren_start;
    for (i, c) in sql[paren_start..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    paren_end = paren_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    if paren_end <= paren_start {
        return (columns, constraints);
    }

    let body_str = &sql[paren_start..paren_end];

    // Split by commas, handling nested parens
    let parts = split_by_top_level_comma(body_str);

    // Compile regexes outside the loop to avoid recompiling on each iteration
    let check_re = Regex::new(r"(?i)CHECK\s*\((.+)\)\s*$").unwrap();
    let idx_re = Regex::new(
        r"(?i)INDEX\s+\[?(\w+)\]?\s*(UNIQUE)?\s*(CLUSTERED|NONCLUSTERED)?\s*\(([^)]+)\)",
    )
    .unwrap();
    let col_re = Regex::new(r"(?i)^\[?(\w+)\]?\s+(\w+(?:\s*\([^)]+\))?)").unwrap();

    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        let upper = trimmed.to_uppercase();

        // Check if this is a constraint
        if upper.starts_with("PRIMARY KEY") {
            // PRIMARY KEY CLUSTERED ([Col1], [Col2])
            let is_clustered = !upper.contains("NONCLUSTERED");
            let pk_columns = extract_constraint_columns(trimmed);
            constraints.push(ExtractedTableTypeConstraint::PrimaryKey {
                columns: pk_columns,
                is_clustered,
            });
        } else if upper.starts_with("UNIQUE") {
            // UNIQUE [CLUSTERED|NONCLUSTERED] ([Col])
            let is_clustered = upper.contains("CLUSTERED") && !upper.contains("NONCLUSTERED");
            let uq_columns = extract_constraint_columns(trimmed);
            constraints.push(ExtractedTableTypeConstraint::Unique {
                columns: uq_columns,
                is_clustered,
            });
        } else if upper.starts_with("CHECK") {
            // CHECK (expression)
            if let Some(caps) = check_re.captures(trimmed) {
                if let Some(expr) = caps.get(1) {
                    constraints.push(ExtractedTableTypeConstraint::Check {
                        expression: expr.as_str().to_string(),
                    });
                }
            }
        } else if upper.starts_with("INDEX") {
            // INDEX [IX_Name] [UNIQUE] [CLUSTERED|NONCLUSTERED] ([Col])
            if let Some(caps) = idx_re.captures(trimmed) {
                let name = caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let is_unique = caps.get(2).is_some();
                let is_clustered = caps
                    .get(3)
                    .map(|m| m.as_str().to_uppercase() == "CLUSTERED")
                    .unwrap_or(false);
                let idx_columns = caps
                    .get(4)
                    .map(|m| parse_column_list(m.as_str()))
                    .unwrap_or_default();
                constraints.push(ExtractedTableTypeConstraint::Index {
                    name,
                    columns: idx_columns,
                    is_unique,
                    is_clustered,
                });
            }
        } else {
            // This is a column definition
            // Pattern: [ColumnName] DataType [NULL|NOT NULL] [DEFAULT (value)]

            if let Some(caps) = col_re.captures(trimmed) {
                let name = caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let data_type = caps
                    .get(2)
                    .map(|m| m.as_str().trim().to_uppercase())
                    .unwrap_or_default();

                // Check nullability
                let is_nullable = if upper.contains("NOT NULL") {
                    false
                } else {
                    true // Default to nullable if not specified
                };

                // Extract DEFAULT value if present
                let default_value = extract_table_type_column_default(trimmed);

                if !name.is_empty() && !data_type.is_empty() {
                    columns.push(ExtractedTableTypeColumn {
                        name,
                        data_type,
                        is_nullable,
                        default_value,
                    });
                }
            }
        }
    }

    (columns, constraints)
}

/// Extract default value from a table type column definition
fn extract_table_type_column_default(col_def: &str) -> Option<String> {
    // Pattern: DEFAULT (value) or DEFAULT value
    let default_re = Regex::new(r"(?i)DEFAULT\s+(\([^)]*(?:\([^)]*\)[^)]*)*\)|\w+\(\))").ok()?;
    default_re
        .captures(col_def)
        .and_then(|caps| caps.get(1).or(caps.get(0)))
        .map(|m| {
            let val = m.as_str();
            // If it starts with DEFAULT, remove that prefix
            if val.to_uppercase().starts_with("DEFAULT") {
                val[7..].trim().to_string()
            } else {
                val.to_string()
            }
        })
}

/// Extract schema and name from CREATE PROCEDURE statement
fn extract_procedure_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // CREATE PROCEDURE [dbo].[ProcName]
    // CREATE PROCEDURE dbo.ProcName
    // CREATE OR ALTER PROCEDURE [schema].[name]
    // CREATE PROC [dbo].[name]
    // CREATE PROCEDURE [dbo].[Name&With&Special]
    // Use [^\]]+ for bracketed identifiers to capture special characters like &
    let re = regex::Regex::new(
        r"(?i)CREATE\s+(?:OR\s+ALTER\s+)?(?:PROCEDURE|PROC)\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))",
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

/// Extract schema and name from ALTER PROCEDURE statement
fn extract_alter_procedure_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // ALTER PROCEDURE [dbo].[ProcName]
    // ALTER PROCEDURE dbo.ProcName
    // ALTER PROC [dbo].[name]
    let re = regex::Regex::new(
        r"(?i)ALTER\s+(?:PROCEDURE|PROC)\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))",
    )
    .ok()?;

    let caps = re.captures(sql)?;
    let schema = caps
        .get(1)
        .or_else(|| caps.get(2))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dbo".to_string());
    let name = caps.get(3).or_else(|| caps.get(4))?.as_str().to_string();

    Some((schema, name))
}

/// Extract schema and name from ALTER FUNCTION statement
fn extract_alter_function_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // ALTER FUNCTION [dbo].[FuncName]
    // ALTER FUNCTION dbo.FuncName
    // Use [^\]]+ for bracketed identifiers to capture special characters like &
    let re = regex::Regex::new(
        r"(?i)ALTER\s+FUNCTION\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))",
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

/// Extract schema and name from ALTER SEQUENCE statement
fn extract_alter_sequence_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // ALTER SEQUENCE [dbo].[SeqName]
    // ALTER SEQUENCE dbo.SeqName
    // Use [^\]]+ for bracketed identifiers to capture special characters like &
    let re = regex::Regex::new(
        r"(?i)ALTER\s+SEQUENCE\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))",
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

/// Extract schema and name from CREATE FUNCTION statement
fn extract_function_name(sql: &str) -> Option<(String, String)> {
    // Match patterns like:
    // CREATE FUNCTION [dbo].[FuncName]
    // CREATE FUNCTION dbo.FuncName
    // CREATE OR ALTER FUNCTION [schema].[name]
    // Use [^\]]+ for bracketed identifiers to capture special characters like &
    let re = regex::Regex::new(
        r"(?i)CREATE\s+(?:OR\s+ALTER\s+)?FUNCTION\s+(?:(?:\[([^\]]+)\]|(\w+))\.)?(?:\[([^\]]+)\]|(\w+))",
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

/// Extract parameters from a function definition
fn extract_function_parameters(sql: &str) -> Vec<ExtractedFunctionParameter> {
    let mut params = Vec::new();

    // Find the function name and the parameters that follow
    // Pattern: CREATE FUNCTION [schema].[name](@param1 TYPE, @param2 TYPE) RETURNS ...
    let sql_upper = sql.to_uppercase();

    // Find opening paren after function name
    let func_name_end = sql_upper
        .find("CREATE FUNCTION")
        .or_else(|| sql_upper.find("ALTER FUNCTION"))
        .and_then(|start| {
            // Skip past CREATE/ALTER FUNCTION and the name
            let after_keyword = &sql_upper[start..];
            after_keyword.find('(').map(|idx| start + idx)
        });

    if func_name_end.is_none() {
        return params;
    }

    let paren_start = func_name_end.unwrap();

    // Find RETURNS to know where parameters end
    let returns_pos = sql_upper.find("RETURNS");
    if returns_pos.is_none() || returns_pos.unwrap() < paren_start {
        return params;
    }

    // Extract the content between first ( and the ) before RETURNS
    let param_section = &sql[paren_start..returns_pos.unwrap()];

    // Find the closing paren
    if let Some(close_paren) = param_section.rfind(')') {
        let param_content = &param_section[1..close_paren];

        // Parse parameters - they start with @
        let param_regex = regex::Regex::new(r"@(\w+)\s+([A-Za-z0-9_\(\),\s]+?)(?:,|$)").unwrap();

        for cap in param_regex.captures_iter(param_content) {
            let name = cap
                .get(1)
                .map(|m| format!("@{}", m.as_str()))
                .unwrap_or_default();
            let data_type = cap
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            if !name.is_empty() && !data_type.is_empty() {
                params.push(ExtractedFunctionParameter {
                    name,
                    data_type: data_type.to_uppercase(),
                });
            }
        }
    }

    params
}

/// Extract the return type from a function definition
fn extract_function_return_type(sql: &str) -> Option<String> {
    let sql_upper = sql.to_uppercase();

    // Find RETURNS keyword
    let returns_pos = sql_upper.find("RETURNS")?;
    let after_returns = &sql[returns_pos + 7..]; // Skip "RETURNS"
    let after_returns_upper = after_returns.trim_start().to_uppercase();

    // Handle TABLE return type (inline table-valued function)
    if after_returns_upper.starts_with("TABLE") {
        return Some("TABLE".to_string());
    }

    // Handle @var TABLE return type (multi-statement table-valued function)
    if after_returns_upper.starts_with("@") {
        return Some("TABLE".to_string());
    }

    // For scalar functions, extract the type before AS
    // Pattern: RETURNS INT AS, RETURNS DECIMAL(18, 2) AS
    let re = regex::Regex::new(r"(?i)RETURNS\s+([A-Za-z0-9_\(\),\s]+?)\s+(?:AS|WITH)").ok()?;

    let caps = re.captures(sql)?;
    let return_type = caps.get(1)?.as_str().trim().to_uppercase();

    Some(return_type)
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

/// Extract full table structure from CREATE TABLE statement
fn extract_table_structure(sql: &str) -> Option<FallbackStatementType> {
    let (schema, name) = extract_generic_object_name(sql, "TABLE")?;

    // Check for graph table syntax (AS NODE or AS EDGE)
    let sql_upper = sql.to_uppercase();
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
    let mut columns = Vec::new();
    let mut constraints = Vec::new();

    // Split by top-level commas (not inside parentheses)
    let parts = split_by_top_level_comma(body);

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
fn split_by_top_level_comma(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
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

/// Parse a column definition
fn parse_column_definition(col_def: &str) -> Option<ExtractedTableColumn> {
    // Column pattern: [Name] TYPE [IDENTITY] [CONSTRAINT name DEFAULT (value)] [NOT NULL|NULL]
    // OR computed column: [Name] AS (expression) [PERSISTED] [NOT NULL]
    // The order can vary, so we need to be flexible

    // First check if this is a computed column: [Name] AS (expression)
    let computed_re = Regex::new(r"(?i)^\[?(\w+)\]?\s+AS\s+\(").ok()?;
    if let Some(caps) = computed_re.captures(col_def) {
        let name = caps.get(1)?.as_str().to_string();

        // Find the start of the expression (position of AS followed by '(')
        let as_match = Regex::new(r"(?i)\bAS\s*\(").ok()?.find(col_def)?;
        let expr_start = as_match.end() - 1; // Position of '('

        // Extract the full balanced expression using existing helper
        // extract_balanced_parens returns content without outer parens, so we add them back
        let inner_expr = extract_balanced_parens(&col_def[expr_start..])?;
        let expression = format!("({})", inner_expr);

        // Check for PERSISTED keyword
        let is_persisted = col_def.to_uppercase().contains("PERSISTED");

        // Check for NOT NULL (computed columns can specify nullability)
        let upper = col_def.to_uppercase();
        let is_nullable = !upper.contains("NOT NULL");

        return Some(ExtractedTableColumn {
            name,
            data_type: String::new(), // Computed columns have no explicit data type
            is_nullable,
            is_identity: false,
            is_rowguidcol: false,
            is_sparse: false,
            is_filestream: false,
            default_constraint_name: None,
            default_value: None,
            check_constraint_name: None,
            check_expression: None,
            computed_expression: Some(expression),
            is_persisted,
        });
    }

    // Not a computed column - parse as regular column
    // First extract the column name and type
    let col_re = Regex::new(r"(?i)^\[?(\w+)\]?\s+(\w+(?:\s*\([^)]+\))?)").ok()?;
    let caps = col_re.captures(col_def)?;

    let name = caps.get(1)?.as_str().to_string();
    let data_type = caps.get(2)?.as_str().trim().to_uppercase();

    // Check for IDENTITY
    let is_identity = col_def.to_uppercase().contains("IDENTITY");

    // Check for ROWGUIDCOL
    let is_rowguidcol = col_def.to_uppercase().contains("ROWGUIDCOL");

    // Check for SPARSE
    let is_sparse = col_def.to_uppercase().contains("SPARSE");

    // Check for FILESTREAM
    let is_filestream = col_def.to_uppercase().contains("FILESTREAM");

    // Check for nullability - look for NOT NULL or NULL
    // NOT NULL takes precedence, default is nullable
    let upper = col_def.to_uppercase();
    let is_nullable = if upper.contains("NOT NULL") {
        false
    } else {
        true // Default to nullable
    };

    // Extract inline DEFAULT constraint
    // Pattern: CONSTRAINT [name] [NOT NULL|NULL] DEFAULT (value) or just DEFAULT (value)
    // Note: The CONSTRAINT name can appear before or with NOT NULL/NULL interleaved
    let default_constraint_name;
    let default_value;

    // Try named constraint with parenthesized value: CONSTRAINT [name] [NOT NULL|NULL] DEFAULT ((value))
    let named_default_paren_re = Regex::new(
        r"(?i)CONSTRAINT\s+\[?(\w+)\]?\s+(?:NOT\s+NULL\s+|NULL\s+)?DEFAULT\s+(\([^)]*(?:\([^)]*\)[^)]*)*\))"
    ).ok()?;

    // Try named constraint with function call: CONSTRAINT [name] [NOT NULL|NULL] DEFAULT GETDATE()
    let named_default_func_re =
        Regex::new(r"(?i)CONSTRAINT\s+\[?(\w+)\]?\s+(?:NOT\s+NULL\s+|NULL\s+)?DEFAULT\s+(\w+\(\))")
            .ok()?;

    if let Some(caps) = named_default_paren_re.captures(col_def) {
        default_constraint_name = caps.get(1).map(|m| m.as_str().to_string());
        default_value = caps.get(2).map(|m| m.as_str().to_string());
    } else if let Some(caps) = named_default_func_re.captures(col_def) {
        default_constraint_name = caps.get(1).map(|m| m.as_str().to_string());
        default_value = caps.get(2).map(|m| m.as_str().to_string());
    } else {
        // Try unnamed default with parenthesized value: DEFAULT ((value))
        let unnamed_default_paren_re =
            Regex::new(r"(?i)DEFAULT\s+(\([^)]*(?:\([^)]*\)[^)]*)*\))").ok()?;

        // Try unnamed default with function call: DEFAULT GETDATE()
        let unnamed_default_func_re = Regex::new(r"(?i)DEFAULT\s+(\w+\(\))").ok()?;

        // Try unnamed default with string literal: DEFAULT 'value'
        let unnamed_default_string_re = Regex::new(r"(?i)DEFAULT\s+('(?:[^']|'')*')").ok()?;

        // Try unnamed default with bare number: DEFAULT 0.00 or DEFAULT 1
        // Match numbers that are NOT followed by CHECK to avoid matching constraint names
        let unnamed_default_number_re = Regex::new(
            r"(?i)DEFAULT\s+(-?\d+(?:\.\d+)?)\s*(?:CHECK|UNIQUE|PRIMARY|FOREIGN|CONSTRAINT|,|$|\))",
        )
        .ok()?;

        if let Some(caps) = unnamed_default_paren_re.captures(col_def) {
            default_constraint_name = None;
            default_value = caps.get(1).map(|m| m.as_str().to_string());
        } else if let Some(caps) = unnamed_default_func_re.captures(col_def) {
            default_constraint_name = None;
            default_value = caps.get(1).map(|m| m.as_str().to_string());
        } else if let Some(caps) = unnamed_default_string_re.captures(col_def) {
            default_constraint_name = None;
            default_value = caps.get(1).map(|m| m.as_str().to_string());
        } else if let Some(caps) = unnamed_default_number_re.captures(col_def) {
            default_constraint_name = None;
            default_value = caps.get(1).map(|m| m.as_str().to_string());
        } else {
            default_constraint_name = None;
            default_value = None;
        }
    }

    // Extract inline CHECK constraint
    // Pattern: [CONSTRAINT name] CHECK (expression)
    let check_constraint_name;
    let check_expression;

    // Try named check constraint: CONSTRAINT [name] CHECK (expression)
    let named_check_re = Regex::new(r"(?i)CONSTRAINT\s+\[?(\w+)\]?\s+CHECK\s*\((.+)\)").ok();

    if let Some(caps) = named_check_re.and_then(|re| re.captures(col_def)) {
        check_constraint_name = caps.get(1).map(|m| m.as_str().to_string());
        check_expression = caps.get(2).map(|m| m.as_str().to_string());
    } else {
        // Try unnamed check constraint: CHECK (expression)
        let unnamed_check_re = Regex::new(r"(?i)\bCHECK\s*\((.+)\)").ok();

        if let Some(caps) = unnamed_check_re.and_then(|re| re.captures(col_def)) {
            check_constraint_name = None;
            check_expression = caps.get(1).map(|m| m.as_str().to_string());
        } else {
            check_constraint_name = None;
            check_expression = None;
        }
    }

    Some(ExtractedTableColumn {
        name,
        data_type,
        is_nullable,
        is_identity,
        is_rowguidcol,
        is_sparse,
        is_filestream,
        default_constraint_name,
        default_value,
        check_constraint_name,
        check_expression,
        computed_expression: None, // Regular column, not computed
        is_persisted: false,
    })
}

/// Parse a table-level constraint
fn parse_table_constraint(
    constraint_def: &str,
    table_name: &str,
) -> Option<ExtractedTableConstraint> {
    let upper = constraint_def.to_uppercase();

    // Extract constraint name if present
    let name_re = Regex::new(r"(?i)CONSTRAINT\s+\[?(\w+)\]?").ok()?;
    let constraint_name = name_re
        .captures(constraint_def)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string());

    if upper.contains("PRIMARY KEY") {
        let is_clustered = !upper.contains("NONCLUSTERED");
        let columns = extract_constraint_columns(constraint_def);
        let name = constraint_name.unwrap_or_else(|| format!("PK_{}", table_name));

        Some(ExtractedTableConstraint::PrimaryKey {
            name,
            columns,
            is_clustered,
        })
    } else if upper.contains("FOREIGN KEY") {
        let columns = extract_fk_columns(constraint_def);
        let (referenced_table, referenced_columns) = extract_fk_references(constraint_def)?;
        let name = constraint_name.unwrap_or_else(|| format!("FK_{}", table_name));

        Some(ExtractedTableConstraint::ForeignKey {
            name,
            columns,
            referenced_table,
            referenced_columns,
        })
    } else if upper.contains("UNIQUE") {
        let is_clustered = upper.contains("CLUSTERED") && !upper.contains("NONCLUSTERED");
        let columns = extract_constraint_columns(constraint_def);
        let name = constraint_name.unwrap_or_else(|| format!("UQ_{}", table_name));

        Some(ExtractedTableConstraint::Unique {
            name,
            columns,
            is_clustered,
        })
    } else if upper.contains("CHECK") {
        // Extract CHECK expression
        let check_re = Regex::new(r"(?i)CHECK\s*\((.+)\)\s*$").ok()?;
        let expression = check_re
            .captures(constraint_def)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())?;
        let name = constraint_name.unwrap_or_else(|| format!("CK_{}", table_name));

        Some(ExtractedTableConstraint::Check { name, expression })
    } else {
        None
    }
}

/// Extract columns from PRIMARY KEY or UNIQUE constraint with ASC/DESC info
fn extract_constraint_columns(constraint_def: &str) -> Vec<ExtractedConstraintColumn> {
    // Find the column list in parentheses after PRIMARY KEY or UNIQUE
    // Pattern: PRIMARY KEY [CLUSTERED|NONCLUSTERED] ([Col1] [ASC|DESC], [Col2] [ASC|DESC])
    let pk_re =
        Regex::new(r"(?i)(?:PRIMARY\s+KEY|UNIQUE)\s*(?:CLUSTERED|NONCLUSTERED)?\s*\(([^)]+)\)")
            .unwrap();

    pk_re
        .captures(constraint_def)
        .and_then(|caps| caps.get(1))
        .map(|m| {
            let cols_str = m.as_str();
            cols_str
                .split(',')
                .filter_map(|col| {
                    let col = col.trim();
                    if col.is_empty() {
                        return None;
                    }

                    // Parse column name and optional ASC/DESC
                    let col_upper = col.to_uppercase();
                    let descending = col_upper.contains("DESC");

                    // Extract just the column name (remove brackets and ASC/DESC)
                    let name_re = Regex::new(r"(?i)\[?(\w+)\]?").unwrap();
                    name_re.captures(col).and_then(|caps| caps.get(1)).map(|m| {
                        ExtractedConstraintColumn {
                            name: m.as_str().to_string(),
                            descending,
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Extract column names from FOREIGN KEY constraint
fn extract_fk_columns(constraint_def: &str) -> Vec<String> {
    // Pattern: FOREIGN KEY ([Col1], [Col2])
    let fk_re = Regex::new(r"(?i)FOREIGN\s+KEY\s*\(([^)]+)\)").unwrap();

    fk_re
        .captures(constraint_def)
        .and_then(|caps| caps.get(1))
        .map(|m| parse_column_list(m.as_str()))
        .unwrap_or_default()
}

/// Extract REFERENCES table and columns from FOREIGN KEY constraint
fn extract_fk_references(constraint_def: &str) -> Option<(String, Vec<String>)> {
    // Pattern: REFERENCES [schema].[table] ([Col1], [Col2]) or REFERENCES [table] ([Col1])
    let ref_re = Regex::new(r"(?i)REFERENCES\s+(\[?\w+\]?(?:\.\[?\w+\]?)?)\s*\(([^)]+)\)").ok()?;

    let caps = ref_re.captures(constraint_def)?;
    let raw_table = caps.get(1)?.as_str();
    let columns = caps
        .get(2)
        .map(|m| parse_column_list(m.as_str()))
        .unwrap_or_default();

    // Normalize the table reference to [schema].[table] format
    let table = normalize_table_reference(raw_table);

    Some((table, columns))
}

/// Normalize a table reference to [schema].[table] format
/// Handles: Table, [Table], dbo.Table, [dbo].[Table], etc.
fn normalize_table_reference(raw: &str) -> String {
    // Check if it already has a schema (contains a dot)
    if raw.contains('.') {
        // Split by dot and normalize each part
        let parts: Vec<&str> = raw.split('.').collect();
        if parts.len() == 2 {
            let schema = parts[0].trim_matches(|c| c == '[' || c == ']');
            let table = parts[1].trim_matches(|c| c == '[' || c == ']');
            return format!("[{}].[{}]", schema, table);
        }
    }

    // No schema - assume dbo
    let table = raw.trim_matches(|c| c == '[' || c == ']');
    format!("[dbo].[{}]", table)
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
fn preprocess_tsql(sql: &str) -> PreprocessResult {
    let mut result = sql.to_string();
    let mut extracted_defaults = Vec::new();

    // 1. Replace VARBINARY(MAX) and BINARY(MAX) with sentinel value
    // sqlparser expects a numeric literal, not MAX for these types
    let binary_max_re = Regex::new(r"(?i)\b(VARBINARY|BINARY)\s*\(\s*MAX\s*\)").unwrap();
    result = binary_max_re
        .replace_all(&result, |caps: &regex::Captures| {
            format!("{}({})", &caps[1], BINARY_MAX_SENTINEL)
        })
        .to_string();

    // 2. Extract and remove CONSTRAINT [name] DEFAULT (value) FOR [column] patterns
    // This T-SQL syntax for named default constraints isn't supported by sqlparser
    // Pattern: CONSTRAINT [name] DEFAULT (expression) FOR [column]
    // The expression can contain nested parentheses, so we need careful matching
    let default_for_re = Regex::new(
        r"(?i),?\s*CONSTRAINT\s+\[?(\w+)\]?\s+DEFAULT\s+(\([^)]*(?:\([^)]*\)[^)]*)*\))\s+FOR\s+\[?(\w+)\]?"
    ).unwrap();

    // Extract all matches first
    for caps in default_for_re.captures_iter(sql) {
        extracted_defaults.push(ExtractedDefaultConstraint {
            name: caps[1].to_string(),
            column: caps[3].to_string(),
            expression: caps[2].to_string(),
        });
    }

    // Remove the DEFAULT FOR constraints from the SQL
    result = default_for_re.replace_all(&result, "").to_string();

    // Clean up any trailing commas before closing parenthesis that might result
    // This handles commas followed by whitespace, comments (-- or /* */), and newlines
    let trailing_comma_re = Regex::new(r",(\s*(--[^\n]*\n)?)*\s*\)").unwrap();
    result = trailing_comma_re.replace_all(&result, ")").to_string();

    PreprocessResult {
        sql: result,
        extracted_defaults,
    }
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

/// Extract full-text index information from CREATE FULLTEXT INDEX statement
/// Syntax: CREATE FULLTEXT INDEX ON [schema].[table] ([col1] LANGUAGE 1033, [col2] LANGUAGE 1033)
///         KEY INDEX [pk_name] ON [catalog] WITH CHANGE_TRACKING AUTO;
fn extract_fulltext_index_info(sql: &str) -> Option<FallbackStatementType> {
    // Match: CREATE FULLTEXT INDEX ON [schema].[table]
    let table_re =
        Regex::new(r"(?i)CREATE\s+FULLTEXT\s+INDEX\s+ON\s+(?:\[?(\w+)\]?\.)?\[?(\w+)\]?").ok()?;

    let caps = table_re.captures(sql)?;
    let table_schema = caps
        .get(1)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dbo".to_string());
    let table_name = caps.get(2)?.as_str().to_string();

    // Extract the column list with optional LANGUAGE specifiers
    // Pattern: ([col1] LANGUAGE 1033, [col2] LANGUAGE 1033, ...)
    let columns = extract_fulltext_columns(sql);

    // Extract KEY INDEX name
    // Pattern: KEY INDEX [pk_name]
    let key_index_re = Regex::new(r"(?i)KEY\s+INDEX\s+\[?(\w+)\]?").ok()?;
    let key_index = key_index_re
        .captures(sql)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())?;

    // Extract optional catalog name
    // Pattern: ON [catalog] (after KEY INDEX, not the table ON)
    let catalog = extract_fulltext_catalog_name(sql);

    // Extract optional change tracking mode
    // Pattern: WITH CHANGE_TRACKING AUTO|MANUAL|OFF
    let change_tracking_re = Regex::new(r"(?i)CHANGE_TRACKING\s+(\w+)").ok()?;
    let change_tracking = change_tracking_re
        .captures(sql)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_uppercase());

    Some(FallbackStatementType::FullTextIndex {
        table_schema,
        table_name,
        columns,
        key_index,
        catalog,
        change_tracking,
    })
}

/// Extract columns with optional LANGUAGE specifiers from full-text index definition
fn extract_fulltext_columns(sql: &str) -> Vec<ExtractedFullTextColumn> {
    let mut columns = Vec::new();

    // Find the column list after ON [schema].[table] (
    // We need to find the first ( after CREATE FULLTEXT INDEX ON [table]
    let sql_upper = sql.to_uppercase();
    let on_pos = match sql_upper.find("CREATE FULLTEXT INDEX ON") {
        Some(pos) => pos,
        None => return columns,
    };

    let remaining = &sql[on_pos..];
    let paren_start = match remaining.find('(') {
        Some(idx) => on_pos + idx + 1,
        None => return columns,
    };

    // Find the matching closing paren
    let mut depth = 1;
    let mut paren_end = paren_start;
    for (i, c) in sql[paren_start..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    paren_end = paren_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    if paren_end <= paren_start {
        return columns;
    }

    let columns_str = &sql[paren_start..paren_end];

    // Compile regex outside the loop to avoid recompiling on each iteration
    let col_re = Regex::new(r"(?i)\[?(\w+)\]?(?:\s+LANGUAGE\s+(\d+))?").unwrap();

    // Split by comma and parse each column
    for col_part in columns_str.split(',') {
        let col_part = col_part.trim();
        if col_part.is_empty() {
            continue;
        }

        // Pattern: [ColumnName] LANGUAGE 1033 or just [ColumnName]
        if let Some(caps) = col_re.captures(col_part) {
            let name = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let language_id = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());

            if !name.is_empty() {
                columns.push(ExtractedFullTextColumn { name, language_id });
            }
        }
    }

    columns
}

/// Extract the catalog name from the full-text index definition
/// Pattern: KEY INDEX [pk_name] ON [catalog]
fn extract_fulltext_catalog_name(sql: &str) -> Option<String> {
    // The catalog appears after KEY INDEX [name] ON [catalog]
    // Be careful not to match the table ON
    let re = Regex::new(r"(?i)KEY\s+INDEX\s+\[?\w+\]?\s+ON\s+\[?(\w+)\]?").ok()?;
    re.captures(sql)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extract full-text catalog information from CREATE FULLTEXT CATALOG statement
/// Syntax: CREATE FULLTEXT CATALOG [name] AS DEFAULT;
fn extract_fulltext_catalog_info(sql: &str) -> Option<FallbackStatementType> {
    // Match: CREATE FULLTEXT CATALOG [name]
    let re = Regex::new(r"(?i)CREATE\s+FULLTEXT\s+CATALOG\s+\[?(\w+)\]?").ok()?;

    let caps = re.captures(sql)?;
    let name = caps.get(1)?.as_str().to_string();

    // Check if this is set AS DEFAULT
    let sql_upper = sql.to_uppercase();
    let is_default = sql_upper.contains("AS DEFAULT");

    Some(FallbackStatementType::FullTextCatalog { name, is_default })
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
        let dialect = MsSqlDialect {};
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
        let dialect = MsSqlDialect {};
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
        let dialect = MsSqlDialect {};
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
}
