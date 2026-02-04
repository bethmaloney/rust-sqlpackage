//! Constraint Building Tests

use super::parse_and_build_model;

// ============================================================================
// Constraint Building Tests
// ============================================================================

#[test]
fn test_build_primary_key_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL,
    CONSTRAINT [PK_T] PRIMARY KEY CLUSTERED ([Id])
);
"#;
    let model = parse_and_build_model(sql);

    let has_pk = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Constraint(c) if c.name.contains("PK_"))
    });
    assert!(has_pk, "Model should contain primary key constraint");
}

#[test]
fn test_build_foreign_key_constraint() {
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

    let has_fk = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Constraint(c) if c.name.contains("FK_"))
    });
    assert!(has_fk, "Model should contain foreign key constraint");
}

#[test]
fn test_build_unique_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Email] NVARCHAR(255) NOT NULL,
    CONSTRAINT [UQ_T_Email] UNIQUE ([Email])
);
"#;
    let model = parse_and_build_model(sql);

    let has_uq = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Constraint(c) if c.name.contains("UQ_"))
    });
    assert!(has_uq, "Model should contain unique constraint");
}

#[test]
fn test_build_check_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Age] INT NOT NULL,
    CONSTRAINT [CK_T_Age] CHECK ([Age] >= 0)
);
"#;
    let model = parse_and_build_model(sql);

    let has_ck = model.elements.iter().any(|e| {
        matches!(e, rust_sqlpackage::model::ModelElement::Constraint(c) if c.name.contains("CK_"))
    });
    assert!(has_ck, "Model should contain check constraint");
}

// ============================================================================
// SqlInlineConstraintAnnotation Tests
// ============================================================================

#[test]
fn test_column_with_inline_default_has_disambiguator() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL,
    [Status] INT NOT NULL DEFAULT 0
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            if t.name == "T" {
                return Some(t);
            }
        }
        None
    });
    assert!(table.is_some(), "Should find table T");
    let table = table.unwrap();

    // Id column should NOT have attached annotations
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some());
    assert!(
        id_col.unwrap().attached_annotations.is_empty(),
        "Id column should not have inline constraint annotations"
    );

    // Status column SHOULD have attached annotations (has inline DEFAULT)
    let status_col = table.columns.iter().find(|c| c.name == "Status");
    assert!(status_col.is_some());
    assert!(
        !status_col.unwrap().attached_annotations.is_empty(),
        "Status column should have inline constraint annotation due to DEFAULT"
    );
}

#[test]
fn test_column_with_inline_check_has_disambiguator() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL,
    [Age] INT NOT NULL CHECK ([Age] >= 0)
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            if t.name == "T" {
                return Some(t);
            }
        }
        None
    });
    assert!(table.is_some(), "Should find table T");
    let table = table.unwrap();

    // Id column should NOT have attached annotations
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some());
    assert!(
        id_col.unwrap().attached_annotations.is_empty(),
        "Id column should not have inline constraint annotations"
    );

    // Age column SHOULD have attached annotations (has inline CHECK)
    let age_col = table.columns.iter().find(|c| c.name == "Age");
    assert!(age_col.is_some());
    assert!(
        !age_col.unwrap().attached_annotations.is_empty(),
        "Age column should have inline constraint annotation due to CHECK"
    );
}

#[test]
fn test_column_with_inline_primary_key_has_disambiguator() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100)
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            if t.name == "T" {
                return Some(t);
            }
        }
        None
    });
    assert!(table.is_some(), "Should find table T");
    let table = table.unwrap();

    // Id column SHOULD have attached annotations (has inline PRIMARY KEY)
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some());
    assert!(
        !id_col.unwrap().attached_annotations.is_empty(),
        "Id column should have inline constraint annotation due to PRIMARY KEY"
    );

    // Name column should NOT have attached annotations
    let name_col = table.columns.iter().find(|c| c.name == "Name");
    assert!(name_col.is_some());
    assert!(
        name_col.unwrap().attached_annotations.is_empty(),
        "Name column should not have inline constraint annotations"
    );
}

/// Tests that tables with mixed inline + table-level constraints correctly
/// assign annotations. This was a bug where inline constraints with
/// `uses_annotation=false` didn't add to `table_annotation`, causing
/// "AttachedAnnotation referencing nonexistent annotation" deployment errors.
///
/// Scenario: Table with PK (table-level) + DEFAULT (inline named) forms a
/// 2-named-constraint case where both constraints get AttachedAnnotation and
/// the table needs Annotation elements for both.
#[test]
fn test_mixed_inline_and_table_level_named_constraints() {
    let sql = r#"
CREATE TABLE [dbo].[Products] (
    [Id] UNIQUEIDENTIFIER NOT NULL,
    [Version] INT CONSTRAINT [DF_Products_Version] DEFAULT ((0)) NOT NULL,
    CONSTRAINT [PK_Products] PRIMARY KEY CLUSTERED ([Id])
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            if t.name == "Products" {
                return Some(t);
            }
        }
        None
    });
    assert!(table.is_some(), "Should find table Products");
    let table = table.unwrap();

    // For 2-named-constraint tables, the table should have 2 Annotation elements
    // (one for each constraint's disambiguator).
    // This was the bug: inline DEFAULT constraint wasn't adding its disambiguator
    // to table_annotation, so the table only had 1 Annotation when it needed 2.
    assert_eq!(
        table.inline_constraint_disambiguators.len(),
        2,
        "Table should have 2 inline_constraint_disambiguators for 2-named-constraint case. \
         Bug: inline constraint wasn't adding to table_annotation."
    );

    // Verify both constraints exist
    let constraints: Vec<_> = model
        .elements
        .iter()
        .filter_map(|e| {
            if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
                if c.table_name == "Products" {
                    return Some(c.name.clone());
                }
            }
            None
        })
        .collect();

    assert!(
        constraints.iter().any(|n| n.contains("PK_Products")),
        "Should have PK_Products constraint"
    );
    assert!(
        constraints
            .iter()
            .any(|n| n.contains("DF_Products_Version")),
        "Should have DF_Products_Version constraint"
    );
}
