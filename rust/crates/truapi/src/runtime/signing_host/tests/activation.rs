use super::*;

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
		futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
		let session = authority.current_session().unwrap();
		let runtime = product_runtime(services, authority.clone());
		let cx = CallContext::default();
		let account = runtime.get_account(
			&cx,
			HostAccountGetRequest::V1(v01::HostAccountGetRequest {
				product_account_id: v01::ProductAccountId {
					dot_ns_identifier: "peopl.dot".to_string(),
					derivation_index: v01::DerivationIndex::Index(0),
				},
			}),
		);
		futures::pin_mut!(account);
		assert!(account.as_mut().now_or_never().is_none());
		match change {
			"lock" => futures::executor::block_on(authority.disconnect()),
			"reactivate" => {
				futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
					.unwrap();
			},
			"reset" => {
				futures::executor::block_on(authority.clear_product_state("myapp.dot")).unwrap()
			},
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
				authority.current_session() == Some(session),
				platform.account_access_reviews.lock().unwrap().len(),
			),
			(expected, change == "reset", 1),
			"{change}",
		);
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
	futures::executor::block_on(authority.clear_product_state("myapp.dot")).unwrap();
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
					dot_ns_identifier: "peopl.dot".to_string(),
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
	futures::executor::block_on(authority.clear_product_state("myapp.dot")).unwrap();
	prompts.push(prompts_after_request());
	assert_eq!(prompts, vec![1, 1, 2, 3, 4]);
}
