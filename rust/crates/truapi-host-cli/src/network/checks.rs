//! What the CLI checks of a resolved network before a command runs: whether
//! the light client can serve it, and whether each endpoint reports the
//! genesis and People the `NetworkSuffix` the network routes it as.

use anyhow::{Result, anyhow, bail, ensure};

use super::{
	ChainEndpoint, NetworkConfig,
	resolve::{BUILT_IN_REFRESH_TIMEOUT, BuiltInCopy, BuiltInOrigin},
};
use crate::chain::ChainBackend;

/// What the CLI checks against the chains before a command runs.
pub struct ChainChecks {
	/// Endpoints an override reroutes; one that does not answer is an error.
	rerouted: Vec<ChainEndpoint>,
	/// The other endpoints of a stack network; one that does not answer within
	/// [`BUILT_IN_REFRESH_TIMEOUT`] is left to fail on first use.
	from_stack: Vec<ChainEndpoint>,
	/// For a built-in stack, the descriptor in use.
	built_in: Option<BuiltInOrigin>,
	/// For a stack network, the TLD People's `NetworkSuffix` must equal.
	expected_suffix: Option<&'static str>,
}

impl ChainChecks {
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

	pub fn copy_in_use(&self) -> Option<BuiltInCopy> {
		self.built_in.as_ref().map(|origin| origin.copy)
	}
}

/// The checks for `config`, resolved from `base`: endpoints `config` routes
/// differently from `base` are rerouted. A stack network reached over RPC
/// without a local network also has its other endpoints and People's
/// `NetworkSuffix` checked.
pub fn plan_chain_checks(
	config: &NetworkConfig,
	base: &NetworkConfig,
	local_network: bool,
	built_in: Option<BuiltInOrigin>,
) -> ChainChecks {
	let (rerouted, untouched) = split_rerouted(config, base);
	let is_stack_network =
		base.disposable_identities && !local_network && config.chain_backend == ChainBackend::Rpc;
	ChainChecks {
		rerouted,
		from_stack: if is_stack_network { untouched } else { Vec::new() },
		built_in,
		expected_suffix: is_stack_network.then_some(config.network_suffix),
	}
}

/// `config`'s endpoints split into those that `base` does not route the same
/// way, by genesis and URL, and those it does.
fn split_rerouted(
	config: &NetworkConfig,
	base: &NetworkConfig,
) -> (Vec<ChainEndpoint>, Vec<ChainEndpoint>) {
	config
		.chains
		.iter()
		.cloned()
		.partition(|endpoint| !base.chains.iter().any(|chain| chain == endpoint))
}

/// What a genesis mismatch on a built-in stack's endpoints adds to its error.
fn format_restart_hint(origin: &BuiltInOrigin) -> String {
	let BuiltInOrigin { id, url, copy } = origin;
	match copy {
		BuiltInCopy::Published => format!(
			"the {id} stack restarted from genesis after the published descriptor in use ({url}) \
			 was published"
		),
		BuiltInCopy::Unreachable => format!(
			"the {id} stack restarted from genesis after the bundled descriptor in use was built, \
			 and the published copy at {url} could not be fetched"
		),
		BuiltInCopy::Rejected => format!(
			"the {id} stack restarted from genesis after the bundled descriptor in use was built, \
			 and the published copy at {url} was rejected, as the warning above reports"
		),
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
				.map(|origin| format!("; {}", format_restart_hint(origin)))
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
