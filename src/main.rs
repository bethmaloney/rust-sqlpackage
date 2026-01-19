use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use rust_sqlpackage::{build_dacpac, BuildOptions};

#[derive(Parser)]
#[command(name = "rust-sqlpackage")]
#[command(author, version, about = "Fast Rust compiler for SQL Server database projects")]
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
    }

    Ok(())
}
