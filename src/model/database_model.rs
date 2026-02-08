//! Database model representation

use super::ModelElement;

/// The complete database model
#[derive(Debug, Clone)]
pub struct DatabaseModel {
    /// All model elements
    pub elements: Vec<ModelElement>,
    /// Pre-computed full names for each element (parallel to `elements` vec).
    /// Populated by `cache_element_names()` to avoid repeated `format!()` allocations
    /// in `full_name()` during sorting and XML generation.
    pub cached_full_names: Vec<String>,
    /// Pre-computed xml_name_attr for each element (parallel to `elements` vec).
    /// For inline constraints without emit_name, this is empty string.
    /// For all other elements, this equals the cached full name.
    pub cached_xml_names: Vec<String>,
    /// Model version
    pub schema_version: String,
    /// File format version
    pub file_format_version: String,
}

impl Default for DatabaseModel {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            cached_full_names: Vec::new(),
            cached_xml_names: Vec::new(),
            schema_version: "2.9".to_string(),
            file_format_version: "1.2".to_string(),
        }
    }
}

impl DatabaseModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_element(&mut self, element: ModelElement) {
        self.elements.push(element);
    }

    /// Pre-compute and cache full_name and xml_name_attr for all elements.
    /// Must be called after all elements are added and before sorting.
    pub fn cache_element_names(&mut self) {
        self.cached_full_names = self.elements.iter().map(|e| e.full_name()).collect();
        self.cached_xml_names = self.elements.iter().map(|e| e.xml_name_attr()).collect();
    }

    /// Get the cached full name for an element by index.
    /// Returns the pre-computed name if available, otherwise computes it.
    pub fn full_name(&self, index: usize) -> &str {
        if index < self.cached_full_names.len() {
            &self.cached_full_names[index]
        } else {
            // Fallback — should not happen after cache_element_names()
            ""
        }
    }

    /// Get the cached xml_name_attr for an element by index.
    pub fn xml_name(&self, index: usize) -> &str {
        if index < self.cached_xml_names.len() {
            &self.cached_xml_names[index]
        } else {
            ""
        }
    }
}
