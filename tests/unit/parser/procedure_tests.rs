//! CREATE PROCEDURE parsing tests

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
// CREATE PROCEDURE Parsing Tests (using fallback parser for T-SQL syntax)
// ============================================================================

#[test]
fn test_parse_simple_procedure() {
    // This syntax MAY be parsed by sqlparser (if it has BEGIN...END and parenthesized params)
    // or may use fallback parsing. Either way, it should parse successfully.
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUsers]
AS
BEGIN
    SELECT * FROM Users
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse simple procedure: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Either sqlparser parsed it or fallback did
    if let Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) =
        &statements[0].fallback_type
    {
        assert_eq!(schema, "dbo");
        assert_eq!(name, "GetUsers");
    } else if let Some(sqlparser::ast::Statement::CreateProcedure { name, .. }) =
        &statements[0].statement
    {
        assert!(name.to_string().contains("GetUsers"));
    } else {
        panic!("Expected CreateProcedure statement or fallback type");
    }
}

#[test]
fn test_parse_procedure_with_parameters() {
    // T-SQL style parameters (@param) will use fallback parsing
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUserById]
    @UserId INT,
    @IncludeDeleted BIT = 0
AS
BEGIN
    SELECT * FROM Users WHERE Id = @UserId
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with parameters: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // T-SQL @param syntax requires fallback parsing
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetUserById");
        }
        _ => panic!("Expected Procedure fallback type for T-SQL @param syntax"),
    }

    // Verify original SQL is preserved
    assert!(statements[0].sql_text.contains("@UserId INT"));
}

#[test]
fn test_parse_procedure_or_alter() {
    // CREATE OR ALTER with T-SQL @params will use fallback
    let sql = r#"
CREATE OR ALTER PROCEDURE [sales].[UpdateOrder]
    @OrderId INT,
    @Status VARCHAR(50)
AS
BEGIN
    UPDATE Orders SET Status = @Status WHERE Id = @OrderId
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CREATE OR ALTER PROCEDURE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "sales");
            assert_eq!(name, "UpdateOrder");
        }
        _ => panic!("Expected Procedure fallback type for CREATE OR ALTER with @params"),
    }
}

#[test]
fn test_parse_procedure_short_form() {
    // T-SQL PROC abbreviation - uses fallback parsing
    let sql = r#"
CREATE PROC [dbo].[QuickProc]
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse PROC abbreviation: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "QuickProc");
        }
        _ => panic!("Expected Procedure fallback type for PROC abbreviation"),
    }
}

#[test]
fn test_parse_procedure_no_schema() {
    // No schema specified - uses fallback, defaults to dbo
    let sql = r#"
CREATE PROCEDURE SimpleProc
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure without schema: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    // Check either fallback or sqlparser parsing
    if let Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) =
        &statements[0].fallback_type
    {
        assert_eq!(schema, "dbo", "Should default to dbo schema");
        assert_eq!(name, "SimpleProc");
    } else if let Some(sqlparser::ast::Statement::CreateProcedure { name, .. }) =
        &statements[0].statement
    {
        assert!(name.to_string().contains("SimpleProc"));
    } else {
        panic!("Expected CreateProcedure statement or fallback type");
    }
}

// ============================================================================
// OUTPUT Parameter Tests
// ============================================================================

#[test]
fn test_parse_procedure_with_output_parameter() {
    let sql = r#"
CREATE PROCEDURE [dbo].[GetNextId]
    @TableName NVARCHAR(128),
    @NextId INT OUTPUT
AS
BEGIN
    SELECT @NextId = MAX(Id) + 1 FROM sys.tables WHERE name = @TableName
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with OUTPUT parameter: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify the procedure is parsed with fallback
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "dbo");
            assert_eq!(name, "GetNextId");
        }
        _ => panic!("Expected Procedure fallback type for OUTPUT parameter syntax"),
    }

    // Verify original SQL preserves OUTPUT keyword
    assert!(
        statements[0].sql_text.contains("OUTPUT"),
        "SQL text should preserve OUTPUT keyword"
    );
}

#[test]
fn test_parse_procedure_with_out_abbreviation() {
    // SQL Server allows OUT as abbreviation for OUTPUT
    let sql = r#"
CREATE PROCEDURE [dbo].[GetValue]
    @Key VARCHAR(50),
    @Value VARCHAR(MAX) OUT
AS
BEGIN
    SELECT @Value = Value FROM Config WHERE [Key] = @Key
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with OUT abbreviation: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("OUT"),
        "SQL text should preserve OUT keyword"
    );
}

#[test]
fn test_parse_procedure_with_multiple_output_parameters() {
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUserDetails]
    @UserId INT,
    @FirstName NVARCHAR(50) OUTPUT,
    @LastName NVARCHAR(50) OUTPUT,
    @Email NVARCHAR(255) OUTPUT,
    @CreatedDate DATETIME2 OUTPUT
AS
BEGIN
    SELECT
        @FirstName = FirstName,
        @LastName = LastName,
        @Email = Email,
        @CreatedDate = CreatedDate
    FROM Users
    WHERE UserId = @UserId
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with multiple OUTPUT parameters: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify all OUTPUT parameters are in the preserved SQL
    let sql_text = &statements[0].sql_text;
    let output_count = sql_text.matches("OUTPUT").count();
    assert_eq!(
        output_count, 4,
        "Should have 4 OUTPUT keywords in the preserved SQL"
    );
}

#[test]
fn test_parse_procedure_with_output_and_default_value() {
    let sql = r#"
CREATE PROCEDURE [dbo].[ProcessWithDefaults]
    @InputValue INT,
    @Multiplier INT = 2,
    @Result INT OUTPUT,
    @Status VARCHAR(20) = 'Pending' OUTPUT
AS
BEGIN
    SET @Result = @InputValue * @Multiplier
    SET @Status = 'Complete'
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with OUTPUT and default value: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify defaults and OUTPUT are preserved
    let sql_text = &statements[0].sql_text;
    assert!(sql_text.contains("= 2"), "Should preserve default value = 2");
    assert!(
        sql_text.contains("'Pending'"),
        "Should preserve default value 'Pending'"
    );
}

#[test]
fn test_parse_procedure_mixed_input_and_output_parameters() {
    let sql = r#"
CREATE PROCEDURE [sales].[CalculateOrderTotal]
    @OrderId INT,
    @ApplyDiscount BIT = 0,
    @DiscountPercent DECIMAL(5,2) = 0.00,
    @Subtotal MONEY OUTPUT,
    @Tax MONEY OUTPUT,
    @Total MONEY OUTPUT
AS
BEGIN
    DECLARE @TaxRate DECIMAL(5,2) = 0.08

    SELECT @Subtotal = SUM(Quantity * UnitPrice)
    FROM OrderItems
    WHERE OrderId = @OrderId

    IF @ApplyDiscount = 1
        SET @Subtotal = @Subtotal * (1 - @DiscountPercent / 100)

    SET @Tax = @Subtotal * @TaxRate
    SET @Total = @Subtotal + @Tax
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with mixed input/output parameters: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    match &statements[0].fallback_type {
        Some(rust_sqlpackage::parser::FallbackStatementType::Procedure { schema, name }) => {
            assert_eq!(schema, "sales");
            assert_eq!(name, "CalculateOrderTotal");
        }
        _ => panic!("Expected Procedure fallback type"),
    }
}

#[test]
fn test_parse_procedure_output_readonly_table_type() {
    // Table-valued parameters are READONLY, not OUTPUT
    // But a procedure can have both TVP and OUTPUT params
    let sql = r#"
CREATE PROCEDURE [dbo].[BulkInsertWithCount]
    @Items dbo.ItemTableType READONLY,
    @InsertedCount INT OUTPUT
AS
BEGIN
    INSERT INTO Items (Name, Value)
    SELECT Name, Value FROM @Items

    SET @InsertedCount = @@ROWCOUNT
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with READONLY and OUTPUT parameters: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    let sql_text = &statements[0].sql_text;
    assert!(
        sql_text.contains("READONLY"),
        "Should preserve READONLY keyword"
    );
    assert!(
        sql_text.contains("OUTPUT"),
        "Should preserve OUTPUT keyword"
    );
}

#[test]
fn test_parse_procedure_cursor_output() {
    // CURSOR OUTPUT is a special case in SQL Server
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUsersCursor]
    @ActiveOnly BIT = 1,
    @UserCursor CURSOR VARYING OUTPUT
AS
BEGIN
    SET @UserCursor = CURSOR FORWARD_ONLY STATIC FOR
        SELECT UserId, UserName FROM Users
        WHERE @ActiveOnly = 0 OR IsActive = 1

    OPEN @UserCursor
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with CURSOR OUTPUT: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("CURSOR VARYING OUTPUT"),
        "Should preserve CURSOR VARYING OUTPUT"
    );
}

// ============================================================================
// JSON in Stored Procedure Tests
// ============================================================================

#[test]
fn test_parse_json_in_stored_procedure() {
    let sql = r#"
CREATE PROCEDURE [dbo].[usp_GetJsonData]
    @Id INT
AS
BEGIN
    SELECT
        [Id],
        JSON_VALUE([Data], '$.name') AS [Name],
        JSON_QUERY([Data], '$.items') AS [Items]
    FROM [dbo].[Documents]
    WHERE [Id] = @Id
    FOR JSON PATH, WITHOUT_ARRAY_WRAPPER;
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse stored procedure with JSON functions: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Should contain JSON function references in the SQL text
    let sql_text = &statements[0].sql_text;
    assert!(
        sql_text.contains("JSON_VALUE") || sql_text.contains("JSON_QUERY"),
        "Stored procedure should contain JSON functions"
    );
}

// ============================================================================
// Temporal Table Query Procedures
// ============================================================================

#[test]
fn test_parse_temporal_table_query_as_of() {
    // View with AS OF temporal query
    let sql = r#"
CREATE PROCEDURE [dbo].[GetEmployeeAsOf]
    @AsOfDate DATETIME2
AS
BEGIN
    SELECT
        [EmployeeId],
        [Name],
        [Department],
        [Salary]
    FROM [dbo].[Employee]
    FOR SYSTEM_TIME AS OF @AsOfDate;
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with FOR SYSTEM_TIME AS OF: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_temporal_table_query_between() {
    // Procedure with BETWEEN temporal query
    let sql = r#"
CREATE PROCEDURE [dbo].[GetEmployeeHistoryBetween]
    @StartDate DATETIME2,
    @EndDate DATETIME2
AS
BEGIN
    SELECT
        [EmployeeId],
        [Name],
        [Department],
        [Salary],
        [SysStartTime],
        [SysEndTime]
    FROM [dbo].[Employee]
    FOR SYSTEM_TIME BETWEEN @StartDate AND @EndDate
    ORDER BY [SysStartTime];
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with FOR SYSTEM_TIME BETWEEN: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_temporal_table_query_from_to() {
    // Procedure with FROM...TO temporal query
    let sql = r#"
CREATE PROCEDURE [dbo].[GetEmployeeHistoryFromTo]
    @StartDate DATETIME2,
    @EndDate DATETIME2
AS
BEGIN
    SELECT
        [EmployeeId],
        [Name],
        [Department],
        [Salary],
        [SysStartTime],
        [SysEndTime]
    FROM [dbo].[Employee]
    FOR SYSTEM_TIME FROM @StartDate TO @EndDate
    ORDER BY [EmployeeId], [SysStartTime];
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with FOR SYSTEM_TIME FROM...TO: {:?}",
        result.err()
    );
}
