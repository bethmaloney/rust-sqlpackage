//! Additional element writers for model.xml generation.
//!
//! This module provides functions for writing index, fulltext, sequence,
//! and extended property elements to the model.xml output.

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use std::io::Write;

use crate::model::{
    DataCompressionType, ExtendedPropertyElement, FilegroupElement, FullTextCatalogElement,
    FullTextIndexElement, IndexElement, PartitionFunctionElement, PartitionSchemeElement,
    SequenceElement,
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

    // Write IsPadded property when PAD_INDEX = ON
    if index.is_padded {
        write_property(writer, "IsPadded", "True")?;
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

    for col in index.columns.iter() {
        writer.write_event(Event::Start(BytesStart::new("Entry")))?;

        // DotNet does NOT include Name attribute on SqlIndexedColumnSpecification elements
        let elem =
            BytesStart::new("Element").with_attributes([("Type", "SqlIndexedColumnSpecification")]);
        writer.write_event(Event::Start(elem))?;

        // Write IsAscending="False" property for descending columns
        // DotNet only emits this property when the column is descending (omit for ascending/default)
        if col.is_descending {
            write_property(writer, "IsAscending", "False")?;
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

/// Write a filegroup element to model.xml
///
/// Format:
/// ```xml
/// <Element Type="SqlFilegroup" Name="[USERDATA]">
///   <Property Name="IsMemoryOptimized" Value="True" />
/// </Element>
/// ```
pub(crate) fn write_filegroup<W: Write>(
    writer: &mut Writer<W>,
    filegroup: &FilegroupElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}]", filegroup.name);

    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlFilegroup"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Write IsMemoryOptimized property if this filegroup contains memory-optimized data
    if filegroup.contains_memory_optimized_data {
        write_property(writer, "IsMemoryOptimized", "True")?;
    }

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write a partition function element to model.xml
///
/// Format:
/// ```xml
/// <Element Type="SqlPartitionFunction" Name="[PF_TransactionDate]">
///   <Property Name="BoundaryIsRight" Value="True" />
///   <Relationship Name="BoundaryValues">
///     <Entry><Element Type="SqlPartitionValue"><Property Name="Expression"><Value><![CDATA['01/01/2014']]></Value></Property></Element></Entry>
///     ...
///   </Relationship>
///   <Relationship Name="ParameterType">
///     <Entry><References ExternalSource="BuiltIns" Name="[date]" /></Entry>
///   </Relationship>
/// </Element>
/// ```
pub(crate) fn write_partition_function<W: Write>(
    writer: &mut Writer<W>,
    partition_func: &PartitionFunctionElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}]", partition_func.name);

    let elem = BytesStart::new("Element").with_attributes([
        ("Type", "SqlPartitionFunction"),
        ("Name", full_name.as_str()),
    ]);
    writer.write_event(Event::Start(elem))?;

    // Write BoundaryIsRight property (RANGE RIGHT = true)
    if partition_func.is_range_right {
        write_property(writer, "BoundaryIsRight", "True")?;
    }

    // Write BoundaryValues relationship with partition value elements
    if !partition_func.boundary_values.is_empty() {
        writer.write_event(Event::Start(
            BytesStart::new("Relationship").with_attributes([("Name", "BoundaryValues")]),
        ))?;

        for value in &partition_func.boundary_values {
            writer.write_event(Event::Start(BytesStart::new("Entry")))?;

            let elem = BytesStart::new("Element").with_attributes([("Type", "SqlPartitionValue")]);
            writer.write_event(Event::Start(elem))?;

            // Write Expression property with the boundary value
            // For string values, wrap in quotes; for numbers, use as-is
            let expr_value = if value.parse::<i64>().is_ok() || value.parse::<f64>().is_ok() {
                value.clone()
            } else {
                format!("'{}'", value)
            };
            write_script_property(writer, "Expression", &expr_value)?;

            writer.write_event(Event::End(BytesEnd::new("Element")))?;
            writer.write_event(Event::End(BytesEnd::new("Entry")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("Relationship")))?;
    }

    // Write ParameterType relationship for the data type
    let type_name = format!("[{}]", partition_func.data_type.to_lowercase());
    write_type_specifier_builtin(writer, &type_name)?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

/// Write a partition scheme element to model.xml
///
/// Format:
/// ```xml
/// <Element Type="SqlPartitionScheme" Name="[PS_TransactionDate]">
///   <Relationship Name="FileGroups">
///     <Entry><References Name="[USERDATA]" /></Entry>
///     ...
///   </Relationship>
///   <Relationship Name="PartitionFunction">
///     <Entry><References Name="[PF_TransactionDate]" /></Entry>
///   </Relationship>
/// </Element>
/// ```
pub(crate) fn write_partition_scheme<W: Write>(
    writer: &mut Writer<W>,
    partition_scheme: &PartitionSchemeElement,
) -> anyhow::Result<()> {
    let full_name = format!("[{}]", partition_scheme.name);

    let elem = BytesStart::new("Element")
        .with_attributes([("Type", "SqlPartitionScheme"), ("Name", full_name.as_str())]);
    writer.write_event(Event::Start(elem))?;

    // Write FileGroups relationship
    if !partition_scheme.filegroups.is_empty() {
        let fg_refs: Vec<String> = partition_scheme
            .filegroups
            .iter()
            .map(|fg| format!("[{}]", fg))
            .collect();
        let fg_refs: Vec<&str> = fg_refs.iter().map(|s| s.as_str()).collect();
        write_relationship(writer, "FileGroups", &fg_refs)?;
    }

    // Write PartitionFunction relationship
    let pf_ref = format!("[{}]", partition_scheme.partition_function);
    write_relationship(writer, "PartitionFunction", &[&pf_ref])?;

    writer.write_event(Event::End(BytesEnd::new("Element")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FullTextColumnElement, IndexColumn, IndexElement};

    #[test]
    fn test_write_index_basic() {
        let index = IndexElement {
            name: "IX_Test".to_string(),
            table_schema: "dbo".to_string(),
            table_name: "TestTable".to_string(),
            columns: vec![
                IndexColumn::new("Col1".to_string()),
                IndexColumn::new("Col2".to_string()),
            ],
            is_unique: false,
            is_clustered: false,
            include_columns: vec![],
            filter_predicate: None,
            fill_factor: None,
            data_compression: None,
            is_padded: false,
        };

        let mut buffer = Vec::new();
        let mut writer = Writer::new(&mut buffer);
        write_index(&mut writer, &index).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        assert!(xml.contains(r#"Type="SqlIndex""#));
        assert!(xml.contains(r#"Name="[dbo].[TestTable].[IX_Test]""#));
        assert!(xml.contains(r#"Name="ColumnSpecifications""#));
        assert!(xml.contains(r#"Name="IndexedObject""#));
        // SqlIndexedColumnSpecification elements should NOT have Name attribute
        assert!(!xml.contains(r#"Type="SqlIndexedColumnSpecification" Name="#));
    }

    #[test]
    fn test_write_index_unique_clustered() {
        let index = IndexElement {
            name: "IX_Unique".to_string(),
            table_schema: "dbo".to_string(),
            table_name: "TestTable".to_string(),
            columns: vec![IndexColumn::new("Col1".to_string())],
            is_unique: true,
            is_clustered: true,
            include_columns: vec![],
            filter_predicate: None,
            fill_factor: None,
            data_compression: None,
            is_padded: false,
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
            columns: vec![IndexColumn::new("Col1".to_string())],
            is_unique: false,
            is_clustered: false,
            include_columns: vec!["Col2".to_string(), "Col3".to_string()],
            filter_predicate: None,
            fill_factor: None,
            data_compression: None,
            is_padded: false,
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
    fn test_write_index_with_descending_column() {
        let index = IndexElement {
            name: "IX_Desc".to_string(),
            table_schema: "dbo".to_string(),
            table_name: "TestTable".to_string(),
            columns: vec![
                IndexColumn::new("Col1".to_string()), // ASC (default)
                IndexColumn::with_direction("Col2".to_string(), true), // DESC
            ],
            is_unique: false,
            is_clustered: false,
            include_columns: vec![],
            filter_predicate: None,
            fill_factor: None,
            data_compression: None,
            is_padded: false,
        };

        let mut buffer = Vec::new();
        let mut writer = Writer::new(&mut buffer);
        write_index(&mut writer, &index).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        // Should have IsAscending="False" for the descending column
        assert!(xml.contains(r#"Name="IsAscending" Value="False""#));
        // The ascending column should NOT have IsAscending property (omitted for default)
        // Count occurrences of IsAscending - should be exactly 1
        assert_eq!(xml.matches("IsAscending").count(), 1);
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
