//! CREATE FUNCTION parsing tests

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
// CREATE FUNCTION Parsing Tests (using fallback parser for T-SQL syntax)
// ============================================================================

#[test]
fn test_parse_scalar_function() {
    // T-SQL function syntax uses fallback parsing since MsSqlDialect doesn't support CREATE FUNCTION
    let sql = r#"
CREATE FUNCTION [dbo].[GetFullName]
(
    @FirstName NVARCHAR(50),
    @LastName NVARCHAR(50)
)
RETURNS NVARCHAR(101)
AS
BEGIN
    RETURN @FirstName + ' ' + @LastName
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse scalar function: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Functions always use fallback parsing with MsSqlDialect
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function {
            schema,
            name,
            function_type,
        }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetFullName");
            assert_eq!(
                *function_type,
                rust_sqlpackage::parser::FallbackFunctionType::Scalar
            );
        }
        _ => panic!("Expected Function fallback type"),
    }
}

#[test]
fn test_parse_table_valued_function() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetUserOrders]
(
    @UserId INT
)
RETURNS TABLE
AS
RETURN
(
    SELECT * FROM Orders WHERE UserId = @UserId
)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table-valued function: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function {
            schema,
            name,
            function_type,
        }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetUserOrders");
            assert_eq!(
                *function_type,
                rust_sqlpackage::parser::FallbackFunctionType::TableValued
            );
        }
        _ => panic!("Expected Function fallback type"),
    }
}

#[test]
fn test_parse_multi_statement_table_function() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetFilteredData]
(
    @MinValue INT
)
RETURNS @ResultTable TABLE
(
    Id INT,
    Value INT
)
AS
BEGIN
    INSERT INTO @ResultTable
    SELECT Id, Value FROM Data WHERE Value >= @MinValue
    RETURN
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse multi-statement table function: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function {
            schema,
            name,
            function_type,
        }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetFilteredData");
            // RETURNS @ResultTable TABLE should be detected as table-valued
            assert_eq!(
                *function_type,
                rust_sqlpackage::parser::FallbackFunctionType::TableValued
            );
        }
        _ => panic!("Expected Function fallback type"),
    }
}

#[test]
fn test_parse_function_or_alter() {
    let sql = r#"
CREATE OR ALTER FUNCTION [utils].[FormatDate]
(
    @Date DATETIME
)
RETURNS VARCHAR(10)
AS
BEGIN
    RETURN CONVERT(VARCHAR(10), @Date, 120)
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CREATE OR ALTER FUNCTION: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function { schema, name, .. }) => {
            assert_eq!(schema, "utils");
            assert_eq!(name, "FormatDate");
        }
        _ => panic!("Expected Function fallback type"),
    }
}

#[test]
fn test_parse_function_no_schema() {
    let sql = r#"
CREATE FUNCTION SimpleFunc()
RETURNS INT
AS
BEGIN
    RETURN 42
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse function without schema: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Function { schema, name, .. }) => {
            assert_eq!(schema, "dbo", "Should default to dbo schema");
            assert_eq!(name, "SimpleFunc");
        }
        _ => panic!("Expected Function fallback type"),
    }
}

// ============================================================================
// Mixed Procedures and Functions in Same File
// ============================================================================

#[test]
fn test_parse_multiple_procedures_and_functions() {
    // All batches use T-SQL syntax that requires fallback parsing
    let sql = r#"
CREATE PROCEDURE [dbo].[Proc1]
    @Id INT
AS
BEGIN
    SELECT @Id
END
GO

CREATE FUNCTION [dbo].[Func1]()
RETURNS INT
AS
BEGIN
    RETURN 1
END
GO

CREATE PROC [dbo].[Proc2]
AS
BEGIN
    SELECT 2
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse multiple procs/funcs: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 3, "Should have 3 statements");

    // Verify each statement has fallback type (uses T-SQL syntax)
    assert!(
        matches!(
            &statements[0].fallback_type,
            Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { name, .. }) if name == "Proc1"
        ),
        "First should be Proc1"
    );
    assert!(
        matches!(
            &statements[1].fallback_type,
            Some(rust_sqlpackage::parser::FallbackStatementType::Function { name, .. }) if name == "Func1"
        ),
        "Second should be Func1"
    );
    assert!(
        matches!(
            &statements[2].fallback_type,
            Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { name, .. }) if name == "Proc2"
        ),
        "Third should be Proc2"
    );
}

// ============================================================================
// Temporal Table Query Functions
// ============================================================================

#[test]
fn test_parse_temporal_table_query_contained_in() {
    // Function with CONTAINED IN temporal query
    let sql = r#"
CREATE FUNCTION [dbo].[GetEmployeeChangesContainedIn]
(
    @StartDate DATETIME2,
    @EndDate DATETIME2
)
RETURNS TABLE
AS
RETURN
(
    SELECT
        [EmployeeId],
        [Name],
        [Department],
        [Salary],
        [SysStartTime],
        [SysEndTime]
    FROM [dbo].[Employee]
    FOR SYSTEM_TIME CONTAINED IN (@StartDate, @EndDate)
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse function with FOR SYSTEM_TIME CONTAINED IN: {:?}",
        result.err()
    );
}
