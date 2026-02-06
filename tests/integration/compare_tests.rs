//! Integration tests for the compare module

use crate::common::TestContext;

#[test]
fn test_compare_dacpac_to_itself_is_identical() {
    let ctx = TestContext::with_fixture("simple_table");
    let dacpac_path = ctx.build_successfully();

    let result = rust_sqlpackage::compare::compare_dacpacs(&dacpac_path, &dacpac_path).unwrap();

    assert!(
        !result.has_differences(),
        "Comparing a dacpac to itself should find no differences"
    );

    // Verify structural correctness of the result
    assert!(result.header_result.is_some());
    assert!(result.elements_result.is_some());
    assert!(result.duplicate_warnings.is_empty());

    let elems = result.elements_result.unwrap();
    assert!(elems.missing_in_rust.is_empty());
    assert!(elems.extra_in_rust.is_empty());
    assert!(elems.differences.is_empty());
    assert!(elems.total_rust > 0, "Should have at least one element");
    assert_eq!(elems.total_rust, elems.total_dotnet);
}

#[test]
fn test_compare_dacpac_with_deploy_scripts() {
    let ctx = TestContext::with_fixture("pre_post_deploy");
    let dacpac_path = ctx.build_successfully();

    let result = rust_sqlpackage::compare::compare_dacpacs(&dacpac_path, &dacpac_path).unwrap();

    assert!(
        !result.has_differences(),
        "Comparing a dacpac with deploy scripts to itself should find no differences"
    );
}

#[test]
fn test_compare_different_dacpacs_finds_differences() {
    let ctx_a = TestContext::with_fixture("simple_table");
    let path_a = ctx_a.build_successfully();

    let ctx_b = TestContext::with_fixture("views");
    let path_b = ctx_b.build_successfully();

    let result = rust_sqlpackage::compare::compare_dacpacs(&path_a, &path_b).unwrap();

    assert!(
        result.has_differences(),
        "Comparing different dacpacs should find differences"
    );

    let elems = result.elements_result.unwrap();
    // The two fixtures have different elements, so there should be missing/extra
    assert!(
        !elems.missing_in_rust.is_empty() || !elems.extra_in_rust.is_empty(),
        "Should have missing or extra elements between different projects"
    );
}

#[test]
fn test_compare_has_differences_returns_correct_value() {
    use rust_sqlpackage::compare::types::*;

    // No differences
    let result = CompareResult {
        file_results: vec![
            (
                "Origin.xml".to_string(),
                FileStatus::Skipped("test".to_string()),
            ),
            ("DacMetadata.xml".to_string(), FileStatus::Ok),
        ],
        header_result: Some(HeaderResult {
            is_ok: true,
            diffs: vec![],
        }),
        elements_result: Some(ModelElementsResult {
            total_rust: 5,
            total_dotnet: 5,
            missing_in_rust: vec![],
            extra_in_rust: vec![],
            differences: vec![],
        }),
        duplicate_warnings: vec![],
    };
    assert!(!result.has_differences());

    // File difference
    let result = CompareResult {
        file_results: vec![(
            "DacMetadata.xml".to_string(),
            FileStatus::Different(vec!["diff".to_string()]),
        )],
        header_result: None,
        elements_result: None,
        duplicate_warnings: vec![],
    };
    assert!(result.has_differences());

    // Missing elements
    let result = CompareResult {
        file_results: vec![],
        header_result: Some(HeaderResult {
            is_ok: true,
            diffs: vec![],
        }),
        elements_result: Some(ModelElementsResult {
            total_rust: 4,
            total_dotnet: 5,
            missing_in_rust: vec![ElementKey::Named {
                element_type: "SqlTable".to_string(),
                name: "[dbo].[T]".to_string(),
            }],
            extra_in_rust: vec![],
            differences: vec![],
        }),
        duplicate_warnings: vec![],
    };
    assert!(result.has_differences());
}
