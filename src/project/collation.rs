//! Collation parsing utilities for SQL Server collation names
//!
//! This module provides functionality to parse SQL Server collation names and extract:
//! - LCID (Locale ID) for the collation
//! - Case sensitivity (CI = case-insensitive, CS = case-sensitive)
//!
//! Reference: https://learn.microsoft.com/en-us/sql/relational-databases/collations/collation-and-unicode-support

/// Information extracted from a SQL Server collation name
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollationInfo {
    /// Locale ID (LCID) for the collation
    pub lcid: u32,
    /// Whether the collation is case-sensitive
    pub case_sensitive: bool,
}

impl Default for CollationInfo {
    fn default() -> Self {
        // Default: US English (1033), case-insensitive
        Self {
            lcid: 1033,
            case_sensitive: false,
        }
    }
}

/// Static mapping from collation prefix to LCID
/// Generated from SQL Server sys.fn_helpcollations() function
///
/// Common collations and their LCIDs:
/// - Latin1_General -> 1033 (US English)
/// - SQL_Latin1_General_CP1 -> 1033 (US English)
/// - Japanese -> 1041
/// - Chinese_PRC -> 2052
/// - Korean -> 1042
/// - Turkish -> 1055
/// - Arabic -> 1025
/// - Hebrew -> 1037
/// - Thai -> 1054
/// - Vietnamese -> 1066
/// - Ukrainian -> 1058
/// - Czech -> 1029
/// - Polish -> 1045
/// - Finnish -> 1035
/// - Swedish -> 1053
/// - Danish -> 1030
/// - Norwegian -> 1044
/// - French -> 1036
/// - German -> 1031
/// - Spanish -> 3082 (Modern Spanish)
/// - Italian -> 1040
/// - Portuguese -> 1046 (Brazilian)
/// - Dutch -> 1043
/// - Greek -> 1032
/// - Russian -> 1049
/// - Cyrillic_General -> 1049
///
/// The mapping is sorted by prefix length (longest first) to ensure correct matching
/// for prefixes that share common beginnings (e.g., "SQL_Latin1_General_CP1" before "Latin1_General")
static COLLATION_LCID_MAP: &[(&str, u32)] = &[
    // SQL-style collations (longer prefixes first)
    ("SQL_Latin1_General_CP1254", 1055),   // Turkish
    ("SQL_Latin1_General_CP1253", 1032),   // Greek
    ("SQL_Latin1_General_CP1251", 1049),   // Cyrillic
    ("SQL_Latin1_General_CP1250", 1045),   // Central European (Polish)
    ("SQL_Latin1_General_CP850", 1033),    // US English (DOS)
    ("SQL_Latin1_General_CP437", 1033),    // US English (DOS)
    ("SQL_Latin1_General_CP1", 1033),      // US English
    ("SQL_Latin1_General_Pref_CP1", 1033), // US English with preferences
    ("SQL_AltDiction_CP1253", 1032),       // Greek alternate
    ("SQL_AltDiction_CP850", 1033),        // US English alternate
    ("SQL_Croatian", 1050),                // Croatian
    ("SQL_Czech", 1029),                   // Czech
    ("SQL_Danish_Pref_CP1", 1030),         // Danish
    ("SQL_EBCDIC037", 1033),               // EBCDIC US
    ("SQL_EBCDIC273", 1031),               // EBCDIC German
    ("SQL_EBCDIC277", 1030),               // EBCDIC Danish
    ("SQL_EBCDIC278", 1035),               // EBCDIC Finnish
    ("SQL_EBCDIC280", 1040),               // EBCDIC Italian
    ("SQL_EBCDIC284", 3082),               // EBCDIC Spanish
    ("SQL_EBCDIC285", 2057),               // EBCDIC UK English
    ("SQL_EBCDIC297", 1036),               // EBCDIC French
    ("SQL_Estonian", 1061),                // Estonian
    ("SQL_Hungarian", 1038),               // Hungarian
    ("SQL_Icelandic_Pref_CP1", 1039),      // Icelandic
    ("SQL_Latin1_General", 1033),          // US English (generic)
    ("SQL_Latvian", 1062),                 // Latvian
    ("SQL_Lithuanian", 1063),              // Lithuanian
    ("SQL_Polish", 1045),                  // Polish
    ("SQL_Romanian", 1048),                // Romanian
    ("SQL_Scandinavian_Pref_CP850", 1044), // Scandinavian
    ("SQL_Scandinavian_CP850", 1044),      // Scandinavian
    ("SQL_Slovak", 1051),                  // Slovak
    ("SQL_Slovenian", 1060),               // Slovenian
    ("SQL_Swedish_Pref_CP1", 1053),        // Swedish
    ("SQL_SwedishPhone_Pref_CP1", 1053),   // Swedish phonebook
    ("SQL_SwedishStd_Pref_CP1", 1053),     // Swedish standard
    ("SQL_Ukrainian", 1058),               // Ukrainian
    // Windows-style collations
    ("Albanian", 1052),
    ("Arabic", 1025),
    ("Assamese", 1101),
    ("Azeri_Cyrillic", 2092),
    ("Azeri_Latin", 1068),
    ("Bashkir", 1133),
    ("Bengali", 1093),
    ("Bosnian_Cyrillic", 8218),
    ("Bosnian_Latin", 5146),
    ("Breton", 1150),
    ("Bulgarian", 1026),
    ("Catalan", 1027),
    ("Chinese_Hong_Kong_Stroke", 3076),
    ("Chinese_Macao_Stroke", 5124),
    ("Chinese_PRC_Stroke", 2052),
    ("Chinese_PRC", 2052),
    ("Chinese_Simplified_Pinyin", 2052),
    ("Chinese_Simplified_Stroke_Order", 2052),
    ("Chinese_Taiwan_Bopomofo", 1028),
    ("Chinese_Taiwan_Stroke", 1028),
    ("Chinese_Traditional_Bopomofo", 1028),
    ("Chinese_Traditional_Pinyin", 1028),
    ("Chinese_Traditional_Stroke_Count", 1028),
    ("Chinese_Traditional_Stroke_Order", 1028),
    ("Corsican", 1155),
    ("Croatian", 1050),
    ("Cyrillic_General", 1049),
    ("Czech", 1029),
    ("Danish_Greenlandic", 1030),
    ("Danish_Norwegian", 1030),
    ("Danish", 1030),
    ("Dari", 1164),
    ("Divehi", 1125),
    ("Dutch", 1043),
    ("Estonian", 1061),
    ("Finnish_Swedish", 1035),
    ("Finnish", 1035),
    ("Frisian", 1122),
    ("French", 1036),
    ("Georgian_Modern_Sort", 1079),
    ("German_PhoneBook", 1031),
    ("German", 1031),
    ("Greek", 1032),
    ("Gujarati", 1095),
    ("Hausa", 1128),
    ("Hebrew", 1037),
    ("Hindi", 1081),
    ("Hungarian_Technical", 1038),
    ("Hungarian", 1038),
    ("Icelandic", 1039),
    ("Igbo", 1136),
    ("Indic_General", 1081),
    ("Indonesian", 1057),
    ("Inuktitut", 1117),
    ("Irish", 2108),
    ("Italian", 1040),
    ("Japanese_Bushu_Kakusu", 1041),
    ("Japanese_Radical_Stroke", 1041),
    ("Japanese_XJIS", 1041),
    ("Japanese", 1041),
    ("Kannada", 1099),
    ("Kazakh", 1087),
    ("Khmer", 1107),
    ("Korean_Wansung", 1042),
    ("Korean", 1042),
    ("Kyrgyz", 1088),
    ("Lao", 1108),
    ("Latin1_General_100", 1033),
    ("Latin1_General", 1033),
    ("Latvian", 1062),
    ("Lithuanian_Classic", 1063),
    ("Lithuanian", 1063),
    ("Luxembourgish", 1134),
    ("Macedonian_FYROM", 1071),
    ("Malay", 1086),
    ("Malayalam", 1100),
    ("Maltese", 1082),
    ("Maori", 1153),
    ("Mapudungan", 1146),
    ("Marathi", 1102),
    ("Mohawk", 1148),
    ("Mongolian", 1104),
    ("Nepali", 1121),
    ("Norwegian", 1044),
    ("Occitan", 1154),
    ("Oriya", 1096),
    ("Pashto", 1123),
    ("Persian", 1065),
    ("Polish", 1045),
    ("Portuguese", 1046),
    ("Punjabi", 1094),
    ("Quechua", 1131),
    ("Romanian", 1048),
    ("Romansh", 1047),
    ("Russian", 1049),
    ("Sami_Norway", 1083),
    ("Sami_Sweden_Finland", 2107),
    ("Sanskrit", 1103),
    ("Serbian_Cyrillic", 3098),
    ("Serbian_Latin", 2074),
    ("Sesotho_sa_Leboa", 1132),
    ("Setswana", 1074),
    ("Sinhala", 1115),
    ("Slovak", 1051),
    ("Slovenian", 1060),
    ("Spanish", 3082),
    ("Syriac", 1114),
    ("Tamazight", 2143),
    ("Tamil", 1097),
    ("Tatar", 1092),
    ("Telugu", 1098),
    ("Thai", 1054),
    ("Tibetan", 1105),
    ("Traditional_Spanish", 1034),
    ("Turkish", 1055),
    ("Turkmen", 1090),
    ("Uighur", 1152),
    ("Ukrainian", 1058),
    ("Upper_Sorbian", 1070),
    ("Urdu", 1056),
    ("Uzbek_Latin", 1091),
    ("Vietnamese", 1066),
    ("Welsh", 1106),
    ("Yakut", 1157),
    ("Yi", 1144),
    ("Yoruba", 1130),
];

/// Parse collation information from a SQL Server collation name
///
/// # Arguments
/// * `collation_name` - The full collation name (e.g., "Latin1_General_CI_AS", "Japanese_CS_AS_KS")
///
/// # Returns
/// CollationInfo with derived LCID and case sensitivity
///
/// # Examples
/// ```ignore
/// let info = parse_collation_info("Latin1_General_CI_AS");
/// assert_eq!(info.lcid, 1033);
/// assert_eq!(info.case_sensitive, false);
///
/// let info = parse_collation_info("Japanese_CS_AS");
/// assert_eq!(info.lcid, 1041);
/// assert_eq!(info.case_sensitive, true);
/// ```
pub fn parse_collation_info(collation_name: &str) -> CollationInfo {
    // Parse case sensitivity from the collation name
    // _CS_ = case-sensitive, _CI_ = case-insensitive
    let case_sensitive = parse_case_sensitivity(collation_name);

    // Parse LCID from collation prefix
    let lcid = parse_lcid(collation_name);

    CollationInfo {
        lcid,
        case_sensitive,
    }
}

/// Parse case sensitivity from collation name
///
/// SQL Server collation names contain sensitivity flags:
/// - _CI_ = Case Insensitive
/// - _CS_ = Case Sensitive
/// - _AI_ = Accent Insensitive
/// - _AS_ = Accent Sensitive
/// - _KS_ = Kana Sensitive (Japanese)
/// - _WS_ = Width Sensitive
/// - _VSS_ = Variation Selector Sensitive
/// - _BIN / _BIN2 = Binary (case-sensitive by nature)
///
/// Returns true if case-sensitive, false otherwise
fn parse_case_sensitivity(collation_name: &str) -> bool {
    let upper = collation_name.to_uppercase();

    // Binary collations are inherently case-sensitive
    if upper.contains("_BIN") {
        return true;
    }

    // Look for explicit case sensitivity markers
    // _CS_ means case-sensitive
    if upper.contains("_CS_") || upper.ends_with("_CS") {
        return true;
    }

    // _CI_ means case-insensitive (default assumption)
    // If neither _CS_ nor _CI_ is found, default to case-insensitive
    false
}

/// Parse LCID from collation name prefix
///
/// Extracts the locale-identifying prefix and looks it up in the LCID map.
/// Falls back to 1033 (US English) if no match is found.
fn parse_lcid(collation_name: &str) -> u32 {
    // Try to match the longest prefix first
    // The COLLATION_LCID_MAP is not sorted by length, so we iterate through all
    for (prefix, lcid) in COLLATION_LCID_MAP {
        if collation_name.starts_with(prefix) {
            return *lcid;
        }
    }

    // Default to US English if no match found
    1033
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_latin1_general_ci_as() {
        let info = parse_collation_info("Latin1_General_CI_AS");
        assert_eq!(info.lcid, 1033);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_latin1_general_cs_as() {
        let info = parse_collation_info("Latin1_General_CS_AS");
        assert_eq!(info.lcid, 1033);
        assert!(info.case_sensitive);
    }

    #[test]
    fn test_parse_sql_latin1_general_cp1_ci_as() {
        let info = parse_collation_info("SQL_Latin1_General_CP1_CI_AS");
        assert_eq!(info.lcid, 1033);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_sql_latin1_general_cp1_cs_as() {
        let info = parse_collation_info("SQL_Latin1_General_CP1_CS_AS");
        assert_eq!(info.lcid, 1033);
        assert!(info.case_sensitive);
    }

    #[test]
    fn test_parse_japanese_ci_as() {
        let info = parse_collation_info("Japanese_CI_AS");
        assert_eq!(info.lcid, 1041);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_japanese_cs_as() {
        let info = parse_collation_info("Japanese_CS_AS");
        assert_eq!(info.lcid, 1041);
        assert!(info.case_sensitive);
    }

    #[test]
    fn test_parse_turkish_ci_as() {
        let info = parse_collation_info("Turkish_CI_AS");
        assert_eq!(info.lcid, 1055);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_chinese_prc_ci_as() {
        let info = parse_collation_info("Chinese_PRC_CI_AS");
        assert_eq!(info.lcid, 2052);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_korean_wansung_ci_as() {
        let info = parse_collation_info("Korean_Wansung_CI_AS");
        assert_eq!(info.lcid, 1042);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_binary_collation() {
        let info = parse_collation_info("Latin1_General_BIN");
        assert_eq!(info.lcid, 1033);
        assert!(info.case_sensitive); // Binary is always case-sensitive
    }

    #[test]
    fn test_parse_binary2_collation() {
        let info = parse_collation_info("Latin1_General_BIN2");
        assert_eq!(info.lcid, 1033);
        assert!(info.case_sensitive); // Binary is always case-sensitive
    }

    #[test]
    fn test_parse_cyrillic_general_ci_as() {
        let info = parse_collation_info("Cyrillic_General_CI_AS");
        assert_eq!(info.lcid, 1049);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_german_phonebook_ci_as() {
        let info = parse_collation_info("German_PhoneBook_CI_AS");
        assert_eq!(info.lcid, 1031);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_french_ci_as() {
        let info = parse_collation_info("French_CI_AS");
        assert_eq!(info.lcid, 1036);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_spanish_ci_as() {
        let info = parse_collation_info("Spanish_CI_AS");
        assert_eq!(info.lcid, 3082);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_unknown_collation_defaults() {
        let info = parse_collation_info("Unknown_Collation_CI_AS");
        assert_eq!(info.lcid, 1033); // Default to US English
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_case_sensitivity_variants() {
        // Accent sensitivity should not affect case sensitivity
        assert!(!parse_collation_info("Latin1_General_CI_AI").case_sensitive);
        assert!(parse_collation_info("Latin1_General_CS_AI").case_sensitive);

        // Kana sensitivity (Japanese)
        assert!(!parse_collation_info("Japanese_CI_AS_KS").case_sensitive);
        assert!(parse_collation_info("Japanese_CS_AS_KS").case_sensitive);

        // Width sensitivity
        assert!(!parse_collation_info("Japanese_CI_AS_WS").case_sensitive);
        assert!(parse_collation_info("Japanese_CS_AS_WS").case_sensitive);
    }

    #[test]
    fn test_collation_info_default() {
        let info = CollationInfo::default();
        assert_eq!(info.lcid, 1033);
        assert!(!info.case_sensitive);
    }

    #[test]
    fn test_parse_latin1_general_100() {
        // Latin1_General_100 series (SQL Server 2008+)
        let info = parse_collation_info("Latin1_General_100_CI_AS");
        assert_eq!(info.lcid, 1033);
        assert!(!info.case_sensitive);

        let info = parse_collation_info("Latin1_General_100_CS_AS");
        assert_eq!(info.lcid, 1033);
        assert!(info.case_sensitive);
    }

    #[test]
    fn test_parse_sql_collation_with_codepage() {
        // SQL collations with specific code pages
        let info = parse_collation_info("SQL_Latin1_General_CP1250_CI_AS");
        assert_eq!(info.lcid, 1045); // Central European (Polish)
        assert!(!info.case_sensitive);

        let info = parse_collation_info("SQL_Latin1_General_CP1251_CI_AS");
        assert_eq!(info.lcid, 1049); // Cyrillic
        assert!(!info.case_sensitive);

        let info = parse_collation_info("SQL_Latin1_General_CP1253_CI_AS");
        assert_eq!(info.lcid, 1032); // Greek
        assert!(!info.case_sensitive);

        let info = parse_collation_info("SQL_Latin1_General_CP1254_CI_AS");
        assert_eq!(info.lcid, 1055); // Turkish
        assert!(!info.case_sensitive);
    }
}
