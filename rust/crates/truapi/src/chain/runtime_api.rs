//! Runtime API calls over the state selected by a transport.

use subxt::{
	config::{RpcConfigFor, SubstrateConfig},
	utils::H256,
};
use subxt_rpcs::{LegacyRpcMethods, RpcClient};

/// Calls a runtime API in the transport's chain and block context.
///
/// A pinned transport keeps related calls at one block. A plain RPC client
/// uses the best block at the time of each request.
#[async_trait::async_trait]
pub trait RuntimeApiCaller: Send {
	/// Returns the runtime's encoded result without interpreting its shape.
	async fn call(&mut self, function: &str, input: &[u8]) -> Result<Vec<u8>, subxt::Error>;

	/// Calls `function` for every input.
	///
	/// Returns one result per input in input order, retaining individual failures.
	/// The transport determines concurrency and the block context.
	async fn call_batch(
		&mut self,
		function: &str,
		inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, subxt::Error>>;
}

#[async_trait::async_trait]
impl RuntimeApiCaller for RpcClient {
	async fn call(&mut self, function: &str, input: &[u8]) -> Result<Vec<u8>, subxt::Error> {
		Ok(state_call(self, function, input, None).await?)
	}

	async fn call_batch(
		&mut self,
		function: &str,
		inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, subxt::Error>> {
		state_call_batch(self, function, inputs, None).await
	}
}

/// Calls runtime APIs at one block over a plain RPC client, so related reads
/// see the same state.
pub struct RpcClientAtBlock {
	rpc: RpcClient,
	at: H256,
}

impl RpcClientAtBlock {
	/// Calls through `rpc`, every one at the block `at`.
	pub fn new(rpc: RpcClient, at: H256) -> Self {
		Self { rpc, at }
	}
}

#[async_trait::async_trait]
impl RuntimeApiCaller for RpcClientAtBlock {
	async fn call(&mut self, function: &str, input: &[u8]) -> Result<Vec<u8>, subxt::Error> {
		Ok(state_call(&self.rpc, function, input, Some(self.at)).await?)
	}

	async fn call_batch(
		&mut self,
		function: &str,
		inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, subxt::Error>> {
		state_call_batch(&self.rpc, function, inputs, Some(self.at)).await
	}
}

/// One `state_call` at `at`, or at the best block when `at` is `None`.
async fn state_call(
	rpc: &RpcClient,
	function: &str,
	input: &[u8],
	at: Option<H256>,
) -> Result<Vec<u8>, subxt_rpcs::Error> {
	LegacyRpcMethods::<RpcConfigFor<SubstrateConfig>>::new(rpc.clone())
		.state_call(function, Some(input), at)
		.await
}

/// A `state_call` for every input, all sent at once, with the results in
/// input order.
async fn state_call_batch(
	rpc: &RpcClient,
	function: &str,
	inputs: Vec<Vec<u8>>,
	at: Option<H256>,
) -> Vec<Result<Vec<u8>, subxt::Error>> {
	let calls = inputs.iter().map(|input| async move {
		state_call(rpc, function, input, at).await.map_err(subxt::Error::from)
	});
	futures::future::join_all(calls).await
}

#[cfg(test)]
mod tests;
