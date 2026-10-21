//! Scripted json-rpc chain provider for tests: each request frame is answered
//! by a closure, and tests push notifications through the same stream.

use std::sync::{
	Arc, Mutex,
	atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use futures::{
	channel::mpsc as fut_mpsc,
	stream::{BoxStream, StreamExt},
};
use serde_json::Value;

use crate::{
	chain_runtime::{RuntimeChainProvider, RuntimeFailure},
	platform::JsonRpcConnection,
};

type Responder = Arc<dyn Fn(&str) -> Vec<String> + Send + Sync>;

/// One connection's response sender. The provider and the connection hold the
/// same slot rather than a clone each, so dropping it through either ends that
/// connection's stream.
type SenderSlot = Mutex<Option<fut_mpsc::UnboundedSender<String>>>;

/// One connection's response stream, taken once by whoever reads it.
type ReceiverSlot = Mutex<Option<fut_mpsc::UnboundedReceiver<String>>>;

/// Provider that echoes a canned response for every request it sees,
/// driven by a `respond` closure. The closure receives each json-rpc
/// request string and returns the response frames the test wants the
/// server to deliver. Keeps the response loop synchronized with the
/// request stream so there is no race between `send` and the response
/// loop draining frames before pending requests have registered.
pub struct ScriptedProvider {
	respond: Responder,
	/// Every request frame the core sent, in order.
	pub sent: Arc<Mutex<Vec<String>>>,
	/// First connection's response sender; dropping it ends that connection's
	/// responses. Later connections each get their own.
	pub sender: Arc<SenderSlot>,
	receiver: Arc<ReceiverSlot>,
	/// How many times the core asked for a connection.
	pub connect_calls: Arc<AtomicUsize>,
}

impl ScriptedProvider {
	/// Provider answering each request frame with `respond`'s frame, if any.
	pub fn new<F>(respond: F) -> Self
	where
		F: Fn(&str) -> Option<String> + Send + Sync + 'static,
	{
		Self::with_frames(move |request| respond(request).into_iter().collect())
	}

	/// Provider answering each request frame with every frame `respond`
	/// returns, delivered in order: a response followed by the notifications
	/// it starts.
	pub fn with_frames<F>(respond: F) -> Self
	where
		F: Fn(&str) -> Vec<String> + Send + Sync + 'static,
	{
		let (tx, rx) = fut_mpsc::unbounded();
		let first: Arc<SenderSlot> = Arc::new(Mutex::new(Some(tx)));
		Self {
			respond: Arc::new(respond),
			sent: Arc::new(Mutex::new(Vec::new())),
			sender: first,
			receiver: Arc::new(Mutex::new(Some(rx))),
			connect_calls: Arc::new(AtomicUsize::new(0)),
		}
	}
}

struct ScriptedConnection {
	respond: Responder,
	sent: Arc<Mutex<Vec<String>>>,
	/// This connection's own sender, so an answer goes back on the socket that
	/// asked rather than on whichever one connected first.
	sender: Arc<SenderSlot>,
	receiver: ReceiverSlot,
}

impl JsonRpcConnection for ScriptedConnection {
	fn send(&self, request: String) {
		self.sent.lock().unwrap().push(request.clone());
		let frames = (self.respond)(&request);
		if let Some(sender) = self.sender.lock().unwrap().as_ref() {
			for frame in frames {
				let _ = sender.unbounded_send(frame);
			}
		}
	}
	fn responses(&self) -> BoxStream<'static, String> {
		self.receiver
			.lock()
			.unwrap()
			.take()
			.expect("ScriptedConnection::responses called twice")
			.boxed()
	}

	fn close(&self) {
		self.sender.lock().unwrap().take();
	}
}

#[async_trait]
impl RuntimeChainProvider for ScriptedProvider {
	async fn connect(
		&self,
		_genesis_hash: Vec<u8>,
	) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
		self.connect_calls.fetch_add(1, Ordering::SeqCst);
		// The first connection takes the channel built with the provider, so
		// `notification_sender` keeps reaching it. Each later one gets its own.
		match self.receiver.lock().unwrap().take() {
			Some(receiver) => Ok(Arc::new(ScriptedConnection {
				respond: self.respond.clone(),
				sent: self.sent.clone(),
				sender: self.sender.clone(),
				receiver: Mutex::new(Some(receiver)),
			})),
			None => {
				let (tx, rx) = fut_mpsc::unbounded();
				let slot: Arc<SenderSlot> = Arc::new(Mutex::new(Some(tx)));
				Ok(Arc::new(ScriptedConnection {
					respond: self.respond.clone(),
					sent: self.sent.clone(),
					sender: slot,
					receiver: Mutex::new(Some(rx)),
				}))
			},
		}
	}
}

/// Clone of the scripted notification sender, used by tests to push
/// asynchronous frames (e.g. follow events) into the response stream.
pub fn notification_sender(provider: &ScriptedProvider) -> fut_mpsc::UnboundedSender<String> {
	provider
		.sender
		.lock()
		.unwrap()
		.as_ref()
		.expect("notification sender available")
		.clone()
}

/// Find the json-rpc request id of the just-sent frame so the scripted
/// responder can mirror it back to the dispatcher.
pub fn extract_id(request: &str) -> Option<String> {
	let value: Value = serde_json::from_str(request).ok()?;
	value.get("id")?.as_str().map(ToString::to_string)
}

/// Poll the sent frames until `predicate` holds or about five seconds pass,
/// returning the frames seen last.
pub fn wait_for_sent(
	provider: &ScriptedProvider,
	predicate: impl Fn(&[String]) -> bool,
) -> Vec<String> {
	for _ in 0..500 {
		let sent = provider.sent.lock().unwrap().clone();
		if predicate(&sent) {
			return sent;
		}
		std::thread::sleep(std::time::Duration::from_millis(10));
	}
	provider.sent.lock().unwrap().clone()
}
