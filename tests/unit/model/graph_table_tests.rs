//! Graph Table Model Tests (NODE and EDGE tables)
//!
//! Tests that graph tables are correctly identified and modeled:
//! - is_node flag for NODE tables
//! - is_edge flag for EDGE tables
//! - Regular tables should have both flags as false

use super::parse_and_build_model;

// ============================================================================
// NODE Table Model Tests
// ============================================================================

#[test]
fn test_build_node_table() {
    let sql = r#"
CREATE TABLE [dbo].[Person] (
    [PersonId] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL
) AS NODE;
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
    assert!(table.name.contains("Person"), "Table name should be Person");
    assert!(table.is_node, "Table should be marked as NODE");
    assert!(!table.is_edge, "Table should not be marked as EDGE");
}

#[test]
fn test_build_node_table_with_columns() {
    let sql = r#"
CREATE TABLE [dbo].[Employee] (
    [EmployeeId] INT NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [Department] NVARCHAR(100) NULL,
    [Salary] DECIMAL(18, 2) NOT NULL
) AS NODE;
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
    assert!(table.is_node, "Table should be marked as NODE");
    assert_eq!(table.columns.len(), 4, "Table should have 4 columns");

    // Verify column details
    let emp_id_col = table.columns.iter().find(|c| c.name == "EmployeeId");
    assert!(emp_id_col.is_some(), "Should have EmployeeId column");
    assert_eq!(
        emp_id_col.unwrap().nullability,
        Some(false),
        "EmployeeId should be explicitly NOT NULL"
    );
}

#[test]
fn test_build_node_table_schema() {
    let sql = r#"
CREATE TABLE [Sales].[Customer] (
    [CustomerId] INT NOT NULL,
    [CustomerName] NVARCHAR(200) NOT NULL
) AS NODE;
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
    assert_eq!(table.schema, "Sales", "Schema should be Sales");
    assert_eq!(table.name, "Customer", "Table name should be Customer");
    assert!(table.is_node, "Table should be marked as NODE");
}

// ============================================================================
// EDGE Table Model Tests
// ============================================================================

#[test]
fn test_build_edge_table() {
    let sql = r#"
CREATE TABLE [dbo].[Knows] (
    [Since] DATE NULL
) AS EDGE;
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
    assert!(table.name.contains("Knows"), "Table name should be Knows");
    assert!(!table.is_node, "Table should not be marked as NODE");
    assert!(table.is_edge, "Table should be marked as EDGE");
}

#[test]
fn test_build_edge_table_with_properties() {
    let sql = r#"
CREATE TABLE [dbo].[WorksFor] (
    [StartDate] DATE NOT NULL,
    [EndDate] DATE NULL,
    [Role] NVARCHAR(100) NULL,
    [Salary] DECIMAL(18, 2) NOT NULL
) AS EDGE;
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
    assert!(table.is_edge, "Table should be marked as EDGE");
    assert_eq!(table.columns.len(), 4, "Table should have 4 columns");

    // Check nullable properties
    let start_date = table.columns.iter().find(|c| c.name == "StartDate");
    assert!(start_date.is_some());
    assert_eq!(
        start_date.unwrap().nullability,
        Some(false),
        "StartDate should be explicitly NOT NULL"
    );

    let end_date = table.columns.iter().find(|c| c.name == "EndDate");
    assert!(end_date.is_some());
    assert_eq!(
        end_date.unwrap().nullability,
        Some(true),
        "EndDate should be explicitly NULL"
    );
}

// ============================================================================
// Regular Table Tests (ensure is_node and is_edge are false)
// ============================================================================

#[test]
fn test_build_regular_table_not_graph() {
    let sql = r#"
CREATE TABLE [dbo].[Orders] (
    [OrderId] INT NOT NULL,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATE NOT NULL
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
    assert!(!table.is_node, "Regular table should not be marked as NODE");
    assert!(!table.is_edge, "Regular table should not be marked as EDGE");
}

#[test]
fn test_build_table_with_node_in_name() {
    // Table name contains "NODE" but is not a graph table
    let sql = r#"
CREATE TABLE [dbo].[NodeConfiguration] (
    [Id] INT NOT NULL,
    [NodeName] NVARCHAR(100) NOT NULL
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
    assert!(
        !table.is_node,
        "Table with NODE in name should not be marked as NODE without AS NODE syntax"
    );
    assert!(
        !table.is_edge,
        "Table with NODE in name should not be marked as EDGE"
    );
}

// ============================================================================
// Mixed Graph and Regular Tables Tests
// ============================================================================

#[test]
fn test_build_mixed_tables() {
    let sql = r#"
CREATE TABLE [dbo].[Person] (
    [PersonId] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL
) AS NODE;
GO
CREATE TABLE [dbo].[FriendOf] (
    [Since] DATE NULL
) AS EDGE;
GO
CREATE TABLE [dbo].[AuditLog] (
    [LogId] INT NOT NULL,
    [Action] NVARCHAR(50) NOT NULL
);
"#;
    let model = parse_and_build_model(sql);

    // Find all tables
    let tables: Vec<_> = model
        .elements
        .iter()
        .filter_map(|e| {
            if let rust_sqlpackage::model::ModelElement::Table(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(tables.len(), 3, "Should have 3 tables");

    // Find Person (NODE)
    let person = tables.iter().find(|t| t.name == "Person");
    assert!(person.is_some(), "Should have Person table");
    assert!(person.unwrap().is_node, "Person should be NODE");
    assert!(!person.unwrap().is_edge, "Person should not be EDGE");

    // Find FriendOf (EDGE)
    let friend_of = tables.iter().find(|t| t.name == "FriendOf");
    assert!(friend_of.is_some(), "Should have FriendOf table");
    assert!(!friend_of.unwrap().is_node, "FriendOf should not be NODE");
    assert!(friend_of.unwrap().is_edge, "FriendOf should be EDGE");

    // Find AuditLog (regular)
    let audit_log = tables.iter().find(|t| t.name == "AuditLog");
    assert!(audit_log.is_some(), "Should have AuditLog table");
    assert!(!audit_log.unwrap().is_node, "AuditLog should not be NODE");
    assert!(!audit_log.unwrap().is_edge, "AuditLog should not be EDGE");
}

// ============================================================================
// Graph Table with Constraints Tests
// ============================================================================

#[test]
fn test_build_node_table_with_primary_key() {
    let sql = r#"
CREATE TABLE [dbo].[Product] (
    [ProductId] INT NOT NULL,
    [ProductName] NVARCHAR(200) NOT NULL,
    CONSTRAINT [PK_Product] PRIMARY KEY CLUSTERED ([ProductId])
) AS NODE;
"#;
    let model = parse_and_build_model(sql);

    // Check table
    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();
    assert!(table.is_node, "Table should be marked as NODE");

    // Check constraint
    let pk = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            if c.name == "PK_Product" {
                Some(c)
            } else {
                None
            }
        } else {
            None
        }
    });

    assert!(pk.is_some(), "Should have PK_Product constraint");
}

#[test]
fn test_build_edge_table_with_check_constraint() {
    let sql = r#"
CREATE TABLE [dbo].[Reviewed] (
    [Rating] INT NOT NULL,
    [ReviewText] NVARCHAR(MAX) NULL,
    CONSTRAINT [CK_Reviewed_Rating] CHECK ([Rating] >= 1 AND [Rating] <= 5)
) AS EDGE;
"#;
    let model = parse_and_build_model(sql);

    // Check table
    let table = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Table(t) = e {
            Some(t)
        } else {
            None
        }
    });

    assert!(table.is_some());
    let table = table.unwrap();
    assert!(table.is_edge, "Table should be marked as EDGE");

    // Check constraint
    let ck = model.elements.iter().find_map(|e| {
        if let rust_sqlpackage::model::ModelElement::Constraint(c) = e {
            if c.name == "CK_Reviewed_Rating" {
                Some(c)
            } else {
                None
            }
        } else {
            None
        }
    });

    assert!(ck.is_some(), "Should have CK_Reviewed_Rating constraint");
}
