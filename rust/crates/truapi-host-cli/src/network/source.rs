//! How a command's [`NetworkSource`] becomes the network it runs against.

use std::{path::PathBuf, time::Duration};

use anyhow::{Result, anyhow, bail};

use super::{
	Network, NetworkChoice, NetworkSource, Stack, StackArg,
	built_in::{BUILT_IN_REFRESH_TIMEOUT, BuiltInCopy, BuiltInOrigin, built_in_key},
	checks::verify_chains,
};
use crate::{
	chain::ChainAccess,
	stack_descriptor::{FETCH_TIMEOUT, load_descriptor},
};

impl NetworkSource {
	/// `--chain-backend` with `--network` or the stack. Fails for `--network`
	/// with a stack, in a message that names where the stack came from.
	///
	/// `stack_copies` is a directory to keep the published descriptor a host
	/// takes for a built-in network, which it then starts from while the
	/// published one is unreachable. Without one it starts from the bundled
	/// copy.
	pub fn new(
		chain_access: ChainAccess,
		network: Option<Network>,
		stack: Option<StackArg>,
		stack_copies: Option<PathBuf>,
	) -> Result<Self> {
		let network = match (network, stack) {
			(Some(_), Some(StackArg { source, .. })) => {
				bail!("--network cannot be used with {source}; unset one of them")
			},
			(None, Some(stack)) => NetworkChoice::Stack(stack),
			(network, None) => NetworkChoice::BuiltIn(network.unwrap_or_default()),
		};
		Ok(Self { chain_access, network, stack_copies })
	}

	/// The network a command runs against, the built-in network or the stack,
	/// once its chains pass the checks. Fails as [`Stack::new`] and
	/// [`verify_chains`] do, for an unreachable or expired `--stack`, and when
	/// the light client is asked for a chain the stack names no light spec
	/// for. A built-in network whose published descriptor is unreachable runs
	/// on the copy a host kept, or else its bundled copy; when that
	/// copy fails a check, the descriptor is fetched once more and the checks
	/// repeat only when the published copy is then in use.
	pub async fn resolve_checked(&self) -> Result<Stack> {
		checked(|fetch_timeout| self.resolve_routes(fetch_timeout)).await
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
				let key = built_in_key(&stack_arg.value);
				let stack = load_descriptor(&stack_arg.value, key.as_deref())
					.await
					.and_then(|loaded| {
						let stack =
							Stack::new(loaded.stack_id, loaded.descriptor, &self.chain_access)?;
						Ok(if loaded.keyed { stack } else { stack.without_key() })
					})
					.map_err(|error| anyhow!("{}: {error}", stack_arg.source))?;
				(stack, None)
			},
			NetworkChoice::BuiltIn(network) => {
				let (stack, origin) = network
					.resolve_built_in(
						fetch_timeout,
						&self.chain_access,
						self.stack_copies.as_deref(),
					)
					.await?;
				(stack, Some(origin))
			},
		};
		Ok((stack, built_in))
	}
}

/// The stack `resolve` gives once its chains pass the checks, as
/// [`NetworkSource::resolve_checked`] describes. `resolve` is given the
/// timeout for a built-in's published descriptor.
async fn checked<Resolved>(resolve: impl Fn(Duration) -> Resolved) -> Result<Stack>
where
	Resolved: Future<Output = Result<(Stack, Option<BuiltInOrigin>)>>,
{
	let (stack, built_in) = resolve(BUILT_IN_REFRESH_TIMEOUT).await?;
	let Err(error) = verify_chains(&stack, built_in.as_ref()).await? else {
		return Ok(stack);
	};
	if built_in.map(|origin| origin.copy) != Some(BuiltInCopy::Unreachable) {
		return Err(error);
	}
	tracing::info!("{error}; fetching the published descriptor again");
	let (stack, built_in) = resolve(FETCH_TIMEOUT).await?;
	if built_in.as_ref().map(|origin| origin.copy) != Some(BuiltInCopy::Published) {
		return Err(error);
	}
	verify_chains(&stack, built_in.as_ref()).await??;
	Ok(stack)
}

#[cfg(test)]
mod tests;
