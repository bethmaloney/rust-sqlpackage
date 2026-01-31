//! Pipeline benchmarks for rust-sqlpackage
//!
//! This benchmark module provides comprehensive performance measurements for:
//! - Full pipeline: sqlproj -> dacpac
//! - SQL project parsing
//! - SQL file parsing
//! - Model building
//! - XML generation
//!
//! Run with: cargo bench
//! Compare against baseline: cargo bench -- --save-baseline before
//!                          (make changes)
//!                          cargo bench -- --baseline before

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rust_sqlpackage::{dacpac, model, parser, project, BuildOptions};
use std::path::PathBuf;
use tempfile::TempDir;

/// Get the path to a test fixture
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Benchmark the full pipeline: sqlproj -> dacpac
fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline");

    // Test with e2e_comprehensive fixture (30 files)
    let project_path = fixture_path("e2e_comprehensive").join("project.sqlproj");
    if project_path.exists() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output.dacpac");

        group.bench_function("e2e_comprehensive", |b| {
            b.iter(|| {
                let options = BuildOptions {
                    project_path: black_box(project_path.clone()),
                    output_path: Some(output_path.clone()),
                    target_platform: "Sql160".to_string(),
                    verbose: false,
                };
                rust_sqlpackage::build_dacpac(options).unwrap()
            })
        });
    }

    // Test with e2e_simple fixture (minimal project)
    let simple_project_path = fixture_path("e2e_simple").join("project.sqlproj");
    if simple_project_path.exists() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output.dacpac");

        group.bench_function("e2e_simple", |b| {
            b.iter(|| {
                let options = BuildOptions {
                    project_path: black_box(simple_project_path.clone()),
                    output_path: Some(output_path.clone()),
                    target_platform: "Sql160".to_string(),
                    verbose: false,
                };
                rust_sqlpackage::build_dacpac(options).unwrap()
            })
        });
    }

    // Test with stress_test fixture (135 files) for high-volume benchmarking
    let stress_project_path = fixture_path("stress_test").join("project.sqlproj");
    if stress_project_path.exists() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output.dacpac");

        group.bench_function("stress_test", |b| {
            b.iter(|| {
                let options = BuildOptions {
                    project_path: black_box(stress_project_path.clone()),
                    output_path: Some(output_path.clone()),
                    target_platform: "Sql160".to_string(),
                    verbose: false,
                };
                rust_sqlpackage::build_dacpac(options).unwrap()
            })
        });
    }

    group.finish();
}

/// Benchmark SQL project parsing
fn bench_sqlproj_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlproj_parsing");

    let project_path = fixture_path("e2e_comprehensive").join("project.sqlproj");
    if project_path.exists() {
        group.bench_function("e2e_comprehensive", |b| {
            b.iter(|| project::parse_sqlproj(black_box(&project_path)).unwrap())
        });
    }

    group.finish();
}

/// Benchmark SQL file parsing
fn bench_sql_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("sql_parsing");

    // Parse SQL files from e2e_comprehensive
    let project_path = fixture_path("e2e_comprehensive").join("project.sqlproj");
    if project_path.exists() {
        let project = project::parse_sqlproj(&project_path).unwrap();

        group.bench_function(
            BenchmarkId::new("e2e_comprehensive", project.sql_files.len()),
            |b| b.iter(|| parser::parse_sql_files(black_box(&project.sql_files)).unwrap()),
        );
    }

    // Parse SQL files from stress_test (135 files)
    let stress_project_path = fixture_path("stress_test").join("project.sqlproj");
    if stress_project_path.exists() {
        let project = project::parse_sqlproj(&stress_project_path).unwrap();

        group.bench_function(
            BenchmarkId::new("stress_test", project.sql_files.len()),
            |b| b.iter(|| parser::parse_sql_files(black_box(&project.sql_files)).unwrap()),
        );
    }

    group.finish();
}

/// Benchmark model building
fn bench_model_building(c: &mut Criterion) {
    let mut group = c.benchmark_group("model_building");

    let project_path = fixture_path("e2e_comprehensive").join("project.sqlproj");
    if project_path.exists() {
        let project = project::parse_sqlproj(&project_path).unwrap();
        let statements = parser::parse_sql_files(&project.sql_files).unwrap();

        group.bench_function(
            BenchmarkId::new("e2e_comprehensive", statements.len()),
            |b| b.iter(|| model::build_model(black_box(&statements), black_box(&project)).unwrap()),
        );
    }

    // Model building for stress_test (135 files)
    let stress_project_path = fixture_path("stress_test").join("project.sqlproj");
    if stress_project_path.exists() {
        let project = project::parse_sqlproj(&stress_project_path).unwrap();
        let statements = parser::parse_sql_files(&project.sql_files).unwrap();

        group.bench_function(BenchmarkId::new("stress_test", statements.len()), |b| {
            b.iter(|| model::build_model(black_box(&statements), black_box(&project)).unwrap())
        });
    }

    group.finish();
}

/// Benchmark XML generation
fn bench_xml_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("xml_generation");

    let project_path = fixture_path("e2e_comprehensive").join("project.sqlproj");
    if project_path.exists() {
        let project = project::parse_sqlproj(&project_path).unwrap();
        let statements = parser::parse_sql_files(&project.sql_files).unwrap();
        let database_model = model::build_model(&statements, &project).unwrap();

        // Set throughput based on number of model elements
        group.throughput(Throughput::Elements(database_model.elements.len() as u64));

        group.bench_function(
            BenchmarkId::new("model_xml", database_model.elements.len()),
            |b| {
                b.iter(|| {
                    let mut buffer = Vec::new();
                    dacpac::generate_model_xml(
                        &mut buffer,
                        black_box(&database_model),
                        black_box(&project),
                    )
                    .unwrap();
                    buffer
                })
            },
        );

        group.bench_function("metadata_xml", |b| {
            b.iter(|| {
                let mut buffer = Vec::new();
                dacpac::generate_metadata_xml(&mut buffer, black_box(&project), "1.0.0.0").unwrap();
                buffer
            })
        });
    }

    group.finish();
}

/// Benchmark dacpac packaging (ZIP creation)
fn bench_dacpac_packaging(c: &mut Criterion) {
    let mut group = c.benchmark_group("dacpac_packaging");

    let project_path = fixture_path("e2e_comprehensive").join("project.sqlproj");
    if project_path.exists() {
        let project = project::parse_sqlproj(&project_path).unwrap();
        let statements = parser::parse_sql_files(&project.sql_files).unwrap();
        let database_model = model::build_model(&statements, &project).unwrap();

        group.bench_function("create_dacpac", |b| {
            let temp_dir = TempDir::new().unwrap();
            let output_path = temp_dir.path().join("output.dacpac");

            b.iter(|| {
                dacpac::create_dacpac(
                    black_box(&database_model),
                    black_box(&project),
                    black_box(&output_path),
                )
                .unwrap()
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_full_pipeline,
    bench_sqlproj_parsing,
    bench_sql_parsing,
    bench_model_building,
    bench_xml_generation,
    bench_dacpac_packaging,
);

criterion_main!(benches);
