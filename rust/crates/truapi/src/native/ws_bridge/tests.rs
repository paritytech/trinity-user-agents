use super::*;
use crate::platform::{HostInfo, PlatformInfo, ProductContext, ProductId, SigningHostConfig};
use parity_scale_codec::{Decode, Encode};
use truapi::v01;

use crate::{
	SigningHostRuntime,
	frame::{Payload, ProtocolMessage, request_ids},
	test_support::{StubPlatform, test_spawner},
};

fn start_test_bridge() -> WsBridge {
	WsBridge::start(0, no_log()).expect("start bridge").0
}

fn test_runtime_factory() -> Arc<dyn WsProductRuntimeFactory> {
	runtime_factory_for(Arc::new(StubPlatform::default()))
}

fn runtime_factory_for(platform: Arc<StubPlatform>) -> Arc<dyn WsProductRuntimeFactory> {
	let config = SigningHostConfig::new(
		HostInfo {
			name: "Polkadot Mobile".to_string(),
			icon: Some("https://example.invalid/dotli.png".to_string()),
			version: None,
			platform: truapi::latest::HostPlatform::Unknown,
		},
		PlatformInfo::default(),
		crate::test_support::paseo_test_stack(),
	)
	.expect("test signing host config is valid");
	let runtime = Arc::new(SigningHostRuntime::new(platform, config, test_spawner()));
	let product = ProductContext::new(ProductId::for_test("dotli.dot"));
	Arc::new(move |sink| runtime.product_runtime(product.clone(), sink))
}

fn no_log() -> BridgeLogger {
	Arc::new(|_, _| {})
}

fn connect(port: u16, token: &str) -> tokio::runtime::Runtime {
	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let url = format!("ws://127.0.0.1:{port}/?t={token}");
	rt.block_on(async { tokio_tungstenite::connect_async(&url).await.expect("dial") });
	rt
}

#[test]
fn path_token_matches_exact() {
	assert!(path_token_matches(Some("/?t=abc"), "abc"));
	assert!(path_token_matches(Some("/?foo=1&t=abc"), "abc"));
	assert!(!path_token_matches(Some("/?t=other"), "abc"));
	assert!(!path_token_matches(Some("/?token=abc"), "abc"));
	assert!(!path_token_matches(Some("/"), "abc"));
	assert!(!path_token_matches(None, "abc"));
}

#[test]
fn path_token_matches_every_duplicated_t_pair_not_just_the_first() {
	assert!(path_token_matches(Some("/?t=wrong&t=abc"), "abc"));
	assert!(path_token_matches(Some("/?t=abc&t=wrong"), "abc"));
	assert!(!path_token_matches(Some("/?t=wrong&t=alsowrong"), "abc"));
}

#[test]
fn drop_from_shared_executor_does_not_block_worker() {
	let bridge = start_test_bridge();
	let executor = shared_native_executor().expect("shared native executor");
	let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();

	executor.handle().spawn(async move {
		drop(bridge);
		let _ = dropped_tx.send(());
	});

	dropped_rx
		.recv_timeout(std::time::Duration::from_secs(2))
		.expect("dropping from an executor worker must not deadlock");
}

/// Spin the shared listener up on `127.0.0.1:0`, register one execution,
/// dial it with a real `tokio-tungstenite` client, send a known SCALE
/// frame, and verify the bridge echoes the SCALE-encoded
/// `feature_supported` response.
#[test]
fn round_trip_feature_supported_through_bridge() {
	let bridge = start_test_bridge();
	let endpoint = bridge.register(test_runtime_factory(), no_log());
	let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

	// Use a fresh `tokio` runtime on the test thread so the client does
	// not depend on the native executor under test.
	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");

	let ids = request_ids("system_feature_supported").expect("known request method");
	let response_bytes = rt.block_on(async {
		let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");

		let value = truapi::versioned::system::HostFeatureSupportedRequest::V1(
			v01::HostFeatureSupportedRequest::Chain { genesis_hash: vec![0u8; 32] },
		)
		.encode();
		let request_frame = ProtocolMessage {
			request_id: "p:1".into(),
			payload: Payload {
				trait_id: ids.trait_id,
				method_id: ids.method_id,
				message_type: crate::frame::MESSAGE_TYPE_REQUEST,
				value,
			},
		};
		ws.send(WsMessage::Binary(request_frame.encode())).await.expect("send");

		// Block until the bridge replies with the response frame.
		loop {
			match ws.next().await {
				Some(Ok(WsMessage::Binary(bytes))) => break bytes,
				Some(Ok(_)) => continue,
				Some(Err(err)) => panic!("ws error: {err}"),
				None => panic!("connection closed before response"),
			}
		}
	});

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

	drop(bridge);
}

#[test]
fn a_vanished_peer_releases_its_connection_slot() {
	let bridge = start_test_bridge();
	let endpoint = bridge.register(test_runtime_factory(), no_log());
	let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);
	let client = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("client runtime");

	for _ in 0..=MAX_WS_CONNECTIONS_PER_EXECUTION {
		let (socket, _) = client
			.block_on(tokio_tungstenite::connect_async(&url))
			.expect("reconnect after an abruptly disconnected peer");
		drop(socket);
		crate::test_support::wait_until(
			|| bridge.registry.total_connections.load(Ordering::Acquire) == 0,
			"an abruptly disconnected peer did not release its connection slot",
		);
	}
}

/// One `system_feature_supported` exchange on a fresh connection. The
/// socket is dropped without a closing handshake when this returns, which
/// is how a product's bridge socket dies when its host is suspended.
async fn feature_supported_round_trip(url: &str, request_id: &str) -> ProtocolMessage {
	let (mut ws, _) = tokio_tungstenite::connect_async(url).await.expect("dial");
	feature_supported_exchange(&mut ws, request_id).await
}

async fn feature_supported_exchange(
	ws: &mut WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
	request_id: &str,
) -> ProtocolMessage {
	let ids = request_ids("system_feature_supported").expect("known request method");
	let value = truapi::versioned::system::HostFeatureSupportedRequest::V1(
		v01::HostFeatureSupportedRequest::Chain { genesis_hash: vec![0u8; 32] },
	)
	.encode();
	let request = ProtocolMessage {
		request_id: request_id.into(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type: crate::frame::MESSAGE_TYPE_REQUEST,
			value,
		},
	};
	ws.send(WsMessage::Binary(request.encode())).await.expect("send");
	loop {
		match ws.next().await {
			Some(Ok(WsMessage::Binary(bytes))) => {
				break ProtocolMessage::decode(&mut &bytes[..]).expect("decode response");
			},
			Some(Ok(_)) => continue,
			Some(Err(err)) => panic!("ws error: {err}"),
			None => panic!("connection closed before response"),
		}
	}
}

/// A product reconnects once per app background for the life of its
/// execution. A single reconnect shows the slot is released; only
/// repetition shows nothing accumulates behind it and that the bridge is
/// still answering on the hundredth one.
#[test]
fn sustained_reconnect_churn_keeps_the_bridge_serving() {
	const CYCLES: usize = 100;

	let bridge = start_test_bridge();
	let endpoint = bridge.register(test_runtime_factory(), no_log());
	let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);
	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");

	for cycle in 0..CYCLES {
		let request_id = format!("p:{cycle}");
		let response = rt.block_on(feature_supported_round_trip(&url, &request_id));

		assert_eq!(
			(response.request_id, response.payload.message_type),
			(request_id, crate::frame::MESSAGE_TYPE_RESPONSE),
			"cycle {cycle} was not served"
		);
		crate::test_support::wait_until(
			|| bridge.registry.total_connections.load(Ordering::Acquire) == 0,
			"a reconnect cycle left its connection slot held",
		);
	}
}

/// Several executions reconnecting at once, which is what a host with more
/// than one product open does the moment it returns to the foreground.
#[test]
fn concurrent_reconnect_churn_releases_every_slot() {
	const EXECUTIONS: usize = 4;
	const ROUNDS: usize = 8;

	let bridge = start_test_bridge();
	let endpoints: Vec<_> = (0..EXECUTIONS)
		.map(|_| bridge.register(test_runtime_factory(), no_log()))
		.collect();
	let rt = tokio::runtime::Builder::new_multi_thread()
		.worker_threads(4)
		.enable_all()
		.build()
		.expect("test runtime");

	for round in 0..ROUNDS {
		rt.block_on(async {
			let mut tasks = Vec::new();
			for endpoint in &endpoints {
				let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);
				let request_id = format!("p:{round}");
				tasks.push(tokio::spawn(async move {
					feature_supported_round_trip(&url, &request_id).await
				}));
			}
			for task in tasks {
				let response = task.await.expect("round trip task");
				assert_eq!(response.payload.message_type, crate::frame::MESSAGE_TYPE_RESPONSE);
			}
		});
	}

	crate::test_support::wait_until(
		|| bridge.registry.total_connections.load(Ordering::Acquire) == 0,
		"concurrent reconnect churn left connection slots held",
	);
}

#[test]
fn two_executions_share_one_port_with_isolated_tokens() {
	let bridge = start_test_bridge();
	let first = bridge.register(test_runtime_factory(), no_log());
	let second = bridge.register(test_runtime_factory(), no_log());

	assert_eq!(first.port, second.port);
	assert_ne!(first.token, second.token);

	connect(first.port, &first.token);
	connect(second.port, &second.token);

	drop(bridge);
}

#[test]
fn wrong_or_unknown_token_is_rejected_at_handshake() {
	let bridge = start_test_bridge();
	let endpoint = bridge.register(test_runtime_factory(), no_log());
	let _second = bridge.register(test_runtime_factory(), no_log());

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");

	let url = format!("ws://127.0.0.1:{}/?t=bogus", endpoint.port);
	let err = rt
		.block_on(async { tokio_tungstenite::connect_async(&url).await })
		.expect_err("connection with an unknown token must be refused");
	let msg = format!("{err}");
	assert!(
		msg.contains("401") || msg.to_lowercase().contains("unauthorized"),
		"expected 401/unauthorized rejection, got: {msg}",
	);

	drop(bridge);
}

struct DisposalWatchFactory {
	inner: Arc<dyn WsProductRuntimeFactory>,
	control: Mutex<Option<crate::ProductRuntimeControl>>,
}

impl WsProductRuntimeFactory for DisposalWatchFactory {
	fn product_runtime(&self, sink: Arc<dyn FrameSink>) -> ProductRuntime {
		let runtime = self.inner.product_runtime(sink);
		*self.control.lock().expect("disposal watch mutex poisoned") = Some(runtime.control());
		runtime
	}
}

#[test]
fn retained_controls_do_not_keep_ended_connections_alive() {
	fn is_closed(control: &crate::ProductRuntimeControl) -> bool {
		matches!(
			control.publish_chat_action(v01::HostChatActionSubscribeItem {
				room_id: "support".into(),
				peer: "dotli.dot".into(),
				payload: v01::ChatActionPayload::ActionTriggered(v01::ActionTrigger {
					message_id: "message".into(),
					action_id: "vote".into(),
					payload: None,
				}),
			}),
			Err(crate::ProductRuntimeError::Closed)
		)
	}

	for ending in ["close", "revoke", "shutdown"] {
		let mut bridge = start_test_bridge();
		let watch = Arc::new(DisposalWatchFactory {
			inner: test_runtime_factory(),
			control: Mutex::new(None),
		});
		let endpoint = bridge.register(watch.clone(), no_log());
		let client = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.expect("client runtime");
		let mut socket = client.block_on(async {
			tokio_tungstenite::connect_async(format!(
				"ws://127.0.0.1:{}/?t={}",
				endpoint.port, endpoint.token
			))
			.await
			.expect("connect")
			.0
		});
		crate::test_support::wait_until(
			|| watch.control.lock().expect("control mutex poisoned").is_some(),
			"connection did not create its runtime",
		);
		let control = watch
			.control
			.lock()
			.expect("control mutex poisoned")
			.clone()
			.expect("connection control");
		assert!(!is_closed(&control), "connection must begin live");

		match ending {
			"close" => client.block_on(socket.close(None)).expect("close client"),
			"revoke" => join_aborted_connections(bridge.revoke(&endpoint.token)),
			"shutdown" => bridge.stop(),
			_ => unreachable!(),
		}
		client.block_on(async {
			tokio::time::timeout(std::time::Duration::from_secs(2), async {
				if ending == "close" {
					assert!(
						matches!(socket.next().await, Some(Ok(WsMessage::Close(_)))),
						"a healthy peer must receive its close acknowledgement"
					);
				}
				while let Some(Ok(_)) = socket.next().await {}
			})
			.await
			.unwrap_or_else(|_| panic!("{ending} left the socket open with a retained control"));
		});
		crate::test_support::wait_until(
			|| bridge.registry.total_connections.load(Ordering::Acquire) == 0,
			"ended connection did not release its capacity",
		);
		assert!(is_closed(&control), "{ending} did not dispose the runtime");
	}
}

#[test]
fn a_full_handshake_backlog_does_not_lock_out_a_new_connection() {
	let bridge = start_test_bridge();
	let endpoint = bridge.register(test_runtime_factory(), no_log());

	// Silent sockets fill the backlog without reaching token authentication.
	let addr = format!("127.0.0.1:{}", endpoint.port);
	let mut stalled = Vec::new();
	for _ in 0..MAX_PENDING_HANDSHAKES {
		stalled.push(std::net::TcpStream::connect(&addr).expect("stall the backlog"));
	}

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);
	rt.block_on(async {
		let (mut ws, _) = tokio::time::timeout(
			std::time::Duration::from_secs(10),
			tokio_tungstenite::connect_async(&url),
		)
		.await
		.expect("a full backlog must not stall a new connection")
		.expect("dial past a full backlog");
		ws.close(None).await.expect("close client");
	});

	drop(stalled);
	drop(bridge);
}

#[test]
fn revoking_one_token_leaves_another_operational() {
	let bridge = start_test_bridge();
	let revoked = bridge.register(test_runtime_factory(), no_log());
	let survives = bridge.register(test_runtime_factory(), no_log());

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let revoked_url = format!("ws://127.0.0.1:{}/?t={}", revoked.port, revoked.token);
	let mut revoked_ws = rt.block_on(async {
		tokio_tungstenite::connect_async(&revoked_url)
			.await
			.expect("dial revoked execution")
			.0
	});

	bridge.revoke(&revoked.token);

	// Cancellation need not complete a WebSocket close handshake.
	rt.block_on(async {
		let deadline = tokio::time::sleep(std::time::Duration::from_secs(2));
		tokio::pin!(deadline);
		loop {
			tokio::select! {
				_ = &mut deadline => panic!("revoked connection was not closed"),
				frame = revoked_ws.next() => {
					match frame {
						None => break,
						Some(Err(_)) => break,
						Some(Ok(WsMessage::Close(_))) => continue,
						Some(Ok(_)) => continue,
					}
				}
			}
		}
	});

	let err = rt
		.block_on(async { tokio_tungstenite::connect_async(&revoked_url).await })
		.expect_err("revoked token must be rejected");
	assert!(format!("{err}").to_lowercase().contains("unauthorized"));

	let survives_url = format!("ws://127.0.0.1:{}/?t={}", survives.port, survives.token);
	rt.block_on(async {
		let (mut ws, _) = tokio_tungstenite::connect_async(&survives_url)
			.await
			.expect("surviving execution remains reachable");
		ws.close(None).await.expect("close client");
	});

	drop(bridge);
}

#[test]
fn reconnecting_after_revoke_gets_a_fresh_token() {
	let bridge = start_test_bridge();
	let first = bridge.register(test_runtime_factory(), no_log());
	bridge.revoke(&first.token);

	let second = bridge.register(test_runtime_factory(), no_log());
	assert_ne!(first.token, second.token);
	assert_eq!(first.port, second.port);

	connect(second.port, &second.token);

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let first_url = format!("ws://127.0.0.1:{}/?t={}", first.port, first.token);
	let err = rt
		.block_on(async { tokio_tungstenite::connect_async(&first_url).await })
		.expect_err("the revoked token must stay rejected");
	assert!(format!("{err}").to_lowercase().contains("unauthorized"));

	drop(bridge);
}

#[test]
fn host_shutdown_closes_every_registered_execution() {
	let bridge = start_test_bridge();
	let first = bridge.register(test_runtime_factory(), no_log());
	let second = bridge.register(test_runtime_factory(), no_log());

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let (mut first_ws, mut second_ws) = rt.block_on(async {
		let (first_ws, _) = tokio_tungstenite::connect_async(format!(
			"ws://127.0.0.1:{}/?t={}",
			first.port, first.token
		))
		.await
		.expect("dial first");
		let (second_ws, _) = tokio_tungstenite::connect_async(format!(
			"ws://127.0.0.1:{}/?t={}",
			second.port, second.token
		))
		.await
		.expect("dial second");
		(first_ws, second_ws)
	});

	drop(bridge);

	rt.block_on(async {
		let deadline = tokio::time::sleep(std::time::Duration::from_secs(2));
		tokio::pin!(deadline);
		tokio::select! {
			_ = &mut deadline => panic!("connections were not closed on host shutdown"),
			_ = async {
				while first_ws.next().await.is_some() {}
				while second_ws.next().await.is_some() {}
			} => {}
		}
	});
}

#[test]
fn shared_ws_bridge_lazily_starts_and_reuses_its_port() {
	let shared = SharedWsBridge::new(no_log());
	let first = shared
		.register(0, test_runtime_factory(), no_log())
		.expect("first registration starts the listener");
	let second = shared
		.register(0, test_runtime_factory(), no_log())
		.expect("second registration reuses it");

	assert_eq!(first.port, second.port);
	assert_ne!(first.token, second.token);

	connect(first.port, &first.token);
	connect(second.port, &second.token);

	shared.revoke(&first.token);
	connect(second.port, &second.token);
}

// Stands in for iOS reclaiming a suspended app's listening socket.
fn lose_listener(shared: &SharedWsBridge) {
	let mut guard = shared.inner.lock().expect("shared ws bridge mutex poisoned");
	let bridge = guard.as_mut().expect("listener started");
	bridge.listener.take().expect("listener running").stop(true);
}

fn recording_logger() -> (BridgeLogger, Arc<Mutex<Vec<String>>>) {
	let markers = Arc::new(Mutex::new(Vec::new()));
	let logger: BridgeLogger = Arc::new({
		let markers = markers.clone();
		move |marker, _| markers.lock().expect("markers mutex poisoned").push(marker.to_string())
	});
	(logger, markers)
}

/// Markers paired with their detail, as the host callback receives them.
type LoggedLines = Arc<Mutex<Vec<(String, String)>>>;

fn recording_detail_logger() -> (BridgeLogger, LoggedLines) {
	let lines = Arc::new(Mutex::new(Vec::new()));
	let logger: BridgeLogger = Arc::new({
		let lines = lines.clone();
		move |marker, detail| {
			lines
				.lock()
				.expect("log lines mutex poisoned")
				.push((marker.to_string(), detail.to_string()))
		}
	});
	(logger, lines)
}

fn logged_detail(lines: &LoggedLines, marker: &str) -> Option<String> {
	lines
		.lock()
		.expect("log lines mutex poisoned")
		.iter()
		.find(|(logged, _)| logged == marker)
		.map(|(_, detail)| detail.clone())
}

/// A host reading its log can tell a connection that was released from one
/// that is still held, which is what a leaked product socket looks like.
#[test]
fn a_connection_reports_what_its_execution_holds_as_it_opens_and_closes() {
	let (logger, lines) = recording_detail_logger();
	let socket = bind_loopback(0).expect("bind");
	let port = socket.local_addr().expect("local addr").port();
	let registry = Arc::new(WsBridgeRegistry::default());
	let token = "c".repeat(64);
	registry.insert(token.clone(), test_runtime_factory(), logger);
	let executor = shared_native_executor().expect("shared native executor");
	let listener = Listener::serve(socket, &executor.handle(), registry, no_log()).expect("serve");

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let url = format!("ws://127.0.0.1:{port}/?t={token}");
	let (ws, _) =
		rt.block_on(async { tokio_tungstenite::connect_async(&url).await.expect("dial") });

	crate::test_support::wait_until(
		|| logged_detail(&lines, markers::WS_BRIDGE_CONNECTION_OPEN.target).is_some(),
		"the connection never reported itself open",
	);
	let opened =
		logged_detail(&lines, markers::WS_BRIDGE_CONNECTION_OPEN.target).expect("an open marker");
	assert!(opened.contains("holds 1 of"), "open marker did not count the connection: {opened}");

	drop(ws);
	drop(rt);
	crate::test_support::wait_until(
		|| logged_detail(&lines, markers::WS_BRIDGE_CONNECTION_CLOSED.target).is_some(),
		"the connection never reported itself closed",
	);
	let closed = logged_detail(&lines, markers::WS_BRIDGE_CONNECTION_CLOSED.target)
		.expect("a closed marker");
	assert!(closed.contains("holds 0 of"), "closed marker did not release the slot: {closed}");
	listener.stop(true);
}

/// Products keep the port and token they were given when the host's
/// listener is reclaimed, so it has to come back exactly where they dial.
#[test]
fn relisten_restores_a_lost_listener_on_the_same_port() {
	let shared = SharedWsBridge::new(no_log());
	let endpoint = shared.register(0, test_runtime_factory(), no_log()).expect("register");
	lose_listener(&shared);
	std::net::TcpStream::connect(("127.0.0.1", endpoint.port))
		.expect_err("a lost listener refuses connections");

	shared.relisten();

	let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);
	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let response = rt.block_on(feature_supported_round_trip(&url, "p:1"));
	assert_eq!(
		(response.request_id, response.payload.message_type),
		("p:1".to_string(), crate::frame::MESSAGE_TYPE_RESPONSE)
	);
}

/// The host rebinds on every return to the foreground, including when the
/// listener survived, so a relisten must not end a live connection.
#[test]
fn relisten_keeps_established_connections_serving() {
	let mut bridge = start_test_bridge();
	let endpoint = bridge.register(test_runtime_factory(), no_log());
	let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);
	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let (mut established, _) = rt
		.block_on(tokio_tungstenite::connect_async(&url))
		.expect("dial before relisten");

	bridge.relisten().expect("relisten");

	let old = rt.block_on(feature_supported_exchange(&mut established, "p:old"));
	let new = rt.block_on(feature_supported_round_trip(&url, "p:new"));
	assert_eq!((old.request_id, new.request_id), ("p:old".to_string(), "p:new".to_string()));
}

/// Products hold the endpoint they were given, so only a listening socket
/// that is itself gone may give up its port. One pending connection failing
/// or a passing shortage of descriptors or buffers must keep it.
#[test]
fn only_a_listener_that_is_gone_gives_up_its_port() {
	let gone = |code| is_listener_gone(&io::Error::from_raw_os_error(code));
	assert_eq!(
		[
			libc::EINVAL,
			libc::EBADF,
			libc::ENOTSOCK,
			libc::EMFILE,
			libc::ENFILE,
			libc::ENOBUFS,
			libc::ECONNABORTED,
			libc::EPROTO,
			libc::ENETUNREACH,
		]
		.map(gone),
		[true, true, true, false, false, false, false, false, false],
	);
}

/// A listener the kernel tears down, as Android does when a socket is
/// destroyed under the app, fails every accept. The loop has to rebind the
/// port once instead of spinning, so products that hold the endpoint can
/// reconnect.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_listener_torn_down_by_the_kernel_is_rebound_on_its_port() {
	let (logger, markers) = recording_logger();
	let socket = bind_loopback(0).expect("bind");
	let port = socket.local_addr().expect("local addr").port();
	let teardown = socket.try_clone().expect("second handle on the listener");
	let registry = Arc::new(WsBridgeRegistry::default());
	let token = "a".repeat(64);
	registry.insert(token.clone(), test_runtime_factory(), no_log());
	let executor = shared_native_executor().expect("shared native executor");
	let listener = Listener::serve(socket, &executor.handle(), registry, logger).expect("serve");

	socket2::SockRef::from(&teardown)
		.shutdown(std::net::Shutdown::Read)
		.expect("shut the listener down");
	drop(teardown);
	crate::test_support::wait_until(
		|| {
			markers
				.lock()
				.expect("markers mutex poisoned")
				.iter()
				.any(|marker| marker == markers::WS_BRIDGE_RELISTENED.target)
		},
		"the torn-down listener was not rebound",
	);

	connect(port, &token);
	let lifecycle: Vec<String> = markers
		.lock()
		.expect("markers mutex poisoned")
		.iter()
		.filter(|marker| marker.contains("listener_lost") || marker.contains("relisten"))
		.cloned()
		.collect();
	assert_eq!(
		lifecycle,
		[markers::WS_BRIDGE_LISTENER_LOST.target, markers::WS_BRIDGE_RELISTENED.target]
	);
	listener.stop(true);
}

/// A rebind can lose the port to another socket; the next return to the
/// foreground must still be able to restore it.
#[test]
fn a_failed_relisten_is_retried_by_the_next_one() {
	let (logger, markers) = recording_logger();
	let shared = SharedWsBridge::new(logger);
	let endpoint = shared.register(0, test_runtime_factory(), no_log()).expect("register");
	lose_listener(&shared);
	let squatter =
		std::net::TcpListener::bind(("127.0.0.1", endpoint.port)).expect("take the port");

	shared.relisten();
	drop(squatter);
	shared.relisten();

	connect(endpoint.port, &endpoint.token);
	let relisten_markers: Vec<String> = markers
		.lock()
		.expect("markers mutex poisoned")
		.iter()
		.filter(|marker| marker.contains("relisten"))
		.cloned()
		.collect();
	assert_eq!(
		relisten_markers,
		[markers::WS_BRIDGE_RELISTEN_FAILED.target, markers::WS_BRIDGE_RELISTENED.target]
	);
}

#[test]
fn per_execution_cap_rejects_the_connection_past_the_limit() {
	let bridge = start_test_bridge();
	let endpoint = bridge.register(test_runtime_factory(), no_log());
	let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let _sockets = rt.block_on(async {
		let mut sockets = Vec::new();
		for _ in 0..MAX_WS_CONNECTIONS_PER_EXECUTION {
			let (ws, _) = tokio_tungstenite::connect_async(&url)
				.await
				.expect("dial under the per-execution cap");
			sockets.push(ws);
		}
		sockets
	});

	let err = rt
		.block_on(async { tokio_tungstenite::connect_async(&url).await })
		.expect_err("the connection past the per-execution cap must be refused");
	let msg = format!("{err}").to_lowercase();
	assert!(
		msg.contains("503") || msg.contains("service unavailable"),
		"expected a 503 rejection past the per-execution cap, got: {err}",
	);

	drop(bridge);
}

#[test]
fn total_capacity_is_reusable_after_a_retained_connection_closes() {
	let bridge = start_test_bridge();
	let extra = bridge.register(test_runtime_factory(), no_log());
	let controls = Arc::new(Mutex::new(Vec::new()));
	let inner = test_runtime_factory();
	let factory: Arc<dyn WsProductRuntimeFactory> = Arc::new({
		let controls = controls.clone();
		move |sink| {
			let runtime = inner.product_runtime(sink);
			controls.lock().expect("controls mutex poisoned").push(runtime.control());
			runtime
		}
	});

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");

	let extra_url = format!("ws://127.0.0.1:{}/?t={}", extra.port, extra.token);
	let mut sockets = rt.block_on(async {
		let mut sockets = Vec::new();
		for _ in 0..MAX_TOTAL_WS_CONNECTIONS / MAX_WS_CONNECTIONS_PER_EXECUTION {
			let endpoint = bridge.register(factory.clone(), no_log());
			for _ in 0..MAX_WS_CONNECTIONS_PER_EXECUTION {
				let (socket, _) = tokio_tungstenite::connect_async(format!(
					"ws://127.0.0.1:{}/?t={}",
					endpoint.port, endpoint.token
				))
				.await
				.expect("connect within capacity");
				sockets.push(socket);
			}
		}
		sockets
	});
	crate::test_support::wait_until(
		|| controls.lock().expect("controls mutex poisoned").len() == MAX_TOTAL_WS_CONNECTIONS,
		"connections did not create their controls",
	);
	let err = rt
		.block_on(async { tokio_tungstenite::connect_async(&extra_url).await })
		.expect_err("a fresh execution must still be refused once the shared listener is full");
	let msg = format!("{err}").to_lowercase();
	assert!(
		msg.contains("503") || msg.contains("service unavailable"),
		"expected a 503 rejection past the total cap, got: {err}",
	);

	rt.block_on(async {
		sockets[0].close(None).await.expect("close one client");
		tokio::time::timeout(std::time::Duration::from_secs(2), async {
			loop {
				if let Ok((socket, _)) = tokio_tungstenite::connect_async(&extra_url).await {
					break socket;
				}
				tokio::time::sleep(std::time::Duration::from_millis(5)).await;
			}
		})
		.await
		.expect("a disconnected execution must release capacity for another product");
	});

	drop(bridge);
}

// Tokio contains task panics in test builds. Release builds use panic=abort,
// so this test cannot promise panic isolation in a production host.
#[test]
fn a_panicking_execution_does_not_affect_a_sibling() {
	let panicking_factory: Arc<dyn WsProductRuntimeFactory> =
		Arc::new(|_sink: Arc<dyn FrameSink>| -> ProductRuntime {
			panic!("intentional test panic: simulating a failing product execution")
		});

	let bridge = start_test_bridge();
	let failing = bridge.register(panicking_factory, no_log());
	let healthy = bridge.register(test_runtime_factory(), no_log());

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");

	rt.block_on(async {
		let failing_url = format!("ws://127.0.0.1:{}/?t={}", failing.port, failing.token);
		let (mut ws, _) = tokio_tungstenite::connect_async(&failing_url)
			.await
			.expect("handshake succeeds; the token itself is valid");
		let deadline = tokio::time::sleep(std::time::Duration::from_secs(2));
		tokio::pin!(deadline);
		loop {
			tokio::select! {
				_ = &mut deadline => panic!("the panicking execution's connection was never closed"),
				frame = ws.next() => match frame {
					None | Some(Err(_)) => break,
					Some(Ok(_)) => continue,
				}
			}
		}
	});

	let healthy_url = format!("ws://127.0.0.1:{}/?t={}", healthy.port, healthy.token);
	rt.block_on(async {
		let (mut ws, _) = tokio_tungstenite::connect_async(&healthy_url)
			.await
			.expect("sibling execution remains reachable");
		ws.close(None).await.expect("close client");
	});

	drop(bridge);
}

#[test]
fn a_stalled_handshake_does_not_block_a_sibling_connection() {
	let bridge = start_test_bridge();
	let stalled = bridge.register(test_runtime_factory(), no_log());
	let healthy = bridge.register(test_runtime_factory(), no_log());

	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");

	rt.block_on(async {
		let _stalled_stream = tokio::net::TcpStream::connect(("127.0.0.1", stalled.port))
			.await
			.expect("open a raw stream to the shared port");

		let healthy_url = format!("ws://127.0.0.1:{}/?t={}", healthy.port, healthy.token);
		let deadline = tokio::time::sleep(std::time::Duration::from_secs(2));
		tokio::pin!(deadline);
		tokio::select! {
			_ = &mut deadline => panic!(
				"sibling connection was blocked by another connection's stalled handshake"
			),
			result = tokio_tungstenite::connect_async(&healthy_url) => {
				let (mut ws, _) = result.expect("sibling handshake must succeed promptly");
				ws.close(None).await.expect("close client");
			}
		}
	});

	drop(bridge);
}

#[test]
fn three_tokens_route_to_their_own_factory_only() {
	fn tracked_factory(called: Arc<AtomicUsize>) -> Arc<dyn WsProductRuntimeFactory> {
		let inner = test_runtime_factory();
		Arc::new(move |sink| {
			called.fetch_add(1, Ordering::SeqCst);
			inner.product_runtime(sink)
		})
	}

	let bridge = start_test_bridge();
	let calls: Vec<Arc<AtomicUsize>> = (0..3).map(|_| Arc::new(AtomicUsize::new(0))).collect();
	let endpoints: Vec<WsBridgeEndpoint> = calls
		.iter()
		.map(|called| bridge.register(tracked_factory(called.clone()), no_log()))
		.collect();

	// A response proves the factory ran; the HTTP upgrade alone does not.
	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("test runtime");
	let url = format!("ws://127.0.0.1:{}/?t={}", endpoints[1].port, endpoints[1].token);
	let ids = request_ids("system_feature_supported").expect("known request method");
	rt.block_on(async {
		let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");
		let request_frame = ProtocolMessage {
			request_id: "p:1".into(),
			payload: Payload {
				trait_id: ids.trait_id,
				method_id: ids.method_id,
				message_type: crate::frame::MESSAGE_TYPE_REQUEST,
				value: truapi::versioned::system::HostFeatureSupportedRequest::V1(
					v01::HostFeatureSupportedRequest::Chain { genesis_hash: vec![0u8; 32] },
				)
				.encode(),
			},
		};
		ws.send(WsMessage::Binary(request_frame.encode())).await.expect("send");
		loop {
			match ws.next().await {
				Some(Ok(WsMessage::Binary(_))) => break,
				Some(Ok(_)) => continue,
				Some(Err(err)) => panic!("ws error: {err}"),
				None => panic!("connection closed before response"),
			}
		}
	});

	assert_eq!(calls[0].load(Ordering::SeqCst), 0, "a sibling's factory must not be invoked");
	assert_eq!(
		calls[1].load(Ordering::SeqCst),
		1,
		"the matching token's own factory must be invoked exactly once"
	);
	assert_eq!(calls[2].load(Ordering::SeqCst), 0, "a sibling's factory must not be invoked");

	drop(bridge);
}
