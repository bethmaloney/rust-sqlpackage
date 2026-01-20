//! Error types for rust-sqlpackage

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during sqlproj compilation
#[derive(Error, Debug)]
pub enum SqlPackageError {
    #[error("Failed to read project file: {path}")]
    ProjectReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse project file: {path}")]
    ProjectParseError {
        path: PathBuf,
        #[source]
        source: roxmltree::Error,
    },

    #[error("Invalid project file format: {message}")]
    InvalidProjectFormat { message: String },

    #[error("Failed to read SQL file: {path}")]
    SqlFileReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("SQL parse error in {path} at line {line}: {message}")]
    SqlParseError {
        path: PathBuf,
        line: usize,
        message: String,
    },

    #[error("Unsupported SQL statement: {statement_type}")]
    UnsupportedStatement { statement_type: String },

    #[error("Failed to create dacpac: {message}")]
    DacpacCreationError { message: String },

    #[error("Failed to write dacpac to {path}")]
    DacpacWriteError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("XML generation error: {message}")]
    XmlGenerationError { message: String },

    #[error("ZIP creation error: {message}")]
    ZipError { message: String },

    #[error("SQLCMD include file not found: {path} (referenced from {source_file})")]
    SqlcmdIncludeNotFound { path: PathBuf, source_file: PathBuf },

    #[error("Circular SQLCMD include detected: {path} (include chain: {chain})")]
    SqlcmdCircularInclude { path: PathBuf, chain: String },
}

impl From<zip::result::ZipError> for SqlPackageError {
    fn from(err: zip::result::ZipError) -> Self {
        SqlPackageError::ZipError {
            message: err.to_string(),
        }
    }
}
