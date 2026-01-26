//! Layer 3: SqlPackage DeployReport Comparison
//!
//! Uses SqlPackage CLI to generate a deployment script between dacpacs.
//! If the script is empty (no DDL changes), the dacpacs are semantically equivalent.
//! This provides external validation that the generated schemas would deploy identically.

use std::path::Path;
use std::process::Command;

use super::types::Layer3Result;

/// Check if SqlPackage is available
pub fn sqlpackage_available() -> bool {
    Command::new("sqlpackage")
        .arg("/Version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Compare dacpacs using SqlPackage DeployReport.
///
/// This generates a deployment script from source to target - if empty, they're equivalent.
/// The deployment script shows what DDL operations would be needed to make target match source.
///
/// # Arguments
/// * `source_dacpac` - Path to the source dacpac (typically Rust-generated)
/// * `target_dacpac` - Path to the target dacpac (typically DotNet-generated)
///
/// # Returns
/// `Layer3Result` containing the deployment script and whether differences were found.
pub fn compare_with_sqlpackage(source_dacpac: &Path, target_dacpac: &Path) -> Layer3Result {
    if !sqlpackage_available() {
        return Layer3Result {
            has_differences: false,
            deploy_script: String::new(),
            error: Some("SqlPackage not available".to_string()),
        };
    }

    // Generate deploy report: what changes would be needed to go from target to source?
    // Note: /TargetDatabaseName is required for /Action:Script even when comparing dacpacs
    let output = Command::new("sqlpackage")
        .arg("/Action:Script")
        .arg(format!("/SourceFile:{}", source_dacpac.display()))
        .arg(format!("/TargetFile:{}", target_dacpac.display()))
        .arg("/TargetDatabaseName:ParityTestDb")
        .arg("/OutputPath:/dev/stdout")
        .arg("/p:IncludeTransactionalScripts=false")
        .arg("/p:CommentOutSetVarDeclarations=true")
        .output();

    match output {
        Ok(result) => {
            let script = String::from_utf8_lossy(&result.stdout).to_string();
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();

            if !result.status.success() {
                return Layer3Result {
                    has_differences: true,
                    deploy_script: script,
                    error: Some(stderr),
                };
            }

            // Check if script contains actual schema changes
            let has_changes = script_has_schema_changes(&script);

            Layer3Result {
                has_differences: has_changes,
                deploy_script: script,
                error: None,
            }
        }
        Err(e) => Layer3Result {
            has_differences: false,
            deploy_script: String::new(),
            error: Some(format!("Failed to run SqlPackage: {}", e)),
        },
    }
}

/// Check if a deployment script contains actual schema changes.
///
/// Filters out boilerplate comments, SET statements, and other non-DDL content
/// to determine if there are actual CREATE/ALTER/DROP statements.
fn script_has_schema_changes(script: &str) -> bool {
    // Filter out comments and empty lines
    let significant_lines: Vec<_> = script
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with("--"))
        .filter(|l| !l.starts_with("/*"))
        .filter(|l| !l.starts_with("PRINT"))
        .filter(|l| !l.starts_with("GO"))
        .filter(|l| !l.starts_with(":")) // SQLCMD variables
        .filter(|l| !l.starts_with("SET "))
        .filter(|l| !l.starts_with("USE "))
        .collect();

    // Look for actual DDL statements
    significant_lines.iter().any(|l| {
        let upper = l.to_uppercase();
        upper.starts_with("CREATE ")
            || upper.starts_with("ALTER ")
            || upper.starts_with("DROP ")
            || upper.starts_with("EXEC ")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_has_no_changes_for_empty() {
        assert!(!script_has_schema_changes(""));
        assert!(!script_has_schema_changes("-- Comment\n"));
        assert!(!script_has_schema_changes("SET ANSI_NULLS ON\nGO\n"));
    }

    #[test]
    fn test_script_has_changes_for_ddl() {
        assert!(script_has_schema_changes(
            "CREATE TABLE [dbo].[Test] ([Id] INT)"
        ));
        assert!(script_has_schema_changes(
            "ALTER TABLE [dbo].[Test] ADD [Col] INT"
        ));
        assert!(script_has_schema_changes("DROP TABLE [dbo].[Test]"));
    }
}
