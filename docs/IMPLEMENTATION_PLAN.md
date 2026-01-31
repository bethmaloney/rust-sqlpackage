# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Status: PARITY COMPLETE | REAL-WORLD COMPATIBILITY IN PROGRESS

**Phases 1-17 complete (203 tasks). Full parity achieved.**
**Phase 18 in progress: BodyDependencies alias resolution (10/15 tasks complete).**
**Phase 19 pending: Whitespace-agnostic trim patterns (0/3 tasks, lower priority).**

**⚠️ Critical:** Phase 18.5 blocks real-world deployment. Unqualified table names cause invalid alias references.

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

## Benchmark Commands

```bash
cargo bench                                  # Run all benchmarks
cargo bench --bench pipeline                 # Run specific benchmark
cargo bench -- --save-baseline before        # Save baseline for comparison
cargo bench -- --baseline before             # Compare against baseline
```

---

## Phase 18: BodyDependencies Alias Resolution

**Goal:** Fix BodyDependencies extraction to correctly resolve table aliases instead of treating them as schema references.

**Background:** When extracting BodyDependencies from procedures, views, and functions, the current implementation incorrectly includes table aliases (like `[A]`, `[ATTAG]`, `[TagDetails]`) as schema references instead of resolving them to actual table names. This causes deployment failures when SqlPackage tries to resolve non-existent objects.

**Problem Examples:**

1. **Table aliases treated as schemas:**
   ```sql
   FROM [dbo].[Account] A
   WHERE A.Id = @AccountId  -- Generates [A].[Id] instead of [dbo].[Account].[Id]
   ```

2. **Subquery aliases treated as schemas:**
   ```sql
   LEFT JOIN (...) AS TagDetails ON TagDetails.AccountId = A.Id
   -- Generates [TagDetails].[AccountId] instead of omitting subquery alias references
   ```

3. **SQL keywords incorrectly parsed:**
   ```sql
   STUFF((SELECT ... FOR XML PATH('')), 1, 1, '')
   -- Generates [dbo].[Account].[STUFF], [dbo].[Account].[FOR], [dbo].[Account].[PATH]
   ```

**Test:** `test_parity_body_dependencies_aliases` in `tests/e2e/dotnet_comparison_tests.rs`
**Fixture:** `tests/fixtures/body_dependencies_aliases/`

### Phase 18.1: Alias Tracking Infrastructure (3/3) ✅

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 18.1.1 | Create alias tracking structure for FROM clause parsing | ✅ | Track `alias -> schema.table` mappings |
| 18.1.2 | Parse FROM clauses to extract table aliases | ✅ | Handle JOIN, OUTER APPLY, subqueries |
| 18.1.3 | Parse subquery aliases in JOIN expressions | ✅ | Track derived table aliases |

### Phase 18.2: Alias Resolution in Body Dependencies (4/4) ✅

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 18.2.1 | Resolve single-letter aliases (A, I, T) to actual tables | ✅ | `[A].[Id]` → `[dbo].[Account].[Id]` |
| 18.2.2 | Resolve multi-letter aliases (ATTAG, TagDetails) | ✅ | Same resolution logic |
| 18.2.3 | Skip subquery/derived table alias references | ✅ | Don't emit refs to `TagDetails.AccountId` |
| 18.2.4 | Filter out SQL keywords from body dependencies | ✅ | Remove STUFF, FOR, PATH, XML, etc. |

### Phase 18.3: Edge Cases (3/3) ✅

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 18.3.1 | Handle OUTER APPLY and CROSS APPLY aliases | ✅ | Subquery results used as table references |
| 18.3.2 | Handle CTE (Common Table Expression) aliases | ✅ | WITH clause table expressions |
| 18.3.3 | Handle nested subquery aliases | ✅ | Multiple levels of aliasing |

**Implementation Notes (18.3.1 - APPLY Aliases):**

The following changes were made to handle CROSS APPLY and OUTER APPLY subquery aliases:

1. **`alias.[column]` pattern in TOKEN_RE** - Added pattern to match cases like `tag.[Name]` where the alias is unbracketed but the column is bracketed.

2. **Boundary handling for `alias.[column]`** - Added boundary `(?:^|[^@\w\]])` to the pattern to prevent partial matches (e.g., avoiding matching `@tag` as `tag`).

3. **`APPLY_KEYWORD_RE` pattern** - Added regex to detect CROSS APPLY and OUTER APPLY keywords in SQL text.

4. **`APPLY_SUBQUERY_ALIAS_RE` pattern** - Added regex to extract aliases from APPLY subqueries, including aliases without the AS keyword (e.g., `CROSS APPLY (...) d`).

5. **Parenthesis counting logic** - Added logic to find the matching closing parenthesis for APPLY subqueries by counting open/close parens.

6. **`ALIAS_AFTER_PAREN_RE` pattern** - Added regex to extract the alias that appears after balanced parentheses in APPLY expressions.

**Implementation Notes (18.3.2 - CTE Aliases):**

The following changes were made to handle CTE (Common Table Expression) aliases:

1. **`CTE_ALIAS_RE` pattern** - Added regex to extract CTE aliases from WITH clauses: `WITH CteName AS (` and `, NextCte AS (`.

2. **CTE alias extraction in `extract_table_aliases_for_body_deps()`** - Added loop to extract CTE names and add them to `subquery_aliases` set so references like `[AccountCte].[Id]` are skipped rather than treated as schema.table references.

3. **`strip_sql_comments_for_body_deps()` function** - Added function to strip SQL comments (both `--` line comments and `/* */` block comments) from body text before dependency extraction. This prevents words in comments from being treated as column/table references.

4. **Added `WITH` to `is_sql_keyword_not_column()`** - Added WITH keyword to the filter to prevent it from being treated as a column name.

5. **Unit tests** - Added `test_extract_table_aliases_cte_single`, `test_extract_table_aliases_cte_multiple`, and `test_body_dependencies_cte_alias_resolution` tests.


**Implementation Notes (18.3.3 - Nested Subquery Aliases):**

The following changes were made to handle nested subquery aliases:

1. **`[alias].column` pattern in TOKEN_RE** - Added pattern for `[alias].column` (bracketed alias with unbracketed column) to the TOKEN_RE regex to match cases like `[TAG].Id` which appear in nested subqueries.

2. **Updated group numbers** - Adjusted capture group numbers to accommodate the new pattern (groups 13-15).

3. **Handler in `extract_body_dependencies()`** - Added corresponding handler that resolves bracketed aliases to actual tables by looking up the alias in the alias map.

4. **Unit tests** - Added `test_extract_table_aliases_nested_subquery` and `test_body_dependencies_nested_subquery_alias_resolution` tests to verify the behavior.

### Phase 18.4: DotNet Compatibility (0/2)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 18.4.1 | Allow duplicate references like DotNet | ⬜ | Remove deduplication |
| 18.4.2 | Match DotNet's ordering of references | ⬜ | Preserve reference order |

### Phase 18.5: Unqualified Table Name Support (0/3)

**Critical Bug:** Alias resolution fails when table names are not schema-qualified. This causes deployment failures in real-world projects.

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 18.5.1 | Fix TABLE_ALIAS_RE to match unqualified table names | ⬜ | `FROM Account A` not just `FROM [dbo].[Account] A` |
| 18.5.2 | Update extract_table_aliases_for_body_deps() for unqualified names | ⬜ | Resolve `Account` to `[dbo].[Account]` using default schema |
| 18.5.3 | Add schema inference for unqualified table references | ⬜ | Look up table in DatabaseModel or assume [dbo] schema |

**Test Coverage:**
- `GetAccountUnqualified.sql` - Procedure with `FROM Account A`
- `AccountSummaryUnqualified.sql` - View with unqualified joins
- `GetAccountCountUnqualified.sql` - Function with unqualified table

**Current Failure:** Generates invalid references like `[A].[Id]` instead of `[dbo].[Account].[Id]`

**Example:**
```sql
FROM Account A          -- Unqualified (broken)
WHERE A.Id = @Id        -- Generates [A].[Id] ✗

FROM [dbo].[Account] A  -- Qualified (works)
WHERE A.Id = @Id        -- Generates [dbo].[Account].[Id] ✓
```

### Implementation Notes

The following changes were made to implement alias resolution:

1. **`extract_table_aliases_for_body_deps()` function** - Added to track table aliases from FROM clauses, mapping alias names to their `schema.table` targets.

2. **`extract_column_aliases_for_body_deps()` function** - Added to track column aliases from AS patterns in SELECT lists, preventing these from being treated as table references.

3. **Modified `extract_body_dependencies()`** - Updated to resolve aliases to actual tables instead of treating single-letter and multi-letter aliases as schema references.

4. **Subquery alias filtering** - Added logic to identify and skip references to derived table aliases (e.g., `TagDetails`, `AccountTags`) which are subquery result sets, not actual tables.

5. **Column alias filtering** - Added detection for `AS identifier` patterns to prevent column aliases from appearing as body dependencies.

6. **SQL keyword filtering** - Extended the keyword filter to include `STUFF`, `FOR`, `PATH`, `STRING_AGG`, and other SQL functions/keywords that were incorrectly appearing as body dependencies.

---

## Phase 19: Whitespace-Agnostic Trim Patterns

**Goal:** Replace space-only `trim_end_matches()` patterns with token-based parsing to handle tabs and multiple spaces.

**Location:** `src/dacpac/model_xml.rs` in TVP parameter parsing

### Phase 19.1: TVP Parameter Whitespace Handling (0/3)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 19.1.1 | Fix `trim_end_matches(" READONLY")` | ⬜ | Line ~2728, use token-based detection |
| 19.1.2 | Fix `trim_end_matches(" NULL")` | ⬜ | Line ~2729, use token-based detection |
| 19.1.3 | Fix `trim_end_matches(" NOT")` | ⬜ | Line ~2730, use token-based detection |

**Implementation Approach:** Tokenize the parameter string and check for trailing `READONLY`, `NULL`, or `NOT NULL` tokens rather than using string suffix matching.

---

## Phase 20: Replace Remaining Regex with Tokenization/AST

**Goal:** Eliminate remaining regex patterns in favor of tokenizer-based or AST-based parsing for better maintainability and correctness.

**Background:** Phase 15 converted many regex patterns to token-based parsing, but several complex patterns remain in `src/dacpac/model_xml.rs` and other modules. These patterns are fragile and can fail on edge cases involving tabs, multiple spaces, or nested expressions.

### Phase 20.1: Parameter Parsing (0/3)

**Location:** `src/dacpac/model_xml.rs`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.1.1 | Replace PROC_PARAM_RE with token-based parser | ⬜ | Line 92-96: Complex regex for `@param type READONLY/OUTPUT` |
| 20.1.2 | Replace FUNC_PARAM_RE with token-based parser | ⬜ | Line 100-102: Function parameter extraction |
| 20.1.3 | Replace parameter name trim_start_matches('@') | ⬜ | Lines 2551, 2575, 2938: Use tokenizer to identify parameters |

**Implementation Approach:** Create a `ParameterParser` using sqlparser-rs tokenization to extract parameter declarations from procedure/function signatures. Parse parameter attributes (OUTPUT, READONLY) as tokens rather than regex captures.

### Phase 20.2: Body Dependency Token Extraction (0/8)

**Location:** `src/dacpac/model_xml.rs`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.2.1 | Replace TOKEN_RE with tokenizer-based scanning | ⬜ | Lines 129-134: Massive regex with 17 capture groups |
| 20.2.2 | Replace COL_REF_RE with tokenizer | ⬜ | Line 77-78: `alias.column` or `schema.table.column` |
| 20.2.3 | Replace BARE_COL_RE with tokenizer | ⬜ | Line 88-89: `[ColumnName]` not preceded by dot |
| 20.2.4 | Replace BRACKETED_IDENT_RE with tokenizer | ⬜ | Line 137-138: `[Name]` pattern |
| 20.2.5 | Replace ALIAS_COL_RE with tokenizer | ⬜ | Line 157-158: `alias.[column]` pattern |
| 20.2.6 | Replace SINGLE_BRACKET_RE with tokenizer | ⬜ | Line 154: `[name]` single identifier |
| 20.2.7 | Replace COLUMN_ALIAS_RE with tokenizer | ⬜ | Line 3469: Column alias detection |
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

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 20.4.1 | Replace TABLE_ALIAS_RE with tokenizer | ⬜ | Line 60-65: `FROM/JOIN table alias` extraction |
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
<summary>Completed Phases Summary (Phases 1-17)</summary>

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
| Phase 18 | BodyDependencies alias resolution: fix table alias handling | 10/15 |
| Phase 19 | Whitespace-agnostic trim patterns (lower priority) | 0/3 |
| Phase 20 | Replace remaining regex with tokenization/AST (cleanup) | 0/35 |

## Performance Metrics

Benchmarks run on criterion 0.5 with 100 samples per measurement.

| Fixture | Files | Mean Time | Notes |
|---------|-------|-----------|-------|
| e2e_simple | minimal | **19.4 ms** | Minimal project baseline |
| e2e_comprehensive | 30 | **85.8 ms** | Production-realistic project |
| stress_test | 135 | **462.1 ms** | High file count stress test |

**Comparison with .NET DacFx:** rust-sqlpackage is **27x faster cold / 9x faster warm** than .NET DacFx.

## Key Implementation Details

### Phase 11: Parity Failures & Error Fixtures
- Fixed Layer 1-4 and relationship parity across all fixtures
- Excluded `external_reference` and `unresolved_reference` from parity testing (DotNet cannot build them)
- Fixed table type indexes, default constraints, and inline annotations
- Removed all `#[ignore]` attributes from passing tests

### Phase 12: Relationship Parity
- **SELECT * expansion**: Added `expand_select_star()` function to look up table columns from DatabaseModel
- **Duplicate references**: Removed deduplication in triggers and views to preserve duplicates in GROUP BY
- **CAST type references**: Added extraction of type references from CAST expressions in computed columns
- **TVF Columns**: Added `Columns` relationship for inline and multi-statement table-valued functions

### Phase 13: TVP Support
- Full table-valued parameter (TVP) support for procedures
- DynamicObjects relationship with SqlDynamicColumnSource elements
- Parameter parsing for `[schema].[type]` format and READONLY keyword
- TVP column reference extraction for BodyDependencies

### Phase 14: Layer 3 SqlPackage Parity
- Fixed DefaultFilegroup relationship in SqlDatabaseOptions
- Added missing database options properties (Collation, IsTornPageProtectionOn, DefaultLanguage, etc.)
- Changed IsFullTextEnabled default from False to True to match DotNet

### Phase 15: Parser Refactoring
Replaced regex-based fallback parsing with token-based parsing using sqlparser-rs custom dialect:
- **15.1**: Created `ExtendedTsqlDialect` wrapper in `src/parser/tsql_dialect.rs`
- **15.2**: Token-based column parsing in `src/parser/column_parser.rs` (D1-D3, E1-E2)
- **15.3**: Token-based DDL object extraction (B1-B8) - procedures, functions, triggers, sequences, types, indexes, fulltext
- **15.4**: Token-based constraint parsing in `src/parser/constraint_parser.rs` (C1-C4)
- **15.5**: Token-based statement detection in `src/parser/statement_parser.rs` (A1-A5)
- **15.6**: Token-based extended property parsing in `src/parser/extended_property_parser.rs` (G1-G3)
- **15.7**: Token-based SQL preprocessing in `src/parser/preprocess_parser.rs` (H1-H3)
- **15.8**: Whitespace-agnostic keyword matching (J1-J7) - replace space-only patterns with tokenizer
- SQLCMD (I1-I2) intentionally remain regex-based for line-oriented preprocessing

See [PARSER_REFACTORING_GUIDE.md](./PARSER_REFACTORING_GUIDE.md) for implementation details.

### Phase 16: Performance Tuning

**16.1: Benchmark Infrastructure (7/7)**
- Added criterion benchmark infrastructure
- Created benchmarks for full pipeline, SQL parsing, model building, XML generation
- Created stress_test fixture with 135 SQL files

**16.2: Quick Wins (5/5)**
- Cached 30 regex compilations using `LazyLock<Regex>` (2-4% full pipeline improvement)
- Optimized string joining with `Cow<'static, str>` (5-9% SQL parsing improvement)
- Cached uppercase SQL in fallback parsing (~1.5% SQL parsing improvement)
- Added capacity hints to 9 key vector allocations

**16.3: Medium Effort Optimizations (3/3)**
- Reduced cloning with `Cow` for schema tracking (~2-3% SQL parsing improvement)
- Pre-computed sort keys with `sort_by_cached_key()` (~2-4% full pipeline improvement)
- Batched 125 `push_attribute()` calls to `with_attributes()` (4-7% full pipeline improvement)

**16.4: Parallelization (2/2)**
- Added rayon for parallel SQL file parsing (43-67% SQL parsing improvement)
- Adaptive threshold: parallel for ≥8 files, sequential for <8 files

**16.5: Documentation (1/1)**
- Created docs/PERFORMANCE.md with full benchmark methodology and optimization history

### Phase 17: Real-World SQL Compatibility

**17.1: Comma-less Constraint Parsing (3/3)**

SQL Server accepts constraints without comma separators:
```sql
CREATE TABLE [dbo].[Example] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL
    PRIMARY KEY ([Id])  -- No comma before PRIMARY KEY
);
```

Fixed by improving fallback parser's `split_by_comma_or_constraint_tokens()` and fixing `emit_default_constraint_name` logic in `column_parser.rs`.

**17.2: SQLCMD Variable Header Format (2/2)**

Refactored to match .NET DacFx format:
```xml
<CustomData Category="SqlCmdVariables" Type="SqlCmdVariable">
  <Metadata Name="Environment" Value="" />
  <Metadata Name="ServerName" Value="" />
</CustomData>
```

### Remaining Hotspots

| Area | Location | Issue | Impact | Status |
|------|----------|-------|--------|--------|
| Cloning | `src/model/builder.rs` | 149 clone() calls | MEDIUM | ⬜ |

</details>
