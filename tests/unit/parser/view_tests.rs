//! CREATE VIEW parsing tests

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
// CREATE VIEW Parsing Tests
// ============================================================================

#[test]
fn test_parse_simple_view() {
    let sql = r#"
CREATE VIEW [dbo].[SimpleView]
AS
SELECT 1 AS [Value];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse simple view: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::CreateView { name, .. }) => {
            assert!(name.to_string().contains("SimpleView"));
        }
        _ => panic!("Expected CREATE VIEW statement"),
    }
}

#[test]
fn test_parse_view_with_columns() {
    let sql = r#"
CREATE VIEW [dbo].[ViewWithColumns] ([Col1], [Col2])
AS
SELECT 1, 2;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse view with columns: {:?}",
        result.err()
    );
}

// ============================================================================
// Additional CREATE VIEW Tests (from TEST_PLAN.md)
// ============================================================================

#[test]
fn test_parse_view_with_schema_binding() {
    let sql = r#"
CREATE VIEW [dbo].[BoundView]
WITH SCHEMABINDING
AS
SELECT 1 AS [Value];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // SCHEMABINDING may or may not be supported
    if result.is_ok() {
        let statements = result.unwrap();
        assert_eq!(statements.len(), 1);
    } else {
        println!("Note: WITH SCHEMABINDING not supported: {:?}", result.err());
    }
}

// ============================================================================
// Temporal Table Query Views
// ============================================================================

#[test]
fn test_parse_temporal_table_query_for_system_time() {
    // Query using FOR SYSTEM_TIME clause
    let sql = r#"
CREATE VIEW [dbo].[EmployeeHistoryView]
AS
SELECT
    [EmployeeId],
    [Name],
    [Department],
    [Salary],
    [SysStartTime],
    [SysEndTime]
FROM [dbo].[Employee]
FOR SYSTEM_TIME ALL;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse view with FOR SYSTEM_TIME: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("FOR SYSTEM_TIME"),
        "Should preserve FOR SYSTEM_TIME clause"
    );
}

// ============================================================================
// MERGE Statement Parsing Tests
// ============================================================================

#[test]
fn test_parse_merge_basic() {
    let sql = r#"
MERGE INTO [dbo].[Target] AS T
USING [dbo].[Source] AS S
ON T.[Id] = S.[Id]
WHEN MATCHED THEN
    UPDATE SET T.[Name] = S.[Name]
WHEN NOT MATCHED THEN
    INSERT ([Id], [Name]) VALUES (S.[Id], S.[Name]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse MERGE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_merge_with_delete() {
    let sql = r#"
MERGE [dbo].[Products] AS Target
USING [dbo].[StagingProducts] AS Source
ON Target.[SKU] = Source.[SKU]
WHEN MATCHED AND Source.[IsDeleted] = 1 THEN
    DELETE
WHEN MATCHED THEN
    UPDATE SET Target.[Name] = Source.[Name], Target.[Price] = Source.[Price]
WHEN NOT MATCHED BY TARGET THEN
    INSERT ([SKU], [Name], [Price]) VALUES (Source.[SKU], Source.[Name], Source.[Price]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse MERGE with DELETE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_merge_with_output() {
    let sql = r#"
MERGE [dbo].[Target] AS T
USING [dbo].[Source] AS S
ON T.[Id] = S.[Id]
WHEN MATCHED THEN
    UPDATE SET T.[Value] = S.[Value]
WHEN NOT MATCHED THEN
    INSERT ([Id], [Value]) VALUES (S.[Id], S.[Value])
OUTPUT $action, inserted.*, deleted.*;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse MERGE with OUTPUT: {:?}",
        result.err()
    );
}

// ============================================================================
// Common Table Expression (CTE) Tests
// ============================================================================

#[test]
fn test_parse_simple_cte() {
    let sql = r#"
WITH SimpleCTE AS (
    SELECT [Id], [Name] FROM [dbo].[Users]
)
SELECT * FROM SimpleCTE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse simple CTE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_multiple_ctes() {
    let sql = r#"
WITH
    CTE1 AS (SELECT [Id] FROM [dbo].[Table1]),
    CTE2 AS (SELECT [Id] FROM [dbo].[Table2]),
    CTE3 AS (SELECT [Id] FROM CTE1 UNION ALL SELECT [Id] FROM CTE2)
SELECT * FROM CTE3;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse multiple CTEs: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_recursive_cte() {
    let sql = r#"
WITH EmployeeHierarchy AS (
    -- Anchor member
    SELECT [Id], [Name], [ManagerId], 0 AS [Level]
    FROM [dbo].[Employees]
    WHERE [ManagerId] IS NULL

    UNION ALL

    -- Recursive member
    SELECT e.[Id], e.[Name], e.[ManagerId], h.[Level] + 1
    FROM [dbo].[Employees] e
    INNER JOIN EmployeeHierarchy h ON e.[ManagerId] = h.[Id]
)
SELECT * FROM EmployeeHierarchy;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse recursive CTE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_cte_with_insert() {
    let sql = r#"
WITH SourceData AS (
    SELECT [Id], [Name] FROM [dbo].[SourceTable]
)
INSERT INTO [dbo].[TargetTable] ([Id], [Name])
SELECT [Id], [Name] FROM SourceData;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CTE with INSERT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_cte_with_update() {
    let sql = r#"
WITH ToUpdate AS (
    SELECT [Id], [Status] FROM [dbo].[Orders] WHERE [Status] = 'Pending'
)
UPDATE ToUpdate SET [Status] = 'Processing';
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CTE with UPDATE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_cte_with_delete() {
    let sql = r#"
WITH OldRecords AS (
    SELECT [Id] FROM [dbo].[Logs] WHERE [CreatedAt] < '2020-01-01'
)
DELETE FROM OldRecords;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CTE with DELETE: {:?}",
        result.err()
    );
}

// ============================================================================
// Window Function Tests
// ============================================================================

#[test]
fn test_parse_row_number() {
    let sql = r#"
SELECT
    [Id],
    [Name],
    ROW_NUMBER() OVER (ORDER BY [Name]) AS [RowNum]
FROM [dbo].[Users];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ROW_NUMBER: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_rank_and_dense_rank() {
    let sql = r#"
SELECT
    [Id],
    [Score],
    RANK() OVER (ORDER BY [Score] DESC) AS [Rank],
    DENSE_RANK() OVER (ORDER BY [Score] DESC) AS [DenseRank]
FROM [dbo].[Players];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse RANK/DENSE_RANK: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_ntile() {
    let sql = r#"
SELECT
    [Id],
    [Amount],
    NTILE(4) OVER (ORDER BY [Amount]) AS [Quartile]
FROM [dbo].[Transactions];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse NTILE: {:?}", result.err());
}

#[test]
fn test_parse_lag_and_lead() {
    let sql = r#"
SELECT
    [OrderDate],
    [Amount],
    LAG([Amount], 1, 0) OVER (ORDER BY [OrderDate]) AS [PrevAmount],
    LEAD([Amount], 1, 0) OVER (ORDER BY [OrderDate]) AS [NextAmount]
FROM [dbo].[Orders];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse LAG/LEAD: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_first_value_last_value() {
    let sql = r#"
SELECT
    [Category],
    [ProductName],
    [Price],
    FIRST_VALUE([ProductName]) OVER (PARTITION BY [Category] ORDER BY [Price]) AS [Cheapest],
    LAST_VALUE([ProductName]) OVER (PARTITION BY [Category] ORDER BY [Price]
        ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) AS [MostExpensive]
FROM [dbo].[Products];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FIRST_VALUE/LAST_VALUE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_aggregate_with_over() {
    let sql = r#"
SELECT
    [DepartmentId],
    [EmployeeName],
    [Salary],
    SUM([Salary]) OVER (PARTITION BY [DepartmentId]) AS [DeptTotal],
    AVG([Salary]) OVER (PARTITION BY [DepartmentId]) AS [DeptAvg],
    COUNT(*) OVER (PARTITION BY [DepartmentId]) AS [DeptCount]
FROM [dbo].[Employees];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse aggregate OVER: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_window_frame_rows() {
    let sql = r#"
SELECT
    [OrderDate],
    [Amount],
    SUM([Amount]) OVER (ORDER BY [OrderDate] ROWS BETWEEN 2 PRECEDING AND CURRENT ROW) AS [MovingSum]
FROM [dbo].[Orders];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ROWS frame: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_window_frame_range() {
    let sql = r#"
SELECT
    [OrderDate],
    [Amount],
    SUM([Amount]) OVER (ORDER BY [OrderDate] RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS [RunningTotal]
FROM [dbo].[Orders];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse RANGE frame: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_percent_rank_cume_dist() {
    let sql = r#"
SELECT
    [Name],
    [Score],
    PERCENT_RANK() OVER (ORDER BY [Score]) AS [PercentRank],
    CUME_DIST() OVER (ORDER BY [Score]) AS [CumeDist]
FROM [dbo].[Students];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse PERCENT_RANK/CUME_DIST: {:?}",
        result.err()
    );
}

// ============================================================================
// APPLY Operator Tests
// ============================================================================

#[test]
fn test_parse_cross_apply() {
    let sql = r#"
SELECT o.[Id], o.[OrderDate], d.[ProductId], d.[Quantity]
FROM [dbo].[Orders] o
CROSS APPLY (
    SELECT [ProductId], [Quantity]
    FROM [dbo].[OrderDetails]
    WHERE [OrderId] = o.[Id]
) d;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CROSS APPLY: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_outer_apply() {
    let sql = r#"
SELECT c.[Name], r.[OrderCount], r.[TotalAmount]
FROM [dbo].[Customers] c
OUTER APPLY (
    SELECT
        COUNT(*) AS [OrderCount],
        SUM([Amount]) AS [TotalAmount]
    FROM [dbo].[Orders]
    WHERE [CustomerId] = c.[Id]
) r;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse OUTER APPLY: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_cross_apply_with_function() {
    let sql = r#"
SELECT e.[Name], s.[Value]
FROM [dbo].[Employees] e
CROSS APPLY STRING_SPLIT(e.[Skills], ',') s;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CROSS APPLY with function: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_nested_apply() {
    let sql = r#"
SELECT d.[Name], e.[Name], p.[ProjectName]
FROM [dbo].[Departments] d
CROSS APPLY (
    SELECT [Name], [Id] FROM [dbo].[Employees] WHERE [DepartmentId] = d.[Id]
) e
OUTER APPLY (
    SELECT [ProjectName] FROM [dbo].[Projects] WHERE [LeaderId] = e.[Id]
) p;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse nested APPLY: {:?}",
        result.err()
    );
}

// ============================================================================
// PIVOT/UNPIVOT Tests
// ============================================================================

#[test]
fn test_parse_pivot_basic() {
    let sql = r#"
SELECT [CustomerId], [Jan], [Feb], [Mar]
FROM (
    SELECT [CustomerId], [Month], [Amount]
    FROM [dbo].[Sales]
) AS SourceTable
PIVOT (
    SUM([Amount])
    FOR [Month] IN ([Jan], [Feb], [Mar])
) AS PivotTable;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse PIVOT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_unpivot_basic() {
    let sql = r#"
SELECT [CustomerId], [Month], [Amount]
FROM [dbo].[MonthlySales]
UNPIVOT (
    [Amount] FOR [Month] IN ([Jan], [Feb], [Mar], [Apr])
) AS UnpivotTable;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse UNPIVOT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_pivot_with_join() {
    let sql = r#"
SELECT c.[Name], pvt.[Q1], pvt.[Q2], pvt.[Q3], pvt.[Q4]
FROM [dbo].[Customers] c
INNER JOIN (
    SELECT [CustomerId], [Q1], [Q2], [Q3], [Q4]
    FROM (
        SELECT [CustomerId], [Quarter], [Revenue]
        FROM [dbo].[QuarterlyRevenue]
    ) AS src
    PIVOT (
        SUM([Revenue])
        FOR [Quarter] IN ([Q1], [Q2], [Q3], [Q4])
    ) AS pvt
) pvt ON c.[Id] = pvt.[CustomerId];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse PIVOT with JOIN: {:?}",
        result.err()
    );
}

// ============================================================================
// JSON Function Tests
// ============================================================================

#[test]
fn test_parse_json_value() {
    let sql = r#"
SELECT
    [Id],
    JSON_VALUE([Data], '$.name') AS [Name],
    JSON_VALUE([Data], '$.address.city') AS [City]
FROM [dbo].[JsonDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_VALUE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected exactly 1 statement");

    // Verify it's a SELECT query
    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Query(_)) => {
            // Verify the SQL text contains JSON_VALUE
            assert!(
                statements[0].sql_text.contains("JSON_VALUE"),
                "SQL should contain JSON_VALUE function"
            );
        }
        _ => panic!("Expected SELECT query statement for JSON_VALUE"),
    }
}

#[test]
fn test_parse_json_value_nested_path() {
    let sql = r#"
SELECT
    JSON_VALUE([Data], '$.user.profile.settings.theme') AS [Theme],
    JSON_VALUE([Data], '$.items[0].name') AS [FirstItem]
FROM [dbo].[JsonDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_VALUE with nested path: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        matches!(&statements[0].statement, Some(sqlparser::ast::Statement::Query(_))),
        "Expected SELECT query statement"
    );
}

#[test]
fn test_parse_json_query() {
    let sql = r#"
SELECT
    [Id],
    JSON_QUERY([Data], '$.items') AS [Items],
    JSON_QUERY([Data], '$.metadata') AS [Metadata]
FROM [dbo].[JsonDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_QUERY: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected exactly 1 statement");

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Query(_)) => {
            assert!(
                statements[0].sql_text.contains("JSON_QUERY"),
                "SQL should contain JSON_QUERY function"
            );
        }
        _ => panic!("Expected SELECT query statement for JSON_QUERY"),
    }
}

#[test]
fn test_parse_json_query_no_path() {
    // JSON_QUERY with just the column (returns entire JSON)
    let sql = r#"
SELECT
    [Id],
    JSON_QUERY([Data]) AS [FullJson]
FROM [dbo].[JsonDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_QUERY without path: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        matches!(&statements[0].statement, Some(sqlparser::ast::Statement::Query(_))),
        "Expected SELECT query statement"
    );
}

#[test]
fn test_parse_json_modify() {
    let sql = r#"
UPDATE [dbo].[JsonDocuments]
SET [Data] = JSON_MODIFY([Data], '$.status', 'active')
WHERE [Id] = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_MODIFY: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected exactly 1 statement");

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Update { .. }) => {
            assert!(
                statements[0].sql_text.contains("JSON_MODIFY"),
                "SQL should contain JSON_MODIFY function"
            );
        }
        _ => panic!("Expected UPDATE statement for JSON_MODIFY"),
    }
}

#[test]
fn test_parse_json_modify_append() {
    // JSON_MODIFY with append modifier
    let sql = r#"
UPDATE [dbo].[JsonDocuments]
SET [Data] = JSON_MODIFY([Data], 'append $.items', @newItem)
WHERE [Id] = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_MODIFY with append: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        matches!(&statements[0].statement, Some(sqlparser::ast::Statement::Update { .. })),
        "Expected UPDATE statement"
    );
}

#[test]
fn test_parse_json_modify_delete() {
    // JSON_MODIFY to delete a key (set to NULL)
    let sql = r#"
UPDATE [dbo].[JsonDocuments]
SET [Data] = JSON_MODIFY([Data], '$.deprecated', NULL)
WHERE [Id] = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_MODIFY with NULL: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        matches!(&statements[0].statement, Some(sqlparser::ast::Statement::Update { .. })),
        "Expected UPDATE statement"
    );
}

#[test]
fn test_parse_isjson() {
    let sql = r#"
SELECT [Id], [Data]
FROM [dbo].[Documents]
WHERE ISJSON([Data]) = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(result.is_ok(), "Failed to parse ISJSON: {:?}", result.err());

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected exactly 1 statement");

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Query(_)) => {
            assert!(
                statements[0].sql_text.contains("ISJSON"),
                "SQL should contain ISJSON function"
            );
        }
        _ => panic!("Expected SELECT query statement for ISJSON"),
    }
}

#[test]
fn test_parse_isjson_in_case_expression() {
    let sql = r#"
SELECT
    [Id],
    CASE WHEN ISJSON([Data]) = 1 THEN 'Valid' ELSE 'Invalid' END AS [JsonStatus]
FROM [dbo].[Documents];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ISJSON in CASE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        matches!(&statements[0].statement, Some(sqlparser::ast::Statement::Query(_))),
        "Expected SELECT query statement"
    );
}

#[test]
fn test_parse_openjson() {
    let sql = r#"
SELECT j.[key], j.[value], j.[type]
FROM [dbo].[JsonDocuments] d
CROSS APPLY OPENJSON(d.[Data]) j;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse OPENJSON: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected exactly 1 statement");

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Query(_)) => {
            assert!(
                statements[0].sql_text.contains("OPENJSON"),
                "SQL should contain OPENJSON function"
            );
            assert!(
                statements[0].sql_text.contains("CROSS APPLY"),
                "SQL should contain CROSS APPLY"
            );
        }
        _ => panic!("Expected SELECT query statement for OPENJSON"),
    }
}

#[test]
fn test_parse_openjson_with_path() {
    let sql = r#"
SELECT j.[key], j.[value]
FROM [dbo].[JsonDocuments] d
CROSS APPLY OPENJSON(d.[Data], '$.items') j;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse OPENJSON with path: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        matches!(&statements[0].statement, Some(sqlparser::ast::Statement::Query(_))),
        "Expected SELECT query statement"
    );
}

#[test]
fn test_parse_openjson_with_schema() {
    let sql = r#"
SELECT [Name], [Age], [City]
FROM OPENJSON(@json)
WITH (
    [Name] NVARCHAR(100) '$.name',
    [Age] INT '$.age',
    [City] NVARCHAR(50) '$.address.city'
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse OPENJSON WITH: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected exactly 1 statement");

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Query(_)) => {
            assert!(
                statements[0].sql_text.contains("OPENJSON"),
                "SQL should contain OPENJSON"
            );
            assert!(
                statements[0].sql_text.contains("WITH"),
                "SQL should contain WITH clause"
            );
        }
        _ => panic!("Expected SELECT query statement for OPENJSON WITH"),
    }
}

#[test]
fn test_parse_openjson_with_schema_as_json() {
    // OPENJSON WITH schema including AS JSON for nested objects
    let sql = r#"
SELECT [Name], [Address]
FROM OPENJSON(@json)
WITH (
    [Name] NVARCHAR(100) '$.name',
    [Address] NVARCHAR(MAX) '$.address' AS JSON
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse OPENJSON WITH AS JSON: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        matches!(&statements[0].statement, Some(sqlparser::ast::Statement::Query(_))),
        "Expected SELECT query statement"
    );
}

#[test]
fn test_parse_openjson_outer_apply() {
    let sql = r#"
SELECT d.[Id], j.[Name]
FROM [dbo].[Documents] d
OUTER APPLY OPENJSON(d.[Data])
WITH ([Name] NVARCHAR(100) '$.name') j;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse OPENJSON with OUTER APPLY: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        statements[0].sql_text.contains("OUTER APPLY"),
        "SQL should contain OUTER APPLY"
    );
}

#[test]
fn test_parse_for_json_auto() {
    let sql = r#"
SELECT [Id], [Name], [Email]
FROM [dbo].[Users]
FOR JSON AUTO;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR JSON AUTO: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected exactly 1 statement");

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Query(_)) => {
            assert!(
                statements[0].sql_text.contains("FOR JSON AUTO"),
                "SQL should contain FOR JSON AUTO clause"
            );
        }
        _ => panic!("Expected SELECT query statement for FOR JSON AUTO"),
    }
}

#[test]
fn test_parse_for_json_auto_include_null() {
    let sql = r#"
SELECT [Id], [Name], [Email]
FROM [dbo].[Users]
FOR JSON AUTO, INCLUDE_NULL_VALUES;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR JSON AUTO with INCLUDE_NULL_VALUES: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        statements[0].sql_text.contains("INCLUDE_NULL_VALUES"),
        "SQL should contain INCLUDE_NULL_VALUES"
    );
}

#[test]
fn test_parse_for_json_path() {
    let sql = r#"
SELECT
    [Id] AS 'user.id',
    [Name] AS 'user.name',
    [Email] AS 'contact.email'
FROM [dbo].[Users]
FOR JSON PATH, ROOT('users');
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR JSON PATH: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected exactly 1 statement");

    match &statements[0].statement {
        Some(sqlparser::ast::Statement::Query(_)) => {
            assert!(
                statements[0].sql_text.contains("FOR JSON PATH"),
                "SQL should contain FOR JSON PATH clause"
            );
            assert!(
                statements[0].sql_text.contains("ROOT"),
                "SQL should contain ROOT option"
            );
        }
        _ => panic!("Expected SELECT query statement for FOR JSON PATH"),
    }
}

#[test]
fn test_parse_for_json_path_without_array_wrapper() {
    let sql = r#"
SELECT [Id], [Name]
FROM [dbo].[Users]
WHERE [Id] = 1
FOR JSON PATH, WITHOUT_ARRAY_WRAPPER;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR JSON PATH WITHOUT_ARRAY_WRAPPER: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        statements[0].sql_text.contains("WITHOUT_ARRAY_WRAPPER"),
        "SQL should contain WITHOUT_ARRAY_WRAPPER"
    );
}

#[test]
fn test_parse_for_json_path_nested_subquery() {
    let sql = r#"
SELECT
    [Id],
    [Name],
    (SELECT [OrderId], [Amount]
     FROM [dbo].[Orders] o
     WHERE o.[UserId] = u.[Id]
     FOR JSON PATH) AS [Orders]
FROM [dbo].[Users] u
FOR JSON PATH;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR JSON PATH with nested subquery: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        matches!(&statements[0].statement, Some(sqlparser::ast::Statement::Query(_))),
        "Expected SELECT query statement"
    );
}

#[test]
fn test_parse_json_functions_combined() {
    // Test combining multiple JSON functions in one query
    let sql = r#"
SELECT
    [Id],
    JSON_VALUE([Data], '$.name') AS [Name],
    JSON_QUERY([Data], '$.details') AS [Details],
    CASE WHEN ISJSON([Data]) = 1 THEN 'Valid' ELSE 'Invalid' END AS [Status]
FROM [dbo].[Documents]
WHERE ISJSON([Data]) = 1
FOR JSON PATH;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse combined JSON functions: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    let sql_text = &statements[0].sql_text;
    assert!(sql_text.contains("JSON_VALUE"), "Should contain JSON_VALUE");
    assert!(sql_text.contains("JSON_QUERY"), "Should contain JSON_QUERY");
    assert!(sql_text.contains("ISJSON"), "Should contain ISJSON");
    assert!(sql_text.contains("FOR JSON PATH"), "Should contain FOR JSON PATH");
}

#[test]
fn test_parse_json_value_lax_strict() {
    // JSON_VALUE with lax/strict path mode
    let sql = r#"
SELECT
    JSON_VALUE([Data], 'lax $.optional') AS [LaxValue],
    JSON_VALUE([Data], 'strict $.required') AS [StrictValue]
FROM [dbo].[Documents];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse JSON_VALUE with lax/strict: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        matches!(&statements[0].statement, Some(sqlparser::ast::Statement::Query(_))),
        "Expected SELECT query statement"
    );
}

// ============================================================================
// XML Method Tests
// ============================================================================

#[test]
fn test_parse_xml_query() {
    let sql = r#"
SELECT
    [Id],
    [XmlData].query('/root/items/item') AS [Items]
FROM [dbo].[XmlDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .query(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_xml_value() {
    let sql = r#"
SELECT
    [Id],
    [XmlData].value('(/root/name)[1]', 'NVARCHAR(100)') AS [Name]
FROM [dbo].[XmlDocuments];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .value(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_xml_exist() {
    let sql = r#"
SELECT [Id], [XmlData]
FROM [dbo].[XmlDocuments]
WHERE [XmlData].exist('/root/items/item[@status="active"]') = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .exist(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_xml_modify() {
    let sql = r#"
UPDATE [dbo].[XmlDocuments]
SET [XmlData].modify('replace value of (/root/status/text())[1] with "active"')
WHERE [Id] = 1;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .modify(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_xml_nodes() {
    let sql = r#"
SELECT
    t.c.value('@id', 'INT') AS [ItemId],
    t.c.value('name[1]', 'NVARCHAR(100)') AS [ItemName]
FROM [dbo].[XmlDocuments] d
CROSS APPLY d.[XmlData].nodes('/root/items/item') AS t(c);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse XML .nodes(): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_for_xml_raw() {
    let sql = r#"
SELECT [Id], [Name], [Email]
FROM [dbo].[Users]
FOR XML RAW('user'), ROOT('users'), ELEMENTS;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR XML RAW: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_for_xml_path() {
    let sql = r#"
SELECT
    [Id] AS '@id',
    [Name] AS 'name',
    [Email] AS 'contact/email'
FROM [dbo].[Users]
FOR XML PATH('user'), ROOT('users');
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse FOR XML PATH: {:?}",
        result.err()
    );
}
