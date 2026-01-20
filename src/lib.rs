//! rust-sqlpackage: A fast Rust compiler for SQL Server database projects
//!
//! This library compiles .sqlproj files into .dacpac packages,
//! providing a faster alternative to the .NET DacFx toolchain.

pub mod dacpac;
pub mod error;
pub mod model;
pub mod parser;
pub mod project;

use std::path::PathBuf;

use anyhow::Result;

pub use error::SqlPackageError;

/// Options for building a dacpac
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// Path to the .sqlproj file
    pub project_path: PathBuf,
    /// Output path for the .dacpac file
    pub output_path: Option<PathBuf>,
    /// Target SQL Server platform (e.g., "Sql160")
    pub target_platform: String,
    /// Enable verbose output
    pub verbose: bool,
}

/// Build a dacpac from a sqlproj file
pub fn build_dacpac(options: BuildOptions) -> Result<PathBuf> {
    if options.verbose {
        println!("Building project: {}", options.project_path.display());
    }

    // Step 1: Parse the sqlproj file
    let project = project::parse_sqlproj(&options.project_path)?;

    if options.verbose {
        println!("Found {} SQL files", project.sql_files.len());
    }

    // Step 2: Parse all SQL files
    let statements = parser::parse_sql_files(&project.sql_files)?;

    if options.verbose {
        println!("Parsed {} SQL statements", statements.len());
    }

    // Step 3: Build the database model
    let database_model = model::build_model(&statements, &project)?;

    if options.verbose {
        println!(
            "Built model with {} elements",
            database_model.elements.len()
        );
    }

    // Step 4: Determine output path
    let output_path = options.output_path.unwrap_or_else(|| {
        let project_dir = options
            .project_path
            .parent()
            .unwrap_or(std::path::Path::new("."));
        let project_name = options
            .project_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        project_dir
            .join("bin")
            .join("Debug")
            .join(format!("{}.dacpac", project_name))
    });

    // Step 5: Generate the dacpac
    dacpac::create_dacpac(&database_model, &project, &output_path)?;

    if options.verbose {
        println!("Created dacpac: {}", output_path.display());
    }

    Ok(output_path)
}
