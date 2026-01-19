//! Create dacpac ZIP package

use std::fs::File;
use std::io::{Cursor, Write};
use std::path::Path;

use anyhow::Result;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::error::SqlPackageError;
use crate::model::DatabaseModel;
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
        .compression_level(Some(6));

    // Write model.xml
    let mut model_buffer = Cursor::new(Vec::new());
    model_xml::generate_model_xml(&mut model_buffer, model, project)?;
    zip.start_file("model.xml", options)?;
    zip.write_all(model_buffer.get_ref())?;

    // Write DacMetadata.xml
    let mut metadata_buffer = Cursor::new(Vec::new());
    metadata_xml::generate_metadata_xml(&mut metadata_buffer, project)?;
    zip.start_file("DacMetadata.xml", options)?;
    zip.write_all(metadata_buffer.get_ref())?;

    // Write Origin.xml
    let mut origin_buffer = Cursor::new(Vec::new());
    origin_xml::generate_origin_xml(&mut origin_buffer)?;
    zip.start_file("Origin.xml", options)?;
    zip.write_all(origin_buffer.get_ref())?;

    // Write [Content_Types].xml (required for package format)
    let content_types = generate_content_types();
    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(content_types.as_bytes())?;

    zip.finish()?;

    Ok(())
}

fn generate_content_types() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="text/xml" />
</Types>"#
        .to_string()
}
