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

## Phase 2: Expand Property Comparison

Current Layer 2 only compares "key properties" per element type. Expand to compare ALL properties.

- [ ] **2.1 Audit all element types and properties**
  - Review DotNet output for all element types
  - Document every property per element type
  - Update `get_key_properties()` or replace with comprehensive comparison

- [ ] **2.2 Implement `compare_all_properties()` function**
  - Location: `tests/e2e/dacpac_compare.rs`
  - Compare all properties from both Rust and DotNet elements
  - Report any property present in one but not the other

- [ ] **2.3 Add property completeness test**
  - New test: `test_property_completeness`
  - Verify Rust generates same properties as DotNet for each element type
  - Track properties missing vs extra

- [ ] **2.4 Add strict mode flag to comparison**
  ```rust
  pub struct ComparisonOptions {
      pub include_layer3: bool,
      pub strict_properties: bool,    // Compare ALL properties
      pub check_relationships: bool,  // Validate all relationships
      pub check_element_order: bool,  // Validate element ordering
  }
  ```

---

## Phase 3: Add Relationship Comparison

Current parser extracts `children` but doesn't fully compare relationships.

- [ ] **3.1 Extend ModelElement to capture relationships**
  ```rust
  pub struct ModelElement {
      pub element_type: String,
      pub name: Option<String>,
      pub properties: BTreeMap<String, String>,
      pub children: Vec<ModelElement>,
      pub relationships: Vec<Relationship>,  // NEW
  }

  pub struct Relationship {
      pub name: String,
      pub references: Vec<String>,
      pub entries: Vec<ModelElement>,
  }
  ```

- [ ] **3.2 Update XML parser to capture all relationships**
  - Location: `parse_element()` in `dacpac_compare.rs`
  - Capture Relationship `Type` attribute
  - Capture all `Reference` elements with their `Name` attributes
  - Preserve entry ordering

- [ ] **3.3 Implement relationship comparison**
  ```rust
  pub fn compare_relationships(
      rust_elem: &ModelElement,
      dotnet_elem: &ModelElement,
  ) -> Vec<RelationshipError>
  ```

- [ ] **3.4 Add relationship comparison to Layer 2**
  - Integrate into `compare_element_pair()`
  - Report missing/extra/different relationships

---

## Phase 4: Add XML Structure Comparison (Layer 4)

New layer for exact XML structural validation.

- [ ] **4.1 Define Layer 4 error types**
  ```rust
  pub enum Layer4Error {
      ElementOrderMismatch {
          element_type: String,
          rust_position: usize,
          dotnet_position: usize,
      },
      RelationshipMismatch {
          element_name: String,
          relationship_type: String,
          rust_refs: Vec<String>,
          dotnet_refs: Vec<String>,
      },
      MissingRelationship {
          element_name: String,
          relationship_type: String,
      },
  }
  ```

- [ ] **4.2 Implement element ordering comparison**
  - Compare order of elements within Model
  - DotNet has specific ordering rules

- [ ] **4.3 Implement `compare_xml_structure()` function**
  - Full structural comparison
  - Report all structural differences

- [ ] **4.4 Add Layer 4 to comparison pipeline**
  - Integrate into `compare_dacpacs()`
  - Add to comparison report output

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
| Phase 2: Property Comparison | Not Started | 0/4 |
| Phase 3: Relationship Comparison | Not Started | 0/4 |
| Phase 4: XML Structure (Layer 4) | Not Started | 0/4 |
| Phase 5: Metadata Files | Not Started | 0/5 |
| Phase 6: Per-Feature Tests | Not Started | 0/5+ |
| Phase 7: Canonical XML | Not Started | 0/4 |
| Phase 8: Infrastructure | Not Started | 0/4 |

**Overall Progress**: 9/39+ tasks complete

**Note**: Phase 1 was largely pre-implemented. Only item 1.1 (Ampersand truncation) required code changes.

---

## Notes

- Each phase builds on previous phases
- Phase 1 should be completed first as it unblocks meaningful parity testing
- Phases 2-5 can be worked in parallel after Phase 1
- Phase 7 is the final validation that confirms true 1-1 matching
- Update this document as tasks are completed with `[x]` markers
