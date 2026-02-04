//! Column registry for schema-aware unqualified column resolution
//!
//! This module provides a registry that maps tables/views to their columns,
//! enabling accurate resolution of unqualified column references by checking
//! which tables in scope actually have a given column.

use std::collections::{HashMap, HashSet};

use crate::model::{DatabaseModel, ModelElement};

/// Registry mapping tables and views to their column names.
///
/// Used during body dependency extraction to resolve unqualified columns
/// to the correct table when multiple tables are in scope.
///
/// All keys and column names are stored in lowercase for case-insensitive lookup.
#[derive(Debug, Default)]
pub struct ColumnRegistry {
    /// Maps lowercase `[schema].[table]` to set of lowercase column names
    table_columns: HashMap<String, HashSet<String>>,
}

impl ColumnRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            table_columns: HashMap::new(),
        }
    }

    /// Build a column registry from a DatabaseModel
    ///
    /// Extracts columns from:
    /// - TableElement objects (regular tables)
    /// - ViewElement objects (views, via their definition parsing)
    pub fn from_model(model: &DatabaseModel) -> Self {
        let mut registry = Self::new();

        for element in &model.elements {
            // Views don't have explicit column definitions in ViewElement,
            // so we can't extract columns from them directly.
            // Future enhancement: parse view SELECT to extract projected columns.
            if let ModelElement::Table(table) = element {
                let table_key = format!("[{}].[{}]", table.schema, table.name).to_lowercase();

                let columns: HashSet<String> = table
                    .columns
                    .iter()
                    .map(|c| c.name.to_lowercase())
                    .collect();

                registry.table_columns.insert(table_key, columns);
            }
        }

        registry
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
    use crate::model::{ColumnElement, TableElement};

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
            computed_expression: None,
            is_persisted: false,
            collation: None,
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

        let registry = ColumnRegistry::from_model(&model);

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

        let registry = ColumnRegistry::from_model(&model);

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

        let registry = ColumnRegistry::from_model(&model);
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

        let registry = ColumnRegistry::from_model(&model);
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

        let registry = ColumnRegistry::from_model(&model);
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

        let registry = ColumnRegistry::from_model(&model);
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

        let registry = ColumnRegistry::from_model(&model);
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

        let registry = ColumnRegistry::from_model(&model);
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

        let registry = ColumnRegistry::from_model(&model);
        let tables = vec!["[dbo].[Users]".to_string()];

        // Should find regardless of case
        let result = find_table_with_column("FIRSTNAME", &tables, &registry);
        assert_eq!(result, Some(&"[dbo].[Users]".to_string()));

        let result = find_table_with_column("firstname", &tables, &registry);
        assert_eq!(result, Some(&"[dbo].[Users]".to_string()));
    }
}
