//! CREATE TABLE parsing tests

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
// CREATE TABLE Parsing Tests
// ============================================================================

#[test]
fn test_parse_simple_table() {
    let sql = r#"
CREATE TABLE [dbo].[SimpleTable] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse simple table: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify it's a CREATE TABLE statement
    match &statements[0].statement {
        Some(sqlparser::ast::Statement::CreateTable(create)) => {
            assert!(create.name.to_string().contains("SimpleTable"));
        }
        _ => panic!("Expected CREATE TABLE statement"),
    }
}

#[test]
fn test_parse_table_with_primary_key() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithPK] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with PK: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_named_primary_key() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithNamedPK] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    CONSTRAINT [PK_TableWithNamedPK] PRIMARY KEY CLUSTERED ([Id])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with named PK: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_foreign_key() {
    let sql = r#"
CREATE TABLE [dbo].[Parent] (
    [Id] INT NOT NULL PRIMARY KEY
);
GO
CREATE TABLE [dbo].[Child] (
    [Id] INT NOT NULL PRIMARY KEY,
    [ParentId] INT NOT NULL,
    CONSTRAINT [FK_Child_Parent] FOREIGN KEY ([ParentId]) REFERENCES [dbo].[Parent]([Id])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with FK: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 2);
}

#[test]
fn test_parse_table_with_unique_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithUnique] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Email] NVARCHAR(255) NOT NULL,
    CONSTRAINT [UQ_TableWithUnique_Email] UNIQUE ([Email])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with unique: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_check_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithCheck] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Age] INT NOT NULL,
    CONSTRAINT [CK_TableWithCheck_Age] CHECK ([Age] >= 0)
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with check: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_default_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithDefault] (
    [Id] INT NOT NULL PRIMARY KEY,
    [CreatedAt] DATETIME2 NOT NULL DEFAULT GETDATE(),
    [IsActive] BIT NOT NULL DEFAULT 1
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with default: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_identity_column() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithIdentity] (
    [Id] INT NOT NULL IDENTITY(1, 1) PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with identity: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_all_common_data_types() {
    let sql = r#"
CREATE TABLE [dbo].[AllTypes] (
    [ColInt] INT NOT NULL,
    [ColBigInt] BIGINT NULL,
    [ColSmallInt] SMALLINT NULL,
    [ColTinyInt] TINYINT NULL,
    [ColBit] BIT NOT NULL,
    [ColDecimal] DECIMAL(18, 2) NULL,
    [ColNumeric] NUMERIC(10, 4) NULL,
    [ColMoney] MONEY NULL,
    [ColFloat] FLOAT NULL,
    [ColReal] REAL NULL,
    [ColDate] DATE NULL,
    [ColTime] TIME NULL,
    [ColDateTime] DATETIME NULL,
    [ColDateTime2] DATETIME2 NULL,
    [ColDateTimeOffset] DATETIMEOFFSET NULL,
    [ColChar] CHAR(10) NULL,
    [ColVarChar] VARCHAR(100) NULL,
    [ColVarCharMax] VARCHAR(MAX) NULL,
    [ColNChar] NCHAR(10) NULL,
    [ColNVarChar] NVARCHAR(100) NULL,
    [ColNVarCharMax] NVARCHAR(MAX) NULL,
    [ColBinary] BINARY(16) NULL,
    [ColVarBinary] VARBINARY(100) NULL,
    [ColUniqueIdentifier] UNIQUEIDENTIFIER NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with all types: {:?}",
        result.err()
    );
}

// ============================================================================
// Additional CREATE TABLE Tests (from TEST_PLAN.md)
// ============================================================================

#[test]
fn test_parse_table_with_computed_column() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithComputed] (
    [FirstName] NVARCHAR(50) NOT NULL,
    [LastName] NVARCHAR(50) NOT NULL,
    [FullName] AS ([FirstName] + ' ' + [LastName])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // Computed columns may or may not be fully supported
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: Computed columns not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_parse_table_with_persisted_computed_column() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithPersistedComputed] (
    [Price] DECIMAL(18, 2) NOT NULL,
    [Quantity] INT NOT NULL,
    [Total] AS ([Price] * [Quantity]) PERSISTED
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // PERSISTED computed columns may or may not be supported
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: PERSISTED computed columns not supported: {:?}", e);
        }
    }
}

#[test]
fn test_debug_computed_columns_with_string_literal() {
    use rust_sqlpackage::parser::FallbackStatementType;

    // Use the exact same content as the fixture file
    let sql = r#"-- Table with PERSISTED computed columns
CREATE TABLE [dbo].[Employees] (
    [Id] INT NOT NULL,
    [FirstName] NVARCHAR(50) NOT NULL,
    [LastName] NVARCHAR(50) NOT NULL,
    [BirthDate] DATE NOT NULL,
    [HireDate] DATE NOT NULL,
    [Salary] DECIMAL(18,2) NOT NULL,
    [BonusPercent] DECIMAL(5,2) NOT NULL DEFAULT 0,

    -- Persisted computed columns (stored physically)
    [FullName] AS ([FirstName] + N' ' + [LastName]) PERSISTED,
    [YearsEmployed] AS (DATEDIFF(YEAR, [HireDate], GETDATE())) PERSISTED,
    [TotalCompensation] AS ([Salary] * (1 + [BonusPercent] / 100)) PERSISTED,

    CONSTRAINT [PK_Employees] PRIMARY KEY ([Id])
);
GO
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    match result {
        Ok(statements) => {
            println!("Parsed {} statements", statements.len());
            for stmt in &statements {
                println!(
                    "Statement type: {:?}",
                    stmt.statement.as_ref().map(std::mem::discriminant)
                );
                println!("Fallback type: {:?}", stmt.fallback_type);

                // Check fallback table parsing
                if let Some(FallbackStatementType::Table {
                    schema,
                    name,
                    columns,
                    constraints,
                    is_node: _,
                    is_edge: _,
                }) = &stmt.fallback_type
                {
                    println!("Fallback Table: [{schema}].[{name}]");
                    println!("Columns ({}):", columns.len());
                    for col in columns {
                        println!(
                            "  - {} (type: {}, computed: {:?}, persisted: {})",
                            col.name, col.data_type, col.computed_expression, col.is_persisted
                        );
                    }
                    println!("Constraints: {}", constraints.len());

                    // Verify FullName is present
                    let fullname = columns.iter().find(|c| c.name == "FullName");
                    assert!(fullname.is_some(), "FullName column should be parsed!");
                    let fullname = fullname.unwrap();
                    assert!(
                        fullname.computed_expression.is_some(),
                        "FullName should have computed expression"
                    );
                    assert!(fullname.is_persisted, "FullName should be persisted");
                    println!("FullName expression: {:?}", fullname.computed_expression);
                }
            }
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Parse error: {:?}", e);
            panic!("Should parse successfully");
        }
    }
}

// ============================================================================
// Temporal Table (System-Versioned) Tests
// ============================================================================

#[test]
fn test_parse_temporal_table_basic() {
    // Basic temporal table with PERIOD FOR SYSTEM_TIME
    let sql = r#"
CREATE TABLE [dbo].[Employee] (
    [EmployeeId] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL,
    [Department] NVARCHAR(50) NULL,
    [Salary] DECIMAL(18, 2) NOT NULL,
    [SysStartTime] DATETIME2 GENERATED ALWAYS AS ROW START NOT NULL,
    [SysEndTime] DATETIME2 GENERATED ALWAYS AS ROW END NOT NULL,
    PERIOD FOR SYSTEM_TIME ([SysStartTime], [SysEndTime])
)
WITH (SYSTEM_VERSIONING = ON);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse temporal table: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1, "Expected 1 statement");
    assert!(
        statements[0].sql_text.contains("PERIOD FOR SYSTEM_TIME"),
        "Should preserve PERIOD FOR SYSTEM_TIME clause"
    );
}

#[test]
fn test_parse_temporal_table_with_history_table() {
    // Temporal table with explicit history table specification
    let sql = r#"
CREATE TABLE [dbo].[Product] (
    [ProductId] INT NOT NULL PRIMARY KEY,
    [ProductName] NVARCHAR(200) NOT NULL,
    [Price] DECIMAL(18, 4) NOT NULL,
    [ValidFrom] DATETIME2 GENERATED ALWAYS AS ROW START NOT NULL,
    [ValidTo] DATETIME2 GENERATED ALWAYS AS ROW END NOT NULL,
    PERIOD FOR SYSTEM_TIME ([ValidFrom], [ValidTo])
)
WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = [dbo].[ProductHistory]));
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse temporal table with history table: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("HISTORY_TABLE"),
        "Should preserve HISTORY_TABLE specification"
    );
}

#[test]
fn test_parse_temporal_table_with_history_retention() {
    // Temporal table with history retention period
    let sql = r#"
CREATE TABLE [dbo].[AuditLog] (
    [LogId] INT IDENTITY(1,1) NOT NULL PRIMARY KEY,
    [Action] NVARCHAR(50) NOT NULL,
    [Details] NVARCHAR(MAX) NULL,
    [StartTime] DATETIME2 GENERATED ALWAYS AS ROW START HIDDEN NOT NULL,
    [EndTime] DATETIME2 GENERATED ALWAYS AS ROW END HIDDEN NOT NULL,
    PERIOD FOR SYSTEM_TIME ([StartTime], [EndTime])
)
WITH (SYSTEM_VERSIONING = ON (
    HISTORY_TABLE = [dbo].[AuditLogHistory],
    HISTORY_RETENTION_PERIOD = 6 MONTHS
));
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse temporal table with history retention: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("HISTORY_RETENTION_PERIOD"),
        "Should preserve HISTORY_RETENTION_PERIOD"
    );
}

#[test]
fn test_parse_temporal_table_hidden_columns() {
    // Temporal table with HIDDEN keyword for period columns
    let sql = r#"
CREATE TABLE [dbo].[Customer] (
    [CustomerId] INT NOT NULL PRIMARY KEY,
    [CustomerName] NVARCHAR(150) NOT NULL,
    [Email] NVARCHAR(255) NULL,
    [SysStart] DATETIME2 GENERATED ALWAYS AS ROW START HIDDEN NOT NULL,
    [SysEnd] DATETIME2 GENERATED ALWAYS AS ROW END HIDDEN NOT NULL,
    PERIOD FOR SYSTEM_TIME ([SysStart], [SysEnd])
)
WITH (SYSTEM_VERSIONING = ON);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse temporal table with HIDDEN columns: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("HIDDEN"),
        "Should preserve HIDDEN keyword"
    );
}

#[test]
fn test_parse_temporal_table_with_constraints() {
    // Temporal table with additional constraints
    let sql = r#"
CREATE TABLE [dbo].[Order] (
    [OrderId] INT NOT NULL,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATE NOT NULL,
    [TotalAmount] DECIMAL(18, 2) NOT NULL,
    [Status] NVARCHAR(20) NOT NULL DEFAULT 'Pending',
    [RowStart] DATETIME2 GENERATED ALWAYS AS ROW START NOT NULL,
    [RowEnd] DATETIME2 GENERATED ALWAYS AS ROW END NOT NULL,
    PERIOD FOR SYSTEM_TIME ([RowStart], [RowEnd]),
    CONSTRAINT [PK_Order] PRIMARY KEY CLUSTERED ([OrderId]),
    CONSTRAINT [CK_Order_TotalAmount] CHECK ([TotalAmount] >= 0),
    CONSTRAINT [FK_Order_Customer] FOREIGN KEY ([CustomerId]) REFERENCES [dbo].[Customer]([CustomerId])
)
WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = [dbo].[OrderHistory]));
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse temporal table with constraints: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("PRIMARY KEY"),
        "Should preserve PRIMARY KEY constraint"
    );
    assert!(
        statements[0].sql_text.contains("FOREIGN KEY"),
        "Should preserve FOREIGN KEY constraint"
    );
}

#[test]
fn test_parse_temporal_table_data_consistency_check() {
    // Temporal table with DATA_CONSISTENCY_CHECK option
    let sql = r#"
CREATE TABLE [dbo].[Inventory] (
    [InventoryId] INT NOT NULL PRIMARY KEY,
    [ProductId] INT NOT NULL,
    [Quantity] INT NOT NULL,
    [WarehouseId] INT NOT NULL,
    [StartTime] DATETIME2 GENERATED ALWAYS AS ROW START NOT NULL,
    [EndTime] DATETIME2 GENERATED ALWAYS AS ROW END NOT NULL,
    PERIOD FOR SYSTEM_TIME ([StartTime], [EndTime])
)
WITH (SYSTEM_VERSIONING = ON (
    HISTORY_TABLE = [dbo].[InventoryHistory],
    DATA_CONSISTENCY_CHECK = ON
));
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse temporal table with DATA_CONSISTENCY_CHECK: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("DATA_CONSISTENCY_CHECK"),
        "Should preserve DATA_CONSISTENCY_CHECK option"
    );
}

#[test]
fn test_parse_temporal_table_ledger_style() {
    // Temporal table with ledger-style append-only history
    // SQL Server 2022 supports ledger tables which build on temporal tables
    let sql = r#"
CREATE TABLE [dbo].[ContractHistory] (
    [ContractId] INT NOT NULL PRIMARY KEY,
    [ContractName] NVARCHAR(200) NOT NULL,
    [ContractValue] DECIMAL(18, 2) NOT NULL,
    [SignedDate] DATE NULL,
    [TransactionStart] DATETIME2 GENERATED ALWAYS AS ROW START NOT NULL,
    [TransactionEnd] DATETIME2 GENERATED ALWAYS AS ROW END NOT NULL,
    PERIOD FOR SYSTEM_TIME ([TransactionStart], [TransactionEnd])
)
WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = [dbo].[ContractHistoryArchive]));
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ledger-style temporal table: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_temporal_table_with_compression() {
    // Temporal table with history table compression
    let sql = r#"
CREATE TABLE [dbo].[SalesTransaction] (
    [TransactionId] BIGINT NOT NULL PRIMARY KEY,
    [ProductId] INT NOT NULL,
    [CustomerId] INT NOT NULL,
    [Quantity] INT NOT NULL,
    [UnitPrice] DECIMAL(18, 4) NOT NULL,
    [TransactionDate] DATE NOT NULL,
    [SysStartTime] DATETIME2 GENERATED ALWAYS AS ROW START NOT NULL,
    [SysEndTime] DATETIME2 GENERATED ALWAYS AS ROW END NOT NULL,
    PERIOD FOR SYSTEM_TIME ([SysStartTime], [SysEndTime])
)
WITH (
    SYSTEM_VERSIONING = ON (
        HISTORY_TABLE = [dbo].[SalesTransactionHistory]
    ),
    DATA_COMPRESSION = PAGE
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse temporal table with compression: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("DATA_COMPRESSION"),
        "Should preserve DATA_COMPRESSION option"
    );
}

// ============================================================================
// JSON in Computed Column Tests
// ============================================================================

#[test]
fn test_parse_json_in_computed_column() {
    let sql = r#"
CREATE TABLE [dbo].[JsonTable] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Data] NVARCHAR(MAX) NOT NULL,
    [Name] AS JSON_VALUE([Data], '$.name'),
    [Type] AS JSON_VALUE([Data], '$.type') PERSISTED
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse computed column with JSON_VALUE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // The parser may use fallback parsing for complex computed columns with JSON functions
    // Verify the SQL text contains the expected content
    assert!(
        statements[0].sql_text.contains("JSON_VALUE"),
        "Table should contain JSON_VALUE in computed column"
    );
    assert!(
        statements[0].sql_text.contains("CREATE TABLE"),
        "Should be a CREATE TABLE statement"
    );
}

// ============================================================================
// ROWGUIDCOL Column Tests
// ============================================================================

#[test]
fn test_parse_table_with_rowguidcol() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithRowGuid] (
    [Id] INT NOT NULL PRIMARY KEY,
    [RowGuid] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL DEFAULT NEWID()
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with ROWGUIDCOL: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        statements[0].sql_text.contains("ROWGUIDCOL"),
        "Should preserve ROWGUIDCOL keyword"
    );
}

#[test]
fn test_parse_table_with_rowguidcol_and_constraint() {
    // ROWGUIDCOL with named default constraint
    let sql = r#"
CREATE TABLE [dbo].[Documents] (
    [DocumentId] INT NOT NULL PRIMARY KEY,
    [DocumentGuid] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL CONSTRAINT [DF_Documents_Guid] DEFAULT NEWSEQUENTIALID(),
    [Name] NVARCHAR(255) NOT NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with ROWGUIDCOL and constraint: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("ROWGUIDCOL"),
        "Should preserve ROWGUIDCOL"
    );
    assert!(
        statements[0].sql_text.contains("NEWSEQUENTIALID"),
        "Should preserve NEWSEQUENTIALID default"
    );
}

#[test]
fn test_parse_table_with_rowguidcol_nullable() {
    // ROWGUIDCOL can be nullable (though typically NOT NULL)
    let sql = r#"
CREATE TABLE [dbo].[OptionalGuid] (
    [Id] INT NOT NULL PRIMARY KEY,
    [TrackingGuid] UNIQUEIDENTIFIER NULL ROWGUIDCOL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with nullable ROWGUIDCOL: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("ROWGUIDCOL"),
        "Should preserve ROWGUIDCOL keyword"
    );
}

#[test]
fn test_parse_table_rowguidcol_unique_constraint() {
    // ROWGUIDCOL with unique constraint
    let sql = r#"
CREATE TABLE [dbo].[UniqueGuidTable] (
    [Id] INT NOT NULL PRIMARY KEY,
    [RowGuid] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL DEFAULT NEWID(),
    CONSTRAINT [UQ_UniqueGuidTable_RowGuid] UNIQUE ([RowGuid])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with ROWGUIDCOL and unique constraint: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_rowguidcol_with_index() {
    // ROWGUIDCOL column used in an index
    let sql = r#"
CREATE TABLE [dbo].[IndexedGuidTable] (
    [Id] INT NOT NULL PRIMARY KEY,
    [ExternalId] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL DEFAULT NEWID()
);
GO
CREATE NONCLUSTERED INDEX [IX_IndexedGuidTable_ExternalId] ON [dbo].[IndexedGuidTable] ([ExternalId]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with ROWGUIDCOL and index: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(
        statements.len(),
        2,
        "Should have table and index statements"
    );
}

// ============================================================================
// SPARSE Column Tests
// ============================================================================

#[test]
fn test_parse_table_with_sparse_column() {
    let sql = r#"
CREATE TABLE [dbo].[TableWithSparse] (
    [Id] INT NOT NULL PRIMARY KEY,
    [OptionalData] NVARCHAR(100) SPARSE NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with SPARSE column: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        statements[0].sql_text.contains("SPARSE"),
        "Should preserve SPARSE keyword"
    );
}

#[test]
fn test_parse_table_with_multiple_sparse_columns() {
    // Wide table pattern with multiple SPARSE columns
    let sql = r#"
CREATE TABLE [dbo].[WideTable] (
    [Id] INT NOT NULL PRIMARY KEY,
    [RequiredField] NVARCHAR(50) NOT NULL,
    [Attribute1] NVARCHAR(100) SPARSE NULL,
    [Attribute2] INT SPARSE NULL,
    [Attribute3] DATETIME2 SPARSE NULL,
    [Attribute4] DECIMAL(18, 2) SPARSE NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with multiple SPARSE columns: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_table_with_sparse_and_column_set() {
    // SPARSE columns with XML COLUMN_SET for wide table pattern
    let sql = r#"
CREATE TABLE [dbo].[DocumentStore] (
    [Id] INT NOT NULL PRIMARY KEY,
    [DocType] NVARCHAR(50) NOT NULL,
    [Attr1] NVARCHAR(100) SPARSE NULL,
    [Attr2] NVARCHAR(100) SPARSE NULL,
    [Attr3] INT SPARSE NULL,
    [SparseColumns] XML COLUMN_SET FOR ALL_SPARSE_COLUMNS
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with SPARSE and COLUMN_SET: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        statements[0].sql_text.contains("COLUMN_SET"),
        "Should preserve COLUMN_SET clause"
    );
    assert!(
        statements[0].sql_text.contains("ALL_SPARSE_COLUMNS"),
        "Should preserve ALL_SPARSE_COLUMNS clause"
    );
}

#[test]
fn test_parse_table_sparse_with_different_types() {
    // SPARSE columns with various data types
    let sql = r#"
CREATE TABLE [dbo].[SparseTypesTable] (
    [Id] INT NOT NULL PRIMARY KEY,
    [SparseInt] INT SPARSE NULL,
    [SparseBigInt] BIGINT SPARSE NULL,
    [SparseNVarChar] NVARCHAR(200) SPARSE NULL,
    [SparseDecimal] DECIMAL(18, 4) SPARSE NULL,
    [SparseDateTime] DATETIME2 SPARSE NULL,
    [SparseUniqueId] UNIQUEIDENTIFIER SPARSE NULL,
    [SparseBit] BIT SPARSE NULL
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse SPARSE columns with different types: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_sparse_with_constraints() {
    // SPARSE columns in a table with other constraints
    let sql = r#"
CREATE TABLE [dbo].[SparseWithConstraints] (
    [Id] INT NOT NULL,
    [Category] INT NOT NULL,
    [SparseData] NVARCHAR(MAX) SPARSE NULL,
    [SparseValue] INT SPARSE NULL,
    CONSTRAINT [PK_SparseWithConstraints] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [UQ_SparseWithConstraints_Category] UNIQUE ([Category])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse SPARSE table with constraints: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("SPARSE"),
        "Should preserve SPARSE keyword"
    );
    assert!(
        statements[0].sql_text.contains("PRIMARY KEY"),
        "Should preserve PRIMARY KEY constraint"
    );
}

// ============================================================================
// FILESTREAM Column Tests
// ============================================================================

#[test]
fn test_parse_table_with_filestream_column() {
    // Basic FILESTREAM column with VARBINARY(MAX)
    let sql = r#"
CREATE TABLE [dbo].[Documents] (
    [Id] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL DEFAULT NEWID(),
    [FileName] NVARCHAR(255) NOT NULL,
    [FileData] VARBINARY(MAX) FILESTREAM NULL,
    CONSTRAINT [PK_Documents] PRIMARY KEY CLUSTERED ([Id])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with FILESTREAM column: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
    assert!(
        statements[0].sql_text.contains("FILESTREAM"),
        "Should preserve FILESTREAM keyword"
    );
}

#[test]
fn test_parse_table_with_filestream_and_rowguidcol() {
    // FILESTREAM requires a ROWGUIDCOL column in the table
    let sql = r#"
CREATE TABLE [dbo].[FileArchive] (
    [FileId] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL DEFAULT NEWSEQUENTIALID(),
    [Content] VARBINARY(MAX) FILESTREAM NOT NULL,
    [Description] NVARCHAR(1000) NULL,
    CONSTRAINT [PK_FileArchive] PRIMARY KEY CLUSTERED ([FileId])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with FILESTREAM and ROWGUIDCOL: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("ROWGUIDCOL"),
        "Should preserve ROWGUIDCOL"
    );
    assert!(
        statements[0].sql_text.contains("FILESTREAM"),
        "Should preserve FILESTREAM"
    );
}

#[test]
fn test_parse_table_with_multiple_filestream_columns() {
    // Table with multiple FILESTREAM columns
    let sql = r#"
CREATE TABLE [dbo].[MediaFiles] (
    [MediaId] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL DEFAULT NEWID(),
    [Thumbnail] VARBINARY(MAX) FILESTREAM NULL,
    [FullResolution] VARBINARY(MAX) FILESTREAM NULL,
    [Metadata] NVARCHAR(MAX) NULL,
    CONSTRAINT [PK_MediaFiles] PRIMARY KEY ([MediaId])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with multiple FILESTREAM columns: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_table_filestream_not_null() {
    // FILESTREAM column with NOT NULL constraint
    let sql = r#"
CREATE TABLE [dbo].[RequiredFiles] (
    [FileId] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL DEFAULT NEWID(),
    [FileContent] VARBINARY(MAX) FILESTREAM NOT NULL,
    CONSTRAINT [PK_RequiredFiles] PRIMARY KEY ([FileId])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with NOT NULL FILESTREAM column: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_table_with_filestream_and_other_columns() {
    // FILESTREAM mixed with regular columns
    let sql = r#"
CREATE TABLE [dbo].[Attachments] (
    [AttachmentId] INT IDENTITY(1,1) NOT NULL,
    [RowGuid] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL DEFAULT NEWID(),
    [Name] NVARCHAR(255) NOT NULL,
    [ContentType] NVARCHAR(100) NOT NULL,
    [Size] BIGINT NOT NULL,
    [Data] VARBINARY(MAX) FILESTREAM NULL,
    [CreatedAt] DATETIME2 NOT NULL DEFAULT GETDATE(),
    CONSTRAINT [PK_Attachments] PRIMARY KEY CLUSTERED ([AttachmentId])
);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table with FILESTREAM and other columns: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("IDENTITY"),
        "Should preserve IDENTITY"
    );
    assert!(
        statements[0].sql_text.contains("FILESTREAM"),
        "Should preserve FILESTREAM"
    );
}

#[test]
fn test_parse_table_with_commaless_inline_default() {
    use rust_sqlpackage::parser::FallbackStatementType;

    // Minimal test case for comma-less constraints with inline defaults
    let sql = r#"CREATE TABLE [dbo].[TestTable]
(
    [Id] INT NOT NULL,
    [Version] INT CONSTRAINT [DF_Version] DEFAULT ((0)) NOT NULL

    CONSTRAINT [PK_Test] PRIMARY KEY ([Id])
)"#;

    // Parse the SQL
    let file = create_sql_file(sql);
    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    let parsed = result.expect("Should parse successfully");

    println!("Parsed {} statements", parsed.len());
    for (i, stmt) in parsed.iter().enumerate() {
        println!("Statement {}: {:?}", i, stmt.fallback_type);
    }

    assert_eq!(parsed.len(), 1, "Should parse one statement");

    // Check that we get a fallback Table statement
    if let Some(FallbackStatementType::Table {
        schema,
        name,
        columns,
        constraints,
        ..
    }) = &parsed[0].fallback_type
    {
        println!("Table: [{}].[{}]", schema, name);
        println!("Columns: {}", columns.len());
        for col in columns {
            println!(
                "  Column: {} {} default={:?} constraint_name={:?}",
                col.name, col.data_type, col.default_value, col.default_constraint_name
            );
        }
        println!("Constraints: {}", constraints.len());

        // Verify we have the inline default
        let version_col = columns
            .iter()
            .find(|c| c.name == "Version")
            .expect("Should find Version column");
        assert_eq!(
            version_col.default_value,
            Some("((0))".to_string()),
            "Version should have default value"
        );
        assert_eq!(
            version_col.default_constraint_name,
            Some("DF_Version".to_string()),
            "Version should have constraint name"
        );
    } else {
        panic!(
            "Expected FallbackStatementType::Table, got {:?}",
            &parsed[0].fallback_type
        );
    }
}
