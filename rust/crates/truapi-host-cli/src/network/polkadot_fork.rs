//! The check that tells a chain carrying one of Polkadot's genesis hashes, a
//! fork, from Polkadot itself.

use anyhow::{Result, anyhow, ensure};
use serde_json::json;
use truapi::statement_allowance::rpc::RpcClient;

use super::{ChainEndpoint, built_in::polkadot_reference, checks::within_check_timeout};
use crate::stack_descriptor::Roles;

/// An endpoint with one of Polkadot's genesis hashes, and the RPC nodes of
/// real Polkadot's chain with that genesis.
#[derive(Debug, PartialEq, Eq)]
pub struct PolkadotChain {
	/// The endpoint that must show it serves a fork.
	pub endpoint: ChainEndpoint,
	/// Real Polkadot's nodes for the endpoint's chain, from the bundled
	/// reference, in the order they are tried.
	pub polkadot_rpc: Vec<String>,
}

/// The endpoints of `chains` with one of Polkadot's genesis hashes, each with
/// real Polkadot's nodes for that chain.
pub fn polkadot_chains(chains: &Roles<ChainEndpoint>) -> Vec<PolkadotChain> {
	let reference = polkadot_reference();
	chains
		.iter()
		.filter_map(|endpoint| {
			let genesis = format!("0x{}", hex::encode(endpoint.genesis));
			let chain = reference.chains.iter().find(|chain| chain.genesis_hash == genesis)?;
			Some(PolkadotChain { endpoint: endpoint.clone(), polkadot_rpc: chain.rpc.clone() })
		})
		.collect()
}

impl PolkadotChain {
	/// Fails unless the endpoint serves a fork of Polkadot rather than
	/// Polkadot itself: at the lower of its finalized block and real
	/// Polkadot's, its block hash must differ from Polkadot's. Also fails when
	/// the endpoint, or every Polkadot node, does not answer within
	/// [`super::built_in::BUILT_IN_REFRESH_TIMEOUT`], since the endpoint is then
	/// not shown to be a fork.
	pub async fn ensure_fork(&self) -> Result<()> {
		let ws = &self.endpoint.ws;
		let hashes = within_check_timeout(self.finalized_hashes()).await;
		let (height, fork_hash, polkadot_hash) = hashes.map_err(|reason| {
			anyhow!(
				"cannot tell {ws} from Polkadot ({reason}); a chain with Polkadot's genesis runs \
				 only once it shows a fork"
			)
		})?;
		ensure!(
			fork_hash != polkadot_hash,
			"{ws} serves Polkadot itself: its block {height} is Polkadot's {polkadot_hash}"
		);
		Ok(())
	}

	/// The lower of the endpoint's and Polkadot's finalized block numbers, and
	/// the block hash each reports there, or why one side did not answer.
	async fn finalized_hashes(&self) -> Result<(u32, String, String), String> {
		let (fork, polkadot) = futures::join!(
			connect_finalized(&self.endpoint.ws),
			connect_first_finalized(&self.polkadot_rpc)
		);
		let (fork, fork_finalized) = fork?;
		let (polkadot, polkadot_finalized) = polkadot?;
		let height = fork_finalized.min(polkadot_finalized);
		let (fork_hash, polkadot_hash) =
			futures::join!(block_hash(&fork, height), block_hash(&polkadot, height));
		let fork_hash = fork_hash.map_err(|reason| format!("its block {height}: {reason}"))?;
		let polkadot_hash =
			polkadot_hash.map_err(|reason| format!("Polkadot's block {height}: {reason}"))?;
		Ok((height, fork_hash, polkadot_hash))
	}
}

/// A connection to `ws` and the number of its finalized block.
async fn connect_finalized(ws: &str) -> Result<(RpcClient, u32), String> {
	let rpc = RpcClient::connect(ws).await.map_err(|error| error.to_string())?;
	let finalized = finalized_number(&rpc)
		.await
		.map_err(|reason| format!("its finalized block: {reason}"))?;
	Ok((rpc, finalized))
}

/// A connection to the first of Polkadot's `nodes` that reports its finalized
/// block, and that block's number.
async fn connect_first_finalized(nodes: &[String]) -> Result<(RpcClient, u32), String> {
	let mut failures = Vec::new();
	for node in nodes {
		match connect_finalized(node).await {
			Ok(connection) => return Ok(connection),
			Err(reason) => failures.push(format!("{node}: {reason}")),
		}
	}
	Err(format!("Polkadot's nodes did not answer: {}", failures.join("; ")))
}

/// The number of the block `rpc` reports as finalized.
async fn finalized_number(rpc: &RpcClient) -> Result<u32, String> {
	let head = rpc
		.call("chain_getFinalizedHead", json!([]))
		.await
		.map_err(|error| error.to_string())?;
	let header = rpc
		.call("chain_getHeader", json!([head]))
		.await
		.map_err(|error| error.to_string())?;
	let number = header["number"].as_str().ok_or("a header without a number")?;
	u32::from_str_radix(number.trim_start_matches("0x"), 16).map_err(|error| error.to_string())
}

/// The hash of `rpc`'s block at `height`.
async fn block_hash(rpc: &RpcClient, height: u32) -> Result<String, String> {
	let hash = rpc
		.call("chain_getBlockHash", json!([height]))
		.await
		.map_err(|error| error.to_string())?;
	hash.as_str()
		.map(str::to_owned)
		.ok_or_else(|| "no block at that height".to_owned())
}

#[cfg(test)]
mod tests;
