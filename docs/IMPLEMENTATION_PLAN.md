# Implementation Plan

---

## Status: PARITY COMPLETE | PERFORMANCE TUNING IN PROGRESS

**Phases 1-69, 71-72 complete. Full parity: 47/48 (97.9%). Performance tuning: Phases 63-69, 71-72 complete, 73-77 pending.**

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

## Completed Phases (1-69, 71-72)

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
| 51-55 | Layer 7 XML parity (canonical comparison, NUMERIC, Scale=0, IsPadded, inline constraint ordering, identifier extraction) | All |
| 56 | Synonym support (CREATE SYNONYM, SqlSynonym element, XML writer) | All |
| 57 | Temporal tables (SYSTEM_VERSIONING, PERIOD FOR SYSTEM_TIME, history table relationships) | All |
| 58 | Security objects (CREATE USER, CREATE ROLE, ALTER ROLE ADD MEMBER, GRANT/DENY/REVOKE) | All |
| 59 | Database scoped configurations (silently skip — DacFx does not support) | All |
| 60 | ALTER VIEW WITH SCHEMABINDING (fallback + AST dual-path support) | All |
| 61 | Columnstore indexes (CREATE CLUSTERED/NONCLUSTERED COLUMNSTORE INDEX) | All |
| 62 | Dynamic data masking (MASKED WITH column property) | All |
| 63-69 | Performance tuning: regex caching, ZIP level, dead code, index-based keys, cached names, zero-alloc CI helpers, Arc\<str\> | All |
| 71 | Pre-allocate model.xml buffer (+ DacMetadata.xml, Origin.xml) | All |
| 72 | Cache ColumnRegistry view extraction results (eliminate double view parsing) | All |

### Key Milestones

- **Parity Achievement (Phase 14):** L1-L3 100%, Relationships 97.9%
- **Performance (Phase 16, improved through 69):** 186x/90x faster than DotNet cold/warm (stress_test, 135 files)
- **Parser Modernization (Phases 15, 20):** All regex replaced with token-based parsing
- **XML Parity (Phases 22-54):** Layer 7 improved from 0% to 50.0%

---

## Performance Tuning (Phases 63-77)

### Completed (Phases 63-69, 71-72)

**Baseline (stress_test, 135 files, 456 elements):** 103ms → 30ms (3.4x improvement)

| Phase | Optimization | Impact |
|-------|-------------|--------|
| 63 | Cache 21 static regex patterns with `LazyLock` | Eliminated per-call regex compilation |
| 64 | Lower ZIP compression (deflate 6→1) | ~29% faster packaging |
| 65 | Remove dead Debug formatting for ROWGUIDCOL/SPARSE/FILESTREAM | Dead code removal |
| 66 | Index-based HashMap keys in disambiguator | Eliminated String clones in constraint mapping |
| 67 | Pre-compute element full_name/xml_name_attr in DatabaseModel | Eliminated repeated `format!()` during sort/XML gen |
| 68 | Zero-alloc case-insensitive helpers (`contains_ci`, `starts_with_ci`) | Eliminated 16 `to_uppercase()` allocations |
| 69 | `Arc<str>` for SQL definition text | Eliminated deep String copies across pipeline |
| 71 | Pre-allocate model.xml buffer (`elements.len() * 2000`) | Eliminated ~24 Vec reallocations for large projects |
| 72 | Cache view extraction results in ColumnRegistry | Eliminated double view parsing (10-20 tokenizations saved per view) |

### Pending (Phases 73-77)

**Large project profiling (920 files, 8083 elements, 15MB model.xml):** ~1050ms total

| Stage | Time | % |
|-------|------|---|
| Project parse | 4ms | 0.4% |
| SQL parsing | 90ms | 8% |
| Model building | 22ms | 2% |
| **XML generation** | **450ms** | **42%** |
| **Dacpac packaging** | **500ms** | **47%** |

At scale, the bottleneck shifts from model building to XML generation and dacpac packaging. Phase 70 (parallelize model building) deferred — model building is only 2% of total time at scale.

---

### Phase 71 — Pre-allocate model.xml buffer — COMPLETE

Pre-allocated `Vec<u8>` buffers in `create_dacpac()`:
- model.xml: `Vec::with_capacity(model.elements.len() * 2000)` — eliminates ~24 reallocations for large projects
- DacMetadata.xml: `Vec::with_capacity(4096)` — small fixed allocation
- Origin.xml: `Vec::with_capacity(4096)` — small fixed allocation

**Files:** `src/dacpac/packager.rs`

---

### Phase 72 — Cache ColumnRegistry view extraction results — COMPLETE

Eliminated double view parsing during XML generation. `ColumnRegistry::from_model()` now caches `(query_script, columns, query_deps)` per view. `write_view()` and `write_raw_view()` look up cached results via `column_registry.get_cached_view()` instead of re-extracting.

**Changes:**
- Added `ViewExtractionResult` struct and `view_cache: HashMap` to `ColumnRegistry`
- `from_model()` now caches extraction results for both `ViewElement` and `RawElement` views
- `write_view()` and `write_raw_view()` use cached results, with fallback to fresh extraction
- Raw views (`ModelElement::Raw` with `sql_type == "SqlView"`) now also populate the column registry
- Made `contains_ci()` in view_writer.rs `pub(crate)` for reuse in column_registry.rs

**Files:** `src/dacpac/model_xml/column_registry.rs`, `src/dacpac/model_xml/view_writer.rs`
**Savings:** Eliminates 10-20 tokenizations per view (estimated 150-200ms on large projects)

---

### Phase 73 — Single tokenization for body dependency extraction — PENDING

Reduce repeated tokenization of procedure/function bodies during XML generation. `extract_body_dependencies()` calls multiple sub-functions (`extract_table_aliases()`, `extract_column_references()`, `extract_declare_types()`, etc.), each of which independently tokenizes the same SQL body.

**Tasks:**
1. Tokenize the body once at the start of `extract_body_dependencies()`
2. Pass the token list to sub-functions instead of raw SQL
3. Update sub-function signatures to accept `&[TokenWithLocation]`
4. Verify parity — body dependency output must be identical

**Files:** `src/dacpac/model_xml/body_deps.rs`
**Estimated savings:** 30-50ms on large projects with many procedures/functions

---

### Phase 74 — Fast-path preprocessing bypass — PENDING

Skip the tokenize-and-reconstruct preprocessing step when no transformations are needed. `preprocess_tsql_tokens()` currently tokenizes every SQL batch and reconstructs it character-by-character even when no BINARY(MAX), DEFAULT FOR, or trailing comma transformations apply. Most batches pass through unchanged.

**Tasks:**
1. Add a fast-path check: scan for trigger keywords (`BINARY`, `DEFAULT`, trailing `,` before `)`) before invoking the full tokenizer
2. If no trigger keywords found, return the input unchanged (zero-alloc)
3. Keep the full preprocessing path for batches that need it
4. Verify all tests pass — preprocessing must still fire when needed

**Files:** `src/parser/preprocess_parser.rs`, `src/parser/tsql_parser.rs`
**Estimated savings:** 15-30ms on large projects

---

### Phase 75 — Zero-alloc fallback parse dispatch — PENDING

Replace `sql.to_uppercase()` allocation in `try_fallback_parse()` with zero-allocation case-insensitive matching. Currently every statement that fails sqlparser-rs gets a full uppercase clone of the SQL text, then 300+ lines of `.contains()` checks against the uppercase copy.

**Tasks:**
1. Replace `let sql_upper = sql.to_uppercase()` with `contains_ci()` / `starts_with_ci()` checks (helpers already exist in `builder.rs`)
2. Move the case-insensitive helpers to a shared utility module (or reuse existing ones)
3. Add early-exit based on first keyword token (peek at position 0-3) before scanning full SQL

**Files:** `src/parser/tsql_parser.rs`
**Estimated savings:** 5-15ms on large projects

---

### Phase 76 — Single tokenization for fallback parser chain — PENDING

Eliminate repeated tokenization in the fallback parser chain. When `try_fallback_parse()` is called, each token-based parser (`parse_create_procedure_tokens`, `parse_create_function_tokens`, `parse_create_index_tokens`, etc.) creates a new `TokenParser` which re-tokenizes the SQL from scratch. A single statement can be tokenized 5-10 times as each parser tries and fails.

**Tasks:**
1. Tokenize once at the top of `try_fallback_parse()`
2. Create a `TokenParser::from_tokens()` constructor that accepts pre-tokenized tokens
3. Pass the shared token list to each fallback parser attempt
4. Verify all tests pass — fallback parsing results must be identical

**Files:** `src/parser/tsql_parser.rs`, `src/parser/token_parser_base.rs`
**Estimated savings:** 20-40ms on large projects

---

### Phase 77 — HashSet dedup and HashMap index in view writer — PENDING

Fix two algorithmic inefficiencies in the view XML writer:

**77a — HashSet for query_deps deduplication:**
`extract_view_columns_and_deps()` uses `Vec::contains()` for deduplication, which is O(n) per check. For views with many column references, this becomes O(n^2).

**Tasks:**
1. Replace `Vec::contains()` checks in `extract_view_columns_and_deps()` with a parallel `HashSet`
2. Keep the `Vec` for ordered output, use `HashSet` only for O(1) membership checks

**77b — HashMap index for SELECT * expansion:**
`expand_select_star()` does a linear scan of all model elements to find matching tables. With thousands of elements and multiple table aliases, this is O(elements * aliases) per view.

**Tasks:**
1. Build a `HashMap<(schema, name), &TableElement>` index before view writing begins
2. Use O(1) lookups in `expand_select_star()` instead of linear scan
3. Pass the index through the view writer call chain

**Files:** `src/dacpac/model_xml/view_writer.rs`, `src/dacpac/model_xml/mod.rs`
**Estimated savings:** 5-10ms on large projects
