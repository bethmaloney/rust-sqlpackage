//! Additional element writers for model.xml generation.
//!
//! This module provides functions for writing index, fulltext, sequence,
//! and extended property elements to the model.xml output.

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

use crate::model::{
    DataCompressionType, ExtendedPropertyElement, FullTextCatalogElement, FullTextIndexElement,
    IndexElement, SequenceElement,
};

use super::body_deps::BodyDependency;
use super::xml_helpers::{
    write_property, write_relationship, write_schema_relationship, write_script_property,
    write_type_specifier_builtin,
};
use super::{extract_filter_predicate_columns, write_body_dependencies};

/// Write an index element to model.xml
pub(crate) fn write_index<W: Write>(
    writer: &mut Writer<W>,
    index: &IndexElement,
) -> anyhow::Result<()> {
    let full_name = format!(
        "[{}].[{}].[{}]",
        index.table_schema, index.table_name, index.name
    );

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlIndex"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    if index.is_unique {
        write_property(writer, "IsUnique", "True")?;
    }

    if index.is_clustered {
        write_property(writer, "IsClustered", "True")?;
    }

    if let Some(fill_factor) = index.fill_factor {
        write_property(writer, "FillFactor", &fill_factor.to_string())?;
    }

    // Write FilterPredicate property for filtered indexes (before relationships)
    // DotNet emits this as a CDATA script property
    if let Some(ref filter_predicate) = index.filter_predicate {
        write_script_property(writer, "FilterPredicate", filter_predicate)?;
    }

    // Reference to table
    let table_ref = format!("[{}].[{}]", index.table_schema, index.table_name);

    // Write BodyDependencies for filtered indexes (column references from filter predicate)
    // DotNet emits this before ColumnSpecifications
    if let Some(ref filter_predicate) = index.filter_predicate {
        let body_deps = extract_filter_predicate_columns(filter_predicate, &table_ref);
        if !body_deps.is_empty() {
            let body_deps: Vec<BodyDependency> = body_deps
                .into_iter()
                .map(BodyDependency::ObjectRef)
                .collect();
            write_body_dependencies(writer, &body_deps)?;
        }
    }

    // Write ColumnSpecifications for key columns
    if !index.columns.is_empty() {
        write_index_column_specifications(writer, index, &table_ref)?;
    }

    // Write DataCompressionOptions relationship if index has compression
    if let Some(ref compression) = index.data_compression {
        write_data_compression_options(writer, compression)?;
    }

    // Write IncludedColumns relationship if present
    if !index.include_columns.is_empty() {
        let include_refs: Vec<String> = index
            .include_columns
            .iter()
            .map(|col| format!("{}.[{}]", table_ref, col))
            .collect();
        let include_refs: Vec<&str> = include_refs.iter().map(|s| s.as_str()).collect();
        write_relationship(writer, "IncludedColumns", &include_refs)?;
    }

    // IndexedObject relationship comes after ColumnSpecifications and IncludedColumns
    write_relationship(writer, "IndexedObject", &[&table_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write index column specifications relationship
fn write_index_column_specifications<W: Write>(
    writer: &mut Writer<W>,
    index: &IndexElement,
    table_ref: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "ColumnSpecifications")]);
    writer.write_event(Event::Start(rel))?;

    for (i, col) in index.columns.iter().enumerate() {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        let spec_name = format!(
            "[{}].[{}].[{}].[{}]",
            index.table_schema, index.table_name, index.name, i
        );

        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let elem = BytesStart::new("Element").with_attributes([
            ("Type", "SqlIndexedColumnSpecification"),
            ("Name", spec_name.as_str()),
        ]);
        writer.write_event(Event::Start(elem))?;

        // Reference to the column
        let col_ref = format!("{}.[{}]", table_ref, col);
        write_relationship(writer, "Column", &[&col_ref])?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write DataCompressionOptions relationship for indexes with data compression
fn write_data_compression_options<W: Write>(
    writer: &mut Writer<W>,
    compression: &DataCompressionType,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "DataCompressionOptions")]);
    writer.write_event(Event::Start(rel))?;

    writer.write_event(Event::Start(BytesStart::new("Entry")))?;

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([("Type", "SqlDataCompressionOption")]);
    writer.write_event(Event::Start(elem))?;

    // Write CompressionLevel property
    write_property(
        writer,
        "CompressionLevel",
        &compression.compression_level().to_string(),
    )?;

    // Write PartitionNumber property (always 1 for single-partition indexes)
    write_property(writer, "PartitionNumber", "1")?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write a fulltext index element to model.xml
pub(crate) fn write_fulltext_index<W: Write>(
    writer: &mut Writer<W>,
    fulltext: &FullTextIndexElement,
) -> anyhow::Result<()> {
    // Full-text index name format: [schema].[table] (same as table name)
    let full_name = format!("[{}].[{}]", fulltext.table_schema, fulltext.table_name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    // Conditional Disambiguator attribute requires separate handling
    let elem = if let Some(disambiguator) = fulltext.disambiguator {
        let disamb_str = disambiguator.to_string();
        BytesStart::new("Element").with_attributes([
            ("Type", "SqlFullTextIndex"),
            ("Name", full_name.as_str()),
            ("Disambiguator", disamb_str.as_str()),
        ])
    } else {
        BytesStart::new("Element")
            .with_attributes([("Type", "SqlFullTextIndex"), ("Name", full_name.as_str())])
    };
    writer.write_event(Event::Start(elem))?;

    // Reference to full-text catalog if specified
    if let Some(catalog) = &fulltext.catalog {
        let catalog_ref = format!("[{}]", catalog);
        write_relationship(writer, "Catalog", &[&catalog_ref])?;
    }

    // Write Columns for full-text columns
    let table_ref = format!("[{}].[{}]", fulltext.table_schema, fulltext.table_name);
    if !fulltext.columns.is_empty() {
        write_fulltext_column_specifications(writer, fulltext, &table_ref)?;
    }

    // Reference to table (IndexedObject)
    write_relationship(writer, "IndexedObject", &[&table_ref])?;

    // Reference to the unique key index (KeyName)
    // Key reference format: [schema].[constraint_name]
    let key_index_ref = format!("[{}].[{}]", fulltext.table_schema, fulltext.key_index);
    write_relationship(writer, "KeyName", &[&key_index_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write fulltext column specifications relationship
fn write_fulltext_column_specifications<W: Write>(
    writer: &mut Writer<W>,
    fulltext: &FullTextIndexElement,
    table_ref: &str,
) -> anyhow::Result<()> {
    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let rel = BytesStart::new("Relationship").with_attributes([("Name", "Columns")]);
    writer.write_event(Event::Start(rel))?;

    for col in fulltext.columns.iter() {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // DotNet uses anonymous elements (no Name attribute) for column specifiers
        // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
        let elem = BytesStart::new("Element")
            .with_attributes([("Type", "SqlFullTextIndexColumnSpecifier")]);
        writer.write_event(Event::Start(elem))?;

        // Add LanguageId property if specified
        if let Some(lang_id) = col.language_id {
            write_property(writer, "LanguageId", &lang_id.to_string())?;
        }

        // Reference to the column
        let col_ref = format!("{}.[{}]", table_ref, col.name);
        write_relationship(writer, "Column", &[&col_ref])?;

        writer.write_event(Event::End(BytesEnd::new("Element")))?;
        writer.write_event(Event::End(BytesEnd::new("Entry")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    Ok(())
}

/// Write a fulltext catalog element to model.xml
pub(crate) fn write_fulltext_catalog<W: Write>(
    writer: &mut Writer<W>,
    catalog: &FullTextCatalogElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}]", catalog.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlFullTextCatalog"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Add IsDefault property if this is the default catalog
    if catalog.is_default {
        write_property(writer, "IsDefault", "True")?;
    }

    // Fulltext catalogs have an Authorizer relationship (defaults to dbo)
    super::write_authorizer_relationship(writer, "dbo")?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write a sequence element to model.xml
pub(crate) fn write_sequence<W: Write>(
    writer: &mut Writer<W>,
    seq: &SequenceElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}].[{}]", seq.schema, seq.name);

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlSequence"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Properties in DotNet order: IsCycling, HasNoMaxValue, HasNoMinValue, MinValue, MaxValue, Increment, StartValue
    if seq.is_cycling {
        write_property(writer, "IsCycling", "True")?;
    }

    // HasNoMaxValue and HasNoMinValue
    let has_no_max = seq.has_no_max_value || seq.max_value.is_none();
    let has_no_min = seq.has_no_min_value || seq.min_value.is_none();
    write_property(
        writer,
        "HasNoMaxValue",
        if has_no_max { "True" } else { "False" },
    )?;
    write_property(
        writer,
        "HasNoMinValue",
        if has_no_min { "True" } else { "False" },
    )?;

    // MinValue and MaxValue
    if let Some(min) = seq.min_value {
        write_property(writer, "MinValue", &min.to_string())?;
    }
    if let Some(max) = seq.max_value {
        write_property(writer, "MaxValue", &max.to_string())?;
    }

    // Increment
    if let Some(inc) = seq.increment_value {
        write_property(writer, "Increment", &inc.to_string())?;
    }

    // StartValue
    if let Some(start) = seq.start_value {
        write_property(writer, "StartValue", &start.to_string())?;
    }

    // CacheSize - write if explicitly specified (not None which means default)
    if let Some(cache) = seq.cache_size {
        write_property(writer, "CacheSize", &cache.to_string())?;
    }

    // Relationship to schema
    write_schema_relationship(writer, &seq.schema)?;

    // TypeSpecifier relationship for data type
    if let Some(ref data_type) = seq.data_type {
        let type_name = format!("[{}]", data_type.to_lowercase());
        write_type_specifier_builtin(writer, &type_name)?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write an extended property element to model.xml
///
/// Format:
/// ```xml
/// <Element Type="SqlExtendedProperty" Name="[dbo].[Table].[MS_Description]">
///   <Property Name="Value">
///     <Value><![CDATA[Description text]]></Value>
///   </Property>
///   <Relationship Name="Host">
///     <Entry>
///       <References Name="[dbo].[Table]"/>
///     </Entry>
///   </Relationship>
/// </Element>
/// ```
pub(crate) fn write_extended_property<W: Write>(
    writer: &mut Writer<W>,
    ext_prop: &ExtendedPropertyElement,
) -> anyhow::Result<()> {
    let full_name = ext_prop.full_name();

    // Use with_attributes for batched attribute setting (Phase 16.3.3 optimization)
    let elem = BytesStart::new("Element").with_attributes([
        ("Type", "SqlExtendedProperty"),
        ("Name", full_name.as_str()),
    ]);
    writer.write_event(Event::Start(elem))?;

    // Write Value property with CDATA (SqlScriptProperty format)
    // The value must be wrapped with N'...' for proper SQL string literal escaping
    // Any single quotes in the value must be doubled for SQL escaping
    let escaped_value = ext_prop.property_value.replace('\'', "''");
    let quoted_value = format!("N'{}'", escaped_value);
    write_script_property(writer, "Value", &quoted_value)?;

    // Write Host relationship pointing to the target object (table or column)
    let extends_ref = ext_prop.extends_object_ref();
    write_relationship(writer, "Host", &[&extends_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FullTextColumnElement, IndexElement};

    #[test]
    fn test_write_index_basic() {
        let index = IndexElement {
            name: "IX_Test".to_string(),
            table_schema: "dbo".to_string(),
            table_name: "TestTable".to_string(),
            columns: vec!["Col1".to_string(), "Col2".to_string()],
            is_unique: false,
            is_clustered: false,
            include_columns: vec![],
            filter_predicate: None,
            fill_factor: None,
            data_compression: None,
        };

        let mut buffer = Vec::new();
        let mut writer = Writer::new(&mut buffer);
        write_index(&mut writer, &index).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        assert!(xml.contains(r#"Type="SqlIndex""#));
        assert!(xml.contains(r#"Name="[dbo].[TestTable].[IX_Test]""#));
        assert!(xml.contains(r#"Name="ColumnSpecifications""#));
        assert!(xml.contains(r#"Name="IndexedObject""#));
    }

    #[test]
    fn test_write_index_unique_clustered() {
        let index = IndexElement {
            name: "IX_Unique".to_string(),
            table_schema: "dbo".to_string(),
            table_name: "TestTable".to_string(),
            columns: vec!["Col1".to_string()],
            is_unique: true,
            is_clustered: true,
            include_columns: vec![],
            filter_predicate: None,
            fill_factor: None,
            data_compression: None,
        };

        let mut buffer = Vec::new();
        let mut writer = Writer::new(&mut buffer);
        write_index(&mut writer, &index).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        assert!(xml.contains(r#"Name="IsUnique""#));
        assert!(xml.contains(r#"Value="True""#));
        assert!(xml.contains(r#"Name="IsClustered""#));
    }

    #[test]
    fn test_write_index_with_include_columns() {
        let index = IndexElement {
            name: "IX_Include".to_string(),
            table_schema: "dbo".to_string(),
            table_name: "TestTable".to_string(),
            columns: vec!["Col1".to_string()],
            is_unique: false,
            is_clustered: false,
            include_columns: vec!["Col2".to_string(), "Col3".to_string()],
            filter_predicate: None,
            fill_factor: None,
            data_compression: None,
        };

        let mut buffer = Vec::new();
        let mut writer = Writer::new(&mut buffer);
        write_index(&mut writer, &index).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        assert!(xml.contains(r#"Name="IncludedColumns""#));
        assert!(xml.contains(r#"[dbo].[TestTable].[Col2]"#));
        assert!(xml.contains(r#"[dbo].[TestTable].[Col3]"#));
    }

    #[test]
    fn test_write_fulltext_catalog() {
        let catalog = FullTextCatalogElement {
            name: "TestCatalog".to_string(),
            is_default: true,
        };

        let mut buffer = Vec::new();
        let mut writer = Writer::new(&mut buffer);
        write_fulltext_catalog(&mut writer, &catalog).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        assert!(xml.contains(r#"Type="SqlFullTextCatalog""#));
        assert!(xml.contains(r#"Name="[TestCatalog]""#));
        assert!(xml.contains(r#"Name="IsDefault""#));
    }

    #[test]
    fn test_write_fulltext_index() {
        let fulltext = FullTextIndexElement {
            table_schema: "dbo".to_string(),
            table_name: "TestTable".to_string(),
            columns: vec![FullTextColumnElement {
                name: "Content".to_string(),
                language_id: Some(1033),
            }],
            key_index: "PK_TestTable".to_string(),
            catalog: Some("TestCatalog".to_string()),
            change_tracking: None,
            disambiguator: None,
        };

        let mut buffer = Vec::new();
        let mut writer = Writer::new(&mut buffer);
        write_fulltext_index(&mut writer, &fulltext).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        assert!(xml.contains(r#"Type="SqlFullTextIndex""#));
        assert!(xml.contains(r#"Name="[dbo].[TestTable]""#));
        assert!(xml.contains(r#"Name="Catalog""#));
        assert!(xml.contains(r#"Name="IndexedObject""#));
        assert!(xml.contains(r#"Name="KeyName""#));
    }

    #[test]
    fn test_write_sequence() {
        let seq = SequenceElement {
            schema: "dbo".to_string(),
            name: "TestSeq".to_string(),
            definition: "".to_string(),
            data_type: Some("BIGINT".to_string()),
            start_value: Some(1),
            increment_value: Some(1),
            min_value: None,
            max_value: None,
            is_cycling: false,
            has_no_min_value: true,
            has_no_max_value: true,
            cache_size: None,
        };

        let mut buffer = Vec::new();
        let mut writer = Writer::new(&mut buffer);
        write_sequence(&mut writer, &seq).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        assert!(xml.contains(r#"Type="SqlSequence""#));
        assert!(xml.contains(r#"Name="[dbo].[TestSeq]""#));
        assert!(xml.contains(r#"Name="HasNoMaxValue""#));
        assert!(xml.contains(r#"Name="HasNoMinValue""#));
    }

    #[test]
    fn test_write_extended_property() {
        let ext_prop = ExtendedPropertyElement {
            target_schema: "dbo".to_string(),
            target_object: "TestTable".to_string(),
            target_column: None,
            property_name: "MS_Description".to_string(),
            property_value: "Test description".to_string(),
            level1type: Some("TABLE".to_string()),
            level2type: None,
        };

        let mut buffer = Vec::new();
        let mut writer = Writer::new(&mut buffer);
        write_extended_property(&mut writer, &ext_prop).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        assert!(xml.contains(r#"Type="SqlExtendedProperty""#));
        // Format: [SqlTableBase].[schema].[table].[property_name] (SqlTableBase is default for TABLE type)
        assert!(xml.contains(r#"Name="[SqlTableBase].[dbo].[TestTable].[MS_Description]""#));
        assert!(xml.contains(r#"Name="Value""#));
        assert!(xml.contains(r#"Name="Host""#));
    }
}
