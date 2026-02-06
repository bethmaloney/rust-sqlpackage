//! Line-by-line text comparison for deploy scripts

use super::types::FileStatus;

/// Compare two text strings with trailing-whitespace normalization per line.
/// Returns a unified diff if different.
pub fn compare_text(a: &str, b: &str) -> FileStatus {
    let lines_a: Vec<&str> = a.lines().map(|l| l.trim_end()).collect();
    let lines_b: Vec<&str> = b.lines().map(|l| l.trim_end()).collect();

    if lines_a == lines_b {
        return FileStatus::Ok;
    }

    // Build a simple unified diff (dotnet = b, rust = a)
    let mut diff_lines = Vec::new();
    diff_lines.push("--- dotnet".to_string());
    diff_lines.push("+++ rust".to_string());

    // Simple line-by-line diff with context
    let max_len = lines_a.len().max(lines_b.len());
    let mut i = 0;
    while i < max_len {
        let la = lines_a.get(i).copied().unwrap_or("");
        let lb = lines_b.get(i).copied().unwrap_or("");
        if la != lb {
            // Find the extent of this hunk
            let start = i;
            while i < max_len {
                let la2 = lines_a.get(i).copied().unwrap_or("");
                let lb2 = lines_b.get(i).copied().unwrap_or("");
                if la2 == lb2 {
                    break;
                }
                i += 1;
            }
            // Show context
            let ctx_start = start.saturating_sub(3);
            let ctx_end = (i + 3).min(max_len);
            diff_lines.push(format!(
                "@@ -{},{} +{},{} @@",
                ctx_start + 1,
                ctx_end - ctx_start,
                ctx_start + 1,
                ctx_end - ctx_start
            ));
            for j in ctx_start..ctx_end {
                let la2 = lines_a.get(j).copied();
                let lb2 = lines_b.get(j).copied();
                if j >= start && j < i {
                    if let Some(lb2) = lb2 {
                        diff_lines.push(format!("-{}", lb2));
                    }
                    if let Some(la2) = la2 {
                        diff_lines.push(format!("+{}", la2));
                    }
                } else {
                    let line = la2.or(lb2).unwrap_or("");
                    diff_lines.push(format!(" {}", line));
                }
            }
        }
        i += 1;
    }

    FileStatus::Different(diff_lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_text() {
        assert!(compare_text("hello\nworld", "hello\nworld").is_ok());
    }

    #[test]
    fn test_trailing_whitespace_ignored() {
        assert!(compare_text("hello  \nworld\t", "hello\nworld").is_ok());
    }

    #[test]
    fn test_different_text() {
        let status = compare_text("hello\nworld", "hello\nearth");
        assert!(!status.is_ok());
        match status {
            FileStatus::Different(lines) => {
                assert!(!lines.is_empty());
            }
            _ => panic!("Expected Different"),
        }
    }

    #[test]
    fn test_empty_identical() {
        assert!(compare_text("", "").is_ok());
    }
}
