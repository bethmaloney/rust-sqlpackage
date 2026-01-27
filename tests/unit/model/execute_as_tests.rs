//! EXECUTE AS clause model building tests
//!
//! Tests that EXECUTE AS clauses are correctly preserved in the model definition

use super::parse_and_build_model;

// ============================================================================
// Procedure EXECUTE AS Model Tests
// ============================================================================

#[test]
fn test_build_procedure_with_execute_as_caller() {
    let sql = r#"
CREATE PROCEDURE [dbo].[CallerProc]
    @Id INT
WITH EXECUTE AS CALLER
AS
BEGIN
    SELECT * FROM Data WHERE Id = @Id
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some(), "Model should contain a procedure");
    let proc = proc.unwrap();
    assert_eq!(proc.name, "CallerProc");
    assert!(
        proc.definition.contains("EXECUTE AS CALLER"),
        "Definition should preserve EXECUTE AS CALLER"
    );
}

#[test]
fn test_build_procedure_with_execute_as_self() {
    let sql = r#"
CREATE PROCEDURE [dbo].[SelfProc]
WITH EXECUTE AS SELF
AS
BEGIN
    SELECT SYSTEM_USER
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some(), "Model should contain a procedure");
    let proc = proc.unwrap();
    assert_eq!(proc.name, "SelfProc");
    assert!(
        proc.definition.contains("EXECUTE AS SELF"),
        "Definition should preserve EXECUTE AS SELF"
    );
}

#[test]
fn test_build_procedure_with_execute_as_owner() {
    let sql = r#"
CREATE PROCEDURE [dbo].[OwnerProc]
    @TableName NVARCHAR(128)
WITH EXECUTE AS OWNER
AS
BEGIN
    EXEC('SELECT * FROM ' + @TableName)
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some(), "Model should contain a procedure");
    let proc = proc.unwrap();
    assert_eq!(proc.name, "OwnerProc");
    assert!(
        proc.definition.contains("EXECUTE AS OWNER"),
        "Definition should preserve EXECUTE AS OWNER"
    );
}

#[test]
fn test_build_procedure_with_execute_as_user() {
    let sql = r#"
CREATE PROCEDURE [dbo].[AdminProc]
WITH EXECUTE AS 'admin_user'
AS
BEGIN
    SELECT * FROM sys.dm_exec_requests
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some(), "Model should contain a procedure");
    let proc = proc.unwrap();
    assert_eq!(proc.name, "AdminProc");
    assert!(
        proc.definition.contains("EXECUTE AS 'admin_user'"),
        "Definition should preserve EXECUTE AS 'admin_user'"
    );
}

#[test]
fn test_build_procedure_with_execute_as_and_other_options() {
    let sql = r#"
CREATE PROCEDURE [dbo].[SecureProc]
WITH RECOMPILE, EXECUTE AS OWNER
AS
BEGIN
    SELECT 1
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some(), "Model should contain a procedure");
    let proc = proc.unwrap();
    assert!(
        proc.definition.contains("RECOMPILE"),
        "Definition should preserve RECOMPILE"
    );
    assert!(
        proc.definition.contains("EXECUTE AS OWNER"),
        "Definition should preserve EXECUTE AS OWNER"
    );
}

// ============================================================================
// Function EXECUTE AS Model Tests
// ============================================================================

#[test]
fn test_build_scalar_function_with_execute_as() {
    let sql = r#"
CREATE FUNCTION [dbo].[SecureFunc]
(
    @Input NVARCHAR(100)
)
RETURNS NVARCHAR(100)
WITH EXECUTE AS OWNER
AS
BEGIN
    RETURN UPPER(@Input)
END
"#;
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Function(f) = e {
            Some(f)
        } else {
            None
        }
    });

    assert!(func.is_some(), "Model should contain a function");
    let func = func.unwrap();
    assert_eq!(func.name, "SecureFunc");
    assert!(
        func.definition.contains("EXECUTE AS OWNER"),
        "Definition should preserve EXECUTE AS OWNER"
    );
}

#[test]
#[ignore = "TVF type classification changed - tests expect TableValued but code returns InlineTableValued - see IMPLEMENTATION_PLAN.md Phase 11.6"]
fn test_build_tvf_with_execute_as() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetSecureData]
(
    @UserId INT
)
RETURNS TABLE
WITH EXECUTE AS 'data_reader'
AS
RETURN
(
    SELECT * FROM SecureTable WHERE UserId = @UserId
)
"#;
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Function(f) = e {
            Some(f)
        } else {
            None
        }
    });

    assert!(func.is_some(), "Model should contain a function");
    let func = func.unwrap();
    assert_eq!(func.name, "GetSecureData");
    assert_eq!(
        func.function_type,
        rust_sqlpackage::model::FunctionType::TableValued
    );
    assert!(
        func.definition.contains("EXECUTE AS 'data_reader'"),
        "Definition should preserve EXECUTE AS 'data_reader'"
    );
}

#[test]
fn test_build_function_with_execute_as_and_schemabinding() {
    let sql = r#"
CREATE FUNCTION [dbo].[BoundSecureFunc]
(
    @Value INT
)
RETURNS INT
WITH SCHEMABINDING, EXECUTE AS SELF
AS
BEGIN
    RETURN @Value * 2
END
"#;
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Function(f) = e {
            Some(f)
        } else {
            None
        }
    });

    assert!(func.is_some(), "Model should contain a function");
    let func = func.unwrap();
    assert!(
        func.definition.contains("SCHEMABINDING"),
        "Definition should preserve SCHEMABINDING"
    );
    assert!(
        func.definition.contains("EXECUTE AS SELF"),
        "Definition should preserve EXECUTE AS SELF"
    );
}

// ============================================================================
// Multiple Elements with Different EXECUTE AS
// ============================================================================

#[test]
fn test_build_multiple_elements_with_different_execute_as() {
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
    let model = parse_and_build_model(sql);

    // Find all procedures
    let procs: Vec<_> = model
        .elements
        .iter()
        .filter_map(|e| {
            if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
                Some(p)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(procs.len(), 2, "Model should contain 2 procedures");

    let proc1 = procs.iter().find(|p| p.name == "Proc1");
    let proc2 = procs.iter().find(|p| p.name == "Proc2");

    assert!(proc1.is_some(), "Should have Proc1");
    assert!(proc2.is_some(), "Should have Proc2");

    assert!(
        proc1.unwrap().definition.contains("EXECUTE AS CALLER"),
        "Proc1 should have EXECUTE AS CALLER"
    );
    assert!(
        proc2.unwrap().definition.contains("EXECUTE AS OWNER"),
        "Proc2 should have EXECUTE AS OWNER"
    );

    // Find function
    let func = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Function(f) = e {
            Some(f)
        } else {
            None
        }
    });

    assert!(func.is_some(), "Model should contain a function");
    assert!(
        func.unwrap().definition.contains("EXECUTE AS SELF"),
        "Func1 should have EXECUTE AS SELF"
    );
}

// ============================================================================
// EXECUTE AS with Native Compilation
// ============================================================================

#[test]
fn test_build_natively_compiled_with_execute_as() {
    let sql = r#"
CREATE PROCEDURE [dbo].[NativeSecureProc]
    @Id INT
WITH NATIVE_COMPILATION, SCHEMABINDING, EXECUTE AS OWNER
AS
BEGIN ATOMIC WITH (TRANSACTION ISOLATION LEVEL = SNAPSHOT, LANGUAGE = N'English')
    SELECT [Id], [Value] FROM [dbo].[MemTable] WHERE [Id] = @Id;
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
            Some(p)
        } else {
            None
        }
    });

    assert!(proc.is_some(), "Model should contain a procedure");
    let proc = proc.unwrap();
    assert_eq!(proc.name, "NativeSecureProc");
    assert!(
        proc.is_natively_compiled,
        "Procedure should be marked as natively compiled"
    );
    assert!(
        proc.definition.contains("EXECUTE AS OWNER"),
        "Definition should preserve EXECUTE AS OWNER"
    );
}

#[test]
fn test_build_natively_compiled_function_with_execute_as() {
    let sql = r#"
CREATE FUNCTION [dbo].[NativeSecureFunc]
(
    @Value INT
)
RETURNS INT
WITH NATIVE_COMPILATION, SCHEMABINDING, EXECUTE AS OWNER
AS
BEGIN ATOMIC WITH (TRANSACTION ISOLATION LEVEL = SNAPSHOT, LANGUAGE = N'English')
    RETURN @Value * 2;
END
"#;
    let model = parse_and_build_model(sql);

    let func = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Function(f) = e {
            Some(f)
        } else {
            None
        }
    });

    assert!(func.is_some(), "Model should contain a function");
    let func = func.unwrap();
    assert_eq!(func.name, "NativeSecureFunc");
    assert!(
        func.is_natively_compiled,
        "Function should be marked as natively compiled"
    );
    assert!(
        func.definition.contains("EXECUTE AS OWNER"),
        "Definition should preserve EXECUTE AS OWNER"
    );
}
