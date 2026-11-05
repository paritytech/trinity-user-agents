//! Remote WebSocket JSON-RPC backend.
//!
//! The connection is a raw string pipe over a jsonrpsee WebSocket transport:
//! a writer task drains queued requests into the socket and the responses
//! stream yields every inbound text frame untouched. Reconnection is
//! deliberately not handled here — a dropped socket ends the responses
//! stream, and the consumer recovers by connecting again.

use core::{
	sync::atomic::{AtomicBool, Ordering},
	time::Duration,
};
use std::sync::Mutex;

use crate::platform::JsonRpcConnection;
use futures::{
	channel::mpsc,
	stream::{self, AbortHandle, BoxStream, Stream, StreamExt},
};
use jsonrpsee_client_transport::ws::WsTransportClientBuilder;
use jsonrpsee_core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
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

/// Open a WebSocket connection to `url`.
///
/// Requires an ambient tokio runtime; both the handshake and the spawned
/// writer task run on it.
pub async fn connect(url: Url) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
	if tokio::runtime::Handle::try_current().is_err() {
		return Err(ProviderError::MissingRuntime);
	}

	let (sender, receiver) = WsTransportClientBuilder::default()
		.with_custom_cert_store(crate::tls::client_config().as_ref().clone())
		.max_request_size(MAX_MESSAGE_BYTES)
		.max_response_size(MAX_MESSAGE_BYTES)
		.build(url.clone())
		.await
		.map_err(|err| ProviderError::Handshake {
			url: crate::error::redacted(&url),
			reason: err.to_string(),
		})?;

	Ok(Box::new(WsConnection::start(sender, receiver)))
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
	/// in-memory transports.
	fn start<S, R>(sender: S, receiver: R) -> Self
	where
		S: TransportSenderT + Send,
		R: TransportReceiverT + Send,
	{
		Self::start_with_buffer(sender, receiver, REQUEST_BUFFER)
	}

	/// [`start`](Self::start) with an explicit request-buffer depth, so tests
	/// can force an overflow deterministically.
	fn start_with_buffer<S, R>(sender: S, receiver: R, buffer: usize) -> Self
	where
		S: TransportSenderT + Send,
		R: TransportReceiverT + Send,
	{
		let (requests, request_queue) = mpsc::channel(buffer);
		let (responses, stream_abort) = stream::abortable(response_stream(receiver));
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
/// frame before declaring the socket dead.
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
/// silence past [`READ_TIMEOUT`], which is the disconnect signal consumers rely
/// on.
///
/// The timeout is what makes that signal reliable. A half-open socket — the peer
/// gone without a FIN, so the kernel keeps the connection open — delivers neither
/// data nor error, and `receive()` on it simply never returns. The writer pings
/// every [`KEEPALIVE_INTERVAL`], so a live peer produces at least a `Pong` well
/// inside this window and only a dead one falls silent for the whole of it.
fn response_stream<R: TransportReceiverT + Send>(receiver: R) -> impl Stream<Item = String> + Send {
	stream::unfold(receiver, |mut receiver| {
		async move {
			loop {
				let received = match tokio::time::timeout(READ_TIMEOUT, receiver.receive()).await {
					Ok(received) => received,
					Err(_elapsed) => {
						tracing::warn!(
							"no WebSocket traffic for {}s; treating the connection as dead",
							READ_TIMEOUT.as_secs()
						);
						return None;
					},
				};
				match received {
					Ok(ReceivedMessage::Text(text)) => return Some((text, receiver)),
					Ok(ReceivedMessage::Bytes(bytes)) => match String::from_utf8(bytes) {
						Ok(text) => return Some((text, receiver)),
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
