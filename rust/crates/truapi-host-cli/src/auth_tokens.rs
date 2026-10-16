//! In-memory auth token keeper for the CLI.
//!
//! Tokens last as long as the process: this exists so a product can exercise
//! AuthTokens headlessly, not to keep anything secret across runs.

use std::{collections::HashMap, sync::Mutex};

use truapi::{
	latest::{AuthToken, GenericError},
	platform::{AuthTokenPlatform, async_trait},
};

/// Tokens by the key the core hands over, each with the product and the
/// account tag it belongs to.
#[derive(Default)]
pub struct CliAuthTokens {
	tokens: Mutex<HashMap<String, Kept>>,
}

/// One kept token and whose it is.
struct Kept {
	product_id: String,
	account: String,
	token: AuthToken,
}

impl CliAuthTokens {
	fn tokens(&self) -> std::sync::MutexGuard<'_, HashMap<String, Kept>> {
		self.tokens.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}
}

#[async_trait]
impl AuthTokenPlatform for CliAuthTokens {
	async fn store_auth_token(
		&self,
		product_id: &str,
		account: &str,
		key: &str,
		token: AuthToken,
	) -> Result<(), GenericError> {
		let kept = Kept { product_id: product_id.to_string(), account: account.to_string(), token };
		self.tokens().insert(key.to_string(), kept);
		Ok(())
	}

	async fn auth_token(
		&self,
		_product_id: &str,
		key: &str,
	) -> Result<Option<AuthToken>, GenericError> {
		Ok(self.tokens().get(key).map(|kept| kept.token.clone()))
	}

	async fn remove_auth_token(&self, _product_id: &str, key: &str) -> Result<(), GenericError> {
		self.tokens().remove(key);
		Ok(())
	}

	async fn auth_token_count(&self, product_id: &str, account: &str) -> Result<u32, GenericError> {
		let count = self
			.tokens()
			.values()
			.filter(|kept| kept.product_id == product_id && kept.account == account)
			.count();
		Ok(u32::try_from(count).unwrap_or(u32::MAX))
	}

	async fn clear_auth_tokens(&self, product_id: Option<&str>) -> Result<(), GenericError> {
		match product_id {
			Some(product_id) => self.tokens().retain(|_, kept| kept.product_id != product_id),
			None => self.tokens().clear(),
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn token() -> AuthToken {
		AuthToken { token: "jwt".to_string(), refresh_token: None, expires_at: None }
	}

	// Clearing one product keeps the others' tokens, and clearing everything,
	// as the core does on sign-out, leaves none.
	#[tokio::test]
	async fn clearing_drops_one_product_or_every_product() {
		let tokens = CliAuthTokens::default();
		tokens.store_auth_token("wallet.dot", "alice", "k1", token()).await.unwrap();
		tokens.store_auth_token("wallet.dot", "alice", "k2", token()).await.unwrap();
		tokens.store_auth_token("other.dot", "alice", "k3", token()).await.unwrap();

		tokens.clear_auth_tokens(Some("wallet.dot")).await.unwrap();
		let after_product = (
			tokens.auth_token_count("wallet.dot", "alice").await.unwrap(),
			tokens.auth_token_count("other.dot", "alice").await.unwrap(),
		);
		tokens.clear_auth_tokens(None).await.unwrap();

		assert_eq!(after_product, (0, 1));
		assert_eq!(tokens.auth_token("other.dot", "k3").await.unwrap(), None);
	}

	// The core caps each account separately, so another account's tokens for
	// the same product must not count.
	#[tokio::test]
	async fn the_count_is_per_account() {
		let tokens = CliAuthTokens::default();
		tokens.store_auth_token("wallet.dot", "alice", "k1", token()).await.unwrap();
		tokens.store_auth_token("wallet.dot", "bob", "k2", token()).await.unwrap();
		tokens.store_auth_token("wallet.dot", "bob", "k3", token()).await.unwrap();

		assert_eq!(tokens.auth_token_count("wallet.dot", "alice").await.unwrap(), 1);
		assert_eq!(tokens.auth_token_count("wallet.dot", "bob").await.unwrap(), 2);
	}
}
