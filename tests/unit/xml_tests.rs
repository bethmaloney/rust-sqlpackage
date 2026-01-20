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

    assert!(xml.contains("<Model>"), "XML should have Model element");
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
    // Use a custom schema (not dbo) since built-in schemas like dbo don't generate
    // SqlSchema elements - they're referenced with ExternalSource="BuiltIns"
    let sql = "CREATE TABLE [sales].[T] ([Id] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlSchema\""),
        "XML should have SqlSchema element type"
    );
    assert!(
        xml.contains("[sales]"),
        "XML should contain the custom schema name"
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
    assert!(xml.contains("TestTable"), "XML should contain table name");
}

#[test]
fn test_generate_column_element() {
    let sql = "CREATE TABLE [dbo].[T] ([MyColumn] INT NOT NULL);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlSimpleColumn\""),
        "XML should have SqlSimpleColumn element type"
    );
    assert!(xml.contains("MyColumn"), "XML should contain column name");
}

#[test]
fn test_generate_view_element() {
    let sql = "CREATE VIEW [dbo].[TestView] AS SELECT 1 AS [Value];";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlView\""),
        "XML should have SqlView element type"
    );
    assert!(xml.contains("TestView"), "XML should contain view name");
}

#[test]
fn test_generate_index_element() {
    let sql = "CREATE INDEX [IX_Test] ON [dbo].[T] ([Col1]);";
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains("Type=\"SqlIndex\""),
        "XML should have SqlIndex element type"
    );
    assert!(xml.contains("IX_Test"), "XML should contain index name");
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
    assert!(xml.contains("GetValue"), "XML should contain function name");
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
// Script Property Format Tests (QueryScript, BodyScript, etc.)
// ============================================================================

#[test]
fn test_view_has_query_script_property() {
    // Views should use QueryScript property with CDATA, not SqlInlineConstraintAnnotation
    let sql = r#"
CREATE VIEW [dbo].[TestView]
AS
SELECT 1 AS Value;
"#;
    let xml = generate_model_xml(sql);

    // Verify view has QueryScript property
    assert!(
        xml.contains(r#"<Property Name="QueryScript">"#),
        "View should have QueryScript property. Got:\n{}",
        xml
    );

    // Verify QueryScript contains the view definition in CDATA
    assert!(
        xml.contains("<![CDATA["),
        "QueryScript should contain CDATA section. Got:\n{}",
        xml
    );

    // Verify we're NOT using the old SqlInlineConstraintAnnotation format
    assert!(
        !xml.contains("SqlInlineConstraintAnnotation"),
        "View should NOT use SqlInlineConstraintAnnotation"
    );
}

#[test]
fn test_procedure_has_body_script_property() {
    let sql = r#"
CREATE PROCEDURE [dbo].[TestProc]
AS
BEGIN
    SELECT 1;
END
"#;
    let xml = generate_model_xml(sql);

    // Procedures should have BodyScript property with CDATA
    assert!(
        xml.contains(r#"<Property Name="BodyScript">"#),
        "Procedure should have BodyScript property. Got:\n{}",
        xml
    );

    // Should have CDATA section
    assert!(
        xml.contains("<![CDATA["),
        "BodyScript should contain CDATA section"
    );

    // Should NOT use old annotation format
    assert!(
        !xml.contains("SqlInlineConstraintAnnotation"),
        "Procedure should NOT use SqlInlineConstraintAnnotation"
    );
}

#[test]
fn test_function_has_body_script_property() {
    let sql = r#"
CREATE FUNCTION [dbo].[GetOne]()
RETURNS INT
AS
BEGIN
    RETURN 1;
END
"#;
    let xml = generate_model_xml(sql);

    // Functions should have BodyScript property with CDATA
    assert!(
        xml.contains(r#"<Property Name="BodyScript">"#),
        "Function should have BodyScript property. Got:\n{}",
        xml
    );

    // Should NOT use old annotation format
    assert!(
        !xml.contains("SqlInlineConstraintAnnotation"),
        "Function should NOT use SqlInlineConstraintAnnotation"
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
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string("ABCD1234");

    assert!(
        origin.contains("<DacOrigin"),
        "Should have DacOrigin root element"
    );
    assert!(
        origin.contains("</DacOrigin>"),
        "Should have closing DacOrigin element"
    );
}

#[test]
fn test_origin_xml_version_format() {
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string("ABCD1234");

    // Version should be 3.1.0.0 for DacFx compatibility
    assert!(
        origin.contains("<Version>3.1.0.0</Version>"),
        "Should have Version 3.1.0.0 for DacFx compatibility. Got:\n{}",
        origin
    );
}

#[test]
fn test_origin_xml_stream_versions_format() {
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string("ABCD1234");

    // StreamVersions should have nested Version elements with StreamName attributes
    assert!(
        origin.contains("<StreamVersions>"),
        "Should have StreamVersions element"
    );
    assert!(
        origin.contains(r#"<Version StreamName="Data">2.0.0.0</Version>"#),
        "Should have Data stream version. Got:\n{}",
        origin
    );
    assert!(
        origin.contains(r#"<Version StreamName="DeploymentContributors">1.0.0.0</Version>"#),
        "Should have DeploymentContributors stream version. Got:\n{}",
        origin
    );
}

#[test]
fn test_origin_xml_checksum_format() {
    let checksum = "18AE866BF3C8C1B7729EB146103A79679CB967AF8D5A3F3D5C7DA8E029DE66D3";
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string(checksum);

    // Checksum should be in Checksums section with Uri attribute
    assert!(
        origin.contains("<Checksums>"),
        "Should have Checksums element"
    );
    assert!(
        origin.contains(r#"<Checksum Uri="/model.xml">"#),
        "Should have Checksum element with Uri attribute. Got:\n{}",
        origin
    );
    assert!(
        origin.contains(checksum),
        "Should contain the actual checksum value"
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
    let open_count =
        xml.matches('<').count() - xml.matches("</").count() - xml.matches("/>").count();
    let close_count = xml.matches("</").count();

    // Allow some tolerance for self-closing tags
    assert!(
        (open_count as i32 - close_count as i32).abs() <= 10,
        "XML tags should roughly balance"
    );
}

// ============================================================================
// Index INCLUDE Columns Tests
// ============================================================================

#[test]
fn test_generate_index_with_include_columns() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_WithInclude]
ON [dbo].[T] ([KeyCol])
INCLUDE ([IncludeCol1], [IncludeCol2]);
"#;
    let xml = generate_model_xml(sql);

    // Should have IncludedColumns relationship
    assert!(
        xml.contains("IncludedColumns"),
        "XML should have IncludedColumns relationship: {xml}"
    );

    // Should reference the included columns
    assert!(
        xml.contains("IncludeCol1"),
        "XML should reference IncludeCol1: {xml}"
    );
    assert!(
        xml.contains("IncludeCol2"),
        "XML should reference IncludeCol2: {xml}"
    );
}

#[test]
fn test_generate_index_include_columns_format() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_Users_Email]
ON [dbo].[Users] ([Email])
INCLUDE ([FirstName], [LastName]);
"#;
    let xml = generate_model_xml(sql);

    // IncludedColumns should be a Relationship with Name attribute
    assert!(
        xml.contains("Relationship Name=\"IncludedColumns\""),
        "IncludedColumns should use Relationship Name attribute format: {xml}"
    );
}

#[test]
fn test_generate_index_without_include_columns() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_NoInclude]
ON [dbo].[T] ([Column1]);
"#;
    let xml = generate_model_xml(sql);

    // Should NOT have IncludedColumns relationship when no INCLUDE clause
    assert!(
        !xml.contains("IncludedColumns"),
        "XML should NOT have IncludedColumns when index has no INCLUDE clause: {xml}"
    );
}

#[test]
fn test_generate_index_column_specifications() {
    let sql = r#"
CREATE NONCLUSTERED INDEX [IX_T_Multi]
ON [dbo].[T] ([Col1], [Col2])
INCLUDE ([Col3]);
"#;
    let xml = generate_model_xml(sql);

    // Should have ColumnSpecifications relationship for key columns
    assert!(
        xml.contains("ColumnSpecifications"),
        "XML should have ColumnSpecifications relationship: {xml}"
    );

    // Should have SqlIndexedColumnSpecification elements
    assert!(
        xml.contains("SqlIndexedColumnSpecification"),
        "XML should have SqlIndexedColumnSpecification elements: {xml}"
    );
}

#[test]
fn test_generate_index_structure_complete() {
    let sql = r#"
CREATE UNIQUE NONCLUSTERED INDEX [IX_Orders_CustomerDate]
ON [dbo].[Orders] ([CustomerId], [OrderDate] DESC)
INCLUDE ([TotalAmount], [Status]);
"#;
    let xml = generate_model_xml(sql);

    // Verify all expected elements are present
    assert!(xml.contains("SqlIndex"), "XML should have SqlIndex element");
    assert!(
        xml.contains("IsUnique"),
        "XML should have IsUnique property for unique index"
    );
    assert!(
        xml.contains("IndexedObject"),
        "XML should have IndexedObject relationship"
    );
    assert!(
        xml.contains("ColumnSpecifications"),
        "XML should have ColumnSpecifications"
    );
    assert!(
        xml.contains("IncludedColumns"),
        "XML should have IncludedColumns"
    );
}
