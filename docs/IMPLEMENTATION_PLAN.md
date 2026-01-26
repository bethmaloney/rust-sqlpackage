# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Completed Phases Summary

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | Fix Known High-Priority Issues (ampersand truncation, constraints, etc.) | ✓ 9/9 |
| Phase 2 | Expand Property Comparison (strict mode, all properties) | ✓ 4/4 |
| Phase 3 | Add Relationship Comparison (references, entries) | ✓ 4/4 |
| Phase 4 | Add XML Structure Comparison (element ordering) | ✓ 4/4 |
| Phase 5 | Add Metadata Files Comparison (Content_Types, DacMetadata, Origin, scripts) | ✓ 5/5 |
| Phase 6 | Per-Feature Parity Tests (all 46 fixtures) | ✓ 5/5 |
| Phase 7 | Canonical XML Comparison (byte-level matching) | ✓ 4/4 |
| Phase 8 | Test Infrastructure (modular parity layers, CI metrics, regression detection) | ✓ 4/4 |

**Phases 1-8 Complete**: 39/39 tasks

---

## Phase 9: Achieve 100% Parity

Fix the remaining parity issues to achieve near-100% pass rates across all comparison layers.

### Current Parity Metrics (as of 2026-01-26)

| Layer | Passing | Rate | Notes |
|-------|---------|------|-------|
| Layer 1 (Inventory) | 11/46 | 23.9% | |
| Layer 2 (Properties) | 35/46 | 76.1% | |
| Layer 3 (Relationships) | 30/46 | 65.2% | Improved from 28/46 due to CheckExpressionDependencies |
| Layer 4 (Structure) | 5/46 | 10.9% | |
| Layer 5 (Metadata) | 41/46 | 89.1% | |

### 9.1 Deterministic Element Ordering

**Goal:** Fix non-deterministic ordering that causes Layer 1 and Layer 4 failures.

- [x] **9.1.1 Replace HashSet with BTreeSet for schemas** ✓
  - File: `src/model/builder.rs:27`
  - Change `HashSet<String>` to `BTreeSet<String>`
  - Ensures consistent schema ordering across builds
  - Expected impact: 5-10 fixtures

- [x] **9.1.2 Sort elements by type then name** ✓
  - File: `src/model/builder.rs:553-620`
  - Added `sort_elements()` and `element_type_priority()` functions
  - Elements sorted by type priority, then alphabetically by full name
  - **Actual impact**: Layer 2: +1 (14→15), Layer 4: +3 (3→6)
  - **Finding**: DotNet ordering is more complex than expected:
    - Named elements appear to be sorted alphabetically by full name
    - Inline/unnamed constraints (no Name attribute) appear before named elements
    - Further refinement may be needed in 9.5 for full match

### 9.2 Property Value Fixes

**Goal:** Fix property mismatches that cause Layer 2 failures.

- [x] **9.2.0 IsNullable property emission** ✓
  - Files: `src/model/elements.rs`, `src/model/builder.rs`, `src/dacpac/model_xml.rs`, `src/parser/tsql_parser.rs`
  - Changed `is_nullable: bool` to `nullability: Option<bool>` to track explicit vs implicit nullability
  - DotNet only emits `IsNullable` for explicit NULL/NOT NULL, omits for implicit
  - DotNet never emits `IsNullable` for `SqlTableTypeSimpleColumn`
  - **Actual impact**: Layer 2: +1 (15→16), Relationships: +1 (27→28)
  - Fixed fixtures: `table_types` now passes Layer 2

- [x] **9.2.1 Type specifier properties** ✓
  - Files: `src/model/builder.rs`, `src/dacpac/model_xml.rs`
  - Fixed property order in SqlTypeSpecifier: Scale, Precision, Length/IsMax now come BEFORE Type relationship
  - Scale comes before Precision (matching DotNet ordering)
  - Added datetime2/time/datetimeoffset precision extraction using Scale property (not Precision)
  - Default Scale="7" for datetime2, time, datetimeoffset when no explicit precision specified
  - **Key finding**: DotNet uses "Scale" property for fractional seconds precision on datetime types

- [x] **9.2.2 Script content normalization**
  - File: `src/dacpac/model_xml.rs`  ✓
  - Implemented CRLF to LF normalization in `write_script_property()` function
  - Normalize CRLF to LF in BodyScript, QueryScript
  - Ensure consistent whitespace in CDATA sections
  - Expected impact: 5-8 fixtures

- [x] **9.2.3 Boolean property consistency** ✓
  - File: `src/dacpac/model_xml.rs`
  - Audit all boolean properties use "True"/"False" (capitalized)
  - **Finding**: Already correctly implemented - all boolean properties use capitalized "True"/"False"
  - No changes needed

- [x] **9.2.4 Constraint expression properties** ✓
  - Files: `src/model/builder.rs`, `src/dacpac/model_xml.rs`, `src/parser/tsql_parser.rs`
  - CheckExpressionScript and DefaultExpressionScript properties were already working correctly (Layer 2 passes)
  - The investigation revealed the real issue was constraint element naming convention
  - Fixed constraint naming from 3-part ([schema].[table].[constraint]) to 2-part ([schema].[constraint]) to match DotNet
  - Fixed IsClustered property emission to only emit non-default values (PK: only emit False, Unique: only emit True)
  - Added is_clustered extraction from raw SQL for sqlparser path since sqlparser doesn't expose CLUSTERED/NONCLUSTERED

- [x] **9.2.5 IsNullable emission fix** ✓
  - File: `src/dacpac/model_xml.rs`
  - Changed emission logic to only emit `IsNullable="False"` for NOT NULL columns
  - DotNet never emits `IsNullable="True"` for nullable columns (explicit or implicit)
  - **Actual impact**: Layer 2: +16 (16→32 fixtures passing, 34.8%→69.6%)

### 9.3 Relationship Completeness

**Goal:** Fix missing relationships that cause Layer 3/5 failures.

- [x] **9.3.1 Procedure/function dependencies** ✓
  - Files: `src/model/builder.rs`, `src/dacpac/model_xml.rs`, `src/model/elements.rs`
  - Added BodyDependencies relationship for SqlProcedure and SqlScalarFunction/SqlMultiStatementTableValuedFunction/SqlInlineTableValuedFunction
  - Added IsAnsiNullsOn property to procedures
  - Dependencies extracted in order of appearance (tables first, then columns and parameters)
  - Unqualified column names resolved against the first table in FROM clause
  - Built-in types from DECLARE statements extracted for functions
  - **Actual impact**: Layer 2: +3 (32→35 fixtures passing, 69.6%→76.1%)

- [x] **9.3.2 Parameter relationships** ✓
  - File: `src/dacpac/model_xml.rs`
  - Enhanced `FunctionParameter` struct to include `data_type` and `default_value` fields
  - Updated `extract_function_parameters` to extract full parameter details (not just names)
  - Added `write_function_parameters` function to write the Parameters relationship for functions
  - **Actual impact**: Relationships layer now passes for `procedure_parameters` fixture

- [x] **9.3.3 Foreign key relationship ordering** ✓
  - File: `src/dacpac/model_xml.rs` (lines 1824-1899)
  - Reordered foreign key relationships to match DotNet: Columns, DefiningTable, ForeignColumns, ForeignTable
  - Note: Different from documented expected order - DotNet actually outputs: Columns → DefiningTable → ForeignColumns → ForeignTable
  - **Actual impact**: Relationships layer improved from 26/46 (56.5%) to 28/46 (60.9%)

- [x] **9.3.4 CheckExpressionDependencies relationship** ✓
  - File: `src/dacpac/model_xml.rs`
  - Added `extract_check_expression_columns()` function that extracts column references from CHECK constraint expressions
  - Modified `write_constraint()` to emit CheckExpressionDependencies relationship with fully-qualified column references before DefiningTable
  - Column references are formatted as `[schema].[table].[column]`
  - Order matches DotNet: CheckExpressionScript property, CheckExpressionDependencies relationship, DefiningTable relationship
  - **Actual impact**: Relationships layer improved from 28/46 (60.9%) to 30/46 (65.2%)

### 9.4 Metadata File Alignment

**Goal:** Fix metadata differences that cause Layer 5 failures.

- [x] **9.4.1 Origin.xml adjustments** ✓
  - File: `src/dacpac/origin_xml.rs`
  - Added `ModelSchemaVersion` element (value: "2.9") after Checksums, before closing DacOrigin tag
  - Added unit test `test_origin_xml_has_model_schema_version` to verify the element
  - **Finding**: Remaining 5 metadata failures are due to:
    - 2 fixtures (`pre_post_deploy`, `sqlcmd_includes`) - SQLCMD `:r` include expansion differs
    - 1 fixture (`e2e_comprehensive`) - Deploy script differences
    - 2 fixtures (`external_reference`, `unresolved_reference`) - DotNet build failures (expected)
  - **Actual impact**: Improves Origin.xml parity but doesn't change metadata pass rate (comparison logic doesn't validate this field)

- [x] **9.4.2 Update comparison tolerance** ✓
  - File: `tests/e2e/parity/layer6_metadata.rs`
  - Changed comparison logic to NOT compare ProductName and ProductVersion since these are expected to differ between rust-sqlpackage and DotNet
  - These fields identify the tool that generated the dacpac, so differences are expected
  - **Actual impact**: Metadata layer improved from 0/46 (0%) to 41/46 (89.1%)

- [ ] **9.4.3 DacMetadata.xml alignment**
  - File: `src/dacpac/metadata_xml.rs`
  - Omit empty Description element
  - Expected impact: 5-10 fixtures

- [ ] **9.4.4 [Content_Types].xml fixes**
  - File: `src/dacpac/packager.rs`
  - Match MIME types exactly
  - Expected impact: 2-5 fixtures

### 9.5 Edge Cases and Polishing

**Goal:** Fix remaining edge cases for final push to 100%.

- [ ] **9.5.1 View columns (if needed)**
  - Add SqlViewColumn elements for view column definitions
  - Expected impact: 2-3 fixtures

- [ ] **9.5.2 Inline constraint annotation disambiguator**
  - Match DotNet's hashing algorithm if different
  - Expected impact: 2-3 fixtures

- [ ] **9.5.3 Trigger support verification**
  - Verify SqlDmlTrigger properties match DotNet
  - Expected impact: 1-2 fixtures

---

### Phase 9 Progress

| Section | Status | Completion |
|---------|--------|------------|
| 9.1 Deterministic Ordering | COMPLETE | 2/2 |
| 9.2 Property Value Fixes | COMPLETE | 6/6 |
| 9.3 Relationship Completeness | COMPLETE | 4/4 |
| 9.4 Metadata File Alignment | IN PROGRESS | 2/4 |
| 9.5 Edge Cases | PENDING | 0/3 |

**Phase 9 Overall**: 14/19 tasks

### Expected Outcomes

| Phase | Layer 1 | Layer 2 | Layer 3 | Layer 4 | Layer 5 |
|-------|---------|---------|---------|---------|---------|
| Current | 6.5% | 30.4% | 58.7% | 6.5% | 2.2% |
| After 9.1 | 40%+ | 35%+ | 60%+ | 40%+ | 2% |
| After 9.2 | 45%+ | 70%+ | 65%+ | 45%+ | 2% |
| After 9.3 | 50%+ | 75%+ | 85%+ | 50%+ | 2% |
| After 9.4 | 50%+ | 75%+ | 85%+ | 50%+ | 85%+ |
| After 9.5 | 90%+ | 90%+ | 95%+ | 90%+ | 90%+ |

### Verification Commands

```bash
just test                                    # Run all tests
cargo test --test e2e_tests test_parity_regression_check  # Check regressions
PARITY_UPDATE_BASELINE=1 cargo test --test e2e_tests test_parity_regression_check -- --nocapture  # Update baseline
cargo test --test e2e_tests test_parity_metrics_collection -- --nocapture  # Check metrics
```

---

## Overall Progress

| Phase | Status |
|-------|--------|
| Phases 1-8 | **COMPLETE** ✓ 39/39 |
| Phase 9 | **IN PROGRESS** 14/19 |

**Total**: 53/58 tasks complete
