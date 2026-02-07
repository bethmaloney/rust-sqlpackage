# Implementation Plan

---

## Status: PARITY COMPLETE | OLTP FEATURE SUPPORT IN PROGRESS

**Phases 1-55 complete. Full parity: 47/48 (97.9%).**

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

## Upcoming Work: OLTP Feature Support

Priority features for standard OLTP application databases, ordered by impact.

### Phase 56: Synonyms

Simple `CREATE SYNONYM` support. High real-world usage for cross-database references and abstraction layers.

**DacFx Element Type:** `SqlSynonym`

#### Phase 56.1: Synonym Parser

Parse `CREATE SYNONYM [schema].[name] FOR [target_schema].[target_name]` (and 3/4-part target names for cross-database/server references).

**Tasks:**
- [ ] Create `src/parser/synonym_parser.rs` with token-based parser
- [ ] Add `FallbackStatementType::Synonym { schema, name, target }` variant to `tsql_parser.rs`
- [ ] Detect `CREATE SYNONYM` in fallback dispatch and route to parser
- [ ] Unit tests for 1-part, 2-part, 3-part, and 4-part target names

**Files:**
- `src/parser/synonym_parser.rs` (new)
- `src/parser/tsql_parser.rs`
- `src/parser/mod.rs`

#### Phase 56.2: Synonym Model Element

Add `SynonymElement` struct and `ModelElement::Synonym` variant.

**Tasks:**
- [ ] Add `SynonymElement` to `src/model/elements.rs` with fields: `schema`, `name`, `target_schema`, `target_name`, `target_database`, `target_server`
- [ ] Implement `type_name()` → `"SqlSynonym"`, `full_name()` → `[schema].[name]`
- [ ] Add `ModelElement::Synonym(SynonymElement)` variant
- [ ] Add match arm in `builder.rs` to construct `SynonymElement` from `FallbackStatementType::Synonym`
- [ ] Track schema via `track_schema()`

**Files:**
- `src/model/elements.rs`
- `src/model/builder.rs`

#### Phase 56.3: Synonym XML Writer

Write `SqlSynonym` elements to model.xml.

**Tasks:**
- [ ] Add `write_synonym()` function in `src/dacpac/model_xml/other_writers.rs`
- [ ] Write `<Element Type="SqlSynonym" Name="[schema].[name]">`
- [ ] Write `ForObject` relationship pointing to the target object
- [ ] Handle cross-database/server target references (ExternalSource)
- [ ] Wire into element dispatch in `model_xml/mod.rs`

**Files:**
- `src/dacpac/model_xml/other_writers.rs`
- `src/dacpac/model_xml/mod.rs`

#### Phase 56.4: Synonym Tests

**Tasks:**
- [ ] Create `tests/fixtures/synonyms/` with `project.sqlproj` and SQL files
- [ ] Cover: basic synonym, cross-schema target, cross-database target, synonym for procedure/function/view
- [ ] Integration test: fixture builds successfully
- [ ] Unit tests: verify element count, names, and target references in model

**Files:**
- `tests/fixtures/synonyms/` (new)
- `tests/integration/dacpac_compatibility_tests.rs`

---

### Phase 57: Temporal Tables (System-Versioned)

Support for `SYSTEM_VERSIONING`, `PERIOD FOR SYSTEM_TIME`, and history table references. The biggest functional gap for modern OLTP apps (audit trails, slowly-changing data).

**DacFx Properties:** Properties on existing `SqlTable` element — no new element type needed.

#### Phase 57.1: Temporal Table Parser — PERIOD FOR SYSTEM_TIME

Parse the `PERIOD FOR SYSTEM_TIME` column-level clause in CREATE TABLE.

**Tasks:**
- [ ] Detect `PERIOD FOR SYSTEM_TIME (start_col, end_col)` in column/constraint parsing
- [ ] Extract the two datetime2 column names (period start, period end)
- [ ] Store in a new `SystemTimePeriod { start_column, end_column }` struct
- [ ] Unit tests for PERIOD syntax variations

**Files:**
- `src/parser/column_parser.rs` or `src/parser/constraint_parser.rs`
- `src/model/elements.rs` (add `SystemTimePeriod` struct)

#### Phase 57.2: Temporal Table Parser — SYSTEM_VERSIONING Option

Parse `WITH (SYSTEM_VERSIONING = ON (...))` table option.

**Tasks:**
- [ ] Detect `SYSTEM_VERSIONING = ON` in table options parsing
- [ ] Extract optional `HISTORY_TABLE = [schema].[name]`
- [ ] Extract optional `DATA_CONSISTENCY_CHECK = ON|OFF`
- [ ] Extract optional `HISTORY_RETENTION_PERIOD = N {DAYS|WEEKS|MONTHS|YEARS}`
- [ ] Unit tests for each option combination

**Files:**
- `src/parser/tsql_parser.rs` (table option parsing)
- `src/model/elements.rs` (add temporal fields to `TableElement`)

#### Phase 57.3: Temporal Table Model Changes

Wire temporal properties into the `TableElement` and model builder.

**Tasks:**
- [ ] Add fields to `TableElement`: `system_time_period: Option<SystemTimePeriod>`, `is_system_versioned: bool`, `history_table_schema: Option<String>`, `history_table_name: Option<String>`
- [ ] Update `builder.rs` to populate temporal fields from parsed data
- [ ] Mark period start/end columns with `GeneratedAlwaysType` (AS ROW START / AS ROW END)
- [ ] Handle `HIDDEN` column attribute for period columns

**Files:**
- `src/model/elements.rs`
- `src/model/builder.rs`

#### Phase 57.4: Temporal Table XML Writer

Generate DacFx-compatible XML properties and relationships for temporal tables.

**Tasks:**
- [ ] Write `IsSystemVersioningOn` property (`"True"`) on temporal tables
- [ ] Write `SystemTimePeriodStartColumn` and `SystemTimePeriodEndColumn` relationships
- [ ] Write `HistoryTable` relationship pointing to `[schema].[history_table]`
- [ ] Write `IsGeneratedAlwaysStart`/`IsGeneratedAlwaysEnd` on period columns
- [ ] Write `IsHidden` property on hidden period columns

**Files:**
- `src/dacpac/model_xml/table_writer.rs`

#### Phase 57.5: Temporal Table Tests

**Tasks:**
- [ ] Create `tests/fixtures/temporal_tables/` with `project.sqlproj` and SQL files
- [ ] Cover: basic temporal table, custom history table name, default history table, hidden period columns, retention period
- [ ] Integration test: fixture builds successfully
- [ ] Unit tests: verify temporal properties and relationships in model.xml

**Files:**
- `tests/fixtures/temporal_tables/` (new)
- `tests/integration/dacpac_compatibility_tests.rs`

---

### Phase 58: Security Objects

Support for users, roles, and permissions. Currently silently skipped (`SkippedSecurityStatement`). Present in virtually every production database.

**DacFx Element Types:** `SqlUser`, `SqlRole`, `SqlPermissionStatement`, `SqlRoleMembership`

#### Phase 58.1: Security Parser — CREATE USER

Parse `CREATE USER` statements with various authentication options.

**Tasks:**
- [ ] Create `src/parser/security_parser.rs`
- [ ] Parse `CREATE USER [name] FOR LOGIN [login]`
- [ ] Parse `CREATE USER [name] WITHOUT LOGIN`
- [ ] Parse `CREATE USER [name] WITH DEFAULT_SCHEMA = [schema]`
- [ ] Parse `CREATE USER [name] FROM EXTERNAL PROVIDER`
- [ ] Add `FallbackStatementType::CreateUser { ... }` variant
- [ ] Unit tests for each CREATE USER variation

**Files:**
- `src/parser/security_parser.rs` (new)
- `src/parser/tsql_parser.rs`
- `src/parser/mod.rs`

#### Phase 58.2: Security Parser — CREATE ROLE and Role Membership

Parse role creation and `ALTER ROLE ... ADD MEMBER` statements.

**Tasks:**
- [ ] Parse `CREATE ROLE [name]` with optional `AUTHORIZATION [owner]`
- [ ] Parse `ALTER ROLE [role] ADD MEMBER [member]`
- [ ] Parse `ALTER ROLE [role] DROP MEMBER [member]`
- [ ] Parse legacy `sp_addrolemember` calls
- [ ] Add `FallbackStatementType::CreateRole { ... }` and `AlterRoleMembership { ... }` variants
- [ ] Unit tests for role and membership variations

**Files:**
- `src/parser/security_parser.rs`
- `src/parser/tsql_parser.rs`

#### Phase 58.3: Security Parser — GRANT, DENY, REVOKE

Parse permission statements.

**Tasks:**
- [ ] Parse `GRANT <permission> ON [object] TO [principal]`
- [ ] Parse `DENY <permission> ON [object] TO [principal]`
- [ ] Parse `REVOKE <permission> ON [object] FROM [principal]`
- [ ] Handle `WITH GRANT OPTION` and `CASCADE`
- [ ] Handle schema-level permissions (`ON SCHEMA::[schema]`)
- [ ] Handle database-level permissions (no ON clause)
- [ ] Add `FallbackStatementType::Permission { action, permission, object, principal }` variant
- [ ] Unit tests for GRANT/DENY/REVOKE on tables, schemas, procedures

**Files:**
- `src/parser/security_parser.rs`
- `src/parser/tsql_parser.rs`

#### Phase 58.4: Security Model Elements

Add element types for users, roles, permissions, and role memberships.

**Tasks:**
- [ ] Add `UserElement` to `elements.rs`: `name`, `login`, `default_schema`, `auth_type`
- [ ] Add `RoleElement`: `name`, `owner`
- [ ] Add `PermissionElement`: `action` (Grant/Deny/Revoke), `permission`, `object_schema`, `object_name`, `principal`
- [ ] Add `RoleMembershipElement`: `role`, `member`
- [ ] Add corresponding `ModelElement` variants
- [ ] Implement `type_name()` for each: `"SqlUser"`, `"SqlRole"`, `"SqlPermissionStatement"`, `"SqlRoleMembership"`
- [ ] Update `builder.rs` match arms to construct elements (replace `SkippedSecurityStatement`)

**Files:**
- `src/model/elements.rs`
- `src/model/builder.rs`

#### Phase 58.5: Security XML Writers

Write security elements to model.xml.

**Tasks:**
- [ ] Add `write_user()` — properties: `AuthenticationType`, `DefaultSchema` relationship
- [ ] Add `write_role()` — properties: `Authorization` relationship
- [ ] Add `write_permission()` — properties: `Permission` value, `Action` value, `SecuredObject` and `Grantor` relationships
- [ ] Add `write_role_membership()` — `Role` and `Member` relationships
- [ ] Wire all into element dispatch in `model_xml/mod.rs`

**Files:**
- `src/dacpac/model_xml/other_writers.rs`
- `src/dacpac/model_xml/mod.rs`

#### Phase 58.6: Security Tests

**Tasks:**
- [ ] Create `tests/fixtures/security_objects/` with SQL files
- [ ] Cover: CREATE USER (login-based, without login, external), CREATE ROLE, ALTER ROLE ADD MEMBER, GRANT/DENY/REVOKE on table/schema/database
- [ ] Integration test: fixture builds successfully (security elements present in dacpac)
- [ ] Unit tests: verify element types, counts, properties, and relationships
- [ ] Verify existing fixtures still pass (security statements no longer skipped — ensure backward compat)

**Files:**
- `tests/fixtures/security_objects/` (new)
- `tests/integration/dacpac_compatibility_tests.rs`

---

### Phase 59: Database Scoped Configurations

Support for `ALTER DATABASE SCOPED CONFIGURATION` statements. Common in modern SQL Server for MAXDOP, parameter sniffing, query optimizer settings.

**DacFx Element Type:** `SqlDatabaseScopedConfigurationOptions` (or property on database model)

#### Phase 59.1: Database Scoped Configuration Parser

Parse `ALTER DATABASE SCOPED CONFIGURATION SET` statements.

**Tasks:**
- [ ] Create `src/parser/db_scoped_config_parser.rs`
- [ ] Parse `ALTER DATABASE SCOPED CONFIGURATION SET <option> = <value>`
- [ ] Parse `ALTER DATABASE SCOPED CONFIGURATION FOR SECONDARY SET <option> = <value>`
- [ ] Handle common options: `MAXDOP`, `LEGACY_CARDINALITY_ESTIMATION`, `PARAMETER_SNIFFING`, `QUERY_OPTIMIZER_HOTFIXES`, `IDENTITY_CACHE`, `BATCH_MODE_ADAPTIVE_JOINS`, `BATCH_MODE_MEMORY_GRANT_FEEDBACK`
- [ ] Add `FallbackStatementType::DatabaseScopedConfiguration { option, value, is_secondary }` variant
- [ ] Unit tests for each option type

**Files:**
- `src/parser/db_scoped_config_parser.rs` (new)
- `src/parser/tsql_parser.rs`
- `src/parser/mod.rs`

#### Phase 59.2: Database Scoped Configuration Model Element

**Tasks:**
- [ ] Add `DatabaseScopedConfigurationElement` to `elements.rs`: `option_name`, `value`, `is_secondary`
- [ ] Add `ModelElement::DatabaseScopedConfiguration` variant
- [ ] Implement `type_name()` → `"SqlDatabaseScopedConfigurationOptions"`
- [ ] Update `builder.rs` match arm

**Files:**
- `src/model/elements.rs`
- `src/model/builder.rs`

#### Phase 59.3: Database Scoped Configuration XML Writer

**Tasks:**
- [ ] Add `write_database_scoped_configuration()` to `other_writers.rs`
- [ ] Write element with configuration option name and value properties
- [ ] Handle primary vs secondary configuration distinction
- [ ] Wire into element dispatch in `model_xml/mod.rs`

**Files:**
- `src/dacpac/model_xml/other_writers.rs`
- `src/dacpac/model_xml/mod.rs`

#### Phase 59.4: Database Scoped Configuration Tests

**Tasks:**
- [ ] Create `tests/fixtures/db_scoped_config/` with SQL files
- [ ] Cover: MAXDOP, LEGACY_CARDINALITY_ESTIMATION, PARAMETER_SNIFFING, FOR SECONDARY variants
- [ ] Integration test: fixture builds successfully
- [ ] Unit tests: verify configuration elements in model.xml

**Files:**
- `tests/fixtures/db_scoped_config/` (new)
- `tests/integration/dacpac_compatibility_tests.rs`

---

## Completed Phases (1-55)

| Phase | Description | Status |
|-------|-------------|--------|
| 1-9 | Core implementation (properties, relationships, XML structure, metadata) | 58/58 |
| 10 | Extended properties, function classification, constraint naming | 5/5 |
| 11 | Remaining parity failures, error fixtures, ignored tests | 70/70 |
| 12-13 | SELECT * expansion, TVF columns, TVP support | 10/10 |
| 14 | Layer 3 (SqlPackage) parity | 3/3 |
| 15, 20 | Parser refactoring: replace regex with token-based parsing | 77/77 |
| 16 | Performance: 116x faster than DotNet cold, 42x faster warm | 18/18 |
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

### Key Milestones

- **Parity Achievement (Phase 14):** L1-L3 100%, Relationships 97.9%
- **Performance (Phase 16):** 116x/42x faster than DotNet cold/warm
- **Parser Modernization (Phases 15, 20):** All regex replaced with token-based parsing
- **XML Parity (Phases 22-54):** Layer 7 improved from 0% to 50.0%
