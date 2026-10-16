use super::*;

use std::sync::Mutex;

use crate::platform::AuthTokenPlatform;

/// A token keeper that only records which clears the core asked for.
#[derive(Default)]
struct ClearRecorder {
	clears: Mutex<Vec<Option<String>>>,
}

impl ClearRecorder {
	fn take(&self) -> Vec<Option<String>> {
		self.clears.lock().expect("clears mutex poisoned").drain(..).collect()
	}
}

#[truapi::async_trait]
impl AuthTokenPlatform for ClearRecorder {
	async fn store_auth_token(
		&self,
		_product_id: &str,
		_account: &str,
		_key: &str,
		_token: v01::AuthToken,
	) -> Result<(), v01::GenericError> {
		Ok(())
	}

	async fn auth_token(
		&self,
		_product_id: &str,
		_key: &str,
	) -> Result<Option<v01::AuthToken>, v01::GenericError> {
		Ok(None)
	}

	async fn remove_auth_token(
		&self,
		_product_id: &str,
		_key: &str,
	) -> Result<(), v01::GenericError> {
		Ok(())
	}

	async fn auth_token_count(
		&self,
		_product_id: &str,
		_account: &str,
	) -> Result<u32, v01::GenericError> {
		Ok(0)
	}

	async fn clear_auth_tokens(&self, product_id: Option<&str>) -> Result<(), v01::GenericError> {
		self.clears
			.lock()
			.expect("clears mutex poisoned")
			.push(product_id.map(str::to_string));
		Ok(())
	}
}

// The wallet ending its own session drops every token, and so does switching
// to another account. Activating the same account again, as the app does on
// every launch, keeps them.
#[test]
fn the_wallet_drops_auth_tokens_when_its_session_ends_or_the_account_changes() {
	let (services, signing_host) = signing_runtime();
	let keeper = Arc::new(ClearRecorder::default());
	assert!(services.install_auth_token_platform(keeper.clone()));

	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("first activation");
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("same account again");
	assert_eq!(keeper.take(), Vec::<Option<String>>::new(), "same account again");

	futures::executor::block_on(signing_host.activate_local_session([0x11; 16].to_vec()))
		.expect("another account");
	assert_eq!(keeper.take(), vec![None], "another account");

	futures::executor::block_on(signing_host.disconnect());
	assert_eq!(keeper.take(), vec![None], "disconnect");
}
