# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Status: PARITY COMPLETE | REAL-WORLD COMPATIBILITY IN PROGRESS

**Phases 1-20.8 complete (250 tasks). Full parity achieved.**

**Current Focus: Phase 20 - Replace Remaining Regex with Tokenization/AST**
- ✅ Phase 20.1 complete: Token-based parameter parsing (3/3 tasks)
- ✅ Phase 20.2 complete: Body dependency token extraction (8/8 tasks)
- ✅ Phase 20.3 complete: Type and declaration parsing (4/4 tasks)
- ✅ Phase 20.4 complete: Table and alias pattern matching (7/7 tasks)
- ✅ Phase 20.5 complete: SQL keyword detection (6/6 tasks)
- ✅ Phase 20.6 complete: Semicolon and whitespace handling (3/3 tasks)
- ✅ Phase 20.7 complete: CTE and subquery pattern matching (4/4 tasks)
- ✅ Phase 20.8 complete: Fix alias resolution bugs in BodyDependencies (11/11 tasks)

**Current Focus: Phase 21 - Split model_xml.rs into Submodules** (5/10 tasks)
- ✅ Phase 21.1 complete: Create module structure (2/2 tasks)
- ✅ Phase 21.2 complete: Extract XML Writing Helpers (2/2 tasks)
- ✅ Phase 21.3.1 complete: Create table_writer.rs for table/column XML
- Target: Break 13,413-line file into ~9 logical submodules (currently ~12,600 lines)

| Layer | Passing | Rate |
|-------|---------|------|
| Layer 1 (Inventory) | 44/44 | 100% |
| Layer 2 (Properties) | 44/44 | 100% |
| Layer 3 (SqlPackage) | 44/44 | 100% |
| Relationships | 44/44 | 100% |
| Layer 4 (Ordering) | 44/44 | 100% |
| Metadata | 44/44 | 100% |

### Excluded Fixtures

Two fixtures are excluded from parity testing because DotNet fails to build them:

1. **external_reference** - References an external database via synonym; DotNet fails with SQL71501
2. **unresolved_reference** - View references non-existent table; DotNet fails with SQL71501

These test Rust's ability to build projects that DotNet cannot handle.

## Phase 20: Replace Remaining Regex with Tokenization/AST

**Goal:** Eliminate remaining regex patterns in favor of tokenizer-based or AST-based parsing for better maintainability and correctness.

**Background:** Phase 15 converted many regex patterns to token-based parsing, but several complex patterns remain in `src/dacpac/model_xml.rs` and other modules. These patterns are fragile and can fail on edge cases involving tabs, multiple spaces, or nested expressions.

**Status:** Phase 20.1 complete (parameter parsing). Phases 20.2-20.7 remain.

### Phase 20.2: Body Dependency Token Extraction (8/8) ✅

**Location:** `src/dacpac/model_xml.rs`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.2.1 | Replace TOKEN_RE with tokenizer-based scanning | ✅ | Lines 129-134: Massive regex with 17 capture groups |
| 20.2.2 | Replace COL_REF_RE with tokenizer | ✅ | Replaced with `extract_column_refs_tokenized()` using `BodyDependencyTokenScanner` |
| 20.2.3 | Replace BARE_COL_RE with tokenizer | ✅ | Handled by `BodyDepToken::SingleBracketed` in `extract_all_column_references()` |
| 20.2.4 | Replace BRACKETED_IDENT_RE with tokenizer | ✅ | Replaced with `extract_bracketed_identifiers_tokenized()` function. Used in `extract_filter_predicate_columns` and `extract_expression_column_references`. |
| 20.2.5 | Replace ALIAS_COL_RE with tokenizer | ✅ | Replaced with `extract_alias_column_refs_tokenized()` using `BodyDepToken::AliasDotBracketedColumn`. Used in `extract_trigger_body_dependencies()` for ON/SET/SELECT clauses. 17 unit tests. |
| 20.2.6 | Replace SINGLE_BRACKET_RE with tokenizer | ✅ | Replaced with `extract_single_bracketed_identifiers()` using `BodyDepToken::SingleBracketed`. Used in `extract_trigger_body_dependencies()` for INSERT column lists. 17 unit tests. |
| 20.2.7 | Replace COLUMN_ALIAS_RE with tokenizer | ✅ | Replaced with `extract_column_aliases_tokenized()` using sqlparser-rs tokenizer. Detects AS keyword and extracts following identifier, filters SQL keywords. 17 unit tests. |
| 20.2.8 | Replace split('.') with qualified name parser | ✅ | Replaced with `parse_qualified_name_tokenized()` using `BodyDependencyTokenScanner`. New `QualifiedName` struct for 1-3 part names. Used in `extract_simple_table_name`, `normalize_table_reference`, `extract_column_name_from_expr_simple`, `resolve_column_reference`, `normalize_type_name`, `expand_select_star`. 28 unit tests. |

**Implementation Approach:** Use sqlparser-rs `Tokenizer` to scan body text and identify SQL tokens. Build a token stream and pattern-match against token sequences instead of regex. This handles whitespace, comments, and nested expressions correctly.

### Phase 20.3: Type and Declaration Parsing (4/4) ✅

**Location:** `src/dacpac/model_xml.rs`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.3.1 | Replace DECLARE_TYPE_RE with tokenizer | ✅ | Replaced with `extract_declare_types_tokenized()` using sqlparser-rs tokenizer. Scans for DECLARE keyword followed by @variable and type name. Handles whitespace correctly. Returns base type names in lowercase. 17 unit tests. |
| 20.3.2 | Replace TVF_COL_TYPE_RE with tokenizer | ✅ | Replaced with `parse_tvf_column_type_tokenized()` using sqlparser-rs tokenizer. Parses type strings like INT, NVARCHAR(100), DECIMAL(18,2). Handles MAX keyword, whitespace (tabs/spaces), and case-insensitive matching. Returns TvfColumnTypeInfo struct with data_type, first_num (length/precision), second_num (scale). 17 unit tests. |
| 20.3.3 | Replace CAST_EXPR_RE with tokenizer | ✅ | Replaced with `extract_cast_expressions_tokenized()` using sqlparser-rs tokenizer. Parses CAST(expr AS type) expressions, handling nested parentheses, variable whitespace (spaces/tabs/newlines), and case-insensitive matching. Returns CastExprInfo struct with type_name, cast_start, cast_end, cast_keyword_pos for proper ordering. 17 unit tests. |
| 20.3.4 | Replace bracket trimming with tokenizer | ✅ | Replaced `trim_start_matches('[')` / `trim_end_matches(']')` patterns with tokenized parsing. Created `split_qualified_name_tokenized()` function using sqlparser-rs tokenizer. Updated `split_qualified_name()` and `normalize_object_name()` to use tokenized parsing. Updated `is_builtin_type_reference()` in model_xml.rs to use `normalize_identifier()`. Updated schema name normalization in builder.rs to use `normalize_identifier()`. Handles whitespace (spaces, tabs), double-quoted identifiers, and special characters. 9 unit tests. |

**Implementation Approach:** Parse DECLARE, CAST, and type definitions using sqlparser-rs AST or tokenizer. Extract type names as tokens rather than string manipulation.

### Phase 20.4: Table and Alias Pattern Matching (7/7) ✅

**Location:** `src/dacpac/model_xml.rs`

**Note:** Phase 18.6 completes task 20.4.1 as part of refactoring alias resolution. The `identifier_utils.rs` module created in Phase 18.6 should be reused here.

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.4.1 | Replace TABLE_ALIAS_RE with tokenizer | ✅ | Reused TableAliasTokenParser with new `extract_aliases_with_table_names()` method. Added `default_schema` field to parser. Removed TABLE_ALIAS_RE regex and related helper functions. |
| 20.4.2 | Replace TRIGGER_ALIAS_RE with tokenizer | ✅ | Reused `TableAliasTokenParser::extract_aliases_with_table_names()` in `extract_trigger_body_dependencies()`. Removed TRIGGER_ALIAS_RE regex. Handles whitespace (tabs/spaces/newlines), bracketed and unbracketed table names, AS keyword, and multiple JOINs. 17 unit tests. |
| 20.4.3 | Replace BRACKETED_TABLE_RE with tokenizer | ✅ | Created `extract_table_refs_tokenized()` using `BodyDependencyTokenScanner`. Extracts `[schema].[table]` patterns via `BodyDepToken::TwoPartBracketed`. Handles whitespace (tabs/spaces/newlines), filters @ parameters. Updated `extract_body_dependencies()` to use tokenized extraction. Removed BRACKETED_TABLE_RE regex. 15 unit tests. |
| 20.4.4 | Replace UNBRACKETED_TABLE_RE with tokenizer | ✅ | Same `extract_table_refs_tokenized()` handles `schema.table` patterns via `BodyDepToken::TwoPartUnbracketed`. Filters SQL keywords and table aliases. Removed UNBRACKETED_TABLE_RE regex. |
| 20.4.5 | Replace QUALIFIED_TABLE_NAME_RE with tokenizer | ✅ | Updated `parse_qualified_table_name()` to use `parse_qualified_name_tokenized()`. Handles whitespace between parts, tabs, newlines. Removed QUALIFIED_TABLE_NAME_RE regex. 9 unit tests. |
| 20.4.6 | Replace INSERT_SELECT_RE with tokenizer | ✅ | Created `InsertSelectTokenParser` with token-based parsing. Handles INSERT INTO [schema].[table] ([cols]) SELECT ... FROM inserted/deleted with or without JOIN. Removed INSERT_SELECT_RE and INSERT_SELECT_JOIN_RE regex patterns. 15 unit tests. |
| 20.4.7 | Replace UPDATE_ALIAS_RE with tokenizer | ✅ | Created `UpdateTokenParser` with token-based parsing. Handles UPDATE alias SET ... FROM [schema].[table] alias (INNER) JOIN inserted/deleted alias ON ... patterns. Removed UPDATE_ALIAS_RE regex. 15 unit tests. |

**Implementation Approach:** Use sqlparser-rs to parse FROM clauses, JOIN clauses, and table references. Extract table names and aliases from AST nodes rather than regex pattern matching.

### Phase 20.5: SQL Keyword Detection (6/6) ✅

**Location:** `src/dacpac/model_xml.rs`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.5.1 | Replace AS_KEYWORD_RE with tokenizer | ✅ | Replaced with `find_function_body_as_tokenized()` using sqlparser-rs tokenizer. Scans for AS keyword after RETURNS, validates it's followed by body-starting keywords (BEGIN, RETURN, SELECT, etc.). Handles whitespace (tabs, spaces, newlines), case-insensitive matching. Updated `extract_function_body()` and `extract_function_header()` to use tokenized parsing. 20 unit tests. |
| 20.5.2 | Replace find_body_separator_as() with tokenizer | ✅ | Replaced with `find_procedure_body_separator_as_tokenized()` using sqlparser-rs tokenizer. Scans for AS keyword followed by body-starting keywords (BEGIN, SET, SELECT, etc.). Updated `extract_procedure_body_only()` to use tokenized parsing. Removed old `find_body_separator_as()` function. 26 unit tests. |
| 20.5.3 | Replace starts_with() SQL keyword checks with tokenizer | ✅ | Completed as part of 20.5.2 - the `starts_with()` checks were inside `find_body_separator_as()` which was completely replaced with token-based parsing. |
| 20.5.4 | Replace ON_KEYWORD_RE with tokenizer | ✅ | Replaced with `extract_on_clause_boundaries_tokenized()` using sqlparser-rs tokenizer. Scans for ON keyword, handles termination at WHERE, GROUP, ORDER, HAVING, UNION, JOIN keywords, and semicolons. Updated `extract_join_on_columns()` to use tokenized boundary detection. Removed ON_KEYWORD_RE and ON_TERMINATOR_RE regex patterns. 18 unit tests. |
| 20.5.5 | Replace GROUP_BY_RE with tokenizer | ✅ | Replaced with `extract_group_by_clause_boundaries_tokenized()` using sqlparser-rs tokenizer. Scans for GROUP followed by BY keyword, handles whitespace (tabs/spaces/newlines), case-insensitive matching. Removed GROUP_BY_RE regex. 18 unit tests. |
| 20.5.6 | Replace GROUP_TERMINATOR_RE with tokenizer | ✅ | Same `extract_group_by_clause_boundaries_tokenized()` handles termination at HAVING, ORDER, UNION keywords, and semicolons. Removed GROUP_TERMINATOR_RE regex. |

**Implementation Approach:** Scan SQL body text with tokenizer and identify keywords as `Token::Word` instances. Check token values instead of string prefix/suffix matching.

### Phase 20.6: Semicolon and Whitespace Handling (3/3) ✅

**Location:** Multiple files

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.6.1 | Replace trim_end_matches(';') in tsql_parser.rs | ✅ | Replaced with `extract_index_filter_predicate_tokenized()` in index_parser.rs. Token-based parsing stops at SemiColon tokens, no string manipulation needed. 17 unit tests. |
| 20.6.2 | Replace trim_end_matches(';') in builder.rs | ✅ | Uses same `extract_index_filter_predicate_tokenized()` function. Removed regex-based `extract_filter_predicate_from_sql()`. |
| 20.6.3 | Replace trim_end_matches([';', ' ']) in model_xml.rs | ✅ | Already completed in a previous phase - no `trim_end_matches` call exists at the referenced location. |

**Implementation Approach:** Created `extract_index_filter_predicate_tokenized()` in `index_parser.rs` using sqlparser-rs tokenizer. Scans for WHERE keyword after closing parenthesis, collects tokens until WITH/semicolon/end, reconstructs predicate string without semicolons. Handles tabs, multiple spaces, and newlines correctly.

### Phase 20.7: CTE and Subquery Pattern Matching (4/4) ✅

**Location:** `src/dacpac/model_xml.rs`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.7.1 | Replace CTE_ALIAS_RE with tokenizer | ✅ | Implemented via `extract_cte_aliases()` method (lines 3794-3849) using `TableAliasTokenParser`. Detects WITH keyword, handles RECURSIVE, parses multiple comma-separated CTEs, uses `skip_balanced_parens()` for CTE bodies. 20 unit tests passing. |
| 20.7.2 | Replace SUBQUERY_ALIAS_RE with tokenizer | ✅ | Implemented via `try_parse_subquery_alias()` method (lines 4018-4105). Token-based detection of `) AS alias` and `) alias` patterns. Filters SQL keywords to avoid false matches. 4 unit tests passing. |
| 20.7.3 | Replace APPLY_KEYWORD_RE with tokenizer | ✅ | Implemented in `extract_all_aliases()` method (lines 3655-3670). Uses `check_word_ci("CROSS")`, `check_word_ci("OUTER")`, and `check_keyword(Keyword::APPLY)` for detection. 3 unit tests passing. |
| 20.7.4 | Replace APPLY_FUNCTION_ALIAS_RE with tokenizer | ✅ | APPLY aliases captured via the `) AS/alias` pattern in `try_parse_subquery_alias()`. Same tokenized approach as subquery aliases. |

**Implementation Approach:** All patterns replaced with token-based parsing using `TableAliasTokenParser` struct and sqlparser-rs tokenizer. CTE extraction runs as first pass, followed by table/subquery alias extraction in second pass.

### Phase 20.8: Fix Alias Resolution Bugs in BodyDependencies (11/11) ✅

**Location:** `src/dacpac/model_xml.rs` - `extract_table_aliases_for_body_deps()` and related functions

**Background:** Integration tests in `tests/integration/dacpac/alias_resolution_tests.rs` expose bugs where table aliases defined in nested contexts (subqueries, CTEs, APPLY clauses) are not properly tracked. When an alias is not found in the alias map, references like `[ALIAS].[Column]` are incorrectly emitted as `[dbo].[PreviousTable].[ALIAS]` instead of being resolved or excluded.

**Test Fixture:** `tests/fixtures/body_dependencies_aliases/`

| ID | Task | Status | Test | Notes |
|----|------|--------|------|-------|
| 20.8.1 | Fix STUFF() nested subquery alias extraction | ✅ | `test_stuff_nested_subquery_alias_resolution` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.2 | Fix multi-level nested subquery alias extraction | ✅ | `test_nested_subquery_alias_resolution` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.3 | Fix CROSS/OUTER APPLY alias extraction in views | ✅ | `test_apply_clause_alias_resolution` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.4 | Exclude CTE names from view dependencies | ✅ | `test_cte_alias_recognition` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.5 | Fix EXISTS/NOT EXISTS subquery alias extraction | ✅ | `test_exists_subquery_alias_resolution` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.6 | Fix IN clause subquery alias extraction | ✅ | `test_in_subquery_alias_resolution` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.7 | Fix correlated subquery alias extraction in SELECT | ✅ | `test_correlated_subquery_alias_resolution` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.8 | Fix CASE expression subquery alias extraction | ✅ | `test_case_subquery_alias_resolution` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.9 | Fix derived table chain alias extraction | ✅ | `test_derived_table_chain_alias_resolution` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.10 | Exclude recursive CTE self-references from dependencies | ✅ | `test_recursive_cte_alias_resolution` | Fixed via table alias filtering in `extract_all_column_references()` |
| 20.8.11 | Fix MERGE TARGET/SOURCE alias handling | ✅ | `test_merge_alias_resolution` | Added MERGE keyword detection to `TableAliasTokenParser::extract_all_aliases()`. Extracts `MERGE INTO [table] AS [alias]` for TARGET alias. USING subquery alias captured by existing `) AS alias` pattern. Inner FROM/JOIN clauses properly scanned. |

**Fix Applied (20.8.1-20.8.10):**

The fix was implemented in the `extract_all_column_references()` function in `src/dacpac/model_xml.rs`. The root cause was that single bracketed identifiers like `[ITTAG]` that are actually table aliases were being incorrectly treated as column names.

**Solution:** Before treating a `SingleBracketed` token as a column reference, the function now checks if the identifier (case-insensitively) matches any known table alias in the `alias_names` set. If it matches a table alias, it is skipped rather than being added as a column reference. This prevents table aliases from being misinterpreted as column dependencies.

**Fix Applied (20.8.11 - MERGE alias handling):**

The fix for MERGE statements was implemented in `TableAliasTokenParser::extract_all_aliases()` in `src/dacpac/model_xml.rs`:
1. Added MERGE keyword detection to identify MERGE statements
2. Extracts `MERGE INTO [table] AS [alias]` pattern to capture the TARGET alias as a table alias
3. The `USING (subquery) AS [alias]` pattern is handled by the existing `) AS alias` pattern handler which captures the SOURCE alias
4. Inner FROM/JOIN clauses inside the USING subquery are properly scanned and their aliases are captured by the main loop

**Original Implementation Approach (for reference):**

The original root cause was that `extract_table_aliases_for_body_deps()` uses regex patterns that only capture aliases from top-level FROM/JOIN clauses. Aliases defined in:
- Nested subqueries (any depth)
- APPLY clause subqueries
- CTE definitions
- EXISTS/IN clause subqueries
- CASE expression subqueries

...are not added to the alias map. When `[ALIAS].[Column]` is encountered, the alias lookup fails and the reference is incorrectly constructed.

**Validation:** All 11 tests pass. Phase 20.8 is complete.

---

## Phase 21: Split model_xml.rs into Submodules (5/10)

**Location:** `src/dacpac/model_xml/mod.rs` (~12,600 lines after 21.3.1)

**Goal:** Break up the largest file in the codebase into logical submodules for improved maintainability, faster compilation, and easier navigation.

**Background:** The `model_xml.rs` file has grown to 13,413 lines containing XML generation, SQL parsing helpers, body dependency extraction, type handling, and ~4,600 lines of tests. These are distinct concerns that should be separated.

### Phase 21.1: Create Module Structure (2/2) ✅

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.1.1 | Create `src/dacpac/model_xml/` directory with `mod.rs` | ✅ | Moved model_xml.rs to model_xml/mod.rs. Public API (generate_model_xml) is re-exported from dacpac/mod.rs. |
| 21.1.2 | Move `generate_model_xml()` entry point to mod.rs | ✅ | Entry point remains in mod.rs. All 492 unit tests + 116 e2e tests pass. |

### Phase 21.2: Extract XML Writing Helpers (2/2) ✅

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.2.1 | Create `xml_helpers.rs` with low-level XML utilities | ✅ | Created with `write_property`, `write_script_property`, `write_relationship`, `write_builtin_type_relationship`, `write_schema_relationship`, `write_type_specifier_builtin`, `normalize_script_content`, `is_builtin_schema`, `BUILTIN_SCHEMAS`. 244 lines including 9 unit tests. |
| 21.2.2 | Create `header.rs` with header/metadata writing | ✅ | Created with `write_header`, `write_custom_data`, `write_database_options`, `write_package_reference`, `write_sqlcmd_variables`, `extract_dacpac_name`. 324 lines including 9 unit tests. |

### Phase 21.3: Extract Element Writers (1/3)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.3.1 | Create `table_writer.rs` for table/column XML | ✅ | 650 lines including 10 unit tests. Extracted: `write_table`, `write_column`, `write_computed_column`, `write_column_with_type`, `write_type_specifier`, `sql_type_to_reference`, `write_column_type_specifier`, `write_table_type_column_with_annotation`, `write_table_type_relationship`, `parse_qualified_table_name`, `is_builtin_type_reference`, `write_expression_dependencies`. |
| 21.3.2 | Create `view_writer.rs` for view XML | ⬜ | `write_view`, `write_view_columns`, `extract_view_*` functions, `ViewColumn` struct (~700 lines) |
| 21.3.3 | Create `programmability_writer.rs` for procs/functions | ⬜ | `write_procedure`, `write_function`, parameter extraction, `write_dynamic_objects` (~800 lines) |

### Phase 21.4: Extract Body Dependencies (0/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.4.1 | Create `body_deps.rs` for dependency extraction | ⬜ | `BodyDependencyTokenScanner`, `extract_body_dependencies`, `extract_table_aliases_for_body_deps`, `TableAliasTokenParser` (~1,500 lines) |
| 21.4.2 | Create `qualified_name.rs` for name parsing | ⬜ | `QualifiedName` struct and impl, `parse_qualified_name_tokenized` (~300 lines) |

### Phase 21.5: Extract Remaining Writers (0/1)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 21.5.1 | Create `other_writers.rs` for remaining elements | ⬜ | `write_index`, `write_constraint`, `write_sequence`, `write_trigger`, `write_fulltext_*`, `write_table_type_*`, `write_extended_property` (~1,200 lines) |

**Implementation Approach:**

1. Create module directory structure first
2. Move functions one module at a time, starting with lowest-dependency utilities
3. Use `pub(crate)` for internal functions, `pub` only for external API
4. Keep tests with their corresponding modules (split `mod tests` accordingly)
5. Update imports incrementally, running tests after each move
6. Final mod.rs should only contain `generate_model_xml()` and re-exports

**Validation:** All existing tests must pass after each phase. No functional changes.

**Expected Result:**

| Module | Estimated Lines | Purpose |
|--------|-----------------|---------|
| `mod.rs` | ~200 | Entry point, re-exports |
| `xml_helpers.rs` | ~244 | Low-level XML utilities |
| `header.rs` | ~324 | Header/metadata generation |
| `table_writer.rs` | ~650 | Table/column XML (extracted in 21.3.1) |
| `view_writer.rs` | ~700 | View XML and column extraction |
| `programmability_writer.rs` | ~800 | Procedures/functions |
| `body_deps.rs` | ~1,500 | Body dependency extraction |
| `qualified_name.rs` | ~300 | Qualified name parsing |
| `other_writers.rs` | ~1,200 | Index/constraint/trigger/etc |
| Tests (distributed) | ~2,400 | Unit tests per module |

---

### Implementation Notes

**Benefits of tokenization over regex:**
- Handles variable whitespace (tabs, multiple spaces, newlines) correctly
- Respects SQL comments and string literals
- More maintainable and easier to extend
- Better error messages when parsing fails
- Faster performance on complex patterns

**Migration Strategy:**
1. Create new token-based parsers alongside existing regex patterns
2. Add unit tests for token-based implementations
3. Switch production code to use token-based parsers
4. Remove regex patterns after validation
5. Update performance benchmarks to measure impact

---

<details>
<summary>Completed Phases Summary (Phases 1-20.1)</summary>

## Phase Overview

| Phase | Description | Tasks |
|-------|-------------|-------|
| Phase 1-9 | Core implementation (properties, relationships, XML structure, metadata) | 58/58 |
| Phase 10 | Fix extended properties, function classification, constraint naming, SqlPackage config | 5/5 |
| Phase 11 | Fix remaining parity failures, error fixtures, ignored tests | 70/70 |
| Phase 12 | SELECT * expansion, TVF columns, duplicate refs | 6/6 |
| Phase 13 | Fix remaining relationship parity issues (TVP support) | 4/4 |
| Phase 14 | Layer 3 (SqlPackage) parity | 3/3 |
| Phase 15 | Parser refactoring: replace regex with token-based parsing | 34/34 |
| Phase 16 | Performance tuning: benchmarks, regex caching, parallelization | 18/18 |
| Phase 17 | Real-world SQL compatibility: comma-less constraints, SQLCMD format | 5/5 |
| Phase 18 | BodyDependencies alias resolution: fix table alias handling | 15/15 |
| Phase 19 | Whitespace-agnostic trim patterns: token-based TVP parsing | 3/3 |
| Phase 20.1 | Token-based parameter parsing for procedures and functions | 3/3 |

## Key Implementation Details

### Phase 19: Whitespace-Agnostic Trim Patterns (3/3)

Replaced space-only `trim_end_matches()` patterns with token-based parsing to handle tabs and multiple spaces.

**19.1: TVP Parameter Whitespace Handling (3/3)**

Refactored `clean_data_type()` function in `src/dacpac/model_xml.rs` to use sqlparser-rs tokenization:
- Token-based scanning handles tabs, multiple spaces, mixed whitespace
- Trailing keyword detection: READONLY, NULL, NOT NULL
- Case-insensitive keyword matching
- Preserves schema-qualified types like `[dbo].[TableType]`
- 18 unit tests covering all whitespace variations

**Location:** `src/dacpac/model_xml.rs` in TVP parameter parsing

### Phase 20.1: Parameter Parsing (3/3)

Replaced regex-based parameter parsing with token-based approach for procedures and functions.

**Key Changes:**

**20.1.1: Procedure Parameter Parsing**
- Extended `ProcedureTokenParser` with full parameter parsing in `src/parser/procedure_parser.rs`
- New `TokenParsedProcedureParameter` struct with fields: name, data_type, is_output, is_readonly, default_value
- Handles simple types (INT, VARCHAR), complex types (DECIMAL(18,2)), schema-qualified types (`[dbo].[TableType]`)
- Detects OUTPUT/OUT, READONLY, and default values
- Whitespace-agnostic (handles tabs, multiple spaces, newlines)
- 42 unit tests

**20.1.2: Function Parameter Parsing**
- Added `extract_function_parameters_tokens()` in `src/parser/function_parser.rs`
- Replaced FUNC_PARAM_RE regex pattern
- 9 unit tests covering all parameter variations

**20.1.3: Consistent Parameter Storage**
- Parameter names now stored WITHOUT `@` prefix for both procedures and functions
- Simplified parameter matching in `extract_body_dependencies()`
- Removed PROC_PARAM_RE and FUNC_PARAM_RE regex patterns from model_xml.rs

### Remaining Hotspots

| Area | Location | Issue | Impact | Status |
|------|----------|-------|--------|--------|
| Cloning | `src/model/builder.rs` | 149 clone() calls | MEDIUM | ⬜ |

</details>
