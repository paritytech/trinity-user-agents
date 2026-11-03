//! Runtime API calls over the state selected by a transport.

use subxt::config::{RpcConfigFor, SubstrateConfig};
use subxt_rpcs::{LegacyRpcMethods, RpcClient};

/// Calls a runtime API in the transport's chain and block context.
///
/// A pinned transport keeps related calls at one block. A plain RPC client
/// uses the best block at the time of each request.
#[async_trait::async_trait]
pub trait RuntimeApi: Send {
	/// Returns the runtime's encoded result without interpreting its shape.
	async fn call(&mut self, function: &str, input: &[u8]) -> Result<Vec<u8>, String>;
}

#[async_trait::async_trait]
impl RuntimeApi for RpcClient {
	async fn call(&mut self, function: &str, input: &[u8]) -> Result<Vec<u8>, String> {
		LegacyRpcMethods::<RpcConfigFor<SubstrateConfig>>::new(self.clone())
			.state_call(function, Some(input), None)
			.await
			.map_err(|error| error.to_string())
	}
}
