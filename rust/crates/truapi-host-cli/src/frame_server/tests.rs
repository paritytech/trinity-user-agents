use super::*;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::{sync::Notify, task::JoinHandle};
use tokio_tungstenite::{client_async, tungstenite::client::IntoClientRequest};

use crate::chain::ChainBackend;

struct UnusedRuntimeFactory;

impl ProductRuntimeFactory for UnusedRuntimeFactory {
	fn product_runtime(
		&self,
		_product: ProductContext,
		_sink: Arc<dyn FrameSink>,
	) -> ProductRuntime {
		panic!("HTTP requests and rejected handshakes must not create a product runtime")
	}
}

#[derive(Default)]
struct PendingRuntime {
	dispatch_started: Notify,
	second_dispatch_finished: Notify,
	dispatch_cancelled: Arc<AtomicBool>,
	dispose_calls: AtomicUsize,
}

struct DispatchCancellation(Arc<AtomicBool>);

impl Drop for DispatchCancellation {
	fn drop(&mut self) {
		self.0.store(true, Ordering::SeqCst);
	}
}

#[async_trait::async_trait]
impl ConnectionRuntime for PendingRuntime {
	async fn receive_frame(&self, frame: Vec<u8>) -> Result<(), ProductRuntimeError> {
		if frame != [0] {
			self.second_dispatch_finished.notify_one();
			return Ok(());
		}
		let _cancellation = DispatchCancellation(self.dispatch_cancelled.clone());
		self.dispatch_started.notify_one();
		std::future::pending().await
	}

	fn dispose(&self) {
		self.dispose_calls.fetch_add(1, Ordering::SeqCst);
	}
}

async fn start_tcp_server(
	runtime: Arc<dyn ProductRuntimeFactory>,
) -> Result<(SocketAddr, JoinHandle<Result<()>>)> {
	let listener = TcpListener::bind("127.0.0.1:0").await?;
	let address = listener.local_addr()?;
	let endpoint = format!("ws://{address}");
	let product = ProductSelection::new(ProductId::for_test("localhost:3000"), ExecutableKind::App);
	let container = tempfile::NamedTempFile::new()?;
	std::fs::write(container.path(), "installSandbox();")?;
	let server = tokio::spawn(async move {
		let (stream, _) = listener.accept().await?;
		serve_tcp_connection(runtime, product, stream, &endpoint, container.path()).await
	});
	Ok((address, server))
}

fn signing_runtime() -> Result<Arc<dyn ProductRuntimeFactory>> {
	let network = crate::network::Network::default().config(ChainBackend::Rpc);
	let platform = crate::platform::CliPlatform::new(
		&network,
		None,
		crate::platform::ApprovalPolicy::AutoAccept,
		None,
	);
	let config = truapi::platform::SigningHostConfig::new(
		truapi::platform::HostInfo {
			name: "Frame server test".into(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Cli,
		},
		truapi::platform::PlatformInfo { kind: Some("test".into()), version: None },
		network.chains.people.genesis,
		network.chains.bulletin.genesis,
		network.chains.asset_hub.genesis,
		network.network_suffix.to_string(),
	)?;
	let spawner: truapi::subscription::Spawner = Arc::new(|_| {});
	Ok(Arc::new(SigningHostRuntime::new(platform, config, spawner)))
}

#[test]
fn the_peer_and_origin_policy_requires_both_tcp_signals_to_be_local() -> Result<()> {
	let loopback = ConnectionPeer::Tcp("127.0.0.1:3000".parse()?);
	let remote = ConnectionPeer::Tcp("192.0.2.1:3000".parse()?);
	let expected = [
		(ConnectionPeer::LocalSocket, None, true),
		(ConnectionPeer::LocalSocket, Some("http://localhost:3000"), true),
		(ConnectionPeer::LocalSocket, Some("https://evil.example"), false),
		(loopback, None, true),
		(loopback, Some("http://LocalHost:3000"), true),
		(loopback, Some("http://127.0.0.2:3000"), true),
		(loopback, Some("http://[::1]:5173"), true),
		(loopback, Some("https://evil.example"), false),
		(loopback, Some("http://localhost.evil.example"), false),
		(loopback, Some("http://rebind.evil.example"), false),
		(loopback, Some("null"), false),
		(loopback, Some("file://"), false),
		(remote, None, false),
		(remote, Some("http://localhost:3000"), false),
		(remote, Some("https://evil.example"), false),
	];
	let actual = expected.map(|(peer, origin, _)| (peer, origin, connection_allowed(peer, origin)));

	assert_eq!(actual, expected);
	Ok(())
}

#[test]
fn a_request_head_is_classified_by_its_upgrade_header() {
	let upgrade =
		b"GET / HTTP/1.1\r\nHost: x\r\nUpgrade: WebSocket\r\nOrigin: http://localhost:3000";
	assert!(is_websocket_upgrade(upgrade));
	assert_eq!(request_path(upgrade).as_deref(), Some("/"));

	let script = b"GET /bootstrap.js?v=2 HTTP/1.1\r\nHost: 127.0.0.1:9955";
	assert!(!is_websocket_upgrade(script));
	assert_eq!(request_path(script).as_deref(), Some("/bootstrap.js"));
}

/// The bridge and the frame socket share one port, so a plain GET must be
/// answered as HTTP, and the script must name the endpoint to dial.
#[tokio::test]
async fn the_bridge_script_is_served_beside_the_frame_socket() -> Result<()> {
	use tokio::io::{AsyncReadExt, AsyncWriteExt};

	async fn fetch(target: &str, endpoint: &str, asset_present: bool) -> Result<String> {
		let listener = TcpListener::bind("127.0.0.1:0").await?;
		let address = listener.local_addr()?;
		let endpoint = endpoint.to_string();
		let container = tempfile::NamedTempFile::new()?;
		std::fs::write(container.path(), "installSandbox();")?;
		if !asset_present {
			std::fs::remove_file(container.path())?;
		}
		let server = tokio::spawn(async move {
			let (mut stream, _) = listener.accept().await?;
			let request = read_request_head(&mut stream).await?;
			assert!(!is_websocket_upgrade(request.head()));
			serve_bridge_script(&mut stream, request.head(), &endpoint, container.path()).await
		});

		let mut client = TcpStream::connect(address).await?;
		client
			.write_all(format!("GET {target} HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes())
			.await?;
		let mut response = String::new();
		client.read_to_string(&mut response).await?;
		server.await??;
		Ok(response)
	}

	let endpoint = "ws://127.0.0.1:9955";
	let script = fetch(bootstrap::PATH, endpoint, true).await?;
	assert!(script.starts_with("HTTP/1.1 200 OK"), "{script}");
	assert!(script.contains("application/javascript"));
	assert!(
		script.contains(&format!(r#"window.__truapi_localhost = {{ url: "{endpoint}" }};"#)),
		"{script}"
	);
	assert!(script.ends_with("\ninstallSandbox();"));

	let missing = fetch("/nope", endpoint, true).await?;
	assert!(missing.starts_with("HTTP/1.1 404 Not Found"), "{missing}");
	let missing_asset = fetch(bootstrap::PATH, endpoint, false).await?;
	assert!(missing_asset.starts_with("HTTP/1.1 500 Internal Server Error"), "{missing_asset}");
	assert!(missing_asset.contains("reinstall truapi-host"), "{missing_asset}");
	Ok(())
}

#[tokio::test]
async fn the_tcp_server_routes_the_bridge_script() -> Result<()> {
	let (address, server) = start_tcp_server(Arc::new(UnusedRuntimeFactory)).await?;
	let mut client = TcpStream::connect(address).await?;
	client
		.write_all(
			format!(
				"GET {} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n",
				bootstrap::PATH
			)
			.as_bytes(),
		)
		.await?;

	let mut response = String::new();
	client.read_to_string(&mut response).await?;
	server.await??;

	assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
	assert!(
		response.contains(&format!(r#"window.__truapi_localhost = {{ url: "ws://{address}" }};"#)),
		"{response}"
	);
	Ok(())
}

/// A partial head must yield until more bytes arrive. Peeking the same
/// unread bytes in a loop monopolizes a current-thread executor, so the
/// client task can never send the second fragment.
#[tokio::test(flavor = "current_thread")]
async fn fragmented_request_heads_do_not_starve_the_executor() -> Result<()> {
	let listener = TcpListener::bind("127.0.0.1:0").await?;
	let address = listener.local_addr()?;
	let mut client = TcpStream::connect(address).await?;
	let (server_stream, _) = listener.accept().await?;
	client.write_all(b"GET /boot").await?;
	let product = ProductSelection::new(ProductId::for_test("localhost:3000"), ExecutableKind::App);
	let endpoint = format!("ws://{address}");

	let client_exchange = async move {
		tokio::time::sleep(Duration::from_millis(10)).await;
		client.write_all(b"strap.js HTTP/1.1\r\nHost: localhost\r\n\r\n").await?;
		let mut response = String::new();
		client.read_to_string(&mut response).await?;
		anyhow::ensure!(response.starts_with("HTTP/1.1 200 OK\r\n"), response);
		Ok::<(), anyhow::Error>(())
	};
	let container = tempfile::NamedTempFile::new()?;
	std::fs::write(container.path(), "installSandbox();")?;
	let server_exchange = serve_tcp_connection(
		Arc::new(UnusedRuntimeFactory),
		product,
		server_stream,
		&endpoint,
		container.path(),
	);

	tokio::try_join!(server_exchange, client_exchange)?;
	Ok(())
}

#[tokio::test]
async fn a_loopback_origin_connection_finishes_when_its_client_disconnects() -> Result<()> {
	let (address, server) = start_tcp_server(signing_runtime()?).await?;
	let mut request = format!("ws://{address}/").into_client_request()?;
	request
		.headers_mut()
		.insert(header::ORIGIN, "http://localhost:3000".parse().expect("valid origin"));
	let stream = TcpStream::connect(address).await?;

	let (websocket, response) = client_async(request, stream).await?;
	assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
	drop(websocket);
	tokio::time::timeout(Duration::from_secs(1), server).await???;
	Ok(())
}

#[tokio::test]
async fn disconnect_cancels_pending_dispatch_and_disposes_runtime() -> Result<()> {
	let listener = TcpListener::bind("127.0.0.1:0").await?;
	let address = listener.local_addr()?;
	let runtime = Arc::new(PendingRuntime::default());
	let server_runtime = runtime.clone();
	let product = ProductSelection::new(ProductId::for_test("localhost:3000"), ExecutableKind::App);
	let product_updates = product.subscribe();
	let server = tokio::spawn(async move {
		let (stream, _) = listener.accept().await?;
		let websocket = accept_async(stream).await?;
		let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
		let result = drive_connection(
			websocket,
			server_runtime,
			None,
			product_updates,
			outbound_tx,
			outbound_rx,
		)
		.await;
		drop(product);
		result
	});

	let stream = TcpStream::connect(address).await?;
	let (mut websocket, _) = client_async("ws://localhost/", stream).await?;
	websocket.send(Message::Binary(vec![0])).await?;
	tokio::time::timeout(Duration::from_secs(1), runtime.dispatch_started.notified()).await?;
	websocket.send(Message::Binary(vec![1])).await?;
	tokio::time::timeout(Duration::from_secs(1), runtime.second_dispatch_finished.notified())
		.await?;
	drop(websocket);

	tokio::time::timeout(Duration::from_secs(1), server).await???;
	assert_eq!(
		(
			runtime.dispose_calls.load(Ordering::SeqCst),
			runtime.dispatch_cancelled.load(Ordering::SeqCst),
		),
		(1, true),
	);
	Ok(())
}

#[tokio::test]
async fn a_loopback_tcp_client_may_omit_origin() -> Result<()> {
	let (address, server) = start_tcp_server(signing_runtime()?).await?;
	let request = format!("ws://{address}/").into_client_request()?;
	assert!(!request.headers().contains_key(header::ORIGIN));
	let stream = TcpStream::connect(address).await?;

	let (websocket, response) = client_async(request, stream).await?;
	assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
	drop(websocket);
	server.abort();
	assert!(
		server
			.await
			.expect_err("aborted connection task must be cancelled")
			.is_cancelled()
	);
	Ok(())
}

#[tokio::test]
async fn a_non_loopback_origin_cannot_open_the_tcp_websocket() -> Result<()> {
	let (address, server) = start_tcp_server(Arc::new(UnusedRuntimeFactory)).await?;
	let mut request = format!("ws://{address}/").into_client_request()?;
	request
		.headers_mut()
		.insert(header::ORIGIN, "https://evil.example".parse().expect("valid origin"));
	let stream = TcpStream::connect(address).await?;

	let error = client_async(request, stream)
		.await
		.expect_err("a non-loopback browser origin must be rejected");
	let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
		panic!("expected an HTTP handshake rejection, got {error}")
	};
	assert_eq!(response.status(), StatusCode::FORBIDDEN);
	assert!(server.await?.is_err());
	Ok(())
}

#[test]
fn product_selection_compares_canonical_ids() {
	let product = ProductSelection::new(ProductId::for_test(" Dotli.DOT "), ExecutableKind::App);

	assert_eq!(product.current().as_str(), "dotli.dot");
	assert!(product.select(ProductId::for_test("localhost:3000")));
	assert_eq!(product.current().as_str(), "localhost:3000");
	assert!(!product.select(ProductId::for_test("LOCALHOST:3000")));
}

#[tokio::test]
async fn changing_product_notifies_connections() -> Result<()> {
	let product = ProductSelection::new(ProductId::for_test("first.dot"), ExecutableKind::App);
	let mut connection = product.subscribe();

	assert!(product.select(ProductId::for_test("second.dot")));
	connection.changed().await?;
	assert_eq!(connection.borrow().product_id.as_str(), "second.dot");
	assert!(!product.select(ProductId::for_test("SECOND.DOT")));
	assert!(!connection.has_changed()?);
	Ok(())
}

#[tokio::test]
async fn explicit_tcp_listener_reports_the_actual_bound_port() -> Result<()> {
	let server = bind(Some("127.0.0.1:0".parse()?)).await?;

	assert!(server.endpoint().starts_with("ws://127.0.0.1:"));
	assert!(!server.endpoint().ends_with(":0"));
	Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn default_unix_listener_carries_websocket_frames_and_cleans_up() -> Result<()> {
	use tokio::net::UnixStream;

	let server = bind(None).await?;
	let socket_path = server.endpoint().strip_prefix("ws+unix:").expect("Unix endpoint prefix");
	let socket_path = std::path::PathBuf::from(socket_path);
	let socket_directory =
		socket_path.parent().expect("socket has a parent directory").to_path_buf();
	assert!(socket_path.exists());

	let FrameListener::Unix(listener) = &server.listener else {
		panic!("default listener must be Unix");
	};
	let server_exchange = async {
		let (stream, _) = listener.accept().await?;
		let mut websocket = accept_async(stream).await?;
		let message = websocket.next().await.context("client closed before sending")??;
		websocket.send(message).await?;
		Ok::<(), anyhow::Error>(())
	};
	let client_exchange = async {
		let stream = UnixStream::connect(&socket_path).await?;
		let (mut websocket, _) = client_async("ws://localhost/", stream).await?;
		websocket.send(Message::Binary(vec![1, 2, 3, 4])).await?;
		assert_eq!(
			websocket.next().await.context("server did not echo")??,
			Message::Binary(vec![1, 2, 3, 4])
		);
		Ok::<(), anyhow::Error>(())
	};
	tokio::try_join!(server_exchange, client_exchange)?;

	drop(server);
	assert!(!socket_directory.exists());
	Ok(())
}

/// A grant between two products, declared the way a developer declares it.
///
/// `peopl.paseo`'s local product config names `dim2` in `trustedProducts`,
/// exactly as the publisher will read it when the product is deployed. The
/// host applies that config and the grant resolves for the run, which is
/// the only way to exercise a partner flow before either product exists on
/// chain.
///
/// This runs against the real `CliPlatform`, so it also pins the half a stub
/// cannot: the host keys storage off the owner encoded in the key rather
/// than off the product that asked, without which every granted read comes
/// back empty and the grant silently does nothing.
mod cross_product_storage {
	use super::*;
	use crate::product_config::{self, LocalProductConfig};
	use parity_scale_codec::{Decode, Encode};
	use std::sync::Mutex;
	use truapi::{
		frame::{
			MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE, Payload, ProtocolMessage, request_ids,
		},
		v01, v02,
		versioned::local_storage::{
			HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
			HostLocalStorageWriteRequest,
		},
	};

	const OWNER: &str = "peopl.paseo";
	const CALLER: &str = "dim2.paseo";
	const KEY: &str = "unwrapped";
	const VALUE: &[u8] = b"the collection state both products describe";

	/// Collects the frames a product runtime answers with.
	#[derive(Default)]
	struct CapturedFrames(Mutex<Vec<Vec<u8>>>);

	impl FrameSink for CapturedFrames {
		fn emit_frame(&self, frame: Vec<u8>) {
			self.0.lock().expect("frame mutex poisoned").push(frame);
		}
	}

	impl CapturedFrames {
		fn take_one(&self) -> Vec<u8> {
			let mut frames = self.0.lock().expect("frame mutex poisoned");
			assert_eq!(frames.len(), 1, "expected exactly one answer");
			frames.remove(0)
		}
	}

	fn host() -> Result<(Arc<crate::platform::CliPlatform>, SigningHostRuntime)> {
		// Every chain routes to a closed port, so nothing connects to live Paseo.
		let mut network = crate::network::Network::default().config(ChainBackend::Rpc);
		network.chains = network.chains.map(|chain| crate::network::ChainEndpoint {
			ws: "ws://127.0.0.1:1".to_owned(),
			..chain
		});
		let asset_hub_genesis = network.chains.asset_hub.genesis;
		let platform = crate::platform::CliPlatform::new(
			&network,
			None,
			crate::platform::ApprovalPolicy::AutoAccept,
			None,
		);
		let config = truapi::platform::SigningHostConfig::new(
			truapi::platform::HostInfo {
				name: "Cross-product storage test".into(),
				icon: None,
				version: None,
				platform: truapi::latest::HostPlatform::Cli,
			},
			truapi::platform::PlatformInfo { kind: Some("test".into()), version: None },
			network.chains.people.genesis,
			network.chains.bulletin.genesis,
			network.chains.asset_hub.genesis,
			network.network_suffix.to_string(),
		)?;
		assert_eq!(
			platform.routed_url(&asset_hub_genesis),
			"ws://127.0.0.1:1",
			"the Asset Hub genesis must route to the closed port, not to live Paseo"
		);
		let spawner: truapi::subscription::Spawner = Arc::new(|_| {});
		Ok((platform.clone(), SigningHostRuntime::new(platform, config, spawner)))
	}

	/// Apply `OWNER`'s local product config, `trusted` being the
	/// `trustedProducts` a developer wrote in it.
	async fn apply_config(platform: &crate::platform::CliPlatform, trusted: &str) -> String {
		let config: LocalProductConfig = serde_json::from_str(&format!(
			r#"{{"productName":"{OWNER}","displayName":"Personhood",
                     "trustedProducts":{trusted}}}"#
		))
		.expect("the config parses");
		let now = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.expect("clock is after the epoch")
			.as_secs();
		let applied = product_config::apply(platform, std::slice::from_ref(&config), now)
			.await
			.expect("the config applies");
		applied.lines.join("\n")
	}

	async fn call(
		runtime: &SigningHostRuntime,
		product: &str,
		method: &str,
		payload: Vec<u8>,
	) -> Vec<u8> {
		let ids = request_ids(method).expect("the method is in the wire table");
		let frames = Arc::new(CapturedFrames::default());
		let connection = runtime
			.product_runtime(ProductContext::new(ProductId::for_test(product)), frames.clone());
		let frame = ProtocolMessage {
			request_id: "1".to_string(),
			payload: Payload {
				trait_id: ids.trait_id,
				method_id: ids.method_id,
				message_type: MESSAGE_TYPE_REQUEST,
				value: payload,
			},
		}
		.encode();
		connection.receive_frame(frame).await.expect("the host answers the frame");
		let answered = ProtocolMessage::decode(&mut frames.take_one().as_slice())
			.expect("the answer is a protocol message");
		assert_eq!(
			(answered.payload.trait_id, answered.payload.method_id, answered.payload.message_type,),
			(ids.trait_id, ids.method_id, MESSAGE_TYPE_RESPONSE),
			"answered on the wrong discriminant"
		);
		answered.payload.value
	}

	/// Store a value in `OWNER`'s own storage, through `OWNER`'s connection.
	async fn write_owner_value(runtime: &SigningHostRuntime) {
		let payload = HostLocalStorageWriteRequest::V1(v01::HostLocalStorageWriteRequest {
			key: KEY.to_string(),
			value: VALUE.to_vec(),
		})
		.encode();
		call(runtime, OWNER, "local_storage_write", payload).await;
	}

	/// `CALLER` reads `OWNER`'s storage at `KEY`.
	async fn read_across(
		runtime: &SigningHostRuntime,
	) -> Result<Option<Vec<u8>>, v02::HostLocalStorageReadError> {
		let payload = HostLocalStorageReadRequest::V2(v02::HostLocalStorageReadRequest {
			product: Some(ProductId::for_test(OWNER)),
			key: KEY.to_string(),
		})
		.encode();
		let answer = call(runtime, CALLER, "local_storage_read", payload).await;
		// `[Result tag][version][body]`: the frame addresses the method, and
		// the payload is the method's own versioned wrapper inside a Result.
		let decoded: Result<
			HostLocalStorageReadResponse,
			truapi::CallError<HostLocalStorageReadError>,
		> = Decode::decode(&mut answer.as_slice()).expect("the answer decodes");
		match decoded {
			Ok(HostLocalStorageReadResponse::V2(v01::HostLocalStorageReadResponse { value })) => {
				Ok(value)
			},
			Ok(other) => panic!("expected a v0.2 answer: {other:?}"),
			Err(truapi::CallError::Domain(HostLocalStorageReadError::V2(error))) => Err(error),
			Err(other) => panic!("unexpected refusal: {other:?}"),
		}
	}

	#[tokio::test]
	async fn a_granted_product_reads_the_granting_products_storage() -> Result<()> {
		let (platform, runtime) = host()?;
		let transcript = apply_config(&platform, r#"{"dim2":["storage"]}"#).await;
		// Nobody should ship believing this grant is published.
		assert_eq!(transcript, "peopl.paseo: dim2 -> [storage]");
		write_owner_value(&runtime).await;
		assert_eq!(read_across(&runtime).await, Ok(Some(VALUE.to_vec())));
		Ok(())
	}

	#[tokio::test]
	async fn an_ungranted_product_is_refused() -> Result<()> {
		let (platform, runtime) = host()?;
		apply_config(&platform, r#"{"stash":["storage"]}"#).await;
		write_owner_value(&runtime).await;
		assert_eq!(
			read_across(&runtime).await,
			Err(v02::HostLocalStorageReadError::AccessNotGranted)
		);
		Ok(())
	}

	#[tokio::test]
	async fn a_product_with_no_config_is_refused_the_same_way() -> Result<()> {
		// An Asset Hub that never answers is refused identically to a
		// config that named someone else: the caller cannot tell "not
		// granted" from "could not look it up".
		let (_platform, runtime) = host()?;
		write_owner_value(&runtime).await;
		assert_eq!(
			read_across(&runtime).await,
			Err(v02::HostLocalStorageReadError::AccessNotGranted)
		);
		Ok(())
	}
}

/// A sink that records nothing: these tests are about the decorator's
/// delegation, not about what reaches the debugger.
struct SilentSink;

impl DebugSink for SilentSink {
	fn emit(&self, _event: truapi::DebugEvent) {}
}

/// A factory that reports a reset signal and refuses to build runtimes, so a
/// test can observe `connection_reset` forwarding on its own.
struct ResettingFactory {
	tx: watch::Sender<u64>,
}

impl ProductRuntimeFactory for ResettingFactory {
	fn product_runtime(
		&self,
		_product: ProductContext,
		_sink: Arc<dyn FrameSink>,
	) -> ProductRuntime {
		panic!("this test observes connection_reset only")
	}

	fn connection_reset(&self) -> Option<watch::Receiver<u64>> {
		Some(self.tx.subscribe())
	}
}

/// `DebugTappedRuntime` must forward `connection_reset`, not fall back to the
/// trait default. Returning `None` would leave product connections alive
/// across a session switch that exists to invalidate them, and nothing else
/// in the accept loop would report the omission.
#[tokio::test]
async fn the_debug_tap_forwards_the_connection_reset_signal() -> Result<()> {
	let (tx, _keepalive) = watch::channel(0u64);
	let inner: Arc<dyn ProductRuntimeFactory> = Arc::new(ResettingFactory { tx: tx.clone() });
	let tapped = DebugTappedRuntime::new(inner, Arc::new(SilentSink));

	let mut reset = tapped.connection_reset().context("the tap dropped the reset signal")?;
	tx.send(7)?;
	reset.changed().await?;
	assert_eq!(*reset.borrow_and_update(), 7);
	Ok(())
}

/// A factory with no reset signal must stay that way through the decorator:
/// forwarding is delegation, not fabrication.
#[test]
fn the_debug_tap_reports_no_reset_signal_when_the_inner_factory_has_none() {
	let tapped = DebugTappedRuntime::new(Arc::new(UnusedRuntimeFactory), Arc::new(SilentSink));
	assert!(tapped.connection_reset().is_none());
}

/// Frame sink for the runtimes the tests below build and dispose.
struct DiscardingFrameSink;

impl FrameSink for DiscardingFrameSink {
	fn emit_frame(&self, _frame: Vec<u8>) {}
}

/// Records what actually reached the debugger, so the tap is asserted on its
/// effect rather than on the wiring call not panicking.
#[derive(Default)]
struct RecordingDebugSink {
	frames: std::sync::Mutex<Vec<(String, truapi::FrameDirection, Vec<u8>)>>,
}

impl DebugSink for RecordingDebugSink {
	fn emit(&self, event: truapi::DebugEvent) {
		// `DebugEvent` is `#[non_exhaustive]` so host-internal events can be
		// added without a break; a tap that only wants frames ignores the rest.
		if let truapi::DebugEvent::Frame { channel_id, dir, bytes } = event {
			self.frames.lock().expect("recording debug sink mutex poisoned").push((
				channel_id.0,
				dir,
				bytes,
			));
		}
	}
}

impl RecordingDebugSink {
	fn taken(&self) -> Vec<(String, truapi::FrameDirection, Vec<u8>)> {
		self.frames.lock().expect("recording debug sink mutex poisoned").clone()
	}
}

fn tapped_product(tapped: &Arc<DebugTappedRuntime>, product_id: &str) -> ProductRuntime {
	let product =
		ProductContext::new_with_execution(ProductId::for_test(product_id), ExecutableKind::App);
	tapped.product_runtime(product, Arc::new(DiscardingFrameSink))
}

/// The decorator builds its runtime through the wrapped factory, and the
/// frames that runtime receives reach the sink under the channel id the
/// decorator minted. Asserting only that the wiring call returns leaves the
/// feature itself - that anything arrives at all - uncovered.
///
/// Arbitrary bytes are enough: the inbound tap fires before decode, so this
/// does not depend on a well-formed `ProtocolMessage`.
#[tokio::test]
async fn the_debug_tap_streams_received_frames_to_the_sink() -> Result<()> {
	let sink = Arc::new(RecordingDebugSink::default());
	let tapped = DebugTappedRuntime::new(signing_runtime()?, sink.clone());
	let runtime = tapped_product(&tapped, "localhost:3000");

	let frame = vec![0xde, 0xad, 0xbe, 0xef];
	let _ = runtime.receive_frame(frame.clone()).await;
	runtime.dispose();

	assert_eq!(
		sink.taken(),
		vec![("localhost:3000#0".to_string(), truapi::FrameDirection::In, frame)]
	);
	Ok(())
}

/// Request ids are minted per connection (`p:1`, `p:2`, ...), and §4 keys a
/// trace on `(channelId, requestId)`. Two peers under one host - a browser
/// page alongside the script runner, or a reload overlapping its predecessor -
/// therefore collide unless the channel ids differ.
#[tokio::test]
async fn concurrent_connections_get_their_own_channel_ids() -> Result<()> {
	let sink = Arc::new(RecordingDebugSink::default());
	let tapped = DebugTappedRuntime::new(signing_runtime()?, sink.clone());

	let first = tapped_product(&tapped, "localhost:3000");
	let second = tapped_product(&tapped, "localhost:3000");
	let _ = first.receive_frame(vec![1]).await;
	let _ = second.receive_frame(vec![2]).await;
	first.dispose();
	second.dispose();

	let channels: Vec<String> = sink.taken().into_iter().map(|(id, _, _)| id).collect();
	assert_eq!(channels, vec!["localhost:3000#0", "localhost:3000#1"]);
	Ok(())
}
