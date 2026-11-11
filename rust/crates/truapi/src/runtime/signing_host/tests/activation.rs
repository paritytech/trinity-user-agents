use super::*;

/// A product reset withdraws its pending consent without ending the wallet
/// session; a lock or reactivation also invalidates the original session.
#[test]
fn pending_account_access_tracks_wallet_activation() {
	use crate::platform::{PermissionAuthorizationStatus, PermissionDecision};
	use futures::FutureExt;
	use truapi::versioned::account::HostAccountGetError;

	for change in ["lock", "reactivate", "switch", "reset"] {
		for decision in [PermissionDecision::AllowOnce, PermissionDecision::AllowAlways] {
			let (release, gate) = futures::channel::oneshot::channel();
			let platform = Arc::new(StubPlatform {
				account_access_confirmation_gate: std::sync::Mutex::new(Some(gate)),
				permission_confirmation_decisions: std::sync::Mutex::new([decision].into()),
				..StubPlatform::default()
			});
			let (services, authority) = signing_runtime_with_platform(platform.clone());
			futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
				.unwrap();
			let session = authority.current_session().unwrap();
			let runtime = product_runtime(services, authority.clone());
			let request = || {
				HostAccountGetRequest::V1(v01::HostAccountGetRequest {
					product_account_id: v01::ProductAccountId {
						dot_ns_identifier: ProductId::for_test("peopl.dot"),
						derivation_index: v01::DerivationIndex::Index(0),
					},
				})
			};
			let cx = CallContext::default();
			let account = runtime.get_account(&cx, request());
			futures::pin_mut!(account);
			assert!(account.as_mut().now_or_never().is_none());
			match change {
				"lock" => futures::executor::block_on(authority.disconnect()),
				"reactivate" => {
					futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
						.unwrap();
				},
				"switch" => {
					futures::executor::block_on(authority.activate_local_session(vec![0xCD; 16]))
						.unwrap();
				},
				"reset" => futures::executor::block_on(
					authority.clear_product_state(&ProductId::for_test("myapp.dot")),
				)
				.unwrap(),
				_ => unreachable!(),
			}
			release.send(()).unwrap();
			let expected = if change == "reset" {
				v01::HostAccountGetError::Rejected
			} else {
				v01::HostAccountGetError::NotConnected
			};
			assert_eq!(
				(
					futures::executor::block_on(account).map(|_| ()),
					authority.current_session().map(|current| current.validation_id) ==
						Some(session.validation_id),
					platform.account_access_reviews.lock().unwrap().len(),
				),
				(Err(CallError::Domain(HostAccountGetError::V1(expected))), change == "reset", 1),
				"{change}, {decision:?}",
			);
			let stored = futures::executor::block_on(
				crate::host_internal::permissions::authorization_status(
					platform.as_ref(),
					CoreStorageKey::product_account_access_authorization(
						&ProductId::for_test("myapp.dot"),
						&ProductId::for_test("peopl.dot"),
					),
				),
			)
			.unwrap();
			assert_eq!(stored, PermissionAuthorizationStatus::NotDetermined);

			// No late answer may authorize a new request, even with the same wallet.
			if change == "lock" {
				futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
					.unwrap();
			}
			*platform.permission_confirmation_decisions.lock().unwrap() =
				[PermissionDecision::Deny].into();
			assert_eq!(
				futures::executor::block_on(runtime.get_account(&cx, request())).map(|_| ()),
				Err(CallError::Domain(HostAccountGetError::V1(v01::HostAccountGetError::Rejected))),
			);
			assert_eq!(platform.account_access_reviews.lock().unwrap().len(), 2);
		}
	}
}

/// A call can capture its session just before activation changes, then first
/// enter consent under the new permission revision. That revision is not authority.
#[test]
fn account_consent_rejects_an_activation_replaced_before_its_first_poll() {
	use crate::{
		platform::{PermissionAuthorizationStatus, PermissionDecision},
		runtime::product_consent::AccountAccessError,
	};

	for retained in [false, true] {
		for decision in [PermissionDecision::AllowOnce, PermissionDecision::AllowAlways] {
			let platform = Arc::new(StubPlatform {
				permission_confirmation_decisions: std::sync::Mutex::new([decision].into()),
				..StubPlatform::default()
			});
			let (_services, authority) = signing_runtime_with_platform(platform.clone());
			futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
				.unwrap();
			let session = authority.current_session().unwrap();
			let requester = ProductId::for_test("myapp.dot");
			let target = ProductId::for_test("peopl.dot");
			let is_current = || authority.session_is_current(&session, None);
			let consent = authority.consent();
			let pending = async {
				if retained {
					consent
						.account_access(&requester, &target, platform.as_ref(), &is_current)
						.await
				} else {
					consent
						.account_disclosure(&requester, &target, platform.as_ref(), &is_current)
						.await
				}
			};

			futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
				.unwrap();
			assert!(matches!(
				futures::executor::block_on(pending),
				Err(AccountAccessError::Superseded),
			));
			assert!(platform.account_access_reviews.lock().unwrap().is_empty());
			assert_eq!(
				futures::executor::block_on(
					crate::host_internal::permissions::authorization_status(
						platform.as_ref(),
						CoreStorageKey::product_account_access_authorization(&requester, &target),
					),
				)
				.unwrap(),
				PermissionAuthorizationStatus::NotDetermined,
			);

			let current = authority.current_session().unwrap();
			assert_eq!(
				futures::executor::block_on(consent.account_access(
					&requester,
					&target,
					platform.as_ref(),
					|| authority.session_is_current(&current, None),
				))
				.unwrap(),
				PermissionAuthorizationStatus::Authorized,
			);
			assert_eq!(platform.account_access_reviews.lock().unwrap().len(), 1);
		}
	}
}

#[test]
fn product_reset_during_allocation_review_cannot_restore_native_grants() {
	use futures::FutureExt;

	let (release, gate) = futures::channel::oneshot::channel();
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		resource_allocation_confirmation_gate: std::sync::Mutex::new(Some(gate)),
		..StubPlatform::default()
	});
	let (services, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
	let runtime = product_runtime(services, authority.clone());
	let cx = CallContext::default();
	let allocation = ResourceAllocation::request(
		&runtime,
		&cx,
		HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
			resources: vec![v01::AllocatableResource::AutoSigning],
		}),
	);
	futures::pin_mut!(allocation);
	assert!(allocation.as_mut().now_or_never().is_none());
	futures::executor::block_on(authority.clear_product_state(&ProductId::for_test("myapp.dot")))
		.unwrap();
	release.send(()).unwrap();
	let result = futures::executor::block_on(allocation);
	let status = keeps_auto_signing(&authority, "myapp.dot");
	assert_eq!(
		(result.map(|_| ()), status, platform.resource_allocation_reviews.lock().unwrap().len()),
		(
			Err(CallError::Domain(
				truapi::versioned::resource_allocation::HostRequestResourceAllocationError::V1(
					v01::ResourceAllocationError::Unknown {
						reason: AuthorityError::Disconnected.to_string()
					}
				)
			)),
			false,
			1
		),
	);
}

/// Epoca's account disclosure consumes Allow once for that call. Ring-key access
/// retains it for the current activation, but neither survives lifecycle changes.
#[test]
fn account_access_allow_once_obeys_disclosure_and_activation_lifetimes() {
	for retained in [false, true] {
		let platform = Arc::new(StubPlatform {
			permission_confirmation_decisions: std::sync::Mutex::new(
				[crate::platform::PermissionDecision::AllowOnce; 5].into(),
			),
			..StubPlatform::default()
		});
		let (services, authority) = signing_runtime_with_platform(platform.clone());
		futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
		let runtime = product_runtime(services, authority.clone());
		let prompts_after_request = || {
			if retained {
				let session = authority.current_session().unwrap();
				assert_eq!(
					futures::executor::block_on(authority.consent().account_access(
						&ProductId::for_test("myapp.dot"),
						&ProductId::for_test("peopl.dot"),
						platform.as_ref(),
						|| authority.session_is_current(&session, None),
					))
					.unwrap(),
					crate::platform::PermissionAuthorizationStatus::Authorized,
				);
			} else {
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
			}
			platform.account_access_reviews.lock().unwrap().len()
		};
		let mut prompts = vec![prompts_after_request(), prompts_after_request()];
		futures::executor::block_on(authority.disconnect());
		futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
		prompts.push(prompts_after_request());
		futures::executor::block_on(authority.activate_local_session(vec![0xCD; 16])).unwrap();
		prompts.push(prompts_after_request());
		futures::executor::block_on(
			authority.clear_product_state(&ProductId::for_test("myapp.dot")),
		)
		.unwrap();
		prompts.push(prompts_after_request());
		assert_eq!(prompts, if retained { vec![1, 1, 2, 3, 4] } else { vec![1, 2, 3, 4, 5] },);
	}
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
