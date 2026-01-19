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
    Constraint(ConstraintElement),
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
                FunctionType::TableValued => "SqlTableValuedFunction",
                FunctionType::InlineTableValued => "SqlInlineTableValuedFunction",
            },
            ModelElement::Index(_) => "SqlIndex",
            ModelElement::Constraint(c) => match c.constraint_type {
                ConstraintType::PrimaryKey => "SqlPrimaryKeyConstraint",
                ConstraintType::ForeignKey => "SqlForeignKeyConstraint",
                ConstraintType::Unique => "SqlUniqueConstraint",
                ConstraintType::Check => "SqlCheckConstraint",
                ConstraintType::Default => "SqlDefaultConstraint",
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
            ModelElement::Index(i) => format!("[{}].[{}].[{}]", i.table_schema, i.table_name, i.name),
            ModelElement::Constraint(c) => format!("[{}].[{}].[{}]", c.table_schema, c.table_name, c.name),
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
}

/// Column element
#[derive(Debug, Clone)]
pub struct ColumnElement {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub is_identity: bool,
    pub default_value: Option<String>,
    pub max_length: Option<i32>,
    pub precision: Option<u8>,
    pub scale: Option<u8>,
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
}

/// Index element
#[derive(Debug, Clone)]
pub struct IndexElement {
    pub name: String,
    pub table_schema: String,
    pub table_name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_clustered: bool,
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
    pub columns: Vec<String>,
    pub definition: Option<String>,
    /// For foreign keys: referenced table
    pub referenced_table: Option<String>,
    /// For foreign keys: referenced columns
    pub referenced_columns: Option<Vec<String>>,
}
