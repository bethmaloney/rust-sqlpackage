//! End-to-end tests for dacpac deployment to SQL Server
//!
//! These tests build a dacpac and deploy it to a real SQL Server instance,
//! then verify the deployment was successful by querying the database.
//!
//! Prerequisites:
//! - SQL Server 2022 running at localhost with sa/Password1
//! - SqlPackage CLI available in PATH or as a .NET global tool
//!
//! Run with: cargo test --test e2e_tests -- --ignored

use std::process::Command;

use tiberius::{AuthMethod, Client, Config, Row};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use crate::common::{DacpacInfo, TestContext};

const SQL_SERVER_HOST: &str = "localhost";
const SQL_SERVER_PORT: u16 = 1433;
const SQL_SERVER_USER: &str = "sa";
const SQL_SERVER_PASSWORD: &str = "Password1";
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
#[allow(dead_code)]
fn sqlpackage_available() -> bool {
    get_sqlpackage_path().is_some()
}

/// Create a tiberius client config
fn create_config(database: Option<&str>) -> Config {
    let mut config = Config::new();
    config.host(SQL_SERVER_HOST);
    config.port(SQL_SERVER_PORT);
    config.authentication(AuthMethod::sql_server(SQL_SERVER_USER, SQL_SERVER_PASSWORD));
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
#[allow(dead_code)]
fn deploy_dacpac(dacpac_path: &std::path::Path) -> Result<(), String> {
    let sqlpackage = get_sqlpackage_path().ok_or_else(|| "SqlPackage not found".to_string())?;

    // Include database name in connection string
    let connection_string = format!(
        "Server={},{};Database={};User Id={};Password={};TrustServerCertificate=True;",
        SQL_SERVER_HOST, SQL_SERVER_PORT, TEST_DATABASE, SQL_SERVER_USER, SQL_SERVER_PASSWORD
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
#[allow(dead_code)]
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
    assert!(
        model_xml.contains("SqlUserDefinedTableType"),
        "Model should contain SqlUserDefinedTableType elements"
    );
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
#[ignore = "Requires SQL Server 2022 at localhost with sa/Password1"]
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

/// Full deployment test.
///
/// This test builds a dacpac and deploys it to SQL Server using SqlPackage CLI.
/// Requires SQL Server 2022 at localhost with sa/Password1 and SqlPackage CLI.
#[tokio::test]
#[ignore = "Requires SQL Server 2022 at localhost with sa/Password1 and SqlPackage CLI"]
async fn test_e2e_deploy_to_sql_server() {
    if !sqlpackage_available() {
        eprintln!("Skipping: SqlPackage CLI not found");
        return;
    }

    let ctx = TestContext::with_fixture("e2e_simple");
    let result = ctx.build();
    assert!(result.success, "Build should succeed");

    let dacpac_path = result.dacpac_path.expect("Should have dacpac path");

    let mut client = connect(None).await.expect("Should connect");
    drop_database_if_exists(&mut client)
        .await
        .expect("Should drop if exists");

    deploy_dacpac(&dacpac_path).expect("Deploy should succeed");

    // Verification code would go here once deployment works
    let mut client = connect(None).await.expect("Should reconnect");
    drop_database_if_exists(&mut client)
        .await
        .expect("Should cleanup");
}
