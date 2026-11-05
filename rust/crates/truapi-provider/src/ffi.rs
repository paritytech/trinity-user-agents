//! Swift/UniFFI bindings (the `uniffi` feature, native targets).
//!
//! Exposes the embedded smoldot [`ChainProvider`](crate::platform::ChainProvider)
//! to Swift (and other UniFFI targets): build a provider, connect to a chain by
//! genesis hash, and drive the raw JSON-RPC string pipe. Chain specs, relay
//! topology, and statement-store placement come from the bundled network
//! catalog, so the only argument a host supplies is the genesis hash.
//!
//! Inbound responses are delivered to a foreign [`ChainMessageListener`]; a
//! background thread pumps the response stream and invokes it until the
//! connection closes.
//!
//! The `tracing` calls in this module are not observable here. `tracing-subscriber`
//! is pulled in by the `js` feature alone and `mod logging` is `wasm32`-only, so a
//! build with `--features uniffi` has no way to install a subscriber and every
//! `warn!` field goes unformatted. The provider also ships as its own staticlib, so
//! a subscriber installed by another crate in the same app reaches a different
//! `tracing-core`. Treat the returned errors as the whole contract: a native host
//! learns nothing from the logs.

use core::future::Future;
use std::{
	fmt,
	sync::{Arc, Weak},
};

use crate::platform::{ChainProvider as _, JsonRpcConnection};
use futures::{
	executor::block_on,
	stream::{BoxStream, StreamExt},
};

use crate::{
	ChainLifecycle, EmbeddedChainProvider,
	storage::{StorageClient, StorageClientError},
};

// uniffi has no fixed-size array type, so a genesis hash crosses as bytes and
// converts back here. `custom_type!` only accepts a single identifier, which is
// all this private name is for.
type GenesisHash = [u8; 32];

uniffi::custom_type!(GenesisHash, Vec<u8>, {
	remote,
	lower: |hash| hash.to_vec(),
	try_lift: |bytes| Ok(bytes.as_slice().try_into()?),
});

/// Errors surfaced to the foreign caller.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ChainProviderError {
	/// The chain could not be connected (unknown genesis, transport failure).
	#[error("{reason}")]
	Connect {
		/// Human-readable failure reason.
		reason: String,
	},
	/// The genesis hash was not exactly 32 bytes.
	#[error("genesis hash must be 32 bytes")]
	BadGenesis,
	/// The host's listener failed in a way it did not declare.
	#[error("{reason}")]
	Listener {
		/// Human-readable failure reason.
		reason: String,
	},
	/// The lifecycle of a chain could not be watched: nothing is connected to
	/// it, or no thread could be started to deliver it.
	#[error("{reason}")]
	Lifecycle {
		/// Human-readable failure reason.
		reason: String,
	},
	/// Storage the host owns, or the snapshot taken to feed it, failed.
	#[error("{reason}")]
	Storage {
		/// Human-readable failure reason.
		reason: String,
	},
}

impl From<uniffi::UnexpectedUniFFICallbackError> for ChainProviderError {
	fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
		// Without this the generic converter panics, and `panic = "abort"` on
		// the shipping profile turns a listener's exception into a process
		// abort rather than an ended stream.
		tracing::warn!(
			reason = %err.reason,
			"chain listener threw an undeclared error; reporting it as a rejection"
		);
		ChainProviderError::Listener { reason: bounded_reason(err.reason) }
	}
}

/// Why the pump stopped delivering a connection's responses.
///
/// This says what the core observed, not whether a reconnect is wanted. On the
/// light-client path a response stream ends only when the connection is closed,
/// including by this host's own `disconnect()` or by dropping the handle, so
/// `StreamEnded` is usually the host's own teardown coming back to it. A host
/// that reconnects on it without checking its own intent will reconnect to a
/// connection it deliberately closed.
///
/// New variants may be added, so a Swift host should carry an `@unknown
/// default` and a Kotlin host an `else` branch.
///
/// Reconnecting is done from another thread, and a serial one: a listener
/// callback runs on the pump thread, and `ChainProvider::connect` refuses to
/// run there. Hopping to a concurrent queue lets reconnects run in parallel,
/// and each one costs a thread plus a chain-spec parse.
///
/// Do not re-queue work with `send` from `on_closed`. By then the connection
/// is closed either way: `close()` is what ended the stream on `StreamEnded`,
/// and the pump closes it before reporting `ListenerFailed`. `send` on a
/// closed connection is dropped silently, with no error and no frame for the
/// request's id, so a consumer correlating by id would wait forever. From `on_message`, where the
/// connection is still open, `send` behaves normally. `disconnect` is
/// idempotent and safe to call from either callback.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ChainCloseReason {
	/// The response stream ended.
	///
	/// Every connection this crate hands a host runs on the embedded light
	/// client, and that stream ends only when the connection is closed, so in
	/// practice this is your own teardown arriving back: `disconnect()`, or
	/// dropping the handle. It is not a report that the peer went away.
	StreamEnded,
	/// This listener returned an error from `on_message`, so the connection was
	/// closed under it.
	///
	/// A listener that rejects frames while it is shutting down reports this
	/// even when the host asked for the teardown: `on_message` blocks in
	/// foreign code, so a `disconnect()` can land while a frame is already in
	/// flight, and rejecting that frame is a listener failure like any other.
	ListenerFailed {
		/// What the listener reported, bounded to 256 characters. For logs
		/// and diagnostics: a
		/// host that needs to branch on its own failure modes should track them
		/// where it raised them.
		reason: String,
	},
}

thread_local! {
	/// Set while this thread is pumping a connection's responses, so every
	/// foreign listener callback runs with it true.
	static PUMPING: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Marks the pump thread for the length of one connection's pumping.
struct PumpGuard(bool);

impl PumpGuard {
	fn enter() -> Self {
		Self(PUMPING.replace(true))
	}
}

impl Drop for PumpGuard {
	fn drop(&mut self) {
		// Restore rather than clear: the spawn site and `pump_responses` both
		// hold one, and an inner drop must not clear the outer's flag.
		PUMPING.set(self.0);
	}
}

/// Longest listener-authored reason carried back to a host.
///
/// The string is whatever the foreign listener threw, so it is unbounded at the
/// source and crosses the boundary twice. 256 is the same order the host
/// callback adapters bound their own reasons to, though each counts in its
/// platform's unit: this one counts Unicode scalar values, so it never splits a
/// character.
pub const CLOSE_REASON_MAX_CHARS: usize = 256;

/// Bound a listener-authored reason to [`CLOSE_REASON_MAX_CHARS`] characters.
/// Render `value` into at most [`CLOSE_REASON_MAX_CHARS`] characters without
/// materializing the whole rendering first. A listener's reason is unbounded at
/// the source, so `to_string()` on it peaks far above what we keep.
fn bounded_display(value: &dyn fmt::Display) -> String {
	use fmt::Write as _;

	struct Capped {
		out: String,
		remaining: usize,
	}

	impl fmt::Write for Capped {
		fn write_str(&mut self, text: &str) -> fmt::Result {
			for character in text.chars() {
				if self.remaining == 0 {
					return Ok(());
				}
				self.out.push(character);
				self.remaining -= 1;
			}
			Ok(())
		}
	}

	let mut capped = Capped { out: String::new(), remaining: CLOSE_REASON_MAX_CHARS };
	// Writing into a String cannot fail.
	let _ = write!(capped, "{value}");
	capped.out
}

fn bounded_reason(reason: String) -> String {
	match reason.char_indices().nth(CLOSE_REASON_MAX_CHARS) {
		Some((end, _)) => reason[..end].to_string(),
		None => reason,
	}
}

/// Read a foreign-supplied genesis hash. UniFFI has no fixed-size array type,
/// so every method taking one takes a `Vec<u8>` and checks its length here.
fn genesis_from(genesis_hash: Vec<u8>) -> Result<[u8; 32], ChainProviderError> {
	genesis_hash.try_into().map_err(|_| ChainProviderError::BadGenesis)
}

/// Sink for a connection's inbound JSON-RPC responses and notifications,
/// implemented on the foreign (Swift) side.
#[uniffi::export(with_foreign)]
pub trait ChainMessageListener: Send + Sync {
	/// Called for each JSON-RPC response or notification string.
	fn on_message(&self, message: String) -> Result<(), ChainProviderError>;
	/// Called once the connection has closed, whichever way it ended.
	fn on_closed(&self, reason: ChainCloseReason) -> Result<(), ChainProviderError>;
}

/// Sink for a chain's lifecycle, implemented on the foreign side.
#[uniffi::export(with_foreign)]
pub trait ChainLifecycleListener: Send + Sync {
	/// Called with the current state, then once per change.
	fn on_lifecycle(&self, state: ChainLifecycle) -> Result<(), ChainProviderError>;
	/// Called once the watch has ended: stopped, the chain no longer running,
	/// or `on_lifecycle` having failed.
	fn on_ended(&self) -> Result<(), ChainProviderError>;
}

/// Without this the generic converter panics, so a store that throws anything
/// it did not declare would kill the process rather than fail the one call.
///
/// It lives here rather than beside the type because the reason is
/// foreign-authored, and this is where the crate bounds foreign text.
impl From<uniffi::UnexpectedUniFFICallbackError> for StorageClientError {
	fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
		tracing::warn!(
			reason = %error.reason,
			"storage threw an undeclared error, reported as a storage failure"
		);
		StorageClientError::new(bounded_reason(error.reason))
	}
}

/// Embedded-smoldot chain provider. Construct one per process and share it;
/// every connection runs on the single embedded light client.
#[derive(uniffi::Object)]
pub struct ChainProvider {
	inner: EmbeddedChainProvider,
}

#[uniffi::export]
impl ChainProvider {
	/// Create a provider backed by the bundled network catalog, with nowhere
	/// to keep finalized state: every chain syncs from the chain-spec
	/// checkpoint on every run, and [`load_database`](Self::load_database) and
	/// [`save_database`](Self::save_database) fail rather than quietly doing nothing.
	///
	/// Use [`with_storage`](Self::with_storage) to resume from state the host
	/// keeps for it.
	#[uniffi::constructor]
	pub fn new() -> Arc<Self> {
		Arc::new(Self { inner: EmbeddedChainProvider::builder().catalog().build() })
	}

	/// Create a provider that reads and writes database blobs through `client`.
	///
	/// The crate stores nothing itself, so a provider built with
	/// [`new`](Self::new) syncs every chain from the chain-spec checkpoint on
	/// every run. The host owns where the bytes live and therefore whether they
	/// are backed up, encrypted, or excluded from cloud sync.
	#[uniffi::constructor(name = "with_storage")]
	pub fn with_storage(client: Arc<dyn StorageClient>) -> Arc<Self> {
		Arc::new(Self { inner: EmbeddedChainProvider::builder().catalog().storage(client).build() })
	}

	/// Read the stored blob for `genesis_hash` into this provider, so the next
	/// [`connect`](Self::connect) to that chain resumes from finalized state
	/// instead of syncing from the chain-spec checkpoint. Answers whether a
	/// blob is now in hand.
	///
	/// Fails when the provider was built through [`new`](Self::new), which has
	/// nowhere to keep blobs. A chain that never warms up looks exactly like
	/// one with nothing stored yet, so the missing store is reported rather
	/// than swallowed: build with [`with_storage`](Self::with_storage). It also
	/// fails for a genesis outside the bundled catalog, which `connect` refuses
	/// too.
	///
	/// Call it before `connect`, and never from a listener callback. `connect`
	/// blocks its calling thread, so a store awaited underneath it would
	/// deadlock. The store is read at most once per chain: smoldot keys a chain
	/// by its genesis hash, so only the first connect to a chain can consume a
	/// blob.
	pub async fn load_database(&self, genesis_hash: Vec<u8>) -> Result<bool, ChainProviderError> {
		self.inner
			.load_database(genesis_from(genesis_hash)?)
			.await
			// Bounded here rather than in an adapter: a foreign store authors
			// this reason, so it is unbounded at the source and crosses the
			// boundary twice.
			.map_err(|error| ChainProviderError::Storage {
				reason: bounded_reason(error.to_string()),
			})
	}

	/// Snapshot the finalized state for `genesis_hash` and hand it to storage.
	/// Answers whether a blob was stored, and fails when the provider was built
	/// without one, for the same reason [`load_database`](Self::load_database) does.
	///
	/// This is a full round trip against the light client rather than a write,
	/// so call it while the app is alive. From a backgrounding callback treat
	/// it as best effort: nothing keeps a backgrounded app scheduled long
	/// enough to guarantee it finishes. A chain that has finalized nothing yet
	/// stores nothing and answers `false`, rather than replacing a good blob
	/// with one smoldot would discard.
	pub async fn save_database(&self, genesis_hash: Vec<u8>) -> Result<bool, ChainProviderError> {
		self.inner
			.save_database(genesis_from(genesis_hash)?)
			.await
			// Bounded here rather than in an adapter: a foreign store authors
			// this reason, so it is unbounded at the source and crosses the
			// boundary twice.
			.map_err(|error| ChainProviderError::Storage {
				reason: bounded_reason(error.to_string()),
			})
	}

	/// Open a connection to the chain identified by `genesis_hash` (32 bytes).
	/// The network is resolved from the catalog; responses are delivered to
	/// `listener` until the connection closes.
	pub fn connect(
		&self,
		genesis_hash: Vec<u8>,
		listener: Arc<dyn ChainMessageListener>,
	) -> Result<Arc<ChainConnection>, ChainProviderError> {
		// A listener callback runs on the pump thread, which is already inside
		// `block_on`; blocking again there is a panic the host cannot catch,
		// and an abort on the shipping profile. Refuse it as an error instead,
		// since reconnecting from `on_closed` is the flow `ChainCloseReason`
		// exists to inform.
		if PUMPING.get() {
			return Err(ChainProviderError::Connect {
				reason: "connect() cannot be called from a listener callback; \
                         call it from another thread"
					.to_string(),
			});
		}
		let genesis = genesis_from(genesis_hash)?;
		let connection = block_on(self.inner.connect(genesis))
			.map_err(|error| ChainProviderError::Connect { reason: error.to_string() })?;
		let connection: Arc<dyn JsonRpcConnection> = Arc::from(connection);

		let responses = connection.responses();
		// Weak on purpose: a strong handle here would keep the connection alive
		// forever. `Drop` is what calls `close()`, `close()` is what ends the
		// response stream, and the pump parks on that stream -- so holding a
		// strong reference means the drop that would release it can never run.
		let pumped = Arc::downgrade(&connection);
		spawn_pump("truapi-pump", pump_responses(responses, pumped, listener)).map_err(
			|error| ChainProviderError::Connect {
				reason: format!("could not start the response pump: {error}"),
			},
		)?;

		Ok(Arc::new(ChainConnection { inner: connection }))
	}

	/// Watch what the light client is doing on the chain identified by
	/// `genesis_hash` (32 bytes). `listener` receives the current state, then
	/// every change, until the watch is stopped or the chain stops running.
	///
	/// Fails when nothing is connected to the chain, so connect first.
	pub fn watch_lifecycle(
		&self,
		genesis_hash: Vec<u8>,
		listener: Arc<dyn ChainLifecycleListener>,
	) -> Result<Arc<LifecycleWatch>, ChainProviderError> {
		let states = self
			.inner
			.lifecycle(genesis_from(genesis_hash)?)
			.map_err(|error| ChainProviderError::Lifecycle { reason: error.to_string() })?;
		let (states, stop) = futures::stream::abortable(states);
		spawn_pump("truapi-lifecycle", pump_lifecycle(states, listener)).map_err(|error| {
			ChainProviderError::Lifecycle {
				reason: format!("could not start the lifecycle pump: {error}"),
			}
		})?;
		Ok(Arc::new(LifecycleWatch { stop }))
	}
}

/// Run `pump` to completion on a thread of its own, where foreign listener
/// callbacks are made.
///
/// Named for crash reports, and fallible: under EAGAIN the unnamed
/// `thread::spawn` panics, which the callers have a `Result` to avoid.
fn spawn_pump(name: &str, pump: impl Future<Output = ()> + Send + 'static) -> std::io::Result<()> {
	std::thread::Builder::new()
		.name(name.to_string())
		.spawn(move || {
			// Entered here, not inside the pump: the guard must outlive the
			// future, because dropping its listener releases the foreign object
			// and runs its destructor on this thread, still inside `block_on`.
			let _pumping = PumpGuard::enter();
			block_on(pump)
		})
		.map(drop)
}

/// Deliver lifecycle states to `listener` until the stream ends or the
/// listener fails, then tell it the watch ended.
async fn pump_lifecycle(
	mut states: impl futures::Stream<Item = ChainLifecycle> + Unpin,
	listener: Arc<dyn ChainLifecycleListener>,
) {
	while let Some(state) = states.next().await {
		if let Err(error) = listener.on_lifecycle(state) {
			tracing::warn!(%error, "lifecycle listener failed; ending the watch");
			break;
		}
	}
	if let Err(error) = listener.on_ended() {
		tracing::warn!(%error, "lifecycle listener failed on end");
	}
}

/// A live watch on one chain's lifecycle. Dropping it stops the watch.
#[derive(uniffi::Object)]
pub struct LifecycleWatch {
	stop: futures::stream::AbortHandle,
}

#[uniffi::export]
impl LifecycleWatch {
	/// Stop the watch; the listener's `on_ended` fires once the pump notices.
	///
	/// Not named `close`, for the reason [`ChainConnection::disconnect`] gives.
	pub fn stop(&self) {
		self.stop.abort();
	}
}

impl Drop for LifecycleWatch {
	fn drop(&mut self) {
		self.stop.abort();
	}
}

/// Deliver a connection's responses to its listener until the stream ends or
/// the listener fails.
///
/// A failing listener closes the connection: the response stream is take-once,
/// so it cannot be pumped again, and leaving the handle open would queue every
/// later send against a receiver that is gone. `on_closed` fires either way,
/// after the connection is closed, and names which of the two stopped the pump.
/// See [`ChainCloseReason`] for what that does and does not tell a host.
async fn pump_responses(
	mut responses: BoxStream<'static, String>,
	connection: Weak<dyn JsonRpcConnection>,
	listener: Arc<dyn ChainMessageListener>,
) {
	let _pumping = PumpGuard::enter();
	let mut reason = ChainCloseReason::StreamEnded;
	while let Some(message) = responses.next().await {
		if let Err(error) = listener.on_message(message) {
			tracing::warn!(%error, "chain listener failed; closing the connection");
			if let Some(connection) = connection.upgrade() {
				connection.close();
			}
			// Load-bearing, not redundant with the `From` impl: a listener that
			// throws a *declared* `ChainProviderError` is lifted by uniffi directly,
			// never crossing that impl, so this is the only bound on that path.
			reason = ChainCloseReason::ListenerFailed { reason: bounded_display(&error) };
			break;
		}
	}
	if let Err(error) = listener.on_closed(reason) {
		tracing::warn!(%error, "chain listener failed on close");
	}
}

/// A live JSON-RPC connection: a raw string pipe to one chain.
#[derive(uniffi::Object)]
pub struct ChainConnection {
	inner: Arc<dyn JsonRpcConnection>,
}

#[cfg(all(test, feature = "networks"))]
mod tests;

#[uniffi::export]
impl ChainConnection {
	/// Queue a JSON-RPC request string.
	pub fn send(&self, request: String) {
		self.inner.send(request);
	}

	/// Close the connection; the listener's `on_closed` fires once the stream
	/// ends.
	///
	/// NOT named `close`: uniffi's generated Kotlin object already implements
	/// `AutoCloseable.close()` for handle disposal, so an exported `close`
	/// produces two methods with the same signature and the module does not
	/// compile ("Conflicting overloads").
	pub fn disconnect(&self) {
		self.inner.close();
	}
}
