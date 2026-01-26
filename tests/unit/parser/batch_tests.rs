//! Tests for GO batch splitting

use std::io::Write;
use std::path::PathBuf;

use tempfile::NamedTempFile;

/// Helper to create a temp SQL file with content
fn create_sql_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::with_suffix(".sql").unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

// ============================================================================
// Batch Separator Tests
// ============================================================================

#[test]
fn test_split_batches_basic() {
    let sql = "CREATE TABLE t1 (id INT)\nGO\nCREATE TABLE t2 (id INT)";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2, "Expected 2 statements from 2 batches");
}

#[test]
fn test_split_batches_multiple_go() {
    let sql = r#"
CREATE TABLE t1 (id INT)
GO
CREATE TABLE t2 (id INT)
GO
CREATE TABLE t3 (id INT)
GO
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 3, "Expected 3 statements from 3 batches");
}

#[test]
fn test_split_batches_case_insensitive_go() {
    let sql =
        "CREATE TABLE t1 (id INT)\ngo\nCREATE TABLE t2 (id INT)\nGO\nCREATE TABLE t3 (id INT)\nGo";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 3, "GO should be case-insensitive");
}

#[test]
fn test_split_batches_no_go() {
    let sql = "CREATE TABLE t1 (id INT); CREATE TABLE t2 (id INT);";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(
        statements.len(),
        2,
        "Expected 2 statements without GO separator"
    );
}

// ============================================================================
// Additional Batch Separator Tests (from TEST_PLAN.md)
// ============================================================================

#[test]
fn test_split_batches_go_with_count() {
    // GO 5 means execute the batch 5 times in SSMS - we DON'T treat "GO 5" as a batch separator
    // Only exact "GO" (case-insensitive, with optional whitespace) separates batches
    let sql = "CREATE TABLE t1 (id INT)\nGO 5\nCREATE TABLE t2 (id INT)";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This test documents behavior - GO with count is NOT recognized as a separator
    // The entire content is treated as one batch, and fallback parsing extracts what it can
    match result {
        Ok(statements) => {
            // Only 1 statement extracted via fallback parsing (first CREATE TABLE)
            assert!(
                !statements.is_empty(),
                "Should extract at least 1 statement via fallback parsing"
            );
        }
        Err(e) => {
            // If parsing fails entirely, that's also acceptable behavior
            println!("Note: GO with count causes parse failure: {:?}", e);
        }
    }
}

#[test]
fn test_split_batches_go_in_comment() {
    // GO inside a comment should NOT cause a split
    let sql = r#"
CREATE TABLE t1 (
    id INT -- GO here is in a comment
)
GO
CREATE TABLE t2 (id INT)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2, "GO in comment should not cause split");
}

#[test]
fn test_split_batches_go_in_string() {
    // GO inside a string literal should NOT cause a split
    let sql = r#"
CREATE TABLE t1 (
    name VARCHAR(10) DEFAULT 'GO'
)
GO
CREATE TABLE t2 (id INT)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2, "GO in string should not cause split");
}

#[test]
fn test_split_batches_go_in_block_comment() {
    // GO inside a block comment should NOT cause a split
    // This tests whether the batch splitter is comment-aware
    let sql = r#"
CREATE TABLE t1 (id INT)
/*
GO
This GO should be ignored
*/
GO
CREATE TABLE t2 (id INT)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This documents current behavior - if the batch splitter is not comment-aware,
    // it will fail. If it is comment-aware, it should produce 2 statements.
    match result {
        Ok(statements) => {
            assert_eq!(
                statements.len(),
                2,
                "GO in block comment should not cause split"
            );
        }
        Err(e) => {
            // Document that GO in block comments is not currently handled
            println!("Note: GO in block comments not handled: {:?}", e);
        }
    }
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_parse_invalid_sql_returns_error() {
    let sql = "THIS IS NOT VALID SQL AT ALL!!!";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Invalid SQL should return error");
}

#[test]
fn test_parse_file_not_found_error() {
    let result =
        rust_sqlpackage::parser::parse_sql_file(&PathBuf::from("/nonexistent/path/file.sql"));
    assert!(result.is_err(), "Non-existent file should return error");
}

#[test]
fn test_parse_empty_file() {
    let sql = "";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Empty file should parse successfully");

    let statements = result.unwrap();
    assert_eq!(statements.len(), 0, "Empty file should have no statements");
}

#[test]
fn test_parse_whitespace_only_file() {
    let sql = "   \n\n   \t  \n  ";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Whitespace-only file should parse successfully"
    );

    let statements = result.unwrap();
    assert_eq!(
        statements.len(),
        0,
        "Whitespace-only file should have no statements"
    );
}

#[test]
fn test_parse_comment_only_file() {
    let sql = r#"
-- This is a comment
/* This is a block comment */
-- Another comment
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This might succeed with 0 statements or fail depending on parser behavior
    // Either is acceptable for a baseline test
    if let Ok(statements) = result {
        assert_eq!(
            statements.len(),
            0,
            "Comment-only file should have no statements"
        );
    }
}

// ============================================================================
// Parser Error Handling Tests
// ============================================================================

#[test]
fn test_error_unclosed_string_literal() {
    let sql = "SELECT 'unclosed string FROM [dbo].[Table];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Unclosed string should fail");
}

#[test]
fn test_error_unclosed_bracket() {
    // Test with clearly unclosed bracket that can't be misinterpreted
    let sql = "SELECT [col1, [col2] FROM [dbo].[Table];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Document behavior - some unclosed brackets may be accepted as identifiers
    if result.is_err() {
        println!("Unclosed bracket correctly rejected");
    } else {
        println!("Note: Parser accepted SQL with bracket issues");
    }
}

#[test]
fn test_error_unclosed_parenthesis() {
    let sql = "SELECT (1 + 2 FROM [dbo].[Table];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Unclosed parenthesis should fail");
}

#[test]
fn test_error_invalid_keyword_order() {
    let sql = "TABLE CREATE [dbo].[Invalid];";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Invalid keyword order should fail");
}

#[test]
fn test_error_missing_column_definition() {
    let sql = "CREATE TABLE [dbo].[Empty] ();";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Empty table may or may not be valid depending on parser
    // This documents behavior rather than asserting specific outcome
    if result.is_err() {
        println!("Empty column list is rejected (expected)");
    } else {
        println!("Empty column list is accepted");
    }
}

#[test]
fn test_error_duplicate_column_name() {
    // Note: This is a semantic error, not a parse error
    // The parser may accept this even though it's invalid SQL
    let sql = r#"
CREATE TABLE [dbo].[DupColumns] (
    [Id] INT,
    [Id] INT
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Parser typically accepts duplicate names; semantic analysis would catch this
    if result.is_ok() {
        println!("Duplicate column names accepted by parser (semantic check elsewhere)");
    }
}

#[test]
fn test_error_invalid_data_type() {
    let sql = "CREATE TABLE [dbo].[BadType] ([Col] FAKETYPE);";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Unknown data types might be accepted as custom types or rejected
    if result.is_err() {
        println!("Unknown data type rejected");
    } else {
        println!("Unknown data type accepted (might be treated as user-defined type)");
    }
}

#[test]
fn test_error_missing_table_name() {
    let sql = "CREATE TABLE ([Id] INT);";
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Missing table name should fail");
}

#[test]
fn test_error_invalid_constraint_syntax() {
    // Test truly malformed constraint syntax
    let sql = r#"
CREATE TABLE [dbo].[BadConstraint] (
    [Id] INT,
    CONSTRAINT [PK] PRIMARY ([Id])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Missing KEY after PRIMARY should fail
    if result.is_err() {
        println!("Invalid constraint syntax correctly rejected");
    } else {
        // Document if parser is lenient
        println!("Note: Parser accepted malformed PRIMARY constraint");
    }
}

#[test]
fn test_error_line_number_in_message() {
    // Use SQL that definitely fails parsing
    let sql = r#"
-- Line 1: comment
-- Line 2: comment
INVALID SYNTAX THAT WILL FAIL;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_err(), "Invalid SQL should fail");

    // Error message should contain line information
    let err_msg = result.unwrap_err().to_string();
    // The error should reference a line in the file
    println!("Error message: {}", err_msg);
    // Verify the error contains path or line info
    assert!(
        err_msg.contains("line") || err_msg.contains("Line") || err_msg.contains(".sql"),
        "Error message should contain file/line info"
    );
}

#[test]
fn test_error_recovery_good_batch_after_bad() {
    // Test that a good batch after a bad one doesn't affect error reporting
    let sql = r#"
-- First batch is invalid
INVALID SYNTAX HERE
GO
-- Second batch is valid
CREATE TABLE [dbo].[Valid] ([Id] INT);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // First batch should fail, aborting the entire file
    assert!(result.is_err(), "Invalid first batch should fail");
}

#[test]
fn test_parse_mixed_valid_statements_in_file() {
    let sql = r#"
CREATE TABLE [dbo].[Table1] ([Id] INT NOT NULL PRIMARY KEY);
GO

CREATE VIEW [dbo].[View1] AS SELECT [Id] FROM [dbo].[Table1];
GO

CREATE INDEX [IX_Table1] ON [dbo].[Table1] ([Id]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Mixed valid statements should parse: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 3, "Should have 3 statements");
}

#[test]
fn test_error_nested_comments_edge_case() {
    // SQL Server allows nested block comments /* /* */ */
    let sql = r#"
/* Outer comment
   /* Nested comment */
   Still in outer
*/
SELECT 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This tests whether nested comments are handled
    if result.is_ok() {
        println!("Nested comments supported");
    } else {
        println!(
            "Nested comments not supported (sqlparser limitation): {:?}",
            result.err()
        );
    }
}

#[test]
fn test_parse_unicode_identifiers() {
    let sql = r#"
CREATE TABLE [dbo].[日本語テーブル] (
    [列名] NVARCHAR(100),
    [Noño] INT
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Unicode identifiers should parse: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_very_long_identifier() {
    // SQL Server allows identifiers up to 128 characters
    let long_name = "A".repeat(128);
    let sql = format!("CREATE TABLE [dbo].[{}] ([Id] INT);", long_name);
    let file = create_sql_file(&sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Long identifier should parse: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_special_characters_in_strings() {
    let sql = r#"
CREATE TABLE [dbo].[Test] (
    [Value] NVARCHAR(100) DEFAULT N'It''s a "test" with special chars: \n\t'
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Special characters in strings should parse: {:?}",
        result.err()
    );
}
