# Implementation Plan: Pre/Post Deployment Script Support

## Overview

Add support for pre-deployment and post-deployment scripts in rust-sqlpackage. These scripts are packaged as separate `.sql` files in the dacpac ZIP archive and executed before/after the main schema deployment.

## DACPAC Format Reference

Per [Microsoft documentation](https://learn.microsoft.com/en-us/sql/tools/sql-database-projects/concepts/pre-post-deployment-scripts) and the [MS-DACPAC specification](https://learn.microsoft.com/en-us/openspecs/sql_data_portability/ms-dacpac/e539cf5f-67bb-4756-a11f-0b7704791bbd):

- Pre/post deploy scripts are stored as **separate SQL files** in the dacpac ZIP
- File names: `predeploy.sql` and `postdeploy.sql`
- Scripts are NOT compiled into model.xml (they're raw SQL)
- A SQL project can have at most one pre-deploy and one post-deploy script
- Multiple source files can be combined using SQLCMD `:r` includes

## Current State

The test fixture exists at `tests/fixtures/pre_post_deploy/` with:
- `project.sqlproj` - Contains `<PreDeploy Include="...">` and `<PostDeploy Include="...">`
- `PreDeployment.sql` - Pre-deployment script
- `PostDeployment.sql` - Post-deployment script
- `Tables/Table1.sql` - Regular table definition

Currently the build succeeds but **ignores** the pre/post deploy scripts entirely.

---

## Implementation Steps

### Phase 1: Data Structures

#### 1.1 Update `SqlProject` struct (`src/project/sqlproj_parser.rs`)

Add fields to track pre/post deploy scripts:

```rust
pub struct SqlProject {
    // ... existing fields ...

    /// Pre-deployment script file (optional, at most one)
    pub pre_deploy_script: Option<PathBuf>,

    /// Post-deployment script file (optional, at most one)
    pub post_deploy_script: Option<PathBuf>,
}
```

### Phase 2: Project Parsing

#### 2.1 Parse `<PreDeploy>` and `<PostDeploy>` items (`src/project/sqlproj_parser.rs`)

Modify `parse_sqlproj()` to extract deployment script paths:

```rust
// In find_sql_files() or new function find_deployment_scripts()
for node in root.descendants() {
    if node.tag_name().name() == "PreDeploy" {
        if let Some(include) = node.attribute("Include") {
            // Store path, warn if multiple PreDeploy items
        }
    }
    if node.tag_name().name() == "PostDeploy" {
        if let Some(include) = node.attribute("Include") {
            // Store path, warn if multiple PostDeploy items
        }
    }
}
```

**Note**: Only one pre-deploy and one post-deploy script is allowed. Log a warning if multiple are specified.

### Phase 3: Dacpac Packaging

#### 3.1 Update `create_dacpac()` (`src/dacpac/packager.rs`)

Add the deployment scripts to the ZIP archive:

```rust
pub fn create_dacpac(
    model: &DatabaseModel,
    project: &SqlProject,
    output_path: &Path,
) -> Result<()> {
    // ... existing code ...

    // Write predeploy.sql (if present)
    if let Some(pre_deploy_path) = &project.pre_deploy_script {
        let content = std::fs::read_to_string(pre_deploy_path)?;
        zip.start_file("predeploy.sql", options)?;
        zip.write_all(content.as_bytes())?;
    }

    // Write postdeploy.sql (if present)
    if let Some(post_deploy_path) = &project.post_deploy_script {
        let content = std::fs::read_to_string(post_deploy_path)?;
        zip.start_file("postdeploy.sql", options)?;
        zip.write_all(content.as_bytes())?;
    }

    // ... rest of existing code ...
}
```

#### 3.2 Update `[Content_Types].xml` (`src/dacpac/packager.rs`)

Add MIME type for `.sql` files:

```rust
fn generate_content_types_xml() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml" />
  <Default Extension="sql" ContentType="text/plain" />
</Types>"#
        .to_string()
}
```

### Phase 4: Testing

#### 4.1 Update `DacpacInfo` test helper (`tests/common/mod.rs`)

Add fields to verify deployment scripts:

```rust
pub struct DacpacInfo {
    // ... existing fields ...
    pub has_predeploy: bool,
    pub has_postdeploy: bool,
    pub predeploy_content: Option<String>,
    pub postdeploy_content: Option<String>,
}
```

#### 4.2 Update integration test (`tests/integration/build_tests.rs`)

Make the test actually verify pre/post deploy scripts:

```rust
#[test]
fn test_build_with_pre_post_deploy_scripts() {
    let ctx = TestContext::with_fixture("pre_post_deploy");
    let result = ctx.build();

    assert!(
        result.success,
        "Build with pre/post deploy should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.unwrap();
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");

    // Verify table is present
    assert!(
        info.tables.iter().any(|t| t.contains("Table1")),
        "Should contain Table1"
    );

    // Verify deployment scripts are packaged
    assert!(info.has_predeploy, "Should contain predeploy.sql");
    assert!(info.has_postdeploy, "Should contain postdeploy.sql");

    // Verify script contents
    let predeploy = info.predeploy_content.expect("Should have predeploy content");
    assert!(predeploy.contains("Starting deployment"), "Predeploy should contain expected content");

    let postdeploy = info.postdeploy_content.expect("Should have postdeploy content");
    assert!(postdeploy.contains("Deployment complete"), "Postdeploy should contain expected content");
}
```

#### 4.3 Add unit tests for sqlproj parsing

```rust
#[test]
fn test_parse_predeploy_postdeploy_items() {
    // Test that PreDeploy and PostDeploy items are correctly parsed
}

#[test]
fn test_warn_on_multiple_predeploy_scripts() {
    // Test that warning is logged when multiple PreDeploy items exist
}
```

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/project/sqlproj_parser.rs` | Add `pre_deploy_script` and `post_deploy_script` fields to `SqlProject`; parse `<PreDeploy>` and `<PostDeploy>` items |
| `src/dacpac/packager.rs` | Write `predeploy.sql` and `postdeploy.sql` to ZIP; update `[Content_Types].xml` |
| `tests/common/mod.rs` | Add `has_predeploy`, `has_postdeploy`, content fields to `DacpacInfo` |
| `tests/integration/build_tests.rs` | Update `test_build_with_pre_post_deploy_scripts` to verify scripts |

---

## Out of Scope (Future Work)

1. **SQLCMD `:r` include expansion** - Combining multiple source files into single deployment script
2. **SQLCMD variable substitution** - Processing `$(Variable)` syntax
3. **RefactorLog.xml generation** - Additional metadata for schema changes
4. **Script validation** - Verifying deployment scripts are valid T-SQL

---

## Verification

After implementation:

1. Run `cargo test test_build_with_pre_post_deploy_scripts` - should pass
2. Build a project with pre/post deploy scripts
3. Rename `.dacpac` to `.zip` and extract
4. Verify `predeploy.sql` and `postdeploy.sql` exist with correct content
5. Optionally deploy with SqlPackage CLI and verify scripts execute

---

## References

- [Pre-Deployment and Post-Deployment Scripts - Microsoft Learn](https://learn.microsoft.com/en-us/sql/tools/sql-database-projects/concepts/pre-post-deployment-scripts)
- [MS-DACPAC File Format Spec](https://learn.microsoft.com/en-us/openspecs/sql_data_portability/ms-dacpac/e539cf5f-67bb-4756-a11f-0b7704791bbd)
- [MSBuild.Sdk.SqlProj Pre/Post Deploy Issue](https://github.com/rr-wfm/MSBuild.Sdk.SqlProj/issues/9)
