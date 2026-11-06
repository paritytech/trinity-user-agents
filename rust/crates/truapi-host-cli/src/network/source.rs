//! How a command's [`NetworkSource`] becomes the network it runs against.

use std::time::Duration;

use anyhow::{Result, anyhow, ensure};

use super::{
	NetworkConfig, NetworkSource,
	built_in::{BUILT_IN_REFRESH_TIMEOUT, BuiltInCopy},
	checks::{ChainChecks, refuse_light_client},
	stack,
};
use crate::{
	chain::ChainBackend,
	stack_descriptor::{FETCH_TIMEOUT, load_descriptor},
};

impl NetworkSource {
	/// The network a command runs against: the built-in network or the stack.
	///
	/// Fails when `--network` is combined with a stack, when a `--stack`
	/// descriptor is unreachable, when the descriptor in use is expired or
	/// lacks a role, when a `--stack` has the id `polkadot`, the retired
	/// namespace, when the light client cannot serve the network, when an
	/// endpoint reports a genesis other than the one it is routed as, when an
	/// endpoint with one of Polkadot's genesis hashes does not show it serves a
	/// fork, and when People's `NetworkSuffix` differs from a stack's TLD. A
	/// built-in network whose published descriptor is unreachable runs on its
	/// bundled copy; when that copy fails a check, the descriptor is fetched
	/// once more and the checks repeat only when the published copy is then in
	/// use.
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
		self.ensure_compatible()?;
		let (config, built_in) = match &self.stack {
			Some(stack) => {
				let config = load_descriptor(&stack.value)
					.await
					.and_then(|descriptor| {
						stack::stack_network_config(&descriptor, self.chain_backend)
					})
					.map_err(|error| anyhow!("{}: {error}", stack.source))?;
				(config, None)
			},
			None => {
				let (config, origin) = self
					.network
					.unwrap_or_default()
					.resolve_built_in(fetch_timeout, self.chain_backend)
					.await?;
				(config, Some(origin))
			},
		};
		refuse_light_client(&config)?;
		let checks = ChainChecks::plan(&config, built_in);
		Ok((config, checks))
	}

	/// Fails for `--network` with a stack, and for the light client with a
	/// stack. Each message prints both sources.
	fn ensure_compatible(&self) -> Result<()> {
		let Some(stack) = &self.stack else { return Ok(()) };
		let source = stack.source;
		ensure!(
			self.network.is_none(),
			"--network cannot be used with {source}; unset one of them"
		);
		let bundled = truapi_provider::known_networks().collect::<Vec<_>>().join(", ");
		ensure!(
			self.chain_backend != ChainBackend::LightClient,
			"--chain-backend light-client cannot run {source} {}: the light client syncs only the \
			 networks truapi-provider bundles ({bundled}); use --chain-backend rpc",
			stack.value
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests;
