use super::*;
use crate::{
	host_internal::{
		permissions::set_authorization_status,
		sso_messages::{RemoteMessage, RemoteMessageData, v1},
	},
	platform::{PermissionAuthorizationStatus, PermissionDecision},
	runtime::{signing_host::SigningHostSsoService, sso_service::Dispatch},
};
use truapi::latest::{HostAccountListRingVrfKeysRequest, RingVrfKeyDisclosure};

#[test]
fn remote_account_access_reuses_shared_decisions_and_preserves_their_lifetime() {
	for operation in ["alias", "list"] {
		for (stored, decision, expected_status, prompts) in [
			(
				Some(PermissionAuthorizationStatus::Authorized),
				PermissionDecision::Deny,
				PermissionAuthorizationStatus::Authorized,
				0,
			),
			(
				Some(PermissionAuthorizationStatus::Denied),
				PermissionDecision::AllowAlways,
				PermissionAuthorizationStatus::Denied,
				0,
			),
			(None, PermissionDecision::AllowAlways, PermissionAuthorizationStatus::Authorized, 1),
			(None, PermissionDecision::AllowOnce, PermissionAuthorizationStatus::NotDetermined, 1),
			(None, PermissionDecision::Deny, PermissionAuthorizationStatus::Denied, 1),
		] {
			let platform = Arc::new(StubPlatform {
				permission_confirmation_decisions: std::sync::Mutex::new(
					[decision, decision].into(),
				),
				..StubPlatform::default()
			});
			cache_grant(&platform, "peopl.dot", "{}");
			let (_, authority) =
				signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
			futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
				.unwrap();
			let session = authority.current_session().unwrap();
			register_full_person_key(&authority, &session, &full_person_ring_location());
			let owner =
				ProductId::for_test(if operation == "alias" { "peopl.dot" } else { "foreign.dot" });
			if let Some(status) = stored {
				futures::executor::block_on(set_authorization_status(
					platform.as_ref(),
					CoreStorageKey::product_account_access_authorization(
						&ProductId::for_test("myapp.dot"),
						&owner,
					),
					status,
				))
				.unwrap();
			}
			let service = SigningHostSsoService::new(authority.clone());
			let mut outcomes = Vec::new();
			for request_id in ["first", "second"] {
				let request = if operation == "alias" {
					RemoteMessage::request(
						request_id.to_string(),
						ProductRequest {
							calling_product_id: ProductId::for_test("myapp.dot"),
							payload: HostAccountGetAliasRequest {
								key_handle: full_person_key_handle(),
								context: v01::ProductProofContext {
									product_id: ProductId::for_test("myapp.dot"),
									suffix: v01::DerivationIndex::Index(0),
								},
								ring_location: full_person_ring_location(),
							},
						},
					)
				} else {
					RemoteMessage::request(
						request_id.to_string(),
						ProductRequest {
							calling_product_id: ProductId::for_test("myapp.dot"),
							payload: HostAccountListRingVrfKeysRequest {
								owner: ProductId::for_test("foreign.dot"),
								disclosure: RingVrfKeyDisclosure::PublicKey,
							},
						},
					)
				};
				let Dispatch::Response(answer) =
					futures::executor::block_on(service.answer(request))
				else {
					panic!("expected an account response")
				};
				outcomes.push(match answer.message.data {
					RemoteMessageData::V1(v1::RemoteMessage::GetAccountAliasResponse(response)) => {
						response.payload.map(|_| ())
					},
					RemoteMessageData::V1(v1::RemoteMessage::ListRingVrfKeysResponse(response)) => {
						response.payload.map(|_| ())
					},
					_ => panic!("unexpected account response"),
				});
			}
			let expected = if expected_status == PermissionAuthorizationStatus::Denied {
				Err(RingVrfError::Rejected)
			} else {
				Ok(())
			};
			assert_eq!(
				(
					outcomes,
					platform.account_access_reviews.lock().unwrap().len(),
					futures::executor::block_on(
						crate::host_internal::permissions::authorization_status(
							platform.as_ref(),
							CoreStorageKey::product_account_access_authorization(
								&ProductId::for_test("myapp.dot"),
								&owner
							),
						)
					)
					.unwrap(),
				),
				(vec![expected.clone(), expected], prompts, expected_status),
				"{operation}, stored decision {stored:?}, answer {decision:?}",
			);
		}
	}
}

use crate::{
	host_internal::{
		permissions::PermissionsService,
		sso_messages::{ProductDeviceChatResponse, SsoProductDeviceChatOperation},
	},
	platform::PermissionAuthorizationRequest,
	runtime::authority::{ProductDeviceChatAuthorityError, ProductDeviceChatAuthorityRequest},
};
use truapi::versioned::account::{
	HostProductDeviceChatError, HostProductDeviceChatRequest, HostProductDeviceChatResponse,
};

#[test]
fn payment_top_up_requires_current_wallet_session_without_chat_grants() {
	use crate::{
		host_internal::sso_messages::PaymentTopUpRequest,
		runtime::authority::PaymentTopUpAuthorityError,
	};
	use truapi::{
		api::Payment,
		versioned::payment::{HostPaymentTopUpError, HostPaymentTopUpRequest},
	};

	futures::executor::block_on(async {
		let platform =
			Arc::new(StubPlatform { chain_connect_error: Some("offline"), ..Default::default() });
		let (services, authority) = signing_runtime_with_platform(platform.clone());
		let runtime = product_runtime(services, authority.clone());
		let request = || {
			HostPaymentTopUpRequest::V1(v01::HostPaymentTopUpRequest {
				id: [0x74; 32],
				into: None,
				amount: 1,
				source: v01::PaymentTopUpSource::ProductAccount {
					derivation_index: v01::DerivationIndex::Index(0),
				},
			})
		};
		assert!(matches!(
			runtime.top_up(&CallContext::default(), request()).await,
			Err(CallError::Domain(HostPaymentTopUpError::V1(
				v01::HostPaymentTopUpError::Unknown { .. }
			)))
		));
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let session = authority.current_session().unwrap();
		// An active wallet reaches source validation, not a Chat/signing prompt.
		assert_eq!(
			runtime.top_up(&CallContext::default(), request()).await,
			Err(CallError::Domain(HostPaymentTopUpError::V1(
				v01::HostPaymentTopUpError::InvalidSource
			)))
		);
		assert!(platform.chat_authority_reviews.lock().is_empty());
		assert!(platform.main_purse_chat_payment_reviews.lock().unwrap().is_empty());
		assert!(platform.sign_raw_reviews.lock().unwrap().is_empty());
		assert!(platform.sign_payload_reviews.lock().unwrap().is_empty());
		assert_eq!(platform.identity_disclosure_calls.load(std::sync::atomic::Ordering::SeqCst), 0);

		authority.disconnect().await;
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		assert_eq!(
			authority
				.payment_top_up(
					&CallContext::default(),
					&session,
					PaymentTopUpRequest {
						calling_product_id: ProductId::for_test("myapp.dot"),
						payload: request(),
					},
				)
				.await,
			Err(PaymentTopUpAuthorityError::Authority(AuthorityError::Disconnected))
		);
	});
}

#[test]
fn concurrent_implicit_allowance_approval_reuses_the_new_grant() {
	use futures::FutureExt;
	let (release, gate) = futures::channel::oneshot::channel();
	let platform =
		Arc::new(StubPlatform { resource_allocation_confirmed: true, ..Default::default() });
	*platform.resource_allocation_confirmation_gate.lock().expect("gate") = Some(gate);
	let (services, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(async {
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let runtime = product_runtime(services, authority.clone());
		let session = authority.current_session().unwrap();
		let first = runtime.require_statement_store_allowance(&session, None);
		futures::pin_mut!(first);
		assert!(first.as_mut().now_or_never().is_none());
		runtime.require_statement_store_allowance(&session, None).await.unwrap();
		release.send(()).unwrap();
		first.await.unwrap();
		assert_eq!(
			runtime
				.connection
				.permission_authorization_status(
					PermissionAuthorizationRequest::StatementStoreAllowance {
						derivation_index: None,
					},
				)
				.await
				.unwrap(),
			PermissionAuthorizationStatus::Authorized,
		);
	});
}

#[test]
fn statement_allowance_decisions_survive_runtime_restart_and_remain_scoped() {
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		chain_connect_error: Some("offline"),
		..Default::default()
	});
	let selector = Some(v01::DerivationIndex::Index(0));
	let request = PermissionAuthorizationRequest::StatementStoreAllowance {
		derivation_index: selector.clone(),
	};
	futures::executor::block_on(async {
		let (services, authority) = signing_runtime_with_platform(platform.clone());
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let first = product_runtime(services, authority.clone());
		let session = authority.current_session().unwrap();
		first
			.require_statement_store_allowance(&session, selector.clone())
			.await
			.unwrap();
		drop(first);
		drop(authority);

		let (services, authority) = signing_runtime_with_platform(platform.clone());
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let restarted = product_runtime(services.clone(), authority.clone());
		let session = authority.current_session().unwrap();
		restarted
			.require_statement_store_allowance(&session, selector.clone())
			.await
			.unwrap();
		assert_eq!(platform.resource_allocation_reviews.lock().unwrap().len(), 1);
		assert_eq!(
			restarted
				.connection
				.permission_authorization_status(request.clone())
				.await
				.unwrap(),
			PermissionAuthorizationStatus::Authorized
		);
		for separate in [
			PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index: None },
			PermissionAuthorizationRequest::StatementStoreAllowance {
				derivation_index: Some(v01::DerivationIndex::Index(1)),
			},
			PermissionAuthorizationRequest::ChatAuthority,
			PermissionAuthorizationRequest::IdentityDisclosure,
		] {
			assert_eq!(
				restarted.connection.permission_authorization_status(separate).await.unwrap(),
				PermissionAuthorizationStatus::NotDetermined
			);
		}
		let other = product_runtime_for(services.clone(), authority.clone(), "other.dot");
		assert_eq!(
			other.connection.permission_authorization_status(request.clone()).await.unwrap(),
			PermissionAuthorizationStatus::NotDetermined
		);

		// A connection with a different artifact store must not inherit a
		// decision even with the same product id and the same authority.
		let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
		adapters.platform = Arc::new(StubPlatform::default());
		let other_artifact = ProductRuntimeHost::from_services(
			services,
			adapters,
			authority.clone(),
			authority,
			ProductContext::new(ProductId::for_test("myapp.dot")),
		);
		assert_eq!(
			other_artifact
				.connection
				.permission_authorization_status(request.clone())
				.await
				.unwrap(),
			PermissionAuthorizationStatus::NotDetermined
		);

		restarted
			.connection
			.set_permission_authorization_status(
				request.clone(),
				PermissionAuthorizationStatus::Denied,
			)
			.await
			.unwrap();
		let result = ResourceAllocation::request(
			&restarted,
			&CallContext::default(),
			HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
				resources: vec![v01::AllocatableResource::ProductStatementStoreAllowance(
					v01::DerivationIndex::Index(0),
				)],
			}),
		)
		.await;
		assert!(result.is_err());
		assert!(platform.sent_rpc.lock().unwrap().is_empty());
		assert_eq!(platform.resource_allocation_reviews.lock().unwrap().len(), 1);
		assert_eq!(
			restarted
				.connection
				.permission_authorization_status(request.clone())
				.await
				.unwrap(),
			PermissionAuthorizationStatus::Denied
		);
		restarted
			.connection
			.set_permission_authorization_status(
				request,
				PermissionAuthorizationStatus::NotDetermined,
			)
			.await
			.unwrap();
		restarted.require_statement_store_allowance(&session, selector).await.unwrap();
		assert_eq!(platform.resource_allocation_reviews.lock().unwrap().len(), 2);
	});
}

#[test]
fn explicit_statement_increases_each_prompt_and_initial_approval_also_grants_ensure() {
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		chain_connect_error: Some("offline"),
		..Default::default()
	});
	let (services, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(async {
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let runtime = product_runtime(services, authority.clone());
		let session = authority.current_session().unwrap();
		for expected_reviews in 1..=2 {
			// Chain availability is independent of consent: a failed
			// provisioning attempt must not force a second grant prompt.
			ResourceAllocation::request(
				&runtime,
				&CallContext::default(),
				HostRequestResourceAllocationRequest::V1(
					v01::HostRequestResourceAllocationRequest {
						resources: vec![v01::AllocatableResource::StatementStoreAllowance],
					},
				),
			)
			.await
			.unwrap();
			assert_eq!(
				platform.resource_allocation_reviews.lock().expect("reviews").len(),
				expected_reviews
			);
			runtime.require_statement_store_allowance(&session, None).await.unwrap();
			assert_eq!(
				platform.resource_allocation_reviews.lock().expect("reviews").len(),
				expected_reviews
			);
		}
	});
}

#[test]
fn cancelling_an_explicit_increase_preserves_the_implicit_allowance_grant() {
	let platform = Arc::new(StubPlatform::default());
	let (services, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(async {
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let runtime = product_runtime(services, authority.clone());
		let grant =
			PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index: None };
		runtime
			.connection
			.set_permission_authorization_status(
				grant.clone(),
				PermissionAuthorizationStatus::Authorized,
			)
			.await
			.unwrap();
		let HostRequestResourceAllocationResponse::V1(response) = ResourceAllocation::request(
			&runtime,
			&CallContext::default(),
			HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
				resources: vec![v01::AllocatableResource::StatementStoreAllowance],
			}),
		)
		.await
		.expect("declining an increase returns its rejected outcome");
		assert_eq!(response.outcomes, vec![v01::AllocationOutcome::Rejected]);
		assert_eq!(
			runtime.connection.permission_authorization_status(grant).await.unwrap(),
			PermissionAuthorizationStatus::Authorized
		);
		runtime
			.require_statement_store_allowance(&authority.current_session().unwrap(), None)
			.await
			.unwrap();
		assert_eq!(platform.resource_allocation_reviews.lock().expect("reviews").len(), 1);
		assert!(platform.sent_rpc.lock().expect("rpc").is_empty());
	});
}

#[test]
fn administration_during_explicit_review_wins_over_the_confirmation() {
	use futures::FutureExt;
	for (before, administrative) in [
		(PermissionAuthorizationStatus::NotDetermined, PermissionAuthorizationStatus::Denied),
		(PermissionAuthorizationStatus::Authorized, PermissionAuthorizationStatus::NotDetermined),
	] {
		let (release, gate) = futures::channel::oneshot::channel();
		let platform =
			Arc::new(StubPlatform { resource_allocation_confirmed: true, ..Default::default() });
		*platform.resource_allocation_confirmation_gate.lock().expect("gate") = Some(gate);
		let (services, authority) = signing_runtime_with_platform(platform.clone());
		futures::executor::block_on(async {
			authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
			let runtime = product_runtime(services, authority);
			let grant =
				PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index: None };
			runtime
				.connection
				.set_permission_authorization_status(grant.clone(), before)
				.await
				.unwrap();
			let cx = CallContext::default();
			let allocation = ResourceAllocation::request(
				&runtime,
				&cx,
				HostRequestResourceAllocationRequest::V1(
					v01::HostRequestResourceAllocationRequest {
						resources: vec![v01::AllocatableResource::StatementStoreAllowance],
					},
				),
			);
			futures::pin_mut!(allocation);
			assert!(allocation.as_mut().now_or_never().is_none());
			runtime
				.connection
				.set_permission_authorization_status(grant.clone(), administrative)
				.await
				.unwrap();
			release.send(()).unwrap();
			assert!(allocation.await.is_err());
			assert_eq!(
				runtime.connection.permission_authorization_status(grant).await.unwrap(),
				administrative
			);
			assert!(platform.sent_rpc.lock().expect("rpc").is_empty());
		});
	}
}

#[test]
fn statement_allowance_storage_failure_never_prompts_or_allocates() {
	let platform = Arc::new(StubPlatform {
		local_storage_error: Some("storage unavailable"),
		resource_allocation_confirmed: true,
		..Default::default()
	});
	let (services, authority) = signing_runtime_with_platform(Arc::new(StubPlatform::default()));
	futures::executor::block_on(async {
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
		adapters.platform = platform.clone();
		let runtime = ProductRuntimeHost::from_services(
			services,
			adapters,
			authority.clone(),
			authority,
			ProductContext::new(ProductId::for_test("myapp.dot")),
		);
		let result = ResourceAllocation::request(
			&runtime,
			&CallContext::default(),
			HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
				resources: vec![v01::AllocatableResource::StatementStoreAllowance],
			}),
		)
		.await;
		assert!(result.is_err());
		assert!(platform.resource_allocation_reviews.lock().unwrap().is_empty());
		assert!(platform.sent_rpc.lock().unwrap().is_empty());
	});
}

#[test]
fn statement_consent_cannot_survive_same_account_reactivation() {
	use futures::FutureExt;
	for explicit in [false, true] {
		let (release, gate) = futures::channel::oneshot::channel();
		let platform =
			Arc::new(StubPlatform { resource_allocation_confirmed: true, ..Default::default() });
		*platform.resource_allocation_confirmation_gate.lock().expect("gate lock") = Some(gate);
		let (services, authority) = signing_runtime_with_platform(platform.clone());
		futures::executor::block_on(async {
			authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
			let runtime = product_runtime(services, authority.clone());
			let session = authority.current_session().unwrap();
			let consent = async {
				if explicit {
					ResourceAllocation::request(
						&runtime,
						&CallContext::default(),
						HostRequestResourceAllocationRequest::V1(
							v01::HostRequestResourceAllocationRequest {
								resources: vec![v01::AllocatableResource::StatementStoreAllowance],
							},
						),
					)
					.await
					.map(|_| ())
					.map_err(|error| format!("{error:?}"))
				} else {
					runtime.require_statement_store_allowance(&session, None).await
				}
			};
			futures::pin_mut!(consent);
			assert!(consent.as_mut().now_or_never().is_none());
			authority.disconnect().await;
			authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
			release.send(()).unwrap();
			assert!(consent.await.is_err());
			assert_eq!(
				runtime
					.connection
					.permission_authorization_status(
						PermissionAuthorizationRequest::StatementStoreAllowance {
							derivation_index: None
						},
					)
					.await
					.unwrap(),
				PermissionAuthorizationStatus::NotDetermined
			);
			assert!(platform.sent_rpc.lock().expect("rpc lock").is_empty());
		});
	}
}

#[test]
fn lock_and_wallet_replacement_release_native_custody_and_fence_old_authority() {
	futures::executor::block_on(async {
		use truapi::latest::{HostProductDeviceChatError, HostProductDeviceChatRequest};
		let (services, authority) = signing_runtime();
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		crate::host_internal::permissions::PermissionsService::new(
			services.platform.as_ref(),
			services.platform.as_ref(),
			&crate::platform::ProductContext::new_with_execution(
				ProductId::for_test("chat.dot"),
				crate::platform::ExecutableKind::Worker,
			),
		)
		.set_authorization_status(
			&PermissionAuthorizationRequest::ChatAuthority,
			PermissionAuthorizationStatus::Authorized,
		)
		.await
		.unwrap();
		let session = authority.current_session().unwrap();
		let context = authority.native_chat_context(&session).unwrap();
		let first = authority
			.native_chat
			.execute(
				context.clone(),
				ProductId::for_test("chat.dot"),
				HostProductDeviceChatRequest::Initialize,
			)
			.await
			.unwrap();
		let wallet = authority.native_chat.wallet(&context).await.unwrap();
		let lifetime = Arc::downgrade(&wallet);
		drop(wallet);
		authority.clear_local_session();
		assert!(lifetime.upgrade().is_none());
		assert_eq!(context.require_current(), Err(HostProductDeviceChatError::NotConnected));
		assert_eq!(
			authority
				.native_chat
				.execute(
					context,
					ProductId::for_test("chat.dot"),
					HostProductDeviceChatRequest::Initialize,
				)
				.await,
			Err(HostProductDeviceChatError::NotConnected)
		);

		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let session = authority.current_session().unwrap();
		let context = authority.native_chat_context(&session).unwrap();
		let restored = authority
			.native_chat
			.execute(
				context.clone(),
				ProductId::for_test("chat.dot"),
				HostProductDeviceChatRequest::Initialize,
			)
			.await
			.unwrap();
		assert_eq!(restored.device, first.device);
		let wallet = authority.native_chat.wallet(&context).await.unwrap();
		let lifetime = Arc::downgrade(&wallet);
		drop(wallet);
		authority.activate_local_session(vec![0xCD; 16]).await.unwrap();
		assert!(lifetime.upgrade().is_none());
		assert_eq!(context.require_current(), Err(HostProductDeviceChatError::NotConnected));
	});
}

#[test]
fn product_review_revalidation_preserves_unrelated_work_and_fences_revocation() {
	use futures::FutureExt;

	for change in AuthorityChange::ALL {
		let (release, gate) = futures::channel::oneshot::channel();
		let platform =
			Arc::new(StubPlatform { resource_allocation_confirmed: true, ..Default::default() });
		*platform.resource_allocation_confirmation_gate.lock().unwrap() = Some(gate);
		let (services, authority) = signing_runtime_with_platform(platform);
		futures::executor::block_on(async {
			authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
			let grant_owner = authority.session_state().current().unwrap();
			let runtime = product_runtime(services, authority.clone());
			let cx = CallContext::default();
			let allocation = ResourceAllocation::request(
				&runtime,
				&cx,
				HostRequestResourceAllocationRequest::V1(
					v01::HostRequestResourceAllocationRequest {
						resources: vec![v01::AllocatableResource::AutoSigning],
					},
				),
			);
			futures::pin_mut!(allocation);
			assert!(allocation.as_mut().now_or_never().is_none());
			change.apply(&authority).await;
			release.send(()).unwrap();
			let result = allocation.await;
			if matches!(change, AuthorityChange::ClearOtherProduct) {
				assert_eq!(
					result.unwrap(),
					HostRequestResourceAllocationResponse::V1(
						v01::HostRequestResourceAllocationResponse {
							outcomes: vec![v01::AllocationOutcome::Allocated],
						},
					),
				);
			} else {
				assert!(result.is_err(), "{change:?}: {result:?}");
				assert!(
					authority
						.grants
						.auto_signing_key(&grant_owner, &ProductId::for_test("myapp.dot"),)
						.await
						.expect("kept keys are readable")
						.is_none()
				);
			}
		});
	}
}

#[test]
fn chat_context_rejects_logout_and_same_wallet_reactivation() {
	futures::executor::block_on(async {
		let (_, authority) = signing_runtime();
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let session = authority.current_session().unwrap();
		let context = authority.native_chat_context(&session).unwrap();
		assert!((context.session_valid)());
		authority.disconnect().await;
		assert!(!(context.session_valid)());
		authority.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		assert!(!(context.session_valid)());
		assert!(matches!(
			authority
				.product_device_chat(
					&CallContext::default(),
					&session,
					ProductDeviceChatAuthorityRequest {
						calling_product_id: ProductId::for_test("myapp.dot"),
						operation: truapi::latest::HostProductDeviceChatRequest::Initialize,
						session_consent: false,
					},
				)
				.await,
			Err(ProductDeviceChatAuthorityError::Disconnected)
		));
		let current = authority.current_session().unwrap();
		assert!((authority.native_chat_context(&current).unwrap().session_valid)());
	});
}

#[test]
fn product_chat_username_grant_does_not_authorize_chat() {
	futures::executor::block_on(async {
		let platform = Arc::new(StubPlatform::default());
		let (services, activation) = signing_runtime_with_platform(platform.clone());
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let runtime = product_runtime(services, activation);
		runtime
			.connection
			.set_permission_authorization_status(
				PermissionAuthorizationRequest::IdentityDisclosure,
				PermissionAuthorizationStatus::Authorized,
			)
			.await
			.unwrap();
		let cx = CallContext::default();
		let request = HostProductDeviceChatRequest::V2(
			truapi::latest::HostProductDeviceChatRequest::Initialize,
		);

		for _ in 0..2 {
			assert!(matches!(
				runtime.product_device_chat(&cx, request.clone()).await,
				Err(CallError::Domain(HostProductDeviceChatError::V1(
					truapi::latest::HostProductDeviceChatError::AccessNotGranted
				)))
			));
		}
		assert_eq!(
			platform.chat_authority_reviews.lock().len(),
			1,
			"Chat requires its own prompt, then respects the persisted refusal"
		);
		assert_eq!(
			runtime
				.connection
				.permission_authorization_status(PermissionAuthorizationRequest::ChatAuthority)
				.await
				.unwrap(),
			PermissionAuthorizationStatus::Denied
		);
		assert_eq!(
			runtime
				.connection
				.permission_authorization_status(PermissionAuthorizationRequest::IdentityDisclosure,)
				.await
				.unwrap(),
			PermissionAuthorizationStatus::Authorized
		);
	});
}

#[test]
fn product_chat_consent_is_cached_and_revocation_blocks_chat() {
	futures::executor::block_on(async {
		let platform =
			Arc::new(StubPlatform { chat_authority_confirmed: true, ..StubPlatform::default() });
		let (services, activation) = signing_runtime_with_platform(platform.clone());
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let runtime = product_runtime(services, activation);
		let cx = CallContext::default();
		let initialize = HostProductDeviceChatRequest::V2(
			truapi::latest::HostProductDeviceChatRequest::Initialize,
		);
		let HostProductDeviceChatResponse::V2(first) =
			runtime.product_device_chat(&cx, initialize.clone()).await.unwrap();
		let HostProductDeviceChatResponse::V2(second) =
			runtime.product_device_chat(&cx, initialize.clone()).await.unwrap();
		assert_eq!(first.device, second.device);
		assert_eq!(first.device.product_account.dot_ns_identifier, "myapp.dot");
		assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
		runtime
			.connection
			.set_permission_authorization_status(
				PermissionAuthorizationRequest::ChatAuthority,
				PermissionAuthorizationStatus::Denied,
			)
			.await
			.unwrap();
		assert!(matches!(
			runtime.product_device_chat(&cx, initialize).await,
			Err(CallError::Domain(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::AccessNotGranted
			)))
		));
		assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
	});
}

#[test]
fn product_chat_allow_once_lasts_for_the_session_without_persisting() {
	futures::executor::block_on(async {
		let platform =
			Arc::new(StubPlatform { chat_authority_confirmed: true, ..StubPlatform::default() });
		platform
			.permission_confirmation_decisions
			.lock()
			.expect("permission confirmation mutex poisoned")
			.push_back(crate::platform::PermissionDecision::AllowOnce);
		let (services, activation) = signing_runtime_with_platform(platform.clone());
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let runtime = product_runtime(services.clone(), activation.clone());
		let cx = CallContext::default();
		let chat =
			|request| runtime.product_device_chat(&cx, HostProductDeviceChatRequest::V2(request));
		chat(truapi::latest::HostProductDeviceChatRequest::Initialize).await.unwrap();
		// The in-actor re-check honours the session grant: the lookup, not
		// authorization, is what fails for an unknown attachment.
		let unknown = truapi::latest::HostProductDeviceChatRequest::OpenAttachment {
			attachment_id: [0x77; 32],
		};
		assert!(!matches!(
			chat(unknown.clone()).await,
			Err(CallError::Domain(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::AccessNotGranted
			)))
		));
		assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
		// A new execution of the same product reuses the session grant.
		let next_execution = product_runtime(services, activation);
		next_execution
			.product_device_chat(
				&cx,
				HostProductDeviceChatRequest::V2(
					truapi::latest::HostProductDeviceChatRequest::Initialize,
				),
			)
			.await
			.unwrap();
		assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
		assert_eq!(
			runtime
				.connection
				.permission_authorization_status(PermissionAuthorizationRequest::ChatAuthority)
				.await
				.unwrap(),
			PermissionAuthorizationStatus::NotDetermined
		);

		runtime
			.connection
			.set_permission_authorization_status(
				PermissionAuthorizationRequest::ChatAuthority,
				PermissionAuthorizationStatus::Denied,
			)
			.await
			.unwrap();
		assert!(matches!(
			chat(unknown).await,
			Err(CallError::Domain(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::AccessNotGranted
			)))
		));
	});
}

#[test]
fn product_chat_allow_once_does_not_survive_wallet_reactivation() {
	futures::executor::block_on(async {
		let platform = Arc::new(StubPlatform::default());
		platform
			.permission_confirmation_decisions
			.lock()
			.expect("permission confirmation mutex poisoned")
			.push_back(crate::platform::PermissionDecision::AllowOnce);
		let (services, activation) = signing_runtime_with_platform(platform.clone());
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let runtime = product_runtime(services, activation.clone());
		let cx = CallContext::default();
		let initialize = HostProductDeviceChatRequest::V2(
			truapi::latest::HostProductDeviceChatRequest::Initialize,
		);
		runtime.product_device_chat(&cx, initialize.clone()).await.unwrap();
		activation.disconnect().await;
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		assert!(matches!(
			runtime.product_device_chat(&cx, initialize).await,
			Err(CallError::Domain(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::AccessNotGranted
			)))
		));
		assert_eq!(platform.chat_authority_reviews.lock().len(), 2);
	});
}

#[test]
fn product_chat_authority_validation_does_not_require_statement_delivery() {
	futures::executor::block_on(async {
		let platform =
			Arc::new(StubPlatform { chat_authority_confirmed: true, ..StubPlatform::default() });
		let (services, activation) = signing_runtime_with_platform(platform);
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let runtime = product_runtime(services, activation);
		runtime
			.connection
			.set_permission_authorization_status(
				PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest {
					permission: v01::RemotePermission::StatementSubmit,
				}),
				PermissionAuthorizationStatus::Denied,
			)
			.await
			.unwrap();
		let request = HostProductDeviceChatRequest::V2(
			truapi::latest::HostProductDeviceChatRequest::SendPayment {
				peer_identity: [0x55; 32],
				request_id: "not-authorized".to_string(),
				amount_cents: 10,
			},
		);
		assert!(matches!(
			runtime.product_device_chat(&CallContext::default(), request).await,
			Err(CallError::Domain(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::PeerNotReady
			)))
		));
		let request = HostProductDeviceChatRequest::V2(
			truapi::latest::HostProductDeviceChatRequest::Prepare {
				peer_identity: [0x55; 32],
				route: truapi::latest::HostNativeChatRoute::Identity,
				plaintext: vec![],
			},
		);
		assert!(matches!(
			runtime.product_device_chat(&CallContext::default(), request).await,
			Err(CallError::Domain(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::InvalidRequest
			)))
		));
	});
}

async fn sso_chat(
	service: &SigningHostSsoService,
	payload: SsoProductDeviceChatOperation,
) -> ProductDeviceChatResponse {
	sso_chat_as(service, "myapp.dot", payload).await
}

async fn sso_chat_as(
	service: &SigningHostSsoService,
	calling_product_id: &str,
	payload: SsoProductDeviceChatOperation,
) -> ProductDeviceChatResponse {
	let message = RemoteMessage::request(
		"chat-consent".to_string(),
		ProductRequest { calling_product_id: ProductId::for_test(calling_product_id), payload },
	);
	let Dispatch::Response(answer) = service.answer(message).await else {
		panic!("expected SSO response");
	};
	let RemoteMessageData::V1(v1::RemoteMessage::ProductDeviceChatResponse(response)) =
		answer.message.data
	else {
		panic!("expected SSO Chat response");
	};
	response.payload
}

#[test]
fn sso_chat_username_grant_does_not_authorize_chat() {
	futures::executor::block_on(async {
		let platform = Arc::new(StubPlatform::default());
		let (_, activation) = signing_runtime_with_platform(platform.clone());
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let service = SigningHostSsoService::new(activation);
		let product = crate::platform::ProductContext::new_with_execution(
			ProductId::for_test("myapp.dot"),
			crate::platform::ExecutableKind::Worker,
		);
		let permissions = PermissionsService::new(platform.as_ref(), platform.as_ref(), &product);
		permissions
			.set_authorization_status(
				&PermissionAuthorizationRequest::IdentityDisclosure,
				PermissionAuthorizationStatus::Authorized,
			)
			.await
			.unwrap();
		let request = SsoProductDeviceChatOperation::V3(
			truapi::latest::HostProductDeviceChatRequest::Initialize,
		);

		for _ in 0..2 {
			assert_eq!(
				sso_chat(&service, request.clone()).await,
				Err(HostProductDeviceChatError::V1(
					truapi::latest::HostProductDeviceChatError::AccessNotGranted,
				))
			);
		}
		assert_eq!(
			platform.chat_authority_reviews.lock().len(),
			1,
			"SSO Chat requires its own prompt, then respects the persisted refusal"
		);
		assert_eq!(
			permissions
				.authorization_status(&PermissionAuthorizationRequest::ChatAuthority)
				.await
				.unwrap(),
			PermissionAuthorizationStatus::Denied
		);
		assert_eq!(
			permissions
				.authorization_status(&PermissionAuthorizationRequest::IdentityDisclosure)
				.await
				.unwrap(),
			PermissionAuthorizationStatus::Authorized
		);
	});
}

#[test]
fn sso_chat_consent_is_cached_and_revocation_blocks_chat() {
	futures::executor::block_on(async {
		let platform =
			Arc::new(StubPlatform { chat_authority_confirmed: true, ..StubPlatform::default() });
		let (_, activation) = signing_runtime_with_platform(platform.clone());
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let service = SigningHostSsoService::new(activation);
		let product = crate::platform::ProductContext::new_with_execution(
			ProductId::for_test("myapp.dot"),
			crate::platform::ExecutableKind::Worker,
		);
		let permissions = PermissionsService::new(platform.as_ref(), platform.as_ref(), &product);
		let initialize = SsoProductDeviceChatOperation::V3(
			truapi::latest::HostProductDeviceChatRequest::Initialize,
		);
		let HostProductDeviceChatResponse::V2(first) =
			sso_chat(&service, initialize.clone()).await.unwrap();
		let HostProductDeviceChatResponse::V2(second) =
			sso_chat(&service, initialize.clone()).await.unwrap();
		assert_eq!(first.device, second.device);
		assert_eq!(first.device.product_account.dot_ns_identifier, "myapp.dot");
		assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
		permissions
			.set_authorization_status(
				&PermissionAuthorizationRequest::ChatAuthority,
				PermissionAuthorizationStatus::Denied,
			)
			.await
			.unwrap();
		assert_eq!(
			sso_chat(&service, initialize).await,
			Err(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::AccessNotGranted,
			))
		);
		assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
	});
}

#[test]
fn sso_chat_allow_once_lasts_for_the_session_without_persisting() {
	futures::executor::block_on(async {
		let platform =
			Arc::new(StubPlatform { chat_authority_confirmed: true, ..StubPlatform::default() });
		platform
			.permission_confirmation_decisions
			.lock()
			.expect("permission confirmation mutex poisoned")
			.push_back(crate::platform::PermissionDecision::AllowOnce);
		let (_, activation) = signing_runtime_with_platform(platform.clone());
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let service = SigningHostSsoService::new(activation);
		let initialize = SsoProductDeviceChatOperation::V3(
			truapi::latest::HostProductDeviceChatRequest::Initialize,
		);
		let unknown = SsoProductDeviceChatOperation::V3(
			truapi::latest::HostProductDeviceChatRequest::OpenAttachment {
				attachment_id: [0x77; 32],
			},
		);
		sso_chat(&service, initialize.clone()).await.unwrap();
		sso_chat(&service, initialize).await.unwrap();
		assert_ne!(
			sso_chat(&service, unknown.clone()).await,
			Err(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::AccessNotGranted,
			))
		);
		assert_eq!(platform.chat_authority_reviews.lock().len(), 1);

		let product = crate::platform::ProductContext::new_with_execution(
			ProductId::for_test("myapp.dot"),
			crate::platform::ExecutableKind::Worker,
		);
		let permissions = PermissionsService::new(platform.as_ref(), platform.as_ref(), &product);
		assert_eq!(
			permissions
				.authorization_status(&PermissionAuthorizationRequest::ChatAuthority)
				.await
				.unwrap(),
			PermissionAuthorizationStatus::NotDetermined
		);
		permissions
			.set_authorization_status(
				&PermissionAuthorizationRequest::ChatAuthority,
				PermissionAuthorizationStatus::Denied,
			)
			.await
			.unwrap();
		assert_eq!(
			sso_chat(&service, unknown).await,
			Err(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::AccessNotGranted,
			))
		);
		assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
	});
}

#[test]
fn sso_chat_keys_consent_and_device_on_the_attested_product() {
	futures::executor::block_on(async {
		let platform =
			Arc::new(StubPlatform { chat_authority_confirmed: true, ..StubPlatform::default() });
		let (_, activation) = signing_runtime_with_platform(platform.clone());
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let service = SigningHostSsoService::new(activation);
		let refused = crate::platform::ProductContext::new_with_execution(
			ProductId::for_test("refused.dot"),
			crate::platform::ExecutableKind::Worker,
		);
		PermissionsService::new(platform.as_ref(), platform.as_ref(), &refused)
			.set_authorization_status(
				&PermissionAuthorizationRequest::ChatAuthority,
				PermissionAuthorizationStatus::Denied,
			)
			.await
			.unwrap();
		let initialize = SsoProductDeviceChatOperation::V3(
			truapi::latest::HostProductDeviceChatRequest::Initialize,
		);

		assert_eq!(
			sso_chat_as(&service, "refused.dot", initialize.clone()).await,
			Err(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::AccessNotGranted,
			))
		);
		assert!(platform.chat_authority_reviews.lock().is_empty());

		let HostProductDeviceChatResponse::V2(approved) =
			sso_chat_as(&service, "approved.dot", initialize.clone()).await.unwrap();
		assert_eq!(approved.device.product_account.dot_ns_identifier, "approved.dot");
		let HostProductDeviceChatResponse::V2(other) =
			sso_chat_as(&service, "other.dot", initialize).await.unwrap();
		assert_eq!(other.device.product_account.dot_ns_identifier, "other.dot");
		assert_ne!(approved.device.account_id, other.device.account_id);
		assert_eq!(
			platform
				.chat_authority_reviews
				.lock()
				.iter()
				.map(|review| review.product_id.as_str())
				.collect::<Vec<_>>(),
			["approved.dot", "other.dot"],
			"each attested product is prompted under its own identity"
		);
	});
}

#[test]
fn sso_chat_authority_validation_does_not_require_statement_delivery() {
	futures::executor::block_on(async {
		let platform =
			Arc::new(StubPlatform { chat_authority_confirmed: true, ..StubPlatform::default() });
		let (_, activation) = signing_runtime_with_platform(platform.clone());
		activation.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let service = SigningHostSsoService::new(activation);
		let product = crate::platform::ProductContext::new_with_execution(
			ProductId::for_test("myapp.dot"),
			crate::platform::ExecutableKind::Worker,
		);
		let permissions = PermissionsService::new(platform.as_ref(), platform.as_ref(), &product);
		permissions
			.set_authorization_status(
				&PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest {
					permission: v01::RemotePermission::StatementSubmit,
				}),
				PermissionAuthorizationStatus::Denied,
			)
			.await
			.unwrap();
		let request = SsoProductDeviceChatOperation::V3(
			truapi::latest::HostProductDeviceChatRequest::SendPayment {
				peer_identity: [0x55; 32],
				request_id: "not-authorized".to_string(),
				amount_cents: 10,
			},
		);
		assert_eq!(
			sso_chat(&service, request).await,
			Err(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::PeerNotReady,
			))
		);
		let request = SsoProductDeviceChatOperation::V3(
			truapi::latest::HostProductDeviceChatRequest::Prepare {
				peer_identity: [0x55; 32],
				route: truapi::latest::HostNativeChatRoute::Identity,
				plaintext: vec![],
			},
		);
		assert_eq!(
			sso_chat(&service, request).await,
			Err(HostProductDeviceChatError::V1(
				truapi::latest::HostProductDeviceChatError::InvalidRequest,
			))
		);
	});
}
