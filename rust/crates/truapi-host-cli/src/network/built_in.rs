//! The built-in networks: each stack's bundled descriptor, its refresh from
//! the published copy, and the notice when that copy is in use.

use std::{collections::HashMap, fs, path::Path, sync::LazyLock, time::Duration};

use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::ValueEnum;
use trinity_stack::{Descriptor, Fetched, Pointer, Status, accept};

use super::{Network, Stack};
use crate::{
	chain::ChainAccess,
	platform::atomic_write,
	stack_descriptor::{
		FetchError, describe_pin_change, fetch, light_client_chains, pointer_of, reads_same,
		unix_now,
	},
};

/// How long a command waits for a built-in stack's published descriptor
/// before it runs on the copy the host last ran on, or else the bundled one.
pub const BUILT_IN_REFRESH_TIMEOUT: Duration = Duration::from_secs(3);

/// A known stack this build ships a signed copy of, so a command starts
/// without the published descriptor when it is unreachable.
/// `scripts/update-stacks.sh` refreshes `stacks/<name>.json`.
struct BuiltInStack {
	name: &'static str,
	bundle: &'static [u8],
}

impl BuiltInStack {
	fn pointer(&self) -> Pointer {
		trinity_stack::known_stack(self.name).expect("built-in stack is not a known stack")
	}

	fn open_bundled(&self) -> Descriptor {
		trinity_stack::open(self.bundle, &self.pointer())
			.expect("invalid bundled stack")
			.descriptor
	}
}

const PASEO_NEXT_V2_STACK: BuiltInStack = BuiltInStack {
	name: "paseo-next-v2",
	bundle: include_bytes!("../../../../../stacks/paseo-next-v2.json"),
};

const PREVIEWNET_STACK: BuiltInStack = BuiltInStack {
	name: "previewnet",
	bundle: include_bytes!("../../../../../stacks/previewnet.json"),
};

const PCF_DEVNET_STACK: BuiltInStack = BuiltInStack {
	name: "pcf-devnet",
	bundle: include_bytes!("../../../../../stacks/pcf-devnet.json"),
};

/// Real Polkadot's descriptor, signed by PCF. Not a network the CLI runs on:
/// it lists Polkadot's chains and the RPC nodes that tell a fork of Polkadot
/// from Polkadot itself. Only the bundled copy is read;
/// `scripts/update-stacks.sh` refreshes it with the built-ins.
const POLKADOT_REFERENCE_STACK: BuiltInStack = BuiltInStack {
	name: "polkadot",
	bundle: include_bytes!("../../../../../stacks/polkadot.json"),
};

/// The bundled copy of real Polkadot's descriptor, opened once per process.
pub fn polkadot_reference() -> &'static Descriptor {
	static REFERENCE: LazyLock<Descriptor> =
		LazyLock::new(|| POLKADOT_REFERENCE_STACK.open_bundled());
	&REFERENCE
}

/// The key of the built-in stack published at `stack`, when `stack` is that URL with no key of
/// its own. A built-in stack named by its bare URL is then verified, and is the same stack as
/// under its network name.
pub fn built_in_key(stack: &str) -> Option<String> {
	let unsigned = pointer_of(stack, None)
		.ok()
		.flatten()
		.filter(|pointer| pointer.key().is_none())?;
	Network::value_variants().iter().find_map(|network| {
		let built_in = network.built_in_stack().pointer();
		let published = Pointer::new(built_in.url().as_str(), None).ok()?;
		let key = built_in.key().filter(|_| published.same_stack(&unsigned))?;
		Some(format!("0x{}", hex::encode(key)))
	})
}

/// Fails when `descriptor`, fetched from `pointer`, is a built-in network's
/// stack the CLI cannot run on over either chain backend, so a bundled copy
/// cannot start every command with an error.
pub fn ensure_built_in_runnable(pointer: &Pointer, descriptor: &Descriptor) -> Result<()> {
	let stack_id = pointer.stack_id();
	let is_built_in = Network::value_variants()
		.iter()
		.any(|network| network.built_in_stack().pointer().stack_id() == stack_id);
	if !is_built_in {
		return Ok(());
	}
	let url = pointer.url();
	if let Err(error) = Stack::new(stack_id, descriptor.clone(), &ChainAccess::Rpc) {
		bail!("{url} is a built-in network's stack the CLI cannot run: {error}");
	}
	if let Some(chain) = light_client_chains(descriptor).find(|chain| chain.light_spec.is_none()) {
		let role = &chain.role;
		bail!(
			"{url} is a built-in network's stack the CLI cannot run on the light client: its \
			 descriptor names no lightSpec for {role}"
		);
	}
	Ok(())
}

/// The light spec with `sha256`, as a descriptor writes it, when a built-in
/// network's bundled copy names it, so the light client starts without the
/// spec's server.
pub fn bundled_light_spec(sha256: &str) -> Option<&'static str> {
	static SPECS: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
		Network::value_variants()
			.iter()
			.flat_map(|network| {
				serde_json::from_slice::<HashMap<String, String>>(network.bundled_light_specs())
					.expect("invalid bundled light specs")
			})
			.collect()
	});
	SPECS.get(sha256).map(String::as_str)
}

impl Network {
	fn built_in_stack(self) -> BuiltInStack {
		match self {
			Self::PaseoNextV2 => PASEO_NEXT_V2_STACK,
			Self::Previewnet => PREVIEWNET_STACK,
			Self::PcfDevnet => PCF_DEVNET_STACK,
		}
	}

	/// The light specs the bundled copy names, by `sha256`, as `stack bundle
	/// --light-specs` writes them. `scripts/update-stacks.sh` refreshes them
	/// with the copy.
	fn bundled_light_specs(self) -> &'static [u8] {
		match self {
			Self::PaseoNextV2 => {
				include_bytes!("../../../../../stacks/light-specs/paseo-next-v2.json")
			},
			Self::Previewnet => include_bytes!("../../../../../stacks/light-specs/previewnet.json"),
			Self::PcfDevnet => include_bytes!("../../../../../stacks/light-specs/pcf-devnet.json"),
		}
	}

	/// The bundled copy of this network's stack, connected to over
	/// `chain_backend`, without fetching anything.
	#[cfg(test)]
	pub fn bundled_stack(self, chain_backend: crate::chain::ChainBackend) -> Stack {
		let chain_access = super::testing::test_access(chain_backend);
		let built_in = self.built_in_stack();
		Stack::new(built_in.pointer().stack_id(), built_in.open_bundled(), &chain_access)
			.expect("invalid bundled stack")
	}

	/// The published descriptor when it is newer than the bundled copy,
	/// otherwise the copy this host last ran on or the bundled copy, with
	/// whether the published one is in use. Logs a notice when the published
	/// descriptor changes a field the CLI reads.
	/// Fails when the descriptor in use is expired or is not a stack the CLI
	/// can run on.
	///
	/// A published descriptor that is in use is kept in `copies`. While the
	/// published one is unreachable, that copy stands in for it, so a host
	/// that ran on a stack after a wipe does not go back to the bundled
	/// copy's chains, which are gone.
	pub async fn resolve_built_in(
		self,
		fetch_timeout: Duration,
		chain_access: &ChainAccess,
		copies: Option<&Path>,
	) -> Result<(Stack, BuiltInOrigin)> {
		let built_in = self.built_in_stack();
		let value = self.to_possible_value().expect("network without a --network value");
		let name = value.get_name();
		let pointer = built_in.pointer();
		let url = pointer.url().as_str();
		let fetched = fetch(url, fetch_timeout).await;
		match &fetched {
			Err(cause @ FetchError::Http(_)) => tracing::warn!(
				"the {name} stack descriptor at {url} returned {cause}; not using it"
			),
			Err(cause) => tracing::debug!("{name} stack descriptor is unreachable: {cause}"),
			Ok(_) => {},
		}
		let copy_path = copies.map(|directory| directory.join(format!("{name}.json")));
		let last_run = copy_path.as_deref().and_then(|path| fs::read(path).ok());
		let now = unix_now() as i64;
		let published = fetched.as_deref().ok();
		let refresh = refresh_from_last_run(
			name,
			&pointer,
			built_in.bundle,
			published,
			last_run.as_deref(),
			now,
			chain_access,
		)?;
		if let (Some(published), BuiltInCopy::Published, Some(path)) =
			(published, refresh.copy, &copy_path) &&
			last_run.as_deref() != Some(published) &&
			let Err(error) = atomic_write(path, published)
		{
			tracing::warn!("could not keep the {name} stack descriptor: {error}");
		}
		match &refresh.notice {
			Some(notice) if notice.new_generation => tracing::warn!("{notice}"),
			Some(notice) => tracing::info!("{notice}"),
			None => {},
		}
		let origin = BuiltInOrigin { id: name.to_owned(), url: url.to_owned(), copy: refresh.copy };
		Ok((refresh.stack, origin))
	}
}

/// Which copy of a built-in stack's descriptor a network runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltInCopy {
	/// The published descriptor, also when it equals the bundled copy.
	Published,
	/// The copy this host last ran on, or else the bundled copy, because the
	/// published descriptor could not be fetched; a later fetch may still
	/// return it.
	Unreachable,
	/// The copy this host last ran on, or else the bundled copy, because the
	/// published descriptor was refused, a rollback, a conflict, changed the
	/// TLD or the id, or is not a stack the CLI can run on; fetching it again
	/// returns the same.
	Rejected,
}

/// The descriptor a built-in stack runs on.
pub struct BuiltInOrigin {
	/// The built-in network's `--network` value.
	pub id: String,
	/// Where the published descriptor is fetched from.
	pub url: String,
	pub copy: BuiltInCopy,
}

impl BuiltInOrigin {
	/// What a genesis mismatch on the stack's endpoints adds to its error.
	pub fn restart_hint(&self) -> String {
		let Self { id, url, copy } = self;
		match copy {
			BuiltInCopy::Published => format!(
				"the {id} stack restarted from genesis after the published descriptor in use \
				 ({url}) was published"
			),
			BuiltInCopy::Unreachable => format!(
				"the {id} stack restarted from genesis after the descriptor in use was \
				 published, and the published copy at {url} could not be fetched"
			),
			BuiltInCopy::Rejected => format!(
				"the {id} stack restarted from genesis after the descriptor in use was \
				 published, and the published copy at {url} was rejected, as the warning above \
				 reports"
			),
		}
	}
}

/// The stack a built-in network runs on and the notice to print about it.
struct StackRefresh {
	stack: Stack,
	notice: Option<StackNotice>,
	copy: BuiltInCopy,
}

/// A published descriptor that changes a field the CLI reads.
struct StackNotice {
	id: String,
	bundle_sequence: u32,
	remote_sequence: u32,
	/// The stack restarted its chains from genesis since the bundled copy.
	new_generation: bool,
}

impl std::fmt::Display for StackNotice {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let Self { id, bundle_sequence, remote_sequence, .. } = self;
		if self.new_generation {
			write!(
				formatter,
				"the {id} stack restarted from genesis: its published descriptor (sequence \
				 {remote_sequence}) is in use instead of the copy this truapi-host ships \
				 (sequence {bundle_sequence}), whose chains are gone; a later truapi-host \
				 release ships the new descriptor"
			)
		} else {
			write!(
				formatter,
				"the published {id} stack descriptor (sequence {remote_sequence}) changes the \
				 copy this truapi-host ships (sequence {bundle_sequence}) and is in use; a later \
				 truapi-host release ships it"
			)
		}
	}
}

/// [`refresh_stack`] with `last_run`, the published descriptor the host kept
/// from an earlier run, standing in for `published` while that is
/// unreachable.
///
/// `last_run` goes through the same rules as a fetched copy, so one that no
/// longer verifies leaves the bundled copy in use, with a warning. One older
/// than the bundled copy, as after an update of the CLI, is passed over
/// without one. Whichever copy is in use without `published` is reported as
/// [`BuiltInCopy::Unreachable`]: the published descriptor may still arrive.
///
/// A `published` copy that [`accept`] does not take over `last_run`, such as
/// one with a lower `sequence` or one that does not verify, is not used, with
/// a warning that says why. It and a `published` copy that [`refresh_stack`]
/// refuses leave `last_run` in use, and are reported as
/// [`BuiltInCopy::Rejected`].
fn refresh_from_last_run(
	id: &str,
	pointer: &Pointer,
	bundled: &[u8],
	published: Option<&[u8]>,
	last_run: Option<&[u8]>,
	now_unix_seconds: i64,
	chain_access: &ChainAccess,
) -> Result<StackRefresh> {
	let last_run = last_run.filter(|last_run| {
		let over_bundled =
			accept(pointer, Some(bundled), Fetched::Document(last_run), now_unix_seconds);
		!over_bundled.is_ok_and(|taken| taken.status == Status::Rollback)
	});
	let not_taken = published.zip(last_run).and_then(|(published, last_run)| {
		let taken = accept(pointer, Some(last_run), Fetched::Document(published), now_unix_seconds);
		match taken.map(|taken| taken.status) {
			Ok(Status::Updated | Status::Unchanged) => None,
			Ok(Status::Rollback) => Some("has a lower sequence".to_owned()),
			Ok(Status::Refused(error)) | Err(error) => {
				Some(format!("is refused ({}): {error}", error.code()))
			},
			Ok(status) => Some(format!("is a {}", status.code())),
		}
	});
	if let Some(reason) = &not_taken {
		tracing::warn!(
			"the published {id} stack descriptor {reason}; keeping the one this host last ran on"
		);
	}
	let rolled_back = not_taken.is_some();
	let refresh_from = |document: Option<&[u8]>, name: &str| {
		let fetched = document.map_or(Fetched::Unreachable, Fetched::Document);
		refresh_stack(id, name, pointer, bundled, fetched, now_unix_seconds, chain_access)
	};
	let published_name = format!("published {id} stack descriptor at {}", pointer.url());
	let from_published = match published.filter(|_| !rolled_back) {
		Some(published) => Some(refresh_from(Some(published), &published_name)?),
		None => None,
	};
	let published_in_use = from_published
		.as_ref()
		.is_some_and(|refresh| refresh.copy == BuiltInCopy::Published);
	let mut refresh = match from_published {
		Some(refresh) if published_in_use || last_run.is_none() => refresh,
		_ => refresh_from(last_run, &format!("{id} stack descriptor this host last ran on"))?,
	};
	refresh.copy = match published {
		None => BuiltInCopy::Unreachable,
		Some(_) if published_in_use => BuiltInCopy::Published,
		Some(_) => BuiltInCopy::Rejected,
	};
	Ok(refresh)
}

/// The stack a built-in network runs on: `fetched` when [`accept`] takes it
/// over `bundled`, has the bundled copy's `tld` and `id`, and is a stack the
/// CLI can run on; otherwise `bundled`, with a warning that calls `fetched`
/// the `fetched_name`. Carries a notice when the stack in use differs from
/// the bundled one in a field the CLI reads. Fails when the descriptor in use
/// is past `expiresAt`, and on the light client when `fetched` runs over RPC
/// but names no light spec the light client needs.
fn refresh_stack(
	id: &str,
	fetched_name: &str,
	pointer: &Pointer,
	bundled: &[u8],
	fetched: Fetched,
	now_unix_seconds: i64,
	chain_access: &ChainAccess,
) -> Result<StackRefresh> {
	let bundled_copy = accept(pointer, None, Fetched::Document(bundled), now_unix_seconds)
		.map_err(|error| anyhow!("the bundled {id} stack does not open: {error}"))?;
	let accepted = accept(pointer, Some(bundled), fetched, now_unix_seconds)
		.map_err(|error| anyhow!("the {id} stack has no usable descriptor: {error}"))?;
	let copy = match &accepted.status {
		Status::Updated | Status::Unchanged => BuiltInCopy::Published,
		Status::Unreachable => BuiltInCopy::Unreachable,
		Status::Refused(error) => {
			tracing::warn!(
				"the {fetched_name} is refused ({}): {error}; not using it",
				error.code()
			);
			BuiltInCopy::Rejected
		},
		status => {
			tracing::warn!("the {fetched_name} is a {}; not using it", status.code());
			BuiltInCopy::Rejected
		},
	};
	let (bundled_descriptor, published) =
		(&bundled_copy.stack.descriptor, &accepted.stack.descriptor);
	let notice = (!reads_same(published, bundled_descriptor)).then(|| StackNotice {
		id: id.to_owned(),
		bundle_sequence: bundled_descriptor.sequence,
		remote_sequence: published.sequence,
		new_generation: accepted.new_generation,
	});
	let published_stack: Result<Stack, String> =
		match describe_pin_change(bundled_descriptor, published) {
			Some(reason) => Err(reason),
			None => {
				ensure!(!accepted.expired, "the {id} stack is past expiresAt; it has shut down");
				match Stack::new(
					pointer.stack_id(),
					accepted.stack.descriptor.clone(),
					&ChainAccess::Rpc,
				) {
					Err(error) => Err(format!("cannot run: {error:#}")),
					Ok(stack) => Ok(match chain_access {
						ChainAccess::Rpc => stack,
						// The bundled copy's chains may be gone, so the light
						// client does not fall back to it.
						light_client => {
							Stack::new(pointer.stack_id(), accepted.stack.descriptor, light_client)
								.with_context(|| format!("the {fetched_name}"))?
						},
					}),
				}
			},
		};
	let (stack, stale, copy, notice) = match published_stack {
		Ok(stack) => (stack, accepted.stale, copy, notice),
		Err(reason) => {
			tracing::warn!("the {fetched_name} {reason}; not using it");
			ensure!(!bundled_copy.expired, "the {id} stack is past expiresAt; it has shut down");
			let bundled =
				Stack::new(pointer.stack_id(), bundled_copy.stack.descriptor, chain_access)?;
			(bundled, bundled_copy.stale, BuiltInCopy::Rejected, None)
		},
	};
	if stale {
		tracing::warn!("the {id} stack descriptor in use is past validUntil");
	}
	Ok(StackRefresh { stack, notice, copy })
}

#[cfg(test)]
mod tests;
