//! How a command's [`NetworkSource`] becomes the network it runs against.

use std::time::Duration;

use anyhow::{Result, anyhow, bail};

use super::{
	Network, NetworkChoice, NetworkSource, Stack, StackArg,
	built_in::{BUILT_IN_REFRESH_TIMEOUT, BuiltInCopy, BuiltInOrigin},
	checks::{require_light_specs, verify_chains},
};
use crate::{
	chain::ChainBackend,
	stack_descriptor::{FETCH_TIMEOUT, load_descriptor},
};

impl NetworkSource {
	/// `--chain-backend` with `--network` or the stack. Fails for `--network`
	/// with a stack, in a message that names where the stack came from.
	pub fn new(
		chain_backend: ChainBackend,
		network: Option<Network>,
		stack: Option<StackArg>,
	) -> Result<Self> {
		let network = match (network, stack) {
			(Some(_), Some(StackArg { source, .. })) => {
				bail!("--network cannot be used with {source}; unset one of them")
			},
			(None, Some(stack)) => NetworkChoice::Stack(stack),
			(network, None) => NetworkChoice::BuiltIn(network.unwrap_or_default()),
		};
		Ok(Self { chain_backend, network })
	}

	/// The network a command runs against, the built-in network or the stack,
	/// once its chains pass the checks. Fails as [`Stack::new`] and
	/// [`verify_chains`] do, for an unreachable or expired `--stack`, and when
	/// the light client is asked for a chain the stack names no light spec
	/// for. A built-in network whose
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
		let (stack, built_in) = match &self.network {
			NetworkChoice::Stack(stack_arg) => {
				let stack = load_descriptor(&stack_arg.value)
					.await
					.and_then(|(pointer, descriptor)| {
						Stack::new(&pointer, descriptor, self.chain_backend)
					})
					.map_err(|error| anyhow!("{}: {error}", stack_arg.source))?;
				(stack, None)
			},
			NetworkChoice::BuiltIn(network) => {
				let (stack, origin) =
					network.resolve_built_in(fetch_timeout, self.chain_backend).await?;
				(stack, Some(origin))
			},
		};
		require_light_specs(&stack)?;
		Ok((stack, built_in))
	}
}

#[cfg(test)]
mod tests;
