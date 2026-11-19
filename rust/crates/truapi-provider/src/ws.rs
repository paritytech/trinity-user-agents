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
	// The dial and handshake bytes belong to no message.
	last_read.message_returned();
	Ok(Box::new(WsConnection::start(sender, receiver, last_read)))
}

/// How many bytes must arrive with nothing returned before they count as a
/// message on its way.
///
/// Control frames arrive between messages too, each under 200 bytes on the
/// wire with TLS. A pong is returned and clears the count, but a ping the
/// transport answers itself is not. The pong to our keepalive clears what such
/// pings add, so this sits far above what they reach in one
/// [`KEEPALIVE_INTERVAL`].
const MESSAGE_ON_ITS_WAY_BYTES: usize = 4096;

/// What a connection's socket has read: when bytes last arrived, and how many
/// have arrived since the receiver last returned anything.
///
/// jsonrpsee yields only whole messages, so the reader times out silence on
/// the socket instead, which a large message that is still arriving does not
/// trip. The count tells such a message from control frames, and the
/// connection reports its silence as
/// [`JsonRpcConnection::mid_message_silence`].
#[derive(Clone)]
struct LastRead(Arc<Mutex<Reads>>);

struct Reads {
	at: Instant,
	unreturned_bytes: usize,
}

impl LastRead {
	fn now() -> Self {
		LastRead(Arc::new(Mutex::new(Reads { at: Instant::now(), unreturned_bytes: 0 })))
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, Reads> {
		self.0.lock().expect("last-read mutex poisoned")
	}

	fn touch(&self, bytes: usize) {
		let mut reads = self.lock();
		reads.at = Instant::now();
		reads.unreturned_bytes = reads.unreturned_bytes.saturating_add(bytes);
	}

	/// The receiver returned a message or a pong, so what was read is spent.
	fn message_returned(&self) {
		self.lock().unreturned_bytes = 0;
	}

	/// How long a message still on its way has gone without a byte.
	fn mid_message_silence(&self) -> Option<Duration> {
		let reads = self.lock();
		(reads.unreturned_bytes >= MESSAGE_ON_ITS_WAY_BYTES).then(|| reads.at.elapsed())
	}

	/// Resolves once nothing has arrived for [`READ_TIMEOUT`].
	async fn silence(&self) {
		loop {
			let deadline = self.lock().at + READ_TIMEOUT;
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
		let read = buf.filled().len() - before;
		if read > 0 {
			this.last_read.touch(read);
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
	last_read: LastRead,
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
		let (responses, stream_abort) =
			stream::abortable(response_stream(receiver, last_read.clone()));
		tokio::spawn(writer_pump(sender, request_queue, stream_abort.clone()));
		WsConnection {
			requests: Mutex::new(requests),
			responses: Mutex::new(Some(responses.boxed())),
			stream_abort,
			closed: AtomicBool::new(false),
			last_read,
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

	fn mid_message_silence(&self) -> Option<Duration> {
		self.last_read.mid_message_silence()
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
				if received.is_ok() {
					last_read.message_returned();
				}
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
mod tests;
