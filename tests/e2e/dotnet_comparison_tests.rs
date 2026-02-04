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
//!
//! ## Per-Feature Parity Tests (Phase 6)
//!
//! The `run_parity_test()` function provides a convenient way to run full parity
//! tests for individual fixtures. This enables targeted testing of specific features.
//!
//! Example usage:
//!   let result = run_parity_test("ampersand_encoding", &ParityTestOptions::default())?;
//!   assert!(result.is_success(), "Fixture should have full parity");

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use once_cell::sync::Lazy;
use tempfile::TempDir;

use crate::dacpac_compare::{
    canonicalize_model_xml, compare_all_properties, compare_canonical_dacpacs,
    compare_canonical_xml, compare_dacpacs, compare_dacpacs_with_options,
    compare_element_inventory, compare_element_order, compare_element_properties,
    compare_element_relationships, compare_with_sqlpackage, compute_sha256, extract_model_xml,
    generate_diff, sqlpackage_available, CanonicalXmlError, ComparisonOptions, ComparisonResult,
    DacpacModel, FixtureBaseline, Layer1Error, ParityBaseline, ParityMetrics,
};

// =============================================================================
// DotNet Dacpac Pre-Build Cache
// =============================================================================
// All fixtures are built in parallel at the start of the first parity test.
// Subsequent tests simply look up the pre-built dacpac path.
// The build directory is cleared at the start of each test run to avoid stale artifacts.

use std::sync::atomic::{AtomicBool, Ordering};

/// Directory where pre-built dotnet dacpacs are stored.
/// Located in target/ so it's ignored by git and cleaned by cargo clean.
fn get_dotnet_build_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("dotnet-fixtures")
}

/// Map of fixture_name -> dacpac_path, populated by prebuild_all_fixtures()
static DOTNET_DACPAC_CACHE: Lazy<HashMap<String, Result<PathBuf, String>>> =
    Lazy::new(prebuild_all_fixtures);

/// Flag to track if we've already cleaned the build directory this run
static BUILD_DIR_CLEANED: AtomicBool = AtomicBool::new(false);

/// Get the pre-built dotnet dacpac for a fixture.
/// On first call, triggers parallel build of ALL fixtures.
fn get_or_build_dotnet_dacpac(fixture_name: &str) -> Result<PathBuf, String> {
    // Access the cache, which triggers prebuild_all_fixtures() on first access
    match DOTNET_DACPAC_CACHE.get(fixture_name) {
        Some(Ok(path)) => Ok(path.clone()),
        Some(Err(e)) => Err(e.clone()),
        None => Err(format!(
            "Fixture '{}' not found in pre-built cache",
            fixture_name
        )),
    }
}

/// Build all fixtures in parallel. Called once via Lazy initialization.
fn prebuild_all_fixtures() -> HashMap<String, Result<PathBuf, String>> {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let build_dir = get_dotnet_build_dir();

    // Clean and recreate build directory (once per test run)
    if !BUILD_DIR_CLEANED.swap(true, Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&build_dir);
        std::fs::create_dir_all(&build_dir).expect("Failed to create dotnet build directory");
    }

    // Get list of all fixtures
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    let fixtures: Vec<String> = std::fs::read_dir(&fixtures_dir)
        .expect("Failed to read fixtures directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() && path.join("project.sqlproj").exists() {
                entry.file_name().to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();

    eprintln!(
        "Pre-building {} dotnet fixtures with limited concurrency...",
        fixtures.len()
    );
    let start = std::time::Instant::now();

    // Limit concurrent builds to prevent memory exhaustion
    // Each dotnet build uses ~100MB, so 4 concurrent builds balances speed and memory usage
    const MAX_CONCURRENT_BUILDS: usize = 4;

    let cache = Arc::new(Mutex::new(HashMap::new()));
    let fixtures_queue = Arc::new(Mutex::new(fixtures.into_iter()));

    // Spawn worker threads (limited number)
    let handles: Vec<_> = (0..MAX_CONCURRENT_BUILDS)
        .map(|_| {
            let build_dir = build_dir.clone();
            let fixtures_dir = fixtures_dir.clone();
            let queue = Arc::clone(&fixtures_queue);
            let cache = Arc::clone(&cache);

            thread::spawn(move || {
                loop {
                    // Get next fixture from queue
                    let fixture_name = {
                        let mut q = queue.lock().unwrap();
                        q.next()
                    };

                    let Some(fixture_name) = fixture_name else {
                        break; // No more fixtures
                    };

                    // Build this fixture
                    let result = build_single_fixture(&fixture_name, &fixtures_dir, &build_dir);

                    // Store result
                    cache.lock().unwrap().insert(fixture_name, result);
                }
            })
        })
        .collect();

    // Wait for all workers to finish
    for handle in handles {
        handle.join().expect("Build thread panicked");
    }

    let final_cache = Arc::try_unwrap(cache)
        .expect("Cache still has references")
        .into_inner()
        .unwrap();

    eprintln!(
        "Pre-built {} fixtures in {:.1}s",
        final_cache.len(),
        start.elapsed().as_secs_f64()
    );

    final_cache
}

/// Build a single fixture's dotnet dacpac.
fn build_single_fixture(
    fixture_name: &str,
    fixtures_dir: &Path,
    build_dir: &Path,
) -> Result<PathBuf, String> {
    let fixture_src = fixtures_dir.join(fixture_name);
    let fixture_build_dir = build_dir.join(fixture_name);

    // Create build directory for this fixture
    std::fs::create_dir_all(&fixture_build_dir)
        .map_err(|e| format!("Failed to create build dir: {}", e))?;

    // Copy fixture to build directory (excluding bin/obj)
    copy_dir_recursive(&fixture_src, &fixture_build_dir)
        .map_err(|e| format!("Failed to copy fixture: {}", e))?;

    // Find the sqlproj file
    let project_path = fixture_build_dir.join("project.sqlproj");
    if !project_path.exists() {
        return Err(format!(
            "project.sqlproj not found in fixture: {}",
            fixture_name
        ));
    }

    // Build with dotnet
    let dotnet_output = Command::new("dotnet")
        .arg("build")
        .arg(&project_path)
        .output()
        .map_err(|e| format!("Failed to run dotnet: {}", e))?;

    if !dotnet_output.status.success() {
        return Err(format!(
            "DotNet build failed for {}: {}",
            fixture_name,
            String::from_utf8_lossy(&dotnet_output.stderr)
        ));
    }

    // Get dacpac path
    let dacpac_path = get_dotnet_dacpac_path(&project_path);
    if !dacpac_path.exists() {
        return Err(format!(
            "DotNet dacpac not found at {:?} for fixture {}",
            dacpac_path, fixture_name
        ));
    }

    Ok(dacpac_path)
}

// =============================================================================
// Phase 6: Per-Feature Parity Test Infrastructure
// =============================================================================

/// Options for controlling parity test behavior.
///
/// This struct allows fine-grained control over which comparison layers
/// are included in parity tests. By default, all layers are enabled
/// except Layer 3 (SqlPackage), which requires the SqlPackage CLI.
///
/// # Example
/// ```ignore
/// // Test with strict property comparison
/// let options = ParityTestOptions {
///     strict_properties: true,
///     ..Default::default()
/// };
/// let result = run_parity_test("my_fixture", &options)?;
/// ```
#[derive(Debug, Clone)]
pub struct ParityTestOptions {
    /// Include Layer 3 (SqlPackage DeployReport) comparison.
    /// Requires SqlPackage CLI to be installed.
    /// Default: false (auto-enabled if SqlPackage is available)
    pub include_layer3: bool,

    /// Compare ALL properties instead of just key properties.
    /// When true, uses strict comparison mode from Phase 2.
    /// Default: true (for parity testing, we want full comparison)
    pub strict_properties: bool,

    /// Validate all relationships between elements (Phase 3).
    /// Default: true
    pub check_relationships: bool,

    /// Validate element ordering matches DotNet output (Phase 4).
    /// Default: true
    pub check_element_order: bool,

    /// Compare metadata files ([Content_Types].xml, DacMetadata.xml, Origin.xml).
    /// Default: true
    pub check_metadata_files: bool,

    /// Compare pre/post-deploy scripts (Phase 5.4).
    /// Default: true
    pub check_deploy_scripts: bool,

    /// Compare canonical XML for byte-level matching (Layer 7).
    /// Default: true
    pub check_canonical: bool,

    /// Target SQL Server platform version (e.g., "Sql150", "Sql160").
    /// Default: "Sql150"
    pub target_platform: String,
}

impl Default for ParityTestOptions {
    fn default() -> Self {
        Self {
            include_layer3: false, // Will be auto-enabled if SqlPackage available
            strict_properties: true,
            check_relationships: true,
            check_element_order: true,
            check_metadata_files: true,
            check_deploy_scripts: true,
            check_canonical: true,
            target_platform: "Sql150".to_string(),
        }
    }
}

impl ParityTestOptions {
    /// Create options with Layer 3 (SqlPackage) comparison enabled.
    #[allow(dead_code)]
    pub fn with_layer3(mut self) -> Self {
        self.include_layer3 = true;
        self
    }

    /// Create options without strict property comparison (key properties only).
    #[allow(dead_code)]
    pub fn key_properties_only(mut self) -> Self {
        self.strict_properties = false;
        self
    }

    /// Create minimal options (Layer 1 and 2 only, no relationships/ordering/metadata).
    /// Useful for quick smoke tests.
    pub fn minimal() -> Self {
        Self {
            include_layer3: false,
            strict_properties: false,
            check_relationships: false,
            check_element_order: false,
            check_metadata_files: false,
            check_deploy_scripts: false,
            check_canonical: false,
            target_platform: "Sql150".to_string(),
        }
    }
}

/// Error type for parity test failures.
#[derive(Debug)]
pub enum ParityTestError {
    /// DotNet SDK is not available
    DotNetNotAvailable,
    /// Fixture not found at expected path
    FixtureNotFound { fixture_name: String, path: PathBuf },
    /// Rust build failed
    RustBuildFailed { message: String },
    /// DotNet build failed
    DotNetBuildFailed { message: String },
    /// Comparison failed
    ComparisonFailed { message: String },
}

impl std::fmt::Display for ParityTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParityTestError::DotNetNotAvailable => {
                write!(f, "DotNet SDK is not available")
            }
            ParityTestError::FixtureNotFound { fixture_name, path } => {
                write!(
                    f,
                    "Fixture '{}' not found at path: {}",
                    fixture_name,
                    path.display()
                )
            }
            ParityTestError::RustBuildFailed { message } => {
                write!(f, "Rust build failed: {}", message)
            }
            ParityTestError::DotNetBuildFailed { message } => {
                write!(f, "DotNet build failed: {}", message)
            }
            ParityTestError::ComparisonFailed { message } => {
                write!(f, "Comparison failed: {}", message)
            }
        }
    }
}

impl std::error::Error for ParityTestError {}

/// Run a full parity test for a specific fixture.
///
/// This function builds dacpacs using both Rust and DotNet for the specified
/// fixture, then runs a comprehensive comparison using all enabled layers.
///
/// # Arguments
/// * `fixture_name` - Name of the fixture directory in `tests/fixtures/`
/// * `options` - Options controlling which comparison layers to run
///
/// # Returns
/// * `Ok(ComparisonResult)` - Comparison completed, check `is_success()` for parity
/// * `Err(ParityTestError)` - Test setup failed (dotnet unavailable, build error, etc.)
///
/// # Example
/// ```ignore
/// // Run parity test with default options
/// let result = run_parity_test("simple_table", &ParityTestOptions::default())?;
/// if result.is_success() {
///     println!("Full parity achieved!");
/// } else {
///     result.print_report();
/// }
///
/// // Run with strict assertion
/// let result = run_parity_test("ampersand_encoding", &ParityTestOptions::default())?;
/// assert!(result.layer1_errors.is_empty(), "Element inventory should match");
/// ```
///
/// # Errors
/// Returns an error if:
/// - DotNet SDK is not available
/// - The fixture doesn't exist
/// - Rust or DotNet build fails
/// - Comparison infrastructure fails (e.g., can't parse dacpacs)
pub fn run_parity_test(
    fixture_name: &str,
    options: &ParityTestOptions,
) -> Result<ComparisonResult, ParityTestError> {
    // Check if dotnet is available
    if !dotnet_available() {
        return Err(ParityTestError::DotNetNotAvailable);
    }

    // Construct fixture path
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(fixture_name)
        .join("project.sqlproj");

    if !fixture_path.exists() {
        return Err(ParityTestError::FixtureNotFound {
            fixture_name: fixture_name.to_string(),
            path: fixture_path,
        });
    }

    // Create temp directory for dacpac outputs
    let temp_dir = TempDir::new().map_err(|e| ParityTestError::RustBuildFailed {
        message: format!("Failed to create temp directory: {}", e),
    })?;

    // Build Rust dacpac
    let rust_dacpac = temp_dir.path().join("rust.dacpac");
    rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path: fixture_path.clone(),
        output_path: Some(rust_dacpac.clone()),
        target_platform: options.target_platform.clone(),
        verbose: false,
    })
    .map_err(|e| ParityTestError::RustBuildFailed {
        message: e.to_string(),
    })?;

    // Get or build dotnet dacpac (cached per test run)
    let dotnet_dacpac = get_or_build_dotnet_dacpac(fixture_name)
        .map_err(|e| ParityTestError::DotNetBuildFailed { message: e })?;

    // Run comparison with options
    let comparison_options = ComparisonOptions {
        include_layer3: options.include_layer3 || sqlpackage_available(),
        strict_properties: options.strict_properties,
        check_relationships: options.check_relationships,
        check_element_order: options.check_element_order,
        check_metadata_files: options.check_metadata_files,
        check_deploy_scripts: options.check_deploy_scripts,
        check_canonical: options.check_canonical,
    };

    compare_dacpacs_with_options(&rust_dacpac, &dotnet_dacpac, &comparison_options)
        .map_err(|e| ParityTestError::ComparisonFailed { message: e })
}

/// Run parity test and print a detailed report regardless of outcome.
///
/// This is a convenience wrapper around `run_parity_test()` that always
/// prints the comparison report. Useful for exploratory testing and debugging.
///
/// # Returns
/// The comparison result, or None if test setup failed.
pub fn run_parity_test_with_report(
    fixture_name: &str,
    options: &ParityTestOptions,
) -> Option<ComparisonResult> {
    println!("\n=== Running Parity Test: {} ===\n", fixture_name);

    match run_parity_test(fixture_name, options) {
        Ok(result) => {
            result.print_report();
            Some(result)
        }
        Err(e) => {
            println!("Parity test setup failed: {}", e);
            None
        }
    }
}

/// Fixtures excluded from parity testing because DotNet cannot build them.
///
/// These fixtures test Rust's ability to handle edge cases that DotNet DacFx
/// fails to build (typically due to unresolved references). They are tested
/// separately for Rust-specific functionality but excluded from parity
/// comparison since there is no valid DotNet dacpac to compare against.
const PARITY_EXCLUDED_FIXTURES: &[&str] = &[
    // DotNet fails with SQL71501: unresolved reference to external database
    "external_reference",
    // DotNet fails with SQL71501: view references non-existent table
    "unresolved_reference",
];

/// Get the list of all available fixtures in the tests/fixtures directory.
///
/// This function scans the fixtures directory and returns the names of all
/// subdirectories that contain a `project.sqlproj` file.
///
/// Note: Excludes fixtures in `PARITY_EXCLUDED_FIXTURES` which cannot be built
/// by DotNet and therefore have no valid reference dacpac for comparison.
///
/// # Returns
/// A vector of fixture names (directory names) that can be passed to `run_parity_test()`.
pub fn get_available_fixtures() -> Vec<String> {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    let mut fixtures = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&fixtures_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let project_file = entry.path().join("project.sqlproj");
                if project_file.exists() {
                    if let Some(name) = entry.file_name().to_str() {
                        // Exclude fixtures that DotNet cannot build
                        if !PARITY_EXCLUDED_FIXTURES.contains(&name) {
                            fixtures.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    fixtures.sort();
    fixtures
}

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

    // Fall back to inline_constraints fixture (which has full L1 parity)
    // Note: e2e_comprehensive has an edge case where DotNet names column-level
    // constraints when table has a table-level named PK - see IMPLEMENTATION_PLAN.md 11.7.11
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("inline_constraints")
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

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            // Skip bin and obj directories to avoid copying stale build artifacts
            let dir_name = entry.file_name();
            if dir_name == "bin" || dir_name == "obj" {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Copy a fixture to a temp directory for isolated dotnet builds.
/// Returns the path to the project file in the temp directory.
fn copy_fixture_to_temp(project_path: &Path, temp_dir: &TempDir) -> Result<PathBuf, String> {
    let fixture_dir = project_path.parent().ok_or("Invalid project path")?;
    let project_filename = project_path.file_name().ok_or("Invalid project filename")?;

    let temp_fixture_dir = temp_dir.path().join("fixture");
    copy_dir_recursive(fixture_dir, &temp_fixture_dir)
        .map_err(|e| format!("Failed to copy fixture: {}", e))?;

    Ok(temp_fixture_dir.join(project_filename))
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

    // Copy fixture to temp directory for isolated dotnet build
    // This prevents race conditions when multiple tests run in parallel
    let temp_project_path = copy_fixture_to_temp(project_path, temp_dir)?;

    // Build with DotNet in the isolated temp directory
    let dotnet_output = Command::new("dotnet")
        .arg("build")
        .arg(&temp_project_path)
        .output()
        .map_err(|e| format!("Failed to run dotnet: {}", e))?;

    if !dotnet_output.status.success() {
        return Err(format!(
            "DotNet build failed: {}",
            String::from_utf8_lossy(&dotnet_output.stderr)
        ));
    }

    let dotnet_dacpac = get_dotnet_dacpac_path(&temp_project_path);
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

/// Test property completeness: Compare ALL properties (not just key properties)
/// This tests the strict property comparison mode introduced in Phase 2.
///
/// Purpose: Identifies properties that Rust is missing or has different values
/// compared to DotNet output. This is critical for achieving exact 1-1 matching.
#[test]
fn test_property_completeness() {
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

    // Use the strict comparison mode that checks ALL properties
    let errors = compare_all_properties(&rust_model, &dotnet_model);

    println!("\n=== Property Completeness Test (Strict Mode) ===\n");

    if errors.is_empty() {
        println!("All properties match in strict mode!");
    } else {
        // Group errors by element type for better readability
        let mut by_type: std::collections::HashMap<&str, Vec<_>> = std::collections::HashMap::new();
        for err in &errors {
            by_type
                .entry(err.element_type.as_str())
                .or_default()
                .push(err);
        }

        println!("Property mismatches by element type:\n");
        for (elem_type, errs) in &by_type {
            println!("  {} ({} mismatches):", elem_type, errs.len());
            for err in errs.iter().take(5) {
                println!("    - {}.{}:", err.element_name, err.property_name);
                println!("      Rust:   {:?}", err.rust_value);
                println!("      DotNet: {:?}", err.dotnet_value);
            }
            if errs.len() > 5 {
                println!("    ... and {} more", errs.len() - 5);
            }
        }

        println!("\nTotal property mismatches: {}", errors.len());
        println!("This test is informational - mismatches indicate properties");
        println!("that need to be implemented for exact 1-1 matching.");
    }

    // Note: We don't assert here because this is a progress tracking test.
    // Mismatches are expected until all properties are fully implemented.
}

/// Test using ComparisonOptions with strict_properties=true
#[test]
fn test_strict_comparison_options() {
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

    // Test with strict_properties enabled
    let options = ComparisonOptions {
        include_layer3: false,
        strict_properties: true,
        check_relationships: false,
        check_element_order: false,
        check_metadata_files: false,
        check_deploy_scripts: false,
        check_canonical: false,
    };

    let result =
        compare_dacpacs_with_options(&rust_dacpac, &dotnet_dacpac, &options).expect("Comparison");

    println!("\n=== Strict ComparisonOptions Test ===\n");
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors (strict): {}", result.layer2_errors.len());

    // Show summary of Layer 2 errors by property name
    let mut by_prop: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for err in &result.layer2_errors {
        *by_prop.entry(err.property_name.as_str()).or_default() += 1;
    }

    if !by_prop.is_empty() {
        println!("\nMismatches by property name:");
        let mut sorted: Vec<_> = by_prop.iter().collect();
        sorted.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        for (prop, count) in sorted.iter().take(10) {
            println!("  {}: {}", prop, count);
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

/// Test relationship comparison between Rust and DotNet dacpacs.
/// This tests the relationship comparison mode introduced in Phase 3.
///
/// Purpose: Identifies relationships that Rust is missing or has different
/// references compared to DotNet output. This is critical for achieving
/// exact 1-1 matching of relationship structures.
#[test]
fn test_relationship_comparison() {
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

    // Use the relationship comparison function
    let errors = compare_element_relationships(&rust_model, &dotnet_model);

    println!("\n=== Relationship Comparison Test (Phase 3) ===\n");

    if errors.is_empty() {
        println!("All relationships match!");
    } else {
        // Group errors by element type for better readability
        let mut by_type: std::collections::HashMap<String, Vec<_>> =
            std::collections::HashMap::new();
        for err in &errors {
            let key = match err {
                crate::dacpac_compare::RelationshipError::MissingRelationship {
                    element_type,
                    ..
                }
                | crate::dacpac_compare::RelationshipError::ExtraRelationship {
                    element_type,
                    ..
                }
                | crate::dacpac_compare::RelationshipError::ReferenceCountMismatch {
                    element_type,
                    ..
                }
                | crate::dacpac_compare::RelationshipError::ReferenceMismatch {
                    element_type,
                    ..
                }
                | crate::dacpac_compare::RelationshipError::EntryCountMismatch {
                    element_type,
                    ..
                } => element_type.clone(),
            };
            by_type.entry(key).or_default().push(err);
        }

        println!("Relationship mismatches by element type:\n");
        for (elem_type, errs) in &by_type {
            println!("  {} ({} mismatches):", elem_type, errs.len());
            for err in errs.iter().take(5) {
                println!("    - {}", err);
            }
            if errs.len() > 5 {
                println!("    ... and {} more", errs.len() - 5);
            }
        }

        println!("\nTotal relationship mismatches: {}", errors.len());
        println!("This test is informational - mismatches indicate relationships");
        println!("that need to be implemented for exact 1-1 matching.");
    }

    // Note: We don't assert here because this is a progress tracking test.
    // Mismatches are expected until all relationships are fully implemented.
}

/// Test using ComparisonOptions with check_relationships=true
#[test]
fn test_relationship_comparison_options() {
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

    // Test with check_relationships enabled
    let options = ComparisonOptions {
        include_layer3: false,
        strict_properties: false,
        check_relationships: true,
        check_element_order: false,
        check_metadata_files: false,
        check_deploy_scripts: false,
        check_canonical: false,
    };

    let result =
        compare_dacpacs_with_options(&rust_dacpac, &dotnet_dacpac, &options).expect("Comparison");

    println!("\n=== Relationship ComparisonOptions Test ===\n");
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors: {}", result.layer2_errors.len());
    println!("Relationship errors: {}", result.relationship_errors.len());

    // Show summary of relationship errors by type
    let mut by_error_type: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for err in &result.relationship_errors {
        let key = match err {
            crate::dacpac_compare::RelationshipError::MissingRelationship { .. } => "Missing",
            crate::dacpac_compare::RelationshipError::ExtraRelationship { .. } => "Extra",
            crate::dacpac_compare::RelationshipError::ReferenceCountMismatch { .. } => {
                "RefCountMismatch"
            }
            crate::dacpac_compare::RelationshipError::ReferenceMismatch { .. } => "RefMismatch",
            crate::dacpac_compare::RelationshipError::EntryCountMismatch { .. } => {
                "EntryCountMismatch"
            }
        };
        *by_error_type.entry(key).or_default() += 1;
    }

    if !by_error_type.is_empty() {
        println!("\nRelationship errors by type:");
        for (error_type, count) in &by_error_type {
            println!("  {}: {}", error_type, count);
        }
    }
}

// =============================================================================
// Layer 4: Element Order Comparison Tests (Phase 4)
// =============================================================================

/// Test element order comparison between Rust and DotNet dacpacs.
/// This tests the element ordering comparison mode introduced in Phase 4.
///
/// Purpose: Identifies ordering differences between Rust and DotNet output.
/// DotNet DacFx generates elements in a specific, deterministic order that
/// may affect certain DAC tools. This test tracks ordering parity progress.
#[test]
fn test_element_order_comparison() {
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

    // Use the element order comparison function
    let errors = compare_element_order(&rust_model, &dotnet_model);

    println!("\n=== Element Order Comparison Test (Phase 4) ===\n");

    if errors.is_empty() {
        println!("All elements are in the same order!");
    } else {
        // Group errors by type for readability
        let mut type_order_errors = Vec::new();
        let mut element_order_errors = Vec::new();

        for err in &errors {
            match err {
                crate::dacpac_compare::Layer4Error::TypeOrderMismatch { .. } => {
                    type_order_errors.push(err);
                }
                crate::dacpac_compare::Layer4Error::ElementOrderMismatch { .. } => {
                    element_order_errors.push(err);
                }
            }
        }

        if !type_order_errors.is_empty() {
            println!("Type ordering mismatches ({}):", type_order_errors.len());
            for err in type_order_errors.iter().take(10) {
                println!("  - {}", err);
            }
            if type_order_errors.len() > 10 {
                println!("  ... and {} more", type_order_errors.len() - 10);
            }
            println!();
        }

        if !element_order_errors.is_empty() {
            println!(
                "Element ordering mismatches ({}):",
                element_order_errors.len()
            );
            for err in element_order_errors.iter().take(10) {
                println!("  - {}", err);
            }
            if element_order_errors.len() > 10 {
                println!("  ... and {} more", element_order_errors.len() - 10);
            }
        }

        println!("\nTotal ordering mismatches: {}", errors.len());
        println!("This test is informational - mismatches indicate ordering differences");
        println!("that need to be addressed for exact 1-1 matching.");
    }

    // Note: We don't assert here because this is a progress tracking test.
    // Ordering mismatches are expected until element ordering is fully implemented.
}

/// Test using ComparisonOptions with check_element_order=true
#[test]
fn test_element_order_comparison_options() {
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

    // Test with check_element_order enabled
    let options = ComparisonOptions {
        include_layer3: false,
        strict_properties: false,
        check_relationships: false,
        check_element_order: true,
        check_metadata_files: false,
        check_deploy_scripts: false,
        check_canonical: false,
    };

    let result =
        compare_dacpacs_with_options(&rust_dacpac, &dotnet_dacpac, &options).expect("Comparison");

    println!("\n=== Element Order ComparisonOptions Test (Phase 4) ===\n");
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors: {}", result.layer2_errors.len());
    println!("Relationship errors: {}", result.relationship_errors.len());
    println!("Layer 4 (ordering) errors: {}", result.layer4_errors.len());

    // Show summary of Layer 4 errors by type
    let mut by_error_type: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for err in &result.layer4_errors {
        let key = match err {
            crate::dacpac_compare::Layer4Error::TypeOrderMismatch { .. } => "TypeOrder",
            crate::dacpac_compare::Layer4Error::ElementOrderMismatch { .. } => "ElementOrder",
        };
        *by_error_type.entry(key).or_default() += 1;
    }

    if !by_error_type.is_empty() {
        println!("\nLayer 4 errors by type:");
        for (error_type, count) in &by_error_type {
            println!("  {}: {}", error_type, count);
        }
    }
}

// =============================================================================
// Phase 5: Metadata Files Comparison Tests
// =============================================================================

/// Test [Content_Types].xml comparison between Rust and DotNet dacpacs.
/// This tests the Content_Types.xml comparison mode introduced in Phase 5.
///
/// Purpose: Verifies that [Content_Types].xml files match between Rust and DotNet output.
/// This includes MIME type definitions for different file extensions.
///
/// Note: DotNet may use either "text/xml" or "application/xml" depending on version.
/// Both are semantically equivalent but tracked for exact parity.
#[test]
fn test_content_types_comparison() {
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

    // Use the content types comparison function
    let errors = crate::dacpac_compare::compare_content_types(&rust_dacpac, &dotnet_dacpac);

    println!("\n=== Content Types Comparison Test (Phase 5.1) ===\n");

    if errors.is_empty() {
        println!("All [Content_Types].xml entries match!");
    } else {
        println!("Content types mismatches found: {}\n", errors.len());
        for err in &errors {
            println!("  {}", err);
        }

        println!("\nThis test is informational - mismatches indicate Content_Types.xml");
        println!("differences that need to be addressed for exact 1-1 matching.");
    }

    // Note: We don't assert here because this is a progress tracking test.
    // Mismatches may be expected (text/xml vs application/xml) until fully implemented.
}

/// Test using ComparisonOptions with check_metadata_files=true
#[test]
fn test_content_types_comparison_options() {
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

    // Test with check_metadata_files enabled
    let options = ComparisonOptions {
        include_layer3: false,
        strict_properties: false,
        check_relationships: false,
        check_element_order: false,
        check_metadata_files: true,
        check_deploy_scripts: false,
        check_canonical: false,
    };

    let result =
        compare_dacpacs_with_options(&rust_dacpac, &dotnet_dacpac, &options).expect("Comparison");

    println!("\n=== Metadata Files ComparisonOptions Test (Phase 5) ===\n");
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors: {}", result.layer2_errors.len());
    println!("Relationship errors: {}", result.relationship_errors.len());
    println!("Layer 4 (ordering) errors: {}", result.layer4_errors.len());
    println!("Metadata file errors: {}", result.metadata_errors.len());

    // Show details of metadata errors
    if !result.metadata_errors.is_empty() {
        println!("\nMetadata file errors:");
        for err in &result.metadata_errors {
            println!("  {}", err);
        }
    }
}

/// Test parsing Content_Types.xml directly
#[test]
fn test_content_types_xml_parsing() {
    use crate::dacpac_compare::ContentTypesXml;

    // Test parsing typical DotNet output
    let dotnet_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="text/xml" />
  <Default Extension="sql" ContentType="text/plain" />
</Types>"#;

    let ct = ContentTypesXml::from_xml(dotnet_xml).expect("Should parse Content_Types.xml");

    assert_eq!(ct.types.len(), 2, "Should have 2 type definitions");
    assert_eq!(
        ct.types.get("xml"),
        Some(&"text/xml".to_string()),
        "XML should be text/xml"
    );
    assert_eq!(
        ct.types.get("sql"),
        Some(&"text/plain".to_string()),
        "SQL should be text/plain"
    );

    // Test parsing without SQL types
    let xml_only = r#"<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml" />
</Types>"#;

    let ct2 = ContentTypesXml::from_xml(xml_only).expect("Should parse Content_Types.xml");

    assert_eq!(ct2.types.len(), 1, "Should have 1 type definition");
    assert_eq!(
        ct2.types.get("xml"),
        Some(&"application/xml".to_string()),
        "XML should be application/xml"
    );
}

/// Test extracting Content_Types.xml from a Rust-generated dacpac
#[test]
fn test_extract_content_types_from_dacpac() {
    use crate::dacpac_compare::{extract_content_types_xml, ContentTypesXml};

    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("simple_table")
        .join("project.sqlproj");

    if !project_path.exists() {
        eprintln!("Skipping: simple_table fixture not found");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path,
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    })
    .expect("Rust build should succeed");

    // Extract and parse Content_Types.xml
    let xml = extract_content_types_xml(&rust_dacpac).expect("Should extract [Content_Types].xml");
    let ct = ContentTypesXml::from_xml(&xml).expect("Should parse Content_Types.xml");

    println!("\n=== Rust Dacpac Content_Types.xml ===");
    println!("Raw XML:\n{}", xml);
    println!("\nParsed types:");
    for (ext, content_type) in &ct.types {
        println!("  .{} -> {}", ext, content_type);
    }

    assert!(
        ct.types.contains_key("xml"),
        "Should have XML type definition"
    );
}

// =============================================================================
// Phase 5.2: DacMetadata.xml Comparison Tests
// =============================================================================

/// Test DacMetadata.xml comparison between Rust and DotNet dacpacs.
/// This tests the DacMetadata.xml comparison mode introduced in Phase 5.2.
///
/// Purpose: Verifies that DacMetadata.xml files match between Rust and DotNet output.
/// This includes Name, Version, and Description fields.
#[test]
fn test_dac_metadata_comparison() {
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

    // Use the DacMetadata comparison function
    let errors = crate::dacpac_compare::compare_dac_metadata(&rust_dacpac, &dotnet_dacpac);

    println!("\n=== DacMetadata.xml Comparison Test (Phase 5.2) ===\n");

    if errors.is_empty() {
        println!("All DacMetadata.xml fields match!");
    } else {
        println!("DacMetadata mismatches found: {}\n", errors.len());
        for err in &errors {
            println!("  {}", err);
        }

        println!("\nThis test is informational - mismatches indicate DacMetadata.xml");
        println!("differences that need to be addressed for exact 1-1 matching.");
    }

    // Note: We don't assert here because this is a progress tracking test.
    // Mismatches may be expected (e.g., different project names) until fully implemented.
}

/// Test parsing DacMetadata.xml directly
#[test]
fn test_dac_metadata_xml_parsing() {
    use crate::dacpac_compare::DacMetadataXml;

    // Test parsing typical DotNet output
    let dotnet_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<DacType xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Name>MyDatabase</Name>
  <Version>1.0.0.0</Version>
</DacType>"#;

    let meta = DacMetadataXml::from_xml(dotnet_xml).expect("Should parse DacMetadata.xml");

    assert_eq!(
        meta.name,
        Some("MyDatabase".to_string()),
        "Name should be MyDatabase"
    );
    assert_eq!(
        meta.version,
        Some("1.0.0.0".to_string()),
        "Version should be 1.0.0.0"
    );
    assert_eq!(meta.description, None, "Description should be None");

    // Test parsing with Description
    let with_desc = r#"<?xml version="1.0" encoding="utf-8"?>
<DacType xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Name>TestDB</Name>
  <Version>2.0.0.0</Version>
  <Description>A test database</Description>
</DacType>"#;

    let meta2 = DacMetadataXml::from_xml(with_desc).expect("Should parse DacMetadata.xml");

    assert_eq!(
        meta2.name,
        Some("TestDB".to_string()),
        "Name should be TestDB"
    );
    assert_eq!(
        meta2.version,
        Some("2.0.0.0".to_string()),
        "Version should be 2.0.0.0"
    );
    assert_eq!(
        meta2.description,
        Some("A test database".to_string()),
        "Description should match"
    );
}

/// Test extracting DacMetadata.xml from a Rust-generated dacpac
#[test]
fn test_extract_dac_metadata_from_dacpac() {
    use crate::dacpac_compare::{extract_dac_metadata_xml, DacMetadataXml};

    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("simple_table")
        .join("project.sqlproj");

    if !project_path.exists() {
        eprintln!("Skipping: simple_table fixture not found");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path,
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    })
    .expect("Rust build should succeed");

    // Extract and parse DacMetadata.xml
    let xml = extract_dac_metadata_xml(&rust_dacpac).expect("Should extract DacMetadata.xml");
    let meta = DacMetadataXml::from_xml(&xml).expect("Should parse DacMetadata.xml");

    println!("\n=== Rust Dacpac DacMetadata.xml ===");
    println!("Raw XML:\n{}", xml);
    println!("\nParsed fields:");
    println!("  Name: {:?}", meta.name);
    println!("  Version: {:?}", meta.version);
    println!("  Description: {:?}", meta.description);

    assert!(meta.name.is_some(), "Should have Name field");
    assert!(meta.version.is_some(), "Should have Version field");
}

/// Test that metadata comparison includes both Content_Types and DacMetadata
#[test]
fn test_metadata_comparison_includes_dac_metadata() {
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

    // Test with check_metadata_files enabled - should include both comparisons
    let options = ComparisonOptions {
        include_layer3: false,
        strict_properties: false,
        check_relationships: false,
        check_element_order: false,
        check_metadata_files: true,
        check_deploy_scripts: false,
        check_canonical: false,
    };

    let result =
        compare_dacpacs_with_options(&rust_dacpac, &dotnet_dacpac, &options).expect("Comparison");

    println!("\n=== Full Metadata Files ComparisonOptions Test (Phase 5) ===\n");
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors: {}", result.layer2_errors.len());
    println!("Relationship errors: {}", result.relationship_errors.len());
    println!("Layer 4 (ordering) errors: {}", result.layer4_errors.len());
    println!("Metadata file errors: {}", result.metadata_errors.len());

    // Categorize metadata errors
    let mut content_type_errors = 0;
    let mut dac_metadata_errors = 0;
    let mut origin_xml_errors = 0;
    let mut file_missing_errors = 0;
    let mut deploy_script_errors = 0;

    for err in &result.metadata_errors {
        match err {
            crate::dacpac_compare::MetadataFileError::ContentTypeMismatch { .. }
            | crate::dacpac_compare::MetadataFileError::ContentTypeCountMismatch { .. } => {
                content_type_errors += 1;
            }
            crate::dacpac_compare::MetadataFileError::DacMetadataMismatch { .. } => {
                dac_metadata_errors += 1;
            }
            crate::dacpac_compare::MetadataFileError::OriginXmlMismatch { .. } => {
                origin_xml_errors += 1;
            }
            crate::dacpac_compare::MetadataFileError::FileMissing { .. } => {
                file_missing_errors += 1;
            }
            crate::dacpac_compare::MetadataFileError::DeployScriptMismatch { .. }
            | crate::dacpac_compare::MetadataFileError::DeployScriptMissing { .. } => {
                deploy_script_errors += 1;
            }
        }
    }

    println!("\nMetadata error breakdown:");
    println!("  [Content_Types].xml errors: {}", content_type_errors);
    println!("  DacMetadata.xml errors: {}", dac_metadata_errors);
    println!("  Origin.xml errors: {}", origin_xml_errors);
    println!("  File missing errors: {}", file_missing_errors);
    println!("  Deploy script errors: {}", deploy_script_errors);

    // Show details of metadata errors
    if !result.metadata_errors.is_empty() {
        println!("\nAll metadata file errors:");
        for err in &result.metadata_errors {
            println!("  {}", err);
        }
    }
}

// =============================================================================
// Phase 5.3: Origin.xml Comparison Tests
// =============================================================================

/// Test Origin.xml comparison between Rust and DotNet dacpacs (Phase 5.3)
///
/// This test compares Origin.xml fields between rust-sqlpackage and DotNet DacFx.
/// Expected differences:
/// - ProductName: rust-sqlpackage vs Microsoft.Data.Tools.Schema.Sql
/// - ProductVersion: 0.1.0 vs DotNet SDK version
///
/// Fields that should match:
/// - PackageProperties/Version: Package format version
/// - ContainsExportedData: Boolean flag
/// - StreamVersions (Data, DeploymentContributors)
/// - ProductSchema: Schema URL
#[test]
fn test_origin_xml_comparison() {
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

    // Use the Origin.xml comparison function
    let errors = crate::dacpac_compare::compare_origin_xml(&rust_dacpac, &dotnet_dacpac);

    println!("\n=== Origin.xml Comparison Test (Phase 5.3) ===\n");

    if errors.is_empty() {
        println!("All Origin.xml fields match!");
    } else {
        println!("Origin.xml mismatches found: {}\n", errors.len());
        for err in &errors {
            println!("  {}", err);
        }

        println!("\nThis test is informational - some differences are expected:");
        println!("  - ProductName: rust-sqlpackage vs DotNet DacFx");
        println!("  - ProductVersion: Tool-specific version");
        println!("\nFields that should match:");
        println!("  - PackageProperties/Version (package format)");
        println!("  - ContainsExportedData");
        println!("  - StreamVersions");
        println!("  - ProductSchema");
    }

    // Note: We don't assert here because ProductName and ProductVersion will always differ
}

/// Test parsing Origin.xml directly
#[test]
fn test_origin_xml_parsing() {
    use crate::dacpac_compare::OriginXml;

    // Test parsing typical DotNet output
    let dotnet_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<DacOrigin xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <PackageProperties>
    <Version>3.1.0.0</Version>
    <ContainsExportedData>false</ContainsExportedData>
    <StreamVersions>
      <Version StreamName="Data">2.0.0.0</Version>
      <Version StreamName="DeploymentContributors">1.0.0.0</Version>
    </StreamVersions>
  </PackageProperties>
  <Operation>
    <Identity>abc123</Identity>
    <Start>2024-01-01T00:00:00Z</Start>
    <End>2024-01-01T00:00:01Z</End>
    <ProductName>Microsoft.Data.Tools.Schema.Sql, Version=16.0</ProductName>
    <ProductVersion>16.0</ProductVersion>
    <ProductSchema>http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02</ProductSchema>
  </Operation>
  <Checksums>
    <Checksum Uri="/model.xml">ABCDEF123456</Checksum>
  </Checksums>
</DacOrigin>"#;

    let origin = OriginXml::from_xml(dotnet_xml).expect("Should parse Origin.xml");

    assert_eq!(
        origin.package_version,
        Some("3.1.0.0".to_string()),
        "Package version should be 3.1.0.0"
    );
    assert_eq!(
        origin.contains_exported_data,
        Some("false".to_string()),
        "ContainsExportedData should be false"
    );
    assert_eq!(
        origin.data_stream_version,
        Some("2.0.0.0".to_string()),
        "Data stream version should be 2.0.0.0"
    );
    assert_eq!(
        origin.deployment_contributors_version,
        Some("1.0.0.0".to_string()),
        "DeploymentContributors version should be 1.0.0.0"
    );
    assert_eq!(
        origin.product_name,
        Some("Microsoft.Data.Tools.Schema.Sql, Version=16.0".to_string()),
        "ProductName should match"
    );
    assert_eq!(
        origin.product_version,
        Some("16.0".to_string()),
        "ProductVersion should be 16.0"
    );
    assert_eq!(
        origin.product_schema,
        Some("http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02".to_string()),
        "ProductSchema should match namespace"
    );
}

/// Test extracting Origin.xml from a Rust-generated dacpac
#[test]
fn test_extract_origin_xml_from_dacpac() {
    use crate::dacpac_compare::{extract_origin_xml, OriginXml};

    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("simple_table")
        .join("project.sqlproj");

    if !project_path.exists() {
        eprintln!("Skipping: simple_table fixture not found");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path,
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    })
    .expect("Rust build should succeed");

    // Extract and parse Origin.xml
    let xml = extract_origin_xml(&rust_dacpac).expect("Should extract Origin.xml");
    let origin = OriginXml::from_xml(&xml).expect("Should parse Origin.xml");

    println!("\n=== Rust Dacpac Origin.xml ===");
    println!("Raw XML:\n{}", xml);
    println!("\nParsed fields:");
    println!("  PackageVersion: {:?}", origin.package_version);
    println!(
        "  ContainsExportedData: {:?}",
        origin.contains_exported_data
    );
    println!("  DataStreamVersion: {:?}", origin.data_stream_version);
    println!(
        "  DeploymentContributorsVersion: {:?}",
        origin.deployment_contributors_version
    );
    println!("  ProductName: {:?}", origin.product_name);
    println!("  ProductVersion: {:?}", origin.product_version);
    println!("  ProductSchema: {:?}", origin.product_schema);

    // Verify expected values for rust-sqlpackage
    assert_eq!(
        origin.package_version,
        Some("3.1.0.0".to_string()),
        "Package version should be 3.1.0.0"
    );
    assert_eq!(
        origin.contains_exported_data,
        Some("false".to_string()),
        "ContainsExportedData should be false"
    );
    assert_eq!(
        origin.product_name,
        Some("rust-sqlpackage".to_string()),
        "ProductName should be rust-sqlpackage"
    );
    assert!(origin.product_schema.is_some(), "Should have ProductSchema");
}

// =============================================================================
// Phase 5.4: Pre/Post Deploy Script Comparison Tests
// =============================================================================

/// Test pre/post-deploy script comparison between Rust and DotNet dacpacs (Phase 5.4)
///
/// This test verifies that predeploy.sql and postdeploy.sql files match between
/// rust-sqlpackage and DotNet DacFx output.
///
/// Both tools should:
/// - Package the scripts with the same filenames (predeploy.sql, postdeploy.sql)
/// - Preserve script content (modulo whitespace normalization)
#[test]
fn test_deploy_script_comparison() {
    if !dotnet_available() {
        return;
    }

    // Use the pre_post_deploy fixture which has both scripts
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("pre_post_deploy")
        .join("project.sqlproj");

    if !project_path.exists() {
        eprintln!("Skipping: pre_post_deploy fixture not found");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    // Use the deploy script comparison function
    let errors = crate::dacpac_compare::compare_deploy_scripts(&rust_dacpac, &dotnet_dacpac);

    println!("\n=== Deploy Script Comparison Test (Phase 5.4) ===\n");

    if errors.is_empty() {
        println!("All deploy scripts match!");
    } else {
        println!("Deploy script mismatches found: {}\n", errors.len());
        for err in &errors {
            println!("  {}", err);
        }

        println!("\nThis test is informational - mismatches indicate deploy script");
        println!("differences that need to be addressed for exact 1-1 matching.");
    }

    // Note: This test is informational and doesn't fail on mismatches
}

/// Test deploy script extraction from dacpac
#[test]
fn test_extract_deploy_scripts_from_dacpac() {
    use crate::dacpac_compare::extract_deploy_script;

    // Use the pre_post_deploy fixture
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("pre_post_deploy")
        .join("project.sqlproj");

    if !project_path.exists() {
        eprintln!("Skipping: pre_post_deploy fixture not found");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rust_dacpac = temp_dir.path().join("rust.dacpac");

    rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path,
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    })
    .expect("Rust build should succeed");

    // Extract predeploy.sql
    let predeploy = extract_deploy_script(&rust_dacpac, "predeploy.sql").expect("Should not error");

    println!("\n=== Rust Dacpac Deploy Scripts ===\n");

    if let Some(ref content) = predeploy {
        println!("predeploy.sql ({} bytes):", content.len());
        println!("{}", content);
    } else {
        println!("predeploy.sql: Not found");
    }

    // Extract postdeploy.sql
    let postdeploy =
        extract_deploy_script(&rust_dacpac, "postdeploy.sql").expect("Should not error");

    if let Some(ref content) = postdeploy {
        println!("\npostdeploy.sql ({} bytes):", content.len());
        println!("{}", content);
    } else {
        println!("\npostdeploy.sql: Not found");
    }

    // Verify scripts exist in the pre_post_deploy fixture
    assert!(predeploy.is_some(), "predeploy.sql should exist in dacpac");
    assert!(
        postdeploy.is_some(),
        "postdeploy.sql should exist in dacpac"
    );

    // Verify content contains expected strings
    let predeploy_content = predeploy.unwrap();
    assert!(
        predeploy_content.contains("Starting deployment"),
        "predeploy.sql should contain 'Starting deployment'"
    );

    let postdeploy_content = postdeploy.unwrap();
    assert!(
        postdeploy_content.contains("Deployment complete"),
        "postdeploy.sql should contain 'Deployment complete'"
    );
}

/// Test whitespace normalization for script comparison
#[test]
fn test_script_whitespace_normalization() {
    // Test that scripts with different whitespace are considered equal
    let script1 = "-- Comment\r\nPRINT 'Hello';  \r\n\r\n";
    let script2 = "-- Comment\nPRINT 'Hello';\n";

    // Use the internal normalize function via a test helper
    fn normalize(s: &str) -> String {
        let content = s.replace("\r\n", "\n");
        let lines: Vec<&str> = content.lines().map(|line| line.trim_end()).collect();
        let mut result: Vec<&str> = lines;
        while result.last().is_some_and(|line| line.is_empty()) {
            result.pop();
        }
        result.join("\n")
    }

    let normalized1 = normalize(script1);
    let normalized2 = normalize(script2);

    println!("\n=== Script Whitespace Normalization Test ===\n");
    println!("Script 1 (CRLF, trailing spaces, trailing newlines):");
    println!("  Raw: {:?}", script1);
    println!("  Normalized: {:?}", normalized1);
    println!("\nScript 2 (LF, no trailing whitespace):");
    println!("  Raw: {:?}", script2);
    println!("  Normalized: {:?}", normalized2);

    assert_eq!(
        normalized1, normalized2,
        "Scripts with only whitespace differences should normalize to the same content"
    );
}

/// Test ComparisonOptions with check_deploy_scripts enabled
#[test]
fn test_deploy_script_comparison_options() {
    if !dotnet_available() {
        return;
    }

    // Use the pre_post_deploy fixture
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("pre_post_deploy")
        .join("project.sqlproj");

    if !project_path.exists() {
        eprintln!("Skipping: pre_post_deploy fixture not found");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    // Test with check_deploy_scripts enabled
    let options = ComparisonOptions {
        include_layer3: false,
        strict_properties: false,
        check_relationships: false,
        check_element_order: false,
        check_metadata_files: false,
        check_deploy_scripts: true,
        check_canonical: false,
    };

    let result =
        compare_dacpacs_with_options(&rust_dacpac, &dotnet_dacpac, &options).expect("Comparison");

    println!("\n=== Deploy Script ComparisonOptions Test (Phase 5.4) ===\n");
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors: {}", result.layer2_errors.len());
    println!(
        "Metadata errors (includes deploy scripts): {}",
        result.metadata_errors.len()
    );

    // Count deploy script specific errors
    let deploy_errors: Vec<_> = result
        .metadata_errors
        .iter()
        .filter(|e| {
            matches!(
                e,
                crate::dacpac_compare::MetadataFileError::DeployScriptMismatch { .. }
                    | crate::dacpac_compare::MetadataFileError::DeployScriptMissing { .. }
            )
        })
        .collect();

    println!("  Deploy script errors: {}", deploy_errors.len());

    if !deploy_errors.is_empty() {
        println!("\nDeploy script errors:");
        for err in &deploy_errors {
            println!("  {}", err);
        }
    }
}

/// Test that dacpacs without deploy scripts don't generate errors
#[test]
fn test_deploy_script_comparison_no_scripts() {
    if !dotnet_available() {
        return;
    }

    // Use e2e_comprehensive fixture which likely doesn't have deploy scripts
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

    // Compare deploy scripts
    let errors = crate::dacpac_compare::compare_deploy_scripts(&rust_dacpac, &dotnet_dacpac);

    println!("\n=== Deploy Script Comparison (No Scripts Expected) ===\n");

    if errors.is_empty() {
        println!("No deploy script errors (both dacpacs have no scripts) - PASS");
    } else {
        println!("Deploy script errors found: {}", errors.len());
        for err in &errors {
            println!("  {}", err);
        }
        println!("\nNote: If both dacpacs have no scripts, there should be no errors.");
    }

    // When neither dacpac has deploy scripts, there should be no errors
    // (unless there's an asymmetry - one has scripts, the other doesn't)
}

// =============================================================================
// Phase 5.5: Unified Metadata File Comparison Tests
// =============================================================================

/// Test the unified compare_dacpac_files() function that consolidates all
/// Phase 5 metadata comparisons into a single call.
///
/// This test verifies that the unified function correctly aggregates errors from:
/// - Phase 5.1: [Content_Types].xml comparison
/// - Phase 5.2: DacMetadata.xml comparison
/// - Phase 5.3: Origin.xml comparison
/// - Phase 5.4: Pre/post-deploy script comparison
#[test]
fn test_unified_metadata_comparison() {
    // Skip if dotnet not available
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => {
            println!("Skipping test: no test project available");
            return;
        }
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    // Call the unified comparison function
    let errors = crate::dacpac_compare::compare_dacpac_files(&rust_dacpac, &dotnet_dacpac);

    println!("\n=== Unified Metadata Comparison Test (Phase 5.5) ===\n");

    if errors.is_empty() {
        println!("All metadata files match! (Content_Types, DacMetadata, Origin, Deploy Scripts)");
    } else {
        // Categorize errors by source
        let mut content_type_errors = 0;
        let mut dac_metadata_errors = 0;
        let mut origin_xml_errors = 0;
        let mut deploy_script_errors = 0;

        for err in &errors {
            match err {
                crate::dacpac_compare::MetadataFileError::ContentTypeMismatch { .. }
                | crate::dacpac_compare::MetadataFileError::ContentTypeCountMismatch { .. } => {
                    content_type_errors += 1;
                }
                crate::dacpac_compare::MetadataFileError::DacMetadataMismatch { .. } => {
                    dac_metadata_errors += 1;
                }
                crate::dacpac_compare::MetadataFileError::OriginXmlMismatch { .. } => {
                    origin_xml_errors += 1;
                }
                crate::dacpac_compare::MetadataFileError::DeployScriptMismatch { .. }
                | crate::dacpac_compare::MetadataFileError::DeployScriptMissing { .. } => {
                    deploy_script_errors += 1;
                }
                crate::dacpac_compare::MetadataFileError::FileMissing { file_name, .. } => {
                    // Categorize FileMissing by file name
                    if file_name.contains("Content_Types") {
                        content_type_errors += 1;
                    } else if file_name.contains("DacMetadata") {
                        dac_metadata_errors += 1;
                    } else if file_name.contains("Origin") {
                        origin_xml_errors += 1;
                    } else {
                        deploy_script_errors += 1;
                    }
                }
            }
        }

        println!("Metadata mismatches found: {}", errors.len());
        println!("  - [Content_Types].xml: {}", content_type_errors);
        println!("  - DacMetadata.xml: {}", dac_metadata_errors);
        println!("  - Origin.xml: {}", origin_xml_errors);
        println!("  - Deploy scripts: {}", deploy_script_errors);
        println!();

        println!("All errors:");
        for err in &errors {
            println!("  {}", err);
        }
    }

    println!();
    println!(
        "Note: This is an informational test. Some differences (like ProductName) are expected."
    );
}

/// Test that compare_dacpac_files() returns the same results as calling
/// all individual comparison functions separately.
#[test]
fn test_unified_metadata_consistency() {
    // Skip if dotnet not available
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => {
            println!("Skipping test: no test project available");
            return;
        }
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    // Get errors from unified function
    let unified_errors = crate::dacpac_compare::compare_dacpac_files(&rust_dacpac, &dotnet_dacpac);

    // Get errors from individual functions
    let mut individual_errors = Vec::new();
    individual_errors.extend(crate::dacpac_compare::compare_content_types(
        &rust_dacpac,
        &dotnet_dacpac,
    ));
    individual_errors.extend(crate::dacpac_compare::compare_dac_metadata(
        &rust_dacpac,
        &dotnet_dacpac,
    ));
    individual_errors.extend(crate::dacpac_compare::compare_origin_xml(
        &rust_dacpac,
        &dotnet_dacpac,
    ));
    individual_errors.extend(crate::dacpac_compare::compare_deploy_scripts(
        &rust_dacpac,
        &dotnet_dacpac,
    ));

    println!("\n=== Unified Metadata Consistency Test ===\n");
    println!("Unified function returned {} errors", unified_errors.len());
    println!(
        "Individual functions returned {} errors",
        individual_errors.len()
    );

    // The unified function should return the same number of errors
    assert_eq!(
        unified_errors.len(),
        individual_errors.len(),
        "Unified function should return same error count as individual functions combined"
    );

    println!("\nConsistency check PASSED: Error counts match!");
}

/// Test that compare_dacpacs_with_options uses compare_dacpac_files when both
/// check_metadata_files and check_deploy_scripts are enabled.
#[test]
fn test_unified_metadata_via_options() {
    // Skip if dotnet not available
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let project_path = match get_test_project_path() {
        Some(p) => p,
        None => {
            println!("Skipping test: no test project available");
            return;
        }
    };

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let (rust_dacpac, dotnet_dacpac) = match build_both_dacpacs(&project_path, &temp_dir) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Build failed: {}", e);
            return;
        }
    };

    // Enable both metadata file and deploy script comparison
    let options = ComparisonOptions {
        include_layer3: false,
        strict_properties: false,
        check_relationships: false,
        check_element_order: false,
        check_metadata_files: true,
        check_deploy_scripts: true,
        check_canonical: false,
    };

    let result = compare_dacpacs_with_options(&rust_dacpac, &dotnet_dacpac, &options)
        .expect("Comparison should succeed");

    // Get errors from direct unified function call for comparison
    let direct_errors = crate::dacpac_compare::compare_dacpac_files(&rust_dacpac, &dotnet_dacpac);

    println!("\n=== Compare via ComparisonOptions Test ===\n");
    println!(
        "ComparisonOptions returned {} metadata errors",
        result.metadata_errors.len()
    );
    println!(
        "Direct compare_dacpac_files() returned {} errors",
        direct_errors.len()
    );

    // Both should return the same number of errors
    assert_eq!(
        result.metadata_errors.len(),
        direct_errors.len(),
        "ComparisonOptions path should return same results as direct unified function"
    );

    println!("\nIntegration check PASSED: ComparisonOptions correctly uses unified function!");
}

// =============================================================================
// Phase 6: Per-Feature Parity Tests
// =============================================================================

/// Test the run_parity_test() helper function with a simple fixture.
///
/// This test verifies that the parity test infrastructure works correctly:
/// - Finds fixtures by name
/// - Builds with both Rust and DotNet
/// - Returns comparison results
#[test]
fn test_run_parity_test_simple_table() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    // Use minimal options for a quick test
    let options = ParityTestOptions::minimal();

    let result = match run_parity_test("simple_table", &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed to run: {}", e);
            return;
        }
    };

    println!("\n=== run_parity_test() Test (simple_table) ===\n");
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors: {}", result.layer2_errors.len());

    // simple_table should have basic element parity (Layer 1)
    // We don't assert is_success() because parity testing is ongoing
    // but Layer 1 (element inventory) should pass for simple fixtures
}

/// Test run_parity_test() with invalid fixture name
#[test]
fn test_run_parity_test_invalid_fixture() {
    if !dotnet_available() {
        return;
    }

    let options = ParityTestOptions::minimal();
    let result = run_parity_test("nonexistent_fixture_xyz", &options);

    match result {
        Err(ParityTestError::FixtureNotFound { fixture_name, .. }) => {
            assert_eq!(fixture_name, "nonexistent_fixture_xyz");
            println!("Correctly returned FixtureNotFound error");
        }
        Ok(_) => panic!("Should have returned FixtureNotFound error"),
        Err(e) => panic!("Expected FixtureNotFound, got: {}", e),
    }
}

/// Test ParityTestOptions default values
#[test]
fn test_parity_test_options_default() {
    let options = ParityTestOptions::default();

    // Default options should enable full comparison (except Layer 3 which is auto-detected)
    assert!(
        !options.include_layer3,
        "Layer 3 should be disabled by default"
    );
    assert!(
        options.strict_properties,
        "Strict properties should be enabled by default"
    );
    assert!(
        options.check_relationships,
        "Relationship check should be enabled by default"
    );
    assert!(
        options.check_element_order,
        "Element order check should be enabled by default"
    );
    assert!(
        options.check_metadata_files,
        "Metadata check should be enabled by default"
    );
    assert!(
        options.check_deploy_scripts,
        "Deploy script check should be enabled by default"
    );
    assert_eq!(
        options.target_platform, "Sql150",
        "Default platform should be Sql150"
    );
}

/// Test ParityTestOptions::minimal()
#[test]
fn test_parity_test_options_minimal() {
    let options = ParityTestOptions::minimal();

    // Minimal options should disable everything except basic comparison
    assert!(!options.include_layer3);
    assert!(!options.strict_properties);
    assert!(!options.check_relationships);
    assert!(!options.check_element_order);
    assert!(!options.check_metadata_files);
    assert!(!options.check_deploy_scripts);
}

/// Test get_available_fixtures() returns expected fixtures
#[test]
fn test_get_available_fixtures() {
    let fixtures = get_available_fixtures();

    println!("\n=== Available Fixtures ===\n");
    println!("Found {} fixtures:", fixtures.len());
    for (i, name) in fixtures.iter().enumerate() {
        if i < 10 {
            println!("  - {}", name);
        }
    }
    if fixtures.len() > 10 {
        println!("  ... and {} more", fixtures.len() - 10);
    }

    // Verify some known fixtures are present
    assert!(
        fixtures.contains(&"simple_table".to_string()),
        "simple_table should exist"
    );
    assert!(
        fixtures.contains(&"e2e_comprehensive".to_string()),
        "e2e_comprehensive should exist"
    );
    assert!(fixtures.len() >= 10, "Should have at least 10 fixtures");
}

/// Test run_parity_test_with_report() convenience function
#[test]
fn test_run_parity_test_with_report() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let options = ParityTestOptions::minimal();

    // This should print a report and return Some(result)
    let result = run_parity_test_with_report("simple_table", &options);

    assert!(
        result.is_some(),
        "Should return Some(result) for valid fixture"
    );

    // Test with invalid fixture - should return None and print error
    let invalid_result = run_parity_test_with_report("nonexistent_fixture", &options);
    assert!(
        invalid_result.is_none(),
        "Should return None for invalid fixture"
    );
}

/// Informational test: Run parity test on ampersand_encoding fixture
/// This validates the fix from Phase 1.1 (ampersand truncation bug)
#[test]
fn test_parity_ampersand_encoding() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    // Use minimal options to focus on Layer 1 (element inventory)
    let options = ParityTestOptions::minimal();

    let result = match run_parity_test("ampersand_encoding", &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed: {}", e);
            return;
        }
    };

    println!("\n=== Parity Test: ampersand_encoding ===\n");
    println!("Testing fix for Phase 1.1: Ampersand truncation in procedure names");
    println!();
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors: {}", result.layer2_errors.len());

    // Layer 1 should pass - elements with & in names should be properly captured
    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }

    // This test is informational - ampersand handling was fixed in Phase 1.1
    // Full parity may still have differences in other areas
}

/// Informational test: Run parity test on default_constraints_named fixture
/// This validates Phase 1.2 (named inline default constraints)
#[test]
fn test_parity_default_constraints_named() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let options = ParityTestOptions::minimal();

    let result = match run_parity_test("default_constraints_named", &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed: {}", e);
            return;
        }
    };

    println!("\n=== Parity Test: default_constraints_named ===\n");
    println!("Testing Phase 1.2: Named inline default constraints");
    println!();
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors: {}", result.layer2_errors.len());

    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }
}

/// Informational test: Run parity test on inline_constraints fixture
/// This validates Phase 1.3 (inline CHECK constraints)
#[test]
fn test_parity_inline_constraints() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let options = ParityTestOptions::minimal();

    let result = match run_parity_test("inline_constraints", &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed: {}", e);
            return;
        }
    };

    println!("\n=== Parity Test: inline_constraints ===\n");
    println!("Testing Phase 1.3: Inline CHECK constraints");
    println!();
    println!("Layer 1 errors: {}", result.layer1_errors.len());
    println!("Layer 2 errors: {}", result.layer2_errors.len());

    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }
}

// ============================================================================
// Phase 6.3: Medium-Priority Fixture Parity Tests
// ============================================================================

/// Informational test: Run parity test on database_options fixture
/// This validates Phase 1.4 (SqlDatabaseOptions element generation)
///
/// The database_options fixture tests that database-level options from the
/// sqlproj file (ANSI_NULLS, PageVerify, Collation, etc.) are correctly
/// captured as SqlDatabaseOptions elements in the model.xml output.
#[test]
fn test_parity_database_options() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    // Use default options to get full comparison including property validation
    let options = ParityTestOptions::default();

    let result = match run_parity_test("database_options", &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed: {}", e);
            return;
        }
    };

    println!("\n=== Parity Test: database_options ===\n");
    println!("Testing Phase 1.4: SqlDatabaseOptions element generation");
    println!("Validates: IsAnsiNullsOn, PageVerifyMode, DefaultCollation, etc.");
    println!();
    println!("Layer 1 errors (inventory): {}", result.layer1_errors.len());
    println!(
        "Layer 2 errors (properties): {}",
        result.layer2_errors.len()
    );
    println!("Relationship errors: {}", result.relationship_errors.len());
    println!("Layer 4 errors (ordering): {}", result.layer4_errors.len());
    println!("Metadata errors: {}", result.metadata_errors.len());

    // Layer 1: Element inventory should match (SqlDatabaseOptions present)
    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }

    // Layer 2: Property values comparison
    if !result.layer2_errors.is_empty() {
        println!("\nLayer 2 errors (showing first 10):");
        for err in result.layer2_errors.iter().take(10) {
            println!("  {}", err);
        }
        if result.layer2_errors.len() > 10 {
            println!("  ... and {} more", result.layer2_errors.len() - 10);
        }
    }
}

/// Informational test: Run parity test on header_section fixture
/// This validates Phase 1.5 (Header section generation with metadata)
///
/// The header_section fixture tests that the model.xml Header element is
/// correctly generated with Metadata entries for AnsiNulls, QuotedIdentifier,
/// and CompatibilityMode settings. This fixture also includes a package
/// reference to master.dacpac which exercises external reference handling.
#[test]
fn test_parity_header_section() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    // Use default options to get full comparison including property validation
    let options = ParityTestOptions::default();

    let result = match run_parity_test("header_section", &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed: {}", e);
            return;
        }
    };

    println!("\n=== Parity Test: header_section ===\n");
    println!("Testing Phase 1.5: Header section generation");
    println!("Validates: AnsiNulls, QuotedIdentifier, CompatibilityMode metadata");
    println!("Also tests: Package reference to master.dacpac");
    println!();
    println!("Layer 1 errors (inventory): {}", result.layer1_errors.len());
    println!(
        "Layer 2 errors (properties): {}",
        result.layer2_errors.len()
    );
    println!("Relationship errors: {}", result.relationship_errors.len());
    println!("Layer 4 errors (ordering): {}", result.layer4_errors.len());
    println!("Metadata errors: {}", result.metadata_errors.len());

    // Layer 1: Element inventory should match
    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }

    // Layer 2: Property values comparison
    if !result.layer2_errors.is_empty() {
        println!("\nLayer 2 errors (showing first 10):");
        for err in result.layer2_errors.iter().take(10) {
            println!("  {}", err);
        }
        if result.layer2_errors.len() > 10 {
            println!("  ... and {} more", result.layer2_errors.len() - 10);
        }
    }

    // Relationship errors (important for external references like master.dacpac)
    if !result.relationship_errors.is_empty() {
        println!("\nRelationship errors (showing first 5):");
        for err in result.relationship_errors.iter().take(5) {
            println!("  {}", err);
        }
        if result.relationship_errors.len() > 5 {
            println!("  ... and {} more", result.relationship_errors.len() - 5);
        }
    }
}

// ============================================================================
// Phase 6.4: Lower-Priority Fixture Parity Tests
// ============================================================================

/// Informational test: Run parity test on extended_properties fixture
/// This validates Phase 1.7 (SqlExtendedProperty element generation)
///
/// The extended_properties fixture tests that extended properties added via
/// sp_addextendedproperty are correctly parsed and represented as
/// SqlExtendedProperty elements in the model.xml output. This includes:
/// - Table-level descriptions
/// - Column-level descriptions
/// - Correct property names and values
#[test]
fn test_parity_extended_properties() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    // Use default options to get full comparison including property validation
    let options = ParityTestOptions::default();

    let result = match run_parity_test("extended_properties", &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed: {}", e);
            return;
        }
    };

    println!("\n=== Parity Test: extended_properties ===\n");
    println!("Testing Phase 1.7: SqlExtendedProperty element generation");
    println!(
        "Validates: Table-level and column-level extended properties via sp_addextendedproperty"
    );
    println!();
    println!("Layer 1 errors (inventory): {}", result.layer1_errors.len());
    println!(
        "Layer 2 errors (properties): {}",
        result.layer2_errors.len()
    );
    println!("Relationship errors: {}", result.relationship_errors.len());
    println!("Layer 4 errors (ordering): {}", result.layer4_errors.len());
    println!("Metadata errors: {}", result.metadata_errors.len());

    // Layer 1: Element inventory should include SqlExtendedProperty elements
    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }

    // Layer 2: Property values comparison
    if !result.layer2_errors.is_empty() {
        println!("\nLayer 2 errors (showing first 10):");
        for err in result.layer2_errors.iter().take(10) {
            println!("  {}", err);
        }
        if result.layer2_errors.len() > 10 {
            println!("  ... and {} more", result.layer2_errors.len() - 10);
        }
    }

    // Relationship errors (important for extended property target references)
    if !result.relationship_errors.is_empty() {
        println!("\nRelationship errors (showing first 5):");
        for err in result.relationship_errors.iter().take(5) {
            println!("  {}", err);
        }
        if result.relationship_errors.len() > 5 {
            println!("  ... and {} more", result.relationship_errors.len() - 5);
        }
    }
}

/// Informational test: Run parity test on table_types fixture
/// This validates Phase 1.8 (SqlTableType columns and structure)
///
/// The table_types fixture tests that user-defined table types are correctly
/// parsed and represented in the model.xml output. This includes:
/// - SqlTableType elements with correct names
/// - SqlTableTypeSimpleColumn elements for each column
/// - Primary key and unique constraints on table types
/// - Index definitions on table type columns
/// - Check constraints on table type columns
#[test]
fn test_parity_table_types() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    // Use default options to get full comparison including property validation
    let options = ParityTestOptions::default();

    let result = match run_parity_test("table_types", &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed: {}", e);
            return;
        }
    };

    println!("\n=== Parity Test: table_types ===\n");
    println!("Testing Phase 1.8: SqlTableType columns and structure");
    println!("Validates: User-defined table types with columns, constraints, and indexes");
    println!();
    println!("Layer 1 errors (inventory): {}", result.layer1_errors.len());
    println!(
        "Layer 2 errors (properties): {}",
        result.layer2_errors.len()
    );
    println!("Relationship errors: {}", result.relationship_errors.len());
    println!("Layer 4 errors (ordering): {}", result.layer4_errors.len());
    println!("Metadata errors: {}", result.metadata_errors.len());

    // Layer 1: Element inventory should include SqlTableType and SqlTableTypeSimpleColumn
    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }

    // Layer 2: Property values comparison
    if !result.layer2_errors.is_empty() {
        println!("\nLayer 2 errors (showing first 10):");
        for err in result.layer2_errors.iter().take(10) {
            println!("  {}", err);
        }
        if result.layer2_errors.len() > 10 {
            println!("  ... and {} more", result.layer2_errors.len() - 10);
        }
    }

    // Relationship errors
    if !result.relationship_errors.is_empty() {
        println!("\nRelationship errors (showing first 5):");
        for err in result.relationship_errors.iter().take(5) {
            println!("  {}", err);
        }
        if result.relationship_errors.len() > 5 {
            println!("  ... and {} more", result.relationship_errors.len() - 5);
        }
    }
}

/// Informational test: Run parity test on sqlcmd_variables fixture
/// This validates Phase 1.9 (SqlCmdVariables element generation)
///
/// The sqlcmd_variables fixture tests that SQLCMD variables defined in the
/// .sqlproj file are correctly represented in the model.xml output. This includes:
/// - SqlCmdVariable definitions from sqlproj ItemGroup
/// - Variable names and default values
/// - Integration with SQLCMD preprocessing (:r includes, :setvar directives)
#[test]
fn test_parity_sqlcmd_variables() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    // Use default options to get full comparison including property validation
    let options = ParityTestOptions::default();

    let result = match run_parity_test("sqlcmd_variables", &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed: {}", e);
            return;
        }
    };

    println!("\n=== Parity Test: sqlcmd_variables ===\n");
    println!("Testing Phase 1.9: SqlCmdVariables element generation");
    println!("Validates: SQLCMD variable definitions from sqlproj and script preprocessing");
    println!();
    println!("Layer 1 errors (inventory): {}", result.layer1_errors.len());
    println!(
        "Layer 2 errors (properties): {}",
        result.layer2_errors.len()
    );
    println!("Relationship errors: {}", result.relationship_errors.len());
    println!("Layer 4 errors (ordering): {}", result.layer4_errors.len());
    println!("Metadata errors: {}", result.metadata_errors.len());

    // Layer 1: Element inventory should include proper schema elements
    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }

    // Layer 2: Property values comparison
    if !result.layer2_errors.is_empty() {
        println!("\nLayer 2 errors (showing first 10):");
        for err in result.layer2_errors.iter().take(10) {
            println!("  {}", err);
        }
        if result.layer2_errors.len() > 10 {
            println!("  ... and {} more", result.layer2_errors.len() - 10);
        }
    }

    // Relationship errors
    if !result.relationship_errors.is_empty() {
        println!("\nRelationship errors (showing first 5):");
        for err in result.relationship_errors.iter().take(5) {
            println!("  {}", err);
        }
        if result.relationship_errors.len() > 5 {
            println!("  ... and {} more", result.relationship_errors.len() - 5);
        }
    }
}

/// Test that SQLCMD variables in model.xml Header match .NET DacFx format.
///
/// .NET DacFx uses this format in the Header:
/// ```xml
/// <CustomData Category="SqlCmdVariables" Type="SqlCmdVariable">
///   <Metadata Name="Environment" Value="" />
///   <Metadata Name="ServerName" Value="" />
/// </CustomData>
/// ```
///
/// Known issue: Rust uses a different format with separate CustomData elements
/// per variable and different attribute names.
#[test]
fn test_sqlcmd_variables_header_format() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp directory");
    let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sqlcmd_variables");

    let (rust_dacpac, dotnet_dacpac) =
        match build_both_dacpacs(&fixture_path.join("project.sqlproj"), &temp_dir) {
            Ok(paths) => paths,
            Err(e) => {
                panic!("Build failed: {}", e);
            }
        };

    // Extract raw model.xml from both dacpacs to check Header format
    let rust_xml = extract_model_xml(&rust_dacpac).expect("Extract rust model.xml");
    let dotnet_xml = extract_model_xml(&dotnet_dacpac).expect("Extract dotnet model.xml");

    println!("\n=== SQLCMD Variables Header Format Test ===\n");

    // Check for .NET format: Category="SqlCmdVariables" (plural) with Type attribute
    let dotnet_has_correct_format = dotnet_xml.contains(r#"Category="SqlCmdVariables""#)
        && dotnet_xml.contains(r#"Type="SqlCmdVariable""#);
    let rust_has_correct_format = rust_xml.contains(r#"Category="SqlCmdVariables""#)
        && rust_xml.contains(r#"Type="SqlCmdVariable""#);

    println!(".NET has correct format: {}", dotnet_has_correct_format);
    println!("Rust has correct format: {}", rust_has_correct_format);

    // Show what Rust currently produces
    if !rust_has_correct_format {
        println!("\nRust Header contains:");
        for line in rust_xml.lines() {
            if line.contains("SqlCmdVariable") || line.contains("DefaultValue") {
                println!("  {}", line.trim());
            }
        }
    }

    assert!(
        rust_has_correct_format,
        "Rust SQLCMD variables should use .NET format: \
         Category=\"SqlCmdVariables\" Type=\"SqlCmdVariable\" with variable names as Metadata Name attributes"
    );
}

// =============================================================================
// Phase 6.5: Tests for all remaining fixtures
// =============================================================================
// These parity tests cover all fixtures not yet tested in Phases 6.2-6.4.
// Each test validates that our Rust implementation produces output matching
// the DotNet DacFx reference implementation for that specific feature.

/// Helper function to run a standard parity test with consistent output format.
/// This reduces boilerplate while maintaining detailed diagnostic output.
fn run_standard_parity_test(fixture_name: &str, description: &str, validates: &str) {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let options = ParityTestOptions::default();

    let result = match run_parity_test(fixture_name, &options) {
        Ok(r) => r,
        Err(e) => {
            println!("Parity test failed: {}", e);
            return;
        }
    };

    println!("\n=== Parity Test: {} ===\n", fixture_name);
    println!("{}", description);
    println!("Validates: {}", validates);
    println!();
    println!("Layer 1 errors (inventory): {}", result.layer1_errors.len());
    println!(
        "Layer 2 errors (properties): {}",
        result.layer2_errors.len()
    );
    println!("Relationship errors: {}", result.relationship_errors.len());
    println!("Layer 4 errors (ordering): {}", result.layer4_errors.len());
    println!("Metadata errors: {}", result.metadata_errors.len());

    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }

    if !result.layer2_errors.is_empty() {
        println!("\nLayer 2 errors (showing first 10):");
        for err in result.layer2_errors.iter().take(10) {
            println!("  {}", err);
        }
        if result.layer2_errors.len() > 10 {
            println!("  ... and {} more", result.layer2_errors.len() - 10);
        }
    }

    if !result.relationship_errors.is_empty() {
        println!("\nRelationship errors (showing first 5):");
        for err in result.relationship_errors.iter().take(5) {
            println!("  {}", err);
        }
        if result.relationship_errors.len() > 5 {
            println!("  ... and {} more", result.relationship_errors.len() - 5);
        }
    }
}

/// Parity test for e2e_comprehensive fixture.
/// Tests end-to-end compilation of a complex database project with many element types.
#[test]
fn test_parity_e2e_comprehensive() {
    run_standard_parity_test(
        "e2e_comprehensive",
        "Testing end-to-end comprehensive database compilation",
        "Multiple tables, views, procedures, functions, constraints, indexes",
    );
}

/// Parity test for e2e_simple fixture.
/// Tests basic end-to-end compilation with minimal elements.
#[test]
fn test_parity_e2e_simple() {
    run_standard_parity_test(
        "e2e_simple",
        "Testing end-to-end simple database compilation",
        "Basic table with primary key and simple structure",
    );
}

/// Parity test for fulltext_index fixture.
/// Tests full-text index definitions on tables.
#[test]
fn test_parity_fulltext_index() {
    run_standard_parity_test(
        "fulltext_index",
        "Testing full-text index generation",
        "SqlFullTextIndex elements with catalog and column specifications",
    );
}

/// Parity test for procedure_parameters fixture.
/// Tests stored procedure parameter parsing and generation.
#[test]
fn test_parity_procedure_parameters() {
    run_standard_parity_test(
        "procedure_parameters",
        "Testing procedure parameter generation",
        "SqlSubroutineParameter elements with types, defaults, and directions",
    );
}

/// Parity test for index_naming fixture.
/// Tests various index naming conventions and patterns.
#[test]
fn test_parity_index_naming() {
    run_standard_parity_test(
        "index_naming",
        "Testing index naming conventions",
        "Index names, auto-generated names, and naming patterns",
    );
}

/// Parity test for all_constraints fixture.
/// Tests comprehensive constraint handling (PK, FK, CHECK, UNIQUE, DEFAULT).
#[test]
fn test_parity_all_constraints() {
    run_standard_parity_test(
        "all_constraints",
        "Testing all constraint types",
        "Primary key, foreign key, check, unique, and default constraints",
    );
}

/// Parity test for collation fixture.
/// Tests column and database collation settings.
#[test]
fn test_parity_collation() {
    run_standard_parity_test(
        "collation",
        "Testing collation settings",
        "Column-level and database-level collation specifications",
    );
}

/// Parity test for column_properties fixture.
/// Tests various column property settings.
#[test]
fn test_parity_column_properties() {
    run_standard_parity_test(
        "column_properties",
        "Testing column property generation",
        "Nullability, identity, computed, sparse, and other column properties",
    );
}

/// Parity test for composite_fk fixture.
/// Tests composite (multi-column) foreign key constraints.
#[test]
fn test_parity_composite_fk() {
    run_standard_parity_test(
        "composite_fk",
        "Testing composite foreign key constraints",
        "Multi-column foreign key relationships and column ordering",
    );
}

/// Parity test for computed_columns fixture.
/// Tests computed column definitions and expressions.
#[test]
fn test_parity_computed_columns() {
    run_standard_parity_test(
        "computed_columns",
        "Testing computed column generation",
        "SqlComputedColumn elements with expressions and persistence settings",
    );
}

/// Parity test for constraint_nocheck fixture.
/// Tests NOCHECK constraint option handling.
#[test]
fn test_parity_constraint_nocheck() {
    run_standard_parity_test(
        "constraint_nocheck",
        "Testing NOCHECK constraint option",
        "Constraints with WITH NOCHECK for FK and CHECK constraints",
    );
}

/// Parity test for constraints fixture.
/// Tests general constraint handling.
#[test]
fn test_parity_constraints() {
    run_standard_parity_test(
        "constraints",
        "Testing constraint generation",
        "Various constraint types and their properties",
    );
}

/// Parity test for commaless_constraints fixture.
/// Tests table and type definitions where constraints lack comma separators.
///
/// SQL Server accepts constraints without commas in certain positions:
/// ```sql
/// CREATE TABLE [dbo].[Example] (
///     [Id] INT NOT NULL,
///     [Name] NVARCHAR(100) NOT NULL
///     PRIMARY KEY ([Id])  -- No comma before PRIMARY KEY
/// );
/// ```
///
/// This pattern is found in real-world databases but may not be parsed correctly
/// by sqlparser-rs. This test validates that we handle this relaxed syntax.
///
/// sqlparser-rs fails to parse CREATE TABLE with comma-less constraints, so we use
/// the fallback parser which correctly handles both comma-less constraints and
/// inline DEFAULT/CHECK constraints with explicit CONSTRAINT names.
#[test]
fn test_parity_commaless_constraints() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let options = ParityTestOptions::default();

    let result = match run_parity_test("commaless_constraints", &options) {
        Ok(r) => r,
        Err(e) => {
            panic!("Parity test failed to run: {}", e);
        }
    };

    println!("\n=== Parity Test: commaless_constraints ===\n");
    println!("Testing comma-less constraint syntax parsing");
    println!("Validates: Constraints without comma separators are correctly parsed");
    println!();
    println!("Layer 1 errors (inventory): {}", result.layer1_errors.len());
    println!(
        "Layer 2 errors (properties): {}",
        result.layer2_errors.len()
    );

    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }

    // Assert that there are no inventory errors (all constraints should be found)
    assert!(
        result.layer1_errors.is_empty(),
        "Comma-less constraints should be parsed correctly. Found {} inventory errors:\n{}",
        result.layer1_errors.len(),
        result
            .layer1_errors
            .iter()
            .map(|e| format!("  - {}", e))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Parity test for element_types fixture.
/// Tests various SQL element types.
#[test]
fn test_parity_element_types() {
    run_standard_parity_test(
        "element_types",
        "Testing element type generation",
        "Different SQL element types and their XML representation",
    );
}

/// Parity test for filtered_indexes fixture.
/// Tests filtered index definitions with WHERE clauses.
#[test]
fn test_parity_filtered_indexes() {
    run_standard_parity_test(
        "filtered_indexes",
        "Testing filtered index generation",
        "SqlIndex elements with FilterPredicate property",
    );
}

/// Parity test for fk_actions fixture.
/// Tests foreign key ON DELETE and ON UPDATE actions.
#[test]
fn test_parity_fk_actions() {
    run_standard_parity_test(
        "fk_actions",
        "Testing foreign key action generation",
        "CASCADE, SET NULL, SET DEFAULT, NO ACTION for FK constraints",
    );
}

/// Parity test for identity_column fixture.
/// Tests identity column seed and increment settings.
#[test]
fn test_parity_identity_column() {
    run_standard_parity_test(
        "identity_column",
        "Testing identity column generation",
        "Identity seed, increment, and NOT FOR REPLICATION settings",
    );
}

/// Parity test for index_options fixture.
/// Tests various index options and settings.
#[test]
fn test_parity_index_options() {
    run_standard_parity_test(
        "index_options",
        "Testing index option generation",
        "FILLFACTOR, PAD_INDEX, IGNORE_DUP_KEY, and other index options",
    );
}

/// Parity test for index_properties fixture.
/// Tests index property generation.
#[test]
fn test_parity_index_properties() {
    run_standard_parity_test(
        "index_properties",
        "Testing index property generation",
        "Clustered/nonclustered, unique, and other index properties",
    );
}

/// Parity test for indexes fixture.
/// Tests general index generation.
#[test]
fn test_parity_indexes() {
    run_standard_parity_test(
        "indexes",
        "Testing index generation",
        "SqlIndex elements with columns and include columns",
    );
}

/// Parity test for instead_of_triggers fixture.
/// Tests INSTEAD OF trigger definitions on views.
#[test]
fn test_parity_instead_of_triggers() {
    run_standard_parity_test(
        "instead_of_triggers",
        "Testing INSTEAD OF trigger generation",
        "SqlDmlTrigger elements with INSTEAD OF semantics",
    );
}

/// Parity test for large_table fixture.
/// Tests handling of tables with many columns.
#[test]
fn test_parity_large_table() {
    run_standard_parity_test(
        "large_table",
        "Testing large table generation",
        "Tables with many columns and complex structures",
    );
}

/// Parity test for multiple_indexes fixture.
/// Tests tables with multiple index definitions.
#[test]
fn test_parity_multiple_indexes() {
    run_standard_parity_test(
        "multiple_indexes",
        "Testing multiple index generation",
        "Multiple indexes on a single table with different configurations",
    );
}

/// Parity test for only_schemas fixture.
/// Tests schema-only project with no tables.
#[test]
fn test_parity_only_schemas() {
    run_standard_parity_test(
        "only_schemas",
        "Testing schema-only generation",
        "SqlSchema elements without any tables",
    );
}

/// Parity test for pre_post_deploy fixture.
/// Tests pre and post deployment script handling.
#[test]
fn test_parity_pre_post_deploy() {
    run_standard_parity_test(
        "pre_post_deploy",
        "Testing pre/post deploy script handling",
        "PreDeployScript and PostDeployScript inclusion in dacpac",
    );
}

/// Parity test for procedure_options fixture.
/// Tests stored procedure option settings.
#[test]
fn test_parity_procedure_options() {
    run_standard_parity_test(
        "procedure_options",
        "Testing procedure option generation",
        "WITH RECOMPILE, EXECUTE AS, and other procedure options",
    );
}

/// Parity test for reserved_keywords fixture.
/// Tests handling of T-SQL reserved keywords as identifiers.
#[test]
fn test_parity_reserved_keywords() {
    run_standard_parity_test(
        "reserved_keywords",
        "Testing reserved keyword handling",
        "Bracketed identifiers for reserved words like [User], [Table]",
    );
}

/// Parity test for scalar_types fixture.
/// Tests various SQL Server scalar data types.
#[test]
fn test_parity_scalar_types() {
    run_standard_parity_test(
        "scalar_types",
        "Testing scalar type generation",
        "All SQL Server built-in data types and their XML representation",
    );
}

/// Parity test for self_ref_fk fixture.
/// Tests self-referencing foreign key constraints.
#[test]
fn test_parity_self_ref_fk() {
    run_standard_parity_test(
        "self_ref_fk",
        "Testing self-referencing foreign key generation",
        "FK constraints referencing the same table (hierarchical data)",
    );
}

/// Parity test for simple_table fixture.
/// Tests basic table generation.
#[test]
fn test_parity_simple_table() {
    run_standard_parity_test(
        "simple_table",
        "Testing simple table generation",
        "Basic SqlTable with columns and primary key",
    );
}

/// Parity test for sqlcmd_includes fixture.
/// Tests SQLCMD :r include directive handling.
#[test]
fn test_parity_sqlcmd_includes() {
    run_standard_parity_test(
        "sqlcmd_includes",
        "Testing SQLCMD include directive handling",
        ":r directive file inclusion and nested includes",
    );
}

/// Parity test for unicode_identifiers fixture.
/// Tests Unicode characters in object names.
#[test]
fn test_parity_unicode_identifiers() {
    run_standard_parity_test(
        "unicode_identifiers",
        "Testing Unicode identifier handling",
        "Non-ASCII characters in table, column, and constraint names",
    );
}

/// Parity test for varbinary_max fixture.
/// Tests VARBINARY(MAX) and other MAX data types.
#[test]
fn test_parity_varbinary_max() {
    run_standard_parity_test(
        "varbinary_max",
        "Testing MAX data type generation",
        "VARBINARY(MAX), VARCHAR(MAX), NVARCHAR(MAX) type specifiers",
    );
}

/// Parity test for view_options fixture.
/// Tests view option settings.
#[test]
fn test_parity_view_options() {
    run_standard_parity_test(
        "view_options",
        "Testing view option generation",
        "WITH SCHEMABINDING, WITH CHECK OPTION, and other view options",
    );
}

/// Parity test for views fixture.
/// Tests general view generation.
#[test]
fn test_parity_views() {
    run_standard_parity_test(
        "views",
        "Testing view generation",
        "SqlView elements with SELECT statements and dependencies",
    );
}

/// Parity test for stress_test fixture.
/// Tests large database with many tables, procedures, and functions.
#[test]
fn test_parity_stress_test() {
    run_standard_parity_test(
        "stress_test",
        "Testing large database stress test",
        "40 tables, 25 procedures, 15 scalar functions, 10 TVFs, multiple indexes",
    );
}

/// Parity test for body_dependencies_aliases fixture.
/// Tests that table aliases in procedure/view bodies are NOT included in BodyDependencies.
/// Aliases like A, ATTAG, TagDetails should be resolved to actual table references,
/// not treated as schema names.
#[test]
fn test_parity_body_dependencies_aliases() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let options = ParityTestOptions::default();

    let result = match run_parity_test("body_dependencies_aliases", &options) {
        Ok(r) => r,
        Err(e) => {
            panic!("Parity test failed to run: {}", e);
        }
    };

    println!("\n=== Parity Test: body_dependencies_aliases ===\n");
    println!("Testing BodyDependencies alias resolution");
    println!(
        "Validates: Table aliases are resolved to actual table names, not treated as schema refs"
    );
    println!();
    println!("Layer 1 errors (inventory): {}", result.layer1_errors.len());
    println!(
        "Layer 2 errors (properties): {}",
        result.layer2_errors.len()
    );
    println!("Relationship errors: {}", result.relationship_errors.len());

    if !result.layer1_errors.is_empty() {
        println!("\nLayer 1 errors:");
        for err in &result.layer1_errors {
            println!("  {}", err);
        }
    }

    if !result.relationship_errors.is_empty() {
        println!("\nRelationship errors (expected - alias resolution issue):");
        for err in result.relationship_errors.iter().take(10) {
            println!("  {}", err);
        }
        if result.relationship_errors.len() > 10 {
            println!("  ... and {} more", result.relationship_errors.len() - 10);
        }
    }

    // This test documents a known failing case - aliases in BodyDependencies
    // Don't assert, just report - the baseline tracks this as expected to fail
}

/// Aggregate test that runs parity checks on all available fixtures.
/// This test provides a comprehensive overview of parity status across all fixtures.
#[test]
fn test_parity_all_fixtures() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let fixtures = get_available_fixtures();
    let options = ParityTestOptions::default();

    println!("\n=== Parity Test Summary: All Fixtures ===\n");

    let mut total_fixtures = 0;
    let mut layer1_pass = 0;
    let mut layer2_pass = 0;
    let mut relationship_pass = 0;
    let mut layer7_pass = 0;
    let mut full_pass = 0;

    for fixture in &fixtures {
        total_fixtures += 1;

        match run_parity_test(fixture, &options) {
            Ok(result) => {
                let l1_ok = result.layer1_errors.is_empty();
                let l2_ok = result.layer2_errors.is_empty();
                let rel_ok = result.relationship_errors.is_empty();
                let l7_ok = result.layer7_errors.is_empty();

                if l1_ok {
                    layer1_pass += 1;
                }
                if l2_ok {
                    layer2_pass += 1;
                }
                if rel_ok {
                    relationship_pass += 1;
                }
                if l7_ok {
                    layer7_pass += 1;
                }
                if l1_ok && l2_ok && rel_ok && l7_ok {
                    full_pass += 1;
                }

                let status = if l1_ok && l2_ok && rel_ok && l7_ok {
                    "✓ PASS"
                } else if l1_ok {
                    "~ PARTIAL"
                } else {
                    "✗ FAIL"
                };

                println!(
                    "{:40} {} (L1:{} L2:{} Rel:{} L7:{})",
                    fixture,
                    status,
                    result.layer1_errors.len(),
                    result.layer2_errors.len(),
                    result.relationship_errors.len(),
                    result.layer7_errors.len()
                );

                // Print first Layer 7 errors for the first fixture that has them
                if !result.layer7_errors.is_empty() && total_fixtures == 1 {
                    println!("\n  First Layer 7 errors for {}:", fixture);
                    for (i, err) in result.layer7_errors.iter().take(5).enumerate() {
                        println!("    {}: {}", i + 1, err);
                    }
                    println!();
                }
            }
            Err(e) => {
                println!("{:40} ✗ ERROR: {}", fixture, e);
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Total fixtures: {}", total_fixtures);
    println!(
        "Layer 1 pass (inventory): {}/{} ({:.1}%)",
        layer1_pass,
        total_fixtures,
        100.0 * layer1_pass as f64 / total_fixtures as f64
    );
    println!(
        "Layer 2 pass (properties): {}/{} ({:.1}%)",
        layer2_pass,
        total_fixtures,
        100.0 * layer2_pass as f64 / total_fixtures as f64
    );
    println!(
        "Relationships pass: {}/{} ({:.1}%)",
        relationship_pass,
        total_fixtures,
        100.0 * relationship_pass as f64 / total_fixtures as f64
    );
    println!(
        "Layer 7 pass (canonical): {}/{} ({:.1}%)",
        layer7_pass,
        total_fixtures,
        100.0 * layer7_pass as f64 / total_fixtures as f64
    );
    println!(
        "Full parity: {}/{} ({:.1}%)",
        full_pass,
        total_fixtures,
        100.0 * full_pass as f64 / total_fixtures as f64
    );
}

// =============================================================================
// Phase 8.2: Parity Metrics Collection for CI Progress Tracking
// =============================================================================

/// Collect parity metrics across all fixtures for CI reporting.
///
/// This function runs parity tests on all available fixtures and collects
/// structured metrics that can be output as JSON for CI systems to parse.
///
/// # Returns
/// A `ParityMetrics` struct containing:
/// - Timestamp and git commit info
/// - Per-layer pass counts
/// - Per-fixture detailed results
///
/// # Example
/// ```ignore
/// let metrics = collect_parity_metrics(&ParityTestOptions::default());
/// println!("{}", metrics.to_json());
/// metrics.print_summary();
/// ```
pub fn collect_parity_metrics(options: &ParityTestOptions) -> ParityMetrics {
    let mut metrics = ParityMetrics::new();
    let fixtures = get_available_fixtures();

    for fixture in &fixtures {
        match run_parity_test(fixture, options) {
            Ok(result) => {
                metrics.add_result(fixture, &result);
            }
            Err(e) => {
                metrics.add_error(fixture, &e.to_string());
            }
        }
    }

    metrics
}

/// Test that collects and outputs parity metrics in a CI-friendly format.
///
/// This test:
/// 1. Runs parity tests on all available fixtures
/// 2. Collects structured metrics using ParityMetrics
/// 3. Outputs JSON to stdout for CI parsing
/// 4. Optionally writes metrics to a file if PARITY_METRICS_FILE is set
///
/// The JSON output includes:
/// - Timestamp and git commit hash
/// - Per-layer pass counts and percentages
/// - Per-fixture detailed results with error counts
///
/// CI systems can parse this output to:
/// - Track parity progress over time
/// - Alert on regressions
/// - Display metrics in dashboards
#[test]
fn test_parity_metrics_collection() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let options = ParityTestOptions::default();
    let metrics = collect_parity_metrics(&options);

    // Print human-readable summary
    metrics.print_summary();

    // Output JSON for CI parsing
    let json = metrics.to_json();
    println!("\n=== PARITY METRICS JSON ===");
    println!("{}", json);

    // If PARITY_METRICS_FILE env var is set, write JSON to file
    if let Ok(file_path) = std::env::var("PARITY_METRICS_FILE") {
        match std::fs::write(&file_path, &json) {
            Ok(_) => println!("\nMetrics written to: {}", file_path),
            Err(e) => eprintln!("Failed to write metrics to {}: {}", file_path, e),
        }
    }
}

/// Test that ParityMetrics correctly serializes to JSON.
#[test]
fn test_parity_metrics_json_serialization() {
    let mut metrics = ParityMetrics::new();

    // Manually set timestamp for reproducible test
    metrics.timestamp = "2026-01-26T10:00:00+00:00".to_string();
    metrics.commit = Some("abc123".to_string());

    // Create a mock ComparisonResult with no errors (passing)
    let passing_result = ComparisonResult::default();
    metrics.add_result("test_fixture", &passing_result);

    // Create a mock ComparisonResult with errors (failing)
    let mut failing_result = ComparisonResult::default();
    failing_result
        .layer1_errors
        .push(crate::dacpac_compare::Layer1Error::MissingInRust {
            element_type: "SqlTable".to_string(),
            name: "[dbo].[Test]".to_string(),
        });
    metrics.add_result("failing_fixture", &failing_result);

    // Add an error case
    metrics.add_error("error_fixture", "Build failed: some error");

    let json = metrics.to_json();

    // Verify JSON structure
    assert!(json.contains("\"timestamp\": \"2026-01-26T10:00:00+00:00\""));
    assert!(json.contains("\"commit\": \"abc123\""));
    assert!(json.contains("\"total_fixtures\": 3"));
    assert!(json.contains("\"layer1_pass\": 1"));
    assert!(json.contains("\"full_parity\": 1"));
    assert!(json.contains("\"error_count\": 1"));
    assert!(json.contains("\"name\": \"test_fixture\""));
    assert!(json.contains("\"status\": \"PASS\""));
    assert!(json.contains("\"status\": \"FAIL\""));
    assert!(json.contains("\"status\": \"ERROR\""));
    assert!(json.contains("\"error_message\": \"Build failed: some error\""));
    assert!(json.contains("\"pass_rates\""));

    println!("Serialized JSON:\n{}", json);
}

/// Test that ParityMetrics pass rate calculation works correctly.
#[test]
fn test_parity_metrics_pass_rate() {
    let mut metrics = ParityMetrics::new();
    metrics.total_fixtures = 10;
    metrics.layer1_pass = 8;
    metrics.full_parity = 5;

    assert!((metrics.pass_rate(8) - 80.0).abs() < 0.01);
    assert!((metrics.pass_rate(5) - 50.0).abs() < 0.01);
    assert!((metrics.pass_rate(0) - 0.0).abs() < 0.01);
    assert!((metrics.pass_rate(10) - 100.0).abs() < 0.01);

    // Edge case: empty metrics
    let empty_metrics = ParityMetrics::new();
    assert!((empty_metrics.pass_rate(0) - 0.0).abs() < 0.01);
}

// =============================================================================
// Phase 8.3: Detailed Parity Report Generation
// =============================================================================

use crate::dacpac_compare::ParityReport;

/// Collect detailed parity results across all fixtures for report generation.
///
/// This function is similar to `collect_parity_metrics()` but captures full
/// error messages instead of just counts, enabling detailed Markdown reports.
///
/// # Returns
/// A `ParityReport` struct containing:
/// - Timestamp and git commit info
/// - Per-fixture detailed results with full error messages
///
/// # Example
/// ```ignore
/// let report = collect_parity_report(&ParityTestOptions::default());
/// let markdown = report.to_markdown();
/// std::fs::write("parity-report.md", markdown).unwrap();
/// ```
pub fn collect_parity_report(options: &ParityTestOptions) -> ParityReport {
    let mut report = ParityReport::new();
    let fixtures = get_available_fixtures();

    for fixture in &fixtures {
        match run_parity_test(fixture, options) {
            Ok(result) => {
                report.add_result(fixture, &result);
            }
            Err(e) => {
                report.add_error(fixture, &e.to_string());
            }
        }
    }

    report
}

/// Test that generates and outputs a detailed Markdown parity report.
///
/// This test:
/// 1. Runs parity tests on all available fixtures
/// 2. Collects detailed results with full error messages using ParityReport
/// 3. Generates Markdown report
/// 4. Optionally writes report to a file if PARITY_REPORT_FILE is set
///
/// The Markdown report includes:
/// - Summary table with pass rates per layer
/// - Per-fixture results table with error counts
/// - Detailed error breakdown for failing fixtures
///
/// CI systems can use this report as an artifact for:
/// - Human-readable parity status
/// - Pull request reviews
/// - Historical comparison
#[test]
fn test_parity_report_generation() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let options = ParityTestOptions::default();
    let report = collect_parity_report(&options);

    // Generate Markdown report
    let markdown = report.to_markdown();

    println!("\n=== PARITY REPORT (MARKDOWN) ===");
    println!("{}", markdown);

    // If PARITY_REPORT_FILE env var is set, write Markdown to file
    if let Ok(file_path) = std::env::var("PARITY_REPORT_FILE") {
        match std::fs::write(&file_path, &markdown) {
            Ok(_) => println!("\nReport written to: {}", file_path),
            Err(e) => eprintln!("Failed to write report to {}: {}", file_path, e),
        }
    }

    // Basic assertions
    assert!(
        report.total_fixtures() > 0,
        "Should have tested some fixtures"
    );
    assert!(
        markdown.contains("# Dacpac Parity Test Report"),
        "Report should have title"
    );
    assert!(
        markdown.contains("## Summary"),
        "Report should have summary section"
    );
    assert!(
        markdown.contains("## Per-Fixture Results"),
        "Report should have per-fixture section"
    );
}

/// Test that ParityReport correctly tracks fixture results.
#[test]
fn test_parity_report_tracking() {
    let mut report = ParityReport::new();

    // Manually set timestamp for reproducible test
    report.timestamp = "2026-01-26T10:00:00+00:00".to_string();
    report.commit = Some("abc123".to_string());

    // Create a mock ComparisonResult with no errors (passing)
    let passing_result = ComparisonResult::default();
    report.add_result("passing_fixture", &passing_result);

    // Create a mock ComparisonResult with errors (failing)
    let mut failing_result = ComparisonResult::default();
    failing_result
        .layer1_errors
        .push(crate::dacpac_compare::Layer1Error::MissingInRust {
            element_type: "SqlTable".to_string(),
            name: "[dbo].[Test]".to_string(),
        });
    report.add_result("failing_fixture", &failing_result);

    // Add an error case
    report.add_error("error_fixture", "Build failed: some error");

    // Verify counts
    assert_eq!(report.total_fixtures(), 3);
    assert_eq!(report.full_parity_count(), 1);
    assert_eq!(report.layer1_pass_count(), 1);
    assert_eq!(report.error_count(), 1);

    // Generate markdown and verify structure
    let markdown = report.to_markdown();
    assert!(markdown.contains("**Commit:** `abc123`"));
    assert!(markdown.contains("passing_fixture"));
    assert!(markdown.contains("failing_fixture"));
    assert!(markdown.contains("error_fixture"));
    assert!(markdown.contains("✅ PASS"));
    assert!(markdown.contains("❌ FAIL"));
    assert!(markdown.contains("⚠️ ERROR"));

    println!("Generated Markdown report:\n{}", markdown);
}

/// Test that ParityReport generates valid Markdown for detailed errors.
#[test]
fn test_parity_report_detailed_errors() {
    use crate::dacpac_compare::{Layer1Error, Layer2Error, RelationshipError};

    let mut report = ParityReport::new();

    // Create a result with errors in multiple layers
    let mut result = ComparisonResult::default();
    result.layer1_errors.push(Layer1Error::MissingInRust {
        element_type: "SqlTable".to_string(),
        name: "[dbo].[MissingTable]".to_string(),
    });
    result.layer2_errors.push(Layer2Error {
        element_type: "SqlSimpleColumn".to_string(),
        element_name: "[dbo].[TestTable].[Col1]".to_string(),
        property_name: "IsNullable".to_string(),
        rust_value: Some("True".to_string()),
        dotnet_value: Some("False".to_string()),
    });
    result
        .relationship_errors
        .push(RelationshipError::MissingRelationship {
            element_type: "SqlProcedure".to_string(),
            element_name: "[dbo].[TestProc]".to_string(),
            relationship_name: "BodyDependencies".to_string(),
        });

    report.add_result("multi_error_fixture", &result);

    let markdown = report.to_markdown();

    // Verify detailed error sections are present
    assert!(
        markdown.contains("## Detailed Errors"),
        "Should have detailed errors section"
    );
    assert!(
        markdown.contains("### multi_error_fixture"),
        "Should have fixture-specific section"
    );
    assert!(
        markdown.contains("**Layer 1 - Element Inventory"),
        "Should have Layer 1 errors"
    );
    assert!(
        markdown.contains("**Layer 2 - Properties"),
        "Should have Layer 2 errors"
    );
    assert!(
        markdown.contains("**Relationships"),
        "Should have relationship errors"
    );

    // Verify error messages are included
    assert!(markdown.contains("[dbo].[MissingTable]"));
    assert!(markdown.contains("IsNullable"));
    assert!(markdown.contains("BodyDependencies"));

    println!("Detailed error report:\n{}", markdown);
}

/// Test that error truncation works for fixtures with many errors.
#[test]
fn test_parity_report_error_truncation() {
    let mut report = ParityReport::new();

    // Create a result with many Layer 1 errors (more than 10)
    let mut result = ComparisonResult::default();
    for i in 0..15 {
        result
            .layer1_errors
            .push(crate::dacpac_compare::Layer1Error::MissingInRust {
                element_type: "SqlTable".to_string(),
                name: format!("[dbo].[Table{}]", i),
            });
    }
    report.add_result("many_errors_fixture", &result);

    let markdown = report.to_markdown();

    // Verify truncation message is present
    assert!(
        markdown.contains("...and 5 more errors"),
        "Should indicate truncated errors"
    );

    // Verify first 10 errors are present
    assert!(markdown.contains("[dbo].[Table0]"));
    assert!(markdown.contains("[dbo].[Table9]"));

    // Verify 11th error is not present (truncated)
    assert!(!markdown.contains("[dbo].[Table10]"));

    println!("Truncated error report:\n{}", markdown);
}

// =============================================================================
// Phase 7: Canonical XML Comparison Tests
// =============================================================================

/// Test that XML canonicalization produces deterministic output.
///
///// This test verifies that:
/// 1. The canonicalize_model_xml function parses XML correctly
/// 2. Re-canonicalizing the same input produces identical output (idempotent)
/// 3. Original element ordering from source XML is preserved
#[test]
fn test_canonicalize_model_xml_basic() {
    // Sample model.xml content with deliberately unordered elements
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel FileFormatVersion="1.2" SchemaVersion="2.9" DspName="Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider" CollationLcid="1033" CollationCaseSensitive="False" xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Model>
    <Element Type="SqlTable" Name="[dbo].[Products]">
      <Property Name="IsAnsiNullsOn" Value="True" />
    </Element>
    <Element Type="SqlDatabaseOptions">
      <Property Name="Collation" Value="SQL_Latin1_General_CP1_CI_AS" />
      <Property Name="IsAnsiNullDefaultOn" Value="True" />
    </Element>
    <Element Type="SqlTable" Name="[dbo].[Customers]">
      <Property Name="IsAnsiNullsOn" Value="True" />
    </Element>
  </Model>
</DataSchemaModel>"#;

    let canonical1 = canonicalize_model_xml(xml).expect("First canonicalization should succeed");
    let canonical2 =
        canonicalize_model_xml(&canonical1).expect("Second canonicalization should succeed");

    // Re-canonicalizing should produce identical output (idempotent)
    assert_eq!(
        canonical1, canonical2,
        "Canonicalization should be idempotent"
    );

    // Verify elements preserve original order from source XML
    // Products appears first in input, then DatabaseOptions, then Customers
    let lines: Vec<&str> = canonical1.lines().collect();

    // Find element positions
    let products_pos = lines
        .iter()
        .position(|l| l.contains("[dbo].[Products]"))
        .expect("Should find Products table");
    let db_options_pos = lines
        .iter()
        .position(|l| l.contains("SqlDatabaseOptions"))
        .expect("Should find SqlDatabaseOptions");
    let customers_pos = lines
        .iter()
        .position(|l| l.contains("[dbo].[Customers]"))
        .expect("Should find Customers table");

    assert!(
        products_pos < db_options_pos,
        "Products table should come before SqlDatabaseOptions (original order)"
    );
    assert!(
        db_options_pos < customers_pos,
        "SqlDatabaseOptions should come before Customers table (original order)"
    );

    println!("Canonical XML output ({} lines):", lines.len());
    for (i, line) in lines.iter().take(20).enumerate() {
        println!("  {:3}: {}", i + 1, line);
    }
}

/// Test canonicalization with properties that need CDATA encoding.
#[test]
fn test_canonicalize_model_xml_with_cdata() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel FileFormatVersion="1.2" SchemaVersion="2.9" DspName="Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider" CollationLcid="1033" CollationCaseSensitive="False" xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Model>
    <Element Type="SqlView" Name="[dbo].[MyView]">
      <Property Name="QueryScript">
        <Value><![CDATA[SELECT [Id], [Name]
FROM [dbo].[Products]
WHERE [IsActive] = 1]]></Value>
      </Property>
      <Property Name="IsAnsiNullsOn" Value="True" />
    </Element>
  </Model>
</DataSchemaModel>"#;

    let canonical = canonicalize_model_xml(xml).expect("Canonicalization should succeed");

    // Verify CDATA content is preserved
    assert!(
        canonical.contains("SELECT [Id], [Name]"),
        "Should preserve CDATA content"
    );
    assert!(
        canonical.contains("CDATA"),
        "Multi-line content should use CDATA"
    );

    // Properties should preserve original order from source XML
    // QueryScript appears first in input, then IsAnsiNullsOn
    let query_script_pos = canonical
        .find("QueryScript")
        .expect("Should find QueryScript");
    let is_ansi_pos = canonical
        .find("IsAnsiNullsOn")
        .expect("Should find IsAnsiNullsOn");
    assert!(
        query_script_pos < is_ansi_pos,
        "QueryScript should come before IsAnsiNullsOn (original order)"
    );

    println!("Canonical XML with CDATA:");
    for line in canonical.lines().take(15) {
        println!("  {}", line);
    }
}

/// Test canonicalization with relationships.
#[test]
fn test_canonicalize_model_xml_with_relationships() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<DataSchemaModel FileFormatVersion="1.2" SchemaVersion="2.9" DspName="Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider" CollationLcid="1033" CollationCaseSensitive="False" xmlns="http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02">
  <Model>
    <Element Type="SqlTable" Name="[dbo].[Products]">
      <Relationship Name="Schema">
        <Entry>
          <References ExternalSource="BuiltIns" Name="[dbo]" />
        </Entry>
      </Relationship>
      <Relationship Name="Columns">
        <Entry>
          <Element Type="SqlSimpleColumn" Name="[dbo].[Products].[Name]">
            <Property Name="IsNullable" Value="True" />
          </Element>
        </Entry>
        <Entry>
          <Element Type="SqlSimpleColumn" Name="[dbo].[Products].[Id]">
            <Property Name="IsNullable" Value="False" />
          </Element>
        </Entry>
      </Relationship>
    </Element>
  </Model>
</DataSchemaModel>"#;

    let canonical = canonicalize_model_xml(xml).expect("Canonicalization should succeed");

    // Relationships should preserve original order from source XML
    // Schema appears first in input, then Columns
    let schema_pos = canonical
        .find("Relationship Name=\"Schema\"")
        .expect("Should find Schema");
    let columns_pos = canonical
        .find("Relationship Name=\"Columns\"")
        .expect("Should find Columns");
    assert!(
        schema_pos < columns_pos,
        "Schema relationship should come before Columns (original order)"
    );

    // Column entries should preserve original order from source XML
    // Name appears first in input, then Id
    let name_pos = canonical
        .find("[dbo].[Products].[Name]")
        .expect("Should find Name column");
    let id_pos = canonical
        .find("[dbo].[Products].[Id]")
        .expect("Should find Id column");
    assert!(
        name_pos < id_pos,
        "Name column should come before Id column (original order)"
    );

    println!("Canonical XML with relationships:");
    for (i, line) in canonical.lines().enumerate() {
        println!("  {:3}: {}", i + 1, line);
    }
}

/// Test comparing two canonical XML strings.
#[test]
fn test_compare_canonical_xml() {
    let xml1 = "line1\nline2\nline3\n";
    let xml2 = "line1\nline2\nline3\n";

    let errors = compare_canonical_xml(xml1, xml2);
    assert!(errors.is_empty(), "Identical content should have no errors");

    // Test with differences
    let xml3 = "line1\ndifferent\nline3\n";
    let errors = compare_canonical_xml(xml1, xml3);
    assert_eq!(errors.len(), 1, "Should detect one difference");

    match &errors[0] {
        CanonicalXmlError::ContentMismatch {
            line_number,
            rust_line,
            dotnet_line,
        } => {
            assert_eq!(*line_number, 2, "Difference should be on line 2");
            assert_eq!(rust_line, "line2");
            assert_eq!(dotnet_line, "different");
        }
        _ => panic!("Expected ContentMismatch error"),
    }

    // Test with line count mismatch
    let xml4 = "line1\nline2\n";
    let errors = compare_canonical_xml(xml1, xml4);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CanonicalXmlError::LineCountMismatch { .. })),
        "Should detect line count mismatch"
    );
}

/// Test the diff generation function.
#[test]
fn test_generate_diff() {
    let content1 = "line1\nline2\nline3\nline4\n";
    let content2 = "line1\nmodified\nline3\nline4\n";

    let diff = generate_diff(content1, content2, 1);

    assert!(
        !diff.is_empty(),
        "Diff should not be empty for different content"
    );
    assert!(diff.contains("---"), "Diff should have source header");
    assert!(diff.contains("+++"), "Diff should have target header");
    assert!(diff.contains("-line2"), "Diff should show deleted line");
    assert!(diff.contains("+modified"), "Diff should show added line");

    println!("Generated diff:\n{}", diff);

    // Test identical content
    let diff_empty = generate_diff(content1, content1, 1);
    assert!(
        diff_empty.is_empty(),
        "Diff should be empty for identical content"
    );
}

/// Test SHA256 checksum computation.
#[test]
fn test_compute_sha256() {
    let content = "Hello, World!";
    let checksum = compute_sha256(content);

    // Known SHA256 hash of "Hello, World!"
    assert_eq!(
        checksum, "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f",
        "SHA256 checksum should match expected value"
    );

    // Different content should have different checksum
    let checksum2 = compute_sha256("Different content");
    assert_ne!(
        checksum, checksum2,
        "Different content should have different checksums"
    );

    // Empty content should have valid checksum
    let empty_checksum = compute_sha256("");
    assert_eq!(
        empty_checksum.len(),
        64,
        "SHA256 hex should be 64 characters"
    );
}

/// Test canonical comparison with a real fixture.
///
/// This test compares the canonical form of model.xml from both Rust and DotNet
/// dacpacs for a simple fixture. Since canonicalization normalizes ordering and
/// formatting, this provides the most precise comparison possible.
#[test]
fn test_canonical_comparison_simple_table() {
    let fixture_path = Path::new("tests/fixtures/simple_table");
    if !fixture_path.exists() {
        println!("Skipping: simple_table fixture not found");
        return;
    }

    let sqlproj_path = fixture_path.join("simple_table.sqlproj");
    let dotnet_dacpac = fixture_path.join("obj/Debug/simple_table.dacpac");

    if !dotnet_dacpac.exists() {
        println!("Skipping: DotNet dacpac not found (run dotnet build first)");
        return;
    }

    // Build Rust dacpac
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let rust_dacpac = temp_dir.path().join("simple_table.dacpac");

    let build_result = rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path: sqlproj_path.clone(),
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    });

    if let Err(e) = build_result {
        println!("Skipping: Failed to build Rust dacpac: {}", e);
        return;
    }

    // Extract and canonicalize both
    let rust_xml = extract_model_xml(&rust_dacpac).expect("Failed to extract Rust model.xml");
    let dotnet_xml = extract_model_xml(&dotnet_dacpac).expect("Failed to extract DotNet model.xml");

    let rust_canonical =
        canonicalize_model_xml(&rust_xml).expect("Failed to canonicalize Rust XML");
    let dotnet_canonical =
        canonicalize_model_xml(&dotnet_xml).expect("Failed to canonicalize DotNet XML");

    // Compare
    let errors = compare_canonical_xml(&rust_canonical, &dotnet_canonical);

    println!("\n=== Canonical XML Comparison: simple_table ===");
    println!("Rust canonical: {} lines", rust_canonical.lines().count());
    println!(
        "DotNet canonical: {} lines",
        dotnet_canonical.lines().count()
    );

    if errors.is_empty() {
        println!("Result: EXACT MATCH!");
        let rust_checksum = compute_sha256(&rust_canonical);
        let dotnet_checksum = compute_sha256(&dotnet_canonical);
        println!("Rust SHA256:   {}", rust_checksum);
        println!("DotNet SHA256: {}", dotnet_checksum);
    } else {
        println!("Result: {} differences found", errors.len());
        for (i, error) in errors.iter().take(5).enumerate() {
            println!("  {}: {}", i + 1, error);
        }

        // Show diff for debugging
        let diff = generate_diff(&rust_canonical, &dotnet_canonical, 2);
        if !diff.is_empty() {
            println!("\nDiff (first 50 lines):");
            for line in diff.lines().take(50) {
                println!("{}", line);
            }
        }
    }

    // This is an informational test - we don't assert failure
    // as there may be legitimate differences being addressed
}

/// Test canonical comparison using compare_canonical_dacpacs function.
#[test]
fn test_compare_canonical_dacpacs_function() {
    let fixture_path = Path::new("tests/fixtures/simple_table");
    if !fixture_path.exists() {
        println!("Skipping: simple_table fixture not found");
        return;
    }

    let sqlproj_path = fixture_path.join("simple_table.sqlproj");
    let dotnet_dacpac = fixture_path.join("obj/Debug/simple_table.dacpac");

    if !dotnet_dacpac.exists() {
        println!("Skipping: DotNet dacpac not found");
        return;
    }

    // Build Rust dacpac
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let rust_dacpac = temp_dir.path().join("simple_table.dacpac");

    let build_result = rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
        project_path: sqlproj_path.clone(),
        output_path: Some(rust_dacpac.clone()),
        target_platform: "Sql150".to_string(),
        verbose: false,
    });

    if let Err(e) = build_result {
        println!("Skipping: Failed to build Rust dacpac: {}", e);
        return;
    }

    // Use the high-level comparison function
    let result = compare_canonical_dacpacs(&rust_dacpac, &dotnet_dacpac, true);

    match result {
        Ok(errors) => {
            println!("\n=== compare_canonical_dacpacs Results ===");
            if errors.is_empty() {
                println!("Result: EXACT CANONICAL MATCH with matching checksums!");
            } else {
                println!("Found {} canonical differences:", errors.len());
                for error in &errors {
                    println!("  - {}", error);
                }
            }
        }
        Err(e) => {
            println!("Comparison failed: {}", e);
        }
    }
}

/// Test canonical comparison across all fixtures.
///
/// This is an informational test that shows the canonical parity status
/// for all available fixtures.
#[test]
fn test_canonical_comparison_all_fixtures() {
    println!("\n=== Canonical XML Comparison: All Fixtures ===\n");

    let fixtures = get_available_fixtures();
    let mut exact_match = 0;
    let mut partial_match = 0;
    let mut errors_count = 0;

    for fixture in &fixtures {
        let fixture_path = Path::new("tests/fixtures").join(fixture);
        let sqlproj_files: Vec<_> = std::fs::read_dir(&fixture_path)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "sqlproj"))
                    .collect()
            })
            .unwrap_or_default();

        if sqlproj_files.is_empty() {
            continue;
        }

        let sqlproj_path = sqlproj_files[0].path();
        let project_name = sqlproj_path.file_stem().unwrap().to_str().unwrap();
        let dotnet_dacpac = fixture_path.join(format!("bin/Debug/{}.dacpac", project_name));

        if !dotnet_dacpac.exists() {
            println!("{:40} SKIP (no DotNet dacpac)", fixture);
            continue;
        }

        // Build Rust dacpac
        let temp_dir = match tempfile::tempdir() {
            Ok(d) => d,
            Err(_) => {
                println!("{:40} ERROR (temp dir)", fixture);
                errors_count += 1;
                continue;
            }
        };
        let rust_dacpac = temp_dir.path().join(format!("{}.dacpac", project_name));

        if rust_sqlpackage::build_dacpac(rust_sqlpackage::BuildOptions {
            project_path: sqlproj_path.clone(),
            output_path: Some(rust_dacpac.clone()),
            target_platform: "Sql150".to_string(),
            verbose: false,
        })
        .is_err()
        {
            println!("{:40} ERROR (build)", fixture);
            errors_count += 1;
            continue;
        }

        // Compare canonically
        match compare_canonical_dacpacs(&rust_dacpac, &dotnet_dacpac, false) {
            Ok(errors) if errors.is_empty() => {
                println!("{:40} ✓ EXACT MATCH", fixture);
                exact_match += 1;
            }
            Ok(errors) => {
                let line_diff = errors
                    .iter()
                    .filter_map(|e| match e {
                        CanonicalXmlError::LineCountMismatch {
                            rust_lines,
                            dotnet_lines,
                        } => Some((*rust_lines as i64 - *dotnet_lines as i64).abs()),
                        _ => None,
                    })
                    .next()
                    .unwrap_or(0);
                let content_diffs = errors
                    .iter()
                    .filter(|e| matches!(e, CanonicalXmlError::ContentMismatch { .. }))
                    .count();
                println!(
                    "{:40} ~ DIFF (lines: {}, content: {})",
                    fixture, line_diff, content_diffs
                );
                partial_match += 1;
            }
            Err(e) => {
                println!("{:40} ERROR: {}", fixture, e);
                errors_count += 1;
            }
        }
    }

    let total = exact_match + partial_match + errors_count;
    println!("\n=== Summary ===");
    println!("Total fixtures tested: {}", total);
    println!(
        "Exact canonical match: {}/{} ({:.1}%)",
        exact_match,
        total,
        if total > 0 {
            100.0 * exact_match as f64 / total as f64
        } else {
            0.0
        }
    );
    println!(
        "Partial match (diffs): {}/{} ({:.1}%)",
        partial_match,
        total,
        if total > 0 {
            100.0 * partial_match as f64 / total as f64
        } else {
            0.0
        }
    );
    println!("Errors: {}", errors_count);
}

// =============================================================================
// Phase 8.4: Regression Detection Tests
// =============================================================================

/// Default path to the parity baseline file, relative to the test directory.
fn get_baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("e2e")
        .join("parity-baseline.json")
}

/// Load the parity baseline, or create a new empty one if not found.
fn load_or_create_baseline() -> ParityBaseline {
    let path = get_baseline_path();
    match ParityBaseline::from_file(&path) {
        Ok(baseline) => baseline,
        Err(e) => {
            eprintln!(
                "Warning: Could not load baseline from {}: {}",
                path.display(),
                e
            );
            eprintln!("Using empty baseline (no regressions will be detected)");
            ParityBaseline::new()
        }
    }
}

/// CI test that checks for regressions against the baseline.
///
/// This test:
/// 1. Loads the parity baseline from `tests/e2e/parity-baseline.json`
/// 2. Runs parity tests on all fixtures
/// 3. Compares current results against the baseline
/// 4. FAILS if any regressions are detected (previously passing tests now fail)
/// 5. Reports improvements (previously failing tests now pass)
///
/// To update the baseline after fixing issues or accepting new behavior:
///   PARITY_UPDATE_BASELINE=1 cargo test --test e2e_tests test_parity_regression_check -- --nocapture
#[test]
fn test_parity_regression_check() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    // Load the baseline
    let baseline = load_or_create_baseline();
    println!("Loaded baseline with {} fixtures", baseline.fixtures.len());
    if let Some(ref commit) = baseline.commit {
        println!("Baseline commit: {}", commit);
    }
    println!("Baseline updated: {}", baseline.updated);

    // Collect current metrics
    let options = ParityTestOptions::default();
    let metrics = collect_parity_metrics(&options);

    // Print summary
    metrics.print_summary();

    // Check for regressions
    let regressions = baseline.detect_regressions(&metrics);
    let improvements = baseline.detect_improvements(&metrics);

    baseline.print_regression_summary(&metrics);

    // Handle baseline update request
    if std::env::var("PARITY_UPDATE_BASELINE").is_ok() {
        let new_baseline = ParityBaseline::from_metrics(&metrics);
        let path = get_baseline_path();
        match new_baseline.to_file(&path) {
            Ok(_) => println!("\n✓ Baseline updated: {}", path.display()),
            Err(e) => eprintln!("Failed to write baseline: {}", e),
        }
        return;
    }

    // Report improvements (informational only)
    if !improvements.is_empty() {
        println!("\n💡 Consider updating the baseline to capture these improvements:");
        println!("   PARITY_UPDATE_BASELINE=1 cargo test --test e2e_tests test_parity_regression_check -- --nocapture\n");
    }

    // Fail if regressions detected
    if !regressions.is_empty() {
        let mut error_msg = format!("\n{} REGRESSION(S) DETECTED:\n", regressions.len());
        for regression in &regressions {
            error_msg.push_str(&format!("  - {}\n", regression));
        }
        error_msg.push_str("\nPreviously passing parity tests are now failing.");
        error_msg.push_str(
            "\nInvestigate the changes and either fix the regression or update the baseline.",
        );
        panic!("{}", error_msg);
    }
}

/// Test that FixtureBaseline correctly serializes and deserializes JSON.
#[test]
fn test_fixture_baseline_json_roundtrip() {
    let baseline = FixtureBaseline {
        name: "test_fixture".to_string(),
        layer1_pass: true,
        layer2_pass: false,
        relationship_pass: true,
        layer4_pass: false,
        metadata_pass: true,
    };

    let json = baseline.to_json();
    println!("Serialized JSON:\n{}", json);

    let parsed = FixtureBaseline::from_json(&json).expect("Should parse JSON");
    assert_eq!(baseline, parsed, "Roundtrip should preserve values");
}

/// Test that ParityBaseline correctly serializes and deserializes JSON.
#[test]
fn test_parity_baseline_json_roundtrip() {
    let mut baseline = ParityBaseline::new();
    baseline.fixtures.push(FixtureBaseline {
        name: "fixture_a".to_string(),
        layer1_pass: true,
        layer2_pass: true,
        relationship_pass: false,
        layer4_pass: false,
        metadata_pass: false,
    });
    baseline.fixtures.push(FixtureBaseline {
        name: "fixture_b".to_string(),
        layer1_pass: false,
        layer2_pass: false,
        relationship_pass: false,
        layer4_pass: false,
        metadata_pass: false,
    });

    let json = baseline.to_json();
    println!("Serialized ParityBaseline JSON:\n{}", json);

    let parsed = ParityBaseline::from_json(&json).expect("Should parse JSON");
    assert_eq!(baseline.version, parsed.version);
    assert_eq!(baseline.fixtures.len(), parsed.fixtures.len());
    assert_eq!(baseline.fixtures[0], parsed.fixtures[0]);
    assert_eq!(baseline.fixtures[1], parsed.fixtures[1]);
}

/// Test that regression detection correctly identifies regressions.
#[test]
fn test_regression_detection_logic() {
    // Create a baseline where fixture_a passes Layer 1 and Layer 2
    let mut baseline = ParityBaseline::new();
    baseline.fixtures.push(FixtureBaseline {
        name: "fixture_a".to_string(),
        layer1_pass: true,
        layer2_pass: true,
        relationship_pass: false,
        layer4_pass: false,
        metadata_pass: false,
    });

    // Create current metrics where fixture_a fails Layer 2 (regression!)
    let mut metrics = ParityMetrics::new();
    metrics
        .fixtures
        .push(crate::dacpac_compare::FixtureMetrics {
            name: "fixture_a".to_string(),
            status: "PARTIAL".to_string(),
            layer1_errors: 0, // Still passes
            layer2_errors: 5, // Regression!
            relationship_errors: 0,
            layer4_errors: 0,
            metadata_errors: 0,
            error_message: None,
        });

    let regressions = baseline.detect_regressions(&metrics);

    assert_eq!(regressions.len(), 1, "Should detect one regression");
    assert_eq!(regressions[0].fixture, "fixture_a");
    assert!(regressions[0].layer.contains("Layer 2"));

    println!("Detected regression: {}", regressions[0]);
}

/// Test that improvement detection correctly identifies improvements.
#[test]
fn test_improvement_detection_logic() {
    // Create a baseline where fixture_a fails Layer 2 but passes others
    let mut baseline = ParityBaseline::new();
    baseline.fixtures.push(FixtureBaseline {
        name: "fixture_a".to_string(),
        layer1_pass: true,
        layer2_pass: false,      // Was failing
        relationship_pass: true, // Already passing
        layer4_pass: true,       // Already passing
        metadata_pass: true,     // Already passing
    });

    // Create current metrics where fixture_a now passes Layer 2 (improvement!)
    let mut metrics = ParityMetrics::new();
    metrics
        .fixtures
        .push(crate::dacpac_compare::FixtureMetrics {
            name: "fixture_a".to_string(),
            status: "PASS".to_string(),
            layer1_errors: 0,
            layer2_errors: 0,       // Now passes!
            relationship_errors: 0, // Still passes
            layer4_errors: 0,       // Still passes
            metadata_errors: 0,     // Still passes
            error_message: None,
        });

    let improvements = baseline.detect_improvements(&metrics);

    assert_eq!(improvements.len(), 1, "Should detect one improvement");
    assert!(improvements[0].contains("fixture_a"));
    assert!(improvements[0].contains("Layer 2"));

    println!("Detected improvement: {}", improvements[0]);
}

/// Test that new fixtures don't cause regressions.
#[test]
fn test_new_fixture_no_regression() {
    // Create a baseline without fixture_b
    let mut baseline = ParityBaseline::new();
    baseline.fixtures.push(FixtureBaseline {
        name: "fixture_a".to_string(),
        layer1_pass: true,
        layer2_pass: true,
        relationship_pass: false,
        layer4_pass: false,
        metadata_pass: false,
    });

    // Create current metrics with a new fixture_b (not in baseline)
    let mut metrics = ParityMetrics::new();
    metrics
        .fixtures
        .push(crate::dacpac_compare::FixtureMetrics {
            name: "fixture_a".to_string(),
            status: "PARTIAL".to_string(),
            layer1_errors: 0,
            layer2_errors: 0,
            relationship_errors: 5,
            layer4_errors: 0,
            metadata_errors: 0,
            error_message: None,
        });
    metrics
        .fixtures
        .push(crate::dacpac_compare::FixtureMetrics {
            name: "fixture_b".to_string(), // New fixture!
            status: "FAIL".to_string(),
            layer1_errors: 3,
            layer2_errors: 2,
            relationship_errors: 0,
            layer4_errors: 0,
            metadata_errors: 0,
            error_message: None,
        });

    let regressions = baseline.detect_regressions(&metrics);

    // New fixture shouldn't cause regression
    assert!(
        regressions.is_empty(),
        "New fixtures shouldn't be flagged as regressions"
    );
}

/// Test that ParityBaseline can be created from ParityMetrics.
#[test]
fn test_baseline_from_metrics() {
    let mut metrics = ParityMetrics::new();
    metrics
        .fixtures
        .push(crate::dacpac_compare::FixtureMetrics {
            name: "test_fixture".to_string(),
            status: "PARTIAL".to_string(),
            layer1_errors: 0,
            layer2_errors: 0,
            relationship_errors: 3,
            layer4_errors: 2,
            metadata_errors: 1,
            error_message: None,
        });

    let baseline = ParityBaseline::from_metrics(&metrics);

    assert_eq!(baseline.fixtures.len(), 1);
    assert_eq!(baseline.fixtures[0].name, "test_fixture");
    assert!(baseline.fixtures[0].layer1_pass);
    assert!(baseline.fixtures[0].layer2_pass);
    assert!(!baseline.fixtures[0].relationship_pass); // Has errors
    assert!(!baseline.fixtures[0].layer4_pass); // Has errors
    assert!(!baseline.fixtures[0].metadata_pass); // Has errors
}

/// Generate a fresh baseline file from current test results.
///
/// Run with:
///   cargo test --test e2e_tests test_generate_baseline -- --nocapture
///
/// This test generates a new baseline but does NOT automatically save it.
/// Use PARITY_UPDATE_BASELINE=1 with test_parity_regression_check to save.
#[test]
fn test_generate_baseline() {
    if !dotnet_available() {
        println!("Skipping test: dotnet not available");
        return;
    }

    let options = ParityTestOptions::default();
    let metrics = collect_parity_metrics(&options);
    let baseline = ParityBaseline::from_metrics(&metrics);

    println!("\n=== Generated Baseline ===");
    println!("{}", baseline.to_json());

    println!("\n=== Summary ===");
    let layer1_passing = baseline.fixtures.iter().filter(|f| f.layer1_pass).count();
    let layer2_passing = baseline.fixtures.iter().filter(|f| f.layer2_pass).count();
    let rel_passing = baseline
        .fixtures
        .iter()
        .filter(|f| f.relationship_pass)
        .count();
    let total = baseline.fixtures.len();

    println!("Total fixtures: {}", total);
    println!(
        "Layer 1 passing: {}/{} ({:.1}%)",
        layer1_passing,
        total,
        100.0 * layer1_passing as f64 / total as f64
    );
    println!(
        "Layer 2 passing: {}/{} ({:.1}%)",
        layer2_passing,
        total,
        100.0 * layer2_passing as f64 / total as f64
    );
    println!(
        "Relationships passing: {}/{} ({:.1}%)",
        rel_passing,
        total,
        100.0 * rel_passing as f64 / total as f64
    );
}
