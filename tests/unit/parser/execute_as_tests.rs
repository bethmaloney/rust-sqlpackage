//! EXECUTE AS clause parsing tests
//!
//! Tests for the EXECUTE AS clause in procedures, functions, and triggers.
//! EXECUTE AS specifies the security context under which the module executes.
//!
//! Options:
//! - EXECUTE AS CALLER - default, runs under the caller's security context
//! - EXECUTE AS SELF - runs under the security context of the module creator
//! - EXECUTE AS OWNER - runs under the security context of the owner
//! - EXECUTE AS 'user_name' - runs under a specific user's security context

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
// EXECUTE AS with Stored Procedures
// ============================================================================

#[test]
fn test_parse_procedure_execute_as_caller() {
    // EXECUTE AS CALLER is the default - explicit declaration
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUserData]
    @UserId INT
WITH EXECUTE AS CALLER
AS
BEGIN
    SELECT * FROM Users WHERE Id = @UserId
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with EXECUTE AS CALLER: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 1);

    // Verify EXECUTE AS CALLER is preserved in SQL text
    assert!(
        statements[0].sql_text.contains("EXECUTE AS CALLER"),
        "SQL text should preserve EXECUTE AS CALLER"
    );
}

#[test]
fn test_parse_procedure_execute_as_self() {
    // EXECUTE AS SELF - runs under the security context of the module creator
    let sql = r#"
CREATE PROCEDURE [dbo].[AuditLogEntry]
    @Action NVARCHAR(100),
    @Details NVARCHAR(MAX)
WITH EXECUTE AS SELF
AS
BEGIN
    INSERT INTO AuditLog (Action, Details, CreatedBy, CreatedAt)
    VALUES (@Action, @Details, SYSTEM_USER, GETDATE())
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with EXECUTE AS SELF: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS SELF"),
        "SQL text should preserve EXECUTE AS SELF"
    );
}

#[test]
fn test_parse_procedure_execute_as_owner() {
    // EXECUTE AS OWNER - runs under the security context of the owner
    let sql = r#"
CREATE PROCEDURE [dbo].[ModifySensitiveData]
    @Id INT,
    @NewValue NVARCHAR(100)
WITH EXECUTE AS OWNER
AS
BEGIN
    UPDATE SensitiveTable SET Value = @NewValue WHERE Id = @Id
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with EXECUTE AS OWNER: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS OWNER"),
        "SQL text should preserve EXECUTE AS OWNER"
    );
}

#[test]
fn test_parse_procedure_execute_as_user() {
    // EXECUTE AS 'user_name' - runs under a specific user's security context
    let sql = r#"
CREATE PROCEDURE [dbo].[RunAsAdmin]
    @Command NVARCHAR(MAX)
WITH EXECUTE AS 'AdminUser'
AS
BEGIN
    EXEC sp_executesql @Command
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with EXECUTE AS 'user': {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS 'AdminUser'"),
        "SQL text should preserve EXECUTE AS 'AdminUser'"
    );
}

#[test]
fn test_parse_procedure_execute_as_with_other_options() {
    // EXECUTE AS combined with other WITH options
    let sql = r#"
CREATE PROCEDURE [dbo].[SecureProc]
    @Data NVARCHAR(MAX)
WITH RECOMPILE, EXECUTE AS OWNER, ENCRYPTION
AS
BEGIN
    SELECT @Data
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with multiple WITH options: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    let sql_text = &statements[0].sql_text;
    assert!(
        sql_text.contains("RECOMPILE"),
        "SQL text should preserve RECOMPILE"
    );
    assert!(
        sql_text.contains("EXECUTE AS OWNER"),
        "SQL text should preserve EXECUTE AS OWNER"
    );
    assert!(
        sql_text.contains("ENCRYPTION"),
        "SQL text should preserve ENCRYPTION"
    );
}

#[test]
fn test_parse_procedure_execute_as_schemabinding() {
    // EXECUTE AS with SCHEMABINDING - common combination
    let sql = r#"
CREATE PROCEDURE [dbo].[BoundProc]
WITH SCHEMABINDING, EXECUTE AS OWNER
AS
BEGIN
    SELECT COUNT(*) FROM [dbo].[BoundTable]
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with SCHEMABINDING and EXECUTE AS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    let sql_text = &statements[0].sql_text;
    assert!(sql_text.contains("SCHEMABINDING"));
    assert!(sql_text.contains("EXECUTE AS OWNER"));
}

// ============================================================================
// EXECUTE AS with Functions
// ============================================================================

#[test]
fn test_parse_function_execute_as_caller() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetUserName]
(
    @UserId INT
)
RETURNS NVARCHAR(100)
WITH EXECUTE AS CALLER
AS
BEGIN
    DECLARE @Name NVARCHAR(100)
    SELECT @Name = UserName FROM Users WHERE Id = @UserId
    RETURN @Name
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse function with EXECUTE AS CALLER: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS CALLER"),
        "SQL text should preserve EXECUTE AS CALLER"
    );
}

#[test]
fn test_parse_function_execute_as_owner() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetSensitiveValue]
(
    @Key NVARCHAR(50)
)
RETURNS NVARCHAR(MAX)
WITH EXECUTE AS OWNER
AS
BEGIN
    DECLARE @Value NVARCHAR(MAX)
    SELECT @Value = Value FROM SecureConfig WHERE [Key] = @Key
    RETURN @Value
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse function with EXECUTE AS OWNER: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS OWNER"),
        "SQL text should preserve EXECUTE AS OWNER"
    );
}

#[test]
fn test_parse_function_execute_as_user() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetAdminData]
(
    @TableName NVARCHAR(128)
)
RETURNS TABLE
WITH EXECUTE AS 'db_admin'
AS
RETURN
(
    SELECT * FROM sys.tables WHERE name = @TableName
)
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse table-valued function with EXECUTE AS 'user': {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS 'db_admin'"),
        "SQL text should preserve EXECUTE AS 'db_admin'"
    );
}

#[test]
fn test_parse_function_execute_as_with_schemabinding() {
    let sql = r#"
CREATE FUNCTION [dbo].[CalculateTotal]
(
    @OrderId INT
)
RETURNS MONEY
WITH SCHEMABINDING, EXECUTE AS OWNER
AS
BEGIN
    DECLARE @Total MONEY
    SELECT @Total = SUM(Quantity * UnitPrice)
    FROM [dbo].[OrderItems]
    WHERE OrderId = @OrderId
    RETURN @Total
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse function with SCHEMABINDING and EXECUTE AS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    let sql_text = &statements[0].sql_text;
    assert!(sql_text.contains("SCHEMABINDING"));
    assert!(sql_text.contains("EXECUTE AS OWNER"));
}

// ============================================================================
// EXECUTE AS with Triggers
// ============================================================================

#[test]
fn test_parse_trigger_execute_as_caller() {
    let sql = r#"
CREATE TRIGGER [dbo].[trg_AuditInsert]
ON [dbo].[SensitiveTable]
AFTER INSERT
WITH EXECUTE AS CALLER
AS
BEGIN
    SET NOCOUNT ON;
    INSERT INTO AuditLog (TableName, Action, UserId)
    SELECT 'SensitiveTable', 'INSERT', SYSTEM_USER
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse trigger with EXECUTE AS CALLER: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS CALLER"),
        "SQL text should preserve EXECUTE AS CALLER"
    );
}

#[test]
fn test_parse_trigger_execute_as_owner() {
    let sql = r#"
CREATE TRIGGER [dbo].[trg_SecurityCheck]
ON [dbo].[ProtectedTable]
INSTEAD OF DELETE
WITH EXECUTE AS OWNER
AS
BEGIN
    SET NOCOUNT ON;
    -- Only owner can delete
    DELETE FROM [dbo].[ProtectedTable]
    WHERE Id IN (SELECT Id FROM deleted)
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse trigger with EXECUTE AS OWNER: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS OWNER"),
        "SQL text should preserve EXECUTE AS OWNER"
    );
}

#[test]
fn test_parse_trigger_execute_as_self() {
    let sql = r#"
CREATE TRIGGER [dbo].[trg_LogChanges]
ON [dbo].[DataTable]
AFTER UPDATE
WITH EXECUTE AS SELF
AS
BEGIN
    SET NOCOUNT ON;
    INSERT INTO ChangeLog (TableName, ModifiedBy, ModifiedAt)
    VALUES ('DataTable', ORIGINAL_LOGIN(), GETDATE())
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse trigger with EXECUTE AS SELF: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS SELF"),
        "SQL text should preserve EXECUTE AS SELF"
    );
}

#[test]
fn test_parse_trigger_execute_as_user() {
    let sql = r#"
CREATE TRIGGER [dbo].[trg_AdminAction]
ON [dbo].[ConfigTable]
AFTER INSERT, UPDATE, DELETE
WITH EXECUTE AS 'TriggerAdmin'
AS
BEGIN
    SET NOCOUNT ON;
    EXEC dbo.LogAdminAction @Table = 'ConfigTable'
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse trigger with EXECUTE AS 'user': {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS 'TriggerAdmin'"),
        "SQL text should preserve EXECUTE AS 'TriggerAdmin'"
    );
}

// ============================================================================
// DDL Triggers with EXECUTE AS
// ============================================================================

#[test]
fn test_parse_ddl_trigger_execute_as() {
    let sql = r#"
CREATE TRIGGER [ddl_AuditDDL]
ON DATABASE
FOR CREATE_TABLE, ALTER_TABLE, DROP_TABLE
WITH EXECUTE AS 'ddl_auditor'
AS
BEGIN
    SET NOCOUNT ON;
    INSERT INTO DDLAuditLog (EventType, ObjectName, LoginName, EventDate)
    SELECT
        EVENTDATA().value('(/EVENT_INSTANCE/EventType)[1]', 'NVARCHAR(100)'),
        EVENTDATA().value('(/EVENT_INSTANCE/ObjectName)[1]', 'NVARCHAR(256)'),
        EVENTDATA().value('(/EVENT_INSTANCE/LoginName)[1]', 'NVARCHAR(256)'),
        GETDATE()
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse DDL trigger with EXECUTE AS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS 'ddl_auditor'"),
        "SQL text should preserve EXECUTE AS 'ddl_auditor'"
    );
}

// ============================================================================
// Edge Cases and Special Scenarios
// ============================================================================

#[test]
fn test_parse_execute_as_with_quoted_identifier_user() {
    // User name with special characters in quotes
    let sql = r#"
CREATE PROCEDURE [dbo].[SpecialUserProc]
WITH EXECUTE AS 'DOMAIN\ServiceAccount'
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with domain user in EXECUTE AS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0]
            .sql_text
            .contains("EXECUTE AS 'DOMAIN\\ServiceAccount'"),
        "SQL text should preserve EXECUTE AS with domain user"
    );
}

#[test]
fn test_parse_execute_as_with_unicode_user() {
    // User name with Unicode characters
    let sql = r#"
CREATE PROCEDURE [dbo].[UnicodeUserProc]
WITH EXECUTE AS N'Bénutzer'
AS
BEGIN
    SELECT 1
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse procedure with Unicode user in EXECUTE AS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    // Check for Unicode string (N'...')
    assert!(
        statements[0].sql_text.contains("EXECUTE AS N'Bénutzer'")
            || statements[0].sql_text.contains("EXECUTE AS N'B"),
        "SQL text should preserve EXECUTE AS with Unicode user"
    );
}

#[test]
fn test_parse_create_or_alter_with_execute_as() {
    let sql = r#"
CREATE OR ALTER PROCEDURE [dbo].[UpdateableProc]
    @Value INT
WITH EXECUTE AS OWNER
AS
BEGIN
    SELECT @Value * 2
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse CREATE OR ALTER with EXECUTE AS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert!(
        statements[0].sql_text.contains("EXECUTE AS OWNER"),
        "SQL text should preserve EXECUTE AS OWNER"
    );
}

#[test]
fn test_parse_multiple_routines_with_different_execute_as() {
    let sql = r#"
CREATE PROCEDURE [dbo].[Proc1]
WITH EXECUTE AS CALLER
AS
BEGIN
    SELECT 1
END
GO

CREATE PROCEDURE [dbo].[Proc2]
WITH EXECUTE AS OWNER
AS
BEGIN
    SELECT 2
END
GO

CREATE FUNCTION [dbo].[Func1]()
RETURNS INT
WITH EXECUTE AS SELF
AS
BEGIN
    RETURN 3
END
"#;
    let file = create_sql_file(sql);

    let result = rust_sqlpackage::parser::parse_sql_file(file.path());
    assert!(
        result.is_ok(),
        "Failed to parse multiple routines with different EXECUTE AS: {:?}",
        result.err()
    );

    let statements = result.unwrap();
    assert_eq!(statements.len(), 3, "Should have 3 statements");

    assert!(
        statements[0].sql_text.contains("EXECUTE AS CALLER"),
        "First procedure should have EXECUTE AS CALLER"
    );
    assert!(
        statements[1].sql_text.contains("EXECUTE AS OWNER"),
        "Second procedure should have EXECUTE AS OWNER"
    );
    assert!(
        statements[2].sql_text.contains("EXECUTE AS SELF"),
        "Function should have EXECUTE AS SELF"
    );
}
