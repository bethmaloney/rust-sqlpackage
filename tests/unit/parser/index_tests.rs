//! CREATE INDEX parsing tests

use std::io::Write;

use tempfile::NamedTempFile;

/// Helper to create a temp SQL file with content
fn create_sql_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::with_suffix(".sql").unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

// ============================================================================
// CREATE INDEX Parsing Tests
// ============================================================================

#[test]
fn test_parse_nonclustered_index() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Table_Column]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse nonclustered index: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_clustered_index() {
    let sql = r#"
CREATE CLUSTERED INDEX [IX_Table_Clustered]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse clustered index: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_unique_index() {
    let sql = r#"
CREATE UNIQUE NONCLUSTERED INDEX [IX_Table_Unique]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse unique index: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_index_with_include() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Table_WithInclude]
ON [dbo].[SomeTable] ([Column1])
INCLUDE ([Column2], [Column3]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse index with include: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_index_include_extracts_columns() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Test_Include]
ON [dbo].[TestTable] ([KeyCol1], [KeyCol2])
INCLUDE ([IncludeCol1], [IncludeCol2], [IncludeCol3]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify the fallback type captured include columns
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            name,
            columns,
            include_columns,
            ..
        }) => {
            assert_eq!(name, "IX_Test_Include");
            assert_eq!(columns.len(), 2, "Should have 2 key columns");
            assert_eq!(include_columns.len(), 3, "Should have 3 include columns");
            assert!(include_columns.contains(&"IncludeCol1".to_string()));
            assert!(include_columns.contains(&"IncludeCol2".to_string()));
            assert!(include_columns.contains(&"IncludeCol3".to_string()));
        }
        _ => panic!("Expected Index fallback type"),
    }
}

#[test]
fn test_parse_index_include_single_column() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Single]
ON [dbo].[T] ([A])
INCLUDE ([B]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok());

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            include_columns, ..
        }) => {
            assert_eq!(include_columns.len(), 1);
            assert_eq!(include_columns[0], "B");
        }
        _ => panic!("Expected Index fallback type"),
    }
}

#[test]
fn test_parse_index_no_include() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_NoInclude]
ON [dbo].[T] ([Col1], [Col2]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok());

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            include_columns, ..
        }) => {
            assert!(
                include_columns.is_empty(),
                "Index without INCLUDE should have no include_columns"
            );
        }
        _ => panic!("Expected Index fallback type"),
    }
}

#[test]
fn test_parse_unique_nonclustered_index_with_include() {
    let sql = r#"
CREATE UNIQUE NONCLUSTERED INDEX [IX_Unique_Include]
ON [dbo].[Orders] ([OrderNumber])
INCLUDE ([CustomerName], [OrderDate], [Total]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok());

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            is_unique,
            is_clustered,
            include_columns,
            ..
        }) => {
            assert!(*is_unique, "Index should be unique");
            assert!(!*is_clustered, "Index should be nonclustered");
            assert_eq!(include_columns.len(), 3);
        }
        _ => panic!("Expected Index fallback type"),
    }
}

#[test]
fn test_parse_index_missing_whitespace_before_on() {
    // Edge case: SQL with missing whitespace between ] and ON (e.g., "]ON" instead of "] ON")
    // This pattern appears in some real-world SQL files
    let sql = r#"CREATE NONCLUSTERED INDEX [IX_Test]ON [dbo].[Table] ([Col1])INCLUDE ([Col2])"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Should parse index with missing whitespace: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Index {
            name,
            table_name,
            columns,
            include_columns,
            ..
        }) => {
            assert_eq!(name, "IX_Test");
            assert_eq!(table_name, "Table");
            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0], "Col1");
            assert_eq!(include_columns.len(), 1);
            assert_eq!(include_columns[0], "Col2");
        }
        _ => panic!("Expected Index fallback type"),
    }
}

// ============================================================================
// Standard CREATE INDEX Test (workaround for sqlparser-rs limitation)
// ============================================================================

#[test]
fn test_parse_standard_index() {
    // Use CREATE INDEX without CLUSTERED/NONCLUSTERED (supported by sqlparser-rs)
    let sql = r#"
CREATE INDEX [IX_Table_Column]
ON [dbo].[SomeTable] ([Column1]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Standard CREATE INDEX should be supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}
