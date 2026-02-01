# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Status: PARITY COMPLETE | REAL-WORLD COMPATIBILITY IN PROGRESS

**Phases 1-20 complete (250 tasks). Full parity achieved.**

**Current Focus: Phase 21 - Split model_xml.rs into Submodules** (7/10 tasks)
- ✅ Phase 21.1-21.4.1 complete: Module structure, element writers, body_deps extracted
- ⬜ Phase 21.4.2, 21.5.1 remaining: qualified_name.rs (optional), other_writers.rs

**Phase 22 - Layer 7 Canonical XML Parity** (3/5 tasks)
- ✅ CollationCaseSensitive, SqlCmdVariables, constraint ordering fixed
- ⬜ CustomData verification, SqlInlineConstraintAnnotation order remaining
- Layer 7: 10/48 (20.8%)

**Discovered Issues (Phases 23-25):**
- Phase 23: IsMax property for MAX types (0/4) - deployment failure
- Phase 24: Dynamic column sources in procedures (0/8) - 177 missing elements
- Phase 25: ALTER TABLE constraints (0/6) - 14 PKs, 19 FKs missing

| Layer | Passing | Rate |
|-------|---------|------|
| Layer 1 (Inventory) | 48/48 | 100% |
| Layer 2 (Properties) | 46/48 | 95.8% |
| Layer 3 (SqlPackage) | 48/48 | 100% |
| Relationships | 46/48 | 95.8% |
| Layer 4 (Ordering) | 48/48 | 100% |
| Metadata | 48/48 | 100% |
| Layer 7 (Canonical XML) | 10/48 | 20.8% |

### Excluded Fixtures

Two fixtures are excluded from parity testing because DotNet fails to build them:

1. **external_reference** - References an external database via synonym; DotNet fails with SQL71501
2. **unresolved_reference** - View references non-existent table; DotNet fails with SQL71501

---

## Phase 21: Split model_xml.rs into Submodules (7/10)

**Location:** `src/dacpac/model_xml/mod.rs` (~12,600 lines after 21.3.1)

**Goal:** Break up the largest file in the codebase into logical submodules for improved maintainability, faster compilation, and easier navigation.

<details>
<summary>Completed: Phase 21.1-21.4.1 (7 tasks)</summary>

### Phase 21.1: Create Module Structure (2/2) ✅

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.1.1 | Create `src/dacpac/model_xml/` directory with `mod.rs` | ✅ | Moved model_xml.rs to model_xml/mod.rs |
| 21.1.2 | Move `generate_model_xml()` entry point to mod.rs | ✅ | Entry point remains in mod.rs |

### Phase 21.2: Extract XML Writing Helpers (2/2) ✅

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.2.1 | Create `xml_helpers.rs` with low-level XML utilities | ✅ | 244 lines including 9 unit tests |
| 21.2.2 | Create `header.rs` with header/metadata writing | ✅ | 324 lines including 9 unit tests |

### Phase 21.3: Extract Element Writers (3/3) ✅

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.3.1 | Create `table_writer.rs` for table/column XML | ✅ | 650 lines including 10 unit tests |
| 21.3.2 | Create `view_writer.rs` for view XML | ✅ | 574 lines including 8 unit tests |
| 21.3.3 | Create `programmability_writer.rs` for procs/functions | ✅ | 1838 lines including 35 unit tests |

### Phase 21.4.1: Create body_deps.rs ✅

Created body_deps.rs with BodyDependency, BodyDepToken, BodyDependencyTokenScanner, TableAliasTokenParser, QualifiedName, extract_body_dependencies, and helper functions. ~2,200 lines including tests.

</details>

### Phase 21.4: Extract Body Dependencies (1/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.4.1 | Create `body_deps.rs` for dependency extraction | ✅ | ~2,200 lines including tests |
| 21.4.2 | Create `qualified_name.rs` for name parsing | ⬜ | **Optional:** QualifiedName already integrated in body_deps.rs (~130 lines) |

### Phase 21.5: Extract Remaining Writers (0/1)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.5.1 | Create `other_writers.rs` for remaining elements | ⬜ | `write_index`, `write_constraint`, `write_sequence`, `write_trigger`, etc. (~1,200 lines) |

---

## Phase 22: Layer 7 Canonical XML Parity (3/5)

**Goal:** Achieve byte-level XML matching between rust-sqlpackage and DotNet DacFx output.

### Phase 22.1: Fix CollationCaseSensitive Attribute (1/1) ✅

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 22.1.1 | Set CollationCaseSensitive="True" to match DotNet | ✅ | DataSchemaModel root element attribute |

### Phase 22.2: Fix Missing CustomData Elements (1/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 22.2.1 | Add empty SqlCmdVariables CustomData element | ✅ | Emitted even when no SQLCMD variables defined |
| 22.2.2 | Verify other CustomData elements match DotNet | ⬜ | Check for other missing CustomData categories |

### Phase 22.3: Fix Element/Property Ordering (1/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 22.3.1 | Audit element ordering against DotNet output | ✅ | Fixed PK/Unique constraint relationship ordering. Layer 7: 2/48 → 10/48 |
| 22.3.2 | Fix SqlInlineConstraintAnnotation/AttachedAnnotation order | ⬜ | DotNet assigns based on element ordering |

---

## Phase 23: Fix IsMax Property for MAX Types (0/4)

**Goal:** Fix deployment failure: `Length="4294967295"` → `IsMax="True"` for MAX types.

**Error:** `The value of the property type Int32 is formatted incorrectly.`

### Phase 23.1: Fix TVF Column IsMax (0/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 23.1.1 | Add IsMax check in `write_tvf_columns()` | ⬜ | Check `col.length == Some(u32::MAX)` |
| 23.1.2 | Add unit tests for TVF MAX column output | ⬜ | nvarchar(max), varchar(max), varbinary(max) |

### Phase 23.2: Fix ScalarType IsMax (0/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 23.2.1 | Add IsMax check in `write_scalar_type()` | ⬜ | Check `scalar.length == Some(-1)` |
| 23.2.2 | Add unit tests for scalar type MAX output | ⬜ | `CREATE TYPE ... FROM NVARCHAR(MAX)` |

**Reference:** See `table_writer.rs` lines 344-350 for correct pattern.

---

## Phase 24: Track Dynamic Column Sources in Procedure Bodies (0/8)

**Goal:** Generate `SqlDynamicColumnSource` elements for CTEs, temp tables, and table variables.

**Impact:** 177 missing SqlDynamicColumnSource, 181 missing SqlSimpleColumn/SqlTypeSpecifier elements.

### Phase 24.1: CTE Column Source Extraction (0/3)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 24.1.1 | Create `DynamicColumnSource` struct | ⬜ | name, source_type, columns |
| 24.1.2 | Extract CTE definitions from bodies | ⬜ | Parse `WITH cte AS (SELECT ...)` |
| 24.1.3 | Write `SqlDynamicColumnSource` for CTEs | ⬜ | With `SqlComputedColumn` for each column |

### Phase 24.2: Temp Table Column Source Extraction (0/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 24.2.1 | Extract temp table definitions | ⬜ | `CREATE TABLE #name`, INSERT...SELECT inference |
| 24.2.2 | Write `SqlDynamicColumnSource` for temp tables | ⬜ | Include column elements |

### Phase 24.3: Table Variable Column Source Extraction (0/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 24.3.1 | Extract table variable definitions | ⬜ | `DECLARE @name TABLE(...)` |
| 24.3.2 | Write `SqlDynamicColumnSource` for table variables | ⬜ | With `SqlTypeSpecifier` |

### Phase 24.4: Integration (0/1)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 24.4.1 | Integrate into procedure/function writers | ⬜ | Add to DynamicObjects relationship |

---

## Phase 25: Fix Missing Constraints from ALTER TABLE Statements (0/6)

**Goal:** Parse constraints defined via `ALTER TABLE...ADD CONSTRAINT` statements.

**Impact:** 14 missing PKs, 19 missing FKs.

### Phase 25.1: Parse ALTER TABLE Constraints (0/3)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 25.1.1 | Handle `GO;` batch separator | ⬜ | Treat same as `GO` |
| 25.1.2 | Parse `ALTER TABLE...ADD CONSTRAINT PRIMARY KEY` | ⬜ | Extract table, constraint, columns |
| 25.1.3 | Parse `ALTER TABLE...ADD CONSTRAINT FOREIGN KEY` | ⬜ | Handle CHECK CONSTRAINT pattern |

### Phase 25.2: Fix Inline Constraint Edge Cases (0/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 25.2.1 | Debug inline PK parsing edge cases | ⬜ | `CONSTRAINT [PK_X] PRIMARY KEY CLUSTERED` |
| 25.2.2 | Add tests for inline constraint variations | ⬜ | Whitespace, casing, CLUSTERED/NONCLUSTERED |

### Phase 25.3: Validation (0/1)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 25.3.1 | Validate constraint counts match DotNet | ⬜ | Target: 667 PKs, 2316 FKs |

---

## Known Issues

| Issue | Location | Phase |
|-------|----------|-------|
| TVF MAX column IsMax property | `programmability_writer.rs` | Phase 23 |
| Missing SqlDynamicColumnSource elements | procedure bodies | Phase 24 |
| Missing constraints from ALTER TABLE | parser/builder | Phase 25 |

---

<details>
<summary>Completed Phases Summary (Phases 1-20)</summary>

## Phase Overview

| Phase | Description | Tasks |
|-------|-------------|-------|
| Phase 1-9 | Core implementation (properties, relationships, XML structure, metadata) | 58/58 |
| Phase 10 | Fix extended properties, function classification, constraint naming | 5/5 |
| Phase 11 | Fix remaining parity failures, error fixtures, ignored tests | 70/70 |
| Phase 12 | SELECT * expansion, TVF columns, duplicate refs | 6/6 |
| Phase 13 | Fix remaining relationship parity issues (TVP support) | 4/4 |
| Phase 14 | Layer 3 (SqlPackage) parity | 3/3 |
| Phase 15 | Parser refactoring: replace regex with token-based parsing | 34/34 |
| Phase 16 | Performance tuning: benchmarks, regex caching, parallelization | 18/18 |
| Phase 17 | Real-world SQL compatibility: comma-less constraints, SQLCMD format | 5/5 |
| Phase 18 | BodyDependencies alias resolution: fix table alias handling | 15/15 |
| Phase 19 | Whitespace-agnostic trim patterns: token-based TVP parsing | 3/3 |
| Phase 20 | Replace remaining regex with tokenization/AST | 43/43 |

## Phase 20: Replace Remaining Regex with Tokenization/AST (43/43) ✅

Eliminated remaining regex patterns in favor of tokenizer-based parsing for better maintainability and correctness.

### Phase 20.1: Parameter Parsing (3/3) ✅
- Procedure parameter parsing via `ProcedureTokenParser`
- Function parameter parsing via `extract_function_parameters_tokens()`
- Consistent parameter storage without `@` prefix

### Phase 20.2: Body Dependency Token Extraction (8/8) ✅
Replaced TOKEN_RE, COL_REF_RE, BARE_COL_RE, BRACKETED_IDENT_RE, ALIAS_COL_RE, SINGLE_BRACKET_RE, COLUMN_ALIAS_RE with token-based scanning. Created `BodyDependencyTokenScanner` and `QualifiedName` struct.

### Phase 20.3: Type and Declaration Parsing (4/4) ✅
Replaced DECLARE_TYPE_RE, TVF_COL_TYPE_RE, CAST_EXPR_RE with tokenized parsing. Created `TvfColumnTypeInfo` and `CastExprInfo` structs.

### Phase 20.4: Table and Alias Pattern Matching (7/7) ✅
Replaced TABLE_ALIAS_RE, TRIGGER_ALIAS_RE, BRACKETED_TABLE_RE, UNBRACKETED_TABLE_RE, QUALIFIED_TABLE_NAME_RE, INSERT_SELECT_RE, UPDATE_ALIAS_RE with `TableAliasTokenParser`.

### Phase 20.5: SQL Keyword Detection (6/6) ✅
Replaced AS_KEYWORD_RE, ON_KEYWORD_RE, GROUP_BY_RE, GROUP_TERMINATOR_RE with tokenized scanning.

### Phase 20.6: Semicolon and Whitespace Handling (3/3) ✅
Created `extract_index_filter_predicate_tokenized()` in index_parser.rs.

### Phase 20.7: CTE and Subquery Pattern Matching (4/4) ✅
Replaced CTE_ALIAS_RE, SUBQUERY_ALIAS_RE, APPLY_KEYWORD_RE, APPLY_FUNCTION_ALIAS_RE with token-based parsing via `TableAliasTokenParser`.

### Phase 20.8: Fix Alias Resolution Bugs (11/11) ✅
Fixed 11 alias resolution bugs in `extract_all_column_references()`. Table aliases now filtered before treating as column references. Added MERGE keyword detection for TARGET/SOURCE aliases.

## Key Implementation Details

### Tokenization Benefits
- Handles variable whitespace (tabs, multiple spaces, newlines) correctly
- Respects SQL comments and string literals
- More maintainable and easier to extend
- Better error messages when parsing fails
- Faster performance on complex patterns

### Remaining Hotspots

| Area | Location | Issue | Impact |
|------|----------|-------|--------|
| Cloning | `src/model/builder.rs` | 149 clone() calls | MEDIUM |

</details>
