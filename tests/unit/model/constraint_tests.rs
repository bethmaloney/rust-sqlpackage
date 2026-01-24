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
