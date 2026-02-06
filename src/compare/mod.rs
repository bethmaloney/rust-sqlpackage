//! Compare two dacpac files and report differences
//!
//! Ports the functionality of `tools/compare_dacpacs.py` into the Rust codebase
//! as a first-class module.

pub mod model_xml;
pub mod reader;
pub mod report;
pub mod simple_xml;
pub mod text;
pub mod types;

use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;

use reader::DacpacContents;
use types::{CompareResult, FileStatus};

/// Known files in a dacpac that we handle explicitly.
const KNOWN_FILES: &[&str] = &[
    "Origin.xml",
    "DacMetadata.xml",
    "[Content_Types].xml",
    "predeploy.sql",
    "postdeploy.sql",
    "model.xml",
];

/// Compare two dacpac files and return a structured result.
pub fn compare_dacpacs(rust_path: &Path, dotnet_path: &Path) -> Result<CompareResult> {
    let rust_dac = DacpacContents::from_path(rust_path)?;
    let dotnet_dac = DacpacContents::from_path(dotnet_path)?;

    let mut file_results = Vec::new();

    // Origin.xml - skip (timestamps/GUIDs always differ)
    file_results.push((
        "Origin.xml".to_string(),
        FileStatus::Skipped("skipped - contains timestamps/GUIDs".to_string()),
    ));

    // Simple XML files: DacMetadata.xml, [Content_Types].xml
    for fname in &["DacMetadata.xml", "[Content_Types].xml"] {
        let status = match (rust_dac.get_string(fname), dotnet_dac.get_string(fname)) {
            (None, None) => continue,
            (None, Some(_)) => FileStatus::MissingInRust,
            (Some(_), None) => FileStatus::MissingInDotnet,
            (Some(a), Some(b)) => simple_xml::compare_simple_xml(&a, &b),
        };
        file_results.push((fname.to_string(), status));
    }

    // Text files: predeploy.sql, postdeploy.sql
    for fname in &["predeploy.sql", "postdeploy.sql"] {
        let status = match (rust_dac.get_string(fname), dotnet_dac.get_string(fname)) {
            (None, None) => continue,
            (None, Some(_)) => FileStatus::MissingInRust,
            (Some(_), None) => FileStatus::MissingInDotnet,
            (Some(a), Some(b)) => text::compare_text(&a, &b),
        };
        file_results.push((fname.to_string(), status));
    }

    // Check for unexpected files
    let known: HashSet<&str> = KNOWN_FILES.iter().copied().collect();
    let rust_files: HashSet<String> = rust_dac.file_names().map(|s| s.to_string()).collect();
    let dotnet_files: HashSet<String> = dotnet_dac.file_names().map(|s| s.to_string()).collect();

    let mut unknown_only_rust: Vec<&String> = rust_files
        .difference(&dotnet_files)
        .filter(|f| !known.contains(f.as_str()))
        .collect();
    unknown_only_rust.sort();

    let mut unknown_only_dotnet: Vec<&String> = dotnet_files
        .difference(&rust_files)
        .filter(|f| !known.contains(f.as_str()))
        .collect();
    unknown_only_dotnet.sort();

    let mut unknown_both: Vec<&String> = rust_files
        .intersection(&dotnet_files)
        .filter(|f| !known.contains(f.as_str()))
        .collect();
    unknown_both.sort();

    if !unknown_only_rust.is_empty() {
        let names: Vec<&str> = unknown_only_rust.iter().map(|s| s.as_str()).collect();
        file_results.push((
            "(unexpected files)".to_string(),
            FileStatus::Different(vec![format!("  Only in rust: {}", names.join(", "))]),
        ));
    }
    if !unknown_only_dotnet.is_empty() {
        let names: Vec<&str> = unknown_only_dotnet.iter().map(|s| s.as_str()).collect();
        let label = if unknown_only_rust.is_empty() {
            "(unexpected files)"
        } else {
            "(unexpected files in dotnet)"
        };
        file_results.push((
            label.to_string(),
            FileStatus::Different(vec![format!("  Only in dotnet: {}", names.join(", "))]),
        ));
    }
    for fname in &unknown_both {
        if let (Some(a), Some(b)) = (rust_dac.get_string(fname), dotnet_dac.get_string(fname)) {
            let status = text::compare_text(&a, &b);
            if !status.is_ok() {
                file_results.push((fname.to_string(), status));
            }
        }
    }

    // model.xml - semantic comparison
    let mut header_result = None;
    let mut elements_result = None;
    let mut duplicate_warnings = Vec::new();

    match (
        rust_dac.get_string("model.xml"),
        dotnet_dac.get_string("model.xml"),
    ) {
        (Some(a), Some(b)) => {
            let (header, elems, dupes) = model_xml::compare_model_xml(&a, &b);
            header_result = Some(header);
            elements_result = Some(elems);
            duplicate_warnings = dupes;
        }
        (None, Some(_)) => {
            file_results.push(("model.xml".to_string(), FileStatus::MissingInRust));
        }
        (Some(_), None) => {
            file_results.push(("model.xml".to_string(), FileStatus::MissingInDotnet));
        }
        (None, None) => {}
    }

    Ok(CompareResult {
        file_results,
        header_result,
        elements_result,
        duplicate_warnings,
    })
}
