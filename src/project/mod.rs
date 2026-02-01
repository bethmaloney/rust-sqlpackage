//! SQL project file parsing

mod collation;
mod sqlproj_parser;

pub use collation::{parse_collation_info, CollationInfo};
pub use sqlproj_parser::{
    parse_sqlproj, DacpacReference, DatabaseOptions, PackageReference, SqlCmdVariable, SqlProject,
    SqlServerVersion,
};
