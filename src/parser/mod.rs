//! T-SQL parsing

mod column_parser;
mod constraint_parser;
mod extended_property_parser;
mod fulltext_parser;
mod function_parser;
mod index_parser;
mod preprocess_parser;
mod procedure_parser;
mod sequence_parser;
mod sqlcmd;
mod statement_parser;
mod table_type_parser;
mod trigger_parser;
mod tsql_dialect;
mod tsql_parser;

pub use sqlcmd::expand_includes;
pub use tsql_dialect::ExtendedTsqlDialect;
pub use tsql_parser::{
    extract_extended_property_from_sql, parse_sql_file, parse_sql_files, ExtractedConstraintColumn,
    ExtractedDefaultConstraint, ExtractedExtendedProperty, ExtractedFullTextColumn,
    ExtractedFunctionParameter, ExtractedTableColumn, ExtractedTableConstraint,
    ExtractedTableTypeColumn, ExtractedTableTypeConstraint, FallbackFunctionType,
    FallbackStatementType, ParsedStatement, BINARY_MAX_SENTINEL,
};
