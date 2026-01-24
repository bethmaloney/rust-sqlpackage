//! View Building Tests

use super::parse_and_build_model;

// ============================================================================
// View Building Tests
// ============================================================================

#[test]
fn test_build_view_element() {
    let sql = r#"
CREATE VIEW [dbo].[TestView]
AS
SELECT 1 AS [Value];
"#;
    let model = parse_and_build_model(sql);

    let view = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::View(v) = e {
            Some(v)
        } else {
            None
        }
    });

    assert!(view.is_some(), "Model should contain a view");
    let view = view.unwrap();
    assert!(
        view.name.contains("TestView"),
        "View name should be TestView"
    );
}

#[test]
fn test_build_view_with_select_statement() {
    let sql = r#"
CREATE VIEW [dbo].[DetailedView]
AS
SELECT [Id], [Name], [CreatedAt]
FROM [dbo].[SomeTable]
WHERE [IsActive] = 1;
"#;
    let model = parse_and_build_model(sql);

    let view = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::View(v) = e {
            Some(v)
        } else {
            None
        }
    });

    assert!(view.is_some(), "Model should contain a view");
    let view = view.unwrap();
    // View should have the definition stored
    assert!(!view.definition.is_empty(), "View should have a definition");
}
