//! Product-frame WebSocket bridge for the pairing host.
//!
//! Each WebSocket connection is one product: inbound binary frames are pushed
//! into a [`ProductRuntime`] and its outgoing frames are written back as
//! binary messages. One binary WS message carries exactly one SCALE
//! `ProtocolMessage`, matching the browser transport's framing.

use core::sync::atomic::{AtomicU64, Ordering};
use std::{
	net::{IpAddr, SocketAddr},
	path::Path,
	pin::Pin,
	sync::{Arc, RwLock},
	task::{Context as TaskContext, Poll},
	time::Duration,
};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::{
	io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
	net::{TcpListener, TcpStream},
	sync::{mpsc, watch},
};
#[cfg(test)]
use tokio_tungstenite::accept_async;
use tokio_tungstenite::{
	accept_hdr_async,
	tungstenite::{
		Message,
		handshake::server::{ErrorResponse, Request, Response},
		http::{StatusCode, header},
	},
};
use tracing::{debug, warn};

use crate::bootstrap;
use truapi::{
	ChannelId, DebugSink, FrameSink, PairingHostRuntime, ProductContext, ProductRuntime,
	ProductRuntimeError, SigningHostRuntime,
	platform::{ExecutableKind, ProductId},
};

/// Pause after a failed `accept()` before trying again.
///
/// A failed accept leaves the peer in the listener's queue, so the listener stays
/// readable and an immediate retry fails the same way. Without a pause that is a
/// full-CPU loop emitting one warning per iteration. The errors that reach here
/// are either process-wide (`EMFILE`/`ENFILE`) or per-connection and transient
/// (`ECONNABORTED`); neither clears faster for being retried in a tight loop.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(50);

/// Cap on the request head buffered while deciding whether a TCP connection is
/// a WebSocket handshake or a plain HTTP request for the bridge script.
const MAX_REQUEST_HEAD: usize = 8 * 1024;

/// Blank line ending an HTTP request head.
const HEAD_TERMINATOR: &[u8] = b"\r\n\r\n";

/// How long a TCP peer has to finish sending its request head before the
/// connection is dropped, so a peer that connects and says nothing cannot pin a
/// task forever.
const REQUEST_HEAD_TIMEOUT: Duration = Duration::from_secs(10);

/// Process-local product selection shared by the command loop and frame server.
pub struct ProductSelection {
	current: watch::Sender<ProductContext>,
	/// Execution kind every selection keeps. A host serves one kind for its
	/// lifetime: the core reads it per connection, and chat is denied to a
	/// connection that opened as `App`.
	execution_kind: ExecutableKind,
}

impl ProductSelection {
	/// Select the initial product identity.
	pub fn new(product_id: ProductId, execution_kind: ExecutableKind) -> Arc<Self> {
		let product = ProductContext::new_with_execution(product_id, execution_kind);
		let (current, _) = watch::channel(product);
		Arc::new(Self { current, execution_kind })
	}

	/// The current product id.
	pub fn current(&self) -> ProductId {
		self.current.borrow().product_id.clone()
	}

	/// Select a product, returning whether the selection changed.
	pub fn select(&self, product_id: ProductId) -> bool {
		let product = ProductContext::new_with_execution(product_id, self.execution_kind);
		self.current.send_if_modified(|current| {
			if current == &product {
				false
			} else {
				*current = product;
				true
			}
		})
	}

	fn subscribe(&self) -> watch::Receiver<ProductContext> {
		self.current.subscribe()
	}
}

pub trait ProductRuntimeFactory: Send + Sync + 'static {
	fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime;

	/// Subscribe to a signal that invalidates existing product connections.
	fn connection_reset(&self) -> Option<watch::Receiver<u64>> {
		None
	}
}

// async-trait's generated boxed Future is already must-use; newer Clippy
// otherwise reports the macro's duplicate annotation on this private trait.
#[allow(clippy::double_must_use)]
#[async_trait::async_trait]
trait ConnectionRuntime: Send + Sync + 'static {
	async fn receive_frame(&self, frame: Vec<u8>) -> Result<(), ProductRuntimeError>;
	fn dispose(&self);
}

#[async_trait::async_trait]
impl ConnectionRuntime for ProductRuntime {
	async fn receive_frame(&self, frame: Vec<u8>) -> Result<(), ProductRuntimeError> {
		ProductRuntime::receive_frame(self, frame).await
	}

	fn dispose(&self) {
		ProductRuntime::dispose(self);
	}
}

impl ProductRuntimeFactory for PairingHostRuntime {
	fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime {
		PairingHostRuntime::product_runtime(self, product, sink)
	}
}

impl ProductRuntimeFactory for SigningHostRuntime {
	fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime {
		SigningHostRuntime::product_runtime(self, product, sink)
	}
}

/// A [`ProductRuntimeFactory`] that installs `sink` on every product runtime the
/// wrapped factory hands out.
///
/// Wrapping the factory rather than each host role keeps the tap in one place:
/// pairing, signing, and the switchable signing runtime all reach the debugger
/// through the same decorator, and none of them knows it is being observed.
pub struct DebugTappedRuntime {
	inner: Arc<dyn ProductRuntimeFactory>,
	sink: Arc<dyn DebugSink>,
	connections: AtomicU64,
}

impl DebugTappedRuntime {
	/// Wrap `inner` so the runtimes it builds report their frames to `sink`.
	pub fn new(inner: Arc<dyn ProductRuntimeFactory>, sink: Arc<dyn DebugSink>) -> Arc<Self> {
		Arc::new(Self { inner, sink, connections: AtomicU64::new(0) })
	}
}

impl ProductRuntimeFactory for DebugTappedRuntime {
	fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime {
		// One runtime is built per accepted socket, and request ids are minted per
		// connection (`p:1`, `p:2`, ...). The product id alone therefore repeats
		// across concurrent peers - a browser page alongside the bundled script
		// runner, or a reload whose new socket overlaps the old - and §4 keys
		// traces on `(channelId, requestId)`, so both id spaces would land on one
		// channel and interleave. The ordinal separates them; the debugger treats
		// a channel id as opaque, so it only has to be distinct and readable.
		let ordinal = self.connections.fetch_add(1, Ordering::Relaxed);
		let channel_id = format!("{}#{ordinal}", product.product_id);
		let runtime = self.inner.product_runtime(product, sink);
		runtime.set_debug_sink(ChannelId(channel_id), Arc::clone(&self.sink));
		runtime
	}

	/// Delegated to the wrapped factory. A decorator that answered the default
	/// `None` here would silently keep product connections alive across a
	/// session switch that is meant to invalidate them.
	fn connection_reset(&self) -> Option<watch::Receiver<u64>> {
		self.inner.connection_reset()
	}
}

/// Signing runtime factory whose active session can be replaced without
/// restarting the frame listener.
pub struct SwitchableSigningRuntime {
	current: RwLock<Arc<SigningHostRuntime>>,
	generation: watch::Sender<u64>,
}

impl SwitchableSigningRuntime {
	pub fn new(runtime: Arc<SigningHostRuntime>) -> Arc<Self> {
		let (generation, _) = watch::channel(0);
		Arc::new(Self { current: RwLock::new(runtime), generation })
	}

	/// Replace the runtime and disconnect every product using the old one.
	pub fn replace(&self, runtime: Arc<SigningHostRuntime>) {
		*self.current.write().expect("runtime lock poisoned") = runtime;
		self.reset_connections();
	}

	/// Disconnect every product currently using the active signing runtime.
	pub fn reset_connections(&self) {
		self.generation
			.send_modify(|generation| *generation = generation.wrapping_add(1));
	}
}

impl ProductRuntimeFactory for SwitchableSigningRuntime {
	fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime {
		self.current
			.read()
			.expect("runtime lock poisoned")
			.product_runtime(product, sink)
	}

	fn connection_reset(&self) -> Option<watch::Receiver<u64>> {
		Some(self.generation.subscribe())
	}
}

/// Frame sink that writes each outgoing protocol frame as one binary message.
struct WsFrameSink {
	outbound: mpsc::UnboundedSender<Message>,
}

impl FrameSink for WsFrameSink {
	fn emit_frame(&self, frame: Vec<u8>) {
		let _ = self.outbound.send(Message::Binary(frame));
	}
}

/// Bound product-frame endpoint. The temporary-directory guard keeps a Unix
/// socket alive for exactly as long as its listener.
pub struct BoundFrameServer {
	listener: FrameListener,
	endpoint: String,
	#[cfg(unix)]
	socket_directory: Option<tempfile::TempDir>,
}

enum FrameListener {
	Tcp(TcpListener),
	#[cfg(unix)]
	Unix(UnixListener),
}

impl BoundFrameServer {
	/// Endpoint passed to the bundled product runner and shown in the CLI.
	pub fn endpoint(&self) -> &str {
		&self.endpoint
	}
}

/// Bind an explicit TCP WebSocket listener, or a private per-process Unix
/// socket when no TCP address was requested.
pub async fn bind(addr: Option<SocketAddr>) -> Result<BoundFrameServer> {
	if let Some(addr) = addr {
		let listener = TcpListener::bind(addr)
			.await
			.with_context(|| format!("frame server failed to bind {addr}"))?;
		let endpoint = format!("ws://{}", listener.local_addr()?);
		return Ok(BoundFrameServer {
			listener: FrameListener::Tcp(listener),
			endpoint,
			#[cfg(unix)]
			socket_directory: None,
		});
	}
	bind_unix()
}

#[cfg(unix)]
fn bind_unix() -> Result<BoundFrameServer> {
	let socket_directory = tempfile::Builder::new()
		.prefix("truapi-host-")
		.tempdir()
		.context("create temporary product-frame socket directory")?;
	let socket_path = socket_directory.path().join("frames.sock");
	let socket_path_text = socket_path
		.to_str()
		.context("product-frame Unix socket path is not valid UTF-8")?;
	let listener = UnixListener::bind(&socket_path)
		.with_context(|| format!("frame server failed to bind {}", socket_path.display()))?;
	Ok(BoundFrameServer {
		listener: FrameListener::Unix(listener),
		endpoint: format!("ws+unix:{socket_path_text}"),
		socket_directory: Some(socket_directory),
	})
}

#[cfg(not(unix))]
fn bind_unix() -> Result<BoundFrameServer> {
	anyhow::bail!(
		"Unix-domain product sockets are unavailable on this platform; pass --frame-listen <address>"
	)
}

/// Accept product-frame connections on `listener` for `product_id` until
/// cancelled.
///
/// Each connection is driven independently on the Tokio worker pool. The
/// shared dispatcher contract requires `Send` futures, while the WASM adapter
/// may still poll those futures on its single-threaded local executor.
pub async fn accept_loop(
	runtime: Arc<dyn ProductRuntimeFactory>,
	product: Arc<ProductSelection>,
	frame_server: BoundFrameServer,
) -> Result<()> {
	let product_id = product.current();
	let endpoint = frame_server.endpoint.clone();
	debug!(%endpoint, %product_id, "product frame server listening");
	#[cfg(unix)]
	let _socket_directory = frame_server.socket_directory;
	match frame_server.listener {
		FrameListener::Tcp(listener) => accept_tcp_loop(runtime, product, listener, endpoint).await,
		#[cfg(unix)]
		FrameListener::Unix(listener) => accept_unix_loop(runtime, product, listener).await,
	}
}

async fn accept_tcp_loop(
	runtime: Arc<dyn ProductRuntimeFactory>,
	product: Arc<ProductSelection>,
	listener: TcpListener,
	endpoint: String,
) -> Result<()> {
	let container = bootstrap::container_path();
	loop {
		let (stream, peer) = match listener.accept().await {
			Ok(accepted) => accepted,
			Err(err) => {
				warn!(
					%err,
					retry_in = ?ACCEPT_RETRY_DELAY,
					"product frame accept failed"
				);
				tokio::time::sleep(ACCEPT_RETRY_DELAY).await;
				continue;
			},
		};
		let runtime = runtime.clone();
		let product = product.clone();
		let endpoint = endpoint.clone();
		let container = container.clone();
		tokio::spawn(async move {
			if let Err(err) =
				serve_tcp_connection(runtime, product, stream, &endpoint, &container).await
			{
				debug!(%peer, %err, "frame connection ended");
			}
		});
	}
}

/// Serve one TCP peer, which is either a product opening the frame socket or a
/// browser fetching the bridge script.
async fn serve_tcp_connection(
	runtime: Arc<dyn ProductRuntimeFactory>,
	product: Arc<ProductSelection>,
	mut stream: TcpStream,
	endpoint: &str,
	container: &Path,
) -> Result<()> {
	let peer = ConnectionPeer::Tcp(stream.peer_addr().context("read TCP peer address")?);
	let request = tokio::time::timeout(REQUEST_HEAD_TIMEOUT, read_request_head(&mut stream))
		.await
		.context("timed out reading the request head")??;
	if is_websocket_upgrade(request.head()) {
		return serve_connection(runtime, product, request.replay(stream), peer).await;
	}
	serve_bridge_script(&mut stream, request.head(), endpoint, container).await
}

struct BufferedRequestHead {
	consumed: Vec<u8>,
	head_end: usize,
}

impl BufferedRequestHead {
	fn head(&self) -> &[u8] {
		&self.consumed[..self.head_end]
	}

	fn replay<S>(self, stream: S) -> ReplayStream<S> {
		ReplayStream { prefix: self.consumed, prefix_position: 0, stream }
	}
}

async fn read_request_head<S>(stream: &mut S) -> Result<BufferedRequestHead>
where
	S: AsyncRead + Unpin,
{
	let mut consumed = Vec::with_capacity(1024);
	let mut buffer = [0u8; 1024];
	loop {
		let remaining = MAX_REQUEST_HEAD - consumed.len();
		if remaining == 0 {
			anyhow::bail!("request head exceeded {MAX_REQUEST_HEAD} bytes");
		}
		let read_limit = remaining.min(buffer.len());
		let read = stream.read(&mut buffer[..read_limit]).await?;
		if read == 0 {
			anyhow::bail!("peer closed before sending a request");
		}
		consumed.extend_from_slice(&buffer[..read]);
		if let Some(head_end) = find_head_end(&consumed) {
			return Ok(BufferedRequestHead {
				consumed,
				head_end: head_end + HEAD_TERMINATOR.len(),
			});
		}
	}
}

struct ReplayStream<S> {
	prefix: Vec<u8>,
	prefix_position: usize,
	stream: S,
}

impl<S> AsyncRead for ReplayStream<S>
where
	S: AsyncRead + Unpin,
{
	fn poll_read(
		self: Pin<&mut Self>,
		context: &mut TaskContext<'_>,
		buffer: &mut ReadBuf<'_>,
	) -> Poll<std::io::Result<()>> {
		let stream = self.get_mut();
		if stream.prefix_position < stream.prefix.len() {
			let available = &stream.prefix[stream.prefix_position..];
			let read = available.len().min(buffer.remaining());
			buffer.put_slice(&available[..read]);
			stream.prefix_position += read;
			return Poll::Ready(Ok(()));
		}
		Pin::new(&mut stream.stream).poll_read(context, buffer)
	}
}

impl<S> AsyncWrite for ReplayStream<S>
where
	S: AsyncWrite + Unpin,
{
	fn poll_write(
		self: Pin<&mut Self>,
		context: &mut TaskContext<'_>,
		buffer: &[u8],
	) -> Poll<std::io::Result<usize>> {
		Pin::new(&mut self.get_mut().stream).poll_write(context, buffer)
	}

	fn poll_flush(
		self: Pin<&mut Self>,
		context: &mut TaskContext<'_>,
	) -> Poll<std::io::Result<()>> {
		Pin::new(&mut self.get_mut().stream).poll_flush(context)
	}

	fn poll_shutdown(
		self: Pin<&mut Self>,
		context: &mut TaskContext<'_>,
	) -> Poll<std::io::Result<()>> {
		Pin::new(&mut self.get_mut().stream).poll_shutdown(context)
	}
}

fn find_head_end(head: &[u8]) -> Option<usize> {
	head.windows(HEAD_TERMINATOR.len()).position(|window| window == HEAD_TERMINATOR)
}

fn is_websocket_upgrade(head: &[u8]) -> bool {
	header_value(head, "upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

/// First value of `name` in a raw request head, if the head is valid UTF-8.
fn header_value<'a>(head: &'a [u8], name: &str) -> Option<&'a str> {
	let head = std::str::from_utf8(head).ok()?;
	head.lines().skip(1).find_map(|line| {
		let (key, value) = line.split_once(':')?;
		key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
	})
}

/// Answer a plain HTTP request with the bridge script, or a 404.
///
/// Products load this from a development-only `<script>` tag, which is a
/// cross-origin request no CORS header can gate, so the script carries no
/// secret. What keeps another page from using the endpoint it names is the
/// origin check on the WebSocket handshake.
async fn serve_bridge_script(
	stream: &mut TcpStream,
	head: &[u8],
	endpoint: &str,
	container: &Path,
) -> Result<()> {
	let response = match request_path(head).as_deref() {
		Some(bootstrap::PATH) => match bootstrap::read_container(container) {
			Ok(container) => http_response(
				"200 OK",
				"application/javascript; charset=utf-8",
				&bootstrap::script(endpoint, &container),
			),
			Err(error) => http_response(
				"500 Internal Server Error",
				"text/plain; charset=utf-8",
				&format!("{error:#}\n"),
			),
		},
		_ => http_response(
			"404 Not Found",
			"text/plain; charset=utf-8",
			&format!("not found; the bridge script is at {}\n", bootstrap::PATH),
		),
	};
	stream.write_all(response.as_bytes()).await?;
	stream.flush().await?;
	Ok(())
}

fn http_response(status: &str, content_type: &str, body: &str) -> String {
	format!(
		"HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {length}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n\
         {body}",
		length = body.len()
	)
}

/// Request target from a raw request head, ignoring any query string.
fn request_path(head: &[u8]) -> Option<String> {
	let head = std::str::from_utf8(head).ok()?;
	let target = head.lines().next()?.split_whitespace().nth(1)?;
	Some(target.split(['?', '#']).next()?.to_string())
}

#[derive(Clone, Debug, PartialEq, Copy)]
enum ConnectionPeer {
	Tcp(SocketAddr),
	LocalSocket,
}

/// A browser proves its page origin, while the transport proves where the
/// client process is. A remote process can forge `Origin`, so TCP requires both
/// signals to be local. Unix sockets are already limited to local processes.
fn connection_allowed(peer: ConnectionPeer, origin: Option<&str>) -> bool {
	if matches!(peer, ConnectionPeer::Tcp(address) if !address.ip().is_loopback()) {
		return false;
	}
	match origin {
		None => true,
		Some(origin) => origin_host(origin).is_some_and(is_loopback_host),
	}
}

/// Handshake callback that rejects a non-loopback peer or browser origin.
// The signature is tungstenite's `Callback` contract, so the error type is not
// ours to shrink.
#[allow(clippy::result_large_err)]
fn check_origin(
	peer: ConnectionPeer,
	request: &Request,
	response: Response,
) -> Result<Response, ErrorResponse> {
	let origin = request
		.headers()
		.get(header::ORIGIN)
		.map(|value| value.to_str().unwrap_or_default());
	if connection_allowed(peer, origin) {
		return Ok(response);
	}
	warn!(?peer, origin = origin.unwrap_or_default(), "rejected a non-loopback frame connection");
	let mut rejection = ErrorResponse::new(Some(
		"frame connections are limited to loopback clients and origins".to_string(),
	));
	*rejection.status_mut() = StatusCode::FORBIDDEN;
	Err(rejection)
}

fn origin_host(origin: &str) -> Option<&str> {
	let authority = origin.split_once("://")?.1;
	match authority.strip_prefix('[') {
		Some(inner) => inner.split_once(']').map(|(host, _)| host),
		None => authority.split(':').next(),
	}
}

fn is_loopback_host(host: &str) -> bool {
	host.eq_ignore_ascii_case("localhost") ||
		host.parse::<IpAddr>().is_ok_and(|address| address.is_loopback())
}

#[cfg(unix)]
async fn accept_unix_loop(
	runtime: Arc<dyn ProductRuntimeFactory>,
	product: Arc<ProductSelection>,
	listener: UnixListener,
) -> Result<()> {
	loop {
		let (stream, peer) = match listener.accept().await {
			Ok(accepted) => accepted,
			Err(err) => {
				warn!(%err, "product frame accept failed");
				continue;
			},
		};
		let runtime = runtime.clone();
		let product = product.clone();
		tokio::spawn(async move {
			if let Err(err) =
				serve_connection(runtime, product, stream, ConnectionPeer::LocalSocket).await
			{
				debug!(?peer, %err, "frame connection ended");
			}
		});
	}
}

async fn serve_connection<S>(
	runtime: Arc<dyn ProductRuntimeFactory>,
	selected_product: Arc<ProductSelection>,
	stream: S,
	peer: ConnectionPeer,
) -> Result<()>
where
	S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
	// Subscribe before resolving the runtime so a concurrent replacement can
	// only cause an extra reconnect, never leave a connection on stale state.
	let reset = runtime.connection_reset();
	let product_updates = selected_product.subscribe();
	#[allow(clippy::result_large_err)]
	let ws = accept_hdr_async(stream, move |request: &Request, response: Response| {
		check_origin(peer, request, response)
	})
	.await?;
	let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<Message>();
	let product = product_updates.borrow().clone();
	let sink = Arc::new(WsFrameSink { outbound: outbound_tx.clone() });
	let product_runtime = Arc::new(runtime.product_runtime(product, sink));

	drive_connection(ws, product_runtime, reset, product_updates, outbound_tx, outbound_rx).await
}

async fn drive_connection<S>(
	ws: tokio_tungstenite::WebSocketStream<S>,
	product_runtime: Arc<dyn ConnectionRuntime>,
	mut reset: Option<watch::Receiver<u64>>,
	mut product_updates: watch::Receiver<ProductContext>,
	outbound_tx: mpsc::UnboundedSender<Message>,
	mut outbound_rx: mpsc::UnboundedReceiver<Message>,
) -> Result<()>
where
	S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
	let (mut write, mut read) = ws.split();
	let writer = tokio::spawn(async move {
		while let Some(message) = outbound_rx.recv().await {
			if write.send(message).await.is_err() {
				break;
			}
		}
	});
	let mut in_flight = tokio::task::JoinSet::new();

	loop {
		let message = tokio::select! {
			_ = connection_reset(&mut reset) => break,
			_ = product_updates.changed() => break,
			message = read.next() => message,
		};
		let Some(message) = message else {
			break;
		};
		let frame = match message {
			Ok(Message::Binary(bytes)) => bytes.to_vec(),
			Ok(Message::Text(text)) => text.as_bytes().to_vec(),
			Ok(Message::Close(_)) | Err(_) => break,
			Ok(_) => continue,
		};
		while in_flight.try_join_next().is_some() {}
		let product_runtime = product_runtime.clone();
		in_flight.spawn(async move {
			if let Err(err) = product_runtime.receive_frame(frame).await {
				debug!(%err, "product runtime rejected frame");
			}
		});
	}

	product_runtime.dispose();
	in_flight.abort_all();
	while in_flight.join_next().await.is_some() {}
	drop(product_runtime);
	drop(outbound_tx);
	let _ = writer.await;
	Ok(())
}

async fn connection_reset(reset: &mut Option<watch::Receiver<u64>>) {
	match reset {
		Some(reset) => {
			let _ = reset.changed().await;
		},
		None => std::future::pending().await,
	}
}

#[cfg(test)]
mod tests;
