//! Chain access interfaces a host implements and the server runtime consumes.

use core::time::Duration;

use crate::ProviderError;
use futures::stream::BoxStream;

/// JSON-RPC provider factory for chain access.
///
/// The platform provides a way to get a JSON-RPC connection for a given chain.
/// The server runtime manages the chainHead v1 state machine on top of this.
/// Host-spec N.6 requires products to access chains through host-mediated
/// providers:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/N-shared-infrastructure.md?plain=1#L91-L102>
#[async_trait::async_trait]
pub trait ChainProvider: Send + Sync {
	/// Open a JSON-RPC connection for the chain identified by `genesis_hash`.
	/// Drop the returned connection to disconnect.
	async fn connect(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError>;
}

/// A live JSON-RPC connection to a chain.
pub trait JsonRpcConnection: Send + Sync {
	/// Send a JSON-RPC request string.
	fn send(&self, request: String);

	/// Stream of JSON-RPC response strings.
	fn responses(&self) -> BoxStream<'static, String>;

	/// How long a message that is still arriving has gone without a byte, or
	/// `None` when no message is partly received or the transport cannot tell.
	///
	/// [`responses`](Self::responses) yields whole messages only, so a large
	/// message on its way looks like silence there. A caller that times out
	/// silence asks this as well, so that a slow download does not read as a
	/// dead connection. Keepalive traffic between messages is no message on its
	/// way, so it does not answer here.
	///
	/// Only a transport written in Rust sees bytes below a whole message, so
	/// this is hidden from the rustdoc the host codegen reads, and JS hosts
	/// are not offered a method the WASM bridge would never call.
	#[doc(hidden)]
	fn mid_message_silence(&self) -> Option<Duration> {
		None
	}

	/// Close the connection lease.
	///
	/// Hosts may keep a shared underlying transport alive, but this handle
	/// must stop receiving responses and release any per-caller resources.
	fn close(&self);
}
