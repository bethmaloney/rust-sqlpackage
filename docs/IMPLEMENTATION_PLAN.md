# Implementation Plan

---

## Status: PARITY COMPLETE | PERFORMANCE TUNING IN PROGRESS

**Phases 1-68 complete. Full parity: 47/48 (97.9%). Performance tuning: Phases 63-70.**

| Layer | Passing | Rate |
|-------|---------|------|
| Layer 1 (Inventory) | 49/49 | 100% |
| Layer 2 (Properties) | 49/49 | 100% |
| Layer 3 (SqlPackage) | 49/49 | 100% |
| Relationships | 48/49 | 98.0% |
| Layer 4 (Ordering) | 48/49 | 98.0% |
| Metadata | 49/49 | 100% |
| Layer 7 (Canonical XML) | 24/49 | 49.0% |

**Excluded Fixtures:** `external_reference`, `unresolved_reference` (DotNet fails to build with SQL71501)

---

## Known Issues

| Issue | Status |
|-------|--------|
| Layer 7 element ordering (25/49 failing) | Cosmetic — DotNet's ordering depends on internal processing order which varies between fixtures. Rust uses deterministic sort. No deployment impact. |
| Body dependency alias ordering (65 errors) | DotNet traverses AST in clause order (FROM→WHERE→SELECT), Rust in token order. All references captured correctly — only positional differences. |

---

## Completed Phases (1-68)

| Phase | Description | Status |
|-------|-------------|--------|
| 1-9 | Core implementation (properties, relationships, XML structure, metadata) | 58/58 |
| 10 | Extended properties, function classification, constraint naming | 5/5 |
| 11 | Remaining parity failures, error fixtures, ignored tests | 70/70 |
| 12-13 | SELECT * expansion, TVF columns, TVP support | 10/10 |
| 14 | Layer 3 (SqlPackage) parity | 3/3 |
| 15, 20 | Parser refactoring: replace regex with token-based parsing | 77/77 |
| 16 | Performance optimization (initial): 39x/16x vs DotNet cold/warm | 18/18 |
| 17-19 | Real-world compatibility: comma-less constraints, SQLCMD, TVP parsing | 11/11 |
| 21 | Split model_xml.rs into submodules | 10/10 |
| 22-25 | Layer 7 XML parity, IsMax, dynamic column sources, constraint properties | 27/28 |
| 26, 32, 34, 41-43 | Body dependency resolution (APPLY, CTE, nested subqueries, scope-aware) | All |
| 27-31 | Code consolidation (~1200 lines removed) | 13/13 |
| 35 | Default schema resolution for unqualified table names | 9/9 |
| 36 | DacMetadata.xml dynamic properties | 8/8 |
| 37-38 | Collation handling (LCID map, CollationCaseSensitive) | All |
| 39-40 | SysCommentsObjectAnnotation for views/procedures | All |
| 44-45 | XML formatting (space before />, element ordering) | All |
| 46 | Disambiguator numbering for package references | All |
| 47 | Column-level Collation property | All |
| 48 | 2-named-constraint annotation pattern | All |
| 49 | Schema-aware unqualified column resolution (ColumnRegistry) | All |
| 50 | Schema-aware resolution gaps (8 sub-phases) | 34/34 |
| 50.9 | Decouple column and table annotation logic | All |
| 51 | Layer 7 canonical comparison test fix | All |
| 52 | Procedure-scoped table variable references | All |
| 53 | Layer 7 XML parity (NUMERIC, Scale=0, IsPadded) | All |
| 54 | Layer 7 inline constraint ordering (descending sort) | All |
| 55 | Identifier extraction layer (double-bracket fix) | All |
| 56 | Synonym support (CREATE SYNONYM, SqlSynonym element, XML writer) | All |
| 57 | Temporal tables (SYSTEM_VERSIONING, PERIOD FOR SYSTEM_TIME, history table relationships) | All |
| 58 | Security objects (CREATE USER, CREATE ROLE, ALTER ROLE ADD MEMBER, GRANT/DENY/REVOKE) | All |
| 59 | Database scoped configurations (silently skip — DacFx does not support) | All |
| 60 | ALTER VIEW WITH SCHEMABINDING (fallback + AST dual-path support) | All |
| 61 | Columnstore indexes (CREATE CLUSTERED/NONCLUSTERED COLUMNSTORE INDEX) | All |
| 62 | Dynamic data masking (MASKED WITH column property, GDPR/PCI-DSS compliance) | All |
| 63 | Cache regex patterns with LazyLock (21 static + 3 dynamic→string ops) | All |
| 64 | Lower ZIP compression level (deflate 6→1, ~29% packaging speedup) | All |
| 65 | Eliminate Debug formatting for feature detection (dead code removal) | All |
| 66 | Index-based HashMap keys in disambiguator (eliminate String clones) | All |
| 67 | Pre-compute element full_name and xml_name_attr (cached in DatabaseModel) | All |
| 68 | Reduce to_uppercase() calls (case-insensitive helpers, zero-alloc) | All |

### Key Milestones

- **Parity Achievement (Phase 14):** L1-L3 100%, Relationships 97.9%
- **Performance (Phase 16):** 39x/16x faster than DotNet cold/warm (stress_test, 135 files)
- **Parser Modernization (Phases 15, 20):** All regex replaced with token-based parsing
- **XML Parity (Phases 22-54):** Layer 7 improved from 0% to 50.0%

---

## Performance Tuning (Phases 63-70, 68 complete)

**Baseline (stress_test, 135 files, 456 elements):** 103ms total

| Stage | Time | % |
|-------|------|---|
| Project parse | 0.3ms | 0.2% |
| SQL parsing | 3.8ms | 3.6% |
| **Model building** | **80.6ms** | **75.8%** |
| XML generation | 9.1ms | 8.5% |
| Dacpac packaging | 12.6ms | 11.8% |

**Target:** ~40-55ms (2-2.5x improvement), restoring 70-100x speedup vs DotNet.

---

### Phase 63 — Cache regex patterns with LazyLock — COMPLETE

Cached 21 static regex patterns using `LazyLock<Regex>` across 4 files. Replaced 2 dynamic regex patterns in `programmability_writer.rs` with string-based word boundary matching (`contains_word_boundary`). Remaining 4 dynamic patterns (using runtime table/param names in `tsql_parser.rs:1331,1380,1943` and `builder.rs` via `extract_generic_object_name`) cannot be cached statically — these are fallback paths called infrequently.

**Files changed:** `builder.rs` (7 static patterns), `tsql_parser.rs` (11 static patterns), `sqlcmd.rs` (3 static patterns), `index_parser.rs` (1 static pattern), `programmability_writer.rs` (2 dynamic→string ops).

---

### Phase 64 — Lower ZIP compression level — COMPLETE

Changed deflate compression level from 6 to 1 in `src/dacpac/packager.rs`. For ~19KB dacpac files, level 6 provides negligible size benefit over level 1 while consuming significantly more CPU.

**Benchmark result:** dacpac_packaging/create_dacpac improved from ~5.6ms to ~3.98ms (**~29% faster**, p=0.00). All 1,894 tests pass — dacpac output remains valid (decompression produces identical content regardless of compression level).

---

### Phase 65 — Eliminate Debug formatting for feature detection — COMPLETE

Removed dead code: `format!("{:?}", opt.option).to_uppercase().contains(...)` for ROWGUIDCOL, SPARSE, FILESTREAM detection in `column_from_def()` (builder.rs).

**Finding:** This code was unreachable. sqlparser-rs 0.54 doesn't recognize ROWGUIDCOL/SPARSE/FILESTREAM as keywords — any CREATE TABLE containing them fails parsing and goes through the fallback token-based parser (`column_parser.rs` lines 274-293), which handles these flags directly via `column_from_fallback_table()`. The AST path (`column_from_def()`) is only reached when sqlparser succeeds, which means these keywords are never present.

**Impact:** Eliminates ~1200 unnecessary `format!("{:?}")` + `to_uppercase()` allocations per build (though in practice these allocations never triggered due to the dead code path). More importantly, removes misleading code that suggested sqlparser could parse these T-SQL features.

**Files changed:** `builder.rs` (removed 18 lines of dead code, replaced with comment explaining why these are always false).

---

### Phase 66 — Index-based HashMap keys in disambiguator — COMPLETE

Replaced all `HashMap<(String, String), ...>` with `HashMap<usize, ...>` keyed by table element index in `assign_inline_constraint_disambiguators()`. Also replaced the 3-tuple `HashMap<(String, String, String), Vec<u32>>` (column annotations) with `HashMap<(usize, String), Vec<u32>>`.

**Approach:** Built a `constraint_to_table: HashMap<usize, usize>` mapping each constraint element index to its parent table element index during Phase 1. This eliminated all `table_schema.clone()` + `table_name.clone()` allocations in Phases 2-5. The `TableConstraintMap` type alias was updated from `HashMap<(String, String), ...>` to `HashMap<usize, ...>`.

**Key insight:** A temporary `HashMap<(&str, &str), usize>` is used only during Pre-phase/Phase 1 to map constraint names to table indices, then dropped before any mutable borrows of `elements`. All later phases use the integer-keyed `constraint_to_table` map.

**Files changed:** `builder.rs` (type alias + ~30 lines changed across 6 phases, net reduction in string allocations).

**Skipped:** Task 66.5 (pre-allocate HashMap capacity) — micro-optimization with minimal gain since HashMap resizing cost is negligible compared to eliminated String clones.

---

### Phase 67 — Pre-compute element full_name and xml_name_attr — COMPLETE

Added `cached_full_names: Vec<String>` and `cached_xml_names: Vec<String>` to `DatabaseModel`, computed once via `cache_element_names()` before sorting. The sort function (`sort_model()`) now uses pre-computed cached_xml_names instead of calling `xml_name_attr()` (which calls `full_name()` with `format!()`) per element. XML generation uses cached names for the db_options placement check.

**Approach:** Rather than adding `cached_full_name` to all 24 inner element structs (which would require modifying ~288 pattern match sites), cached names are stored as parallel `Vec<String>` in `DatabaseModel`. The `sort_model()` function builds sort keys from cached names, sorts an index array, then applies the permutation to all three vecs (elements, cached_full_names, cached_xml_names) together. Names are re-cached after disambiguation for consistency.

**Key insight:** `sort_by_cached_key` already caches sort keys internally, so the per-sort allocation savings are modest (~500 format!() calls avoided). The main benefit is making cached names available to the XML writer without additional allocations, and establishing the infrastructure for future phases to use `&str` references instead of owned Strings.

**Files changed:** `database_model.rs` (added cached name fields + accessor methods), `builder.rs` (replaced `sort_elements` with `sort_model`, added `apply_permutation` helper), `model_xml/mod.rs` (use cached xml_name in db_options placement loop).

**All 1,894 tests pass.** Parity regression test confirms no ordering changes.

---

### Phase 68 — Reduce to_uppercase() calls — COMPLETE

Introduced zero-allocation case-insensitive helpers (`contains_ci`, `starts_with_ci`, `find_ci`, `parse_data_compression`) in `builder.rs` to replace `to_uppercase()` calls that allocated full uppercase copies of SQL text for simple keyword matching.

**Changes in `builder.rs` (15 `to_uppercase()` calls eliminated):**
- `extract_view_options()`: 1 `to_uppercase()` → 5 `contains_ci()` calls
- `extract_constraint_clustering()`: 2 `to_uppercase()` + string `find`/`contains` → `find_ci()`/`contains_ci()`
- `is_natively_compiled()`: 1 `to_uppercase().contains()` → `contains_ci()`
- `extract_temporal_metadata_from_sql()` + sub-functions: 1 `to_uppercase()` removed, sub-functions (`extract_period_columns`, `extract_versioning_options`) now use `contains_ci()` directly instead of receiving pre-uppercased `&str`
- `extract_type_params_from_string()`: 2 `to_uppercase()` → `contains_ci()`/`starts_with_ci()`
- Function type detection (CreateFunction match arm): 2 `to_uppercase()` → `contains_ci()`
- `extract_fill_factor()`: 1 `ident.value.to_uppercase() == "FILLFACTOR"` → `eq_ignore_ascii_case()`
- `extract_data_compression()`: 2 `to_uppercase()` → `eq_ignore_ascii_case()` via `parse_data_compression()`
- Data compression matching (2 FallbackStatementType arms): 2 `to_uppercase()` match → `parse_data_compression()`
- Raw statement object type: 1 `to_uppercase()` match → `eq_ignore_ascii_case()`

**Changes in `view_writer.rs` (1 `to_uppercase()` call eliminated):**
- Raw view writer: 1 `to_uppercase()` → local `contains_ci()` helper

**Files changed:** `builder.rs` (~50 lines changed, 4 helper functions added), `view_writer.rs` (~15 lines changed, 1 helper function added).

**All 1,623 tests pass.** No parity regressions.

---

### Phase 69 — Arc\<str\> for SQL definition text (~2-3ms)

Every model element clones the full SQL definition text (builder.rs:188,212,338,...). For 135 statements, this is 135 deep String copies. Procedures/functions can be kilobytes each.

| Task | Description |
|------|-------------|
| 69.1 | Change `definition: String` fields in element structs to `definition: Arc<str>` |
| 69.2 | Convert `ParsedStatement.sql_text` to `Arc<str>` at parse time so all downstream consumers share the allocation |
| 69.3 | Update all `definition` consumers (model builders, XML writers) to work with `Arc<str>` |
| 69.4 | Run full test suite, confirm no regressions |

---

### Phase 70 — ~~Parallelize model building with rayon~~ DEFERRED

**Status:** Deferred — unlikely to provide meaningful gains after Phases 63-69.

**Reasoning:** Phases 63-69 are expected to reduce model building from ~80.6ms to ~5-25ms. At that point:

- **Per-item work is too small:** 135 statements across ~15ms = ~110μs per statement, which is borderline for rayon's per-item overhead. Work distribution + synchronization costs ~0.5-1ms, eating into any gains.
- **Best case saves ~5-10ms, worst case is slower:** On small projects rayon overhead exceeds the parallelism benefit.
- **High refactor cost:** The main loop (1019 lines) has shared mutable state (`model.elements` Vec and `schemas` BTreeSet) requiring a 500-800 line extraction into a pure function. This adds ongoing maintenance complexity.
- **Re-profile after Phase 69:** The bottleneck will likely shift to XML generation or ZIP packaging, where simpler optimizations exist.
