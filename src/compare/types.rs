//! Core types for dacpac comparison

use std::fmt;

/// Key identifying a model element.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElementKey {
    /// Element with a Name attribute: (Type, Name)
    Named { element_type: String, name: String },
    /// Unnamed element keyed by relationships: (Type, composite)
    Composite {
        element_type: String,
        composite: String,
    },
    /// Singleton element with only a Type: (Type,)
    Singleton { element_type: String },
}

impl fmt::Display for ElementKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ElementKey::Named { element_type, name } => write!(f, "{} {}", element_type, name),
            ElementKey::Composite {
                element_type,
                composite,
            } => write!(f, "{} {}", element_type, composite),
            ElementKey::Singleton { element_type } => write!(f, "{}", element_type),
        }
    }
}

/// A relationship entry: either a reference or an inline element fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelEntry {
    /// Named reference, possibly with `@ExternalSource` suffix
    Ref(String),
    /// Inline element represented by its fingerprint
    Inline(String),
}

impl fmt::Display for RelEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelEntry::Ref(s) => write!(f, "('ref', '{}')", s),
            RelEntry::Inline(s) => write!(f, "('inline', '{}')", s),
        }
    }
}

/// Status of a file-level comparison.
#[derive(Debug, Clone)]
pub enum FileStatus {
    /// Files are identical
    Ok,
    /// File was skipped (with reason)
    Skipped(String),
    /// File is missing in the rust dacpac
    MissingInRust,
    /// File is missing in the dotnet dacpac
    MissingInDotnet,
    /// Files differ, with detail lines
    Different(Vec<String>),
}

impl FileStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, FileStatus::Ok | FileStatus::Skipped(_))
    }
}

/// Result of comparing the model.xml Header section.
#[derive(Debug, Clone)]
pub struct HeaderResult {
    pub is_ok: bool,
    pub diffs: Vec<String>,
}

/// Result of comparing model.xml elements.
#[derive(Debug, Clone)]
pub struct ModelElementsResult {
    pub total_rust: usize,
    pub total_dotnet: usize,
    pub missing_in_rust: Vec<ElementKey>,
    pub extra_in_rust: Vec<ElementKey>,
    pub differences: Vec<(ElementKey, Vec<String>)>,
}

/// Overall result of comparing two dacpacs.
#[derive(Debug)]
pub struct CompareResult {
    /// Per-file comparison results: (label, status)
    pub file_results: Vec<(String, FileStatus)>,
    /// Header comparison result (None if model.xml missing)
    pub header_result: Option<HeaderResult>,
    /// Element comparison result (None if model.xml missing)
    pub elements_result: Option<ModelElementsResult>,
    /// Duplicate key warnings: (source, keys)
    pub duplicate_warnings: Vec<(String, Vec<ElementKey>)>,
}

impl CompareResult {
    /// Returns true if any differences were found.
    pub fn has_differences(&self) -> bool {
        for (label, status) in &self.file_results {
            if label == "Origin.xml" {
                continue;
            }
            if !status.is_ok() {
                return true;
            }
        }
        if let Some(header) = &self.header_result {
            if !header.is_ok {
                return true;
            }
        }
        if let Some(elems) = &self.elements_result {
            if !elems.missing_in_rust.is_empty()
                || !elems.extra_in_rust.is_empty()
                || !elems.differences.is_empty()
            {
                return true;
            }
        }
        false
    }
}
