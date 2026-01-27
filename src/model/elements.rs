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
                // Full-text index name format: [schema].[table].[FullTextIndex]
                format!("[{}].[{}].[FullTextIndex]", f.table_schema, f.table_name)
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
            ModelElement::Raw(r) => format!("[{}].[{}]", r.schema, r.name),
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
    /// Disambiguator for SqlInlineConstraintAnnotation (if table has inline constraints)
    /// Tables with inline constraints get their own annotation, and named table-level
    /// constraints (like CONSTRAINT [PK_Table]) reference this via AttachedAnnotation
    pub inline_constraint_disambiguator: Option<u32>,
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
    /// Computed column expression (e.g., "[Qty] * [Price]")
    /// If Some, this is a computed column (SqlComputedColumn) instead of SqlSimpleColumn
    pub computed_expression: Option<String>,
    /// Whether the computed column is PERSISTED (stored physically)
    pub is_persisted: bool,
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
}

/// Parameter element
#[derive(Debug, Clone)]
pub struct ParameterElement {
    pub name: String,
    pub data_type: String,
    pub is_output: bool,
    pub default_value: Option<String>,
}

/// Function type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    Scalar,
    TableValued,
    InlineTableValued,
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
}

/// Index element
#[derive(Debug, Clone)]
pub struct IndexElement {
    pub name: String,
    pub table_schema: String,
    pub table_name: String,
    pub columns: Vec<String>,
    /// Columns included in the index leaf level (INCLUDE clause)
    pub include_columns: Vec<String>,
    pub is_unique: bool,
    pub is_clustered: bool,
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
    /// Whether this is an inline constraint (defined on column without CONSTRAINT keyword)
    /// Inline constraints have no Name attribute in XML and get SqlInlineConstraintAnnotation
    pub is_inline: bool,
    /// Disambiguator for SqlInlineConstraintAnnotation (inline constraints only)
    /// Also used for AttachedAnnotation on named constraints that reference a table's disambiguator
    pub inline_constraint_disambiguator: Option<u32>,
}

/// Sequence element
#[derive(Debug, Clone)]
pub struct SequenceElement {
    pub schema: String,
    pub name: String,
    pub definition: String,
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
    /// Note: DotNet never emits IsNullable for SqlTableTypeSimpleColumn regardless of this value
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
