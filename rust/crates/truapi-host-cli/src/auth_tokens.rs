//! In-memory auth token keeper for the CLI.
//!
//! Tokens last as long as the process: this exists so a product can exercise
//! AuthTokens headlessly, not to keep anything secret across runs.

use std::{collections::HashMap, sync::Mutex};

use truapi::{
	latest::{AuthToken, GenericError},
	platform::{AuthTokenPlatform, ProductContext, async_trait},
};

/// Tokens keyed by product id, then service.
#[derive(Default)]
pub struct CliAuthTokens {
	tokens: Mutex<HashMap<(String, String), AuthToken>>,
}

impl CliAuthTokens {
	/// Drop every product's tokens, as a host does on sign-out.
	pub fn clear(&self) {
		self.tokens().clear();
	}

	fn tokens(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), AuthToken>> {
		self.tokens.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
		self.tokens().insert((product.product_id.clone(), service), token);
		Ok(())
	}

	async fn auth_token(
		&self,
		product: &ProductContext,
		service: String,
	) -> Result<Option<AuthToken>, GenericError> {
		Ok(self.tokens().get(&(product.product_id.clone(), service)).cloned())
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

#[cfg(test)]
mod tests {
	use super::*;

	// Tokens belong to the signed-in user, so `/logout` leaves no product able
	// to read one back.
	#[tokio::test]
	async fn clearing_drops_every_products_tokens() {
		let tokens = CliAuthTokens::default();
		let wallet = ProductContext::new("wallet.dot".to_string()).expect("valid product id");
		let other = ProductContext::new("other.dot".to_string()).expect("valid product id");
		let token = AuthToken { token: "jwt".to_string(), refresh_token: None, expires_at: None };
		for product in [&wallet, &other] {
			tokens
				.store_auth_token(product, "onramp".to_string(), token.clone())
				.await
				.unwrap();
		}

		tokens.clear();

		for product in [&wallet, &other] {
			assert_eq!(tokens.auth_token(product, "onramp".to_string()).await.unwrap(), None);
		}
	}
}
