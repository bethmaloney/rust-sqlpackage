# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Current State

The test infrastructure uses a 3-layer comparison approach:
- **Layer 1**: Element inventory (names and types)
- **Layer 2**: Key property comparison (subset of properties)
- **Layer 3**: SqlPackage deployment equivalence

**Gap**: Tests verify semantic equivalence but not exact structural/binary matching.

---

## Phase 1: Fix Known High-Priority Issues

These issues are documented in TESTING.md and block exact matching.

### High Priority

- [x] **1.1 Ampersand truncation in procedure names** ✓ FIXED
  - Issue: Procedure names containing `&` were truncated (now fixed)
  - Root cause: Regex patterns used `\w+` which doesn't match special chars like `&`
  - Fix: Updated all name extraction regexes to use `[^\]]+` for bracketed identifiers
  - Files changed: `src/parser/tsql_parser.rs` (7 functions fixed)
  - Test added: `test_ampersand_in_function_name` in unit tests
  - Acceptance: Names like `GetP&L_Report` preserved correctly ✓

- [x] **1.2 Named inline default constraints** ✓ ALREADY IMPLEMENTED
  - Issue: `CONSTRAINT [DF_Name] DEFAULT (value)` syntax was already supported
  - Implementation: Model builder extracts from sqlparser's `ColumnOption::Default` with names
  - Unnamed defaults auto-generate names like `DF_TableName_ColumnName`
  - Test fixture: `default_constraints_named/` (all tests pass)
  - Acceptance: Named default constraints appear as separate `SqlDefaultConstraint` elements ✓

- [x] **1.3 Inline CHECK constraints** ✓ ALREADY IMPLEMENTED
  - Issue: Inline CHECK constraints were already being captured
  - Implementation: Model builder extracts from sqlparser's `ColumnOption::Check`
  - Fallback parser also extracts via regex from column definitions
  - Test fixture: `inline_constraints/` (all tests pass)
  - Acceptance: Inline CHECK constraints appear as `SqlCheckConstraint` elements ✓

### Medium Priority

- [x] **1.4 SqlDatabaseOptions element** ✓ ALREADY IMPLEMENTED
  - Verified: model.xml contains `<Element Type="SqlDatabaseOptions">` with properties
  - Properties include: IsAnsiNullDefaultOn, IsAnsiNullsOn, IsArithAbortOn, etc.
  - Acceptance: `SqlDatabaseOptions` element present with correct properties ✓

- [x] **1.5 Header section generation** ✓ ALREADY IMPLEMENTED
  - Verified: model.xml contains Metadata elements for AnsiNulls, QuotedIdentifier, CompatibilityMode
  - Example: `<Metadata Name="AnsiNulls" Value="True"/>`, etc.
  - Acceptance: Header element with correct SET options ✓

- [x] **1.6 SqlInlineConstraintAnnotation** ✓ ALREADY IMPLEMENTED
  - Implementation: Columns with inline constraints get hash-based disambiguator values
  - XML output: `<Annotation Type="SqlInlineConstraintAnnotation" Disambiguator="123456"/>`
  - Acceptance: Annotation elements linking columns to their inline constraints ✓

### Lower Priority

- [x] **1.7 SqlExtendedProperty** ✓ ALREADY IMPLEMENTED
  - Implementation: Parser extracts `sp_addextendedproperty` via regex
  - Supports both table-level and column-level extended properties
  - Test fixture: `extended_properties/` (all tests pass)
  - Acceptance: Extended properties appear as `SqlExtendedProperty` elements ✓

- [x] **1.8 SqlTableType columns** ✓ ALREADY IMPLEMENTED
  - Implementation: Table types parsed with columns and constraints
  - Test fixture: `table_types/` (all tests pass)
  - Acceptance: Table type columns have required properties ✓

- [x] **1.9 SqlCmdVariables** ✓ ALREADY IMPLEMENTED
  - Implementation: SQLCMD `:r` includes and `:setvar` directives supported
  - Test fixture: `sqlcmd_variables/` (all tests pass)
  - Acceptance: SQLCMD include expansion and variable substitution working ✓

---

## Phase 2: Expand Property Comparison ✓ COMPLETE

Current Layer 2 only compares "key properties" per element type. Expand to compare ALL properties.

- [x] **2.1 Audit all element types and properties** ✓ COMPLETE
  - Reviewed DotNet output for all 20+ element types
  - Documented every property per element type in `get_all_properties()` docstring
  - Added `get_all_properties()` function alongside existing `get_key_properties()`
  - Element types documented: SqlDatabaseOptions, SqlTable, SqlSimpleColumn, SqlComputedColumn,
    SqlTypeSpecifier, SqlIndex, SqlIndexedColumnSpecification, SqlPrimaryKeyConstraint,
    SqlUniqueConstraint, SqlForeignKeyConstraint, SqlCheckConstraint, SqlDefaultConstraint,
    SqlProcedure, SqlScalarFunction, SqlMultiStatementTableValuedFunction, SqlView,
    SqlSubroutineParameter, SqlExtendedProperty, SqlSequence, SqlTableType, SqlTableTypeSimpleColumn

- [x] **2.2 Implement `compare_all_properties()` function** ✓ COMPLETE
  - Location: `tests/e2e/dacpac_compare.rs:545-562`
  - Uses `compare_element_pair_strict()` which calls `get_all_properties()`
  - Compares all documented properties for each element type
  - Returns Layer2Error for each mismatch

- [x] **2.3 Add property completeness test** ✓ COMPLETE
  - Added test: `test_property_completeness` in `dotnet_comparison_tests.rs:347-407`
  - Groups mismatches by element type for readability
  - Informational test (doesn't fail) to track parity progress
  - Also added `test_strict_comparison_options` to test the options struct

- [x] **2.4 Add strict mode flag to comparison** ✓ COMPLETE
  - Added `ComparisonOptions` struct in `dacpac_compare.rs:74-85`
  - Added `compare_dacpacs_with_options()` function that uses options
  - Original `compare_dacpacs()` preserved for backward compatibility
  - Options include: `include_layer3`, `strict_properties`, `check_relationships`, `check_element_order`
  - `check_relationships` and `check_element_order` are placeholders for Phases 3 & 4

---

## Phase 3: Add Relationship Comparison ✓ COMPLETE

Current parser extracts `children` but doesn't fully compare relationships.

- [x] **3.1 Extend ModelElement to capture relationships** ✓ COMPLETE
  - Extended `ModelElement` struct with `relationships: Vec<Relationship>` field
  - Added `Relationship` struct with `name`, `references: Vec<ReferenceEntry>`, and `entries: Vec<ModelElement>`
  - Added `ReferenceEntry` struct to capture `Name` and optional `ExternalSource` attributes
  - Location: `tests/e2e/dacpac_compare.rs:22-50`

- [x] **3.2 Update XML parser to capture all relationships** ✓ COMPLETE
  - Added `parse_relationship()` function that captures Relationship `Name` attribute
  - Captures all `Reference` elements with `Name` and `ExternalSource` attributes
  - Captures nested `Entry` elements as `ModelElement` children
  - Preserves entry ordering
  - Location: `tests/e2e/dacpac_compare.rs:228-258`

- [x] **3.3 Implement relationship comparison** ✓ COMPLETE
  - Implemented `compare_relationships()` function comparing two vectors of relationships
  - Implemented `compare_element_relationships()` for element-level comparison
  - Added `RelationshipError` enum with 5 variants:
    - `MissingRelationship` - relationship exists in DotNet but not Rust
    - `ExtraRelationship` - relationship exists in Rust but not DotNet
    - `ReferenceCountMismatch` - different number of references
    - `ReferenceMismatch` - reference name or external source differs
    - `EntryCountMismatch` - different number of entries in relationship
  - Location: `tests/e2e/dacpac_compare.rs:97-136` (enum), `tests/e2e/dacpac_compare.rs:668-827` (functions)

- [x] **3.4 Add relationship comparison to Layer 2** ✓ COMPLETE
  - Added `relationship_errors` field to `ComparisonResult`
  - Updated `compare_dacpacs_with_options()` to use `check_relationships` option
  - Added `Display` impl for `RelationshipError` for readable error output
  - Location: `tests/e2e/dacpac_compare.rs:1024-1089` (Display impl)
  - Tests added:
    - `test_relationship_comparison` - informational test comparing relationships between Rust/DotNet
    - `test_relationship_comparison_options` - tests ComparisonOptions with `check_relationships=true`
  - Location: `tests/e2e/dotnet_comparison_tests.rs:681-825`

---

## Phase 4: Add XML Structure Comparison (Layer 4) ✓ COMPLETE

New layer for exact XML structural validation.

- [x] **4.1 Define Layer 4 error types** ✓ COMPLETE
  - Added `Layer4Error` enum with two variants:
    - `ElementOrderMismatch` - tracks element name, type, and positions in both Rust and DotNet
    - `TypeOrderMismatch` - tracks first occurrence positions of element types across outputs
  - Location: `tests/e2e/dacpac_compare.rs:138-172`

- [x] **4.2 Implement element ordering comparison** ✓ COMPLETE
  - Implemented `compare_element_order()` function that compares element positions
  - Implemented `build_element_position_map()` to create maps of element positions by key
  - Implemented `compare_type_ordering()` to compare ordering of element types
  - Implemented `find_type_first_positions()` to find first occurrence of each type
  - Compares both individual element positions and overall type ordering
  - Location: `tests/e2e/dacpac_compare.rs:866-978`

- [x] **4.3 Implement `compare_element_order()` function** ✓ COMPLETE
  - Implemented as part of 4.2 with full structural comparison of element positions
  - Compares element ordering by building position maps and finding mismatches
  - Also compares type ordering to detect structural differences in type grouping

- [x] **4.4 Add Layer 4 to comparison pipeline** ✓ COMPLETE
  - Added `layer4_errors` field to `ComparisonResult`
  - Integrated `check_element_order` option into `compare_dacpacs_with_options()`
  - Added `Display` impl for `Layer4Error` for readable error output
  - Updated `print_report()` to display Layer 4 errors
  - Tests added:
    - `test_element_order_comparison` - informational test comparing element ordering between Rust/DotNet
    - `test_element_order_comparison_options` - tests ComparisonOptions with `check_element_order=true`
  - Location: `tests/e2e/dotnet_comparison_tests.rs:830-971`

---

## Phase 5: Add Metadata Files Comparison

Extend comparison beyond model.xml to all dacpac files.

- [x] **5.1 Implement `[Content_Types].xml` comparison** ✓ COMPLETE
  - Added `MetadataFileError` enum with variants for content type mismatches, count mismatches, and missing files
  - Added `ContentTypesXml` struct to parse and represent [Content_Types].xml structure
  - Implemented `extract_content_types_xml()` to extract [Content_Types].xml from dacpac ZIP
  - Implemented `ContentTypesXml::from_xml()` to parse Default and Override elements
  - Implemented `compare_content_types()` to compare MIME type definitions between dacpacs
  - Added `check_metadata_files` option to `ComparisonOptions`
  - Added `metadata_errors` field to `ComparisonResult`
  - Updated `is_success()` and `print_report()` to include metadata errors
  - Tests added:
    - `test_content_types_comparison` - informational test comparing [Content_Types].xml between Rust/DotNet
    - `test_content_types_comparison_options` - tests ComparisonOptions with `check_metadata_files=true`
    - `test_content_types_xml_parsing` - unit test for XML parsing logic
    - `test_extract_content_types_from_dacpac` - test extraction from Rust-generated dacpac
  - Location: `tests/e2e/dacpac_compare.rs:176-211` (types), `tests/e2e/dacpac_compare.rs:1019-1156` (functions)
  - Acceptance: [Content_Types].xml comparison reports MIME type differences ✓

- [x] **5.2 Implement `DacMetadata.xml` comparison** ✓ COMPLETE
  - Added `DacMetadataXml` struct to represent parsed DacMetadata.xml with name, version, description fields
  - Added `DacMetadataMismatch` variant to `MetadataFileError` enum for field mismatches
  - Added `extract_dac_metadata_xml()` function to extract DacMetadata.xml from dacpac ZIP
  - Implemented `DacMetadataXml::from_xml()` to parse DacMetadata.xml elements
  - Implemented `compare_dac_metadata()` to compare metadata between Rust and DotNet dacpacs
  - Added Display impl for `DacMetadataMismatch` error variant
  - Integrated into `compare_dacpacs_with_options()` when `check_metadata_files` is enabled
  - Updated `print_report()` section title to include DacMetadata.xml
  - Tests added:
    - `test_dac_metadata_comparison` - informational test comparing DacMetadata.xml between Rust/DotNet
    - `test_dac_metadata_xml_parsing` - unit test for XML parsing logic
    - `test_extract_dac_metadata_from_dacpac` - test extraction from Rust-generated dacpac
    - `test_metadata_comparison_includes_dac_metadata` - tests ComparisonOptions with both Content_Types and DacMetadata
  - Location: `tests/e2e/dacpac_compare.rs:204-230` (types), `tests/e2e/dacpac_compare.rs:1182-1250` (extraction/parsing), `tests/e2e/dacpac_compare.rs:1262-1317` (comparison)
  - Acceptance: DacMetadata.xml comparison reports field differences ✓

- [x] **5.3 Implement `Origin.xml` comparison** ✓ COMPLETE
  - Added `OriginXml` struct to represent parsed Origin.xml with fields:
    - package_version, contains_exported_data
    - data_stream_version, deployment_contributors_version
    - product_name, product_version, product_schema
  - Added `OriginXmlMismatch` variant to `MetadataFileError` enum for field mismatches
  - Added `extract_origin_xml()` function to extract Origin.xml from dacpac ZIP
  - Implemented `OriginXml::from_xml()` and `OriginXml::from_dacpac()` parsing methods
  - Implemented `compare_origin_xml()` to compare between Rust and DotNet dacpacs:
    - PackageProperties/Version
    - ContainsExportedData
    - StreamVersions (Data, DeploymentContributors)
    - Operation/ProductName, ProductVersion, ProductSchema
    - Ignores timestamps (Start/End) and Checksums as they always differ
  - Added Display impl for `OriginXmlMismatch` error variant
  - Integrated into `compare_dacpacs_with_options()` when `check_metadata_files` is enabled
  - Updated `print_report()` section title to include Origin.xml
  - Tests added:
    - `test_origin_xml_comparison` - informational test comparing Origin.xml between Rust/DotNet
    - `test_origin_xml_parsing` - unit test for XML parsing logic
    - `test_extract_origin_xml_from_dacpac` - test extraction from Rust-generated dacpac
    - Updated `test_metadata_comparison_includes_dac_metadata` to count Origin.xml errors
  - Acceptance: Origin.xml comparison reports field differences (ignoring timestamps/checksums) ✓

- [x] **5.4 Implement pre/post deploy script comparison** ✓ COMPLETE
  - Added `DeployScriptMismatch` and `DeployScriptMissing` variants to `MetadataFileError` enum
  - Added `extract_deploy_script()` function to extract predeploy.sql/postdeploy.sql from dacpac ZIP
  - Added `normalize_script_whitespace()` function with rules:
    - Convert CRLF to LF (Windows to Unix line endings)
    - Trim trailing whitespace from each line
    - Remove trailing empty lines
    - Preserve leading whitespace and internal blank lines
  - Added `compare_deploy_scripts()` and `compare_single_deploy_script()` functions
  - Added `check_deploy_scripts` option to `ComparisonOptions` struct
  - Integrated into `compare_dacpacs_with_options()` when option enabled
  - Added Display impl for new error variants
  - Updated `print_report()` section title to include deploy scripts
  - Tests added:
    - `test_deploy_script_comparison` - informational test comparing scripts between Rust/DotNet
    - `test_extract_deploy_scripts_from_dacpac` - test extraction from Rust-generated dacpac
    - `test_script_whitespace_normalization` - unit test for whitespace normalization
    - `test_deploy_script_comparison_options` - tests ComparisonOptions with `check_deploy_scripts=true`
    - `test_deploy_script_comparison_no_scripts` - tests that dacpacs without scripts don't generate errors
  - Location: `tests/e2e/dacpac_compare.rs:1582-1732` (functions), `tests/e2e/dotnet_comparison_tests.rs:1596-1863` (tests)
  - Acceptance: Pre/post-deploy script comparison reports content differences (with whitespace normalization) ✓

- [x] **5.5 Create unified metadata comparison function** ✓ COMPLETE
  - Added `compare_dacpac_files()` function to consolidate all Phase 5 metadata comparisons
  - Location: `tests/e2e/dacpac_compare.rs:1776-1792`
  - Unified function aggregates:
    - Phase 5.1: [Content_Types].xml comparison via `compare_content_types()`
    - Phase 5.2: DacMetadata.xml comparison via `compare_dac_metadata()`
    - Phase 5.3: Origin.xml comparison via `compare_origin_xml()`
    - Phase 5.4: Pre/post-deploy script comparison via `compare_deploy_scripts()`
  - Updated `compare_dacpacs_with_options()` to use unified function when both
    `check_metadata_files` and `check_deploy_scripts` are enabled
  - Tests added:
    - `test_unified_metadata_comparison` - Main informational test
    - `test_unified_metadata_consistency` - Verifies unified function returns same results as individual calls
    - `test_unified_metadata_via_options` - Verifies ComparisonOptions integration
  - Acceptance: Unified `compare_dacpac_files()` aggregates all Phase 5 comparisons ✓

---

## Phase 6: Per-Feature Parity Tests

Create targeted tests for each fixture and known issue.

- [x] **6.1 Create parity test helper function** ✓ COMPLETE
  - Added `run_parity_test(fixture_name, options)` function in `dotnet_comparison_tests.rs:219-293`
  - Returns `Result<ComparisonResult, ParityTestError>` for ergonomic error handling
  - Added `ParityTestOptions` struct with configurable comparison options
  - Added `ParityTestError` enum for detailed error reporting
  - Added `run_parity_test_with_report()` convenience function that prints results
  - Added `get_available_fixtures()` to list all testable fixtures
  - Tests added:
    - `test_run_parity_test_simple_table` - Basic function test
    - `test_run_parity_test_invalid_fixture` - Error handling test
    - `test_parity_test_options_default` - Default options test
    - `test_parity_test_options_minimal` - Minimal options test
    - `test_get_available_fixtures` - Fixture discovery test
    - `test_run_parity_test_with_report` - Report function test
  - Acceptance: Helper function works with any fixture and returns comparison results ✓

- [x] **6.2 Add tests for high-priority fixtures** ✓ COMPLETE
  - [x] `test_parity_ampersand_encoding` - Tests Phase 1.1 fix
  - [x] `test_parity_default_constraints_named` - Tests Phase 1.2 implementation
  - [x] `test_parity_inline_constraints` - Tests Phase 1.3 implementation
  - Note: These are informational tests that use `run_parity_test()` helper
  - Location: `tests/e2e/dotnet_comparison_tests.rs:2549-2651`

- [x] **6.3 Add tests for medium-priority fixtures** ✓ COMPLETE
  - [x] `test_parity_database_options` - Tests Phase 1.4 (SqlDatabaseOptions element)
  - [x] `test_parity_header_section` - Tests Phase 1.5 (Header section generation)
  - Note: These tests use `ParityTestOptions::default()` for full comparison including property validation
  - Location: `tests/e2e/dotnet_comparison_tests.rs:2688-2817`

- [x] **6.4 Add tests for lower-priority fixtures** ✓ COMPLETE
  - [x] `test_parity_extended_properties` - Tests Phase 1.7 (SqlExtendedProperty element generation)
  - [x] `test_parity_table_types` - Tests Phase 1.8 (SqlTableType columns and structure)
  - [x] `test_parity_sqlcmd_variables` - Tests Phase 1.9 (SqlCmdVariables element generation)
  - Note: These tests use `ParityTestOptions::default()` for full comparison
  - Location: `tests/e2e/dotnet_comparison_tests.rs:2819-3034`
  - Key findings from test output:
    - Extended properties: Rust uses different naming format than DotNet (missing element type prefix like `[SqlColumn]`)
    - Table types: IsNullable property explicitly set in Rust but omitted in DotNet
    - Procedures: Relationship entries (BodyDependencies, DynamicObjects, Parameters) not emitted by Rust

- [ ] **6.5 Add tests for all remaining fixtures**
  - [ ] `test_parity_e2e_comprehensive`
  - [ ] `test_parity_e2e_simple`
  - [ ] `test_parity_fulltext_index`
  - [ ] `test_parity_procedure_parameters`
  - [ ] `test_parity_index_naming`
  - [ ] (Add remaining 35+ fixtures)

---

## Phase 7: Canonical XML Comparison

Final validation layer for true byte-level matching.

- [ ] **7.1 Implement XML canonicalization**
  ```rust
  fn canonicalize_model_xml(xml: &str) -> String {
      // Parse XML
      // Sort elements by (Type, Name)
      // Sort properties by Name
      // Sort relationships by Type
      // Normalize whitespace
      // Re-serialize
  }
  ```

- [ ] **7.2 Add canonical comparison test**
  ```rust
  #[test]
  fn test_canonical_xml_match() {
      let rust_canonical = canonicalize_model_xml(&rust_xml);
      let dotnet_canonical = canonicalize_model_xml(&dotnet_xml);
      assert_eq!(rust_canonical, dotnet_canonical);
  }
  ```

- [ ] **7.3 Add diff output for canonical failures**
  - Show line-by-line diff on failure
  - Highlight specific differences

- [ ] **7.4 Add SHA256 checksum comparison**
  - Optional final validation
  - Compare checksums of canonicalized XML

---

## Phase 8: Test Infrastructure Improvements

Reorganize and improve test infrastructure.

- [ ] **8.1 Reorganize parity test files**
  ```
  tests/e2e/
  ├── dotnet_comparison_tests.rs     # Main orchestration
  ├── dacpac_compare.rs              # Comparison infrastructure
  ├── parity/
  │   ├── mod.rs
  │   ├── layer1_inventory.rs
  │   ├── layer2_properties.rs
  │   ├── layer3_sqlpackage.rs
  │   ├── layer4_structure.rs
  │   ├── layer5_relationships.rs
  │   └── layer6_metadata.rs
  ```

- [ ] **8.2 Add comparison progress tracking to CI**
  - Track number of passing parity tests over time
  - Report comparison metrics in CI output

- [ ] **8.3 Add comparison report generation**
  - Generate HTML/Markdown report of all differences
  - Save as CI artifact

- [ ] **8.4 Add regression detection**
  - Fail CI if previously passing parity tests now fail
  - Track known failures vs new failures

---

## Progress Tracking

| Phase | Status | Completion |
|-------|--------|------------|
| Phase 1: High-Priority Issues | **COMPLETE** | 9/9 ✓ |
| Phase 2: Property Comparison | **COMPLETE** | 4/4 ✓ |
| Phase 3: Relationship Comparison | **COMPLETE** | 4/4 ✓ |
| Phase 4: XML Structure (Layer 4) | **COMPLETE** | 4/4 ✓ |
| Phase 5: Metadata Files | **COMPLETE** | 5/5 ✓ |
| Phase 6: Per-Feature Tests | In Progress | 4/5 ✓ |
| Phase 7: Canonical XML | Not Started | 0/4 |
| Phase 8: Infrastructure | Not Started | 0/4 |

**Overall Progress**: 30/39+ tasks complete

**Note**: Phase 1 was largely pre-implemented. Only item 1.1 (Ampersand truncation) required code changes.
Phase 2 added comprehensive property documentation and strict comparison mode for parity testing.
Phase 3 added relationship parsing and comparison infrastructure with comprehensive error types.
Phase 4 added element ordering infrastructure to compare structural differences in element positions and type ordering.
Phase 5 started with [Content_Types].xml comparison. Implemented extraction, parsing, and comparison infrastructure for metadata files.
Phase 5.4 added pre/post-deploy script comparison with whitespace normalization for parity testing.
Phase 5.5 completed the metadata comparison infrastructure with a unified `compare_dacpac_files()` function that consolidates all metadata comparisons into a single entry point.
Phase 6.1 added the `run_parity_test()` helper function infrastructure enabling per-fixture parity tests. Phase 6.2 added the high-priority fixture tests using the new helper.
Phase 6.3 added medium-priority fixture tests for `database_options` and `header_section` fixtures, validating SqlDatabaseOptions element generation and Header section metadata.
Phase 6.4 added lower-priority fixture tests for `extended_properties`, `table_types`, and `sqlcmd_variables` fixtures. Key parity gaps identified: extended property naming format differences, IsNullable property handling for table type columns, and missing procedure relationship entries.

---

## Notes

- Each phase builds on previous phases
- Phase 1 should be completed first as it unblocks meaningful parity testing
- Phases 2-5 can be worked in parallel after Phase 1
- Phase 7 is the final validation that confirms true 1-1 matching
- Update this document as tasks are completed with `[x]` markers
