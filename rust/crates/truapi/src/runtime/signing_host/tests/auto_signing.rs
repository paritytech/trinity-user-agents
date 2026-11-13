//! An AutoSigning grant or a blessed caller waives the per-call confirmation on
//! the signing role.

use super::*;
use crate::{
	host_internal::sso_messages::{RemoteMessage, RemoteMessageData, v1},
	runtime::sso_service::Dispatch,
};
use futures::FutureExt;
use truapi::versioned::{
	account::HostAccountSignVrfRequest,
	signing::{HostSignRawWithLegacyAccountRequest, HostSignRawWithLegacyAccountResponse},
};

/// A platform that approves the allocation and declines every signing prompt,
/// so any call that succeeds did so without asking.
fn granting_platform() -> Arc<StubPlatform> {
	Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		sign_raw_confirmed: false,
		sign_payload_confirmed: false,
		create_transaction_confirmed: false,
		..StubPlatform::default()
	})
}

fn raw_request(product_id: &str) -> HostSignRawRequest {
	HostSignRawRequest::V1(v01::HostSignRawRequest {
		account: v01::ProductAccountId {
			dot_ns_identifier: ProductId::for_test(product_id),
			derivation_index: v01::DerivationIndex::Index(0),
		},
		payload: v01::RawPayload::Bytes { bytes: b"hello world".to_vec() },
	})
}

#[test]
fn a_granted_product_signs_raw_without_a_prompt() {
	let platform = granting_platform();
	let (services, activation) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime(services, activation);
	grant_auto_signing(&runtime);

	let HostSignRawResponse::V1(response) = futures::executor::block_on(
		runtime.sign_raw(&CallContext::default(), raw_request("myapp.dot")),
	)
	.expect("a granted product signs without confirmation");

	assert!(
		platform
			.sign_raw_reviews
			.lock()
			.expect("raw signing review list mutex poisoned")
			.is_empty(),
		"the grant waives the prompt",
	);
	let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
	let keypair =
		derive_product_keypair(&root, &ProductId::for_test("myapp.dot"), index_bytes(0)).unwrap();
	let signature =
		schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
	assert!(
		keypair
			.public
			.verify_simple(b"substrate", b"<Bytes>hello world</Bytes>", &signature)
			.is_ok(),
		"the grant signs the same bytes the prompt would have shown",
	);
}

#[test]
fn an_ungranted_product_still_prompts_for_raw_signing() {
	let platform = granting_platform();
	let (services, activation) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime(services, activation);

	let error = futures::executor::block_on(
		runtime.sign_raw(&CallContext::default(), raw_request("myapp.dot")),
	)
	.expect_err("the stub declines the confirmation");

	assert!(matches!(
		error,
		CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
	));
	assert_eq!(
		platform
			.sign_raw_reviews
			.lock()
			.expect("raw signing review list mutex poisoned")
			.len(),
		1,
		"without a grant the user is asked exactly once",
	);
}

#[test]
fn a_grant_does_not_cover_the_unwatermarked_raw_signing_api() {
	// The deprecated API's signatures are not domain-separated from
	// transaction signatures, so a standing grant must not waive its prompt.
	let platform = granting_platform();
	let (services, activation) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime(services, activation);
	grant_auto_signing(&runtime);

	#[allow(deprecated)]
	let error = futures::executor::block_on(
		runtime
			.sign_raw_unwatermarked_deprecated(&CallContext::default(), raw_request("myapp.dot")),
	)
	.expect_err("the stub declines the confirmation");

	assert!(matches!(
		error,
		CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
	));
	assert_eq!(
		platform
			.sign_raw_reviews
			.lock()
			.expect("raw signing review list mutex poisoned")
			.len(),
		1,
		"the unwatermarked API prompts whatever the grant says",
	);
}

#[test]
fn a_blessed_product_signs_its_own_account_without_a_grant() {
	for (product_id, blessed) in [("dim2.paseo", true), ("app.dim2.paseo", false)] {
		let platform = granting_platform();
		let (services, activation) = signing_runtime_with_platform(platform.clone());
		futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
			.expect("activation succeeds");
		let runtime = product_runtime_for(services, activation, product_id);

		let signed = futures::executor::block_on(
			runtime.sign_raw(&CallContext::default(), raw_request(product_id)),
		)
		.is_ok();

		assert_eq!(
			(signed, platform.sign_raw_reviews.lock().unwrap().len()),
			(blessed, usize::from(!blessed)),
			"{product_id}",
		);
	}
}

/// Only the local runtime vouches for its caller, and only for its own account;
/// a relayed request can claim any product id.
#[test]
fn a_blessed_vrf_signature_skips_the_prompt_only_locally_for_its_own_account() {
	let platform = granting_platform();
	let (services, activation) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime_for(services, activation.clone(), "dim2.paseo");
	let request = vrf_request("dim2.paseo");

	let sign_locally = |request| {
		futures::executor::block_on(
			runtime.sign_vrf(&CallContext::default(), HostAccountSignVrfRequest::V1(request)),
		)
		.is_ok()
	};
	let own_signed = sign_locally(request.clone());
	let foreign_signed = sign_locally(vrf_request("other.paseo"));
	let Ok(Dispatch::Response(answer)) =
		futures::executor::block_on(sso_service(&activation).answer(RemoteMessage::request(
			"relayed-vrf".to_string(),
			ProductRequest {
				calling_product_id: ProductId::for_test("dim2.paseo"),
				payload: request,
			},
		)))
	else {
		panic!("expected a VRF response")
	};
	let RemoteMessageData::V1(v1::RemoteMessage::SignVrfResponse(response)) = answer.message.data
	else {
		panic!("expected a VRF signing response")
	};

	assert_eq!(
		(
			own_signed,
			foreign_signed,
			response.payload.map(|_| ()),
			platform.sign_vrf_reviews.lock().unwrap().len(),
		),
		(true, false, Err(v01::HostAccountSignVrfError::Rejected), 2),
	);
}

/// A relayed request names its calling product without proof, so it must not
/// ride on that product's local AutoSigning grant.
#[test]
fn a_relayed_vrf_signature_cannot_reuse_a_local_auto_signing_grant() {
	let platform = granting_platform();
	let (services, activation) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	grant_auto_signing(&product_runtime(services, activation.clone()));

	let Ok(Dispatch::Response(answer)) =
		futures::executor::block_on(sso_service(&activation).answer(RemoteMessage::request(
			"relayed-vrf".to_string(),
			ProductRequest {
				calling_product_id: ProductId::for_test("myapp.dot"),
				payload: vrf_request("myapp.dot"),
			},
		)))
	else {
		panic!("expected a VRF response")
	};
	let RemoteMessageData::V1(v1::RemoteMessage::SignVrfResponse(response)) = answer.message.data
	else {
		panic!("expected a VRF signing response")
	};

	assert_eq!(
		(response.payload.map(|_| ()), platform.sign_vrf_reviews.lock().unwrap().len()),
		(Err(v01::HostAccountSignVrfError::Rejected), 1),
	);
}

/// A signing host on which `myapp.dot` keeps its AutoSigning key, registered in
/// a ring of that key alone.
///
/// The host's resolver holds the first ring resolution until
/// `resolution_gate`, when given, is released.
fn kept_key_in_its_own_ring(
	resolution_gate: Option<futures::channel::oneshot::Receiver<()>>,
) -> Arc<SigningHostRole> {
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));
	let entropy =
		derive_ring_vrf_entropy(&ENTROPY, &product.product_id, &v01::DerivationIndex::Index(0))
			.unwrap();
	let member = futures::executor::block_on(crate::runtime::vrf::load())
		.expect("verifiable is linked")
		.member(&entropy)
		.unwrap();
	let resolver = Arc::new(StubRingResolver {
		validation_gate: Default::default(),
		resolution_gate: Arc::new(std::sync::Mutex::new(resolution_gate)),
		collection: *b"pop:polkadot.network/people     ",
		ring: ResolvedRing {
			selected: MemberCandidate { member },
			ring_index: 7,
			ring_revision: 11,
			domain_size: crate::runtime::vrf::DOMAIN_2E11,
			members: vec![member],
		},
	});
	let platform =
		Arc::new(StubPlatform { resource_allocation_confirmed: true, ..StubPlatform::default() });
	let (services, activation) = signing_runtime_with_ring_resolver(platform, resolver);
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	grant_auto_signing(&product_runtime(services, activation.clone()));
	let session = activation.accounts().current_session().expect("an active wallet");
	futures::executor::block_on(activation.accounts().register_ring_vrf_key(
		&session,
		&CallContext::default(),
		&product,
		HostAccountRegisterRingVrfKeyRequest {
			index: v01::DerivationIndex::Index(0),
			ring: full_person_ring_location(),
		},
	))
	.expect("the kept key registers");
	activation
}

/// A proof with `myapp.dot`'s first account in its own context.
fn own_proof_request() -> HostAccountCreateProofRequest {
	let product_id = ProductId::for_test("myapp.dot");
	HostAccountCreateProofRequest {
		key_handle: v01::ProductAccountId {
			dot_ns_identifier: product_id.clone(),
			derivation_index: v01::DerivationIndex::Index(0),
		},
		context: v01::ProductProofContext { product_id, suffix: v01::DerivationIndex::Index(0) },
		ring_location: full_person_ring_location(),
		message: b"prove me".to_vec(),
	}
}

/// A product with a kept AutoSigning key proves membership with that key
/// itself, so the proof carries the alias its own key has in the context and
/// the ring the host resolved.
#[test]
fn a_kept_key_proves_with_its_own_alias() {
	let activation = kept_key_in_its_own_ring(None);
	let session = activation.accounts().current_session().expect("an active wallet");
	let product_id = ProductId::for_test("myapp.dot");
	let request = own_proof_request();
	let context = crate::runtime::signing_host::ring_vrf::context_bytes(&request.context);

	let proof = futures::executor::block_on(activation.accounts().create_proof(
		&session,
		&CallContext::default(),
		&ProductContext::new(product_id.clone()),
		request,
	))
	.expect("the kept key proves");

	let entropy =
		derive_ring_vrf_entropy(&ENTROPY, &product_id, &v01::DerivationIndex::Index(0)).unwrap();
	let vrf =
		futures::executor::block_on(crate::runtime::vrf::load()).expect("verifiable is linked");
	let alias =
		v01::ContextualAlias { context, alias: vrf.alias(&entropy, &context).unwrap().to_vec() };
	assert_eq!(
		(proof.contextual_alias, proof.ring_index, proof.ring_revision, proof.proof.is_empty()),
		(alias, 7, 11, false)
	);
}

/// A product reset revokes the kept key a ring proof is being made with, so a
/// proof that finishes after the reset is refused and never reaches the
/// product.
#[test]
fn a_kept_key_proof_finished_after_a_product_reset_is_refused() {
	let (release, gate) = futures::channel::oneshot::channel();
	let activation = kept_key_in_its_own_ring(Some(gate));
	let runtime = product_runtime(activation.services.clone(), activation.clone());
	let cx = CallContext::default();
	let proof = runtime.create_account_proof(
		&cx,
		truapi::versioned::account::HostAccountCreateProofRequest::V1(own_proof_request()),
	);
	futures::pin_mut!(proof);
	assert!(proof.as_mut().now_or_never().is_none(), "the proof waits on its ring");

	activation.clear_product_state(&ProductId::for_test("myapp.dot"));
	release.send(()).unwrap();

	assert_eq!(
		futures::executor::block_on(proof).map(|_| ()),
		Err(CallError::Domain(truapi::versioned::account::HostAccountCreateProofError::V1(
			v01::HostAccountCreateProofError::Unknown {
				reason: AuthorityError::Disconnected.to_string()
			}
		)))
	);
}

/// A product that withdrew its call gets no statement proof signed for it,
/// even when a kept key could sign without asking anyone.
#[test]
fn a_withdrawn_statement_proof_is_not_signed_with_a_kept_key() {
	let (services, activation) = signing_runtime_with_platform(granting_platform());
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	grant_auto_signing(&product_runtime(services, activation.clone()));
	let session = activation.accounts().current_session().expect("an active wallet");
	let cancel = truapi::CancellationToken::default();
	cancel.cancel();
	let cx = CallContext::with_parts("withdrawn-proof".to_string(), cancel);

	let signed =
		futures::executor::block_on(activation.accounts().sign_statement_store_product_payload(
			&session,
			&cx,
			&connection(&activation, "myapp.dot"),
			v01::ProductAccountId {
				dot_ns_identifier: ProductId::for_test("myapp.dot"),
				derivation_index: v01::DerivationIndex::Index(0),
			},
			b"statement".to_vec(),
		));

	assert_eq!(
		signed,
		Err(AuthorityError::Cancelled(crate::runtime::authority::AuthorityCancelError::new(
			"withdrawn-proof",
			truapi::CancellationReason::Cancelled,
		)))
	);
}

/// The user's own account, `identity`, asked to sign `hello world`.
fn identity_raw_request(identity: &schnorrkel::Keypair) -> HostSignRawWithLegacyAccountRequest {
	HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
		signer: subxt::utils::AccountId32(identity.public.to_bytes()).to_string(),
		payload: v01::RawPayload::Bytes { bytes: b"hello world".to_vec() },
	})
}

/// Bytes the user's account signs inside the `<Bytes>` watermark cannot pass
/// for a transaction's signing payload, so a trusted product gets that
/// signature the way it gets one from its own accounts.
#[test]
fn a_trusted_product_signs_watermarked_bytes_with_the_users_account_without_a_prompt() {
	let platform = granting_platform();
	let (services, activation) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime_for(services, activation, "dim2.paseo");
	let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX).unwrap();

	let HostSignRawWithLegacyAccountResponse::V1(response) = futures::executor::block_on(
		runtime
			.sign_raw_with_legacy_account(&CallContext::default(), identity_raw_request(&identity)),
	)
	.expect("a trusted product signs without confirmation");

	let signature =
		schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
	let watermarked = identity
		.public
		.verify_simple(b"substrate", b"<Bytes>hello world</Bytes>", &signature)
		.is_ok();
	assert_eq!((platform.sign_raw_reviews.lock().unwrap().len(), watermarked), (0, true));
}

/// Every other raw signature with the user's account is shown to the user:
/// unwatermarked bytes can be a transaction's signing payload, and a product
/// that is not trusted never skips the prompt.
#[test]
fn other_raw_signatures_with_the_users_account_still_ask() {
	for (product_id, watermarked) in [("dim2.paseo", false), ("myapp.dot", true)] {
		let platform = granting_platform();
		let (services, activation) = signing_runtime_with_platform(platform.clone());
		futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
			.expect("activation succeeds");
		let runtime = product_runtime_for(services, activation, product_id);
		let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX).unwrap();
		let request = identity_raw_request(&identity);
		let cx = CallContext::default();

		#[allow(deprecated)]
		let signed = futures::executor::block_on(async {
			if watermarked {
				runtime.sign_raw_with_legacy_account(&cx, request).await.is_ok()
			} else {
				runtime
					.sign_raw_unwatermarked_deprecated_with_legacy_account(&cx, request)
					.await
					.is_ok()
			}
		});

		assert_eq!(
			(signed, platform.sign_raw_reviews.lock().unwrap().len()),
			(false, 1),
			"{product_id}, watermarked: {watermarked}",
		);
	}
}
