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

- [ ] **5.1 Implement `[Content_Types].xml` comparison**
  - Extract from both dacpacs
  - Compare MIME type definitions
  - Report differences

- [ ] **5.2 Implement `DacMetadata.xml` comparison**
  - Compare metadata fields
  - Ignore timestamps/version fields that will naturally differ
  - Compare ServerVersion, DacVersion, etc.

- [ ] **5.3 Implement `Origin.xml` comparison**
  - Compare origin fields
  - Ignore timestamps
  - Compare ProductSchema, ProductVersion patterns

- [ ] **5.4 Implement pre/post deploy script comparison**
  - Compare `predeploy.sql` if present
  - Compare `postdeploy.sql` if present
  - Normalize whitespace for comparison

- [ ] **5.5 Create unified metadata comparison function**
  ```rust
  pub fn compare_dacpac_files(
      rust_dacpac: &Path,
      dotnet_dacpac: &Path,
  ) -> Vec<FileComparisonError>
  ```

---

## Phase 6: Per-Feature Parity Tests

Create targeted tests for each fixture and known issue.

- [ ] **6.1 Create parity test helper function**
  ```rust
  fn run_parity_test(fixture_name: &str) -> ComparisonResult {
      // Build both dacpacs for fixture
      // Run full comparison
      // Return result
  }
  ```

- [ ] **6.2 Add tests for high-priority fixtures**
  - [ ] `test_parity_ampersand_encoding`
  - [ ] `test_parity_default_constraints_named`
  - [ ] `test_parity_inline_constraints`

- [ ] **6.3 Add tests for medium-priority fixtures**
  - [ ] `test_parity_database_options`
  - [ ] `test_parity_header_section`

- [ ] **6.4 Add tests for lower-priority fixtures**
  - [ ] `test_parity_extended_properties`
  - [ ] `test_parity_table_types`
  - [ ] `test_parity_sqlcmd_variables`

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
| Phase 5: Metadata Files | Not Started | 0/5 |
| Phase 6: Per-Feature Tests | Not Started | 0/5+ |
| Phase 7: Canonical XML | Not Started | 0/4 |
| Phase 8: Infrastructure | Not Started | 0/4 |

**Overall Progress**: 21/39+ tasks complete

**Note**: Phase 1 was largely pre-implemented. Only item 1.1 (Ampersand truncation) required code changes.
Phase 2 added comprehensive property documentation and strict comparison mode for parity testing.
Phase 3 added relationship parsing and comparison infrastructure with comprehensive error types.
Phase 4 added element ordering infrastructure to compare structural differences in element positions and type ordering.

---

## Notes

- Each phase builds on previous phases
- Phase 1 should be completed first as it unblocks meaningful parity testing
- Phases 2-5 can be worked in parallel after Phase 1
- Phase 7 is the final validation that confirms true 1-1 matching
- Update this document as tasks are completed with `[x]` markers
