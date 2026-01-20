//! T-SQL parsing

mod sqlcmd;
mod tsql_parser;

pub use sqlcmd::expand_includes;
pub use tsql_parser::{
    parse_sql_file, parse_sql_files, ExtractedDefaultConstraint, FallbackFunctionType,
    FallbackStatementType, ParsedStatement, BINARY_MAX_SENTINEL,
};
