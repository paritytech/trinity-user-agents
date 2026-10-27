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
									product_id: "myapp.dot".parse().expect("valid product id"),
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
								owner: "foreign.dot".parse().expect("valid product id"),
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
