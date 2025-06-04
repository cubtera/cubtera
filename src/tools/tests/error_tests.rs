#[cfg(test)]
mod tests {
    use crate::tools::error::*;

    #[test]
    fn test_error_creation() {
        let error = ToolsError::path_error("test path error");
        assert!(error.to_string().contains("test path error"));

        let error = ToolsError::process_error("test process error");
        assert!(error.to_string().contains("test process error"));

        let error = ToolsError::validation_error("test validation error");
        assert!(error.to_string().contains("test validation error"));
    }

    #[test]
    fn test_error_from_conversions() {
        // Test automatic conversions from standard error types
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let tools_error: ToolsError = io_error.into();
        assert!(tools_error.to_string().contains("IO error"));

        let json_error = serde_json::from_str::<serde_json::Value>("invalid json");
        assert!(json_error.is_err());
        let tools_error: ToolsError = json_error.unwrap_err().into();
        assert!(tools_error.to_string().contains("JSON error"));
    }

    #[test]
    fn test_result_type() {
        fn test_function() -> Result<String> {
            Ok("success".to_string())
        }

        fn test_error_function() -> Result<String> {
            Err(ToolsError::operation_failed("test error"))
        }

        assert!(test_function().is_ok());
        assert!(test_error_function().is_err());
    }
} 