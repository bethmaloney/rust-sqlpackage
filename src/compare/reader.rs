//! Read dacpac ZIP contents into memory

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::Result;
use zip::ZipArchive;

use crate::error::SqlPackageError;

/// All files from a dacpac ZIP, loaded into memory.
pub struct DacpacContents {
    files: HashMap<String, Vec<u8>>,
}

impl DacpacContents {
    /// Read all entries from a dacpac ZIP file.
    pub fn from_path(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| SqlPackageError::DacpacReadError {
            path: path.to_path_buf(),
            source: e,
        })?;

        let mut archive = ZipArchive::new(file).map_err(|e| SqlPackageError::ZipError {
            message: format!("Failed to read dacpac {}: {}", path.display(), e),
        })?;

        let mut files = HashMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| SqlPackageError::ZipError {
                message: format!("Failed to read entry {} in {}: {}", i, path.display(), e),
            })?;

            let name = entry.name().to_string();
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| SqlPackageError::DacpacReadError {
                    path: path.to_path_buf(),
                    source: e,
                })?;
            files.insert(name, data);
        }

        Ok(Self { files })
    }

    /// Get file contents as a UTF-8 string.
    pub fn get_string(&self, name: &str) -> Option<String> {
        self.files
            .get(name)
            .and_then(|data| String::from_utf8(data.clone()).ok())
    }

    /// Get raw file contents.
    pub fn get_bytes(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(|v| v.as_slice())
    }

    /// List all file names in the dacpac.
    pub fn file_names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn create_test_zip(entries: &[(&str, &[u8])]) -> NamedTempFile {
        let tmp = NamedTempFile::new().unwrap();
        let mut zip = ZipWriter::new(tmp.reopen().unwrap());
        let options = SimpleFileOptions::default();
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        tmp
    }

    #[test]
    fn test_read_zip_contents() {
        let tmp = create_test_zip(&[("model.xml", b"<root/>"), ("DacMetadata.xml", b"<meta/>")]);

        let contents = DacpacContents::from_path(tmp.path()).unwrap();
        assert_eq!(contents.get_string("model.xml").unwrap(), "<root/>");
        assert_eq!(contents.get_string("DacMetadata.xml").unwrap(), "<meta/>");
        assert!(contents.get_string("missing.xml").is_none());
    }

    #[test]
    fn test_file_names() {
        let tmp = create_test_zip(&[("a.xml", b"a"), ("b.sql", b"b")]);
        let contents = DacpacContents::from_path(tmp.path()).unwrap();
        let mut names: Vec<&str> = contents.file_names().collect();
        names.sort();
        assert_eq!(names, vec!["a.xml", "b.sql"]);
    }

    #[test]
    fn test_nonexistent_path() {
        let result = DacpacContents::from_path(Path::new("/nonexistent/file.dacpac"));
        assert!(result.is_err());
    }
}
