//! What the CLI checks of a resolved network before a command runs: whether
//! the light client can serve it, whether each endpoint reports the genesis
//! it is routed as, whether People reports the network's TLD as its
//! `NetworkSuffix`, and whether an endpoint with one of Polkadot's genesis
//! hashes serves a fork rather than Polkadot itself.

use anyhow::{Result, anyhow, ensure};

use super::{
	ChainEndpoint, Stack,
	built_in::{BUILT_IN_REFRESH_TIMEOUT, BuiltInCopy, BuiltInOrigin},
	polkadot_fork::{PolkadotChain, polkadot_chains},
};
use crate::{chain::ChainBackend, stack_descriptor::Roles};

/// What the CLI checks against the chains before a command runs.
pub struct ChainChecks {
	/// The stack's endpoints, when reached over RPC; one that does not answer
	/// within [`BUILT_IN_REFRESH_TIMEOUT`] is left to fail on first use.
	endpoints: Vec<ChainEndpoint>,
	/// For a built-in stack, the descriptor in use.
	built_in: Option<BuiltInOrigin>,
	/// The endpoints with one of Polkadot's genesis hashes, each of which must
	/// show it serves a fork.
	polkadot_chains: Vec<PolkadotChain>,
}

impl ChainChecks {
	/// The checks for `config`: a network reached over RPC has every endpoint
	/// and People's `NetworkSuffix` checked.
	pub fn plan(config: &Stack, built_in: Option<BuiltInOrigin>) -> Self {
		let chains = config.chains();
		let polkadot_chains = polkadot_chains(&chains);
		let endpoints = match config.chain_backend() {
			ChainBackend::Rpc => {
				let Roles { people, asset_hub, bulletin } = chains;
				vec![people, asset_hub, bulletin]
			},
			ChainBackend::LightClient => Vec::new(),
		};
		Self { endpoints, built_in, polkadot_chains }
	}

	/// Fails when an endpoint with one of Polkadot's genesis hashes does not
	/// show it serves a fork; otherwise the outcome of the endpoint checks,
	/// which a fetch of the published descriptor may resolve.
	pub async fn verify(&self, config: &Stack) -> Result<Result<()>> {
		let (endpoints, polkadot_forks) = futures::join!(
			verify_endpoints(config, &self.endpoints, self.built_in.as_ref()),
			futures::future::try_join_all(
				self.polkadot_chains.iter().map(PolkadotChain::ensure_fork)
			),
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
pub fn refuse_light_client(config: &Stack) -> Result<()> {
	if config.chain_backend() != ChainBackend::LightClient {
		return Ok(());
	}
	let namespace = config.namespace();
	let (_, catalog) = truapi_provider::EmbeddedChainProviderBuilder::new()
		.add_network(namespace)
		.map_err(|error| {
			anyhow!("--chain-backend light-client has no chain specs for {namespace}: {error}")
		})?;
	let genesis = config.genesis();
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
	config: &Stack,
	endpoint: &ChainEndpoint,
	read_suffix: bool,
) -> Result<EndpointReport, String> {
	use truapi::statement_allowance::{fetch_genesis_hash, rpc::RpcClient, slot};
	let provider = crate::chain::CliChainProvider::for_endpoint(config, endpoint);
	let client = provider.rpc_client(endpoint.genesis).await.map_err(|error| error.to_string())?;
	let rpc = RpcClient::new(client);
	let genesis = fetch_genesis_hash(&rpc).await.map_err(|error| error.to_string())?;
	let network_suffix = match read_suffix && genesis == endpoint.genesis {
		true => Some(slot::read_network_suffix(&rpc).await.map_err(|error| error.to_string())),
		false => None,
	};
	Ok(EndpointReport { genesis, network_suffix })
}

/// `check`'s outcome, or an error once [`BUILT_IN_REFRESH_TIMEOUT`] passes
/// without one.
pub async fn within_check_timeout<T>(
	check: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
	tokio::time::timeout(BUILT_IN_REFRESH_TIMEOUT, check).await.unwrap_or_else(|_| {
		Err(format!("no answer within {}s", BUILT_IN_REFRESH_TIMEOUT.as_secs()))
	})
}

/// Fails when one of `endpoints` reports a genesis other than the one it is
/// routed as, adding the restart hint of `built_in` to the error, or when
/// People reports a `NetworkSuffix` other than the stack's TLD. An endpoint
/// that does not answer, or a People chain without a readable suffix, is
/// logged.
async fn verify_endpoints(
	config: &Stack,
	endpoints: &[ChainEndpoint],
	built_in: Option<&BuiltInOrigin>,
) -> Result<()> {
	let people_genesis = config.genesis().people;
	let checks = endpoints.iter().map(|endpoint| async move {
		let read_suffix = endpoint.genesis == people_genesis;
		let report = match within_check_timeout(probe_endpoint(config, endpoint, read_suffix)).await
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
			config.namespace(),
			hex::encode(endpoint.genesis),
			built_in
				.map(|origin| format!("; {}", origin.restart_hint()))
				.unwrap_or_default()
		);
		let Some(suffix) = report.network_suffix else {
			return Ok(());
		};
		let expected = config.tld();
		match suffix {
			Ok(suffix) => ensure!(
				suffix == expected.as_bytes(),
				"{} reports NetworkSuffix {}, but the {} stack's tld is {expected}; identities \
				 derived under one would not match the other",
				endpoint.ws,
				String::from_utf8_lossy(&suffix),
				config.namespace()
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
