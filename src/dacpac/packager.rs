//! Create dacpac ZIP package

use std::fs::File;
use std::io::{Cursor, Write};
use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::error::SqlPackageError;
use crate::model::DatabaseModel;
use crate::parser::expand_includes;
use crate::project::SqlProject;

use super::{metadata_xml, model_xml, origin_xml};

/// Create a dacpac file from the database model
pub fn create_dacpac(
    model: &DatabaseModel,
    project: &SqlProject,
    output_path: &Path,
) -> Result<()> {
    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SqlPackageError::DacpacWriteError {
            path: output_path.to_path_buf(),
            source: e,
        })?;
    }

    let file = File::create(output_path).map_err(|e| SqlPackageError::DacpacWriteError {
        path: output_path.to_path_buf(),
        source: e,
    })?;

    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(1));

    // Write model.xml
    let mut model_buffer = Cursor::new(Vec::with_capacity(model.elements.len() * 2000));
    model_xml::generate_model_xml(&mut model_buffer, model, project)?;
    zip.start_file("model.xml", options)?;
    zip.write_all(model_buffer.get_ref())?;

    // Write DacMetadata.xml
    let mut metadata_buffer = Cursor::new(Vec::with_capacity(4096));
    metadata_xml::generate_metadata_xml(&mut metadata_buffer, project, &project.dac_version)?;
    zip.start_file("DacMetadata.xml", options)?;
    zip.write_all(metadata_buffer.get_ref())?;

    // Compute SHA256 checksum of model.xml for Origin.xml
    let mut hasher = Sha256::new();
    hasher.update(model_buffer.get_ref());
    let model_checksum = format!("{:X}", hasher.finalize());

    // Write Origin.xml
    let mut origin_buffer = Cursor::new(Vec::with_capacity(4096));
    origin_xml::generate_origin_xml(&mut origin_buffer, &model_checksum)?;
    zip.start_file("Origin.xml", options)?;
    zip.write_all(origin_buffer.get_ref())?;

    // Write [Content_Types].xml (required for package format)
    let has_deploy_scripts =
        project.pre_deploy_script.is_some() || project.post_deploy_script.is_some();
    let content_types = generate_content_types_xml(has_deploy_scripts);
    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(content_types.as_bytes())?;

    // Write predeploy.sql (if present)
    // Expands SQLCMD :r include directives to inline referenced files
    // DotNet ensures deploy scripts end with a GO statement
    if let Some(pre_deploy_path) = &project.pre_deploy_script {
        let content = std::fs::read_to_string(pre_deploy_path).map_err(|e| {
            SqlPackageError::SqlFileReadError {
                path: pre_deploy_path.clone(),
                source: e,
            }
        })?;
        let expanded = expand_includes(&content, pre_deploy_path)?;
        let normalized = ensure_trailing_go(&expanded);
        zip.start_file("predeploy.sql", options)?;
        zip.write_all(normalized.as_bytes())?;
    }

    // Write postdeploy.sql (if present)
    // Expands SQLCMD :r include directives to inline referenced files
    // DotNet ensures deploy scripts end with a GO statement
    if let Some(post_deploy_path) = &project.post_deploy_script {
        let content = std::fs::read_to_string(post_deploy_path).map_err(|e| {
            SqlPackageError::SqlFileReadError {
                path: post_deploy_path.clone(),
                source: e,
            }
        })?;
        let expanded = expand_includes(&content, post_deploy_path)?;
        let normalized = ensure_trailing_go(&expanded);
        zip.start_file("postdeploy.sql", options)?;
        zip.write_all(normalized.as_bytes())?;
    }

    zip.finish()?;

    Ok(())
}

pub(crate) fn generate_content_types_xml(include_sql: bool) -> String {
    if include_sql {
        r#"<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="text/xml" />
  <Default Extension="sql" ContentType="text/plain" />
</Types>"#
            .to_string()
    } else {
        r#"<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="text/xml" />
</Types>"#
            .to_string()
    }
}

/// Ensure deploy script content ends with a GO statement (matches DotNet behavior).
///
/// DotNet dacpacs always end deploy scripts with a trailing GO statement.
/// This function normalizes the content by:
/// 1. Normalizing line endings (CRLF -> LF)
/// 2. Trimming trailing whitespace
/// 3. Appending a GO statement if one is not already present
fn ensure_trailing_go(content: &str) -> String {
    // Normalize CRLF to LF
    let content = content.replace("\r\n", "\n");

    // Trim trailing whitespace
    let trimmed = content.trim_end();

    // Check if it already ends with GO (case-insensitive, possibly followed by whitespace/newline)
    let lines: Vec<&str> = trimmed.lines().collect();
    let ends_with_go = lines
        .last()
        .map(|line| line.trim().eq_ignore_ascii_case("GO"))
        .unwrap_or(false);

    if ends_with_go {
        // Already ends with GO, just normalize line endings
        format!("{}\n", trimmed)
    } else {
        // Add GO at the end
        format!("{}\nGO\n", trimmed)
    }
}
