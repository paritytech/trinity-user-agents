//! How a command's [`NetworkSource`] becomes the network it runs against.
//!
//! The core resolves every stack: a built-in network from the copy this build
//! ships, any other link or URL from the copy kept for it or else by fetching
//! it, and a local descriptor file as it reads. The CLI keeps the record of
//! each stack's newest copy in the stack's state directory.

use std::{path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow, bail, ensure};
use trinity_stack::{DEFAULT_LINK_BASE, Pointer};
use truapi::{
	platform::{ReservedValuesPolicy, StackRefresh},
	time::{RealTimer, SystemClock},
};

use super::{
	DescriptorRefresh, Network, NetworkChoice, NetworkSource, Stack, StackArg,
	built_in::built_in_key, checks::verify_chains, host::CliStackHost, records::StackRecordFile,
	stack::state_directory_of,
};
use crate::{
	chain::ChainAccess,
	stack_descriptor::{load_descriptor, pointer_of, stack_id},
};

/// The CLI allows Polkadot's reserved values on every stack, since it runs
/// Polkadot forks for testing and checks itself that none is Polkadot.
const RESERVED_VALUES: ReservedValuesPolicy = ReservedValuesPolicy::ALLOWED;

impl NetworkSource {
	/// `--chain-backend` with `--network` or the stack, keeping each stack's
	/// state under `state_base`, `<base-path>/v2`. Fails for `--network` with
	/// a stack, in a message that names where the stack came from.
	pub fn new(
		chain_access: ChainAccess,
		network: Option<Network>,
		stack: Option<StackArg>,
		state_base: PathBuf,
	) -> Result<Self> {
		let network = match (network, stack) {
			(Some(_), Some(StackArg { source, .. })) => {
				bail!("--network cannot be used with {source}; unset one of them")
			},
			(None, Some(stack)) => NetworkChoice::Stack(stack),
			(network, None) => NetworkChoice::BuiltIn(network.unwrap_or_default()),
		};
		Ok(Self { chain_access, network, state_base })
	}

	/// The network a command runs against once its chains pass the checks.
	/// When an endpoint or light spec fails them, the descriptor is fetched
	/// again, since its operator may have published the chains' new genesis
	/// hashes, and the checks repeat once on a newer copy. Fails as
	/// [`Self::resolve`] and [`verify_chains`] do.
	pub async fn resolve_checked(&self) -> Result<Stack> {
		let stack = self.open().await?;
		let Err(error) = verify_chains(&stack).await? else {
			return Ok(stack.kept_current());
		};
		let Some(refresh) = &stack.refresh else {
			return Err(error);
		};
		tracing::info!("{error}; fetching the stack's descriptor again");
		let held = refresh.descriptor();
		let status = refresh.fetch_now().await;
		if !status.restart_required && refresh.descriptor() == held {
			return Err(error);
		}
		let stack = self.open().await?;
		verify_chains(&stack).await??;
		Ok(stack.kept_current())
	}

	/// [`Self::resolve_checked`] without contacting any chain. A stack that
	/// is not built in is fetched only when its state directory holds no
	/// copy yet. Fails when the stack is past `expiresAt`, when it is not one
	/// the CLI can run, and for a `--stack` that cannot be read.
	pub async fn resolve(&self) -> Result<Stack> {
		Ok(self.open().await?.kept_current())
	}

	/// The stack, with nothing refreshing it yet: a refresh started now could
	/// have its fetch out when [`Self::resolve_checked`] asks for one.
	async fn open(&self) -> Result<Stack> {
		match &self.network {
			NetworkChoice::BuiltIn(network) => self.built_in(*network).await,
			NetworkChoice::Stack(stack_arg) => self
				.stack(&stack_arg.value)
				.await
				.map_err(|error| anyhow!("{}: {error:#}", stack_arg.source)),
		}
	}

	/// A `--stack` value: a built-in network's link, bare URL or keyless
	/// link is that network, state included; any other link or URL is
	/// imported, and anything else is a local descriptor file.
	async fn stack(&self, value: &str) -> Result<Stack> {
		let key = built_in_key(value);
		let Some(pointer) = pointer_of(value, key.as_deref())? else {
			return self.local(value).await;
		};
		match Network::of_stack(&pointer.stack_id()) {
			Some(network) => self.built_in(network).await,
			None => self.import(pointer).await,
		}
	}

	async fn built_in(&self, network: Network) -> Result<Stack> {
		let (link, bundled) = network.link_and_bundle();
		let pointer = Pointer::from_link(&link).expect("a built-in network's link is valid");
		let refresh = StackRefresh::load(
			&link,
			bundled,
			RESERVED_VALUES,
			Box::new(self.records(&pointer)),
			Arc::new(RealTimer),
			Box::new(SystemClock),
		)
		.await?;
		self.running(refresh, pointer)
	}

	async fn import(&self, pointer: Pointer) -> Result<Stack> {
		let host = self.host(pointer.clone());
		let refresh = StackRefresh::import(
			&pointer.to_link(DEFAULT_LINK_BASE),
			RESERVED_VALUES,
			Box::new(self.records(&pointer)),
			&host,
			Arc::new(RealTimer),
			Box::new(SystemClock),
		)
		.await?;
		self.running(refresh, pointer)
	}

	/// A local descriptor file, read as it is and never refreshed.
	async fn local(&self, path: &str) -> Result<Stack> {
		let id = stack_id(path, None)?;
		let descriptor = load_descriptor(path, None).await?;
		let core = truapi::platform::Stack::local(id, &descriptor, &RESERVED_VALUES)?;
		Stack::new(core.without_relay(), None, descriptor, &self.chain_access)
	}

	fn running(&self, refresh: StackRefresh, pointer: Pointer) -> Result<Stack> {
		let status = refresh.status();
		let url = pointer.url();
		ensure!(!status.expired, "the stack at {url} is past expiresAt; it has shut down");
		if status.stale {
			tracing::warn!(
				"the stack descriptor at {url} in use is past validUntil; its operator should \
				 republish"
			);
		}
		let (core, descriptor) = (refresh.stack().without_relay(), refresh.descriptor());
		let refresh = DescriptorRefresh { refresh, host: self.host(pointer) };
		Stack::new(core, Some(refresh), descriptor, &self.chain_access)
	}

	fn records(&self, pointer: &Pointer) -> StackRecordFile {
		StackRecordFile::new(self.state_base.join(state_directory_of(&pointer.stack_id())))
	}

	fn host(&self, pointer: Pointer) -> CliStackHost {
		let on_light_client = matches!(self.chain_access, ChainAccess::LightClient { .. });
		CliStackHost::new(pointer, on_light_client)
	}
}

#[cfg(test)]
mod tests;
