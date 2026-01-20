//! SQLCMD directive processing
//!
//! Handles SQLCMD directives like `:r` (include file) that are commonly used
//! in SQL Server deployment scripts.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use encoding_rs::WINDOWS_1252;
use regex::Regex;

use crate::error::SqlPackageError;

/// Read a file as a string, trying UTF-8 first, then Windows-1252 as fallback
fn read_file_with_encoding_fallback(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;

    // Try UTF-8 first (handles BOM automatically if present)
    match String::from_utf8(bytes.clone()) {
        Ok(s) => Ok(s),
        Err(_) => {
            // Fall back to Windows-1252 (common for SQL files created on Windows)
            let (decoded, _, had_errors) = WINDOWS_1252.decode(&bytes);
            if had_errors {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "File contains invalid characters",
                ))
            } else {
                Ok(decoded.into_owned())
            }
        }
    }
}

/// Expand all `:r` include directives in SQL content
///
/// The `:r` directive includes the contents of another SQL file at that point.
/// Paths can be relative (resolved from the source file's directory) or absolute.
///
/// # Arguments
/// * `content` - The SQL content to process
/// * `source_file` - The file containing this content (for relative path resolution)
///
/// # Returns
/// The expanded SQL content with all `:r` directives replaced by file contents
///
/// # Errors
/// Returns an error if:
/// - An included file cannot be found
/// - A circular include is detected
pub fn expand_includes(content: &str, source_file: &Path) -> Result<String> {
    let mut visited = HashSet::new();
    visited.insert(source_file.canonicalize().unwrap_or_else(|_| source_file.to_path_buf()));
    expand_includes_recursive(content, source_file, &mut visited)
}

/// Recursive implementation of include expansion
fn expand_includes_recursive(
    content: &str,
    source_file: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<String> {
    // First, extract any :setvar definitions and build a variable map
    let mut variables = std::collections::HashMap::new();
    let setvar_re = Regex::new(r#"(?m)^\s*:setvar\s+(\w+)\s+"?([^"\r\n]+)"?\s*$"#)
        .expect("Invalid setvar regex");
    for caps in setvar_re.captures_iter(content) {
        let var_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let var_value = caps
            .get(2)
            .map(|m| m.as_str().trim_matches('"'))
            .unwrap_or("");
        variables.insert(var_name.to_string(), var_value.to_string());
    }

    // Regex to match :r directives
    // Matches: :r path\to\file.sql or :r "path with spaces\file.sql"
    // The :r must be at the start of a line (possibly with leading whitespace)
    let re = Regex::new(r#"(?m)^\s*:r\s+(?:"([^"]+)"|(\S+))\s*$"#)
        .expect("Invalid regex pattern");

    let source_dir = source_file.parent().unwrap_or(Path::new("."));
    let mut result = String::new();
    let mut last_end = 0;

    for caps in re.captures_iter(content) {
        let match_range = caps.get(0).unwrap();

        // Add content before this match
        result.push_str(&content[last_end..match_range.start()]);

        // Extract the file path (either quoted or unquoted)
        let include_path_str = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");

        // Substitute SQLCMD variables $(varname)
        let var_re = Regex::new(r"\$\((\w+)\)").expect("Invalid var regex");
        let include_path_str = var_re
            .replace_all(include_path_str, |caps: &regex::Captures| {
                let var_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                variables
                    .get(var_name)
                    .cloned()
                    .unwrap_or_else(|| format!("$({})", var_name))
            })
            .to_string();

        // Normalize path separators (Windows paths use backslash)
        let include_path_str = include_path_str.replace('\\', "/");
        let include_path = Path::new(&include_path_str);

        // Resolve relative paths from source file's directory
        let resolved_path = if include_path.is_absolute() {
            include_path.to_path_buf()
        } else {
            source_dir.join(include_path)
        };

        // Canonicalize for comparison (handles . and ..)
        let canonical_path = resolved_path
            .canonicalize()
            .map_err(|_| SqlPackageError::SqlcmdIncludeNotFound {
                path: resolved_path.clone(),
                source_file: source_file.to_path_buf(),
            })?;

        // Check for circular includes
        if visited.contains(&canonical_path) {
            let chain = visited
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(SqlPackageError::SqlcmdCircularInclude {
                path: canonical_path,
                chain,
            }
            .into());
        }

        // Read the included file (supports UTF-8 and Windows-1252 encodings)
        let included_content =
            read_file_with_encoding_fallback(&canonical_path).map_err(|_| {
                SqlPackageError::SqlcmdIncludeNotFound {
                    path: resolved_path.clone(),
                    source_file: source_file.to_path_buf(),
                }
            })?;

        // Strip UTF-8 BOM if present
        let included_content = included_content
            .strip_prefix('\u{FEFF}')
            .unwrap_or(&included_content);

        // Track this file to detect circular includes
        visited.insert(canonical_path.clone());

        // Recursively expand includes in the included file
        let expanded = expand_includes_recursive(included_content, &canonical_path, visited)?;

        // Add a comment to show where the included content comes from
        result.push_str(&format!(
            "-- BEGIN :r {}\n",
            include_path_str
        ));
        result.push_str(&expanded);
        if !expanded.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&format!("-- END :r {}\n", include_path_str));

        // Remove from visited after processing (allows same file in different branches)
        visited.remove(&canonical_path);

        last_end = match_range.end();
    }

    // Add remaining content after last match
    result.push_str(&content[last_end..]);

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_no_includes() {
        let dir = TempDir::new().unwrap();
        let source = create_test_file(dir.path(), "main.sql", "SELECT 1;");

        let result = expand_includes("SELECT 1;", &source).unwrap();
        assert_eq!(result, "SELECT 1;");
    }

    #[test]
    fn test_simple_include() {
        let dir = TempDir::new().unwrap();
        let _included = create_test_file(dir.path(), "included.sql", "SELECT 2;");
        let source = create_test_file(dir.path(), "main.sql", "SELECT 1;\n:r included.sql\nSELECT 3;");

        let result = expand_includes("SELECT 1;\n:r included.sql\nSELECT 3;", &source).unwrap();
        assert!(result.contains("SELECT 1;"));
        assert!(result.contains("SELECT 2;"));
        assert!(result.contains("SELECT 3;"));
        assert!(result.contains("-- BEGIN :r included.sql"));
        assert!(result.contains("-- END :r included.sql"));
    }

    #[test]
    fn test_include_with_backslash() {
        let dir = TempDir::new().unwrap();
        let _included = create_test_file(dir.path(), "Scripts/seed.sql", "INSERT INTO t VALUES(1);");
        let source = create_test_file(dir.path(), "main.sql", ":r Scripts\\seed.sql");

        let result = expand_includes(":r Scripts\\seed.sql", &source).unwrap();
        assert!(result.contains("INSERT INTO t VALUES(1);"));
    }

    #[test]
    fn test_quoted_path_with_spaces() {
        let dir = TempDir::new().unwrap();
        let _included = create_test_file(dir.path(), "My Scripts/data.sql", "SELECT 'space';");
        let source = create_test_file(dir.path(), "main.sql", ":r \"My Scripts/data.sql\"");

        let result = expand_includes(":r \"My Scripts/data.sql\"", &source).unwrap();
        assert!(result.contains("SELECT 'space';"));
    }

    #[test]
    fn test_nested_includes() {
        let dir = TempDir::new().unwrap();
        let _deep = create_test_file(dir.path(), "deep.sql", "SELECT 'deep';");
        let _mid = create_test_file(dir.path(), "mid.sql", "SELECT 'mid';\n:r deep.sql");
        let source = create_test_file(dir.path(), "main.sql", ":r mid.sql");

        let result = expand_includes(":r mid.sql", &source).unwrap();
        assert!(result.contains("SELECT 'mid';"));
        assert!(result.contains("SELECT 'deep';"));
    }

    #[test]
    fn test_circular_include_detected() {
        let dir = TempDir::new().unwrap();
        let _a = create_test_file(dir.path(), "a.sql", ":r b.sql");
        let _b = create_test_file(dir.path(), "b.sql", ":r a.sql");
        let source = dir.path().join("a.sql");

        let result = expand_includes(":r b.sql", &source);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Circular"));
    }

    #[test]
    fn test_missing_file() {
        let dir = TempDir::new().unwrap();
        let source = create_test_file(dir.path(), "main.sql", ":r nonexistent.sql");

        let result = expand_includes(":r nonexistent.sql", &source);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_relative_parent_path() {
        let dir = TempDir::new().unwrap();
        let _shared = create_test_file(dir.path(), "shared.sql", "SELECT 'shared';");
        let source = create_test_file(dir.path(), "scripts/deploy.sql", ":r ../shared.sql");

        let result = expand_includes(":r ../shared.sql", &source).unwrap();
        assert!(result.contains("SELECT 'shared';"));
    }

    #[test]
    fn test_multiple_includes() {
        let dir = TempDir::new().unwrap();
        let _a = create_test_file(dir.path(), "a.sql", "SELECT 'a';");
        let _b = create_test_file(dir.path(), "b.sql", "SELECT 'b';");
        let source = create_test_file(dir.path(), "main.sql", ":r a.sql\n:r b.sql");

        let result = expand_includes(":r a.sql\n:r b.sql", &source).unwrap();
        assert!(result.contains("SELECT 'a';"));
        assert!(result.contains("SELECT 'b';"));
    }
}
