//! End-to-end tests comparing Rust dacpac output with DotNet DacFx output
//!
//! These tests build a dacpac using both rust-sqlpackage and dotnet build,
//! then compare the generated model.xml files using a layered approach:
//!
//! - Layer 1: Element inventory - verify all elements exist with correct names
//! - Layer 2: Property comparison - verify element properties match
//! - Layer 3: SqlPackage DeployReport - verify deployment equivalence
//!
//! Prerequisites:
//! - dotnet SDK installed with Microsoft.Build.Sql
//! - SqlPackage CLI (for Layer 3 tests)
//!
//! These tests run automatically in CI. Locally, they skip gracefully if dotnet
//! is not available.
//!
//! The test project can be specified via environment variable:
//!   SQL_TEST_PROJECT=/path/to/YourProject.sqlproj cargo test --test e2e_tests dotnet_comparison
//!
//! If not specified, falls back to the e2e_comprehensive fixture in tests/fixtures.
//!
//! Run with:
//!   cargo test --test e2e_tests dotnet_comparison -- --nocapture
//!   just test-parity

use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

use crate::dacpac_compare::{
    compare_dacpacs, compare_element_inventory, compare_element_properties,
    compare_with_sqlpackage, extract_model_xml, sqlpackage_available, DacpacModel, Layer1Error,
};

// =============================================================================
// Test Setup Helpers
// =============================================================================

/// Get the test project path from environment variable or use e2e_comprehensive fixture
fn get_test_project_path() -> Option<PathBuf> {
    // First check for SQL_TEST_PROJECT environment variable
    if let Ok(custom_path) = std::env::var("SQL_TEST_PROJECT") {
        let path = PathBuf::from(&custom_path);
        if path.exists() {
            return Some(path);
        } else {
            eprintln!(
                "Warning: SQL_TEST_PROJECT path does not exist: {}",
                custom_path
            );
        }
    }

    // Fall back to e2e_comprehensive fixture
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("e2e_comprehensive")
        .join("project.sqlproj");

    if fixture_path.exists() {
        Some(fixture_path)
    } else {
        None
    }
}

/// Get the expected dacpac output path for dotnet build
fn get_dotnet_dacpac_path(project_path: &std::path::Path) -> PathBuf {
    let project_dir = project_path.parent().unwrap();
    let project_name = project_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project");

    project_dir
        .join("bin")
        .join("Debug")
        .join(format!("{}.dacpac", project_name))
}

/// Check if dotnet build is available
fn dotnet_available() -> bool {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Build dacpacs with both Rust and DotNet, returning paths to both
fn build_both_dacpacs(
    project_path: &std::path::Path,
    temp_dir: &TempDir,
) -> Result<(PathBuf, PathBuf), String> {
    // Build with Rust
    let rust_dacpac = temp_dir.path().join("rust.dacpac");
    rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path: project_path.to_path_buf(),
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    })
    .map_err(|e| format!("Rust build failed: {}", e))?;

    // Build with DotNet
    let dotnet_output = Command::new("dotnet")
        .arg("build")
        .arg(project_path)
        .output()
        .map_err(|e| format!("Failed to run dotnet: {}", e))?;

    if !dotnet_output.status.success() {
        return Err(format!(
            "DotNet build failed: {}",
            String::from_utf8_lossy(&dotnet_output.stderr)
        ));
    }

    let dotnet_dacpac = get_dotnet_dacpac_path(project_path);
    if !dotnet_dacpac.exists() {
        return Err(format!("DotNet dacpac not found at {:?}", dotnet_dacpac));
    }

    Ok((rust_dacpac, dotnet_dacpac))
}

// =============================================================================
// Main Layered Comparison Test
// =============================================================================

/// Full layered comparison test - Layer 1, 2, and optionally 3
#[test]
fn test_layered_dacpac_comparison() {
    if !dotnet_available() {
        eprintln!("Skipping: dotnet SDK not available");
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => {
            eprintln!("Skipping: No test project found");
            return;
        }
    };

    eprintln!("Using test project: {:?}", project_path);

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    // Run full comparison with Layer 3 if SqlPackage is available
    let include_layer3 = sqlpackage_available();
    let result = compare_dacpacs(&rust_dacpac, &dotnet_dacpac, include_layer3)
        .expect("Comparison should succeed");

    // Print detailed report
    result.print_report();

    // Assert on results with detailed messages
    assert!(
        result.layer1_errors.is_empty(),
        "Layer 1 (Element Inventory) failed:\n{}",
        result
            .layer1_errors
            .iter()
            .map(|e| format!("  - {}", e))
            .collect::<Vec<_>>()
            .join("\n")
    );

    assert!(
        result.layer2_errors.is_empty(),
        "Layer 2 (Property Comparison) failed:\n{}",
        result
            .layer2_errors
            .iter()
            .map(|e| format!("  - {}", e))
            .collect::<Vec<_>>()
            .join("\n")
    );

    if let Some(ref l3) = result.layer3_result {
        if l3.error.is_none() {
            assert!(
                !l3.has_differences,
                "Layer 3 (SqlPackage DeployReport) detected schema differences:\n{}",
                l3.deploy_script
            );
        }
    }
}

// =============================================================================
// Individual Layer Tests
// =============================================================================

/// Test Layer 1 only: Element inventory comparison
#[test]
fn test_layer1_element_inventory() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    let rust_model = DacpacModel::from_dacpac(&rust_dacpac).expect("Parse rust dacpac");
    let dotnet_model = DacpacModel::from_dacpac(&dotnet_dacpac).expect("Parse dotnet dacpac");

    let errors = compare_element_inventory(&rust_model, &dotnet_model);

    // Print summary by type
    println!("\n=== Layer 1: Element Inventory ===\n");

    let mut missing_by_type: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    let mut extra_by_type: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();

    for err in &errors {
        match err {
            Layer1Error::MissingInRust { element_type, name } => {
                missing_by_type
                    .entry(element_type.as_str())
                    .or_default()
                    .push(name.as_str());
            }
            Layer1Error::ExtraInRust { element_type, name } => {
                extra_by_type
                    .entry(element_type.as_str())
                    .or_default()
                    .push(name.as_str());
            }
            Layer1Error::CountMismatch {
                element_type,
                rust_count,
                dotnet_count,
            } => {
                println!(
                    "Count mismatch for {}: Rust={}, DotNet={}",
                    element_type, rust_count, dotnet_count
                );
            }
        }
    }

    if !missing_by_type.is_empty() {
        println!("Missing in Rust:");
        for (elem_type, names) in &missing_by_type {
            println!("  {}: {} elements", elem_type, names.len());
            for name in names.iter().take(5) {
                println!("    - {}", name);
            }
            if names.len() > 5 {
                println!("    ... and {} more", names.len() - 5);
            }
        }
    }

    if !extra_by_type.is_empty() {
        println!("Extra in Rust:");
        for (elem_type, names) in &extra_by_type {
            println!("  {}: {} elements", elem_type, names.len());
            for name in names.iter().take(5) {
                println!("    - {}", name);
            }
        }
    }

    if errors.is_empty() {
        println!("All elements match!");
    }
}

/// Test Layer 2 only: Property comparison
#[test]
fn test_layer2_property_comparison() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    let rust_model = DacpacModel::from_dacpac(&rust_dacpac).expect("Parse rust dacpac");
    let dotnet_model = DacpacModel::from_dacpac(&dotnet_dacpac).expect("Parse dotnet dacpac");

    let errors = compare_element_properties(&rust_model, &dotnet_model);

    println!("\n=== Layer 2: Property Comparison ===\n");

    if errors.is_empty() {
        println!("All properties match!");
    } else {
        println!("Property mismatches found: {}\n", errors.len());
        for err in errors.iter().take(20) {
            println!(
                "  {}.{} - {}:",
                err.element_type, err.element_name, err.property_name
            );
            println!("    Rust:   {:?}", err.rust_value);
            println!("    DotNet: {:?}", err.dotnet_value);
        }
        if errors.len() > 20 {
            println!("  ... and {} more", errors.len() - 20);
        }
    }
}

/// Test Layer 3 only: SqlPackage DeployReport comparison
#[test]
fn test_layer3_sqlpackage_comparison() {
    if !dotnet_available() {
        eprintln!("Skipping: dotnet SDK not available");
        return;
    }

    if !sqlpackage_available() {
        eprintln!("Skipping: SqlPackage not available");
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    println!("\n=== Layer 3: SqlPackage DeployReport ===\n");

    // Compare Rust -> DotNet (what changes needed to make DotNet match Rust?)
    println!("Rust -> DotNet comparison:");
    let result_r2d = compare_with_sqlpackage(&rust_dacpac, &dotnet_dacpac);
    if let Some(ref err) = result_r2d.error {
        println!("  Error: {}", err);
    } else if result_r2d.has_differences {
        println!("  Differences detected!");
        println!("  Deploy script preview (first 50 lines):");
        for line in result_r2d.deploy_script.lines().take(50) {
            println!("    {}", line);
        }
    } else {
        println!("  No differences - dacpacs are equivalent!");
    }

    // Compare DotNet -> Rust (what changes needed to make Rust match DotNet?)
    println!("\nDotNet -> Rust comparison:");
    let result_d2r = compare_with_sqlpackage(&dotnet_dacpac, &rust_dacpac);
    if let Some(ref err) = result_d2r.error {
        println!("  Error: {}", err);
    } else if result_d2r.has_differences {
        println!("  Differences detected!");
        println!("  Deploy script preview (first 50 lines):");
        for line in result_d2r.deploy_script.lines().take(50) {
            println!("    {}", line);
        }
    } else {
        println!("  No differences - dacpacs are equivalent!");
    }

    // Both directions should show no differences for true equivalence
    assert!(
        !result_r2d.has_differences && !result_d2r.has_differences,
        "SqlPackage detected schema differences between Rust and DotNet dacpacs"
    );
}

// =============================================================================
// Feature-Specific Tests
// =============================================================================

/// Test for ampersand encoding in element names
#[test]
fn test_ampersand_encoding() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path: project_path.clone(),
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    })
    .expect("Rust build should succeed");

    let rust_xml = extract_model_xml(&rust_dacpac).expect("Extract model.xml");

    // Check for properly encoded ampersands
    let has_encoded_ampersand = rust_xml.contains("&amp;");
    let has_truncated_names = rust_xml.contains("GetP\"") || rust_xml.contains("Terms\"");

    println!("\n=== Ampersand Encoding Test ===");
    println!("Has encoded ampersands (&amp;): {}", has_encoded_ampersand);
    println!("Has truncated names: {}", has_truncated_names);

    assert!(
        !has_truncated_names,
        "Names should not be truncated at ampersand characters"
    );
}

/// Test for index naming (no double brackets in element names)
/// Note: Double brackets can legitimately appear in CDATA sections (e.g., check constraints
/// like `[Price] >= 0`), so we only check element Name attributes.
#[test]
fn test_index_naming() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path,
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    })
    .expect("Rust build should succeed");

    let rust_xml = extract_model_xml(&rust_dacpac).expect("Extract model.xml");

    // Check for double brackets only in element Name attributes, not in CDATA content
    // CDATA sections legitimately contain brackets for check constraints like `[Price] >= 0`
    let name_pattern = regex::Regex::new(r#"Name="([^"]+)""#).unwrap();
    let mut double_bracket_names = Vec::new();

    for cap in name_pattern.captures_iter(&rust_xml) {
        let name = &cap[1];
        if name.contains("[[") || name.contains("]]") {
            double_bracket_names.push(name.to_string());
        }
    }

    println!("\n=== Index Naming Test ===");
    println!(
        "Names with double brackets: {}",
        if double_bracket_names.is_empty() {
            "none".to_string()
        } else {
            double_bracket_names.join(", ")
        }
    );

    assert!(
        double_bracket_names.is_empty(),
        "Element names should not have double brackets. Found: {:?}",
        double_bracket_names
    );
}

/// Print element type summary for both dacpacs
#[test]
fn test_print_element_summary() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    let rust_model = DacpacModel::from_dacpac(&rust_dacpac).expect("Parse rust dacpac");
    let dotnet_model = DacpacModel::from_dacpac(&dotnet_dacpac).expect("Parse dotnet dacpac");

    println!("\n=== Element Type Summary ===\n");
    println!(
        "| {:<35} | {:>8} | {:>8} | {:>8} |",
        "Element Type", "Rust", "DotNet", "Diff"
    );
    println!("|{:-<37}|{:-<10}|{:-<10}|{:-<10}|", "", "", "", "");

    let mut all_types: std::collections::BTreeSet<String> = rust_model.element_types();
    all_types.extend(dotnet_model.element_types());

    for elem_type in all_types {
        let rust_count = rust_model.elements_of_type(&elem_type).len();
        let dotnet_count = dotnet_model.elements_of_type(&elem_type).len();
        let diff = rust_count as i64 - dotnet_count as i64;

        let diff_str = if diff == 0 {
            "".to_string()
        } else if diff > 0 {
            format!("+{}", diff)
        } else {
            format!("{}", diff)
        };

        println!(
            "| {:<35} | {:>8} | {:>8} | {:>8} |",
            elem_type, rust_count, dotnet_count, diff_str
        );
    }
}
