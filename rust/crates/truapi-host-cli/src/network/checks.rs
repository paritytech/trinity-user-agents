//! What the CLI checks of a resolved network before a command runs: whether
//! the stack names a light spec for each chain the light client runs and each
//! spec is the one it names, whether each endpoint reports the genesis it is
//! routed as, whether People reports the network's TLD as its
//! `NetworkSuffix`, and whether an endpoint with one of Polkadot's genesis
//! hashes serves a fork rather than Polkadot itself.

use anyhow::{Result, bail, ensure};
use truapi::statement_allowance::{fetch_genesis_hash, rpc::RpcClient, slot};

use super::{
	ChainEndpoint, Roles, Stack,
	built_in::BuiltInOrigin,
	polkadot_fork::{PolkadotChain, polkadot_chains},
	within_check_timeout,
};
use crate::{
	chain::{ChainBackend, MissingSpec, StackLightClient},
	stack_descriptor::ROLE_NAMES,
};

/// The relay's `role` in a descriptor's `chains`.
const RELAY_ROLE: &str = "Relay";

/// The checks of `stack`, a built-in from `built_in` or a `--stack` when
/// `None`: over RPC every endpoint and People's `NetworkSuffix` are checked,
/// on the light client every light spec is, and on either backend every endpoint with one of
/// Polkadot's genesis hashes is compared with real Polkadot. Fails when such an endpoint does not
/// show it serves a fork; otherwise the outcome of the endpoint checks, which a
/// fetch of the published descriptor may resolve.
pub async fn verify_chains(stack: &Stack, built_in: Option<&BuiltInOrigin>) -> Result<Result<()>> {
	let polkadot_chains = polkadot_chains(stack);
	let (endpoints, polkadot_forks) = futures::join!(
		verify_endpoints(stack, built_in),
		futures::future::try_join_all(polkadot_chains.iter().map(PolkadotChain::ensure_fork)),
	);
	polkadot_forks?;
	Ok(endpoints)
}

/// The chains the light client runs for `stack`: the relay and each role the
/// CLI routes.
fn light_client_chains(stack: &Stack) -> impl Iterator<Item = &trinity_stack::Chain> {
	let Roles { people, asset_hub, bulletin } = ROLE_NAMES;
	[RELAY_ROLE, people, asset_hub, bulletin]
		.into_iter()
		.filter_map(|role| stack.descriptor().chain(role))
}

/// Fails when `stack` runs on the light client and its descriptor names no
/// light spec for one of the chains the light client runs.
pub fn require_light_specs(stack: &Stack) -> Result<()> {
	if stack.chain_backend() != ChainBackend::LightClient {
		return Ok(());
	}
	for chain in light_client_chains(stack) {
		ensure!(
			chain.light_spec.is_some(),
			"--chain-backend light-client cannot run {}: its descriptor names no lightSpec for {}; \
			 use --chain-backend rpc",
			stack.namespace(),
			chain.role
		);
	}
	Ok(())
}

/// Gets every light spec of `stack` into `light`. Fails when one is not the
/// spec the descriptor names. A spec that cannot be fetched is logged, and
/// fetched again when a command first connects to its chain.
async fn verify_light_specs(light: &StackLightClient, stack: &Stack) -> Result<()> {
	let loads = light_client_chains(stack).map(|chain| async move {
		match light.load_spec(chain.genesis()).await {
			Ok(()) => Ok(()),
			Err(MissingSpec::Unreachable(reason)) => {
				tracing::warn!("{reason}; the chain is unavailable until it can be fetched");
				Ok(())
			},
			Err(MissingSpec::Wrong(error)) => bail!("the {} stack: {error}", stack.namespace()),
		}
	});
	futures::future::try_join_all(loads).await?;
	Ok(())
}

/// What an endpoint reports about its chain.
struct EndpointReport {
	genesis: [u8; 32],
	/// People's `NetworkSuffix`, read only for People's endpoint once its
	/// genesis matches.
	network_suffix: Option<Result<Vec<u8>>>,
}

/// The genesis `endpoint` reports and, for `stack`'s People, its
/// `NetworkSuffix` over the same connection.
async fn probe_endpoint(stack: &Stack, endpoint: &ChainEndpoint) -> Result<EndpointReport> {
	let rpc = RpcClient::connect(&endpoint.ws).await?;
	let genesis = fetch_genesis_hash(&rpc).await?;
	let read_suffix = endpoint.genesis == stack.genesis().people && genesis == endpoint.genesis;
	let network_suffix = match read_suffix {
		true => Some(slot::read_network_suffix(&rpc).await.map_err(anyhow::Error::from)),
		false => None,
	};
	Ok(EndpointReport { genesis, network_suffix })
}

/// Fails when an endpoint of `stack` reports a genesis other than the one it
/// is routed as, adding the restart hint of `built_in` to the error, or when
/// People reports a `NetworkSuffix` other than the stack's TLD. An endpoint
/// that does not answer within [`super::CHAIN_CHECK_TIMEOUT`], or a People
/// chain without a readable suffix, is logged. The light client does not use
/// the endpoints, so it has its light specs checked instead.
async fn verify_endpoints(stack: &Stack, built_in: Option<&BuiltInOrigin>) -> Result<()> {
	if stack.chain_backend() == ChainBackend::LightClient {
		return verify_light_specs(&*StackLightClient::of(stack)?, stack).await;
	}
	let checks = stack.chains().iter().map(|endpoint| async move {
		let report = match within_check_timeout(probe_endpoint(stack, endpoint)).await {
			Ok(report) => report,
			Err(error) => {
				tracing::warn!("cannot check the genesis of {}: {error:#}", endpoint.ws);
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
				tracing::warn!("cannot read the NetworkSuffix of {}: {error:#}", endpoint.ws)
			},
		}
		Ok(())
	});
	futures::future::try_join_all(checks).await?;
	Ok(())
}

#[cfg(test)]
mod tests;
