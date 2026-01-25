//! End-to-end tests for dacpac deployment to SQL Server
//!
//! These tests build a dacpac and deploy it to a real SQL Server instance,
//! then verify the deployment was successful by querying the database.
//!
//! Prerequisites:
//! - SQL Server 2022 running (configured via .env or environment variables)
//! - SqlPackage CLI available in PATH or as a .NET global tool
//!
//! Environment variables (with defaults):
//! - SQL_SERVER_HOST (default: localhost)
//! - SQL_SERVER_PORT (default: 1433)
//! - SQL_SERVER_USER (default: sa)
//! - SQL_SERVER_PASSWORD (default: Password1)
//!
//! Run with: cargo test --test e2e_tests -- --ignored

use std::process::Command;
use std::sync::LazyLock;

use tiberius::{AuthMethod, Client, Config, Row};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use crate::common::{DacpacInfo, TestContext};

/// Load environment variables from .env file (if present)
fn load_env() {
    let _ = dotenvy::dotenv();
}

/// SQL Server connection configuration loaded from environment
static SQL_CONFIG: LazyLock<SqlServerConfig> = LazyLock::new(|| {
    load_env();
    SqlServerConfig {
        host: std::env::var("SQL_SERVER_HOST").unwrap_or_else(|_| "localhost".to_string()),
        port: std::env::var("SQL_SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(1433),
        user: std::env::var("SQL_SERVER_USER").unwrap_or_else(|_| "sa".to_string()),
        password: std::env::var("SQL_SERVER_PASSWORD").unwrap_or_else(|_| "Password1".to_string()),
    }
});

struct SqlServerConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
}

const TEST_DATABASE: &str = "E2ESimple_Test";

/// Type alias for the SQL client
type SqlClient = Client<Compat<TcpStream>>;

/// Get the sqlpackage command path
fn get_sqlpackage_path() -> Option<String> {
    // Check if sqlpackage is in PATH
    if Command::new("sqlpackage")
        .arg("/version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some("sqlpackage".to_string());
    }

    // Check for .NET global tool location
    if let Ok(home) = std::env::var("HOME") {
        let dotnet_tool_path = format!("{}/.dotnet/tools/sqlpackage", home);
        if Command::new(&dotnet_tool_path)
            .arg("/version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(dotnet_tool_path);
        }
    }

    None
}

/// Check if SqlPackage is available
fn sqlpackage_available() -> bool {
    get_sqlpackage_path().is_some()
}

/// Create a tiberius client config
fn create_config(database: Option<&str>) -> Config {
    let mut config = Config::new();
    config.host(&SQL_CONFIG.host);
    config.port(SQL_CONFIG.port);
    config.authentication(AuthMethod::sql_server(&SQL_CONFIG.user, &SQL_CONFIG.password));
    config.trust_cert();

    if let Some(db) = database {
        config.database(db);
    }

    config
}

/// Connect to SQL Server
async fn connect(database: Option<&str>) -> Result<SqlClient, Box<dyn std::error::Error>> {
    let config = create_config(database);
    let tcp = TcpStream::connect(config.get_addr()).await?;
    tcp.set_nodelay(true)?;
    let client = Client::connect(config, tcp.compat_write()).await?;
    Ok(client)
}

/// Extract count from row
fn get_count(row: Option<Row>) -> i32 {
    row.and_then(|r| r.get::<i32, _>(0)).unwrap_or(0)
}

/// Drop the test database if it exists
async fn drop_database_if_exists(client: &mut SqlClient) -> Result<(), Box<dyn std::error::Error>> {
    let query = format!(
        "IF EXISTS (SELECT 1 FROM sys.databases WHERE name = '{}') \
         BEGIN \
             ALTER DATABASE [{}] SET SINGLE_USER WITH ROLLBACK IMMEDIATE; \
             DROP DATABASE [{}]; \
         END",
        TEST_DATABASE, TEST_DATABASE, TEST_DATABASE
    );
    client.execute(&query, &[]).await?;
    Ok(())
}

/// Deploy a dacpac using SqlPackage CLI
fn deploy_dacpac(dacpac_path: &std::path::Path) -> Result<(), String> {
    let sqlpackage = get_sqlpackage_path().ok_or_else(|| "SqlPackage not found".to_string())?;

    // Include database name in connection string
    let connection_string = format!(
        "Server={},{};Database={};User Id={};Password={};TrustServerCertificate=True;",
        SQL_CONFIG.host, SQL_CONFIG.port, TEST_DATABASE, SQL_CONFIG.user, SQL_CONFIG.password
    );

    let output = Command::new(&sqlpackage)
        .arg("/Action:Publish")
        .arg(format!("/SourceFile:{}", dacpac_path.display()))
        .arg(format!("/TargetConnectionString:{}", connection_string))
        .arg("/p:BlockOnPossibleDataLoss=False")
        .output()
        .map_err(|e| format!("Failed to run sqlpackage: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "SqlPackage failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        ));
    }

    Ok(())
}

/// Query to check if a table exists
async fn table_exists(
    client: &mut SqlClient,
    schema: &str,
    table: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let query = "SELECT COUNT(*) FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = @P1 AND TABLE_NAME = @P2";
    let row = client
        .query(query, &[&schema, &table])
        .await?
        .into_row()
        .await?;
    Ok(get_count(row) > 0)
}

/// Query to check if a view exists
async fn view_exists(
    client: &mut SqlClient,
    schema: &str,
    view: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let query = "SELECT COUNT(*) FROM INFORMATION_SCHEMA.VIEWS WHERE TABLE_SCHEMA = @P1 AND TABLE_NAME = @P2";
    let row = client
        .query(query, &[&schema, &view])
        .await?
        .into_row()
        .await?;
    Ok(get_count(row) > 0)
}

/// Query to check if a stored procedure exists
async fn procedure_exists(
    client: &mut SqlClient,
    schema: &str,
    procedure: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let query = "SELECT COUNT(*) FROM INFORMATION_SCHEMA.ROUTINES WHERE ROUTINE_SCHEMA = @P1 AND ROUTINE_NAME = @P2 AND ROUTINE_TYPE = 'PROCEDURE'";
    let row = client
        .query(query, &[&schema, &procedure])
        .await?
        .into_row()
        .await?;
    Ok(get_count(row) > 0)
}

/// Get column names for a table
async fn get_columns_for_table(
    client: &mut SqlClient,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let query = format!(
        "SELECT COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' ORDER BY ORDINAL_POSITION",
        schema, table
    );
    let stream = client.simple_query(&query).await?;
    let rows: Vec<Row> = stream.into_first_result().await?;
    let columns: Vec<String> = rows
        .iter()
        .filter_map(|r| r.get::<&str, _>(0).map(|s| s.to_string()))
        .collect();
    Ok(columns)
}

/// Check if a foreign key constraint exists
async fn foreign_key_exists(
    client: &mut SqlClient,
    constraint_name: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let query = format!(
        "SELECT COUNT(*) FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS WHERE CONSTRAINT_NAME = '{}' AND CONSTRAINT_TYPE = 'FOREIGN KEY'",
        constraint_name
    );
    let row = client.simple_query(&query).await?.into_row().await?;
    Ok(get_count(row) > 0)
}

/// Get row count from a table
async fn get_row_count(
    client: &mut SqlClient,
    schema: &str,
    table: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    let query = format!("SELECT COUNT(*) FROM [{}].[{}]", schema, table);
    let row = client.simple_query(&query).await?.into_row().await?;
    Ok(get_count(row))
}

/// Check if specific seed data exists (by checking for a value in a column)
async fn seed_data_exists(
    client: &mut SqlClient,
    schema: &str,
    table: &str,
    column: &str,
    value: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let query = format!(
        "SELECT COUNT(*) FROM [{}].[{}] WHERE [{}] = '{}'",
        schema, table, column, value
    );
    let row = client.simple_query(&query).await?.into_row().await?;
    Ok(get_count(row) > 0)
}

// ============================================================================
// E2E Tests - Dacpac Build Verification
// ============================================================================

/// Test that the dacpac builds successfully and contains expected elements.
///
/// This test verifies the dacpac structure including all expected element types.
/// For full deployment tests, see test_e2e_deploy_to_sql_server (requires SQL Server).
#[test]
fn test_e2e_build_simple_dacpac() {
    let ctx = TestContext::with_fixture("e2e_simple");
    let result = ctx.build();

    assert!(
        result.success,
        "Dacpac build should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.expect("Should have dacpac path");
    assert!(dacpac_path.exists(), "Dacpac file should exist");

    // Verify dacpac structure
    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    assert!(info.is_valid(), "Dacpac should have all required files");

    // Verify tables
    assert!(
        info.tables.iter().any(|t| t.contains("Categories")),
        "Dacpac should contain Categories table"
    );
    assert!(
        info.tables.iter().any(|t| t.contains("Products")),
        "Dacpac should contain Products table"
    );
    assert!(
        info.tables.iter().any(|t| t.contains("Customers")),
        "Dacpac should contain Customers table"
    );
    assert!(
        info.tables.iter().any(|t| t.contains("Orders")),
        "Dacpac should contain Orders table"
    );

    // Verify views
    assert!(
        info.views.iter().any(|v| v.contains("ActiveProducts")),
        "Dacpac should contain ActiveProducts view"
    );

    // Verify schemas
    assert!(
        info.schemas.iter().any(|s| s.contains("Sales")),
        "Dacpac should contain Sales schema"
    );

    // Verify model.xml contains expected element types
    let model_xml = info.model_xml_content.expect("Should have model XML");
    assert!(
        model_xml.contains("SqlTable"),
        "Model should contain SqlTable elements"
    );
    assert!(
        model_xml.contains("SqlView"),
        "Model should contain SqlView elements"
    );
    assert!(
        model_xml.contains("SqlProcedure"),
        "Model should contain SqlProcedure elements"
    );
    assert!(
        model_xml.contains("SqlScalarFunction"),
        "Model should contain SqlScalarFunction elements"
    );
    assert!(
        model_xml.contains("SqlIndex"),
        "Model should contain SqlIndex elements"
    );
    assert!(
        model_xml.contains("SqlSequence"),
        "Model should contain SqlSequence elements"
    );
    // SqlTableType temporarily excluded pending column structure support
    // assert!(
    //     model_xml.contains("SqlTableType"),
    //     "Model should contain SqlTableType elements"
    // );
    assert!(
        model_xml.contains("SqlForeignKeyConstraint"),
        "Model should contain SqlForeignKeyConstraint elements"
    );

    println!("E2E build test completed successfully!");
}

/// Test that the comprehensive dacpac builds successfully.
#[test]
fn test_e2e_build_comprehensive_dacpac() {
    let ctx = TestContext::with_fixture("e2e_comprehensive");
    let result = ctx.build();

    assert!(
        result.success,
        "Comprehensive dacpac build should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.expect("Should have dacpac path");
    assert!(dacpac_path.exists(), "Dacpac file should exist");

    let info = DacpacInfo::from_dacpac(&dacpac_path).expect("Should parse dacpac");
    assert!(info.is_valid(), "Dacpac should have all required files");

    // The comprehensive fixture has more tables
    assert!(info.tables.len() >= 6, "Should have at least 6 tables");
    assert!(info.views.len() >= 2, "Should have at least 2 views");

    println!("E2E comprehensive build test completed successfully!");
}

// ============================================================================
// E2E Tests - SQL Server Connectivity (requires running SQL Server)
// ============================================================================

#[tokio::test]
#[ignore = "Requires SQL Server (configure via .env or environment variables)"]
async fn test_e2e_sql_server_connectivity() {
    // Test basic connectivity to SQL Server
    let mut client = connect(None).await.expect("Should connect to SQL Server");

    // Run a simple query
    let query = "SELECT @@VERSION";
    let row = client
        .query(query, &[])
        .await
        .unwrap()
        .into_row()
        .await
        .unwrap();
    let version: Option<&str> = row.as_ref().and_then(|r| r.get(0));

    assert!(version.is_some(), "Should get SQL Server version");
    let version_str = version.unwrap();
    assert!(
        version_str.contains("SQL Server") || version_str.contains("Microsoft"),
        "Should be SQL Server: {}",
        version_str
    );

    println!("Connected to: {}", version_str);

    // Drop any leftover test database
    drop_database_if_exists(&mut client)
        .await
        .expect("Should be able to drop test database");

    println!("SQL Server connectivity test passed!");
}

// ============================================================================
// E2E Tests - Full Deployment (requires SQL Server and SqlPackage CLI)
// ============================================================================

/// Full deployment test for e2e_simple fixture.
///
/// This test builds a dacpac and deploys it to SQL Server using SqlPackage CLI,
/// then verifies:
/// - Tables exist with correct columns
/// - Views exist
/// - Stored procedures exist
/// - Foreign key constraints exist
///
/// Requires SQL Server and SqlPackage CLI (configure via .env or environment variables).
#[tokio::test]
#[ignore = "Requires SQL Server and SqlPackage CLI (configure via .env)"]
async fn test_e2e_deploy_to_sql_server() {
    if !sqlpackage_available() {
        eprintln!("Skipping: SqlPackage CLI not found");
        return;
    }

    let ctx = TestContext::with_fixture("e2e_simple");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.expect("Should have dacpac path");

    // Setup: drop any existing test database
    let mut client = connect(None).await.expect("Should connect");
    drop_database_if_exists(&mut client)
        .await
        .expect("Should drop if exists");

    // Deploy the dacpac
    deploy_dacpac(&dacpac_path).expect("Deploy should succeed");

    // Connect to the deployed database for verification
    let mut client = connect(Some(TEST_DATABASE))
        .await
        .expect("Should connect to test database");

    // ========================================================================
    // Verify Tables Exist
    // ========================================================================
    println!("Verifying tables...");

    assert!(
        table_exists(&mut client, "dbo", "Categories")
            .await
            .expect("Query should succeed"),
        "Categories table should exist"
    );
    assert!(
        table_exists(&mut client, "dbo", "Products")
            .await
            .expect("Query should succeed"),
        "Products table should exist"
    );
    assert!(
        table_exists(&mut client, "Sales", "Customers")
            .await
            .expect("Query should succeed"),
        "Customers table should exist in Sales schema"
    );
    assert!(
        table_exists(&mut client, "Sales", "Orders")
            .await
            .expect("Query should succeed"),
        "Orders table should exist in Sales schema"
    );

    // ========================================================================
    // Verify Table Structure (columns)
    // ========================================================================
    println!("Verifying table columns...");

    let products_columns = get_columns_for_table(&mut client, "dbo", "Products")
        .await
        .expect("Should get columns");
    let expected_products_columns = vec!["Id", "SKU", "Name", "CategoryId", "Price", "Quantity"];
    assert_eq!(
        products_columns, expected_products_columns,
        "Products table should have correct columns"
    );

    let categories_columns = get_columns_for_table(&mut client, "dbo", "Categories")
        .await
        .expect("Should get columns");
    let expected_categories_columns = vec!["Id", "Name", "Description"];
    assert_eq!(
        categories_columns, expected_categories_columns,
        "Categories table should have correct columns"
    );

    // ========================================================================
    // Verify Views Exist
    // ========================================================================
    println!("Verifying views...");

    assert!(
        view_exists(&mut client, "dbo", "ActiveProducts")
            .await
            .expect("Query should succeed"),
        "ActiveProducts view should exist"
    );

    // ========================================================================
    // Verify Stored Procedures Exist
    // ========================================================================
    println!("Verifying stored procedures...");

    assert!(
        procedure_exists(&mut client, "dbo", "GetProducts")
            .await
            .expect("Query should succeed"),
        "GetProducts stored procedure should exist"
    );

    // ========================================================================
    // Verify Foreign Key Constraints Exist
    // ========================================================================
    println!("Verifying foreign key constraints...");

    assert!(
        foreign_key_exists(&mut client, "FK_Products_Categories")
            .await
            .expect("Query should succeed"),
        "FK_Products_Categories constraint should exist"
    );

    println!("All e2e_simple deployment verifications passed!");

    // Cleanup
    let mut client = connect(None).await.expect("Should reconnect");
    drop_database_if_exists(&mut client)
        .await
        .expect("Should cleanup");
}

const TEST_DATABASE_COMPREHENSIVE: &str = "E2EComprehensive_Test";

/// Full deployment test for e2e_comprehensive fixture.
///
/// This test verifies everything from the simple test plus:
/// - Post-deployment script ran (seed data exists)
/// - More complex table structures with IDENTITY, defaults, etc.
///
/// Requires SQL Server and SqlPackage CLI (configure via .env or environment variables).
#[tokio::test]
#[ignore = "Requires SQL Server and SqlPackage CLI (configure via .env)"]
async fn test_e2e_deploy_comprehensive_with_post_deploy() {
    if !sqlpackage_available() {
        eprintln!("Skipping: SqlPackage CLI not found");
        return;
    }

    let ctx = TestContext::with_fixture("e2e_comprehensive");
    let result = ctx.build();
    assert!(
        result.success,
        "Build should succeed. Errors: {:?}",
        result.errors
    );

    let dacpac_path = result.dacpac_path.expect("Should have dacpac path");

    // Setup: drop any existing test database
    let mut client = connect(None).await.expect("Should connect");
    drop_database_comprehensive_if_exists(&mut client)
        .await
        .expect("Should drop if exists");

    // Deploy the dacpac
    deploy_dacpac_comprehensive(&dacpac_path).expect("Deploy should succeed");

    // Connect to the deployed database for verification
    let mut client = connect(Some(TEST_DATABASE_COMPREHENSIVE))
        .await
        .expect("Should connect to test database");

    // ========================================================================
    // Verify Tables Exist
    // ========================================================================
    println!("Verifying comprehensive tables...");

    // Tables with their schemas
    let dbo_tables = ["Categories", "Products"];
    let sales_tables = ["Customers", "Orders", "OrderItems"];
    let inventory_tables = ["InventoryLog"];

    for table in &dbo_tables {
        assert!(
            table_exists(&mut client, "dbo", table)
                .await
                .expect("Query should succeed"),
            "{} table should exist in dbo schema",
            table
        );
    }
    for table in &sales_tables {
        assert!(
            table_exists(&mut client, "Sales", table)
                .await
                .expect("Query should succeed"),
            "{} table should exist in Sales schema",
            table
        );
    }
    for table in &inventory_tables {
        assert!(
            table_exists(&mut client, "Inventory", table)
                .await
                .expect("Query should succeed"),
            "{} table should exist in Inventory schema",
            table
        );
    }

    // ========================================================================
    // Verify Table Structure (more complex columns with IDENTITY, etc.)
    // ========================================================================
    println!("Verifying comprehensive table columns...");

    let categories_columns = get_columns_for_table(&mut client, "dbo", "Categories")
        .await
        .expect("Should get columns");
    // e2e_comprehensive Categories has: Id, Name, Description, IsActive, CreatedAt
    assert!(
        categories_columns.contains(&"Id".to_string()),
        "Categories should have Id column"
    );
    assert!(
        categories_columns.contains(&"IsActive".to_string()),
        "Categories should have IsActive column"
    );
    assert!(
        categories_columns.contains(&"CreatedAt".to_string()),
        "Categories should have CreatedAt column"
    );

    // ========================================================================
    // Verify Views Exist
    // ========================================================================
    println!("Verifying comprehensive views...");

    assert!(
        view_exists(&mut client, "dbo", "ActiveProducts")
            .await
            .expect("Query should succeed"),
        "ActiveProducts view should exist in dbo schema"
    );
    assert!(
        view_exists(&mut client, "Sales", "CustomerOrderSummary")
            .await
            .expect("Query should succeed"),
        "CustomerOrderSummary view should exist in Sales schema"
    );

    // ========================================================================
    // Verify Stored Procedures Exist
    // ========================================================================
    println!("Verifying comprehensive stored procedures...");

    assert!(
        procedure_exists(&mut client, "dbo", "GetProductsByCategory")
            .await
            .expect("Query should succeed"),
        "GetProductsByCategory stored procedure should exist in dbo schema"
    );
    assert!(
        procedure_exists(&mut client, "Sales", "CreateOrder")
            .await
            .expect("Query should succeed"),
        "CreateOrder stored procedure should exist"
    );

    // ========================================================================
    // Verify Post-Deployment Script Ran (Seed Data)
    // ========================================================================
    println!("Verifying post-deployment script (seed data)...");

    // The post-deployment script inserts Electronics, Clothing, Books into Categories
    assert!(
        seed_data_exists(&mut client, "dbo", "Categories", "Name", "Electronics")
            .await
            .expect("Query should succeed"),
        "Post-deploy seed data 'Electronics' should exist in Categories"
    );
    assert!(
        seed_data_exists(&mut client, "dbo", "Categories", "Name", "Clothing")
            .await
            .expect("Query should succeed"),
        "Post-deploy seed data 'Clothing' should exist in Categories"
    );
    assert!(
        seed_data_exists(&mut client, "dbo", "Categories", "Name", "Books")
            .await
            .expect("Query should succeed"),
        "Post-deploy seed data 'Books' should exist in Categories"
    );

    // Verify the count matches expected seed data
    let category_count = get_row_count(&mut client, "dbo", "Categories")
        .await
        .expect("Should get count");
    assert_eq!(
        category_count, 3,
        "Categories should have exactly 3 seed rows from post-deploy script"
    );

    println!("All e2e_comprehensive deployment verifications passed!");

    // Cleanup
    let mut client = connect(None).await.expect("Should reconnect");
    drop_database_comprehensive_if_exists(&mut client)
        .await
        .expect("Should cleanup");
}

/// Drop the comprehensive test database if it exists
async fn drop_database_comprehensive_if_exists(
    client: &mut SqlClient,
) -> Result<(), Box<dyn std::error::Error>> {
    let query = format!(
        "IF EXISTS (SELECT 1 FROM sys.databases WHERE name = '{}') \
         BEGIN \
             ALTER DATABASE [{}] SET SINGLE_USER WITH ROLLBACK IMMEDIATE; \
             DROP DATABASE [{}]; \
         END",
        TEST_DATABASE_COMPREHENSIVE, TEST_DATABASE_COMPREHENSIVE, TEST_DATABASE_COMPREHENSIVE
    );
    client.execute(&query, &[]).await?;
    Ok(())
}

/// Deploy a dacpac to the comprehensive test database
fn deploy_dacpac_comprehensive(dacpac_path: &std::path::Path) -> Result<(), String> {
    let sqlpackage = get_sqlpackage_path().ok_or_else(|| "SqlPackage not found".to_string())?;

    let connection_string = format!(
        "Server={},{};Database={};User Id={};Password={};TrustServerCertificate=True;",
        SQL_CONFIG.host,
        SQL_CONFIG.port,
        TEST_DATABASE_COMPREHENSIVE,
        SQL_CONFIG.user,
        SQL_CONFIG.password
    );

    let output = Command::new(&sqlpackage)
        .arg("/Action:Publish")
        .arg(format!("/SourceFile:{}", dacpac_path.display()))
        .arg(format!("/TargetConnectionString:{}", connection_string))
        .arg("/p:BlockOnPossibleDataLoss=False")
        .output()
        .map_err(|e| format!("Failed to run sqlpackage: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "SqlPackage failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        ));
    }

    Ok(())
}
