//! What the CLI checks of a resolved network before a command runs: whether
//! the light client can serve it, whether each endpoint reports the genesis
//! it is routed as, and whether People reports the network's TLD as its
//! `NetworkSuffix`.

use anyhow::{Result, anyhow, bail, ensure};

use super::{
	ChainEndpoint, NetworkConfig,
	built_in::{BUILT_IN_REFRESH_TIMEOUT, BuiltInCopy, BuiltInOrigin},
};
use crate::{chain::ChainBackend, stack_descriptor::Roles};

/// What the CLI checks against the chains before a command runs.
pub struct ChainChecks {
	/// Endpoints an override reroutes; one that does not answer is an error.
	rerouted: Vec<ChainEndpoint>,
	/// The other endpoints of the stack; one that does not answer within
	/// [`BUILT_IN_REFRESH_TIMEOUT`] is left to fail on first use.
	from_stack: Vec<ChainEndpoint>,
	/// For a built-in stack, the descriptor in use.
	built_in: Option<BuiltInOrigin>,
	/// The TLD People's `NetworkSuffix` must equal, when the stack's own
	/// endpoints are checked.
	expected_suffix: Option<&'static str>,
}

impl ChainChecks {
	/// The checks for `config`, resolved from `base_chains`: endpoints `config`
	/// routes differently from `base_chains`, by genesis and URL, are
	/// rerouted. A network reached over RPC without a local network also has
	/// its other endpoints and People's `NetworkSuffix` checked.
	pub fn plan(
		config: &NetworkConfig,
		base_chains: &Roles<ChainEndpoint>,
		local_network: bool,
		built_in: Option<BuiltInOrigin>,
	) -> Self {
		let (rerouted, untouched): (Vec<_>, Vec<_>) = config
			.chains
			.iter()
			.cloned()
			.partition(|endpoint| !base_chains.iter().any(|chain| chain == endpoint));
		let checks_stack = !local_network && config.chain_backend == ChainBackend::Rpc;
		Self {
			rerouted,
			from_stack: if checks_stack { untouched } else { Vec::new() },
			built_in,
			expected_suffix: checks_stack.then_some(config.network_suffix),
		}
	}

	/// Fails when an endpoint an override reroutes fails its check; otherwise
	/// the outcome of the stack's own endpoints, which a fetch of the published
	/// descriptor may resolve.
	pub async fn verify(&self, config: &NetworkConfig) -> Result<Result<()>> {
		let (rerouted, from_stack) = futures::join!(
			verify_endpoints(config, &self.rerouted, Unanswered::Fail, None, self.expected_suffix),
			verify_endpoints(
				config,
				&self.from_stack,
				Unanswered::Warn,
				self.built_in.as_ref(),
				self.expected_suffix
			),
		);
		rerouted?;
		Ok(from_stack)
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
pub fn refuse_light_client(config: &NetworkConfig) -> Result<()> {
	if config.chain_backend != ChainBackend::LightClient {
		return Ok(());
	}
	let (_, catalog) = truapi_provider::EmbeddedChainProviderBuilder::new()
		.add_network(&config.id)
		.map_err(|error| {
			anyhow!("--chain-backend light-client has no chain specs for {}: {error}", config.id)
		})?;
	ensure!(
		(catalog.people, catalog.assethub, catalog.bulletin) ==
			(
				config.chains.people.genesis,
				config.chains.asset_hub.genesis,
				config.chains.bulletin.genesis
			),
		"this build's light-client chain specs for {} predate the stack's restart from genesis; \
		 use --chain-backend rpc, or `truapi-host update` once a release ships the new specs",
		config.id
	);
	Ok(())
}

/// What a genesis check does with an endpoint that does not answer.
#[derive(Clone, Copy)]
enum Unanswered {
	/// Fail the command.
	Fail,
	/// Log it after at most [`BUILT_IN_REFRESH_TIMEOUT`] and leave it to fail
	/// on first use.
	Warn,
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
	config: &NetworkConfig,
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

/// Fails when one of `endpoints` reports a genesis other than the one it is
/// routed as, adding the restart hint of `built_in` to the error, when People
/// reports a `NetworkSuffix` other than `expected_suffix`, or when an endpoint
/// does not answer with `Unanswered::Fail`. A People chain without a readable
/// suffix is logged.
async fn verify_endpoints(
	config: &NetworkConfig,
	endpoints: &[ChainEndpoint],
	unanswered: Unanswered,
	built_in: Option<&BuiltInOrigin>,
	expected_suffix: Option<&str>,
) -> Result<()> {
	let checks = endpoints.iter().map(|endpoint| async move {
		let expected_suffix =
			expected_suffix.filter(|_| endpoint.genesis == config.chains.people.genesis);
		let probe = probe_endpoint(config, endpoint, expected_suffix.is_some());
		let report = match unanswered {
			Unanswered::Fail => probe.await,
			Unanswered::Warn => {
				tokio::time::timeout(BUILT_IN_REFRESH_TIMEOUT, probe).await.unwrap_or_else(|_| {
					Err(format!("no answer within {}s", BUILT_IN_REFRESH_TIMEOUT.as_secs()))
				})
			},
		};
		let report = match (report, unanswered) {
			(Ok(report), _) => report,
			(Err(error), Unanswered::Fail) => {
				bail!("cannot check the genesis of {}: {error}", endpoint.ws)
			},
			(Err(error), Unanswered::Warn) => {
				tracing::warn!("cannot check the genesis of {}: {error}", endpoint.ws);
				return Ok(());
			},
		};
		ensure!(
			report.genesis == endpoint.genesis,
			"{} reports genesis 0x{}, but {} routes it as the chain with genesis 0x{}{}",
			endpoint.ws,
			hex::encode(report.genesis),
			config.id,
			hex::encode(endpoint.genesis),
			built_in
				.map(|origin| format!("; {}", origin.restart_hint()))
				.unwrap_or_default()
		);
		let (Some(expected), Some(suffix)) = (expected_suffix, report.network_suffix) else {
			return Ok(());
		};
		match suffix {
			Ok(suffix) => ensure!(
				suffix == expected.as_bytes(),
				"{} reports NetworkSuffix {}, but the {} stack's tld is {expected}; identities \
				 derived under one would not match the other",
				endpoint.ws,
				String::from_utf8_lossy(&suffix),
				config.id
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
