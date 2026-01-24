//! ModelElement Method Tests, Schema Deduplication, Index Property Tests, Constraint Detail Tests

use super::parse_and_build_model;

// ============================================================================
// Index Property Tests
// ============================================================================

#[test]
fn test_build_unique_index() {
    let sql = r#"
CREATE TABLE [dbo].[T] ([Col1] INT NOT NULL);
GO
CREATE UNIQUE INDEX [IX_T_Col1_Unique]
ON [dbo].[T] ([Col1]);
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
    let index = index.unwrap();
    assert!(index.is_unique, "Index should be marked as unique");
}

#[test]
fn test_build_index_columns() {
    let sql = r#"
CREATE TABLE [dbo].[T] ([Col1] INT, [Col2] INT, [Col3] INT);
GO
CREATE INDEX [IX_T_Multi]
ON [dbo].[T] ([Col1], [Col2]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some());
    let index = index.unwrap();
    assert_eq!(index.columns.len(), 2, "Index should have 2 columns");
}

#[test]
fn test_build_index_table_reference() {
    let sql = r#"
CREATE TABLE [sales].[Orders] ([Id] INT NOT NULL);
GO
CREATE INDEX [IX_Orders_Id]
ON [sales].[Orders] ([Id]);
"#;
    let model = parse_and_build_model(sql);

    let index = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Index(i) = e {
            Some(i)
        } else {
            None
        }
    });

    assert!(index.is_some());
    let index = index.unwrap();
    assert!(
        index.table_schema.contains("sales"),
        "Index should reference sales schema"
    );
    assert!(
        index.table_name.contains("Orders"),
        "Index should reference Orders table"
    );
}

// ============================================================================
// Constraint Detail Tests
// ============================================================================

#[test]
fn test_build_primary_key_columns() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id1] INT NOT NULL,
    [Id2] INT NOT NULL,
    CONSTRAINT [PK_T] PRIMARY KEY CLUSTERED ([Id1], [Id2])
);
"#;
    let model = parse_and_build_model(sql);

    let pk = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            if matches!(
                c.constraint_type,
                rust_sqlpackage::model::ConstraintType::PrimaryKey
            ) {
                Some(c)
            } else {
                None
            }
        } else {
            None
        }
    });

    assert!(pk.is_some(), "Model should contain PK constraint");
    let pk = pk.unwrap();
    assert_eq!(pk.columns.len(), 2, "PK should have 2 columns");
}

#[test]
fn test_build_foreign_key_referenced_table() {
    let sql = r#"
CREATE TABLE [dbo].[Parent] ([Id] INT NOT NULL PRIMARY KEY);
GO
CREATE TABLE [dbo].[Child] (
    [Id] INT NOT NULL PRIMARY KEY,
    [ParentId] INT NOT NULL,
    CONSTRAINT [FK_Child_Parent] FOREIGN KEY ([ParentId]) REFERENCES [dbo].[Parent]([Id])
);
"#;
    let model = parse_and_build_model(sql);

    let fk = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            if matches!(
                c.constraint_type,
                rust_sqlpackage::model::ConstraintType::ForeignKey
            ) {
                Some(c)
            } else {
                None
            }
        } else {
            None
        }
    });

    assert!(fk.is_some(), "Model should contain FK constraint");
    let fk = fk.unwrap();
    assert!(
        fk.referenced_table.is_some(),
        "FK should have referenced table"
    );
    assert!(
        fk.referenced_table.as_ref().unwrap().contains("Parent"),
        "FK should reference Parent table"
    );
    assert!(
        fk.referenced_columns.is_some(),
        "FK should have referenced columns"
    );
}

#[test]
fn test_build_check_constraint_definition() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Age] INT NOT NULL,
    CONSTRAINT [CK_T_Age] CHECK ([Age] >= 0 AND [Age] <= 150)
);
"#;
    let model = parse_and_build_model(sql);

    let ck = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            if matches!(
                c.constraint_type,
                rust_sqlpackage::model::ConstraintType::Check
            ) {
                Some(c)
            } else {
                None
            }
        } else {
            None
        }
    });

    assert!(ck.is_some(), "Model should contain CHECK constraint");
    let ck = ck.unwrap();
    assert!(ck.definition.is_some(), "CHECK should have definition");
    let def = ck.definition.as_ref().unwrap();
    assert!(
        def.contains("Age"),
        "CHECK definition should reference Age column"
    );
}

// ============================================================================
// Schema Deduplication Tests
// ============================================================================

#[test]
fn test_schemas_are_deduplicated() {
    let sql = r#"
CREATE TABLE [dbo].[T1] ([Id] INT NOT NULL);
GO
CREATE TABLE [dbo].[T2] ([Id] INT NOT NULL);
GO
CREATE TABLE [dbo].[T3] ([Id] INT NOT NULL);
"#;
    let model = parse_and_build_model(sql);

    let schema_count = model
        .elements
        .iter()
        .filter(|e| matches!(e, rust_sqlpackage::model::ModelElement::Schema(s) if s.name == "dbo"))
        .count();

    assert_eq!(schema_count, 1, "dbo schema should appear exactly once");
}

#[test]
fn test_multiple_schemas_all_present() {
    let sql = r#"
CREATE TABLE [dbo].[T1] ([Id] INT NOT NULL);
GO
CREATE TABLE [sales].[T2] ([Id] INT NOT NULL);
GO
CREATE TABLE [hr].[T3] ([Id] INT NOT NULL);
"#;
    let model = parse_and_build_model(sql);

    let schema_names: Vec<_> = model
        .elements
        .iter()
        .filter_map(|e| {
            if let rust_sqlpackage::model::ModelElement::Schema(s) = e {
                Some(s.name.as_str())
            } else {
                None
            }
        })
        .collect();

    assert!(
        schema_names.iter().any(|s| s.contains("dbo")),
        "Should have dbo schema"
    );
    assert!(
        schema_names.iter().any(|s| s.contains("sales")),
        "Should have sales schema"
    );
    assert!(
        schema_names.iter().any(|s| s.contains("hr")),
        "Should have hr schema"
    );
}

// ============================================================================
// ModelElement Method Tests
// ============================================================================

#[test]
fn test_model_element_type_name_table() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model
        .elements
        .iter()
        .find(|e| matches!(e, rust_sqlpackage::model::ModelElement::Table(_)));

    assert!(table.is_some());
    assert_eq!(table.unwrap().type_name(), "SqlTable");
}

#[test]
fn test_model_element_type_name_view() {
    let sql = "CREATE VIEW [dbo].[V] AS SELECT 1 AS [Val];";
    let model = parse_and_build_model(sql);

    let view = model
        .elements
        .iter()
        .find(|e| matches!(e, rust_sqlpackage::model::ModelElement::View(_)));

    assert!(view.is_some());
    assert_eq!(view.unwrap().type_name(), "SqlView");
}

#[test]
fn test_model_element_type_name_procedure() {
    let sql = r#"
CREATE PROCEDURE [dbo].[P]
AS
BEGIN
    SELECT 1
END
"#;
    let model = parse_and_build_model(sql);

    let proc = model
        .elements
        .iter()
        .find(|e| matches!(e, rust_sqlpackage::model::ModelElement::Procedure(_)));

    assert!(proc.is_some());
    assert_eq!(proc.unwrap().type_name(), "SqlProcedure");
}

#[test]
fn test_model_element_type_name_scalar_function() {
    let sql = r#"
CREATE FUNCTION [dbo].[F]()
RETURNS INT
AS
BEGIN
    RETURN 1
END
"#;
    let model = parse_and_build_model(sql);

    let func = model
        .elements
        .iter()
        .find(|e| matches!(e, rust_sqlpackage::model::ModelElement::Function(_)));

    assert!(func.is_some());
    assert_eq!(func.unwrap().type_name(), "SqlScalarFunction");
}

#[test]
fn test_model_element_type_name_table_valued_function() {
    let sql = r#"
CREATE FUNCTION [dbo].[TVF]()
RETURNS TABLE
AS
RETURN (SELECT 1 AS [Val])
"#;
    let model = parse_and_build_model(sql);

    let func = model
        .elements
        .iter()
        .find(|e| matches!(e, rust_sqlpackage::model::ModelElement::Function(_)));

    assert!(func.is_some());
    assert_eq!(func.unwrap().type_name(), "SqlMultiStatementTableValuedFunction");
}

#[test]
fn test_model_element_full_name_table() {
    let sql = "CREATE TABLE [sales].[Orders] ([Id] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model
        .elements
        .iter()
        .find(|e| matches!(e, rust_sqlpackage::model::ModelElement::Table(_)));

    assert!(table.is_some());
    let full_name = table.unwrap().full_name();
    assert!(
        full_name.contains("sales") && full_name.contains("Orders"),
        "Full name should be [sales].[Orders], got: {}",
        full_name
    );
}

#[test]
fn test_model_element_full_name_schema() {
    let sql = "CREATE TABLE [myschema].[T] ([Id] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let schema = model.elements.iter().find(|e| {
        if let rust_sqlpackage::model::ModelElement::Schema(s) = e {
            s.name.contains("myschema")
        } else {
            false
        }
    });

    assert!(schema.is_some());
    let full_name = schema.unwrap().full_name();
    assert!(
        full_name.contains("myschema"),
        "Full name should contain myschema, got: {}",
        full_name
    );
}
