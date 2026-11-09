use super::{super::LocalActivation, *};
use crate::{
	host_internal::{
		sso_messages::{
			self, GetAccountAliasResponse, OnExistingAllowancePolicy, RemoteMessage, RingVrfError,
			SsoAllocatedResource, SsoAllocationOutcome,
		},
		sso_wire::ResponseOutcome,
	},
	host_logic::{
		attestation::SigningContexts,
		product_account::{
			derive_identity_keypair, derive_product_subtree_keypair,
			derive_ring_vrf_domain_entropy, derive_root_keypair_from_entropy,
		},
		sso::pairing::{
			ResponderIdentity, SsoStatementData, VersionedHandshakeResponse,
			decode_app_handshake_data, decrypt_session_statement_data,
			decrypt_v2_handshake_response, derive_x25519_keypair_from_entropy,
			encrypt_v2_handshake_response, establish_responder_session_info,
			generate_pairing_device_identity,
		},
		statement_store::build_signed_statement,
	},
	platform::ProductId,
};
use parity_scale_codec::Encode;

/// The key a host advertises on chain must be the one it serves over
/// pairing. These derive independently, so a test that asks only one of
/// them cannot see the two drift apart — which is the failure this pins.
#[test]
fn the_advertised_chat_key_is_the_one_the_responder_serves() {
	const CHAT_ENTROPY: [u8; 16] = [0xAB; 16];
	const SUFFIX: &str = "paseo";

	let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
	let session = signing_host.wallet().current_session().unwrap();
	let host = generate_pairing_device_identity().unwrap();
	let peer = PairedSsoPeer {
		statement_account_id: host.statement_store_public_key,
		encryption_public_key: host.encryption_public_key,
	};
	let (_, answer) = signing_host.wallet().pairing_answer(&session, peer, [0; 32]).unwrap();
	let verified = decode_verified_statement_data(&answer, session.identity_account_id).unwrap();
	let VersionedHandshakeResponse::V2 { public_key, encrypted_message } =
		decode_app_handshake_data(&verified.data).unwrap();
	let v2::EncryptedResponse::Success(served) =
		decrypt_v2_handshake_response(host.encryption_secret_key, public_key, &encrypted_message)
			.unwrap()
	else {
		panic!("expected pairing success")
	};
	let registration = crate::host_logic::attestation::build_lite_registration(
		&CHAT_ENTROPY,
		SUFFIX,
		[0x11; 32],
		"headless",
		None,
		0,
		SigningContexts { people: None, asset_hub: None },
	)
	.expect("registration builds");

	assert_eq!(
		&registration.identifier_key[1..33],
		&X25519PrivateKey::from(served.identity_chat_private_key).public_key().0,
		"the advertised chat key is not the one served over pairing"
	);
}
use crate::{
	host_logic::statement_store::{SUBMIT_STATEMENT_METHOD, decode_verified_statement_data},
	platform::{
		CreateTransactionReview, HostInfo, LegacySigningAccount, Platform, PlatformInfo,
		SigningHostConfig,
	},
	runtime::{HostSession, services::RuntimeServices},
	test_support::{StubPlatform, recorded_rpc_method_count, test_spawner},
};
use std::sync::Arc;
use truapi::latest as api;

const ENTROPY: [u8; 16] = [0xab; 16];
/// The fixture's People chain is paseo-next-v2 (see `PEOPLE_METADATA`),
/// whose runtime carries the `paseo` network suffix.
const NETWORK_SUFFIX: &str = "paseo";

fn signing_fixture(platform: Arc<StubPlatform>) -> (Arc<RuntimeServices>, Arc<SigningHost>) {
	let platform: Arc<dyn Platform> = platform;
	let config = SigningHostConfig::new(
		HostInfo {
			name: "Polkadot Mobile".to_string(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Unknown,
		},
		PlatformInfo::default(),
		crate::test_support::test_stack(NETWORK_SUFFIX, [0; 32], [0xbb; 32], None),
	)
	.expect("signing host config is valid");
	let services = RuntimeServices::new(
		platform.clone(),
		config.host.host_info.clone(),
		config.stack.clone(),
		test_spawner(),
	);
	let signing_host = SigningHost::new(services.clone());
	futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	(services, signing_host)
}

#[test]
fn responder_advertises_and_signs_with_the_local_uid_identity() {
	let (_services, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
	let wallet_session = signing_host.wallet().current_session().unwrap();
	let local_identity = wallet_session.identity_account_id.unwrap();
	// The statement identity is the network's `uid.<suffix>` account, the
	// one the pairing host resolves a username for; a `.dot` account has
	// no lite record on a test network.
	assert_ne!(derive_identity_keypair(&ENTROPY, "dot").unwrap().public.to_bytes(), local_identity);

	let (_, host_encryption_public_key) = derive_x25519_keypair_from_entropy(&[0x42; 16], b"sso");
	let session = signing_host
		.wallet()
		.sso_session(
			&wallet_session,
			PairedSsoPeer {
				statement_account_id: [0x55; 32],
				encryption_public_key: host_encryption_public_key,
			},
		)
		.unwrap();
	let statement = build_signed_statement(
		&session,
		[0x66; 32],
		[0x77; 32],
		b"handshake".to_vec(),
		fresh_statement_expiry(),
	)
	.unwrap();
	let verified = decode_verified_statement_data(&statement, Some(local_identity)).unwrap();
	assert_eq!(verified.signer, local_identity);
}

#[test]
fn advertised_device_key_is_independent_of_the_identity() {
	let (services, _signing_host) = signing_fixture(Arc::new(StubPlatform::default()));

	let secret = futures::executor::block_on(services.device_encryption_secret()).unwrap();
	let advertised = X25519PrivateKey::from(secret).public_key().0;

	// The regression this guards: advertising the SSO channel key as the
	// device key makes every device sharing an identity indistinguishable.
	let (_, sso_public) = derive_x25519_keypair_from_entropy(&ENTROPY, b"sso");
	assert_ne!(advertised, sso_public);
}

/// Both handshake notices the signing host sends outside `Success` must be
/// readable by the host that advertised the keys, or the pairing host is
/// left on a screen no later message can move it off.
#[test]
fn the_pairing_notices_decode_on_the_pairing_host() {
	// Encrypt to the key a pairing host actually advertises, so each notice
	// is decrypted with the secret that host kept rather than one the test
	// chose for both sides.
	let pairing_identity = generate_pairing_device_identity().unwrap();
	let peer = PairedSsoPeer {
		statement_account_id: pairing_identity.statement_store_public_key,
		encryption_public_key: pairing_identity.encryption_public_key,
	};

	for response in [
		v2::EncryptedResponse::Pending(v2::Status::AllowanceAllocation),
		v2::EncryptedResponse::Failed("no free StatementStore slot".to_string()),
	] {
		let VersionedHandshakeResponse::V2 { encrypted_message, public_key } =
			encrypt_v2_handshake_response(peer.encryption_public_key, &response).unwrap();

		assert_eq!(
			decrypt_v2_handshake_response(
				pairing_identity.encryption_secret_key,
				public_key,
				&encrypted_message,
			)
			.unwrap(),
			response,
		);
	}
}

fn pairing_deeplink(peer: PairedSsoPeer) -> String {
	let proposal = VersionedHandshakeProposal::V2(v2::Proposal {
		device: v2::Device {
			statement_account_id: peer.statement_account_id,
			encryption_public_key: peer.encryption_public_key,
		},
		metadata: vec![v2::MetadataEntry(v2::MetadataKey::HostName, "paired host".to_string())],
	});
	format!("polkadotapp://pair?handshake={}", hex::encode(proposal.encode()))
}

#[derive(Default)]
struct RecordingPairingObserver {
	paired: std::sync::Mutex<Vec<PairedSsoPeer>>,
}

impl RecordingPairingObserver {
	fn paired(&self) -> Vec<PairedSsoPeer> {
		self.paired.lock().expect("paired device list mutex poisoned").clone()
	}
}

impl DevicePairingObserver for RecordingPairingObserver {
	fn device_paired(&self, device: PairedSsoPeer) {
		self.paired.lock().expect("paired device list mutex poisoned").push(device);
	}
}

fn pairing_fixture(submit_status: &'static str) -> (Arc<RuntimeServices>, Arc<SigningHost>) {
	signing_fixture(Arc::new(StubPlatform {
		rpc_method_responses: submit_answers(submit_status, 1),
		..Default::default()
	}))
}

/// Without this the host never learns a device paired, so no contact is
/// told a new device joined.
#[test]
fn a_paired_device_is_reported_to_the_host() {
	let peer = paired_peer();
	let (services, signing_host) = pairing_fixture("new");
	let observer = Arc::new(RecordingPairingObserver::default());
	assert!(services.install_device_pairing_observer(observer.clone()));

	futures::executor::block_on(
		SsoResponderService::new(services, signing_host).establish_pairing(&pairing_deeplink(peer)),
	)
	.expect("the handshake is answered");

	assert_eq!(observer.paired(), vec![peer]);
}

/// Reporting a handshake the peer never received would have the host
/// announce a device over a session that does not exist.
#[test]
fn a_handshake_that_never_landed_reports_no_device() {
	let peer = paired_peer();
	// Neither "new" nor "known", so the submit is rejected.
	let (services, signing_host) = pairing_fixture("ignored");
	let observer = Arc::new(RecordingPairingObserver::default());
	assert!(services.install_device_pairing_observer(observer.clone()));

	let failure = futures::executor::block_on(
		SsoResponderService::new(services, signing_host).establish_pairing(&pairing_deeplink(peer)),
	)
	.expect_err("a rejected handshake submit fails the pairing");
	// Without this the test would also pass on a failure from earlier,
	// leaving the rule unexercised.
	assert!(
		failure.contains("statement_submit not accepted"),
		"pairing failed before the handshake submit: {failure}"
	);

	assert_eq!(observer.paired(), Vec::new());
}

#[test]
fn pairing_without_an_observer_still_answers_the_handshake() {
	let peer = paired_peer();
	let (services, signing_host) = pairing_fixture("new");

	futures::executor::block_on(
		SsoResponderService::new(services, signing_host).establish_pairing(&pairing_deeplink(peer)),
	)
	.expect("the handshake is answered without an observer");
}

/// The pairing host waits on the failure notice with no deadline, so a
/// wallet change between the two notices must not silence it.
#[test]
fn a_pairing_failure_is_sent_after_the_wallet_changes() {
	let peer = PairedSsoPeer {
		statement_account_id: [0x31; 32],
		encryption_public_key: X25519PrivateKey::from([0x42; 32]).public_key().0,
	};
	let submitted = || ("statement_submit", r#"{"status":"new"}"#.to_string());
	let (services, signing_host) = signing_fixture(Arc::new(StubPlatform {
		rpc_method_responses: vec![submitted(), submitted()],
		..Default::default()
	}));
	let responder = SsoResponderService::new(services, signing_host.clone());
	let announced = futures::executor::block_on(
		responder.notify_pairing_allowance_allocation(&pairing_deeplink(peer)),
	)
	.expect("the allocation is announced");

	futures::executor::block_on(signing_host.activate_local_session(vec![0xCD; 16]))
		.expect("another wallet activates");

	futures::executor::block_on(
		responder.notify_pairing_failed(&announced, "no free StatementStore slot".to_string()),
	)
	.expect("the failure notice is still sent");
}

/// The peer writes what the pairing prompt calls it. Rendering that as
/// sent lets it reorder or pad out the prompt's own words around itself.
#[test]
fn proposal_metadata_is_sanitized_before_a_host_prompts_with_it() {
	let long = "n".repeat(MAX_PAIRING_METADATA_CHARS + 10);
	let proposal = VersionedHandshakeProposal::V2(v2::Proposal {
		device: v2::Device { statement_account_id: [0x31; 32], encryption_public_key: [0x42; 32] },
		metadata: vec![
			// A right-to-left override and a newline, either of which
			// rewrites what the surrounding prompt text reads as.
			v2::MetadataEntry(v2::MetadataKey::HostName, "  Wallet\u{202e}trebo\n  ".to_string()),
			v2::MetadataEntry(v2::MetadataKey::HostVersion, long.clone()),
			v2::MetadataEntry(v2::MetadataKey::HostIcon, "\u{2066}\t ".to_string()),
			v2::MetadataEntry(v2::MetadataKey::Custom("seat".to_string()), "3".to_string()),
		],
	});
	let deeplink = format!("polkadotapp://pair?handshake={}", hex::encode(proposal.encode()));

	assert_eq!(
		PairingProposal::from_deeplink(&deeplink)
			.expect("a well-formed deeplink decodes")
			.metadata,
		PairingProposalMetadata {
			host_name: Some("Wallettrebo".to_string()),
			host_version: Some("n".repeat(MAX_PAIRING_METADATA_CHARS)),
			// Nothing renderable survived, so the host has no name to show
			// rather than an empty one.
			host_icon: None,
			// Neither key was sent, and a key outside the well-known set
			// is not one a prompt has a place for.
			platform_type: None,
			platform_version: None,
		}
	);
}

#[test]
fn pairing_deeplink_exposes_the_public_material_needed_to_resume() {
	let proposal = VersionedHandshakeProposal::V2(v2::Proposal {
		device: v2::Device { statement_account_id: [0x31; 32], encryption_public_key: [0x42; 32] },
		metadata: vec![v2::MetadataEntry(v2::MetadataKey::HostName, "paired host".to_string())],
	});
	let deeplink = format!("polkadotapp://pair?handshake={}", hex::encode(proposal.encode()));

	assert_eq!(
		PairedSsoPeer::from_deeplink(&deeplink).unwrap(),
		PairedSsoPeer { statement_account_id: [0x31; 32], encryption_public_key: [0x42; 32] }
	);
}

#[test]
fn persisted_peer_rebuilds_the_original_responder_session() {
	let peer = PairedSsoPeer {
		statement_account_id: [0x53; 32],
		encryption_public_key: X25519PrivateKey::from([0x64; 32]).public_key().0,
	};
	let statement = derive_identity_keypair(&ENTROPY, NETWORK_SUFFIX).unwrap();
	let (encryption_secret_key, encryption_public_key) =
		derive_x25519_keypair_from_entropy(&ENTROPY, b"sso");
	let identity = ResponderIdentity {
		statement_secret: statement.secret.to_bytes(),
		statement_public_key: statement.public.to_bytes(),
		encryption_secret_key,
		encryption_public_key,
	};
	let mut expected = establish_responder_session_info(
		&identity,
		peer.statement_account_id,
		peer.encryption_public_key,
	)
	.unwrap();
	let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
	let selected = signing_host.wallet().current_session().unwrap();
	let resumed = signing_host.wallet().sso_session(&selected, peer).unwrap();

	assert_eq!(
		crate::host_logic::statement_store::statement_public_key_from_secret(resumed.ss_secret)
			.unwrap(),
		expected.ss_public_key
	);
	expected.ss_secret = resumed.ss_secret;

	assert_eq!(resumed, expected);
}

#[test]
fn replayed_disconnect_still_terminates_the_peer() {
	let disconnect = IncomingSsoRequest {
		request_id: "disconnect-1".to_string(),
		expires_at_unix_secs: Some(200),
		messages: vec![RemoteMessage {
			message_id: "message-1".to_string(),
			data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
		}],
	};
	let ordinary = IncomingSsoRequest {
		request_id: "empty-1".to_string(),
		expires_at_unix_secs: Some(200),
		messages: Vec::new(),
	};

	assert_eq!(
		(duplicate_request_exit(&disconnect), duplicate_request_exit(&ordinary)),
		(Some(ResponderExit::PeerDisconnected), None)
	);
}

fn answer(
	signing_host: &Arc<SigningHost>,
	message_id: &str,
	request: v1::RemoteMessage,
) -> v1::RemoteMessage {
	let service = SigningHostSsoService::new(signing_host.clone());
	let message =
		RemoteMessage { message_id: message_id.to_string(), data: RemoteMessageData::V1(request) };
	let Dispatch::Response(answer) = futures::executor::block_on(service.answer(message)) else {
		panic!("expected a response");
	};
	let RemoteMessageData::V1(data) = answer.message.data;
	data
}

#[test]
fn dispatch_without_session_distinguishes_requests_responses_and_disconnects() {
	let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
	let service = SigningHostSsoService::new(signing_host);
	let dispatch = |data| {
		futures::executor::block_on(service.dispatch(
			None,
			RemoteMessage { message_id: "m-1".to_string(), data: RemoteMessageData::V1(data) },
		))
	};

	assert_eq!(dispatch(v1::RemoteMessage::Disconnected), Dispatch::Disconnected);
	assert_eq!(
		dispatch(v1::RemoteMessage::ProductSubtreeResponse(sso_messages::Response {
			responding_to: "m-1".to_string(),
			payload: Ok([7; 32]),
		})),
		Dispatch::NotARequest("ProductSubtreeResponse"),
	);
	let Dispatch::Response(answer) =
		dispatch(v1::RemoteMessage::ProductSubtreeRequest(sso_messages::ProductSubtreeRequest {
			product_id: ProductId::for_test("myapp.dot"),
		}))
	else {
		panic!("expected a disconnected error response");
	};
	let RemoteMessageData::V1(v1::RemoteMessage::ProductSubtreeResponse(response)) =
		answer.message.data
	else {
		panic!("expected the request's response variant");
	};
	assert_eq!(response.responding_to, "m-1");
	assert_eq!(response.payload, Err("signing host session is not active".to_string()));
}

#[test]
fn response_summary_reports_protocol_errors_without_multiline_output() {
	let payload: GetAccountAliasResponse =
		Err(RingVrfError::Unknown { reason: "chain RPC\ntimed out".to_string() });

	let result = ResponseOutcome::from_payload(&payload);
	let summary = response_cli_summary(
		"SSO response sent",
		"get_account_alias",
		"statement-1",
		"alias-1",
		"alias-1:response",
		&result,
		42,
	);

	assert_eq!(result.outcome, "error");
	assert_eq!(
		summary,
		"SSO response sent · get_account_alias · error\n\
         statement_request_id=statement-1\n\
         responding_to=alias-1\n\
         response_message_id=alias-1:response\n\
         elapsed_ms=42\n\
        reason=Unknown: chain RPC timed out"
	);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn allocation_failure_details_reach_the_response_transcript() {
	let (_, signing_host) = signing_fixture(Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		chain_connect_error: Some("allocation node unavailable"),
		..StubPlatform::default()
	}));
	let service = SigningHostSsoService::new(signing_host);
	let request = RemoteMessage::request(
		"allocation-1".to_string(),
		sso_messages::ResourceAllocationRequest {
			calling_product_id: ProductId::for_test("myapp.dot"),
			resources: vec![
				api::AllocatableResource::AutoSigning,
				api::AllocatableResource::BulletinAllowance,
				api::AllocatableResource::StatementStoreAllowance,
			],
			on_existing: OnExistingAllowancePolicy::Ignore,
		},
	);
	let Dispatch::Response(answer) = futures::executor::block_on(service.answer(request)) else {
		panic!("expected an allocation response");
	};
	let RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(response)) =
		answer.message.data
	else {
		panic!("expected an allocation response");
	};
	assert!(matches!(
		response.payload.unwrap().as_slice(),
		[
			SsoAllocationOutcome::Allocated(SsoAllocatedResource::AutoSigning { .. }),
			SsoAllocationOutcome::NotAvailable,
			SsoAllocationOutcome::NotAvailable,
		]
	));
	assert_eq!(answer.outcome.outcome, "partial");
	let reason = answer.outcome.reason.as_deref().unwrap();
	assert!(reason.starts_with("1 of 3 requested resources allocated; 2 unavailable"));
	assert_eq!(reason.matches("allocation node unavailable").count(), 2, "{reason}");
	assert!(!reason.contains(['\r', '\n']));
	let cli = response_cli_summary(
		"SSO response sent",
		"resource_allocation",
		"allocation-1",
		"allocation-1",
		&answer.message.message_id,
		&answer.outcome,
		0,
	);
	assert!(cli.contains(&format!("reason={reason}")));
}

#[test]
fn account_alias_requires_confirmation_for_cross_product_request() {
	let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));

	let response = answer(
		&signing_host,
		"alias-1",
		v1::RemoteMessage::GetAccountAliasRequest(sso_messages::ProductRequest {
			calling_product_id: ProductId::for_test("myapp.dot"),
			payload: api::HostAccountGetAliasRequest {
				key_handle: api::ProductAccountId {
					dot_ns_identifier: ProductId::for_test("peopl.dot"),
					derivation_index: api::DerivationIndex::Index(0),
				},
				context: api::ProductProofContext {
					product_id: ProductId::for_test("other.dot"),
					suffix: api::DerivationIndex::Index(0),
				},
				ring_location: api::RingLocation { chain_id: [0; 32], junctions: vec![] },
			},
		}),
	);

	let v1::RemoteMessage::GetAccountAliasResponse(response) = response else {
		panic!("expected alias response");
	};
	assert_eq!(response.payload.unwrap_err(), RingVrfError::Rejected);
}

#[test]
fn resource_allocation_requires_confirmation_before_allocation() {
	let platform = Arc::new(StubPlatform::default());
	let (_, signing_host) = signing_fixture(platform.clone());

	let response = answer(
		&signing_host,
		"alloc-1",
		v1::RemoteMessage::ResourceAllocationRequest(sso_messages::ResourceAllocationRequest {
			calling_product_id: ProductId::for_test("myapp.dot"),
			resources: vec![api::AllocatableResource::StatementStoreAllowance],
			on_existing: sso_messages::OnExistingAllowancePolicy::Ignore,
		}),
	);

	let v1::RemoteMessage::ResourceAllocationResponse(response) = response else {
		panic!("expected resource allocation response");
	};
	assert_eq!(response.payload.unwrap(), vec![SsoAllocationOutcome::Rejected]);

	// The confirmation review names the beneficiary product so the user
	// knows which product receives the delegated allowance key.
	let reviews = platform
		.resource_allocation_reviews
		.lock()
		.expect("resource allocation review list mutex poisoned");
	assert_eq!(reviews.len(), 1);
	assert_eq!(reviews[0].calling_product_id, ProductId::for_test("myapp.dot"));
}

#[test]
fn auto_signing_allocation_returns_the_product_subtree_secret() {
	let platform =
		Arc::new(StubPlatform { resource_allocation_confirmed: true, ..StubPlatform::default() });
	let (_, signing_host) = signing_fixture(platform);
	let root = derive_root_keypair_from_entropy(&ENTROPY).expect("root derives");
	let expected_secret = derive_product_subtree_keypair(&root, &ProductId::for_test("myapp.dot"))
		.expect("product subtree secret derives")
		.secret
		.to_bytes();
	let expected_ring_vrf_domain_entropy =
		derive_ring_vrf_domain_entropy(&ENTROPY, &ProductId::for_test("myapp.dot"))
			.expect("ring-VRF domain entropy derives");

	let response = answer(
		&signing_host,
		"alloc-auto-signing",
		v1::RemoteMessage::ResourceAllocationRequest(sso_messages::ResourceAllocationRequest {
			calling_product_id: ProductId::for_test("myapp.dot"),
			resources: vec![api::AllocatableResource::AutoSigning],
			on_existing: sso_messages::OnExistingAllowancePolicy::Ignore,
		}),
	);

	let v1::RemoteMessage::ResourceAllocationResponse(response) = response else {
		panic!("expected resource allocation response");
	};
	assert_eq!(
		response.payload.unwrap(),
		vec![SsoAllocationOutcome::Allocated(SsoAllocatedResource::AutoSigning {
			product_root_private_key: expected_secret,
			ring_vrf_domain_entropy: expected_ring_vrf_domain_entropy,
		})]
	);
}

fn allocation_after_session_change(replacement: Option<Vec<u8>>) {
	use futures::{FutureExt, channel::oneshot};

	let (release, gate) = oneshot::channel();
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		resource_allocation_confirmation_gate: std::sync::Mutex::new(Some(gate)),
		..StubPlatform::default()
	});
	let (_, signing_host) = signing_fixture(platform.clone());
	let service = SigningHostSsoService::new(signing_host.clone());
	let message = RemoteMessage::request(
		"alloc-stale".to_string(),
		sso_messages::ResourceAllocationRequest {
			calling_product_id: ProductId::for_test("myapp.dot"),
			resources: vec![api::AllocatableResource::AutoSigning],
			on_existing: OnExistingAllowancePolicy::Ignore,
		},
	);

	futures::executor::block_on(async {
		let answer = service.answer(message);
		futures::pin_mut!(answer);
		assert!(answer.as_mut().now_or_never().is_none());
		assert_eq!(platform.resource_allocation_reviews.lock().unwrap().len(), 1);

		signing_host.disconnect().await;
		if let Some(entropy) = replacement {
			signing_host.activate_local_session(entropy).await.unwrap();
		}
		release.send(()).unwrap();

		let Dispatch::Response(answer) = answer.await else {
			panic!("expected an allocation response");
		};
		let RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(response)) =
			answer.message.data
		else {
			panic!("expected an allocation response");
		};
		assert_eq!(response.responding_to, "alloc-stale");
		assert!(response.payload.is_err(), "stale consent must not release keys");
		assert!(platform.sent_rpc.lock().unwrap().is_empty());
	});
}

fn allocation_request(message_id: &str) -> RemoteMessage {
	RemoteMessage::request(
		message_id.to_string(),
		sso_messages::ResourceAllocationRequest {
			calling_product_id: ProductId::for_test("myapp.dot"),
			resources: vec![api::AllocatableResource::AutoSigning],
			on_existing: OnExistingAllowancePolicy::Ignore,
		},
	)
}

fn cancel(message_id: &str, target: &str) -> RemoteMessage {
	RemoteMessage {
		message_id: message_id.to_string(),
		data: RemoteMessageData::V1(v1::RemoteMessage::Cancel(sso_messages::Withdrawal {
			message_id: target.to_string(),
		})),
	}
}

/// The phone spends once the person approves an allocation, so a
/// withdrawal that arrives while the prompt is open must end the prompt
/// with nothing allocated and nothing to post.
#[test]
fn a_cancel_withdraws_an_allocation_while_its_prompt_is_open() {
	let (_release, gate) = futures::channel::oneshot::channel();
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		resource_allocation_confirmation_gate: std::sync::Mutex::new(Some(gate)),
		..StubPlatform::default()
	});
	let (_, signing_host) = signing_fixture(platform.clone());
	let service = SigningHostSsoService::new(signing_host);
	let allocation = service.answer(allocation_request("alloc-1"));
	futures::pin_mut!(allocation);
	assert!(allocation.as_mut().now_or_never().is_none());

	let withdrawal = service
		.answer(cancel("cancel-1", "alloc-1"))
		.now_or_never()
		.expect("a withdrawal is answered at once");
	let allocation = allocation
		.as_mut()
		.now_or_never()
		.expect("a withdrawn request stops waiting on its prompt");

	assert_eq!(
		(withdrawal, allocation, platform.sent_rpc.lock().unwrap().len()),
		(Dispatch::Withdraw("alloc-1".to_string()), Dispatch::Withdrawn, 0)
	);
}

/// Statements arrive out of order, so a withdrawal can land first. The
/// request it names must then never reach the person at all.
#[test]
fn a_cancel_that_arrives_first_stops_its_request_before_it_prompts() {
	let platform =
		Arc::new(StubPlatform { resource_allocation_confirmed: true, ..StubPlatform::default() });
	let (_, signing_host) = signing_fixture(platform.clone());
	let service = SigningHostSsoService::new(signing_host);

	futures::executor::block_on(service.answer(cancel("cancel-1", "alloc-1")));
	let allocation = futures::executor::block_on(service.answer(allocation_request("alloc-1")));

	assert_eq!(allocation, Dispatch::Withdrawn);
	assert!(platform.resource_allocation_reviews.lock().unwrap().is_empty());
}

fn statement(request_id: &str, message: RemoteMessage) -> Vec<IncomingSsoRequest> {
	vec![IncomingSsoRequest {
		request_id: request_id.to_string(),
		expires_at_unix_secs: None,
		messages: vec![message],
	}]
}

/// Without this a `Cancel` waits behind the very request it withdraws,
/// and reaches it only after the person has answered.
#[test]
fn a_cancel_is_read_while_the_request_it_names_is_being_served() {
	let withdrawals = SsoWithdrawals::default();
	let (pages, statements) = futures::channel::mpsc::unbounded();
	pages
		.unbounded_send(Ok(statement("stmt-1", allocation_request("alloc-1"))))
		.unwrap();
	let serving = serve_pages(statements, &withdrawals, |incoming| {
		let request = withdrawals.begin(&incoming.messages[0].message_id);
		async move {
			if let Some(request) = request {
				request.cancel.cancelled().await;
			}
			Ok(Some(ResponderExit::PeerDisconnected))
		}
	});
	futures::pin_mut!(serving);
	assert!(serving.as_mut().now_or_never().is_none());

	pages
		.unbounded_send(Ok(statement("stmt-2", cancel("cancel-1", "alloc-1"))))
		.unwrap();

	assert_eq!(serving.as_mut().now_or_never(), Some(Ok(ResponderExit::PeerDisconnected)));
}

/// The request being served has been acked and recorded as started, so
/// redelivery would never answer it. A broken subscription must let it
/// finish before the loop reports the failure.
#[test]
fn a_failing_subscription_lets_the_request_being_served_finish() {
	let withdrawals = SsoWithdrawals::default();
	let (pages, statements) = futures::channel::mpsc::unbounded();
	let (release, gate) = futures::channel::oneshot::channel::<()>();
	let mut gate = Some(gate);
	pages
		.unbounded_send(Ok(statement("stmt-1", allocation_request("alloc-1"))))
		.unwrap();
	let serving = serve_pages(statements, &withdrawals, |_| {
		let gate = gate.take();
		async move {
			if let Some(gate) = gate {
				let _ = gate.await;
			}
			Ok(None)
		}
	});
	futures::pin_mut!(serving);
	assert!(serving.as_mut().now_or_never().is_none());

	pages.unbounded_send(Err("subscription failed".to_string())).unwrap();
	let before_the_request_finished = serving.as_mut().now_or_never();
	let once_it_finished = before_the_request_finished.is_none().then(|| {
		let _ = release.send(());
		serving.as_mut().now_or_never()
	});

	assert_eq!(
		(before_the_request_finished, once_it_finished),
		(None, Some(Some(Err("subscription failed".to_string()))))
	);
}

fn paired_peer() -> PairedSsoPeer {
	PairedSsoPeer {
		statement_account_id: [0x31; 32],
		encryption_public_key: X25519PrivateKey::from([0x42; 32]).public_key().0,
	}
}

fn subtree_request() -> IncomingSsoRequest {
	IncomingSsoRequest {
		request_id: "stmt-1".to_string(),
		expires_at_unix_secs: None,
		messages: vec![RemoteMessage::request(
			"subtree-1".to_string(),
			sso_messages::ProductSubtreeRequest { product_id: ProductId::for_test("browse.dot") },
		)],
	}
}

/// Serve one product subtree request from [`paired_peer`] over `platform`.
fn serve_subtree_request(platform: Arc<StubPlatform>) -> Result<Option<ResponderExit>, String> {
	deliver_subtree_request(platform, 1).remove(0)
}

/// Deliver one product subtree request from [`paired_peer`] `deliveries`
/// times to one responder over `platform`.
fn deliver_subtree_request(
	platform: Arc<StubPlatform>,
	deliveries: usize,
) -> Vec<Result<Option<ResponderExit>, String>> {
	let peer = paired_peer();
	let replay_scope =
		peer.replay_scope(derive_root_keypair_from_entropy(&ENTROPY).unwrap().public.to_bytes());
	let (services, signing_host) = signing_fixture(platform);
	let session = paired_session(&signing_host);
	let service = SigningHostSsoService::new(signing_host.clone());
	let responder = SsoResponderService::new(services, signing_host);
	(0..deliveries)
		.map(|_| {
			futures::executor::block_on(serve_statement(
				&responder,
				&service,
				&session,
				replay_scope,
				subtree_request(),
			))
		})
		.collect()
}

/// The statement data the responder published on `platform`, oldest first.
fn published_statement_data(platform: &StubPlatform) -> Vec<SsoStatementData> {
	let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
	let session = paired_session(&signing_host);
	platform
		.sent_rpc
		.lock()
		.unwrap()
		.iter()
		.map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
		.filter(|request| request["method"] == SUBMIT_STATEMENT_METHOD)
		.map(|request| {
			let statement_hex = request["params"][0].as_str().unwrap();
			let statement = hex::decode(statement_hex.trim_start_matches("0x")).unwrap();
			let verified = decode_verified_statement_data(&statement, None).unwrap();
			decrypt_session_statement_data(&session, &verified.data).unwrap()
		})
		.collect()
}

/// The session the fixture wallet serves to [`paired_peer`].
fn paired_session(signing_host: &SigningHost) -> SsoSessionInfo {
	let wallet = signing_host.wallet();
	wallet.sso_session(&wallet.current_session().unwrap(), paired_peer()).unwrap()
}

fn submit_answers(status: &str, count: usize) -> Vec<(&'static str, String)> {
	vec![(SUBMIT_STATEMENT_METHOD, format!(r#"{{"status":"{status}"}}"#)); count]
}

/// One refused connection must not end the session.
#[test]
fn a_failed_connect_does_not_end_the_session() {
	let platform = Arc::new(StubPlatform {
		chain_connect_failures: std::sync::Mutex::new(["Connection reset by peer"].into()),
		// The ack, then the response.
		rpc_method_responses: submit_answers("new", 2),
		..StubPlatform::default()
	});

	let served = serve_subtree_request(platform.clone());

	assert_eq!(
		(served, recorded_rpc_method_count(&platform.sent_rpc, SUBMIT_STATEMENT_METHOD)),
		(Ok(None), 2)
	);
}

/// A request that ran is not run again, so its answer must outlast the
/// ack's resend budget.
#[test]
fn an_answer_is_sent_until_the_people_chain_takes_it() {
	let platform = Arc::new(StubPlatform {
		chain_connect_failures: std::sync::Mutex::new(
			vec!["Connection reset by peer"; MAX_RESENDS as usize + 1].into(),
		),
		rpc_method_responses: submit_answers("new", 1),
		..StubPlatform::default()
	});
	let (services, signing_host) = signing_fixture(platform.clone());
	let session = paired_session(&signing_host);
	let service = SigningHostSsoService::new(signing_host);

	let answered = futures::executor::block_on(serve_request(
		&services,
		&service,
		&session,
		subtree_request(),
		&mut Vec::new(),
	));

	assert_eq!(
		(answered, recorded_rpc_method_count(&platform.sent_rpc, SUBMIT_STATEMENT_METHOD)),
		(Ok(None), 1)
	);
}

/// A rejection is the store's answer, so resending would only repeat it.
#[test]
fn a_rejected_ack_is_not_sent_again() {
	let platform = Arc::new(StubPlatform {
		rpc_method_responses: submit_answers("ignored", MAX_RESENDS as usize + 1),
		..StubPlatform::default()
	});

	let served = serve_subtree_request(platform.clone());

	assert_eq!(
		(served.is_err(), recorded_rpc_method_count(&platform.sent_rpc, SUBMIT_STATEMENT_METHOD)),
		(true, 1)
	);
}

/// A request that ran is not run again, so an answer the People chain
/// refused has to be published when the request is delivered again.
#[test]
fn a_refused_answer_is_published_when_the_request_is_delivered_again() {
	let platform = Arc::new(StubPlatform {
		// Ack, refused answer, then the ack and answer for the redelivery.
		rpc_method_responses: ["new", "ignored", "new", "new"]
			.into_iter()
			.flat_map(|status| submit_answers(status, 1))
			.collect(),
		..StubPlatform::default()
	});

	let [refused, redelivered] = deliver_subtree_request(platform.clone(), 2).try_into().unwrap();

	let answers = published_statement_data(&platform)
		.into_iter()
		.filter(|data| matches!(data, SsoStatementData::Request { .. }))
		.collect::<Vec<_>>();
	let [refused_answer, published_answer] = answers.as_slice() else {
		panic!("expected the answer and its republication, got {answers:?}");
	};
	assert_eq!((refused.is_err(), redelivered, published_answer), (true, Ok(None), refused_answer));
}

/// A request whose ack failed must run when it is delivered again, not be
/// skipped as already handled.
#[test]
fn a_request_whose_ack_failed_is_answered_when_delivered_again() {
	let core_storage = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
	let unreachable = Arc::new(StubPlatform {
		chain_connect_error: Some("Connection reset by peer"),
		local_storage: core_storage.clone(),
		..StubPlatform::default()
	});
	let reconnected = Arc::new(StubPlatform {
		rpc_method_responses: submit_answers("new", 2),
		local_storage: core_storage,
		..StubPlatform::default()
	});

	let failed_ack = serve_subtree_request(unreachable);
	let redelivered = serve_subtree_request(reconnected.clone());

	assert_eq!(
		(
			failed_ack.is_err(),
			redelivered,
			recorded_rpc_method_count(&reconnected.sent_rpc, SUBMIT_STATEMENT_METHOD)
		),
		(true, Ok(None), 2)
	);
}

#[test]
fn resource_consent_cannot_authorize_a_replacement_account() {
	allocation_after_session_change(Some(vec![0xcd; 16]));
}

#[test]
fn resource_consent_cannot_survive_same_account_reactivation() {
	allocation_after_session_change(Some(ENTROPY.to_vec()));
}

#[test]
fn resource_consent_cannot_survive_disconnect() {
	allocation_after_session_change(None);
}

#[test]
fn legacy_transaction_request_uses_the_controlled_identity_account() {
	let platform = Arc::new(StubPlatform {
		create_transaction_confirmed: true,
		chain_connect_error: Some("fixture has no live chain"),
		..StubPlatform::default()
	});
	let (_, signing_host) = signing_fixture(platform.clone());
	let identity = derive_identity_keypair(&ENTROPY, NETWORK_SUFFIX).unwrap();
	let payload = api::LegacyAccountTxPayload {
		signer: identity.public.to_bytes(),
		genesis_hash: [0xaa; 32],
		call_data: vec![0x00, 0x00],
		extensions: vec![api::TxPayloadExtension {
			id: "CheckNonce".to_string(),
			extra: vec![1],
			additional_signed: vec![2, 3],
		}],
		tx_ext_version: 0,
	};

	let response = answer(
		&signing_host,
		"legacy-tx-1",
		v1::RemoteMessage::CreateTransactionWithLegacyAccountRequest(
			sso_messages::CreateTransactionWithLegacyAccountRequest {
				payload: sso_messages::CreateTransactionLegacyPayload::V1(payload.clone()),
			},
		),
	);

	let v1::RemoteMessage::CreateTransactionResponse(response) = response else {
		panic!("expected create transaction response");
	};
	// The identity key is accepted, so the request gets as far as reading
	// the runtime metadata.
	let error = response.payload.expect_err("fixture has no chain metadata");
	assert!(format!("{error:?}").contains("cannot load chain metadata"), "{error:?}");
	assert_eq!(
		*platform.create_transaction_reviews.lock().unwrap(),
		vec![CreateTransactionReview::LegacyAccount {
			account: LegacySigningAccount::Identity,
			payload,
		}]
	);
}

#[test]
fn a_legacy_request_for_another_account_is_refused_before_the_user_sees_it() {
	let platform = Arc::new(StubPlatform {
		sign_raw_confirmed: true,
		create_transaction_confirmed: true,
		..StubPlatform::default()
	});
	let (_, signing_host) = signing_fixture(platform.clone());
	let other_account = [7; 32];

	let raw = answer(
		&signing_host,
		"legacy-raw-other",
		v1::RemoteMessage::SignRawWithLegacyAccountRequest(
			sso_messages::SignRawWithLegacyAccountRequest {
				account: other_account,
				data: api::RawPayload::Bytes { bytes: vec![1] },
			},
		),
	);
	let transaction = answer(
		&signing_host,
		"legacy-tx-other",
		v1::RemoteMessage::CreateTransactionWithLegacyAccountRequest(
			sso_messages::CreateTransactionWithLegacyAccountRequest {
				payload: sso_messages::CreateTransactionLegacyPayload::V1(
					api::LegacyAccountTxPayload {
						signer: other_account,
						genesis_hash: [0xaa; 32],
						call_data: vec![0x00, 0x00],
						extensions: vec![],
						tx_ext_version: 0,
					},
				),
			},
		),
	);

	let v1::RemoteMessage::SignRawWithLegacyAccountResponse(raw) = raw else {
		panic!("expected legacy raw signing response");
	};
	let v1::RemoteMessage::CreateTransactionResponse(transaction) = transaction else {
		panic!("expected create transaction response");
	};
	assert!(raw.payload.is_err());
	assert!(transaction.payload.is_err());
	assert!(platform.sign_raw_reviews.lock().unwrap().is_empty());
	assert!(platform.create_transaction_reviews.lock().unwrap().is_empty());
}

#[test]
fn product_subtree_request_is_consent_free_and_hard_derived() {
	let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
	let response = answer(
		&signing_host,
		"subtree-1",
		v1::RemoteMessage::ProductSubtreeRequest(sso_messages::ProductSubtreeRequest {
			product_id: ProductId::for_test("browse.dot"),
		}),
	);

	let v1::RemoteMessage::ProductSubtreeResponse(response) = response else {
		panic!("expected product subtree response");
	};
	let root = derive_root_keypair_from_entropy(&ENTROPY).expect("fixture entropy derives root");
	let expected = crate::host_logic::product_account::derive_product_subtree_keypair(
		&root,
		&ProductId::for_test("browse.dot"),
	)
	.expect("fixture derives subtree")
	.public
	.to_bytes();
	assert_eq!(response.responding_to, "subtree-1");
	assert_eq!(response.payload, Ok(expected));
}

#[test]
fn decode_failure_request_ids_dedup_and_bound() {
	let mut served = DecodeFailureRequestIds::new();

	// First sighting is served; an immediate duplicate is rejected.
	assert!(served.insert("req-a".to_string()));
	assert!(!served.insert("req-a".to_string()));

	// Fill to capacity with distinct ids; the set never exceeds the bound.
	for i in 0..MAX_DECODE_FAILURE_REQUEST_IDS {
		served.insert(format!("fill-{i}"));
	}
	assert_eq!(served.seen.len(), MAX_DECODE_FAILURE_REQUEST_IDS);
	assert_eq!(served.order.len(), MAX_DECODE_FAILURE_REQUEST_IDS);

	// The oldest id ("req-a") has been evicted, so it is accepted again,
	// while a recent id is still deduped — memory stays bounded regardless
	// of how many ids a peer streams.
	assert!(served.insert("req-a".to_string()));
	assert!(!served.insert(format!("fill-{}", MAX_DECODE_FAILURE_REQUEST_IDS - 1)));
	assert_eq!(served.seen.len(), MAX_DECODE_FAILURE_REQUEST_IDS);
}
