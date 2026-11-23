use super::*;
use crate::{host_logic::x25519_seal::X25519PrivateKey, platform::AuthState};

/// A product reset concerns that product's grants, not the wallet: account
/// work already under way survives it, while a lock or a wallet change ends it.
#[test]
fn pending_account_access_tracks_wallet_activation() {
	use futures::FutureExt;
	use truapi::versioned::account::HostAccountGetError;

	for change in ["lock", "reactivate", "reset"] {
		let (release, gate) = futures::channel::oneshot::channel();
		let platform = Arc::new(StubPlatform {
			account_access_confirmation_gate: std::sync::Mutex::new(Some(gate)),
			permission_confirmation_decisions: std::sync::Mutex::new(
				[crate::platform::PermissionDecision::AllowOnce].into(),
			),
			..StubPlatform::default()
		});
		let (services, authority) = signing_runtime_with_platform(platform.clone());
		futures::executor::block_on(authority.activate_local_session(RootEntropy::new(ENTROPY)))
			.unwrap();
		let session = authority.accounts().current_session().unwrap();
		let runtime = product_runtime(services, authority.clone());
		let cx = CallContext::default();
		let account = runtime.get_account(
			&cx,
			HostAccountGetRequest::V1(v01::HostAccountGetRequest {
				product_account_id: v01::ProductAccountId {
					dot_ns_identifier: ProductId::for_test("peopl.dot"),
					derivation_index: v01::DerivationIndex::Index(0),
				},
			}),
		);
		futures::pin_mut!(account);
		assert!(account.as_mut().now_or_never().is_none());
		match change {
			"lock" => futures::executor::block_on(authority.disconnect()),
			"reactivate" => {
				futures::executor::block_on(
					authority.activate_local_session(RootEntropy::new(ENTROPY)),
				)
				.unwrap();
			},
			"reset" => authority.clear_product_state(&ProductId::for_test("myapp.dot")),
			_ => unreachable!(),
		}
		release.send(()).unwrap();
		let expected = if change == "reset" {
			Ok(())
		} else {
			Err(CallError::Domain(HostAccountGetError::V1(v01::HostAccountGetError::NotConnected)))
		};
		assert_eq!(
			(
				futures::executor::block_on(account).map(|_| ()),
				authority.accounts().current_session().map(|current| current.validation_id) ==
					Some(session.validation_id),
				platform.account_access_reviews.lock().unwrap().len(),
			),
			(expected, change == "reset", 1),
			"{change}",
		);
	}
}

/// A signing host answers pairing proposals with its install-wide key, so the
/// paired devices address it by that key: the host must project and serve the
/// same one, with no paired wallet device of its own to name.
#[test]
fn a_local_session_projects_the_install_wide_device_key() {
	let platform = Arc::new(StubPlatform::default());
	let (_, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(authority.activate_local_session(RootEntropy::new(ENTROPY)))
		.unwrap();

	let install_secret = futures::executor::block_on(authority.device_encryption_secret()).unwrap();
	let auth_states = platform.auth_states.lock().unwrap();
	let Some(AuthState::Connected(ui_info)) = auth_states.last() else {
		panic!("activation should end connected: {auth_states:?}");
	};
	assert_eq!(
		(
			ui_info.device_enc_public_key,
			ui_info.peer_device_enc_public_key,
			futures::executor::block_on(authority.accounts().device_encryption_secret()),
		),
		(
			Some(X25519PrivateKey::from(install_secret).public_key().0),
			None,
			Ok(Some(install_secret)),
		)
	);
}

/// An active session's `AuthState::Connected` already names the device key,
/// and contacts encrypt to it, so the key cannot change under that session.
#[test]
fn an_active_session_keeps_its_device_key() {
	let (_, authority) = signing_runtime_with_platform(Arc::new(StubPlatform::default()));
	futures::executor::block_on(authority.activate_local_session(RootEntropy::new(ENTROPY)))
		.unwrap();
	let install_secret = futures::executor::block_on(authority.device_encryption_secret()).unwrap();

	let adoption = futures::executor::block_on(authority.adopt_device_encryption_secret([9; 32]));

	assert_eq!(
		(adoption.is_err(), futures::executor::block_on(authority.device_encryption_secret())),
		(true, Ok(install_secret))
	);
}

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
	futures::executor::block_on(authority.activate_local_session(RootEntropy::new(ENTROPY)))
		.unwrap();
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
	futures::executor::block_on(authority.activate_local_session(RootEntropy::new(ENTROPY)))
		.unwrap();
	prompts.push(prompts_after_request());
	futures::executor::block_on(authority.activate_local_session(RootEntropy::new([0xCD; 16])))
		.unwrap();
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
		host_logic::{root_entropy::RootEntropy, x25519_seal::X25519PrivateKey},
		nft_purse::StaticReceiveSecret,
	};

	let (_services, authority) = signing_runtime_with_platform(Arc::new(StubPlatform::default()));
	let signed_out = authority.static_receive_secret().map(|secret| secret.public_key());
	futures::executor::block_on(authority.activate_local_session(RootEntropy::new(ENTROPY)))
		.unwrap();

	let signed_in = authority.static_receive_secret().unwrap().public_key();

	let chat_key =
		X25519PrivateKey::from(RootEntropy::new(ENTROPY).derive_identity_chat_private_key());
	assert_eq!(signed_in, chat_key.public_key());
	assert!(matches!(signed_out, Err(AuthorityError::Disconnected)));
}

/// A host signs its own writes as the active identity through the runtime,
/// so it keeps no entropy once the session is active.
#[test]
fn identity_signer_signs_as_the_active_identity_and_needs_a_session() {
	use subxt::tx::Signer;

	let (_, authority) = signing_runtime_with_platform(Arc::new(StubPlatform::default()));
	let without_session = authority.identity_signer().ok().map(|signer| signer.account_id().0);
	futures::executor::block_on(authority.activate_local_session(RootEntropy::new(ENTROPY)))
		.unwrap();
	let identity = RootEntropy::new(ENTROPY).derive_identity_keypair(TEST_NETWORK_SUFFIX).unwrap();

	assert_eq!(
		(without_session, authority.identity_signer().unwrap().account_id().0),
		(None, identity.public.to_bytes())
	);
}
