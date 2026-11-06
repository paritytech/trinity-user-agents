//! What the CLI checks of a resolved network before a command runs: whether
//! the light client can serve it, whether each endpoint reports the genesis
//! it is routed as, whether People reports the network's TLD as its
//! `NetworkSuffix`, and whether an endpoint with one of Polkadot's genesis
//! hashes serves a fork rather than Polkadot itself.

use anyhow::{Result, anyhow, ensure};

use super::{
	ChainEndpoint, Stack,
	built_in::{BuiltInCopy, BuiltInOrigin},
	polkadot_fork::{PolkadotChain, polkadot_chains},
	within_check_timeout,
};
use crate::chain::ChainBackend;

/// What the CLI checks against the chains before a command runs.
pub struct ChainChecks {
	/// For a built-in stack, the descriptor in use.
	built_in: Option<BuiltInOrigin>,
}

impl ChainChecks {
	/// The checks for a stack from `built_in`, or from `--stack` when `None`:
	/// a stack reached over RPC has every endpoint and People's
	/// `NetworkSuffix` checked, and on either backend every endpoint with one
	/// of Polkadot's genesis hashes is compared with real Polkadot.
	pub fn plan(built_in: Option<BuiltInOrigin>) -> Self {
		Self { built_in }
	}

	/// Fails when an endpoint of `stack` with one of Polkadot's genesis hashes
	/// does not show it serves a fork; otherwise the outcome of the endpoint
	/// checks, which a fetch of the published descriptor may resolve.
	pub async fn verify(&self, stack: &Stack) -> Result<Result<()>> {
		let endpoints = match stack.chain_backend() {
			ChainBackend::Rpc => stack.chains().iter().collect(),
			ChainBackend::LightClient => Vec::new(),
		};
		let polkadot_chains = polkadot_chains(stack);
		let (endpoints, polkadot_forks) = futures::join!(
			verify_endpoints(stack, &endpoints, self.built_in.as_ref()),
			futures::future::try_join_all(polkadot_chains.iter().map(PolkadotChain::ensure_fork)),
		);
		polkadot_forks?;
		Ok(endpoints)
	}

	/// Which copy of a built-in stack's descriptor the checked network runs
	/// on; `None` for any other network.
	pub fn copy_in_use(&self) -> Option<BuiltInCopy> {
		self.built_in.as_ref().map(|origin| origin.copy)
	}
}

/// The light client knows only the networks `truapi-provider` bundles, by
/// genesis hash, so it can serve neither another network nor a built-in stack
/// that restarted after this build's chain specs.
pub fn refuse_light_client(stack: &Stack) -> Result<()> {
	if stack.chain_backend() != ChainBackend::LightClient {
		return Ok(());
	}
	let namespace = stack.namespace();
	let (_, catalog) = truapi_provider::EmbeddedChainProviderBuilder::new()
		.add_network(namespace)
		.map_err(|error| {
			anyhow!("--chain-backend light-client has no chain specs for {namespace}: {error}")
		})?;
	let genesis = stack.genesis();
	ensure!(
		(catalog.people, catalog.assethub, catalog.bulletin) ==
			(genesis.people, genesis.asset_hub, genesis.bulletin),
		"this build's light-client chain specs for {namespace} predate the stack's restart from \
		 genesis; use --chain-backend rpc, or `truapi-host update` once a release ships the new \
		 specs"
	);
	Ok(())
}

/// What an endpoint reports about its chain.
struct EndpointReport {
	genesis: [u8; 32],
	/// People's `NetworkSuffix`, read only when requested and the genesis
	/// matches the one the endpoint is routed as.
	network_suffix: Option<Result<Vec<u8>, String>>,
}

/// The genesis `endpoint` reports and, with `read_suffix`, its
/// `NetworkSuffix` over the same connection.
async fn probe_endpoint(
	stack: &Stack,
	endpoint: &ChainEndpoint,
	read_suffix: bool,
) -> Result<EndpointReport, String> {
	use truapi::statement_allowance::{fetch_genesis_hash, rpc::RpcClient, slot};
	let provider = crate::chain::CliChainProvider::for_endpoint(stack, endpoint);
	let client = provider.rpc_client(endpoint.genesis).await.map_err(|error| error.to_string())?;
	let rpc = RpcClient::new(client);
	let genesis = fetch_genesis_hash(&rpc).await.map_err(|error| error.to_string())?;
	let network_suffix = match read_suffix && genesis == endpoint.genesis {
		true => Some(slot::read_network_suffix(&rpc).await.map_err(|error| error.to_string())),
		false => None,
	};
	Ok(EndpointReport { genesis, network_suffix })
}

/// Fails when one of `endpoints` reports a genesis other than the one it is
/// routed as, adding the restart hint of `built_in` to the error, or when
/// People reports a `NetworkSuffix` other than the stack's TLD. An endpoint
/// that does not answer within [`super::CHAIN_CHECK_TIMEOUT`], or a People
/// chain without a readable suffix, is logged.
async fn verify_endpoints(
	stack: &Stack,
	endpoints: &[&ChainEndpoint],
	built_in: Option<&BuiltInOrigin>,
) -> Result<()> {
	let people_genesis = stack.genesis().people;
	let checks = endpoints.iter().map(|endpoint| async move {
		let read_suffix = endpoint.genesis == people_genesis;
		let report = match within_check_timeout(probe_endpoint(stack, endpoint, read_suffix)).await
		{
			Ok(report) => report,
			Err(error) => {
				tracing::warn!("cannot check the genesis of {}: {error}", endpoint.ws);
				return Ok(());
			},
		};
		ensure!(
			report.genesis == endpoint.genesis,
			"{} reports genesis 0x{}, but {} routes it as the chain with genesis 0x{}{}",
			endpoint.ws,
			hex::encode(report.genesis),
			stack.namespace(),
			hex::encode(endpoint.genesis),
			built_in
				.map(|origin| format!("; {}", origin.restart_hint()))
				.unwrap_or_default()
		);
		let Some(suffix) = report.network_suffix else {
			return Ok(());
		};
		let expected = stack.tld();
		match suffix {
			Ok(suffix) => ensure!(
				suffix == expected.as_bytes(),
				"{} reports NetworkSuffix {}, but the {} stack's tld is {expected}; identities \
				 derived under one would not match the other",
				endpoint.ws,
				String::from_utf8_lossy(&suffix),
				stack.namespace()
			),
			Err(error) => {
				tracing::warn!("cannot read the NetworkSuffix of {}: {error}", endpoint.ws)
			},
		}
		Ok(())
	});
	futures::future::try_join_all(checks).await?;
	Ok(())
}

#[cfg(test)]
mod tests;
