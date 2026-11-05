//! Localhost WebSocket bridge. Binds to `127.0.0.1:<port>`, gates each
//! connection on a session token, and relays SCALE-encoded
//! [`ProtocolMessage`](crate::frame::ProtocolMessage) frames into a
//! product-scoped runtime.
//!
//! Executions under one host share a [`SharedWsBridge`] listener and the
//! process-wide `tokio` runtime, with independent tokens and connections.
//!
//! Each upgrade requires an execution's random 256-bit token (`?t=<token>`).
//! Token comparisons scan all candidates to avoid revealing which matched.
//! Tokens go only to the host's embedded WebView; its origin is not known in
//! advance, so the `Origin` header is not pinned.
//!
//! Message size, outbound queues, authenticated connections and pending
//! handshakes are bounded to contain a misbehaving local peer.

use core::{future::ready, ops::ControlFlow};
use std::{
	collections::{HashMap, VecDeque},
	io,
	net::SocketAddr,
	sync::{
		Arc, Mutex,
		atomic::{AtomicUsize, Ordering},
	},
};

use futures::{FutureExt, SinkExt, StreamExt};
use rand::RngCore;
use tokio::{
	net::TcpListener,
	runtime::Handle,
	sync::{mpsc, oneshot},
};
use tokio_tungstenite::{
	WebSocketStream,
	tungstenite::{
		Message as WsMessage,
		handshake::server::{ErrorResponse, Request, Response},
		http::{Response as HttpResponse, StatusCode},
		protocol::WebSocketConfig,
	},
};

use super::executor::shared_native_executor;
use crate::{
	FrameSink, ProductRuntime, markers,
	time::{Backoff, RealTimer, Timer, retry},
};

// Allow reconnect overlap without one execution exhausting the shared limit.
const MAX_WS_CONNECTIONS_PER_EXECUTION: usize = 8;

// Bound resource use even when a peer holds several valid execution tokens.
const MAX_TOTAL_WS_CONNECTIONS: usize = 64;

// Unauthenticated peers do not count against the connection limits.
const MAX_PENDING_HANDSHAKES: usize = 64;

/// Bound on the per-connection outbound frame queue. A peer that stops reading
/// cannot make the core buffer responses without limit; once the queue fills
/// the connection is treated as closed.
const OUTBOUND_QUEUE_CAP: usize = 4096;

/// Ceiling on a single inbound WebSocket message / frame. `ProtocolMessage`
/// frames on this SCALE control channel are small; the cap prevents a
/// memory-amplification DoS well below tungstenite's 64 MiB default.
const MAX_WS_MESSAGE_BYTES: usize = 8 << 20;

// Stalled handshakes must eventually release their sockets.
const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

// A rebind that keeps failing, for example once another socket holds the port,
// must not retry in a tight loop; a successful rebind resets the pause.
const RELISTEN_PAUSES: Backoff =
	Backoff::new(std::time::Duration::from_millis(50), std::time::Duration::from_secs(5));
const ACCEPT_RETRY_PAUSE: std::time::Duration = std::time::Duration::from_millis(50);

/// Per-session descriptor returned to the host: product uses `port + token`
/// to build its WebSocket URL (e.g. `ws://127.0.0.1:<port>/?t=<token>`).
#[derive(Clone, Debug, uniffi::Record)]
pub struct WsBridgeEndpoint {
	/// Localhost port the bridge is listening on.
	pub port: u16,
	/// Session token; the connecting client must supply this as the
	/// `?t=<token>` query parameter to be accepted.
	pub token: String,
}

/// Failure modes returned from host-facing `start_ws_bridge` wrappers.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum WsBridgeStartError {
	/// This execution already has a registered bridge token.
	#[error("ws bridge already running")]
	AlreadyRunning,
	/// Anything else (bind failure, runtime spin-up failure, ...).
	#[error("ws bridge start failed: {0}")]
	Io(String),
}

impl From<io::Error> for WsBridgeStartError {
	fn from(err: io::Error) -> Self {
		if err.kind() == io::ErrorKind::AlreadyExists {
			WsBridgeStartError::AlreadyRunning
		} else {
			WsBridgeStartError::Io(err.to_string())
		}
	}
}

/// Logger callback shape used by the bridge for lifecycle events. The
/// Android and iOS wrappers adapt their per-platform callback interfaces to
/// this platform-neutral shape.
pub type BridgeLogger = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Factory used by the bridge to create one product runtime per WebSocket
/// connection.
pub trait WsProductRuntimeFactory: Send + Sync {
	/// Create a runtime that emits outgoing frames into `sink`.
	fn product_runtime(&self, sink: Arc<dyn FrameSink>) -> ProductRuntime;
}

impl<F> WsProductRuntimeFactory for F
where
	F: Fn(Arc<dyn FrameSink>) -> ProductRuntime + Send + Sync,
{
	fn product_runtime(&self, sink: Arc<dyn FrameSink>) -> ProductRuntime {
		self(sink)
	}
}

struct RegistryEntry {
	runtime_factory: Arc<dyn WsProductRuntimeFactory>,
	logger: BridgeLogger,
	connection_count: Arc<AtomicUsize>,
	connections: Mutex<EntryConnections>,
}

// One lock prevents a completed handshake from registering past revocation.
#[derive(Default)]
struct EntryConnections {
	revoked: bool,
	handles: Vec<tokio::task::JoinHandle<()>>,
}

impl EntryConnections {
	fn abort_and_take(&mut self) -> Vec<tokio::task::JoinHandle<()>> {
		for handle in self.handles.iter() {
			handle.abort();
		}
		std::mem::take(&mut self.handles)
	}
}

#[derive(Default)]
struct WsBridgeRegistry {
	entries: Mutex<HashMap<String, Arc<RegistryEntry>>>,
	total_connections: Arc<AtomicUsize>,
}

impl WsBridgeRegistry {
	fn insert(
		&self,
		token: String,
		runtime_factory: Arc<dyn WsProductRuntimeFactory>,
		logger: BridgeLogger,
	) {
		self.entries.lock().expect("ws bridge registry mutex poisoned").insert(
			token,
			Arc::new(RegistryEntry {
				runtime_factory,
				logger,
				connection_count: Arc::new(AtomicUsize::new(0)),
				connections: Mutex::new(EntryConnections::default()),
			}),
		);
	}

	fn revoke(&self, token: &str) -> Vec<tokio::task::JoinHandle<()>> {
		let Some(entry) =
			self.entries.lock().expect("ws bridge registry mutex poisoned").remove(token)
		else {
			return Vec::new();
		};
		let mut state = entry.connections.lock().expect("ws bridge registry entry mutex poisoned");
		state.revoked = true;
		state.abort_and_take()
	}

	// Scan all candidates so an early match cannot reveal token order.
	fn find_matching(&self, path_and_query: Option<&str>) -> Option<Arc<RegistryEntry>> {
		// Peer-supplied query processing must not hold up registry updates.
		let candidates: Vec<(String, Arc<RegistryEntry>)> = self
			.entries
			.lock()
			.expect("ws bridge registry mutex poisoned")
			.iter()
			.map(|(token, entry)| (token.clone(), entry.clone()))
			.collect();
		let mut found = None;
		for (token, entry) in candidates.iter() {
			if path_token_matches(path_and_query, token) {
				found = Some(entry.clone());
			}
		}
		found
	}

	// Revoked entries are already removed; their caller owns the aborted tasks.
	fn take_all_handles(&self) -> Vec<tokio::task::JoinHandle<()>> {
		let entries = self.entries.lock().expect("ws bridge registry mutex poisoned");
		let mut all = Vec::new();
		for entry in entries.values() {
			let mut state =
				entry.connections.lock().expect("ws bridge registry entry mutex poisoned");
			all.append(&mut state.abort_and_take());
		}
		all
	}
}

/// Lazy listener shared by a host runtime and its product executions.
pub struct SharedWsBridge {
	inner: Mutex<Option<WsBridge>>,
	logger: BridgeLogger,
}

impl SharedWsBridge {
	/// Construct a lazy listener with host-owned lifecycle logging.
	pub fn new(logger: BridgeLogger) -> Self {
		Self { inner: Mutex::new(None), logger }
	}

	/// Register an execution with its own token and connection logger.
	///
	/// The first registration starts the listener on `bind_port`. Later
	/// registrations reuse that port; conflicting nonzero requests are logged.
	pub fn register(
		&self,
		bind_port: u16,
		runtime_factory: Arc<dyn WsProductRuntimeFactory>,
		logger: BridgeLogger,
	) -> Result<WsBridgeEndpoint, WsBridgeStartError> {
		// Log after unlocking because host callbacks can re-enter the bridge.
		let mut pending_logs: Vec<(&'static str, String)> = Vec::new();
		let endpoint = {
			let mut guard = self.inner.lock().expect("shared ws bridge mutex poisoned");
			if guard.is_none() {
				let (bridge, logs) = WsBridge::start(bind_port, self.logger.clone())?;
				*guard = Some(bridge);
				pending_logs = logs;
			} else if bind_port != 0 {
				let running_port = guard.as_ref().expect("just checked Some").port;
				if bind_port != running_port {
					pending_logs.push((
						markers::WS_BRIDGE_BIND_PORT_IGNORED.target,
						format!("requested={bind_port} running={running_port}"),
					));
				}
			}
			guard
				.as_ref()
				.expect("shared bridge just inserted")
				.register(runtime_factory, logger)
		};
		for (event, detail) in &pending_logs {
			(self.logger)(event, detail);
		}
		Ok(endpoint)
	}

	/// Revoke one execution's token. No-op if the listener was never
	/// started or the token is unknown.
	///
	/// Off the shared executor, waits for connection tasks to release their
	/// sockets and capacity. On that executor, cancellation is requested
	/// without waiting to avoid deadlocking a worker.
	pub fn revoke(&self, token: &str) {
		let aborted = {
			let guard = self.inner.lock().expect("shared ws bridge mutex poisoned");
			match guard.as_ref() {
				Some(bridge) => bridge.revoke(token),
				None => return,
			}
		};
		join_aborted_connections(aborted);
	}

	/// Rebind the listener on its port, keeping every token and live
	/// connection. No-op if the listener was never started.
	///
	/// iOS reclaims a suspended app's listening sockets while its products
	/// keep dialing the port they were given, so hosts call this on each
	/// return to the foreground. A failed rebind is logged and left for the
	/// next call to retry.
	pub fn relisten(&self) {
		let relistened = {
			let mut guard = self.inner.lock().expect("shared ws bridge mutex poisoned");
			match guard.as_mut() {
				Some(bridge) => bridge.relisten().map(|()| bridge.port),
				None => return,
			}
		};
		match relistened {
			Ok(port) => {
				(self.logger)(markers::WS_BRIDGE_RELISTENED.target, &format!("port={port}"))
			},
			Err(err) => (self.logger)(markers::WS_BRIDGE_RELISTEN_FAILED.target, &err.to_string()),
		}
	}
}

/// Running listener owned by [`SharedWsBridge`]. Dropping it stops acceptance
/// and cancels its connections without stopping the shared executor.
struct WsBridge {
	// Absent after a failed relisten.
	listener: Option<Listener>,
	runtime_id: tokio::runtime::Id,
	registry: Arc<WsBridgeRegistry>,
	logger: BridgeLogger,
	port: u16,
}

// Bind synchronously so callers surface bind errors and learn the actual port.
fn bind_loopback(port: u16) -> io::Result<std::net::TcpListener> {
	let socket = std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))?;
	socket.set_nonblocking(true)?;
	Ok(socket)
}

// One bound socket's accept loop. Stopping it leaves accepted connections running.
struct Listener {
	shutdown: oneshot::Sender<()>,
	stopped: std::sync::mpsc::Receiver<()>,
	accept_task: tokio::task::JoinHandle<()>,
}

impl Listener {
	fn serve(
		socket: std::net::TcpListener,
		handle: &Handle,
		registry: Arc<WsBridgeRegistry>,
		logger: BridgeLogger,
	) -> io::Result<Self> {
		let port = socket.local_addr()?.port();
		// Register with the shared runtime's I/O driver before returning so a
		// successful start always yields a ready endpoint.
		let listener = {
			let _entered = handle.enter();
			TcpListener::from_std(socket)?
		};
		let (shutdown, shutdown_rx) = oneshot::channel::<()>();
		let (stopped_tx, stopped) = std::sync::mpsc::channel::<()>();
		let accept_task = handle.spawn(async move {
			accept_loop(listener, port, registry, logger, shutdown_rx).await;
			let _ = stopped_tx.send(());
		});
		Ok(Self { shutdown, stopped, accept_task })
	}

	// Waiting releases the socket before returning. The accept loop needs a
	// worker to observe shutdown, so callers on the shared executor must not wait.
	fn stop(self, wait: bool) {
		let _ = self.shutdown.send(());
		if wait && self.stopped.recv().is_err() && !self.accept_task.is_finished() {
			self.accept_task.abort();
		}
	}
}

impl WsBridge {
	// Return startup logs so the caller can emit them outside its lock.
	fn start(
		bind_port: u16,
		logger: BridgeLogger,
	) -> io::Result<(Self, Vec<(&'static str, String)>)> {
		let socket = bind_loopback(bind_port)?;
		let port = socket.local_addr()?.port();

		let executor = shared_native_executor()?;
		let handle = executor.handle();
		let runtime_id = handle.id();
		let registry = Arc::new(WsBridgeRegistry::default());
		let listener = Listener::serve(socket, &handle, registry.clone(), logger.clone())?;

		let pending_logs = vec![(
			markers::WS_BRIDGE_STARTED.target,
			format!("port={port} runtime_id={runtime_id}"),
		)];

		Ok((Self { listener: Some(listener), runtime_id, registry, logger, port }, pending_logs))
	}

	// The old socket must close before its port can be bound again.
	fn relisten(&mut self) -> io::Result<()> {
		if let Some(listener) = self.listener.take() {
			listener.stop(!self.on_shared_executor());
		}
		let socket = bind_loopback(self.port)?;
		let executor = shared_native_executor()?;
		self.listener = Some(Listener::serve(
			socket,
			&executor.handle(),
			self.registry.clone(),
			self.logger.clone(),
		)?);
		Ok(())
	}

	fn on_shared_executor(&self) -> bool {
		Handle::try_current().is_ok_and(|current| current.id() == self.runtime_id)
	}

	fn register(
		&self,
		runtime_factory: Arc<dyn WsProductRuntimeFactory>,
		logger: BridgeLogger,
	) -> WsBridgeEndpoint {
		let mut token_bytes = [0u8; 32];
		rand::thread_rng().fill_bytes(&mut token_bytes);
		let token = hex::encode(token_bytes);
		self.registry.insert(token.clone(), runtime_factory, logger);
		WsBridgeEndpoint { port: self.port, token }
	}

	// Joining from this executor could block the worker needed for cancellation.
	fn revoke(&self, token: &str) -> Vec<tokio::task::JoinHandle<()>> {
		let connections = self.registry.revoke(token);
		if self.on_shared_executor() {
			return Vec::new();
		}
		connections
	}

	// Off the shared executor, wait for tracked connection tasks to release
	// their sockets. Dispatch tasks are cancelled but not joined.
	fn stop(&mut self) {
		let wait = !self.on_shared_executor();
		if let Some(listener) = self.listener.take() {
			listener.stop(wait);
		}
		let connections = self.registry.take_all_handles();
		if wait {
			join_aborted_connections(connections);
		}
	}
}

impl Drop for WsBridge {
	fn drop(&mut self) {
		self.stop();
	}
}

// Synchronous host callers need not have an executor. This wait relies on
// connection tasks yielding so cancellation can complete.
fn join_aborted_connections(handles: Vec<tokio::task::JoinHandle<()>>) {
	if handles.is_empty() {
		return;
	}
	let Ok(executor) = shared_native_executor() else {
		return;
	};
	let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
	executor.handle().spawn(async move {
		for handle in handles {
			let _ = handle.await;
		}
		let _ = done_tx.send(());
	});
	let _ = done_rx.recv();
}

// Only these mean the listening socket itself is gone, as when Android destroys
// it (EINVAL). Any other error fails one pending connection or is a shortage
// that passes, so the listener keeps its port.
fn is_listener_gone(err: &io::Error) -> bool {
	matches!(err.raw_os_error(), Some(libc::EINVAL | libc::EBADF | libc::ENOTSOCK))
}

async fn accept_loop(
	listener: TcpListener,
	port: u16,
	registry: Arc<WsBridgeRegistry>,
	logger: BridgeLogger,
	mut shutdown: oneshot::Receiver<()>,
) {
	// Independent setup tasks keep a stalled handshake from blocking acceptance.
	let mut setup_tasks: VecDeque<tokio::task::JoinHandle<()>> = VecDeque::new();
	// Absent while a lost listener waits to be bound again on the same port.
	let mut listener = Some(listener);
	loop {
		tokio::select! {
			// Shutdown first, so a stopping loop never binds its port again.
			biased;
			_ = &mut shutdown => {
				logger(markers::WS_BRIDGE_SHUTDOWN.target, "accept loop exiting");
				for task in &setup_tasks {
					task.abort();
				}
				for task in setup_tasks {
					let _ = task.await;
				}
				break;
			}
			accepted = async {
				match &listener {
					Some(listener) => listener.accept().await,
					None => core::future::pending().await,
				}
			} => {
				let (stream, peer) = match accepted {
					Ok(pair) => pair,
					Err(err) if is_listener_gone(&err) => {
						// Dropping the socket frees its port for the rebind.
						logger(markers::WS_BRIDGE_LISTENER_LOST.target, &err.to_string());
						listener = None;
						continue;
					}
					Err(err) => {
						logger(markers::WS_BRIDGE_ACCEPT_ERROR.target, &err.to_string());
						// A shortage such as EMFILE fails every accept until it passes.
						tokio::time::sleep(ACCEPT_RETRY_PAUSE).await;
						continue;
					}
				};
				setup_tasks.retain(|task| !task.is_finished());
				// Evict the oldest pending handshake so stalled peers cannot
				// reserve the entire backlog until their timeouts expire.
				if setup_tasks.len() >= MAX_PENDING_HANDSHAKES
					&& let Some(oldest) = setup_tasks.pop_front()
				{
					oldest.abort();
					logger(markers::WS_BRIDGE_HANDSHAKE_BACKLOG_EVICTED.target, &peer.to_string());
				}
				let registry = registry.clone();
				let logger = logger.clone();
				setup_tasks.push_back(tokio::spawn(async move {
					connection_setup(stream, peer, registry, logger).await;
				}));
			}
			rebound = rebind(port, &logger), if listener.is_none() => {
				logger(markers::WS_BRIDGE_RELISTENED.target, &format!("port={port}"));
				listener = Some(rebound);
			}
		}
	}
}

/// Bind `port` again for a lost listener, backing off between failed binds.
async fn rebind(port: u16, logger: &BridgeLogger) -> TcpListener {
	let mut pauses = RELISTEN_PAUSES;
	// Pause before binding too, so a listener lost right after each bind can't spin.
	RealTimer.sleep(pauses.next_pause()).await;
	retry(
		|| {
			ready(match bind_loopback(port).and_then(TcpListener::from_std) {
				Ok(listener) => ControlFlow::Break(listener),
				Err(err) => ControlFlow::Continue(err),
			})
		},
		|err| {
			logger(markers::WS_BRIDGE_RELISTEN_FAILED.target, &err.to_string());
			pauses.next_pause()
		},
	)
	.await
}

async fn connection_setup(
	stream: tokio::net::TcpStream,
	peer: SocketAddr,
	registry: Arc<WsBridgeRegistry>,
	logger: BridgeLogger,
) {
	let auth_result = tokio::time::timeout(
		HANDSHAKE_TIMEOUT,
		authenticate_and_upgrade(stream, peer, &registry, logger.clone()),
	)
	.await;
	let Some((ws, entry, guard)) = (match auth_result {
		Ok(resolved) => resolved,
		Err(_) => {
			logger(markers::WS_BRIDGE_HANDSHAKE_TIMEOUT.target, &peer.to_string());
			return;
		},
	}) else {
		return;
	};
	let logger = entry.logger.clone();
	let revoked = {
		let mut state = entry.connections.lock().expect("ws bridge registry entry mutex poisoned");
		state.handles.retain(|h| !h.is_finished());
		if state.revoked {
			true
		} else {
			let conn_entry = entry.clone();
			state.handles.push(tokio::spawn(async move {
				connection_lifecycle(ws, peer, conn_entry, guard).await;
			}));
			false
		}
	};
	// Host callbacks may re-enter revocation, so logging must stay outside the lock.
	if revoked {
		logger(markers::WS_BRIDGE_CONNECTION_REVOKED_DURING_SETUP.target, &peer.to_string());
	}
}

struct ConnectionCountGuard {
	total: Arc<AtomicUsize>,
	per_entry: Arc<AtomicUsize>,
}

impl Drop for ConnectionCountGuard {
	fn drop(&mut self) {
		self.total.fetch_sub(1, Ordering::AcqRel);
		self.per_entry.fetch_sub(1, Ordering::AcqRel);
	}
}

// Task cancellation skips explicit cleanup after an await, but still drops guards.
struct DisposeGuard(Arc<ProductRuntime>);

impl Drop for DisposeGuard {
	fn drop(&mut self) {
		self.0.dispose();
	}
}

type AuthenticatedConnection =
	(WebSocketStream<tokio::net::TcpStream>, Arc<RegistryEntry>, ConnectionCountGuard);

type MatchedReservation = Arc<Mutex<Option<(Arc<RegistryEntry>, ConnectionCountGuard)>>>;

// Concurrent handshakes must not both claim the last available slot.
fn try_reserve(counter: &AtomicUsize, limit: usize) -> bool {
	// `fetch_update` is deprecated on nightly; `try_update` is not yet stable.
	let mut current = counter.load(Ordering::Acquire);
	loop {
		if current >= limit {
			return false;
		}
		match counter.compare_exchange_weak(
			current,
			current + 1,
			Ordering::AcqRel,
			Ordering::Acquire,
		) {
			Ok(_) => return true,
			Err(actual) => current = actual,
		}
	}
}

// The handshake callback's error type is fixed by tokio-tungstenite.
#[allow(clippy::result_large_err)]
async fn authenticate_and_upgrade(
	stream: tokio::net::TcpStream,
	peer: SocketAddr,
	registry: &Arc<WsBridgeRegistry>,
	logger: BridgeLogger,
) -> Option<AuthenticatedConnection> {
	let matched: MatchedReservation = Arc::new(Mutex::new(None));
	let auth_registry = registry.clone();
	let auth_matched = matched.clone();
	let auth_logger = logger.clone();
	let callback = move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
		let path_and_query = req.uri().path_and_query().map(|p| p.as_str());
		let Some(entry) = auth_registry.find_matching(path_and_query) else {
			auth_logger(markers::WS_BRIDGE_REJECT_UNAUTHORIZED.target, &peer.to_string());
			let mut err: ErrorResponse = HttpResponse::new(Some("invalid token".to_string()));
			*err.status_mut() = StatusCode::UNAUTHORIZED;
			return Err(err);
		};

		// Reject over-cap connections before acknowledging the HTTP upgrade.
		if !try_reserve(&entry.connection_count, MAX_WS_CONNECTIONS_PER_EXECUTION) {
			(entry.logger)(markers::WS_BRIDGE_CONNECTION_LIMIT_EXECUTION.target, &peer.to_string());
			let mut err: ErrorResponse =
				HttpResponse::new(Some("execution connection limit reached".to_string()));
			*err.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
			return Err(err);
		}
		if !try_reserve(&auth_registry.total_connections, MAX_TOTAL_WS_CONNECTIONS) {
			entry.connection_count.fetch_sub(1, Ordering::AcqRel);
			(entry.logger)(markers::WS_BRIDGE_CONNECTION_LIMIT_TOTAL.target, &peer.to_string());
			let mut err: ErrorResponse =
				HttpResponse::new(Some("listener at capacity".to_string()));
			*err.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
			return Err(err);
		}
		// Keep the reservation guarded even if the upgrade fails or times out.
		let guard = ConnectionCountGuard {
			total: auth_registry.total_connections.clone(),
			per_entry: entry.connection_count.clone(),
		};

		*auth_matched.lock().expect("ws bridge handshake mutex poisoned") = Some((entry, guard));
		Ok(resp)
	};

	// Cap inbound message/frame size so a peer cannot force the runtime to
	// buffer up to tungstenite's 64 MiB default on this small control channel.
	let config = WebSocketConfig {
		max_message_size: Some(MAX_WS_MESSAGE_BYTES),
		max_frame_size: Some(MAX_WS_MESSAGE_BYTES),
		..Default::default()
	};
	let ws = match tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(config))
		.await
	{
		Ok(ws) => ws,
		Err(err) => {
			logger(markers::WS_BRIDGE_HANDSHAKE_ERROR.target, &err.to_string());
			return None;
		},
	};

	let (entry, guard) = matched
		.lock()
		.expect("ws bridge handshake mutex poisoned")
		.take()
		.expect("a successful upgrade always resolved a matching registry entry");
	let open = entry.connection_count.load(Ordering::Acquire);
	(entry.logger)(
		markers::WS_BRIDGE_CONNECTION_OPEN.target,
		&format!(
			"{peer} connected; this execution now holds {open} of {MAX_WS_CONNECTIONS_PER_EXECUTION} connections"
		),
	);
	Some((ws, entry, guard))
}

/// Serve one product connection until its socket ends.
///
/// The connection's slot in the execution's count is held here so the closing
/// marker reports what the execution still holds once this one is released.
async fn connection_lifecycle(
	ws: WebSocketStream<tokio::net::TcpStream>,
	peer: SocketAddr,
	entry: Arc<RegistryEntry>,
	counted: ConnectionCountGuard,
) {
	let logger = &entry.logger;
	let (mut sink, mut source) = ws.split();
	let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(OUTBOUND_QUEUE_CAP);
	let frame_sink = Arc::new(WsFrameSink::new(out_tx));
	let product_runtime = Arc::new(entry.runtime_factory.product_runtime(frame_sink));
	let dispose_guard = DisposeGuard(product_runtime.clone());

	// Dispatch each inbound frame on its own `Send` task so a slow request
	// handler cannot stall the read loop and independent frames can run on
	// different executor workers. Responses may interleave; the wire protocol
	// matches them by request id, and `WsFrameSink::emit_frame` is thread-safe.
	let mut in_flight = tokio::task::JoinSet::new();
	{
		let writer = async {
			while let Some(bytes) = out_rx.recv().await {
				if let Err(err) = sink.send(WsMessage::Binary(bytes)).await {
					logger(markers::WS_BRIDGE_SEND_ERROR.target, &err.to_string());
					break;
				}
			}
		};
		tokio::pin!(writer);
		loop {
			let frame = tokio::select! {
				_ = &mut writer => break,
				frame = source.next() => frame,
			};
			match frame {
				Some(Ok(WsMessage::Binary(bytes))) => {
					while in_flight.try_join_next().is_some() {}
					let product_runtime = product_runtime.clone();
					let frame_logger = logger.clone();
					in_flight.spawn(async move {
						if let Err(err) = product_runtime.receive_frame(bytes.to_vec()).await {
							frame_logger(markers::WS_BRIDGE_FRAME_ERROR.target, &err.to_string());
						}
					});
				},
				Some(Ok(WsMessage::Text(_))) => {
					logger(markers::WS_BRIDGE_TEXT_FRAME_IGNORED.target, "");
				},
				None | Some(Ok(WsMessage::Close(_))) => break,
				Some(Ok(_)) => {},
				Some(Err(err)) => {
					logger(markers::WS_BRIDGE_READ_ERROR.target, &err.to_string());
					break;
				},
			}
		}
	}

	// A slow peer must not retain capacity while its close reply waits.
	let _ = sink.close().now_or_never();
	drop(in_flight);
	drop(dispose_guard);
	drop(counted);
	let open = entry.connection_count.load(Ordering::Acquire);
	logger(
		markers::WS_BRIDGE_CONNECTION_CLOSED.target,
		&format!(
			"{peer} disconnected; this execution now holds {open} of {MAX_WS_CONNECTIONS_PER_EXECUTION} connections"
		),
	);
}

// Scan duplicate `t=` parameters too, so their order cannot expose an early match.
fn path_token_matches(path_and_query: Option<&str>, expected: &str) -> bool {
	let Some(raw) = path_and_query else {
		return false;
	};
	let query = match raw.find('?') {
		Some(idx) => &raw[idx + 1..],
		None => return false,
	};
	let mut matched = false;
	for pair in query.split('&') {
		let (key, value) = match pair.split_once('=') {
			Some(kv) => kv,
			None => continue,
		};
		if key == "t" && constant_time_eq(value.as_bytes(), expected.as_bytes()) {
			matched = true;
		}
	}
	matched
}

/// Constant-time byte-slice equality, used for the session-token check so a
/// local peer cannot recover the token via early-exit comparison timing. The
/// token length is fixed and public, so a length mismatch may short-circuit;
/// only the value comparison must be constant time.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
	if a.len() != b.len() {
		return false;
	}
	let mut diff = 0u8;
	for (x, y) in a.iter().zip(b.iter()) {
		diff |= x ^ y;
	}
	diff == 0
}

struct WsFrameSink {
	outbound: mpsc::Sender<Vec<u8>>,
	closed: Mutex<bool>,
}

impl WsFrameSink {
	fn new(outbound: mpsc::Sender<Vec<u8>>) -> Self {
		Self { outbound, closed: Mutex::new(false) }
	}
}

impl FrameSink for WsFrameSink {
	fn emit_frame(&self, frame: Vec<u8>) {
		if *self.closed.lock().unwrap() {
			return;
		}
		// Non-blocking: a full queue means the peer stopped reading, so the
		// connection is treated as closed rather than buffering without bound.
		if self.outbound.try_send(frame).is_err() {
			*self.closed.lock().unwrap() = true;
		}
	}
}

#[cfg(test)]
mod tests;
