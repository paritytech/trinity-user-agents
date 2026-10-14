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
	authority.clear_product_state("myapp.dot").unwrap();
	prompts.push(prompts_after_request());
	assert_eq!(prompts, vec![1, 1, 2, 3, 4]);
}
