//! The check that tells a chain carrying one of Polkadot's genesis hashes, a
//! fork, from Polkadot itself.

use anyhow::{Result, anyhow, ensure};
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::json;
use truapi::statement_allowance::rpc::RpcClient;

use super::{ChainEndpoint, Stack, built_in::polkadot_reference, within_check_timeout};

/// An endpoint with one of Polkadot's genesis hashes, and the RPC nodes of
/// real Polkadot's chain with that genesis.
#[derive(Debug, PartialEq, Eq)]
pub struct PolkadotChain<'a> {
	/// The endpoint that must show it serves a fork.
	pub endpoint: &'a ChainEndpoint,
	/// Real Polkadot's nodes for the endpoint's chain, from the bundled
	/// reference, all contacted at once.
	pub polkadot_rpc: &'a [String],
}

/// The endpoints of `stack` with one of Polkadot's genesis hashes, each with
/// real Polkadot's nodes for that chain.
pub fn polkadot_chains(stack: &Stack) -> Vec<PolkadotChain<'_>> {
	let reference = polkadot_reference();
	stack
		.chains()
		.iter()
		.filter_map(|endpoint| {
			let chain =
				reference.chains.iter().find(|chain| chain.genesis() == endpoint.genesis)?;
			Some(PolkadotChain { endpoint, polkadot_rpc: &chain.rpc })
		})
		.collect()
}

impl PolkadotChain<'_> {
	/// Fails unless the endpoint serves a fork of Polkadot rather than
	/// Polkadot itself: at the lower of its finalized block and real
	/// Polkadot's, its block hash must differ from Polkadot's. Also fails when
	/// the endpoint, or every Polkadot node, does not answer within
	/// [`super::CHAIN_CHECK_TIMEOUT`] at a step, since the endpoint is then not
	/// shown to be a fork.
	pub async fn ensure_fork(&self) -> Result<()> {
		let ws = &self.endpoint.ws;
		let (height, fork_hash, polkadot_hash) =
			self.finalized_hashes().await.map_err(|reason| {
				anyhow!(
					"cannot tell {ws} from Polkadot ({reason}); a chain with Polkadot's genesis runs \
				 only once it shows a fork"
				)
			})?;
		ensure!(
			fork_hash != polkadot_hash,
			"{ws} cannot run: its block {height} is Polkadot's {polkadot_hash}: it is Polkadot \
			 itself, or a fork that has not finalized past its fork point"
		);
		Ok(())
	}

	/// The lower of the endpoint's and Polkadot's finalized block numbers, and
	/// the block hash each reports there, or which side did not answer.
	async fn finalized_hashes(&self) -> Result<(u32, String, String), String> {
		let ws = &self.endpoint.ws;
		let ((fork, fork_finalized), (polkadot, polkadot_finalized, polkadot_ws)) = futures::try_join!(
			async {
				within_check_timeout(connect_finalized(ws))
					.await
					.map_err(|reason| format!("{ws}: {reason}"))
			},
			connect_first_finalized(self.polkadot_rpc),
		)?;
		let height = fork_finalized.min(polkadot_finalized);
		let (fork_hash, polkadot_hash) = futures::try_join!(
			async {
				within_check_timeout(block_hash(&fork, height))
					.await
					.map_err(|reason| format!("{ws}: its block {height}: {reason}"))
			},
			async {
				within_check_timeout(block_hash(&polkadot, height)).await.map_err(|reason| {
					format!("Polkadot's {polkadot_ws}: its block {height}: {reason}")
				})
			},
		)?;
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

/// A connection to the first of Polkadot's `nodes` to report its finalized
/// block within [`super::CHAIN_CHECK_TIMEOUT`], that block's number and the
/// node's URL. All nodes are contacted at once, so one that stays silent
/// does not delay the others.
async fn connect_first_finalized(nodes: &[String]) -> Result<(RpcClient, u32, &str), String> {
	let mut attempts: FuturesUnordered<_> = nodes
		.iter()
		.map(|node| async move { (node, within_check_timeout(connect_finalized(node)).await) })
		.collect();
	let mut failures = Vec::new();
	while let Some((node, outcome)) = attempts.next().await {
		match outcome {
			Ok((rpc, finalized)) => return Ok((rpc, finalized, node)),
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
