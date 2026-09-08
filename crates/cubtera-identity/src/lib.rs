//! Cubtera v3 secret/identity resolution.
//!
//! [`cubtera_app::ports::IdentityProvider`] turns an [`OutputValue::Secret`]
//! ref (see `cubtera_model::output_set`) into its real value, at execution
//! time only - never at display time (`OutputValue::redacted` covers
//! that). `cubtera-store` never holds a resolved secret value, only the
//! ref string, so which backend actually resolves it is entirely this
//! crate's concern; `cubtera-app` only ever sees the trait.
//!
//! P6 ships exactly one production-usable scheme, `env:` (read a host
//! environment variable) - a real vault/KMS-backed provider is a separate,
//! deliberate decision this phase doesn't make; adding one means adding a
//! new `IdentityProvider` impl here, not touching `cubtera-app`.

use async_trait::async_trait;
use cubtera_app::error::{AppError, AppResult};
use cubtera_app::ports::IdentityProvider;
use serde_json::Value;

/// Resolves a secret ref of the form `"<scheme>:<rest>"`:
///
/// - `env:VAR_NAME` - the value of host environment variable `VAR_NAME`,
///   as a JSON string. Missing means the run fails outright (a secret ref
///   that can't be resolved is never silently treated as absent - that
///   would turn a redacted value into a null one downstream).
/// - `literal:<value>` - the literal string `<value>`, verbatim. Exists
///   for fixtures/tests and for a unit that genuinely wants a fixed,
///   non-sensitive "secret" (e.g. a well-known placeholder) without a real
///   backend - not meant for anything actually sensitive in production.
///
/// A ref with no recognized `<scheme>:` prefix is a validation error, not
/// a guess.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnvIdentityProvider;

#[async_trait]
impl IdentityProvider for EnvIdentityProvider {
    async fn resolve_secret(&self, secret_ref: &str) -> AppResult<Value> {
        if let Some(var) = secret_ref.strip_prefix("env:") {
            let value = std::env::var(var).map_err(|_| {
                AppError::backend(format!(
                    "secret ref 'env:{var}': environment variable is not set"
                ))
            })?;
            return Ok(Value::String(value));
        }

        if let Some(literal) = secret_ref.strip_prefix("literal:") {
            return Ok(Value::String(literal.to_string()));
        }

        Err(AppError::validation(format!(
            "secret ref '{secret_ref}' has no recognized scheme (expected 'env:' or 'literal:')"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolves_an_env_var() {
        std::env::set_var("CUBTERA_IDENTITY_TEST_VAR", "shh");
        let provider = EnvIdentityProvider;
        let value = provider
            .resolve_secret("env:CUBTERA_IDENTITY_TEST_VAR")
            .await
            .unwrap();
        assert_eq!(value, Value::String("shh".to_string()));
        std::env::remove_var("CUBTERA_IDENTITY_TEST_VAR");
    }

    #[tokio::test]
    async fn missing_env_var_is_a_hard_error() {
        std::env::remove_var("CUBTERA_IDENTITY_TEST_VAR_MISSING");
        let provider = EnvIdentityProvider;
        let err = provider
            .resolve_secret("env:CUBTERA_IDENTITY_TEST_VAR_MISSING")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Backend(_)));
    }

    #[tokio::test]
    async fn literal_scheme_passes_through_verbatim() {
        let provider = EnvIdentityProvider;
        let value = provider.resolve_secret("literal:hello").await.unwrap();
        assert_eq!(value, Value::String("hello".to_string()));
    }

    #[tokio::test]
    async fn unknown_scheme_is_a_validation_error() {
        let provider = EnvIdentityProvider;
        let err = provider
            .resolve_secret("vault:prod/kms#arn")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
