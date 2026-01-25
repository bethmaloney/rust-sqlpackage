//! Table Building Tests

use super::parse_and_build_model;

// ============================================================================
// Table Building Tests
// ============================================================================

#[test]
fn test_build_table_element() {
    let sql = "CREATE TABLE [dbo].[Users] ([Id] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");
    let table = table.unwrap();
    assert!(table.name.contains("Users"), "Table name should be Users");
}

#[test]
fn test_build_table_with_columns() {
    let sql = r#"
CREATE TABLE [dbo].[Users] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NULL,
    [Email] VARCHAR(255) NOT NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();
    assert_eq!(table.columns.len(), 3, "Table should have 3 columns");
}

#[test]
fn test_build_column_types_int() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(
        col.data_type.to_uppercase().contains("INT"),
        "Column type should be INT, got: {}",
        col.data_type
    );
}

#[test]
fn test_build_column_types_varchar() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] VARCHAR(100) NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(
        col.data_type.to_uppercase().contains("VARCHAR"),
        "Column type should contain VARCHAR, got: {}",
        col.data_type
    );
}

#[test]
fn test_build_column_types_decimal() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] DECIMAL(18, 2) NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(
        col.data_type.to_uppercase().contains("DECIMAL"),
        "Column type should contain DECIMAL, got: {}",
        col.data_type
    );
}

#[test]
fn test_build_column_types_datetime() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] DATETIME2 NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(
        col.data_type.to_uppercase().contains("DATETIME"),
        "Column type should contain DATETIME, got: {}",
        col.data_type
    );
}

#[test]
fn test_build_column_nullable() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] INT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(col.is_nullable, "Column should be nullable");
}

#[test]
fn test_build_column_not_nullable() {
    let sql = "CREATE TABLE [dbo].[T] ([Col] INT NOT NULL);";
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let col = &table.unwrap().columns[0];
    assert!(!col.is_nullable, "Column should not be nullable");
}

// ============================================================================
// ROWGUIDCOL Model Tests
// ============================================================================

#[test]
fn test_build_column_rowguidcol() {
    // Test that ROWGUIDCOL column is detected via fallback parser
    let sql = r#"
CREATE TABLE [dbo].[TableWithRowGuid] (
    [Id] INT NOT NULL,
    [RowGuid] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");
    let table = table.unwrap();
    assert_eq!(table.columns.len(), 2, "Table should have 2 columns");

    // Find the RowGuid column
    let rowguid_col = table.columns.iter().find(|c| c.name == "RowGuid");
    assert!(rowguid_col.is_some(), "Should have RowGuid column");
    let rowguid_col = rowguid_col.unwrap();
    assert!(
        rowguid_col.is_rowguidcol,
        "Column should be marked as ROWGUIDCOL"
    );
    assert!(
        !rowguid_col.is_nullable,
        "ROWGUIDCOL column should be NOT NULL"
    );

    // Non-ROWGUIDCOL columns should not have the flag
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some(), "Should have Id column");
    assert!(
        !id_col.unwrap().is_rowguidcol,
        "Id column should not be ROWGUIDCOL"
    );
}

#[test]
fn test_build_column_rowguidcol_with_default() {
    // ROWGUIDCOL with DEFAULT NEWID()
    let sql = r#"
CREATE TABLE [dbo].[Entities] (
    [Id] INT NOT NULL,
    [EntityGuid] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL DEFAULT NEWID()
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();

    let guid_col = table.columns.iter().find(|c| c.name == "EntityGuid");
    assert!(guid_col.is_some(), "Should have EntityGuid column");
    let guid_col = guid_col.unwrap();
    assert!(
        guid_col.is_rowguidcol,
        "Column should be marked as ROWGUIDCOL"
    );
}

#[test]
fn test_build_column_no_rowguidcol() {
    // Regular UNIQUEIDENTIFIER without ROWGUIDCOL
    let sql = r#"
CREATE TABLE [dbo].[RegularGuid] (
    [Id] INT NOT NULL,
    [SomeGuid] UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID()
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();

    // Neither column should be ROWGUIDCOL
    for col in &table.columns {
        assert!(
            !col.is_rowguidcol,
            "Column {} should not be ROWGUIDCOL",
            col.name
        );
    }
}

// ============================================================================
// SPARSE Column Model Tests
// ============================================================================

#[test]
fn test_build_column_sparse() {
    // Test that SPARSE column is detected via fallback parser
    let sql = r#"
CREATE TABLE [dbo].[TableWithSparse] (
    [Id] INT NOT NULL,
    [OptionalData] NVARCHAR(100) SPARSE NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");
    let table = table.unwrap();
    assert_eq!(table.columns.len(), 2, "Table should have 2 columns");

    // Find the OptionalData column
    let sparse_col = table.columns.iter().find(|c| c.name == "OptionalData");
    assert!(sparse_col.is_some(), "Should have OptionalData column");
    let sparse_col = sparse_col.unwrap();
    assert!(sparse_col.is_sparse, "Column should be marked as SPARSE");
    assert!(sparse_col.is_nullable, "SPARSE column should be NULL");

    // Non-SPARSE columns should not have the flag
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some(), "Should have Id column");
    assert!(!id_col.unwrap().is_sparse, "Id column should not be SPARSE");
}

#[test]
fn test_build_column_sparse_with_multiple() {
    // Multiple SPARSE columns in a table
    let sql = r#"
CREATE TABLE [dbo].[WideTable] (
    [Id] INT NOT NULL,
    [RequiredField] NVARCHAR(50) NOT NULL,
    [Attribute1] NVARCHAR(100) SPARSE NULL,
    [Attribute2] INT SPARSE NULL,
    [Attribute3] DATETIME2 SPARSE NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();
    assert_eq!(table.columns.len(), 5, "Table should have 5 columns");

    // Check non-sparse columns
    let id_col = table.columns.iter().find(|c| c.name == "Id").unwrap();
    assert!(!id_col.is_sparse, "Id should not be SPARSE");

    let required_col = table
        .columns
        .iter()
        .find(|c| c.name == "RequiredField")
        .unwrap();
    assert!(
        !required_col.is_sparse,
        "RequiredField should not be SPARSE"
    );

    // Check sparse columns
    for attr_name in &["Attribute1", "Attribute2", "Attribute3"] {
        let col = table.columns.iter().find(|c| &c.name == attr_name);
        assert!(col.is_some(), "Should have {} column", attr_name);
        let col = col.unwrap();
        assert!(col.is_sparse, "{} should be SPARSE", attr_name);
        assert!(col.is_nullable, "{} should be NULL", attr_name);
    }
}

#[test]
fn test_build_column_no_sparse() {
    // Regular columns without SPARSE
    let sql = r#"
CREATE TABLE [dbo].[RegularTable] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();

    // Neither column should be SPARSE
    for col in &table.columns {
        assert!(!col.is_sparse, "Column {} should not be SPARSE", col.name);
    }
}

#[test]
fn test_build_column_sparse_with_column_set() {
    // SPARSE columns with XML COLUMN_SET (wide table pattern)
    let sql = r#"
CREATE TABLE [dbo].[DocumentStore] (
    [Id] INT NOT NULL,
    [DocType] NVARCHAR(50) NOT NULL,
    [Attr1] NVARCHAR(100) SPARSE NULL,
    [Attr2] NVARCHAR(100) SPARSE NULL,
    [Attr3] INT SPARSE NULL,
    [SparseColumns] XML COLUMN_SET FOR ALL_SPARSE_COLUMNS
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");
    let table = table.unwrap();

    // Check SPARSE columns are marked correctly
    let attr1 = table.columns.iter().find(|c| c.name == "Attr1");
    assert!(attr1.is_some(), "Should have Attr1 column");
    assert!(attr1.unwrap().is_sparse, "Attr1 should be SPARSE");

    let attr2 = table.columns.iter().find(|c| c.name == "Attr2");
    assert!(attr2.is_some(), "Should have Attr2 column");
    assert!(attr2.unwrap().is_sparse, "Attr2 should be SPARSE");

    let attr3 = table.columns.iter().find(|c| c.name == "Attr3");
    assert!(attr3.is_some(), "Should have Attr3 column");
    assert!(attr3.unwrap().is_sparse, "Attr3 should be SPARSE");

    // Non-SPARSE regular columns
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some(), "Should have Id column");
    assert!(!id_col.unwrap().is_sparse, "Id should not be SPARSE");
}

// ============================================================================
// FILESTREAM Column Model Tests
// ============================================================================

#[test]
fn test_build_column_filestream() {
    // Test that FILESTREAM column is detected via fallback parser
    let sql = r#"
CREATE TABLE [dbo].[Documents] (
    [Id] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL,
    [FileData] VARBINARY(MAX) FILESTREAM NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some(), "Model should contain a table");
    let table = table.unwrap();
    assert_eq!(table.columns.len(), 2, "Table should have 2 columns");

    // Find the FileData column
    let filestream_col = table.columns.iter().find(|c| c.name == "FileData");
    assert!(filestream_col.is_some(), "Should have FileData column");
    let filestream_col = filestream_col.unwrap();
    assert!(
        filestream_col.is_filestream,
        "Column should be marked as FILESTREAM"
    );
    assert!(
        filestream_col.is_nullable,
        "FILESTREAM column should be NULL"
    );

    // Non-FILESTREAM columns should not have the flag
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some(), "Should have Id column");
    assert!(
        !id_col.unwrap().is_filestream,
        "Id column should not be FILESTREAM"
    );
}

#[test]
fn test_build_column_filestream_not_null() {
    // FILESTREAM column with NOT NULL
    let sql = r#"
CREATE TABLE [dbo].[RequiredFiles] (
    [Id] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL,
    [Content] VARBINARY(MAX) FILESTREAM NOT NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();

    let content_col = table.columns.iter().find(|c| c.name == "Content");
    assert!(content_col.is_some(), "Should have Content column");
    let content_col = content_col.unwrap();
    assert!(
        content_col.is_filestream,
        "Column should be marked as FILESTREAM"
    );
    assert!(
        !content_col.is_nullable,
        "FILESTREAM column should be NOT NULL"
    );
}

#[test]
fn test_build_column_filestream_with_rowguidcol() {
    // FILESTREAM table typically requires a ROWGUIDCOL column
    let sql = r#"
CREATE TABLE [dbo].[FileArchive] (
    [FileId] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL,
    [Data] VARBINARY(MAX) FILESTREAM NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();

    // Check ROWGUIDCOL column
    let guid_col = table.columns.iter().find(|c| c.name == "FileId");
    assert!(guid_col.is_some(), "Should have FileId column");
    let guid_col = guid_col.unwrap();
    assert!(guid_col.is_rowguidcol, "FileId should be ROWGUIDCOL");
    assert!(!guid_col.is_filestream, "FileId should not be FILESTREAM");

    // Check FILESTREAM column
    let data_col = table.columns.iter().find(|c| c.name == "Data");
    assert!(data_col.is_some(), "Should have Data column");
    let data_col = data_col.unwrap();
    assert!(data_col.is_filestream, "Data should be FILESTREAM");
    assert!(!data_col.is_rowguidcol, "Data should not be ROWGUIDCOL");
}

#[test]
fn test_build_column_no_filestream() {
    // Regular VARBINARY(MAX) without FILESTREAM
    let sql = r#"
CREATE TABLE [dbo].[RegularBinary] (
    [Id] INT NOT NULL,
    [BinaryData] VARBINARY(MAX) NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();

    // Neither column should be FILESTREAM
    for col in &table.columns {
        assert!(
            !col.is_filestream,
            "Column {} should not be FILESTREAM",
            col.name
        );
    }
}

#[test]
fn test_build_column_multiple_filestream() {
    // Multiple FILESTREAM columns in one table
    let sql = r#"
CREATE TABLE [dbo].[MediaFiles] (
    [Id] UNIQUEIDENTIFIER NOT NULL ROWGUIDCOL,
    [Thumbnail] VARBINARY(MAX) FILESTREAM NULL,
    [FullSize] VARBINARY(MAX) FILESTREAM NULL,
    [Name] NVARCHAR(100) NOT NULL
);
"#;
    let model = parse_and_build_model(sql);

    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();
    assert_eq!(table.columns.len(), 4, "Table should have 4 columns");

    // Check FILESTREAM columns
    let thumbnail = table.columns.iter().find(|c| c.name == "Thumbnail");
    assert!(thumbnail.is_some(), "Should have Thumbnail column");
    assert!(
        thumbnail.unwrap().is_filestream,
        "Thumbnail should be FILESTREAM"
    );

    let fullsize = table.columns.iter().find(|c| c.name == "FullSize");
    assert!(fullsize.is_some(), "Should have FullSize column");
    assert!(
        fullsize.unwrap().is_filestream,
        "FullSize should be FILESTREAM"
    );

    // Non-FILESTREAM columns
    let id_col = table.columns.iter().find(|c| c.name == "Id");
    assert!(id_col.is_some(), "Should have Id column");
    assert!(
        !id_col.unwrap().is_filestream,
        "Id should not be FILESTREAM"
    );

    let name_col = table.columns.iter().find(|c| c.name == "Name");
    assert!(name_col.is_some(), "Should have Name column");
    assert!(
        !name_col.unwrap().is_filestream,
        "Name should not be FILESTREAM"
    );
}
