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
	platform::{ReservedValuesPolicy, StackRefresh, StackRefreshOutcome, StackStatus},
	time::{RealTimer, SystemClock},
};

use super::{
	DescriptorRefresh, Network, NetworkChoice, NetworkSource, PRE_START_FETCH_TIMEOUT, Stack,
	StackArg, built_in::built_in_key, checks::verify_chains, host::CliStackHost,
	records::StackRecordFile, stack::state_directory_of,
};
use crate::{
	chain::ChainAccess,
	stack_descriptor::{load_descriptor, pointer_of, stack_id},
};

/// A stack published at a URL: its pointer, and for a built-in network the
/// copy of its descriptor this build ships.
struct Published {
	pointer: Pointer,
	shipped: Option<&'static [u8]>,
}

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
			refresh.host.warn_if_refused(&status);
			return Err(error);
		}
		let stack = self.open().await?;
		verify_chains(&stack).await??;
		Ok(stack.kept_current())
	}

	/// [`Self::resolve_checked`] without contacting any chain. The descriptor
	/// is fetched first when the core holds no copy yet, or when the copy it
	/// holds is due for a fetch, as [`Self::open_published`] says. Fails when
	/// the stack is past `expiresAt`, when it is not one the CLI can run, and
	/// for a `--stack` that cannot be read.
	pub async fn resolve(&self) -> Result<Stack> {
		Ok(self.open().await?.kept_current())
	}

	/// The stack, with nothing refreshing it yet: a refresh started now could
	/// have its fetch out when [`Self::resolve_checked`] asks for one.
	async fn open(&self) -> Result<Stack> {
		match &self.network {
			NetworkChoice::BuiltIn(network) => {
				let (link, shipped) = network.link_and_bundle();
				let pointer =
					Pointer::from_link(&link).expect("a built-in network's link is valid");
				self.open_published(&Published { pointer, shipped: Some(shipped) }).await
			},
			NetworkChoice::Stack(stack_arg) => self
				.open_stack(&stack_arg.value)
				.await
				.map_err(|error| anyhow!("{}: {error:#}", stack_arg.source)),
		}
	}

	/// A `--stack` value: a built-in network's link, bare URL or keyless
	/// link is that network, state included, fetched from the URL given; any
	/// other link or URL is imported, and anything else is a local
	/// descriptor file.
	async fn open_stack(&self, value: &str) -> Result<Stack> {
		let key = built_in_key(value);
		let Some(pointer) = pointer_of(value, key.as_deref())? else {
			return self.local(value).await;
		};
		let shipped =
			Network::of_stack(&pointer.stack_id()).map(|network| network.link_and_bundle().1);
		self.open_published(&Published { pointer, shipped }).await
	}

	/// The stack `published` names, run by the core from the copy it holds.
	///
	/// A command can exit before a background fetch is stored, so the copy is
	/// fetched first when it is due: last fetched over an hour ago, or never,
	/// as a shipped copy is. A copy the CLI cannot run, such as one past
	/// `expiresAt` or one without the light specs this run needs, is fetched
	/// once more unless this run fetched already, since its operator may have
	/// published a fix. Each of these fetches waits at most
	/// [`PRE_START_FETCH_TIMEOUT`] for the server, and a newer copy is run at
	/// once; without one the command runs on, or fails on, the copy it holds.
	async fn open_published(&self, published: &Published) -> Result<Stack> {
		let mut refresh = self.load(published).await?;
		// An imported stack's first copy was just fetched.
		let mut fetched = refresh.status().last_refresh.is_some();
		let mut last_fetch = None;
		if !fetched && refresh.fetch_is_due() {
			let (reloaded, status) = self.fetched(refresh, published).await?;
			(refresh, last_fetch, fetched) = (reloaded, Some(status), true);
		}
		let opened = match self.running(refresh, published.pointer.clone()) {
			Err(error) if !fetched => {
				tracing::info!("{error:#}; fetching the stack's descriptor again");
				let (refresh, status) =
					self.fetched(self.load(published).await?, published).await?;
				last_fetch = Some(status);
				self.running(refresh, published.pointer.clone())
			},
			opened => opened,
		};
		// A stack that runs reports its status once its refresh starts.
		if opened.is_err() &&
			let Some(status) = &last_fetch
		{
			self.host(published.pointer.clone()).warn_if_refused(status);
		}
		opened
	}

	/// The core's refresh of `published`, from the copy it holds. An imported
	/// stack the CLI keeps no copy of is fetched first.
	async fn load(&self, published: &Published) -> Result<StackRefresh> {
		let Published { pointer, shipped } = published;
		let (link, records) = (pointer.to_link(DEFAULT_LINK_BASE), self.records(pointer));
		let refresh = match shipped {
			Some(shipped) => {
				StackRefresh::load(
					&link,
					shipped,
					RESERVED_VALUES,
					Box::new(records),
					Arc::new(RealTimer),
					Box::new(SystemClock),
				)
				.await?
			},
			None => {
				StackRefresh::import(
					&link,
					RESERVED_VALUES,
					Box::new(records),
					&self.host(pointer.clone()),
					Arc::new(RealTimer),
					Box::new(SystemClock),
				)
				.await?
			},
		};
		Ok(refresh)
	}

	/// `refresh` after one fetch of its descriptor that waits at most
	/// [`PRE_START_FETCH_TIMEOUT`] for the server, loaded again when the fetch
	/// stored a copy that changes the stack, with the status the fetch left.
	async fn fetched(
		&self,
		refresh: StackRefresh,
		published: &Published,
	) -> Result<(StackRefresh, StackStatus)> {
		let url = published.pointer.url();
		tracing::info!("fetching the stack descriptor at {url} before the command runs");
		let host = self.host(published.pointer.clone()).within(PRE_START_FETCH_TIMEOUT);
		let status = refresh.fetch_now(&host).await;
		if let Some(StackRefreshOutcome::Unreachable { reason }) = &status.last_refresh {
			tracing::info!(
				"the stack descriptor could not be fetched ({reason}); using the copy held"
			);
		}
		if !status.restart_required {
			return Ok((refresh, status));
		}
		drop(refresh);
		Ok((self.load(published).await?, status))
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
