use super::*;
use crate::{
	host_internal::{
		permissions::set_authorization_status,
		sso_messages::{RemoteMessage, RemoteMessageData, v1},
	},
	platform::{PermissionAuthorizationStatus, PermissionDecision},
	runtime::sso_service::Dispatch,
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
			futures::executor::block_on(
				authority.activate_local_session(RootEntropy::new(ENTROPY)),
			)
			.unwrap();
			let session = authority.accounts().current_session().unwrap();
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
			let service = sso_service(&authority);
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
				let Ok(Dispatch::Response(answer)) =
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

/// A paired host names the caller itself. A trusted label under a TLD the
/// stack does not serve is refused before any grant, prompt or key is used,
/// instead of passing as the trusted product without a prompt.
#[test]
fn a_relayed_caller_the_stack_does_not_serve_is_refused() {
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", "{}");
	let (_, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(RootEntropy::new(ENTROPY)))
		.unwrap();
	let session = authority.accounts().current_session().unwrap();
	register_full_person_key(&authority, &session, &full_person_ring_location());

	let alias = paired_request(
		&authority,
		"peopl.testnet",
		HostAccountGetAliasRequest {
			key_handle: full_person_key_handle(),
			context: v01::ProductProofContext {
				product_id: ProductId::for_test("peopl.dot"),
				suffix: v01::DerivationIndex::Index(0),
			},
			ring_location: full_person_ring_location(),
		},
	);
	let list = paired_request(
		&authority,
		"peopl.testnet",
		HostAccountListRingVrfKeysRequest {
			owner: ProductId::for_test("peopl.dot"),
			disclosure: RingVrfKeyDisclosure::PublicKey,
		},
	);

	let unserved = Err(RingVrfError::Unknown {
		reason: "product peopl.testnet is not under a TLD the dot stack serves".to_string(),
	});
	assert_eq!(
		(
			alias.map(|_| ()),
			list.map(|_| ()),
			platform.account_access_reviews.lock().unwrap().len()
		),
		(unserved.clone(), unserved, 0)
	);
}

/// A relayed request is held to the rule a product's own request is: every
/// product it names must be one the stack serves. An unserved owner or
/// context is refused before the user is asked, so no Allow Always, which is
/// filed by bare label, can be won for it.
#[test]
fn a_relayed_request_naming_an_unserved_product_is_refused() {
	let platform = Arc::new(StubPlatform {
		permission_confirmation_decisions: std::sync::Mutex::new(
			[PermissionDecision::AllowAlways; 3].into(),
		),
		..StubPlatform::default()
	});
	cache_grant(&platform, "peopl.dot", "{}");
	let (_, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(RootEntropy::new(ENTROPY)))
		.unwrap();
	let session = authority.accounts().current_session().unwrap();
	register_full_person_key(&authority, &session, &full_person_ring_location());

	let alias = |key_handle, context: &str| {
		let request = HostAccountGetAliasRequest {
			key_handle,
			context: v01::ProductProofContext {
				product_id: ProductId::for_test(context),
				suffix: v01::DerivationIndex::Index(0),
			},
			ring_location: full_person_ring_location(),
		};
		paired_request(&authority, "bar.dot", request).map(|_| ())
	};
	let unserved_owner = v01::ProductAccountId {
		dot_ns_identifier: ProductId::for_test("foo.testnet"),
		derivation_index: v01::DerivationIndex::Index(0),
	};
	let list = HostAccountListRingVrfKeysRequest {
		owner: ProductId::for_test("foo.testnet"),
		disclosure: RingVrfKeyDisclosure::PublicKey,
	};
	let outcomes = [
		alias(unserved_owner, "bar.dot"),
		alias(full_person_key_handle(), "foo.testnet"),
		paired_request(&authority, "bar.dot", list).map(|_| ()),
	];

	let unserved = || {
		Err(RingVrfError::Unknown {
			reason: "product foo.testnet is not under a TLD the dot stack serves".to_string(),
		})
	};
	assert_eq!(
		(outcomes, platform.account_access_reviews.lock().unwrap().len()),
		([unserved(), unserved(), unserved()], 0)
	);
}
