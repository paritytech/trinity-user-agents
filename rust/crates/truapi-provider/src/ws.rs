//! Remote WebSocket JSON-RPC backend.
//!
//! The connection is a raw string pipe over a jsonrpsee WebSocket transport:
//! a writer task drains queued requests into the socket and the responses
//! stream yields every inbound text frame untouched. Reconnection is
//! deliberately not handled here — a dropped socket ends the responses
//! stream, and the consumer recovers by connecting again.

use core::{
	net::SocketAddr,
	pin::Pin,
	sync::atomic::{AtomicBool, Ordering},
	task::{Context, Poll},
	time::Duration,
};
use std::sync::{Arc, Mutex};

use crate::platform::JsonRpcConnection;
use futures::{
	channel::mpsc,
	future::{self, Either},
	stream::{self, AbortHandle, BoxStream, Stream, StreamExt},
};
use jsonrpsee_client_transport::ws::{WsHandshakeError, WsTransportClientBuilder};
use jsonrpsee_core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
use tokio::{
	io::{AsyncRead, AsyncWrite, ReadBuf},
	net::TcpStream,
	time::Instant,
};
use tokio_rustls::{TlsConnector, rustls::pki_types::ServerName};
use url::Url;

use crate::error::ProviderError;

/// Bounded depth of the outbound request buffer. The sole producer is a trusted
/// in-process consumer, so this rarely fills; when it does the socket is not
/// draining, and [`send`](WsConnection::send) ends the response stream rather
/// than letting the buffer grow without bound.
const REQUEST_BUFFER: usize = 1024;

/// Largest message the transport sends or accepts.
///
/// A Bulletin block holds up to 10 MiB of extrinsics and its body arrives
/// hex-encoded, so a busy block can pass jsonrpsee's 10 MiB default. Bulletin
/// submission fetches those bodies to find its transaction.
const MAX_MESSAGE_BYTES: u32 = 64 * 1024 * 1024;

/// How long resolving the host, connecting and the TLS handshake may each
/// take, as jsonrpsee allows for the WebSocket handshake that follows.
const DIAL_TIMEOUT: Duration = Duration::from_secs(10);

/// How many redirects the handshake follows, as jsonrpsee's own dial does.
const MAX_REDIRECTS: usize = 5;

/// Open a WebSocket connection to `url`.
///
/// Requires an ambient tokio runtime; both the handshake and the spawned
/// writer task run on it.
///
/// The socket is dialed here rather than by jsonrpsee, so that every read is
/// seen (see [`LastRead`]). Like jsonrpsee, the dial tries each resolved address
/// in turn and follows redirects.
pub async fn connect(url: Url) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
	if tokio::runtime::Handle::try_current().is_err() {
		return Err(ProviderError::MissingRuntime);
	}
	open(url.clone())
		.await
		.map_err(|reason| ProviderError::Handshake { url: crate::error::redacted(&url), reason })
}

/// Why one URL could not be opened.
#[derive(Debug)]
enum Unopened {
	/// The node answered the upgrade with a redirect to this location.
	Redirected(String),
	Failed(String),
}

impl From<String> for Unopened {
	fn from(reason: String) -> Self {
		Unopened::Failed(reason)
	}
}

/// Open `url`, following redirects. A failure past a redirect names the
/// redacted URL it happened at, since the caller reports only the URL it asked
/// for.
async fn open(mut url: Url) -> Result<Box<dyn JsonRpcConnection>, String> {
	for redirects in 0..=MAX_REDIRECTS {
		match open_once(&url).await {
			Ok(connection) => return Ok(connection),
			Err(Unopened::Failed(reason)) if redirects == 0 => return Err(reason),
			Err(Unopened::Failed(reason)) => {
				return Err(format!(
					"after a redirect to {}: {reason}",
					crate::error::redacted(&url)
				));
			},
			Err(Unopened::Redirected(location)) => {
				url = url
					.join(&location)
					.map_err(|err| format!("redirected to a location that is not a URL: {err}"))?;
			},
		}
	}
	Err(format!("more than {MAX_REDIRECTS} redirects"))
}

async fn open_once(url: &Url) -> Result<Box<dyn JsonRpcConnection>, Unopened> {
	let secure = match url.scheme() {
		"ws" => false,
		"wss" => true,
		other => return Err(format!("`{other}` is not a WebSocket URL scheme").into()),
	};
	let host = url
		.host_str()
		.ok_or("the URL names no host".to_owned())?
		.trim_start_matches('[')
		.trim_end_matches(']');
	let port = url.port_or_known_default().ok_or("the URL names no port".to_owned())?;
	let server_name = secure
		.then(|| ServerName::try_from(host.to_owned()))
		.transpose()
		.map_err(|err| err.to_string())?;
	let addresses =
		within_dial_timeout("resolving the host", tokio::net::lookup_host((host, port))).await?;
	open_any(url, server_name.as_ref(), addresses).await
}

/// Open `url` at each of `addresses` in turn, until one answers.
async fn open_any(
	url: &Url,
	server_name: Option<&ServerName<'static>>,
	addresses: impl IntoIterator<Item = SocketAddr>,
) -> Result<Box<dyn JsonRpcConnection>, Unopened> {
	let mut failure = "the host resolves to no address".to_owned();
	for address in addresses {
		match open_at(url, server_name, address).await {
			Err(Unopened::Failed(reason)) => {
				tracing::debug!(%address, "WebSocket dial failed: {reason}");
				failure = reason;
			},
			opened_or_redirected => return opened_or_redirected,
		}
	}
	Err(failure.into())
}

/// Open `url` at `address`, over TLS when `server_name` names the host.
async fn open_at(
	url: &Url,
	server_name: Option<&ServerName<'static>>,
	address: SocketAddr,
) -> Result<Box<dyn JsonRpcConnection>, Unopened> {
	let tcp = within_dial_timeout("connecting", TcpStream::connect(address)).await?;
	if let Err(err) = tcp.set_nodelay(true) {
		tracing::debug!("TCP_NODELAY not set: {err}");
	}
	let last_read = LastRead::now();
	let socket = Watched { socket: tcp, last_read: last_read.clone() };
	let Some(server_name) = server_name else {
		return handshake(url, socket, last_read).await;
	};
	let tls = TlsConnector::from(crate::tls::client_config()).connect(server_name.clone(), socket);
	handshake(url, within_dial_timeout("the TLS handshake", tls).await?, last_read).await
}

/// Run one step of a dial within [`DIAL_TIMEOUT`].
async fn within_dial_timeout<T>(
	step: &str,
	dial: impl Future<Output = std::io::Result<T>>,
) -> Result<T, String> {
	match tokio::time::timeout(DIAL_TIMEOUT, dial).await {
		Ok(dialed) => dialed.map_err(|err| err.to_string()),
		Err(_elapsed) => Err(format!("{step} took longer than {}s", DIAL_TIMEOUT.as_secs())),
	}
}

async fn handshake<S>(
	url: &Url,
	socket: S,
	last_read: LastRead,
) -> Result<Box<dyn JsonRpcConnection>, Unopened>
where
	S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
	let (sender, receiver) = WsTransportClientBuilder::default()
		.max_request_size(MAX_MESSAGE_BYTES)
		.max_response_size(MAX_MESSAGE_BYTES)
		.build_with_stream(url.clone(), socket)
		.await
		.map_err(|err| match err {
			WsHandshakeError::Redirected { location, .. } => Unopened::Redirected(location),
			other => Unopened::Failed(other.to_string()),
		})?;
	Ok(Box::new(WsConnection::start(sender, receiver, last_read)))
}

/// When bytes last arrived on a connection's socket.
///
/// jsonrpsee yields only whole messages, so the reader times out this silence
/// instead, which a large message that is still arriving does not trip.
#[derive(Clone)]
struct LastRead(Arc<Mutex<Instant>>);

impl LastRead {
	fn now() -> Self {
		LastRead(Arc::new(Mutex::new(Instant::now())))
	}

	fn touch(&self) {
		*self.0.lock().expect("last-read mutex poisoned") = Instant::now();
	}

	/// Resolves once nothing has arrived for [`READ_TIMEOUT`].
	async fn silence(&self) {
		loop {
			let deadline = *self.0.lock().expect("last-read mutex poisoned") + READ_TIMEOUT;
			if Instant::now() >= deadline {
				return;
			}
			tokio::time::sleep_until(deadline).await;
		}
	}
}

/// A socket that records each read in its [`LastRead`].
struct Watched<S> {
	socket: S,
	last_read: LastRead,
}

impl<S: AsyncRead + Unpin> AsyncRead for Watched<S> {
	fn poll_read(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut ReadBuf<'_>,
	) -> Poll<std::io::Result<()>> {
		let this = self.get_mut();
		let before = buf.filled().len();
		let polled = Pin::new(&mut this.socket).poll_read(cx, buf);
		if buf.filled().len() > before {
			this.last_read.touch();
		}
		polled
	}
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Watched<S> {
	fn poll_write(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<std::io::Result<usize>> {
		Pin::new(&mut self.get_mut().socket).poll_write(cx, buf)
	}

	fn poll_write_vectored(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		bufs: &[std::io::IoSlice<'_>],
	) -> Poll<std::io::Result<usize>> {
		Pin::new(&mut self.get_mut().socket).poll_write_vectored(cx, bufs)
	}

	fn is_write_vectored(&self) -> bool {
		self.socket.is_write_vectored()
	}

	fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		Pin::new(&mut self.get_mut().socket).poll_flush(cx)
	}

	fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		Pin::new(&mut self.get_mut().socket).poll_shutdown(cx)
	}
}

/// A live WebSocket connection exposed as a raw JSON-RPC pipe.
struct WsConnection {
	requests: Mutex<mpsc::Sender<String>>,
	responses: Mutex<Option<BoxStream<'static, String>>>,
	stream_abort: AbortHandle,
	closed: AtomicBool,
}

impl WsConnection {
	/// Spawn the writer task and set up the responses stream.
	///
	/// Generic over the jsonrpsee transport traits so tests can inject
	/// in-memory transports. `last_read` is the receiver's socket's.
	fn start<S, R>(sender: S, receiver: R, last_read: LastRead) -> Self
	where
		S: TransportSenderT + Send,
		R: TransportReceiverT + Send,
	{
		Self::start_with_buffer(sender, receiver, last_read, REQUEST_BUFFER)
	}

	/// [`start`](Self::start) with an explicit request-buffer depth, so tests
	/// can force an overflow deterministically.
	fn start_with_buffer<S, R>(sender: S, receiver: R, last_read: LastRead, buffer: usize) -> Self
	where
		S: TransportSenderT + Send,
		R: TransportReceiverT + Send,
	{
		let (requests, request_queue) = mpsc::channel(buffer);
		let (responses, stream_abort) = stream::abortable(response_stream(receiver, last_read));
		tokio::spawn(writer_pump(sender, request_queue, stream_abort.clone()));
		WsConnection {
			requests: Mutex::new(requests),
			responses: Mutex::new(Some(responses.boxed())),
			stream_abort,
			closed: AtomicBool::new(false),
		}
	}
}

impl JsonRpcConnection for WsConnection {
	fn send(&self, request: String) {
		if self.closed.load(Ordering::SeqCst) {
			return;
		}
		// A full buffer means the socket is not draining, and a disconnected
		// channel means the writer is already gone. Either way, end the
		// response stream so the id-correlating consumer reconnects rather than
		// buffering without bound or waiting on a request that will not be sent.
		if self
			.requests
			.lock()
			.expect("requests mutex poisoned")
			.try_send(request)
			.is_err()
		{
			self.close();
		}
	}

	fn responses(&self) -> BoxStream<'static, String> {
		match self.responses.lock().expect("responses mutex poisoned").take() {
			Some(responses) => responses,
			None => stream::empty().boxed(),
		}
	}

	fn close(&self) {
		if self.closed.swap(true, Ordering::SeqCst) {
			return;
		}
		self.requests.lock().expect("requests mutex poisoned").close_channel();
		self.stream_abort.abort();
		self.responses.lock().expect("responses mutex poisoned").take();
	}
}

impl Drop for WsConnection {
	fn drop(&mut self) {
		self.close();
	}
}

/// How often a quiet connection is pinged, and how long the reader waits for any
/// byte before declaring the socket dead.
///
/// The read timeout is a multiple of the ping interval so that one lost ping or
/// a slow peer does not tear down a healthy connection: two pings must go
/// unanswered first.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(20);
const READ_TIMEOUT: Duration = Duration::from_secs(50);

/// Drain queued requests into the transport; on queue close, close the
/// transport gracefully.
///
/// A send failure means the socket is gone, so it also aborts the response
/// stream: without that, in-flight requests would never be answered and the
/// consumer — which correlates responses by id — would hang. Ending the stream
/// is the disconnect signal it acts on.
async fn writer_pump<S: TransportSenderT>(
	mut sender: S,
	mut request_queue: mpsc::Receiver<String>,
	stream_abort: AbortHandle,
) {
	loop {
		// A silent connection still has to be probed: a half-open socket delivers
		// nothing and reports nothing, so without a ping there is no traffic to
		// fail on and the reader below waits forever.
		let next = tokio::time::timeout(KEEPALIVE_INTERVAL, request_queue.next()).await;
		let request = match next {
			Ok(Some(request)) => request,
			Ok(None) => break,
			Err(_elapsed) => {
				if let Err(err) = sender.send_ping().await {
					tracing::warn!("WebSocket keepalive ping failed: {err}");
					stream_abort.abort();
					return;
				}
				continue;
			},
		};
		if let Err(err) = sender.send(request).await {
			tracing::warn!("WebSocket send failed: {err}");
			stream_abort.abort();
			return;
		}
	}
	if let Err(err) = sender.close().await {
		tracing::debug!("WebSocket close failed: {err}");
	}
}

/// Yield every inbound text frame; the stream ends on transport error, EOF, or
/// no bytes arriving for [`READ_TIMEOUT`], which is the disconnect signal
/// consumers rely on.
///
/// The timeout is what makes that signal reliable. A half-open socket — the peer
/// gone without a FIN, so the kernel keeps the connection open — delivers neither
/// data nor error, and `receive()` on it simply never returns. The writer pings
/// every [`KEEPALIVE_INTERVAL`], so a live peer produces at least a `Pong` well
/// inside this window and only a dead one falls silent for the whole of it. A
/// message still arriving keeps the connection alive however long it takes.
fn response_stream<R: TransportReceiverT + Send>(
	receiver: R,
	last_read: LastRead,
) -> impl Stream<Item = String> + Send {
	stream::unfold((receiver, last_read), |(mut receiver, last_read)| {
		async move {
			loop {
				let received = {
					let receive = receiver.receive();
					let silence = last_read.silence();
					futures::pin_mut!(receive, silence);
					match future::select(receive, silence).await {
						Either::Left((received, _)) => received,
						Either::Right(((), _)) => {
							tracing::warn!(
								"no WebSocket traffic for {}s; treating the connection as dead",
								READ_TIMEOUT.as_secs()
							);
							return None;
						},
					}
				};
				match received {
					Ok(ReceivedMessage::Text(text)) => return Some((text, (receiver, last_read))),
					Ok(ReceivedMessage::Bytes(bytes)) => match String::from_utf8(bytes) {
						Ok(text) => return Some((text, (receiver, last_read))),
						Err(_) => tracing::warn!("dropping non-UTF-8 binary WebSocket frame"),
					},
					// Answers our keepalive: proof of life, nothing to forward.
					Ok(ReceivedMessage::Pong) => {},
					Err(err) => {
						tracing::debug!("WebSocket receive ended: {err}");
						return None;
					},
				}
			}
		}
	})
}

#[cfg(test)]
mod tests {
	use std::sync::Mutex;

	use crate::platform::JsonRpcConnection;
	use futures::{channel::mpsc, stream::StreamExt};
	use jsonrpsee_core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};

	use super::{
		LastRead, READ_TIMEOUT, Watched, WsConnection, connect, handshake, open, open_any,
	};

	/// Local error type so the fakes need no extra dev-deps.
	#[derive(Debug)]
	struct FakeError(&'static str);

	impl std::fmt::Display for FakeError {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			f.write_str(self.0)
		}
	}
	impl std::error::Error for FakeError {}

	struct FakeSender {
		sent: mpsc::UnboundedSender<String>,
		closed: std::sync::Arc<Mutex<bool>>,
	}

	impl TransportSenderT for FakeSender {
		type Error = FakeError;

		async fn send(&mut self, msg: String) -> Result<(), Self::Error> {
			self.sent.unbounded_send(msg).map_err(|_| FakeError("sink gone"))
		}

		async fn close(&mut self) -> Result<(), Self::Error> {
			*self.closed.lock().expect("lock") = true;
			Ok(())
		}
	}

	/// Sender whose `send` always fails, modelling a dead socket.
	struct FailingSender;

	impl TransportSenderT for FailingSender {
		type Error = FakeError;

		async fn send(&mut self, _msg: String) -> Result<(), Self::Error> {
			Err(FakeError("dead socket"))
		}

		async fn close(&mut self) -> Result<(), Self::Error> {
			Ok(())
		}
	}

	/// Sender whose `send` never resolves, so the writer parks after taking one
	/// request and the bounded buffer fills.
	struct StalledSender;

	impl TransportSenderT for StalledSender {
		type Error = FakeError;

		async fn send(&mut self, _msg: String) -> Result<(), Self::Error> {
			core::future::pending().await
		}

		async fn close(&mut self) -> Result<(), Self::Error> {
			Ok(())
		}
	}

	struct FakeReceiver {
		frames: mpsc::UnboundedReceiver<Result<ReceivedMessage, FakeError>>,
	}

	impl TransportReceiverT for FakeReceiver {
		type Error = FakeError;

		async fn receive(&mut self) -> Result<ReceivedMessage, Self::Error> {
			match self.frames.next().await {
				Some(frame) => frame,
				None => Err(FakeError("eof")),
			}
		}
	}

	struct Harness {
		connection: WsConnection,
		sent: mpsc::UnboundedReceiver<String>,
		frames: mpsc::UnboundedSender<Result<ReceivedMessage, FakeError>>,
		sender_closed: std::sync::Arc<Mutex<bool>>,
	}

	fn harness() -> Harness {
		let (sent_tx, sent_rx) = mpsc::unbounded();
		let (frames_tx, frames_rx) = mpsc::unbounded();
		let sender_closed = std::sync::Arc::new(Mutex::new(false));
		let connection = WsConnection::start(
			FakeSender { sent: sent_tx, closed: std::sync::Arc::clone(&sender_closed) },
			FakeReceiver { frames: frames_rx },
			LastRead::now(),
		);
		Harness { connection, sent: sent_rx, frames: frames_tx, sender_closed }
	}

	#[tokio::test]
	async fn requests_reach_the_transport() {
		let mut harness = harness();
		harness.connection.send("one".to_owned());
		harness.connection.send("two".to_owned());
		assert_eq!(harness.sent.next().await.as_deref(), Some("one"));
		assert_eq!(harness.sent.next().await.as_deref(), Some("two"));
	}

	#[tokio::test]
	async fn text_and_utf8_bytes_are_yielded_and_pongs_skipped() {
		let harness = harness();
		let mut responses = harness.connection.responses();
		harness.frames.unbounded_send(Ok(ReceivedMessage::Pong)).expect("send frame");
		harness
			.frames
			.unbounded_send(Ok(ReceivedMessage::Text("hello".to_owned())))
			.expect("send frame");
		harness
			.frames
			.unbounded_send(Ok(ReceivedMessage::Bytes(b"raw".to_vec())))
			.expect("send frame");
		assert_eq!(responses.next().await.as_deref(), Some("hello"));
		assert_eq!(responses.next().await.as_deref(), Some("raw"));
	}

	#[tokio::test]
	async fn receive_error_ends_the_stream() {
		let harness = harness();
		let mut responses = harness.connection.responses();
		harness.frames.unbounded_send(Err(FakeError("boom"))).expect("send frame");
		assert_eq!(responses.next().await, None);
	}

	#[tokio::test]
	async fn second_responses_call_is_empty() {
		let harness = harness();
		let _live = harness.connection.responses();
		let mut second = harness.connection.responses();
		assert_eq!(second.next().await, None);
	}

	#[tokio::test]
	async fn close_is_idempotent_ends_stream_and_closes_transport() {
		let mut harness = harness();
		let mut responses = harness.connection.responses();
		harness.connection.close();
		harness.connection.close();
		assert_eq!(responses.next().await, None);
		harness.connection.send("late".to_owned());
		// The writer drains the closed queue and then closes the transport.
		assert_eq!(harness.sent.next().await, None);
		tokio::task::yield_now().await;
		assert!(*harness.sender_closed.lock().expect("lock"));
	}

	/// A half-open socket: the peer is gone without a FIN, so the kernel keeps
	/// the connection and `receive()` neither returns nor errors, ever.
	struct SilentReceiver;

	impl TransportReceiverT for SilentReceiver {
		type Error = FakeError;

		async fn receive(&mut self) -> Result<ReceivedMessage, Self::Error> {
			std::future::pending().await
		}
	}

	/// Silence must end the stream, because that is the only disconnect signal
	/// the consumer has. Without the read timeout this test hangs forever.
	///
	/// Time is paused, so tokio advances it as soon as every task is idle and the
	/// timeout fires immediately instead of after the real `READ_TIMEOUT`.
	#[tokio::test(start_paused = true)]
	async fn a_silent_socket_ends_the_response_stream() {
		let (sent, _sent_rx) = mpsc::unbounded::<String>();
		let connection = WsConnection::start(
			FakeSender { sent, closed: std::sync::Arc::new(Mutex::new(false)) },
			SilentReceiver,
			LastRead::now(),
		);
		let mut responses = connection.responses();
		assert_eq!(responses.next().await, None);
	}

	/// One message whose bytes trickle in for several read timeouts, as a
	/// 13 MiB Bulletin block body does on a link under about 0.3 MB/s.
	struct TricklingReceiver {
		last_read: LastRead,
	}

	impl TransportReceiverT for TricklingReceiver {
		type Error = FakeError;

		async fn receive(&mut self) -> Result<ReceivedMessage, Self::Error> {
			for _ in 0..10 {
				tokio::time::sleep(READ_TIMEOUT * 2 / 3).await;
				self.last_read.touch();
			}
			Ok(ReceivedMessage::Text("block".to_owned()))
		}
	}

	/// A healthy connection carrying a slow message must not be treated as dead.
	#[tokio::test(start_paused = true)]
	async fn a_message_slower_than_the_read_timeout_still_arrives() {
		let (sent, _sent_rx) = mpsc::unbounded::<String>();
		let last_read = LastRead::now();
		let connection = WsConnection::start(
			FakeSender { sent, closed: std::sync::Arc::new(Mutex::new(false)) },
			TricklingReceiver { last_read: last_read.clone() },
			last_read,
		);
		assert_eq!(connection.responses().next().await.as_deref(), Some("block"));
	}

	/// Bytes read through the socket restart the silence the reader times out.
	#[tokio::test(start_paused = true)]
	async fn a_read_from_the_socket_restarts_the_silence() {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};

		let (near, mut far) = tokio::io::duplex(64);
		let last_read = LastRead::now();
		let mut socket = Watched { socket: near, last_read: last_read.clone() };
		tokio::time::advance(READ_TIMEOUT / 2).await;
		far.write_all(b"x").await.expect("the far end writes");
		socket.read_exact(&mut [0; 1]).await.expect("the byte arrives");

		let read_at = tokio::time::Instant::now();
		last_read.silence().await;
		assert_eq!(read_at.elapsed(), READ_TIMEOUT);
	}

	/// A WebSocket server on loopback that keeps every connection open.
	async fn websocket_server() -> std::net::SocketAddr {
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
			.await
			.expect("bind a loopback listener");
		let address = listener.local_addr().expect("listener address");
		tokio::spawn(async move {
			while let Ok((socket, _)) = listener.accept().await {
				tokio::spawn(async move {
					let Ok(mut socket) = tokio_tungstenite::accept_async(socket).await else {
						return;
					};
					while let Some(Ok(_)) = socket.next().await {}
				});
			}
		});
		address
	}

	/// A loopback server that answers every request with `response`, then closes.
	async fn raw_server(response: String) -> std::net::SocketAddr {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};

		let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
			.await
			.expect("bind a loopback listener");
		let address = listener.local_addr().expect("listener address");
		tokio::spawn(async move {
			while let Ok((mut socket, _)) = listener.accept().await {
				let mut request = [0; 1024];
				let _ = socket.read(&mut request).await;
				let _ = socket.write_all(response.as_bytes()).await;
			}
		});
		address
	}

	/// An address nothing listens on.
	async fn refusing_address() -> std::net::SocketAddr {
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
			.await
			.expect("bind a loopback listener");
		listener.local_addr().expect("listener address")
	}

	fn ws_url(address: std::net::SocketAddr) -> url::Url {
		url::Url::parse(&format!("ws://{address}")).expect("loopback URL parses")
	}

	/// A host is reached at the first address that answers, past addresses that
	/// refuse the connection or the upgrade, as a dead IPv6 route does.
	#[tokio::test]
	async fn a_failed_address_falls_back_to_the_next() {
		let live = websocket_server().await;
		let addresses = [refusing_address().await, raw_server(String::new()).await, live];

		let opened = open_any(&ws_url(live), None, addresses).await;
		assert!(opened.is_ok(), "{:?}", opened.err());
	}

	#[tokio::test]
	async fn a_redirect_is_followed() {
		let live = websocket_server().await;
		let redirect = raw_server(format!(
			"HTTP/1.1 301 Moved Permanently\r\nLocation: ws://{live}/\r\nContent-Length: 0\r\n\r\n"
		))
		.await;

		let opened = open(ws_url(redirect)).await;
		assert!(opened.is_ok(), "{:?}", opened.err());
	}

	/// The caller reports the URL it asked for, so a failure past a redirect has
	/// to name where it happened, and never the credentials a redirect carried.
	#[tokio::test]
	async fn a_failure_after_a_redirect_names_the_target_without_its_credentials() {
		let refusing = refusing_address().await;
		let redirect = raw_server(format!(
			"HTTP/1.1 301 Moved Permanently\r\nLocation: ws://user:secret@{refusing}/\r\n\
			 Content-Length: 0\r\n\r\n"
		))
		.await;

		let Err(reason) = open(ws_url(redirect)).await else {
			panic!("nothing listens at the redirect target");
		};
		assert!(reason.contains(&refusing.to_string()), "{reason}");
		assert!(!reason.contains("secret"), "{reason}");
	}

	/// A dial that hangs says which step hung, so a dead resolver is not taken
	/// for a firewall.
	#[tokio::test(start_paused = true)]
	async fn a_dial_timeout_names_the_step_that_hung() {
		let hung = super::within_dial_timeout(
			"resolving the host",
			core::future::pending::<std::io::Result<()>>(),
		)
		.await;
		assert_eq!(hung, Err("resolving the host took longer than 10s".to_owned()));
	}

	/// A URL that cannot be a WebSocket is refused before anything is dialed,
	/// rather than after a connection attempt that may take the whole timeout.
	#[tokio::test]
	async fn a_url_that_is_not_a_websocket_is_refused_before_dialing() {
		let Err(error) =
			connect(url::Url::parse("https://192.0.2.1").expect("static URL parses")).await
		else {
			panic!("an https URL is not a WebSocket");
		};
		assert!(error.to_string().contains("`https` is not a WebSocket URL scheme"), "{error}");
	}

	/// jsonrpsee checks the URL it is handed even over a socket secured here,
	/// and accepts `wss` only with its `tls` feature on.
	#[tokio::test]
	async fn a_wss_url_is_accepted_over_a_socket_secured_here() {
		let live = websocket_server().await;
		let socket = tokio::net::TcpStream::connect(live).await.expect("reach the server");
		let url = url::Url::parse(&format!("wss://{live}")).expect("loopback URL parses");

		let opened = handshake(&url, socket, LastRead::now()).await;
		assert!(opened.is_ok(), "{:?}", opened.err());
	}

	#[tokio::test]
	async fn writer_failure_ends_the_response_stream() {
		// A dead socket (send fails) must end the responses stream so the
		// consumer sees a disconnect instead of hanging on the request.
		let (_frames_tx, frames_rx) = mpsc::unbounded::<Result<ReceivedMessage, FakeError>>();
		let connection =
			WsConnection::start(FailingSender, FakeReceiver { frames: frames_rx }, LastRead::now());
		let mut responses = connection.responses();
		connection.send("req".to_owned());
		assert_eq!(responses.next().await, None);
	}

	#[tokio::test]
	async fn a_full_request_buffer_ends_the_response_stream() {
		// The writer stalls on the first request, so a tiny buffer fills and a
		// later send overflows; the consumer must see the stream end, not hang.
		let (_frames_tx, frames_rx) = mpsc::unbounded::<Result<ReceivedMessage, FakeError>>();
		let connection = WsConnection::start_with_buffer(
			StalledSender,
			FakeReceiver { frames: frames_rx },
			LastRead::now(),
			2,
		);
		let mut responses = connection.responses();
		for _ in 0..64 {
			connection.send("req".to_owned());
		}
		assert_eq!(responses.next().await, None);
	}

	#[tokio::test]
	async fn drop_closes_the_connection() {
		let harness = harness();
		let mut responses = harness.connection.responses();
		drop(harness.connection);
		assert_eq!(responses.next().await, None);
	}
}
