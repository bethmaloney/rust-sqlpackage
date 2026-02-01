# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Status: PARITY COMPLETE | REAL-WORLD COMPATIBILITY IN PROGRESS

**Phases 1-20.1 complete (224 tasks). Full parity achieved.**

**Current Focus: Phase 20 - Replace Remaining Regex with Tokenization/AST**
- ✅ Phase 20.1 complete: Token-based parameter parsing (3/3 tasks)
- 🔄 Phase 20.2-20.7: Body dependency, type, table, keyword, and CTE parsing (25 tasks remaining)

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

### Phase 20.2: Body Dependency Token Extraction (7/8)

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
| 20.2.8 | Replace split('.') with qualified name parser | ⬜ | Lines 1563, 1578-1580, 1819, 1848, 2333: Use tokenizer for dotted names |

**Implementation Approach:** Use sqlparser-rs `Tokenizer` to scan body text and identify SQL tokens. Build a token stream and pattern-match against token sequences instead of regex. This handles whitespace, comments, and nested expressions correctly.

### Phase 20.3: Type and Declaration Parsing (0/4)

**Location:** `src/dacpac/model_xml.rs`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.3.1 | Replace DECLARE_TYPE_RE with tokenizer | ⬜ | Line 105-107: `DECLARE @var type` extraction |
| 20.3.2 | Replace TVF_COL_TYPE_RE with tokenizer | ⬜ | Line 55-57: TVF column type with precision/scale |
| 20.3.3 | Replace CAST_EXPR_RE with tokenizer | ⬜ | Line 141-142: `CAST(expr AS type)` extraction |
| 20.3.4 | Replace bracket trimming with tokenizer | ⬜ | Lines 754-755, 748-749: trim_start_matches('['), trim_end_matches(']') |

**Implementation Approach:** Parse DECLARE, CAST, and type definitions using sqlparser-rs AST or tokenizer. Extract type names as tokens rather than string manipulation.

### Phase 20.4: Table and Alias Pattern Matching (0/7)

**Location:** `src/dacpac/model_xml.rs`

**Note:** Phase 18.6 completes task 20.4.1 as part of refactoring alias resolution. The `identifier_utils.rs` module created in Phase 18.6 should be reused here.

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.4.1 | Replace TABLE_ALIAS_RE with tokenizer | ⬜ | Completed in Phase 18.6.5 - reuse that approach |
| 20.4.2 | Replace TRIGGER_ALIAS_RE with tokenizer | ⬜ | Line 149-151: Trigger table aliases |
| 20.4.3 | Replace BRACKETED_TABLE_RE with tokenizer | ⬜ | Line 110-111: `[schema].[table]` pattern |
| 20.4.4 | Replace UNBRACKETED_TABLE_RE with tokenizer | ⬜ | Line 114-116: `schema.table` pattern |
| 20.4.5 | Replace QUALIFIED_TABLE_NAME_RE with tokenizer | ⬜ | Line 47-48: `^\[schema\]\.\[table\]$` |
| 20.4.6 | Replace INSERT_SELECT_RE with tokenizer | ⬜ | Line 161-166: Complex INSERT...SELECT pattern |
| 20.4.7 | Replace UPDATE_ALIAS_RE with tokenizer | ⬜ | Line 177-182: UPDATE with JOIN pattern |

**Implementation Approach:** Use sqlparser-rs to parse FROM clauses, JOIN clauses, and table references. Extract table names and aliases from AST nodes rather than regex pattern matching.

### Phase 20.5: SQL Keyword Detection (0/6)

**Location:** `src/dacpac/model_xml.rs`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.5.1 | Replace AS_KEYWORD_RE with tokenizer | ⬜ | Line 145-146: Find AS keyword in function body |
| 20.5.2 | Replace find_body_separator_as() with tokenizer | ⬜ | Lines 4022-4076: Manual character scanning for AS |
| 20.5.3 | Replace starts_with() SQL keyword checks with tokenizer | ⬜ | Lines 4054-4065: BEGIN, RETURN, SELECT, etc. |
| 20.5.4 | Replace ON_KEYWORD_RE with tokenizer | ⬜ | Line 68: `ON` keyword in JOIN clauses |
| 20.5.5 | Replace GROUP_BY_RE with tokenizer | ⬜ | Line 81: `GROUP BY` keyword |
| 20.5.6 | Replace terminator patterns with tokenizer | ⬜ | Lines 71-73, 84-85: WHERE, HAVING, ORDER, etc. |

**Implementation Approach:** Scan SQL body text with tokenizer and identify keywords as `Token::Word` instances. Check token values instead of string prefix/suffix matching.

### Phase 20.6: Semicolon and Whitespace Handling (0/3)

**Location:** Multiple files

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.6.1 | Replace trim_end_matches(';') in tsql_parser.rs | ⬜ | Line 1472: Predicate semicolon removal |
| 20.6.2 | Replace trim_end_matches(';') in builder.rs | ⬜ | Line 1647: Predicate semicolon removal |
| 20.6.3 | Replace trim_end_matches([';', ' ']) in model_xml.rs | ⬜ | Line 1525: Table name cleanup |

**Implementation Approach:** Use tokenizer to parse statements. Semicolons and whitespace are automatically handled as separate tokens. Extract statement content without string manipulation.

### Phase 20.7: CTE and Subquery Pattern Matching (0/4)

**Location:** `src/dacpac/model_xml.rs`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.7.1 | Replace CTE_ALIAS_RE with tokenizer | ⬜ | Line 3235: `WITH CteName AS (` pattern |
| 20.7.2 | Replace SUBQUERY_ALIAS_RE with tokenizer | ⬜ | Line 3211: Derived table alias detection |
| 20.7.3 | Replace APPLY_KEYWORD_RE with tokenizer | ⬜ | Line 3215: CROSS/OUTER APPLY detection |
| 20.7.4 | Replace APPLY_FUNCTION_ALIAS_RE with tokenizer | ⬜ | Line 3221-3229: APPLY subquery alias extraction |

**Implementation Approach:** Parse WITH clauses, subqueries, and APPLY expressions using sqlparser-rs AST. Extract CTE names and subquery aliases from the syntax tree.

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
