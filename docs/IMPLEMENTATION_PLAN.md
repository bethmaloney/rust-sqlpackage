# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Completed Phases Summary

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1-9 | Core implementation (properties, relationships, XML structure, metadata) | 58/58 |
| Phase 10 | Fix extended properties, function classification, constraint naming, SqlPackage config | 5/5 |

**Total Completed**: 63/63 tasks

---

## Current Parity Metrics (as of 2026-01-29)

| Layer | Passing | Rate | Notes |
|-------|---------|------|-------|
| Layer 1 (Inventory) | 44/44 | 100% | All fixtures pass |
| Layer 2 (Properties) | 44/44 | 100% | All fixtures pass |
| Relationships | 37/44 | 84.1% | 7 fixtures with relationship differences |
| Layer 4 (Ordering) | 44/44 | 100% | All fixtures pass |
| Metadata | 44/44 | 100% | All fixtures pass |
| **Full Parity** | **37/44** | **84.1%** | 37 fixtures pass all layers |

**Note:** Error fixtures (`external_reference`, `unresolved_reference`) are now excluded from parity testing since DotNet cannot build them. These test Rust's ability to handle edge cases.

### Excluded Fixtures

Two fixtures are excluded from parity testing because DotNet fails to build them:

1. **external_reference** - References an external database via synonym; DotNet fails with SQL71501
2. **unresolved_reference** - View references non-existent table; DotNet fails with SQL71501

These fixtures test Rust's ability to build projects that DotNet cannot handle. They are not bugs - they are intentional edge case tests.

---

## Phase 11: Fix Remaining Parity Failures

> **Status (2026-01-29):** DotNet 8.0.417 is now available. Most issues resolved. Remaining work: Error fixtures investigation and final verification.

### Completed Sections (11.1-11.4, 11.7)

All tasks in sections 11.1 (Layer 1), 11.2 (Layer 2), 11.3 (Relationships), 11.4 (Layer 4 Ordering), and 11.7 (Inline Constraint Handling) have been completed. See git history for details.

---

### 11.5 Error Fixtures

#### 11.5.1 External Reference Fixture
**Fixtures:** `external_reference`
**Status:** RESOLVED - Excluded from parity testing

- [x] **11.5.1.1** Investigate external_reference DotNet build failure
  - DotNet fails with SQL71501: Synonym references external database `[OtherDatabase].[dbo].[SomeTable]`
  - View depends on the synonym, causing cascading unresolved reference error
- [x] **11.5.1.2** Fix or mark as expected failure
  - Excluded from parity testing via `PARITY_EXCLUDED_FIXTURES` constant
  - Fixture remains for testing Rust's ability to handle external references

#### 11.5.2 Unresolved Reference Fixture
**Fixtures:** `unresolved_reference`
**Status:** RESOLVED - Excluded from parity testing

- [x] **11.5.2.1** Investigate unresolved_reference DotNet build failure
  - DotNet fails with SQL71501: View references non-existent table `[dbo].[NonExistentTable]`
  - This is expected - the fixture intentionally tests unresolved references
- [x] **11.5.2.2** Fix or mark as expected failure
  - Excluded from parity testing via `PARITY_EXCLUDED_FIXTURES` constant
  - Fixture remains for testing Rust's lenient reference handling

---

### 11.6 Final Verification: 100% Parity

#### 11.6.1 Complete Verification Checklist
**Goal:** Verify all tests pass, no clippy warnings, and full parity achieved.

- [x] **11.6.1.1** Run `just test` - all unit and integration tests pass
- [x] **11.6.1.2** Run `cargo clippy` - no warnings
  - Fixed: Added SQL Server availability check to `test_e2e_sql_server_connectivity`
  - Fixed: Clippy warnings (regex in loops, collapsible match, doc comments)
- [x] **11.6.1.3** Run parity regression check - 44 fixtures tested (2 excluded)
- [x] **11.6.1.4** Verify Layer 1 (inventory) at 100%
- [x] **11.6.1.5** Verify Layer 2 (properties) at 100%
- [ ] **11.6.1.6** Verify Relationships at 100% (currently 84.1%)
- [x] **11.6.1.7** Verify Layer 4 (ordering) at 100%
- [x] **11.6.1.8** Verify Metadata at 100%
- [x] **11.6.1.9** Document any intentional deviations from DotNet behavior
- [x] **11.6.1.10** Update baseline and confirm no regressions

**Note (2026-01-29):** Baseline updated. Error fixtures excluded from parity testing. Remaining 7 fixtures have relationship differences (not Layer 1-4 or metadata issues).

---

### Phase 11 Progress

| Section | Description | Tasks | Status |
|---------|-------------|-------|--------|
| 11.1 | Layer 1: Element Inventory | 8/8 | Complete |
| 11.2 | Layer 2: Properties | 2/2 | Complete |
| 11.3 | Relationships | 19/19 | Complete |
| 11.4 | Layer 4: Ordering | 3/3 | Complete (100% pass rate) |
| 11.5 | Error Fixtures | 4/4 | Complete (excluded from parity testing) |
| 11.6 | Final Verification | 9/10 | In Progress (relationships remaining) |
| 11.7 | Inline Constraint Handling | 11/11 | Complete |

**Phase 11 Total**: 56/57 tasks complete

> **Status (2026-01-29):** Layer 1, Layer 2, Layer 4, and Metadata all at 100%. Relationships at 84.1% (37/44). Error fixtures resolved by excluding from parity testing.

---

## Verification Commands

```bash
just test                                    # Run all tests
cargo test --test e2e_tests test_parity_regression_check  # Check regressions
PARITY_UPDATE_BASELINE=1 cargo test --test e2e_tests test_parity_regression_check -- --nocapture  # Update baseline

# Test specific fixture
SQL_TEST_PROJECT=tests/fixtures/<name>/project.sqlproj cargo test --test e2e_tests test_layer1 -- --nocapture
SQL_TEST_PROJECT=tests/fixtures/<name>/project.sqlproj cargo test --test e2e_tests test_layer2 -- --nocapture
SQL_TEST_PROJECT=tests/fixtures/<name>/project.sqlproj cargo test --test e2e_tests test_relationship -- --nocapture
```

---

## Overall Progress

| Phase | Status |
|-------|--------|
| Phases 1-10 | **COMPLETE** 63/63 |
| Phase 11 | **IN PROGRESS** 56/57 |

**Total**: 119/120 tasks complete

---

<details>
<summary>Archived: Phases 1-10 Details</summary>

### Phase 1-9 Summary (58 tasks)
- Phase 1: Fix Known High-Priority Issues (ampersand truncation, constraints, etc.)
- Phase 2: Expand Property Comparison (strict mode, all properties)
- Phase 3: Add Relationship Comparison (references, entries)
- Phase 4: Add XML Structure Comparison (element ordering)
- Phase 5: Add Metadata Files Comparison (Content_Types, DacMetadata, Origin, scripts)
- Phase 6: Per-Feature Parity Tests (all 46 fixtures)
- Phase 7: Canonical XML Comparison (byte-level matching)
- Phase 8: Test Infrastructure (modular parity layers, CI metrics, regression detection)
- Phase 9: Achieve 100% Parity (ordering, properties, relationships, metadata, edge cases)

### Phase 10 Summary (5 tasks)
- 10.1 Extended Property Key Format - Add parent type prefix
- 10.2 Function Type Classification - Distinguish inline vs multi-statement TVFs
- 10.3 View IsAnsiNullsOn Property - RESOLVED (was incorrect assumption)
- 10.4 Default Constraint Naming Fix - Fix inline constraint name handling
- 10.5 SqlPackage Test Configuration - Add TargetDatabaseName parameter
- 10.6 View Relationship Parity - RESOLVED (implementation was correct)

</details>
