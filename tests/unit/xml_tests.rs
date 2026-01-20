//! Unit tests for XML generation
//!
//! These tests verify the correctness of model.xml, DacMetadata.xml,
//! Origin.xml, and [Content_Types].xml generation.

use std::io::Write;
use std::path::PathBuf;

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
        sql_files: vec![],
        dacpac_references: vec![],
        project_dir: PathBuf::new(),
        pre_deploy_script: None,
        post_deploy_script: None,
    }
}

/// Helper to parse SQL, build model, and generate model XML
fn generate_model_xml(sql: &str) -> String {
    let file = create_sql_file(sql);
    let statements = rust_sqlpackage::parser::parse_sql_file(file.path()).unwrap();
    let project = create_test_project();
    let model = rust_sqlpackage::model::build_model(&statements, &project).unwrap();

    rust_sqlpackage::dacpac::generate_model_xml_string(
        &model,
        rust_sqlpackage::project::SqlServerVersion::Sql160,
        1033,
        false,
    )
}

// ============================================================================
// model.xml Structure Tests
// ============================================================================

#[test]
fn test_generate_data_schema_model_root() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("<DataSchemaModel"),
        "XML should have DataSchemaModel root element"
    );
    assert!(
        xml.contains("</DataSchemaModel>"),
        "XML should have closing DataSchemaModel tag"
    );
}

#[test]
fn test_generate_file_format_version() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("FileFormatVersion="),
        "XML should have FileFormatVersion attribute"
    );
}

#[test]
fn test_generate_schema_version() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("SchemaVersion="),
        "XML should have SchemaVersion attribute"
    );
}

#[test]
fn test_generate_dsp_name() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("DspName="),
        "XML should have DspName attribute"
    );
    assert!(
        xml.contains("Sql160DatabaseSchemaProvider"),
        "DspName should be Sql160 provider"
    );
}

#[test]
fn test_generate_collation_attributes() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("CollationLcid="),
        "XML should have CollationLcid attribute"
    );
}

#[test]
fn test_generate_model_element() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("<Model>"),
        "XML should have Model element"
    );
    assert!(
        xml.contains("</Model>"),
        "XML should have closing Model tag"
    );
}

// ============================================================================
// Element Generation Tests
// ============================================================================

#[test]
fn test_generate_schema_element() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlSchema\""),
        "XML should have SqlSchema element type"
    );
}

#[test]
fn test_generate_table_element() {
    let sql = "CREATE TABLE [dbo].[TestTable] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlTable\""),
        "XML should have SqlTable element type"
    );
    assert!(
        xml.contains("TestTable"),
        "XML should contain table name"
    );
}

#[test]
fn test_generate_column_element() {
    let sql = "CREATE TABLE [dbo].[T] ([MyColumn] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlSimpleColumn\""),
        "XML should have SqlSimpleColumn element type"
    );
    assert!(
        xml.contains("MyColumn"),
        "XML should contain column name"
    );
}

#[test]
fn test_generate_view_element() {
    let sql = "CREATE VIEW [dbo].[TestView] AS SELECT 1 AS [Value];";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlView\""),
        "XML should have SqlView element type"
    );
    assert!(
        xml.contains("TestView"),
        "XML should contain view name"
    );
}

#[test]
fn test_generate_index_element() {
    let sql = "CREATE INDEX [IX_Test] ON [dbo].[T] ([Col1]);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlIndex\""),
        "XML should have SqlIndex element type"
    );
    assert!(
        xml.contains("IX_Test"),
        "XML should contain index name"
    );
}

#[test]
fn test_generate_primary_key_constraint_element() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL,
    CONSTRAINT [PK_T] PRIMARY KEY ([Id])
);
"#;
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlPrimaryKeyConstraint\""),
        "XML should have SqlPrimaryKeyConstraint element type"
    );
}

#[test]
fn test_generate_foreign_key_constraint_element() {
    let sql = r#"
CREATE TABLE [dbo].[Parent] ([Id] INT NOT NULL PRIMARY KEY);
GO
CREATE TABLE [dbo].[Child] (
    [Id] INT NOT NULL PRIMARY KEY,
    [ParentId] INT NOT NULL,
    CONSTRAINT [FK_Child_Parent] FOREIGN KEY ([ParentId]) REFERENCES [dbo].[Parent]([Id])
);
"#;
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlForeignKeyConstraint\""),
        "XML should have SqlForeignKeyConstraint element type"
    );
}

#[test]
fn test_generate_unique_constraint_element() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Email] NVARCHAR(255) NOT NULL,
    CONSTRAINT [UQ_T_Email] UNIQUE ([Email])
);
"#;
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlUniqueConstraint\""),
        "XML should have SqlUniqueConstraint element type"
    );
}

#[test]
fn test_generate_procedure_element() {
    let sql = r#"
CREATE PROCEDURE [dbo].[TestProc]
AS
BEGIN
    SELECT 1;
END
"#;
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlProcedure\""),
        "XML should have SqlProcedure element type"
    );
    assert!(
        xml.contains("TestProc"),
        "XML should contain procedure name"
    );
}

#[test]
fn test_generate_scalar_function_element() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetValue]()
RETURNS INT
AS
BEGIN
    RETURN 1;
END
"#;
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlScalarFunction\""),
        "XML should have SqlScalarFunction element type"
    );
    assert!(
        xml.contains("GetValue"),
        "XML should contain function name"
    );
}

#[test]
fn test_generate_check_constraint_element() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Age] INT NOT NULL,
    CONSTRAINT [CK_T_Age] CHECK ([Age] >= 0)
);
"#;
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlCheckConstraint\""),
        "XML should have SqlCheckConstraint element type"
    );
}

// ============================================================================
// Relationship Generation Tests
// ============================================================================

#[test]
fn test_generate_schema_relationship() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    // Tables should have a relationship to their schema
    assert!(
        xml.contains("<Relationship") || xml.contains("Relationship"),
        "XML should have Relationship elements"
    );
}

#[test]
fn test_relationship_name_as_attribute() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    // Name should be an attribute on Relationship, not a child Attribute element
    assert!(
        xml.contains("Relationship Name=\"Schema\""),
        "Relationship should have Name as attribute: {xml}"
    );
    assert!(
        xml.contains("Relationship Name=\"Columns\""),
        "Columns relationship should have Name as attribute: {xml}"
    );
    // Should NOT have the old format with child Attribute element
    assert!(
        !xml.contains("<Attribute Name=\"Name\" Value=\"Schema\"/>"),
        "Should not use child Attribute for relationship name"
    );
}

#[test]
fn test_builtin_type_has_external_source() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    // Built-in types should have ExternalSource="BuiltIns" attribute
    assert!(
        xml.contains("ExternalSource=\"BuiltIns\""),
        "Built-in type references should have ExternalSource attribute: {xml}"
    );
    assert!(
        xml.contains("References ExternalSource=\"BuiltIns\" Name=\"[int]\""),
        "INT type should reference BuiltIns: {xml}"
    );
}

#[test]
fn test_generate_columns_relationship() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL, [Name] NVARCHAR(100) NULL);";
    let xml = generate_model_xml(sql);

    // Should have column definitions
    assert!(
        xml.contains("SqlSimpleColumn"),
        "XML should have column elements in relationship"
    );
}

// ============================================================================
// Data Type Tests
// ============================================================================

#[test]
fn test_generate_varchar_type() {
    let sql = "CREATE TABLE [dbo].[T] ([Name] VARCHAR(100) NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Name=\"[varchar]\""),
        "XML should reference varchar type: {xml}"
    );
}

#[test]
fn test_generate_nvarchar_type() {
    let sql = "CREATE TABLE [dbo].[T] ([Name] NVARCHAR(255) NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Name=\"[nvarchar]\""),
        "XML should reference nvarchar type: {xml}"
    );
}

#[test]
fn test_generate_decimal_type() {
    let sql = "CREATE TABLE [dbo].[T] ([Amount] DECIMAL(18,2) NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Name=\"[decimal]\""),
        "XML should reference decimal type: {xml}"
    );
}

#[test]
fn test_generate_datetime_type() {
    let sql = "CREATE TABLE [dbo].[T] ([Created] DATETIME NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Name=\"[datetime]\""),
        "XML should reference datetime type: {xml}"
    );
}

#[test]
fn test_generate_uniqueidentifier_type() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] UNIQUEIDENTIFIER NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Name=\"[uniqueidentifier]\""),
        "XML should reference uniqueidentifier type: {xml}"
    );
}

// ============================================================================
// Property Generation Tests
// ============================================================================

#[test]
fn test_generate_isnullable_property() {
    let sql = "CREATE TABLE [dbo].[T] ([NullCol] INT NULL, [NotNullCol] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("IsNullable"),
        "XML should have IsNullable property"
    );
}

#[test]
fn test_generate_isclustered_property() {
    let sql = "CREATE CLUSTERED INDEX [IX_Clustered] ON [dbo].[T] ([Col1]);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("IsClustered"),
        "XML should have IsClustered property"
    );
}

// ============================================================================
// XML Namespace Tests
// ============================================================================

#[test]
fn test_model_xml_has_correct_namespace() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("http://schemas.microsoft.com/sqlserver/dac/Serialization"),
        "XML should have correct Microsoft namespace"
    );
}

// ============================================================================
// DacMetadata.xml Tests
// ============================================================================

#[test]
fn test_generate_dac_metadata() {
    let metadata = rust_sqlpackage::dacpac::generate_dac_metadata_xml("TestProject", "1.0.0.0");

    assert!(
        metadata.contains("<DacMetadata"),
        "Should have DacMetadata root element"
    );
    assert!(
        metadata.contains("TestProject"),
        "Should contain project name"
    );
}

#[test]
fn test_generate_dac_metadata_version() {
    let metadata = rust_sqlpackage::dacpac::generate_dac_metadata_xml("TestProject", "2.1.0.0");

    assert!(
        metadata.contains("2.1.0.0"),
        "Should contain version number"
    );
}

// ============================================================================
// Origin.xml Tests
// ============================================================================

#[test]
fn test_generate_origin_xml() {
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string("checksum123");

    assert!(
        origin.contains("<Origin") || origin.contains("Origin"),
        "Should have Origin element"
    );
}

#[test]
fn test_generate_origin_checksum() {
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string("abc123checksum");

    assert!(
        origin.contains("abc123checksum") || origin.contains("Checksum"),
        "Should contain checksum information"
    );
}

// ============================================================================
// [Content_Types].xml Tests
// ============================================================================

#[test]
fn test_generate_content_types() {
    let content_types = rust_sqlpackage::dacpac::generate_content_types_xml();

    assert!(
        content_types.contains("<Types"),
        "Should have Types root element"
    );
    assert!(
        content_types.contains("application/xml"),
        "Should have XML content type"
    );
}

// ============================================================================
// Full Workflow XML Tests
// ============================================================================

#[test]
fn test_complete_model_xml_structure() {
    let sql = r#"
CREATE TABLE [dbo].[Users] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    [Email] NVARCHAR(255) NOT NULL,
    CONSTRAINT [PK_Users] PRIMARY KEY ([Id]),
    CONSTRAINT [UQ_Users_Email] UNIQUE ([Email])
);
GO
CREATE VIEW [dbo].[ActiveUsers]
AS
SELECT [Id], [Name], [Email]
FROM [dbo].[Users];
GO
CREATE INDEX [IX_Users_Name]
ON [dbo].[Users] ([Name]);
"#;
    let xml = generate_model_xml(sql);

    // Verify overall structure
    assert!(xml.contains("<DataSchemaModel"));
    assert!(xml.contains("<Model>"));
    assert!(xml.contains("</Model>"));
    assert!(xml.contains("</DataSchemaModel>"));

    // Verify elements
    assert!(xml.contains("SqlTable"));
    assert!(xml.contains("SqlView"));
    assert!(xml.contains("SqlIndex"));
    assert!(xml.contains("SqlPrimaryKeyConstraint"));
    assert!(xml.contains("SqlUniqueConstraint"));
    assert!(xml.contains("SqlSimpleColumn"));

    // Verify names are present
    assert!(xml.contains("Users"));
    assert!(xml.contains("ActiveUsers"));
    assert!(xml.contains("IX_Users_Name"));
}

#[test]
fn test_xml_is_well_formed() {
    let sql = "CREATE TABLE [dbo].[T] ([Id] INT NOT NULL PRIMARY KEY);";
    let xml = generate_model_xml(sql);

    // Basic well-formedness checks
    assert!(xml.starts_with("<?xml") || xml.starts_with("<DataSchemaModel"));

    // Count opening and closing tags roughly match
    let open_count = xml.matches('<').count() - xml.matches("</").count() - xml.matches("/>").count();
    let close_count = xml.matches("</").count();

    // Allow some tolerance for self-closing tags
    assert!(
        (open_count as i32 - close_count as i32).abs() <= 10,
        "XML tags should roughly balance"
    );
}
