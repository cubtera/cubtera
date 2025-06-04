use super::error::{Result, ToolsError};

/// Connect to MongoDB database with proper error handling
/// 
/// # Arguments
/// * `connection_string` - MongoDB connection string
/// 
/// # Returns
/// * `Result<mongodb::sync::Client>` - Database client or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::db::connect;
/// 
/// let client = connect("mongodb://localhost:27017")?;
/// ```
pub fn connect(connection_string: &str) -> Result<mongodb::sync::Client> {
    // For sync client, use with_uri_str directly
    let client = mongodb::sync::Client::with_uri_str(connection_string)
        .map_err(|e| ToolsError::database_error(
            format!("Failed to connect to database '{}': {}", connection_string, e)
        ))?;
    
    Ok(client)
}

/// Test database connection
/// 
/// # Arguments
/// * `client` - MongoDB client to test
/// 
/// # Returns
/// * `Result<()>` - Success or error
pub fn test_connection(client: &mongodb::sync::Client) -> Result<()> {
    // Try to list databases to test connection
    let _databases = client.list_database_names()
        .run()
        .map_err(|e| ToolsError::database_error(
            format!("Connection test failed: {}", e)
        ))?;
    
    Ok(())
}

/// Connect and test database connection
/// 
/// # Arguments
/// * `connection_string` - MongoDB connection string
/// 
/// # Returns
/// * `Result<mongodb::sync::Client>` - Database client or error
pub fn connect_and_test(connection_string: &str) -> Result<mongodb::sync::Client> {
    let client = connect(connection_string)?;
    test_connection(&client)?;
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_connect_invalid_connection_string() {
        let result = connect("invalid://connection");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_connect_empty_connection_string() {
        let result = connect("");
        assert!(result.is_err());
    }
    
    // Note: We can't test actual connections without a running MongoDB instance
    // These tests just verify error handling for invalid inputs
}

// TODO: Add more database operations as needed
// - test_connection
// - execute_query
// - insert_document
// - update_document
// - delete_document
// etc. 