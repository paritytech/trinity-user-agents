//! Product-frame WebSocket bridge for the pairing host.
//!
//! Each WebSocket connection is one product: inbound binary frames are pushed
//! into a [`ProductRuntime`] and its outgoing frames are written back as
//! binary messages. One binary WS message carries exactly one SCALE
//! `ProtocolMessage`, matching the browser transport's framing.

use core::sync::atomic::{AtomicU64, Ordering};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
#[cfg(test)]
use tokio_tungstenite::accept_async;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{StatusCode, header};
use tracing::{debug, warn};

use crate::bootstrap;
use truapi::platform::ProductExecutionKind;
use truapi::{
    ChannelId, DebugSink, FrameSink, PairingHostRuntime, ProductContext, ProductRuntime,
    ProductRuntimeError, SigningHostRuntime,
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
    execution_kind: ProductExecutionKind,
}

impl ProductSelection {
    /// Validate and normalize the initial product id.
    pub fn new(product_id: String, execution_kind: ProductExecutionKind) -> Result<Arc<Self>> {
        let product = ProductContext::new_with_execution(product_id, execution_kind)
            .map_err(|error| anyhow::anyhow!("invalid product id: {error}"))?;
        let (current, _) = watch::channel(product);
        Ok(Arc::new(Self {
            current,
            execution_kind,
        }))
    }

    /// Return the normalized current product id.
    pub fn current(&self) -> String {
        self.current.borrow().product_id.clone()
    }

    /// Select a validated product, returning whether the selection changed.
    pub fn select(&self, product_id: String) -> Result<bool> {
        let product = ProductContext::new_with_execution(product_id, self.execution_kind)
            .map_err(|error| anyhow::anyhow!("invalid product id: {error}"))?;
        Ok(self.current.send_if_modified(|current| {
            if current == &product {
                false
            } else {
                *current = product;
                true
            }
        }))
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
        Arc::new(Self {
            inner,
            sink,
            connections: AtomicU64::new(0),
        })
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
        Arc::new(Self {
            current: RwLock::new(runtime),
            generation,
        })
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
            }
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
        ReplayStream {
            prefix: self.consumed,
            prefix_position: 0,
            stream,
        }
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
    head.windows(HEAD_TERMINATOR.len())
        .position(|window| window == HEAD_TERMINATOR)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    warn!(
        ?peer,
        origin = origin.unwrap_or_default(),
        "rejected a non-loopback frame connection"
    );
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
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
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
            }
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
    let sink = Arc::new(WsFrameSink {
        outbound: outbound_tx.clone(),
    });
    let product_runtime = Arc::new(runtime.product_runtime(product, sink));

    drive_connection(
        ws,
        product_runtime,
        reset,
        product_updates,
        outbound_tx,
        outbound_rx,
    )
    .await
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
        }
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::Notify;
    use tokio::task::JoinHandle;
    use tokio_tungstenite::client_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

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
        ui: Option<crate::terminal_ui::UiHandle>,
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
            if let Some(ui) = &self.ui {
                ui.confirm("sign raw", "pending product payload").await;
                Ok(())
            } else {
                std::future::pending().await
            }
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
        let product = ProductSelection::new("localhost:3000".into(), ProductExecutionKind::App)?;
        let container = tempfile::NamedTempFile::new()?;
        std::fs::write(container.path(), "installSandbox();")?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            serve_tcp_connection(runtime, product, stream, &endpoint, container.path()).await
        });
        Ok((address, server))
    }

    fn signing_runtime() -> Result<Arc<dyn ProductRuntimeFactory>> {
        let network = crate::network::Network::default().config();
        let platform = crate::platform::CliPlatform::new(
            network,
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
            truapi::platform::PlatformInfo {
                kind: Some("test".into()),
                version: None,
            },
            network.people_genesis,
            network.bulletin_genesis,
            network.asset_hub_genesis,
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
            (
                ConnectionPeer::LocalSocket,
                Some("http://localhost:3000"),
                true,
            ),
            (
                ConnectionPeer::LocalSocket,
                Some("https://evil.example"),
                false,
            ),
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
        let actual =
            expected.map(|(peer, origin, _)| (peer, origin, connection_allowed(peer, origin)));

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
            script.contains(&format!(
                r#"window.__truapi_localhost = {{ url: "{endpoint}" }};"#
            )),
            "{script}"
        );
        assert!(script.ends_with("\ninstallSandbox();"));

        let missing = fetch("/nope", endpoint, true).await?;
        assert!(missing.starts_with("HTTP/1.1 404 Not Found"), "{missing}");
        let missing_asset = fetch(bootstrap::PATH, endpoint, false).await?;
        assert!(
            missing_asset.starts_with("HTTP/1.1 500 Internal Server Error"),
            "{missing_asset}"
        );
        assert!(
            missing_asset.contains("reinstall truapi-host"),
            "{missing_asset}"
        );
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
            response.contains(&format!(
                r#"window.__truapi_localhost = {{ url: "ws://{address}" }};"#
            )),
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
        let product = ProductSelection::new("localhost:3000".into(), ProductExecutionKind::App)?;
        let endpoint = format!("ws://{address}");

        let client_exchange = async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            client
                .write_all(b"strap.js HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .await?;
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
        request.headers_mut().insert(
            header::ORIGIN,
            "http://localhost:3000".parse().expect("valid origin"),
        );
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
        let (mut ui, handle) = crate::terminal_ui::TerminalUi::new(
            "testnet",
            "localhost:3000",
            "default",
            Vec::new(),
            "info".into(),
        );
        let runtime = Arc::new(PendingRuntime {
            ui: Some(handle),
            ..PendingRuntime::default()
        });
        let server_runtime = runtime.clone();
        let product = ProductSelection::new("localhost:3000".into(), ProductExecutionKind::App)?;
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
        assert!(ui.has_pending_approval());
        websocket.send(Message::Binary(vec![1])).await?;
        tokio::time::timeout(
            Duration::from_secs(1),
            runtime.second_dispatch_finished.notified(),
        )
        .await?;
        drop(websocket);

        tokio::time::timeout(Duration::from_secs(1), server).await???;
        assert!(!ui.has_pending_approval());
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
        request.headers_mut().insert(
            header::ORIGIN,
            "https://evil.example".parse().expect("valid origin"),
        );
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
    fn product_selection_validates_and_normalizes_ids() -> Result<()> {
        let product = ProductSelection::new(" Dotli.DOT ".to_string(), ProductExecutionKind::App)?;

        assert_eq!(product.current(), "dotli.dot");
        assert!(product.select("localhost:3000".to_string())?);
        assert_eq!(product.current(), "localhost:3000");
        assert!(!product.select("LOCALHOST:3000".to_string())?);
        assert!(product.select("example.com".to_string()).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn changing_product_notifies_connections() -> Result<()> {
        let product = ProductSelection::new("first.dot".to_string(), ProductExecutionKind::App)?;
        let mut connection = product.subscribe();

        assert!(product.select("second.dot".to_string())?);
        connection.changed().await?;
        assert_eq!(connection.borrow().product_id, "second.dot");
        assert!(!product.select("SECOND.DOT".to_string())?);
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
        let socket_path = server
            .endpoint()
            .strip_prefix("ws+unix:")
            .expect("Unix endpoint prefix");
        let socket_path = std::path::PathBuf::from(socket_path);
        let socket_directory = socket_path
            .parent()
            .expect("socket has a parent directory")
            .to_path_buf();
        assert!(socket_path.exists());

        let FrameListener::Unix(listener) = &server.listener else {
            panic!("default listener must be Unix");
        };
        let server_exchange = async {
            let (stream, _) = listener.accept().await?;
            let mut websocket = accept_async(stream).await?;
            let message = websocket
                .next()
                .await
                .context("client closed before sending")??;
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
        use truapi::frame::{
            MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE, Payload, ProtocolMessage, request_ids,
        };
        use truapi::versioned::local_storage::{
            HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
            HostLocalStorageWriteRequest,
        };
        use truapi::{v01, v02};

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
            // The override must land on `live_chain_endpoints`: that is the
            // table `CliPlatform` resolves a genesis through. `asset_hub_ws`
            // feeds the standalone `AssetHubReader` subcommands. Only Asset Hub
            // is rerouted here.
            static CLOSED_ASSET_HUB: &[crate::network::ChainEndpoint] =
                &[crate::network::ChainEndpoint {
                    genesis: crate::network::PASEO_ASSET_HUB.genesis,
                    ws: "ws://127.0.0.1:1",
                    required_for_host: true,
                }];
            let mut network = crate::network::Network::default().config();
            network.live_chain_endpoints = CLOSED_ASSET_HUB;
            let asset_hub_genesis = network.asset_hub_genesis;
            let platform = crate::platform::CliPlatform::new(
                network,
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
                truapi::platform::PlatformInfo {
                    kind: Some("test".into()),
                    version: None,
                },
                network.people_genesis,
                network.bulletin_genesis,
                network.asset_hub_genesis,
                network.network_suffix.to_string(),
            )?;
            // An override written to a field this path never reads compiles,
            // changes nothing, and looks identical in timings. Assert it.
            assert_eq!(
                platform.routed_url(&asset_hub_genesis),
                "ws://127.0.0.1:1",
                "the Asset Hub genesis must route to the closed port, not to live Paseo"
            );
            let spawner: truapi::subscription::Spawner = Arc::new(|_| {});
            Ok((
                platform.clone(),
                SigningHostRuntime::new(platform, config, spawner),
            ))
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
            let connection = runtime.product_runtime(
                ProductContext::new(product.to_string()).expect("the product id is valid"),
                frames.clone(),
            );
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
            connection
                .receive_frame(frame)
                .await
                .expect("the host answers the frame");
            let answered = ProtocolMessage::decode(&mut frames.take_one().as_slice())
                .expect("the answer is a protocol message");
            assert_eq!(
                (
                    answered.payload.trait_id,
                    answered.payload.method_id,
                    answered.payload.message_type,
                ),
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
                product: Some(OWNER.to_string()),
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
                Ok(HostLocalStorageReadResponse::V2(v01::HostLocalStorageReadResponse {
                    value,
                })) => Ok(value),
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

        let mut reset = tapped
            .connection_reset()
            .context("the tap dropped the reset signal")?;
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
            if let truapi::DebugEvent::Frame {
                channel_id,
                dir,
                bytes,
            } = event
            {
                self.frames
                    .lock()
                    .expect("recording debug sink mutex poisoned")
                    .push((channel_id.0, dir, bytes));
            }
        }
    }

    impl RecordingDebugSink {
        fn taken(&self) -> Vec<(String, truapi::FrameDirection, Vec<u8>)> {
            self.frames
                .lock()
                .expect("recording debug sink mutex poisoned")
                .clone()
        }
    }

    fn tapped_product(
        tapped: &Arc<DebugTappedRuntime>,
        product_id: &str,
    ) -> Result<ProductRuntime> {
        let product =
            ProductContext::new_with_execution(product_id.into(), ProductExecutionKind::App)?;
        Ok(tapped.product_runtime(product, Arc::new(DiscardingFrameSink)))
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
        let runtime = tapped_product(&tapped, "localhost:3000")?;

        let frame = vec![0xde, 0xad, 0xbe, 0xef];
        let _ = runtime.receive_frame(frame.clone()).await;
        runtime.dispose();

        assert_eq!(
            sink.taken(),
            vec![(
                "localhost:3000#0".to_string(),
                truapi::FrameDirection::In,
                frame
            )]
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

        let first = tapped_product(&tapped, "localhost:3000")?;
        let second = tapped_product(&tapped, "localhost:3000")?;
        let _ = first.receive_frame(vec![1]).await;
        let _ = second.receive_frame(vec![2]).await;
        first.dispose();
        second.dispose();

        let channels: Vec<String> = sink.taken().into_iter().map(|(id, _, _)| id).collect();
        assert_eq!(channels, vec!["localhost:3000#0", "localhost:3000#1"]);
        Ok(())
    }
}
