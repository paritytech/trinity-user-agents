use super::*;
use crate::{
	platform::HostInfo,
	runtime::{
		authority::ProductAuthority,
		bulletin_rpc::BulletinRpc,
		signing_host::LocalActivation,
		statement_allowance::{
			ASSET_HUB_TEST_METADATA_BYTES, collection::PersonhoodCollection, ring::RingParams,
		},
	},
	test_support::{StubPlatform, test_spawner},
};
use futures::FutureExt;
use parity_scale_codec::Encode;
use std::sync::Arc;

const ENTROPY: [u8; 16] = [0xab; 16];

fn services(platform: Arc<StubPlatform>) -> Arc<RuntimeServices> {
	RuntimeServices::new(
		platform,
		HostInfo {
			name: "Resource recovery".into(),
			icon: None,
			version: None,
			platform: api::HostPlatform::Unknown,
		},
		[0x22; 32],
		[0xbb; 32],
		[0xaa; 32],
		test_spawner(),
	)
}

fn fixture(bulletin: Arc<StubPlatform>, asset_hub: Arc<StubPlatform>) -> Arc<SigningHost> {
	let bulletin_services = services(bulletin);
	let mut services = services(asset_hub);
	Arc::get_mut(&mut services).expect("unshared services").bulletin = BulletinRpc::new(
		bulletin_services.chain.clone(),
		bulletin_services.extrinsics.clone(),
		[0xbb; 32],
	);
	SigningHost::new(
		services,
		"paseo".into(),
		None,
		crate::platform::ProductConsentPolicy::default(),
	)
}

fn bulletin(value: &str) -> Arc<StubPlatform> {
	Arc::new(StubPlatform {
		rpc_method_responses: (0..4)
			.flat_map(|_| {
				[
					("state_getStorage", value.to_owned()),
					("chain_getHeader", r#"{"number":"0x1"}"#.into()),
				]
			})
			.collect(),
		..Default::default()
	})
}

fn funded_bulletin() -> Arc<StubPlatform> {
	let authorization = (0u32, 10u32, 0u64, 0u64, 1024u64, 100u32).encode();
	bulletin(&format!(r#""0x{}""#, hex::encode(authorization)))
}

fn asset_hub(value: &str) -> Arc<StubPlatform> {
	Arc::new(StubPlatform {
		rpc_method_responses: (0..4)
			.flat_map(|_| {
				[
					(
						"state_getRuntimeVersion",
						r#"{"specVersion":3000000,"transactionVersion":1}"#.into(),
					),
					("chain_getBlockHash", format!(r#""0x{}""#, hex::encode([0xaa; 32]))),
					(
						"Metadata_metadata_at_version",
						format!(
							r#""0x{}""#,
							hex::encode(Some(ASSET_HUB_TEST_METADATA_BYTES.to_vec()).encode())
						),
					),
					("state_getStorage", value.to_owned()),
				]
			})
			.collect(),
		..Default::default()
	})
}

async fn bounded<T>(future: impl Future<Output = T>) -> T {
	futures::pin_mut!(future);
	futures::select! {
		result = future.fuse() => result,
		_ = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse() => {
			panic!("resource recovery waited on an unexpected chain operation")
		}
	}
}

fn assert_read_only(platform: &StubPlatform) {
	for request in platform.sent_rpc.lock().unwrap().iter() {
		let request: serde_json::Value = serde_json::from_str(request).unwrap();
		assert!(
			matches!(
				request["method"].as_str(),
				Some(
					"state_getStorage" |
						"chain_getHeader" | "state_getRuntimeVersion" |
						"chain_getBlockHash" |
						"state_call"
				)
			),
			"unexpected mutation or proof discovery: {request}"
		);
	}
}

#[test]
fn recovery_preserves_partial_funding_and_grants_only_after_both_chain_reads() {
	let bulletin = funded_bulletin();
	let asset_hub = asset_hub(&format!(r#""0x{}""#, hex::encode(1u128.to_le_bytes())));
	let host = fixture(bulletin.clone(), asset_hub.clone());
	let product = ProductId::for_test("myapp.dot");
	futures::executor::block_on(async {
		host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let before = bounded(host.product_resource_status(&product)).await.unwrap();
		assert!(before.bulletin_available && before.contract_funded);
		assert!(!before.auto_signing, "inspection must not grant authority");
		assert_eq!(
			before.bulletin_account,
			derive_sr25519_hard_path(&ENTROPY, &["allowance", "bulletin", product.as_str()],)
				.unwrap()
				.public
				.to_bytes()
		);
		assert_eq!(
			before.contract_account,
			host.product_keypair(&api::ProductAccountId {
				dot_ns_identifier: product.clone(),
				derivation_index: api::DerivationIndex::Index(0),
			})
			.unwrap()
			.public
			.to_bytes()
		);
		let recovered = bounded(host.recover_product_resources(&product, false)).await.unwrap();
		assert!(
			recovered.bulletin_available && recovered.contract_funded && recovered.auto_signing
		);
		assert_eq!(recovered.bulletin_account, before.bulletin_account);
		assert_eq!(recovered.contract_account, before.contract_account);
	});
	assert_read_only(&bulletin);
	assert_read_only(&asset_hub);
	assert!(
		asset_hub
			.chain_connects
			.lock()
			.unwrap()
			.iter()
			.all(|genesis| *genesis == [0xaa; 32])
	);
}

#[test]
fn unconfirmed_recovery_cannot_reallocate_missing_capacity_or_grant_signing() {
	let bulletin = bulletin("null");
	let asset_hub = asset_hub("null");
	let host = fixture(bulletin.clone(), asset_hub.clone());
	let product = ProductId::for_test("myapp.dot");
	futures::executor::block_on(async {
		host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let status = bounded(host.product_resource_status(&product)).await.unwrap();
		assert!(!status.bulletin_available && !status.contract_funded && !status.auto_signing);
		assert!(matches!(
			bounded(host.recover_product_resources(&product, false)).await,
			Err(AuthorityError::Unavailable { .. })
		));
		let status = bounded(host.product_resource_status(&product)).await.unwrap();
		assert!(!status.bulletin_available && !status.contract_funded && !status.auto_signing);
	});
	assert_read_only(&bulletin);
	assert_read_only(&asset_hub);
}

#[test]
fn unreadable_resources_never_become_permission_to_allocate() {
	for invalid_bulletin in [false, true] {
		let bulletin = if invalid_bulletin { bulletin("{}") } else { bulletin("null") };
		let asset_hub = asset_hub("{}");
		let host = fixture(bulletin.clone(), asset_hub.clone());
		let product = ProductId::for_test("myapp.dot");
		futures::executor::block_on(async {
			host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
			assert!(bounded(host.recover_product_resources(&product, true)).await.is_err());
			let session = host.current_session().unwrap();
			let (_, generation) = host.require_current_product_session(&session, &product).unwrap();
			assert!(!host.has_auto_signing_grant(
				generation,
				session.public_key,
				&product,
				&product
			));
		});
		assert_read_only(&bulletin);
		assert_read_only(&asset_hub);
	}
}

#[test]
fn implicit_allocators_reuse_available_bulletin_and_partial_pgas() {
	let bulletin = funded_bulletin();
	let asset_hub = asset_hub(&format!(r#""0x{}""#, hex::encode(1u128.to_le_bytes())));
	let host = fixture(bulletin.clone(), asset_hub.clone());
	let product = ProductId::for_test("myapp.dot");
	futures::executor::block_on(async {
		host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
		let session = host.current_session().unwrap();
		let key = bounded(host.bulletin_allowance_key(
			&truapi::CallContext::default(),
			&session,
			&product,
		))
		.await
		.unwrap();
		assert_eq!(
			schnorrkel::SecretKey::from_bytes(key.as_secret_bytes())
				.unwrap()
				.to_public()
				.to_bytes(),
			derive_sr25519_hard_path(&ENTROPY, &["allowance", "bulletin", product.as_str()],)
				.unwrap()
				.public
				.to_bytes(),
		);
		bounded(allocate_smart_contract_allowance(
			&host.services,
			&host,
			&session,
			&product,
			api::DerivationIndex::Index(0),
			OnExistingAllowancePolicy::Ignore,
		))
		.await
		.unwrap();
	});
	assert_read_only(&bulletin);
	assert_read_only(&asset_hub);
}

#[test]
fn queued_resource_routes_reject_account_and_product_replacement_before_network() {
	for same_product_reset in [false, true] {
		for route in 0..7 {
			let bulletin = Arc::new(StubPlatform::default());
			let asset_hub = Arc::new(StubPlatform::default());
			let host = fixture(bulletin.clone(), asset_hub.clone());
			let product = ProductId::for_test("myapp.dot");
			futures::executor::block_on(async {
				host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
				let session = host.current_session().unwrap();
				let lock = host.resource_allocation.lock().await;
				let cx = truapi::CallContext::default();
				let pending = async {
					match route {
						0 => host.recover_product_resources(&product, true).await.map(|_| ()),
						1 => host.product_resource_status(&product).await.map(|_| ()),
						2 => host.bulletin_allowance_key(&cx, &session, &product).await.map(|_| ()),
						3 => host
							.statement_store_allowance_key(&cx, &session, &product)
							.await
							.map(|_| ()),
						4 => allocate_smart_contract_allowance(
							&host.services,
							&host,
							&session,
							&product,
							api::DerivationIndex::Index(0),
							OnExistingAllowancePolicy::Ignore,
						)
						.await
						.map_err(Into::into),
						5 => allocate_product_statement_store_allowance(
							&host.services,
							&host,
							&session,
							&product,
							&api::DerivationIndex::Index(0),
							OnExistingAllowancePolicy::Ignore,
						)
						.await
						.map_err(Into::into),
						_ => host
							.allocate_resources(
								&cx,
								&session,
								&product,
								api::HostRequestResourceAllocationRequest {
									resources: vec![api::AllocatableResource::AutoSigning],
								},
							)
							.await
							.map(|_| ()),
					}
				};
				futures::pin_mut!(pending);
				assert!(
					pending.as_mut().now_or_never().is_none(),
					"route {route} bypassed allocation serialization"
				);
				if same_product_reset {
					host.clear_product_state(&product).await.unwrap();
				} else {
					host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
				}
				drop(lock);
				assert_eq!(
					bounded(pending).await,
					Err(AuthorityError::Disconnected),
					"route {route}"
				);
			});
			assert!(bulletin.sent_rpc.lock().unwrap().is_empty());
			assert!(asset_hub.sent_rpc.lock().unwrap().is_empty());
		}
	}
}

#[test]
fn personhood_secret_use_survives_only_unrelated_product_reset() {
	futures::executor::block_on(async {
		for reactivate in [false, true] {
			let host = fixture(Arc::default(), Arc::default());
			host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
			let product = ProductId::for_test("myapp.dot");
			let session = host.current_session().unwrap();
			let candidates = host.reserved_person_collection_candidates(&session).unwrap();
			let signer = SessionPersonhoodSigner::new(&host, &session, &product, &candidates)
				.await
				.unwrap();
			let collection = PersonhoodCollection::People;
			let member = signer.member(collection).unwrap();
			let alias = signer.alias(collection, b"context").unwrap();
			host.clear_product_state(&ProductId::for_test("other.dot")).await.unwrap();
			assert_eq!(signer.member(collection).unwrap(), member);
			assert_eq!(signer.alias(collection, b"context").unwrap(), alias);
			if reactivate {
				host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
			} else {
				host.clear_product_state(&product).await.unwrap();
			}
			assert!(matches!(
				signer.member(collection),
				Err(StatementAllowanceError::SessionInvalidated)
			));
			assert!(matches!(
				signer.alias(collection, b"context"),
				Err(StatementAllowanceError::SessionInvalidated)
			));
			let ring = RingParams {
				collection,
				members: vec![member],
				exponent: 9,
				ring_index: 0,
				block_hash: "0xfinal".into(),
			};
			assert!(matches!(
				signer.prove(&ring, b"context", b"message"),
				Err(StatementAllowanceError::SessionInvalidated)
			));
		}
	});
}

#[test]
fn reviewed_sso_allocation_cannot_release_keys_after_queued_product_reset() {
	use crate::{
		host_internal::sso_messages::{
			RemoteMessage, RemoteMessageData, ResourceAllocationRequest, v1,
		},
		runtime::{signing_host::SigningHostSsoService, sso_service::Dispatch},
	};
	for resource in [
		api::AllocatableResource::AutoSigning,
		api::AllocatableResource::BulletinAllowance,
		api::AllocatableResource::SmartContractAllowance(api::DerivationIndex::Index(0)),
		api::AllocatableResource::StatementStoreAllowance,
		api::AllocatableResource::ProductStatementStoreAllowance(api::DerivationIndex::Index(0)),
	] {
		let platform =
			Arc::new(StubPlatform { resource_allocation_confirmed: true, ..Default::default() });
		let host = fixture(Arc::default(), platform.clone());
		let product = ProductId::for_test("myapp.dot");
		futures::executor::block_on(async {
			host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
			let service = SigningHostSsoService::new(host.clone());
			let lock = host.resource_allocation.lock().await;
			let pending = service.answer(RemoteMessage::request(
				"queued-allocation".into(),
				ResourceAllocationRequest {
					calling_product_id: product.clone(),
					resources: vec![resource],
					on_existing: OnExistingAllowancePolicy::Ignore,
				},
			));
			futures::pin_mut!(pending);
			assert!(pending.as_mut().now_or_never().is_none());
			assert_eq!(platform.resource_allocation_reviews.lock().unwrap().len(), 1);
			host.clear_product_state(&product).await.unwrap();
			drop(lock);
			let Dispatch::Response(answer) = bounded(pending).await else {
				panic!("expected allocation failure response");
			};
			let RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(response)) =
				answer.message.data
			else {
				panic!("expected resource allocation response");
			};
			assert!(response.payload.is_err(), "revoked authority must not release resource keys");
		});
		assert!(platform.sent_rpc.lock().unwrap().is_empty());
	}
}

#[test]
fn resource_reads_revalidate_account_and_product_but_preserve_sibling_work() {
	for recover in [false, true] {
		for change in 0..3 {
			let bulletin = funded_bulletin();
			let asset_hub = asset_hub(&format!(r#""0x{}""#, hex::encode(1u128.to_le_bytes())));
			let host = fixture(bulletin.clone(), asset_hub.clone());
			let product = ProductId::for_test("myapp.dot");
			futures::executor::block_on(async {
				host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
				let (release, gate) = futures::channel::oneshot::channel();
				*bulletin.rpc_method_responses_gate.lock().unwrap() = Some(gate);
				let pending = async {
					if recover {
						host.recover_product_resources(&product, false).await
					} else {
						host.product_resource_status(&product).await
					}
				};
				futures::pin_mut!(pending);
				assert!(pending.as_mut().now_or_never().is_none());
				match change {
					0 => host.clear_product_state(&product).await.unwrap(),
					1 => host.clear_product_state(&ProductId::for_test("other.dot")).await.unwrap(),
					_ => host.activate_local_session(ENTROPY.to_vec()).await.unwrap(),
				}
				let _ = release.send(());
				let result = bounded(pending).await;
				if change == 1 {
					let status = result.unwrap();
					assert!(status.bulletin_available && status.contract_funded);
					assert_eq!(status.auto_signing, recover);
				} else {
					assert!(matches!(result, Err(AuthorityError::Disconnected)));
				}
			});
			assert_read_only(&bulletin);
			assert_read_only(&asset_hub);
		}
	}
}
