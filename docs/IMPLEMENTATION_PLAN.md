# Implementation Plan

---

## Status: PARITY COMPLETE | PERFORMANCE TUNING IN PROGRESS

**Phases 1-64 complete. Full parity: 47/48 (97.9%). Performance tuning: Phases 63-70.**

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

## Completed Phases (1-64)

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

### Key Milestones

- **Parity Achievement (Phase 14):** L1-L3 100%, Relationships 97.9%
- **Performance (Phase 16):** 39x/16x faster than DotNet cold/warm (stress_test, 135 files)
- **Parser Modernization (Phases 15, 20):** All regex replaced with token-based parsing
- **XML Parity (Phases 22-54):** Layer 7 improved from 0% to 50.0%

---

## Performance Tuning (Phases 63-70)

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

### Phase 65 — Eliminate Debug formatting for feature detection (~5-10ms)

`builder.rs:1843,1850,1857` uses `format!("{:?}", opt.option).to_uppercase().contains(...)` to detect ROWGUIDCOL, SPARSE, FILESTREAM. This Debug-formats entire AST nodes to strings for every column option (~1200 allocations).

| Task | Description |
|------|-------------|
| 65.1 | Replace ROWGUIDCOL detection (`builder.rs:1843`) with direct AST `ColumnOption` variant matching or raw SQL text check |
| 65.2 | Replace SPARSE detection (`builder.rs:1850`) with direct AST variant matching or raw SQL text check |
| 65.3 | Replace FILESTREAM detection (`builder.rs:1857`) with direct AST variant matching or raw SQL text check |
| 65.4 | Run model_building criterion benchmark, confirm no regression in existing tests |

---

### Phase 66 — Index-based HashMap keys in disambiguator (~10-15ms)

`assign_inline_constraint_disambiguators()` (builder.rs:1346-1791) makes 6 passes over all elements, building `HashMap<(String, String), ...>` by cloning table_schema/table_name strings. ~320 constraints produce thousands of redundant String allocations.

| Task | Description |
|------|-------------|
| 66.1 | Replace `HashMap<(String, String), Vec<...>>` with `HashMap<usize, Vec<...>>` keyed by table element index in Phase 1 (lines 1362-1390) |
| 66.2 | Update Phase 2 (lines 1395-1488) to use index-based lookups |
| 66.3 | Update Passes A/B (lines 1490-1620) to use index-based lookups |
| 66.4 | Update Phase 4-5 (lines 1625-1791) to use index-based lookups |
| 66.5 | Pre-allocate HashMap capacity using known element counts |
| 66.6 | Run model_building criterion benchmark, confirm no regression in existing tests |

---

### Phase 67 — Pre-compute element full_name (~3-5ms)

`full_name()` (elements.rs:91-131) allocates via `format!()` on every call. Called during sorting (456 elements x 3 keys), schema deduplication, and disambiguation.

| Task | Description |
|------|-------------|
| 67.1 | Add a `cached_full_name: String` field to each `ModelElement` variant's inner struct, populated during element creation |
| 67.2 | Update `full_name()` to return `&str` referencing the cached field |
| 67.3 | Update `xml_name_attr()` and `sort_elements()` to use cached names |
| 67.4 | Run model_building criterion benchmark, confirm improvement |

---

### Phase 68 — Reduce to_uppercase() calls (~2-3ms)

Multiple call sites convert full SQL text to uppercase for case-insensitive matching: `builder.rs:1017,2173,2351,2429,2467`. Called per table (~40x), per constraint (~80x), per proc/func (~50x).

| Task | Description |
|------|-------------|
| 68.1 | Replace `to_uppercase().contains()` patterns with case-insensitive regex (already cached from Phase 63) or `str::to_ascii_uppercase` where needed |
| 68.2 | In `extract_temporal_metadata_from_sql` (builder.rs:2467), compute uppercase once and pass to all sub-functions |
| 68.3 | In `extract_constraint_clustering` (builder.rs:2173), use case-insensitive matching instead of `to_uppercase()` |
| 68.4 | In `is_natively_compiled` (builder.rs:2351), use `contains`-style case-insensitive check |
| 68.5 | Run model_building criterion benchmark, confirm no regression |

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
