//! End-to-end tests comparing Rust dacpac output with DotNet DacFx output
//!
//! These tests build a dacpac using both rust-sqlpackage and dotnet build,
//! then compare the generated model.xml files to verify compatibility.
//!
//! Prerequisites:
//! - dotnet SDK installed with Microsoft.Build.Sql
//!
//! The test project can be specified via environment variable:
//!   SQL_TEST_PROJECT=/path/to/YourProject.sqlproj cargo test --test e2e_tests -- --ignored
//!
//! If not specified, falls back to the e2e_comprehensive fixture in tests/fixtures.
//!
//! Run with:
//!   cargo test --test e2e_tests -- --ignored dotnet_comparison

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;
use zip::ZipArchive;

/// Get the test project path from environment variable or use e2e_comprehensive fixture
fn get_test_project_path() -> Option<PathBuf> {
    // First check for SQL_TEST_PROJECT environment variable
    if let Ok(custom_path) = std::env::var("SQL_TEST_PROJECT") {
        let path = PathBuf::from(&custom_path);
        if path.exists() {
            return Some(path);
        } else {
            eprintln!("Warning: SQL_TEST_PROJECT path does not exist: {}", custom_path);
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

/// Extract model.xml from a dacpac
fn extract_model_xml(dacpac_path: &std::path::Path) -> Result<String, String> {
    let file =
        fs::File::open(dacpac_path).map_err(|e| format!("Failed to open dacpac: {}", e))?;

    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

        if file.name() == "model.xml" {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| format!("Failed to read model.xml: {}", e))?;
            return Ok(content);
        }
    }

    Err("model.xml not found in dacpac".to_string())
}

/// Count occurrences of an element type in model.xml
fn count_elements(model_xml: &str, element_type: &str) -> usize {
    let pattern = format!("Type=\"{}\"", element_type);
    model_xml.matches(&pattern).count()
}

/// Comparison result between Rust and DotNet dacpacs
#[derive(Debug)]
struct DacpacComparison {
    rust_tables: usize,
    dotnet_tables: usize,
    rust_indexes: usize,
    dotnet_indexes: usize,
    rust_constraints: usize,
    dotnet_constraints: usize,
    rust_defaults: usize,
    dotnet_defaults: usize,
    rust_procedures: usize,
    dotnet_procedures: usize,
    rust_functions: usize,
    dotnet_functions: usize,
    rust_views: usize,
    dotnet_views: usize,
    rust_parameters: usize,
    dotnet_parameters: usize,
    dotnet_has_header: bool,
    rust_has_header: bool,
    dotnet_has_db_options: bool,
    rust_has_db_options: bool,
    dotnet_inline_annotations: usize,
    rust_inline_annotations: usize,
    dotnet_computed_columns: usize,
    rust_computed_columns: usize,
    dotnet_extended_properties: usize,
    rust_extended_properties: usize,
}

impl DacpacComparison {
    fn from_model_xmls(rust_xml: &str, dotnet_xml: &str) -> Self {
        Self {
            rust_tables: count_elements(rust_xml, "SqlTable"),
            dotnet_tables: count_elements(dotnet_xml, "SqlTable"),
            rust_indexes: count_elements(rust_xml, "SqlIndex"),
            dotnet_indexes: count_elements(dotnet_xml, "SqlIndex"),
            rust_constraints: count_elements(rust_xml, "SqlPrimaryKeyConstraint")
                + count_elements(rust_xml, "SqlForeignKeyConstraint")
                + count_elements(rust_xml, "SqlUniqueConstraint")
                + count_elements(rust_xml, "SqlCheckConstraint"),
            dotnet_constraints: count_elements(dotnet_xml, "SqlPrimaryKeyConstraint")
                + count_elements(dotnet_xml, "SqlForeignKeyConstraint")
                + count_elements(dotnet_xml, "SqlUniqueConstraint")
                + count_elements(dotnet_xml, "SqlCheckConstraint"),
            rust_defaults: count_elements(rust_xml, "SqlDefaultConstraint"),
            dotnet_defaults: count_elements(dotnet_xml, "SqlDefaultConstraint"),
            rust_procedures: count_elements(rust_xml, "SqlProcedure"),
            dotnet_procedures: count_elements(dotnet_xml, "SqlProcedure"),
            rust_functions: count_elements(rust_xml, "SqlScalarFunction")
                + count_elements(rust_xml, "SqlMultiStatementTableValuedFunction"),
            dotnet_functions: count_elements(dotnet_xml, "SqlScalarFunction")
                + count_elements(dotnet_xml, "SqlMultiStatementTableValuedFunction"),
            rust_views: count_elements(rust_xml, "SqlView"),
            dotnet_views: count_elements(dotnet_xml, "SqlView"),
            rust_parameters: count_elements(rust_xml, "SqlSubroutineParameter"),
            dotnet_parameters: count_elements(dotnet_xml, "SqlSubroutineParameter"),
            dotnet_has_header: dotnet_xml.contains("<Header>"),
            rust_has_header: rust_xml.contains("<Header>"),
            dotnet_has_db_options: dotnet_xml.contains("SqlDatabaseOptions"),
            rust_has_db_options: rust_xml.contains("SqlDatabaseOptions"),
            dotnet_inline_annotations: count_elements(dotnet_xml, "SqlInlineConstraintAnnotation"),
            rust_inline_annotations: count_elements(rust_xml, "SqlInlineConstraintAnnotation"),
            dotnet_computed_columns: count_elements(dotnet_xml, "SqlComputedColumn"),
            rust_computed_columns: count_elements(rust_xml, "SqlComputedColumn"),
            dotnet_extended_properties: count_elements(dotnet_xml, "SqlExtendedProperty"),
            rust_extended_properties: count_elements(rust_xml, "SqlExtendedProperty"),
        }
    }

    fn print_report(&self) {
        println!("\n=== Dacpac Comparison Report ===\n");
        println!(
            "| Element Type                  | Rust   | DotNet | Diff   | % Coverage |"
        );
        println!(
            "|-------------------------------|--------|--------|--------|------------|"
        );

        self.print_row("SqlTable", self.rust_tables, self.dotnet_tables);
        self.print_row("SqlIndex", self.rust_indexes, self.dotnet_indexes);
        self.print_row("Constraints (PK/FK/UQ/CK)", self.rust_constraints, self.dotnet_constraints);
        self.print_row("SqlDefaultConstraint", self.rust_defaults, self.dotnet_defaults);
        self.print_row("SqlProcedure", self.rust_procedures, self.dotnet_procedures);
        self.print_row("Functions (Scalar/TVF)", self.rust_functions, self.dotnet_functions);
        self.print_row("SqlView", self.rust_views, self.dotnet_views);
        self.print_row("SqlSubroutineParameter", self.rust_parameters, self.dotnet_parameters);
        self.print_row(
            "SqlInlineConstraintAnnotation",
            self.rust_inline_annotations,
            self.dotnet_inline_annotations,
        );
        self.print_row(
            "SqlComputedColumn",
            self.rust_computed_columns,
            self.dotnet_computed_columns,
        );
        self.print_row(
            "SqlExtendedProperty",
            self.rust_extended_properties,
            self.dotnet_extended_properties,
        );

        println!();
        println!("Header section: Rust={}, DotNet={}", self.rust_has_header, self.dotnet_has_header);
        println!(
            "SqlDatabaseOptions: Rust={}, DotNet={}",
            self.rust_has_db_options, self.dotnet_has_db_options
        );
    }

    fn print_row(&self, name: &str, rust: usize, dotnet: usize) {
        let diff = dotnet as i64 - rust as i64;
        let coverage = if dotnet > 0 {
            (rust as f64 / dotnet as f64 * 100.0) as i32
        } else if rust > 0 {
            100 // Rust has more
        } else {
            100 // Both zero
        };

        println!(
            "| {:<29} | {:>6} | {:>6} | {:>+6} | {:>9}% |",
            name, rust, dotnet, diff, coverage
        );
    }
}

// ============================================================================
// E2E Comparison Tests
// ============================================================================

/// Compare dacpac output between Rust and DotNet
#[test]
#[ignore = "Requires dotnet SDK"]
fn test_compare_dacpacs() {
    if !dotnet_available() {
        eprintln!("Skipping: dotnet SDK not available");
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => {
            eprintln!("Skipping: No test project found. Set SQL_TEST_PROJECT or ensure e2e_comprehensive fixture exists.");
            return;
        }
    };

    eprintln!("Using test project: {:?}", project_path);

    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Build with Rust
    let rust_dacpac = temp_dir.path().join("rust.dacpac");
    let rust_result = rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path: project_path.clone(),
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    });

    assert!(rust_result.is_ok(), "Rust build should succeed: {:?}", rust_result.err());

    // Build with DotNet
    let dotnet_output = Command::new("dotnet")
        .arg("build")
        .arg(&project_path)
        .output()
        .expect("Failed to run dotnet build");

    if !dotnet_output.status.success() {
        eprintln!(
            "DotNet build failed: {}",
            String::from_utf8_lossy(&dotnet_output.stderr)
        );
        return;
    }

    // Find the dotnet dacpac
    let dotnet_dacpac = get_dotnet_dacpac_path(&project_path);

    if !dotnet_dacpac.exists() {
        eprintln!("DotNet dacpac not found at {:?}", dotnet_dacpac);
        return;
    }

    // Extract and compare model.xml
    let rust_xml = extract_model_xml(&rust_dacpac).expect("Should extract rust model.xml");
    let dotnet_xml = extract_model_xml(&dotnet_dacpac).expect("Should extract dotnet model.xml");

    let comparison = DacpacComparison::from_model_xmls(&rust_xml, &dotnet_xml);
    comparison.print_report();

    // Assertions for critical compatibility
    assert_eq!(
        comparison.rust_tables, comparison.dotnet_tables,
        "Table count should match"
    );
    assert_eq!(
        comparison.rust_views, comparison.dotnet_views,
        "View count should match"
    );
    assert_eq!(
        comparison.rust_procedures, comparison.dotnet_procedures,
        "Procedure count should match"
    );
}

/// Test for missing Header section
#[test]
#[ignore = "Requires dotnet SDK"]
fn test_missing_header_section() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    let _ = rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path,
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    });

    let rust_xml = extract_model_xml(&rust_dacpac).expect("Should extract rust model.xml");

    // Check for Header section
    let has_header = rust_xml.contains("<Header>");
    let has_ansi_nulls = rust_xml.contains("AnsiNulls");
    let has_quoted_identifier = rust_xml.contains("QuotedIdentifier");
    let has_compat_mode = rust_xml.contains("CompatibilityMode");
    let has_reference = rust_xml.contains("Reference");
    let has_sqlcmd_vars = rust_xml.contains("SqlCmdVariable");

    println!("\n=== Header Section Analysis ===");
    println!("Has <Header> section: {}", has_header);
    println!("Has AnsiNulls: {}", has_ansi_nulls);
    println!("Has QuotedIdentifier: {}", has_quoted_identifier);
    println!("Has CompatibilityMode: {}", has_compat_mode);
    println!("Has Reference (master.dacpac): {}", has_reference);
    println!("Has SqlCmdVariables: {}", has_sqlcmd_vars);

    // Currently expected to fail - these are missing features
    assert!(has_header, "Rust dacpac should have Header section");
}

/// Test for missing SqlDatabaseOptions
#[test]
#[ignore = "Requires dotnet SDK"]
fn test_missing_database_options() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    let _ = rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path,
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    });

    let rust_xml = extract_model_xml(&rust_dacpac).expect("Should extract rust model.xml");

    let has_db_options = rust_xml.contains("SqlDatabaseOptions");
    let has_collation = rust_xml.contains("Collation");
    let has_ansi_settings =
        rust_xml.contains("IsAnsiNullsOn") || rust_xml.contains("IsAnsiWarningsOn");

    println!("\n=== Database Options Analysis ===");
    println!("Has SqlDatabaseOptions: {}", has_db_options);
    println!("Has Collation: {}", has_collation);
    println!("Has ANSI settings: {}", has_ansi_settings);

    // Currently expected to fail - this is a missing feature
    assert!(
        has_db_options,
        "Rust dacpac should have SqlDatabaseOptions element"
    );
}

/// Test for ampersand encoding bug
#[test]
#[ignore = "Requires dotnet SDK"]
fn test_ampersand_encoding_bug() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    let _ = rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path: project_path.clone(),
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    });

    let dotnet_dacpac = get_dotnet_dacpac_path(&project_path);

    let rust_xml = extract_model_xml(&rust_dacpac).expect("Should extract rust model.xml");
    let dotnet_xml = extract_model_xml(&dotnet_dacpac).expect("Should extract dotnet model.xml");

    // Check for procedures/views with ampersand in name (P&L in e2e_comprehensive)
    let dotnet_has_ampersand = dotnet_xml.contains("P&amp;L") || dotnet_xml.contains("Terms&amp;Conditions");
    let rust_has_ampersand = rust_xml.contains("P&amp;L") || rust_xml.contains("P&L")
        || rust_xml.contains("Terms&amp;Conditions") || rust_xml.contains("Terms&Conditions");
    let rust_truncated = rust_xml.contains("GetP\"") || rust_xml.contains("GetP<")
        || rust_xml.contains("Terms\"") || rust_xml.contains("Terms<");

    println!("\n=== Ampersand Encoding Analysis ===");
    println!("DotNet has ampersand in names: {}", dotnet_has_ampersand);
    println!("Rust has ampersand in names: {}", rust_has_ampersand);
    println!("Rust name is truncated: {}", rust_truncated);

    assert!(
        !rust_truncated,
        "Rust should not truncate names at ampersand"
    );
}

/// Test for index double-bracketing bug
#[test]
#[ignore = "Requires dotnet SDK"]
fn test_index_double_bracket_bug() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    let _ = rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path,
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    });

    let rust_xml = extract_model_xml(&rust_dacpac).expect("Should extract rust model.xml");

    // Check for double brackets [[IX_
    let has_double_brackets = rust_xml.contains("[[IX_") || rust_xml.contains(".[[");

    println!("\n=== Index Naming Analysis ===");
    println!("Has double brackets: {}", has_double_brackets);

    assert!(
        !has_double_brackets,
        "Index names should not have double brackets"
    );
}

/// Test for missing inline constraint annotations
#[test]
#[ignore = "Requires dotnet SDK"]
fn test_missing_inline_constraint_annotations() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    let _ = rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path: project_path.clone(),
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    });

    let dotnet_dacpac = get_dotnet_dacpac_path(&project_path);

    let rust_xml = extract_model_xml(&rust_dacpac).expect("Should extract rust model.xml");
    let dotnet_xml = extract_model_xml(&dotnet_dacpac).expect("Should extract dotnet model.xml");

    let rust_annotations = count_elements(&rust_xml, "SqlInlineConstraintAnnotation");
    let dotnet_annotations = count_elements(&dotnet_xml, "SqlInlineConstraintAnnotation");

    println!("\n=== Inline Constraint Annotation Analysis ===");
    println!("Rust SqlInlineConstraintAnnotation: {}", rust_annotations);
    println!("DotNet SqlInlineConstraintAnnotation: {}", dotnet_annotations);

    // Currently expected to fail - this is a missing feature
    assert!(
        rust_annotations > 0,
        "Rust should generate SqlInlineConstraintAnnotation elements"
    );
}

/// Test for default constraint count discrepancy
#[test]
#[ignore = "Requires dotnet SDK"]
fn test_default_constraint_coverage() {
    if !dotnet_available() {
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => return,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    let _ = rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path: project_path.clone(),
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    });

    let dotnet_dacpac = get_dotnet_dacpac_path(&project_path);

    let rust_xml = extract_model_xml(&rust_dacpac).expect("Should extract rust model.xml");
    let dotnet_xml = extract_model_xml(&dotnet_dacpac).expect("Should extract dotnet model.xml");

    let rust_defaults = count_elements(&rust_xml, "SqlDefaultConstraint");
    let dotnet_defaults = count_elements(&dotnet_xml, "SqlDefaultConstraint");

    let coverage = if dotnet_defaults > 0 {
        (rust_defaults as f64 / dotnet_defaults as f64 * 100.0) as i32
    } else {
        100
    };

    println!("\n=== Default Constraint Coverage ===");
    println!("Rust SqlDefaultConstraint: {}", rust_defaults);
    println!("DotNet SqlDefaultConstraint: {}", dotnet_defaults);
    println!("Coverage: {}%", coverage);

    // Currently ~45% coverage
    assert!(
        coverage >= 90,
        "Default constraint coverage should be at least 90%, got {}%",
        coverage
    );
}
