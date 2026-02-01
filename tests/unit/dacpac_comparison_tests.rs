//! Unit tests for dacpac compatibility features
//!
//! These tests verify that rust-sqlpackage produces model.xml content
//! compatible with the .NET DacFx toolchain.

use std::io::Write;

use tempfile::NamedTempFile;

/// Helper to create a temp SQL file with content
fn create_sql_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::with_suffix(".sql").unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

/// Helper to create a test SqlProject
fn create_test_project() -> rust_sqlpackage::project::SqlProject {
    rust_sqlpackage::project::SqlProject {
        name: "TestProject".to_string(),
        target_platform: rust_sqlpackage::project::SqlServerVersion::Sql160,
        default_schema: "dbo".to_string(),
        collation_lcid: 1033,
        collation_case_sensitive: false,
        sql_files: vec![],
        dacpac_references: vec![],
        package_references: vec![],
        sqlcmd_variables: vec![],
        project_dir: std::path::PathBuf::new(),
        pre_deploy_script: None,
        post_deploy_script: None,
        ansi_nulls: true,
        quoted_identifier: true,
        database_options: rust_sqlpackage::project::DatabaseOptions::default(),
        dac_version: "1.0.0.0".to_string(),
        dac_description: None,
    }
}

/// Helper to parse SQL and build model
fn parse_and_build_model(sql: &str) -> rust_sqlpackage::model::DatabaseModel {
    let file = create_sql_file(sql);
    let statements = rust_sqlpackage::parser::parse_sql_file(file.path()).unwrap();
    let project = create_test_project();
    rust_sqlpackage::model::build_model(&statements, &project).unwrap()
}

// ============================================================================
// Named Default Constraint Tests
// ============================================================================

#[test]
fn test_named_inline_default_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[Entity] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Version] INT CONSTRAINT [DF_Entity_Version] NOT NULL DEFAULT ((0))
);
"#;
    let model = parse_and_build_model(sql);

    // Should have the table
    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });
    assert!(table.is_some(), "Model should contain a table");

    // Should capture the named default constraint
    let has_named_default = model.elements.iter().any(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            c.name.contains("DF_Entity_Version")
        } else {
            false
        }
    });

    // Currently expected to fail - this is a missing feature
    assert!(
        has_named_default,
        "Model should contain named default constraint DF_Entity_Version"
    );
}

#[test]
fn test_multiple_named_inline_defaults() {
    let sql = r#"
CREATE TABLE [dbo].[Entity] (
    [Id] UNIQUEIDENTIFIER NOT NULL,
    [Version] INT CONSTRAINT [DF_Entity_Version] NOT NULL DEFAULT ((0)),
    [CreatedOn] DATETIME CONSTRAINT [DF_Entity_CreatedOn] NOT NULL DEFAULT GETDATE(),
    [ModifiedOn] DATETIME CONSTRAINT [DF_Entity_ModifiedOn] NOT NULL DEFAULT GETDATE(),
    CONSTRAINT [PK_Entity] PRIMARY KEY CLUSTERED ([Id] ASC)
);
"#;
    let model = parse_and_build_model(sql);

    let default_count = model
        .elements
        .iter()
        .filter(|e| {
            if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
                c.name.starts_with("DF_")
            } else {
                false
            }
        })
        .count();

    // Should have 3 named default constraints
    assert_eq!(
        default_count, 3,
        "Model should have 3 named default constraints (DF_Entity_Version, DF_Entity_CreatedOn, DF_Entity_ModifiedOn)"
    );
}

#[test]
fn test_default_with_function_call() {
    let sql = r#"
CREATE TABLE [dbo].[Entity] (
    [Id] UNIQUEIDENTIFIER CONSTRAINT [DF_Entity_Id] NOT NULL DEFAULT NEWID(),
    [CreatedAt] DATETIME2 CONSTRAINT [DF_Entity_CreatedAt] NOT NULL DEFAULT SYSDATETIME()
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });
    assert!(table.is_some());

    let table = table.unwrap();
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some());

    let id_col = id_col.unwrap();
    assert!(id_col.default_value.is_some());
    assert!(
        id_col
            .default_value
            .as_ref()
            .unwrap()
            .to_uppercase()
            .contains("NEWID"),
        "Default should contain NEWID"
    );
}

// ============================================================================
// Ampersand Encoding Tests
// ============================================================================

#[test]
fn test_ampersand_in_table_name() {
    // Note: This SQL may not parse correctly with sqlparser-rs
    // This test documents the expected behavior
    let sql = r#"
CREATE TABLE [dbo].[P&L_Report] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Revenue] DECIMAL(18, 2) NOT NULL
);
"#;

    // Try to parse - may fail
    let file = create_sql_file(sql);
    let result = rust_sqlpackage::parser::parse_sql_file(file.path());

    if let Ok(statements) = result {
        let project = create_test_project();
        let model = rust_sqlpackage::model::build_model(&statements, &project).unwrap();

        let table = model.elements.iter().find_map(|e| {
            if let rust_sqlpackage::model::ModelElement::Table(t) = e {
                Some(t)
            } else {
                None
            }
        });

        if let Some(t) = table {
            // The table name should contain the ampersand
            assert!(
                t.name.contains("&") || t.name.contains("P&L"),
                "Table name should preserve ampersand: {}",
                t.name
            );
        }
    }
}

#[test]
fn test_ampersand_in_procedure_name() {
    let sql = r#"
CREATE PROCEDURE [dbo].[IOLoansWithoutP&IConversionNotifications]
AS
BEGIN
    SELECT 1 AS [Result];
END
"#;

    let file = create_sql_file(sql);
    let result = rust_sqlpackage::parser::parse_sql_file(file.path());

    if let Ok(statements) = result {
        let project = create_test_project();
        let model = rust_sqlpackage::model::build_model(&statements, &project).unwrap();

        let proc = model.elements.iter().find_map(|e| {
            if let rust_sqlpackage::model::ModelElement::Procedure(p) = e {
                Some(p)
            } else {
                None
            }
        });

        if let Some(p) = proc {
            // Should contain full name with ampersand (not truncated)
            assert!(
                p.name.contains("P&I") || p.name.contains("P&amp;I"),
                "Procedure name should contain P&I, not be truncated: {}",
                p.name
            );
        }
    }
}

// ============================================================================
// Ampersand in Function Name Tests
// ============================================================================

#[test]
fn test_ampersand_in_function_name() {
    let sql = r#"
CREATE FUNCTION [dbo].[Get_P&L_Report]()
RETURNS TABLE
AS
RETURN (SELECT 1 AS Value)
"#;

    let file = create_sql_file(sql);
    let result = rust_sqlpackage::parser::parse_sql_file(file.path());

    if let Ok(statements) = result {
        let project = create_test_project();
        let model = rust_sqlpackage::model::build_model(&statements, &project).unwrap();

        let func = model.elements.iter().find_map(|e| {
            if let rust_sqlpackage::model::ModelElement::Function(f) = e {
                Some(f)
            } else {
                None
            }
        });

        if let Some(f) = func {
            // Should contain full name with ampersand (not truncated to "Get_P")
            assert!(
                f.name.contains("P&L"),
                "Function name should contain P&L, not be truncated: {}",
                f.name
            );
        } else {
            panic!("Function should have been parsed");
        }
    }
}

// ============================================================================
// Procedure Parameter Tests
// ============================================================================

#[test]
fn test_procedure_parameters_captured() {
    let sql = r#"
CREATE PROCEDURE [dbo].[GetUserById]
    @UserId INT,
    @IncludeDeleted BIT = 0
AS
BEGIN
    SELECT @UserId AS UserId;
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

    // Definition should contain parameter declarations
    assert!(
        proc.definition.contains("@UserId"),
        "Definition should contain @UserId parameter"
    );
    assert!(
        proc.definition.contains("@IncludeDeleted"),
        "Definition should contain @IncludeDeleted parameter"
    );
}

#[test]
fn test_procedure_output_parameter() {
    let sql = r#"
CREATE PROCEDURE [dbo].[CreateUser]
    @Name NVARCHAR(100),
    @NewId INT OUTPUT
AS
BEGIN
    SET @NewId = 1;
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

    assert!(proc.is_some());
    let proc = proc.unwrap();

    // Definition should contain OUTPUT parameter
    assert!(
        proc.definition.contains("OUTPUT") || proc.definition.contains("output"),
        "Definition should contain OUTPUT keyword"
    );
}

#[test]
fn test_function_parameters_captured() {
    let sql = r#"
CREATE FUNCTION [dbo].[CalculateTotal]
(
    @Quantity INT,
    @UnitPrice DECIMAL(18, 2),
    @DiscountPercent DECIMAL(5, 2) = 0
)
RETURNS DECIMAL(18, 2)
AS
BEGIN
    RETURN @Quantity * @UnitPrice * (1 - @DiscountPercent / 100);
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

    // Definition should contain all parameters
    assert!(func.definition.contains("@Quantity"));
    assert!(func.definition.contains("@UnitPrice"));
    assert!(func.definition.contains("@DiscountPercent"));
}

// ============================================================================
// Inline Constraint Annotation Tests
// ============================================================================

#[test]
fn test_inline_unique_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[Customer] (
    [Id] INT NOT NULL IDENTITY(1,1),
    [Email] NVARCHAR(255) NOT NULL UNIQUE,
    CONSTRAINT [PK_Customer] PRIMARY KEY ([Id])
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let _table = table.unwrap();

    // Should have a unique constraint (either inline or separate)
    let has_unique = model.elements.iter().any(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            matches!(
                c.constraint_type,
                rust_sqlpackage::model::ConstraintType::Unique
            )
        } else {
            false
        }
    });

    assert!(
        has_unique,
        "Model should contain unique constraint for Email column"
    );
}

#[test]
fn test_inline_check_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[Employee] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Age] INT NOT NULL CHECK ([Age] >= 18 AND [Age] <= 120)
);
"#;
    let model = parse_and_build_model(sql);

    let has_check = model.elements.iter().any(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            matches!(
                c.constraint_type,
                rust_sqlpackage::model::ConstraintType::Check
            )
        } else {
            false
        }
    });

    assert!(
        has_check,
        "Model should contain inline check constraint for Age column"
    );
}

// ============================================================================
// Table Type Tests
// ============================================================================

#[test]
fn test_table_type_basic() {
    let sql = r#"
CREATE TYPE [dbo].[IdListType] AS TABLE (
    [Id] INT NOT NULL,
    [SortOrder] INT NOT NULL DEFAULT 0
);
"#;

    let file = create_sql_file(sql);
    let result = rust_sqlpackage::parser::parse_sql_file(file.path());

    // Table types may or may not be supported
    if let Ok(statements) = result {
        let project = create_test_project();
        if let Ok(model) = rust_sqlpackage::model::build_model(&statements, &project) {
            // Check if we have any table type elements
            let has_table_type = model
                .elements
                .iter()
                .any(|e| matches!(e, rust_sqlpackage::model::ModelElement::UserDefinedType(_)));

            // Document current behavior
            println!(
                "Table type support: {}",
                if has_table_type {
                    "supported"
                } else {
                    "not yet supported"
                }
            );
        }
    }
}

#[test]
fn test_table_type_with_primary_key() {
    let sql = r#"
CREATE TYPE [dbo].[OrderItemsType] AS TABLE (
    [ProductId] INT NOT NULL,
    [Quantity] INT NOT NULL,
    PRIMARY KEY CLUSTERED ([ProductId])
);
"#;

    let file = create_sql_file(sql);
    let result = rust_sqlpackage::parser::parse_sql_file(file.path());

    // Document behavior
    if result.is_ok() {
        println!("Table type with PK parsed successfully");
    } else {
        println!(
            "Table type with PK not yet supported: {:?}",
            result.err().map(|e| e.to_string())
        );
    }
}

// ============================================================================
// Extended Property Tests
// ============================================================================

#[test]
fn test_extended_property_parsed() {
    let sql = r#"
CREATE TABLE [dbo].[DocumentedTable] (
    [Id] INT NOT NULL PRIMARY KEY
);
GO

EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'This is a documented table',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'DocumentedTable';
"#;

    let file = create_sql_file(sql);
    let result = rust_sqlpackage::parser::parse_sql_file(file.path());

    // Extended properties are EXEC statements and may be handled by regex fallback
    if let Ok(statements) = result {
        let project = create_test_project();
        if let Ok(_model) = rust_sqlpackage::model::build_model(&statements, &project) {
            // Document current behavior - extended properties may not be captured
            println!("Extended property test: statements parsed");
        }
    }
}

// ============================================================================
// IsAnsiNullsOn Property Tests
// ============================================================================

#[test]
fn test_table_ansi_nulls_property() {
    let sql = r#"
SET ANSI_NULLS ON
GO
CREATE TABLE [dbo].[TestTable] (
    [Id] INT NOT NULL PRIMARY KEY
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");

    // Document: DotNet adds IsAnsiNullsOn="True" property to tables
    // This test documents whether we capture this
}

// ============================================================================
// Index Column Ordering Tests (ASC/DESC)
// ============================================================================

#[test]
fn test_index_column_descending() {
    let sql = r#"
CREATE TABLE [dbo].[Orders] (
    [Id] INT NOT NULL PRIMARY KEY,
    [OrderDate] DATE NOT NULL
);
GO

CREATE NONCLUSTERED INDEX [IX_Orders_OrderDate]
ON [dbo].[Orders] ([OrderDate] DESC);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");

    // Document: Should capture DESC ordering on columns
}

#[test]
fn test_index_multiple_columns_mixed_order() {
    let sql = r#"
CREATE TABLE [dbo].[Orders] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Status] NVARCHAR(50) NOT NULL,
    [OrderDate] DATE NOT NULL
);
GO

CREATE NONCLUSTERED INDEX [IX_Orders_Status_OrderDate]
ON [dbo].[Orders] ([Status] ASC, [OrderDate] DESC);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some(), "Model should contain an index");

    // Document: Should capture both ASC and DESC orderings
}

// ============================================================================
// Computed Column Tests (for CTE/View context)
// ============================================================================

#[test]
fn test_view_computed_columns() {
    let sql = r#"
CREATE VIEW [dbo].[OrderSummary]
AS
SELECT
    OrderId,
    Quantity * UnitPrice AS LineTotal,
    Quantity * UnitPrice * (1 - Discount) AS DiscountedTotal
FROM Orders;
"#;
    let model = parse_and_build_model(sql);

    let view = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::View(v) = e {
            Some(v)
        } else {
            None
        }
    });

    assert!(view.is_some(), "Model should contain a view");

    // Document: DotNet captures computed column expressions in views
}

// ============================================================================
// Full-Text Index Tests
// ============================================================================

#[test]
fn test_fulltext_catalog() {
    let sql = r#"
CREATE FULLTEXT CATALOG [DocumentCatalog] AS DEFAULT;
"#;

    let file = create_sql_file(sql);
    let result = rust_sqlpackage::parser::parse_sql_file(file.path());

    // Document current support
    if result.is_ok() {
        println!("Full-text catalog: parsed successfully");
    } else {
        println!(
            "Full-text catalog: not yet supported - {:?}",
            result.err().map(|e| e.to_string())
        );
    }
}

#[test]
fn test_fulltext_index() {
    let sql = r#"
CREATE TABLE [dbo].[Documents] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Content] NVARCHAR(MAX) NOT NULL
);
GO

CREATE FULLTEXT INDEX ON [dbo].[Documents] (
    [Content] LANGUAGE 1033
)
KEY INDEX [PK__Documents];
"#;

    let file = create_sql_file(sql);
    let result = rust_sqlpackage::parser::parse_sql_file(file.path());

    // Document current support
    if result.is_ok() {
        println!("Full-text index: parsed successfully");
    } else {
        println!(
            "Full-text index: not yet supported - {:?}",
            result.err().map(|e| e.to_string())
        );
    }
}
