//! In-memory auth token keeper for the CLI.
//!
//! Tokens last as long as the process: this exists so a product can exercise
//! AuthTokens headlessly, not to keep anything secret across runs.

use std::collections::HashMap;
use std::sync::Mutex;

use truapi::latest::{AuthToken, GenericError};
use truapi::platform::{AuthTokenPlatform, ProductContext, async_trait};

/// Tokens keyed by product id, then service.
#[derive(Default)]
pub struct CliAuthTokens {
    tokens: Mutex<HashMap<(String, String), AuthToken>>,
}

impl CliAuthTokens {
    fn tokens(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), AuthToken>> {
        self.tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl AuthTokenPlatform for CliAuthTokens {
    async fn store_auth_token(
        &self,
        product: &ProductContext,
        service: String,
        token: AuthToken,
    ) -> Result<(), GenericError> {
        self.tokens()
            .insert((product.product_id.clone(), service), token);
        Ok(())
    }

    async fn auth_token(
        &self,
        product: &ProductContext,
        service: String,
    ) -> Result<Option<AuthToken>, GenericError> {
        Ok(self
            .tokens()
            .get(&(product.product_id.clone(), service))
            .cloned())
    }

    async fn remove_auth_token(
        &self,
        product: &ProductContext,
        service: String,
    ) -> Result<(), GenericError> {
        self.tokens().remove(&(product.product_id.clone(), service));
        Ok(())
    }
}
