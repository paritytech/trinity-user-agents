//! How a command's [`NetworkSource`] becomes the network it runs against.

use std::time::Duration;

use anyhow::{Result, anyhow, ensure};

use super::{
	NetworkSource, STACK_ENV, Stack,
	built_in::{BUILT_IN_REFRESH_TIMEOUT, BuiltInCopy, BuiltInOrigin},
	checks::{refuse_light_client, verify_chains},
};
use crate::{
	chain::ChainBackend,
	stack_descriptor::{FETCH_TIMEOUT, load_descriptor},
};

/// Env vars earlier releases read to route a network, which a stack
/// descriptor now describes.
const REMOVED_ENV: [&str; 6] = [
	"HOST_CLI_PEOPLE_WS",
	"HOST_CLI_ASSET_HUB_WS",
	"HOST_CLI_BULLETIN_WS",
	"HOST_CLI_IDENTITY_BACKEND_BASE",
	"HOST_CLI_DOTNS_POP_CONTROLLER",
	"HOST_CLI_LOCAL_NETWORK_CONFIG",
];

impl NetworkSource {
	/// The network a command runs against, the built-in network or the stack,
	/// once its chains pass the checks. Fails as [`Stack::new`] and
	/// [`verify_chains`] do, for `--network` with a stack, for a removed
	/// routing env var, for an unreachable or expired `--stack`, and when the
	/// light client cannot serve the network. A built-in network whose
	/// published descriptor is unreachable runs on its bundled copy; when that
	/// copy fails a check, the descriptor is fetched once more and the checks
	/// repeat only when the published copy is then in use.
	pub async fn resolve_checked(&self) -> Result<Stack> {
		let (stack, built_in) = self.resolve_routes(BUILT_IN_REFRESH_TIMEOUT).await?;
		let Err(error) = verify_chains(&stack, built_in.as_ref()).await? else {
			return Ok(stack);
		};
		if built_in.map(|origin| origin.copy) != Some(BuiltInCopy::Unreachable) {
			return Err(error);
		}
		tracing::info!("{error}; fetching the published descriptor again");
		let (stack, built_in) = self.resolve_routes(FETCH_TIMEOUT).await?;
		if built_in.as_ref().map(|origin| origin.copy) != Some(BuiltInCopy::Published) {
			return Err(error);
		}
		verify_chains(&stack, built_in.as_ref()).await??;
		Ok(stack)
	}

	/// [`Self::resolve_checked`] without contacting any chain; only a
	/// descriptor is fetched. An unreachable descriptor fails only for
	/// `--stack`.
	pub async fn resolve(&self) -> Result<Stack> {
		Ok(self.resolve_routes(BUILT_IN_REFRESH_TIMEOUT).await?.0)
	}

	/// The network, and for a built-in, which copy of its descriptor is in
	/// use. A built-in stack's published descriptor gets `fetch_timeout`.
	async fn resolve_routes(
		&self,
		fetch_timeout: Duration,
	) -> Result<(Stack, Option<BuiltInOrigin>)> {
		self.ensure_compatible()?;
		ensure_removed_env_unset(|name| std::env::var(name).ok())?;
		let (stack, built_in) = match &self.stack {
			Some(stack_arg) => {
				let stack = load_descriptor(&stack_arg.value)
					.await
					.and_then(|descriptor| Stack::new(descriptor, self.chain_backend))
					.map_err(|error| anyhow!("{}: {error}", stack_arg.source))?;
				(stack, None)
			},
			None => {
				let (stack, origin) = self
					.network
					.unwrap_or_default()
					.resolve_built_in(fetch_timeout, self.chain_backend)
					.await?;
				(stack, Some(origin))
			},
		};
		refuse_light_client(&stack)?;
		Ok((stack, built_in))
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

/// Fails when `env_value` returns a non-blank value for a routing env var
/// earlier releases read, so a harness that still sets one does not run
/// against another network than it describes.
fn ensure_removed_env_unset(env_value: impl Fn(&str) -> Option<String>) -> Result<()> {
	for name in REMOVED_ENV {
		ensure!(
			env_value(name).is_none_or(|value| value.trim().is_empty()),
			"{name} is no longer read; describe the network as a stack descriptor and pass it \
			 with --stack or {STACK_ENV}"
		);
	}
	Ok(())
}

#[cfg(test)]
mod tests;
