use std::sync::Mutex;

use crate::platform::JsonRpcConnection;
use futures::{channel::mpsc, stream::StreamExt};
use jsonrpsee_core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};

use super::WsConnection;

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
	);
	let mut responses = connection.responses();
	assert_eq!(responses.next().await, None);
}

#[tokio::test]
async fn writer_failure_ends_the_response_stream() {
	// A dead socket (send fails) must end the responses stream so the
	// consumer sees a disconnect instead of hanging on the request.
	let (_frames_tx, frames_rx) = mpsc::unbounded::<Result<ReceivedMessage, FakeError>>();
	let connection = WsConnection::start(FailingSender, FakeReceiver { frames: frames_rx });
	let mut responses = connection.responses();
	connection.send("req".to_owned());
	assert_eq!(responses.next().await, None);
}

#[tokio::test]
async fn a_full_request_buffer_ends_the_response_stream() {
	// The writer stalls on the first request, so a tiny buffer fills and a
	// later send overflows; the consumer must see the stream end, not hang.
	let (_frames_tx, frames_rx) = mpsc::unbounded::<Result<ReceivedMessage, FakeError>>();
	let connection =
		WsConnection::start_with_buffer(StalledSender, FakeReceiver { frames: frames_rx }, 2);
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
