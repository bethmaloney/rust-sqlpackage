//! Parser for SQL Server storage elements (Filegroup, Partition Function, Partition Scheme)
//!
//! These are database-level storage constructs that don't use schema qualification.

use crate::parser::token_parser_base::TokenParser;

/// Result of parsing ALTER DATABASE ... ADD FILEGROUP
#[derive(Debug, Clone)]
pub struct ParsedFilegroup {
    pub name: String,
    /// Whether this filegroup contains memory-optimized data
    pub contains_memory_optimized_data: bool,
}

/// Result of parsing CREATE PARTITION FUNCTION
#[derive(Debug, Clone)]
pub struct ParsedPartitionFunction {
    pub name: String,
    /// Data type of the partition column
    pub data_type: String,
    /// Boundary values that define partitions
    pub boundary_values: Vec<String>,
    /// Whether boundary is RIGHT (true) or LEFT (false)
    pub is_range_right: bool,
}

/// Result of parsing CREATE PARTITION SCHEME
#[derive(Debug, Clone)]
pub struct ParsedPartitionScheme {
    pub name: String,
    /// Name of the partition function this scheme references
    pub partition_function: String,
    /// List of filegroups to map partitions to
    pub filegroups: Vec<String>,
}

/// Parse ALTER DATABASE ... ADD FILEGROUP statement
///
/// Examples:
/// - ALTER DATABASE [$(DatabaseName)] ADD FILEGROUP [USERDATA];
/// - ALTER DATABASE MyDB ADD FILEGROUP [WWI_MemoryOptimized_Data] CONTAINS MEMORY_OPTIMIZED_DATA;
pub fn parse_filegroup_tokens(sql: &str) -> Option<ParsedFilegroup> {
    let mut parser = TokenParser::new(sql)?;

    // Skip ALTER DATABASE
    parser.skip_keyword("ALTER")?;
    parser.skip_keyword("DATABASE")?;

    // Skip database name (can be bracketed variable like [$(DatabaseName)] or identifier)
    parser.skip_identifier()?;

    // Skip ADD FILEGROUP
    parser.skip_keyword("ADD")?;
    parser.skip_keyword("FILEGROUP")?;

    // Get filegroup name
    let name = parser.expect_identifier()?;

    // Check for CONTAINS MEMORY_OPTIMIZED_DATA
    let contains_memory_optimized_data =
        parser.try_skip_keyword("CONTAINS") && parser.try_skip_keyword("MEMORY_OPTIMIZED_DATA");

    Some(ParsedFilegroup {
        name,
        contains_memory_optimized_data,
    })
}

/// Parse CREATE PARTITION FUNCTION statement
///
/// Examples:
/// - CREATE PARTITION FUNCTION [PF_TransactionDate](DATE) AS RANGE RIGHT FOR VALUES ('01/01/2014 00:00:00', '01/01/2015 00:00:00');
/// - CREATE PARTITION FUNCTION PF_Int(INT) AS RANGE LEFT FOR VALUES (100, 200, 300);
pub fn parse_partition_function_tokens(sql: &str) -> Option<ParsedPartitionFunction> {
    let mut parser = TokenParser::new(sql)?;

    // Skip CREATE PARTITION FUNCTION
    parser.skip_keyword("CREATE")?;
    parser.skip_keyword("PARTITION")?;
    parser.skip_keyword("FUNCTION")?;

    // Get function name
    let name = parser.expect_identifier()?;

    // Skip opening paren for parameter type
    parser.skip_symbol('(')?;

    // Get data type - can be multi-word like "DATETIME2(7)"
    let data_type = parser.expect_data_type()?;

    // Skip closing paren
    parser.skip_symbol(')')?;

    // Skip AS RANGE
    parser.skip_keyword("AS")?;
    parser.skip_keyword("RANGE")?;

    // Get boundary side (RIGHT or LEFT, default is RIGHT)
    let is_range_right = if parser.try_skip_keyword("RIGHT") {
        true
    } else if parser.try_skip_keyword("LEFT") {
        false
    } else {
        true // Default is RIGHT
    };

    // Skip FOR VALUES
    parser.skip_keyword("FOR")?;
    parser.skip_keyword("VALUES")?;

    // Parse boundary values list
    parser.skip_symbol('(')?;
    let boundary_values = parser.parse_string_list()?;

    Some(ParsedPartitionFunction {
        name,
        data_type,
        boundary_values,
        is_range_right,
    })
}

/// Parse CREATE PARTITION SCHEME statement
///
/// Examples:
/// - CREATE PARTITION SCHEME [PS_TransactionDate] AS PARTITION [PF_TransactionDate] TO ([USERDATA], [USERDATA], [USERDATA]);
/// - CREATE PARTITION SCHEME PS_Int AS PARTITION PF_Int TO (FG1, FG2, FG3, FG4);
/// - CREATE PARTITION SCHEME PS_All AS PARTITION PF_Range ALL TO ([PRIMARY]);
pub fn parse_partition_scheme_tokens(sql: &str) -> Option<ParsedPartitionScheme> {
    let mut parser = TokenParser::new(sql)?;

    // Skip CREATE PARTITION SCHEME
    parser.skip_keyword("CREATE")?;
    parser.skip_keyword("PARTITION")?;
    parser.skip_keyword("SCHEME")?;

    // Get scheme name
    let name = parser.expect_identifier()?;

    // Skip AS PARTITION
    parser.skip_keyword("AS")?;
    parser.skip_keyword("PARTITION")?;

    // Get partition function name
    let partition_function = parser.expect_identifier()?;

    // Check for ALL keyword (all partitions go to same filegroup)
    let _is_all = parser.try_skip_keyword("ALL");

    // Skip TO
    parser.skip_keyword("TO")?;

    // Parse filegroup list
    parser.skip_symbol('(')?;
    let filegroups = parser.parse_identifier_list()?;

    Some(ParsedPartitionScheme {
        name,
        partition_function,
        filegroups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    mod filegroup_tests {
        use super::*;

        #[test]
        fn test_parse_basic_filegroup() {
            let sql = "ALTER DATABASE [$(DatabaseName)] ADD FILEGROUP [USERDATA];";
            let result = parse_filegroup_tokens(sql).unwrap();
            assert_eq!(result.name, "USERDATA");
            assert!(!result.contains_memory_optimized_data);
        }

        #[test]
        fn test_parse_filegroup_without_brackets() {
            let sql = "ALTER DATABASE MyDB ADD FILEGROUP UserData";
            let result = parse_filegroup_tokens(sql).unwrap();
            assert_eq!(result.name, "UserData");
            assert!(!result.contains_memory_optimized_data);
        }

        #[test]
        fn test_parse_memory_optimized_filegroup() {
            let sql = "ALTER DATABASE [$(DatabaseName)] ADD FILEGROUP [WWI_MemoryOptimized_Data] CONTAINS MEMORY_OPTIMIZED_DATA;";
            let result = parse_filegroup_tokens(sql).unwrap();
            assert_eq!(result.name, "WWI_MemoryOptimized_Data");
            assert!(result.contains_memory_optimized_data);
        }

        #[test]
        fn test_parse_filegroup_multiline() {
            let sql = "ALTER DATABASE [$(DatabaseName)]\n    ADD FILEGROUP [USERDATA];";
            let result = parse_filegroup_tokens(sql).unwrap();
            assert_eq!(result.name, "USERDATA");
        }
    }

    mod partition_function_tests {
        use super::*;

        #[test]
        fn test_parse_basic_partition_function() {
            let sql = "CREATE PARTITION FUNCTION [PF_TransactionDate](DATE) AS RANGE RIGHT FOR VALUES ('01/01/2014 00:00:00', '01/01/2015 00:00:00');";
            let result = parse_partition_function_tokens(sql).unwrap();
            assert_eq!(result.name, "PF_TransactionDate");
            assert_eq!(result.data_type, "DATE");
            assert!(result.is_range_right);
            assert_eq!(result.boundary_values.len(), 2);
            assert_eq!(result.boundary_values[0], "01/01/2014 00:00:00");
            assert_eq!(result.boundary_values[1], "01/01/2015 00:00:00");
        }

        #[test]
        fn test_parse_partition_function_datetime() {
            let sql = "CREATE PARTITION FUNCTION [PF_TransactionDateTime](DATETIME) AS RANGE RIGHT FOR VALUES ('01/01/2014 00:00:00', '01/01/2015 00:00:00', '01/01/2016 00:00:00', '01/01/2017 00:00:00');";
            let result = parse_partition_function_tokens(sql).unwrap();
            assert_eq!(result.name, "PF_TransactionDateTime");
            assert_eq!(result.data_type, "DATETIME");
            assert!(result.is_range_right);
            assert_eq!(result.boundary_values.len(), 4);
        }

        #[test]
        fn test_parse_partition_function_range_left() {
            let sql =
                "CREATE PARTITION FUNCTION PF_Int(INT) AS RANGE LEFT FOR VALUES (100, 200, 300)";
            let result = parse_partition_function_tokens(sql).unwrap();
            assert_eq!(result.name, "PF_Int");
            assert_eq!(result.data_type, "INT");
            assert!(!result.is_range_right);
            assert_eq!(result.boundary_values.len(), 3);
        }

        #[test]
        fn test_parse_partition_function_multiline() {
            let sql = r#"CREATE PARTITION FUNCTION [PF_TransactionDate](DATE)
    AS RANGE RIGHT
    FOR VALUES ('01/01/2014 00:00:00', '01/01/2015 00:00:00', '01/01/2016 00:00:00', '01/01/2017 00:00:00');"#;
            let result = parse_partition_function_tokens(sql).unwrap();
            assert_eq!(result.name, "PF_TransactionDate");
            assert_eq!(result.data_type, "DATE");
            assert_eq!(result.boundary_values.len(), 4);
        }
    }

    mod partition_scheme_tests {
        use super::*;

        #[test]
        fn test_parse_basic_partition_scheme() {
            let sql = "CREATE PARTITION SCHEME [PS_TransactionDate] AS PARTITION [PF_TransactionDate] TO ([USERDATA], [USERDATA], [USERDATA]);";
            let result = parse_partition_scheme_tokens(sql).unwrap();
            assert_eq!(result.name, "PS_TransactionDate");
            assert_eq!(result.partition_function, "PF_TransactionDate");
            assert_eq!(result.filegroups.len(), 3);
            assert_eq!(result.filegroups[0], "USERDATA");
        }

        #[test]
        fn test_parse_partition_scheme_multiple_filegroups() {
            let sql = "CREATE PARTITION SCHEME [PS_TransactionDateTime] AS PARTITION [PF_TransactionDateTime] TO ([USERDATA], [USERDATA], [USERDATA], [USERDATA], [USERDATA], [USERDATA]);";
            let result = parse_partition_scheme_tokens(sql).unwrap();
            assert_eq!(result.name, "PS_TransactionDateTime");
            assert_eq!(result.partition_function, "PF_TransactionDateTime");
            assert_eq!(result.filegroups.len(), 6);
        }

        #[test]
        fn test_parse_partition_scheme_all() {
            let sql = "CREATE PARTITION SCHEME PS_All AS PARTITION PF_Range ALL TO ([PRIMARY])";
            let result = parse_partition_scheme_tokens(sql).unwrap();
            assert_eq!(result.name, "PS_All");
            assert_eq!(result.partition_function, "PF_Range");
            assert_eq!(result.filegroups.len(), 1);
            assert_eq!(result.filegroups[0], "PRIMARY");
        }

        #[test]
        fn test_parse_partition_scheme_multiline() {
            let sql = r#"CREATE PARTITION SCHEME [PS_TransactionDate]
    AS PARTITION [PF_TransactionDate]
    TO ([USERDATA], [USERDATA], [USERDATA], [USERDATA], [USERDATA], [USERDATA]);"#;
            let result = parse_partition_scheme_tokens(sql).unwrap();
            assert_eq!(result.name, "PS_TransactionDate");
            assert_eq!(result.partition_function, "PF_TransactionDate");
            assert_eq!(result.filegroups.len(), 6);
        }
    }
}
