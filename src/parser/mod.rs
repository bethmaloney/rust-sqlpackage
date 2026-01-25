//! T-SQL parsing

mod sqlcmd;
mod tsql_parser;

pub use sqlcmd::expand_includes;
pub use tsql_parser::{
    extract_extended_property_from_sql, parse_sql_file, parse_sql_files,
    ExtractedConstraintColumn, ExtractedDefaultConstraint, ExtractedExtendedProperty,
    ExtractedFullTextColumn, ExtractedFunctionParameter, ExtractedTableColumn,
    ExtractedTableConstraint, ExtractedTableTypeColumn, ExtractedTableTypeConstraint,
    FallbackFunctionType, FallbackStatementType, ParsedStatement, BINARY_MAX_SENTINEL,
};
