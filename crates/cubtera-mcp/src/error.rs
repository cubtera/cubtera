//! `AppError` -> MCP `ErrorData` mapping
//!
//! Same boundary rule as `cubtera-api`'s `problem+json` and `cubtera`'s exit
//! codes (migration plan item 9): `AppError` only gets translated to a
//! protocol-specific shape at the interface layer, never inside
//! domain/core/infra.

use cubtera_core::error::AppError;
use rmcp::ErrorData as McpError;

pub fn to_mcp_error(err: AppError) -> McpError {
    let message = err.to_string();
    match err {
        AppError::NotFound { .. } => McpError::resource_not_found(message, None),
        AppError::Validation(_) | AppError::Domain(_) | AppError::AccessDenied(_) => {
            McpError::invalid_params(message, None)
        }
        AppError::Repository(_) | AppError::Runner(_) | AppError::Io(_) | AppError::Config(_) => {
            McpError::internal_error(message, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ErrorCode;

    #[test]
    fn not_found_maps_to_resource_not_found() {
        let err = to_mcp_error(AppError::not_found("dimension", "dc:missing"));
        assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);
        assert!(err.message.contains("dc:missing"));
    }

    #[test]
    fn validation_and_access_denied_map_to_invalid_params() {
        assert_eq!(
            to_mcp_error(AppError::validation("bad input")).code,
            ErrorCode::INVALID_PARAMS
        );
        assert_eq!(
            to_mcp_error(AppError::access_denied("nope")).code,
            ErrorCode::INVALID_PARAMS
        );
    }

    #[test]
    fn repository_and_io_errors_map_to_internal_error() {
        assert_eq!(
            to_mcp_error(AppError::repository("db down")).code,
            ErrorCode::INTERNAL_ERROR
        );
        assert_eq!(
            to_mcp_error(AppError::io("disk full")).code,
            ErrorCode::INTERNAL_ERROR
        );
    }
}
