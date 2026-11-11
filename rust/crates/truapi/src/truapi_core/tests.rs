use super::*;
use parity_scale_codec::Encode;
use truapi::v01;

use crate::{
	ProductId,
	frame::{Payload, request_ids, subscription_ids},
	test_support::{StubPlatform, runtime_config, test_spawner},
};

/// A request payload must consume exactly its own bytes. Trailing bytes
/// mean the sender and this build disagree about the shape, so running the
/// handler on the prefix would act on a frame neither side agreed to.
#[test]
fn a_request_with_trailing_bytes_is_refused_before_the_handler_runs() {
	let (host_config, product) = runtime_config("dotli.dot");
	let core = TrUApiCore::from_platform_with_config(
		Arc::new(StubPlatform::default()),
		host_config,
		product,
		test_spawner(),
	);
	let request = v01::HostFeatureSupportedRequest::Chain { genesis_hash: vec![0u8; 32] };
	let ids = request_ids("system_feature_supported").expect("known request method");
	let mut value = truapi::versioned::system::HostFeatureSupportedRequest::V1(request).encode();
	value.extend_from_slice(&[0xde, 0xad]);
	let frame = ProtocolMessage {
		request_id: "p:1".into(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type: crate::frame::MESSAGE_TYPE_REQUEST,
			value,
		},
	};
	let response_bytes = futures::executor::block_on(core.receive_from_product(&frame.encode()))
		.expect("dispatcher should emit a response");
	let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
	let decoded = Result::<
		truapi::versioned::system::HostFeatureSupportedResponse,
		truapi::CallError<truapi::versioned::system::HostFeatureSupportedError>,
	>::decode(&mut &response.payload.value[..])
	.expect("decode response payload");
	assert!(
		matches!(decoded, Err(truapi::CallError::MalformedFrame { .. })),
		"trailing bytes must be refused, got {decoded:?}"
	);
}

#[test]
fn from_platform_dispatches_feature_supported() {
	let (host_config, product) = runtime_config("dotli.dot");
	let core = TrUApiCore::from_platform_with_config(
		Arc::new(StubPlatform::default()),
		host_config,
		product,
		test_spawner(),
	);
	let request = v01::HostFeatureSupportedRequest::Chain { genesis_hash: vec![0u8; 32] };
	let ids = request_ids("system_feature_supported").expect("known request method");
	let value = truapi::versioned::system::HostFeatureSupportedRequest::V1(request).encode();
	let frame = ProtocolMessage {
		request_id: "p:1".into(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type: crate::frame::MESSAGE_TYPE_REQUEST,
			value,
		},
	};
	let encoded = frame.encode();
	let response_bytes = futures::executor::block_on(core.receive_from_product(&encoded))
		.expect("dispatcher should emit a response");
	let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
	assert_eq!(response.request_id, "p:1");
	assert_eq!(response.payload.trait_id, ids.trait_id);
	assert_eq!(response.payload.method_id, ids.method_id);
	assert_eq!(response.payload.message_type, crate::frame::MESSAGE_TYPE_RESPONSE);
	let expected: Result<
		truapi::versioned::system::HostFeatureSupportedResponse,
		truapi::CallError<truapi::versioned::system::HostFeatureSupportedError>,
	> = Ok(truapi::versioned::system::HostFeatureSupportedResponse::V1(
		v01::HostFeatureSupportedResponse { supported: true },
	));
	assert_eq!(response.payload.value, expected.encode());
}

/// Drive a request frame through `TrUApiCore::receive_from_product` and
/// return the response payload's raw bytes: the SCALE encoding of
/// `Result<{Method}Response, CallError<{Method}Error>>`. `request_value`
/// is the method's own request wrapper, already SCALE-encoded (e.g.
/// `HostLocalStorageReadRequest::V1(request).encode()`). Shared by the
/// runtime-delegation tests below.
fn run_request(core: &TrUApiCore, method: &str, request_value: Vec<u8>) -> Vec<u8> {
	let ids = request_ids(method).expect("known request method");
	let frame = ProtocolMessage {
		request_id: "p:1".into(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type: crate::frame::MESSAGE_TYPE_REQUEST,
			value: request_value,
		},
	};
	let response_bytes = futures::executor::block_on(core.receive_from_product(&frame.encode()))
		.expect("dispatcher should emit a response");
	let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
	assert_eq!(response.request_id, "p:1");
	assert_eq!(response.payload.trait_id, ids.trait_id);
	assert_eq!(response.payload.method_id, ids.method_id);
	assert_eq!(response.payload.message_type, crate::frame::MESSAGE_TYPE_RESPONSE);
	response.payload.value
}

fn make_core() -> TrUApiCore {
	let (host_config, product) = runtime_config("dotli.dot");
	TrUApiCore::from_platform_with_config(
		Arc::new(StubPlatform::default()),
		host_config,
		product,
		test_spawner(),
	)
}

#[test]
fn the_core_constructor_hands_its_services_the_asset_hub_hash() {
	// The third constructor path, and the only one nothing else pins. A
	// transposition here compiles and dials People or Bulletin, where no
	// dotNS contract is deployed, which is #660's failure mode one line
	// over.
	let platform = Arc::new(StubPlatform {
		// Ends the follow rather than waiting out `OPERATION_TIMEOUT`; this
		// asserts which chain was dialled, not that the lookup succeeded.
		chain_responses_end: true,
		..StubPlatform::default()
	});
	// people [0; 32], bulletin [0xbb; 32], asset hub [0xcc; 32].
	let (host_config, product) = runtime_config("dotli.dot");
	let core = TrUApiCore::from_platform_with_config(
		platform.clone(),
		host_config,
		product,
		test_spawner(),
	);
	// Nothing is cached for `wallet.dot`, so the read has to resolve a
	// manifest, which is the only thing that reaches a chain with this hash.
	let request = truapi::versioned::local_storage::HostLocalStorageReadRequest::V2(
		truapi::v02::HostLocalStorageReadRequest {
			product: Some(ProductId::for_test("wallet.dot")),
			key: "k".to_string(),
		},
	);
	let _ = run_request(&core, "local_storage_read", request.encode());
	assert_eq!(
		platform.chain_connects.lock().expect("chain connect mutex poisoned").clone(),
		vec![[0xcc; 32]],
		"the core dials Asset Hub, not People ([0; 32]) or Bulletin ([0xbb; 32])"
	);
}

#[test]
fn local_storage_read_round_trips_none() {
	let core = make_core();
	let request = v01::HostLocalStorageReadRequest { key: "missing".into() };
	let payload = run_request(
		&core,
		"local_storage_read",
		truapi::versioned::local_storage::HostLocalStorageReadRequest::V1(request).encode(),
	);
	let expected: Result<
		truapi::versioned::local_storage::HostLocalStorageReadResponse,
		truapi::CallError<truapi::versioned::local_storage::HostLocalStorageReadError>,
	> = Ok(truapi::versioned::local_storage::HostLocalStorageReadResponse::V1(
		v01::HostLocalStorageReadResponse { value: None },
	));
	assert_eq!(payload, expected.encode());
}

#[test]
fn local_storage_write_round_trips_unit_ok() {
	let core = make_core();
	let request = v01::HostLocalStorageWriteRequest { key: "k".into(), value: vec![1, 2, 3] };
	let payload = run_request(
		&core,
		"local_storage_write",
		truapi::versioned::local_storage::HostLocalStorageWriteRequest::V1(request).encode(),
	);
	let expected: Result<
		truapi::versioned::local_storage::HostLocalStorageWriteResponse,
		truapi::CallError<truapi::versioned::local_storage::HostLocalStorageWriteError>,
	> = Ok(truapi::versioned::local_storage::HostLocalStorageWriteResponse::V1);
	assert_eq!(payload, expected.encode());
}

#[test]
fn local_storage_clear_round_trips_unit_ok() {
	let core = make_core();
	let request = v01::HostLocalStorageClearRequest { key: "k".into() };
	let payload = run_request(
		&core,
		"local_storage_clear",
		truapi::versioned::local_storage::HostLocalStorageClearRequest::V1(request).encode(),
	);
	let expected: Result<
		truapi::versioned::local_storage::HostLocalStorageClearResponse,
		truapi::CallError<truapi::versioned::local_storage::HostLocalStorageClearError>,
	> = Ok(truapi::versioned::local_storage::HostLocalStorageClearResponse::V1);
	assert_eq!(payload, expected.encode());
}

#[test]
fn send_push_notification_delegates_to_platform() {
	let core = make_core();
	let request =
		v01::HostPushNotificationRequest { text: "hi".into(), deeplink: None, scheduled_at: None };
	let payload = run_request(
		&core,
		"notifications_send_push_notification",
		truapi::versioned::notifications::HostPushNotificationRequest::V1(request).encode(),
	);
	let expected: Result<
		truapi::versioned::notifications::HostPushNotificationResponse,
		truapi::CallError<truapi::versioned::notifications::HostPushNotificationError>,
	> = Ok(truapi::versioned::notifications::HostPushNotificationResponse::V1(
		v01::HostPushNotificationResponse { id: 0 },
	));
	assert_eq!(payload, expected.encode());
}

#[test]
fn request_remote_permission_round_trips_granted() {
	let core = make_core();
	let request = v01::RemotePermissionRequest { permission: v01::RemotePermission::ChainSubmit };
	let payload = run_request(
		&core,
		"permissions_request_remote_permission",
		truapi::versioned::permissions::RemotePermissionRequest::V1(request).encode(),
	);
	// Stub permissions grants every request.
	let expected: Result<
		truapi::versioned::permissions::RemotePermissionResponse,
		truapi::CallError<truapi::versioned::permissions::RemotePermissionError>,
	> = Ok(truapi::versioned::permissions::RemotePermissionResponse::V1(
		v01::RemotePermissionResponse { granted: true },
	));
	assert_eq!(payload, expected.encode());
}

/// `connection_status_subscribe` produces a stream whose first item is
/// the current session state. Drive it through the dispatcher with a
/// recording transport and assert exactly one `_receive` frame appears.
#[test]
fn connection_status_subscribe_yields_initial_disconnected() {
	use std::sync::Mutex;

	#[derive(Default)]
	struct RecordingTransport {
		sent: Mutex<Vec<ProtocolMessage>>,
	}
	impl Transport for RecordingTransport {
		fn send(&self, message: ProtocolMessage) {
			self.sent.lock().unwrap().push(message);
		}
		fn on_message(
			&self,
			_handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
		) -> Box<dyn FnOnce()> {
			Box::new(|| {})
		}
	}

	let core = make_core();
	let transport = Arc::new(RecordingTransport::default());
	let dyn_transport: Arc<dyn Transport> = transport.clone();

	let sub_ids =
		subscription_ids("account_connection_status_subscribe").expect("known subscription");
	let frame = ProtocolMessage {
		request_id: "p:1".into(),
		payload: Payload {
			trait_id: sub_ids.trait_id,
			method_id: sub_ids.method_id,
			message_type: crate::frame::MESSAGE_TYPE_START,
			value: truapi::versioned::account::HostAccountConnectionStatusSubscribeRequest::V1
				.encode(),
		},
	};
	futures::executor::block_on(core.dispatch(frame, dyn_transport));

	// Wait briefly for the spawned thread to emit the initial item.
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
	loop {
		if !transport.sent.lock().unwrap().is_empty() {
			break;
		}
		if std::time::Instant::now() > deadline {
			panic!("subscription did not yield an item in time");
		}
		std::thread::sleep(std::time::Duration::from_millis(10));
	}

	let sent = transport.sent.lock().unwrap().clone();
	assert!(!sent.is_empty(), "expected at least one _receive frame");
	let first = &sent[0];
	assert_eq!(first.payload.trait_id, sub_ids.trait_id);
	assert_eq!(first.payload.method_id, sub_ids.method_id);
	assert_eq!(first.payload.message_type, crate::frame::MESSAGE_TYPE_RECEIVE);
	let expected = truapi::versioned::account::HostAccountConnectionStatusSubscribeItem::V1(
		v01::HostAccountConnectionStatusSubscribeItem::Disconnected,
	)
	.encode();
	assert_eq!(first.payload.value, expected);
}

/// A dial needs the user's `JamPeers` grant for its genesis, so a refusal
/// answers `NotGranted` before anything connects. The browser core never
/// asks: its JavaScript session serves trait 111 before frames reach it.
#[test]
fn a_dial_the_user_refuses_is_not_granted() {
	let genesis = [0x35; 32];
	let platform = Arc::new(StubPlatform { remote_permission_denied: true, ..Default::default() });
	let asked = platform.remote_permission_requests.clone();
	let (host_config, product) = runtime_config("dotli.dot");
	let core =
		TrUApiCore::from_platform_with_config(platform, host_config, product, test_spawner());
	let ids = request_ids("jam_peer_transport_dial").expect("registered peer transport");
	let frame = ProtocolMessage {
		request_id: "p:peer".into(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type: crate::frame::MESSAGE_TYPE_REQUEST,
			value: truapi::versioned::jam_peer_transport::HostJamPeerTransportDialRequest::V1(
				truapi::latest::HostJamPeerTransportDialRequest {
					genesis,
					ip: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 127, 0, 0, 1],
					port: 43000,
					ed25519: [0; 32],
					p256: None,
				},
			)
			.encode(),
		},
	};
	let response = futures::executor::block_on(core.receive_from_product(&frame.encode()))
		.expect("registered method must answer explicitly");
	let prompted = if cfg!(target_arch = "wasm32") {
		vec![]
	} else {
		vec![v01::RemotePermissionRequest {
			permission: v01::RemotePermission::JamPeers { genesis },
		}]
	};
	assert_eq!(
		(
			ProtocolMessage::decode(&mut &response[..]).unwrap().payload.value,
			asked.lock().unwrap().clone(),
		),
		(
			Err::<truapi::versioned::jam_peer_transport::HostJamPeerTransportDialResponse, _>(
				truapi::CallError::Domain(
					truapi::versioned::jam_peer_transport::HostJamPeerTransportDialError::V1(
						truapi::latest::HostJamPeerTransportDialError::NotGranted,
					),
				),
			)
			.encode(),
			prompted,
		),
	);
}
