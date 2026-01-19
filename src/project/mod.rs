//! SQL project file parsing

mod sqlproj_parser;

pub use sqlproj_parser::{parse_sqlproj, DacpacReference, SqlProject, SqlServerVersion};
