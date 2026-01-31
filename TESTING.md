# Testing Guide

This document describes the testing strategy for rust-sqlpackage, including how to run tests and the parity testing approach used to ensure compatibility with the official DotNet DacFx toolchain.

## Testing Philosophy

rust-sqlpackage follows a **TDD (Test-Driven Development)** approach:
- Any new feature or bug fix must first have a test created for it
- Tests should be written before or alongside implementation
- All tests must pass before merging

## Test Structure

Tests are organized into three levels:

| Level | Location | Purpose |
|-------|----------|---------|
| **Unit** | `tests/unit/` | Test individual functions and modules in isolation |
| **Integration** | `tests/integration/` | Test module interactions and dacpac structure |
| **E2E** | `tests/e2e/` | End-to-end tests including dotnet parity and SQL Server deploy |

## Running Tests

```bash
# Run all tests (unit, integration, parity, deploy)
# Parity tests skip if dotnet unavailable, deploy tests skip if no SQL Server
just test

# Run a specific test by name
just test-one test_name

# Full CI check (fmt, lint, tests)
just ci
```

## Parity Testing with DotNet DacFx

A key goal of rust-sqlpackage is to produce dacpac files that are **byte-for-byte compatible** with those produced by the official Microsoft DotNet DacFx toolchain. To verify this, we use a three-layer comparison approach.

### Layer 1: Element Inventory

Verifies that all database elements exist with correct names and types.

**What it checks:**
- Same number of tables, views, procedures, etc.
- All element names match exactly
- No missing or extra elements

**Error example:**
```
MISSING in Rust: SqlTable [dbo].[Users]
EXTRA in Rust: SqlTable [dbo].[User]
```

### Layer 2: Property Comparison

Verifies that element properties match between Rust and DotNet output.

**What it checks:**
- Column nullability, data types, lengths
- Index properties (unique, clustered)
- Constraint definitions
- All metadata properties

**Error example:**
```
PROPERTY MISMATCH: SqlSimpleColumn.[dbo].[Users].[Id]
  Property: IsNullable
  Rust: None
  DotNet: Some("False")
```

### Layer 3: SqlPackage DeployReport

Uses the official SqlPackage tool to generate a deployment script comparing the two dacpacs. If the dacpacs are truly equivalent, SqlPackage should report no schema differences.

**What it checks:**
- Actual deployment equivalence
- Catches subtle differences that XML comparison might miss
- The ultimate test of compatibility

**Error example:**
```
SqlPackage detected schema differences:
ALTER TABLE [dbo].[Users] ADD CONSTRAINT [DF_Users_Active] DEFAULT ((1)) FOR [Active];
```

### Running Parity Tests

Parity tests require:
- **DotNet SDK 8.0+** with Microsoft.Build.Sql templates
- **SqlPackage CLI** (for Layer 3 tests)

```bash
# Install prerequisites (one-time setup)
dotnet new install Microsoft.Build.Sql.Templates
dotnet tool install -g microsoft.sqlpackage

# Run all tests (includes parity)
just test

# Run parity tests only
cargo test --test e2e_tests dotnet_comparison -- --nocapture

# Run with a custom SQL project
SQL_TEST_PROJECT=/path/to/YourProject.sqlproj cargo test --test e2e_tests dotnet_comparison -- --nocapture
```

When running locally **without dotnet installed**, parity tests will skip gracefully (not fail). In CI, dotnet is always available, so tests run and must pass.

## Test Fixtures

Test fixtures in `tests/fixtures/` are self-contained SQL projects used for testing specific features:

| Fixture | Purpose |
|---------|---------|
| `e2e_comprehensive/` | Main fixture for parity testing (covers most features) |
| `extended_properties/` | Tests sp_addextendedproperty support |
| `fulltext_index/` | Tests FULLTEXT INDEX support |
| `table_types/` | Tests CREATE TYPE AS TABLE support |
| `ampersand_encoding/` | Tests & in identifiers |
| `index_naming/` | Tests index name format |
| `default_constraints_named/` | Tests CONSTRAINT [name] DEFAULT |
| `inline_constraints/` | Tests inline UNIQUE/CHECK constraints |
| `procedure_parameters/` | Tests parameter capture (including OUTPUT) |
| `sqlcmd_variables/` | Tests SqlCmdVariable in project |
| `header_section/` | Tests Header generation with options |
| `database_options/` | Tests SqlDatabaseOptions element |
| `commaless_constraints/` | Tests constraints without comma separators (known failing - see Known Limitations) |

### Failing Fixtures (TDD)

These fixtures have failing tests that document known issues pending fixes (see Phase 17 in IMPLEMENTATION_PLAN.md):

| Fixture | Test | Issue |
|---------|------|-------|
| `commaless_constraints/` | `test_parity_commaless_constraints` | Constraints without comma separators are not parsed by sqlparser-rs |
| `sqlcmd_variables/` | `test_sqlcmd_variables_header_format` | Header format differs from .NET (separate CustomData vs single with Type attribute) |

### Adding New Fixtures

1. Create a new directory under `tests/fixtures/`
2. Add a `.sqlproj` file (SDK-style recommended)
3. Add SQL files with the schema to test
4. Create corresponding tests in `tests/unit/`, `tests/integration/`, or `tests/e2e/`

## CI Pipeline

GitHub Actions runs two jobs on every push and PR:

### Job 1: build-and-test (Fast Feedback)
- Formatting check (`cargo fmt --check`)
- Linting (`cargo clippy -D warnings`)
- Unit and integration tests (`cargo test`)

### Job 2: parity-tests (Dotnet Comparison)
- Installs DotNet SDK 8.0
- Installs Microsoft.Build.Sql templates
- Installs SqlPackage CLI
- Runs all parity tests with assertions

Both jobs must pass for CI to go green. The parity-tests job depends on build-and-test, so basic issues are caught quickly before the slower parity tests run.

## Intentional Deviations from DotNet DacFx

rust-sqlpackage aims for byte-for-byte compatibility with DotNet DacFx output. However, some differences are **intentional and acceptable**:

### Metadata Differences (Expected)

These fields differ by design and are excluded from parity comparison:

| Field | rust-sqlpackage | DotNet DacFx | Reason |
|-------|-----------------|--------------|--------|
| ProductName | `rust-sqlpackage` | `Microsoft.Data.Tools.Schema.Sql` | Different tool |
| ProductVersion | `0.1.0` | SDK version (e.g., `161.9149.0`) | Different versioning |
| Timestamps | Current build time | Current build time | Always differ |
| Checksums | Computed at build | Computed at build | File-dependent |

### Matched Behaviors (Explicit Implementation)

These behaviors were carefully studied and matched to DotNet:

1. **Script Content Normalization**
   - Both normalize CRLF to LF in script content
   - Location: `normalize_script_content()` in model_xml.rs

2. **IsNullable Property Emission**
   - Only emit `IsNullable="False"` for NOT NULL columns
   - Never emit `IsNullable="True"` (nullable is the default)
   - Never emit IsNullable for `SqlTableTypeSimpleColumn`

3. **IsClustered Property Emission**
   - Primary Key: Only emit when NONCLUSTERED (default is CLUSTERED)
   - Unique: Only emit when CLUSTERED (default is NONCLUSTERED)

4. **IsAnsiNullsOn for Views**
   - Always emit `IsAnsiNullsOn="True"` for all views
   - Modern DotNet DacFx emits this property for all views

5. **ExternalSource="BuiltIns" Attribute**
   - Used for references to built-in schemas (dbo, sys, etc.)
   - Used for references to built-in data types (int, varchar, etc.)
   - Matches DotNet schema format requirements

6. **No Schema Relationship for Triggers**
   - DotNet does not emit a Schema relationship for triggers
   - rust-sqlpackage follows this pattern

### Testing Tolerances

The parity testing framework accepts minor differences that don't affect functionality:

- **MIME Types**: DotNet may use "text/xml" or "application/xml" depending on version
- **Whitespace**: Minor formatting differences in non-semantic positions
- **Element Ordering**: Some ordering variations (tracked in Layer 4 tests)

## Debugging Test Failures

### View detailed output
```bash
cargo test --test e2e_tests dotnet_comparison -- --nocapture
```

### Print element summary
```bash
cargo test --test e2e_tests test_print_element_summary -- --ignored --nocapture
```

### Run individual layer tests
```bash
# Layer 1 only
cargo test --test e2e_tests test_layer1_element_inventory -- --nocapture

# Layer 2 only
cargo test --test e2e_tests test_layer2_property_comparison -- --nocapture

# Layer 3 only
cargo test --test e2e_tests test_layer3_sqlpackage_comparison -- --nocapture
```

### Compare with a specific project
```bash
SQL_TEST_PROJECT=/path/to/MyDatabase.sqlproj cargo test --test e2e_tests -- --nocapture
```
