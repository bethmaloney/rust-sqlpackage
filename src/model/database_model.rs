//! Database model representation

use super::ModelElement;

/// The complete database model
#[derive(Debug, Clone)]
pub struct DatabaseModel {
    /// All model elements
    pub elements: Vec<ModelElement>,
    /// Model version
    pub schema_version: String,
    /// File format version
    pub file_format_version: String,
}

impl Default for DatabaseModel {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
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
}
