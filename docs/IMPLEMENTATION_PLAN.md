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

| Layer | Passing | Rate |
|-------|---------|------|
| Layer 1 (Inventory) | 3/46 | 6.5% |
| Layer 2 (Properties) | 14/46 | 30.4% |
| Layer 3 (Relationships) | 27/46 | 58.7% |
| Layer 4 (Structure) | 3/46 | 6.5% |
| Layer 5 (Metadata) | 1/46 | 2.2% |

### 9.1 Deterministic Element Ordering

**Goal:** Fix non-deterministic ordering that causes Layer 1 and Layer 4 failures.

- [ ] **9.1.1 Replace HashSet with BTreeSet for schemas**
  - File: `src/model/builder.rs:27`
  - Change `HashSet<String>` to `BTreeSet<String>`
  - Ensures consistent schema ordering across builds
  - Expected impact: 5-10 fixtures

- [ ] **9.1.2 Sort elements by type then name**
  - File: `src/model/builder.rs` (end of `build_model()`)
  - Add sorting logic before returning model
  - DotNet element type order:
    1. SqlDatabaseOptions
    2. SqlSchema
    3. SqlTable (with nested columns)
    4. SqlView
    5. SqlProcedure
    6. SqlScalarFunction / SqlTableValuedFunction
    7. SqlIndex
    8. SqlPrimaryKeyConstraint
    9. SqlForeignKeyConstraint
    10. SqlUniqueConstraint
    11. SqlCheckConstraint
    12. SqlDefaultConstraint
    13. SqlSequence
    14. SqlTableType
    15. SqlExtendedProperty
  - Expected impact: 15-20 fixtures across Layer 1 and Layer 4

### 9.2 Property Value Fixes

**Goal:** Fix property mismatches that cause Layer 2 failures.

- [ ] **9.2.1 Type specifier properties**
  - File: `src/dacpac/model_xml.rs`
  - Ensure Length, Precision, Scale, IsMax output correctly
  - Handle datetime2(7), time(7), datetimeoffset(7) precision
  - Expected impact: 5-10 fixtures

- [ ] **9.2.2 Script content normalization**
  - File: `src/dacpac/model_xml.rs`
  - Normalize CRLF to LF in BodyScript, QueryScript
  - Ensure consistent whitespace in CDATA sections
  - Expected impact: 5-8 fixtures

- [ ] **9.2.3 Boolean property consistency**
  - File: `src/dacpac/model_xml.rs`
  - Audit all boolean properties use "True"/"False" (capitalized)
  - Expected impact: 3-5 fixtures

- [ ] **9.2.4 Constraint expression properties**
  - File: `src/dacpac/model_xml.rs`
  - Verify `CheckExpressionScript` and `DefaultExpressionScript`
  - Expected impact: 3-5 fixtures

### 9.3 Relationship Completeness

**Goal:** Fix missing relationships that cause Layer 3/5 failures.

- [ ] **9.3.1 Procedure/function dependencies**
  - File: `src/dacpac/model_xml.rs`
  - Add BodyDependencies relationship for procedures
  - Expected impact: 5-8 fixtures

- [ ] **9.3.2 Parameter relationships**
  - File: `src/dacpac/model_xml.rs`
  - Ensure SqlSubroutineParameter has correct TypeSpecifier relationship
  - Expected impact: 3-5 fixtures

- [ ] **9.3.3 Foreign key relationship ordering**
  - File: `src/dacpac/model_xml.rs`
  - Order: DefiningTable, Columns, ForeignTable, ForeignColumns
  - Expected impact: 2-4 fixtures

### 9.4 Metadata File Alignment

**Goal:** Fix metadata differences that cause Layer 5 failures.

- [ ] **9.4.1 Origin.xml adjustments**
  - File: `src/dacpac/origin_xml.rs`
  - Match StreamVersions format
  - Expected impact: 40+ fixtures

- [ ] **9.4.2 Update comparison tolerance**
  - File: `tests/e2e/parity/layer6_metadata.rs`
  - Allow ProductName/ProductVersion to differ (expected)
  - Expected impact: 40+ fixtures

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
| 9.1 Deterministic Ordering | PENDING | 0/2 |
| 9.2 Property Value Fixes | PENDING | 0/4 |
| 9.3 Relationship Completeness | PENDING | 0/3 |
| 9.4 Metadata File Alignment | PENDING | 0/4 |
| 9.5 Edge Cases | PENDING | 0/3 |

**Phase 9 Overall**: 0/16 tasks

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
| Phase 9 | **IN PROGRESS** 0/16 |

**Total**: 39/55 tasks complete
