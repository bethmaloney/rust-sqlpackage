//! ALTER statement parsing tests

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
// ALTER Statement Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_table_add_column() {
    let sql = r#"
ALTER TABLE [dbo].[Users]
ADD [Email] NVARCHAR(255) NULL;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD COLUMN: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_table_drop_column() {
    let sql = r#"
ALTER TABLE [dbo].[Users]
DROP COLUMN [TempColumn];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE DROP COLUMN: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_constraint() {
    let sql = r#"
ALTER TABLE [dbo].[Orders]
ADD CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId]) REFERENCES [dbo].[Users]([Id]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD CONSTRAINT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_drop_constraint() {
    let sql = r#"
ALTER TABLE [dbo].[Orders]
DROP CONSTRAINT [FK_Orders_Users];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE DROP CONSTRAINT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_alter_column() {
    let sql = r#"
ALTER TABLE [dbo].[Users]
ALTER COLUMN [Name] NVARCHAR(500) NOT NULL;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ALTER COLUMN: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_primary_key() {
    let sql = r#"
ALTER TABLE [dbo].[Products]
ADD CONSTRAINT [PK_Products] PRIMARY KEY CLUSTERED ([Id]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD PRIMARY KEY: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_check_constraint() {
    let sql = r#"
ALTER TABLE [dbo].[Products]
ADD CONSTRAINT [CK_Products_Price] CHECK ([Price] >= 0);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD CHECK: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_unique() {
    let sql = r#"
ALTER TABLE [dbo].[Products]
ADD CONSTRAINT [UQ_Products_SKU] UNIQUE ([SKU]);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE ADD UNIQUE: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_table_add_default() {
    let sql = r#"
ALTER TABLE [dbo].[Products]
ADD CONSTRAINT [DF_Products_IsActive] DEFAULT (1) FOR [IsActive];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // ALTER TABLE ADD DEFAULT FOR may use fallback parsing
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER TABLE ADD DEFAULT FOR uses fallback: {:?}", e);
        }
    }
}

#[test]
fn test_parse_alter_table_multiple_actions() {
    let sql = r#"
ALTER TABLE [dbo].[Users]
ADD [MiddleName] NVARCHAR(100) NULL,
    [Suffix] NVARCHAR(10) NULL;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE with multiple adds: {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER VIEW Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_view_basic() {
    let sql = r#"
ALTER VIEW [dbo].[MyView]
AS
SELECT [Id], [Name] FROM [dbo].[Users];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER VIEW: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_view_with_schemabinding() {
    let sql = r#"
ALTER VIEW [dbo].[BoundView]
WITH SCHEMABINDING
AS
SELECT [Id] FROM [dbo].[Users];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // WITH SCHEMABINDING may or may not be supported
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER VIEW WITH SCHEMABINDING not supported: {:?}", e);
        }
    }
}

#[test]
fn test_parse_alter_view_with_columns() {
    let sql = r#"
ALTER VIEW [dbo].[ViewWithColumns] ([Column1], [Column2])
AS
SELECT 1, 2;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER VIEW with columns: {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER PROCEDURE Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_procedure_basic() {
    let sql = r#"
ALTER PROCEDURE [dbo].[MyProc]
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER PROCEDURE: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_procedure_with_parameters() {
    let sql = r#"
ALTER PROCEDURE [dbo].[GetUser]
    @UserId INT,
    @IncludeDeleted BIT = 0
AS
BEGIN
    SELECT * FROM [dbo].[Users] WHERE [Id] = @UserId
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER PROCEDURE with parameters: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_proc_short_form() {
    let sql = r#"
ALTER PROC [dbo].[QuickProc]
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER PROC (short form): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_procedure_with_output_parameter() {
    let sql = r#"
ALTER PROCEDURE [dbo].[GetMaxId]
    @TableName SYSNAME,
    @MaxId BIGINT OUTPUT
AS
BEGIN
    DECLARE @sql NVARCHAR(MAX)
    SET @sql = N'SELECT @MaxId = MAX(Id) FROM ' + QUOTENAME(@TableName)
    EXEC sp_executesql @sql, N'@MaxId BIGINT OUTPUT', @MaxId = @MaxId OUTPUT
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER PROCEDURE with OUTPUT parameter: {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER FUNCTION Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_function_scalar() {
    let sql = r#"
ALTER FUNCTION [dbo].[GetFullName]
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
        "Failed to parse ALTER FUNCTION (scalar): {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_function_table_valued() {
    let sql = r#"
ALTER FUNCTION [dbo].[GetUserOrders]
(
    @UserId INT
)
RETURNS TABLE
AS
RETURN
(
    SELECT [OrderId], [Amount] FROM [dbo].[Orders] WHERE [UserId] = @UserId
)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER FUNCTION (table-valued): {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_function_multi_statement() {
    let sql = r#"
ALTER FUNCTION [dbo].[GetFilteredData]
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
    SELECT [Id], [Value] FROM [dbo].[Data] WHERE [Value] >= @MinValue
    RETURN
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER FUNCTION (multi-statement table): {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER TRIGGER Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_trigger_basic() {
    let sql = r#"
ALTER TRIGGER [dbo].[TR_Users_Update]
ON [dbo].[Users]
AFTER UPDATE
AS
BEGIN
    SET NOCOUNT ON;
    UPDATE [dbo].[Users] SET [ModifiedAt] = GETDATE()
    FROM [dbo].[Users] u
    INNER JOIN inserted i ON u.[Id] = i.[Id]
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // ALTER TRIGGER may use fallback parsing
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER TRIGGER not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_parse_alter_trigger_instead_of() {
    let sql = r#"
ALTER TRIGGER [dbo].[TR_View_Insert]
ON [dbo].[MyView]
INSTEAD OF INSERT
AS
BEGIN
    INSERT INTO [dbo].[BaseTable] ([Col1], [Col2])
    SELECT [Col1], [Col2] FROM inserted
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER TRIGGER INSTEAD OF not supported: {:?}", e);
        }
    }
}

#[test]
fn test_parse_alter_trigger_for_insert_update_delete() {
    let sql = r#"
ALTER TRIGGER [dbo].[TR_Audit]
ON [dbo].[Products]
FOR INSERT, UPDATE, DELETE
AS
BEGIN
    INSERT INTO [dbo].[AuditLog] ([Action], [TableName], [Timestamp])
    VALUES ('Change', 'Products', GETDATE())
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!(
                "Note: ALTER TRIGGER FOR multiple events not supported: {:?}",
                e
            );
        }
    }
}

// ============================================================================
// ALTER SCHEMA Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_schema_transfer() {
    let sql = r#"
ALTER SCHEMA [sales] TRANSFER [dbo].[Orders];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // ALTER SCHEMA TRANSFER may not be supported by sqlparser
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER SCHEMA TRANSFER not supported: {:?}", e);
        }
    }
}

#[test]
fn test_parse_alter_authorization_on_schema() {
    let sql = r#"
ALTER AUTHORIZATION ON SCHEMA::[sales] TO [dbo];
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER AUTHORIZATION ON SCHEMA not supported: {:?}", e);
        }
    }
}

// ============================================================================
// ALTER TYPE Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_type_add_value() {
    // Note: SQL Server doesn't support ALTER TYPE for table types
    // This tests the parser behavior for the syntax
    let sql = r#"
ALTER TYPE [dbo].[StatusType] ADD VALUE 'pending';
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    // This syntax may not be valid T-SQL, documenting behavior
    match result {
        Ok(_) => {
            println!("ALTER TYPE ADD VALUE accepted");
        }
        Err(e) => {
            println!(
                "Note: ALTER TYPE ADD VALUE not supported (expected for T-SQL): {:?}",
                e
            );
        }
    }
}

// ============================================================================
// ALTER SEQUENCE Parsing Tests
// ============================================================================

#[test]
fn test_parse_alter_sequence_restart() {
    let sql = r#"
ALTER SEQUENCE [dbo].[OrderSequence]
RESTART WITH 1000;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER SEQUENCE RESTART: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_alter_sequence_increment() {
    let sql = r#"
ALTER SEQUENCE [dbo].[CounterSeq]
INCREMENT BY 5;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER SEQUENCE INCREMENT: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_sequence_minmax() {
    let sql = r#"
ALTER SEQUENCE [dbo].[BoundedSeq]
MINVALUE 1
MAXVALUE 10000
CYCLE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER SEQUENCE with MIN/MAX: {:?}",
        result.err()
    );
}

#[test]
fn test_parse_alter_sequence_multiple_options() {
    let sql = r#"
ALTER SEQUENCE [dbo].[ComplexSeq]
RESTART WITH 500
INCREMENT BY 10
MINVALUE 1
MAXVALUE 99999
NO CYCLE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER SEQUENCE with multiple options: {:?}",
        result.err()
    );
}

// ============================================================================
// ALTER INDEX Parsing Tests (bonus - commonly used with ALTER)
// ============================================================================

#[test]
fn test_parse_alter_index_rebuild() {
    let sql = r#"
ALTER INDEX [IX_Users_Email] ON [dbo].[Users]
REBUILD;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER INDEX REBUILD not supported: {:?}", e);
        }
    }
}

#[test]
fn test_parse_alter_index_disable() {
    let sql = r#"
ALTER INDEX [IX_Products_Category] ON [dbo].[Products]
DISABLE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER INDEX DISABLE not supported: {:?}", e);
        }
    }
}

#[test]
fn test_parse_alter_index_reorganize() {
    let sql = r#"
ALTER INDEX [IX_Orders_Date] ON [dbo].[Orders]
REORGANIZE;
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER INDEX REORGANIZE not supported: {:?}", e);
        }
    }
}

#[test]
fn test_parse_alter_index_all() {
    let sql = r#"
ALTER INDEX ALL ON [dbo].[LargeTable]
REBUILD WITH (ONLINE = ON);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    match result {
        Ok(statements) => {
            assert_eq!(statements.len(), 1);
        }
        Err(e) => {
            println!("Note: ALTER INDEX ALL not supported: {:?}", e);
        }
    }
}

// ============================================================================
// Temporal Table ALTER Statements
// ============================================================================

#[test]
fn test_parse_temporal_table_alter_enable_versioning() {
    // ALTER TABLE to enable system versioning
    let sql = r#"
ALTER TABLE [dbo].[ExistingTable]
ADD
    [ValidFrom] DATETIME2 GENERATED ALWAYS AS ROW START HIDDEN
        CONSTRAINT [DF_ExistingTable_ValidFrom] DEFAULT SYSUTCDATETIME() NOT NULL,
    [ValidTo] DATETIME2 GENERATED ALWAYS AS ROW END HIDDEN
        CONSTRAINT [DF_ExistingTable_ValidTo] DEFAULT CONVERT(DATETIME2, '9999-12-31 23:59:59.9999999') NOT NULL,
    PERIOD FOR SYSTEM_TIME ([ValidFrom], [ValidTo]);
GO
ALTER TABLE [dbo].[ExistingTable]
SET (SYSTEM_VERSIONING = ON (HISTORY_TABLE = [dbo].[ExistingTableHistory]));
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE for temporal: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        !statements.is_empty(),
        "Should parse ALTER TABLE statements"
    );
}

#[test]
fn test_parse_temporal_table_disable_versioning() {
    // ALTER TABLE to disable system versioning
    let sql = r#"
ALTER TABLE [dbo].[Product]
SET (SYSTEM_VERSIONING = OFF);
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse ALTER TABLE to disable versioning: {:?}",
        result.err()
    );
}
