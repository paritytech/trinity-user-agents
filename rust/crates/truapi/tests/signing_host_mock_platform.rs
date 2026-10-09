//! Keystone wiring test: a signing host backed by `MockPlatform` signs.
//!
//! The in-memory mock has only ever backed the pairing host, where every
//! signature is answered by a remote device and so parks against a mock that
//! never answers. That left an open question underneath every plan to test
//! products against it: whether the mock can back a host that owns the keys,
//! or whether signing drags in chain access the mock cannot serve.
//!
//! It can, and it does not. `sign_raw` reaches the key through a remote
//! *permission* named `ChainSubmit` — a policy question the platform answers
//! locally, not an RPC — so the only host calls on the path are ones the mock
//! already implements. This drives a real `signing_sign_raw` product frame
//! through the dispatcher into a `SigningHostRuntime` whose entire platform is
//! the mock, and verifies the returned sr25519 signature against a key derived
//! independently from the activation entropy. Nothing is stubbed between the
//! seed and the signature.

#![cfg(not(target_arch = "wasm32"))]

use std::{
	sync::{Arc, Condvar, Mutex},
	time::{Duration, Instant},
};

use parity_scale_codec::{Decode, Encode};
use schnorrkel::Signature;

use truapi::{
	FrameSink, SigningHostRuntime,
	frame::{
		MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE, MESSAGE_TYPE_START,
		Payload, ProtocolMessage, request_ids, subscription_ids,
	},
	host_logic::product_account::{
		SR25519_SIGNING_CONTEXT, derivation_index_bytes, derive_product_keypair,
		derive_root_keypair_from_entropy,
	},
	platform::{
		HostInfo, PlatformInfo, ProductContext, SigningHostConfig,
		mock::{ConfirmKind, MockConfig, MockPlatform},
	},
	v01,
};

// Shared harness; this binary uses only the spawner.
#[allow(dead_code)]
mod common;
use common::test_spawner;

/// BIP-39 entropy the local session is activated from.
const ENTROPY: [u8; 32] = [0xab; 32];
/// dotNS identifier of the product doing the signing.
const PRODUCT_ID: &str = "myapp.dot";
/// Message the product asks to have signed.
const MESSAGE: &[u8] = b"keystone";
/// Content seeded into the mock's preimage store.
const PREIMAGE: &[u8] = b"seeded through the core";

/// Frame sink that keeps every emitted frame in send order.
#[derive(Default)]
struct RecordingSink {
	frames: Mutex<Vec<Vec<u8>>>,
	emitted: Condvar,
}

impl RecordingSink {
	/// Wait until at least `count` frames have been emitted, or `timeout`
	/// elapses. A subscription's frames come from the spawner, so the dispatch
	/// that starts one returns before any of them are sent.
	fn wait_for(&self, count: usize, timeout: Duration) {
		let deadline = Instant::now() + timeout;
		let mut frames = self.frames.lock().unwrap();
		while frames.len() < count {
			let now = Instant::now();
			if now >= deadline {
				break;
			}
			let (guard, _) = self.emitted.wait_timeout(frames, deadline - now).unwrap();
			frames = guard;
		}
	}
}

impl FrameSink for RecordingSink {
	fn emit_frame(&self, frame: Vec<u8>) {
		self.frames.lock().unwrap().push(frame);
		self.emitted.notify_all();
	}
}

/// Signing-host configuration on the `paseo` network suffix.
fn signing_config() -> SigningHostConfig {
	SigningHostConfig::new(
		HostInfo {
			name: "Mock Signing Host".to_string(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Unknown,
		},
		PlatformInfo::default(),
		[0; 32],
		[0xbb; 32],
		// Distinct from its siblings so a transposition stays visible.
		[0xcc; 32],
		"paseo".to_string(),
	)
	.expect("signing host config is valid")
}

/// The account selector the product signs with.
fn account() -> v01::ProductAccountId {
	v01::ProductAccountId {
		dot_ns_identifier: PRODUCT_ID.to_string(),
		derivation_index: v01::DerivationIndex::Index(0),
	}
}

#[test]
fn a_signing_host_on_the_mock_platform_returns_a_verifiable_signature() {
	let platform = Arc::new(MockPlatform::new());
	let runtime = SigningHostRuntime::new(platform.clone(), signing_config(), test_spawner());
	futures::executor::block_on(runtime.activate_local_session(ENTROPY.to_vec()))
		.expect("a signing host activates a local session from raw entropy");

	let product = ProductContext::new(PRODUCT_ID.to_string()).expect("product context is valid");
	let sink = Arc::new(RecordingSink::default());
	let product_runtime = runtime.product_runtime(product, sink.clone());

	let ids = request_ids("signing_sign_raw").expect("known request method");
	let value = truapi::versioned::signing::HostSignRawRequest::V1(v01::HostSignRawRequest {
		account: account(),
		payload: v01::RawPayload::Bytes { bytes: MESSAGE.to_vec() },
	})
	.encode();
	let frame = ProtocolMessage {
		request_id: "sign:1".to_string(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type: MESSAGE_TYPE_REQUEST,
			value,
		},
	};
	futures::executor::block_on(product_runtime.receive_frame(frame.encode()))
		.expect("dispatcher accepted the sign_raw frame");

	let frames = sink.frames.lock().unwrap().clone();
	let response = frames
		.iter()
		.map(|bytes| ProtocolMessage::decode(&mut &bytes[..]).expect("decode emitted frame"))
		.find(|message| {
			message.payload.trait_id == ids.trait_id && message.payload.method_id == ids.method_id
		})
		.expect("dispatcher emitted a sign_raw response");
	assert_eq!(response.payload.message_type, MESSAGE_TYPE_RESPONSE);

	let decoded: Result<
		truapi::versioned::signing::HostSignRawResponse,
		truapi::CallError<truapi::versioned::signing::HostSignRawError>,
	> = Decode::decode(&mut &response.payload.value[..])
		.expect("sign_raw response decodes at the wire shape");
	let truapi::versioned::signing::HostSignRawResponse::V1(payload) =
		decoded.expect("the mock platform confirms the review, so signing succeeds");

	// The signature has to verify against the key the activation entropy
	// derives, not merely against whatever the runtime happens to hold: that is
	// what makes this a signature by *the user's* product account.
	let root = derive_root_keypair_from_entropy(&ENTROPY).expect("root keypair derives");
	let keypair = derive_product_keypair(
		&root,
		PRODUCT_ID,
		derivation_index_bytes(&v01::DerivationIndex::Index(0)),
	)
	.expect("product keypair derives");

	// `sign_raw` watermarks the payload before signing, so the signed message
	// is the wrapped form rather than the bytes the product handed over.
	let signed_message = [b"<Bytes>".as_slice(), MESSAGE, b"</Bytes>"].concat();
	let signature =
		Signature::from_bytes(&payload.signature).expect("response carries an sr25519 signature");
	keypair
		.public
		.verify_simple(SR25519_SIGNING_CONTEXT, &signed_message, &signature)
		.expect("the signature verifies against the account derived from the activation entropy");

	// A raw signature is not a transaction, and nothing on this path should
	// have built one.
	assert_eq!(payload.signed_transaction, None);

	// The user was asked, through the mock, before the key was used.
	assert_eq!(platform.confirmations(), vec![ConfirmKind::SignRaw]);
}

#[test]
fn the_mock_platform_never_reaches_the_chain_to_sign_raw() {
	// The mock's default chain behavior records requests and never answers, so
	// any genuine chain dependency on this path would park the test rather than
	// fail it. Asserting the recording is empty is what distinguishes "signing
	// is local" from "signing got lucky".
	let platform = Arc::new(MockPlatform::new());
	let runtime = SigningHostRuntime::new(platform.clone(), signing_config(), test_spawner());
	futures::executor::block_on(runtime.activate_local_session(ENTROPY.to_vec()))
		.expect("local activation succeeds");

	let product = ProductContext::new(PRODUCT_ID.to_string()).expect("product context is valid");
	let sink = Arc::new(RecordingSink::default());
	let product_runtime = runtime.product_runtime(product, sink);

	let ids = request_ids("signing_sign_raw").expect("known request method");
	let value = truapi::versioned::signing::HostSignRawRequest::V1(v01::HostSignRawRequest {
		account: account(),
		payload: v01::RawPayload::Bytes { bytes: MESSAGE.to_vec() },
	})
	.encode();
	let frame = ProtocolMessage {
		request_id: "sign:1".to_string(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type: MESSAGE_TYPE_REQUEST,
			value,
		},
	};
	futures::executor::block_on(product_runtime.receive_frame(frame.encode()))
		.expect("dispatcher accepted the sign_raw frame");

	assert!(
		platform.sent_rpc().is_empty(),
		"signing a raw payload sent chain RPC: {:?}",
		platform.sent_rpc(),
	);
}

#[test]
fn a_declined_confirmation_withholds_the_signature() {
	// The mock auto-confirms by default, which is the only reason the signing
	// test above gets a signature at all. Unless declining actually withholds
	// one, that default is indistinguishable from a review that is never
	// consulted, and the passing test above would prove nothing about consent.
	let platform = Arc::new(MockPlatform::with_config(MockConfig {
		confirm_user_actions: false,
		..MockConfig::default()
	}));
	let runtime = SigningHostRuntime::new(platform.clone(), signing_config(), test_spawner());
	futures::executor::block_on(runtime.activate_local_session(ENTROPY.to_vec()))
		.expect("local activation succeeds");

	let product = ProductContext::new(PRODUCT_ID.to_string()).expect("product context is valid");
	let sink = Arc::new(RecordingSink::default());
	let product_runtime = runtime.product_runtime(product, sink.clone());

	let ids = request_ids("signing_sign_raw").expect("known request method");
	let value = truapi::versioned::signing::HostSignRawRequest::V1(v01::HostSignRawRequest {
		account: account(),
		payload: v01::RawPayload::Bytes { bytes: MESSAGE.to_vec() },
	})
	.encode();
	let frame = ProtocolMessage {
		request_id: "sign:1".to_string(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type: MESSAGE_TYPE_REQUEST,
			value,
		},
	};
	futures::executor::block_on(product_runtime.receive_frame(frame.encode()))
		.expect("dispatcher accepted the sign_raw frame");

	let frames = sink.frames.lock().unwrap().clone();
	let response = frames
		.iter()
		.map(|bytes| ProtocolMessage::decode(&mut &bytes[..]).expect("decode emitted frame"))
		.find(|message| {
			message.payload.trait_id == ids.trait_id && message.payload.method_id == ids.method_id
		})
		.expect("dispatcher emitted a sign_raw response");

	let decoded: Result<
		truapi::versioned::signing::HostSignRawResponse,
		truapi::CallError<truapi::versioned::signing::HostSignRawError>,
	> = Decode::decode(&mut &response.payload.value[..])
		.expect("sign_raw response decodes at the wire shape");
	assert!(
		matches!(
			decoded,
			Err(truapi::CallError::Domain(truapi::versioned::signing::HostSignRawError::V1(
				v01::HostSignPayloadError::Rejected
			)))
		),
		"a declined review still produced {decoded:?}",
	);
	assert_eq!(platform.confirmations(), vec![ConfirmKind::SignRaw]);
}

#[test]
fn a_seeded_preimage_is_reachable_through_the_core() {
	// `insert_preimage` is only worth anything if the key it hands back is the
	// key the core asks the host for. The core content-addresses preimages and
	// downgrades a hash mismatch to a miss, so a mock that keys its store any
	// other way round-trips perfectly against itself while every seeded
	// preimage stays unreachable from a product. Only a lookup driven as a
	// product frame, with the core in the path, can tell those apart.
	let platform = Arc::new(MockPlatform::new());
	let key = platform.insert_preimage(PREIMAGE.to_vec());

	let runtime = SigningHostRuntime::new(platform.clone(), signing_config(), test_spawner());
	let product = ProductContext::new(PRODUCT_ID.to_string()).expect("product context is valid");
	let sink = Arc::new(RecordingSink::default());
	let product_runtime = runtime.product_runtime(product, sink.clone());

	let ids = subscription_ids("preimage_lookup_subscribe").expect("known subscription method");
	let value = truapi::versioned::preimage::RemotePreimageLookupSubscribeRequest::V1(
		v01::RemotePreimageLookupSubscribeRequest { key },
	)
	.encode();
	let frame = ProtocolMessage {
		request_id: "preimage:1".to_string(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type: MESSAGE_TYPE_START,
			value,
		},
	};
	futures::executor::block_on(product_runtime.receive_frame(frame.encode()))
		.expect("dispatcher accepted the lookup_subscribe frame");

	sink.wait_for(1, Duration::from_secs(5));
	let frames = sink.frames.lock().unwrap().clone();
	let item = frames
		.iter()
		.map(|bytes| ProtocolMessage::decode(&mut &bytes[..]).expect("decode emitted frame"))
		.find(|message| {
			message.payload.trait_id == ids.trait_id &&
				message.payload.method_id == ids.method_id &&
				message.payload.message_type == MESSAGE_TYPE_RECEIVE
		})
		.expect("the subscription emitted its current-value item");

	// A subscription item frame carries the item itself; only an interrupt
	// carries a `Result`.
	let truapi::versioned::preimage::RemotePreimageLookupSubscribeItem::V1(payload) =
		Decode::decode(&mut &item.payload.value[..])
			.expect("lookup_subscribe item decodes at the wire shape");
	assert_eq!(payload.value, Some(PREIMAGE.to_vec()));
}
