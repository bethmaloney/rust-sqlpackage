//! Constraint tests
//!
//! Tests for primary key, foreign key, check, and default constraints.

use crate::common::{DacpacInfo, TestContext};

use super::{find_elements_by_type, has_relationship, parse_model_xml};

// ============================================================================
// Constraint Tests (Medium Priority)
// ============================================================================

#[test]
fn test_primary_key_constraint() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let pk_constraints = find_elements_by_type(&doc, "SqlPrimaryKeyConstraint");

    assert!(
        !pk_constraints.is_empty(),
        "Model should contain SqlPrimaryKeyConstraint elements"
    );

    // Verify PK has DefiningTable relationship
    for pk in &pk_constraints {
        assert!(
            has_relationship(pk, "DefiningTable"),
            "SqlPrimaryKeyConstraint '{}' should have DefiningTable relationship",
            pk.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify the named constraint exists
    let has_pk_primary_key_table = pk_constraints.iter().any(|pk| {
        pk.attribute("Name")
            .is_some_and(|n| n.contains("PK_PrimaryKeyTable"))
    });
    assert!(
        has_pk_primary_key_table,
        "Should have PK_PrimaryKeyTable constraint. Found: {:?}",
        pk_constraints
            .iter()
            .filter_map(|pk| pk.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_foreign_key_constraint_with_referenced_table() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let fk_constraints = find_elements_by_type(&doc, "SqlForeignKeyConstraint");

    assert!(
        !fk_constraints.is_empty(),
        "Model should contain SqlForeignKeyConstraint elements"
    );

    // Verify FK has DefiningTable relationship
    for fk in &fk_constraints {
        assert!(
            has_relationship(fk, "DefiningTable"),
            "SqlForeignKeyConstraint '{}' should have DefiningTable relationship",
            fk.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify FK has ForeignTable relationship
    for fk in &fk_constraints {
        assert!(
            has_relationship(fk, "ForeignTable"),
            "SqlForeignKeyConstraint '{}' should have ForeignTable relationship",
            fk.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify the named constraint exists
    let has_fk_foreign_key_table = fk_constraints.iter().any(|fk| {
        fk.attribute("Name")
            .is_some_and(|n| n.contains("FK_ForeignKeyTable_Parent"))
    });
    assert!(
        has_fk_foreign_key_table,
        "Should have FK_ForeignKeyTable_Parent constraint. Found: {:?}",
        fk_constraints
            .iter()
            .filter_map(|fk| fk.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_check_constraint_with_definition() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let ck_constraints = find_elements_by_type(&doc, "SqlCheckConstraint");

    assert!(
        !ck_constraints.is_empty(),
        "Model should contain SqlCheckConstraint elements"
    );

    // Verify check constraints have DefiningTable relationship
    for ck in &ck_constraints {
        assert!(
            has_relationship(ck, "DefiningTable"),
            "SqlCheckConstraint '{}' should have DefiningTable relationship",
            ck.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify check constraints have CheckExpressionScript property with CDATA
    for ck in &ck_constraints {
        let has_check_expression = ck.children().any(|c| {
            c.tag_name().name() == "Property"
                && c.attribute("Name") == Some("CheckExpressionScript")
        });
        assert!(
            has_check_expression,
            "SqlCheckConstraint '{}' should have CheckExpressionScript property with the check expression",
            ck.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify check constraints have CheckExpressionDependencies relationship
    // DotNet emits this relationship referencing the columns used in the CHECK expression
    for ck in &ck_constraints {
        assert!(
            has_relationship(ck, "CheckExpressionDependencies"),
            "SqlCheckConstraint '{}' should have CheckExpressionDependencies relationship",
            ck.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Verify named constraints exist
    let has_age_check = ck_constraints.iter().any(|ck| {
        ck.attribute("Name")
            .is_some_and(|n| n.contains("CK_CheckConstraintTable_Age"))
    });
    assert!(
        has_age_check,
        "Should have CK_CheckConstraintTable_Age constraint"
    );
}

#[test]
fn test_default_constraint() {
    // Note: The constraints fixture doesn't have default constraints currently.
    // This test verifies the structure works when SqlDefaultConstraint elements exist.
    // For now, we just verify the model can be built and parsed.
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let default_constraints = find_elements_by_type(&doc, "SqlDefaultConstraint");

    // Default constraints are optional - test structure if they exist
    for df in &default_constraints {
        assert!(
            has_relationship(df, "DefiningTable"),
            "SqlDefaultConstraint '{}' should have DefiningTable relationship",
            df.attribute("Name").unwrap_or("unnamed")
        );
    }

    // Just verify the model is valid - default constraints may or may not be present
    assert!(
        doc.root_element().tag_name().name() == "DataSchemaModel",
        "Model should be valid DataSchemaModel"
    );
}

/// Test a table with all constraint types: PK, FK, UQ, CK, DF.
#[test]
fn test_all_constraint_types_combined() {
    let ctx = TestContext::with_fixture("all_constraints");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Verify all constraint types are present
    let pk = find_elements_by_type(&doc, "SqlPrimaryKeyConstraint");
    let fk = find_elements_by_type(&doc, "SqlForeignKeyConstraint");
    let uq = find_elements_by_type(&doc, "SqlUniqueConstraint");
    let ck = find_elements_by_type(&doc, "SqlCheckConstraint");
    let df = find_elements_by_type(&doc, "SqlDefaultConstraint");

    assert!(!pk.is_empty(), "Should have primary key constraint");
    assert!(!fk.is_empty(), "Should have foreign key constraint");
    assert!(!uq.is_empty(), "Should have unique constraint");
    assert!(!ck.is_empty(), "Should have check constraint");
    assert!(!df.is_empty(), "Should have default constraint");

    println!(
        "Found constraints - PK: {}, FK: {}, UQ: {}, CK: {}, DF: {}",
        pk.len(),
        fk.len(),
        uq.len(),
        ck.len(),
        df.len()
    );
}
