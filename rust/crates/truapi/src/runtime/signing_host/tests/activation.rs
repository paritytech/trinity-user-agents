use super::*;

/// Allow once lasts for one unlocked wallet and one installed product: lock,
/// a wallet switch and a product reset each make the next request ask again.
#[test]
fn lock_wallet_switch_and_product_reset_forget_allow_once() {
	let platform = Arc::new(StubPlatform {
		permission_confirmation_decisions: std::sync::Mutex::new(
			[crate::platform::PermissionDecision::AllowOnce; 4].into(),
		),
		..StubPlatform::default()
	});
	let (services, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
	let runtime = product_runtime(services, authority.clone());
	let prompts_after_request = || {
		futures::executor::block_on(runtime.get_account(
			&CallContext::default(),
			HostAccountGetRequest::V1(v01::HostAccountGetRequest {
				product_account_id: v01::ProductAccountId {
					dot_ns_identifier: ProductId::for_test("peopl.dot"),
					derivation_index: v01::DerivationIndex::Index(0),
				},
			}),
		))
		.unwrap();
		platform.account_access_reviews.lock().unwrap().len()
	};
	let mut prompts = vec![prompts_after_request(), prompts_after_request()];
	futures::executor::block_on(authority.disconnect());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
	prompts.push(prompts_after_request());
	futures::executor::block_on(authority.activate_local_session(vec![0xCD; 16])).unwrap();
	prompts.push(prompts_after_request());
	authority.clear_product_state(&ProductId::for_test("myapp.dot"));
	prompts.push(prompts_after_request());
	assert_eq!(prompts, vec![1, 1, 2, 3, 4]);
}

/// Senders seal `Static` cheques to the user's chat key, so the static receive
/// secret is that key, and there is none while signed out.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_static_receive_secret_is_the_users_chat_key() {
	use crate::{
		host_logic::{
			sso::pairing::derive_identity_chat_private_key, x25519_seal::X25519PrivateKey,
		},
		nft_purse::StaticReceiveSecret,
	};

	let (_services, authority) = signing_runtime_with_platform(Arc::new(StubPlatform::default()));
	let signed_out = authority.static_receive_secret().map(|secret| secret.public_key());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();

	let signed_in = authority.static_receive_secret().unwrap().public_key();

	let chat_key = X25519PrivateKey::from(derive_identity_chat_private_key(&ENTROPY));
	assert_eq!(signed_in, chat_key.public_key());
	assert!(matches!(signed_out, Err(AuthorityError::Disconnected)));
}
