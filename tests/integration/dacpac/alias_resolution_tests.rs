/// Tests for table alias resolution in BodyDependencies
///
/// These tests verify that table aliases defined in SQL bodies (procedures, functions, views)
/// are correctly resolved to their actual table references, not treated as schema names.
use crate::common::{DacpacInfo, TestContext};

/// Helper to get body dependencies from a scalar function in the model XML
fn get_scalar_function_body_dependencies(func_name: &str, model_xml: &str) -> Vec<String> {
    let func_element_tag = format!(
        "Element Type=\"SqlScalarFunction\" Name=\"[dbo].[{}]\"",
        func_name
    );

    if let Some(func_start) = model_xml.find(&func_element_tag) {
        if let Some(deps_start) = model_xml[func_start..].find("BodyDependencies") {
            let deps_section_start = func_start + deps_start;
            if let Some(deps_end) = model_xml[deps_section_start..].find("</Relationship>") {
                let deps_section = &model_xml[deps_section_start..deps_section_start + deps_end];

                let mut references = Vec::new();
                for line in deps_section.lines() {
                    if line.contains("<References Name=") {
                        if let Some(name_start) = line.find("Name=\"") {
                            let name_content = &line[name_start + 6..];
                            if let Some(name_end) = name_content.find('"') {
                                let ref_name = &name_content[..name_end];
                                references.push(ref_name.to_string());
                            }
                        }
                    }
                }
                return references;
            }
        }
    }

    Vec::new()
}

/// Helper to get body dependencies from a table-valued function in the model XML
fn get_tvf_body_dependencies(func_name: &str, model_xml: &str) -> Vec<String> {
    // Try inline TVF first
    let func_element_tag = format!(
        "Element Type=\"SqlInlineTableValuedFunction\" Name=\"[dbo].[{}]\"",
        func_name
    );

    if let Some(func_start) = model_xml.find(&func_element_tag) {
        if let Some(deps_start) = model_xml[func_start..].find("BodyDependencies") {
            let deps_section_start = func_start + deps_start;
            if let Some(deps_end) = model_xml[deps_section_start..].find("</Relationship>") {
                let deps_section = &model_xml[deps_section_start..deps_section_start + deps_end];

                let mut references = Vec::new();
                for line in deps_section.lines() {
                    if line.contains("<References Name=") {
                        if let Some(name_start) = line.find("Name=\"") {
                            let name_content = &line[name_start + 6..];
                            if let Some(name_end) = name_content.find('"') {
                                let ref_name = &name_content[..name_end];
                                references.push(ref_name.to_string());
                            }
                        }
                    }
                }
                return references;
            }
        }
    }

    Vec::new()
}

/// Helper to get body dependencies from a view in the model XML
fn get_view_body_dependencies(view_name: &str, model_xml: &str) -> Vec<String> {
    // Parse XML to find the view and extract its QueryDependencies (same as BodyDependencies for views)
    let view_element_tag = format!("Element Type=\"SqlView\" Name=\"[dbo].[{}]\"", view_name);

    if let Some(view_start) = model_xml.find(&view_element_tag) {
        if let Some(deps_start) = model_xml[view_start..].find("QueryDependencies") {
            let deps_section_start = view_start + deps_start;
            if let Some(deps_end) = model_xml[deps_section_start..].find("</Relationship>") {
                let deps_section = &model_xml[deps_section_start..deps_section_start + deps_end];

                // Extract all References Name attributes
                let mut references = Vec::new();
                for line in deps_section.lines() {
                    if line.contains("<References Name=") {
                        if let Some(name_start) = line.find("Name=\"") {
                            let name_content = &line[name_start + 6..];
                            if let Some(name_end) = name_content.find('"') {
                                let ref_name = &name_content[..name_end];
                                references.push(ref_name.to_string());
                            }
                        }
                    }
                }
                return references;
            }
        }
    }

    Vec::new()
}

/// Helper to get body dependencies from a procedure in the model XML
fn get_procedure_body_dependencies(proc_name: &str, model_xml: &str) -> Vec<String> {
    // Parse XML to find the procedure and extract its BodyDependencies
    let proc_element_tag = format!(
        "Element Type=\"SqlProcedure\" Name=\"[dbo].[{}]\"",
        proc_name
    );

    if let Some(proc_start) = model_xml.find(&proc_element_tag) {
        if let Some(deps_start) = model_xml[proc_start..].find("BodyDependencies") {
            let deps_section_start = proc_start + deps_start;
            if let Some(deps_end) = model_xml[deps_section_start..].find("</Relationship>") {
                let deps_section = &model_xml[deps_section_start..deps_section_start + deps_end];

                // Extract all References Name attributes
                let mut references = Vec::new();
                for line in deps_section.lines() {
                    if line.contains("<References Name=") {
                        if let Some(name_start) = line.find("Name=\"") {
                            let name_content = &line[name_start + 6..];
                            if let Some(name_end) = name_content.find('"') {
                                let ref_name = &name_content[..name_end];
                                references.push(ref_name.to_string());
                            }
                        }
                    }
                }
                return references;
            }
        }
    }

    Vec::new()
}

#[test]
fn test_stuff_nested_subquery_alias_resolution() {
    // Test the ITTAG alias bug: aliases in STUFF() nested subqueries should be resolved
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("InstrumentWithTags", &model_xml);

    println!("\nInstrumentWithTags dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid references to aliases as schema.table
    assert!(
        !deps.iter().any(|d| d.contains("[Instrument].[ITTAG]")),
        "ITTAG alias should not appear as [Instrument].[ITTAG] - it's an alias for Tag table"
    );
    assert!(
        !deps.iter().any(|d| d.contains("[Instrument].[IT2]")),
        "IT2 alias should not appear as [Instrument].[IT2] - it's an alias for InstrumentTag table"
    );

    // Should contain proper table references
    assert!(
        deps.iter().any(|d| d == "[dbo].[Tag]"),
        "Should have reference to Tag table (for ITTAG alias)"
    );
    assert!(
        deps.iter().any(|d| d == "[dbo].[InstrumentTag]"),
        "Should have reference to InstrumentTag table (for IT2 alias)"
    );

    // Should contain proper column references
    assert!(
        deps.iter().any(|d| d == "[dbo].[Tag].[Name]"),
        "Should have reference to Tag.Name (from [ITTAG].[Name])"
    );
}

#[test]
fn test_nested_subquery_alias_resolution() {
    // Test multiple levels of nested subqueries with aliases
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithNestedSubqueries", &model_xml);

    println!("\nAccountWithNestedSubqueries dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains(".[AT1]") || d.contains(".[AT2]") || d.contains(".[T2]")),
        "Aliases AT1, AT2, T2 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

#[test]
fn test_apply_clause_alias_resolution() {
    // Test CROSS APPLY and OUTER APPLY with aliases
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithApply", &model_xml);

    println!("\nAccountWithApply dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps.iter().any(|d| d.contains(".[ATAG]")
            || d.contains(".[ATA]")
            || d.contains(".[T3]")
            || d.contains(".[AT3]")),
        "Aliases ATAG, ATA, T3, AT3 should be resolved to actual tables"
    );

    // Should contain proper references
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
}

#[test]
fn test_procedure_apply_clause_alias_resolution() {
    // Test CROSS APPLY and OUTER APPLY with aliases in procedures (Phase 26.2.2)
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_procedure_body_dependencies("GetAccountWithApply", &model_xml);

    println!("\nGetAccountWithApply procedure dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid APPLY subquery alias references
    // 'd' is CROSS APPLY alias, 't' is OUTER APPLY alias
    // They should not appear as schema names (e.g., [d].[TagCount]) or column prefixes
    assert!(
        !deps
            .iter()
            .any(|d| d.starts_with("[d].") || d.starts_with("[t].")),
        "APPLY aliases 'd' and 't' should NOT appear as schema-qualified references. Got: {:?}",
        deps
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

#[test]
fn test_cte_alias_recognition() {
    // Test that CTE names are NOT treated as table references
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithCTE", &model_xml);

    println!("\nAccountWithCTE dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain reference to CTE as if it were a table
    assert!(
        !deps.iter().any(|d| d.contains("[TaggedAccounts]")),
        "TaggedAccounts is a CTE name, not a table - should not appear in dependencies"
    );

    // Should contain actual table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
}

#[test]
fn test_procedure_nested_alias_resolution() {
    // Test aliases in procedure bodies
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_procedure_body_dependencies("GetAccountsWithTags", &model_xml);

    println!("\nGetAccountsWithTags dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains(".[TNAME]") || d.contains(".[AT2]")),
        "Aliases TNAME, AT2 should be resolved to actual tables"
    );

    // Should contain proper references
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(
        deps.iter().any(|d| d == "[dbo].[Tag].[Name]"),
        "Should have reference to Tag.Name from [TNAME].[Name]"
    );
}

#[test]
fn test_simple_alias_resolution() {
    // Test simple, flat aliases (this should work correctly)
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountSummary", &model_xml);

    println!("\nAccountSummary dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain aliases as if they were schemas
    assert!(
        !deps.iter().any(|d| d.starts_with("[A].")),
        "A is an alias for Account, not a schema"
    );
    assert!(
        !deps.iter().any(|d| d.starts_with("[T].")),
        "T is an alias for Tag, not a schema"
    );
    assert!(
        !deps.iter().any(|d| d.starts_with("[AT].")),
        "AT is an alias for AccountTag, not a schema"
    );

    // Should contain actual table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
}

// ============================================================================
// Tests for existing fixtures that lacked coverage
// ============================================================================

#[test]
fn test_scalar_function_nested_alias_resolution() {
    // Test aliases in scalar function bodies (STUFF pattern)
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_scalar_function_body_dependencies("GetInstrumentTagList", &model_xml);

    println!("\nGetInstrumentTagList dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains(".[ITAG]") || d.contains(".[ITA]")),
        "Aliases ITAG, ITA should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(
        deps.iter().any(|d| d == "[dbo].[InstrumentTag]"),
        "Should have reference to InstrumentTag table"
    );
    assert!(
        deps.iter().any(|d| d == "[dbo].[Tag]"),
        "Should have reference to Tag table"
    );
}

#[test]
fn test_tvf_nested_alias_resolution() {
    // Test aliases in table-valued function bodies (OUTER APPLY pattern)
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_tvf_body_dependencies("GetAccountTagsTable", &model_xml);

    println!("\nGetAccountTagsTable dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains(".[ATAG]") || d.contains(".[ATA]") || d.contains(".[AT2]")),
        "Aliases ATAG, ATA, AT2 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

#[test]
fn test_multiple_ctes_in_procedure() {
    // Test multiple CTEs in sequence within a procedure
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_procedure_body_dependencies("GetAccountWithCte", &model_xml);

    println!("\nGetAccountWithCte dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain CTE names as table references
    assert!(
        !deps.iter().any(|d| d.contains("[AccountCte]")),
        "AccountCte is a CTE name, not a table"
    );
    assert!(
        !deps.iter().any(|d| d.contains("[TagCte]")),
        "TagCte is a CTE name, not a table"
    );
    assert!(
        !deps.iter().any(|d| d.contains("[AccountTagCte]")),
        "AccountTagCte is a CTE name, not a table"
    );

    // Should contain actual table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
}

// ============================================================================
// Tests for EXISTS/NOT EXISTS subqueries
// ============================================================================

#[test]
fn test_exists_subquery_alias_resolution() {
    // Test aliases inside EXISTS and NOT EXISTS subqueries
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithExistsSubquery", &model_xml);

    println!("\nAccountWithExistsSubquery dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps.iter().any(|d| d.contains(".[AT1]")
            || d.contains(".[AT2]")
            || d.contains(".[T1]")
            || d.contains(".[T2]")),
        "Aliases AT1, AT2, T1, T2 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for IN clause subqueries
// ============================================================================

#[test]
fn test_in_subquery_alias_resolution() {
    // Test aliases inside IN clause subqueries (including nested IN)
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithInSubquery", &model_xml);

    println!("\nAccountWithInSubquery dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains(".[AT1]") || d.contains(".[T1]") || d.contains(".[T2]")),
        "Aliases AT1, T1, T2 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for correlated subqueries in SELECT list
// ============================================================================

#[test]
fn test_correlated_subquery_alias_resolution() {
    // Test aliases in correlated subqueries in SELECT list
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithCorrelatedSubquery", &model_xml);

    println!("\nAccountWithCorrelatedSubquery dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains(".[AT1]") || d.contains(".[AT2]") || d.contains(".[T1]")),
        "Aliases AT1, AT2, T1 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for UNION with different aliases
// ============================================================================

#[test]
fn test_union_alias_resolution() {
    // Test aliases in UNION branches (each SELECT has different aliases)
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithUnion", &model_xml);

    println!("\nAccountWithUnion dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references from either UNION branch
    assert!(
        !deps.iter().any(|d| d.contains(".[A1]")
            || d.contains(".[A2]")
            || d.contains(".[T1]")
            || d.contains(".[T2]")
            || d.contains(".[AT1]")
            || d.contains(".[AT2]")),
        "Aliases A1, A2, T1, T2, AT1, AT2 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for CASE expression subqueries
// ============================================================================

#[test]
fn test_case_subquery_alias_resolution() {
    // Test aliases in subqueries inside CASE expressions
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithCaseSubquery", &model_xml);

    println!("\nAccountWithCaseSubquery dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps.iter().any(|d| d.contains(".[AT1]")
            || d.contains(".[AT2]")
            || d.contains(".[AT3]")
            || d.contains(".[T1]")),
        "Aliases AT1, AT2, AT3, T1 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for window functions with aliases
// ============================================================================

#[test]
fn test_window_function_alias_resolution() {
    // Test aliases in window function OVER clauses
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithWindowFunction", &model_xml);

    println!("\nAccountWithWindowFunction dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain aliases as schema references
    assert!(
        !deps
            .iter()
            .any(|d| d.starts_with("[A].") || d.starts_with("[T].") || d.starts_with("[AT].")),
        "Aliases A, T, AT should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for derived table chains
// ============================================================================

#[test]
fn test_derived_table_chain_alias_resolution() {
    // Test aliases through multiple levels of derived tables
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithDerivedTableChain", &model_xml);

    println!("\nAccountWithDerivedTableChain dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain derived table aliases as table references
    assert!(
        !deps.iter().any(|d| d.contains("[OuterDerived]")
            || d.contains("[MiddleDerived]")
            || d.contains(".[AT1]")),
        "Derived table aliases should not appear in dependencies"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
}

// ============================================================================
// Tests for recursive CTEs
// ============================================================================

#[test]
fn test_recursive_cte_alias_resolution() {
    // Test that recursive CTE self-references don't appear as dependencies
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_view_body_dependencies("AccountWithRecursiveCTE", &model_xml);

    println!("\nAccountWithRecursiveCTE dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain CTE name as a dependency
    assert!(
        !deps.iter().any(|d| d.contains("[TagHierarchy]")),
        "TagHierarchy is a recursive CTE, not a table"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for UPDATE with FROM clause
// ============================================================================

#[test]
fn test_update_from_alias_resolution() {
    // Test aliases in UPDATE...FROM statements
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_procedure_body_dependencies("UpdateAccountWithFrom", &model_xml);

    println!("\nUpdateAccountWithFrom dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains(".[AT1]") || d.contains(".[T1]")),
        "Aliases AT1, T1 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for DELETE with FROM clause
// ============================================================================

#[test]
fn test_delete_from_alias_resolution() {
    // Test aliases in DELETE...FROM statements
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_procedure_body_dependencies("DeleteAccountWithFrom", &model_xml);

    println!("\nDeleteAccountWithFrom dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains(".[AT1]") || d.contains(".[A1]") || d.contains(".[T1]")),
        "Aliases AT1, A1, T1 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for MERGE statements
// ============================================================================

#[test]
fn test_merge_alias_resolution() {
    // Test aliases in MERGE statements (TARGET and SOURCE)
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_procedure_body_dependencies("MergeAccountTags", &model_xml);

    println!("\nMergeAccountTags dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain MERGE aliases as table references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains("[TARGET]") || d.contains("[SOURCE]")),
        "TARGET and SOURCE are MERGE aliases, not tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[Account]"));
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}

// ============================================================================
// Tests for INSERT...SELECT with aliases
// ============================================================================

#[test]
fn test_insert_select_alias_resolution() {
    // Test aliases in INSERT...SELECT with nested EXISTS
    let ctx = TestContext::with_fixture("body_dependencies_aliases");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    let model_xml = info.model_xml_content.expect("Should have model XML");

    let deps = get_procedure_body_dependencies("InsertSelectWithAliases", &model_xml);

    println!("\nInsertSelectWithAliases dependencies:");
    for dep in &deps {
        println!("  - {}", dep);
    }

    // Should NOT contain invalid alias references
    assert!(
        !deps
            .iter()
            .any(|d| d.contains(".[AT1]") || d.contains(".[AT2]") || d.contains(".[T1]")),
        "Aliases AT1, AT2, T1 should be resolved to actual tables"
    );

    // Should contain proper table references
    assert!(deps.iter().any(|d| d == "[dbo].[AccountTag]"));
    assert!(deps.iter().any(|d| d == "[dbo].[Tag]"));
}
