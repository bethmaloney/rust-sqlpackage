//! Layer 7: Canonical XML Comparison
//!
//! Final validation layer for true byte-level matching.
//! Canonicalizes model.xml by normalizing element order, property order,
//! and formatting, then compares the results.

use std::path::Path;

use super::types::{extract_model_xml, CanonicalXmlError};

/// Represents a canonicalized element for sorting and serialization
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalElement {
    /// Element type (e.g., "SqlTable", "SqlView")
    element_type: String,
    /// Element name (e.g., "[dbo].[Products]") - empty string for unnamed elements
    name: String,
    /// Sorted properties as (name, value) pairs
    properties: Vec<(String, String)>,
    /// Sorted relationships
    relationships: Vec<CanonicalRelationship>,
    /// Nested child elements (recursively canonicalized)
    children: Vec<CanonicalElement>,
}

/// Represents a canonicalized relationship for sorting
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalRelationship {
    /// Relationship name (e.g., "Schema", "Columns")
    name: String,
    /// Sorted references as (name, external_source) pairs
    references: Vec<(String, Option<String>)>,
    /// Nested entry elements (recursively canonicalized)
    entries: Vec<CanonicalElement>,
}

/// Canonicalize a model.xml string for deterministic comparison.
///
/// Canonicalization normalizes the XML structure by:
/// 1. Parsing the XML into a structured representation
/// 2. Sorting top-level elements by (Type, Name) - preserving semantic grouping
/// 3. Sorting properties alphabetically by Name within each element
/// 4. Sorting relationships alphabetically by Name within each element
/// 5. Recursively canonicalizing nested elements within relationships
/// 6. Normalizing whitespace (consistent indentation, no trailing whitespace)
/// 7. Re-serializing to a deterministic XML string
///
/// This allows byte-level comparison of semantically equivalent model.xml files
/// that may have different element ordering or formatting.
pub fn canonicalize_model_xml(xml: &str) -> Result<String, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("Failed to parse XML: {}", e))?;

    let root = doc.root_element();

    // Extract root attributes
    let file_format_version = root.attribute("FileFormatVersion").unwrap_or("1.2");
    let schema_version = root.attribute("SchemaVersion").unwrap_or("2.9");
    let dsp_name = root
        .attribute("DspName")
        .unwrap_or("Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider");
    let collation_lcid = root.attribute("CollationLcid").unwrap_or("1033");
    let collation_case_sensitive = root.attribute("CollationCaseSensitive").unwrap_or("False");

    // Find Model element
    let model_node = root
        .children()
        .find(|n| n.has_tag_name("Model"))
        .ok_or("No Model element found")?;

    // Parse and canonicalize all elements
    let mut canonical_elements: Vec<CanonicalElement> = model_node
        .children()
        .filter(|n| n.has_tag_name("Element"))
        .map(|node| canonicalize_element(&node))
        .collect();

    // Sort elements by (type, name) for deterministic ordering
    canonical_elements.sort();

    // Find Header element (if present)
    let header_node = root.children().find(|n| n.has_tag_name("Header"));

    // Serialize to canonical XML
    let mut output = String::new();
    output.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    output.push_str(&format!(
        "<DataSchemaModel FileFormatVersion=\"{}\" SchemaVersion=\"{}\" DspName=\"{}\" CollationLcid=\"{}\" CollationCaseSensitive=\"{}\" xmlns=\"http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02\">\n",
        file_format_version, schema_version, dsp_name, collation_lcid, collation_case_sensitive
    ));

    // Serialize Header if present
    if let Some(header) = header_node {
        serialize_header(&header, &mut output, 1);
    }

    // Serialize Model with canonicalized elements
    output.push_str("  <Model>\n");
    for element in &canonical_elements {
        serialize_canonical_element(element, &mut output, 2);
    }
    output.push_str("  </Model>\n");

    output.push_str("</DataSchemaModel>\n");

    Ok(output)
}

/// Convert a parsed element node into a canonical representation
fn canonicalize_element(node: &roxmltree::Node) -> CanonicalElement {
    let element_type = node.attribute("Type").unwrap_or("Unknown").to_string();
    let name = node.attribute("Name").unwrap_or("").to_string();

    // Collect and sort properties
    let mut properties: Vec<(String, String)> = node
        .children()
        .filter(|n| n.has_tag_name("Property"))
        .filter_map(|n| {
            let prop_name = n.attribute("Name")?;
            // Handle both inline Value attribute and nested <Value> element
            let prop_value = if let Some(val) = n.attribute("Value") {
                val.to_string()
            } else {
                // Look for nested <Value> element (for CDATA content)
                n.children()
                    .find(|c| c.has_tag_name("Value"))
                    .map(|v| v.text().unwrap_or("").to_string())
                    .unwrap_or_default()
            };
            Some((prop_name.to_string(), prop_value))
        })
        .collect();
    properties.sort();

    // Collect and sort relationships
    let mut relationships: Vec<CanonicalRelationship> = node
        .children()
        .filter(|n| n.has_tag_name("Relationship"))
        .map(|rel_node| canonicalize_relationship(&rel_node))
        .collect();
    relationships.sort();

    // Collect and canonicalize direct child elements (for Annotation elements)
    let mut children: Vec<CanonicalElement> = node
        .children()
        .filter(|n| n.has_tag_name("Annotation"))
        .map(|n| canonicalize_annotation(&n))
        .collect();
    children.sort();

    CanonicalElement {
        element_type,
        name,
        properties,
        relationships,
        children,
    }
}

/// Convert a relationship node into a canonical representation
fn canonicalize_relationship(node: &roxmltree::Node) -> CanonicalRelationship {
    let name = node.attribute("Name").unwrap_or("Unknown").to_string();

    let mut references: Vec<(String, Option<String>)> = Vec::new();
    let mut entries: Vec<CanonicalElement> = Vec::new();

    for entry in node.children().filter(|n| n.has_tag_name("Entry")) {
        for child in entry.children() {
            if child.has_tag_name("References") {
                if let Some(ref_name) = child.attribute("Name") {
                    let external_source = child.attribute("ExternalSource").map(|s| s.to_string());
                    references.push((ref_name.to_string(), external_source));
                }
            } else if child.has_tag_name("Element") {
                entries.push(canonicalize_element(&child));
            }
        }
    }

    // Sort references and entries for deterministic ordering
    references.sort();
    entries.sort();

    CanonicalRelationship {
        name,
        references,
        entries,
    }
}

/// Convert an Annotation element into a canonical representation
fn canonicalize_annotation(node: &roxmltree::Node) -> CanonicalElement {
    let element_type = node.attribute("Type").unwrap_or("Annotation").to_string();
    let disambiguator = node.attribute("Disambiguator").unwrap_or("").to_string();

    // Collect and sort properties
    let mut properties: Vec<(String, String)> = node
        .children()
        .filter(|n| n.has_tag_name("Property"))
        .filter_map(|n| {
            let prop_name = n.attribute("Name")?;
            let prop_value = if let Some(val) = n.attribute("Value") {
                val.to_string()
            } else {
                n.children()
                    .find(|c| c.has_tag_name("Value"))
                    .map(|v| v.text().unwrap_or("").to_string())
                    .unwrap_or_default()
            };
            Some((prop_name.to_string(), prop_value))
        })
        .collect();
    properties.sort();

    // Add disambiguator as a property if present
    if !disambiguator.is_empty() {
        properties.push(("Disambiguator".to_string(), disambiguator));
        properties.sort();
    }

    CanonicalElement {
        element_type,
        name: String::new(),
        properties,
        relationships: Vec::new(),
        children: Vec::new(),
    }
}

/// Serialize the Header element to canonical XML
fn serialize_header(header: &roxmltree::Node, output: &mut String, indent: usize) {
    let indent_str = "  ".repeat(indent);
    output.push_str(&format!("{}<Header>\n", indent_str));

    // Collect CustomData elements and sort by Category
    let mut custom_data: Vec<_> = header
        .children()
        .filter(|n| n.has_tag_name("CustomData"))
        .map(|n| {
            let category = n.attribute("Category").unwrap_or("");
            let type_attr = n.attribute("Type");
            let metadata: Vec<_> = n
                .children()
                .filter(|m| m.has_tag_name("Metadata"))
                .map(|m| {
                    (
                        m.attribute("Name").unwrap_or("").to_string(),
                        m.attribute("Value").unwrap_or("").to_string(),
                    )
                })
                .collect();
            (
                category.to_string(),
                type_attr.map(|s| s.to_string()),
                metadata,
            )
        })
        .collect();

    custom_data.sort_by(|a, b| a.0.cmp(&b.0));

    for (category, type_attr, metadata) in custom_data {
        let type_str = type_attr
            .map(|t| format!(" Type=\"{}\"", t))
            .unwrap_or_default();

        if metadata.is_empty() {
            output.push_str(&format!(
                "{}  <CustomData Category=\"{}\"{} />\n",
                indent_str, category, type_str
            ));
        } else {
            output.push_str(&format!(
                "{}  <CustomData Category=\"{}\"{}>\n",
                indent_str, category, type_str
            ));
            for (name, value) in metadata {
                output.push_str(&format!(
                    "{}    <Metadata Name=\"{}\" Value=\"{}\" />\n",
                    indent_str, name, value
                ));
            }
            output.push_str(&format!("{}  </CustomData>\n", indent_str));
        }
    }

    output.push_str(&format!("{}</Header>\n", indent_str));
}

/// Serialize a canonical element to XML
fn serialize_canonical_element(element: &CanonicalElement, output: &mut String, indent: usize) {
    let indent_str = "  ".repeat(indent);

    // Build opening tag
    let name_attr = if element.name.is_empty() {
        String::new()
    } else {
        format!(" Name=\"{}\"", escape_xml_attr(&element.name))
    };

    // Check if element is empty (no properties, relationships, or children)
    let is_empty = element.properties.is_empty()
        && element.relationships.is_empty()
        && element.children.is_empty();

    if is_empty {
        output.push_str(&format!(
            "{}<Element Type=\"{}\"{} />\n",
            indent_str, element.element_type, name_attr
        ));
        return;
    }

    output.push_str(&format!(
        "{}<Element Type=\"{}\"{}>\n",
        indent_str, element.element_type, name_attr
    ));

    // Serialize properties
    for (prop_name, prop_value) in &element.properties {
        // Check if value needs CDATA (contains special chars or is multi-line)
        if needs_cdata(prop_value) {
            output.push_str(&format!(
                "{}  <Property Name=\"{}\">\n",
                indent_str, prop_name
            ));
            output.push_str(&format!(
                "{}    <Value><![CDATA[{}]]></Value>\n",
                indent_str, prop_value
            ));
            output.push_str(&format!("{}  </Property>\n", indent_str));
        } else {
            output.push_str(&format!(
                "{}  <Property Name=\"{}\" Value=\"{}\" />\n",
                indent_str,
                prop_name,
                escape_xml_attr(prop_value)
            ));
        }
    }

    // Serialize relationships
    for rel in &element.relationships {
        serialize_canonical_relationship(rel, output, indent + 1);
    }

    // Serialize child annotations
    for child in &element.children {
        serialize_canonical_annotation(child, output, indent + 1);
    }

    output.push_str(&format!("{}</Element>\n", indent_str));
}

/// Serialize a canonical relationship to XML
fn serialize_canonical_relationship(
    rel: &CanonicalRelationship,
    output: &mut String,
    indent: usize,
) {
    let indent_str = "  ".repeat(indent);

    // Check if relationship is empty
    if rel.references.is_empty() && rel.entries.is_empty() {
        output.push_str(&format!(
            "{}<Relationship Name=\"{}\" />\n",
            indent_str, rel.name
        ));
        return;
    }

    output.push_str(&format!(
        "{}<Relationship Name=\"{}\">\n",
        indent_str, rel.name
    ));

    // Serialize references
    for (ref_name, external_source) in &rel.references {
        let ext_attr = external_source
            .as_ref()
            .map(|s| format!(" ExternalSource=\"{}\"", s))
            .unwrap_or_default();
        output.push_str(&format!("{}  <Entry>\n", indent_str));
        output.push_str(&format!(
            "{}    <References{} Name=\"{}\" />\n",
            indent_str,
            ext_attr,
            escape_xml_attr(ref_name)
        ));
        output.push_str(&format!("{}  </Entry>\n", indent_str));
    }

    // Serialize entry elements
    for entry in &rel.entries {
        output.push_str(&format!("{}  <Entry>\n", indent_str));
        serialize_canonical_element(entry, output, indent + 2);
        output.push_str(&format!("{}  </Entry>\n", indent_str));
    }

    output.push_str(&format!("{}</Relationship>\n", indent_str));
}

/// Serialize a canonical annotation to XML
fn serialize_canonical_annotation(element: &CanonicalElement, output: &mut String, indent: usize) {
    let indent_str = "  ".repeat(indent);

    // Check for Disambiguator in properties
    let disambiguator = element
        .properties
        .iter()
        .find(|(k, _)| k == "Disambiguator")
        .map(|(_, v)| v.clone());

    // Filter out Disambiguator from serialized properties
    let props_without_disambiguator: Vec<_> = element
        .properties
        .iter()
        .filter(|(k, _)| k != "Disambiguator")
        .collect();

    let disamb_attr = disambiguator
        .map(|d| format!(" Disambiguator=\"{}\"", d))
        .unwrap_or_default();

    if props_without_disambiguator.is_empty() {
        output.push_str(&format!(
            "{}<Annotation Type=\"{}\"{} />\n",
            indent_str, element.element_type, disamb_attr
        ));
    } else {
        output.push_str(&format!(
            "{}<Annotation Type=\"{}\"{}>\n",
            indent_str, element.element_type, disamb_attr
        ));
        for (prop_name, prop_value) in props_without_disambiguator {
            if needs_cdata(prop_value) {
                output.push_str(&format!(
                    "{}  <Property Name=\"{}\">\n",
                    indent_str, prop_name
                ));
                output.push_str(&format!(
                    "{}    <Value><![CDATA[{}]]></Value>\n",
                    indent_str, prop_value
                ));
                output.push_str(&format!("{}  </Property>\n", indent_str));
            } else {
                output.push_str(&format!(
                    "{}  <Property Name=\"{}\" Value=\"{}\" />\n",
                    indent_str,
                    prop_name,
                    escape_xml_attr(prop_value)
                ));
            }
        }
        output.push_str(&format!("{}</Annotation>\n", indent_str));
    }
}

/// Check if a value needs CDATA encoding
fn needs_cdata(value: &str) -> bool {
    value.contains('\n') || value.contains('<') || value.contains('>') || value.contains("]]>")
}

/// Escape special characters for XML attribute values
fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Compare two canonicalized XML strings and return differences.
///
/// This performs a line-by-line comparison of canonicalized XML, reporting:
/// - The first line that differs between the two outputs
/// - Line count mismatches if the outputs have different lengths
pub fn compare_canonical_xml(
    rust_canonical: &str,
    dotnet_canonical: &str,
) -> Vec<CanonicalXmlError> {
    let mut errors = Vec::new();

    let rust_lines: Vec<&str> = rust_canonical.lines().collect();
    let dotnet_lines: Vec<&str> = dotnet_canonical.lines().collect();

    // Check line counts
    if rust_lines.len() != dotnet_lines.len() {
        errors.push(CanonicalXmlError::LineCountMismatch {
            rust_lines: rust_lines.len(),
            dotnet_lines: dotnet_lines.len(),
        });
    }

    // Compare line by line
    let min_len = rust_lines.len().min(dotnet_lines.len());
    for i in 0..min_len {
        if rust_lines[i] != dotnet_lines[i] {
            errors.push(CanonicalXmlError::ContentMismatch {
                line_number: i + 1,
                rust_line: rust_lines[i].to_string(),
                dotnet_line: dotnet_lines[i].to_string(),
            });
            // Only report first 10 differences to avoid overwhelming output
            if errors.len() >= 10 {
                break;
            }
        }
    }

    errors
}

/// Generate a unified diff between two strings.
///
/// Returns a string showing additions (+), deletions (-), and context lines.
/// This is useful for debugging canonical XML comparison failures.
pub fn generate_diff(rust_content: &str, dotnet_content: &str, context_lines: usize) -> String {
    let rust_lines: Vec<&str> = rust_content.lines().collect();
    let dotnet_lines: Vec<&str> = dotnet_content.lines().collect();

    if rust_lines == dotnet_lines {
        return String::new();
    }

    let mut output = String::new();
    output.push_str("--- Rust (actual)\n");
    output.push_str("+++ DotNet (expected)\n");

    // Find differences using a simple LCS-based approach
    let diff_ranges = find_diff_ranges(&rust_lines, &dotnet_lines);

    for (rust_start, rust_end, dotnet_start, dotnet_end) in diff_ranges {
        // Calculate context range
        let ctx_start_rust = rust_start.saturating_sub(context_lines);
        let ctx_end_rust = (rust_end + context_lines).min(rust_lines.len());
        let ctx_start_dotnet = dotnet_start.saturating_sub(context_lines);
        let ctx_end_dotnet = (dotnet_end + context_lines).min(dotnet_lines.len());

        output.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            ctx_start_rust + 1,
            ctx_end_rust - ctx_start_rust,
            ctx_start_dotnet + 1,
            ctx_end_dotnet - ctx_start_dotnet
        ));

        // Output context before
        for line in rust_lines.iter().take(rust_start).skip(ctx_start_rust) {
            output.push_str(&format!(" {}\n", line));
        }

        // Output deleted lines (from Rust)
        for line in rust_lines.iter().take(rust_end).skip(rust_start) {
            output.push_str(&format!("-{}\n", line));
        }

        // Output added lines (from DotNet)
        for line in dotnet_lines.iter().take(dotnet_end).skip(dotnet_start) {
            output.push_str(&format!("+{}\n", line));
        }

        // Output context after
        for line in rust_lines.iter().take(ctx_end_rust).skip(rust_end) {
            output.push_str(&format!(" {}\n", line));
        }
    }

    output
}

/// Find ranges of differences between two line vectors.
/// Returns vector of (rust_start, rust_end, dotnet_start, dotnet_end) tuples.
fn find_diff_ranges(
    rust_lines: &[&str],
    dotnet_lines: &[&str],
) -> Vec<(usize, usize, usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    let mut j = 0;

    while i < rust_lines.len() || j < dotnet_lines.len() {
        // Skip matching lines
        while i < rust_lines.len() && j < dotnet_lines.len() && rust_lines[i] == dotnet_lines[j] {
            i += 1;
            j += 1;
        }

        if i >= rust_lines.len() && j >= dotnet_lines.len() {
            break;
        }

        // Found a difference - find the extent
        let rust_diff_start = i;
        let dotnet_diff_start = j;

        // Try to find where they sync up again
        let mut found_sync = false;
        for look_ahead in 1..=20 {
            // Check if Rust catches up to DotNet
            if i + look_ahead < rust_lines.len()
                && j < dotnet_lines.len()
                && rust_lines[i + look_ahead] == dotnet_lines[j]
            {
                ranges.push((rust_diff_start, i + look_ahead, dotnet_diff_start, j));
                i += look_ahead;
                found_sync = true;
                break;
            }
            // Check if DotNet catches up to Rust
            if j + look_ahead < dotnet_lines.len()
                && i < rust_lines.len()
                && dotnet_lines[j + look_ahead] == rust_lines[i]
            {
                ranges.push((rust_diff_start, i, dotnet_diff_start, j + look_ahead));
                j += look_ahead;
                found_sync = true;
                break;
            }
        }

        if !found_sync {
            // Can't find sync point - treat rest as different
            ranges.push((
                rust_diff_start,
                rust_lines.len(),
                dotnet_diff_start,
                dotnet_lines.len(),
            ));
            break;
        }
    }

    ranges
}

/// Compute SHA256 checksum of a string.
///
/// # Returns
/// A lowercase hex string representation of the SHA256 hash.
pub fn compute_sha256(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Compare two dacpacs using canonical XML comparison.
///
/// This function:
/// 1. Extracts model.xml from both dacpacs
/// 2. Canonicalizes both XML documents
/// 3. Compares the canonicalized output
/// 4. Optionally computes and compares SHA256 checksums
pub fn compare_canonical_dacpacs(
    rust_dacpac: &Path,
    dotnet_dacpac: &Path,
    include_checksum: bool,
) -> Result<Vec<CanonicalXmlError>, String> {
    // Extract model.xml from both dacpacs
    let rust_xml = extract_model_xml(rust_dacpac)?;
    let dotnet_xml = extract_model_xml(dotnet_dacpac)?;

    // Canonicalize both
    let rust_canonical = canonicalize_model_xml(&rust_xml)?;
    let dotnet_canonical = canonicalize_model_xml(&dotnet_xml)?;

    // Compare canonicalized output
    let mut errors = compare_canonical_xml(&rust_canonical, &dotnet_canonical);

    // Add checksum comparison if requested
    if include_checksum && errors.is_empty() {
        let rust_checksum = compute_sha256(&rust_canonical);
        let dotnet_checksum = compute_sha256(&dotnet_canonical);

        if rust_checksum != dotnet_checksum {
            errors.push(CanonicalXmlError::ChecksumMismatch {
                rust_checksum,
                dotnet_checksum,
            });
        }
    }

    Ok(errors)
}
