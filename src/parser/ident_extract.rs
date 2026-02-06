//! Identifier extraction utilities for sqlparser Expr types.
//!
//! Note: For ObjectName types, use `extract_schema_and_name()` in builder.rs
//! which already correctly extracts unbracketed values via `.value.clone()`.

use sqlparser::ast::{Expr, ObjectName};

/// Extract the last identifier value from an ObjectName.
///
/// ObjectName is a Vec<Ident> representing qualified names like [dbo].[Table].
/// Returns the final component without brackets.
pub fn from_object_name(name: &ObjectName) -> String {
    name.0.last().map(|i| i.value.clone()).unwrap_or_default()
}

/// Extract column name from an Expr (for index column references).
///
/// Handles Identifier and CompoundIdentifier expressions.
/// Returns unbracketed column name, or None for complex expressions.
pub fn column_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(ident) => Some(ident.value.clone()),
        Expr::CompoundIdentifier(parts) => parts.last().map(|i| i.value.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::Ident;

    #[test]
    fn test_from_object_name() {
        let name = ObjectName(vec![
            Ident::with_quote('[', "dbo"),
            Ident::with_quote('[', "IX_Test"),
        ]);
        assert_eq!(from_object_name(&name), "IX_Test");
    }

    #[test]
    fn test_from_object_name_single() {
        let name = ObjectName(vec![Ident::with_quote('[', "IX_SinglePart")]);
        assert_eq!(from_object_name(&name), "IX_SinglePart");
    }

    #[test]
    fn test_from_object_name_unquoted() {
        let name = ObjectName(vec![Ident::new("dbo"), Ident::new("IX_Unquoted")]);
        assert_eq!(from_object_name(&name), "IX_Unquoted");
    }

    #[test]
    fn test_from_object_name_empty() {
        let name = ObjectName(vec![]);
        assert_eq!(from_object_name(&name), "");
    }

    #[test]
    fn test_column_from_expr_simple() {
        let expr = Expr::Identifier(Ident::with_quote('[', "CustomerId"));
        assert_eq!(column_from_expr(&expr), Some("CustomerId".to_string()));
    }

    #[test]
    fn test_column_from_expr_unquoted() {
        let expr = Expr::Identifier(Ident::new("CustomerId"));
        assert_eq!(column_from_expr(&expr), Some("CustomerId".to_string()));
    }

    #[test]
    fn test_column_from_expr_compound() {
        let expr = Expr::CompoundIdentifier(vec![
            Ident::with_quote('[', "dbo"),
            Ident::with_quote('[', "Table"),
            Ident::with_quote('[', "Column"),
        ]);
        assert_eq!(column_from_expr(&expr), Some("Column".to_string()));
    }

    #[test]
    fn test_column_from_expr_complex_returns_none() {
        // A function call should return None
        use sqlparser::ast::Function;
        let expr = Expr::Function(Function {
            name: ObjectName(vec![Ident::new("GETDATE")]),
            args: sqlparser::ast::FunctionArguments::None,
            filter: None,
            null_treatment: None,
            over: None,
            within_group: Vec::new(),
            parameters: sqlparser::ast::FunctionArguments::None,
            uses_odbc_syntax: false,
        });
        assert_eq!(column_from_expr(&expr), None);
    }
}
