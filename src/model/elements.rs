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
    ExtendedProperty(ExtendedPropertyElement),
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
            ModelElement::ExtendedProperty(_) => "SqlExtendedProperty",
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
                format!("[{}].[{}].[{}]", c.table_schema, c.table_name, c.name)
            }
            ModelElement::Sequence(s) => format!("[{}].[{}]", s.schema, s.name),
            ModelElement::UserDefinedType(u) => format!("[{}].[{}]", u.schema, u.name),
            ModelElement::ExtendedProperty(e) => e.full_name(),
            ModelElement::Raw(r) => format!("[{}].[{}]", r.schema, r.name),
        }
    }
}

/// Schema element
#[derive(Debug, Clone)]
pub struct SchemaElement {
    pub name: String,
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
}

/// Column element
#[derive(Debug, Clone)]
pub struct ColumnElement {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub is_identity: bool,
    pub is_rowguidcol: bool,
    pub is_sparse: bool,
    pub is_filestream: bool,
    pub default_value: Option<String>,
    pub max_length: Option<i32>,
    pub precision: Option<u8>,
    pub scale: Option<u8>,
    /// Disambiguator for inline constraint annotation (if column has inline constraints)
    pub inline_constraint_disambiguator: Option<u32>,
}

/// View element
#[derive(Debug, Clone)]
pub struct ViewElement {
    pub schema: String,
    pub name: String,
    pub definition: String,
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

/// Column element for table types
#[derive(Debug, Clone)]
pub struct TableTypeColumnElement {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
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
}

impl ExtendedPropertyElement {
    /// Get the full qualified name for this extended property
    /// Format: [schema].[object].[property] for table-level properties
    /// Format: [schema].[object].[column].[property] for column-level properties
    pub fn full_name(&self) -> String {
        if let Some(ref column) = self.target_column {
            format!(
                "[{}].[{}].[{}].[{}]",
                self.target_schema, self.target_object, column, self.property_name
            )
        } else {
            format!(
                "[{}].[{}].[{}]",
                self.target_schema, self.target_object, self.property_name
            )
        }
    }

    /// Get the reference to the extended object
    /// For column-level: [schema].[table].[column]
    /// For table-level: [schema].[table]
    pub fn extends_object_ref(&self) -> String {
        if let Some(ref column) = self.target_column {
            format!("[{}].[{}].[{}]", self.target_schema, self.target_object, column)
        } else {
            format!("[{}].[{}]", self.target_schema, self.target_object)
        }
    }
}
