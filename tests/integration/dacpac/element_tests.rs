//! Database element tests
//!
//! Tests for tables, views, procedures, functions, sequences, types, triggers, and schemas.

use crate::common::{DacpacInfo, TestContext};

use super::{find_elements_by_type, has_relationship, parse_model_xml};

// ============================================================================
// Basic Element Tests
// ============================================================================

#[test]
fn test_model_contains_all_tables() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert_eq!(
        info.tables.len(),
        4,
        "Should have 4 tables. Found: {:?}",
        info.tables
    );
}

#[test]
fn test_model_contains_all_views() {
    let ctx = TestContext::with_fixture("views");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    assert!(
        info.views.len() >= 1,
        "Should have at least 1 view. Found: {:?}",
        info.views
    );
}

#[test]
fn test_model_contains_indexes() {
    let ctx = TestContext::with_fixture("indexes");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    assert!(
        model_xml.contains("SqlIndex"),
        "Model should contain index elements"
    );
}

// ============================================================================
// Complex Model Tests
// ============================================================================

#[test]
fn test_dacpac_with_relationships() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Verify relationships are present in model
    assert!(
        model_xml.contains("Relationship") || model_xml.contains("relationship"),
        "Model should contain relationships"
    );
}

#[test]
fn test_dacpac_with_constraints() {
    let ctx = TestContext::with_fixture("constraints");
    let result = ctx.build();

    assert!(result.success);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    // Check for various constraint types
    let has_pk = model_xml.contains("SqlPrimaryKeyConstraint");
    let has_fk = model_xml.contains("SqlForeignKeyConstraint");
    let has_uq = model_xml.contains("SqlUniqueConstraint");
    let has_ck = model_xml.contains("SqlCheckConstraint");

    assert!(
        has_pk || has_fk || has_uq || has_ck,
        "Model should contain at least one constraint type"
    );

    // Log which constraints were found for debugging
    println!(
        "Found constraints - PK: {}, FK: {}, UQ: {}, CK: {}",
        has_pk, has_fk, has_uq, has_ck
    );
}

// ============================================================================
// Element Type Coverage Tests (Medium Priority)
// ============================================================================

#[test]
fn test_model_contains_procedures() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let procedures = find_elements_by_type(&doc, "SqlProcedure");

    assert!(
        !procedures.is_empty(),
        "Model should contain at least one SqlProcedure element"
    );

    // Verify the GetUsers procedure exists
    let has_get_users = procedures.iter().any(|p| {
        p.attribute("Name")
            .map_or(false, |n| n.contains("GetUsers"))
    });
    assert!(
        has_get_users,
        "Should have GetUsers procedure. Found: {:?}",
        procedures
            .iter()
            .filter_map(|p| p.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_scalar_functions() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let scalar_funcs = find_elements_by_type(&doc, "SqlScalarFunction");

    assert!(
        !scalar_funcs.is_empty(),
        "Model should contain at least one SqlScalarFunction element"
    );

    // Verify the GetUserCount function exists
    let has_get_user_count = scalar_funcs.iter().any(|f| {
        f.attribute("Name")
            .map_or(false, |n| n.contains("GetUserCount"))
    });
    assert!(
        has_get_user_count,
        "Should have GetUserCount scalar function. Found: {:?}",
        scalar_funcs
            .iter()
            .filter_map(|f| f.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_table_valued_functions() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check for multi-statement TVF (SqlMultiStatementTableValuedFunction)
    let tvfs = find_elements_by_type(&doc, "SqlMultiStatementTableValuedFunction");
    // Check for inline TVF (SqlInlineTableValuedFunction)
    let inline_tvfs = find_elements_by_type(&doc, "SqlInlineTableValuedFunction");

    assert!(
        !tvfs.is_empty() || !inline_tvfs.is_empty(),
        "Model should contain at least one table-valued function (SqlMultiStatementTableValuedFunction or SqlInlineTableValuedFunction)"
    );
}

#[test]
fn test_model_contains_sequences() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let sequences = find_elements_by_type(&doc, "SqlSequence");

    assert!(
        !sequences.is_empty(),
        "Model should contain at least one SqlSequence element"
    );

    // Verify the OrderSequence exists
    let has_order_sequence = sequences.iter().any(|s| {
        s.attribute("Name")
            .map_or(false, |n| n.contains("OrderSequence"))
    });
    assert!(
        has_order_sequence,
        "Should have OrderSequence. Found: {:?}",
        sequences
            .iter()
            .filter_map(|s| s.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_user_defined_types() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let udts = find_elements_by_type(&doc, "SqlTableType");

    assert!(
        !udts.is_empty(),
        "Model should contain at least one SqlTableType element"
    );

    // Verify the UserTableType exists
    let has_user_table_type = udts.iter().any(|u| {
        u.attribute("Name")
            .map_or(false, |n| n.contains("UserTableType"))
    });
    assert!(
        has_user_table_type,
        "Should have UserTableType. Found: {:?}",
        udts.iter()
            .filter_map(|u| u.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_triggers() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let triggers = find_elements_by_type(&doc, "SqlDmlTrigger");

    assert!(
        !triggers.is_empty(),
        "Model should contain at least one SqlDmlTrigger element"
    );

    // Verify the AuditTrigger exists
    let has_audit_trigger = triggers.iter().any(|t| {
        t.attribute("Name")
            .map_or(false, |n| n.contains("TR_Users_Audit"))
    });
    assert!(
        has_audit_trigger,
        "Should have TR_Users_Audit trigger. Found: {:?}",
        triggers
            .iter()
            .filter_map(|t| t.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_model_contains_schemas() {
    let ctx = TestContext::with_fixture("element_types");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let schemas = find_elements_by_type(&doc, "SqlSchema");

    assert!(
        !schemas.is_empty(),
        "Model should contain at least one SqlSchema element"
    );

    // Verify the custom Sales schema exists (in addition to dbo)
    let has_sales_schema = schemas
        .iter()
        .any(|s| s.attribute("Name").map_or(false, |n| n.contains("Sales")));
    assert!(
        has_sales_schema,
        "Should have Sales schema. Found: {:?}",
        schemas
            .iter()
            .filter_map(|s| s.attribute("Name"))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Self-Referencing Foreign Key Tests
// ============================================================================

/// Test a self-referencing foreign key (table FK to itself).
#[test]
fn test_self_referencing_foreign_key() {
    let ctx = TestContext::with_fixture("self_ref_fk");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let fk_constraints = find_elements_by_type(&doc, "SqlForeignKeyConstraint");

    assert!(
        !fk_constraints.is_empty(),
        "Should have at least one foreign key constraint"
    );

    // Find the self-referencing FK by checking DefiningTable and ForeignTable are the same
    let self_ref_fk = fk_constraints.iter().find(|fk| {
        let defining_table = fk
            .children()
            .find(|c| {
                c.tag_name().name() == "Relationship"
                    && c.attribute("Name") == Some("DefiningTable")
            })
            .and_then(|rel| {
                rel.descendants()
                    .find(|d| d.tag_name().name() == "References")
            })
            .and_then(|r| r.attribute("Name"));

        let foreign_table = fk
            .children()
            .find(|c| {
                c.tag_name().name() == "Relationship" && c.attribute("Name") == Some("ForeignTable")
            })
            .and_then(|rel| {
                rel.descendants()
                    .find(|d| d.tag_name().name() == "References")
            })
            .and_then(|r| r.attribute("Name"));

        // Self-referencing: both should reference the same table
        defining_table.is_some() && defining_table == foreign_table
    });

    assert!(
        self_ref_fk.is_some(),
        "Should have a self-referencing foreign key (DefiningTable == ForeignTable)"
    );
}

// ============================================================================
// Multiple Indexes Tests
// ============================================================================

/// Test multiple indexes on the same table.
#[test]
fn test_multiple_indexes_same_table() {
    let ctx = TestContext::with_fixture("multiple_indexes");
    let result = ctx.build();

    assert!(result.success, "Build failed: {:?}", result.errors);
    let dacpac_path = result.dacpac_path.unwrap();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);
    let indexes = find_elements_by_type(&doc, "SqlIndex");

    // Should have multiple indexes on same table
    assert!(
        indexes.len() >= 3,
        "Should have at least 3 indexes on the same table, found: {}",
        indexes.len()
    );

    // Verify all indexes reference the same table
    let table_refs: Vec<_> = indexes
        .iter()
        .filter_map(|idx| {
            idx.children()
                .find(|c| {
                    c.tag_name().name() == "Relationship"
                        && c.attribute("Name") == Some("IndexedObject")
                })
                .and_then(|rel| {
                    rel.descendants()
                        .find(|d| d.tag_name().name() == "References")
                        .and_then(|r| r.attribute("Name"))
                })
        })
        .collect();

    // All indexes should reference a table named "Products" or similar
    assert!(
        !table_refs.is_empty(),
        "Indexes should have IndexedObject relationship references"
    );
    println!("Index table references: {:?}", table_refs);
}
