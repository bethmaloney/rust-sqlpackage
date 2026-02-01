//! Generate model.xml for dacpac

mod header;
mod table_writer;
mod view_writer;
mod xml_helpers;

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use regex::Regex;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer};
use std::io::Write;
use std::sync::LazyLock;

use crate::model::{
    ConstraintColumn, ConstraintElement, ConstraintType, DataCompressionType, DatabaseModel,
    ExtendedPropertyElement, FullTextCatalogElement, FullTextIndexElement, FunctionElement,
    IndexElement, ModelElement, ProcedureElement, RawElement, ScalarTypeElement, SchemaElement,
    SequenceElement, SortDirection, TableTypeColumnElement, TableTypeConstraint, TriggerElement,
    UserDefinedTypeElement,
};
use crate::parser::identifier_utils::format_word;
use crate::parser::{extract_function_parameters_tokens, extract_procedure_parameters_tokens};
use crate::project::SqlProject;

// Re-export XML helper functions for use within this module
use xml_helpers::{
    is_builtin_schema, write_property, write_relationship, write_schema_relationship,
    write_script_property, write_type_specifier_builtin,
};

// Re-export header functions for use within this module
use header::{write_database_options, write_header};

// Re-export table writer functions for use within this module
use table_writer::{
    write_column_type_specifier, write_table, write_table_type_column_with_annotation,
    write_table_type_relationship,
};

// Re-export view writer functions for use within this module
use view_writer::{
    extract_view_columns_and_deps, write_raw_view, write_view, write_view_columns, ViewColumn,
};

#[cfg(test)]
use table_writer::parse_qualified_table_name;

const NAMESPACE: &str = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02";

// =============================================================================
// Cached Regex Patterns
// =============================================================================
// These static patterns are compiled once and reused across all function calls,
// providing significant performance improvement over repeated Regex::new() calls.

// Note: QUALIFIED_TABLE_NAME_RE has been removed and replaced with token-based parsing in Phase 20.4.3.
// Qualified table name parsing now uses parse_qualified_name_tokenized() via parse_qualified_table_name().

/// Multi-statement TVF detection: RETURNS @var TABLE (
static MULTI_STMT_TVF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)RETURNS\s+@\w+\s+TABLE\s*\(").unwrap());

// Note: TVF_COL_TYPE_RE has been removed and replaced with token-based parsing in Phase 20.3.2.
// TVF column type extraction now uses parse_tvf_column_type_tokenized() with sqlparser-rs tokenizer.

// Note: TABLE_ALIAS_RE has been removed and replaced with token-based parsing in Phase 20.4.1.
// Table alias extraction now uses TableAliasTokenParser::extract_aliases_with_table_names().

// Note: ON_KEYWORD_RE has been removed and replaced with token-based parsing in Phase 20.5.4.
// ON clause boundary detection now uses extract_on_clause_boundaries_tokenized().

// Note: ON_TERMINATOR_RE has been removed and replaced with token-based parsing in Phase 20.5.4.
// ON clause termination detection is now handled by extract_on_clause_boundaries_tokenized().

// Note: COL_REF_RE has been removed and replaced with token-based parsing in Phase 20.2.2.
// Column reference extraction now uses extract_column_refs_tokenized() with BodyDependencyTokenScanner.

// Note: BARE_COL_RE has been removed and replaced with token-based parsing in Phase 20.2.2.
// Single bracketed column detection now uses BodyDepToken::SingleBracketed in extract_all_column_references().

// Note: GROUP_BY_RE has been removed and replaced with token-based parsing in Phase 20.5.5.
// GROUP BY clause detection now uses extract_group_by_clause_boundaries_tokenized().

// Note: GROUP_TERMINATOR_RE has been removed and replaced with token-based parsing in Phase 20.5.6.
// GROUP BY clause termination detection is now handled by extract_group_by_clause_boundaries_tokenized().

// Note: PROC_PARAM_RE has been removed and replaced with token-based parsing in Phase 20.1.3.
// Procedure parameter extraction now uses extract_procedure_parameters_tokens() from procedure_parser.rs.

// Note: FUNC_PARAM_RE has been removed and replaced with token-based parsing in Phase 20.1.2.
// Function parameter extraction now uses extract_function_parameters_tokens() from function_parser.rs.

// Note: DECLARE_TYPE_RE has been removed and replaced with token-based parsing in Phase 20.3.1.
// DECLARE type extraction now uses extract_declare_types_tokenized().

// Note: BRACKETED_TABLE_RE has been removed and replaced with token-based parsing in Phase 20.4.3.
// Bracketed table reference extraction now uses extract_table_refs_tokenized() with BodyDependencyTokenScanner.

// Note: UNBRACKETED_TABLE_RE has been removed and replaced with token-based parsing in Phase 20.4.3.
// Unbracketed table reference extraction now uses extract_table_refs_tokenized() with BodyDependencyTokenScanner.

// Note: TOKEN_RE has been replaced with BodyDependencyTokenScanner in Phase 20.2.1
// The token-based scanner handles whitespace (tabs, multiple spaces, newlines) correctly.

// Note: BRACKETED_IDENT_RE has been replaced with extract_bracketed_identifiers_tokenized() in Phase 20.2.4
// The token-based function handles whitespace, comments, and multi-part references correctly.

// Note: CAST_EXPR_RE has been replaced with extract_cast_expressions_tokenized() in Phase 20.3.3
// The token-based function handles whitespace, nested parentheses, and comments correctly.

// Note: AS_KEYWORD_RE has been replaced with find_function_body_as_tokenized() in Phase 20.5.1
// The token-based function properly identifies AS as a SQL keyword token, handling
// whitespace (tabs, spaces, newlines) and avoiding false matches within identifiers.

// TRIGGER_ALIAS_RE removed - replaced by TableAliasTokenParser::extract_aliases_with_table_names() (Phase 20.4.2)

// SINGLE_BRACKET_RE removed - replaced by extract_single_bracketed_identifiers() (Phase 20.2.6)
// ALIAS_COL_RE removed - replaced by extract_alias_column_refs_tokenized() (Phase 20.2.5)

// INSERT_SELECT_RE removed - replaced by InsertSelectTokenParser (Phase 20.4.6)
// INSERT_SELECT_JOIN_RE removed - replaced by InsertSelectTokenParser (Phase 20.4.6)
// UPDATE_ALIAS_RE removed - replaced by UpdateTokenParser (Phase 20.4.7)

pub fn generate_model_xml<W: Write>(
    writer: W,
    model: &DatabaseModel,
    project: &SqlProject,
) -> anyhow::Result<()> {
    let mut xml_writer = Writer::new_with_indent(writer, b' ', 2);

    // XML declaration
    xml_writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    // Root element - pre-compute collation_lcid before batching attributes (Phase 16.3.3 optimization)
    let collation_lcid = project.collation_lcid.to_string();
    let root = BytesStart::new("DataSchemaModel").with_attributes([
        ("FileFormatVersion", model.file_format_version.as_str()),
        ("SchemaVersion", model.schema_version.as_str()),
        ("DspName", project.target_platform.dsp_name()),
        ("CollationLcid", collation_lcid.as_str()),
        ("CollationCaseSensitive", "False"),
        ("xmlns", NAMESPACE),
    ]);
    xml_writer.write_event(Event::Start(root))?;

    // Header element with CustomData entries
    write_header(&mut xml_writer, project)?;

    // Model element
    xml_writer.write_event(Event::Start(BytesStart::new("Model")))?;

    // Write elements in DotNet sort order: (Name, Type) where empty Name sorts first.
    // SqlDatabaseOptions has sort key ("", "sqldatabaseoptions") and must be interleaved
    // at the correct position among the other elements.
    // Comparison is case-insensitive to match DotNet's sorting behavior.
    //
    // Use static string slices for db_options_sort_key to avoid allocation.
    // SqlDatabaseOptions has empty Name and Type "sqldatabaseoptions" (lowercase for comparison).
    let db_options_sort_key: (&str, &str) = ("", "sqldatabaseoptions");
    let mut db_options_written = false;

    for element in &model.elements {
        // Check if SqlDatabaseOptions should be written before this element
        if !db_options_written {
            // Compute sort key only when needed (before db_options is written)
            let elem_name = element.xml_name_attr().to_lowercase();
            let elem_type = element.type_name().to_lowercase();
            if db_options_sort_key <= (elem_name.as_str(), elem_type.as_str()) {
                write_database_options(&mut xml_writer, project)?;
                db_options_written = true;
            }
        }
        write_element(&mut xml_writer, element, model)?;
    }

    // Write SqlDatabaseOptions at the end if not yet written (happens when all elements
    // have empty Name and Type < "SqlDatabaseOptions", which is rare)
    if !db_options_written {
        write_database_options(&mut xml_writer, project)?;
    }

    // Close Model
    xml_writer.write_event(Event::End(BytesEnd::new("Model")))?;

    // Close root
    xml_writer.write_event(Event::End(BytesEnd::new("DataSchemaModel")))?;

    Ok(())
}

fn write_element<W: Write>(
    writer: &mut Writer<W>,
    element: &ModelElement,
    model: &DatabaseModel,
) -> anyhow::Result<()> {
    match element {
        ModelElement::Schema(s) => write_schema(writer, s),
        ModelElement::Table(t) => write_table(writer, t),
        ModelElement::View(v) => write_view(writer, v, model),
        ModelElement::Procedure(p) => write_procedure(writer, p, model),
        ModelElement::Function(f) => write_function(writer, f, model),
        ModelElement::Index(i) => write_index(writer, i),
        ModelElement::FullTextIndex(f) => write_fulltext_index(writer, f),
        ModelElement::FullTextCatalog(c) => write_fulltext_catalog(writer, c),
        ModelElement::Constraint(c) => write_constraint(writer, c),
        ModelElement::Sequence(s) => write_sequence(writer, s),
        ModelElement::UserDefinedType(u) => write_user_defined_type(writer, u),
        ModelElement::ScalarType(s) => write_scalar_type(writer, s),
        ModelElement::ExtendedProperty(e) => write_extended_property(writer, e),
        ModelElement::Trigger(t) => write_trigger(writer, t),
        ModelElement::Raw(r) => write_raw(writer, r, model),
    }
}

fn write_schema<W: Write>(writer: &mut Writer<W>, schema: &SchemaElement) -> anyhow::Result<()> {
    // Skip built-in schemas - they exist by default in SQL Server and are referenced
    // with ExternalSource="BuiltIns" in relationships
    if is_builtin_schema(&schema.name) {
        return Ok(());
    }

    // Pre-compute schema name before attribute batching (Phase 16.3.3 optimization)
    let schema_name = format!("[{}]", schema.name);
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlSchema"), ("Name", schema_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Write Authorizer relationship - DotNet always emits this, defaulting to dbo
    let auth = schema.authorization.as_deref().unwrap_or("dbo");
    write_authorizer_relationship(writer, auth)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write an Authorizer relationship for schema authorization
fn write_authorizer_relationship<W: Write>(
    writer: &mut Writer<W>,
    owner: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Authorizer")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let owner_ref = format!("[{}]", owner);
    // Conditional attribute - use with_attributes with appropriate attributes
    let refs = if is_builtin_schema(owner) {
        BytesStart::new("References")
            .with_attributes([("ExternalSource", "BuiltIns"), ("Name", owner_ref.as_str())])
    } else {
        BytesStart::new("References").with_attributes([("Name", owner_ref.as_str())])
    };
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Extract columns from an inline table-valued function's RETURN statement
/// The body contains "RETURN (SELECT [cols] FROM ...)" or "RETURN SELECT [cols] FROM ..."
/// func_full_name is needed to construct parameter references like [dbo].[FuncName].[@Param]
fn extract_inline_tvf_columns(
    body: &str,
    func_full_name: &str,
    default_schema: &str,
    model: &DatabaseModel,
) -> Vec<ViewColumn> {
    // Extract the SELECT statement from RETURN clause
    // Pattern: RETURN followed by optional whitespace, optional parenthesis, then SELECT
    let body_upper = body.to_uppercase();

    // Find RETURN keyword
    if let Some(return_pos) = body_upper.find("RETURN") {
        let after_return = &body[return_pos + 6..]; // Skip "RETURN"

        // Skip whitespace and optional opening parenthesis
        let trimmed = after_return.trim_start();
        let query_start = trimmed.strip_prefix('(').unwrap_or(trimmed);

        // Now we should have the SELECT statement
        // Use the existing extract_view_columns_and_deps logic
        // TVFs don't have SCHEMABINDING affecting GROUP BY, use false
        let (mut columns, _deps) =
            extract_view_columns_and_deps(query_start, default_schema, model, false);

        // For inline TVFs, handle parameter references in the SELECT list
        // When column expression is a parameter reference like @CustomerId,
        // the source_ref should be [schema].[FuncName].[@ParamName]
        let select_columns = extract_select_columns(query_start);
        for (idx, col_expr) in select_columns.iter().enumerate() {
            if idx < columns.len() {
                let trimmed_expr = col_expr.trim();
                // Check if the expression (before AS) is a parameter reference
                // Use token-based parsing to handle any whitespace around AS (tabs, multiple spaces, etc.)
                let expr_part = extract_expression_before_as(trimmed_expr);

                // If it's a parameter reference like @ParamName
                if expr_part.starts_with('@') && !expr_part.contains('(') {
                    let param_name = expr_part.trim_matches(|c| c == '[' || c == ']');
                    // DotNet format: [schema].[FuncName].[@ParamName] (brackets around the @param)
                    columns[idx].source_ref = Some(format!("{}.[{}]", func_full_name, param_name));
                }
            }
        }

        return columns;
    }

    Vec::new()
}

/// Column definition for multi-statement TVF (RETURNS @Table TABLE (...))
#[derive(Debug)]
struct TvfColumn {
    name: String,
    data_type: String,
    length: Option<u32>,
    precision: Option<u8>,
    scale: Option<u8>,
}

/// Extract columns from a multi-statement TVF's RETURNS @TableVar TABLE (...) clause
fn extract_multi_statement_tvf_columns(definition: &str) -> Vec<TvfColumn> {
    // Find the RETURNS @var TABLE (...) clause
    // We need to handle nested parentheses in column types like NVARCHAR(100)
    // First find "RETURNS @name TABLE ("
    let def_upper = definition.to_uppercase();

    if let Some(table_match) = MULTI_STMT_TVF_RE.find(&def_upper) {
        let start_pos = table_match.end();
        // Now find the matching closing paren, accounting for nested parens
        if let Some(cols_str) = extract_balanced_parens(definition, start_pos) {
            // Split by comma, respecting parentheses for types like NVARCHAR(100)
            let col_defs = split_column_definitions(&cols_str);

            // Pre-allocate based on column count
            let mut columns = Vec::with_capacity(col_defs.len());
            for col_def in col_defs {
                if let Some(col) = parse_tvf_column_definition(&col_def) {
                    columns.push(col);
                }
            }
            return columns;
        }
    }

    Vec::new()
}

/// Extract content inside balanced parentheses, handling nested parens
fn extract_balanced_parens(input: &str, start: usize) -> Option<String> {
    let bytes = input.as_bytes();
    let mut depth = 1; // We start after the opening paren
    let mut end = start;

    while end < bytes.len() && depth > 0 {
        match bytes[end] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth > 0 {
            end += 1;
        }
    }

    if depth == 0 {
        Some(input[start..end].to_string())
    } else {
        None
    }
}

/// Split column definitions by comma, respecting parentheses
fn split_column_definitions(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;

    for c in input.chars() {
        match c {
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth -= 1;
                current.push(c);
            }
            ',' if paren_depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }

    result
}

/// Parse a single column definition like "Id INT" or "Name NVARCHAR(100)"
///
/// Uses token-based parsing via `parse_tvf_column_type_tokenized()` to handle
/// type specifications with optional length/precision/scale parameters.
fn parse_tvf_column_definition(def: &str) -> Option<TvfColumn> {
    let def = def.trim();
    if def.is_empty() {
        return None;
    }

    // Pattern: [col_name] type[(length/precision,scale)] [optional modifiers]
    // Examples: Id INT, Name NVARCHAR(100), Price DECIMAL(18,2)
    let parts: Vec<&str> = def.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let name = parts[0].trim_matches(|c| c == '[' || c == ']').to_string();
    if parts.len() < 2 {
        return None;
    }

    // Join remaining parts and parse type with optional length/precision/scale
    let type_part = parts[1..].join(" ");

    // Use token-based parsing instead of regex (Phase 20.3.2)
    if let Some(type_info) = parse_tvf_column_type_tokenized(&type_part) {
        // Determine if first_num is length or precision based on type
        let (length, precision, scale) = if is_precision_scale_type(&type_info.data_type) {
            (
                None,
                type_info.first_num.map(|n| n as u8),
                type_info.second_num,
            )
        } else {
            (type_info.first_num, None, None)
        };

        Some(TvfColumn {
            name,
            data_type: type_info.data_type,
            length,
            precision,
            scale,
        })
    } else {
        None
    }
}

/// Check if a data type uses precision/scale (like DECIMAL, NUMERIC) vs length (like VARCHAR)
fn is_precision_scale_type(data_type: &str) -> bool {
    matches!(
        data_type.to_lowercase().as_str(),
        "decimal" | "numeric" | "float" | "real" | "money" | "smallmoney"
    )
}

/// Extract table aliases from FROM and JOIN clauses
/// Returns a map of alias -> full table reference (e.g., "p" -> "[dbo].[Products]")
/// Uses token-based parsing for robust handling of whitespace, comments, and edge cases.
fn extract_table_aliases(query: &str, default_schema: &str) -> Vec<(String, String)> {
    // Use token-based parser for robust extraction
    let mut parser = match TableAliasTokenParser::with_default_schema(query, default_schema) {
        Some(p) => p,
        None => return Vec::new(),
    };

    parser.extract_aliases_with_table_names()
}
/// Extract SELECT column expressions from the query
fn extract_select_columns(query: &str) -> Vec<String> {
    let mut columns = Vec::new();

    // Find the SELECT keyword
    let upper = query.to_uppercase();
    let select_pos = upper.find("SELECT");
    let from_pos = upper.find("FROM");

    if let Some(start) = select_pos {
        // Determine where the SELECT column list ends
        // If there's a FROM clause, columns are between SELECT and FROM
        // If there's no FROM clause (e.g., SELECT 1 AS Id), columns run to end or semicolon
        let end = if let Some(from_end) = from_pos {
            from_end
        } else {
            // No FROM clause - find the end of the SELECT (semicolon or end of query)
            upper.find(';').unwrap_or(query.len())
        };

        let select_section = &query[start + 6..end].trim();

        // Split by comma, but handle nested parentheses
        let mut current = String::new();
        let mut paren_depth = 0;

        for ch in select_section.chars() {
            match ch {
                '(' => {
                    paren_depth += 1;
                    current.push(ch);
                }
                ')' => {
                    paren_depth -= 1;
                    current.push(ch);
                }
                ',' if paren_depth == 0 => {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        columns.push(trimmed);
                    }
                    current = String::new();
                }
                _ => current.push(ch),
            }
        }

        // Add the last column
        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() {
            columns.push(trimmed);
        }
    }

    columns
}

/// Parse a column expression and return (output_name, source_reference)
/// Uses token-based parsing to correctly handle AS aliases with any whitespace (tabs, spaces, etc.)
fn parse_column_expression(
    expr: &str,
    table_aliases: &[(String, String)],
    default_schema: &str,
) -> (String, Option<String>) {
    let trimmed = expr.trim();

    // Tokenize the expression using sqlparser
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, trimmed).tokenize() {
        Ok(t) => t,
        Err(_) => {
            // Fallback: if tokenization fails, use simple extraction
            let output_name = extract_column_name_from_expr_simple(trimmed);
            let source_ref = resolve_column_reference(trimmed, table_aliases, default_schema);
            return (output_name, source_ref);
        }
    };

    // Find the last AS keyword at top level (not inside parentheses)
    // We iterate forward and keep updating, so we end up with the last match
    let mut as_position: Option<usize> = None;
    let mut paren_depth: i32 = 0;

    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::Word(w) if w.keyword == Keyword::AS && paren_depth == 0 => {
                as_position = Some(i);
            }
            _ => {}
        }
    }

    // Extract alias and column expression based on AS position
    let (col_expr, alias) = if let Some(as_idx) = as_position {
        // Extract alias: tokens after AS
        let alias = extract_alias_from_tokens(&tokens[as_idx + 1..]);

        // Reconstruct column expression: tokens before AS
        let col_expr = reconstruct_tokens(&tokens[..as_idx]);

        (col_expr, alias)
    } else {
        // No AS keyword found
        (trimmed.to_string(), None)
    };

    // Determine the output column name
    let output_name = alias.unwrap_or_else(|| {
        // Extract the column name from the expression
        extract_column_name_from_expr_simple(&col_expr)
    });

    // Determine the source reference (for simple column references)
    let source_ref = resolve_column_reference(&col_expr, table_aliases, default_schema);

    (output_name, source_ref)
}

/// Extract alias name from tokens after AS keyword
fn extract_alias_from_tokens(tokens: &[Token]) -> Option<String> {
    // Skip whitespace and find the first meaningful token
    for token in tokens {
        match token {
            Token::Whitespace(_) => continue,
            Token::Word(w) => {
                // Return the word value (unquoted)
                return Some(w.value.clone());
            }
            Token::SingleQuotedString(s) => {
                // Handle 'alias' style (SQL Server allows this)
                return Some(s.clone());
            }
            _ => break,
        }
    }
    None
}

/// Reconstruct SQL text from tokens
fn reconstruct_tokens(tokens: &[Token]) -> String {
    let mut result = String::new();
    for token in tokens {
        result.push_str(&token_to_sql(token));
    }
    result.trim().to_string()
}

/// Convert a token back to its SQL representation
fn token_to_sql(token: &Token) -> String {
    // Handle Word tokens using centralized format_word to preserve bracket quoting
    if let Token::Word(w) = token {
        return format_word(w);
    }
    // For everything else, use the Display impl
    token.to_string()
}

/// Check if an expression starts with a specific SQL keyword using tokenizer
fn starts_with_keyword(expr: &str, keyword: Keyword) -> bool {
    let dialect = MsSqlDialect {};
    if let Ok(tokens) = Tokenizer::new(&dialect, expr).tokenize() {
        for token in tokens {
            match token {
                Token::Whitespace(_) => continue,
                Token::Word(w) if w.keyword == keyword => return true,
                _ => return false,
            }
        }
    }
    false
}

/// Extract the expression part before the AS keyword (if present)
/// Uses token-based parsing to handle any whitespace (tabs, multiple spaces, newlines)
/// Returns the expression before AS, or the original expression if no AS found
fn extract_expression_before_as(expr: &str) -> String {
    let trimmed = expr.trim();

    // Tokenize the expression using sqlparser
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, trimmed).tokenize() {
        Ok(t) => t,
        Err(_) => {
            // Fallback: if tokenization fails, return trimmed expression
            return trimmed.to_string();
        }
    };

    // Find the last AS keyword at top level (not inside parentheses)
    let mut as_position: Option<usize> = None;
    let mut paren_depth: i32 = 0;

    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::Word(w) if w.keyword == Keyword::AS && paren_depth == 0 => {
                as_position = Some(i);
            }
            _ => {}
        }
    }

    // Return expression before AS, or original if no AS found
    if let Some(as_idx) = as_position {
        reconstruct_tokens(&tokens[..as_idx])
    } else {
        trimmed.to_string()
    }
}

/// Extract the column name from a simple expression like "[Id]", "t.[Name]", "COUNT(*)"
/// This is a fallback for when we don't have an AS alias.
/// Uses token-based parsing for proper handling of qualified references.
fn extract_column_name_from_expr_simple(expr: &str) -> String {
    let trimmed = expr.trim();

    // If it's a function call (contains parentheses), return the expression as-is
    if trimmed.contains('(') {
        return trimmed.to_string();
    }

    // Use tokenized parsing to handle qualified references like "t.[Name]" or "[dbo].[Products].[Name]"
    if let Some(qn) = parse_qualified_name_tokenized(trimmed) {
        return qn.last_part().to_string();
    }

    // Fallback: if tokenization fails, just strip brackets
    trimmed.trim_matches(|c| c == '[' || c == ']').to_string()
}

/// Extract column references from a SQL clause using token-based scanning.
/// Replaces COL_REF_RE regex with proper tokenization for whitespace/comment handling.
/// Returns raw column reference strings (e.g., "alias.column", "[schema].[table].[column]")
/// that can be passed to resolve_column_reference.
fn extract_column_refs_tokenized(sql: &str) -> Vec<String> {
    let mut refs = Vec::new();

    if let Some(mut scanner) = BodyDependencyTokenScanner::new(sql) {
        for token in scanner.scan() {
            // Only process tokens that represent column references (dotted identifiers)
            // Skip single identifiers and parameters as they're handled separately
            let ref_str = match token {
                // Three-part: [schema].[table].[column]
                BodyDepToken::ThreePartBracketed {
                    schema,
                    table,
                    column,
                } => Some(format!("[{}].[{}].[{}]", schema, table, column)),

                // Two-part bracketed: [alias].[column] or [schema].[table]
                BodyDepToken::TwoPartBracketed { first, second } => {
                    Some(format!("[{}].[{}]", first, second))
                }

                // alias.[column] - unbracketed alias with bracketed column
                BodyDepToken::AliasDotBracketedColumn { alias, column } => {
                    Some(format!("{}.[{}]", alias, column))
                }

                // [alias].column - bracketed alias with unbracketed column
                BodyDepToken::BracketedAliasDotColumn { alias, column } => {
                    Some(format!("[{}].{}", alias, column))
                }

                // schema.table - unbracketed two-part
                BodyDepToken::TwoPartUnbracketed { first, second } => {
                    Some(format!("{}.{}", first, second))
                }

                // Single identifiers and parameters are not column references
                // (they're handled elsewhere or need alias resolution separately)
                BodyDepToken::SingleBracketed(_)
                | BodyDepToken::SingleUnbracketed(_)
                | BodyDepToken::Parameter(_) => None,
            };

            if let Some(r) = ref_str {
                refs.push(r);
            }
        }
    }

    refs
}

/// Extract alias.[column] patterns from a SQL clause using token-based scanning.
/// Replaces ALIAS_COL_RE regex with proper tokenization for whitespace/comment handling.
/// Returns Vec of (alias, column) tuples in order of appearance.
///
/// This function specifically handles the `alias.[column]` pattern where:
/// - The alias is an unbracketed identifier (e.g., `i`, `d`, `t1`)
/// - The column is a bracketed identifier (e.g., `[Id]`, `[Name]`)
///
/// Used in trigger body dependency extraction to find column references like:
/// - `i.[Id]` (from inserted.Id)
/// - `d.[Name]` (from deleted.Name)
fn extract_alias_column_refs_tokenized(sql: &str) -> Vec<(String, String)> {
    let mut refs = Vec::new();

    if let Some(mut scanner) = BodyDependencyTokenScanner::new(sql) {
        for token in scanner.scan() {
            // Only extract AliasDotBracketedColumn patterns (alias.[column])
            if let BodyDepToken::AliasDotBracketedColumn { alias, column } = token {
                refs.push((alias, column));
            }
        }
    }

    refs
}

/// Extract single bracketed identifiers from SQL text using tokenization.
///
/// This function scans SQL and returns all `[identifier]` patterns that are not
/// part of multi-part names (e.g., standalone `[Col1]` but not `[schema].[table]`).
///
/// Used for extracting column names from INSERT column lists like `([Col1], [Col2], [Col3])`.
///
/// # Arguments
/// * `sql` - SQL text to scan (e.g., column list or SELECT clause)
///
/// # Returns
/// A vector of identifier names (without brackets) in order of appearance.
fn extract_single_bracketed_identifiers(sql: &str) -> Vec<String> {
    let mut results = Vec::new();

    if let Some(mut scanner) = BodyDependencyTokenScanner::new(sql) {
        for token in scanner.scan() {
            // Only extract SingleBracketed patterns (standalone [ident])
            if let BodyDepToken::SingleBracketed(ident) = token {
                results.push(ident);
            }
        }
    }

    results
}

/// Extract DECLARE variable types from SQL text using tokenization.
///
/// This function scans SQL and extracts type names from DECLARE statements.
/// Pattern: `DECLARE @varname typename` or `DECLARE @varname typename(precision)`
///
/// Used in `extract_body_dependencies()` to find built-in type dependencies
/// from DECLARE statements in function/procedure bodies.
///
/// # Arguments
/// * `sql` - SQL text to scan (e.g., function or procedure body)
///
/// # Returns
/// A vector of type names (lowercase) in order of appearance.
/// Types include base names without precision/scale (e.g., "nvarchar" not "nvarchar(50)").
fn extract_declare_types_tokenized(sql: &str) -> Vec<String> {
    let mut results = Vec::new();

    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, sql).tokenize_with_location() else {
        return results;
    };

    let mut i = 0;
    while i < tokens.len() {
        // Skip whitespace
        while i < tokens.len() && matches!(&tokens[i].token, Token::Whitespace(_)) {
            i += 1;
        }
        if i >= tokens.len() {
            break;
        }

        // Look for DECLARE keyword
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("DECLARE") {
                i += 1;

                // Skip whitespace after DECLARE
                while i < tokens.len() && matches!(&tokens[i].token, Token::Whitespace(_)) {
                    i += 1;
                }
                if i >= tokens.len() {
                    break;
                }

                // Expect variable name (@name) - MsSqlDialect tokenizes as a single Word
                if let Token::Word(var_word) = &tokens[i].token {
                    if var_word.value.starts_with('@') {
                        i += 1;

                        // Skip whitespace after variable name
                        while i < tokens.len() && matches!(&tokens[i].token, Token::Whitespace(_)) {
                            i += 1;
                        }
                        if i >= tokens.len() {
                            break;
                        }

                        // Extract type name (next identifier)
                        if let Token::Word(type_word) = &tokens[i].token {
                            // Get the base type name (without any precision/scale)
                            let type_name = type_word.value.to_lowercase();
                            results.push(type_name);
                            i += 1;
                            continue;
                        }
                    }
                }
            }
        }

        i += 1;
    }

    results
}

/// Parsed TVF column type result from tokenized parsing.
///
/// Contains the extracted data type name and optional length/precision/scale values.
#[derive(Debug, PartialEq)]
struct TvfColumnTypeInfo {
    data_type: String,
    first_num: Option<u32>,
    second_num: Option<u8>,
}

/// Parse TVF column type definition using tokenization.
///
/// This function replaces TVF_COL_TYPE_RE regex pattern. It parses type strings like:
/// - `INT` -> TvfColumnTypeInfo { data_type: "int", first_num: None, second_num: None }
/// - `NVARCHAR(100)` -> TvfColumnTypeInfo { data_type: "nvarchar", first_num: Some(100), second_num: None }
/// - `DECIMAL(18, 2)` -> TvfColumnTypeInfo { data_type: "decimal", first_num: Some(18), second_num: Some(2) }
///
/// # Arguments
/// * `type_str` - Type specification string (e.g., "NVARCHAR(100)" or "[DECIMAL](18, 2)")
///
/// # Returns
/// `Some(TvfColumnTypeInfo)` with parsed type information, or `None` if parsing fails.
fn parse_tvf_column_type_tokenized(type_str: &str) -> Option<TvfColumnTypeInfo> {
    let type_str = type_str.trim();
    if type_str.is_empty() {
        return None;
    }

    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, type_str).tokenize() else {
        return None;
    };

    let mut i = 0;

    // Helper to skip whitespace tokens
    let skip_whitespace = |tokens: &[Token], mut idx: usize| -> usize {
        while idx < tokens.len() && matches!(&tokens[idx], Token::Whitespace(_)) {
            idx += 1;
        }
        idx
    };

    i = skip_whitespace(&tokens, i);
    if i >= tokens.len() {
        return None;
    }

    // Extract the type name from first token (Word token, possibly quoted with brackets)
    let data_type = match &tokens[i] {
        Token::Word(w) => w.value.to_lowercase(),
        _ => return None,
    };
    i += 1;

    // Check for optional parameters: (num) or (num, num)
    i = skip_whitespace(&tokens, i);

    // If no more tokens, return type without parameters
    if i >= tokens.len() {
        return Some(TvfColumnTypeInfo {
            data_type,
            first_num: None,
            second_num: None,
        });
    }

    // Look for opening parenthesis
    if !matches!(&tokens[i], Token::LParen) {
        return Some(TvfColumnTypeInfo {
            data_type,
            first_num: None,
            second_num: None,
        });
    }
    i += 1;

    // Skip whitespace inside parentheses
    i = skip_whitespace(&tokens, i);
    if i >= tokens.len() {
        return None;
    }

    // Check for MAX keyword (e.g., VARCHAR(MAX))
    if let Token::Word(w) = &tokens[i] {
        if w.value.eq_ignore_ascii_case("MAX") {
            return Some(TvfColumnTypeInfo {
                data_type,
                first_num: Some(u32::MAX), // Special marker for MAX
                second_num: None,
            });
        }
    }

    // First number (length or precision)
    let first_num = match &tokens[i] {
        Token::Number(n, _) => n.parse::<u32>().ok(),
        _ => return None,
    };
    i += 1;

    // Skip whitespace
    i = skip_whitespace(&tokens, i);
    if i >= tokens.len() {
        return None;
    }

    // Check for comma (second parameter) or closing paren
    if matches!(&tokens[i], Token::RParen) {
        return Some(TvfColumnTypeInfo {
            data_type,
            first_num,
            second_num: None,
        });
    }

    // Expect comma for second parameter
    if !matches!(&tokens[i], Token::Comma) {
        return Some(TvfColumnTypeInfo {
            data_type,
            first_num,
            second_num: None,
        });
    }
    i += 1;

    // Skip whitespace after comma
    i = skip_whitespace(&tokens, i);
    if i >= tokens.len() {
        return None;
    }

    // Second number (scale)
    let second_num = match &tokens[i] {
        Token::Number(n, _) => n.parse::<u8>().ok(),
        _ => None,
    };

    Some(TvfColumnTypeInfo {
        data_type,
        first_num,
        second_num,
    })
}

/// Result from tokenized CAST expression parsing.
///
/// Contains the extracted type name and byte positions for ordering column references.
#[derive(Debug, PartialEq)]
struct CastExprInfo {
    /// The data type being cast to, in lowercase (e.g., "nvarchar", "int")
    type_name: String,
    /// Byte position where the CAST keyword starts
    cast_start: usize,
    /// Byte position where the CAST expression ends (after closing paren or type)
    cast_end: usize,
    /// Byte position of the CAST keyword itself (for type reference ordering)
    cast_keyword_pos: usize,
}

/// Extract CAST expressions from SQL text using tokenization.
///
/// This function replaces CAST_EXPR_RE regex pattern. It scans for CAST expressions
/// and extracts the target type name along with positions for proper ordering.
///
/// Pattern matched: `CAST(expression AS type)`
///
/// # Arguments
/// * `sql` - SQL text containing expressions (e.g., CHECK constraint or computed column)
///
/// # Returns
/// A vector of `CastExprInfo` containing type names and positions.
fn extract_cast_expressions_tokenized(sql: &str) -> Vec<CastExprInfo> {
    let mut results = Vec::new();
    let sql_trimmed = sql.trim();
    if sql_trimmed.is_empty() {
        return results;
    }

    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, sql).tokenize_with_location() else {
        return results;
    };

    // Build line offset map for byte position calculation
    let line_offsets = compute_line_offsets(sql);

    let len = tokens.len();
    let mut i = 0;

    // Helper to skip whitespace tokens
    let skip_whitespace =
        |tokens: &[sqlparser::tokenizer::TokenWithSpan], mut idx: usize| -> usize {
            while idx < tokens.len() && matches!(&tokens[idx].token, Token::Whitespace(_)) {
                idx += 1;
            }
            idx
        };

    while i < len {
        // Look for CAST keyword (unquoted word)
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("CAST") {
                let cast_keyword_pos = location_to_byte_offset(
                    &line_offsets,
                    tokens[i].span.start.line,
                    tokens[i].span.start.column,
                );
                let cast_start = cast_keyword_pos;

                // Move past CAST keyword
                let mut j = i + 1;
                j = skip_whitespace(&tokens, j);

                // Expect opening parenthesis
                if j < len && matches!(&tokens[j].token, Token::LParen) {
                    j += 1;

                    // Track parenthesis nesting to find the AS keyword at the right level
                    let mut paren_depth = 1;
                    let mut as_pos = None;

                    while j < len && paren_depth > 0 {
                        match &tokens[j].token {
                            Token::LParen => paren_depth += 1,
                            Token::RParen => {
                                paren_depth -= 1;
                                if paren_depth == 0 {
                                    break;
                                }
                            }
                            Token::Word(w)
                                if w.quote_style.is_none()
                                    && w.value.eq_ignore_ascii_case("AS")
                                    && paren_depth == 1 =>
                            {
                                // Found AS at the outermost level of CAST
                                as_pos = Some(j);
                            }
                            _ => {}
                        }
                        j += 1;
                    }

                    // If we found AS, extract the type name after it
                    if let Some(as_idx) = as_pos {
                        let mut type_idx = as_idx + 1;
                        type_idx = skip_whitespace(&tokens, type_idx);

                        if type_idx < len {
                            // Extract type name (could be a Word token)
                            if let Token::Word(type_word) = &tokens[type_idx].token {
                                let type_name = type_word.value.to_lowercase();

                                // Calculate cast_end position
                                // Find the closing paren position
                                let cast_end = if j < len {
                                    let loc = &tokens[j].span.start;
                                    location_to_byte_offset(&line_offsets, loc.line, loc.column) + 1
                                } else {
                                    sql.len()
                                };

                                results.push(CastExprInfo {
                                    type_name,
                                    cast_start,
                                    cast_end,
                                    cast_keyword_pos,
                                });
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }

    results
}

/// Extract column aliases from SQL text using tokenization.
///
/// This function scans SQL and extracts identifiers that follow the AS keyword.
/// Pattern: `expr AS alias` or `expr AS [alias]`
///
/// Used in `extract_column_aliases_for_body_deps()` to find output column names
/// that should not be treated as column references.
///
/// # Arguments
/// * `sql` - SQL text to scan (e.g., SELECT clause with aliases)
///
/// # Returns
/// A vector of alias names (without brackets, lowercase) in order of appearance.
fn extract_column_aliases_tokenized(sql: &str) -> Vec<String> {
    let mut results = Vec::new();

    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, sql).tokenize_with_location() else {
        return results;
    };

    // SQL keywords that should not be treated as aliases
    let alias_keywords = [
        "ON", "WHERE", "INNER", "LEFT", "RIGHT", "OUTER", "CROSS", "JOIN", "GROUP", "ORDER",
        "HAVING", "UNION", "WITH", "AND", "OR", "NOT", "SET", "FROM", "SELECT", "INTO", "BEGIN",
        "END", "NULL", "INT", "VARCHAR", "NVARCHAR", "DATETIME", "BIT", "DECIMAL",
    ];

    let mut i = 0;
    while i < tokens.len() {
        // Skip whitespace
        while i < tokens.len() {
            if matches!(&tokens[i].token, Token::Whitespace(_)) {
                i += 1;
            } else {
                break;
            }
        }
        if i >= tokens.len() {
            break;
        }

        // Look for the AS keyword
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("AS") {
                i += 1;

                // Skip whitespace after AS
                while i < tokens.len() {
                    if matches!(&tokens[i].token, Token::Whitespace(_)) {
                        i += 1;
                    } else {
                        break;
                    }
                }

                // Extract the alias (next identifier, bracketed or unbracketed)
                if i < tokens.len() {
                    if let Token::Word(alias_word) = &tokens[i].token {
                        let alias_name = &alias_word.value;
                        let alias_upper = alias_name.to_uppercase();

                        // Skip if alias is a SQL keyword
                        if !alias_keywords.iter().any(|&k| k == alias_upper)
                            && !alias_name.is_empty()
                        {
                            results.push(alias_name.to_lowercase());
                        }
                        i += 1;
                        continue;
                    }
                }
            }
        }

        i += 1;
    }

    results
}

/// Resolve a column reference to its full [schema].[table].[column] form
/// Returns None for aggregate/function expressions or complex expressions (CASE, etc.)
/// Uses token-based parsing for proper handling of qualified names.
fn resolve_column_reference(
    expr: &str,
    table_aliases: &[(String, String)],
    _default_schema: &str,
) -> Option<String> {
    let trimmed = expr.trim();

    // If it's a function call (contains parentheses), no direct reference
    // This catches IIF(...), COALESCE(...), NULLIF(...), COUNT(*), etc.
    if trimmed.contains('(') {
        return None;
    }

    // Check for CASE expression using tokenizer (CASE doesn't use parens)
    if starts_with_keyword(trimmed, Keyword::CASE) {
        return None;
    }

    // Parse the column reference using tokenization
    let qn = parse_qualified_name_tokenized(trimmed)?;

    match qn.part_count() {
        1 => {
            // Just column name, try to resolve using first table alias
            let col_name = &qn.first;
            // Don't emit [*] column reference for SELECT * - matches DotNet behavior
            if col_name == "*" {
                return None;
            }
            if let Some((_, table_ref)) = table_aliases.first() {
                return Some(format!("{}.[{}]", table_ref, col_name));
            }
            None
        }
        2 => {
            // alias.column or schema.table
            let alias_or_schema = &qn.first;
            let col_or_table = qn.second.as_ref()?;

            // Don't emit [*] column reference for alias.* - matches DotNet behavior
            if col_or_table == "*" {
                return None;
            }

            // Try to find matching alias
            for (alias, table_ref) in table_aliases {
                if alias.eq_ignore_ascii_case(alias_or_schema) {
                    return Some(format!("{}.[{}]", table_ref, col_or_table));
                }
            }

            // If not found as alias, assume it's schema.table (unusual for column ref)
            None
        }
        3 => {
            // schema.table.column
            let schema = &qn.first;
            let table = qn.second.as_ref()?;
            let column = qn.third.as_ref()?;
            // Don't emit [*] column reference for schema.table.* - matches DotNet behavior
            if column == "*" {
                return None;
            }
            Some(format!("[{}].[{}].[{}]", schema, table, column))
        }
        _ => None,
    }
}

/// Extract column references from JOIN ON clauses
/// These need to come before SELECT columns in QueryDependencies to match DotNet ordering
///
/// Phase 20.5.4: Uses token-based ON clause boundary detection instead of ON_KEYWORD_RE regex.
fn extract_join_on_columns(
    query: &str,
    table_aliases: &[(String, String)],
    default_schema: &str,
) -> Vec<String> {
    let mut refs = Vec::new();

    // Phase 20.5.4: Use token-based ON clause boundary detection
    for (start, end) in extract_on_clause_boundaries_tokenized(query) {
        let clause_text = &query[start..end];

        // Phase 20.2.2: Use token-based extraction instead of COL_REF_RE regex
        for col_ref in extract_column_refs_tokenized(clause_text) {
            if let Some(resolved) =
                resolve_column_reference(&col_ref, table_aliases, default_schema)
            {
                if !refs.contains(&resolved) {
                    refs.push(resolved);
                }
            }
        }
    }

    refs
}

/// Extract column references from GROUP BY clause
///
/// Phase 20.5.5-20.5.6: Uses token-based boundary detection instead of
/// GROUP_BY_RE and GROUP_TERMINATOR_RE regex patterns.
fn extract_group_by_columns(
    query: &str,
    table_aliases: &[(String, String)],
    default_schema: &str,
) -> Vec<String> {
    let mut refs = Vec::new();

    // Phase 20.5.5-20.5.6: Use token-based GROUP BY clause boundary detection
    if let Some((start, end)) = extract_group_by_clause_boundaries_tokenized(query) {
        let clause_text = &query[start..end];

        // Phase 20.2.2: Use token-based extraction instead of COL_REF_RE regex
        for col_ref in extract_column_refs_tokenized(clause_text) {
            if let Some(resolved) =
                resolve_column_reference(&col_ref, table_aliases, default_schema)
            {
                // No dedup within GROUP BY - preserve order
                refs.push(resolved);
            }
        }
    }

    refs
}

/// Extract all column references from the entire query (SELECT, WHERE, ON, GROUP BY, etc.)
fn extract_all_column_references(
    query: &str,
    table_aliases: &[(String, String)],
    default_schema: &str,
) -> Vec<String> {
    let mut refs = Vec::new();

    // Phase 20.2.2: Use token-based extraction instead of COL_REF_RE and BARE_COL_RE regex
    // This handles both dotted references (alias.column) and single bracketed identifiers
    if let Some(mut scanner) = BodyDependencyTokenScanner::new(query) {
        for token in scanner.scan() {
            let col_ref = match token {
                // Three-part: [schema].[table].[column]
                BodyDepToken::ThreePartBracketed {
                    schema,
                    table,
                    column,
                } => Some(format!("[{}].[{}].[{}]", schema, table, column)),

                // Two-part bracketed: [alias].[column] or [schema].[table]
                BodyDepToken::TwoPartBracketed { first, second } => {
                    Some(format!("[{}].[{}]", first, second))
                }

                // alias.[column] - unbracketed alias with bracketed column
                BodyDepToken::AliasDotBracketedColumn { alias, column } => {
                    Some(format!("{}.[{}]", alias, column))
                }

                // [alias].column - bracketed alias with unbracketed column
                BodyDepToken::BracketedAliasDotColumn { alias, column } => {
                    Some(format!("[{}].{}", alias, column))
                }

                // schema.table - unbracketed two-part
                BodyDepToken::TwoPartUnbracketed { first, second } => {
                    Some(format!("{}.{}", first, second))
                }

                // Single bracketed identifier (e.g., [IsActive] in WHERE clause)
                // This replaces BARE_COL_RE functionality
                // Skip if this identifier is a known table alias (to avoid treating alias
                // definitions like [ITTAG] in "JOIN [dbo].[Tag] [ITTAG]" as column refs)
                BodyDepToken::SingleBracketed(ident) => {
                    // Check if this is a table alias - if so, skip it
                    let is_alias = table_aliases
                        .iter()
                        .any(|(alias, _)| alias.eq_ignore_ascii_case(&ident));
                    if is_alias {
                        None
                    } else {
                        Some(ident)
                    }
                }

                // Skip parameters and single unbracketed identifiers
                BodyDepToken::SingleUnbracketed(_) | BodyDepToken::Parameter(_) => None,
            };

            if let Some(ref_str) = col_ref {
                // Try to resolve to full column reference
                if let Some(resolved) =
                    resolve_column_reference(&ref_str, table_aliases, default_schema)
                {
                    if !refs.contains(&resolved) {
                        refs.push(resolved);
                    }
                }
            }
        }
    }

    refs
}

/// Write Columns relationship for multi-statement TVFs
/// Uses SqlSimpleColumn with TypeSpecifier (different from views which use SqlComputedColumn)
fn write_tvf_columns<W: Write>(
    writer: &mut Writer<W>,
    func_full_name: &str,
    columns: &[TvfColumn],
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", func_full_name, col.name);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlSimpleColumn"),
            ("Name", col_full_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        // Write TypeSpecifier relationship
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let type_rel = BytesStart::new("Relationship").with_attributes([("Name", "TypeSpecifier")]);
        writer.write_event(Event::Start(type_rel))?;

        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let spec_elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
        writer.write_event(Event::Start(spec_elem))?;

        // Write Length or Precision/Scale properties if present
        if let Some(length) = col.length {
            write_property(writer, "Length", &length.to_string())?;
        }
        if let Some(precision) = col.precision {
            write_property(writer, "Precision", &precision.to_string())?;
        }
        if let Some(scale) = col.scale {
            write_property(writer, "Scale", &scale.to_string())?;
        }

        // Write Type reference to built-in type
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let inner_type_rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
        writer.write_event(Event::Start(inner_type_rel))?;

        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let type_ref = format!("[{}]", col.data_type);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let refs = BytesStart::new("References")
            .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref.as_str())]);
        writer.write_event(Event::Empty(refs))?;

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        writer.write_event(Event::End(BytesEnd::new("Relationship")))?; // Type

        writer.write_event(Event::End(BytesEnd::new("Element")))?; // SqlTypeSpecifier
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        writer.write_event(Event::End(BytesEnd::new("Relationship")))?; // TypeSpecifier

        writer.write_event(Event::End(BytesEnd::new("Element")))?; // SqlSimpleColumn
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

fn write_procedure<W: Write>(
    writer: &mut Writer<W>,
    proc: &ProcedureElement,
    model: &DatabaseModel,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", proc.schema, proc.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlProcedure"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Extract parameters for both writing and dependency extraction
    let params = extract_procedure_parameters(&proc.definition);

    // Find table type parameters (TVPs) - these have READONLY modifier or reference a table type
    let tvp_params: Vec<(&ProcedureParameter, Option<&UserDefinedTypeElement>)> = params
        .iter()
        .filter_map(|p| {
            // Check if parameter references a table type in the model
            let table_type = find_table_type_for_parameter(&p.data_type, model);
            if table_type.is_some() || p.is_readonly {
                Some((p, table_type))
            } else {
                None
            }
        })
        .collect();

    // Calculate disambiguator for TVP parameters (DotNet uses specific values)
    // The disambiguator is typically 2 for first TVP, 3 for second, etc.
    let tvp_disambiguator_base = 2u32;

    // Extract just the body part (after final AS)
    let body = extract_procedure_body_only(&proc.definition);

    // Write BodyScript property first
    write_script_property(writer, "BodyScript", &body)?;

    // Write IsAnsiNullsOn property (always true for procedures)
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Write IsNativelyCompiled property if true
    if proc.is_natively_compiled {
        write_property(writer, "IsNativelyCompiled", "True")?;
    }

    // Extract and write BodyDependencies
    // For procedures with TVPs, we need special handling for TVP column references
    // For all procedures, we still need regular body dependencies (table refs, param refs, etc.)
    let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    let body_deps = if tvp_params.is_empty() {
        // No TVPs - use regular body dependency extraction
        extract_body_dependencies(&body, &full_name, &param_names)
    } else {
        // Has TVPs - extract TVP-specific dependencies
        extract_body_dependencies_with_tvp(&body, &full_name, &param_names, &tvp_params)
    };
    write_body_dependencies(writer, &body_deps)?;

    // Write DynamicObjects relationship for TVP parameters
    if !tvp_params.is_empty() {
        write_dynamic_objects(writer, &full_name, &tvp_params)?;
    }

    // Write Parameters relationship
    if !params.is_empty() {
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let param_rel = BytesStart::new("Relationship").with_attributes([("Name", "Parameters")]);
        writer.write_event(Event::Start(param_rel))?;

        for param in params.iter() {
            writer.write_event(Event::Start(BytesStart::new("Entry")))?;

            // Parameter name must include @ prefix
            let param_name_with_at = if param.name.starts_with('@') {
                param.name.clone()
            } else {
                format!("@{}", param.name)
            };
            let param_name = format!("{}.[{}]", full_name, param_name_with_at);

            // Check if this is a TVP parameter
            let tvp_idx = tvp_params.iter().position(|(p, _)| std::ptr::eq(*p, param));
            let is_tvp = tvp_idx.is_some();
            let disambiguator = tvp_idx.map(|i| tvp_disambiguator_base + i as u32);

            // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
            // Conditional Disambiguator attribute requires separate handling
            let param_elem = if let Some(disamb) = disambiguator {
                let disamb_str = disamb.to_string();
                BytesStart::new("Element").with_attributes([
                    ("Type", "SqlSubroutineParameter"),
                    ("Name", param_name.as_str()),
                    ("Disambiguator", disamb_str.as_str()),
                ])
            } else {
                BytesStart::new("Element").with_attributes([
                    ("Type", "SqlSubroutineParameter"),
                    ("Name", param_name.as_str()),
                ])
            };
            writer.write_event(Event::Start(param_elem))?;

            // Write default value if present
            if let Some(ref default_val) = param.default_value {
                write_script_property(writer, "DefaultExpressionScript", default_val)?;
            }

            // IsOutput property if applicable
            if param.is_output {
                write_property(writer, "IsOutput", "True")?;
            }

            // IsReadOnly property for TVP parameters
            if param.is_readonly || is_tvp {
                write_property(writer, "IsReadOnly", "True")?;
            }

            // Data type relationship - different handling for TVPs vs built-in types
            if is_tvp {
                write_table_type_relationship(writer, &param.data_type)?;
            } else {
                write_data_type_relationship(writer, &param.data_type)?;
            }

            writer.write_event(Event::End(BytesEnd::new("Element")))?;
            writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    write_schema_relationship(writer, &proc.schema)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Find a table type element in the model matching the parameter data type
fn find_table_type_for_parameter<'a>(
    data_type: &str,
    model: &'a DatabaseModel,
) -> Option<&'a UserDefinedTypeElement> {
    // Normalize the data type for comparison
    // Handle both [dbo].[TypeName] and dbo.TypeName formats
    let normalized = normalize_type_name(data_type);

    for element in &model.elements {
        if let ModelElement::UserDefinedType(udt) = element {
            let type_full_name = format!("[{}].[{}]", udt.schema, udt.name);
            if type_full_name.eq_ignore_ascii_case(&normalized) {
                return Some(udt);
            }
        }
    }
    None
}

/// Normalize a type name to [schema].[name] format.
/// Uses token-based parsing for proper handling of various identifier formats.
pub(crate) fn normalize_type_name(type_name: &str) -> String {
    let trimmed = type_name.trim();

    // Already in [schema].[name] format
    if trimmed.starts_with('[') && trimmed.contains("].[") {
        return trimmed.to_string();
    }

    // Use tokenized parsing to handle qualified names
    if let Some(qn) = parse_qualified_name_tokenized(trimmed) {
        if let Some((schema, name)) = qn.schema_and_table() {
            return format!("[{}].[{}]", schema, name);
        }
    }

    // Return as-is if we can't normalize
    trimmed.to_string()
}

/// Write DynamicObjects relationship for table-valued parameters
fn write_dynamic_objects<W: Write>(
    writer: &mut Writer<W>,
    proc_full_name: &str,
    tvp_params: &[(&ProcedureParameter, Option<&UserDefinedTypeElement>)],
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "DynamicObjects")]);
    writer.write_event(Event::Start(rel))?;

    for (param, table_type_opt) in tvp_params {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let param_name_with_at = if param.name.starts_with('@') {
            param.name.clone()
        } else {
            format!("@{}", param.name)
        };
        let dynamic_source_name = format!("{}.[{}]", proc_full_name, param_name_with_at);

        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlDynamicColumnSource"),
            ("Name", dynamic_source_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        // Write Columns relationship if we have the table type definition
        if let Some(table_type) = table_type_opt {
            write_dynamic_columns(writer, &dynamic_source_name, table_type)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write Columns relationship for a SqlDynamicColumnSource
fn write_dynamic_columns<W: Write>(
    writer: &mut Writer<W>,
    dynamic_source_name: &str,
    table_type: &UserDefinedTypeElement,
) -> anyhow::Result<()> {
    if table_type.columns.is_empty() {
        return Ok(());
    }

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in &table_type.columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", dynamic_source_name, col.name);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let col_elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlSimpleColumn"),
            ("Name", col_full_name.as_str()),
        ]);
        writer.write_event(Event::Start(col_elem))?;

        // Write IsNullable property - in DynamicObjects columns, IsNullable is based on
        // the table type column definition
        let is_nullable = col.nullability.unwrap_or(true);
        if !is_nullable {
            write_property(writer, "IsNullable", "False")?;
        }

        // Write TypeSpecifier relationship
        write_column_type_specifier(writer, &col.data_type, col.precision, col.scale)?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Extract body dependencies including TVP column references
fn extract_body_dependencies_with_tvp(
    body: &str,
    full_name: &str,
    _params: &[String],
    tvp_params: &[(&ProcedureParameter, Option<&UserDefinedTypeElement>)],
) -> Vec<BodyDependency> {
    use std::collections::HashSet;
    let mut deps = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Build a map of TVP param names (with @ prefix for body matching) to their table type columns
    // Note: param.name is stored WITHOUT @ prefix (Phase 20.1.3)
    let tvp_columns: std::collections::HashMap<String, Vec<String>> = tvp_params
        .iter()
        .filter_map(|(param, tt_opt)| {
            tt_opt.map(|tt| {
                // Add @ prefix for body pattern matching
                let param_name = format!("@{}", param.name);
                let cols = tt.columns.iter().map(|c| c.name.clone()).collect();
                (param_name, cols)
            })
        })
        .collect();

    // First, add the TVP parameter reference with disambiguator
    // This reference appears first in BodyDependencies with the same disambiguator as in Parameters
    // Note: param.name is stored WITHOUT @ prefix (Phase 20.1.3)
    for (idx, (param, _)) in tvp_params.iter().enumerate() {
        let disambiguator = 2 + idx as u32;
        // Use param.name directly - it's already without @ prefix
        let param_ref = format!("{}.[@{}]", full_name, param.name);
        // Store with disambiguator info - we'll need to emit this specially
        if !seen.contains(&param_ref) {
            seen.insert(param_ref.clone());
            deps.push(BodyDependency::TvpParameter(param_ref, disambiguator));
        }
    }

    // Now scan the body for column references from TVP parameters
    // Pattern: FROM @ParamName or @ParamName.Column or just column names used with TVP
    for (tvp_param_name, columns) in &tvp_columns {
        // Look for column references in SELECT, WHERE, etc. that match TVP columns
        // Pattern: column names that appear after FROM @ParamName
        let param_pattern = format!(r"(?i)FROM\s+{}\b", regex::escape(tvp_param_name));
        if regex::Regex::new(&param_pattern).unwrap().is_match(body) {
            // This TVP is used as a table source - add column references
            for col_name in columns {
                // Check if this column is referenced in the body
                let col_pattern = format!(r"\b{}\b", regex::escape(col_name));
                if regex::Regex::new(&col_pattern).unwrap().is_match(body) {
                    let col_ref = format!(
                        "{}.[@{}].[{}]",
                        full_name,
                        tvp_param_name.trim_start_matches('@'),
                        col_name
                    );
                    if !seen.contains(&col_ref) {
                        seen.insert(col_ref.clone());
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    }
                }
            }
        }
    }

    deps
}

/// Represents an extracted procedure parameter
#[derive(Debug)]
struct ProcedureParameter {
    name: String,
    data_type: String,
    is_output: bool,
    /// Whether this is a READONLY table-valued parameter
    is_readonly: bool,
    #[allow(dead_code)] // Captured for potential future use
    default_value: Option<String>,
}

/// Extract parameters from a CREATE PROCEDURE definition using token-based parsing.
///
/// Phase 20.1.3: Replaced PROC_PARAM_RE regex with token-based parser.
/// Uses the same token-based infrastructure as function parameter parsing.
/// Parameter names are stored WITHOUT the @ prefix for consistency.
fn extract_procedure_parameters(definition: &str) -> Vec<ProcedureParameter> {
    // Use token-based parameter extraction
    let token_params = extract_procedure_parameters_tokens(definition);

    // Convert TokenParsedProcedureParameter to ProcedureParameter
    // Note: TokenParsedProcedureParameter.name does NOT include @ prefix
    token_params
        .into_iter()
        .map(|p| ProcedureParameter {
            name: p.name, // Already without @ prefix
            data_type: p.data_type,
            is_output: p.is_output,
            is_readonly: p.is_readonly,
            default_value: p.default_value,
        })
        .collect()
}

/// Represents an extracted function parameter with full details
#[derive(Debug)]
struct FunctionParameter {
    name: String,
    data_type: String,
    default_value: Option<String>,
}

/// Extract parameters from a CREATE FUNCTION definition using token-based parsing.
///
/// Phase 20.1.2: Replaced FUNC_PARAM_RE regex with token-based parser.
/// Uses the same token-based infrastructure as procedure parameter parsing.
fn extract_function_parameters(definition: &str) -> Vec<FunctionParameter> {
    // Use token-based parameter extraction
    let token_params = extract_function_parameters_tokens(definition);

    // Convert TokenParsedParameter to FunctionParameter
    token_params
        .into_iter()
        .map(|p| {
            // Strip @ prefix from parameter name (TokenParsedParameter includes it)
            let name = p.name.strip_prefix('@').unwrap_or(&p.name).to_string();
            FunctionParameter {
                name,
                data_type: p.data_type,
                default_value: p.default_value,
            }
        })
        .collect()
}

/// Write Parameters relationship for function parameters
fn write_function_parameters<W: Write>(
    writer: &mut Writer<W>,
    params: &[FunctionParameter],
    full_name: &str,
) -> anyhow::Result<()> {
    if params.is_empty() {
        return Ok(());
    }

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let param_rel = BytesStart::new("Relationship").with_attributes([("Name", "Parameters")]);
    writer.write_event(Event::Start(param_rel))?;

    for param in params.iter() {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Parameter name must include @ prefix
        let param_name_with_at = if param.name.starts_with('@') {
            param.name.clone()
        } else {
            format!("@{}", param.name)
        };
        let param_name = format!("{}.[{}]", full_name, param_name_with_at);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let param_elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlSubroutineParameter"),
            ("Name", param_name.as_str()),
        ]);
        writer.write_event(Event::Start(param_elem))?;

        // Write default value if present
        if let Some(ref default_val) = param.default_value {
            write_script_property(writer, "DefaultExpressionScript", default_val)?;
        }

        // Data type relationship
        write_data_type_relationship(writer, &param.data_type)?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Find the standalone AS keyword that separates procedure header from body
/// Note: Previously used by regex-based procedure parsing (pre-Phase 20.1.3).
/// Kept for tests and potential future use.
#[allow(dead_code)]
fn find_standalone_as(s: &str) -> Option<usize> {
    let upper = s.to_uppercase();
    let chars: Vec<char> = upper.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Look for AS preceded by whitespace/newline and followed by whitespace/newline
        if i + 2 <= chars.len() && chars[i] == 'A' && chars[i + 1] == 'S' {
            let prev_ok = i == 0 || chars[i - 1].is_whitespace();
            let next_ok = i + 2 >= chars.len() || chars[i + 2].is_whitespace();
            if prev_ok && next_ok {
                // Make sure this isn't part of a longer word
                let next_next_ok = i + 3 >= chars.len() || !chars[i + 2].is_alphanumeric();
                if next_next_ok {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

/// Clean up a data type string removing trailing keywords using tokenizer.
///
/// This function uses sqlparser-rs tokenization to handle any whitespace
/// (spaces, tabs, multiple spaces) before READONLY, NULL, or NOT NULL.
///
/// Phase 19.1: Replaced space-only trim_end_matches patterns with token-based parsing.
/// Note: Previously used by regex-based procedure parsing (pre-Phase 20.1.3).
/// Kept for tests and potential future use.
#[allow(dead_code)]
fn clean_data_type(dt: &str) -> String {
    let trimmed = dt.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Use tokenizer to find trailing keywords (READONLY, NULL, NOT NULL)
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, trimmed).tokenize() {
        Ok(t) => t,
        Err(_) => {
            // Fallback to original string if tokenization fails
            return trimmed.to_string();
        }
    };

    // Find the position where trailing keywords start by scanning from the end
    // We need to handle: READONLY, NULL, NOT NULL (in that order)
    let non_ws_tokens: Vec<(usize, &Token)> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| !matches!(t, Token::Whitespace(_)))
        .collect();

    if non_ws_tokens.is_empty() {
        return String::new();
    }

    // Calculate how many trailing tokens to remove
    let mut tokens_to_remove = 0;

    // Check for trailing READONLY
    if let Some((_, token)) = non_ws_tokens.last() {
        if matches!(
            token,
            Token::Word(w) if w.keyword == Keyword::NoKeyword && w.value.eq_ignore_ascii_case("READONLY")
        ) {
            tokens_to_remove = 1;
        }
    }

    // Check for trailing NULL (after potentially removing READONLY)
    let remaining_count = non_ws_tokens.len() - tokens_to_remove;
    if remaining_count > 0 {
        if let Some((_, token)) = non_ws_tokens.get(remaining_count - 1) {
            if matches!(token, Token::Word(w) if w.keyword == Keyword::NULL) {
                tokens_to_remove += 1;

                // Check for NOT NULL (NOT precedes NULL)
                let remaining_count = non_ws_tokens.len() - tokens_to_remove;
                if remaining_count > 0 {
                    if let Some((_, token)) = non_ws_tokens.get(remaining_count - 1) {
                        if matches!(token, Token::Word(w) if w.keyword == Keyword::NOT) {
                            tokens_to_remove += 1;
                        }
                    }
                }
            }
        }
    }

    // If no tokens to remove, return the original (uppercased for built-in types)
    if tokens_to_remove == 0 {
        return if trimmed.starts_with('[') || trimmed.contains(".[") {
            trimmed.to_string()
        } else {
            trimmed.to_uppercase()
        };
    }

    // Find the last token index to keep (the one just before the removed tokens)
    let last_keep_idx = non_ws_tokens.len() - tokens_to_remove - 1;
    let (token_idx, _) = non_ws_tokens[last_keep_idx];

    // Reconstruct the type up to the last kept token
    let mut result = String::with_capacity(trimmed.len());
    for (i, token) in tokens.iter().enumerate() {
        if i > token_idx {
            // Only include trailing whitespace before the removed keywords
            if matches!(token, Token::Whitespace(_)) {
                continue;
            }
            break;
        }
        match token {
            Token::Word(w) => {
                if w.quote_style == Some('[') {
                    result.push_str(&format!("[{}]", w.value));
                } else if w.quote_style == Some('"') {
                    result.push_str(&format!("\"{}\"", w.value));
                } else {
                    result.push_str(&w.value.to_uppercase());
                }
            }
            Token::Period => result.push('.'),
            Token::LParen => result.push('('),
            Token::RParen => result.push(')'),
            Token::Comma => result.push(','),
            Token::Number(n, _) => result.push_str(n),
            Token::Whitespace(ws) => result.push_str(&ws.to_string()),
            _ => {
                // For other tokens, use their debug representation
                result.push_str(&format!("{token}"));
            }
        }
    }

    result.trim().to_string()
}

/// Represents a dependency extracted from a procedure/function body
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum BodyDependency {
    /// Reference to a built-in type (e.g., [int], [decimal])
    BuiltInType(String),
    /// Reference to a table or other object (e.g., [dbo].[Products])
    ObjectRef(String),
    /// Reference to a TVP parameter with its disambiguator
    TvpParameter(String, u32),
}

/// Extract body dependencies from a procedure/function body
/// This extracts dependencies in order of appearance:
/// 1. Built-in types from DECLARE statements
/// 2. Table references, columns, and parameters in the order they appear
fn extract_body_dependencies(
    body: &str,
    full_name: &str,
    params: &[String],
) -> Vec<BodyDependency> {
    use std::collections::{HashMap, HashSet};
    // Estimate ~10 dependencies typical for a procedure/function body
    let mut deps = Vec::with_capacity(10);
    // Track seen items for deduplication:
    // - DotNet deduplicates built-in types
    // - DotNet deduplicates table references (2-part refs like [schema].[table])
    // - DotNet deduplicates parameter references
    // - DotNet deduplicates DIRECT column references (columns matched without alias resolution)
    // - DotNet does NOT deduplicate ALIAS-RESOLVED column references (alias.column patterns)
    let mut seen_types: HashSet<String> = HashSet::with_capacity(5);
    let mut seen_tables: HashSet<String> = HashSet::with_capacity(10);
    let mut seen_params: HashSet<String> = HashSet::with_capacity(5);
    let mut seen_direct_columns: HashSet<String> = HashSet::with_capacity(10);

    // Extract DECLARE type dependencies first (for scalar functions)
    // Uses token-based extraction (Phase 20.3.1) for proper whitespace handling
    for type_name in extract_declare_types_tokenized(body) {
        let type_ref = format!("[{}]", type_name);
        // Only deduplicate built-in types
        if !seen_types.contains(&type_ref) {
            seen_types.insert(type_ref.clone());
            deps.push(BodyDependency::BuiltInType(type_ref));
        }
    }

    // Strip SQL comments from body to prevent words in comments being treated as references
    let body_no_comments = strip_sql_comments_for_body_deps(body);
    let body = body_no_comments.as_str();

    // Phase 18: Extract table aliases for resolution
    // Maps alias (lowercase) -> table reference (e.g., "a" -> "[dbo].[Account]")
    let mut table_aliases: HashMap<String, String> = HashMap::new();
    // Track subquery/derived table aliases - these should be skipped, not resolved
    let mut subquery_aliases: HashSet<String> = HashSet::new();
    // Track column aliases (AS identifier) - these should not be treated as column references
    let mut column_aliases: HashSet<String> = HashSet::new();

    // Extract aliases from FROM/JOIN clauses with proper alias tracking
    extract_table_aliases_for_body_deps(body, &mut table_aliases, &mut subquery_aliases);

    // Extract column aliases (SELECT expr AS alias patterns)
    extract_column_aliases_for_body_deps(body, &mut column_aliases);

    // First pass: collect all table references using token-based extraction
    // Phase 20.4.3: Replaced BRACKETED_TABLE_RE and UNBRACKETED_TABLE_RE with tokenization
    // This handles whitespace (tabs, multiple spaces, newlines) correctly and is more robust
    let table_refs = extract_table_refs_tokenized(body, &table_aliases);

    // Scan body sequentially for all references in order of appearance using token-based scanner
    // Note: DotNet has a complex ordering that depends on SQL clause structure (FROM first, etc.)
    // We process in textual order which may differ from DotNet's order but contains the same refs
    // Phase 20.2.1: Replaced TOKEN_RE regex with BodyDependencyTokenScanner for robust whitespace handling

    if let Some(mut scanner) = BodyDependencyTokenScanner::new(body) {
        for token in scanner.scan() {
            match token {
                BodyDepToken::Parameter(param_name) => {
                    // Pattern 1: Parameter reference: @param
                    // Check if this is a declared parameter (not a local variable)
                    // Note: params contains parameter names WITHOUT @ prefix (Phase 20.1.3)
                    if params.iter().any(|p| p.eq_ignore_ascii_case(&param_name)) {
                        let param_ref = format!("{}.[@{}]", full_name, param_name);
                        // DotNet deduplicates parameter references
                        if !seen_params.contains(&param_ref) {
                            seen_params.insert(param_ref.clone());
                            deps.push(BodyDependency::ObjectRef(param_ref));
                        }
                    }
                }
                BodyDepToken::ThreePartBracketed {
                    schema,
                    table,
                    column,
                } => {
                    // Pattern 2: Three-part bracketed reference: [schema].[table].[column]
                    if !schema.starts_with('@') && !table.starts_with('@') {
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        let table_ref = format!("[{}].[{}]", schema, table);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }

                        // Direct three-part column refs ARE deduplicated by DotNet
                        let col_ref = format!("[{}].[{}].[{}]", schema, table, column);
                        if !seen_direct_columns.contains(&col_ref) {
                            seen_direct_columns.insert(col_ref.clone());
                            deps.push(BodyDependency::ObjectRef(col_ref));
                        }
                    }
                }
                BodyDepToken::TwoPartBracketed { first, second } => {
                    // Pattern 3: Two-part bracketed reference: [schema].[table] or [alias].[column]
                    if first.starts_with('@') || second.starts_with('@') {
                        continue;
                    }

                    let first_lower = first.to_lowercase();

                    // Check if first_part is a subquery/derived table alias - skip entirely
                    if subquery_aliases.contains(&first_lower) {
                        continue;
                    }

                    // Check if first_part is a table alias that should be resolved
                    if let Some(resolved_table) = table_aliases.get(&first_lower) {
                        // This is alias.column - resolve to [schema].[table].[column]
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        if !seen_tables.contains(resolved_table) {
                            seen_tables.insert(resolved_table.clone());
                            deps.push(BodyDependency::ObjectRef(resolved_table.clone()));
                        }

                        // Then emit the column reference (DotNet does NOT deduplicate columns)
                        let col_ref = format!("{}.[{}]", resolved_table, second);
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    } else {
                        // Not an alias - treat as [schema].[table] (DotNet deduplicates tables)
                        let table_ref = format!("[{}].[{}]", first, second);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }
                    }
                }
                BodyDepToken::AliasDotBracketedColumn { alias, column } => {
                    // Pattern 4: Unbracketed alias with bracketed column: alias.[column]
                    let alias_lower = alias.to_lowercase();

                    // Check if alias is a subquery/derived table alias - skip entirely
                    if subquery_aliases.contains(&alias_lower) {
                        continue;
                    }

                    // Check if alias is a table alias that should be resolved
                    if let Some(resolved_table) = table_aliases.get(&alias_lower) {
                        // This is alias.[column] - resolve to [schema].[table].[column]
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        if !seen_tables.contains(resolved_table) {
                            seen_tables.insert(resolved_table.clone());
                            deps.push(BodyDependency::ObjectRef(resolved_table.clone()));
                        }

                        // Then emit the column reference (DotNet does NOT deduplicate columns)
                        let col_ref = format!("{}.[{}]", resolved_table, column);
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    } else {
                        // Not a known alias - treat as [alias].[column] (might be schema.table)
                        let table_ref = format!("[{}].[{}]", alias, column);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }
                    }
                }
                BodyDepToken::BracketedAliasDotColumn { alias, column } => {
                    // Pattern 5: Bracketed alias with unbracketed column: [alias].column
                    let alias_lower = alias.to_lowercase();

                    // Check if alias is a subquery/derived table alias - skip entirely
                    if subquery_aliases.contains(&alias_lower) {
                        continue;
                    }

                    // Check if alias is a table alias that should be resolved
                    if let Some(resolved_table) = table_aliases.get(&alias_lower) {
                        // This is [alias].column - resolve to [schema].[table].[column]
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        if !seen_tables.contains(resolved_table) {
                            seen_tables.insert(resolved_table.clone());
                            deps.push(BodyDependency::ObjectRef(resolved_table.clone()));
                        }

                        // Then emit the column reference (DotNet does NOT deduplicate columns)
                        let col_ref = format!("{}.[{}]", resolved_table, column);
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    } else {
                        // Not a known alias - treat as [alias].[column] (might be schema.table)
                        let table_ref = format!("[{}].[{}]", alias, column);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }
                    }
                }
                BodyDepToken::SingleBracketed(ident) => {
                    // Pattern 6: Single bracketed identifier: [ident]
                    let ident_lower = ident.to_lowercase();
                    let upper_ident = ident.to_uppercase();

                    // Skip SQL keywords (but allow column names that happen to match type names)
                    if is_sql_keyword_not_column(&upper_ident) {
                        continue;
                    }

                    // Skip if this is a known table alias, subquery alias, or column alias
                    if table_aliases.contains_key(&ident_lower)
                        || subquery_aliases.contains(&ident_lower)
                        || column_aliases.contains(&ident_lower)
                    {
                        continue;
                    }

                    // Skip if this is part of a table reference (schema or table name)
                    let is_table_or_schema = table_refs.iter().any(|t| {
                        t.ends_with(&format!("].[{}]", ident))
                            || t.starts_with(&format!("[{}].", ident))
                    });

                    // If not a table/schema, treat as unqualified column -> resolve against first table
                    if !is_table_or_schema {
                        if let Some(first_table) = table_refs.first() {
                            // First emit the table reference if not seen (DotNet deduplicates tables)
                            if !seen_tables.contains(first_table) {
                                seen_tables.insert(first_table.clone());
                                deps.push(BodyDependency::ObjectRef(first_table.clone()));
                            }

                            // Direct column refs (single bracketed) ARE deduplicated by DotNet
                            let col_ref = format!("{}.[{}]", first_table, ident);
                            if !seen_direct_columns.contains(&col_ref) {
                                seen_direct_columns.insert(col_ref.clone());
                                deps.push(BodyDependency::ObjectRef(col_ref));
                            }
                        }
                    }
                }
                BodyDepToken::TwoPartUnbracketed { first, second } => {
                    // Pattern 7: Unbracketed two-part reference: schema.table or alias.column
                    let first_lower = first.to_lowercase();
                    let first_upper = first.to_uppercase();

                    // Skip if first part is a keyword
                    if is_sql_keyword(&first_upper) {
                        continue;
                    }

                    // Check if first_part is a subquery/derived table alias - skip entirely
                    if subquery_aliases.contains(&first_lower) {
                        continue;
                    }

                    // Check if first_part is a table alias that should be resolved
                    if let Some(resolved_table) = table_aliases.get(&first_lower) {
                        // This is alias.column - resolve to [schema].[table].[column]
                        // First emit the table reference if not seen (DotNet deduplicates tables)
                        if !seen_tables.contains(resolved_table) {
                            seen_tables.insert(resolved_table.clone());
                            deps.push(BodyDependency::ObjectRef(resolved_table.clone()));
                        }

                        // Then emit the column reference (DotNet does NOT deduplicate columns)
                        let col_ref = format!("{}.[{}]", resolved_table, second);
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    } else {
                        // Not an alias - treat as schema.table (DotNet deduplicates tables)
                        let table_ref = format!("[{}].[{}]", first, second);
                        if !seen_tables.contains(&table_ref) {
                            seen_tables.insert(table_ref.clone());
                            deps.push(BodyDependency::ObjectRef(table_ref));
                        }
                    }
                }
                BodyDepToken::SingleUnbracketed(ident) => {
                    // Pattern 8: Unbracketed single identifier: might be a column name
                    let ident_lower = ident.to_lowercase();
                    let upper_ident = ident.to_uppercase();

                    // Skip SQL keywords
                    if is_sql_keyword_not_column(&upper_ident) {
                        continue;
                    }

                    // Skip if this is a known table alias, subquery alias, or column alias
                    if table_aliases.contains_key(&ident_lower)
                        || subquery_aliases.contains(&ident_lower)
                        || column_aliases.contains(&ident_lower)
                    {
                        continue;
                    }

                    // Skip if this is part of a table reference (schema or table name)
                    let is_table_or_schema = table_refs.iter().any(|t| {
                        // Check case-insensitive match for unbracketed identifiers
                        let t_lower = t.to_lowercase();
                        t_lower.ends_with(&format!("].[{}]", ident_lower))
                            || t_lower.starts_with(&format!("[{}].", ident_lower))
                    });

                    // If not a table/schema, treat as unqualified column -> resolve against first table
                    if !is_table_or_schema {
                        if let Some(first_table) = table_refs.first() {
                            // First emit the table reference if not seen (DotNet deduplicates tables)
                            if !seen_tables.contains(first_table) {
                                seen_tables.insert(first_table.clone());
                                deps.push(BodyDependency::ObjectRef(first_table.clone()));
                            }

                            // Direct column refs (single unbracketed) ARE deduplicated by DotNet
                            let col_ref = format!("{}.[{}]", first_table, ident);
                            if !seen_direct_columns.contains(&col_ref) {
                                seen_direct_columns.insert(col_ref.clone());
                                deps.push(BodyDependency::ObjectRef(col_ref));
                            }
                        }
                    }
                }
            }
        }
    }

    deps
}

/// Extract table aliases from FROM/JOIN clauses for body dependency resolution.
/// Populates two maps:
/// - table_aliases: maps alias (lowercase) -> full table reference (e.g., "a" -> "[dbo].[Account]")
/// - subquery_aliases: set of aliases that refer to subqueries/derived tables (should be skipped)
///
/// Handles:
/// - FROM [schema].[table] alias
/// - FROM [schema].[table] AS alias
/// - JOIN [schema].[table] alias ON ...
/// - LEFT JOIN (...) AS SubqueryAlias ON ...
/// - CROSS APPLY (...) AS ApplyAlias
///
/// This implementation uses sqlparser-rs tokenizer instead of regex for more robust parsing.
fn extract_table_aliases_for_body_deps(
    body: &str,
    table_aliases: &mut std::collections::HashMap<String, String>,
    subquery_aliases: &mut std::collections::HashSet<String>,
) {
    let mut parser = match TableAliasTokenParser::new(body) {
        Some(p) => p,
        None => return,
    };
    parser.extract_all_aliases(table_aliases, subquery_aliases);
}

/// Token-based parser for extracting table aliases from SQL body text.
/// Replaces 6 regex patterns with a single tokenizer-based implementation.
struct TableAliasTokenParser {
    tokens: Vec<sqlparser::tokenizer::TokenWithSpan>,
    pos: usize,
    default_schema: String,
}

impl TableAliasTokenParser {
    /// Create a new parser for SQL body text
    fn new(sql: &str) -> Option<Self> {
        Self::with_default_schema(sql, "dbo")
    }

    /// Create a new parser with a custom default schema
    fn with_default_schema(sql: &str, default_schema: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;
        Some(Self {
            tokens,
            pos: 0,
            default_schema: default_schema.to_string(),
        })
    }

    /// Extract all aliases from the SQL body
    fn extract_all_aliases(
        &mut self,
        table_aliases: &mut std::collections::HashMap<String, String>,
        subquery_aliases: &mut std::collections::HashSet<String>,
    ) {
        // First pass: extract CTE aliases from WITH clauses
        self.extract_cte_aliases(subquery_aliases);

        // Reset position for second pass
        self.pos = 0;

        // Second pass: extract table aliases and subquery aliases from FROM/JOIN/APPLY
        // We scan the entire token stream without skipping nested parens for table aliases,
        // because table aliases inside subqueries are still valid and need to be captured.
        while !self.is_at_end() {
            self.skip_whitespace();

            // Look for FROM, JOIN variants, or APPLY keywords
            if self.check_keyword(Keyword::FROM) {
                self.advance();
                self.extract_table_reference_after_from_join(table_aliases, subquery_aliases);
            } else if self.is_join_keyword() {
                self.skip_join_keywords();
                self.extract_table_reference_after_from_join(table_aliases, subquery_aliases);
            } else if self.check_word_ci("CROSS") || self.check_word_ci("OUTER") {
                // Check for APPLY - just skip past the APPLY keyword and let the loop
                // continue to find FROM/JOIN inside the APPLY subquery
                // The subquery alias will be captured via the ) AS/alias pattern
                let saved_pos = self.pos;
                self.advance();
                self.skip_whitespace();
                if self.check_keyword(Keyword::APPLY) || self.check_word_ci("APPLY") {
                    self.advance();
                    // Don't extract alias here - let the loop continue to scan content
                    // The ) alias pattern will capture the APPLY alias
                } else {
                    // Not an APPLY, restore position and continue
                    self.pos = saved_pos;
                    self.advance();
                }
            } else if self.check_keyword(Keyword::MERGE) || self.check_word_ci("MERGE") {
                // Handle MERGE INTO [table] AS [alias] pattern
                // This extracts the TARGET alias which is the alias for the target table
                self.advance();
                self.skip_whitespace();

                // Skip optional INTO keyword
                if self.check_keyword(Keyword::INTO) || self.check_word_ci("INTO") {
                    self.advance();
                    self.skip_whitespace();
                }

                // Parse the target table name
                if let Some((schema, table_name)) = self.parse_table_name() {
                    self.skip_whitespace();

                    // Check for AS keyword (optional)
                    if self.check_keyword(Keyword::AS) {
                        self.advance();
                        self.skip_whitespace();
                    }

                    // Try to get alias
                    if let Some(alias) = self.try_parse_table_alias() {
                        let alias_lower = alias.to_lowercase();
                        if !Self::is_alias_keyword(&alias_lower)
                            && !table_aliases.contains_key(&alias_lower)
                        {
                            let table_ref = format!("[{}].[{}]", schema, table_name);
                            table_aliases.insert(alias_lower, table_ref);
                        }
                    }
                }
            } else if self.check_word_ci("USING") {
                // Handle USING ... pattern in MERGE statements
                // For USING (subquery) AS alias - the subquery alias will be captured by
                // the RParen handler when it sees ) AS alias pattern.
                // For USING table AS alias - we handle it here as a table alias.
                self.advance();
                self.skip_whitespace();

                // Check if followed by opening paren (subquery)
                if self.check_token(&Token::LParen) {
                    // Don't skip balanced parens - let the main loop continue scanning
                    // the subquery contents. FROM/JOIN inside will be captured.
                    // The ) AS alias pattern will be captured by the RParen handler.
                    self.advance();
                } else {
                    // USING references a table directly (less common but valid)
                    // Parse table name and optional alias
                    if let Some((schema, table_name)) = self.parse_table_name() {
                        self.skip_whitespace();

                        // Check for AS keyword (optional)
                        if self.check_keyword(Keyword::AS) {
                            self.advance();
                            self.skip_whitespace();
                        }

                        // Try to get alias
                        if let Some(alias) = self.try_parse_table_alias() {
                            let alias_lower = alias.to_lowercase();
                            if !Self::is_alias_keyword(&alias_lower)
                                && !table_aliases.contains_key(&alias_lower)
                            {
                                let table_ref = format!("[{}].[{}]", schema, table_name);
                                table_aliases.insert(alias_lower, table_ref);
                            }
                        }
                    }
                }
            } else if self.check_token(&Token::RParen) {
                // After closing paren, check for subquery alias pattern: ) AS alias or ) alias
                self.advance();
                self.skip_whitespace();

                // Check for AS keyword (optional)
                if self.check_keyword(Keyword::AS) {
                    self.advance();
                    self.skip_whitespace();
                }

                // Try to get an alias - but only if it's a valid identifier
                if let Some(alias) = self.try_parse_subquery_alias() {
                    let alias_lower = alias.to_lowercase();
                    if !Self::is_alias_keyword(&alias_lower) {
                        subquery_aliases.insert(alias_lower);
                    }
                }
            } else {
                self.advance();
            }
        }
    }

    /// Extract aliases with table names for view column resolution.
    /// Returns Vec of (alias/table_name, full_table_ref) pairs.
    /// Unlike `extract_all_aliases`, this also includes the table name itself as a lookup key.
    fn extract_aliases_with_table_names(&mut self) -> Vec<(String, String)> {
        let mut result = Vec::new();
        let mut seen_tables: std::collections::HashSet<String> = std::collections::HashSet::new();

        // First pass: extract CTE aliases into a set (to exclude them from table references)
        let mut cte_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        self.extract_cte_aliases(&mut cte_names);

        // Reset position for second pass
        self.pos = 0;

        // Second pass: extract table aliases and table names
        while !self.is_at_end() {
            self.skip_whitespace();

            // Look for FROM, JOIN variants, or APPLY keywords
            if self.check_keyword(Keyword::FROM) {
                self.advance();
                self.extract_table_with_alias(&mut result, &mut seen_tables, &cte_names);
            } else if self.is_join_keyword() {
                self.skip_join_keywords();
                self.extract_table_with_alias(&mut result, &mut seen_tables, &cte_names);
            } else if self.check_word_ci("CROSS") || self.check_word_ci("OUTER") {
                // Check for APPLY
                let saved_pos = self.pos;
                self.advance();
                self.skip_whitespace();
                if self.check_keyword(Keyword::APPLY) || self.check_word_ci("APPLY") {
                    self.advance();
                    // APPLY subquery - don't extract here, continue scanning
                } else {
                    self.pos = saved_pos;
                    self.advance();
                }
            } else {
                self.advance();
            }
        }

        result
    }

    /// Extract table reference and alias after FROM/JOIN, adding both to result.
    fn extract_table_with_alias(
        &mut self,
        result: &mut Vec<(String, String)>,
        seen_tables: &mut std::collections::HashSet<String>,
        cte_names: &std::collections::HashSet<String>,
    ) {
        self.skip_whitespace();

        // Check if it's a subquery (starts with paren)
        if self.check_token(&Token::LParen) {
            return;
        }

        // Parse table name (could be qualified or unqualified)
        let (schema, table_name) = match self.parse_table_name() {
            Some(t) => t,
            None => return,
        };

        let table_ref = format!("[{}].[{}]", schema, table_name);

        // Skip if this is a CTE name (not a real table)
        let table_name_lower = table_name.to_lowercase();
        if cte_names.contains(&table_name_lower) {
            return;
        }

        self.skip_whitespace();

        // Check for AS keyword (optional)
        if self.check_keyword(Keyword::AS) {
            self.advance();
            self.skip_whitespace();
        }

        // Check for alias
        if let Some(alias) = self.try_parse_table_alias() {
            let alias_lower = alias.to_lowercase();

            // Skip if alias is a SQL keyword
            if !Self::is_alias_keyword(&alias_lower) {
                result.push((alias, table_ref.clone()));
            }
        }

        // Always add the table name itself as an alias (for unaliased references like Products.Name)
        if !seen_tables.contains(&table_name_lower) {
            seen_tables.insert(table_name_lower);
            result.push((table_name, table_ref));
        }
    }

    /// Extract CTE aliases from WITH clause
    fn extract_cte_aliases(&mut self, subquery_aliases: &mut std::collections::HashSet<String>) {
        while !self.is_at_end() {
            self.skip_whitespace();

            // Look for WITH keyword (start of CTE)
            if self.check_keyword(Keyword::WITH) {
                self.advance();
                self.skip_whitespace();

                // Skip RECURSIVE if present
                if self.check_word_ci("RECURSIVE") {
                    self.advance();
                    self.skip_whitespace();
                }

                // Parse CTE definitions: name AS (...), name AS (...), ...
                loop {
                    // Get CTE name
                    if let Some(cte_name) = self.parse_identifier() {
                        let cte_name_lower = cte_name.to_lowercase();

                        self.skip_whitespace();

                        // Expect AS keyword
                        if self.check_keyword(Keyword::AS) {
                            self.advance();
                            self.skip_whitespace();

                            // Expect opening paren
                            if self.check_token(&Token::LParen) {
                                // This is a valid CTE - add to subquery aliases
                                if !Self::is_alias_keyword(&cte_name_lower) {
                                    subquery_aliases.insert(cte_name_lower);
                                }

                                // Skip past the balanced parens
                                self.skip_balanced_parens();

                                self.skip_whitespace();

                                // Check for comma (more CTEs) or end of WITH clause
                                if self.check_token(&Token::Comma) {
                                    self.advance();
                                    self.skip_whitespace();
                                    continue; // Parse next CTE
                                }
                            }
                        }
                    }
                    break; // End of CTEs
                }
            } else {
                self.advance();
            }
        }
    }

    /// Extract table reference after FROM or JOIN keyword
    fn extract_table_reference_after_from_join(
        &mut self,
        table_aliases: &mut std::collections::HashMap<String, String>,
        _subquery_aliases: &mut std::collections::HashSet<String>,
    ) {
        self.skip_whitespace();

        // Check if it's a subquery (starts with paren)
        if self.check_token(&Token::LParen) {
            // This is a subquery - don't skip it, let the main loop continue scanning
            // The subquery alias will be captured when we hit the closing paren + AS pattern
            return;
        }

        // Parse table name (could be qualified or unqualified)
        let (schema, table_name) = match self.parse_table_name() {
            Some(t) => t,
            None => return,
        };

        self.skip_whitespace();

        // Check for AS keyword (optional)
        if self.check_keyword(Keyword::AS) {
            self.advance();
            self.skip_whitespace();
        }

        // Check for alias - must be an identifier that's not a keyword like ON, WHERE, etc.
        if let Some(alias) = self.try_parse_table_alias() {
            let alias_lower = alias.to_lowercase();

            // Skip if alias is a SQL keyword
            if Self::is_alias_keyword(&alias_lower) {
                return;
            }

            // Don't overwrite if already captured by a more specific pattern
            if table_aliases.contains_key(&alias_lower) {
                return;
            }

            let table_ref = format!("[{}].[{}]", schema, table_name);
            table_aliases.insert(alias_lower, table_ref);
        }
    }

    /// Parse a table name (qualified or unqualified)
    /// Returns (schema, table_name)
    fn parse_table_name(&mut self) -> Option<(String, String)> {
        let first_ident = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for dot (schema.table pattern)
        if self.check_token(&Token::Period) {
            self.advance();
            self.skip_whitespace();

            let second_ident = self.parse_identifier()?;

            // Skip if schema is a SQL keyword (would make this not a valid schema.table)
            if is_sql_keyword(&first_ident.to_uppercase()) {
                return None;
            }

            Some((first_ident, second_ident))
        } else {
            // Unqualified table - use default schema
            // Skip if table name is a SQL keyword
            if is_sql_keyword(&first_ident.to_uppercase()) {
                return None;
            }
            Some((self.default_schema.clone(), first_ident))
        }
    }

    /// Try to parse a table alias (identifier that's not a reserved keyword for clause structure)
    fn try_parse_table_alias(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        // Check if current token is a word that could be an alias
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                let value_upper = w.value.to_uppercase();

                // These keywords indicate end of table reference, not an alias
                if matches!(
                    value_upper.as_str(),
                    "ON" | "WHERE"
                        | "INNER"
                        | "LEFT"
                        | "RIGHT"
                        | "OUTER"
                        | "CROSS"
                        | "FULL"
                        | "JOIN"
                        | "GROUP"
                        | "ORDER"
                        | "HAVING"
                        | "UNION"
                        | "WITH"
                        | "AND"
                        | "OR"
                        | "NOT"
                        | "SET"
                        | "FROM"
                        | "SELECT"
                        | "INTO"
                        | "WHEN"
                        | "THEN"
                        | "ELSE"
                        | "END"
                        | "CASE"
                        | "FOR"
                ) {
                    return None;
                }

                // Also check if it's a sqlparser keyword that indicates clause structure
                if matches!(
                    w.keyword,
                    Keyword::ON
                        | Keyword::WHERE
                        | Keyword::INNER
                        | Keyword::LEFT
                        | Keyword::RIGHT
                        | Keyword::OUTER
                        | Keyword::CROSS
                        | Keyword::FULL
                        | Keyword::JOIN
                        | Keyword::GROUP
                        | Keyword::ORDER
                        | Keyword::HAVING
                        | Keyword::UNION
                        | Keyword::WITH
                        | Keyword::AND
                        | Keyword::OR
                        | Keyword::NOT
                        | Keyword::SET
                        | Keyword::FROM
                        | Keyword::SELECT
                        | Keyword::INTO
                        | Keyword::WHEN
                        | Keyword::THEN
                        | Keyword::ELSE
                        | Keyword::END
                        | Keyword::CASE
                        | Keyword::FOR
                ) {
                    return None;
                }

                // This is a valid alias
                let alias = w.value.clone();
                self.advance();
                return Some(alias);
            }
        }

        None
    }

    /// Try to parse a subquery alias after closing paren
    /// This is similar to try_parse_table_alias but handles the ) AS alias or ) alias pattern
    fn try_parse_subquery_alias(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        // Check if current token is a word that could be a subquery alias
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                let value_upper = w.value.to_uppercase();

                // These keywords indicate something other than a subquery alias
                if matches!(
                    value_upper.as_str(),
                    "ON" | "WHERE"
                        | "INNER"
                        | "LEFT"
                        | "RIGHT"
                        | "OUTER"
                        | "CROSS"
                        | "FULL"
                        | "JOIN"
                        | "GROUP"
                        | "ORDER"
                        | "HAVING"
                        | "UNION"
                        | "WITH"
                        | "AND"
                        | "OR"
                        | "NOT"
                        | "SET"
                        | "FROM"
                        | "SELECT"
                        | "INTO"
                        | "WHEN"
                        | "THEN"
                        | "ELSE"
                        | "END"
                        | "CASE"
                        | "FOR"
                        | "AS" // Don't consume AS here - it's handled by caller
                ) {
                    return None;
                }

                // Also check if it's a sqlparser keyword that indicates clause structure
                if matches!(
                    w.keyword,
                    Keyword::ON
                        | Keyword::WHERE
                        | Keyword::INNER
                        | Keyword::LEFT
                        | Keyword::RIGHT
                        | Keyword::OUTER
                        | Keyword::CROSS
                        | Keyword::FULL
                        | Keyword::JOIN
                        | Keyword::GROUP
                        | Keyword::ORDER
                        | Keyword::HAVING
                        | Keyword::UNION
                        | Keyword::WITH
                        | Keyword::AND
                        | Keyword::OR
                        | Keyword::NOT
                        | Keyword::SET
                        | Keyword::FROM
                        | Keyword::SELECT
                        | Keyword::INTO
                        | Keyword::WHEN
                        | Keyword::THEN
                        | Keyword::ELSE
                        | Keyword::END
                        | Keyword::CASE
                        | Keyword::FOR
                        | Keyword::AS
                ) {
                    return None;
                }

                // This is a valid subquery alias
                let alias = w.value.clone();
                self.advance();
                return Some(alias);
            }
        }

        None
    }

    /// Check if a word is a SQL keyword that should not be treated as an alias
    fn is_alias_keyword(word: &str) -> bool {
        matches!(
            word.to_uppercase().as_str(),
            "ON" | "WHERE"
                | "INNER"
                | "LEFT"
                | "RIGHT"
                | "OUTER"
                | "CROSS"
                | "JOIN"
                | "GROUP"
                | "ORDER"
                | "HAVING"
                | "UNION"
                | "WITH"
                | "AS"
                | "AND"
                | "OR"
                | "NOT"
                | "SET"
                | "FROM"
                | "SELECT"
                | "INTO"
        )
    }

    /// Check if current position is at a JOIN keyword (INNER, LEFT, RIGHT, FULL, CROSS, JOIN)
    fn is_join_keyword(&self) -> bool {
        self.check_keyword(Keyword::INNER)
            || self.check_keyword(Keyword::LEFT)
            || self.check_keyword(Keyword::RIGHT)
            || self.check_keyword(Keyword::FULL)
            || self.check_keyword(Keyword::JOIN)
    }

    /// Skip past JOIN keyword variants (INNER JOIN, LEFT OUTER JOIN, etc.)
    fn skip_join_keywords(&mut self) {
        // Skip INNER/LEFT/RIGHT/FULL/CROSS
        if self.check_keyword(Keyword::INNER)
            || self.check_keyword(Keyword::LEFT)
            || self.check_keyword(Keyword::RIGHT)
            || self.check_keyword(Keyword::FULL)
            || self.check_keyword(Keyword::CROSS)
        {
            self.advance();
            self.skip_whitespace();
        }

        // Skip OUTER (for LEFT OUTER JOIN, etc.)
        if self.check_keyword(Keyword::OUTER) {
            self.advance();
            self.skip_whitespace();
        }

        // Skip JOIN
        if self.check_keyword(Keyword::JOIN) {
            self.advance();
            self.skip_whitespace();
        }
    }

    /// Skip balanced parentheses
    fn skip_balanced_parens(&mut self) {
        if !self.check_token(&Token::LParen) {
            return;
        }

        let mut depth = 0;
        while !self.is_at_end() {
            if self.check_token(&Token::LParen) {
                depth += 1;
            } else if self.check_token(&Token::RParen) {
                depth -= 1;
                if depth == 0 {
                    self.advance();
                    return;
                }
            }
            self.advance();
        }
    }

    /// Parse an identifier (bracketed or unbracketed)
    fn parse_identifier(&mut self) -> Option<String> {
        if self.is_at_end() {
            return None;
        }

        let token = self.current_token()?;
        if let Token::Word(w) = &token.token {
            let name = w.value.clone();
            self.advance();
            Some(name)
        } else {
            None
        }
    }

    /// Skip whitespace tokens
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                if matches!(&token.token, Token::Whitespace(_)) {
                    self.advance();
                } else {
                    break;
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
    fn current_token(&self) -> Option<&sqlparser::tokenizer::TokenWithSpan> {
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

// =============================================================================
// INSERT SELECT Token Parser (Phase 20.4.6)
// =============================================================================
// Replaces INSERT_SELECT_RE and INSERT_SELECT_JOIN_RE regex patterns with
// tokenizer-based parsing. Handles INSERT INTO ... SELECT FROM inserted/deleted
// statements in trigger bodies, with or without JOIN clauses.

/// Represents a parsed INSERT...SELECT statement from a trigger body
#[derive(Debug, Clone)]
pub(crate) struct InsertSelectStatement {
    /// Schema of the target table
    pub schema: String,
    /// Name of the target table
    pub table: String,
    /// Column list in the INSERT clause (raw text between parens)
    pub column_list: String,
    /// SELECT expression (raw text between SELECT and FROM)
    pub select_expr: String,
    /// Whether this has a JOIN clause
    pub has_join: bool,
    /// First alias (for JOIN case: alias of first inserted/deleted)
    pub alias1: Option<String>,
    /// Second alias (for JOIN case: alias of second inserted/deleted)
    pub alias2: Option<String>,
    /// ON clause content (for JOIN case)
    pub on_clause: Option<String>,
}

/// Token-based parser for INSERT...SELECT statements in trigger bodies.
/// Replaces INSERT_SELECT_RE and INSERT_SELECT_JOIN_RE regex patterns.
pub(crate) struct InsertSelectTokenParser {
    tokens: Vec<sqlparser::tokenizer::TokenWithSpan>,
    pos: usize,
    source: String,           // Keep source for extracting raw text segments
    line_offsets: Vec<usize>, // For converting token locations to byte offsets
}

impl InsertSelectTokenParser {
    /// Create a new parser for SQL body text
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;
        let line_offsets = compute_line_offsets(sql);
        Some(Self {
            tokens,
            pos: 0,
            source: sql.to_string(),
            line_offsets,
        })
    }

    /// Extract all INSERT...SELECT statements from the SQL body
    pub fn extract_statements(&mut self) -> Vec<InsertSelectStatement> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace();

            // Look for INSERT keyword
            if self.check_keyword(Keyword::INSERT) {
                if let Some(stmt) = self.try_parse_insert_select() {
                    statements.push(stmt);
                } else {
                    self.advance();
                }
            } else {
                self.advance();
            }
        }

        statements
    }

    /// Try to parse an INSERT...SELECT FROM inserted/deleted statement
    fn try_parse_insert_select(&mut self) -> Option<InsertSelectStatement> {
        let start_pos = self.pos;

        // Expect INSERT
        if !self.check_keyword(Keyword::INSERT) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Expect INTO
        if !self.check_keyword(Keyword::INTO) {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse target table: [schema].[table]
        let (schema, table) = self.parse_bracketed_table_name()?;

        self.skip_whitespace();

        // Expect column list in parentheses
        if !self.check_token(&Token::LParen) {
            self.pos = start_pos;
            return None;
        }

        let column_list = self.extract_balanced_parens_content()?;

        self.skip_whitespace();

        // Expect SELECT keyword
        if !self.check_keyword(Keyword::SELECT) {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Extract SELECT expression (everything until FROM)
        let select_start = self.current_byte_offset();
        if !self.scan_until_keyword(Keyword::FROM) {
            self.pos = start_pos;
            return None;
        }
        let select_end = self.current_byte_offset();
        let select_expr = self.source[select_start..select_end].trim().to_string();

        // Now at FROM keyword - advance past it
        self.advance();
        self.skip_whitespace();

        // Check if next token is "inserted" or "deleted"
        if !self.check_inserted_or_deleted() {
            self.pos = start_pos;
            return None;
        }
        self.advance(); // past inserted/deleted
        self.skip_whitespace();

        // Check for alias (word that's not a semicolon or keyword)
        // Pattern 1: FROM inserted; (no alias, no JOIN)
        // Pattern 2: FROM inserted alias JOIN deleted alias2 ON ...;
        if self.check_token(&Token::SemiColon) {
            // Simple case: no alias, no JOIN
            return Some(InsertSelectStatement {
                schema,
                table,
                column_list,
                select_expr,
                has_join: false,
                alias1: None,
                alias2: None,
                on_clause: None,
            });
        }

        // Check for alias
        let alias1 = self.try_parse_identifier();
        self.skip_whitespace();

        // Check for JOIN (or INNER JOIN)
        let has_inner = self.check_keyword(Keyword::INNER);
        if has_inner {
            self.advance();
            self.skip_whitespace();
        }

        if !self.check_keyword(Keyword::JOIN) {
            // No JOIN - this is the simple form with just FROM inserted/deleted
            // Could have alias or not
            return Some(InsertSelectStatement {
                schema,
                table,
                column_list,
                select_expr,
                has_join: false,
                alias1: None,
                alias2: None,
                on_clause: None,
            });
        }

        // JOIN case - advance past JOIN
        self.advance();
        self.skip_whitespace();

        // Check for second inserted/deleted
        if !self.check_inserted_or_deleted() {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse second alias
        let alias2 = self.try_parse_identifier();
        self.skip_whitespace();

        // Expect ON keyword
        if !self.check_keyword(Keyword::ON) {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Extract ON clause (everything until semicolon)
        let on_start = self.current_byte_offset();
        while !self.is_at_end() && !self.check_token(&Token::SemiColon) {
            self.advance();
        }
        let on_end = self.current_byte_offset();
        let on_clause = self.source[on_start..on_end].trim().to_string();

        Some(InsertSelectStatement {
            schema,
            table,
            column_list,
            select_expr,
            has_join: true,
            alias1,
            alias2,
            on_clause: Some(on_clause),
        })
    }

    /// Parse a bracketed table name: [schema].[table]
    fn parse_bracketed_table_name(&mut self) -> Option<(String, String)> {
        // First part: [schema]
        let schema = self.parse_bracketed_identifier()?;
        self.skip_whitespace();

        // Expect period
        if !self.check_token(&Token::Period) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Second part: [table]
        let table = self.parse_bracketed_identifier()?;

        Some((schema, table))
    }

    /// Parse a bracketed identifier like [Name]
    fn parse_bracketed_identifier(&mut self) -> Option<String> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                if w.quote_style == Some('[') {
                    let name = w.value.clone();
                    self.advance();
                    return Some(name);
                }
            }
        }
        None
    }

    /// Extract content inside balanced parentheses (consuming the parens)
    fn extract_balanced_parens_content(&mut self) -> Option<String> {
        if !self.check_token(&Token::LParen) {
            return None;
        }

        self.advance(); // Past opening paren
        self.skip_whitespace();

        // Record start position (after opening paren and whitespace)
        let start = self.current_byte_offset();

        let mut depth = 1;
        let mut end = start;
        while !self.is_at_end() && depth > 0 {
            if self.check_token(&Token::LParen) {
                depth += 1;
                self.advance();
            } else if self.check_token(&Token::RParen) {
                depth -= 1;
                if depth == 0 {
                    // Record end before the closing paren
                    end = self.current_byte_offset();
                    self.advance(); // Past closing paren
                    break;
                }
                self.advance();
            } else {
                self.advance();
            }
        }

        if start < end && end <= self.source.len() {
            Some(self.source[start..end].trim().to_string())
        } else if start < self.source.len() {
            Some(self.source[start..].trim().to_string())
        } else {
            Some(String::new())
        }
    }

    /// Scan tokens until we find a specific keyword, returning true if found
    fn scan_until_keyword(&mut self, keyword: Keyword) -> bool {
        while !self.is_at_end() {
            if self.check_keyword(keyword) {
                return true;
            }
            self.advance();
        }
        false
    }

    /// Check if current token is "inserted" or "deleted" (case-insensitive)
    fn check_inserted_or_deleted(&self) -> bool {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                return w.value.eq_ignore_ascii_case("inserted")
                    || w.value.eq_ignore_ascii_case("deleted");
            }
        }
        false
    }

    /// Try to parse an identifier (not a keyword)
    fn try_parse_identifier(&mut self) -> Option<String> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                // Skip SQL keywords that would terminate the expression
                let upper = w.value.to_uppercase();
                if matches!(
                    upper.as_str(),
                    "INNER" | "JOIN" | "ON" | "WHERE" | "AND" | "OR" | "ORDER" | "GROUP"
                ) {
                    return None;
                }
                // Also check sqlparser keyword enum
                if matches!(
                    w.keyword,
                    Keyword::INNER
                        | Keyword::JOIN
                        | Keyword::ON
                        | Keyword::WHERE
                        | Keyword::AND
                        | Keyword::OR
                        | Keyword::ORDER
                        | Keyword::GROUP
                ) {
                    return None;
                }

                let ident = w.value.clone();
                self.advance();
                return Some(ident);
            }
        }
        None
    }

    /// Get current byte offset in source text
    fn current_byte_offset(&self) -> usize {
        if let Some(token) = self.current_token() {
            // Convert line/column to byte offset
            location_to_byte_offset(
                &self.line_offsets,
                token.span.start.line,
                token.span.start.column,
            )
        } else {
            self.source.len()
        }
    }

    /// Skip whitespace tokens
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                if matches!(&token.token, Token::Whitespace(_)) {
                    self.advance();
                } else {
                    break;
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
    fn current_token(&self) -> Option<&sqlparser::tokenizer::TokenWithSpan> {
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

    /// Check if current token matches a specific token type
    fn check_token(&self, expected: &Token) -> bool {
        if let Some(token) = self.current_token() {
            std::mem::discriminant(&token.token) == std::mem::discriminant(expected)
        } else {
            false
        }
    }
}

// =============================================================================
// UPDATE Token Parser (Phase 20.4.7)
// =============================================================================
// Replaces UPDATE_ALIAS_RE regex pattern with tokenizer-based parsing.
// Handles UPDATE alias SET ... FROM [schema].[table] alias (INNER) JOIN inserted/deleted alias ON ...
// statements in trigger bodies.

/// Represents a parsed UPDATE...FROM...JOIN statement from a trigger body
#[derive(Debug, Clone)]
pub(crate) struct UpdateStatement {
    /// Alias used in UPDATE clause (e.g., "p" in UPDATE p SET ...)
    pub update_alias: String,
    /// SET clause content (raw text between SET and FROM)
    pub set_clause: String,
    /// Schema of the target table
    pub schema: String,
    /// Name of the target table
    pub table: String,
    /// Alias for the target table in FROM clause
    pub table_alias: String,
    /// Magic table reference (inserted or deleted) - used in tests for validation
    #[allow(dead_code)]
    pub magic_table: String,
    /// Alias for the magic table
    pub magic_alias: String,
    /// ON clause content
    pub on_clause: String,
}

/// Token-based parser for UPDATE...FROM...JOIN statements in trigger bodies.
/// Replaces UPDATE_ALIAS_RE regex pattern.
pub(crate) struct UpdateTokenParser {
    tokens: Vec<sqlparser::tokenizer::TokenWithSpan>,
    pos: usize,
    source: String,           // Keep source for extracting raw text segments
    line_offsets: Vec<usize>, // For converting token locations to byte offsets
}

impl UpdateTokenParser {
    /// Create a new parser for SQL body text
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;
        let line_offsets = compute_line_offsets(sql);
        Some(Self {
            tokens,
            pos: 0,
            source: sql.to_string(),
            line_offsets,
        })
    }

    /// Extract all UPDATE...FROM...JOIN statements from the SQL body
    pub fn extract_statements(&mut self) -> Vec<UpdateStatement> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace();

            // Look for UPDATE keyword
            if self.check_keyword(Keyword::UPDATE) {
                if let Some(stmt) = self.try_parse_update() {
                    statements.push(stmt);
                } else {
                    self.advance();
                }
            } else {
                self.advance();
            }
        }

        statements
    }

    /// Try to parse an UPDATE...FROM...JOIN inserted/deleted statement
    fn try_parse_update(&mut self) -> Option<UpdateStatement> {
        let start_pos = self.pos;

        // Expect UPDATE
        if !self.check_keyword(Keyword::UPDATE) {
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Parse update alias (word before SET)
        let update_alias = self.try_parse_any_identifier()?;
        self.skip_whitespace();

        // Expect SET
        if !self.check_keyword(Keyword::SET) {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Extract SET clause (everything until FROM)
        let set_start = self.current_byte_offset();
        if !self.scan_until_keyword(Keyword::FROM) {
            self.pos = start_pos;
            return None;
        }
        let set_end = self.current_byte_offset();
        let set_clause = self.source[set_start..set_end].trim().to_string();

        // Now at FROM keyword - advance past it
        self.advance();
        self.skip_whitespace();

        // Parse table: [schema].[table] or schema.table
        let (schema, table) = self.parse_table_name()?;
        self.skip_whitespace();

        // Parse table alias (must be present for this pattern)
        let table_alias = match self.try_parse_any_identifier() {
            Some(alias) => alias,
            None => {
                self.pos = start_pos;
                return None;
            }
        };
        self.skip_whitespace();

        // Optional INNER keyword before JOIN
        if self.check_keyword(Keyword::INNER) {
            self.advance();
            self.skip_whitespace();
        }

        // Expect JOIN
        if !self.check_keyword(Keyword::JOIN) {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Check for inserted/deleted magic table
        if !self.check_inserted_or_deleted() {
            self.pos = start_pos;
            return None;
        }
        let magic_table = self.current_word_value().unwrap_or_default();
        self.advance();
        self.skip_whitespace();

        // Parse magic alias
        let magic_alias = match self.try_parse_any_identifier() {
            Some(alias) => alias,
            None => {
                self.pos = start_pos;
                return None;
            }
        };
        self.skip_whitespace();

        // Expect ON keyword
        if !self.check_keyword(Keyword::ON) {
            self.pos = start_pos;
            return None;
        }
        self.advance();
        self.skip_whitespace();

        // Extract ON clause (everything until semicolon or end)
        let on_start = self.current_byte_offset();
        while !self.is_at_end() && !self.check_token(&Token::SemiColon) {
            self.advance();
        }
        let on_end = self.current_byte_offset();
        let on_clause = self.source[on_start..on_end].trim().to_string();

        Some(UpdateStatement {
            update_alias,
            set_clause,
            schema,
            table,
            table_alias,
            magic_table,
            magic_alias,
            on_clause,
        })
    }

    /// Parse a table name: [schema].[table] or schema.table
    fn parse_table_name(&mut self) -> Option<(String, String)> {
        // Try bracketed form first
        if let Some(first) = self.try_parse_bracketed_identifier() {
            self.skip_whitespace();
            if self.check_token(&Token::Period) {
                self.advance();
                self.skip_whitespace();
                if let Some(second) = self.try_parse_bracketed_identifier() {
                    return Some((first, second));
                } else if let Some(second) = self.try_parse_any_identifier() {
                    return Some((first, second));
                }
            }
            // Single bracketed identifier - might be just table name, use dbo as default
            return Some(("dbo".to_string(), first));
        }

        // Try unbracketed form
        if let Some(first) = self.try_parse_any_identifier() {
            self.skip_whitespace();
            if self.check_token(&Token::Period) {
                self.advance();
                self.skip_whitespace();
                if let Some(second) = self.try_parse_bracketed_identifier() {
                    return Some((first, second));
                } else if let Some(second) = self.try_parse_any_identifier() {
                    return Some((first, second));
                }
            }
            // Single identifier - use as table with dbo schema
            return Some(("dbo".to_string(), first));
        }

        None
    }

    /// Try to parse a bracketed identifier like [Name]
    fn try_parse_bracketed_identifier(&mut self) -> Option<String> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                if w.quote_style == Some('[') {
                    let name = w.value.clone();
                    self.advance();
                    return Some(name);
                }
            }
        }
        None
    }

    /// Try to parse any identifier (not a keyword)
    fn try_parse_any_identifier(&mut self) -> Option<String> {
        // Try bracketed first
        if let Some(name) = self.try_parse_bracketed_identifier() {
            return Some(name);
        }

        // Then try unbracketed word
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                // Skip SQL keywords that would terminate the expression
                let upper = w.value.to_uppercase();
                if matches!(
                    upper.as_str(),
                    "SET"
                        | "FROM"
                        | "INNER"
                        | "JOIN"
                        | "ON"
                        | "WHERE"
                        | "AND"
                        | "OR"
                        | "ORDER"
                        | "GROUP"
                        | "HAVING"
                ) {
                    return None;
                }
                // Also check sqlparser keyword enum
                if matches!(
                    w.keyword,
                    Keyword::SET
                        | Keyword::FROM
                        | Keyword::INNER
                        | Keyword::JOIN
                        | Keyword::ON
                        | Keyword::WHERE
                        | Keyword::AND
                        | Keyword::OR
                        | Keyword::ORDER
                        | Keyword::GROUP
                        | Keyword::HAVING
                ) {
                    return None;
                }

                let ident = w.value.clone();
                self.advance();
                return Some(ident);
            }
        }
        None
    }

    /// Get the value of the current word token (if any)
    fn current_word_value(&self) -> Option<String> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                return Some(w.value.clone());
            }
        }
        None
    }

    /// Scan tokens until we find a specific keyword, returning true if found
    fn scan_until_keyword(&mut self, keyword: Keyword) -> bool {
        while !self.is_at_end() {
            if self.check_keyword(keyword) {
                return true;
            }
            self.advance();
        }
        false
    }

    /// Check if current token is "inserted" or "deleted" (case-insensitive)
    fn check_inserted_or_deleted(&self) -> bool {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                return w.value.eq_ignore_ascii_case("inserted")
                    || w.value.eq_ignore_ascii_case("deleted");
            }
        }
        false
    }

    /// Get current byte offset in source text
    fn current_byte_offset(&self) -> usize {
        if let Some(token) = self.current_token() {
            // Convert line/column to byte offset
            location_to_byte_offset(
                &self.line_offsets,
                token.span.start.line,
                token.span.start.column,
            )
        } else {
            self.source.len()
        }
    }

    /// Skip whitespace tokens
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                if matches!(&token.token, Token::Whitespace(_)) {
                    self.advance();
                } else {
                    break;
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
    fn current_token(&self) -> Option<&sqlparser::tokenizer::TokenWithSpan> {
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

    /// Check if current token matches a specific token type
    fn check_token(&self, expected: &Token) -> bool {
        if let Some(token) = self.current_token() {
            std::mem::discriminant(&token.token) == std::mem::discriminant(expected)
        } else {
            false
        }
    }
}

// =============================================================================
// Body Dependency Token Scanner (Phase 20.2.1)
// =============================================================================
// Replaces TOKEN_RE regex with tokenizer-based scanning for body dependency extraction.
// Handles 8 token patterns:
// 1. @param - parameter references
// 2. [a].[b].[c] - three-part bracketed reference (schema.table.column)
// 3. [a].[b] - two-part bracketed reference (schema.table or alias.column)
// 4. alias.[column] - unbracketed alias with bracketed column
// 5. [alias].column - bracketed alias with unbracketed column
// 6. [ident] - single bracketed identifier
// 7. schema.table - unbracketed two-part reference
// 8. ident - unbracketed single identifier

/// Represents a token pattern matched by the body dependency scanner
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BodyDepToken {
    /// @param - parameter reference
    Parameter(String),
    /// [schema].[table].[column] - three-part bracketed
    ThreePartBracketed {
        schema: String,
        table: String,
        column: String,
    },
    /// [first].[second] - two-part bracketed (schema.table or alias.column)
    TwoPartBracketed { first: String, second: String },
    /// alias.[column] - unbracketed alias with bracketed column
    AliasDotBracketedColumn { alias: String, column: String },
    /// [alias].column - bracketed alias with unbracketed column
    BracketedAliasDotColumn { alias: String, column: String },
    /// [ident] - single bracketed identifier
    SingleBracketed(String),
    /// schema.table - unbracketed two-part reference
    TwoPartUnbracketed { first: String, second: String },
    /// ident - single unbracketed identifier
    SingleUnbracketed(String),
}

/// Token-based scanner for body dependency extraction.
/// Replaces TOKEN_RE regex with proper tokenization for handling whitespace, comments,
/// and SQL syntax correctly.
pub(crate) struct BodyDependencyTokenScanner {
    tokens: Vec<sqlparser::tokenizer::TokenWithSpan>,
    pos: usize,
}

impl BodyDependencyTokenScanner {
    /// Create a new scanner for SQL body text
    pub fn new(sql: &str) -> Option<Self> {
        let dialect = MsSqlDialect {};
        let tokens = Tokenizer::new(&dialect, sql)
            .tokenize_with_location()
            .ok()?;
        Some(Self { tokens, pos: 0 })
    }

    /// Scan the body and return all matched tokens in order of appearance
    pub fn scan(&mut self) -> Vec<BodyDepToken> {
        let mut results = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace();
            if self.is_at_end() {
                break;
            }

            // Try to match patterns in order of specificity
            if let Some(token) = self.try_scan_token() {
                results.push(token);
            } else {
                // No pattern matched, advance to next token
                self.advance();
            }
        }

        results
    }

    /// Try to scan a single token pattern at the current position
    fn try_scan_token(&mut self) -> Option<BodyDepToken> {
        // Pattern 1: @param - parameter reference
        // MsSqlDialect tokenizes @param as a single Word token with @ prefix
        if self.is_parameter_word() {
            return self.try_scan_parameter();
        }

        // Patterns 2-6: Start with a bracketed identifier [ident]
        if self.is_bracketed_word() {
            return self.try_scan_bracketed_pattern();
        }

        // Patterns 7-8: Unbracketed identifiers (not starting with @)
        if self.is_unbracketed_word() {
            return self.try_scan_unbracketed_pattern();
        }

        None
    }

    /// Try to scan a parameter reference: @param
    /// MsSqlDialect tokenizes @param as a single Word with "@param" as value
    fn try_scan_parameter(&mut self) -> Option<BodyDepToken> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                if w.quote_style.is_none() && w.value.starts_with('@') {
                    // Extract parameter name without @ prefix
                    let param_name = w.value[1..].to_string();
                    self.advance();
                    return Some(BodyDepToken::Parameter(param_name));
                }
            }
        }
        None
    }

    /// Check if current token is a parameter word (starts with @)
    fn is_parameter_word(&self) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.quote_style.is_none() && w.value.starts_with('@'))
        } else {
            false
        }
    }

    /// Try to scan patterns starting with a bracketed identifier
    fn try_scan_bracketed_pattern(&mut self) -> Option<BodyDepToken> {
        let first_ident = self.parse_bracketed_identifier()?;
        self.skip_whitespace();

        // Check for dot separator
        if self.check_token(&Token::Period) {
            self.advance(); // consume .
            self.skip_whitespace();

            // Could be: [a].[b], [a].[b].[c], or [alias].column
            if self.is_bracketed_word() {
                // [a].[b] or [a].[b].[c]
                let second_ident = self.parse_bracketed_identifier()?;
                self.skip_whitespace();

                // Check for third part
                if self.check_token(&Token::Period) {
                    self.advance(); // consume .
                    self.skip_whitespace();

                    if self.is_bracketed_word() {
                        // [a].[b].[c] - three-part bracketed
                        let third_ident = self.parse_bracketed_identifier()?;
                        return Some(BodyDepToken::ThreePartBracketed {
                            schema: first_ident,
                            table: second_ident,
                            column: third_ident,
                        });
                    }
                }

                // [a].[b] - two-part bracketed
                return Some(BodyDepToken::TwoPartBracketed {
                    first: first_ident,
                    second: second_ident,
                });
            } else if self.is_unbracketed_word() {
                // [alias].column - bracketed alias with unbracketed column
                let column = self.parse_unbracketed_identifier()?;
                return Some(BodyDepToken::BracketedAliasDotColumn {
                    alias: first_ident,
                    column,
                });
            }
        }

        // Just [ident] - single bracketed identifier
        Some(BodyDepToken::SingleBracketed(first_ident))
    }

    /// Try to scan patterns starting with an unbracketed identifier
    fn try_scan_unbracketed_pattern(&mut self) -> Option<BodyDepToken> {
        // Check word boundary - we need to make sure we're not continuing from another token
        // This is handled by checking the previous token isn't a word character

        let first_ident = self.parse_unbracketed_identifier()?;
        self.skip_whitespace();

        // Check for dot separator
        if self.check_token(&Token::Period) {
            self.advance(); // consume .
            self.skip_whitespace();

            if self.is_bracketed_word() {
                // alias.[column] - unbracketed alias with bracketed column
                let column = self.parse_bracketed_identifier()?;
                return Some(BodyDepToken::AliasDotBracketedColumn {
                    alias: first_ident,
                    column,
                });
            } else if self.is_unbracketed_word() {
                // schema.table - unbracketed two-part
                let second_ident = self.parse_unbracketed_identifier()?;
                return Some(BodyDepToken::TwoPartUnbracketed {
                    first: first_ident,
                    second: second_ident,
                });
            }
        }

        // Just ident - single unbracketed identifier
        Some(BodyDepToken::SingleUnbracketed(first_ident))
    }

    /// Parse a bracketed identifier and return the inner value
    fn parse_bracketed_identifier(&mut self) -> Option<String> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                // Check if it's actually bracketed (quote_style shows the quote type)
                if w.quote_style.is_some() {
                    let value = w.value.clone();
                    self.advance();
                    return Some(value);
                }
            }
        }
        None
    }

    /// Parse an unbracketed identifier
    fn parse_unbracketed_identifier(&mut self) -> Option<String> {
        if let Some(token) = self.current_token() {
            if let Token::Word(w) = &token.token {
                // Check if it's unbracketed (no quote_style)
                if w.quote_style.is_none() {
                    let value = w.value.clone();
                    self.advance();
                    return Some(value);
                }
            }
        }
        None
    }

    /// Check if current token is a bracketed word (identifier with quote_style)
    fn is_bracketed_word(&self) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.quote_style.is_some())
        } else {
            false
        }
    }

    /// Check if current token is an unbracketed word (identifier without quote_style, not starting with @)
    fn is_unbracketed_word(&self) -> bool {
        if let Some(token) = self.current_token() {
            matches!(&token.token, Token::Word(w) if w.quote_style.is_none() && !w.value.starts_with('@'))
        } else {
            false
        }
    }

    /// Skip whitespace tokens
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            if let Some(token) = self.current_token() {
                if matches!(&token.token, Token::Whitespace(_)) {
                    self.advance();
                } else {
                    break;
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
    fn current_token(&self) -> Option<&sqlparser::tokenizer::TokenWithSpan> {
        self.tokens.get(self.pos)
    }

    /// Advance to next token
    fn advance(&mut self) {
        if !self.is_at_end() {
            self.pos += 1;
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

// =============================================================================
// Table Reference Extraction (Phase 20.4.3)
// =============================================================================
// Token-based extraction of table references from SQL body text.
// Replaces BRACKETED_TABLE_RE and UNBRACKETED_TABLE_RE regex patterns.

/// Extract all two-part table references from SQL body text using tokenization.
///
/// This function scans the body and extracts references in both formats:
/// - Bracketed: `[schema].[table]`
/// - Unbracketed: `schema.table`
///
/// It filters out:
/// - Parameter references (starting with @)
/// - SQL keywords as schema names (FROM.something)
/// - Table alias references (alias.column)
///
/// Returns a deduplicated list of table references in `[schema].[table]` format.
///
/// This replaces the BRACKETED_TABLE_RE and UNBRACKETED_TABLE_RE regex patterns
/// for more robust parsing that handles whitespace, comments, and edge cases correctly.
pub(crate) fn extract_table_refs_tokenized(
    body: &str,
    table_aliases: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let mut table_refs: Vec<String> = Vec::with_capacity(5);

    let Some(mut scanner) = BodyDependencyTokenScanner::new(body) else {
        return table_refs;
    };

    for token in scanner.scan() {
        match token {
            BodyDepToken::TwoPartBracketed { first, second } => {
                // [schema].[table] pattern - equivalent to BRACKETED_TABLE_RE
                // Skip if either part starts with @ (parameter)
                if !first.starts_with('@') && !second.starts_with('@') {
                    let table_ref = format!("[{}].[{}]", first, second);
                    if !table_refs.contains(&table_ref) {
                        table_refs.push(table_ref);
                    }
                }
            }
            BodyDepToken::TwoPartUnbracketed { first, second } => {
                // schema.table pattern - equivalent to UNBRACKETED_TABLE_RE
                // Skip if first part is a SQL keyword (like FROM.something)
                if is_sql_keyword(&first.to_uppercase()) {
                    continue;
                }
                // Skip if first part is a table alias (alias.column reference)
                if table_aliases.contains_key(&first.to_lowercase()) {
                    continue;
                }
                let table_ref = format!("[{}].[{}]", first, second);
                if !table_refs.contains(&table_ref) {
                    table_refs.push(table_ref);
                }
            }
            BodyDepToken::AliasDotBracketedColumn { alias, column } => {
                // alias.[column] pattern - could be schema.[table] if alias is not a known alias
                if is_sql_keyword(&alias.to_uppercase()) {
                    continue;
                }
                // If not a known alias, treat as potential schema.table reference
                if !table_aliases.contains_key(&alias.to_lowercase()) {
                    let table_ref = format!("[{}].[{}]", alias, column);
                    if !table_refs.contains(&table_ref) {
                        table_refs.push(table_ref);
                    }
                }
            }
            BodyDepToken::BracketedAliasDotColumn { alias, column } => {
                // [alias].column pattern - could be [schema].table if alias is not a known alias
                if !alias.starts_with('@') {
                    if is_sql_keyword(&alias.to_uppercase()) {
                        continue;
                    }
                    // If not a known alias, treat as potential schema.table reference
                    if !table_aliases.contains_key(&alias.to_lowercase()) {
                        let table_ref = format!("[{}].[{}]", alias, column);
                        if !table_refs.contains(&table_ref) {
                            table_refs.push(table_ref);
                        }
                    }
                }
            }
            BodyDepToken::ThreePartBracketed { schema, table, .. } => {
                // [schema].[table].[column] - extract the table part
                if !schema.starts_with('@') && !table.starts_with('@') {
                    let table_ref = format!("[{}].[{}]", schema, table);
                    if !table_refs.contains(&table_ref) {
                        table_refs.push(table_ref);
                    }
                }
            }
            // Skip single identifiers and parameters - they're not table references
            BodyDepToken::SingleBracketed(_)
            | BodyDepToken::SingleUnbracketed(_)
            | BodyDepToken::Parameter(_) => {}
        }
    }

    table_refs
}

// =============================================================================
// Qualified Name Tokenization (Phase 20.2.8)
// =============================================================================
// Token-based parsing for qualified SQL names like [schema].[table].[column].
// Replaces split('.') string operations with proper tokenization that handles
// whitespace, comments, and various bracket/quote styles correctly.

/// Represents a parsed qualified name with 1-3 parts.
/// Used for schema.table or schema.table.column references.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QualifiedName {
    /// The first part (schema for 2+ parts, or name for single part)
    pub first: String,
    /// The second part (table name for 2+ parts)
    pub second: Option<String>,
    /// The third part (column name for 3 parts)
    pub third: Option<String>,
}

impl QualifiedName {
    /// Creates a single-part name
    pub fn single(name: String) -> Self {
        Self {
            first: name,
            second: None,
            third: None,
        }
    }

    /// Creates a two-part name (schema.table)
    pub fn two_part(first: String, second: String) -> Self {
        Self {
            first,
            second: Some(second),
            third: None,
        }
    }

    /// Creates a three-part name (schema.table.column)
    pub fn three_part(first: String, second: String, third: String) -> Self {
        Self {
            first,
            second: Some(second),
            third: Some(third),
        }
    }

    /// Returns the number of parts in this qualified name
    pub fn part_count(&self) -> usize {
        if self.third.is_some() {
            3
        } else if self.second.is_some() {
            2
        } else {
            1
        }
    }

    /// Returns the last part of the name (column for 3-part, table for 2-part, name for 1-part)
    pub fn last_part(&self) -> &str {
        self.third
            .as_deref()
            .or(self.second.as_deref())
            .unwrap_or(&self.first)
    }

    /// Returns the schema and table as a tuple if this is a 2+ part name
    pub fn schema_and_table(&self) -> Option<(&str, &str)> {
        self.second
            .as_ref()
            .map(|table| (self.first.as_str(), table.as_str()))
    }

    /// Formats as a bracketed reference: [first].[second] or [first].[second].[third]
    #[cfg(test)]
    pub fn to_bracketed(&self) -> String {
        match (&self.second, &self.third) {
            (Some(second), Some(third)) => {
                format!("[{}].[{}].[{}]", self.first, second, third)
            }
            (Some(second), None) => format!("[{}].[{}]", self.first, second),
            (None, _) => format!("[{}]", self.first),
        }
    }
}

/// Parse a qualified name from a string using tokenization.
///
/// Handles all combinations of bracketed and unbracketed identifiers:
/// - `[schema].[table].[column]` → 3-part
/// - `[schema].[table]` → 2-part
/// - `schema.table` → 2-part (unbracketed)
/// - `alias.[column]` → 2-part (mixed)
/// - `[alias].column` → 2-part (mixed)
/// - `[name]` or `name` → 1-part
///
/// This replaces split('.') operations with proper tokenization that handles
/// whitespace and SQL syntax correctly.
pub(crate) fn parse_qualified_name_tokenized(sql: &str) -> Option<QualifiedName> {
    let mut scanner = BodyDependencyTokenScanner::new(sql)?;
    scanner.skip_whitespace();

    if scanner.is_at_end() {
        return None;
    }

    // Try to parse a token pattern - this will give us the qualified name structure
    let token = scanner.try_scan_token()?;

    // Convert BodyDepToken to QualifiedName
    match token {
        BodyDepToken::ThreePartBracketed {
            schema,
            table,
            column,
        } => Some(QualifiedName::three_part(schema, table, column)),

        BodyDepToken::TwoPartBracketed { first, second } => {
            Some(QualifiedName::two_part(first, second))
        }

        BodyDepToken::AliasDotBracketedColumn { alias, column } => {
            Some(QualifiedName::two_part(alias, column))
        }

        BodyDepToken::BracketedAliasDotColumn { alias, column } => {
            Some(QualifiedName::two_part(alias, column))
        }

        BodyDepToken::TwoPartUnbracketed { first, second } => {
            Some(QualifiedName::two_part(first, second))
        }

        BodyDepToken::SingleBracketed(name) => Some(QualifiedName::single(name)),

        BodyDepToken::SingleUnbracketed(name) => Some(QualifiedName::single(name)),

        BodyDepToken::Parameter(_) => None, // Parameters are not qualified names
    }
}

/// Represents a bracketed identifier with its position in the source text.
/// Used for extracting `[ColumnName]` patterns from SQL expressions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BracketedIdentWithPos {
    /// The identifier name without brackets
    pub name: String,
    /// The byte position where the identifier starts (position of the '[')
    pub position: usize,
}

/// Extract single bracketed identifiers from SQL text using tokenization.
///
/// This function uses the sqlparser tokenizer to find all `[identifier]` patterns
/// in the SQL text, returning them with their positions. This replaces the
/// `BRACKETED_IDENT_RE` regex for more robust parsing.
///
/// Only single bracketed identifiers are returned; multi-part references like
/// `[schema].[table]` are not included as individual components.
pub(crate) fn extract_bracketed_identifiers_tokenized(sql: &str) -> Vec<BracketedIdentWithPos> {
    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, sql).tokenize_with_location() else {
        return Vec::new();
    };

    // Build a line/column to byte offset map for position calculation
    // This allows us to convert token Location (line, column) to byte offset
    let line_offsets = compute_line_offsets(sql);

    let mut results = Vec::new();
    let mut i = 0;
    let len = tokens.len();

    while i < len {
        let token = &tokens[i];

        // Look for bracketed Word tokens (quote_style is Some('[') for bracketed identifiers)
        if let Token::Word(w) = &token.token {
            if w.quote_style == Some('[') {
                // Check if this is a standalone bracketed identifier
                // (not followed by a dot, which would make it part of a multi-part name)
                let is_standalone = {
                    // Look ahead for Period token (skip whitespace)
                    let mut j = i + 1;
                    while j < len {
                        match &tokens[j].token {
                            Token::Whitespace(_) => j += 1,
                            Token::Period => break,
                            _ => break,
                        }
                    }
                    // If followed by period, it's not standalone
                    j >= len || !matches!(&tokens[j].token, Token::Period)
                };

                // Also check if this is preceded by a dot (meaning it's the second/third part)
                let not_preceded_by_dot = {
                    if i == 0 {
                        true
                    } else {
                        // Look back for Period token (skip whitespace)
                        let mut j = i as isize - 1;
                        while j >= 0 {
                            match &tokens[j as usize].token {
                                Token::Whitespace(_) => j -= 1,
                                Token::Period => break,
                                _ => break,
                            }
                        }
                        j < 0 || !matches!(&tokens[j as usize].token, Token::Period)
                    }
                };

                if is_standalone && not_preceded_by_dot {
                    // Convert (line, column) to byte offset
                    let location = &token.span.start;
                    let byte_pos =
                        location_to_byte_offset(&line_offsets, location.line, location.column);
                    results.push(BracketedIdentWithPos {
                        name: w.value.clone(),
                        position: byte_pos,
                    });
                }
            }
        }

        i += 1;
    }

    results
}

/// Compute byte offsets for each line in the source text.
/// Returns a vector where index i contains the byte offset where line (i+1) starts.
fn compute_line_offsets(sql: &str) -> Vec<usize> {
    let mut offsets = vec![0]; // Line 1 starts at offset 0
    for (i, ch) in sql.char_indices() {
        if ch == '\n' {
            // Next line starts after this newline
            offsets.push(i + 1);
        }
    }
    offsets
}

/// Convert a (1-based line, 1-based column) Location to a byte offset.
fn location_to_byte_offset(line_offsets: &[usize], line: u64, column: u64) -> usize {
    if line == 0 || line as usize > line_offsets.len() {
        return 0;
    }
    let line_start = line_offsets[(line - 1) as usize];
    // Column is 1-based, so subtract 1 to get offset within line
    line_start + (column.saturating_sub(1) as usize)
}

/// Strip SQL comments from body text for dependency extraction.
/// Removes both line comments (-- ...) and block comments (/* ... */).
/// This prevents words in comments from being treated as column/table references.
fn strip_sql_comments_for_body_deps(body: &str) -> String {
    let mut result = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    let mut in_string = false;
    let mut string_delimiter = ' ';

    while let Some(c) = chars.next() {
        // Handle string literals - don't strip comments inside strings
        if (c == '\'' || c == '"') && !in_string {
            in_string = true;
            string_delimiter = c;
            result.push(c);
            continue;
        }
        if c == string_delimiter && in_string {
            in_string = false;
            result.push(c);
            continue;
        }
        if in_string {
            result.push(c);
            continue;
        }

        // Check for line comment: --
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next(); // consume second -
                          // Skip until end of line
            while let Some(&ch) = chars.peek() {
                chars.next();
                if ch == '\n' {
                    result.push('\n'); // preserve line structure
                    break;
                }
            }
            continue;
        }

        // Check for block comment: /* ... */
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume *
                          // Skip until */
            while let Some(ch) = chars.next() {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next(); // consume /
                    result.push(' '); // replace comment with space to preserve word boundaries
                    break;
                }
            }
            continue;
        }

        result.push(c);
    }

    result
}

/// Extract column aliases from SELECT expressions (expr AS alias patterns).
/// These are output column names that should not be treated as column references.
fn extract_column_aliases_for_body_deps(
    body: &str,
    column_aliases: &mut std::collections::HashSet<String>,
) {
    // Use tokenizer-based extraction (replaces COLUMN_ALIAS_RE regex)
    for alias in extract_column_aliases_tokenized(body) {
        column_aliases.insert(alias);
    }
}

/// Check if a word is a SQL keyword (to filter out from column detection)
fn is_sql_keyword(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "FROM"
            | "WHERE"
            | "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "IS"
            | "IN"
            | "AS"
            | "ON"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "INNER"
            | "OUTER"
            | "CROSS"
            | "FULL"
            | "INSERT"
            | "INTO"
            | "VALUES"
            | "UPDATE"
            | "SET"
            | "DELETE"
            | "CREATE"
            | "ALTER"
            | "DROP"
            | "TABLE"
            | "VIEW"
            | "INDEX"
            | "PROCEDURE"
            | "FUNCTION"
            | "TRIGGER"
            | "BEGIN"
            | "END"
            | "IF"
            | "ELSE"
            | "WHILE"
            | "RETURN"
            | "DECLARE"
            | "INT"
            | "VARCHAR"
            | "NVARCHAR"
            | "CHAR"
            | "NCHAR"
            | "TEXT"
            | "NTEXT"
            | "BIT"
            | "TINYINT"
            | "SMALLINT"
            | "BIGINT"
            | "DECIMAL"
            | "NUMERIC"
            | "FLOAT"
            | "REAL"
            | "MONEY"
            | "SMALLMONEY"
            | "DATE"
            | "TIME"
            | "DATETIME"
            | "DATETIME2"
            | "SMALLDATETIME"
            | "DATETIMEOFFSET"
            | "UNIQUEIDENTIFIER"
            | "BINARY"
            | "VARBINARY"
            | "IMAGE"
            | "XML"
            | "SQL_VARIANT"
            | "TIMESTAMP"
            | "ROWVERSION"
            | "GEOGRAPHY"
            | "GEOMETRY"
            | "HIERARCHYID"
            | "PRIMARY"
            | "KEY"
            | "FOREIGN"
            | "REFERENCES"
            | "UNIQUE"
            | "CHECK"
            | "DEFAULT"
            | "CONSTRAINT"
            | "IDENTITY"
            | "NOCOUNT"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "ISNULL"
            | "COALESCE"
            | "CAST"
            | "CONVERT"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "EXEC"
            | "EXECUTE"
            | "GO"
            | "USE"
            | "DATABASE"
            | "SCHEMA"
            | "GRANT"
            | "REVOKE"
            | "DENY"
            | "ORDER"
            | "BY"
            | "GROUP"
            | "HAVING"
            | "DISTINCT"
            | "TOP"
            | "OFFSET"
            | "FETCH"
            | "NEXT"
            | "ROWS"
            | "ONLY"
            | "UNION"
            | "ALL"
            | "EXCEPT"
            | "INTERSECT"
            | "EXISTS"
            | "ANY"
            | "SOME"
            | "LIKE"
            | "BETWEEN"
            | "ASC"
            | "DESC"
            | "CLUSTERED"
            | "NONCLUSTERED"
            | "OUTPUT"
            | "SCOPE_IDENTITY"
    )
}

/// Check if a word is a SQL keyword that should be filtered from column detection in procedure bodies.
/// This is a more permissive filter than `is_sql_keyword` - it allows words that are commonly
/// used as column names (like TIMESTAMP, ACTION, ID, etc.) even though they're also SQL keywords/types.
fn is_sql_keyword_not_column(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "FROM"
            | "WHERE"
            | "AND"
            | "OR"
            | "NOT"
            | "NULL"
            | "IS"
            | "IN"
            | "AS"
            | "ON"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "INNER"
            | "OUTER"
            | "CROSS"
            | "FULL"
            | "INSERT"
            | "INTO"
            | "VALUES"
            | "UPDATE"
            | "SET"
            | "DELETE"
            | "CREATE"
            | "ALTER"
            | "DROP"
            | "TABLE"
            | "VIEW"
            | "INDEX"
            | "PROCEDURE"
            | "FUNCTION"
            | "TRIGGER"
            | "BEGIN"
            | "END"
            | "IF"
            | "ELSE"
            | "WHILE"
            | "RETURN"
            | "DECLARE"
            | "PRIMARY"
            | "KEY"
            | "FOREIGN"
            | "REFERENCES"
            | "UNIQUE"
            | "CHECK"
            | "DEFAULT"
            | "CONSTRAINT"
            | "IDENTITY"
            | "NOCOUNT"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "ISNULL"
            | "COALESCE"
            | "CAST"
            | "CONVERT"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "EXEC"
            | "EXECUTE"
            | "GO"
            | "USE"
            | "DATABASE"
            | "SCHEMA"
            | "GRANT"
            | "REVOKE"
            | "DENY"
            | "ORDER"
            | "BY"
            | "GROUP"
            | "HAVING"
            | "DISTINCT"
            | "TOP"
            | "OFFSET"
            | "FETCH"
            | "NEXT"
            | "ROWS"
            | "ONLY"
            | "UNION"
            | "ALL"
            | "EXCEPT"
            | "INTERSECT"
            | "EXISTS"
            | "ANY"
            | "SOME"
            | "LIKE"
            | "BETWEEN"
            | "ASC"
            | "DESC"
            | "CLUSTERED"
            | "NONCLUSTERED"
            | "OUTPUT"
            | "SCOPE_IDENTITY"
            // Core data types that are rarely used as column names
            | "INT"
            | "INTEGER"
            | "VARCHAR"
            | "NVARCHAR"
            | "CHAR"
            | "NCHAR"
            | "BIT"
            | "TINYINT"
            | "SMALLINT"
            | "BIGINT"
            | "DECIMAL"
            | "NUMERIC"
            | "FLOAT"
            | "REAL"
            | "MONEY"
            | "SMALLMONEY"
            | "DATETIME"
            | "DATETIME2"
            | "SMALLDATETIME"
            | "DATETIMEOFFSET"
            | "UNIQUEIDENTIFIER"
            | "BINARY"
            | "VARBINARY"
            | "XML"
            | "SQL_VARIANT"
            | "ROWVERSION"
            | "GEOGRAPHY"
            | "GEOMETRY"
            | "HIERARCHYID"
            | "NTEXT"
            // SQL Server specific functions and keywords commonly found in queries
            | "STUFF"
            | "FOR"
            | "PATH"
            | "STRING_AGG"
            | "CONCAT"
            | "LEN"
            | "CHARINDEX"
            | "SUBSTRING"
            | "REPLACE"
            | "LTRIM"
            | "RTRIM"
            | "TRIM"
            | "UPPER"
            | "LOWER"
            | "GETDATE"
            | "GETUTCDATE"
            | "DATEADD"
            | "DATEDIFF"
            | "DATENAME"
            | "DATEPART"
            | "YEAR"
            | "MONTH"
            | "DAY"
            | "HOUR"
            | "MINUTE"
            | "SECOND"
            | "APPLY"
            | "WITH"
    )
    // Intentionally excludes: TIMESTAMP, ACTION, ID, TEXT, IMAGE, DATE, TIME, etc.
    // as these are commonly used as column names
}

/// Extract column references from a CHECK constraint expression.
///
/// CHECK expressions reference columns by their unqualified names (e.g., `[Price] >= 0`).
/// This function extracts those column names and returns them as fully-qualified references
/// in the format `[schema].[table].[column]`.
///
/// DotNet emits these as the `CheckExpressionDependencies` relationship.
fn extract_check_expression_columns(
    expression: &str,
    table_schema: &str,
    table_name: &str,
) -> Vec<String> {
    extract_expression_column_references(expression, table_schema, table_name)
}

/// Extract column references from a filtered index predicate.
///
/// Filter predicates reference columns by their unqualified names
/// (e.g., `[DeletedAt] IS NULL` or `[Status] = N'Pending' AND [IsActive] = 1`).
/// This function extracts those column names and returns them as fully-qualified references.
///
/// DotNet emits these as the `BodyDependencies` relationship for filtered indexes.
fn extract_filter_predicate_columns(predicate: &str, table_ref: &str) -> Vec<String> {
    use std::collections::HashSet;
    let mut columns = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Use token-based extraction for single bracketed identifiers [ColumnName]
    // This replaces BRACKETED_IDENT_RE for better whitespace and comment handling
    for ident in extract_bracketed_identifiers_tokenized(predicate) {
        let upper_name = ident.name.to_uppercase();

        // Skip SQL keywords
        if is_sql_keyword(&upper_name) {
            continue;
        }

        // Build fully-qualified column reference using provided table_ref
        // table_ref is in format "[schema].[table]"
        let col_ref = format!("{}.[{}]", table_ref, ident.name);

        // Only add each column once, but preserve order of first appearance
        if !seen.contains(&col_ref) {
            seen.insert(col_ref.clone());
            columns.push(col_ref);
        }
    }

    columns
}

/// Extract column references from a computed column expression.
///
/// Computed column expressions reference columns by their unqualified names
/// (e.g., `[Quantity] * [UnitPrice]`). This function extracts those column names
/// and returns them as fully-qualified references in the format `[schema].[table].[column]`.
///
/// DotNet emits these as the `ExpressionDependencies` relationship.
pub(crate) fn extract_computed_expression_columns(
    expression: &str,
    table_schema: &str,
    table_name: &str,
) -> Vec<String> {
    extract_expression_column_references(expression, table_schema, table_name)
}

/// Extract column references and type references from an expression.
///
/// Expressions reference columns by their unqualified names (e.g., `[ColumnName]`).
/// This function extracts those column names and returns them as fully-qualified references
/// in the format `[schema].[table].[column]`.
///
/// Additionally, CAST expressions emit type references (e.g., `[nvarchar]`) to match
/// DotNet DacFx behavior.
///
/// Used by both CHECK constraints and computed columns.
fn extract_expression_column_references(
    expression: &str,
    table_schema: &str,
    table_name: &str,
) -> Vec<String> {
    use std::collections::HashSet;
    let mut refs = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Process expression to preserve order of appearance
    // We need to track positions where each reference appears
    let mut position_refs: Vec<(usize, String)> = Vec::new();

    // Track CAST ranges so we can skip column references inside CAST expressions
    // (they'll be processed separately after the CAST type ref)
    // Uses token-based extraction (Phase 20.3.3) for better whitespace handling
    let mut cast_ranges: Vec<(usize, usize, usize)> = Vec::new(); // (cast_start, cast_end, type_pos)
    for cast_info in extract_cast_expressions_tokenized(expression) {
        // Emit type reference at the CAST keyword position (before inner column refs)
        let type_ref = format!("[{}]", cast_info.type_name);
        position_refs.push((cast_info.cast_keyword_pos, type_ref));
        cast_ranges.push((
            cast_info.cast_start,
            cast_info.cast_end,
            cast_info.cast_keyword_pos,
        ));
    }

    // Collect column references with their positions using token-based extraction
    // This replaces BRACKETED_IDENT_RE for better whitespace and comment handling
    for ident in extract_bracketed_identifiers_tokenized(expression) {
        let upper_name = ident.name.to_uppercase();

        // Skip SQL keywords
        if is_sql_keyword(&upper_name) {
            continue;
        }

        // Build fully-qualified column reference
        let col_ref = format!("[{}].[{}].[{}]", table_schema, table_name, ident.name);
        let pos = ident.position;

        // For columns inside a CAST, adjust position to appear after the type
        // This matches DotNet's behavior: CAST type first, then inner columns
        let adjusted_pos = cast_ranges
            .iter()
            .find(|(start, end, _)| pos >= *start && pos < *end)
            .map(|(_, _, type_pos)| type_pos + 1)
            .unwrap_or(pos);

        position_refs.push((adjusted_pos, col_ref));
    }

    // Sort by position to maintain order of appearance in expression
    // Use stable sort to preserve original order when positions are equal
    position_refs.sort_by_key(|(pos, _)| *pos);

    // Add references in order, deduplicating
    for (_, ref_str) in position_refs {
        if !seen.contains(&ref_str) {
            seen.insert(ref_str.clone());
            refs.push(ref_str);
        }
    }

    refs
}

/// Write BodyDependencies relationship for procedures and functions
fn write_body_dependencies<W: Write>(
    writer: &mut Writer<W>,
    deps: &[BodyDependency],
) -> anyhow::Result<()> {
    if deps.is_empty() {
        return Ok(());
    }

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "BodyDependencies")]);
    writer.write_event(Event::Start(rel))?;

    for dep in deps {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        match dep {
            BodyDependency::BuiltInType(type_ref) => {
                let refs = BytesStart::new("References")
                    .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref.as_str())]);
                writer.write_event(Event::Empty(refs))?;
            }
            BodyDependency::ObjectRef(obj_ref) => {
                let refs =
                    BytesStart::new("References").with_attributes([("Name", obj_ref.as_str())]);
                writer.write_event(Event::Empty(refs))?;
            }
            BodyDependency::TvpParameter(param_ref, disambiguator) => {
                let disamb_str = disambiguator.to_string();
                let refs = BytesStart::new("References").with_attributes([
                    ("Name", param_ref.as_str()),
                    ("Disambiguator", disamb_str.as_str()),
                ]);
                writer.write_event(Event::Empty(refs))?;
            }
        }

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Extract just the body after AS from a procedure definition using token-based parsing.
fn extract_procedure_body_only(definition: &str) -> String {
    // Use tokenized parsing to find the AS keyword that separates header from body
    if let Some((_as_start, as_end)) = find_procedure_body_separator_as_tokenized(definition) {
        // as_end points to the first token after AS (after any whitespace)
        // Return the body starting from that position
        return definition[as_end..].to_string();
    }
    definition.to_string()
}

/// Find the AS keyword that separates procedure header from body using token-based parsing.
///
/// This function tokenizes the input string and looks for the AS keyword that is followed
/// by BEGIN, SET, SELECT, or other body-starting statements. Unlike `find_function_body_as_tokenized`,
/// this scans from the beginning of the definition (procedures don't have a RETURNS keyword).
///
/// # Arguments
/// * `definition` - The full CREATE PROCEDURE definition
///
/// # Returns
/// * `Some((as_start, as_end))` - The start and end byte positions of the AS keyword
///   where `as_end` includes any trailing whitespace after AS
/// * `None` - If no valid body separator AS was found
fn find_procedure_body_separator_as_tokenized(definition: &str) -> Option<(usize, usize)> {
    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, definition).tokenize_with_location() else {
        return None;
    };

    if tokens.is_empty() {
        return None;
    }

    // Build line offset map for byte position calculation
    let line_offsets = compute_line_offsets(definition);

    let len = tokens.len();
    let mut i = 0;

    // Helper to skip whitespace tokens, returns the next non-whitespace index
    let skip_whitespace =
        |tokens: &[sqlparser::tokenizer::TokenWithSpan], mut idx: usize| -> usize {
            while idx < tokens.len() && matches!(&tokens[idx].token, Token::Whitespace(_)) {
                idx += 1;
            }
            idx
        };

    // Keywords that can start a procedure body after AS
    let body_start_keywords = [
        "BEGIN", "RETURN", "SET", "SELECT", "IF", "WHILE", "DECLARE", "WITH", "INSERT", "UPDATE",
        "DELETE", "EXEC", "EXECUTE",
    ];

    while i < len {
        // Look for AS keyword (unquoted word)
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("AS") {
                // Calculate byte position of AS keyword
                let as_byte_start = location_to_byte_offset(
                    &line_offsets,
                    tokens[i].span.start.line,
                    tokens[i].span.start.column,
                );

                // Look at what comes after AS
                let j = skip_whitespace(&tokens, i + 1);

                // Calculate end position (after AS and any whitespace)
                let as_byte_end = if j < len {
                    location_to_byte_offset(
                        &line_offsets,
                        tokens[j].span.start.line,
                        tokens[j].span.start.column,
                    )
                } else {
                    // AS is at the end, end is after "AS" (2 chars)
                    as_byte_start + 2
                };

                // Check if followed by a body-starting keyword
                if j < len {
                    if let Token::Word(next_word) = &tokens[j].token {
                        if next_word.quote_style.is_none() {
                            let next_upper = next_word.value.to_uppercase();
                            if body_start_keywords.contains(&next_upper.as_str()) {
                                // This AS is the body separator
                                return Some((as_byte_start, as_byte_end));
                            }
                        }
                    }
                } else {
                    // AS is at the very end - still a valid body separator
                    return Some((as_byte_start, as_byte_end));
                }
            }
        }
        i += 1;
    }

    None
}

/// Find the AS keyword that separates function header from body using token-based parsing.
///
/// This function tokenizes the input string and looks for the AS keyword that appears
/// after RETURNS and is followed by BEGIN, RETURN, or other body-starting statements.
/// It returns the byte position where the AS token ends (after any trailing whitespace).
///
/// # Arguments
/// * `definition` - The full CREATE FUNCTION definition
/// * `after_returns_start` - The byte position where RETURNS was found
///
/// # Returns
/// * `Some((as_start, as_end))` - The start and end byte positions of the AS keyword
///   where `as_end` includes any trailing whitespace after AS
/// * `None` - If no valid body separator AS was found
fn find_function_body_as_tokenized(
    definition: &str,
    after_returns_start: usize,
) -> Option<(usize, usize)> {
    let after_returns = &definition[after_returns_start..];

    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, after_returns).tokenize_with_location() else {
        return None;
    };

    if tokens.is_empty() {
        return None;
    }

    // Build line offset map for byte position calculation
    let line_offsets = compute_line_offsets(after_returns);

    let len = tokens.len();
    let mut i = 0;

    // Helper to skip whitespace tokens, returns the next non-whitespace index
    let skip_whitespace =
        |tokens: &[sqlparser::tokenizer::TokenWithSpan], mut idx: usize| -> usize {
            while idx < tokens.len() && matches!(&tokens[idx].token, Token::Whitespace(_)) {
                idx += 1;
            }
            idx
        };

    // Keywords that can start a function body after AS
    let body_start_keywords = [
        "BEGIN", "RETURN", "SET", "SELECT", "IF", "WHILE", "DECLARE", "WITH", "INSERT", "UPDATE",
        "DELETE", "EXEC", "EXECUTE",
    ];

    while i < len {
        // Look for AS keyword (unquoted word)
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("AS") {
                // Calculate byte position of AS keyword
                let as_byte_start = location_to_byte_offset(
                    &line_offsets,
                    tokens[i].span.start.line,
                    tokens[i].span.start.column,
                );

                // Look at what comes after AS
                let j = skip_whitespace(&tokens, i + 1);

                // Calculate end position (after AS and any whitespace)
                let as_byte_end = if j < len {
                    location_to_byte_offset(
                        &line_offsets,
                        tokens[j].span.start.line,
                        tokens[j].span.start.column,
                    )
                } else {
                    // AS is at the end, end is after "AS" (2 chars)
                    as_byte_start + 2
                };

                // Check if followed by a body-starting keyword
                if j < len {
                    if let Token::Word(next_word) = &tokens[j].token {
                        if next_word.quote_style.is_none() {
                            let next_upper = next_word.value.to_uppercase();
                            if body_start_keywords.contains(&next_upper.as_str()) {
                                // This AS is the body separator
                                return Some((
                                    after_returns_start + as_byte_start,
                                    after_returns_start + as_byte_end,
                                ));
                            }
                        }
                    }
                } else {
                    // AS is at the very end - still a valid body separator
                    return Some((
                        after_returns_start + as_byte_start,
                        after_returns_start + as_byte_end,
                    ));
                }
            }
        }
        i += 1;
    }

    None
}

/// Extract all ON clause boundaries from a SQL query using token-based parsing.
///
/// This function tokenizes the input string and finds all ON keywords that introduce
/// JOIN conditions. It returns a vector of (start, end) byte positions for each ON clause,
/// where start is the position after "ON " and end is where the clause terminates.
///
/// # Arguments
/// * `query` - The SQL query text
///
/// # Returns
/// * `Vec<(usize, usize)>` - Vector of (start, end) byte positions for ON clause content
///
/// # Terminating Keywords
/// ON clauses terminate at: WHERE, GROUP, ORDER, HAVING, UNION, INNER, LEFT, RIGHT,
/// OUTER, CROSS, JOIN, or semicolon.
fn extract_on_clause_boundaries_tokenized(query: &str) -> Vec<(usize, usize)> {
    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, query).tokenize_with_location() else {
        return Vec::new();
    };

    if tokens.is_empty() {
        return Vec::new();
    }

    let line_offsets = compute_line_offsets(query);
    let mut results = Vec::new();
    let len = tokens.len();
    let mut i = 0;

    // Helper to skip whitespace tokens
    let skip_whitespace =
        |tokens: &[sqlparser::tokenizer::TokenWithSpan], mut idx: usize| -> usize {
            while idx < tokens.len() && matches!(&tokens[idx].token, Token::Whitespace(_)) {
                idx += 1;
            }
            idx
        };

    // Keywords that terminate an ON clause
    let terminator_keywords = [
        "WHERE", "GROUP", "ORDER", "HAVING", "UNION", "INNER", "LEFT", "RIGHT", "OUTER", "CROSS",
        "JOIN",
    ];

    while i < len {
        // Look for ON keyword (unquoted word)
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("ON") {
                // Skip the ON keyword and any whitespace after it
                let j = skip_whitespace(&tokens, i + 1);

                // Calculate start position (after ON and whitespace)
                let clause_start = if j < len {
                    location_to_byte_offset(
                        &line_offsets,
                        tokens[j].span.start.line,
                        tokens[j].span.start.column,
                    )
                } else {
                    // ON is at the end
                    query.len()
                };

                // Find where the ON clause ends
                let mut k = j;
                let mut clause_end = query.len();
                while k < len {
                    match &tokens[k].token {
                        // Check for terminator keywords
                        Token::Word(word) => {
                            if word.quote_style.is_none() {
                                let upper = word.value.to_uppercase();
                                if terminator_keywords.contains(&upper.as_str()) {
                                    clause_end = location_to_byte_offset(
                                        &line_offsets,
                                        tokens[k].span.start.line,
                                        tokens[k].span.start.column,
                                    );
                                    break;
                                }
                            }
                        }
                        // Check for semicolon
                        Token::SemiColon => {
                            clause_end = location_to_byte_offset(
                                &line_offsets,
                                tokens[k].span.start.line,
                                tokens[k].span.start.column,
                            );
                            break;
                        }
                        _ => {}
                    }
                    k += 1;
                }

                if clause_start < clause_end {
                    results.push((clause_start, clause_end));
                }

                // Move past the ON clause we just found
                i = k;
                continue;
            }
        }
        i += 1;
    }

    results
}

/// Extract GROUP BY clause boundaries from a SQL query using token-based parsing.
///
/// Phase 20.5.5-20.5.6: Replaces GROUP_BY_RE and GROUP_TERMINATOR_RE regex patterns.
///
/// # Arguments
/// * `query` - The SQL query text
///
/// # Returns
/// * `Option<(usize, usize)>` - The (start, end) byte positions of the GROUP BY clause content
///   (excluding the "GROUP BY" keywords themselves), or None if no GROUP BY clause found
///
/// # Terminating Keywords
/// GROUP BY clauses terminate at: HAVING, ORDER, UNION, or semicolon.
fn extract_group_by_clause_boundaries_tokenized(query: &str) -> Option<(usize, usize)> {
    let dialect = MsSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, query).tokenize_with_location() else {
        return None;
    };

    if tokens.is_empty() {
        return None;
    }

    let line_offsets = compute_line_offsets(query);
    let len = tokens.len();
    let mut i = 0;

    // Helper to skip whitespace tokens
    let skip_whitespace =
        |tokens: &[sqlparser::tokenizer::TokenWithSpan], mut idx: usize| -> usize {
            while idx < tokens.len() && matches!(&tokens[idx].token, Token::Whitespace(_)) {
                idx += 1;
            }
            idx
        };

    // Keywords that terminate a GROUP BY clause
    let terminator_keywords = ["HAVING", "ORDER", "UNION"];

    while i < len {
        // Look for GROUP keyword (unquoted word)
        if let Token::Word(w) = &tokens[i].token {
            if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("GROUP") {
                // Skip whitespace after GROUP
                let j = skip_whitespace(&tokens, i + 1);

                // Check for BY keyword
                if j < len {
                    if let Token::Word(by_word) = &tokens[j].token {
                        if by_word.quote_style.is_none() && by_word.value.eq_ignore_ascii_case("BY")
                        {
                            // Found GROUP BY - skip whitespace after BY
                            let k = skip_whitespace(&tokens, j + 1);

                            // Calculate start position (after GROUP BY and whitespace)
                            let clause_start = if k < len {
                                location_to_byte_offset(
                                    &line_offsets,
                                    tokens[k].span.start.line,
                                    tokens[k].span.start.column,
                                )
                            } else {
                                // GROUP BY is at the end
                                return None;
                            };

                            // Find where the GROUP BY clause ends
                            let mut m = k;
                            let mut clause_end = query.len();
                            while m < len {
                                match &tokens[m].token {
                                    // Check for terminator keywords
                                    Token::Word(word) => {
                                        if word.quote_style.is_none() {
                                            let upper = word.value.to_uppercase();
                                            if terminator_keywords.contains(&upper.as_str()) {
                                                clause_end = location_to_byte_offset(
                                                    &line_offsets,
                                                    tokens[m].span.start.line,
                                                    tokens[m].span.start.column,
                                                );
                                                break;
                                            }
                                        }
                                    }
                                    // Check for semicolon
                                    Token::SemiColon => {
                                        clause_end = location_to_byte_offset(
                                            &line_offsets,
                                            tokens[m].span.start.line,
                                            tokens[m].span.start.column,
                                        );
                                        break;
                                    }
                                    _ => {}
                                }
                                m += 1;
                            }

                            if clause_start < clause_end {
                                return Some((clause_start, clause_end));
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }

    None
}

/// Write the data type relationship for a parameter with inline type specifier
fn write_data_type_relationship<W: Write>(
    writer: &mut Writer<W>,
    data_type: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Parse the data type and write an inline SqlTypeSpecifier element
    let (base_type, length, precision, scale) = parse_data_type(data_type);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
    writer.write_event(Event::Start(elem))?;

    // Write length/precision/scale if applicable
    if let Some(len) = length {
        if len == -1 {
            write_property(writer, "IsMax", "True")?;
        } else {
            write_property(writer, "Length", &len.to_string())?;
        }
    }
    if let Some(prec) = precision {
        write_property(writer, "Precision", &prec.to_string())?;
    }
    if let Some(sc) = scale {
        write_property(writer, "Scale", &sc.to_string())?;
    }

    // Write the base type as a reference
    let type_ref = format!("[{}]", base_type.to_lowercase());
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let type_rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(type_rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let refs = BytesStart::new("References")
        .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref.as_str())]);
    writer.write_event(Event::Empty(refs))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Parse a SQL data type into (base_type, length, precision, scale)
pub(crate) fn parse_data_type(data_type: &str) -> (String, Option<i32>, Option<i32>, Option<i32>) {
    let dt_upper = data_type.to_uppercase().trim().to_string();

    // Handle types with parameters like VARCHAR(50), DECIMAL(10,2), VARCHAR(MAX)
    if let Some(paren_pos) = dt_upper.find('(') {
        let base_type = dt_upper[..paren_pos].to_string();
        let params_end = dt_upper.rfind(')').unwrap_or(dt_upper.len());
        let params = &dt_upper[paren_pos + 1..params_end];

        // Check for MAX
        if params.trim().eq_ignore_ascii_case("MAX") {
            return (base_type, Some(-1), None, None);
        }

        // Parse numeric parameters
        let parts: Vec<&str> = params.split(',').collect();
        if parts.len() == 1 {
            // Single parameter (length or precision)
            let val: i32 = parts[0].trim().parse().unwrap_or(0);
            match base_type.as_str() {
                "DECIMAL" | "NUMERIC" => (base_type, None, Some(val), Some(0)),
                _ => (base_type, Some(val), None, None),
            }
        } else if parts.len() == 2 {
            // Two parameters (precision, scale)
            let prec: i32 = parts[0].trim().parse().unwrap_or(0);
            let scale: i32 = parts[1].trim().parse().unwrap_or(0);
            (base_type, None, Some(prec), Some(scale))
        } else {
            (base_type, None, None, None)
        }
    } else {
        (dt_upper, None, None, None)
    }
}

fn write_function<W: Write>(
    writer: &mut Writer<W>,
    func: &FunctionElement,
    model: &DatabaseModel,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", func.schema, func.name);
    let type_name = match func.function_type {
        crate::model::FunctionType::Scalar => "SqlScalarFunction",
        crate::model::FunctionType::TableValued => "SqlMultiStatementTableValuedFunction",
        crate::model::FunctionType::InlineTableValued => "SqlInlineTableValuedFunction",
    };

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", type_name), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Write IsAnsiNullsOn property (always true for functions)
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Write IsNativelyCompiled property if true
    if func.is_natively_compiled {
        write_property(writer, "IsNativelyCompiled", "True")?;
    }

    // Extract function body for dependency analysis
    let body = extract_function_body(&func.definition);
    let header = extract_function_header(&func.definition);

    // Extract function parameters for dependency analysis
    let func_params = extract_function_parameters(&func.definition);
    let param_names: Vec<String> = func_params.iter().map(|p| p.name.clone()).collect();

    // Extract and write BodyDependencies
    let body_deps = extract_body_dependencies(&body, &full_name, &param_names);
    write_body_dependencies(writer, &body_deps)?;

    // For inline TVFs, write Columns relationship (after BodyDependencies, before FunctionBody)
    if matches!(
        func.function_type,
        crate::model::FunctionType::InlineTableValued
    ) {
        let inline_tvf_columns = extract_inline_tvf_columns(&body, &full_name, &func.schema, model);
        if !inline_tvf_columns.is_empty() {
            write_view_columns(writer, &full_name, &inline_tvf_columns)?;
        }
    }

    // For multi-statement TVFs, write Columns relationship from RETURNS @Table TABLE definition
    if matches!(func.function_type, crate::model::FunctionType::TableValued) {
        let tvf_columns = extract_multi_statement_tvf_columns(&func.definition);
        if !tvf_columns.is_empty() {
            write_tvf_columns(writer, &full_name, &tvf_columns)?;
        }
    }

    // Write FunctionBody relationship with SqlScriptFunctionImplementation
    // BodyScript contains only the function body (BEGIN...END), not the header
    write_function_body_with_annotation(writer, &body, &header)?;

    // Write Parameters relationship for function parameters
    write_function_parameters(writer, &func_params, &full_name)?;

    write_schema_relationship(writer, &func.schema)?;

    // Write Type relationship for return type (scalar functions only)
    if matches!(func.function_type, crate::model::FunctionType::Scalar) {
        if let Some(ref return_type) = func.return_type {
            write_function_return_type(writer, return_type)?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write FunctionBody relationship for functions with nested SqlScriptFunctionImplementation
/// Includes SysCommentsObjectAnnotation with HeaderContents for DacFx compatibility
fn write_function_body_with_annotation<W: Write>(
    writer: &mut Writer<W>,
    body: &str,
    header: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "FunctionBody")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem =
        BytesStart::new("Element").with_attributes([("Type", "SqlScriptFunctionImplementation")]);
    writer.write_event(Event::Start(elem))?;

    // Write BodyScript property with the function body only (BEGIN...END)
    write_script_property(writer, "BodyScript", body)?;

    // Write SysCommentsObjectAnnotation with HeaderContents
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let annotation =
        BytesStart::new("Annotation").with_attributes([("Type", "SysCommentsObjectAnnotation")]);
    writer.write_event(Event::Start(annotation))?;

    // Calculate length (header + body)
    let total_length = header.len() + body.len();
    write_property(writer, "Length", &total_length.to_string())?;
    write_property(writer, "StartLine", "1")?;
    write_property(writer, "StartColumn", "1")?;

    // Write HeaderContents with XML-escaped header
    write_property(writer, "HeaderContents", header)?;

    writer.write_event(Event::End(BytesEnd::new("Annotation")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Write Type relationship for scalar function return type
/// Format: <Relationship Name="Type"><Entry><Element Type="SqlTypeSpecifier">
///           <Relationship Name="Type"><Entry><References ExternalSource="BuiltIns" Name="[type]"/></Entry></Relationship>
///         </Element></Entry></Relationship>
fn write_function_return_type<W: Write>(
    writer: &mut Writer<W>,
    return_type: &str,
) -> anyhow::Result<()> {
    // Extract base type name (e.g., "INT" -> "int", "DECIMAL(18,2)" -> "decimal")
    let base_type = extract_base_type_name(return_type);
    let type_ref = format!("[{}]", base_type.to_lowercase());

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlTypeSpecifier")]);
    writer.write_event(Event::Start(elem))?;

    // Nested Type relationship referencing the built-in type
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let inner_rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(inner_rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let refs = BytesStart::new("References")
        .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref.as_str())]);
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    Ok(())
}

/// Extract base type name from a type specification
/// e.g., "DECIMAL(18,2)" -> "decimal", "VARCHAR(100)" -> "varchar", "INT" -> "int"
fn extract_base_type_name(type_spec: &str) -> String {
    let type_upper = type_spec.trim().to_uppercase();
    // Remove parentheses and everything after
    if let Some(paren_pos) = type_upper.find('(') {
        type_upper[..paren_pos].trim().to_lowercase()
    } else {
        type_upper.to_lowercase()
    }
}

/// Extract the body part from a CREATE FUNCTION definition
/// Returns just the body (BEGIN...END or RETURN(...)) without the header
///
/// Uses token-based parsing to find the AS keyword that separates header from body,
/// handling whitespace (tabs, spaces, newlines) correctly.
fn extract_function_body(definition: &str) -> String {
    let def_upper = definition.to_uppercase();

    // Find RETURNS and then AS after it
    // Pattern: CREATE FUNCTION [name](...) RETURNS type AS BEGIN ... END
    if let Some(returns_pos) = def_upper.find("RETURNS") {
        // Use token-based parsing to find the AS keyword
        if let Some((_as_start, as_end)) = find_function_body_as_tokenized(definition, returns_pos)
        {
            // Return everything after AS (as_end is after AS and any trailing whitespace)
            return definition[as_end..].trim().to_string();
        }
    }

    // Fallback: return the original definition
    definition.to_string()
}

/// Extract the header part from a CREATE FUNCTION definition
/// Returns everything up to and including AS (CREATE FUNCTION [name](...) RETURNS type AS\n)
/// Preserves trailing whitespace after AS to ensure proper separation from body
///
/// Uses token-based parsing to find the AS keyword that separates header from body,
/// handling whitespace (tabs, spaces, newlines) correctly.
fn extract_function_header(definition: &str) -> String {
    let def_upper = definition.to_uppercase();

    // Find RETURNS and then AS after it
    if let Some(returns_pos) = def_upper.find("RETURNS") {
        // Use token-based parsing to find the AS keyword
        if let Some((_as_start, as_end)) = find_function_body_as_tokenized(definition, returns_pos)
        {
            // Return everything up to and including AS with trailing whitespace
            // Use trim_start() to only remove leading whitespace, preserving trailing newline
            return definition[..as_end].trim_start().to_string();
        }
    }

    // Fallback: return empty string
    String::new()
}

fn write_index<W: Write>(writer: &mut Writer<W>, index: &IndexElement) -> anyhow::Result<()> {
    let full_name = format!(
        "[{}].[{}].[{}]",
        index.table_schema, index.table_name, index.name
    );

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlIndex"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    if index.is_unique {
        write_property(writer, "IsUnique", "True")?;
    }

    if index.is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    if let Some(fill_factor) = index.fill_factor {
        write_property(writer, "FillFactor", &fill_factor.to_string())?;
    }

    // Write FilterPredicate property for filtered indexes (before relationships)
    // DotNet emits this as a CDATA script property
    if let Some(ref filter_predicate) = index.filter_predicate {
        write_script_property(writer, "FilterPredicate", filter_predicate)?;
    }

    // Reference to table
    let table_ref = format!("[{}].[{}]", index.table_schema, index.table_name);

    // Write BodyDependencies for filtered indexes (column references from filter predicate)
    // DotNet emits this before ColumnSpecifications
    if let Some(ref filter_predicate) = index.filter_predicate {
        let body_deps = extract_filter_predicate_columns(filter_predicate, &table_ref);
        if !body_deps.is_empty() {
            let body_deps: Vec<BodyDependency> = body_deps
                .into_iter()
                .map(BodyDependency::ObjectRef)
                .collect();
            write_body_dependencies(writer, &body_deps)?;
        }
    }

    // Write ColumnSpecifications for key columns
    if !index.columns.is_empty() {
        write_index_column_specifications(writer, index, &table_ref)?;
    }

    // Write DataCompressionOptions relationship if index has compression
    if let Some(ref compression) = index.data_compression {
        write_data_compression_options(writer, compression)?;
    }

    // Write IncludedColumns relationship if present
    if !index.include_columns.is_empty() {
        let include_refs: Vec<String> = index
            .include_columns
            .iter()
            .map(|col| format!("{}.[{}]", table_ref, col))
            .collect();
        let include_refs: Vec<&str> = include_refs.iter().map(|s| s.as_str()).collect();
        write_relationship(writer, "IncludedColumns", &include_refs)?;
    }

    // IndexedObject relationship comes after ColumnSpecifications and IncludedColumns
    write_relationship(writer, "IndexedObject", &[&table_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_index_column_specifications<W: Write>(
    writer: &mut Writer<W>,
    index: &IndexElement,
    table_ref: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "ColumnSpecifications")]);
    writer.write_event(Event::Start(rel))?;

    for (i, col) in index.columns.iter().enumerate() {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let spec_name = format!(
            "[{}].[{}].[{}].[{}]",
            index.table_schema, index.table_name, index.name, i
        );

        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlIndexedColumnSpecification"),
            ("Name", spec_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        // Reference to the column
        let col_ref = format!("{}.[{}]", table_ref, col);
        write_relationship(writer, "Column", &[&col_ref])?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write DataCompressionOptions relationship for indexes with data compression
fn write_data_compression_options<W: Write>(
    writer: &mut Writer<W>,
    compression: &DataCompressionType,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "DataCompressionOptions")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlDataCompressionOption")]);
    writer.write_event(Event::Start(elem))?;

    // Write CompressionLevel property
    write_property(
        writer,
        "CompressionLevel",
        &compression.compression_level().to_string(),
    )?;

    // Write PartitionNumber property (always 1 for single-partition indexes)
    write_property(writer, "PartitionNumber", "1")?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

fn write_fulltext_index<W: Write>(
    writer: &mut Writer<W>,
    fulltext: &FullTextIndexElement,
) -> anyhow::Result<()> {
    // Full-text index name format: [schema].[table] (same as table name)
    let full_name = format!("[{}].[{}]", fulltext.table_schema, fulltext.table_name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    // Conditional Disambiguator attribute requires separate handling
    let elem = if let Some(disambiguator) = fulltext.disambiguator {
        let disamb_str = disambiguator.to_string();
        BytesStart::new("Element").with_attributes([
            ("Type", "SqlFullTextIndex"),
            ("Name", full_name.as_str()),
            ("Disambiguator", disamb_str.as_str()),
        ])
    } else {
        BytesStart::new("Element")
            .with_attributes([("Type", "SqlFullTextIndex"), ("Name", full_name.as_str())])
    };
    writer.write_event(Event::Start(elem))?;

    // Reference to full-text catalog if specified
    if let Some(catalog) = &fulltext.catalog {
        let catalog_ref = format!("[{}]", catalog);
        write_relationship(writer, "Catalog", &[&catalog_ref])?;
    }

    // Write Columns for full-text columns
    let table_ref = format!("[{}].[{}]", fulltext.table_schema, fulltext.table_name);
    if !fulltext.columns.is_empty() {
        write_fulltext_column_specifications(writer, fulltext, &table_ref)?;
    }

    // Reference to table (IndexedObject)
    write_relationship(writer, "IndexedObject", &[&table_ref])?;

    // Reference to the unique key index (KeyName)
    // Key reference format: [schema].[constraint_name]
    let key_index_ref = format!("[{}].[{}]", fulltext.table_schema, fulltext.key_index);
    write_relationship(writer, "KeyName", &[&key_index_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_fulltext_column_specifications<W: Write>(
    writer: &mut Writer<W>,
    fulltext: &FullTextIndexElement,
    table_ref: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in fulltext.columns.iter() {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // DotNet uses anonymous elements (no Name attribute) for column specifiers
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let elem = BytesStart::new("Element")
            .with_attributes([("Type", "SqlFullTextIndexColumnSpecifier")]);
        writer.write_event(Event::Start(elem))?;

        // Add LanguageId property if specified
        if let Some(lang_id) = col.language_id {
            write_property(writer, "LanguageId", &lang_id.to_string())?;
        }

        // Reference to the column
        let col_ref = format!("{}.[{}]", table_ref, col.name);
        write_relationship(writer, "Column", &[&col_ref])?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

fn write_fulltext_catalog<W: Write>(
    writer: &mut Writer<W>,
    catalog: &FullTextCatalogElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}]", catalog.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlFullTextCatalog"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Add IsDefault property if this is the default catalog
    if catalog.is_default {
        write_property(writer, "IsDefault", "True")?;
    }

    // Fulltext catalogs have an Authorizer relationship (defaults to dbo)
    write_authorizer_relationship(writer, "dbo")?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_constraint<W: Write>(
    writer: &mut Writer<W>,
    constraint: &ConstraintElement,
) -> anyhow::Result<()> {
    // DotNet uses two-part names for constraints: [schema].[constraint_name]
    // But inline constraints (without CONSTRAINT keyword) have no Name attribute
    let full_name = format!("[{}].[{}]", constraint.table_schema, constraint.name);

    let type_name = match constraint.constraint_type {
        ConstraintType::PrimaryKey => "SqlPrimaryKeyConstraint",
        ConstraintType::ForeignKey => "SqlForeignKeyConstraint",
        ConstraintType::Unique => "SqlUniqueConstraint",
        ConstraintType::Check => "SqlCheckConstraint",
        ConstraintType::Default => "SqlDefaultConstraint",
    };

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    // Conditional Name attribute requires separate handling
    let elem = if constraint.emit_name {
        BytesStart::new("Element")
            .with_attributes([("Type", type_name), ("Name", full_name.as_str())])
    } else {
        BytesStart::new("Element").with_attributes([("Type", type_name)])
    };
    writer.write_event(Event::Start(elem))?;

    // Write IsClustered property for primary keys and unique constraints
    // DotNet only emits IsClustered when it differs from the default:
    // - Primary Key: default is CLUSTERED, so only emit when NONCLUSTERED (False)
    // - Unique: default is NONCLUSTERED, so only emit when CLUSTERED (True)
    if let Some(is_clustered) = constraint.is_clustered {
        match constraint.constraint_type {
            ConstraintType::PrimaryKey if !is_clustered => {
                // PK is nonclustered (non-default), emit IsClustered=False
                write_property(writer, "IsClustered", "False")?;
            }
            ConstraintType::Unique if is_clustered => {
                // Unique is clustered (non-default), emit IsClustered=True
                write_property(writer, "IsClustered", "True")?;
            }
            _ => {
                // Default values - don't emit
            }
        }
    }

    // Reference to table
    let table_ref = format!("[{}].[{}]", constraint.table_schema, constraint.table_name);

    // Handle CHECK constraints with special ordering:
    // DotNet order for CHECK: CheckExpressionScript, CheckExpressionDependencies, DefiningTable
    if constraint.constraint_type == ConstraintType::Check {
        // Write CheckExpressionScript property first
        if let Some(ref definition) = constraint.definition {
            write_script_property(writer, "CheckExpressionScript", definition)?;

            // Extract and write CheckExpressionDependencies relationship
            let col_refs = extract_check_expression_columns(
                definition,
                &constraint.table_schema,
                &constraint.table_name,
            );
            if !col_refs.is_empty() {
                let col_refs_str: Vec<&str> = col_refs.iter().map(|s| s.as_str()).collect();
                write_relationship(writer, "CheckExpressionDependencies", &col_refs_str)?;
            }
        }

        // DefiningTable comes after CheckExpressionDependencies
        write_relationship(writer, "DefiningTable", &[&table_ref])?;
    } else {
        // Write column relationships and DefiningTable based on constraint type
        // DotNet ordering for foreign keys: Columns, DefiningTable, ForeignColumns, ForeignTable
        // DotNet ordering for PK/Unique: DefiningTable, ColumnSpecifications
        if !constraint.columns.is_empty() {
            match constraint.constraint_type {
                ConstraintType::PrimaryKey | ConstraintType::Unique => {
                    // PK/Unique: DefiningTable first, then ColumnSpecifications
                    write_relationship(writer, "DefiningTable", &[&table_ref])?;

                    // Primary keys and unique constraints use ColumnSpecifications with inline elements
                    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
                    let rel = BytesStart::new("Relationship")
                        .with_attributes([("Name", "ColumnSpecifications")]);
                    writer.write_event(Event::Start(rel))?;

                    for col in &constraint.columns {
                        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

                        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
                        let col_elem = BytesStart::new("Element")
                            .with_attributes([("Type", "SqlIndexedColumnSpecification")]);
                        writer.write_event(Event::Start(col_elem))?;

                        // Note: DacFx SqlIndexedColumnSpecification doesn't have a property for
                        // descending sort order - columns default to ascending. The sort direction
                        // is stored in the model for potential future use.

                        // Reference to the actual column
                        let col_ref = format!("{}.[{}]", table_ref, col.name);
                        write_relationship(writer, "Column", &[&col_ref])?;

                        writer.write_event(Event::End(BytesEnd::new("Element")))?;
                        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
                    }

                    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
                }
                ConstraintType::ForeignKey => {
                    // Foreign keys: Columns, DefiningTable, ForeignColumns, ForeignTable (DotNet order)
                    let column_refs: Vec<String> = constraint
                        .columns
                        .iter()
                        .map(|c| format!("{}.[{}]", table_ref, c.name))
                        .collect();
                    let column_refs_str: Vec<&str> =
                        column_refs.iter().map(|s| s.as_str()).collect();
                    write_relationship(writer, "Columns", &column_refs_str)?;

                    write_relationship(writer, "DefiningTable", &[&table_ref])?;

                    // Add ForeignColumns and ForeignTable relationships
                    if let Some(ref foreign_table) = constraint.referenced_table {
                        // ForeignColumns comes before ForeignTable in DotNet
                        if let Some(ref foreign_columns) = constraint.referenced_columns {
                            if !foreign_columns.is_empty() {
                                let foreign_col_refs: Vec<String> = foreign_columns
                                    .iter()
                                    .map(|c| format!("{}.[{}]", foreign_table, c))
                                    .collect();
                                let foreign_col_refs_str: Vec<&str> =
                                    foreign_col_refs.iter().map(|s| s.as_str()).collect();
                                write_relationship(
                                    writer,
                                    "ForeignColumns",
                                    &foreign_col_refs_str,
                                )?;
                            }
                        }
                        write_relationship(writer, "ForeignTable", &[foreign_table])?;
                    }
                }
                _ => {
                    // Other constraint types: DefiningTable only
                    write_relationship(writer, "DefiningTable", &[&table_ref])?;
                }
            }
        } else {
            // No columns - still write DefiningTable for constraints that need it
            write_relationship(writer, "DefiningTable", &[&table_ref])?;
        }
    }

    // Default constraint expression - handled separately since it has different structure
    if constraint.constraint_type == ConstraintType::Default {
        // Default constraint expression
        if let Some(ref definition) = constraint.definition {
            write_script_property(writer, "DefaultExpressionScript", definition)?;
        }
        // Default constraints need a ForColumn relationship to specify the target column
        if !constraint.columns.is_empty() {
            let table_ref = format!("[{}].[{}]", constraint.table_schema, constraint.table_name);
            let col_ref = format!("{}.[{}]", table_ref, constraint.columns[0].name);
            write_relationship(writer, "ForColumn", &[&col_ref])?;
        }
    }

    // Write annotation at the end of the constraint element
    // - Inline constraints: <Annotation Type="SqlInlineConstraintAnnotation" Disambiguator="X" />
    // - Named constraints: <AttachedAnnotation Disambiguator="X" /> (referencing table's disambiguator)
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    if let Some(disambiguator) = constraint.inline_constraint_disambiguator {
        let disamb_str = disambiguator.to_string();
        if constraint.is_inline {
            // Inline constraint gets its own SqlInlineConstraintAnnotation
            let annotation = BytesStart::new("Annotation").with_attributes([
                ("Type", "SqlInlineConstraintAnnotation"),
                ("Disambiguator", disamb_str.as_str()),
            ]);
            writer.write_event(Event::Empty(annotation))?;
        } else {
            // Named constraint references the table's disambiguator via AttachedAnnotation
            let annotation = BytesStart::new("AttachedAnnotation")
                .with_attributes([("Disambiguator", disamb_str.as_str())]);
            writer.write_event(Event::Empty(annotation))?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_sequence<W: Write>(writer: &mut Writer<W>, seq: &SequenceElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", seq.schema, seq.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlSequence"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Properties in DotNet order: IsCycling, HasNoMaxValue, HasNoMinValue, MinValue, MaxValue, Increment, StartValue
    if seq.is_cycling {
        write_property(writer, "IsCycling", "True")?;
    }

    // HasNoMaxValue and HasNoMinValue
    let has_no_max = seq.has_no_max_value || seq.max_value.is_none();
    let has_no_min = seq.has_no_min_value || seq.min_value.is_none();
    write_property(
        writer,
        "HasNoMaxValue",
        if has_no_max { "True" } else { "False" },
    )?;
    write_property(
        writer,
        "HasNoMinValue",
        if has_no_min { "True" } else { "False" },
    )?;

    // MinValue and MaxValue
    if let Some(min) = seq.min_value {
        write_property(writer, "MinValue", &min.to_string())?;
    }
    if let Some(max) = seq.max_value {
        write_property(writer, "MaxValue", &max.to_string())?;
    }

    // Increment
    if let Some(inc) = seq.increment_value {
        write_property(writer, "Increment", &inc.to_string())?;
    }

    // StartValue
    if let Some(start) = seq.start_value {
        write_property(writer, "StartValue", &start.to_string())?;
    }

    // Relationship to schema
    write_schema_relationship(writer, &seq.schema)?;

    // TypeSpecifier relationship for data type
    if let Some(ref data_type) = seq.data_type {
        let type_name = format!("[{}]", data_type.to_lowercase());
        write_type_specifier_builtin(writer, &type_name)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write SqlUserDefinedDataType element for scalar types (alias types)
/// e.g., CREATE TYPE [dbo].[PhoneNumber] FROM VARCHAR(20) NOT NULL
fn write_scalar_type<W: Write>(
    writer: &mut Writer<W>,
    scalar: &ScalarTypeElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", scalar.schema, scalar.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([
        ("Type", "SqlUserDefinedDataType"),
        ("Name", full_name.as_str()),
    ]);
    writer.write_event(Event::Start(elem))?;

    // Properties - IsNullable only if explicitly false (NOT NULL)
    if !scalar.is_nullable {
        write_property(writer, "IsNullable", "False")?;
    }

    // Scale (appears before Precision in DotNet output for decimal types)
    if let Some(scale) = scalar.scale {
        write_property(writer, "Scale", &scale.to_string())?;
    }

    // Precision for decimal types
    if let Some(precision) = scalar.precision {
        write_property(writer, "Precision", &precision.to_string())?;
    }

    // Length for string types
    if let Some(length) = scalar.length {
        write_property(writer, "Length", &length.to_string())?;
    }

    // Relationship to schema
    write_schema_relationship(writer, &scalar.schema)?;

    // Relationship to base type (Type relationship points to built-in type)
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Type")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let type_ref = format!("[{}]", scalar.base_type);
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let refs = BytesStart::new("References")
        .with_attributes([("ExternalSource", "BuiltIns"), ("Name", type_ref.as_str())]);
    writer.write_event(Event::Empty(refs))?;

    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

fn write_user_defined_type<W: Write>(
    writer: &mut Writer<W>,
    udt: &UserDefinedTypeElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", udt.schema, udt.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlTableType"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Calculate disambiguators:
    // - Start at 5 for first default constraint annotation
    // - Increment for each default constraint and index
    let mut disambiguator = 5;

    // Build map of column name to disambiguator for columns with defaults
    let mut column_disambiguators: std::collections::HashMap<&str, u32> =
        std::collections::HashMap::new();
    for col in &udt.columns {
        if col.default_value.is_some() {
            column_disambiguators.insert(&col.name, disambiguator);
            disambiguator += 1;
        }
    }

    // Track index disambiguators
    let mut index_disambiguators: Vec<u32> = Vec::new();
    for constraint in &udt.constraints {
        if matches!(constraint, TableTypeConstraint::Index { .. }) {
            index_disambiguators.push(disambiguator);
            disambiguator += 1;
        }
    }

    // Track the highest disambiguator used for the type-level AttachedAnnotation
    let type_disambiguator = if !index_disambiguators.is_empty() {
        Some(*index_disambiguators.last().unwrap())
    } else {
        None
    };

    // Relationship to schema
    write_schema_relationship(writer, &udt.schema)?;

    // Relationship to columns (table types use SqlTableTypeColumn instead of SqlSimpleColumn)
    if !udt.columns.is_empty() {
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
        writer.write_event(Event::Start(rel))?;

        for col in &udt.columns {
            let col_disambiguator = column_disambiguators.get(col.name.as_str()).copied();
            write_table_type_column_with_annotation(writer, col, &full_name, col_disambiguator)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    // Separate constraints from indexes
    let non_index_constraints: Vec<_> = udt
        .constraints
        .iter()
        .filter(|c| !matches!(c, TableTypeConstraint::Index { .. }))
        .collect();
    let index_constraints: Vec<_> = udt
        .constraints
        .iter()
        .filter_map(|c| match c {
            TableTypeConstraint::Index {
                name,
                columns,
                is_unique,
                is_clustered,
            } => Some((name, columns, *is_unique, *is_clustered)),
            _ => None,
        })
        .collect();

    // Collect columns with defaults for default constraint emission
    let columns_with_defaults: Vec<_> = udt
        .columns
        .iter()
        .filter(|c| c.default_value.is_some())
        .collect();

    // Write Constraints relationship (non-index constraints + default constraints)
    if !non_index_constraints.is_empty() || !columns_with_defaults.is_empty() {
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let rel = BytesStart::new("Relationship").with_attributes([("Name", "Constraints")]);
        writer.write_event(Event::Start(rel))?;

        // Write default constraints first (DotNet order)
        for col in &columns_with_defaults {
            if let Some(default_value) = &col.default_value {
                let col_disambiguator = column_disambiguators.get(col.name.as_str()).copied();
                write_table_type_default_constraint(
                    writer,
                    &full_name,
                    &col.name,
                    default_value,
                    col_disambiguator,
                )?;
            }
        }

        // Write other constraints (PK, UNIQUE, CHECK)
        for (idx, constraint) in non_index_constraints.iter().enumerate() {
            write_table_type_constraint(writer, constraint, &full_name, idx, &udt.columns)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    // Write Indexes relationship separately
    if !index_constraints.is_empty() {
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let rel = BytesStart::new("Relationship").with_attributes([("Name", "Indexes")]);
        writer.write_event(Event::Start(rel))?;

        for (i, (name, columns, is_unique, is_clustered)) in index_constraints.iter().enumerate() {
            let idx_disambiguator = index_disambiguators.get(i).copied();
            write_table_type_index_with_annotation(
                writer,
                &full_name,
                name,
                columns,
                *is_unique,
                *is_clustered,
                idx_disambiguator,
            )?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    // Type-level AttachedAnnotation (if we have indexes)
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    if let Some(disam) = type_disambiguator {
        let disamb_str = disam.to_string();
        let annotation = BytesStart::new("AttachedAnnotation")
            .with_attributes([("Disambiguator", disamb_str.as_str())]);
        writer.write_event(Event::Empty(annotation))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write a table type constraint (PrimaryKey, Unique, Check, Index)
fn write_table_type_constraint<W: Write>(
    writer: &mut Writer<W>,
    constraint: &TableTypeConstraint,
    type_name: &str,
    idx: usize,
    columns: &[TableTypeColumnElement],
) -> anyhow::Result<()> {
    match constraint {
        TableTypeConstraint::PrimaryKey {
            columns: pk_cols,
            is_clustered,
        } => {
            write_table_type_pk_constraint(writer, type_name, pk_cols, *is_clustered, columns)?;
        }
        TableTypeConstraint::Unique {
            columns: uq_cols,
            is_clustered,
        } => {
            write_table_type_unique_constraint(
                writer,
                type_name,
                uq_cols,
                *is_clustered,
                idx,
                columns,
            )?;
        }
        TableTypeConstraint::Check { expression } => {
            write_table_type_check_constraint(writer, type_name, expression, idx)?;
        }
        TableTypeConstraint::Index {
            name,
            columns: idx_cols,
            is_unique,
            is_clustered,
        } => {
            write_table_type_index(writer, type_name, name, idx_cols, *is_unique, *is_clustered)?;
        }
    }
    Ok(())
}

/// Write SqlTableTypePrimaryKeyConstraint element (Entry + Element only, no outer Relationship)
fn write_table_type_pk_constraint<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    pk_columns: &[ConstraintColumn],
    is_clustered: bool,
    all_columns: &[TableTypeColumnElement],
) -> anyhow::Result<()> {
    // Entry for this constraint (parent Constraints relationship is written by caller)
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem =
        BytesStart::new("Element").with_attributes([("Type", "SqlTableTypePrimaryKeyConstraint")]);
    writer.write_event(Event::Start(elem))?;

    // IsClustered property
    if is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    // ColumnSpecifications relationship
    if !pk_columns.is_empty() {
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let col_rel =
            BytesStart::new("Relationship").with_attributes([("Name", "ColumnSpecifications")]);
        writer.write_event(Event::Start(col_rel))?;

        for pk_col in pk_columns {
            let is_descending = pk_col.sort_direction == SortDirection::Descending;
            write_table_type_indexed_column_spec(
                writer,
                type_name,
                &pk_col.name,
                is_descending,
                all_columns,
            )?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write SqlTableTypeUniqueConstraint element (Entry + Element only, no outer Relationship)
fn write_table_type_unique_constraint<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    uq_columns: &[ConstraintColumn],
    is_clustered: bool,
    _idx: usize,
    all_columns: &[TableTypeColumnElement],
) -> anyhow::Result<()> {
    // Entry for this constraint (parent Constraints relationship is written by caller)
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem =
        BytesStart::new("Element").with_attributes([("Type", "SqlTableTypeUniqueConstraint")]);
    writer.write_event(Event::Start(elem))?;

    // IsClustered property
    if is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    // ColumnSpecifications relationship
    if !uq_columns.is_empty() {
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let col_rel =
            BytesStart::new("Relationship").with_attributes([("Name", "ColumnSpecifications")]);
        writer.write_event(Event::Start(col_rel))?;

        for uq_col in uq_columns {
            let is_descending = uq_col.sort_direction == SortDirection::Descending;
            write_table_type_indexed_column_spec(
                writer,
                type_name,
                &uq_col.name,
                is_descending,
                all_columns,
            )?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write SqlTableTypeCheckConstraint element (Entry + Element only, no outer Relationship)
fn write_table_type_check_constraint<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    expression: &str,
    idx: usize,
) -> anyhow::Result<()> {
    // Entry for this constraint (parent Constraints relationship is written by caller)
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Generate a disambiguator for unnamed check constraints
    let disambiguator = format!("{}_CK{}", type_name, idx);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([
        ("Type", "SqlTableTypeCheckConstraint"),
        ("Disambiguator", disambiguator.as_str()),
    ]);
    writer.write_event(Event::Start(elem))?;

    // Expression property
    write_script_property(writer, "Expression", expression)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write SqlTableTypeDefaultConstraint element for columns with DEFAULT values
fn write_table_type_default_constraint<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    column_name: &str,
    default_value: &str,
    disambiguator: Option<u32>,
) -> anyhow::Result<()> {
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem =
        BytesStart::new("Element").with_attributes([("Type", "SqlTableTypeDefaultConstraint")]);
    writer.write_event(Event::Start(elem))?;

    // DefaultExpressionScript property
    write_script_property(writer, "DefaultExpressionScript", default_value)?;

    // ForColumn relationship
    let col_ref = format!("{}.[{}]", type_name, column_name);
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "ForColumn")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let refs = BytesStart::new("References").with_attributes([("Name", col_ref.as_str())]);
    writer.write_event(Event::Empty(refs))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;

    // AttachedAnnotation linking to the column's SqlInlineConstraintAnnotation
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    if let Some(disam) = disambiguator {
        let disamb_str = disam.to_string();
        let annotation = BytesStart::new("AttachedAnnotation")
            .with_attributes([("Disambiguator", disamb_str.as_str())]);
        writer.write_event(Event::Empty(annotation))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write table type index element (Entry + Element only, no outer Relationship)
fn write_table_type_index<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    name: &str,
    idx_columns: &[String],
    is_unique: bool,
    is_clustered: bool,
) -> anyhow::Result<()> {
    // Entry for this constraint (parent Constraints relationship is written by caller)
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let idx_name = format!("{}.[{}]", type_name, name);
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlTableTypeIndex"), ("Name", idx_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Properties
    if is_unique {
        write_property(writer, "IsUnique", "True")?;
    }
    if is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    // ColumnSpecifications relationship (DotNet uses ColumnSpecifications, not Columns)
    if !idx_columns.is_empty() {
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let col_rel =
            BytesStart::new("Relationship").with_attributes([("Name", "ColumnSpecifications")]);
        writer.write_event(Event::Start(col_rel))?;

        for col_name in idx_columns {
            // Default to ascending (is_descending = false) since Vec<String> doesn't track sort direction
            write_table_type_indexed_column_spec(writer, type_name, col_name, false, &[])?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write table type index element with SqlInlineIndexAnnotation for Indexes relationship
fn write_table_type_index_with_annotation<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    name: &str,
    idx_columns: &[String],
    is_unique: bool,
    is_clustered: bool,
    disambiguator: Option<u32>,
) -> anyhow::Result<()> {
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    let idx_name = format!("{}.[{}]", type_name, name);
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlTableTypeIndex"), ("Name", idx_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Properties
    if is_unique {
        write_property(writer, "IsUnique", "True")?;
    }
    if is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    // ColumnSpecifications relationship
    if !idx_columns.is_empty() {
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let col_rel =
            BytesStart::new("Relationship").with_attributes([("Name", "ColumnSpecifications")]);
        writer.write_event(Event::Start(col_rel))?;

        for col_name in idx_columns {
            write_table_type_indexed_column_spec(writer, type_name, col_name, false, &[])?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    // SqlInlineIndexAnnotation
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    if let Some(disam) = disambiguator {
        let disamb_str = disam.to_string();
        let annotation = BytesStart::new("Annotation").with_attributes([
            ("Type", "SqlInlineIndexAnnotation"),
            ("Disambiguator", disamb_str.as_str()),
        ]);
        writer.write_event(Event::Empty(annotation))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write SqlTableTypeIndexedColumnSpecification element
fn write_table_type_indexed_column_spec<W: Write>(
    writer: &mut Writer<W>,
    type_name: &str,
    column_name: &str,
    is_descending: bool,
    _all_columns: &[TableTypeColumnElement],
) -> anyhow::Result<()> {
    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlTableTypeIndexedColumnSpecification")]);
    writer.write_event(Event::Start(elem))?;

    // IsAscending property (true by default, false if descending)
    if is_descending {
        write_property(writer, "IsAscending", "False")?;
    }

    // Column relationship
    let col_ref = format!("{}.[{}]", type_name, column_name);
    write_relationship(writer, "Column", &[&col_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    Ok(())
}

/// Write a DML trigger element to model.xml
/// DotNet format:
/// - Properties: IsInsertTrigger, IsUpdateTrigger, IsDeleteTrigger, SqlTriggerType, BodyScript, IsAnsiNullsOn
/// - Relationships: BodyDependencies, Parent (the table/view), no Schema relationship
fn write_trigger<W: Write>(writer: &mut Writer<W>, trigger: &TriggerElement) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", trigger.schema, trigger.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlDmlTrigger"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Write properties in DotNet order:
    // 1. IsInsertTrigger (only if true)
    if trigger.is_insert_trigger {
        write_property(writer, "IsInsertTrigger", "True")?;
    }

    // 2. IsUpdateTrigger (only if true)
    if trigger.is_update_trigger {
        write_property(writer, "IsUpdateTrigger", "True")?;
    }

    // 3. IsDeleteTrigger (only if true)
    if trigger.is_delete_trigger {
        write_property(writer, "IsDeleteTrigger", "True")?;
    }

    // 4. SqlTriggerType: 2 = AFTER/FOR, 3 = INSTEAD OF
    write_property(writer, "SqlTriggerType", &trigger.trigger_type.to_string())?;

    // 5. BodyScript - extract just the trigger body (after AS)
    let body_script = extract_trigger_body(&trigger.definition);
    write_script_property(writer, "BodyScript", &body_script)?;

    // 6. IsAnsiNullsOn - always True for now (matches typical SQL Server defaults)
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Write BodyDependencies relationship (before Parent)
    let parent_ref = format!("[{}].[{}]", trigger.parent_schema, trigger.parent_name);
    let body_deps = extract_trigger_body_dependencies(&body_script, &parent_ref);
    write_body_dependencies(writer, &body_deps)?;

    // Write Parent relationship (the table or view the trigger is on)
    write_relationship(writer, "Parent", &[&parent_ref])?;

    // Note: DotNet does NOT emit a Schema relationship for triggers

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Extract the trigger body (everything after AS keyword) from the full trigger definition
/// Uses token-based parsing (Phase 15.8 J4/J5) to handle any whitespace around keywords
fn extract_trigger_body(definition: &str) -> String {
    // The pattern is: CREATE TRIGGER ... ON ... (FOR|AFTER|INSTEAD OF) ... AS <body>
    // We need to find the AS keyword that comes after FOR/AFTER/INSTEAD OF

    // Tokenize the definition using sqlparser
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, definition).tokenize() {
        Ok(t) => t,
        Err(_) => {
            // Fallback: return the original definition if tokenization fails
            return definition.to_string();
        }
    };

    // Find the position of FOR/AFTER keyword (or INSTEAD OF pair)
    // Then find the first AS keyword at top level after that position
    let mut found_trigger_action = false;
    let mut paren_depth: i32 = 0;

    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            // Look for trigger action keywords: FOR, AFTER, or INSTEAD (followed by OF)
            Token::Word(w)
                if paren_depth == 0
                    && (w.keyword == Keyword::FOR
                        || w.keyword == Keyword::AFTER
                        || w.value.eq_ignore_ascii_case("INSTEAD")) =>
            {
                found_trigger_action = true;
            }
            // Once we've found the trigger action, look for AS keyword at top level
            Token::Word(w)
                if w.keyword == Keyword::AS && paren_depth == 0 && found_trigger_action =>
            {
                // Found the AS keyword - return everything after it
                return reconstruct_tokens(&tokens[i + 1..]);
            }
            _ => {}
        }
    }

    // Fallback: return the original definition if we can't find the pattern
    definition.to_string()
}

/// Extract body dependencies from a trigger body
/// This handles the special "inserted" and "deleted" magic tables by resolving
/// column references from them to the parent table/view.
///
/// The dependencies are extracted in order of appearance and include:
/// - Table references like [dbo].[Products]
/// - Column references like [dbo].[Products].[Id]
/// - Columns from INSERT column lists
/// - Columns from SELECT/UPDATE referencing inserted/deleted resolved to parent
fn extract_trigger_body_dependencies(body: &str, parent_ref: &str) -> Vec<BodyDependency> {
    use std::collections::HashSet;
    let mut deps = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Track table aliases: maps alias (lowercase) -> table reference
    // For triggers, "inserted" and "deleted" map to the parent table/view
    let mut table_aliases: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    table_aliases.insert("inserted".to_string(), parent_ref.to_string());
    table_aliases.insert("deleted".to_string(), parent_ref.to_string());

    // First pass: find all table aliases using token-based parsing (Phase 20.4.2)
    // Pattern: FROM [schema].[table] alias or JOIN [schema].[table] alias
    // Uses TableAliasTokenParser which handles whitespace, comments, and nested queries correctly
    if let Some(mut parser) = TableAliasTokenParser::new(body) {
        for (alias_or_name, table_ref) in parser.extract_aliases_with_table_names() {
            let alias_lower = alias_or_name.to_lowercase();
            // Don't overwrite inserted/deleted mappings
            if alias_lower != "inserted" && alias_lower != "deleted" {
                table_aliases.insert(alias_lower, table_ref);
            }
        }
    }

    // Process INSERT statements with SELECT FROM inserted/deleted
    // Uses tokenized parsing (Phase 20.4.6) instead of INSERT_SELECT_RE and INSERT_SELECT_JOIN_RE regex
    if let Some(mut parser) = InsertSelectTokenParser::new(body) {
        for stmt in parser.extract_statements() {
            let table_ref = format!("[{}].[{}]", stmt.schema, stmt.table);

            if stmt.has_join {
                // JOIN case: INSERT INTO ... SELECT ... FROM inserted alias JOIN deleted alias ON ...

                // Skip if already processed
                if seen.contains(&table_ref) {
                    continue;
                }

                // Emit table reference first
                seen.insert(table_ref.clone());
                deps.push(BodyDependency::ObjectRef(table_ref.clone()));

                // Emit each column reference from the INSERT column list (no dedup - DotNet preserves order)
                for col in extract_single_bracketed_identifiers(&stmt.column_list) {
                    let col_ref = format!("{}.[{}]", table_ref, col);
                    deps.push(BodyDependency::ObjectRef(col_ref));
                }

                // Add aliases for the JOIN tables (both map to parent)
                if let Some(ref alias1) = stmt.alias1 {
                    table_aliases.insert(alias1.to_lowercase(), parent_ref.to_string());
                }
                if let Some(ref alias2) = stmt.alias2 {
                    table_aliases.insert(alias2.to_lowercase(), parent_ref.to_string());
                }

                // DotNet processes ON clause first, then SELECT columns (skipping duplicates)
                let mut emitted: std::collections::HashSet<(String, String)> =
                    std::collections::HashSet::new();

                // 1. Emit column references from ON clause first (no dedup within ON)
                if let Some(ref on_clause) = stmt.on_clause {
                    for (alias, col) in extract_alias_column_refs_tokenized(on_clause) {
                        let alias_lower = alias.to_lowercase();

                        if let Some(resolved_table) = table_aliases.get(&alias_lower) {
                            let col_ref = format!("{}.[{}]", resolved_table, col);
                            emitted.insert((alias_lower.clone(), col.to_lowercase()));
                            deps.push(BodyDependency::ObjectRef(col_ref));
                        }
                    }
                }

                // 2. Emit column references from SELECT clause (skip if already in ON clause with same alias)
                for (alias, col) in extract_alias_column_refs_tokenized(&stmt.select_expr) {
                    let alias_lower = alias.to_lowercase();
                    let key = (alias_lower.clone(), col.to_lowercase());

                    // Skip if this exact alias.column was already emitted from ON clause
                    if emitted.contains(&key) {
                        continue;
                    }

                    // Resolve alias to table reference
                    if let Some(resolved_table) = table_aliases.get(&alias_lower) {
                        let col_ref = format!("{}.[{}]", resolved_table, col);
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    }
                }
            } else {
                // Simple case: INSERT INTO ... SELECT ... FROM inserted|deleted;

                // Emit table reference first
                if !seen.contains(&table_ref) {
                    seen.insert(table_ref.clone());
                    deps.push(BodyDependency::ObjectRef(table_ref.clone()));
                }

                // Emit each column reference from the INSERT column list
                for col in extract_single_bracketed_identifiers(&stmt.column_list) {
                    let col_ref = format!("{}.[{}]", table_ref, col);
                    if !seen.contains(&col_ref) {
                        seen.insert(col_ref.clone());
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    }
                }

                // Emit column references from SELECT clause - these come from inserted/deleted (parent)
                for col in extract_single_bracketed_identifiers(&stmt.select_expr) {
                    // These columns come from inserted/deleted, resolve to parent
                    let col_ref = format!("{}.[{}]", parent_ref, col);
                    // Deduplicate - DotNet doesn't emit the same column twice from inserted/deleted
                    if !seen.contains(&col_ref) {
                        seen.insert(col_ref.clone());
                        deps.push(BodyDependency::ObjectRef(col_ref));
                    }
                }
            }
        }
    }

    // Process UPDATE with alias pattern: UPDATE alias SET ... FROM [schema].[table] alias JOIN inserted/deleted ON ...
    // Uses tokenized parsing (Phase 20.4.7) instead of UPDATE_ALIAS_RE regex
    if let Some(mut parser) = UpdateTokenParser::new(body) {
        for stmt in parser.extract_statements() {
            let table_ref = format!("[{}].[{}]", stmt.schema, stmt.table);

            // Add aliases
            table_aliases.insert(stmt.update_alias.to_lowercase(), table_ref.clone());
            table_aliases.insert(stmt.table_alias.to_lowercase(), table_ref.clone());
            table_aliases.insert(stmt.magic_alias.to_lowercase(), parent_ref.to_string());

            // Emit table reference first
            if !seen.contains(&table_ref) {
                seen.insert(table_ref.clone());
                deps.push(BodyDependency::ObjectRef(table_ref.clone()));
            }

            // Process ON clause FIRST - extract alias.[col] patterns (these can be duplicated)
            // Use tokenized extraction instead of ALIAS_COL_RE regex
            for (alias, col) in extract_alias_column_refs_tokenized(&stmt.on_clause) {
                let alias_lower = alias.to_lowercase();

                if let Some(resolved_table) = table_aliases.get(&alias_lower) {
                    let col_ref = format!("{}.[{}]", resolved_table, col);
                    // DotNet allows duplicates for columns in ON clause
                    deps.push(BodyDependency::ObjectRef(col_ref));
                }
            }

            // Process SET clause - extract alias.[col] = patterns
            // Use tokenized extraction instead of ALIAS_COL_RE regex
            for (alias, col) in extract_alias_column_refs_tokenized(&stmt.set_clause) {
                let alias_lower = alias.to_lowercase();

                if let Some(resolved_table) = table_aliases.get(&alias_lower) {
                    let col_ref = format!("{}.[{}]", resolved_table, col);
                    // DotNet allows duplicates for SET clause columns too
                    deps.push(BodyDependency::ObjectRef(col_ref));
                }
            }
        }
    }

    deps
}

fn write_raw<W: Write>(
    writer: &mut Writer<W>,
    raw: &RawElement,
    model: &DatabaseModel,
) -> anyhow::Result<()> {
    // Handle SqlView specially to get full property/relationship support
    if raw.sql_type == "SqlView" {
        return write_raw_view(writer, raw, model);
    }

    let full_name = format!("[{}].[{}]", raw.schema, raw.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([
        ("Type", raw.sql_type.as_str()),
        ("Name", full_name.as_str()),
    ]);
    writer.write_event(Event::Start(elem))?;

    // Write BodyScript property with CDATA containing the definition
    write_script_property(writer, "BodyScript", &raw.definition)?;

    // Relationship to schema
    write_schema_relationship(writer, &raw.schema)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write an extended property element
/// Format:
/// ```xml
/// <Element Type="SqlExtendedProperty" Name="[dbo].[Table].[MS_Description]">
///   <Property Name="Value">
///     <Value><![CDATA[Description text]]></Value>
///   </Property>
///   <Relationship Name="Host">
///     <Entry>
///       <References Name="[dbo].[Table]"/>
///     </Entry>
///   </Relationship>
/// </Element>
/// ```
fn write_extended_property<W: Write>(
    writer: &mut Writer<W>,
    ext_prop: &ExtendedPropertyElement,
) -> anyhow::Result<()> {
    let full_name = ext_prop.full_name();

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([
        ("Type", "SqlExtendedProperty"),
        ("Name", full_name.as_str()),
    ]);
    writer.write_event(Event::Start(elem))?;

    // Write Value property with CDATA (SqlScriptProperty format)
    // The value must be wrapped with N'...' for proper SQL string literal escaping
    // Any single quotes in the value must be doubled for SQL escaping
    let escaped_value = ext_prop.property_value.replace('\'', "''");
    let quoted_value = format!("N'{}'", escaped_value);
    write_script_property(writer, "Value", &quoted_value)?;

    // Write Host relationship pointing to the target object (table or column)
    let extends_ref = ext_prop.extends_object_ref();
    write_relationship(writer, "Host", &[&extends_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper for testing parse_column_expression
    fn parse_expr(expr: &str) -> (String, Option<String>) {
        parse_column_expression(expr, &[], "dbo")
    }

    // ============================================================================
    // AS keyword whitespace handling tests
    // ============================================================================

    #[test]
    fn test_as_alias_with_space() {
        let (name, _) = parse_expr("column AS alias");
        assert_eq!(name, "alias");
    }

    #[test]
    fn test_as_alias_with_tab() {
        let (name, _) = parse_expr("column\tAS\talias");
        assert_eq!(name, "alias");
    }

    #[test]
    fn test_as_alias_with_multiple_spaces() {
        let (name, _) = parse_expr("column   AS   alias");
        assert_eq!(name, "alias");
    }

    #[test]
    fn test_as_alias_with_mixed_whitespace() {
        let (name, _) = parse_expr("column \t AS \t alias");
        assert_eq!(name, "alias");
    }

    #[test]
    fn test_as_alias_with_newline() {
        let (name, _) = parse_expr("column\nAS\nalias");
        assert_eq!(name, "alias");
    }

    #[test]
    fn test_bracketed_column_as_alias() {
        let (name, _) = parse_expr("[MyColumn] AS [My Alias]");
        assert_eq!(name, "My Alias");
    }

    #[test]
    fn test_bracketed_column_as_alias_with_tab() {
        let (name, _) = parse_expr("[MyColumn]\tAS\t[My Alias]");
        assert_eq!(name, "My Alias");
    }

    // ============================================================================
    // Column expression without alias
    // ============================================================================

    #[test]
    fn test_simple_column_no_alias() {
        let (name, _) = parse_expr("[Column]");
        assert_eq!(name, "Column");
    }

    #[test]
    fn test_qualified_column_no_alias() {
        let (name, _) = parse_expr("t.[Column]");
        assert_eq!(name, "Column");
    }

    // ============================================================================
    // Function calls
    // ============================================================================

    #[test]
    fn test_function_with_as_alias() {
        let (name, _) = parse_expr("COUNT(*) AS Total");
        assert_eq!(name, "Total");
    }

    #[test]
    fn test_function_with_as_alias_tab() {
        let (name, _) = parse_expr("COUNT(*)\tAS\tTotal");
        assert_eq!(name, "Total");
    }

    #[test]
    fn test_nested_function_with_alias() {
        let (name, _) = parse_expr("COALESCE(NULLIF(a, ''), b) AS Result");
        assert_eq!(name, "Result");
    }

    // ============================================================================
    // CASE expressions
    // ============================================================================

    #[test]
    fn test_case_expression_with_alias() {
        let (name, _) = parse_expr("CASE WHEN x = 1 THEN 'a' ELSE 'b' END AS Result");
        assert_eq!(name, "Result");
    }

    #[test]
    fn test_case_expression_with_tab_alias() {
        let (name, _) = parse_expr("CASE WHEN x = 1 THEN 'a' END\tAS\tResult");
        assert_eq!(name, "Result");
    }

    // ============================================================================
    // Edge cases
    // ============================================================================

    #[test]
    fn test_string_containing_as_word() {
        // The word 'AS' appears inside the string literal, should not be treated as keyword
        let (name, _) = parse_expr("'has' AS Label");
        assert_eq!(name, "Label");
    }

    #[test]
    fn test_cast_expression_with_alias() {
        // CAST contains 'AS' keyword inside parens - should find outer AS
        let (name, _) = parse_expr("CAST(x AS INT) AS Value");
        assert_eq!(name, "Value");
    }

    #[test]
    fn test_cast_expression_with_tab_alias() {
        let (name, _) = parse_expr("CAST(x AS VARCHAR(50))\tAS\tValue");
        assert_eq!(name, "Value");
    }

    // ============================================================================
    // extract_expression_before_as tests (J2 - TVF parameter references)
    // ============================================================================

    #[test]
    fn test_extract_expression_before_as_with_space() {
        let result = extract_expression_before_as("@CustomerId AS [CustomerId]");
        assert_eq!(result, "@CustomerId");
    }

    #[test]
    fn test_extract_expression_before_as_with_tab() {
        let result = extract_expression_before_as("@CustomerId\tAS\t[CustomerId]");
        assert_eq!(result, "@CustomerId");
    }

    #[test]
    fn test_extract_expression_before_as_with_multiple_spaces() {
        let result = extract_expression_before_as("@CustomerId   AS   [CustomerId]");
        assert_eq!(result, "@CustomerId");
    }

    #[test]
    fn test_extract_expression_before_as_with_mixed_whitespace() {
        let result = extract_expression_before_as("@CustomerId \t AS \t [CustomerId]");
        assert_eq!(result, "@CustomerId");
    }

    #[test]
    fn test_extract_expression_before_as_with_newline() {
        let result = extract_expression_before_as("@CustomerId\nAS\n[CustomerId]");
        assert_eq!(result, "@CustomerId");
    }

    #[test]
    fn test_extract_expression_before_as_no_alias() {
        let result = extract_expression_before_as("@CustomerId");
        assert_eq!(result, "@CustomerId");
    }

    #[test]
    fn test_extract_expression_before_as_cast_with_alias() {
        // CAST contains AS inside parens - should find outer AS
        let result = extract_expression_before_as("CAST(@Value AS INT) AS IntValue");
        assert_eq!(result, "CAST(@Value AS INT)");
    }

    #[test]
    fn test_extract_expression_before_as_cast_tab_alias() {
        let result = extract_expression_before_as("CAST(@Value AS INT)\tAS\tIntValue");
        assert_eq!(result, "CAST(@Value AS INT)");
    }

    #[test]
    fn test_extract_expression_before_as_simple_column() {
        let result = extract_expression_before_as("[Column] AS [Alias]");
        assert_eq!(result, "[Column]");
    }

    #[test]
    fn test_extract_expression_before_as_qualified_column() {
        let result = extract_expression_before_as("t.[Column]\tAS\t[Alias]");
        assert_eq!(result, "t.[Column]");
    }

    // ============================================================================
    // OUTER APPLY / CROSS APPLY alias extraction tests
    // ============================================================================

    #[test]
    fn test_extract_table_aliases_cross_apply_subquery() {
        use std::collections::{HashMap, HashSet};

        let sql = r#"
SELECT a.Id, d.TagCount
FROM [dbo].[Account] a
CROSS APPLY (
    SELECT COUNT(*) AS TagCount
    FROM [dbo].[AccountTag]
    WHERE AccountId = a.Id
) d
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        // 'a' should be a table alias for [dbo].[Account]
        assert_eq!(table_aliases.get("a"), Some(&"[dbo].[Account]".to_string()));

        // 'd' should be recognized as a subquery alias (CROSS APPLY result)
        assert!(
            subquery_aliases.contains("d"),
            "Expected 'd' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
    }

    #[test]
    fn test_extract_table_aliases_outer_apply_subquery() {
        use std::collections::{HashMap, HashSet};

        let sql = r#"
SELECT a.Id, t.FirstTagName
FROM [dbo].[Account] a
OUTER APPLY (
    SELECT TOP 1 tag.[Name] AS FirstTagName
    FROM [dbo].[AccountTag] at
    INNER JOIN [dbo].[Tag] tag ON at.TagId = tag.Id
    WHERE at.AccountId = a.Id
) t
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        println!("Table aliases: {:?}", table_aliases);
        println!("Subquery aliases: {:?}", subquery_aliases);

        // 'a' should be a table alias for [dbo].[Account]
        assert_eq!(table_aliases.get("a"), Some(&"[dbo].[Account]".to_string()));

        // 'at' should be a table alias for [dbo].[AccountTag] (inside the subquery)
        assert_eq!(
            table_aliases.get("at"),
            Some(&"[dbo].[AccountTag]".to_string())
        );

        // 'tag' should be a table alias for [dbo].[Tag] (inside the subquery)
        assert_eq!(table_aliases.get("tag"), Some(&"[dbo].[Tag]".to_string()));

        // 't' should be recognized as a subquery alias (OUTER APPLY result)
        assert!(
            subquery_aliases.contains("t"),
            "Expected 't' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
    }

    #[test]
    fn test_body_dependencies_outer_apply_alias_column() {
        // Test that tag.[Name] is correctly resolved to [dbo].[Tag].[Name]
        let sql = r#"
SELECT a.Id, t.FirstTagName
FROM [dbo].[Account] a
OUTER APPLY (
    SELECT TOP 1 tag.[Name] AS FirstTagName
    FROM [dbo].[AccountTag] at
    INNER JOIN [dbo].[Tag] tag ON at.TagId = tag.Id
    WHERE at.AccountId = a.Id
) t
"#;
        let deps = extract_body_dependencies(sql, "[dbo].[TestProc]", &[]);

        // Should contain [dbo].[Tag].[Name] (resolved from tag.[Name])
        let has_tag_name = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Tag].[Name]",
            _ => false,
        });
        assert!(
            has_tag_name,
            "Expected [dbo].[Tag].[Name] in body deps. Got: {:?}",
            deps
        );

        // Should NOT contain [dbo].[Account].[Name]
        let has_account_name = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Account].[Name]",
            _ => false,
        });
        assert!(
            !has_account_name,
            "Should NOT have [dbo].[Account].[Name] in body deps. Got: {:?}",
            deps
        );
    }

    #[test]
    fn test_extract_table_aliases_cte_single() {
        use std::collections::{HashMap, HashSet};

        let sql = r#"
WITH AccountCte AS (
    SELECT A.Id, A.AccountNumber, A.Status
    FROM [dbo].[Account] A
    WHERE A.Id = @AccountId
)
SELECT AccountCte.Id, AccountCte.AccountNumber, AccountCte.Status
FROM AccountCte;
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        // 'A' should be a table alias for [dbo].[Account]
        assert_eq!(table_aliases.get("a"), Some(&"[dbo].[Account]".to_string()));

        // 'AccountCte' should be recognized as a CTE/subquery alias (not a table)
        assert!(
            subquery_aliases.contains("accountcte"),
            "Expected 'accountcte' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
    }

    #[test]
    fn test_extract_table_aliases_cte_multiple() {
        use std::collections::{HashMap, HashSet};

        let sql = r#"
WITH TagCte AS (
    SELECT T.Id, T.Name
    FROM [dbo].[Tag] T
),
AccountTagCte AS (
    SELECT AT.AccountId, AT.TagId
    FROM [dbo].[AccountTag] AT
)
SELECT TagCte.Id AS TagId, TagCte.Name AS TagName, AccountTagCte.AccountId
FROM TagCte
INNER JOIN AccountTagCte ON AccountTagCte.TagId = TagCte.Id
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        // 'T' should be a table alias for [dbo].[Tag]
        assert_eq!(table_aliases.get("t"), Some(&"[dbo].[Tag]".to_string()));

        // 'AT' should be a table alias for [dbo].[AccountTag]
        assert_eq!(
            table_aliases.get("at"),
            Some(&"[dbo].[AccountTag]".to_string())
        );

        // Both CTE names should be recognized as subquery aliases
        assert!(
            subquery_aliases.contains("tagcte"),
            "Expected 'tagcte' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
        assert!(
            subquery_aliases.contains("accounttagcte"),
            "Expected 'accounttagcte' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
    }

    #[test]
    fn test_body_dependencies_cte_alias_resolution() {
        // Test that CTE aliases are NOT included as schema references in body deps
        let sql = r#"
WITH AccountCte AS (
    SELECT A.Id, A.AccountNumber
    FROM [dbo].[Account] A
)
SELECT AccountCte.Id, AccountCte.AccountNumber
FROM AccountCte;
"#;
        let deps = extract_body_dependencies(sql, "[dbo].[TestProc]", &[]);

        // Should contain [dbo].[Account] (the actual table)
        let has_account = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Account]",
            _ => false,
        });
        assert!(
            has_account,
            "Expected [dbo].[Account] in body deps. Got: {:?}",
            deps
        );

        // Should NOT contain [AccountCte].* as a schema reference
        let has_cte_as_schema = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r.starts_with("[AccountCte]"),
            _ => false,
        });
        assert!(
            !has_cte_as_schema,
            "Should NOT have [AccountCte].* as schema in body deps. Got: {:?}",
            deps
        );
    }

    #[test]
    fn test_extract_table_aliases_nested_subquery() {
        use std::collections::{HashMap, HashSet};

        // Test double-nested subquery: LEFT JOIN subquery containing STUFF subquery
        let sql = r#"
SELECT A.Id AS AccountBusinessKey
FROM [dbo].[Account] A
LEFT JOIN (
    SELECT AccountTags.AccountId,
           STUFF((
               SELECT ', ' + [ATTAG].[Name]
               FROM [dbo].[AccountTag] [AT]
               INNER JOIN [dbo].[Tag] [ATTAG] ON [AT].TagId = [ATTAG].Id
               WHERE AccountTags.AccountId = [AT].AccountId
               FOR XML PATH('')
           ), 1, 1, '') AS TagList
    FROM [dbo].[AccountTag] AccountTags
    INNER JOIN [dbo].[Tag] [TAG] ON AccountTags.TagId = [TAG].Id
    GROUP BY AccountTags.AccountId
) AS TagDetails ON TagDetails.AccountId = A.Id
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        println!("Table aliases: {:?}", table_aliases);
        println!("Subquery aliases: {:?}", subquery_aliases);

        // 'A' should be a table alias for [dbo].[Account]
        assert_eq!(
            table_aliases.get("a"),
            Some(&"[dbo].[Account]".to_string()),
            "Expected 'A' -> [dbo].[Account]"
        );

        // 'AccountTags' should be a table alias for [dbo].[AccountTag] (first level nested)
        assert_eq!(
            table_aliases.get("accounttags"),
            Some(&"[dbo].[AccountTag]".to_string()),
            "Expected 'AccountTags' -> [dbo].[AccountTag]"
        );

        // '[AT]' should be a table alias for [dbo].[AccountTag] (second level nested)
        assert_eq!(
            table_aliases.get("at"),
            Some(&"[dbo].[AccountTag]".to_string()),
            "Expected 'AT' -> [dbo].[AccountTag]"
        );

        // '[ATTAG]' should be a table alias for [dbo].[Tag] (second level nested)
        assert_eq!(
            table_aliases.get("attag"),
            Some(&"[dbo].[Tag]".to_string()),
            "Expected 'ATTAG' -> [dbo].[Tag]"
        );

        // '[TAG]' should be a table alias for [dbo].[Tag] (first level nested)
        assert_eq!(
            table_aliases.get("tag"),
            Some(&"[dbo].[Tag]".to_string()),
            "Expected 'TAG' -> [dbo].[Tag]"
        );

        // 'TagDetails' should be recognized as a subquery alias
        assert!(
            subquery_aliases.contains("tagdetails"),
            "Expected 'TagDetails' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
    }

    #[test]
    fn test_body_dependencies_nested_subquery_alias_resolution() {
        // Test that nested subquery aliases are resolved correctly in body deps
        // References like [ATTAG].[Name] inside STUFF should resolve to [dbo].[Tag].[Name]
        // References to TagDetails.* should be skipped (subquery alias)
        let sql = r#"
SELECT A.Id AS AccountBusinessKey, TagDetails.TagList
FROM [dbo].[Account] A
LEFT JOIN (
    SELECT AccountTags.AccountId,
           STUFF((
               SELECT ', ' + [ATTAG].[Name]
               FROM [dbo].[AccountTag] [AT]
               INNER JOIN [dbo].[Tag] [ATTAG] ON [AT].TagId = [ATTAG].Id
               WHERE AccountTags.AccountId = [AT].AccountId
               FOR XML PATH('')
           ), 1, 1, '') AS TagList
    FROM [dbo].[AccountTag] AccountTags
    INNER JOIN [dbo].[Tag] [TAG] ON AccountTags.TagId = [TAG].Id
) AS TagDetails ON TagDetails.AccountId = A.Id
"#;
        let deps = extract_body_dependencies(sql, "[dbo].[TestProc]", &[]);

        println!("Body dependencies:");
        for d in &deps {
            println!("  {:?}", d);
        }

        // Should contain [dbo].[Account] (outer table)
        let has_account = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Account]",
            _ => false,
        });
        assert!(
            has_account,
            "Expected [dbo].[Account] in body deps. Got: {:?}",
            deps
        );

        // Should contain [dbo].[AccountTag] (from nested subquery)
        let has_account_tag = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[AccountTag]",
            _ => false,
        });
        assert!(
            has_account_tag,
            "Expected [dbo].[AccountTag] in body deps. Got: {:?}",
            deps
        );

        // Should contain [dbo].[Tag] (from doubly-nested STUFF subquery)
        let has_tag = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Tag]",
            _ => false,
        });
        assert!(
            has_tag,
            "Expected [dbo].[Tag] in body deps. Got: {:?}",
            deps
        );

        // Should contain [dbo].[Tag].[Name] (resolved from [ATTAG].[Name])
        let has_tag_name = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Tag].[Name]",
            _ => false,
        });
        assert!(
            has_tag_name,
            "Expected [dbo].[Tag].[Name] in body deps. Got: {:?}",
            deps
        );

        // Should contain [dbo].[Tag].[Id] (from INNER JOIN condition)
        let has_tag_id = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Tag].[Id]",
            _ => false,
        });
        assert!(
            has_tag_id,
            "Expected [dbo].[Tag].[Id] in body deps. Got: {:?}",
            deps
        );

        // Should NOT contain [TagDetails].* as a schema reference
        let has_tag_details = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r.starts_with("[TagDetails]"),
            _ => false,
        });
        assert!(
            !has_tag_details,
            "Should NOT have [TagDetails].* in body deps. Got: {:?}",
            deps
        );

        // Should NOT contain [ATTAG].* as a schema reference (should be resolved)
        let has_attag = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r.starts_with("[ATTAG]"),
            _ => false,
        });
        assert!(
            !has_attag,
            "Should NOT have [ATTAG].* in body deps. Got: {:?}",
            deps
        );
    }

    #[test]
    fn test_extract_table_aliases_unqualified_single() {
        use std::collections::{HashMap, HashSet};

        // Test unqualified table name with alias: FROM Account A
        let sql = r#"
SELECT A.Id, A.AccountNumber
FROM Account A
WHERE A.Id = @AccountId
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        println!("Table aliases: {:?}", table_aliases);

        // 'A' should be a table alias for [dbo].[Account] (default schema)
        assert_eq!(
            table_aliases.get("a"),
            Some(&"[dbo].[Account]".to_string()),
            "Expected 'A' -> [dbo].[Account]"
        );
    }

    #[test]
    fn test_extract_table_aliases_unqualified_multiple_joins() {
        use std::collections::{HashMap, HashSet};

        // Test unqualified table names with multiple joins
        let sql = r#"
SELECT A.Id, T.Name
FROM Account A
INNER JOIN AccountTag AT ON AT.AccountId = A.Id
INNER JOIN Tag T ON T.Id = AT.TagId
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        println!("Table aliases: {:?}", table_aliases);

        // 'A' should be a table alias for [dbo].[Account]
        assert_eq!(
            table_aliases.get("a"),
            Some(&"[dbo].[Account]".to_string()),
            "Expected 'A' -> [dbo].[Account]"
        );

        // 'AT' should be a table alias for [dbo].[AccountTag]
        assert_eq!(
            table_aliases.get("at"),
            Some(&"[dbo].[AccountTag]".to_string()),
            "Expected 'AT' -> [dbo].[AccountTag]"
        );

        // 'T' should be a table alias for [dbo].[Tag]
        assert_eq!(
            table_aliases.get("t"),
            Some(&"[dbo].[Tag]".to_string()),
            "Expected 'T' -> [dbo].[Tag]"
        );
    }

    #[test]
    fn test_extract_table_aliases_unqualified_bracketed() {
        use std::collections::{HashMap, HashSet};

        // Test unqualified bracketed table name: FROM [Account] A
        let sql = r#"
SELECT A.Id, A.AccountNumber
FROM [Account] A
WHERE A.Id = @AccountId
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        println!("Table aliases: {:?}", table_aliases);

        // 'A' should be a table alias for [dbo].[Account] (default schema)
        assert_eq!(
            table_aliases.get("a"),
            Some(&"[dbo].[Account]".to_string()),
            "Expected 'A' -> [dbo].[Account]"
        );
    }

    #[test]
    fn test_body_dependencies_unqualified_alias_resolution() {
        // Test that unqualified table aliases are resolved correctly in body deps
        // FROM Account A should resolve A.Id to [dbo].[Account].[Id]
        let sql = r#"
SELECT A.Id AS AccountId, A.AccountNumber, T.Name AS TagName
FROM Account A
INNER JOIN AccountTag AT ON AT.AccountId = A.Id
INNER JOIN Tag T ON T.Id = AT.TagId
WHERE A.Id = @AccountId
"#;
        let deps = extract_body_dependencies(sql, "[dbo].[TestProc]", &[]);

        println!("Body dependencies:");
        for d in &deps {
            println!("  {:?}", d);
        }

        // Should contain [dbo].[Account] (resolved from 'Account')
        let has_account = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Account]",
            _ => false,
        });
        assert!(
            has_account,
            "Expected [dbo].[Account] in body deps. Got: {:?}",
            deps
        );

        // Should contain [dbo].[Account].[Id] (resolved from A.Id)
        let has_account_id = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Account].[Id]",
            _ => false,
        });
        assert!(
            has_account_id,
            "Expected [dbo].[Account].[Id] in body deps. Got: {:?}",
            deps
        );

        // Should NOT contain [A].* as a schema reference
        let has_a_as_schema = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r.starts_with("[A]"),
            _ => false,
        });
        assert!(
            !has_a_as_schema,
            "Should NOT have [A].* in body deps. Got: {:?}",
            deps
        );

        // Should NOT contain [AT].* as a schema reference
        let has_at_as_schema = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r.starts_with("[AT]"),
            _ => false,
        });
        assert!(
            !has_at_as_schema,
            "Should NOT have [AT].* in body deps. Got: {:?}",
            deps
        );

        // Should NOT contain [T].* as a schema reference
        let has_t_as_schema = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r.starts_with("[T]"),
            _ => false,
        });
        assert!(
            !has_t_as_schema,
            "Should NOT have [T].* in body deps. Got: {:?}",
            deps
        );
    }

    #[test]
    fn test_extract_table_aliases_qualified_takes_precedence() {
        use std::collections::{HashMap, HashSet};

        // When both qualified and unqualified patterns could match,
        // the qualified pattern should take precedence
        let sql = r#"
SELECT A.Id
FROM [dbo].[Account] A
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        // Should use the qualified version from [dbo].[Account]
        assert_eq!(
            table_aliases.get("a"),
            Some(&"[dbo].[Account]".to_string()),
            "Expected 'A' -> [dbo].[Account]"
        );
    }

    // ============================================================================
    // Phase 19.1: clean_data_type whitespace handling tests
    // ============================================================================

    #[test]
    fn test_clean_data_type_readonly_with_space() {
        // Standard single space before READONLY
        let result = clean_data_type("[dbo].[TableType] READONLY");
        assert_eq!(result, "[dbo].[TableType]");
    }

    #[test]
    fn test_clean_data_type_readonly_with_tab() {
        // Tab before READONLY
        let result = clean_data_type("[dbo].[TableType]\tREADONLY");
        assert_eq!(result, "[dbo].[TableType]");
    }

    #[test]
    fn test_clean_data_type_readonly_with_multiple_spaces() {
        // Multiple spaces before READONLY
        let result = clean_data_type("[dbo].[TableType]   READONLY");
        assert_eq!(result, "[dbo].[TableType]");
    }

    #[test]
    fn test_clean_data_type_readonly_with_mixed_whitespace() {
        // Mixed tabs and spaces before READONLY
        let result = clean_data_type("[dbo].[TableType] \t READONLY");
        assert_eq!(result, "[dbo].[TableType]");
    }

    #[test]
    fn test_clean_data_type_null_with_space() {
        // Standard single space before NULL
        let result = clean_data_type("INT NULL");
        assert_eq!(result, "INT");
    }

    #[test]
    fn test_clean_data_type_null_with_tab() {
        // Tab before NULL
        let result = clean_data_type("INT\tNULL");
        assert_eq!(result, "INT");
    }

    #[test]
    fn test_clean_data_type_null_with_multiple_spaces() {
        // Multiple spaces before NULL
        let result = clean_data_type("VARCHAR(100)   NULL");
        assert_eq!(result, "VARCHAR(100)");
    }

    #[test]
    fn test_clean_data_type_not_null_with_space() {
        // Standard spaces before NOT NULL
        let result = clean_data_type("DATETIME NOT NULL");
        assert_eq!(result, "DATETIME");
    }

    #[test]
    fn test_clean_data_type_not_null_with_tabs() {
        // Tabs before NOT NULL
        let result = clean_data_type("DECIMAL(10,2)\tNOT\tNULL");
        assert_eq!(result, "DECIMAL(10,2)");
    }

    #[test]
    fn test_clean_data_type_not_null_with_mixed_whitespace() {
        // Mixed whitespace before NOT NULL
        let result = clean_data_type("BIGINT \t NOT  \t NULL");
        assert_eq!(result, "BIGINT");
    }

    #[test]
    fn test_clean_data_type_qualified_type_no_keywords() {
        // Schema-qualified type with no trailing keywords
        let result = clean_data_type("[dbo].[CustomType]");
        assert_eq!(result, "[dbo].[CustomType]");
    }

    #[test]
    fn test_clean_data_type_builtin_type_no_keywords() {
        // Built-in type with no trailing keywords (should uppercase)
        let result = clean_data_type("int");
        assert_eq!(result, "INT");
    }

    #[test]
    fn test_clean_data_type_with_precision() {
        // Type with precision, NULL removed
        let result = clean_data_type("NVARCHAR(50) NULL");
        assert_eq!(result, "NVARCHAR(50)");
    }

    #[test]
    fn test_clean_data_type_empty_string() {
        // Empty string should return empty
        let result = clean_data_type("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_clean_data_type_whitespace_only() {
        // Whitespace only should return empty
        let result = clean_data_type("   \t  ");
        assert_eq!(result, "");
    }

    #[test]
    fn test_clean_data_type_readonly_case_insensitive() {
        // READONLY in lowercase
        let result = clean_data_type("[dbo].[Type] readonly");
        assert_eq!(result, "[dbo].[Type]");
    }

    #[test]
    fn test_clean_data_type_null_case_insensitive() {
        // NULL in mixed case
        let result = clean_data_type("INT Null");
        assert_eq!(result, "INT");
    }

    #[test]
    fn test_clean_data_type_not_null_case_insensitive() {
        // NOT NULL in mixed case
        let result = clean_data_type("VARCHAR(MAX) Not Null");
        assert_eq!(result, "VARCHAR(MAX)");
    }

    // ============================================================================
    // BodyDependencyTokenScanner tests (Phase 20.2.1)
    // ============================================================================

    #[test]
    fn test_body_dep_scanner_parameter() {
        // @param pattern
        let mut scanner = BodyDependencyTokenScanner::new("@userId").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], BodyDepToken::Parameter("userId".to_string()));
    }

    #[test]
    fn test_body_dep_scanner_parameter_with_whitespace() {
        // @param with surrounding whitespace
        let mut scanner = BodyDependencyTokenScanner::new("  @userId  ").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], BodyDepToken::Parameter("userId".to_string()));
    }

    #[test]
    fn test_body_dep_scanner_three_part_bracketed() {
        // [schema].[table].[column]
        let mut scanner = BodyDependencyTokenScanner::new("[dbo].[Users].[Name]").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::ThreePartBracketed {
                schema: "dbo".to_string(),
                table: "Users".to_string(),
                column: "Name".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_three_part_with_whitespace() {
        // [schema] . [table] . [column] with whitespace around dots
        let mut scanner = BodyDependencyTokenScanner::new("[dbo] . [Users] . [Name]").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::ThreePartBracketed {
                schema: "dbo".to_string(),
                table: "Users".to_string(),
                column: "Name".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_three_part_with_tabs() {
        // [schema]\t.\t[table]\t.\t[column] with tabs
        let mut scanner = BodyDependencyTokenScanner::new("[dbo]\t.\t[Users]\t.\t[Name]").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::ThreePartBracketed {
                schema: "dbo".to_string(),
                table: "Users".to_string(),
                column: "Name".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_two_part_bracketed() {
        // [schema].[table]
        let mut scanner = BodyDependencyTokenScanner::new("[dbo].[Users]").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::TwoPartBracketed {
                first: "dbo".to_string(),
                second: "Users".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_two_part_with_whitespace() {
        // [schema] . [table] with whitespace
        let mut scanner = BodyDependencyTokenScanner::new("[dbo]  .  [Users]").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::TwoPartBracketed {
                first: "dbo".to_string(),
                second: "Users".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_alias_dot_bracketed_column() {
        // alias.[column]
        let mut scanner = BodyDependencyTokenScanner::new("u.[Name]").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::AliasDotBracketedColumn {
                alias: "u".to_string(),
                column: "Name".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_alias_dot_bracketed_with_whitespace() {
        // alias . [column] with whitespace
        let mut scanner = BodyDependencyTokenScanner::new("u  .  [Name]").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::AliasDotBracketedColumn {
                alias: "u".to_string(),
                column: "Name".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_bracketed_alias_dot_column() {
        // [alias].column
        let mut scanner = BodyDependencyTokenScanner::new("[u].Name").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::BracketedAliasDotColumn {
                alias: "u".to_string(),
                column: "Name".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_bracketed_alias_dot_column_with_whitespace() {
        // [alias] . column with whitespace
        let mut scanner = BodyDependencyTokenScanner::new("[u]  .  Name").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::BracketedAliasDotColumn {
                alias: "u".to_string(),
                column: "Name".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_single_bracketed() {
        // [ident]
        let mut scanner = BodyDependencyTokenScanner::new("[Name]").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], BodyDepToken::SingleBracketed("Name".to_string()));
    }

    #[test]
    fn test_body_dep_scanner_two_part_unbracketed() {
        // schema.table
        let mut scanner = BodyDependencyTokenScanner::new("dbo.Users").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::TwoPartUnbracketed {
                first: "dbo".to_string(),
                second: "Users".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_two_part_unbracketed_with_whitespace() {
        // schema . table with whitespace
        let mut scanner = BodyDependencyTokenScanner::new("dbo  .  Users").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::TwoPartUnbracketed {
                first: "dbo".to_string(),
                second: "Users".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_single_unbracketed() {
        // ident
        let mut scanner = BodyDependencyTokenScanner::new("Name").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::SingleUnbracketed("Name".to_string())
        );
    }

    #[test]
    fn test_body_dep_scanner_multiple_tokens() {
        // Multiple patterns in sequence
        let sql = "@userId [dbo].[Users] u.[Name]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], BodyDepToken::Parameter("userId".to_string()));
        assert_eq!(
            tokens[1],
            BodyDepToken::TwoPartBracketed {
                first: "dbo".to_string(),
                second: "Users".to_string()
            }
        );
        assert_eq!(
            tokens[2],
            BodyDepToken::AliasDotBracketedColumn {
                alias: "u".to_string(),
                column: "Name".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_realistic_select() {
        // Realistic SELECT statement
        let sql = "SELECT [Id], [Name], u.[Email] FROM [dbo].[Users] u WHERE @userId = u.[Id]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();

        // Expected tokens: SELECT, [Id], [Name], u.[Email], FROM, [dbo].[Users], u, WHERE, @userId, =, u.[Id]
        // Token scanner should pick up: [Id], [Name], u.[Email], [dbo].[Users], u, @userId, u.[Id]
        let param_count = tokens
            .iter()
            .filter(|t| matches!(t, BodyDepToken::Parameter(_)))
            .count();
        let two_part_count = tokens
            .iter()
            .filter(|t| matches!(t, BodyDepToken::TwoPartBracketed { .. }))
            .count();
        let alias_col_count = tokens
            .iter()
            .filter(|t| matches!(t, BodyDepToken::AliasDotBracketedColumn { .. }))
            .count();

        assert_eq!(param_count, 1); // @userId
        assert_eq!(two_part_count, 1); // [dbo].[Users]
        assert_eq!(alias_col_count, 2); // u.[Email], u.[Id]
    }

    #[test]
    fn test_body_dep_scanner_with_newlines() {
        // SQL with newlines
        let sql = "SELECT\n    [Id],\n    [Name]\nFROM\n    [dbo].[Users]";
        let mut scanner = BodyDependencyTokenScanner::new(sql).unwrap();
        let tokens = scanner.scan();

        let single_bracket_count = tokens
            .iter()
            .filter(|t| matches!(t, BodyDepToken::SingleBracketed(_)))
            .count();
        let two_part_count = tokens
            .iter()
            .filter(|t| matches!(t, BodyDepToken::TwoPartBracketed { .. }))
            .count();

        assert_eq!(single_bracket_count, 2); // [Id], [Name]
        assert_eq!(two_part_count, 1); // [dbo].[Users]
    }

    #[test]
    fn test_body_dep_scanner_special_chars_in_brackets() {
        // Identifiers with spaces and special chars inside brackets
        let mut scanner =
            BodyDependencyTokenScanner::new("[My Schema].[My Table].[My Column]").unwrap();
        let tokens = scanner.scan();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            BodyDepToken::ThreePartBracketed {
                schema: "My Schema".to_string(),
                table: "My Table".to_string(),
                column: "My Column".to_string()
            }
        );
    }

    #[test]
    fn test_body_dep_scanner_empty_input() {
        // Empty input
        let mut scanner = BodyDependencyTokenScanner::new("").unwrap();
        let tokens = scanner.scan();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_body_dep_scanner_whitespace_only() {
        // Whitespace only
        let mut scanner = BodyDependencyTokenScanner::new("   \t\n   ").unwrap();
        let tokens = scanner.scan();
        assert!(tokens.is_empty());
    }

    // Phase 20.2.2: Tests for extract_column_refs_tokenized (replacing COL_REF_RE)

    #[test]
    fn test_extract_col_refs_two_part_bracketed() {
        let refs = extract_column_refs_tokenized("[alias].[column]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "[alias].[column]");
    }

    #[test]
    fn test_extract_col_refs_three_part_bracketed() {
        let refs = extract_column_refs_tokenized("[dbo].[Users].[Id]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "[dbo].[Users].[Id]");
    }

    #[test]
    fn test_extract_col_refs_alias_dot_bracketed() {
        let refs = extract_column_refs_tokenized("u.[Name]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "u.[Name]");
    }

    #[test]
    fn test_extract_col_refs_bracketed_dot_unbracketed() {
        let refs = extract_column_refs_tokenized("[u].Name");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "[u].Name");
    }

    #[test]
    fn test_extract_col_refs_unbracketed_two_part() {
        let refs = extract_column_refs_tokenized("alias.column");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "alias.column");
    }

    #[test]
    fn test_extract_col_refs_with_whitespace() {
        // Token-based extraction handles variable whitespace
        let refs = extract_column_refs_tokenized("[alias]  .  [column]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "[alias].[column]");
    }

    #[test]
    fn test_extract_col_refs_with_tabs() {
        let refs = extract_column_refs_tokenized("[alias]\t.\t[column]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "[alias].[column]");
    }

    #[test]
    fn test_extract_col_refs_multiple_refs() {
        let refs = extract_column_refs_tokenized("a.[x] = b.[y] AND [dbo].[Users].[Id] = c.Id");
        assert_eq!(refs.len(), 4);
        assert!(refs.contains(&"a.[x]".to_string()));
        assert!(refs.contains(&"b.[y]".to_string()));
        assert!(refs.contains(&"[dbo].[Users].[Id]".to_string()));
        assert!(refs.contains(&"c.Id".to_string()));
    }

    #[test]
    fn test_extract_col_refs_on_clause() {
        // Simulating ON clause text
        let refs = extract_column_refs_tokenized("t1.Id = t2.UserId");
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&"t1.Id".to_string()));
        assert!(refs.contains(&"t2.UserId".to_string()));
    }

    #[test]
    fn test_extract_col_refs_group_by_clause() {
        // Simulating GROUP BY clause text
        let refs = extract_column_refs_tokenized("u.Department, u.Status");
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&"u.Department".to_string()));
        assert!(refs.contains(&"u.Status".to_string()));
    }

    #[test]
    fn test_extract_col_refs_skips_single_idents() {
        // Single identifiers are not column references (no dot)
        let refs = extract_column_refs_tokenized("column_name");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_col_refs_skips_parameters() {
        // Parameters are not column references
        let refs = extract_column_refs_tokenized("@userId");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_col_refs_empty_input() {
        let refs = extract_column_refs_tokenized("");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_col_refs_whitespace_only() {
        let refs = extract_column_refs_tokenized("   \t\n   ");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_col_refs_special_chars_in_brackets() {
        // Identifiers with spaces and special chars
        let refs = extract_column_refs_tokenized("[My Schema].[My Table]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "[My Schema].[My Table]");
    }

    // ============================================================================
    // Tests for extract_bracketed_identifiers_tokenized (Phase 20.2.4)
    // ============================================================================

    #[test]
    fn test_bracketed_idents_single_column() {
        let idents = extract_bracketed_identifiers_tokenized("[ColumnName]");
        assert_eq!(idents.len(), 1);
        assert_eq!(idents[0].name, "ColumnName");
        assert_eq!(idents[0].position, 0);
    }

    #[test]
    fn test_bracketed_idents_multiple_columns() {
        let idents = extract_bracketed_identifiers_tokenized("[Col1] AND [Col2]");
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0].name, "Col1");
        assert_eq!(idents[1].name, "Col2");
    }

    #[test]
    fn test_bracketed_idents_skip_multipart_reference() {
        // Two-part references should be skipped (they are part of qualified names)
        let idents = extract_bracketed_identifiers_tokenized("[schema].[table]");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_bracketed_idents_skip_three_part_reference() {
        // Three-part references should be skipped
        let idents = extract_bracketed_identifiers_tokenized("[schema].[table].[column]");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_bracketed_idents_with_whitespace() {
        let idents = extract_bracketed_identifiers_tokenized("[Col1]\tAND\t[Col2]");
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0].name, "Col1");
        assert_eq!(idents[1].name, "Col2");
    }

    #[test]
    fn test_bracketed_idents_with_newlines() {
        let idents = extract_bracketed_identifiers_tokenized("[Col1]\nAND\n[Col2]");
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0].name, "Col1");
        assert_eq!(idents[1].name, "Col2");
    }

    #[test]
    fn test_bracketed_idents_position_tracking() {
        let idents = extract_bracketed_identifiers_tokenized("[A] = [B]");
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0].name, "A");
        assert_eq!(idents[0].position, 0);
        assert_eq!(idents[1].name, "B");
        assert_eq!(idents[1].position, 6);
    }

    #[test]
    fn test_bracketed_idents_filter_predicate_example() {
        // Example from filtered index predicate
        let idents =
            extract_bracketed_identifiers_tokenized("[DeletedAt] IS NULL AND [Status] = N'Active'");
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0].name, "DeletedAt");
        assert_eq!(idents[1].name, "Status");
    }

    #[test]
    fn test_bracketed_idents_computed_column_example() {
        // Example from computed column expression
        let idents = extract_bracketed_identifiers_tokenized("[Quantity] * [UnitPrice]");
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0].name, "Quantity");
        assert_eq!(idents[1].name, "UnitPrice");
    }

    #[test]
    fn test_bracketed_idents_empty_input() {
        let idents = extract_bracketed_identifiers_tokenized("");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_bracketed_idents_whitespace_only() {
        let idents = extract_bracketed_identifiers_tokenized("   \t\n   ");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_bracketed_idents_no_brackets() {
        // Unbracketed identifiers should not be returned
        let idents = extract_bracketed_identifiers_tokenized("Col1 AND Col2");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_bracketed_idents_mixed_qualified_and_standalone() {
        // Only standalone bracketed identifiers should be returned
        let idents = extract_bracketed_identifiers_tokenized("[standalone] AND [schema].[table]");
        assert_eq!(idents.len(), 1);
        assert_eq!(idents[0].name, "standalone");
    }

    #[test]
    fn test_bracketed_idents_with_spaces_in_name() {
        // Bracketed identifiers can contain spaces
        let idents = extract_bracketed_identifiers_tokenized("[Column Name]");
        assert_eq!(idents.len(), 1);
        assert_eq!(idents[0].name, "Column Name");
    }

    #[test]
    fn test_bracketed_idents_with_dots_between_whitespace() {
        // Ensure whitespace around dots is handled correctly
        let idents = extract_bracketed_identifiers_tokenized("[a] . [b]");
        // These are still part of a qualified name despite whitespace
        assert!(idents.is_empty());
    }

    // ============================================================================
    // Tests for extract_alias_column_refs_tokenized (Phase 20.2.5)
    // ============================================================================

    #[test]
    fn test_alias_col_simple() {
        // Basic alias.[column] pattern
        let refs = extract_alias_column_refs_tokenized("i.[Id]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("i".to_string(), "Id".to_string()));
    }

    #[test]
    fn test_alias_col_multiple() {
        // Multiple alias.[column] patterns
        let refs = extract_alias_column_refs_tokenized("i.[Id] = d.[Id]");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], ("i".to_string(), "Id".to_string()));
        assert_eq!(refs[1], ("d".to_string(), "Id".to_string()));
    }

    #[test]
    fn test_alias_col_with_whitespace() {
        // Whitespace around dot
        let refs = extract_alias_column_refs_tokenized("alias . [Column]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("alias".to_string(), "Column".to_string()));
    }

    #[test]
    fn test_alias_col_with_tabs() {
        // Tabs instead of spaces
        let refs = extract_alias_column_refs_tokenized("alias\t.\t[Column]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("alias".to_string(), "Column".to_string()));
    }

    #[test]
    fn test_alias_col_trigger_on_clause() {
        // Typical trigger ON clause
        let refs = extract_alias_column_refs_tokenized("i.[ProductId] = d.[ProductId]");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], ("i".to_string(), "ProductId".to_string()));
        assert_eq!(refs[1], ("d".to_string(), "ProductId".to_string()));
    }

    #[test]
    fn test_alias_col_trigger_select() {
        // SELECT clause in trigger
        let refs = extract_alias_column_refs_tokenized("d.[Id], i.[Name], d.[Value]");
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0], ("d".to_string(), "Id".to_string()));
        assert_eq!(refs[1], ("i".to_string(), "Name".to_string()));
        assert_eq!(refs[2], ("d".to_string(), "Value".to_string()));
    }

    #[test]
    fn test_alias_col_update_set() {
        // SET clause in UPDATE
        let refs = extract_alias_column_refs_tokenized("t.[Quantity] = i.[Quantity]");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], ("t".to_string(), "Quantity".to_string()));
        assert_eq!(refs[1], ("i".to_string(), "Quantity".to_string()));
    }

    #[test]
    fn test_alias_col_empty() {
        let refs = extract_alias_column_refs_tokenized("");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_alias_col_whitespace_only() {
        let refs = extract_alias_column_refs_tokenized("   \t\n   ");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_alias_col_no_match() {
        // No alias.[column] patterns - should return empty
        let refs = extract_alias_column_refs_tokenized("[schema].[table].[column]");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_alias_col_skip_bracketed_alias() {
        // [alias].[column] is a different pattern (TwoPartBracketed)
        let refs = extract_alias_column_refs_tokenized("[t].[Column]");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_alias_col_underscore_alias() {
        // Alias starting with underscore
        let refs = extract_alias_column_refs_tokenized("_temp.[Value]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("_temp".to_string(), "Value".to_string()));
    }

    #[test]
    fn test_alias_col_long_alias() {
        // Longer alias name
        let refs = extract_alias_column_refs_tokenized("inserted.[ProductId]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("inserted".to_string(), "ProductId".to_string()));
    }

    #[test]
    fn test_alias_col_special_chars_in_column() {
        // Column name with spaces and special chars in brackets
        let refs = extract_alias_column_refs_tokenized("t.[Column Name]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("t".to_string(), "Column Name".to_string()));
    }

    #[test]
    fn test_alias_col_mixed_patterns() {
        // Mix of alias.[col] with other patterns - only alias.[col] extracted
        let refs = extract_alias_column_refs_tokenized("t.[Id] AND [schema].[table] AND u.[Name]");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], ("t".to_string(), "Id".to_string()));
        assert_eq!(refs[1], ("u".to_string(), "Name".to_string()));
    }

    #[test]
    fn test_alias_col_with_newlines() {
        // Newlines in the SQL
        let refs = extract_alias_column_refs_tokenized("i.[Id]\nAND\nd.[Name]");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], ("i".to_string(), "Id".to_string()));
        assert_eq!(refs[1], ("d".to_string(), "Name".to_string()));
    }

    #[test]
    fn test_alias_col_complex_expression() {
        // Complex expression with multiple patterns
        let refs = extract_alias_column_refs_tokenized(
            "CASE WHEN i.[Status] = 1 THEN d.[OldValue] ELSE i.[NewValue] END",
        );
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0], ("i".to_string(), "Status".to_string()));
        assert_eq!(refs[1], ("d".to_string(), "OldValue".to_string()));
        assert_eq!(refs[2], ("i".to_string(), "NewValue".to_string()));
    }

    // ============================================================================
    // Tests for extract_single_bracketed_identifiers (Phase 20.2.6)
    // ============================================================================

    #[test]
    fn test_single_bracketed_simple() {
        // Single bracketed identifier
        let idents = extract_single_bracketed_identifiers("[Column1]");
        assert_eq!(idents.len(), 1);
        assert_eq!(idents[0], "Column1");
    }

    #[test]
    fn test_single_bracketed_multiple() {
        // Multiple bracketed identifiers in a column list
        let idents = extract_single_bracketed_identifiers("[Col1], [Col2], [Col3]");
        assert_eq!(idents.len(), 3);
        assert_eq!(idents[0], "Col1");
        assert_eq!(idents[1], "Col2");
        assert_eq!(idents[2], "Col3");
    }

    #[test]
    fn test_single_bracketed_with_spaces() {
        // Spaces between columns
        let idents = extract_single_bracketed_identifiers("[Id]  ,  [Name]  ,  [Value]");
        assert_eq!(idents.len(), 3);
        assert_eq!(idents[0], "Id");
        assert_eq!(idents[1], "Name");
        assert_eq!(idents[2], "Value");
    }

    #[test]
    fn test_single_bracketed_with_tabs() {
        // Tabs between columns
        let idents = extract_single_bracketed_identifiers("[Id]\t,\t[Name]\t,\t[Value]");
        assert_eq!(idents.len(), 3);
        assert_eq!(idents[0], "Id");
        assert_eq!(idents[1], "Name");
        assert_eq!(idents[2], "Value");
    }

    #[test]
    fn test_single_bracketed_with_newlines() {
        // Newlines in the SQL
        let idents = extract_single_bracketed_identifiers("[Col1],\n[Col2],\n[Col3]");
        assert_eq!(idents.len(), 3);
        assert_eq!(idents[0], "Col1");
        assert_eq!(idents[1], "Col2");
        assert_eq!(idents[2], "Col3");
    }

    #[test]
    fn test_single_bracketed_special_chars() {
        // Column name with spaces in brackets
        let idents = extract_single_bracketed_identifiers("[Column Name], [Another Column]");
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0], "Column Name");
        assert_eq!(idents[1], "Another Column");
    }

    #[test]
    fn test_single_bracketed_empty() {
        let idents = extract_single_bracketed_identifiers("");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_single_bracketed_whitespace_only() {
        let idents = extract_single_bracketed_identifiers("   \t\n   ");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_single_bracketed_skip_two_part() {
        // Two-part bracketed names should NOT produce SingleBracketed tokens
        // [schema].[table] produces TwoPartBracketed, not two SingleBracketed
        let idents = extract_single_bracketed_identifiers("[dbo].[Users]");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_single_bracketed_skip_three_part() {
        // Three-part names should NOT produce SingleBracketed tokens
        let idents = extract_single_bracketed_identifiers("[dbo].[Users].[Id]");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_single_bracketed_skip_alias_dot_column() {
        // alias.[column] produces AliasDotBracketedColumn, not SingleBracketed
        let idents = extract_single_bracketed_identifiers("t.[Column]");
        assert!(idents.is_empty());
    }

    #[test]
    fn test_single_bracketed_insert_column_list() {
        // Typical INSERT column list
        let idents =
            extract_single_bracketed_identifiers("[ProductId], [ProductName], [Price], [Stock]");
        assert_eq!(idents.len(), 4);
        assert_eq!(idents[0], "ProductId");
        assert_eq!(idents[1], "ProductName");
        assert_eq!(idents[2], "Price");
        assert_eq!(idents[3], "Stock");
    }

    #[test]
    fn test_single_bracketed_mixed_pattern() {
        // Mix of single bracketed with other patterns - only extract singles
        let idents =
            extract_single_bracketed_identifiers("[Col1], alias.[Col2], [Col3], [dbo].[Table]");
        // [Col1] and [Col3] are single, alias.[Col2] is AliasDotBracketed, [dbo].[Table] is TwoPartBracketed
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0], "Col1");
        assert_eq!(idents[1], "Col3");
    }

    #[test]
    fn test_single_bracketed_select_clause() {
        // SELECT clause - typical trigger usage
        let idents = extract_single_bracketed_identifiers("SELECT [Id], [Name], [Value]");
        assert_eq!(idents.len(), 3);
        assert_eq!(idents[0], "Id");
        assert_eq!(idents[1], "Name");
        assert_eq!(idents[2], "Value");
    }

    #[test]
    fn test_single_bracketed_preserves_order() {
        // Order should be preserved
        let idents = extract_single_bracketed_identifiers("[Z], [A], [M], [B]");
        assert_eq!(idents.len(), 4);
        assert_eq!(idents[0], "Z");
        assert_eq!(idents[1], "A");
        assert_eq!(idents[2], "M");
        assert_eq!(idents[3], "B");
    }

    #[test]
    fn test_single_bracketed_numeric_name() {
        // Numeric-looking column name
        let idents = extract_single_bracketed_identifiers("[123], [456]");
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0], "123");
        assert_eq!(idents[1], "456");
    }

    #[test]
    fn test_single_bracketed_unicode() {
        // Unicode in column name
        let idents = extract_single_bracketed_identifiers("[名前], [価格]");
        assert_eq!(idents.len(), 2);
        assert_eq!(idents[0], "名前");
        assert_eq!(idents[1], "価格");
    }

    // ============================================================================
    // Tests for extract_column_aliases_tokenized (Phase 20.2.7)
    // ============================================================================

    #[test]
    fn test_column_alias_simple() {
        // Basic AS alias pattern
        let aliases = extract_column_aliases_tokenized("SELECT col AS alias");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], "alias");
    }

    #[test]
    fn test_column_alias_bracketed() {
        // AS [alias] pattern with brackets
        let aliases = extract_column_aliases_tokenized("SELECT col AS [MyAlias]");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], "myalias");
    }

    #[test]
    fn test_column_alias_multiple() {
        // Multiple aliases in SELECT
        let aliases =
            extract_column_aliases_tokenized("SELECT a.Id AS Id1, b.Name AS Name2, c.Val AS Val3");
        assert_eq!(aliases.len(), 3);
        assert_eq!(aliases[0], "id1");
        assert_eq!(aliases[1], "name2");
        assert_eq!(aliases[2], "val3");
    }

    #[test]
    fn test_column_alias_with_tabs() {
        // Tabs instead of spaces
        let aliases = extract_column_aliases_tokenized("SELECT col\tAS\talias");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], "alias");
    }

    #[test]
    fn test_column_alias_with_multiple_spaces() {
        // Multiple spaces
        let aliases = extract_column_aliases_tokenized("SELECT col   AS   alias");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], "alias");
    }

    #[test]
    fn test_column_alias_with_newlines() {
        // Newlines between tokens
        let aliases = extract_column_aliases_tokenized("SELECT col\nAS\nalias");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], "alias");
    }

    #[test]
    fn test_column_alias_case_insensitive() {
        // AS keyword is case-insensitive
        let aliases = extract_column_aliases_tokenized("SELECT col as alias1, val As alias2");
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0], "alias1");
        assert_eq!(aliases[1], "alias2");
    }

    #[test]
    fn test_column_alias_skip_keywords() {
        // SQL keywords after AS should be skipped
        let aliases = extract_column_aliases_tokenized("SELECT col AS FROM");
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_column_alias_skip_join_keyword() {
        // JOIN keyword after AS should be skipped
        let aliases = extract_column_aliases_tokenized("SELECT col AS LEFT");
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_column_alias_skip_null_keyword() {
        // NULL keyword after AS should be skipped
        let aliases = extract_column_aliases_tokenized("SELECT col AS NULL");
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_column_alias_count_function() {
        // COUNT(*) AS alias pattern
        let aliases = extract_column_aliases_tokenized("SELECT COUNT(*) AS Occurrences");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], "occurrences");
    }

    #[test]
    fn test_column_alias_qualified_column() {
        // Qualified column AS alias
        let aliases = extract_column_aliases_tokenized("SELECT A.Id AS AccountBusinessKey");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], "accountbusinesskey");
    }

    #[test]
    fn test_column_alias_empty() {
        let aliases = extract_column_aliases_tokenized("");
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_column_alias_no_aliases() {
        // SELECT without aliases
        let aliases = extract_column_aliases_tokenized("SELECT col1, col2, col3");
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_column_alias_mixed() {
        // Mix of aliased and non-aliased columns
        let aliases = extract_column_aliases_tokenized("SELECT col1, col2 AS alias2, col3");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], "alias2");
    }

    #[test]
    fn test_column_alias_complex_expression() {
        // Complex expression with AS
        let aliases = extract_column_aliases_tokenized(
            "SELECT CASE WHEN a = 1 THEN b ELSE c END AS Result, d + e AS Total",
        );
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0], "result");
        assert_eq!(aliases[1], "total");
    }

    #[test]
    fn test_column_alias_underscore() {
        // Alias with underscore
        let aliases = extract_column_aliases_tokenized("SELECT col AS my_alias");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], "my_alias");
    }

    // ============================================================================
    // QualifiedName and parse_qualified_name_tokenized tests (Phase 20.2.8)
    // ============================================================================

    #[test]
    fn test_qualified_name_single_bracketed() {
        let qn = parse_qualified_name_tokenized("[TableName]").unwrap();
        assert_eq!(qn.part_count(), 1);
        assert_eq!(qn.first, "TableName");
        assert!(qn.second.is_none());
        assert!(qn.third.is_none());
        assert_eq!(qn.last_part(), "TableName");
        assert_eq!(qn.to_bracketed(), "[TableName]");
    }

    #[test]
    fn test_qualified_name_single_unbracketed() {
        let qn = parse_qualified_name_tokenized("TableName").unwrap();
        assert_eq!(qn.part_count(), 1);
        assert_eq!(qn.first, "TableName");
        assert!(qn.second.is_none());
        assert_eq!(qn.last_part(), "TableName");
    }

    #[test]
    fn test_qualified_name_two_part_bracketed() {
        let qn = parse_qualified_name_tokenized("[dbo].[Products]").unwrap();
        assert_eq!(qn.part_count(), 2);
        assert_eq!(qn.first, "dbo");
        assert_eq!(qn.second.as_deref(), Some("Products"));
        assert!(qn.third.is_none());
        assert_eq!(qn.last_part(), "Products");
        assert_eq!(qn.schema_and_table(), Some(("dbo", "Products")));
        assert_eq!(qn.to_bracketed(), "[dbo].[Products]");
    }

    #[test]
    fn test_qualified_name_two_part_unbracketed() {
        let qn = parse_qualified_name_tokenized("dbo.Products").unwrap();
        assert_eq!(qn.part_count(), 2);
        assert_eq!(qn.first, "dbo");
        assert_eq!(qn.second.as_deref(), Some("Products"));
        assert_eq!(qn.last_part(), "Products");
    }

    #[test]
    fn test_qualified_name_three_part_bracketed() {
        let qn = parse_qualified_name_tokenized("[dbo].[Products].[Id]").unwrap();
        assert_eq!(qn.part_count(), 3);
        assert_eq!(qn.first, "dbo");
        assert_eq!(qn.second.as_deref(), Some("Products"));
        assert_eq!(qn.third.as_deref(), Some("Id"));
        assert_eq!(qn.last_part(), "Id");
        assert_eq!(qn.to_bracketed(), "[dbo].[Products].[Id]");
    }

    #[test]
    fn test_qualified_name_mixed_alias_dot_bracketed() {
        // alias.[column] pattern
        let qn = parse_qualified_name_tokenized("t.[Name]").unwrap();
        assert_eq!(qn.part_count(), 2);
        assert_eq!(qn.first, "t");
        assert_eq!(qn.second.as_deref(), Some("Name"));
        assert_eq!(qn.last_part(), "Name");
    }

    #[test]
    fn test_qualified_name_mixed_bracketed_dot_unbracketed() {
        // [alias].column pattern
        let qn = parse_qualified_name_tokenized("[t].Name").unwrap();
        assert_eq!(qn.part_count(), 2);
        assert_eq!(qn.first, "t");
        assert_eq!(qn.second.as_deref(), Some("Name"));
        assert_eq!(qn.last_part(), "Name");
    }

    #[test]
    fn test_qualified_name_with_whitespace() {
        // Tokenizer should handle spaces between parts
        let qn = parse_qualified_name_tokenized("[dbo] . [Products]").unwrap();
        assert_eq!(qn.part_count(), 2);
        assert_eq!(qn.first, "dbo");
        assert_eq!(qn.second.as_deref(), Some("Products"));
    }

    #[test]
    fn test_qualified_name_with_tabs() {
        // Tokenizer should handle tabs between parts
        let qn = parse_qualified_name_tokenized("[dbo]\t.\t[Products]").unwrap();
        assert_eq!(qn.part_count(), 2);
        assert_eq!(qn.first, "dbo");
        assert_eq!(qn.second.as_deref(), Some("Products"));
    }

    #[test]
    fn test_qualified_name_with_special_chars() {
        // Names with spaces inside brackets
        let qn = parse_qualified_name_tokenized("[dbo].[My Table Name]").unwrap();
        assert_eq!(qn.part_count(), 2);
        assert_eq!(qn.first, "dbo");
        assert_eq!(qn.second.as_deref(), Some("My Table Name"));
    }

    #[test]
    fn test_qualified_name_empty() {
        assert!(parse_qualified_name_tokenized("").is_none());
    }

    #[test]
    fn test_qualified_name_whitespace_only() {
        assert!(parse_qualified_name_tokenized("   ").is_none());
    }

    #[test]
    fn test_qualified_name_parameter_returns_none() {
        // Parameters are not qualified names
        assert!(parse_qualified_name_tokenized("@param").is_none());
    }

    #[test]
    fn test_normalize_type_name_already_bracketed() {
        assert_eq!(normalize_type_name("[dbo].[MyType]"), "[dbo].[MyType]");
    }

    #[test]
    fn test_normalize_type_name_unbracketed() {
        assert_eq!(normalize_type_name("dbo.MyType"), "[dbo].[MyType]");
    }

    #[test]
    fn test_normalize_type_name_no_schema() {
        // Can't normalize single-part type without schema
        assert_eq!(normalize_type_name("MyType"), "MyType");
    }

    #[test]
    fn test_extract_column_name_from_expr_simple_qualified() {
        assert_eq!(
            extract_column_name_from_expr_simple("[dbo].[Products].[Id]"),
            "Id"
        );
    }

    #[test]
    fn test_extract_column_name_from_expr_simple_alias() {
        assert_eq!(extract_column_name_from_expr_simple("t.[Name]"), "Name");
    }

    #[test]
    fn test_extract_column_name_from_expr_simple_single() {
        assert_eq!(extract_column_name_from_expr_simple("[Id]"), "Id");
    }

    #[test]
    fn test_extract_column_name_from_expr_simple_function() {
        // Functions should be returned as-is
        assert_eq!(extract_column_name_from_expr_simple("COUNT(*)"), "COUNT(*)");
    }

    // ============================================================================
    // extract_declare_types_tokenized tests (Phase 20.3.1)
    // ============================================================================

    #[test]
    fn test_declare_type_simple_int() {
        let types = extract_declare_types_tokenized("DECLARE @Count INT");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_simple_nvarchar() {
        let types = extract_declare_types_tokenized("DECLARE @Name NVARCHAR(50)");
        assert_eq!(types, vec!["nvarchar"]);
    }

    #[test]
    fn test_declare_type_decimal_with_precision() {
        let types = extract_declare_types_tokenized("DECLARE @Total DECIMAL(18, 2)");
        assert_eq!(types, vec!["decimal"]);
    }

    #[test]
    fn test_declare_type_multiple_variables() {
        let types = extract_declare_types_tokenized(
            "DECLARE @Count INT; DECLARE @Name NVARCHAR(100); DECLARE @Value DECIMAL(10,2)",
        );
        assert_eq!(types, vec!["int", "nvarchar", "decimal"]);
    }

    #[test]
    fn test_declare_type_with_tabs() {
        let types = extract_declare_types_tokenized("DECLARE\t@Count\tINT");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_with_multiple_spaces() {
        let types = extract_declare_types_tokenized("DECLARE   @Count   INT");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_with_newlines() {
        let types = extract_declare_types_tokenized("DECLARE\n@Count\nINT");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_mixed_whitespace() {
        let types = extract_declare_types_tokenized("DECLARE \t @Count \n INT");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_case_insensitive() {
        let types = extract_declare_types_tokenized("declare @count int");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_mixed_case() {
        let types = extract_declare_types_tokenized("Declare @Count Int");
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_empty() {
        let types = extract_declare_types_tokenized("");
        assert!(types.is_empty());
    }

    #[test]
    fn test_declare_type_no_declare() {
        let types = extract_declare_types_tokenized("SELECT * FROM Table");
        assert!(types.is_empty());
    }

    #[test]
    fn test_declare_type_datetime() {
        let types = extract_declare_types_tokenized("DECLARE @Date DATETIME");
        assert_eq!(types, vec!["datetime"]);
    }

    #[test]
    fn test_declare_type_varchar_max() {
        let types = extract_declare_types_tokenized("DECLARE @Content VARCHAR(MAX)");
        assert_eq!(types, vec!["varchar"]);
    }

    #[test]
    fn test_declare_type_bit() {
        let types = extract_declare_types_tokenized("DECLARE @Active BIT");
        assert_eq!(types, vec!["bit"]);
    }

    #[test]
    fn test_declare_type_in_function_body() {
        let body = r#"
            DECLARE @Count INT;
            SET @Count = (SELECT COUNT(*) FROM Users);
            RETURN @Count;
        "#;
        let types = extract_declare_types_tokenized(body);
        assert_eq!(types, vec!["int"]);
    }

    #[test]
    fn test_declare_type_multiple_in_procedure_body() {
        let body = r#"
            DECLARE @Total DECIMAL(18, 2);
            DECLARE @Count INT;
            DECLARE @Result NVARCHAR(100);

            SELECT @Count = COUNT(*) FROM Orders;
            SELECT @Total = SUM(Amount) FROM Orders;
            SET @Result = CAST(@Count AS NVARCHAR) + ' orders totaling ' + CAST(@Total AS NVARCHAR);
            SELECT @Result;
        "#;
        let types = extract_declare_types_tokenized(body);
        assert_eq!(types, vec!["decimal", "int", "nvarchar"]);
    }

    // ============================================================================
    // parse_tvf_column_type_tokenized tests (Phase 20.3.2)
    // ============================================================================

    #[test]
    fn test_tvf_type_simple_int() {
        let result = parse_tvf_column_type_tokenized("INT");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "int".to_string(),
                first_num: None,
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_simple_nvarchar() {
        let result = parse_tvf_column_type_tokenized("NVARCHAR");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "nvarchar".to_string(),
                first_num: None,
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_nvarchar_with_length() {
        let result = parse_tvf_column_type_tokenized("NVARCHAR(100)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "nvarchar".to_string(),
                first_num: Some(100),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_varchar_with_length() {
        let result = parse_tvf_column_type_tokenized("VARCHAR(50)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "varchar".to_string(),
                first_num: Some(50),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_decimal_with_precision_scale() {
        let result = parse_tvf_column_type_tokenized("DECIMAL(18, 2)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "decimal".to_string(),
                first_num: Some(18),
                second_num: Some(2),
            })
        );
    }

    #[test]
    fn test_tvf_type_decimal_no_spaces() {
        let result = parse_tvf_column_type_tokenized("DECIMAL(18,2)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "decimal".to_string(),
                first_num: Some(18),
                second_num: Some(2),
            })
        );
    }

    #[test]
    fn test_tvf_type_numeric_with_precision() {
        let result = parse_tvf_column_type_tokenized("NUMERIC(10, 0)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "numeric".to_string(),
                first_num: Some(10),
                second_num: Some(0),
            })
        );
    }

    #[test]
    fn test_tvf_type_varchar_max() {
        let result = parse_tvf_column_type_tokenized("VARCHAR(MAX)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "varchar".to_string(),
                first_num: Some(u32::MAX),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_nvarchar_max() {
        let result = parse_tvf_column_type_tokenized("NVARCHAR(MAX)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "nvarchar".to_string(),
                first_num: Some(u32::MAX),
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_case_insensitive() {
        let result = parse_tvf_column_type_tokenized("decimal(18, 2)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "decimal".to_string(),
                first_num: Some(18),
                second_num: Some(2),
            })
        );
    }

    #[test]
    fn test_tvf_type_mixed_case() {
        let result = parse_tvf_column_type_tokenized("Decimal(18, 2)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "decimal".to_string(),
                first_num: Some(18),
                second_num: Some(2),
            })
        );
    }

    #[test]
    fn test_tvf_type_with_tabs() {
        let result = parse_tvf_column_type_tokenized("DECIMAL(\t18\t,\t2\t)");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "decimal".to_string(),
                first_num: Some(18),
                second_num: Some(2),
            })
        );
    }

    #[test]
    fn test_tvf_type_with_multiple_spaces() {
        let result = parse_tvf_column_type_tokenized("DECIMAL(   18   ,   2   )");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "decimal".to_string(),
                first_num: Some(18),
                second_num: Some(2),
            })
        );
    }

    #[test]
    fn test_tvf_type_empty() {
        let result = parse_tvf_column_type_tokenized("");
        assert_eq!(result, None);
    }

    #[test]
    fn test_tvf_type_whitespace_only() {
        let result = parse_tvf_column_type_tokenized("   ");
        assert_eq!(result, None);
    }

    #[test]
    fn test_tvf_type_datetime() {
        let result = parse_tvf_column_type_tokenized("DATETIME");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "datetime".to_string(),
                first_num: None,
                second_num: None,
            })
        );
    }

    #[test]
    fn test_tvf_type_bit() {
        let result = parse_tvf_column_type_tokenized("BIT");
        assert_eq!(
            result,
            Some(TvfColumnTypeInfo {
                data_type: "bit".to_string(),
                first_num: None,
                second_num: None,
            })
        );
    }

    // ==========================================
    // Phase 20.3.3: CAST Expression Tokenized Tests
    // ==========================================

    #[test]
    fn test_cast_expr_simple_int() {
        let result = extract_cast_expressions_tokenized("CAST([Value] AS INT)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "int");
        assert_eq!(result[0].cast_start, 0);
    }

    #[test]
    fn test_cast_expr_simple_nvarchar() {
        let result = extract_cast_expressions_tokenized("CAST([Name] AS NVARCHAR)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "nvarchar");
    }

    #[test]
    fn test_cast_expr_with_length() {
        // The type name is captured as just the base type (nvarchar), not including (100)
        let result = extract_cast_expressions_tokenized("CAST([Name] AS NVARCHAR(100))");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "nvarchar");
    }

    #[test]
    fn test_cast_expr_lowercase() {
        let result = extract_cast_expressions_tokenized("cast([value] as varchar)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "varchar");
    }

    #[test]
    fn test_cast_expr_mixed_case() {
        let result = extract_cast_expressions_tokenized("Cast([Value] As NVarChar)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "nvarchar");
    }

    #[test]
    fn test_cast_expr_with_whitespace() {
        let result = extract_cast_expressions_tokenized("CAST  (  [Value]   AS   INT  )");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "int");
    }

    #[test]
    fn test_cast_expr_with_tabs() {
        let result = extract_cast_expressions_tokenized("CAST\t(\t[Value]\tAS\tINT\t)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "int");
    }

    #[test]
    fn test_cast_expr_with_newlines() {
        let result = extract_cast_expressions_tokenized("CAST(\n    [Value]\n    AS\n    INT\n)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "int");
    }

    #[test]
    fn test_cast_expr_multiple() {
        let result = extract_cast_expressions_tokenized(
            "CAST([A] AS INT) + CAST([B] AS VARCHAR) + CAST([C] AS DECIMAL)",
        );
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].type_name, "int");
        assert_eq!(result[1].type_name, "varchar");
        assert_eq!(result[2].type_name, "decimal");
    }

    #[test]
    fn test_cast_expr_nested_function() {
        // CAST with a function call inside - should still find the AS type
        let result = extract_cast_expressions_tokenized("CAST(LEN([Name]) AS INT)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "int");
    }

    #[test]
    fn test_cast_expr_nested_parens() {
        // Expression with nested parentheses inside CAST
        let result =
            extract_cast_expressions_tokenized("CAST(([A] + [B]) * ([C] - [D]) AS DECIMAL)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "decimal");
    }

    #[test]
    fn test_cast_expr_no_cast() {
        // No CAST expression
        let result = extract_cast_expressions_tokenized("[A] + [B]");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_cast_expr_in_expression() {
        // CAST in a larger expression
        let result = extract_cast_expressions_tokenized("[Quantity] * CAST([Price] AS MONEY)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "money");
    }

    #[test]
    fn test_cast_expr_position_ordering() {
        // Verify positions are correct for ordering
        let result = extract_cast_expressions_tokenized("ABC CAST([X] AS INT) DEF");
        assert_eq!(result.len(), 1);
        // CAST starts at position 4 (after "ABC ")
        assert_eq!(result[0].cast_start, 4);
        assert_eq!(result[0].cast_keyword_pos, 4);
    }

    #[test]
    fn test_cast_expr_decimal_with_precision() {
        let result = extract_cast_expressions_tokenized("CAST([Value] AS DECIMAL(18,2))");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_name, "decimal");
    }

    #[test]
    fn test_cast_expr_empty_string() {
        let result = extract_cast_expressions_tokenized("");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_cast_expr_whitespace_only() {
        let result = extract_cast_expressions_tokenized("   \t\n   ");
        assert_eq!(result.len(), 0);
    }

    // ========== Phase 20.4.2: Trigger Alias Token Extraction Tests ==========
    // These tests verify that TableAliasTokenParser correctly extracts table aliases
    // for trigger body dependency analysis (replacing TRIGGER_ALIAS_RE regex).

    #[test]
    fn test_trigger_alias_basic_from() {
        // Basic FROM clause with alias
        let sql = "FROM [dbo].[Products] p";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        // Should have both "p" and "Products" as keys
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
        assert_eq!(
            alias_map.get("Products"),
            Some(&"[dbo].[Products]".to_string())
        );
    }

    #[test]
    fn test_trigger_alias_basic_join() {
        // JOIN clause with alias
        let sql = "FROM [dbo].[Orders] o JOIN [dbo].[Products] p ON o.[ProductId] = p.[Id]";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("o"), Some(&"[dbo].[Orders]".to_string()));
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
    }

    #[test]
    fn test_trigger_alias_with_tabs() {
        // Tabs instead of spaces (edge case that regex would fail on)
        let sql = "FROM\t[dbo].[Products]\tp";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
    }

    #[test]
    fn test_trigger_alias_with_newlines() {
        // Newlines in statement
        let sql = "FROM\n    [dbo].[Products]\n    p";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
    }

    #[test]
    fn test_trigger_alias_multiple_spaces() {
        // Multiple spaces between tokens (edge case that single \s+ regex handles but fragile)
        let sql = "FROM   [dbo].[Products]   prod";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("prod"), Some(&"[dbo].[Products]".to_string()));
    }

    #[test]
    fn test_trigger_alias_inner_join() {
        // INNER JOIN keyword
        let sql =
            "FROM [dbo].[Products] p INNER JOIN [dbo].[Categories] c ON p.[CategoryId] = c.[Id]";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
        assert_eq!(alias_map.get("c"), Some(&"[dbo].[Categories]".to_string()));
    }

    #[test]
    fn test_trigger_alias_left_join() {
        // LEFT JOIN keyword
        let sql =
            "FROM [dbo].[Products] p LEFT JOIN [dbo].[Categories] c ON p.[CategoryId] = c.[Id]";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
        assert_eq!(alias_map.get("c"), Some(&"[dbo].[Categories]".to_string()));
    }

    #[test]
    fn test_trigger_alias_custom_schema() {
        // Non-dbo schema
        let sql = "FROM [sales].[Products] p";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[sales].[Products]".to_string()));
    }

    #[test]
    fn test_trigger_alias_with_as_keyword() {
        // Using AS keyword
        let sql = "FROM [dbo].[Products] AS p";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
    }

    #[test]
    fn test_trigger_alias_no_alias() {
        // Table without alias - should still include table name as key
        let sql = "FROM [dbo].[Products]";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(
            alias_map.get("Products"),
            Some(&"[dbo].[Products]".to_string())
        );
    }

    #[test]
    fn test_trigger_alias_unbracketed_table() {
        // Unbracketed table name (should still work)
        let sql = "FROM dbo.Products p";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
    }

    #[test]
    fn test_trigger_alias_multiple_joins() {
        // Multiple JOINs
        let sql = "FROM [dbo].[Orders] o \
                   JOIN [dbo].[Products] p ON o.[ProductId] = p.[Id] \
                   JOIN [dbo].[Categories] c ON p.[CategoryId] = c.[Id]";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("o"), Some(&"[dbo].[Orders]".to_string()));
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
        assert_eq!(alias_map.get("c"), Some(&"[dbo].[Categories]".to_string()));
    }

    #[test]
    fn test_trigger_alias_empty_string() {
        let parser = TableAliasTokenParser::new("");
        assert!(parser.is_some());
        let mut parser = parser.unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_trigger_alias_whitespace_only() {
        let parser = TableAliasTokenParser::new("   \t\n   ");
        assert!(parser.is_some());
        let mut parser = parser.unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_trigger_alias_case_insensitive_from() {
        // Case insensitive FROM keyword
        let sql = "from [dbo].[Products] p";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
    }

    #[test]
    fn test_trigger_alias_case_insensitive_join() {
        // Case insensitive JOIN keyword
        let sql = "FROM [dbo].[Products] p join [dbo].[Categories] c ON p.[CatId] = c.[Id]";
        let mut parser = TableAliasTokenParser::new(sql).unwrap();
        let aliases = parser.extract_aliases_with_table_names();
        let alias_map: std::collections::HashMap<_, _> = aliases.into_iter().collect();
        assert_eq!(alias_map.get("p"), Some(&"[dbo].[Products]".to_string()));
        assert_eq!(alias_map.get("c"), Some(&"[dbo].[Categories]".to_string()));
    }

    // =============================================================================
    // Phase 20.4.3: extract_table_refs_tokenized() Tests
    // =============================================================================

    #[test]
    fn test_extract_table_refs_tokenized_bracketed() {
        // Basic bracketed table reference
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized("SELECT * FROM [dbo].[Employees]", &aliases);
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_unbracketed() {
        // Basic unbracketed table reference
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized("SELECT * FROM dbo.Employees", &aliases);
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_mixed() {
        // Mix of bracketed and unbracketed
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized(
            "SELECT * FROM [dbo].[Employees] e JOIN sales.Orders o ON e.Id = o.EmployeeId",
            &aliases,
        );
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
        assert!(refs.contains(&"[sales].[Orders]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_skip_aliases() {
        // Should skip alias.column references
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("e".to_string(), "[dbo].[Employees]".to_string());
        let refs = extract_table_refs_tokenized("SELECT e.Name FROM [dbo].[Employees] e", &aliases);
        // Should contain the table but not treat e.Name as a table
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
        assert!(!refs.contains(&"[e].[Name]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_skip_parameters() {
        // Should skip @ prefixed identifiers
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized(
            "SELECT * FROM [dbo].[Employees] WHERE [@Schema].[@Table] = 1",
            &aliases,
        );
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
        // Parameters with @ should be excluded
        assert!(!refs.iter().any(|r| r.contains("@")));
    }

    #[test]
    fn test_extract_table_refs_tokenized_skip_keywords() {
        // Should skip keyword.something patterns like FROM.anything
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized("SELECT * FROM dbo.Employees", &aliases);
        // Should not treat FROM as a schema
        assert!(!refs.contains(&"[FROM].[dbo]".to_string()));
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_whitespace() {
        // Handles whitespace between parts
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized("SELECT * FROM [dbo] . [Employees]", &aliases);
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_tabs() {
        // Handles tabs and newlines
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized("SELECT * FROM [dbo]\t.\n[Employees]", &aliases);
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_three_part() {
        // Extracts table from three-part references [schema].[table].[column]
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized(
            "SELECT [dbo].[Employees].[Name] FROM [dbo].[Employees]",
            &aliases,
        );
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_deduplicates() {
        // Deduplicates repeated references
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized(
            "SELECT * FROM [dbo].[Employees] WHERE EXISTS (SELECT 1 FROM [dbo].[Employees])",
            &aliases,
        );
        // Should only appear once
        assert_eq!(refs.iter().filter(|r| *r == "[dbo].[Employees]").count(), 1);
    }

    #[test]
    fn test_extract_table_refs_tokenized_multiple_tables() {
        // Multiple different tables
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized(
            "SELECT * FROM [dbo].[Employees] e
             JOIN [dbo].[Departments] d ON e.DeptId = d.Id
             JOIN [hr].[Managers] m ON d.ManagerId = m.Id",
            &aliases,
        );
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
        assert!(refs.contains(&"[dbo].[Departments]".to_string()));
        assert!(refs.contains(&"[hr].[Managers]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_alias_dot_bracketed() {
        // alias.[column] pattern - should not be treated as table if alias is known
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("e".to_string(), "[dbo].[Employees]".to_string());
        let refs = extract_table_refs_tokenized(
            "SELECT e.[Name], e.[Age] FROM [dbo].[Employees] e",
            &aliases,
        );
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
        // e.[Name] should NOT be treated as a table reference
        assert!(!refs.contains(&"[e].[Name]".to_string()));
        assert!(!refs.contains(&"[e].[Age]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_bracketed_alias_dot_column() {
        // [alias].column pattern - should not be treated as table if alias is known
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("e".to_string(), "[dbo].[Employees]".to_string());
        let refs = extract_table_refs_tokenized(
            "SELECT [e].Name, [e].Age FROM [dbo].[Employees] e",
            &aliases,
        );
        assert!(refs.contains(&"[dbo].[Employees]".to_string()));
        // [e].Name should NOT be treated as a table reference
        assert!(!refs.contains(&"[e].[Name]".to_string()));
    }

    #[test]
    fn test_extract_table_refs_tokenized_empty() {
        // Empty body
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized("", &aliases);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_table_refs_tokenized_no_tables() {
        // SQL with no table references
        let aliases = std::collections::HashMap::new();
        let refs = extract_table_refs_tokenized("SELECT 1 + 2 AS Result", &aliases);
        assert!(refs.is_empty());
    }

    // =============================================================================
    // Phase 20.4.3: parse_qualified_table_name() Tests (Tokenized Version)
    // =============================================================================

    #[test]
    fn test_parse_qualified_table_name_tokenized_basic() {
        // Basic bracketed qualified name
        let result = parse_qualified_table_name("[dbo].[Employees]");
        assert_eq!(result, Some(("dbo".to_string(), "Employees".to_string())));
    }

    #[test]
    fn test_parse_qualified_table_name_tokenized_whitespace() {
        // With whitespace between parts
        let result = parse_qualified_table_name("[dbo] . [Employees]");
        assert_eq!(result, Some(("dbo".to_string(), "Employees".to_string())));
    }

    #[test]
    fn test_parse_qualified_table_name_tokenized_tabs() {
        // With tabs and newlines
        let result = parse_qualified_table_name("[dbo]\t.\n[Employees]");
        assert_eq!(result, Some(("dbo".to_string(), "Employees".to_string())));
    }

    #[test]
    fn test_parse_qualified_table_name_tokenized_unbracketed() {
        // Unbracketed identifiers work too
        let result = parse_qualified_table_name("dbo.Employees");
        assert_eq!(result, Some(("dbo".to_string(), "Employees".to_string())));
    }

    #[test]
    fn test_parse_qualified_table_name_tokenized_mixed() {
        // Mixed bracketed/unbracketed
        let result = parse_qualified_table_name("[dbo].Employees");
        assert_eq!(result, Some(("dbo".to_string(), "Employees".to_string())));
    }

    #[test]
    fn test_parse_qualified_table_name_tokenized_single() {
        // Single part returns None (needs two parts)
        let result = parse_qualified_table_name("[Employees]");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_qualified_table_name_tokenized_three_part() {
        // Three-part name - returns schema and table (ignores column)
        let result = parse_qualified_table_name("[dbo].[Employees].[Name]");
        // schema_and_table returns first two parts
        assert_eq!(result, Some(("dbo".to_string(), "Employees".to_string())));
    }

    #[test]
    fn test_parse_qualified_table_name_tokenized_empty() {
        let result = parse_qualified_table_name("");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_qualified_table_name_tokenized_whitespace_only() {
        let result = parse_qualified_table_name("   \t\n   ");
        assert_eq!(result, None);
    }

    // =============================================================================
    // Phase 20.4.6: InsertSelectTokenParser Tests
    // =============================================================================
    // Tests for token-based INSERT...SELECT parsing (replaces INSERT_SELECT_RE and
    // INSERT_SELECT_JOIN_RE regex patterns).

    #[test]
    fn test_insert_select_parser_simple() {
        // Simple INSERT INTO ... SELECT FROM inserted;
        let sql = r#"
            INSERT INTO [dbo].[Products] ([Id], [Name], [Price])
            SELECT [Id], [Name], [Price]
            FROM inserted;
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.schema, "dbo");
        assert_eq!(stmt.table, "Products");
        assert!(stmt.column_list.contains("[Id]"));
        assert!(stmt.column_list.contains("[Name]"));
        assert!(!stmt.has_join);
        assert!(stmt.alias1.is_none());
        assert!(stmt.alias2.is_none());
        assert!(stmt.on_clause.is_none());
    }

    #[test]
    fn test_insert_select_parser_with_join() {
        // INSERT INTO ... SELECT FROM inserted i INNER JOIN deleted d ON ...;
        let sql = r#"
            INSERT INTO [dbo].[ProductHistory] ([ProductId], [Action], [OldName], [NewName])
            SELECT i.[Id], 'UPDATE', d.[Name], i.[Name]
            FROM inserted i
            INNER JOIN deleted d ON i.[Id] = d.[Id];
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.schema, "dbo");
        assert_eq!(stmt.table, "ProductHistory");
        assert!(stmt.has_join);
        assert_eq!(stmt.alias1.as_deref(), Some("i"));
        assert_eq!(stmt.alias2.as_deref(), Some("d"));
        assert!(stmt.on_clause.is_some());
        let on_clause = stmt.on_clause.as_ref().unwrap();
        assert!(on_clause.contains("i.[Id]") || on_clause.contains("[Id]"));
    }

    #[test]
    fn test_insert_select_parser_join_without_inner() {
        // INSERT INTO ... SELECT FROM inserted i JOIN deleted d ON ...;
        let sql = r#"
            INSERT INTO [dbo].[AuditLog] ([Id], [Change])
            SELECT i.[Id], d.[Value]
            FROM inserted i
            JOIN deleted d ON i.[Id] = d.[Id];
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert!(stmt.has_join);
        assert_eq!(stmt.alias1.as_deref(), Some("i"));
        assert_eq!(stmt.alias2.as_deref(), Some("d"));
    }

    #[test]
    fn test_insert_select_parser_from_deleted() {
        // INSERT INTO ... SELECT FROM deleted;
        let sql = r#"
            INSERT INTO [dbo].[DeletedHistory] ([Id], [Name])
            SELECT [Id], [Name]
            FROM deleted;
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.schema, "dbo");
        assert_eq!(stmt.table, "DeletedHistory");
        assert!(!stmt.has_join);
    }

    #[test]
    fn test_insert_select_parser_multiple_statements() {
        // Multiple INSERT statements in trigger body
        let sql = r#"
            INSERT INTO [dbo].[Products] ([Id], [Name])
            SELECT [Id], [Name]
            FROM inserted;

            INSERT INTO [dbo].[ProductHistory] ([ProductId], [Action])
            SELECT [Id], 'INSERT'
            FROM inserted;
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0].table, "Products");
        assert_eq!(stmts[1].table, "ProductHistory");
    }

    #[test]
    fn test_insert_select_parser_with_whitespace() {
        // Varying whitespace (tabs, multiple spaces, newlines)
        let sql = "INSERT  \t INTO\t[dbo] . [Products]\n([Id],[Name])\nSELECT  [Id],[Name]\n\tFROM   inserted\t;";
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.schema, "dbo");
        assert_eq!(stmt.table, "Products");
    }

    #[test]
    fn test_insert_select_parser_case_insensitive() {
        // Case insensitive keywords
        let sql = r#"
            insert INTO [dbo].[Products] ([Id])
            SELECT [Id]
            from INSERTED;
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_insert_select_parser_empty() {
        // Empty body
        let mut parser = InsertSelectTokenParser::new("").unwrap();
        let stmts = parser.extract_statements();
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_insert_select_parser_no_insert_select() {
        // Body without INSERT...SELECT
        let sql = "UPDATE [dbo].[Products] SET [Name] = 'Test';";
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_insert_select_parser_insert_values_not_matched() {
        // INSERT with VALUES (not SELECT) should not match
        let sql = "INSERT INTO [dbo].[Products] ([Id]) VALUES (1);";
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_insert_select_parser_insert_from_regular_table() {
        // INSERT SELECT from regular table (not inserted/deleted) should not match
        let sql = r#"
            INSERT INTO [dbo].[Products] ([Id])
            SELECT [Id]
            FROM [dbo].[OtherTable];
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_insert_select_parser_nested_parens_in_select() {
        // SELECT with function calls and parentheses (but not nested SELECTs)
        // Note: Nested SELECT inside expressions is a complex edge case not typically
        // found in trigger INSERT statements that reference inserted/deleted.
        let sql = r#"
            INSERT INTO [dbo].[Results] ([Value])
            SELECT COALESCE([Val], 0)
            FROM inserted;
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        assert!(stmts[0].select_expr.contains("COALESCE"));
    }

    #[test]
    fn test_insert_select_parser_complex_column_list() {
        // Column list with special characters
        let sql = r#"
            INSERT INTO [dbo].[Data] ([Id], [User Name], [Price$])
            SELECT [Id], [User Name], [Price$]
            FROM inserted;
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert!(stmt.column_list.contains("[User Name]"));
        assert!(stmt.column_list.contains("[Price$]"));
    }

    #[test]
    fn test_insert_select_parser_complex_on_clause() {
        // Complex ON clause with multiple conditions
        let sql = r#"
            INSERT INTO [dbo].[History] ([Id], [OldVal], [NewVal])
            SELECT i.[Id], d.[Value], i.[Value]
            FROM inserted i
            INNER JOIN deleted d ON i.[Id] = d.[Id] AND i.[Type] = d.[Type];
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert!(stmt.has_join);
        let on_clause = stmt.on_clause.as_ref().unwrap();
        // The ON clause should have the AND condition
        assert!(on_clause.contains("AND") || on_clause.len() > 20);
    }

    #[test]
    fn test_insert_select_parser_alias_extraction() {
        // Verify alias extraction for both inserted and deleted
        let sql = r#"
            INSERT INTO [dbo].[Log] ([A])
            SELECT ins.[A]
            FROM inserted ins
            INNER JOIN deleted del ON ins.[Id] = del.[Id];
        "#;
        let mut parser = InsertSelectTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.alias1.as_deref(), Some("ins"));
        assert_eq!(stmt.alias2.as_deref(), Some("del"));
    }

    // =========================================================================
    // UpdateTokenParser Tests (Phase 20.4.7)
    // =========================================================================
    // Tests for token-based UPDATE...FROM...JOIN parsing (replaces UPDATE_ALIAS_RE regex pattern).

    #[test]
    fn test_update_parser_basic() {
        // Basic UPDATE alias SET ... FROM [schema].[table] alias INNER JOIN inserted alias ON ...
        let sql = r#"
            UPDATE p
            SET p.[Name] = i.[Name], p.[Price] = i.[Price]
            FROM [dbo].[Products] p
            INNER JOIN inserted i ON p.[Id] = i.[Id];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.update_alias, "p");
        assert_eq!(stmt.schema, "dbo");
        assert_eq!(stmt.table, "Products");
        assert_eq!(stmt.table_alias, "p");
        assert_eq!(stmt.magic_table.to_lowercase(), "inserted");
        assert_eq!(stmt.magic_alias, "i");
        assert!(stmt.set_clause.contains("p.[Name]"));
        assert!(stmt.on_clause.contains("p.[Id]"));
    }

    #[test]
    fn test_update_parser_with_deleted() {
        // UPDATE with JOIN to deleted instead of inserted
        let sql = r#"
            UPDATE t
            SET t.[Status] = 'DELETED'
            FROM [dbo].[Audit] t
            INNER JOIN deleted d ON t.[Id] = d.[Id];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.update_alias, "t");
        assert_eq!(stmt.magic_table.to_lowercase(), "deleted");
        assert_eq!(stmt.magic_alias, "d");
    }

    #[test]
    fn test_update_parser_without_inner_keyword() {
        // UPDATE with JOIN (without INNER keyword)
        let sql = r#"
            UPDATE p
            SET p.[Name] = i.[Name]
            FROM [dbo].[Products] p
            JOIN inserted i ON p.[Id] = i.[Id];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.update_alias, "p");
        assert_eq!(stmt.table_alias, "p");
    }

    #[test]
    fn test_update_parser_with_whitespace() {
        // UPDATE with tabs and newlines
        let sql = "UPDATE\tp\n\tSET\tp.[Name] = i.[Name]\n\tFROM\t[dbo].[Products]\tp\n\tINNER JOIN inserted i ON p.[Id] = i.[Id];";
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.update_alias, "p");
        assert_eq!(stmt.schema, "dbo");
        assert_eq!(stmt.table, "Products");
    }

    #[test]
    fn test_update_parser_case_insensitive() {
        // Keywords in different cases
        let sql = r#"
            update p
            SET p.[Name] = i.[Name]
            from [dbo].[Products] p
            INNER join INSERTED i on p.[Id] = i.[Id];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.update_alias, "p");
        assert!(stmt.magic_table.eq_ignore_ascii_case("inserted"));
    }

    #[test]
    fn test_update_parser_empty() {
        let sql = "";
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_update_parser_no_matching_pattern() {
        // Regular UPDATE without JOIN to inserted/deleted
        let sql = r#"
            UPDATE [dbo].[Products]
            SET [Name] = 'New Name'
            WHERE [Id] = 1;
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_update_parser_update_without_from() {
        // UPDATE alias SET without FROM clause
        let sql = r#"
            UPDATE p
            SET p.[Name] = 'New Name';
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_update_parser_join_to_regular_table() {
        // UPDATE with JOIN to a regular table (not inserted/deleted)
        let sql = r#"
            UPDATE p
            SET p.[Name] = t.[Name]
            FROM [dbo].[Products] p
            INNER JOIN [dbo].[Temp] t ON p.[Id] = t.[Id];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_update_parser_complex_set_clause() {
        // UPDATE with complex SET clause (multiple columns, GETDATE())
        let sql = r#"
            UPDATE p
            SET
                p.[Name] = i.[Name],
                p.[Price] = i.[Price],
                p.[ModifiedAt] = GETDATE()
            FROM [dbo].[Products] p
            INNER JOIN inserted i ON p.[Id] = i.[Id];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert!(stmt.set_clause.contains("p.[Name]"));
        assert!(stmt.set_clause.contains("p.[Price]"));
        assert!(stmt.set_clause.contains("GETDATE()"));
    }

    #[test]
    fn test_update_parser_complex_on_clause() {
        // UPDATE with complex ON clause (multiple conditions)
        let sql = r#"
            UPDATE p
            SET p.[Name] = i.[Name]
            FROM [dbo].[Products] p
            INNER JOIN inserted i ON p.[Id] = i.[Id] AND p.[Version] = i.[Version];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert!(stmt.on_clause.contains("p.[Id]"));
        assert!(stmt.on_clause.contains("i.[Id]"));
    }

    #[test]
    fn test_update_parser_alias_differences() {
        // UPDATE where update_alias differs from table_alias (rare but possible)
        let sql = r#"
            UPDATE upd
            SET upd.[Name] = i.[Name]
            FROM [dbo].[Products] tbl
            INNER JOIN inserted i ON tbl.[Id] = i.[Id];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.update_alias, "upd");
        assert_eq!(stmt.table_alias, "tbl");
    }

    #[test]
    fn test_update_parser_multiple_spaces() {
        // UPDATE with multiple spaces between tokens
        let sql = "UPDATE   p   SET   p.[Name] = i.[Name]   FROM   [dbo].[Products]   p   INNER   JOIN   inserted   i   ON   p.[Id] = i.[Id];";
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.update_alias, "p");
        assert_eq!(stmt.schema, "dbo");
    }

    #[test]
    fn test_update_parser_unbracketed_schema_table() {
        // UPDATE with unbracketed schema.table
        let sql = r#"
            UPDATE p
            SET p.[Name] = i.[Name]
            FROM dbo.Products p
            INNER JOIN inserted i ON p.[Id] = i.[Id];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.schema, "dbo");
        assert_eq!(stmt.table, "Products");
    }

    #[test]
    fn test_update_parser_custom_schema() {
        // UPDATE with custom schema (not dbo)
        let sql = r#"
            UPDATE p
            SET p.[Name] = i.[Name]
            FROM [Sales].[Products] p
            INNER JOIN inserted i ON p.[Id] = i.[Id];
        "#;
        let mut parser = UpdateTokenParser::new(sql).unwrap();
        let stmts = parser.extract_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.schema, "Sales");
        assert_eq!(stmt.table, "Products");
    }

    // ============================================================================
    // find_function_body_as_tokenized tests (Phase 20.5.1)
    // ============================================================================

    #[test]
    fn test_find_function_body_as_basic_begin() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT AS BEGIN RETURN 1 END";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_function_body_as_with_newline() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT\nAS\nBEGIN RETURN 1 END";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_function_body_as_with_tabs() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT\tAS\tBEGIN RETURN 1 END";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_function_body_as_with_return() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT AS RETURN 1";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("RETURN"));
    }

    #[test]
    fn test_find_function_body_as_with_select() {
        let sql = "CREATE FUNCTION [dbo].[GetData]() RETURNS TABLE AS RETURN SELECT * FROM t";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        // For inline TVF, body starts with RETURN
        assert!(sql[end..].trim().starts_with("RETURN"));
    }

    #[test]
    fn test_find_function_body_as_lowercase() {
        let sql = "create function [dbo].[GetValue]() returns int as begin return 1 end";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().to_uppercase().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_function_body_as_mixed_case() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() Returns INT As Begin RETURN 1 End";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("Begin"));
    }

    #[test]
    fn test_find_function_body_as_with_declare() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT AS DECLARE @x INT; RETURN @x";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("DECLARE"));
    }

    #[test]
    fn test_find_function_body_as_with_set() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT AS SET NOCOUNT ON; RETURN 1";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("SET"));
    }

    #[test]
    fn test_find_function_body_as_with_if() {
        let sql = "CREATE FUNCTION [dbo].[GetValue](@x INT) RETURNS INT AS IF @x > 0 RETURN 1 ELSE RETURN 0";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("IF"));
    }

    #[test]
    fn test_find_function_body_as_with_with_cte() {
        let sql = "CREATE FUNCTION [dbo].[GetData]() RETURNS TABLE AS WITH cte AS (SELECT 1 AS x) RETURN SELECT * FROM cte";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("WITH"));
    }

    #[test]
    fn test_find_function_body_as_with_multiple_whitespace() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT   AS   BEGIN RETURN 1 END";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_function_body_as_with_crlf() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT\r\nAS\r\nBEGIN RETURN 1 END";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_function_body_as_not_alias() {
        // This should NOT match AS in "AS alias" pattern - it should match the body separator AS
        let sql =
            "CREATE FUNCTION [dbo].[GetValue]() RETURNS TABLE AS RETURN SELECT x AS alias FROM t";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        // Body starts with RETURN, not the alias pattern
        assert!(sql[end..].trim().starts_with("RETURN"));
    }

    #[test]
    fn test_find_function_body_as_no_match() {
        // No AS keyword after RETURNS
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT";
        let returns_pos = sql.to_uppercase().find("RETURNS").unwrap();
        let result = find_function_body_as_tokenized(sql, returns_pos);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_function_body_as_empty_after_returns() {
        let sql = "RETURNS";
        let result = find_function_body_as_tokenized(sql, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_function_body_basic() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT AS BEGIN RETURN 1 END";
        let body = extract_function_body(sql);
        assert!(body.starts_with("BEGIN"));
        assert!(body.contains("RETURN 1"));
    }

    #[test]
    fn test_extract_function_body_with_tabs() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT\tAS\tBEGIN RETURN 1 END";
        let body = extract_function_body(sql);
        assert!(body.starts_with("BEGIN"));
    }

    #[test]
    fn test_extract_function_header_basic() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT AS BEGIN RETURN 1 END";
        let header = extract_function_header(sql);
        assert!(header.contains("CREATE FUNCTION"));
        assert!(header.contains("RETURNS INT"));
        assert!(header.ends_with("AS ") || header.ends_with("AS"));
    }

    #[test]
    fn test_extract_function_header_with_tabs() {
        let sql = "CREATE FUNCTION [dbo].[GetValue]() RETURNS INT\tAS\tBEGIN RETURN 1 END";
        let header = extract_function_header(sql);
        assert!(header.contains("CREATE FUNCTION"));
        assert!(header.contains("RETURNS INT"));
    }

    // ============================================================================
    // find_procedure_body_separator_as_tokenized tests (Phase 20.5.2)
    // ============================================================================

    #[test]
    fn test_find_procedure_body_as_basic_begin() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc] AS BEGIN SELECT 1 END";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_with_newline() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc]\nAS\nBEGIN SELECT 1 END";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_with_tabs() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc]\tAS\tBEGIN SELECT 1 END";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_with_set() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc] AS SET NOCOUNT ON; SELECT 1";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("SET"));
    }

    #[test]
    fn test_find_procedure_body_as_with_select() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc] AS SELECT * FROM t";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("SELECT"));
    }

    #[test]
    fn test_find_procedure_body_as_lowercase() {
        let sql = "create procedure [dbo].[myproc] as begin select 1 end";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().to_uppercase().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_mixed_case() {
        let sql = "CREATE Procedure [dbo].[MyProc] As Begin SELECT 1 End";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("Begin"));
    }

    #[test]
    fn test_find_procedure_body_as_with_declare() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc] AS DECLARE @x INT; SELECT @x";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("DECLARE"));
    }

    #[test]
    fn test_find_procedure_body_as_with_if() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc] @x INT AS IF @x > 0 SELECT 1 ELSE SELECT 0";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("IF"));
    }

    #[test]
    fn test_find_procedure_body_as_with_with_cte() {
        let sql =
            "CREATE PROCEDURE [dbo].[MyProc] AS WITH cte AS (SELECT 1 AS x) SELECT * FROM cte";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("WITH"));
    }

    #[test]
    fn test_find_procedure_body_as_with_multiple_whitespace() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc]   AS   BEGIN SELECT 1 END";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_with_crlf() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc]\r\nAS\r\nBEGIN SELECT 1 END";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_with_parameters() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc] @id INT, @name VARCHAR(50) AS BEGIN SELECT @id, @name END";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_with_insert() {
        let sql = "CREATE PROCEDURE [dbo].[InsertProc] @val INT AS INSERT INTO t VALUES (@val)";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("INSERT"));
    }

    #[test]
    fn test_find_procedure_body_as_with_update() {
        let sql = "CREATE PROCEDURE [dbo].[UpdateProc] @id INT, @val INT AS UPDATE t SET x = @val WHERE id = @id";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("UPDATE"));
    }

    #[test]
    fn test_find_procedure_body_as_with_delete() {
        let sql = "CREATE PROCEDURE [dbo].[DeleteProc] @id INT AS DELETE FROM t WHERE id = @id";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("DELETE"));
    }

    #[test]
    fn test_find_procedure_body_as_with_exec() {
        let sql = "CREATE PROCEDURE [dbo].[ExecProc] AS EXEC sp_help";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("EXEC"));
    }

    #[test]
    fn test_find_procedure_body_as_with_execute() {
        let sql = "CREATE PROCEDURE [dbo].[ExecProc] AS EXECUTE sp_help";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("EXECUTE"));
    }

    #[test]
    fn test_find_procedure_body_as_no_match() {
        // No AS keyword followed by body-starting keyword
        let sql = "CREATE PROCEDURE [dbo].[MyProc] @x INT";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_procedure_body_as_empty() {
        let sql = "";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_procedure_body_only_basic() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc] AS BEGIN SELECT 1 END";
        let body = extract_procedure_body_only(sql);
        assert!(body.starts_with("BEGIN"));
        assert!(body.contains("SELECT 1"));
    }

    #[test]
    fn test_extract_procedure_body_only_with_tabs() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc]\tAS\tBEGIN SELECT 1 END";
        let body = extract_procedure_body_only(sql);
        assert!(body.starts_with("BEGIN"));
    }

    #[test]
    fn test_extract_procedure_body_only_with_params() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc] @id INT, @name VARCHAR(50) AS SET NOCOUNT ON; SELECT @id, @name";
        let body = extract_procedure_body_only(sql);
        assert!(body.starts_with("SET"));
    }

    #[test]
    fn test_extract_procedure_body_only_multiline() {
        let sql = "CREATE PROCEDURE [dbo].[MyProc]\n@id INT\nAS\nBEGIN\n    SELECT @id\nEND";
        let body = extract_procedure_body_only(sql);
        assert!(body.trim().starts_with("BEGIN"));
    }

    #[test]
    fn test_find_procedure_body_as_while() {
        let sql = "CREATE PROCEDURE [dbo].[LoopProc] @count INT AS WHILE @count > 0 BEGIN SET @count = @count - 1 END";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("WHILE"));
    }

    #[test]
    fn test_find_procedure_body_as_return() {
        let sql = "CREATE PROCEDURE [dbo].[ReturnProc] AS RETURN 0";
        let result = find_procedure_body_separator_as_tokenized(sql);
        assert!(result.is_some());
        let (_start, end) = result.unwrap();
        assert!(sql[end..].trim().starts_with("RETURN"));
    }

    // ============================================================================
    // extract_on_clause_boundaries_tokenized tests (Phase 20.5.4)
    // ============================================================================

    #[test]
    fn test_on_clause_basic_join() {
        let sql = "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        assert!(sql[start..end].contains("t1.id"));
        assert!(sql[start..end].contains("t2.id"));
    }

    #[test]
    fn test_on_clause_with_where() {
        let sql = "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id WHERE t1.active = 1";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        let clause = &sql[start..end];
        assert!(clause.contains("t1.id"));
        // WHERE should not be included
        assert!(!clause.to_uppercase().contains("WHERE"));
    }

    #[test]
    fn test_on_clause_with_group_by() {
        let sql = "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id GROUP BY t1.name";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        let clause = &sql[start..end];
        assert!(clause.contains("t1.id"));
        // GROUP should not be included
        assert!(!clause.to_uppercase().contains("GROUP"));
    }

    #[test]
    fn test_on_clause_with_order_by() {
        let sql = "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id ORDER BY t1.name";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        let clause = &sql[start..end];
        assert!(clause.contains("t1.id"));
        // ORDER should not be included
        assert!(!clause.to_uppercase().contains("ORDER"));
    }

    #[test]
    fn test_on_clause_multiple_joins() {
        let sql = "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id JOIN t3 ON t2.id = t3.id";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 2);
        // First ON clause
        let clause1 = &sql[result[0].0..result[0].1];
        assert!(clause1.contains("t1.id"));
        assert!(clause1.contains("t2.id"));
        // Second ON clause
        let clause2 = &sql[result[1].0..result[1].1];
        assert!(clause2.contains("t2.id"));
        assert!(clause2.contains("t3.id"));
    }

    #[test]
    fn test_on_clause_left_join() {
        let sql = "SELECT * FROM t1 LEFT JOIN t2 ON t1.id = t2.id LEFT JOIN t3 ON t2.id = t3.id";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_on_clause_inner_join() {
        let sql = "SELECT * FROM t1 INNER JOIN t2 ON t1.id = t2.id";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        assert!(sql[start..end].contains("t1.id"));
    }

    #[test]
    fn test_on_clause_with_semicolon() {
        let sql = "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id;";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        let clause = &sql[start..end];
        assert!(!clause.contains(";"));
    }

    #[test]
    fn test_on_clause_with_tabs() {
        let sql = "SELECT * FROM t1 JOIN t2\tON\tt1.id = t2.id";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        assert!(sql[start..end].contains("t1.id"));
    }

    #[test]
    fn test_on_clause_with_newlines() {
        let sql = "SELECT * FROM t1 JOIN t2\nON\nt1.id = t2.id";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        assert!(sql[start..end].contains("t1.id"));
    }

    #[test]
    fn test_on_clause_case_insensitive() {
        let sql = "SELECT * FROM t1 join t2 on t1.id = t2.id";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        assert!(sql[start..end].contains("t1.id"));
    }

    #[test]
    fn test_on_clause_with_having() {
        let sql = "SELECT t1.id FROM t1 JOIN t2 ON t1.id = t2.id HAVING COUNT(*) > 1";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        let clause = &sql[start..end];
        assert!(!clause.to_uppercase().contains("HAVING"));
    }

    #[test]
    fn test_on_clause_with_union() {
        let sql =
            "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id UNION SELECT * FROM t3 JOIN t4 ON t3.id = t4.id";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 2);
        let clause1 = &sql[result[0].0..result[0].1];
        assert!(!clause1.to_uppercase().contains("UNION"));
    }

    #[test]
    fn test_on_clause_no_join() {
        let sql = "SELECT * FROM t1 WHERE t1.id = 1";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_on_clause_empty_string() {
        let sql = "";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_on_clause_with_complex_condition() {
        let sql =
            "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id AND t1.status = 'active' WHERE t1.x = 1";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        let clause = &sql[start..end];
        assert!(clause.contains("t1.id"));
        assert!(clause.contains("status"));
        assert!(!clause.to_uppercase().contains("WHERE"));
    }

    #[test]
    fn test_on_clause_cross_join_terminates() {
        let sql = "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id CROSS JOIN t3";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 1);
        let (start, end) = result[0];
        let clause = &sql[start..end];
        assert!(!clause.to_uppercase().contains("CROSS"));
    }

    #[test]
    fn test_on_clause_outer_join_terminates() {
        let sql = "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id OUTER JOIN t3 ON t3.id = t2.id";
        let result = extract_on_clause_boundaries_tokenized(sql);
        assert_eq!(result.len(), 2);
    }

    // ============================================================================
    // extract_group_by_clause_boundaries_tokenized tests (Phase 20.5.5-20.5.6)
    // ============================================================================

    #[test]
    fn test_group_by_basic() {
        let sql = "SELECT name, COUNT(*) FROM users GROUP BY name";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.trim().eq_ignore_ascii_case("name"));
    }

    #[test]
    fn test_group_by_multiple_columns() {
        let sql = "SELECT dept, role, COUNT(*) FROM employees GROUP BY dept, role";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
        assert!(clause.contains("role"));
    }

    #[test]
    fn test_group_by_with_having() {
        let sql = "SELECT dept, COUNT(*) FROM employees GROUP BY dept HAVING COUNT(*) > 5";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
        // HAVING should NOT be included
        assert!(!clause.to_uppercase().contains("HAVING"));
    }

    #[test]
    fn test_group_by_with_order_by() {
        let sql = "SELECT dept, COUNT(*) FROM employees GROUP BY dept ORDER BY dept";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
        // ORDER should NOT be included
        assert!(!clause.to_uppercase().contains("ORDER"));
    }

    #[test]
    fn test_group_by_with_union() {
        let sql = "SELECT dept FROM employees GROUP BY dept UNION SELECT dept FROM contractors";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
        // UNION should NOT be included
        assert!(!clause.to_uppercase().contains("UNION"));
    }

    #[test]
    fn test_group_by_with_semicolon() {
        let sql = "SELECT dept, COUNT(*) FROM employees GROUP BY dept;";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
        // Semicolon should NOT be included
        assert!(!clause.contains(";"));
    }

    #[test]
    fn test_group_by_with_tabs() {
        let sql = "SELECT\tdept,\tCOUNT(*)\tFROM\temployees\tGROUP\tBY\tdept";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
    }

    #[test]
    fn test_group_by_with_newlines() {
        let sql = "SELECT dept, COUNT(*)\nFROM employees\nGROUP BY\n  dept";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
    }

    #[test]
    fn test_group_by_case_insensitive() {
        let sql = "SELECT dept FROM employees group by dept";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
    }

    #[test]
    fn test_group_by_no_match() {
        let sql = "SELECT * FROM employees WHERE dept = 'IT'";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_group_by_empty() {
        let sql = "";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_group_by_quoted_group_not_keyword() {
        // [GROUP] as a column name should not be treated as GROUP BY keyword
        let sql = "SELECT [GROUP], COUNT(*) FROM items";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_none());
    }

    #[test]
    fn test_group_by_with_qualified_columns() {
        let sql = "SELECT t.[dept], COUNT(*) FROM employees t GROUP BY t.[dept]";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("t.[dept]"));
    }

    #[test]
    fn test_group_by_multiple_spaces() {
        let sql = "SELECT dept FROM employees GROUP    BY    dept";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
    }

    #[test]
    fn test_group_by_with_having_and_order() {
        let sql =
            "SELECT dept, COUNT(*) FROM employees GROUP BY dept HAVING COUNT(*) > 5 ORDER BY dept";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
        assert!(!clause.to_uppercase().contains("HAVING"));
        assert!(!clause.to_uppercase().contains("ORDER"));
    }

    #[test]
    fn test_group_by_complex_expression() {
        let sql = "SELECT YEAR(hire_date), COUNT(*) FROM employees GROUP BY YEAR(hire_date)";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("YEAR"));
        assert!(clause.contains("hire_date"));
    }

    #[test]
    fn test_group_by_with_alias_table() {
        let sql = "SELECT e.dept, COUNT(*) FROM employees e GROUP BY e.dept";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("e.dept"));
    }

    #[test]
    fn test_group_by_mixed_case_keywords() {
        let sql = "SELECT dept FROM employees Group By dept Having COUNT(*) > 0";
        let result = extract_group_by_clause_boundaries_tokenized(sql);
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        let clause = &sql[start..end];
        assert!(clause.contains("dept"));
        assert!(!clause.to_uppercase().contains("HAVING"));
    }

    #[test]
    fn test_extract_table_aliases_stuff_nested_subquery() {
        use std::collections::{HashMap, HashSet};

        // Test the exact pattern from InstrumentWithTags fixture
        // Note: Subquery alias is without AS (Tags not AS Tags)
        let sql = r#"
SELECT
    I.Id,
    I.Name,
    Tags.TagList
FROM [dbo].[Instrument] I
LEFT JOIN (
    SELECT
        IT.InstrumentId,
        STUFF((
            SELECT ', ' + [ITTAG].[Name]
            FROM [dbo].[InstrumentTag] [IT2]
            INNER JOIN [dbo].[Tag] [ITTAG] ON [IT2].TagId = [ITTAG].Id
            WHERE IT.InstrumentId = [IT2].InstrumentId
            FOR XML PATH('')
        ), 1, 2, '') AS TagList
    FROM [dbo].[InstrumentTag] IT
    GROUP BY IT.InstrumentId
) Tags ON Tags.InstrumentId = I.Id
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        // 'I' should be a table alias for [dbo].[Instrument]
        assert_eq!(
            table_aliases.get("i"),
            Some(&"[dbo].[Instrument]".to_string()),
            "Expected 'I' -> [dbo].[Instrument]"
        );

        // 'IT' should be a table alias for [dbo].[InstrumentTag] (first level nested)
        assert_eq!(
            table_aliases.get("it"),
            Some(&"[dbo].[InstrumentTag]".to_string()),
            "Expected 'IT' -> [dbo].[InstrumentTag]"
        );

        // '[IT2]' should be a table alias for [dbo].[InstrumentTag] (second level nested)
        assert_eq!(
            table_aliases.get("it2"),
            Some(&"[dbo].[InstrumentTag]".to_string()),
            "Expected 'IT2' -> [dbo].[InstrumentTag]"
        );

        // '[ITTAG]' should be a table alias for [dbo].[Tag] (second level nested)
        assert_eq!(
            table_aliases.get("ittag"),
            Some(&"[dbo].[Tag]".to_string()),
            "Expected 'ITTAG' -> [dbo].[Tag]"
        );

        // 'Tags' should be recognized as a subquery alias (without AS keyword)
        assert!(
            subquery_aliases.contains("tags"),
            "Expected 'Tags' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
    }

    #[test]
    fn test_extract_table_aliases_for_view_stuff_nested_subquery() {
        // This tests the VIEW path using extract_table_aliases (not extract_table_aliases_for_body_deps)
        // This is what views use for QueryDependencies
        let sql = r#"
SELECT
    I.Id,
    I.Name,
    Tags.TagList
FROM [dbo].[Instrument] I
LEFT JOIN (
    SELECT
        IT.InstrumentId,
        STUFF((
            SELECT ', ' + [ITTAG].[Name]
            FROM [dbo].[InstrumentTag] [IT2]
            INNER JOIN [dbo].[Tag] [ITTAG] ON [IT2].TagId = [ITTAG].Id
            WHERE IT.InstrumentId = [IT2].InstrumentId
            FOR XML PATH('')
        ), 1, 2, '') AS TagList
    FROM [dbo].[InstrumentTag] IT
    GROUP BY IT.InstrumentId
) Tags ON Tags.InstrumentId = I.Id
"#;
        // Use the view-specific function
        let aliases = extract_table_aliases(sql, "dbo");

        // 'I' should be a table alias for [dbo].[Instrument]
        assert!(
            aliases
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("I") && v == "[dbo].[Instrument]"),
            "Expected 'I' -> [dbo].[Instrument], got: {:?}",
            aliases
        );

        // 'IT' should be a table alias for [dbo].[InstrumentTag] (first level nested)
        assert!(
            aliases
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("IT") && v == "[dbo].[InstrumentTag]"),
            "Expected 'IT' -> [dbo].[InstrumentTag], got: {:?}",
            aliases
        );

        // '[IT2]' should be a table alias for [dbo].[InstrumentTag] (second level nested)
        assert!(
            aliases
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("IT2") && v == "[dbo].[InstrumentTag]"),
            "Expected 'IT2' -> [dbo].[InstrumentTag], got: {:?}",
            aliases
        );

        // '[ITTAG]' should be a table alias for [dbo].[Tag] (second level nested)
        assert!(
            aliases
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("ITTAG") && v == "[dbo].[Tag]"),
            "Expected 'ITTAG' -> [dbo].[Tag], got: {:?}",
            aliases
        );
    }

    #[test]
    fn test_view_all_column_references_stuff_nested_subquery() {
        // This tests the full extract_all_column_references path
        let sql = r#"
SELECT
    I.Id,
    I.Name,
    Tags.TagList
FROM [dbo].[Instrument] I
LEFT JOIN (
    SELECT
        IT.InstrumentId,
        STUFF((
            SELECT ', ' + [ITTAG].[Name]
            FROM [dbo].[InstrumentTag] [IT2]
            INNER JOIN [dbo].[Tag] [ITTAG] ON [IT2].TagId = [ITTAG].Id
            WHERE IT.InstrumentId = [IT2].InstrumentId
            FOR XML PATH('')
        ), 1, 2, '') AS TagList
    FROM [dbo].[InstrumentTag] IT
    GROUP BY IT.InstrumentId
) Tags ON Tags.InstrumentId = I.Id
"#;
        let aliases = extract_table_aliases(sql, "dbo");
        let all_refs = extract_all_column_references(sql, &aliases, "dbo");

        // Should NOT contain [Instrument].[IT2] or [Instrument].[ITTAG]
        assert!(
            !all_refs
                .iter()
                .any(|r| r.contains("[Instrument].[IT2]") || r.contains("[Instrument].[ITTAG]")),
            "Should not have [Instrument].[IT2] or [Instrument].[ITTAG]. Got: {:?}",
            all_refs
        );

        // Should have [dbo].[Tag].[Name] for [ITTAG].[Name]
        assert!(
            all_refs.iter().any(|r| r == "[dbo].[Tag].[Name]"),
            "Expected [dbo].[Tag].[Name]. Got: {:?}",
            all_refs
        );
    }

    // ============================================================================
    // Tests for MERGE statement alias extraction (Phase 20.8.11)
    // ============================================================================

    #[test]
    fn test_extract_merge_target_alias() {
        use std::collections::{HashMap, HashSet};

        // Test MERGE INTO [table] AS [alias] pattern
        let sql = r#"
MERGE INTO [dbo].[AccountTag] AS [TARGET]
USING (
    SELECT A.Id AS AccountId, T.Id AS TagId
    FROM [dbo].[Account] A
    CROSS JOIN [dbo].[Tag] T
) AS [SOURCE]
ON [TARGET].AccountId = [SOURCE].AccountId
WHEN NOT MATCHED BY TARGET THEN
    INSERT (AccountId, TagId)
    VALUES ([SOURCE].AccountId, [SOURCE].TagId);
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        println!("Table aliases: {:?}", table_aliases);
        println!("Subquery aliases: {:?}", subquery_aliases);

        // 'TARGET' should be a table alias for [dbo].[AccountTag]
        assert_eq!(
            table_aliases.get("target"),
            Some(&"[dbo].[AccountTag]".to_string()),
            "Expected 'TARGET' -> [dbo].[AccountTag]"
        );

        // 'SOURCE' should be a subquery alias
        assert!(
            subquery_aliases.contains("source"),
            "Expected 'SOURCE' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
    }

    #[test]
    fn test_extract_merge_target_alias_without_brackets() {
        use std::collections::{HashMap, HashSet};

        // Test MERGE INTO table AS alias pattern without brackets
        let sql = r#"
MERGE INTO dbo.AccountTag AS TARGET
USING (
    SELECT Id FROM dbo.Account
) AS SOURCE
ON TARGET.AccountId = SOURCE.Id
WHEN NOT MATCHED THEN
    INSERT (AccountId) VALUES (SOURCE.Id);
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        println!("Table aliases: {:?}", table_aliases);
        println!("Subquery aliases: {:?}", subquery_aliases);

        // 'TARGET' should be a table alias for [dbo].[AccountTag]
        assert_eq!(
            table_aliases.get("target"),
            Some(&"[dbo].[AccountTag]".to_string()),
            "Expected 'TARGET' -> [dbo].[AccountTag]"
        );

        // 'SOURCE' should be a subquery alias
        assert!(
            subquery_aliases.contains("source"),
            "Expected 'SOURCE' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
    }

    #[test]
    fn test_extract_merge_without_into_keyword() {
        use std::collections::{HashMap, HashSet};

        // Test MERGE [table] AS [alias] pattern without INTO
        let sql = r#"
MERGE [dbo].[AccountTag] AS TARGET
USING dbo.Account AS SOURCE
ON TARGET.AccountId = SOURCE.Id
WHEN MATCHED THEN
    UPDATE SET TARGET.AccountId = SOURCE.Id;
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        println!("Table aliases: {:?}", table_aliases);
        println!("Subquery aliases: {:?}", subquery_aliases);

        // 'TARGET' should be a table alias for [dbo].[AccountTag]
        assert_eq!(
            table_aliases.get("target"),
            Some(&"[dbo].[AccountTag]".to_string()),
            "Expected 'TARGET' -> [dbo].[AccountTag]"
        );

        // When USING references a table directly (not subquery), it's a table alias
        assert_eq!(
            table_aliases.get("source"),
            Some(&"[dbo].[Account]".to_string()),
            "Expected 'SOURCE' -> [dbo].[Account]"
        );
    }

    #[test]
    fn test_extract_merge_with_inner_from_join() {
        use std::collections::{HashMap, HashSet};

        // Test MERGE with inner FROM/JOIN inside USING subquery
        let sql = r#"
MERGE INTO [dbo].[AccountTag] AS [TARGET]
USING (
    SELECT A.Id AS AccountId, T.Id AS TagId
    FROM [dbo].[Account] A
    CROSS JOIN [dbo].[Tag] T
    WHERE A.Id = @AccountId AND T.Id = @TagId
) AS [SOURCE]
ON [TARGET].AccountId = [SOURCE].AccountId
WHEN MATCHED THEN
    UPDATE SET [TARGET].AccountId = [SOURCE].AccountId;
"#;
        let mut table_aliases: HashMap<String, String> = HashMap::new();
        let mut subquery_aliases: HashSet<String> = HashSet::new();

        extract_table_aliases_for_body_deps(sql, &mut table_aliases, &mut subquery_aliases);

        println!("Table aliases: {:?}", table_aliases);
        println!("Subquery aliases: {:?}", subquery_aliases);

        // 'TARGET' should be a table alias for [dbo].[AccountTag]
        assert_eq!(
            table_aliases.get("target"),
            Some(&"[dbo].[AccountTag]".to_string()),
            "Expected 'TARGET' -> [dbo].[AccountTag]"
        );

        // 'A' should be a table alias for [dbo].[Account] (inside USING subquery)
        assert_eq!(
            table_aliases.get("a"),
            Some(&"[dbo].[Account]".to_string()),
            "Expected 'A' -> [dbo].[Account]"
        );

        // 'T' should be a table alias for [dbo].[Tag] (inside USING subquery)
        assert_eq!(
            table_aliases.get("t"),
            Some(&"[dbo].[Tag]".to_string()),
            "Expected 'T' -> [dbo].[Tag]"
        );

        // 'SOURCE' should be a subquery alias
        assert!(
            subquery_aliases.contains("source"),
            "Expected 'SOURCE' to be in subquery_aliases: {:?}",
            subquery_aliases
        );
    }

    #[test]
    fn test_body_dependencies_merge_alias_resolution() {
        // Test that MERGE aliases are resolved correctly in body deps
        // References like [TARGET].[Column] should resolve to [dbo].[AccountTag].[Column]
        // References like [SOURCE].[Column] should be skipped (subquery alias)
        let sql = r#"
MERGE INTO [dbo].[AccountTag] AS [TARGET]
USING (
    SELECT A.Id AS AccountId, T.Id AS TagId
    FROM [dbo].[Account] A
    CROSS JOIN [dbo].[Tag] T
    WHERE A.Id = @AccountId AND T.Id = @TagId
) AS [SOURCE]
ON [TARGET].AccountId = [SOURCE].AccountId
    AND [TARGET].TagId = [SOURCE].TagId
WHEN NOT MATCHED BY TARGET THEN
    INSERT (AccountId, TagId)
    VALUES ([SOURCE].AccountId, [SOURCE].TagId)
WHEN MATCHED THEN
    UPDATE SET [TARGET].AccountId = [SOURCE].AccountId;
"#;
        let params = vec!["AccountId".to_string(), "TagId".to_string()];
        let deps = extract_body_dependencies(sql, "[dbo].[TestProc]", &params);

        println!("Body dependencies:");
        for d in &deps {
            println!("  {:?}", d);
        }

        // Should contain [dbo].[AccountTag] (the TARGET table)
        let has_account_tag = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[AccountTag]",
            _ => false,
        });
        assert!(
            has_account_tag,
            "Expected [dbo].[AccountTag] in body deps. Got: {:?}",
            deps
        );

        // Should contain [dbo].[Account] (from USING subquery)
        let has_account = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Account]",
            _ => false,
        });
        assert!(
            has_account,
            "Expected [dbo].[Account] in body deps. Got: {:?}",
            deps
        );

        // Should contain [dbo].[Tag] (from USING subquery)
        let has_tag = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[Tag]",
            _ => false,
        });
        assert!(
            has_tag,
            "Expected [dbo].[Tag] in body deps. Got: {:?}",
            deps
        );

        // Should NOT contain [TARGET].* as a schema reference
        let has_target = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r.starts_with("[TARGET]") || r.contains("[TARGET]"),
            _ => false,
        });
        assert!(
            !has_target,
            "Should NOT have [TARGET].* in body deps. Got: {:?}",
            deps
        );

        // Should NOT contain [SOURCE].* as a schema reference
        let has_source = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r.starts_with("[SOURCE]") || r.contains("[SOURCE]"),
            _ => false,
        });
        assert!(
            !has_source,
            "Should NOT have [SOURCE].* in body deps. Got: {:?}",
            deps
        );

        // Should contain resolved column reference [dbo].[AccountTag].[AccountId]
        let has_account_id = deps.iter().any(|d| match d {
            BodyDependency::ObjectRef(r) => r == "[dbo].[AccountTag].[AccountId]",
            _ => false,
        });
        assert!(
            has_account_id,
            "Expected [dbo].[AccountTag].[AccountId] in body deps. Got: {:?}",
            deps
        );
    }
}
