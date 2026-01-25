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

    // Id column should NOT have inline constraint disambiguator
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some());
    assert!(
        id_col.unwrap().inline_constraint_disambiguator.is_none(),
        "Id column should not have inline constraint annotation"
    );

    // Status column SHOULD have inline constraint disambiguator (has DEFAULT)
    let status_col = table.columns.iter().find(|c| c.name == "Status");
    assert!(status_col.is_some());
    assert!(
        status_col.unwrap().inline_constraint_disambiguator.is_some(),
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

    // Id column should NOT have inline constraint disambiguator
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some());
    assert!(
        id_col.unwrap().inline_constraint_disambiguator.is_none(),
        "Id column should not have inline constraint annotation"
    );

    // Age column SHOULD have inline constraint disambiguator (has CHECK)
    let age_col = table.columns.iter().find(|c| c.name == "Age");
    assert!(age_col.is_some());
    assert!(
        age_col.unwrap().inline_constraint_disambiguator.is_some(),
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

    // Id column SHOULD have inline constraint disambiguator (has inline PRIMARY KEY)
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some());
    assert!(
        id_col.unwrap().inline_constraint_disambiguator.is_some(),
        "Id column should have inline constraint annotation due to PRIMARY KEY"
    );

    // Name column should NOT have inline constraint disambiguator
    let name_col = table.columns.iter().find(|c| c.name == "Name");
    assert!(name_col.is_some());
    assert!(
        name_col.unwrap().inline_constraint_disambiguator.is_none(),
        "Name column should not have inline constraint annotation"
    );
}
