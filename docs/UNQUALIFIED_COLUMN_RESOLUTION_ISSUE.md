# Unqualified Column Resolution Issue

## Summary

When building a large real-world SQL project with rust-sqlpackage and deploying with sqlpackage, deployment fails with errors like:

```
The reference to the element that has the name [dbo].[SomeTable].[ColumnName] could not be resolved because no element with that name exists.
```

The root cause is that rust-sqlpackage's body dependency extraction incorrectly resolves unqualified single identifiers (column names without table prefixes) against tables that don't have those columns.

## Examples of Errors Encountered

1. `[dbo].[Payment].[Payment]` - Table name treated as column name
2. `[dbo].[fn_split].[UserId]` - Column from table variable resolved against function
3. `[dbo].[fn_split].[CreatedOn]` - Common column resolved against function
4. `[dbo].[fn_split].[IIF]` - SQL function treated as column name
5. `[dbo].[Configuration].[Name]` - Common column resolved against wrong table

## Root Cause

In `src/dacpac/model_xml/body_deps.rs`, the `extract_body_dependencies` function scans SQL body text for references. When it encounters a single identifier like `CreatedOn` or `Name`:

1. It checks if it's a keyword, alias, or known table name
2. If not, it calls `find_scope_table()` to get the "first table in scope"
3. It creates a column reference like `[first_table].[identifier]`

The problem is that `find_scope_table()` returns whichever table happens to be first in `table_refs`, which may be:
- A table-valued function (like `dbo.fn_split`)
- A table that doesn't have the column being referenced
- The wrong table when multiple tables are in scope

## Fixes Applied (Partially Successful)

### 1. Table Name Self-Aliases
**File:** `body_deps.rs:1948-1957`

When parsing `FROM Payment p`, now both `p` AND `payment` are added as aliases. This prevents the table name from being treated as an unqualified column.

### 2. Missing SQL Functions in Keyword List
**File:** `body_deps.rs:2881-2928`

Added ~40 SQL functions that were being mistaken for column names:
- `IIF`, `NULLIF`, `CHOOSE`
- `ROW_NUMBER`, `RANK`, `DENSE_RANK`, `LAG`, `LEAD`
- `JSON_VALUE`, `JSON_QUERY`, `STRING_SPLIT`
- `TRY_CAST`, `TRY_CONVERT`, `FORMAT`
- And many more

### 3. Function Call Argument Skipping
**File:** `body_deps.rs:1920-1926`

When parsing `dbo.fn_split(@args, ',') [Split]`, now skips the `(...)` to properly capture the alias `[Split]`.

### 4. Table Variable Column Tracking
**File:** `body_deps.rs:889-895`

Extracts column names from `DECLARE @var TABLE ([col] type, ...)` and excludes them from unqualified column resolution.

### 5. Function Call Filtering
**File:** `body_deps.rs:832-930`

Detects function call patterns like `FROM dbo.fn_split(` and excludes them from being used as resolution targets for unqualified columns.

## Latest Error

```
The reference to the element that has the name [dbo].[Configuration].[Name] could not be resolved because no element with that name exists.
```

`Configuration` is a regular table (not a function), so the function call filtering doesn't help here. The unqualified column `Name` is being resolved against whatever table happens to be first in scope.

---

## Industry Research: How Other Tools Handle This

### The Fundamental Problem

We are attempting **semantic analysis without semantic context**. All production systems that correctly resolve unqualified columns do so by:

1. Having complete schema knowledge (which tables have which columns)
2. Building a scope graph that tracks what's visible where
3. Checking if the column name exists in exactly one table in scope

Our current approach of "pick the first table in scope" is a heuristic that will always fail when:
- The first table doesn't have that column
- Multiple tables have the same column name (ambiguous)
- The "table" is actually a TVF or derived table

### Tool Analysis

#### sqlparser-rs (what we use)

- **Intentionally provides NO semantic analysis** - it's a pure syntax parser
- From their docs: "This crate avoids semantic analysis because it varies drastically between dialects"
- All name resolution logic must be built on top of the AST
- Source: https://github.com/apache/datafusion-sqlparser-rs

#### SqlScriptDOM (Microsoft's T-SQL parser)

- Also primarily syntactic - provides AST traversal via `TSqlFragmentVisitor`
- Does NOT include built-in semantic analysis for column-to-table binding
- You extract `ColumnReferenceExpression` nodes but must implement your own resolution
- The visitor pattern lets you find column references, but doesn't tell you which table they belong to
- Source: https://devblogs.microsoft.com/azure-sql/programmatically-parsing-transact-sql-t-sql-with-the-scriptdom-parser/

#### DacFx/SSDT (what we're trying to match)

- Uses an internal model with **full schema knowledge** from all source files
- Key insight: **DacFx builds a complete schema model FIRST, then uses that model to resolve references**
- Has access to `sys.sql_expression_dependencies`-like resolution internally
- The model.xml stores column type info in verbose XML with full relationship tracking
- For reference resolution, only object names matter - body text, triggers, permissions are not needed
- Source: https://github.com/microsoft/DacFx/issues/360

#### SQL Server's sys.sql_expression_dependencies

- Tracks dependencies at runtime with actual schema knowledge
- Column-level dependencies are only tracked for **schema-bound objects**
- Uses **deferred name resolution** - can reference non-existent objects
- Key column `is_caller_dependent` indicates when resolution depends on caller's schema
- When `is_caller_dependent = 1`, `referenced_id` is NULL (can't resolve statically)
- Source: https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-sql-expression-dependencies-transact-sql

#### Apache DataFusion

- Uses `ContextProvider` interface that provides schema information
- `SqlToRel` does "name and type resolution (called 'binding')" using this provider
- **Cannot resolve columns without schema knowledge**
- Every expression has a name used as the column name for references
- Source: https://datafusion.apache.org/library-user-guide/building-logical-plans.html

#### CG/SQL (Facebook's SQL compiler)

Uses a **multi-phase resolver** that checks categories in order:
1. Arguments - parameters in current function/procedure
2. Columns - from the FROM clause (with scope support)
3. Virtual rowid - implicit rowid column
4. Cursor as expression - cursor as boolean
5. Variables - local or global
6. Enums - constant enum values (scope-qualified)
7. Cursor fields - fields within cursor (scope-qualified)
8. Argument bundles - bundled argument sets

For columns specifically, uses "existing name resolution infrastructure to find which table in the join scope contains the column" - **requires knowing which tables have which columns**.

Source: https://cgsql.dev/cql-guide/int02/

#### JetBrains DataGrip/IntelliJ

- **Requires mapping SQL files to actual data sources** for resolution
- Cannot resolve unqualified columns without connection to real database
- Uses hierarchical scope resolution (project → directory → file)
- Source: https://www.jetbrains.com/help/datagrip/settings-languages-sql-resolution-scopes.html

### SQL Standard Behavior for Ambiguous Columns

When multiple tables in scope have the same column name:
- SQL engines **require explicit qualification** - they raise "ambiguous column name" error
- Resolution searches scopes from innermost to outermost
- When multiple matches exist at same scope level without qualification, error is raised
- There is NO "automatic resolution" algorithm - qualification is required

Source: https://docs.oracle.com/cd/B10501_01/appdev.920/a96624/d_names.htm

---

## Refined Solution Approaches

### Approach A: Two-Pass Schema-Aware Resolution - SELECTED APPROACH

**Status: SELECTED** - This is the only viable approach for DotNet parity.

Build a column registry during model construction, then use it during body dependency extraction.

```rust
// Phase 1: During model building, collect column information
struct ColumnRegistry {
    // Map: [schema].[table] -> Set<column_name>
    table_columns: HashMap<String, HashSet<String>>,
}

impl ColumnRegistry {
    fn table_has_column(&self, table: &str, column: &str) -> bool { ... }
    fn find_tables_with_column(&self, column: &str, in_scope: &[String]) -> Vec<String> { ... }
}

// Phase 2: When resolving unqualified column
fn resolve_unqualified_column(
    column: &str,
    tables_in_scope: &[String],
    registry: &ColumnRegistry,
) -> Option<String> {
    let candidates: Vec<_> = tables_in_scope
        .iter()
        .filter(|t| registry.table_has_column(t, column))
        .collect();

    match candidates.len() {
        1 => Some(candidates[0].clone()),  // Unique match
        0 => None,                          // No match - don't emit dependency
        _ => None,                          // Ambiguous - don't emit dependency
    }
}
```

**Pros:**
- Correct resolution matching how real SQL engines work
- Eliminates false positives AND false negatives
- Only approach that achieves DotNet parity

**Cons:**
- Requires refactoring to pass column info through pipeline
- May need to process files in dependency order
- Most complex to implement

**Implementation Planning - Codebase Research Needed:**

Before implementing, explore these questions in the codebase:

1. **Where is column information captured?**
   - How does `src/model/` extract columns from CREATE TABLE statements?
   - Is column info already stored in `ModelElement::Table`?

2. **What's the data flow?**
   - Pipeline: `.sqlproj` → `SqlProject` → SQL Files → AST → `DatabaseModel` → XML → `.dacpac`
   - At what point is the full model available?
   - When is `extract_body_dependencies()` called relative to model building?

3. **Where to inject the registry?**
   - Can we build the registry after `build_model()` completes?
   - How to pass it to dacpac generation / body dependency extraction?

4. **Edge cases to handle:**
   - Views (do they have "columns" we need to track?)
   - Table-valued functions
   - CTEs (temporary scope, not in registry)
   - Table variables (temporary scope, not in registry)

**Implementation steps:**
1. Add `column_names: HashSet<String>` to `Table` model element (if not already present)
2. Build `ColumnRegistry` after model construction completes
3. Pass registry to `extract_body_dependencies`
4. Resolve unqualified columns only when exactly one match in scope

### ~~Approach B: Conservative Non-Resolution~~ (UNWORKABLE - Breaks Parity)

**Status: REJECTED** - This approach breaks DotNet parity which is a project requirement.

The idea was to skip unqualified column resolution entirely:

```rust
BodyDepToken::SingleUnbracketed(ident) | BodyDepToken::SingleBracketed(ident) => {
    // Skip unqualified columns entirely - only qualified refs create dependencies
    continue;
}
```

**Why this doesn't work:**

DotNet DOES emit unqualified column dependencies. Our investigation confirmed that DotNet produces references like `[dbo].[Account].[Id]` from `A.Id` where `A` is an alias for `Account`. Skipping these would cause parity test failures and potentially deployment issues where DotNet-built dacpacs work but rust-sqlpackage-built dacpacs fail due to missing dependencies.

**Tested result:** Disabling unqualified column resolution causes 126+ parity test failures.

### ~~Approach C: Exclusion-Based Heuristics~~ (UNWORKABLE - DO NOT USE)

**Status: REJECTED** - This approach is fundamentally flawed and should not be pursued.

The idea was to exclude SQL keywords and common column names from resolution:

```rust
// DON'T DO THIS - fundamentally broken approach
const EXCLUDED_KEYWORDS: &[&str] = &["MERGE", "MATCHED", "USING", ...];

fn should_skip_unqualified_column(name: &str) -> bool {
    EXCLUDED_KEYWORDS.contains(&name.to_uppercase().as_str())
}
```

**Why this doesn't work:**

SQL Server allows ANY keyword to be used as an identifier when bracketed:

```sql
CREATE TABLE [dbo].[Config] (
    [Id] INT,
    [MERGE] NVARCHAR(50),    -- Valid column named MERGE!
    [SELECT] NVARCHAR(100),  -- Valid column named SELECT!
    [Status] NVARCHAR(20)
)
```

This means keyword exclusion causes **two types of errors**:

1. **False positives** (current problem): Keywords we forgot to exclude are treated as columns
   - Example: `MERGE` keyword → `[dbo].[AccountTag].[MERGE]` (invalid reference)

2. **False negatives** (introduced by exclusion): Keywords that ARE actually columns get skipped
   - Example: `[dbo].[Config].[MERGE]` is a real column but would be excluded

**Without schema knowledge, we cannot distinguish between:**
- `MERGE` as a SQL keyword in a MERGE statement
- `MERGE` as a column name in a table

**Conclusion:** Any heuristic-based approach will be wrong in some cases. Only schema-aware resolution (Approach A) can correctly handle this.

### Approach D: DotNet Behavior Empirical Analysis (Investigation) - COMPLETE

**Status: COMPLETE** - See "Investigation Results" section below.

**Summary of findings:**
1. DotNet DOES emit unqualified column dependencies (e.g., `A.Id` → `[dbo].[Account].[Id]`)
2. DotNet uses **schema-aware resolution** - it knows which tables have which columns
3. DotNet correctly handles SQL keywords in MERGE statements without false positives
4. Keyword exclusion alone cannot replicate DotNet's behavior (see Approach C: UNWORKABLE)

**Conclusion:** Approach A (schema-aware resolution) is the only viable path to DotNet parity.

---

## Investigation Plan

### Phase 1: Empirical DotNet Analysis

1. Build a real-world project with both rust-sqlpackage and DotNet
2. Extract and diff the model.xml files
3. Focus on procedures that cause deployment failures
4. Document exact dependency differences

### Phase 2: Parity Test Analysis

1. Review the 126 failing tests when unqualified resolution is disabled
2. Categorize: Are they testing unqualified column deps specifically?
3. Check if test expectations match actual DotNet output
4. Identify false expectations vs real parity gaps

### Phase 3: Solution Selection

Based on findings:
- If DotNet doesn't resolve unqualified columns → Implement Approach B
- If DotNet uses schema knowledge → Implement Approach A
- If DotNet uses specific heuristics → Match those heuristics

### Phase 4: Implementation

Implement chosen solution with:
- Feature flag for gradual rollout
- Updated parity tests
- Documentation of behavior differences

---

## Investigation Results (2026-02-04)

### Test Fixture Analysis: `body_dependencies_aliases`

We compared DotNet and Rust output for the `body_dependencies_aliases` fixture which has `relationship_pass: false` in parity tests.

#### Key Findings

**1. Most unqualified column resolution is working correctly**

Both DotNet and Rust correctly resolve:
- `Account A` → `[dbo].[Account]`
- `A.Id` → `[dbo].[Account].[Id]`
- `T.Name` → `[dbo].[Tag].[Name]`
- Nested subquery aliases like `[ITTAG].[Name]` → `[dbo].[Tag].[Name]`

The parity differences are mostly about **ordering** and **duplicate handling**, not incorrect references.

**2. Concrete false positives identified**

Rust incorrectly emits these references that DotNet does NOT produce:

```xml
<References Name="[dbo].[AccountTag].[MERGE]" />
<References Name="[dbo].[AccountTag].[USING]" />
<References Name="[dbo].[AccountTag].[MATCHED]" />
```

These are SQL keywords from MERGE statements being treated as column names.

**Root cause:** The keywords `MERGE`, `MATCHED`, and `USING` are missing from the `is_sql_keyword_not_column()` function at line ~2888 in `body_deps.rs`.

**3. Current parity test status**

```
Layer 1 errors (inventory): 0
Layer 2 errors (properties): 0
Relationship errors: 65
```

The 65 relationship errors are primarily:
- Reference count mismatches (different duplicate handling)
- Reference ordering differences
- The MERGE keyword false positives (3 per procedure using MERGE)

### ~~Immediate Fix~~ (NOT RECOMMENDED)

~~Add these keywords to `is_sql_keyword_not_column()`:~~

```rust
// DON'T DO THIS - see Approach C analysis above
// Adding keywords creates false negatives when keywords are used as column names
| "MERGE"
| "MATCHED"
| "USING"
```

**Why this won't work:** SQL Server allows keywords as column names when bracketed (e.g., `[MERGE]`). Adding keywords to the exclusion list would cause us to miss legitimate column references. See "Approach C: UNWORKABLE" section above.

### Actual Fix Required

The only reliable solution is **Approach A: Schema-Aware Resolution** or **Approach B: Conservative Non-Resolution**.

- **Approach A** requires building a column registry during model construction
- **Approach B** would skip unqualified column resolution entirely, relying on qualified refs

### Investigation Questions Answered

**Q: Does DotNet emit unqualified column dependencies?**
A: Yes, DotNet does emit column dependencies for alias-qualified columns like `A.Id` where `A` is a table alias.

**Q: How does DotNet resolve them correctly?**
A: DotNet appears to have schema knowledge - it knows that `Account A` (unqualified) resolves to `[dbo].[Account]` and then correctly maps `A.Id` to `[dbo].[Account].[Id]`. The exact mechanism is internal to DacFx.

**Q: Are there patterns in what DotNet includes vs excludes?**
A: DotNet correctly handles MERGE statement syntax without creating false column references. However, DotNet likely uses **schema-aware resolution** internally - it has access to the full model and knows which tables have which columns. This is why it can correctly distinguish between `MERGE` as a keyword vs `[MERGE]` as a column name.

**Q: What about real-world deployment failures?**
A: The specific errors like `[dbo].[Configuration].[Name]` are likely from:
1. Tables/TVFs being selected as resolution targets incorrectly (first table in scope, not the correct table)
2. Unqualified columns being resolved against tables that don't have them
3. Edge cases not covered by the test fixtures

### Recommended Next Steps

**Research phase (Option D) is COMPLETE.**

**Next: Implementation Planning for Approach A**

1. **Codebase exploration** - Understand data flow and where to inject column registry
   - Examine `src/model/` for column extraction
   - Trace pipeline from model building to dacpac generation
   - Identify injection point for registry

2. **Design the ColumnRegistry** - Define the data structure and API
   - Consider case sensitivity (SQL Server is case-insensitive by default)
   - Handle schema qualification (`[dbo].[Table]` vs `Table`)
   - Consider views and TVFs in addition to tables

3. **Implement incrementally**
   - Start with tables only
   - Add views if needed for parity
   - Test against `body_dependencies_aliases` fixture
   - Test against [WideWorldImporters](https://github.com/microsoft/sql-server-samples/tree/master/samples/databases/wide-world-importers/wwi-ssdt) sample database

**DO NOT:**
- Add more keywords to exclusion list (Approach C - fundamentally broken)
- Skip unqualified column resolution (Approach B - breaks parity)

---

## Files Modified

- `src/dacpac/model_xml/body_deps.rs` - Main body dependency extraction logic

## Related Code Locations

- `extract_body_dependencies()` - Main entry point (line ~940)
- `find_scope_table()` - Returns first table for resolution (line ~1390)
- `extract_table_refs_tokenized()` - Extracts table references (line ~728)
- `extract_table_aliases_for_body_deps()` - Extracts table aliases (line ~1206)
- `is_sql_keyword_not_column()` - Keyword filtering (line ~2723)
- `extract_function_call_refs()` - Function call detection (line ~842)
- `resolve_alias_for_position()` - Position-aware alias resolution (line ~1413)

## References

- [Apache DataFusion sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs) - Parser-only design
- [SqlScriptDOM Blog](https://devblogs.microsoft.com/azure-sql/programmatically-parsing-transact-sql-t-sql-with-the-scriptdom-parser/) - TSqlFragmentVisitor usage
- [DataFusion Logical Plans](https://datafusion.apache.org/library-user-guide/building-logical-plans.html) - Schema-aware resolution
- [CG/SQL Semantic Analysis](https://cgsql.dev/cql-guide/int02/) - Multi-phase resolver pattern
- [sys.sql_expression_dependencies](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-sql-expression-dependencies-transact-sql) - SQL Server dependency tracking
- [DacFx Performance Issue #360](https://github.com/microsoft/DacFx/issues/360) - Model.xml internals discussion
- [DacFx RelationshipType](https://learn.microsoft.com/en-us/dotnet/api/microsoft.sqlserver.dac.model.relationshiptype?view=sql-dacfx-161) - DacFx model types
- [Oracle PL/SQL Name Resolution](https://docs.oracle.com/cd/B10501_01/appdev.920/a96624/d_names.htm) - SQL standard resolution algorithm
- [JetBrains SQL Resolution Scopes](https://www.jetbrains.com/help/datagrip/settings-languages-sql-resolution-scopes.html) - IDE approach
