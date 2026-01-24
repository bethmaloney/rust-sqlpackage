//! Index Building Tests

use super::parse_and_build_model;

// ============================================================================
// Index Building Tests
// ============================================================================

#[test]
fn test_build_index_element() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_Column]
ON [dbo].[T] ([Column1]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(
        index.name.contains("IX_"),
        "Index name should contain IX_, got: {}",
        index.name
    );
}

#[test]
fn test_build_clustered_index() {
    let sql = r#"
CREATE CLUSTERED INDEX [IX_T_Clustered]
ON [dbo].[T] ([Column1]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(index.is_clustered, "Index should be clustered");
}

#[test]
fn test_build_index_with_included_columns() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_WithInclude]
ON [dbo].[T] ([Column1])
INCLUDE ([Column2], [Column3]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(
        index.name.contains("IX_T_WithInclude"),
        "Index should be named correctly"
    );

    // Verify include columns are captured
    assert_eq!(
        index.include_columns.len(),
        2,
        "Index should have 2 include columns"
    );
    assert!(
        index.include_columns.contains(&"Column2".to_string()),
        "Include columns should contain Column2"
    );
    assert!(
        index.include_columns.contains(&"Column3".to_string()),
        "Include columns should contain Column3"
    );
}

#[test]
fn test_build_index_with_single_include_column() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_SingleInclude]
ON [dbo].[T] ([Col1], [Col2])
INCLUDE ([Col3]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert_eq!(index.columns.len(), 2, "Index should have 2 key columns");
    assert_eq!(
        index.include_columns.len(),
        1,
        "Index should have 1 include column"
    );
    assert_eq!(index.include_columns[0], "Col3");
}

#[test]
fn test_build_index_without_include_clause() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_NoInclude]
ON [dbo].[T] ([Column1], [Column2]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(
        index.include_columns.is_empty(),
        "Index without INCLUDE should have empty include_columns"
    );
}

#[test]
fn test_build_unique_clustered_index_with_include() {
    let sql = r#"
CREATE UNIQUE NONCLUSTERED INDEX [IX_T_UniqueWithInclude]
ON [dbo].[T] ([KeyCol])
INCLUDE ([Data1], [Data2], [Data3]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");
    let index = index.unwrap();
    assert!(index.is_unique, "Index should be unique");
    assert!(!index.is_clustered, "Index should be nonclustered");
    assert_eq!(index.columns.len(), 1, "Index should have 1 key column");
    assert_eq!(
        index.include_columns.len(),
        3,
        "Index should have 3 include columns"
    );
}
