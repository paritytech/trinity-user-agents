use std::sync::Mutex;

use crate::platform::JsonRpcConnection;
use futures::{channel::mpsc, stream::StreamExt};
use jsonrpsee_core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};

use super::{
	LastRead, MESSAGE_ON_ITS_WAY_BYTES, READ_TIMEOUT, Watched, WsConnection, connect, handshake,
	open, open_any,
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
			self.last_read.touch(64 * 1024);
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

/// The consumer times silence by whole messages and asks this to tell a slow
/// message from keepalive traffic. Bytes count as a message on its way only
/// past what control frames add up to, and stop counting once the receiver
/// returns what they carried.
#[tokio::test(start_paused = true)]
async fn only_a_message_on_its_way_reports_its_silence() {
	let (sent, _sent_rx) = mpsc::unbounded::<String>();
	let (frames, frames_rx) = mpsc::unbounded();
	let last_read = LastRead::now();
	let connection = WsConnection::start(
		FakeSender { sent, closed: std::sync::Arc::new(Mutex::new(false)) },
		FakeReceiver { frames: frames_rx },
		last_read.clone(),
	);
	let mut responses = connection.responses();

	last_read.touch(MESSAGE_ON_ITS_WAY_BYTES - 1);
	let control_frames = connection.mid_message_silence();
	last_read.touch(1);
	let quiet = READ_TIMEOUT / 2;
	tokio::time::advance(quiet).await;
	let on_its_way = connection.mid_message_silence();
	frames
		.unbounded_send(Ok(ReceivedMessage::Text("message".to_owned())))
		.expect("send frame");
	responses.next().await;

	assert_eq!(
		[control_frames, on_its_way, connection.mid_message_silence()],
		[None, Some(quiet), None]
	);
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

/// A listener on a free loopback port, and the address it listens on.
async fn loopback_listener() -> (tokio::net::TcpListener, std::net::SocketAddr) {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
		.await
		.expect("bind a loopback listener");
	let address = listener.local_addr().expect("listener address");
	(listener, address)
}

/// A WebSocket server on loopback that keeps every connection open.
async fn websocket_server() -> std::net::SocketAddr {
	let (listener, address) = loopback_listener().await;
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

	let (listener, address) = loopback_listener().await;
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
	let (_, address) = loopback_listener().await;
	address
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
