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
        package_references: vec![],
        sqlcmd_variables: vec![],
        project_dir: PathBuf::new(),
        pre_deploy_script: None,
        post_deploy_script: None,
        ansi_nulls: true,
        quoted_identifier: true,
        database_options: rust_sqlpackage::project::DatabaseOptions::default(),
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

#[test]
fn test_script_content_normalizes_crlf_to_lf() {
    // Create SQL content with Windows line endings (CRLF)
    let sql = "CREATE PROCEDURE [dbo].[TestProc]\r\nAS\r\nBEGIN\r\n    SELECT 1;\r\nEND\r\n";
    let xml = generate_model_xml(sql);

    // The generated XML should not contain any CRLF sequences
    assert!(
        !xml.contains("\r\n"),
        "Generated XML should normalize CRLF to LF. Found CRLF in output."
    );

    // Should still contain LF line endings in CDATA
    assert!(
        xml.contains("<![CDATA["),
        "BodyScript should contain CDATA section"
    );

    // Verify the body content is present (with LF endings)
    assert!(
        xml.contains("SELECT 1;"),
        "BodyScript should contain the procedure body"
    );
}

#[test]
fn test_view_query_script_normalizes_crlf() {
    // Create view SQL with Windows line endings
    let sql = "CREATE VIEW [dbo].[TestView]\r\nAS\r\nSELECT\r\n    1 AS Col1,\r\n    2 AS Col2\r\n";
    let xml = generate_model_xml(sql);

    // The generated XML should not contain any CRLF sequences
    assert!(
        !xml.contains("\r\n"),
        "Generated XML should normalize CRLF to LF in QueryScript"
    );

    assert!(
        xml.contains(r#"<Property Name="QueryScript">"#),
        "View should have QueryScript property"
    );
}

// ============================================================================
// SqlInlineConstraintAnnotation Tests
// ============================================================================

#[test]
fn test_column_with_inline_default_has_annotation() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL,
    [Status] INT NOT NULL DEFAULT 0
);
"#;
    let xml = generate_model_xml(sql);

    // The Status column should have SqlInlineConstraintAnnotation due to DEFAULT
    assert!(
        xml.contains("SqlInlineConstraintAnnotation"),
        "Column with inline DEFAULT should have SqlInlineConstraintAnnotation. Got:\n{}",
        xml
    );

    // The annotation should have Disambiguator attribute
    assert!(
        xml.contains(r#"Annotation Type="SqlInlineConstraintAnnotation" Disambiguator="#),
        "SqlInlineConstraintAnnotation should have Disambiguator attribute. Got:\n{}",
        xml
    );
}

#[test]
fn test_column_without_inline_constraint_no_annotation() {
    let sql = r#"
CREATE TABLE [dbo].[T] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100)
);
"#;
    let xml = generate_model_xml(sql);

    // Neither column has inline constraints, so there should be no annotation
    assert!(
        !xml.contains("SqlInlineConstraintAnnotation"),
        "Columns without inline constraints should NOT have SqlInlineConstraintAnnotation. Got:\n{}",
        xml
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

    // DacMetadata.xml uses DacType as root element (per MS XSD schema)
    assert!(
        metadata.contains("<DacType"),
        "Should have DacType root element (per MS schema)"
    );
    assert!(
        metadata.contains("TestProject"),
        "Should contain project name"
    );
    // Empty Description should not be emitted (matches dotnet behavior)
    assert!(
        !metadata.contains("<Description>"),
        "Should not emit empty Description element"
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

#[test]
fn test_origin_xml_element_order() {
    // Per XSD schema: PackageProperties -> Operation -> Server? -> ExportStatistics? -> Checksums
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string("ABCD1234");

    // Verify Operation comes before Checksums (XSD order)
    let operation_pos = origin
        .find("<Operation>")
        .expect("Should have Operation element");
    let checksums_pos = origin
        .find("<Checksums>")
        .expect("Should have Checksums element");
    assert!(
        operation_pos < checksums_pos,
        "Operation should come before Checksums per XSD schema. Got:\n{}",
        origin
    );
}

#[test]
fn test_origin_xml_has_product_name() {
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string("ABCD1234");

    // Should have ProductName element with value "rust-sqlpackage"
    assert!(
        origin.contains("<ProductName>rust-sqlpackage</ProductName>"),
        "Should have ProductName element. Got:\n{}",
        origin
    );
}

#[test]
fn test_origin_xml_has_product_version() {
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string("ABCD1234");

    // Should have ProductVersion element (version from Cargo.toml)
    assert!(
        origin.contains("<ProductVersion>"),
        "Should have ProductVersion element. Got:\n{}",
        origin
    );
    assert!(
        origin.contains("</ProductVersion>"),
        "Should have closing ProductVersion element. Got:\n{}",
        origin
    );
}

#[test]
fn test_origin_xml_product_schema_is_url() {
    let origin = rust_sqlpackage::dacpac::generate_origin_xml_string("ABCD1234");

    // ProductSchema should be a simple URL string, not nested MajorVersion element
    assert!(
        origin.contains("<ProductSchema>http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02</ProductSchema>"),
        "ProductSchema should be a simple URL string (not nested MajorVersion). Got:\n{}",
        origin
    );
    assert!(
        !origin.contains("<MajorVersion"),
        "Should NOT have MajorVersion element (old format). Got:\n{}",
        origin
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
        content_types.contains("text/xml"),
        "Should have XML content type (text/xml to match dotnet)"
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

// ============================================================================
// Native Compilation XML Tests
// ============================================================================

#[test]
fn test_generate_natively_compiled_procedure_has_property() {
    let sql = r#"
CREATE PROCEDURE [dbo].[NativeProc]
    @Id INT
WITH NATIVE_COMPILATION, SCHEMABINDING
AS
BEGIN ATOMIC WITH (TRANSACTION ISOLATION LEVEL = SNAPSHOT, LANGUAGE = N'English')
    SELECT [Id] FROM [dbo].[MemTable] WHERE [Id] = @Id;
END
"#;
    let xml = generate_model_xml(sql);

    // Should have IsNativelyCompiled property
    assert!(
        xml.contains(r#"<Property Name="IsNativelyCompiled" Value="True"/>"#),
        "Natively compiled procedure should have IsNativelyCompiled=True property. Got:\n{}",
        xml
    );

    // Should be a SqlProcedure element
    assert!(
        xml.contains("Type=\"SqlProcedure\""),
        "Should have SqlProcedure element type"
    );
}

#[test]
fn test_generate_regular_procedure_no_native_property() {
    let sql = r#"
CREATE PROCEDURE [dbo].[RegularProc]
AS
BEGIN
    SELECT 1;
END
"#;
    let xml = generate_model_xml(sql);

    // Should NOT have IsNativelyCompiled property
    assert!(
        !xml.contains("IsNativelyCompiled"),
        "Regular procedure should NOT have IsNativelyCompiled property. Got:\n{}",
        xml
    );
}

#[test]
fn test_generate_natively_compiled_function_has_property() {
    let sql = r#"
CREATE FUNCTION [dbo].[NativeFunc]
(
    @Value INT
)
RETURNS INT
WITH NATIVE_COMPILATION, SCHEMABINDING
AS
BEGIN ATOMIC WITH (TRANSACTION ISOLATION LEVEL = SNAPSHOT, LANGUAGE = N'English')
    RETURN @Value * 2;
END
"#;
    let xml = generate_model_xml(sql);

    // Should have IsNativelyCompiled property
    assert!(
        xml.contains(r#"<Property Name="IsNativelyCompiled" Value="True"/>"#),
        "Natively compiled function should have IsNativelyCompiled=True property. Got:\n{}",
        xml
    );

    // Should be a SqlScalarFunction element
    assert!(
        xml.contains("Type=\"SqlScalarFunction\""),
        "Should have SqlScalarFunction element type"
    );
}

#[test]
fn test_generate_regular_function_no_native_property() {
    let sql = r#"
CREATE FUNCTION [dbo].[RegularFunc]()
RETURNS INT
AS
BEGIN
    RETURN 1;
END
"#;
    let xml = generate_model_xml(sql);

    // Should NOT have IsNativelyCompiled property
    assert!(
        !xml.contains("IsNativelyCompiled"),
        "Regular function should NOT have IsNativelyCompiled property. Got:\n{}",
        xml
    );
}

// ============================================================================
// FILESTREAM Column XML Tests
// ============================================================================

#[test]
fn test_generate_filestream_column_has_property() {
    let sql = r#"
CREATE TABLE [dbo].[Documents] (
    [Id] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL,
    [FileData] VARBINARY(MAX) FILESTREAM NULL
);
"#;
    let xml = generate_model_xml(sql);

    // Should have IsFileStream property for the FILESTREAM column
    assert!(
        xml.contains(r#"<Property Name="IsFileStream" Value="True"/>"#),
        "FILESTREAM column should have IsFileStream=True property. Got:\n{}",
        xml
    );
}

#[test]
fn test_generate_regular_varbinary_no_filestream_property() {
    let sql = r#"
CREATE TABLE [dbo].[RegularBinary] (
    [Id] INT NOT NULL,
    [Data] VARBINARY(MAX) NULL
);
"#;
    let xml = generate_model_xml(sql);

    // Should NOT have IsFileStream property for regular VARBINARY(MAX)
    assert!(
        !xml.contains("IsFileStream"),
        "Regular VARBINARY(MAX) should NOT have IsFileStream property. Got:\n{}",
        xml
    );
}

#[test]
fn test_generate_filestream_column_structure() {
    let sql = r#"
CREATE TABLE [dbo].[FileArchive] (
    [FileId] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL,
    [Content] VARBINARY(MAX) FILESTREAM NOT NULL
);
"#;
    let xml = generate_model_xml(sql);

    // Verify overall structure
    assert!(xml.contains("SqlTable"), "Should have SqlTable element");
    assert!(
        xml.contains("SqlSimpleColumn"),
        "Should have SqlSimpleColumn elements"
    );
    assert!(
        xml.contains(r#"<Property Name="IsFileStream" Value="True"/>"#),
        "FILESTREAM column should have IsFileStream property"
    );
    // Verify data type reference
    assert!(
        xml.contains("Name=\"[varbinary]\""),
        "Should reference varbinary type"
    );
}

#[test]
fn test_generate_multiple_filestream_columns() {
    let sql = r#"
CREATE TABLE [dbo].[MediaFiles] (
    [Id] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL,
    [Thumbnail] VARBINARY(MAX) FILESTREAM NULL,
    [FullSize] VARBINARY(MAX) FILESTREAM NULL,
    [Name] NVARCHAR(100) NOT NULL
);
"#;
    let xml = generate_model_xml(sql);

    // Count IsFileStream properties - should be 2
    let filestream_count = xml
        .matches(r#"<Property Name="IsFileStream" Value="True"/>"#)
        .count();
    assert!(
        filestream_count == 2,
        "Should have exactly 2 IsFileStream=True properties for 2 FILESTREAM columns. Got: {}",
        filestream_count
    );
}

// ============================================================================
// Scalar Function Return Type Tests
// ============================================================================

#[test]
fn test_scalar_function_has_type_relationship() {
    // Scalar functions must have a Type relationship containing SqlTypeSpecifier
    // that references the return type
    let sql = r#"
CREATE FUNCTION [dbo].[GetValue]()
RETURNS INT
AS
BEGIN
    RETURN 1;
END
"#;
    let xml = generate_model_xml(sql);

    // Must have Type relationship
    assert!(
        xml.contains(r#"<Relationship Name="Type">"#),
        "Scalar function should have Type relationship for return type. Got:\n{}",
        xml
    );

    // Type relationship must contain SqlTypeSpecifier element
    assert!(
        xml.contains(r#"Type="SqlTypeSpecifier""#),
        "Type relationship should contain SqlTypeSpecifier element. Got:\n{}",
        xml
    );

    // SqlTypeSpecifier must reference the int type
    assert!(
        xml.contains(r#"Name="[int]""#),
        "SqlTypeSpecifier should reference [int] type. Got:\n{}",
        xml
    );
}

#[test]
fn test_scalar_function_decimal_return_type() {
    // Test that DECIMAL(18,2) return type is properly represented
    let sql = r#"
CREATE FUNCTION [Sales].[GetOrderTotal](@OrderId INT)
RETURNS DECIMAL(18, 2)
AS
BEGIN
    RETURN 100.00;
END
"#;
    let xml = generate_model_xml(sql);

    // Must have Type relationship with SqlTypeSpecifier
    assert!(
        xml.contains(r#"<Relationship Name="Type">"#),
        "Scalar function should have Type relationship. Got:\n{}",
        xml
    );
    assert!(
        xml.contains(r#"Type="SqlTypeSpecifier""#),
        "Should have SqlTypeSpecifier element. Got:\n{}",
        xml
    );

    // Should reference decimal type
    assert!(
        xml.contains(r#"Name="[decimal]""#),
        "SqlTypeSpecifier should reference [decimal] type. Got:\n{}",
        xml
    );
}

#[test]
fn test_scalar_function_body_script_excludes_header() {
    // BodyScript should contain only the function body (BEGIN...END),
    // not the RETURNS clause or parameters
    let sql = r#"
CREATE FUNCTION [dbo].[GetValue]()
RETURNS INT
AS
BEGIN
    RETURN 1;
END
"#;
    let xml = generate_model_xml(sql);

    // Extract the BodyScript CDATA content to check it specifically
    // BodyScript should start with BEGIN, not with parameters or RETURNS
    assert!(
        xml.contains("<![CDATA[BEGIN"),
        "BodyScript should start with BEGIN (not parameters or RETURNS). Got:\n{}",
        xml
    );

    // BodyScript should NOT contain RETURNS in the CDATA section
    // (RETURNS is allowed in HeaderContents, just not in BodyScript)
    let cdata_start = xml.find("<![CDATA[").unwrap();
    let cdata_end = xml.find("]]>").unwrap();
    let body_script = &xml[cdata_start..cdata_end];
    assert!(
        !body_script.contains("RETURNS INT"),
        "BodyScript CDATA should not contain RETURNS clause. Got:\n{}",
        body_script
    );

    // BodyScript SHOULD contain BEGIN...END
    assert!(
        xml.contains("BEGIN") && xml.contains("RETURN 1") && xml.contains("END"),
        "BodyScript should contain function body. Got:\n{}",
        xml
    );
}

#[test]
fn test_scalar_function_has_header_annotation() {
    // Scalar functions should have SysCommentsObjectAnnotation with HeaderContents
    let sql = r#"
CREATE FUNCTION [dbo].[GetValue]()
RETURNS INT
AS
BEGIN
    RETURN 1;
END
"#;
    let xml = generate_model_xml(sql);

    // Should have SysCommentsObjectAnnotation
    assert!(
        xml.contains(r#"<Annotation Type="SysCommentsObjectAnnotation">"#),
        "Scalar function should have SysCommentsObjectAnnotation. Got:\n{}",
        xml
    );

    // Should have HeaderContents property
    assert!(
        xml.contains(r#"<Property Name="HeaderContents""#),
        "Annotation should have HeaderContents property. Got:\n{}",
        xml
    );
}

#[test]
fn test_scalar_function_header_ends_with_whitespace() {
    // The header must end with whitespace (newline) after AS so that when
    // SqlPackage concatenates header + body, we get "AS\nBEGIN" not "ASBEGIN"
    let sql = r#"
CREATE FUNCTION [dbo].[GetValue]()
RETURNS INT
AS
BEGIN
    RETURN 1;
END
"#;
    let xml = generate_model_xml(sql);

    // Extract HeaderContents value
    // The pattern is: <Property Name="HeaderContents" Value="..."/>
    let header_start = xml
        .find(r#"<Property Name="HeaderContents" Value=""#)
        .expect("Should have HeaderContents property");
    let value_start = header_start + r#"<Property Name="HeaderContents" Value=""#.len();
    let value_end = xml[value_start..]
        .find(r#""/>"#)
        .expect("Should find end of HeaderContents");
    let header_value = &xml[value_start..value_start + value_end];

    // Header should end with newline (encoded as &#xD;&#xA; or &#xA; in XML)
    // or at minimum should end with AS followed by whitespace
    assert!(
        header_value.ends_with("&#xA;")
            || header_value.ends_with("&#xD;&#xA;")
            || header_value.ends_with("\n"),
        "HeaderContents should end with newline after AS to prevent ASBEGIN. Got: {:?}",
        header_value
    );

    // Also verify the header contains "AS" near the end (not trimmed off)
    assert!(
        header_value.contains("AS"),
        "HeaderContents should contain AS keyword. Got: {:?}",
        header_value
    );
}

#[test]
fn test_scalar_function_has_ansi_nulls_property() {
    // Scalar functions should have IsAnsiNullsOn property
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
        xml.contains(r#"<Property Name="IsAnsiNullsOn" Value="True"/>"#),
        "Scalar function should have IsAnsiNullsOn property. Got:\n{}",
        xml
    );
}

#[test]
fn test_scalar_function_varchar_return_type() {
    // Test VARCHAR return type
    let sql = r#"
CREATE FUNCTION [dbo].[GetName]()
RETURNS VARCHAR(100)
AS
BEGIN
    RETURN 'Test';
END
"#;
    let xml = generate_model_xml(sql);

    assert!(
        xml.contains(r#"<Relationship Name="Type">"#),
        "Should have Type relationship. Got:\n{}",
        xml
    );
    assert!(
        xml.contains(r#"Name="[varchar]""#),
        "Should reference [varchar] type. Got:\n{}",
        xml
    );
}
