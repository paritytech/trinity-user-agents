//! How a command's [`NetworkSource`] becomes the network it runs against.

use std::time::Duration;

use anyhow::{Result, anyhow, ensure};

use super::{
	IDENTITY_BACKEND_BASE_ENV, NetworkConfig, NetworkSource,
	built_in::{BUILT_IN_REFRESH_TIMEOUT, BuiltInCopy},
	checks::{ChainChecks, refuse_light_client},
	local_network,
	overrides::{ChainOverrides, apply_backend_override},
	reserved, stack,
};
use crate::{
	chain::ChainBackend,
	stack_descriptor::{FETCH_TIMEOUT, load_descriptor},
};

impl NetworkSource {
	/// The network a command runs against: the built-in network or the stack,
	/// then `HOST_CLI_*_WS` or `HOST_CLI_LOCAL_NETWORK_CONFIG` on top.
	///
	/// Fails when the routing sources cannot be combined, when a `--stack`
	/// descriptor is unreachable, when the descriptor in use is expired or
	/// lacks a role, when a network carries one of Polkadot's genesis hashes
	/// or the retired `polkadot` namespace, when a `--stack` uses the `dot`
	/// TLD or pcf-devnet's namespace, when the light client cannot serve the
	/// network, when an endpoint reports a genesis other than the one it is
	/// routed as, when an endpoint an override reroutes does not answer, and
	/// when People's `NetworkSuffix` differs from a stack's TLD. A built-in
	/// network whose published descriptor is unreachable runs on its bundled
	/// copy; when that copy fails a check, the descriptor is fetched once more
	/// and the checks repeat only when the published copy is then in use.
	pub async fn cli_config(&self) -> Result<NetworkConfig> {
		let (config, checks) = self.resolve_routes(BUILT_IN_REFRESH_TIMEOUT).await?;
		let Err(error) = checks.verify(&config).await? else { return Ok(config) };
		if checks.copy_in_use() != Some(BuiltInCopy::Unreachable) {
			return Err(error);
		}
		tracing::info!("{error}; fetching the published descriptor again");
		let (config, checks) = self.resolve_routes(FETCH_TIMEOUT).await?;
		if checks.copy_in_use() != Some(BuiltInCopy::Published) {
			return Err(error);
		}
		checks.verify(&config).await??;
		Ok(config)
	}

	/// [`Self::cli_config`] without contacting any chain; only a descriptor is
	/// fetched. An unreachable descriptor fails only for `--stack`.
	pub async fn resolve(&self) -> Result<NetworkConfig> {
		Ok(self.resolve_routes(BUILT_IN_REFRESH_TIMEOUT).await?.0)
	}

	/// The network and the [`ChainChecks`] to run against its chains. A
	/// built-in stack's published descriptor gets `fetch_timeout`.
	async fn resolve_routes(
		&self,
		fetch_timeout: Duration,
	) -> Result<(NetworkConfig, ChainChecks)> {
		let overrides = ChainOverrides::from_env()?;
		let local_path = local_network::config_path();
		self.ensure_compatible(&overrides.env_names_in_use(), local_path.is_some())?;
		let (base, built_in) = match (&self.stack, &local_path) {
			(Some(stack), _) => {
				let descriptor = load_descriptor(&stack.value).await;
				let base = descriptor
					.and_then(|descriptor| {
						stack::stack_network_config(&descriptor, self.chain_backend)
					})
					.and_then(|base| reserved::ensure_stack_not_reserved(&base).map(|()| base))
					.map_err(|error| anyhow!("{}: {error}", stack.source))?;
				(base, None)
			},
			// A local copy replaces every route, so the bundled copy is enough.
			(None, Some(_)) => (self.network.unwrap_or_default().config(self.chain_backend), None),
			(None, None) => {
				let (base, origin) = self
					.network
					.unwrap_or_default()
					.resolve_built_in(fetch_timeout, self.chain_backend)
					.await?;
				(base, Some(origin))
			},
		};
		let base_chains = base.chains.clone();
		let mut config = overrides
			.apply(apply_backend_override(base, std::env::var(IDENTITY_BACKEND_BASE_ENV).ok()));
		if let Some(path) = &local_path {
			config = local_network::resolve(config, path)?;
		}
		refuse_light_client(&config)?;
		reserved::ensure_no_reserved_values(&config)?;
		let checks = ChainChecks::plan(&config, &base_chains, local_path.is_some(), built_in);
		Ok((config, checks))
	}

	/// Fails when two routing sources cannot be combined: `--network` with a
	/// stack, a stack or a `HOST_CLI_*_WS` override (`override_names`) with the
	/// local-network file, and the light client with a stack, an override or
	/// the local-network file. Each message prints both sources.
	fn ensure_compatible(&self, override_names: &[&str], local_network: bool) -> Result<()> {
		let local_env = local_network::CONFIG_ENV;
		let light_client = self.chain_backend == ChainBackend::LightClient;
		if let Some(stack) = &self.stack {
			let source = stack.source;
			ensure!(
				self.network.is_none(),
				"--network cannot be used with {source}; unset one of them"
			);
			ensure!(
				!local_network,
				"{local_env} and {source} cannot be used together: both route every chain; unset \
				 one of them"
			);
			let bundled = truapi_provider::known_networks().collect::<Vec<_>>().join(", ");
			ensure!(
				!light_client,
				"--chain-backend light-client cannot run {source} {}: the light client syncs only \
				 the networks truapi-provider bundles ({bundled}); use --chain-backend rpc",
				stack.value
			);
		}
		// The chain URL overrides point a network at a fork with the same genesis;
		// the local-network file replaces the genesis too. Applying one over the
		// other would route some roles to one network and some to another.
		ensure!(
			override_names.is_empty() || !local_network,
			"{local_env} and {} cannot be used together: the file routes every chain itself; \
			 unset one of them",
			override_names.join(", ")
		);
		ensure!(
			override_names.is_empty() || !light_client,
			"{} and --chain-backend light-client cannot be used together: the light client syncs \
			 the live chains by genesis hash; unset one of them",
			override_names.join(", ")
		);
		ensure!(
			!local_network || !light_client,
			"{local_env} and --chain-backend light-client cannot be used together: the light \
			 client resolves chains by genesis hash from its bundled catalog, which has no local \
			 network; unset one of them"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests;
