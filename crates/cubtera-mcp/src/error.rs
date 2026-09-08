//! `ClientError` -> MCP `ErrorData` mapping
//!
//! Same boundary rule as `cubtera-api`'s `problem+json` and `cubtera`'s exit
//! codes: an error only gets translated to a protocol-specific shape at the
//! interface layer. Since P7, `cubtera-mcp` is a pure HTTP client of
//! `cubtera-server` (see `client.rs`), so the thing being translated here is
//! the HTTP status `cubtera-server`'s `ApiError` returned, not a local
//! `AppError` variant.

use crate::client::ClientError;
use rmcp::ErrorData as McpError;

pub fn to_mcp_error(err: ClientError) -> McpError {
    let message = err.to_string();
    match err.status {
        Some(404) => McpError::resource_not_found(message, None),
        Some(400) | Some(403) => McpError::invalid_params(message, None),
        _ => McpError::internal_error(message, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ErrorCode;

    fn err(status: Option<u16>) -> ClientError {
        ClientError {
            status,
            message: "boom".to_string(),
        }
    }

    #[test]
    fn not_found_maps_to_resource_not_found() {
        assert_eq!(
            to_mcp_error(err(Some(404))).code,
            ErrorCode::RESOURCE_NOT_FOUND
        );
    }

    #[test]
    fn bad_request_and_forbidden_map_to_invalid_params() {
        assert_eq!(to_mcp_error(err(Some(400))).code, ErrorCode::INVALID_PARAMS);
        assert_eq!(to_mcp_error(err(Some(403))).code, ErrorCode::INVALID_PARAMS);
    }

    #[test]
    fn server_errors_and_network_failures_map_to_internal_error() {
        assert_eq!(to_mcp_error(err(Some(500))).code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(to_mcp_error(err(None)).code, ErrorCode::INTERNAL_ERROR);
    }
}
