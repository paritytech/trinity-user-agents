//! Chain access interfaces a host implements and the server runtime consumes.

use crate::ProviderError;
use async_trait::async_trait;
use futures::stream::BoxStream;

/// JSON-RPC provider factory for chain access.
///
/// The platform provides a way to get a JSON-RPC connection for a given chain.
/// The server runtime manages the chainHead v1 state machine on top of this.
/// Host-spec N.6 requires products to access chains through host-mediated
/// providers:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/N-shared-infrastructure.md?plain=1#L91-L102>
#[async_trait]
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

	/// Close the connection lease.
	///
	/// Hosts may keep a shared underlying transport alive, but this handle
	/// must stop receiving responses and release any per-caller resources.
	fn close(&self);
}
