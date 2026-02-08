//! Column registry for schema-aware unqualified column resolution
//!
//! This module provides a registry that maps tables/views to their columns,
//! enabling accurate resolution of unqualified column references by checking
//! which tables in scope actually have a given column.

use std::collections::{HashMap, HashSet};

use crate::model::{DatabaseModel, ModelElement};

use super::view_writer::{extract_view_columns_and_deps, extract_view_query, ViewColumn};

/// Cached view extraction results to avoid duplicate tokenization.
///
/// During model XML generation, each view's SQL is extracted and parsed twice:
/// once for ColumnRegistry population and once for XML writing. This cache
/// stores the results from the first extraction for reuse during writing.
#[derive(Debug, Clone)]
pub struct ViewExtractionResult {
    pub query_script: String,
    pub columns: Vec<ViewColumn>,
    pub query_deps: Vec<String>,
}

/// Registry mapping tables and views to their column names.
///
/// Used during body dependency extraction to resolve unqualified columns
/// to the correct table when multiple tables are in scope.
///
/// All keys and column names are stored in lowercase for case-insensitive lookup.
#[derive(Debug, Default)]
pub struct ColumnRegistry {
    /// Maps lowercase `[schema].[table]` or `[schema].[view]` to set of lowercase column names
    table_columns: HashMap<String, HashSet<String>>,
    /// Tracks views with SELECT * that have unknown columns (can't be resolved statically)
    /// These views should be excluded from unqualified column resolution
    views_with_wildcard: HashSet<String>,
    /// Cached view extraction results keyed by lowercase `[schema].[name]`
    /// Populated during from_model() and consumed by write_view()/write_raw_view()
    view_cache: HashMap<String, ViewExtractionResult>,
}

impl ColumnRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            table_columns: HashMap::new(),
            views_with_wildcard: HashSet::new(),
            view_cache: HashMap::new(),
        }
    }

    /// Build a column registry from a DatabaseModel
    ///
    /// Extracts columns from:
    /// - TableElement objects (regular tables)
    /// - ViewElement objects (views, via their SELECT clause parsing)
    ///
    /// Views with SELECT * are tracked separately as having "unknown columns"
    /// and are excluded from unqualified column resolution.
    pub fn from_model(model: &DatabaseModel, default_schema: &str) -> Self {
        let mut registry = Self::new();

        for element in &model.elements {
            match element {
                ModelElement::Table(table) => {
                    let table_key = format!("[{}].[{}]", table.schema, table.name).to_lowercase();

                    let columns: HashSet<String> = table
                        .columns
                        .iter()
                        .map(|c| c.name.to_lowercase())
                        .collect();

                    registry.table_columns.insert(table_key, columns);
                }
                ModelElement::View(view) => {
                    let view_key = format!("[{}].[{}]", view.schema, view.name).to_lowercase();

                    // Extract the SELECT query from the view definition
                    let query_script = extract_view_query(&view.definition);

                    // Extract columns from the view SELECT clause
                    // Pass the model to enable SELECT * expansion
                    let (view_columns, query_deps) = extract_view_columns_and_deps(
                        &query_script,
                        default_schema,
                        model,
                        view.is_schema_bound,
                    );

                    // Check if any columns came from SELECT * expansion
                    let has_wildcard = view_columns.iter().any(|c| c.from_select_star);

                    if has_wildcard {
                        // Track this view as having unknown columns
                        // When SELECT * is used, we can't statically know all columns
                        // because the underlying table may change
                        registry.views_with_wildcard.insert(view_key.clone());
                    }

                    // Add all extracted column names to the registry
                    let columns: HashSet<String> =
                        view_columns.iter().map(|c| c.name.to_lowercase()).collect();

                    registry.table_columns.insert(view_key.clone(), columns);

                    // Cache extraction results for reuse during XML writing (Phase 72)
                    registry.view_cache.insert(
                        view_key,
                        ViewExtractionResult {
                            query_script,
                            columns: view_columns,
                            query_deps,
                        },
                    );
                }
                ModelElement::Raw(raw) if raw.sql_type == "SqlView" => {
                    let view_key = format!("[{}].[{}]", raw.schema, raw.name).to_lowercase();

                    // Extract view properties from raw SQL text (same logic as write_raw_view)
                    let is_schema_bound = {
                        let def = &raw.definition;
                        super::view_writer::contains_ci(def, "WITH SCHEMABINDING")
                            || super::view_writer::contains_ci(def, ", SCHEMABINDING")
                            || super::view_writer::contains_ci(def, ",SCHEMABINDING")
                    };

                    // Extract the SELECT query from the raw view definition
                    let query_script = extract_view_query(&raw.definition);

                    // Extract columns from the view SELECT clause
                    let (view_columns, query_deps) = extract_view_columns_and_deps(
                        &query_script,
                        default_schema,
                        model,
                        is_schema_bound,
                    );

                    // Check for SELECT * wildcard
                    let has_wildcard = view_columns.iter().any(|c| c.from_select_star);
                    if has_wildcard {
                        registry.views_with_wildcard.insert(view_key.clone());
                    }

                    // Add column names to the registry
                    let columns: HashSet<String> =
                        view_columns.iter().map(|c| c.name.to_lowercase()).collect();
                    registry.table_columns.insert(view_key.clone(), columns);

                    // Cache extraction results for reuse during XML writing (Phase 72)
                    registry.view_cache.insert(
                        view_key,
                        ViewExtractionResult {
                            query_script,
                            columns: view_columns,
                            query_deps,
                        },
                    );
                }
                _ => {}
            }
        }

        registry
    }

    /// Look up cached view extraction results.
    /// Returns None if no cache entry exists for this view key.
    /// The key should be in format `[schema].[name]` (brackets required, case-insensitive).
    pub fn get_cached_view(&self, view_key: &str) -> Option<&ViewExtractionResult> {
        let key = view_key.to_lowercase();
        self.view_cache.get(&key)
    }

    /// Check if a table has a specific column
    ///
    /// Both `table_ref` and `column` are compared case-insensitively.
    /// `table_ref` should be in format `[schema].[table]` (brackets required).
    #[cfg(test)]
    pub fn table_has_column(&self, table_ref: &str, column: &str) -> bool {
        let key = table_ref.to_lowercase();
        let col = column.to_lowercase();

        self.table_columns
            .get(&key)
            .is_some_and(|cols| cols.contains(&col))
    }

    /// Find all tables from the given set that have a specific column
    ///
    /// Returns a vector of table references (in their original format) that have the column.
    /// Used to determine if a column reference is unambiguous (exactly 1 match).
    pub fn find_tables_with_column<'a>(
        &self,
        column: &str,
        tables_in_scope: &'a [String],
    ) -> Vec<&'a String> {
        let col = column.to_lowercase();

        tables_in_scope
            .iter()
            .filter(|table_ref| {
                let key = table_ref.to_lowercase();
                self.table_columns
                    .get(&key)
                    .is_some_and(|cols| cols.contains(&col))
            })
            .collect()
    }

    /// Get the number of tables registered
    #[cfg(test)]
    pub fn table_count(&self) -> usize {
        self.table_columns.len()
    }

    /// Check if a view has SELECT * (wildcard columns)
    ///
    /// Views with SELECT * have "unknown columns" that can't be resolved statically
    /// because the underlying table columns may change. When such a view is in scope,
    /// unqualified column resolution should be more conservative.
    #[cfg(test)]
    pub fn view_has_wildcard(&self, view_ref: &str) -> bool {
        let key = view_ref.to_lowercase();
        self.views_with_wildcard.contains(&key)
    }
}

/// Find the unique table that has a specific column among the tables in scope.
///
/// Returns `Some(&table_ref)` only if exactly one table in scope has the column.
/// Returns `None` if 0 or >1 tables have the column (ambiguous or unknown).
///
/// This is the core resolution logic for unqualified columns.
#[allow(dead_code)] // Used in tests and for documentation of the resolution pattern
pub fn find_table_with_column<'a>(
    column: &str,
    tables_in_scope: &'a [String],
    registry: &ColumnRegistry,
) -> Option<&'a String> {
    let matches = registry.find_tables_with_column(column, tables_in_scope);

    match matches.len() {
        1 => Some(matches[0]), // Unique match - safe to resolve
        _ => None,             // 0 or ambiguous - skip resolution
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ColumnElement, TableElement, ViewElement};

    fn create_test_column(name: &str) -> ColumnElement {
        ColumnElement {
            name: name.to_string(),
            data_type: "int".to_string(),
            nullability: None,
            is_identity: false,
            is_rowguidcol: false,
            is_sparse: false,
            is_filestream: false,
            default_value: None,
            max_length: None,
            precision: None,
            scale: None,
            attached_annotations: vec![],
            inline_constraint_annotation: None,
            computed_expression: None,
            is_persisted: false,
            collation: None,
            is_generated_always_start: false,
            is_generated_always_end: false,
            is_hidden: false,
            masking_function: None,
        }
    }

    fn create_test_table(schema: &str, name: &str, columns: &[&str]) -> TableElement {
        TableElement {
            schema: schema.to_string(),
            name: name.to_string(),
            columns: columns.iter().map(|c| create_test_column(c)).collect(),
            is_node: false,
            is_edge: false,
            inline_constraint_disambiguators: vec![],
            attached_annotations_before_annotation: vec![],
            attached_annotations_after_annotation: vec![],
            system_time_start_column: None,
            system_time_end_column: None,
            is_system_versioned: false,
            history_table_schema: None,
            history_table_name: None,
        }
    }

    #[test]
    fn test_column_registry_from_model() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["Id", "Name", "Email"],
        )));
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Orders",
            &["Id", "UserId", "Total"],
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");

        assert_eq!(registry.table_count(), 2);
        assert!(registry.table_has_column("[dbo].[Users]", "Id"));
        assert!(registry.table_has_column("[dbo].[Users]", "Name"));
        assert!(registry.table_has_column("[dbo].[Orders]", "UserId"));
        assert!(!registry.table_has_column("[dbo].[Users]", "UserId"));
    }

    #[test]
    fn test_column_registry_case_insensitive() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["Id", "Name"],
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");

        // Table ref case insensitive
        assert!(registry.table_has_column("[DBO].[USERS]", "Id"));
        assert!(registry.table_has_column("[dbo].[users]", "Id"));

        // Column name case insensitive
        assert!(registry.table_has_column("[dbo].[Users]", "ID"));
        assert!(registry.table_has_column("[dbo].[Users]", "id"));
        assert!(registry.table_has_column("[dbo].[Users]", "NAME"));
    }

    #[test]
    fn test_find_tables_with_column_unique() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["Id", "Name"],
        )));
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Orders",
            &["Id", "UserId"],
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");
        let tables = vec!["[dbo].[Users]".to_string(), "[dbo].[Orders]".to_string()];

        // "Name" only exists in Users
        let matches = registry.find_tables_with_column("Name", &tables);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "[dbo].[Users]");

        // "UserId" only exists in Orders
        let matches = registry.find_tables_with_column("UserId", &tables);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "[dbo].[Orders]");
    }

    #[test]
    fn test_find_tables_with_column_ambiguous() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["Id", "Name"],
        )));
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Orders",
            &["Id", "UserId"],
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");
        let tables = vec!["[dbo].[Users]".to_string(), "[dbo].[Orders]".to_string()];

        // "Id" exists in both tables - ambiguous
        let matches = registry.find_tables_with_column("Id", &tables);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_find_tables_with_column_none() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["Id", "Name"],
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");
        let tables = vec!["[dbo].[Users]".to_string()];

        // "NonExistent" column doesn't exist
        let matches = registry.find_tables_with_column("NonExistent", &tables);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_find_table_with_column_unique_match() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["Id", "Name"],
        )));
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Orders",
            &["Id", "UserId"],
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");
        let tables = vec!["[dbo].[Users]".to_string(), "[dbo].[Orders]".to_string()];

        // Unique match
        let result = find_table_with_column("Name", &tables, &registry);
        assert_eq!(result, Some(&"[dbo].[Users]".to_string()));

        let result = find_table_with_column("UserId", &tables, &registry);
        assert_eq!(result, Some(&"[dbo].[Orders]".to_string()));
    }

    #[test]
    fn test_find_table_with_column_ambiguous_returns_none() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["Id", "Name"],
        )));
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Orders",
            &["Id", "UserId"],
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");
        let tables = vec!["[dbo].[Users]".to_string(), "[dbo].[Orders]".to_string()];

        // "Id" is ambiguous (both tables have it) - should return None
        let result = find_table_with_column("Id", &tables, &registry);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_table_with_column_no_match_returns_none() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["Id", "Name"],
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");
        let tables = vec!["[dbo].[Users]".to_string()];

        // No table has this column
        let result = find_table_with_column("NonExistent", &tables, &registry);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_table_with_column_case_insensitive() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["FirstName"],
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");
        let tables = vec!["[dbo].[Users]".to_string()];

        // Should find regardless of case
        let result = find_table_with_column("FIRSTNAME", &tables, &registry);
        assert_eq!(result, Some(&"[dbo].[Users]".to_string()));

        let result = find_table_with_column("firstname", &tables, &registry);
        assert_eq!(result, Some(&"[dbo].[Users]".to_string()));
    }

    // =========================================================================
    // View Column Extraction Tests (Phase 50.2)
    // =========================================================================

    fn create_test_view(schema: &str, name: &str, definition: &str) -> ViewElement {
        ViewElement {
            schema: schema.to_string(),
            name: name.to_string(),
            definition: definition.into(),
            is_schema_bound: false,
            is_with_check_option: false,
            is_metadata_reported: false,
        }
    }

    #[test]
    fn test_view_column_extraction_explicit_columns() {
        // Create the underlying table first
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Products",
            &["Id", "Name", "Price", "Category"],
        )));

        // Add a view with explicit columns (no SELECT *)
        model.elements.push(ModelElement::View(create_test_view(
            "dbo",
            "vProductSummary",
            "CREATE VIEW [dbo].[vProductSummary] AS SELECT Id, Name, Price FROM [dbo].[Products]",
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");

        // Should have both table and view
        assert_eq!(registry.table_count(), 2);

        // View should have the extracted columns
        assert!(registry.table_has_column("[dbo].[vProductSummary]", "Id"));
        assert!(registry.table_has_column("[dbo].[vProductSummary]", "Name"));
        assert!(registry.table_has_column("[dbo].[vProductSummary]", "Price"));
        // Category was not selected, should NOT be in view
        assert!(!registry.table_has_column("[dbo].[vProductSummary]", "Category"));

        // View should NOT have wildcard flag
        assert!(!registry.view_has_wildcard("[dbo].[vProductSummary]"));
    }

    #[test]
    fn test_view_column_extraction_with_aliases() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Customers",
            &["Id", "FirstName", "LastName"],
        )));

        // View with column aliases
        model.elements.push(ModelElement::View(create_test_view(
            "dbo",
            "vCustomerNames",
            "CREATE VIEW [dbo].[vCustomerNames] AS SELECT Id, FirstName AS GivenName, LastName AS FamilyName FROM [dbo].[Customers]",
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");

        // View columns should use the alias names
        assert!(registry.table_has_column("[dbo].[vCustomerNames]", "Id"));
        assert!(registry.table_has_column("[dbo].[vCustomerNames]", "GivenName"));
        assert!(registry.table_has_column("[dbo].[vCustomerNames]", "FamilyName"));
        // Original column names should NOT be present (alias replaces them)
        assert!(!registry.table_has_column("[dbo].[vCustomerNames]", "FirstName"));
        assert!(!registry.table_has_column("[dbo].[vCustomerNames]", "LastName"));
    }

    #[test]
    fn test_view_column_extraction_select_star() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Orders",
            &["Id", "CustomerId", "Total", "OrderDate"],
        )));

        // View with SELECT * - columns should be expanded from underlying table
        model.elements.push(ModelElement::View(create_test_view(
            "dbo",
            "vAllOrders",
            "CREATE VIEW [dbo].[vAllOrders] AS SELECT * FROM [dbo].[Orders]",
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");

        // View should have all columns from the underlying table
        assert!(registry.table_has_column("[dbo].[vAllOrders]", "Id"));
        assert!(registry.table_has_column("[dbo].[vAllOrders]", "CustomerId"));
        assert!(registry.table_has_column("[dbo].[vAllOrders]", "Total"));
        assert!(registry.table_has_column("[dbo].[vAllOrders]", "OrderDate"));

        // View SHOULD have wildcard flag (SELECT * means columns are dynamically determined)
        assert!(registry.view_has_wildcard("[dbo].[vAllOrders]"));
    }

    #[test]
    fn test_view_column_resolution_unique() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Users",
            &["Id", "Name", "Email"],
        )));
        model.elements.push(ModelElement::View(create_test_view(
            "dbo",
            "vUserEmails",
            "CREATE VIEW [dbo].[vUserEmails] AS SELECT Id, Email FROM [dbo].[Users]",
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");
        let tables_and_views = vec![
            "[dbo].[Users]".to_string(),
            "[dbo].[vUserEmails]".to_string(),
        ];

        // "Name" only exists in Users table
        let result = find_table_with_column("Name", &tables_and_views, &registry);
        assert_eq!(result, Some(&"[dbo].[Users]".to_string()));

        // "Id" exists in both - ambiguous
        let result = find_table_with_column("Id", &tables_and_views, &registry);
        assert_eq!(result, None);

        // "Email" exists in both
        let result = find_table_with_column("Email", &tables_and_views, &registry);
        assert_eq!(result, None);
    }

    #[test]
    fn test_view_column_case_insensitive() {
        let mut model = DatabaseModel::default();
        model.elements.push(ModelElement::Table(create_test_table(
            "dbo",
            "Products",
            &["ProductId", "ProductName"],
        )));
        model.elements.push(ModelElement::View(create_test_view(
            "dbo",
            "vProducts",
            "CREATE VIEW [dbo].[vProducts] AS SELECT ProductId, ProductName AS Name FROM [dbo].[Products]",
        )));

        let registry = ColumnRegistry::from_model(&model, "dbo");

        // View reference case insensitive
        assert!(registry.table_has_column("[DBO].[VPRODUCTS]", "ProductId"));
        assert!(registry.table_has_column("[dbo].[vproducts]", "ProductId"));

        // Column name case insensitive
        assert!(registry.table_has_column("[dbo].[vProducts]", "PRODUCTID"));
        assert!(registry.table_has_column("[dbo].[vProducts]", "NAME"));
    }
}
