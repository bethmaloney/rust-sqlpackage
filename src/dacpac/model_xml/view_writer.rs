//! View XML writing utilities for model.xml generation.
//!
//! This module provides functions for writing view elements to the model.xml output.
//! It handles view properties, column extraction, query dependencies, and schema binding.

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, Tokenizer};
use std::collections::HashSet;
use std::io::Write;

use crate::model::{DatabaseModel, ModelElement, RawElement, ViewElement};
pub(crate) use crate::util::contains_ci;

use super::xml_helpers::{
    escape_newlines_for_attr, normalize_script_content, write_property, write_property_raw,
    write_schema_relationship, write_script_property,
};
use super::{
    extract_all_column_references, extract_cte_definitions, extract_group_by_columns,
    extract_join_on_columns, extract_select_columns, extract_table_aliases,
    parse_column_expression, reconstruct_tokens, CteColumn,
};

/// Represents a view column with its name and optional source dependency
#[derive(Debug, Clone)]
pub(crate) struct ViewColumn {
    /// The output column name (alias or original name)
    pub name: String,
    /// The source column reference (if direct column reference), e.g., "[dbo].[Products].[Id]"
    pub source_ref: Option<String>,
    /// Whether this column was expanded from SELECT * (for QueryDependencies filtering)
    pub from_select_star: bool,
}

use super::column_registry::ColumnRegistry;

/// Write a view element to XML.
///
/// Generates the SqlView Element with properties, Columns relationship,
/// QueryDependencies relationship, and Schema relationship.
pub(crate) fn write_view<W: Write>(
    writer: &mut Writer<W>,
    view: &ViewElement,
    model: &DatabaseModel,
    default_schema: &str,
    column_registry: &ColumnRegistry,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", view.schema, view.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlView"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Write properties in DotNet order:
    // 1. IsSchemaBound (if true)
    if view.is_schema_bound {
        write_property(writer, "IsSchemaBound", "True")?;
    }

    // 2. IsMetadataReported (if true)
    if view.is_metadata_reported {
        write_property(writer, "IsMetadataReported", "True")?;
    }

    // Phase 72: Use cached extraction results from ColumnRegistry instead of re-extracting.
    // The cache is populated during ColumnRegistry::from_model() which already calls
    // extract_view_query() and extract_view_columns_and_deps() for every view.
    let cached = column_registry.get_cached_view(&full_name);

    // 3. QueryScript
    let query_script_owned;
    let query_script = if let Some(c) = cached {
        &c.query_script
    } else {
        query_script_owned = extract_view_query(&view.definition);
        &query_script_owned
    };
    write_script_property(writer, "QueryScript", query_script)?;

    // 4. IsWithCheckOption (if true)
    if view.is_with_check_option {
        write_property(writer, "IsWithCheckOption", "True")?;
    }

    // 5. IsAnsiNullsOn - always emit for views (current DotNet behavior)
    // Modern .NET DacFx emits this property for all views
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Extract view columns and dependencies from the query
    // DotNet emits Columns and QueryDependencies for ALL views
    // Phase 72: Use cached results if available, otherwise extract fresh
    let columns_owned;
    let query_deps_owned;
    let (columns, query_deps) = if let Some(c) = cached {
        (&c.columns, &c.query_deps)
    } else {
        let (c, d) = extract_view_columns_and_deps(
            query_script,
            default_schema,
            model,
            view.is_schema_bound,
            column_registry,
        );
        columns_owned = c;
        query_deps_owned = d;
        (&columns_owned, &query_deps_owned)
    };

    // 6. Write Columns relationship with SqlComputedColumn elements
    if !columns.is_empty() {
        write_view_columns(writer, &full_name, columns)?;
    }

    // 7. Write DynamicObjects relationship for CTEs
    // NOTE: Use project's default_schema for unqualified table resolution, NOT the view's schema.
    write_view_cte_dynamic_objects(writer, &full_name, query_script, default_schema)?;

    // 8. Write QueryDependencies relationship
    if !query_deps.is_empty() {
        write_query_dependencies(writer, query_deps)?;
    }

    // 9. Schema relationship
    write_schema_relationship(writer, &view.schema)?;

    // 10. SysCommentsObjectAnnotation with header/footer contents
    write_view_annotation(writer, &view.definition)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write a view from a RawElement (for views parsed via fallback)
/// Mirrors the write_view function but works with raw definition text
pub(crate) fn write_raw_view<W: Write>(
    writer: &mut Writer<W>,
    raw: &RawElement,
    model: &DatabaseModel,
    default_schema: &str,
    column_registry: &ColumnRegistry,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", raw.schema, raw.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlView"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Extract view options from raw SQL text (case-insensitive, no allocation)
    // WITH SCHEMABINDING appears before AS in the view definition
    let is_schema_bound = contains_ci(&raw.definition, "WITH SCHEMABINDING")
        || contains_ci(&raw.definition, ", SCHEMABINDING")
        || contains_ci(&raw.definition, ",SCHEMABINDING");

    // WITH CHECK OPTION appears at the end of the view definition
    let is_with_check_option = contains_ci(&raw.definition, "WITH CHECK OPTION");

    // VIEW_METADATA appears in WITH clause before AS
    let is_metadata_reported = contains_ci(&raw.definition, "VIEW_METADATA");

    // Write properties in DotNet order:
    // 1. IsSchemaBound (if true)
    if is_schema_bound {
        write_property(writer, "IsSchemaBound", "True")?;
    }

    // 2. IsMetadataReported (if true)
    if is_metadata_reported {
        write_property(writer, "IsMetadataReported", "True")?;
    }

    // Phase 72: Use cached extraction results from ColumnRegistry instead of re-extracting.
    let cached = column_registry.get_cached_view(&full_name);

    // 3. QueryScript
    let query_script_owned;
    let query_script = if let Some(c) = cached {
        &c.query_script
    } else {
        query_script_owned = extract_view_query(&raw.definition);
        &query_script_owned
    };
    write_script_property(writer, "QueryScript", query_script)?;

    // 4. IsWithCheckOption (if true)
    if is_with_check_option {
        write_property(writer, "IsWithCheckOption", "True")?;
    }

    // 5. IsAnsiNullsOn - always emit for views (current DotNet behavior)
    // Modern .NET DacFx emits this property for all views
    write_property(writer, "IsAnsiNullsOn", "True")?;

    // Extract view columns and dependencies from the query
    // DotNet emits Columns and QueryDependencies for ALL views
    // Phase 72: Use cached results if available, otherwise extract fresh
    let columns_owned;
    let query_deps_owned;
    let (columns, query_deps) = if let Some(c) = cached {
        (&c.columns, &c.query_deps)
    } else {
        let (c, d) = extract_view_columns_and_deps(
            query_script,
            default_schema,
            model,
            is_schema_bound,
            column_registry,
        );
        columns_owned = c;
        query_deps_owned = d;
        (&columns_owned, &query_deps_owned)
    };

    // 6. Write Columns relationship with SqlComputedColumn elements
    if !columns.is_empty() {
        write_view_columns(writer, &full_name, columns)?;
    }

    // 7. Write DynamicObjects relationship for CTEs
    // NOTE: Use project's default_schema for unqualified table resolution, NOT the view's schema.
    write_view_cte_dynamic_objects(writer, &full_name, query_script, default_schema)?;

    // 8. Write QueryDependencies relationship
    if !query_deps.is_empty() {
        write_query_dependencies(writer, query_deps)?;
    }

    // 9. Schema relationship
    write_schema_relationship(writer, &raw.schema)?;

    // 10. SysCommentsObjectAnnotation with header/footer contents
    write_view_annotation(writer, &raw.definition)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Extract the query part from a CREATE VIEW definition
/// Strips the "CREATE VIEW [name] AS" prefix, leaving just the SELECT statement
/// Also strips trailing semicolon to match DotNet behavior
/// Uses token-based parsing to handle any whitespace (tabs, multiple spaces, newlines)
pub(crate) fn extract_view_query(definition: &str) -> String {
    // Tokenize the definition using sqlparser
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, definition).tokenize() {
        Ok(t) => t,
        Err(_) => {
            // Fallback: return the original definition if tokenization fails
            return definition.to_string();
        }
    };

    // Find the first AS keyword at top level (after CREATE VIEW [name])
    // We need to skip past the CREATE VIEW ... part and find the AS that starts the query
    let mut paren_depth: i32 = 0;
    let mut found_view = false;

    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::Word(w) if w.keyword == Keyword::VIEW => {
                found_view = true;
            }
            Token::Word(w) if w.keyword == Keyword::AS && paren_depth == 0 && found_view => {
                // Found the AS keyword - return everything after it
                let query = reconstruct_tokens(&tokens[i + 1..]);
                // Strip trailing semicolon to match DotNet behavior
                return query.trim_end().trim_end_matches(';').to_string();
            }
            _ => {}
        }
    }

    // Fallback: return the original definition if we can't find AS
    definition.to_string()
}

/// Extract the header portion of a CREATE VIEW definition (up to and including AS).
/// Returns the header text for use in SysCommentsObjectAnnotation.
/// Uses token-based parsing to handle any whitespace variations.
fn extract_view_header(definition: &str) -> String {
    // Tokenize the definition using sqlparser
    let dialect = MsSqlDialect {};
    let tokens = match Tokenizer::new(&dialect, definition).tokenize() {
        Ok(t) => t,
        Err(_) => {
            // Fallback: return empty string if tokenization fails
            return String::new();
        }
    };

    // Find the first AS keyword at top level (after CREATE VIEW [name])
    let mut paren_depth: i32 = 0;
    let mut found_view = false;

    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::Word(w) if w.keyword == Keyword::VIEW => {
                found_view = true;
            }
            Token::Word(w) if w.keyword == Keyword::AS && paren_depth == 0 && found_view => {
                // Found the AS keyword - return everything up to and including it
                return reconstruct_tokens(&tokens[..=i]);
            }
            _ => {}
        }
    }

    // Fallback: return empty string if we can't find AS
    String::new()
}

/// Write SysCommentsObjectAnnotation for a view.
/// DotNet emits this annotation with Length, StartLine, StartColumn, HeaderContents, and FooterContents.
fn write_view_annotation<W: Write>(writer: &mut Writer<W>, definition: &str) -> anyhow::Result<()> {
    // Normalize the definition to have consistent line endings
    let normalized_def = normalize_script_content(definition);

    // Extract header (CREATE VIEW ... AS)
    let header = extract_view_header(&normalized_def);
    if header.is_empty() {
        // If we can't extract the header, skip the annotation
        return Ok(());
    }

    // Calculate total length
    let total_length = normalized_def.len();

    // Detect trailing semicolon for FooterContents
    let footer = if normalized_def.trim_end().ends_with(';') {
        ";"
    } else {
        ""
    };

    // Write the annotation
    let annotation =
        BytesStart::new("Annotation").with_attributes([("Type", "SysCommentsObjectAnnotation")]);
    writer.write_event(Event::Start(annotation))?;

    write_property(writer, "Length", &total_length.to_string())?;
    write_property(writer, "StartLine", "1")?;
    write_property(writer, "StartColumn", "1")?;
    // Escape newlines for XML attribute value (DotNet uses &#xA; for newlines)
    // Use write_property_raw to avoid double-escaping the & in &#xA;
    let escaped_header = escape_newlines_for_attr(&header);
    write_property_raw(writer, "HeaderContents", &escaped_header)?;
    if !footer.is_empty() {
        write_property(writer, "FooterContents", footer)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Annotation")))?;

    Ok(())
}

/// Expand SELECT * to actual table columns using the database model
/// When a view uses SELECT *, DotNet expands it to the actual columns from the referenced table(s).
/// Uses token-based parsing for proper handling of table references.
/// Phase 77b: Uses ColumnRegistry's table_index for O(1) table lookups instead of linear scan.
fn expand_select_star(
    table_aliases: &[(String, String)],
    model: &DatabaseModel,
    column_registry: &ColumnRegistry,
) -> Vec<ViewColumn> {
    // Estimate ~5 columns per table on average
    let mut columns = Vec::with_capacity(table_aliases.len() * 5);

    // For each table in the FROM clause, look up its columns in the model
    for (_alias, table_ref) in table_aliases {
        // Phase 77b: O(1) lookup via table_index instead of O(n) linear scan
        if let Some(idx) = column_registry.get_table_element_index(table_ref) {
            if let Some(ModelElement::Table(table)) = model.elements.get(idx) {
                // Add each column from the table
                for col in &table.columns {
                    let col_ref = format!("{}.[{}]", table_ref, col.name);
                    columns.push(ViewColumn {
                        name: col.name.clone(),
                        source_ref: Some(col_ref),
                        from_select_star: true, // Mark as expanded from SELECT *
                    });
                }
            }
        }
    }

    columns
}

/// Extract view columns and query dependencies from a SELECT statement
/// Returns: (columns, query_dependencies)
/// - columns: List of output columns with their source references
/// - query_dependencies: All tables and columns referenced in the query
/// - is_schema_bound: If true, allows GROUP BY columns to duplicate SELECT columns
pub(crate) fn extract_view_columns_and_deps(
    query: &str,
    default_schema: &str,
    model: &DatabaseModel,
    is_schema_bound: bool,
    column_registry: &ColumnRegistry,
) -> (Vec<ViewColumn>, Vec<String>) {
    // Parse table aliases from FROM clause and JOINs
    let table_aliases = extract_table_aliases(query, default_schema);

    // Extract SELECT column list
    let select_columns = extract_select_columns(query);

    // Pre-allocate based on expected sizes
    let mut columns = Vec::with_capacity(select_columns.len());
    // Estimate: tables + columns (~2x select columns + tables)
    let mut query_deps = Vec::with_capacity(table_aliases.len() + select_columns.len() * 2);
    // Phase 77a: HashSet for O(1) membership checks instead of Vec::contains() O(n)
    let mut query_deps_set: HashSet<String> =
        HashSet::with_capacity(table_aliases.len() + select_columns.len() * 2);

    for col_expr in select_columns {
        let (col_name, source_ref) =
            parse_column_expression(&col_expr, &table_aliases, default_schema);
        // Handle SELECT * - expand to actual table columns using the model
        if col_name == "*" {
            // For SELECT *, expand to actual columns from the referenced table(s)
            // DotNet expands these to the actual table columns
            let expanded = expand_select_star(&table_aliases, model, column_registry);
            columns.extend(expanded);
            continue;
        }
        columns.push(ViewColumn {
            name: col_name,
            source_ref,
            from_select_star: false,
        });
    }

    // Build QueryDependencies in DotNet order:
    // 1. Tables (in order of appearance) - unique
    // 2. JOIN ON columns - unique
    // 3. SELECT list columns - allow duplicates of JOIN ON columns (but unique within SELECT)
    // 4. WHERE/other columns - unique against all previous
    // 5. GROUP BY columns - allow duplicates of SELECT columns (unique within GROUP BY)

    // 1. Add all referenced tables (unique)
    for (_alias, table_ref) in &table_aliases {
        if query_deps_set.insert(table_ref.clone()) {
            query_deps.push(table_ref.clone());
        }
    }

    // 2. Add JOIN ON condition columns (unique)
    let join_on_cols = extract_join_on_columns(query, &table_aliases, default_schema);
    for col_ref in &join_on_cols {
        if query_deps_set.insert(col_ref.clone()) {
            query_deps.push(col_ref.clone());
        }
    }

    // Track SELECT columns separately for dedup within SELECT phase
    let mut select_seen: HashSet<String> = HashSet::new();

    // 3. Add column references from the SELECT columns
    // DotNet allows duplicates of JOIN ON columns (unique within SELECT)
    // Skip columns expanded from SELECT * - they go in ExpressionDependencies, not QueryDependencies
    for col in &columns {
        if col.from_select_star {
            continue; // SELECT * column refs don't go in QueryDependencies
        }
        if let Some(ref source_ref) = col.source_ref {
            // Unique within SELECT phase only
            if select_seen.insert(source_ref.clone()) {
                query_deps.push(source_ref.clone());
            }
        }
    }

    // 4. Add remaining column references from the query (WHERE, HAVING, etc.)
    // These are unique against all previous (JOIN ON + SELECT)
    // Update query_deps_set with SELECT columns added in step 3
    for dep in select_seen.iter() {
        query_deps_set.insert(dep.clone());
    }
    let all_column_refs = extract_all_column_references(query, &table_aliases, default_schema);
    for col_ref in &all_column_refs {
        if query_deps_set.insert(col_ref.clone()) {
            query_deps.push(col_ref.clone());
        }
    }

    // 5. Add GROUP BY columns
    // DotNet behavior varies based on SCHEMABINDING:
    // - WITH SCHEMABINDING: GROUP BY adds duplicates for all columns (max 2 total)
    // - Without SCHEMABINDING: GROUP BY only adds duplicates for columns in JOIN ON
    let group_by_cols = extract_group_by_columns(query, &table_aliases, default_schema);
    let join_on_set: HashSet<String> = join_on_cols.iter().cloned().collect();
    let mut group_by_added: HashSet<String> = HashSet::new();
    for col_ref in group_by_cols {
        let already_present = query_deps_set.contains(&col_ref);
        let in_join_on = join_on_set.contains(&col_ref);

        if !group_by_added.contains(&col_ref) {
            if !already_present {
                // Not present yet - add it
                group_by_added.insert(col_ref.clone());
                query_deps_set.insert(col_ref.clone());
                query_deps.push(col_ref);
            } else if is_schema_bound {
                // SCHEMABINDING views: allow duplicates for all columns (max 2)
                let existing_count = query_deps.iter().filter(|r| *r == &col_ref).count();
                if existing_count < 2 {
                    group_by_added.insert(col_ref.clone());
                    query_deps.push(col_ref);
                }
            } else if in_join_on {
                // Non-SCHEMABINDING views: only allow duplicates for JOIN ON columns
                let existing_count = query_deps.iter().filter(|r| *r == &col_ref).count();
                if existing_count < 2 {
                    group_by_added.insert(col_ref.clone());
                    query_deps.push(col_ref);
                }
            }
            // If already present, not schema_bound, and NOT in JOIN ON, skip
        }
    }

    (columns, query_deps)
}

/// Write view columns as SqlComputedColumn elements
pub(crate) fn write_view_columns<W: Write>(
    writer: &mut Writer<W>,
    view_full_name: &str,
    columns: &[ViewColumn],
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", view_full_name, col.name);
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlComputedColumn"),
            ("Name", col_full_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        // Write ExpressionDependencies if this column has a source reference
        if let Some(source_ref) = &col.source_ref {
            // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
            let dep_rel = BytesStart::new("Relationship")
                .with_attributes([("Name", "ExpressionDependencies")]);
            writer.write_event(Event::Start(dep_rel))?;

            writer.write_event(Event::Start(BytesStart::new("Entry")))?;

            // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
            let refs =
                BytesStart::new("References").with_attributes([("Name", source_ref.as_str())]);
            writer.write_event(Event::Empty(refs))?;

            writer.write_event(Event::End(BytesEnd::new("Entry")))?;
            writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write QueryDependencies relationship
pub(crate) fn write_query_dependencies<W: Write>(
    writer: &mut Writer<W>,
    deps: &[String],
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "QueryDependencies")]);
    writer.write_event(Event::Start(rel))?;

    for dep in deps {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let refs = BytesStart::new("References").with_attributes([("Name", dep.as_str())]);
        writer.write_event(Event::Empty(refs))?;

        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

// =============================================================================
// CTE DynamicObjects Writing for Views (Phase 24.1.3)
// =============================================================================

/// Write DynamicObjects relationship for CTEs extracted from a view's query.
/// Each CTE becomes a SqlDynamicColumnSource element with SqlComputedColumn children.
fn write_view_cte_dynamic_objects<W: Write>(
    writer: &mut Writer<W>,
    full_name: &str,
    query_script: &str,
    default_schema: &str,
) -> anyhow::Result<()> {
    // Extract CTEs from the query script
    let cte_defs = extract_cte_definitions(query_script, default_schema);

    if cte_defs.is_empty() {
        return Ok(());
    }

    let rel = BytesStart::new("Relationship").with_attributes([("Name", "DynamicObjects")]);
    writer.write_event(Event::Start(rel))?;

    for cte in &cte_defs {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // Element name: [schema].[view].[CTEn].[cte_name]
        let cte_source_name = format!("{}.[CTE{}].[{}]", full_name, cte.cte_number, cte.name);

        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlDynamicColumnSource"),
            ("Name", cte_source_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        // Write Columns relationship with SqlComputedColumn elements
        if !cte.columns.is_empty() {
            write_cte_columns_for_view(writer, &cte_source_name, &cte.columns)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write Columns relationship for a CTE SqlDynamicColumnSource in a view.
fn write_cte_columns_for_view<W: Write>(
    writer: &mut Writer<W>,
    cte_source_name: &str,
    columns: &[CteColumn],
) -> anyhow::Result<()> {
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in columns {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let col_full_name = format!("{}.[{}]", cte_source_name, col.name);
        let col_elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlComputedColumn"),
            ("Name", col_full_name.as_str()),
        ]);
        writer.write_event(Event::Start(col_elem))?;

        // Write ExpressionDependencies if any
        if !col.expression_dependencies.is_empty() {
            write_expression_dependencies_for_view(writer, &col.expression_dependencies)?;
        }

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write ExpressionDependencies relationship for a CTE column in a view.
fn write_expression_dependencies_for_view<W: Write>(
    writer: &mut Writer<W>,
    dependencies: &[String],
) -> anyhow::Result<()> {
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "ExpressionDependencies")]);
    writer.write_event(Event::Start(rel))?;

    for dep in dependencies {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;
        let refs_elem = BytesStart::new("References").with_attributes([("Name", dep.as_str())]);
        writer.write_event(Event::Empty(refs_elem))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_test_writer() -> Writer<Cursor<Vec<u8>>> {
        Writer::new(Cursor::new(Vec::new()))
    }

    fn get_output(writer: Writer<Cursor<Vec<u8>>>) -> String {
        String::from_utf8(writer.into_inner().into_inner()).unwrap()
    }

    #[test]
    fn test_extract_view_query_simple() {
        let definition = "CREATE VIEW [dbo].[MyView] AS SELECT * FROM [dbo].[Table1]";
        let result = extract_view_query(definition);
        assert!(result.contains("SELECT"));
        assert!(result.contains("[dbo].[Table1]"));
        assert!(!result.contains("CREATE VIEW"));
    }

    #[test]
    fn test_extract_view_query_with_schemabinding() {
        let definition =
            "CREATE VIEW [dbo].[MyView] WITH SCHEMABINDING AS SELECT Id FROM [dbo].[Table1]";
        let result = extract_view_query(definition);
        assert!(result.contains("SELECT"));
        assert!(result.contains("Id"));
        assert!(!result.contains("CREATE VIEW"));
        assert!(!result.contains("WITH SCHEMABINDING"));
    }

    #[test]
    fn test_extract_view_query_with_tabs_and_newlines() {
        let definition = "CREATE\tVIEW\n[dbo].[MyView]\nAS\nSELECT * FROM [dbo].[Table1]";
        let result = extract_view_query(definition);
        assert!(result.contains("SELECT"));
        assert!(result.contains("[dbo].[Table1]"));
    }

    #[test]
    fn test_write_view_columns_single() {
        let mut writer = create_test_writer();
        let columns = vec![ViewColumn {
            name: "Id".to_string(),
            source_ref: Some("[dbo].[Table1].[Id]".to_string()),
            from_select_star: false,
        }];

        write_view_columns(&mut writer, "[dbo].[MyView]", &columns).unwrap();
        let output = get_output(writer);

        assert!(output.contains(r#"<Relationship Name="Columns">"#));
        assert!(output.contains(r#"Type="SqlComputedColumn""#));
        assert!(output.contains(r#"Name="[dbo].[MyView].[Id]""#));
        assert!(output.contains(r#"<Relationship Name="ExpressionDependencies">"#));
        assert!(output.contains(r#"Name="[dbo].[Table1].[Id]""#));
    }

    #[test]
    fn test_write_view_columns_no_source_ref() {
        let mut writer = create_test_writer();
        let columns = vec![ViewColumn {
            name: "Computed".to_string(),
            source_ref: None,
            from_select_star: false,
        }];

        write_view_columns(&mut writer, "[dbo].[MyView]", &columns).unwrap();
        let output = get_output(writer);

        assert!(output.contains(r#"Name="[dbo].[MyView].[Computed]""#));
        // Should not have ExpressionDependencies for columns without source ref
        assert!(!output.contains("ExpressionDependencies"));
    }

    #[test]
    fn test_write_query_dependencies() {
        let mut writer = create_test_writer();
        let deps = vec![
            "[dbo].[Table1]".to_string(),
            "[dbo].[Table1].[Id]".to_string(),
        ];

        write_query_dependencies(&mut writer, &deps).unwrap();
        let output = get_output(writer);

        assert!(output.contains(r#"<Relationship Name="QueryDependencies">"#));
        assert!(output.contains(r#"Name="[dbo].[Table1]""#));
        assert!(output.contains(r#"Name="[dbo].[Table1].[Id]""#));
    }

    #[test]
    fn test_write_query_dependencies_empty() {
        let mut writer = create_test_writer();
        let deps: Vec<String> = vec![];

        write_query_dependencies(&mut writer, &deps).unwrap();
        let output = get_output(writer);

        assert!(output.contains(r#"<Relationship Name="QueryDependencies">"#));
        // Should have opening and closing tags but no Entry elements
        assert!(!output.contains("<Entry>"));
    }

    #[test]
    fn test_view_column_struct() {
        let col = ViewColumn {
            name: "TestCol".to_string(),
            source_ref: Some("[dbo].[Table].[TestCol]".to_string()),
            from_select_star: true,
        };

        assert_eq!(col.name, "TestCol");
        assert!(col.source_ref.is_some());
        assert!(col.from_select_star);
    }
}
