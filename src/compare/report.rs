//! Human-readable comparison report printer

use super::types::{CompareResult, FileStatus};

/// Print the comparison report to stdout, matching the Python tool's format.
pub fn print_report(result: &CompareResult) {
    println!("=== Dacpac Comparison Report ===");
    println!();

    // File-level results
    for (label, status) in &result.file_results {
        println!("--- {} ---", label);
        match status {
            FileStatus::Ok => println!("OK (identical)"),
            FileStatus::Skipped(reason) => println!("OK ({})", reason),
            FileStatus::MissingInRust => println!("  File missing in rust dacpac"),
            FileStatus::MissingInDotnet => println!("  File missing in dotnet dacpac"),
            FileStatus::Different(lines) => {
                let limit = 50;
                for line in lines.iter().take(limit) {
                    println!("{}", line);
                }
                if lines.len() > limit {
                    println!("  ... ({} more lines)", lines.len() - limit);
                }
            }
        }
        println!();
    }

    // Header
    if let Some(header) = &result.header_result {
        println!("--- model.xml: Header ---");
        if header.is_ok {
            println!("OK (identical)");
        } else {
            for line in &header.diffs {
                println!("{}", line);
            }
        }
        println!();
    }

    // Elements
    if let Some(elems) = &result.elements_result {
        println!("--- model.xml: Elements ---");
        println!(
            "Total elements: rust={}, dotnet={}",
            elems.total_rust, elems.total_dotnet
        );
        println!();

        println!("Missing in rust ({}):", elems.missing_in_rust.len());
        if elems.missing_in_rust.is_empty() {
            println!("  (none)");
        } else {
            for key in &elems.missing_in_rust {
                println!("  {}", key);
            }
        }
        println!();

        println!("Extra in rust ({}):", elems.extra_in_rust.len());
        if elems.extra_in_rust.is_empty() {
            println!("  (none)");
        } else {
            for key in &elems.extra_in_rust {
                println!("  {}", key);
            }
        }
        println!();

        println!("Differences ({}):", elems.differences.len());
        if elems.differences.is_empty() {
            println!("  (none)");
        } else {
            for (key, diff_lines) in &elems.differences {
                println!("  {}:", key);
                for line in diff_lines {
                    println!("{}", line);
                }
            }
        }
        println!();

        println!(
            "Summary: {} missing, {} extra, {} different",
            elems.missing_in_rust.len(),
            elems.extra_in_rust.len(),
            elems.differences.len()
        );
    }
}
