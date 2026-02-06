use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

use rust_sqlpackage::{build_dacpac, BuildOptions};

#[derive(Parser)]
#[command(name = "rust-sqlpackage")]
#[command(
    author,
    version,
    about = "Fast Rust compiler for SQL Server database projects"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build a .sqlproj file into a .dacpac package
    Build {
        /// Path to the .sqlproj file
        #[arg(short, long)]
        project: PathBuf,

        /// Output path for the .dacpac file (defaults to bin/Debug/<project>.dacpac)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Target SQL Server platform (Sql130, Sql140, Sql150, Sql160)
        #[arg(short, long, default_value = "Sql160")]
        target_platform: String,

        /// Enable verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Compare two dacpac files and report differences
    Compare {
        /// Path to the rust-generated dacpac
        rust_dacpac: PathBuf,

        /// Path to the dotnet-generated dacpac
        dotnet_dacpac: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            project,
            output,
            target_platform,
            verbose,
        } => {
            let options = BuildOptions {
                project_path: project,
                output_path: output,
                target_platform,
                verbose,
            };

            build_dacpac(options)?;
        }

        Commands::Compare {
            rust_dacpac,
            dotnet_dacpac,
        } => {
            let result = rust_sqlpackage::compare::compare_dacpacs(&rust_dacpac, &dotnet_dacpac)?;

            // Print duplicate warnings to stderr
            for (source, keys) in &result.duplicate_warnings {
                eprintln!(
                    "WARNING: {} duplicate keys in {} model.xml",
                    keys.len(),
                    source
                );
                for key in keys.iter().take(5) {
                    eprintln!("  {}", key);
                }
            }

            rust_sqlpackage::compare::report::print_report(&result);

            if result.has_differences() {
                process::exit(1);
            }
        }
    }

    Ok(())
}
