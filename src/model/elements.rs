//! Database model element types

/// A database model element
#[derive(Debug, Clone)]
pub enum ModelElement {
    Schema(SchemaElement),
    Table(TableElement),
    View(ViewElement),
    Procedure(ProcedureElement),
    Function(FunctionElement),
    Index(IndexElement),
    FullTextIndex(FullTextIndexElement),
    FullTextCatalog(FullTextCatalogElement),
    Constraint(ConstraintElement),
    Sequence(SequenceElement),
    UserDefinedType(UserDefinedTypeElement),
    /// User-defined scalar data type (alias type) - CREATE TYPE x FROM basetype
    ScalarType(ScalarTypeElement),
    ExtendedProperty(ExtendedPropertyElement),
    Trigger(TriggerElement),
    /// Filegroup element (ALTER DATABASE ... ADD FILEGROUP)
    Filegroup(FilegroupElement),
    /// Partition function element (CREATE PARTITION FUNCTION)
    PartitionFunction(PartitionFunctionElement),
    /// Partition scheme element (CREATE PARTITION SCHEME)
    PartitionScheme(PartitionSchemeElement),
    /// Synonym element (CREATE SYNONYM ... FOR ...)
    Synonym(SynonymElement),
    /// Generic raw element for statements that couldn't be fully parsed
    Raw(RawElement),
}

impl ModelElement {
    /// Get the element type name for XML
    pub fn type_name(&self) -> &'static str {
        match self {
            ModelElement::Schema(_) => "SqlSchema",
            ModelElement::Table(_) => "SqlTable",
            ModelElement::View(_) => "SqlView",
            ModelElement::Procedure(_) => "SqlProcedure",
            ModelElement::Function(f) => match f.function_type {
                FunctionType::Scalar => "SqlScalarFunction",
                FunctionType::TableValued => "SqlMultiStatementTableValuedFunction",
                FunctionType::InlineTableValued => "SqlInlineTableValuedFunction",
            },
            ModelElement::Index(_) => "SqlIndex",
            ModelElement::FullTextIndex(_) => "SqlFullTextIndex",
            ModelElement::FullTextCatalog(_) => "SqlFullTextCatalog",
            ModelElement::Constraint(c) => match c.constraint_type {
                ConstraintType::PrimaryKey => "SqlPrimaryKeyConstraint",
                ConstraintType::ForeignKey => "SqlForeignKeyConstraint",
                ConstraintType::Unique => "SqlUniqueConstraint",
                ConstraintType::Check => "SqlCheckConstraint",
                ConstraintType::Default => "SqlDefaultConstraint",
            },
            ModelElement::Sequence(_) => "SqlSequence",
            ModelElement::UserDefinedType(_) => "SqlTableType",
            ModelElement::ScalarType(_) => "SqlUserDefinedDataType",
            ModelElement::ExtendedProperty(_) => "SqlExtendedProperty",
            ModelElement::Trigger(_) => "SqlDmlTrigger",
            ModelElement::Filegroup(_) => "SqlFilegroup",
            ModelElement::PartitionFunction(_) => "SqlPartitionFunction",
            ModelElement::PartitionScheme(_) => "SqlPartitionScheme",
            ModelElement::Synonym(_) => "SqlSynonym",
            ModelElement::Raw(r) => match r.sql_type.as_str() {
                "SqlTable" => "SqlTable",
                "SqlView" => "SqlView",
                "SqlDmlTrigger" => "SqlDmlTrigger",
                "SqlAlterTableStatement" => "SqlAlterTableStatement",
                _ => "SqlUnknown",
            },
        }
    }

    /// Get the full name (e.g., [dbo].[Users])
    pub fn full_name(&self) -> String {
        match self {
            ModelElement::Schema(s) => format!("[{}]", s.name),
            ModelElement::Table(t) => format!("[{}].[{}]", t.schema, t.name),
            ModelElement::View(v) => format!("[{}].[{}]", v.schema, v.name),
            ModelElement::Procedure(p) => format!("[{}].[{}]", p.schema, p.name),
            ModelElement::Function(f) => format!("[{}].[{}]", f.schema, f.name),
            ModelElement::Index(i) => {
                format!("[{}].[{}].[{}]", i.table_schema, i.table_name, i.name)
            }
            ModelElement::FullTextIndex(f) => {
                // Full-text index name format: [schema].[table] (same as table name)
                format!("[{}].[{}]", f.table_schema, f.table_name)
            }
            ModelElement::FullTextCatalog(c) => {
                format!("[{}]", c.name)
            }
            ModelElement::Constraint(c) => {
                // DotNet uses two-part names for constraints: [schema].[constraint_name]
                format!("[{}].[{}]", c.table_schema, c.name)
            }
            ModelElement::Sequence(s) => format!("[{}].[{}]", s.schema, s.name),
            ModelElement::UserDefinedType(u) => format!("[{}].[{}]", u.schema, u.name),
            ModelElement::ScalarType(s) => format!("[{}].[{}]", s.schema, s.name),
            ModelElement::ExtendedProperty(e) => e.full_name(),
            ModelElement::Trigger(t) => format!("[{}].[{}]", t.schema, t.name),
            // Filegroups, partition functions, and partition schemes are NOT schema-qualified
            ModelElement::Filegroup(f) => format!("[{}]", f.name),
            ModelElement::PartitionFunction(pf) => format!("[{}]", pf.name),
            ModelElement::PartitionScheme(ps) => format!("[{}]", ps.name),
            ModelElement::Synonym(s) => format!("[{}].[{}]", s.schema, s.name),
            ModelElement::Raw(r) => format!("[{}].[{}]", r.schema, r.name),
        }
    }

    /// Get the Name attribute value as it appears in XML.
    /// Returns empty string for elements without Name attribute (SqlDatabaseOptions, inline constraints).
    /// DotNet sorts elements by (Name, Type) where empty names sort first.
    pub fn xml_name_attr(&self) -> String {
        match self {
            // Constraints: emit_name determines if Name attribute is present
            ModelElement::Constraint(c) => {
                if c.emit_name {
                    format!("[{}].[{}]", c.table_schema, c.name)
                } else {
                    String::new() // No Name attribute for inline constraints without emit_name
                }
            }
            // All other elements always have Name attribute (or none like SqlDatabaseOptions)
            _ => self.full_name(),
        }
    }

    /// Get the secondary sort key for elements with the same (Name, Type).
    /// For inline constraints without a Name attribute, this returns the DefiningTable reference
    /// to ensure deterministic ordering that matches DotNet DacFx.
    /// Returns empty string for elements that don't need secondary sorting.
    pub fn secondary_sort_key(&self) -> String {
        match self {
            ModelElement::Constraint(c) => {
                if !c.emit_name {
                    // Inline constraint - sort by DefiningTable reference
                    format!("[{}].[{}]", c.table_schema, c.table_name)
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        }
    }
}

/// Schema element
#[derive(Debug, Clone)]
pub struct SchemaElement {
    pub name: String,
    /// The authorization owner (from AUTHORIZATION clause), if specified
    pub authorization: Option<String>,
}

/// Table element
#[derive(Debug, Clone)]
pub struct TableElement {
    pub schema: String,
    pub name: String,
    pub columns: Vec<ColumnElement>,
    /// Whether this is a graph node table (CREATE TABLE AS NODE)
    pub is_node: bool,
    /// Whether this is a graph edge table (CREATE TABLE AS EDGE)
    pub is_edge: bool,
    /// Disambiguators for SqlInlineConstraintAnnotation elements.
    /// Tables get Annotation elements for constraints that use AttachedAnnotation.
    /// For single named constraint: 1 entry (table gets Annotation, constraint gets AttachedAnnotation)
    /// For exactly 2 named constraints: 2 entries (both constraints get AttachedAnnotation)
    /// For 3+ named constraints: 1 entry (last constraint gets AttachedAnnotation)
    pub inline_constraint_disambiguators: Vec<u32>,
    /// AttachedAnnotation disambiguators to output BEFORE the Annotation element.
    /// DotNet outputs AttachedAnnotations for constraints that appear AFTER the annotated
    /// constraint in the sorted element list, in descending order.
    pub attached_annotations_before_annotation: Vec<u32>,
    /// AttachedAnnotation disambiguators to output AFTER the Annotation element.
    /// DotNet outputs AttachedAnnotations for constraints that appear BEFORE the annotated
    /// constraint in the sorted element list, in ascending order.
    pub attached_annotations_after_annotation: Vec<u32>,
}

/// Column element
#[derive(Debug, Clone)]
pub struct ColumnElement {
    pub name: String,
    pub data_type: String,
    /// Column nullability: Some(true) = explicit NULL, Some(false) = explicit NOT NULL, None = implicit (default nullable)
    pub nullability: Option<bool>,
    pub is_identity: bool,
    pub is_rowguidcol: bool,
    pub is_sparse: bool,
    pub is_filestream: bool,
    pub default_value: Option<String>,
    pub max_length: Option<i32>,
    pub precision: Option<u8>,
    pub scale: Option<u8>,
    /// Attached annotations from inline constraints (CHECK, DEFAULT, UNIQUE, PRIMARY KEY on column)
    /// Each u32 is a disambiguator linking this column to its inline constraint(s)
    /// A column can have multiple (e.g., both a CHECK and DEFAULT constraint)
    pub attached_annotations: Vec<u32>,
    /// Annotation (not AttachedAnnotation) for single named inline constraint.
    /// DotNet pattern: when a table has exactly one named inline constraint,
    /// the column (not the table) carries the Annotation element.
    /// The constraint then carries AttachedAnnotation with the same disambiguator.
    pub inline_constraint_annotation: Option<u32>,
    /// Computed column expression (e.g., "[Qty] * [Price]")
    /// If Some, this is a computed column (SqlComputedColumn) instead of SqlSimpleColumn
    pub computed_expression: Option<String>,
    /// Whether the computed column is PERSISTED (stored physically)
    pub is_persisted: bool,
    /// Collation name for the column (e.g., "Latin1_General_CI_AS")
    /// Only populated for string columns with explicit COLLATE clause
    pub collation: Option<String>,
}

/// View element
#[derive(Debug, Clone)]
pub struct ViewElement {
    pub schema: String,
    pub name: String,
    pub definition: String,
    /// Whether the view has WITH SCHEMABINDING option
    pub is_schema_bound: bool,
    /// Whether the view has WITH CHECK OPTION
    pub is_with_check_option: bool,
    /// Whether the view has WITH VIEW_METADATA option
    pub is_metadata_reported: bool,
}

/// Stored procedure element
#[derive(Debug, Clone)]
pub struct ProcedureElement {
    pub schema: String,
    pub name: String,
    pub definition: String,
    pub parameters: Vec<ParameterElement>,
    /// Whether this procedure is natively compiled (WITH NATIVE_COMPILATION)
    pub is_natively_compiled: bool,
    /// Dynamic column sources discovered in the procedure body (CTEs, temp tables, table variables)
    pub dynamic_sources: Vec<DynamicColumnSource>,
}

/// Parameter element
#[derive(Debug, Clone)]
pub struct ParameterElement {
    pub name: String,
    pub data_type: String,
    pub is_output: bool,
    pub default_value: Option<String>,
}

/// Type of dynamic column source in procedure/function bodies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicColumnSourceType {
    /// Common Table Expression (WITH cte AS (...))
    Cte,
    /// Temporary table (CREATE TABLE #name (...))
    TempTable,
    /// Table variable (DECLARE @name TABLE (...))
    TableVariable,
}

/// A column within a dynamic column source (CTE, temp table, or table variable)
#[derive(Debug, Clone)]
pub struct DynamicColumn {
    /// Column name
    pub name: String,
    /// Data type (e.g., "int", "varchar(50)") - for temp tables and table variables
    pub data_type: Option<String>,
    /// Whether the column is nullable
    pub is_nullable: bool,
    /// Precision for decimal types
    pub precision: Option<u8>,
    /// Scale for decimal types
    pub scale: Option<u8>,
    /// Expression dependencies for CTE columns (references to source columns)
    /// e.g., ["[dbo].[Products].[Id]", "[dbo].[Products].[Name]"]
    pub expression_dependencies: Vec<String>,
}

/// A dynamic column source discovered in a procedure or function body.
/// These are CTEs, temp tables, and table variables that DotNet DacFx
/// tracks as SqlDynamicColumnSource elements.
#[derive(Debug, Clone)]
pub struct DynamicColumnSource {
    /// Name of the source (CTE name, temp table name with #, or table variable name with @)
    pub name: String,
    /// Type of dynamic source
    pub source_type: DynamicColumnSourceType,
    /// Columns in this source
    pub columns: Vec<DynamicColumn>,
}

/// Function type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    Scalar,
    TableValued,
    InlineTableValued,
}

/// Data compression type for indexes and tables
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataCompressionType {
    /// No compression (CompressionLevel = 0)
    None,
    /// Row-level compression (CompressionLevel = 1)
    Row,
    /// Page-level compression (CompressionLevel = 2)
    Page,
    /// Columnstore compression (CompressionLevel = 3)
    Columnstore,
    /// Columnstore archive compression (CompressionLevel = 4)
    ColumnstoreArchive,
}

impl DataCompressionType {
    /// Get the compression level value for model.xml
    pub fn compression_level(&self) -> u8 {
        match self {
            DataCompressionType::None => 0,
            DataCompressionType::Row => 1,
            DataCompressionType::Page => 2,
            DataCompressionType::Columnstore => 3,
            DataCompressionType::ColumnstoreArchive => 4,
        }
    }
}

/// Function element
#[derive(Debug, Clone)]
pub struct FunctionElement {
    pub schema: String,
    pub name: String,
    pub definition: String,
    pub function_type: FunctionType,
    pub parameters: Vec<ParameterElement>,
    pub return_type: Option<String>,
    /// Whether this function is natively compiled (WITH NATIVE_COMPILATION)
    pub is_natively_compiled: bool,
    /// Dynamic column sources discovered in the function body (CTEs, temp tables, table variables)
    pub dynamic_sources: Vec<DynamicColumnSource>,
}

/// Index element
#[derive(Debug, Clone)]
pub struct IndexElement {
    pub name: String,
    pub table_schema: String,
    pub table_name: String,
    /// Key columns in the index with sort direction
    pub columns: Vec<IndexColumn>,
    /// Columns included in the index leaf level (INCLUDE clause)
    pub include_columns: Vec<String>,
    pub is_unique: bool,
    pub is_clustered: bool,
    /// Fill factor percentage (0-100), controls page fill density
    pub fill_factor: Option<u8>,
    /// Filter predicate for filtered indexes (WHERE clause condition)
    pub filter_predicate: Option<String>,
    /// Data compression type (NONE, ROW, PAGE, COLUMNSTORE, COLUMNSTORE_ARCHIVE)
    pub data_compression: Option<DataCompressionType>,
    /// Whether PAD_INDEX is ON (applies fill factor to intermediate pages)
    pub is_padded: bool,
}

/// A column in a full-text index with optional language specification
#[derive(Debug, Clone)]
pub struct FullTextColumnElement {
    /// Column name
    pub name: String,
    /// Language ID (e.g., 1033 for English)
    pub language_id: Option<u32>,
}

/// Full-text index element
#[derive(Debug, Clone)]
pub struct FullTextIndexElement {
    pub table_schema: String,
    pub table_name: String,
    /// Columns included in the full-text index
    pub columns: Vec<FullTextColumnElement>,
    /// Key index name (required unique index)
    pub key_index: String,
    /// Full-text catalog name (optional)
    pub catalog: Option<String>,
    /// Change tracking mode (AUTO, MANUAL, OFF)
    pub change_tracking: Option<String>,
    /// Disambiguator for elements sharing the same name (e.g., table)
    pub disambiguator: Option<u32>,
}

/// Full-text catalog element
#[derive(Debug, Clone)]
pub struct FullTextCatalogElement {
    pub name: String,
    /// Whether this is the default catalog
    pub is_default: bool,
}

/// Sort direction for constraint/index columns
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

/// A column in an index with sort direction
#[derive(Debug, Clone)]
pub struct IndexColumn {
    pub name: String,
    pub is_descending: bool,
}

impl IndexColumn {
    pub fn new(name: String) -> Self {
        Self {
            name,
            is_descending: false,
        }
    }

    pub fn with_direction(name: String, is_descending: bool) -> Self {
        Self {
            name,
            is_descending,
        }
    }
}

/// A column in a constraint with optional sort direction
#[derive(Debug, Clone)]
pub struct ConstraintColumn {
    pub name: String,
    pub sort_direction: SortDirection,
}

impl ConstraintColumn {
    pub fn new(name: String) -> Self {
        Self {
            name,
            sort_direction: SortDirection::Ascending,
        }
    }

    pub fn with_direction(name: String, descending: bool) -> Self {
        Self {
            name,
            sort_direction: if descending {
                SortDirection::Descending
            } else {
                SortDirection::Ascending
            },
        }
    }
}

/// Constraint type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintType {
    PrimaryKey,
    ForeignKey,
    Unique,
    Check,
    Default,
}

/// Constraint element
#[derive(Debug, Clone)]
pub struct ConstraintElement {
    pub name: String,
    pub table_schema: String,
    pub table_name: String,
    pub constraint_type: ConstraintType,
    pub columns: Vec<ConstraintColumn>,
    pub definition: Option<String>,
    /// For foreign keys: referenced table
    pub referenced_table: Option<String>,
    /// For foreign keys: referenced columns
    pub referenced_columns: Option<Vec<String>>,
    /// Whether this constraint is clustered (for PK/unique)
    pub is_clustered: Option<bool>,
    /// Whether this is an inline constraint (defined within a column definition).
    /// DotNet DacFx treats ALL column-level constraints as inline, regardless of whether
    /// they have explicit CONSTRAINT names. Only table-level constraints (at end of
    /// CREATE TABLE or via ALTER TABLE) are treated as non-inline.
    /// Inline constraints have no Name attribute in XML and get SqlInlineConstraintAnnotation.
    pub is_inline: bool,
    /// Disambiguator for SqlInlineConstraintAnnotation
    /// Every constraint gets a unique disambiguator (starting from 3)
    pub inline_constraint_disambiguator: Option<u32>,
    /// Whether this constraint writes Annotation (true) or AttachedAnnotation (false).
    /// DotNet pattern:
    /// - Inline constraints: always use Annotation
    /// - Single named constraint in table: use AttachedAnnotation (table uses Annotation)
    /// - Multiple named constraints in table: most use Annotation, one uses AttachedAnnotation
    pub uses_annotation: bool,
    /// Whether to emit the Name attribute in XML.
    /// - True for all table-level (non-inline) constraints
    /// - True for inline constraints with explicit CONSTRAINT [name] in SQL
    /// - False for inline constraints without explicit CONSTRAINT name
    pub emit_name: bool,
    /// Source order within the table's CREATE TABLE statement.
    /// Used to assign disambiguators in the same order as DotNet (source order, not alphabetical).
    /// Set during model building based on the order constraints appear in SQL.
    pub source_order: u32,
}

/// Sequence element
#[derive(Debug, Clone)]
pub struct SequenceElement {
    pub schema: String,
    pub name: String,
    pub definition: String,
    /// Data type (e.g., "INT", "BIGINT")
    pub data_type: Option<String>,
    /// START WITH value
    pub start_value: Option<i64>,
    /// INCREMENT BY value
    pub increment_value: Option<i64>,
    /// MINVALUE value (None means NO MINVALUE)
    pub min_value: Option<i64>,
    /// MAXVALUE value (None means NO MAXVALUE)
    pub max_value: Option<i64>,
    /// CYCLE / NO CYCLE
    pub is_cycling: bool,
    /// Explicit NO MINVALUE
    pub has_no_min_value: bool,
    /// Explicit NO MAXVALUE
    pub has_no_max_value: bool,
    /// CACHE size (None means default cache)
    pub cache_size: Option<i64>,
}

/// User-defined type element (table types, etc.)
#[derive(Debug, Clone)]
pub struct UserDefinedTypeElement {
    pub schema: String,
    pub name: String,
    pub definition: String,
    /// Columns for table types (if parsed)
    pub columns: Vec<TableTypeColumnElement>,
    /// Constraints for table types (PRIMARY KEY, UNIQUE, CHECK, INDEX)
    pub constraints: Vec<TableTypeConstraint>,
}

/// Scalar type element (alias type) - CREATE TYPE x FROM basetype
/// e.g., CREATE TYPE [dbo].[PhoneNumber] FROM VARCHAR(20) NOT NULL
#[derive(Debug, Clone)]
pub struct ScalarTypeElement {
    pub schema: String,
    pub name: String,
    /// The base type (e.g., VARCHAR, DECIMAL, NVARCHAR)
    pub base_type: String,
    /// Whether this type allows NULL values (false if NOT NULL specified)
    pub is_nullable: bool,
    /// Length for string types (VARCHAR, NVARCHAR, CHAR, etc.)
    pub length: Option<i32>,
    /// Precision for decimal types
    pub precision: Option<u8>,
    /// Scale for decimal types
    pub scale: Option<u8>,
}

/// Column element for table types
#[derive(Debug, Clone)]
pub struct TableTypeColumnElement {
    pub name: String,
    pub data_type: String,
    /// Column nullability: Some(true) = explicit NULL, Some(false) = explicit NOT NULL, None = implicit
    /// DotNet emits IsNullable="True" for nullable columns (Some(true) or None)
    pub nullability: Option<bool>,
    pub default_value: Option<String>,
    pub max_length: Option<i32>,
    pub precision: Option<u8>,
    pub scale: Option<u8>,
}

/// Constraint for table types
#[derive(Debug, Clone)]
pub enum TableTypeConstraint {
    PrimaryKey {
        columns: Vec<ConstraintColumn>,
        is_clustered: bool,
    },
    Unique {
        columns: Vec<ConstraintColumn>,
        is_clustered: bool,
    },
    Check {
        expression: String,
    },
    Index {
        name: String,
        columns: Vec<String>,
        is_unique: bool,
        is_clustered: bool,
    },
}

/// DML Trigger element
#[derive(Debug, Clone)]
pub struct TriggerElement {
    pub schema: String,
    pub name: String,
    /// The raw SQL definition including CREATE TRIGGER
    pub definition: String,
    /// Schema of the parent table/view
    pub parent_schema: String,
    /// Name of the parent table/view
    pub parent_name: String,
    /// True if trigger fires on INSERT
    pub is_insert_trigger: bool,
    /// True if trigger fires on UPDATE
    pub is_update_trigger: bool,
    /// True if trigger fires on DELETE
    pub is_delete_trigger: bool,
    /// Trigger type: 2 = AFTER, 3 = INSTEAD OF
    pub trigger_type: u8,
}

/// Generic raw element for statements that couldn't be fully parsed
#[derive(Debug, Clone)]
pub struct RawElement {
    pub schema: String,
    pub name: String,
    pub sql_type: String,
    pub definition: String,
}

/// Extended property element (from sp_addextendedproperty)
#[derive(Debug, Clone)]
pub struct ExtendedPropertyElement {
    /// Property name (e.g., "MS_Description")
    pub property_name: String,
    /// Property value (e.g., "Unique identifier for the documented item")
    pub property_value: String,
    /// Target schema (e.g., "dbo")
    pub target_schema: String,
    /// Target object (table name for level1, e.g., "DocumentedTable")
    pub target_object: String,
    /// Target column (if level2 is COLUMN, e.g., "Id")
    pub target_column: Option<String>,
    /// Level 1 type (e.g., "TABLE", "VIEW", "PROCEDURE", "FUNCTION")
    pub level1type: Option<String>,
    /// Level 2 type (e.g., "COLUMN", "INDEX", "CONSTRAINT")
    pub level2type: Option<String>,
}

impl ExtendedPropertyElement {
    /// Convert SQL Server level type to DotNet SqlType prefix for extended property naming.
    /// DotNet uses different type names than SQL Server's sp_addextendedproperty:
    /// - TABLE -> SqlTableBase (not SqlTable)
    /// - COLUMN -> SqlColumn
    /// - VIEW -> SqlView
    /// - PROCEDURE -> SqlProcedure
    /// - FUNCTION -> SqlScalarFunction (simplified, functions could vary)
    fn level_type_to_sql_type_prefix(level_type: &str) -> &'static str {
        match level_type.to_uppercase().as_str() {
            "TABLE" => "SqlTableBase",
            "COLUMN" => "SqlColumn",
            "VIEW" => "SqlView",
            "PROCEDURE" => "SqlProcedure",
            "FUNCTION" => "SqlScalarFunction",
            "INDEX" => "SqlIndex",
            "CONSTRAINT" => "SqlConstraint",
            _ => "SqlTableBase", // Default fallback for unknown types
        }
    }

    /// Get the full qualified name for this extended property
    /// Format: [ParentType].[schema].[object].[property] for table-level properties
    /// Format: [ParentType].[schema].[object].[column].[property] for column-level properties
    /// Where ParentType is the SqlType of the object being extended (e.g., SqlTableBase, SqlColumn)
    pub fn full_name(&self) -> String {
        if let Some(ref column) = self.target_column {
            // Column-level property: prefix is SqlColumn (from level2type)
            let prefix = self
                .level2type
                .as_ref()
                .map(|t| Self::level_type_to_sql_type_prefix(t))
                .unwrap_or("SqlColumn");
            format!(
                "[{}].[{}].[{}].[{}].[{}]",
                prefix, self.target_schema, self.target_object, column, self.property_name
            )
        } else {
            // Table/View/Procedure-level property: prefix is from level1type
            let prefix = self
                .level1type
                .as_ref()
                .map(|t| Self::level_type_to_sql_type_prefix(t))
                .unwrap_or("SqlTableBase");
            format!(
                "[{}].[{}].[{}].[{}]",
                prefix, self.target_schema, self.target_object, self.property_name
            )
        }
    }

    /// Get the reference to the extended object
    /// For column-level: [schema].[table].[column]
    /// For table-level: [schema].[table]
    pub fn extends_object_ref(&self) -> String {
        if let Some(ref column) = self.target_column {
            format!(
                "[{}].[{}].[{}]",
                self.target_schema, self.target_object, column
            )
        } else {
            format!("[{}].[{}]", self.target_schema, self.target_object)
        }
    }
}

/// Filegroup element (ALTER DATABASE ... ADD FILEGROUP)
/// Filegroups are NOT schema-qualified in SQL Server
#[derive(Debug, Clone)]
pub struct FilegroupElement {
    pub name: String,
    /// Whether this filegroup contains memory-optimized data
    pub contains_memory_optimized_data: bool,
}

/// Partition function element (CREATE PARTITION FUNCTION)
/// Partition functions are NOT schema-qualified in SQL Server
#[derive(Debug, Clone)]
pub struct PartitionFunctionElement {
    pub name: String,
    /// Data type of the partition column (e.g., "INT", "DATETIME", "DATE")
    pub data_type: String,
    /// Boundary values that define partitions
    pub boundary_values: Vec<String>,
    /// Whether boundary is RIGHT or LEFT (default is RIGHT for RANGE RIGHT)
    pub is_range_right: bool,
}

/// Partition scheme element (CREATE PARTITION SCHEME)
/// Partition schemes are NOT schema-qualified in SQL Server
#[derive(Debug, Clone)]
pub struct PartitionSchemeElement {
    pub name: String,
    /// Name of the partition function this scheme references
    pub partition_function: String,
    /// List of filegroups to map partitions to
    pub filegroups: Vec<String>,
}

/// Synonym element (CREATE SYNONYM ... FOR ...)
#[derive(Debug, Clone)]
pub struct SynonymElement {
    pub schema: String,
    pub name: String,
    /// Target schema (the schema of the referenced object)
    pub target_schema: String,
    /// Target name (the name of the referenced object)
    pub target_name: String,
    /// Target database (for cross-database synonyms)
    pub target_database: Option<String>,
    /// Target server (for cross-server synonyms)
    pub target_server: Option<String>,
}
