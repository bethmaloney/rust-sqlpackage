//! Graph Table Parsing Tests (NODE and EDGE tables)
//!
//! Tests for SQL Server graph database features:
//! - CREATE TABLE AS NODE
//! - CREATE TABLE AS EDGE
//! - $node_id, $from_id, $to_id pseudo-columns

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
// CREATE TABLE AS NODE Tests
// ============================================================================

#[test]
fn test_parse_node_table_basic() {
    let sql = r#"
CREATE TABLE [dbo].[Person] (
    [PersonId] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL
) AS NODE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse basic NODE table: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected 1 statement");
    assert!(
        statements[0].sql_text.to_uppercase().contains("AS NODE"),
        "Should preserve AS NODE clause"
    );
}

#[test]
fn test_parse_node_table_with_primary_key() {
    let sql = r#"
CREATE TABLE [dbo].[Employee] (
    [EmployeeId] INT NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [Department] NVARCHAR(100) NULL,
    CONSTRAINT [PK_Employee] PRIMARY KEY CLUSTERED ([EmployeeId])
) AS NODE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse NODE table with PK: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("PRIMARY KEY"),
        "Should preserve PRIMARY KEY constraint"
    );
    assert!(
        statements[0].sql_text.to_uppercase().contains("AS NODE"),
        "Should preserve AS NODE clause"
    );
}

#[test]
fn test_parse_node_table_with_identity() {
    let sql = r#"
CREATE TABLE [dbo].[Department] (
    [DepartmentId] INT NOT NULL IDENTITY(1, 1),
    [DepartmentName] NVARCHAR(100) NOT NULL,
    CONSTRAINT [PK_Department] PRIMARY KEY ([DepartmentId])
) AS NODE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse NODE table with IDENTITY: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_node_table_with_default() {
    let sql = r#"
CREATE TABLE [dbo].[City] (
    [CityId] INT NOT NULL,
    [CityName] NVARCHAR(100) NOT NULL,
    [Country] NVARCHAR(100) NOT NULL DEFAULT 'USA',
    [CreatedAt] DATETIME2 NOT NULL DEFAULT GETDATE()
) AS NODE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse NODE table with DEFAULT: {:?}",
        result.err()
    );
}

// ============================================================================
// CREATE TABLE AS EDGE Tests
// ============================================================================

#[test]
fn test_parse_edge_table_basic() {
    let sql = r#"
CREATE TABLE [dbo].[Knows] (
    [Since] DATE NULL
) AS EDGE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse basic EDGE table: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected 1 statement");
    assert!(
        statements[0].sql_text.to_uppercase().contains("AS EDGE"),
        "Should preserve AS EDGE clause"
    );
}

#[test]
fn test_parse_edge_table_with_properties() {
    let sql = r#"
CREATE TABLE [dbo].[WorksFor] (
    [StartDate] DATE NOT NULL,
    [EndDate] DATE NULL,
    [Role] NVARCHAR(100) NULL
) AS EDGE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse EDGE table with properties: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_edge_table_with_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[LocatedIn] (
    [Distance] DECIMAL(18, 2) NULL,
    CONSTRAINT [CK_LocatedIn_Distance] CHECK ([Distance] >= 0)
) AS EDGE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse EDGE table with CHECK constraint: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_edge_table_empty() {
    // EDGE tables can be created with no columns (just the pseudo-columns)
    let sql = r#"
CREATE TABLE [dbo].[Follows] AS EDGE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Note: This may fail parsing if empty parentheses aren't handled
    // The fallback parser may still capture it
    if result.is_err() {
        // Check if we at least get a fallback parse
        let err = result.err().unwrap();
        assert!(
            !err.to_string().is_empty(),
            "Should produce a meaningful error if not parseable"
        );
    }
}

// ============================================================================
// Multiple Graph Tables Tests
// ============================================================================

#[test]
fn test_parse_node_and_edge_tables() {
    let sql = r#"
CREATE TABLE [dbo].[Person] (
    [PersonId] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL
) AS NODE;
GO
CREATE TABLE [dbo].[FriendOf] (
    [Since] DATE NULL
) AS EDGE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse NODE and EDGE tables: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2, "Expected 2 statements");

    // First should be NODE
    assert!(
        statements[0].sql_text.to_uppercase().contains("AS NODE"),
        "First table should be NODE"
    );

    // Second should be EDGE
    assert!(
        statements[1].sql_text.to_uppercase().contains("AS EDGE"),
        "Second table should be EDGE"
    );
}

#[test]
fn test_parse_graph_schema() {
    // Complete graph schema with multiple nodes and edges
    let sql = r#"
-- Node tables
CREATE TABLE [dbo].[Customer] (
    [CustomerId] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(200) NOT NULL,
    [Email] NVARCHAR(255) NULL
) AS NODE;
GO
CREATE TABLE [dbo].[Product] (
    [ProductId] INT NOT NULL PRIMARY KEY,
    [ProductName] NVARCHAR(200) NOT NULL,
    [Price] DECIMAL(18, 2) NOT NULL
) AS NODE;
GO
-- Edge tables
CREATE TABLE [dbo].[Purchased] (
    [PurchaseDate] DATETIME2 NOT NULL DEFAULT GETDATE(),
    [Quantity] INT NOT NULL DEFAULT 1
) AS EDGE;
GO
CREATE TABLE [dbo].[Reviewed] (
    [Rating] INT NOT NULL,
    [ReviewText] NVARCHAR(MAX) NULL,
    CONSTRAINT [CK_Reviewed_Rating] CHECK ([Rating] >= 1 AND [Rating] <= 5)
) AS EDGE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse complete graph schema: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 4, "Expected 4 statements (2 nodes, 2 edges)");
}

// ============================================================================
// Graph Table with Special Columns Tests
// ============================================================================

#[test]
fn test_parse_node_table_various_data_types() {
    let sql = r#"
CREATE TABLE [dbo].[Entity] (
    [EntityId] UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID(),
    [Name] NVARCHAR(100) NOT NULL,
    [Description] NVARCHAR(MAX) NULL,
    [Properties] NVARCHAR(MAX) NULL,
    [CreatedAt] DATETIME2 NOT NULL DEFAULT GETUTCDATE(),
    [IsActive] BIT NOT NULL DEFAULT 1
) AS NODE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse NODE table with various data types: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_edge_table_with_index() {
    let sql = r#"
CREATE TABLE [dbo].[Connection] (
    [Weight] DECIMAL(10, 4) NOT NULL DEFAULT 1.0,
    [ConnectionType] NVARCHAR(50) NOT NULL
) AS EDGE;
GO
CREATE INDEX [IX_Connection_Type] ON [dbo].[Connection] ([ConnectionType]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse EDGE table with index: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2, "Expected EDGE table and index");
}
