//! Index property tests
//!
//! Tests for unique, clustered, column specs, and include columns.

use crate::common::{DacpacInfo, TestContext};

use super::{find_index_by_name, get_property_value, has_relationship, parse_model_xml};

// ============================================================================
// Index Property Tests (XSD Compliance)
// ============================================================================

/// Test that unique indexes have the IsUnique property set to True.
/// This verifies that CREATE UNIQUE INDEX statements produce indexes with IsUnique=True.
#[test]
fn test_index_is_unique_property() {
    let ctx = TestContext::with_fixture("index_properties");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check unique index has IsUnique=True
    let unique_index = find_index_by_name(&doc, "UX_Products_SKU");
    assert!(
        unique_index.is_some(),
        "Should find UX_Products_SKU index as SqlIndex element"
    );

    let unique_index = unique_index.unwrap();
    let is_unique = get_property_value(&unique_index, "IsUnique");
    assert_eq!(
        is_unique,
        Some("True".to_string()),
        "UX_Products_SKU should have IsUnique=True"
    );

    // Check non-unique index does NOT have IsUnique=True (or has IsUnique=False)
    let non_unique_index = find_index_by_name(&doc, "IX_Products_Category");
    assert!(
        non_unique_index.is_some(),
        "Should find IX_Products_Category index as SqlIndex element"
    );

    let non_unique_index = non_unique_index.unwrap();
    let non_unique_is_unique = get_property_value(&non_unique_index, "IsUnique");
    // Per XSD, property is only written when True, so None means not unique
    assert!(
        non_unique_is_unique.is_none() || non_unique_is_unique == Some("False".to_string()),
        "IX_Products_Category should NOT have IsUnique=True. Got: {:?}",
        non_unique_is_unique
    );
}

/// Test that clustered indexes have the IsClustered property set to True.
/// This verifies that CREATE CLUSTERED INDEX statements produce indexes with IsClustered=True.
#[test]
fn test_index_is_clustered_property() {
    let ctx = TestContext::with_fixture("index_properties");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Check clustered index has IsClustered=True
    let clustered_index = find_index_by_name(&doc, "IX_Products_CreatedAt");
    assert!(
        clustered_index.is_some(),
        "Should find IX_Products_CreatedAt index as SqlIndex element"
    );

    let clustered_index = clustered_index.unwrap();
    let is_clustered = get_property_value(&clustered_index, "IsClustered");
    assert_eq!(
        is_clustered,
        Some("True".to_string()),
        "IX_Products_CreatedAt should have IsClustered=True"
    );

    // Check nonclustered index does NOT have IsClustered=True
    let nonclustered_index = find_index_by_name(&doc, "IX_Products_Category");
    assert!(
        nonclustered_index.is_some(),
        "Should find IX_Products_Category index as SqlIndex element"
    );

    let nonclustered_index = nonclustered_index.unwrap();
    let non_clustered_is_clustered = get_property_value(&nonclustered_index, "IsClustered");
    // Per XSD, property is only written when True, so None means not clustered
    assert!(
        non_clustered_is_clustered.is_none()
            || non_clustered_is_clustered == Some("False".to_string()),
        "IX_Products_Category should NOT have IsClustered=True. Got: {:?}",
        non_clustered_is_clustered
    );
}

/// Test that indexes have ColumnSpecifications relationship with proper column references.
/// This verifies the index key columns are correctly referenced.
#[test]
fn test_index_column_specifications() {
    let ctx = TestContext::with_fixture("index_properties");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Find the multi-column index
    let multi_col_index = find_index_by_name(&doc, "IX_Products_Category_Name_Include");
    assert!(
        multi_col_index.is_some(),
        "Should find IX_Products_Category_Name_Include index as SqlIndex element"
    );

    let multi_col_index = multi_col_index.unwrap();

    // Verify ColumnSpecifications relationship exists
    assert!(
        has_relationship(&multi_col_index, "ColumnSpecifications"),
        "Index should have ColumnSpecifications relationship"
    );

    // Get the ColumnSpecifications relationship
    let col_specs_rel = multi_col_index
        .children()
        .find(|c| {
            c.tag_name().name() == "Relationship"
                && c.attribute("Name") == Some("ColumnSpecifications")
        })
        .expect("Should have ColumnSpecifications relationship");

    // Count Entry elements (should have 2 for Category and Name key columns)
    let entries: Vec<_> = col_specs_rel
        .children()
        .filter(|c| c.tag_name().name() == "Entry")
        .collect();

    assert_eq!(
        entries.len(),
        2,
        "ColumnSpecifications should have 2 entries for the 2 key columns (Category, Name). Found: {}",
        entries.len()
    );

    // Verify each entry contains SqlIndexedColumnSpecification
    for entry in &entries {
        let has_spec = entry.descendants().any(|d| {
            d.tag_name().name() == "Element"
                && d.attribute("Type") == Some("SqlIndexedColumnSpecification")
        });
        assert!(
            has_spec,
            "Each Entry should contain a SqlIndexedColumnSpecification element"
        );
    }

    // Verify column references exist
    let column_refs: Vec<_> = col_specs_rel
        .descendants()
        .filter(|n| n.tag_name().name() == "Relationship" && n.attribute("Name") == Some("Column"))
        .collect();

    assert_eq!(
        column_refs.len(),
        2,
        "Should have 2 Column relationships (one per key column). Found: {}",
        column_refs.len()
    );
}

/// Test that indexes with INCLUDE columns have the IncludedColumns relationship.
/// This verifies that the INCLUDE clause columns are properly captured.
#[test]
fn test_index_include_columns() {
    let ctx = TestContext::with_fixture("index_properties");
    let dacpac_path = ctx.build_successfully();

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let doc = parse_model_xml(&model_xml);

    // Find the index with INCLUDE columns
    let include_index = find_index_by_name(&doc, "IX_Products_Category_Name_Include");
    assert!(
        include_index.is_some(),
        "Should find IX_Products_Category_Name_Include index as SqlIndex element"
    );

    let include_index = include_index.unwrap();

    // Verify IncludedColumns relationship exists
    assert!(
        has_relationship(&include_index, "IncludedColumns"),
        "Index with INCLUDE clause should have IncludedColumns relationship"
    );

    // Get the IncludedColumns relationship
    let include_rel = include_index
        .children()
        .find(|c| {
            c.tag_name().name() == "Relationship" && c.attribute("Name") == Some("IncludedColumns")
        })
        .expect("Should have IncludedColumns relationship");

    // Count Entry elements (should have 2 for Price and Description)
    let entries: Vec<_> = include_rel
        .children()
        .filter(|c| c.tag_name().name() == "Entry")
        .collect();

    assert_eq!(
        entries.len(),
        2,
        "IncludedColumns should have 2 entries for the 2 included columns (Price, Description). Found: {}",
        entries.len()
    );

    // Verify entries contain References to the columns
    let refs: Vec<_> = include_rel
        .descendants()
        .filter(|n| n.tag_name().name() == "References")
        .collect();

    assert_eq!(
        refs.len(),
        2,
        "IncludedColumns should have 2 References (one per included column). Found: {}",
        refs.len()
    );

    // Verify the column names are Price and Description
    let ref_names: Vec<_> = refs.iter().filter_map(|r| r.attribute("Name")).collect();

    let has_price = ref_names.iter().any(|n| n.contains("Price"));
    let has_description = ref_names.iter().any(|n| n.contains("Description"));

    assert!(
        has_price,
        "IncludedColumns should reference Price column. Found refs: {:?}",
        ref_names
    );
    assert!(
        has_description,
        "IncludedColumns should reference Description column. Found refs: {:?}",
        ref_names
    );

    // Verify that an index WITHOUT INCLUDE does NOT have IncludedColumns relationship
    let simple_index = find_index_by_name(&doc, "IX_Products_Category");
    if let Some(simple_index) = simple_index {
        assert!(
            !has_relationship(&simple_index, "IncludedColumns"),
            "Index without INCLUDE clause should NOT have IncludedColumns relationship"
        );
    }
}
