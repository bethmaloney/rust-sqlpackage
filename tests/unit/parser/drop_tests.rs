//! DROP statement parsing tests

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
// DROP Statement Parsing Tests
// ============================================================================

#[test]
fn test_parse_drop_table() {
    let sql = "DROP TABLE [dbo].[TempTable];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP TABLE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { object_type, .. }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::Table);
        }
        _ => panic!("Expected DROP TABLE statement"),
    }
}

#[test]
fn test_parse_drop_table_if_exists() {
    let sql = "DROP TABLE IF EXISTS [dbo].[TempTable];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP TABLE IF EXISTS: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_view() {
    let sql = "DROP VIEW [dbo].[MyView];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP VIEW: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_procedure() {
    let sql = "DROP PROCEDURE [dbo].[MyProc];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP PROCEDURE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_function() {
    let sql = "DROP FUNCTION [dbo].[MyFunc];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP FUNCTION: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_index() {
    // T-SQL DROP INDEX syntax: DROP INDEX [name] ON [table]
    let sql = "DROP INDEX [IX_Users_Email] ON [dbo].[Users];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP INDEX ON table syntax not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_schema() {
    let sql = "DROP SCHEMA [temp_schema];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP SCHEMA: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_trigger() {
    let sql = "DROP TRIGGER [dbo].[TR_Users_Insert];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP TRIGGER not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_multiple_tables() {
    let sql = "DROP TABLE [dbo].[Table1], [dbo].[Table2], [dbo].[Table3];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP multiple tables: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_type() {
    let sql = "DROP TYPE [dbo].[AddressTableType];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP TYPE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { object_type, .. }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::Type);
        }
        _ => panic!("Expected DROP TYPE statement"),
    }
}

#[test]
fn test_parse_drop_type_if_exists() {
    let sql = "DROP TYPE IF EXISTS [dbo].[OrderItemType];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP TYPE IF EXISTS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { if_exists, .. }) => {
            assert!(*if_exists, "Expected IF EXISTS to be true");
        }
        _ => panic!("Expected DROP statement"),
    }
}

#[test]
fn test_parse_drop_sequence() {
    let sql = "DROP SEQUENCE [dbo].[OrderNumberSequence];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP SEQUENCE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { object_type, .. }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::Sequence);
        }
        _ => panic!("Expected DROP SEQUENCE statement"),
    }
}

#[test]
fn test_parse_drop_sequence_if_exists() {
    let sql = "DROP SEQUENCE IF EXISTS [dbo].[InvoiceSequence];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP SEQUENCE IF EXISTS: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_synonym() {
    let sql = "DROP SYNONYM [dbo].[ProductAlias];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP SYNONYM not supported by sqlparser: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_synonym_if_exists() {
    let sql = "DROP SYNONYM IF EXISTS [dbo].[CustomerAlias];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP SYNONYM IF EXISTS not supported by sqlparser: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_view_if_exists() {
    let sql = "DROP VIEW IF EXISTS [dbo].[vw_ActiveUsers];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP VIEW IF EXISTS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop {
            object_type,
            if_exists,
            ..
        }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::View);
            assert!(*if_exists, "Expected IF EXISTS to be true");
        }
        _ => panic!("Expected DROP VIEW statement"),
    }
}

#[test]
fn test_parse_drop_procedure_if_exists() {
    let sql = "DROP PROCEDURE IF EXISTS [dbo].[usp_GetUserById];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP PROCEDURE IF EXISTS: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_function_if_exists() {
    let sql = "DROP FUNCTION IF EXISTS [dbo].[fn_CalculateTotal];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP FUNCTION IF EXISTS: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_trigger_if_exists() {
    let sql = "DROP TRIGGER IF EXISTS [dbo].[TR_Orders_Insert];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP TRIGGER IF EXISTS not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_schema_if_exists() {
    let sql = "DROP SCHEMA IF EXISTS [staging];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP SCHEMA IF EXISTS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop {
            object_type,
            if_exists,
            ..
        }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::Schema);
            assert!(*if_exists, "Expected IF EXISTS to be true");
        }
        _ => panic!("Expected DROP SCHEMA statement"),
    }
}

#[test]
fn test_parse_drop_index_if_exists() {
    // T-SQL: DROP INDEX IF EXISTS [name] ON [table]
    let sql = "DROP INDEX IF EXISTS [IX_Users_Email] ON [dbo].[Users];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP INDEX IF EXISTS ON table syntax not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_view_with_ast_verification() {
    let sql = "DROP VIEW [dbo].[vw_CustomerOrders];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP VIEW: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop {
            object_type, names, ..
        }) => {
            assert_eq!(*object_type, sqlparser::ast::ObjectType::View);
            assert_eq!(names.len(), 1);
            assert!(names[0].to_string().contains("vw_CustomerOrders"));
        }
        _ => panic!("Expected DROP VIEW statement"),
    }
}

#[test]
fn test_parse_drop_multiple_views() {
    let sql = "DROP VIEW [dbo].[View1], [dbo].[View2];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP multiple views: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Drop { names, .. }) => {
            assert_eq!(names.len(), 2, "Expected 2 view names in DROP statement");
        }
        _ => panic!("Expected DROP statement"),
    }
}

#[test]
fn test_parse_drop_multiple_procedures() {
    let sql = "DROP PROCEDURE [dbo].[Proc1], [dbo].[Proc2], [dbo].[Proc3];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP multiple procedures: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_multiple_functions() {
    let sql = "DROP FUNCTION [dbo].[Func1], [dbo].[Func2];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP multiple functions: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_drop_proc_abbreviation() {
    // SQL Server supports DROP PROC as abbreviation for DROP PROCEDURE
    let sql = "DROP PROC [dbo].[MyProc];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP PROC abbreviation not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_drop_statements_in_batch() {
    let sql = r#"
DROP TABLE IF EXISTS [dbo].[TempTable1];
GO
DROP VIEW IF EXISTS [dbo].[TempView];
GO
DROP PROCEDURE IF EXISTS [dbo].[TempProc];
GO
DROP FUNCTION IF EXISTS [dbo].[TempFunc];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DROP statements in batch: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 4, "Expected 4 DROP statements in batch");
}

#[test]
fn test_parse_drop_cascade_restrict() {
    // SQL Server doesn't use CASCADE/RESTRICT like PostgreSQL, but sqlparser may parse it
    let sql = "DROP TABLE [dbo].[Orders] CASCADE;";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP TABLE CASCADE syntax not supported: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

// ============================================================================
// DROP/CREATE SYNONYM Tests
// ============================================================================

#[test]
fn test_parse_drop_and_recreate_synonym() {
    // SQL Server doesn't have ALTER SYNONYM - you must drop and recreate
    let sql = r#"
DROP SYNONYM [dbo].[ProductAlias];
GO
CREATE SYNONYM [dbo].[ProductAlias] FOR [inventory].[Products];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "DROP/CREATE SYNONYM not supported by sqlparser: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2);
}
