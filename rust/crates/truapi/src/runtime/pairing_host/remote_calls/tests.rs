use super::*;
use crate::{
	host_internal::sso_messages::{RemoteMessage, RemoteMessageData, Response, v1},
	runtime::{ProductRuntimeHost, authority::AuthorityCancelError},
	test_support::{
		StubPlatform, runtime_config, sso_session_info, sso_success_response_script, test_spawner,
	},
};
use std::{sync::Arc, time::Duration};
use truapi::{CancellationReason, CancellationToken};

#[test]
fn unexpected_resource_reasons_include_only_safe_discriminants() {
	let resource = SsoAllocatedResource::AutoSigning {
		product_root_private_key: [0xA5; 64],
		ring_vrf_domain_entropy: [0x5A; 32],
	};

	let AuthorityError::Unknown { reason } =
		unexpected_resource("statement-store allowance", &resource)
	else {
		panic!("expected an unknown authority error");
	};

	assert_eq!(reason, "Unexpected statement-store allowance response resource: auto-signing");
	assert!(!reason.contains("165, 165"));
}

#[test]
fn product_account_allocation_neither_creates_nor_replaces_delegated_allowance_keys() {
	for delegated in [false, true] {
		let session = sso_session_info();
		let outcomes = vec![
			SsoAllocationOutcome::Allocated(SsoAllocatedResource::ProductStatementStoreAllowance),
			SsoAllocationOutcome::Rejected,
			SsoAllocationOutcome::NotAvailable,
			SsoAllocationOutcome::Allocated(SsoAllocatedResource::SmartContractAllowance),
		];
		let platform = Arc::new(StubPlatform {
			sso_response_script: Some(sso_success_response_script(
				&session,
				RemoteMessage {
					message_id: "wallet-product-allocation".to_string(),
					data: RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(
						Response {
							responding_to: "product-allocation".to_string(),
							payload: Ok(outcomes),
						},
					)),
				},
			)),
			..Default::default()
		});
		let (config, product) = runtime_config("myapp.dot");
		let (host, pairing_host) =
			ProductRuntimeHost::new_pairing_for_tests(platform, config, product, test_spawner());
		host.test_session_state().set_session(session.clone());
		let revision = pairing_host.grants.lifecycle().revision();
		let product_id = ProductId::for_test("myapp.dot");
		let other_product = ProductId::for_test("other.dot");
		let secret =
			crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16])
				.unwrap()
				.secret
				.to_bytes();

		futures::executor::block_on(async {
			for owner in [&product_id, &other_product] {
				if delegated || owner == &other_product {
					pairing_host
						.retain_grant(
							&CallContext::default(),
							&session,
							revision,
							owner,
							decode_grant(SsoAllocatedResource::StatementStoreAllowance {
								slot_account_key: secret.to_vec(),
							})
							.unwrap(),
						)
						.await
						.unwrap();
				}
			}
			let mapped = pairing_host
				.remote_allocate_resources(
					&CallContext::with_request_id("product-allocation".to_string()),
					&session,
					revision,
					&product_id,
					latest::HostRequestResourceAllocationRequest {
						resources: vec![
							latest::AllocatableResource::ProductStatementStoreAllowance(
								latest::DerivationIndex::Index(0),
							),
							latest::AllocatableResource::AutoSigning,
							latest::AllocatableResource::BulletinAllowance,
							latest::AllocatableResource::SmartContractAllowance(
								latest::DerivationIndex::Index(0),
							),
						],
					},
				)
				.await
				.unwrap();
			assert_eq!(
				mapped.outcomes,
				vec![
					latest::AllocationOutcome::Allocated,
					latest::AllocationOutcome::Rejected,
					latest::AllocationOutcome::NotAvailable,
					latest::AllocationOutcome::Allocated,
				],
			);
			for (owner, expected) in
				[(&product_id, delegated.then_some(secret)), (&other_product, Some(secret))]
			{
				let cached = pairing_host
					.grants
					.cached_allowance(&session, revision, owner, AllowanceResource::StatementStore)
					.await
					.unwrap();
				assert_eq!(cached.map(|allowance| *allowance.as_secret_bytes()), expected);
			}
		});
	}
}

#[test]
fn remote_payment_and_chat_preserve_authority_cancellation() {
	for reason in [
		CancellationReason::Cancelled,
		CancellationReason::TimedOut { timeout: Duration::from_secs(30) },
	] {
		let platform = Arc::new(StubPlatform::default());
		let (config, product) = runtime_config("myapp.dot");
		let (host, pairing_host) =
			ProductRuntimeHost::new_pairing_for_tests(platform, config, product, test_spawner());
		let session = sso_session_info();
		host.test_session_state().set_session(session.clone());
		let cancel = CancellationToken::default();
		cancel.cancel_with_reason(reason.clone());
		let cx = CallContext::with_parts("cancelled-remote-call".to_string(), cancel);
		let expected =
			AuthorityError::Cancelled(AuthorityCancelError::new(cx.request_id(), reason));

		futures::executor::block_on(async {
			let payment_error = pairing_host
				.remote_payment_top_up(
					&cx,
					&session,
					PaymentTopUpRequest {
						calling_product_id: ProductId::for_test("myapp.dot"),
						payload: truapi::versioned::payment::HostPaymentTopUpRequest::V2(
							latest::HostPaymentTopUpRequest {
								into: None,
								amount: 1,
								source: latest::PaymentTopUpSource::Coins {
									sr25519_secret_keys: Vec::new(),
								},
								id: [1; 32],
								funding: None,
							},
						),
					},
				)
				.await
				.unwrap_err();
			assert_eq!(payment_error, PaymentTopUpAuthorityError::Authority(expected.clone()));

			let chat_error = pairing_host
				.remote_product_device_chat(
					&cx,
					&session,
					ProductDeviceChatAuthorityRequest {
						calling_product_id: ProductId::for_test("myapp.dot"),
						operation: latest::HostProductDeviceChatRequest::Initialize,
						session_consent: false,
					},
				)
				.await
				.unwrap_err();
			assert_eq!(
				chat_error,
				ProductDeviceChatAuthorityError::Unavailable(expected.to_string()),
			);
		});
	}
}

#[test]
fn remote_allocation_keeps_delegated_keys_private_and_product_scoped() {
	let session = sso_session_info();
	let product_id = ProductId::for_test("myapp.dot");
	let other_product = ProductId::for_test("other.dot");
	let statement_key =
		crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16])
			.unwrap()
			.secret
			.to_bytes();
	let bulletin_key =
		crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xCD; 16])
			.unwrap()
			.secret
			.to_bytes();
	let platform = Arc::new(StubPlatform {
		sso_response_script: Some(sso_success_response_script(
			&session,
			RemoteMessage {
				message_id: "wallet-delegated-allocation".to_string(),
				data: RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(
					Response {
						responding_to: "delegated-allocation".to_string(),
						payload: Ok(vec![
							SsoAllocationOutcome::Allocated(
								SsoAllocatedResource::StatementStoreAllowance {
									slot_account_key: statement_key.to_vec(),
								},
							),
							SsoAllocationOutcome::Rejected,
							SsoAllocationOutcome::Allocated(
								SsoAllocatedResource::BulletinAllowance {
									slot_account_key: bulletin_key.to_vec(),
								},
							),
							SsoAllocationOutcome::NotAvailable,
						]),
					},
				)),
			},
		)),
		..Default::default()
	});
	let (config, product) = runtime_config("myapp.dot");
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		config,
		product,
		test_spawner(),
	);
	host.test_session_state().set_session(session.clone());
	let revision = pairing_host.grants.lifecycle().revision();

	futures::executor::block_on(async {
		let response = pairing_host
			.remote_allocate_resources(
				&CallContext::with_request_id("delegated-allocation".to_string()),
				&session,
				revision,
				&product_id,
				latest::HostRequestResourceAllocationRequest {
					resources: vec![
						latest::AllocatableResource::StatementStoreAllowance,
						latest::AllocatableResource::AutoSigning,
						latest::AllocatableResource::BulletinAllowance,
						latest::AllocatableResource::SmartContractAllowance(
							latest::DerivationIndex::Index(0),
						),
					],
				},
			)
			.await
			.unwrap();
		assert_eq!(
			response.outcomes,
			vec![
				latest::AllocationOutcome::Allocated,
				latest::AllocationOutcome::Rejected,
				latest::AllocationOutcome::Allocated,
				latest::AllocationOutcome::NotAvailable,
			],
		);

		let (config, product) = runtime_config("myapp.dot");
		let (restored_host, restored_pairing_host) =
			ProductRuntimeHost::new_pairing_for_tests(platform, config, product, test_spawner());
		restored_host.test_session_state().set_session(session.clone());
		for pairing in [&pairing_host, &restored_pairing_host] {
			let revision = pairing.grants.lifecycle().revision();
			for (resource, expected) in [
				(AllowanceResource::StatementStore, statement_key),
				(AllowanceResource::Bulletin, bulletin_key),
			] {
				let cached = pairing
					.grants
					.cached_allowance(&session, revision, &product_id, resource)
					.await
					.unwrap()
					.expect("allocated key is retained in memory and storage");
				assert_eq!(cached.as_secret_bytes(), &expected);
				assert!(
					pairing
						.grants
						.cached_allowance(&session, revision, &other_product, resource)
						.await
						.unwrap()
						.is_none(),
					"allocation must not grant another product authority",
				);
			}
		}
	});
}

#[test]
fn remote_allocation_rejects_a_product_revision_revoked_before_submission() {
	let platform = Arc::new(StubPlatform::default());
	let (config, product) = runtime_config("myapp.dot");
	let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
		platform.clone(),
		config,
		product,
		test_spawner(),
	);
	let session = sso_session_info();
	host.test_session_state().set_session(session.clone());
	let product_id = ProductId::for_test("myapp.dot");
	let revision = pairing_host.grants.lifecycle().revision();

	futures::executor::block_on(async {
		pairing_host.clear_product_state(&product_id).await.unwrap();
		let error = pairing_host
			.remote_allocate_resources(
				&CallContext::default(),
				&session,
				revision,
				&product_id,
				latest::HostRequestResourceAllocationRequest {
					resources: vec![latest::AllocatableResource::StatementStoreAllowance],
				},
			)
			.await
			.unwrap_err();
		assert_eq!(error, AuthorityError::Disconnected);
		assert!(platform.sent_rpc.lock().unwrap().is_empty());
	});
}
