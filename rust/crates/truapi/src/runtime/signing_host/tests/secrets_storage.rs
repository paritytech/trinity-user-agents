use super::*;

use crate::secrets::{SecretKind, SecretOwner};

// The wallet ending its own session drops every secret, and so does switching
// to another account. Activating the same account again, as the app does on
// every launch, keeps them.
#[test]
fn the_wallet_drops_product_secrets_when_its_session_ends_or_the_account_changes() {
	let (services, signing_host) = signing_runtime();
	let storage = services.install_memory_secrets();
	let secrets = services.core_secrets().expect("secrets installed");
	let keep_secret = || {
		let owner = SecretOwner { product_id: "wallet.dot", account: Some(&[1; 32]) };
		futures::executor::block_on(secrets.put(
			SecretKind::ProductSecret,
			b"id",
			owner,
			b"secret",
		))
		.expect("secret kept");
	};
	let kept = || storage.stored().len();

	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("first activation");
	keep_secret();
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("same account again");
	assert_eq!(kept(), 1, "same account again");

	futures::executor::block_on(signing_host.activate_local_session([0x11; 16].to_vec()))
		.expect("another account");
	assert_eq!(kept(), 0, "another account");

	keep_secret();
	futures::executor::block_on(signing_host.disconnect());
	assert_eq!(kept(), 0, "disconnect");
}
