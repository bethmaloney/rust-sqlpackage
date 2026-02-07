//! T-SQL parsing

mod column_parser;
mod constraint_parser;
mod extended_property_parser;
mod fulltext_parser;
mod function_parser;
pub mod ident_extract;
pub mod identifier_utils;
pub mod index_parser;
mod preprocess_parser;
mod procedure_parser;
mod security_parser;
mod sequence_parser;
mod sqlcmd;
mod statement_parser;
mod storage_parser;
mod synonym_parser;
mod table_type_parser;
pub mod token_parser_base;
mod trigger_parser;
mod tsql_dialect;
mod tsql_parser;

pub use function_parser::{extract_function_parameters_tokens, TokenParsedParameter};
pub use procedure_parser::{
    extract_procedure_parameters_tokens, parse_alter_procedure_full, parse_create_procedure_full,
    TokenParsedProcedure, TokenParsedProcedureParameter,
};
pub use sqlcmd::expand_includes;
pub use tsql_dialect::ExtendedTsqlDialect;
pub use tsql_parser::{
    extract_extended_property_from_sql, parse_sql_file, parse_sql_files, ExtractedConstraintColumn,
    ExtractedDefaultConstraint, ExtractedExtendedProperty, ExtractedFullTextColumn,
    ExtractedFunctionParameter, ExtractedTableColumn, ExtractedTableConstraint,
    ExtractedTableTypeColumn, ExtractedTableTypeConstraint, FallbackFunctionType,
    FallbackStatementType, ParsedStatement, BINARY_MAX_SENTINEL,
};
