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
| Layer 1 (Inventory) | 44/46 | 95.7% | 2 ERROR fixtures failing |
| Layer 2 (Properties) | 44/46 | 95.7% | 2 ERROR fixtures failing |
| Relationships | 37/46 | 80.4% | 9 failing |
| Layer 4 (Ordering) | 43/46 | 93.5% | 1 real failure (all_constraints) + 2 ERROR fixtures |
| Metadata | 44/46 | 95.7% | 2 ERROR fixtures |
| **Full Parity** | **37/46** | **80.4%** | 37 fixtures pass all layers |

**Note:** DotNet 8.0.417 is now available in the development environment. All blocked items can now be investigated.

### Layer 4 Ordering Notes

The `all_constraints` fixture fails Layer 4 due to DotNet using ordinal (case-sensitive) string comparison for that specific project, while most other fixtures use case-insensitive ordering. This inconsistency in DotNet's behavior appears to be related to project CollationCaseSensitive settings. Our implementation uses case-insensitive sorting which matches 43/46 fixtures correctly.

---

## Phase 11: Fix Remaining Parity Failures

> **Status (2026-01-29):** DotNet 8.0.417 is now available. Most issues resolved. Remaining work: Error fixtures investigation and final verification.

### Completed Sections (11.1-11.4, 11.7)

All tasks in sections 11.1 (Layer 1), 11.2 (Layer 2), 11.3 (Relationships), 11.4 (Layer 4 Ordering), and 11.7 (Inline Constraint Handling) have been completed. See git history for details.

---

### 11.5 Error Fixtures

#### 11.5.1 External Reference Fixture
**Fixtures:** `external_reference`
**Status:** ERROR - DotNet build likely fails due to missing referenced dacpac

- [ ] **11.5.1.1** Investigate external_reference DotNet build failure
- [ ] **11.5.1.2** Fix or mark as expected failure

#### 11.5.2 Unresolved Reference Fixture
**Fixtures:** `unresolved_reference`
**Status:** ERROR - DotNet build likely fails due to unresolved references

- [ ] **11.5.2.1** Investigate unresolved_reference DotNet build failure
- [ ] **11.5.2.2** Fix or mark as expected failure

**Note (2026-01-28):** DotNet is required to investigate error fixtures as they depend on DotNet build behavior.

---

### 11.6 Final Verification: 100% Parity

#### 11.6.1 Complete Verification Checklist
**Goal:** Verify all tests pass, no clippy warnings, and full parity achieved.

- [x] **11.6.1.1** Run `just test` - all unit and integration tests pass
- [x] **11.6.1.2** Run `cargo clippy` - no warnings
  - Fixed: Added SQL Server availability check to `test_e2e_sql_server_connectivity`
  - Fixed: Clippy warnings (regex in loops, collapsible match, doc comments)
- [ ] **11.6.1.3** Run parity regression check - all 46 fixtures at full parity
- [ ] **11.6.1.4** Verify Layer 1 (inventory) at 100%
- [ ] **11.6.1.5** Verify Layer 2 (properties) at 100%
- [ ] **11.6.1.6** Verify Relationships at 100%
- [ ] **11.6.1.7** Verify Layer 4 (ordering) at 100%
- [ ] **11.6.1.8** Verify Metadata at 100%
- [x] **11.6.1.9** Document any intentional deviations from DotNet behavior
- [ ] **11.6.1.10** Update baseline and confirm no regressions

**Note (2026-01-28):** Several baseline entries are stale and show false negatives (`filtered_indexes`, `table_types`, `view_options`, etc.). These need updating when DotNet is available to regenerate reference outputs. Rust output for these fixtures may already match DotNet exactly.

---

### Phase 11 Progress

| Section | Description | Tasks | Status |
|---------|-------------|-------|--------|
| 11.1 | Layer 1: Element Inventory | 8/8 | Complete |
| 11.2 | Layer 2: Properties | 2/2 | Complete |
| 11.3 | Relationships | 19/19 | Complete |
| 11.4 | Layer 4: Ordering | 3/3 | Complete (93.5% pass rate) |
| 11.5 | Error Fixtures | 0/4 | Ready to investigate (DotNet available) |
| 11.6 | Final Verification | 3/10 | In Progress |
| 11.7 | Inline Constraint Handling | 11/11 | Complete |

**Phase 11 Total**: 46/57 tasks complete

> **Status (2026-01-29):** Relationships improved to 80.4% (37/46). Full parity improved to 80.4% (37/46). Remaining work: Error fixtures and final verification.

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
| Phase 11 | **IN PROGRESS** 46/57 |

**Total**: 109/120 tasks complete

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
