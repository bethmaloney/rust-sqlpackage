//! T-SQL parsing

mod tsql_parser;

pub use tsql_parser::{
    parse_sql_file, parse_sql_files, FallbackFunctionType, FallbackStatementType, ParsedStatement,
};
