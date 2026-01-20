# Rust sqlproj to dacpac Compiler - Implementation Checklist

**Target**: SQL Server 2022 (Sql160) | Standard DDL/DML | System database references only

---

## Phase 1: Project Setup and Core Infrastructure
- [x] Create Cargo.toml with dependencies
- [x] Create src/main.rs CLI entry point
- [x] Create src/lib.rs library exports
- [x] Create src/error.rs error types
- [x] Verify project compiles

## Phase 2: Sqlproj Parser
- [x] Create src/project/mod.rs
- [x] Create src/project/sqlproj_parser.rs
- [x] Parse target platform from sqlproj
- [x] Extract SQL file paths (Build items)
- [x] Extract database references
- [x] Unit tests for parser

## Phase 3: T-SQL Parser Integration
- [x] Create src/parser/mod.rs
- [x] Create src/parser/tsql_parser.rs
- [x] Handle GO batch separator preprocessing
- [x] Parse CREATE TABLE statements
- [x] Parse CREATE VIEW statements
- [x] Parse CREATE PROCEDURE statements (fallback parsing for T-SQL syntax)
- [x] Parse CREATE FUNCTION statements (fallback parsing for T-SQL syntax)
- [x] Unit tests for parser

## Phase 4: Database Model Builder
- [x] Create src/model/mod.rs
- [x] Create src/model/database_model.rs
- [x] Create src/model/elements.rs
- [x] Create src/model/builder.rs
- [x] Build TableElement from AST
- [x] Build ViewElement from AST
- [x] Build ProcedureElement from AST
- [x] Build FunctionElement from AST
- [x] Handle constraints and indexes
- [x] Unit tests for model builder

## Phase 5: Model XML Generator
- [x] Create src/dacpac/mod.rs
- [x] Create src/dacpac/model_xml.rs
- [x] Create src/dacpac/metadata_xml.rs
- [x] Create src/dacpac/origin_xml.rs
- [x] Generate DataSchemaModel root element
- [x] Generate Element nodes for each object
- [x] Generate Property and Relationship nodes
- [x] Match DacFx XML schema exactly (Relationship Name as attribute, ExternalSource for built-ins)
- [x] Unit tests for XML generation (37 tests covering elements, relationships, types, properties)

## Phase 6: Dacpac Package Generator
- [x] Create src/dacpac/packager.rs
- [x] Create ZIP file structure
- [x] Write model.xml to ZIP
- [x] Write DacMetadata.xml to ZIP
- [x] Write Origin.xml to ZIP
- [ ] Integration test with real sqlproj

## Phase 7: CLI Interface
- [x] Create CLI in src/main.rs (using clap derive)
- [x] Add build command
- [x] Add verbose flag
- [x] Add output path option
- [ ] Error reporting with line numbers
- [ ] End-to-end test

## Verification
- [ ] Compare output dacpac with .NET-generated dacpac
- [ ] Deploy Rust-generated dacpac to SQL Server
- [ ] Performance benchmark vs .NET

---

## Phase 8: Index INCLUDE Clause Support
- [ ] Update fallback index parsing regex to capture INCLUDE columns
- [ ] Add `include_columns: Vec<String>` to index model element
- [ ] Generate INCLUDE columns in model.xml output
- [ ] Add test fixture with INCLUDE clause indexes
- [ ] Unit tests for INCLUDE extraction

## Phase 9: Procedure/Function Parameter Extraction
- [ ] Parse parameter definitions from CREATE PROCEDURE
- [ ] Parse parameter definitions from CREATE FUNCTION
- [ ] Extract parameter name, data type, direction (IN/OUT/INOUT)
- [ ] Extract default values
- [ ] Add parameters to model elements
- [ ] Generate parameter XML in model.xml
- [ ] Unit tests for parameter extraction

## Phase 10: Trigger Support
- [ ] Add TriggerElement to model
- [ ] Parse CREATE TRIGGER statements (fallback regex)
- [ ] Extract trigger timing (AFTER/INSTEAD OF)
- [ ] Extract trigger events (INSERT/UPDATE/DELETE)
- [ ] Extract target table reference
- [ ] Generate trigger XML in model.xml
- [ ] Add test fixture with triggers
- [ ] Unit tests for trigger parsing

## Phase 11: Synonym Support
- [ ] Add SynonymElement to model
- [ ] Parse CREATE SYNONYM statements
- [ ] Extract target object reference
- [ ] Generate synonym XML in model.xml
- [ ] Unit tests for synonym parsing

## Phase 12: Database Properties
- [ ] Parse DefaultFilegroup from sqlproj
- [ ] Parse PermissionSet from sqlproj
- [ ] Parse PageVerify from sqlproj
- [ ] Parse suppressed warnings from sqlproj
- [ ] Include database properties in model.xml header
- [ ] Unit tests for property parsing

## Phase 13: Extended Properties
- [ ] Parse sp_addextendedproperty calls
- [ ] Extract MS_Description and other properties
- [ ] Associate properties with target objects
- [ ] Generate extended property XML
- [ ] Unit tests for extended properties

## Future Considerations
- [ ] Full-text catalogs and indexes
- [ ] Assembly/CLR objects
- [ ] Service Broker objects
- [ ] Database users/roles/permissions
