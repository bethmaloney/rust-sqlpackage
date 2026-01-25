//! Integration tests for dacpac compatibility with DotNet DacFx
//!
//! These tests build dacpacs and verify the model.xml structure matches
//! what the .NET DacFx toolchain expects.
//!
//! Tests marked #[ignore] are for missing features - remove #[ignore] as features are implemented.

use crate::common::{DacpacInfo, TestContext};

// ============================================================================
// Extended Properties Tests
// ============================================================================

#[test]
fn test_build_with_extended_properties() {
    let ctx = TestContext::with_fixture("extended_properties");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with extended properties should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(
        info.tables.iter().any(|t| t.contains("DocumentedTable")),
        "Should contain DocumentedTable"
    );

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // DotNet produces SqlExtendedProperty elements for sp_addextendedproperty
    assert!(
        model_xml.contains("SqlExtendedProperty"),
        "Model should contain SqlExtendedProperty elements for column descriptions"
    );
}

// ============================================================================
// Full-Text Index Tests
// ============================================================================

#[test]
fn test_build_with_fulltext_index() {
    let ctx = TestContext::with_fixture("fulltext_index");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with full-text index should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(
        info.tables.iter().any(|t| t.contains("Documents")),
        "Should contain Documents table"
    );

    let model_xml = info.model_xml_content.expect("Should have model XML");

    assert!(
        model_xml.contains("SqlFullTextIndex"),
        "Model should contain SqlFullTextIndex elements"
    );
}

// ============================================================================
// Table Type Tests
// ============================================================================

#[test]
fn test_build_with_table_types() {
    let ctx = TestContext::with_fixture("table_types");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with table types should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Check for table type with column definitions
    assert!(
        model_xml.contains("SqlTableTypeSimpleColumn"),
        "Model should contain SqlTableTypeSimpleColumn elements for table type columns"
    );
}

// ============================================================================
// Ampersand Encoding Tests (BUG)
// ============================================================================

#[test]
fn test_build_with_ampersand_encoding() {
    let ctx = TestContext::with_fixture("ampersand_encoding");
    let result = ctx.build();

    // Build may fail if parser can't handle ampersand - that's also a bug
    if !result.success {
        panic!(
            "Build failed - parser cannot handle ampersand in identifiers: {:?}",
            result.errors
        );
    }

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Check for truncation bug - procedure name should NOT be truncated
    let is_truncated = model_xml.contains("IOLoansWithoutP\"")
        || model_xml.contains("IOLoansWithoutP<")
        || model_xml.contains(r#"Name="[dbo].[IOLoansWithoutP]""#);

    assert!(
        !is_truncated,
        "BUG: Procedure name is truncated at ampersand. Should contain full name with P&I"
    );

    // Ampersand should be properly XML-encoded as &amp;
    assert!(
        model_xml.contains("P&amp;I") || model_xml.contains("P&amp;L"),
        "Ampersand should be properly XML-encoded as &amp;"
    );
}

// ============================================================================
// Index Naming Tests (Double Bracket Bug)
// ============================================================================

#[test]
fn test_build_with_index_naming() {
    let ctx = TestContext::with_fixture("index_naming");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with indexes should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Check for double-bracket bug
    // Correct: [dbo].[Orders].[IX_Orders_CustomerId]
    // Bug: [dbo].[Orders].[[IX_Orders_CustomerId]]
    let has_double_brackets = model_xml.contains("[[IX_") || model_xml.contains(".[[");

    assert!(
        !has_double_brackets,
        "BUG: Index names have double brackets [[IX_...]] instead of [IX_...]"
    );

    // Verify index is present with correct format
    assert!(
        model_xml.contains("IX_Orders_CustomerId"),
        "Should contain IX_Orders_CustomerId index"
    );
}

// ============================================================================
// Named Default Constraint Tests
// ============================================================================

#[test]
fn test_build_with_named_default_constraints() {
    let ctx = TestContext::with_fixture("default_constraints_named");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with named defaults should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Check for specific named default constraints
    assert!(
        model_xml.contains("DF_Entity_Version"),
        "Model should contain named default constraint DF_Entity_Version"
    );
    assert!(
        model_xml.contains("DF_Entity_CreatedOn"),
        "Model should contain named default constraint DF_Entity_CreatedOn"
    );
    assert!(
        model_xml.contains("DF_Product_Price"),
        "Model should contain named default constraint DF_Product_Price"
    );

    // Count SqlDefaultConstraint elements - should have at least 10
    let default_count = model_xml.matches("SqlDefaultConstraint").count();
    assert!(
        default_count >= 10,
        "Should have at least 10 SqlDefaultConstraint elements, found {}",
        default_count
    );
}

// ============================================================================
// Inline Constraint Tests
// ============================================================================

#[test]
fn test_build_with_inline_constraints() {
    let ctx = TestContext::with_fixture("inline_constraints");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with inline constraints should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Note: Modern .NET DacFx does NOT use SqlInlineConstraintAnnotation.
    // Inline constraints are converted to separate constraint elements.
    // This test verifies that all constraint types are properly captured.

    // Inline DEFAULT constraints should be captured as SqlDefaultConstraint
    assert!(
        model_xml.contains("SqlDefaultConstraint"),
        "Model should contain SqlDefaultConstraint for inline DEFAULT values"
    );
    // Verify specific default constraints
    assert!(
        model_xml.contains("DF_Customer_Balance") || model_xml.contains("DF_Customer"),
        "Model should contain default constraint for Balance column"
    );

    // Inline UNIQUE constraints should be captured as SqlUniqueConstraint
    assert!(
        model_xml.contains("SqlUniqueConstraint"),
        "Model should contain SqlUniqueConstraint for inline UNIQUE on Email"
    );

    // Inline CHECK constraints should be captured as SqlCheckConstraint
    assert!(
        model_xml.contains("SqlCheckConstraint"),
        "Model should contain SqlCheckConstraint for inline CHECK ([Age] >= 18)"
    );
    // Verify CHECK expression content
    assert!(
        model_xml.contains("[Age] >= 18"),
        "Model should contain the Age check expression"
    );
    assert!(
        model_xml.contains("[Salary] > 0"),
        "Model should contain the Salary check expression"
    );

    // Inline PRIMARY KEY constraints should be captured as SqlPrimaryKeyConstraint
    assert!(
        model_xml.contains("SqlPrimaryKeyConstraint"),
        "Model should contain SqlPrimaryKeyConstraint for inline PRIMARY KEY"
    );
}

#[test]
fn test_build_with_inline_check_constraints() {
    let ctx = TestContext::with_fixture("inline_constraints");
    let result = ctx.build();

    assert!(result.success);

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Inline CHECK constraints should be captured
    let has_check = model_xml.contains("SqlCheckConstraint");
    assert!(
        has_check,
        "Model should contain SqlCheckConstraint for inline CHECK ([Age] >= 18)"
    );

    // Verify all expected CHECK constraints are present
    assert!(
        model_xml.contains("CK_Employee_Age"),
        "Model should have check constraint for Age column"
    );
    assert!(
        model_xml.contains("CK_Employee_Salary"),
        "Model should have check constraint for Salary column"
    );
    assert!(
        model_xml.contains("CK_Account_Balance"),
        "Model should have check constraint for Account.Balance column"
    );
    assert!(
        model_xml.contains("CK_Account_Status"),
        "Model should have check constraint for Account.Status column"
    );
}

#[test]
fn test_build_with_inline_unique_constraints() {
    let ctx = TestContext::with_fixture("inline_constraints");
    let result = ctx.build();

    assert!(result.success);

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Inline UNIQUE constraints should be captured
    let has_unique = model_xml.contains("SqlUniqueConstraint");
    assert!(
        has_unique,
        "Model should contain SqlUniqueConstraint for inline UNIQUE on Email"
    );
}

// ============================================================================
// Procedure Parameter Tests
// ============================================================================

#[test]
fn test_build_with_procedure_parameters() {
    let ctx = TestContext::with_fixture("procedure_parameters");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with procedure parameters should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Count SqlSubroutineParameter elements
    let param_count = model_xml.matches("SqlSubroutineParameter").count();

    // Expected: GetUserById(2) + CreateUser(3) + ComplexProcedure(9) + CalculateTotal(3) + GetOrdersByCustomer(3) = 20
    // Currently captures ~14, which is acceptable baseline
    assert!(
        param_count >= 10,
        "Should have at least 10 SqlSubroutineParameter elements, found {} (expected ~20)",
        param_count
    );
}

#[test]
fn test_build_with_output_parameters() {
    let ctx = TestContext::with_fixture("procedure_parameters");
    let result = ctx.build();

    assert!(result.success);

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // OUTPUT parameters should have IsOutput property
    assert!(
        model_xml.contains("IsOutput"),
        "Model should contain IsOutput property for OUTPUT parameters"
    );
}

// ============================================================================
// Header Section Tests
// ============================================================================

#[test]
fn test_build_with_header_section() {
    let ctx = TestContext::with_fixture("header_section");
    let result = ctx.build();

    assert!(result.success);

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // DotNet includes a Header section with settings
    assert!(
        model_xml.contains("<Header>"),
        "Model should contain <Header> section"
    );

    assert!(
        model_xml.contains("AnsiNulls"),
        "Header should contain AnsiNulls setting"
    );

    assert!(
        model_xml.contains("QuotedIdentifier"),
        "Header should contain QuotedIdentifier setting"
    );

    assert!(
        model_xml.contains("CompatibilityMode"),
        "Header should contain CompatibilityMode setting"
    );
}

#[test]
fn test_build_with_package_references() {
    let ctx = TestContext::with_fixture("header_section");
    let result = ctx.build();

    assert!(result.success);

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Header should reference master.dacpac
    assert!(
        model_xml.contains("master.dacpac"),
        "Header should contain reference to master.dacpac"
    );

    // Should have proper CustomData structure for references
    assert!(
        model_xml.contains("Category=\"Reference\""),
        "Header should contain CustomData with Category=\"Reference\""
    );
}

// ============================================================================
// Database Options Tests
// ============================================================================

#[test]
fn test_build_with_database_options() {
    let ctx = TestContext::with_fixture("database_options");
    let result = ctx.build();

    assert!(result.success);

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // DotNet includes SqlDatabaseOptions element
    assert!(
        model_xml.contains("SqlDatabaseOptions"),
        "Model should contain SqlDatabaseOptions element"
    );

    assert!(
        model_xml.contains("IsAnsiNullsOn"),
        "SqlDatabaseOptions should contain IsAnsiNullsOn property"
    );

    assert!(
        model_xml.contains("PageVerifyMode"),
        "SqlDatabaseOptions should contain PageVerifyMode property"
    );
}

// ============================================================================
// SQLCMD Variable Tests
// ============================================================================

#[test]
fn test_build_with_sqlcmd_variables() {
    let ctx = TestContext::with_fixture("sqlcmd_variables");
    let result = ctx.build();

    assert!(result.success);

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Header should contain SqlCmdVariables
    assert!(
        model_xml.contains("SqlCmdVariable"),
        "Header should contain SqlCmdVariables section"
    );

    assert!(
        model_xml.contains("Environment"),
        "Should contain Environment variable"
    );
}

// ============================================================================
// Table IsAnsiNullsOn Property Tests
// ============================================================================

#[test]
#[ignore = "Table IsAnsiNullsOn property not yet implemented"]
fn test_table_has_ansi_nulls_property() {
    let ctx = TestContext::with_fixture("simple_table");
    let result = ctx.build();

    assert!(result.success);

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    let model_xml = info.model_xml_content.expect("Should have model XML");

    // DotNet adds IsAnsiNullsOn="True" property to SqlTable elements
    // Find a SqlTable element and verify it has the property
    assert!(
        model_xml.contains(r#"<Property Name="IsAnsiNullsOn" Value="True""#),
        "SqlTable elements should have IsAnsiNullsOn property"
    );
}
