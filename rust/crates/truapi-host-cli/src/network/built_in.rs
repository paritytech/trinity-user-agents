//! The built-in networks: each stack's bundled descriptor, its refresh from
//! the published copy, and the notice when that copy is in use.

use std::{sync::LazyLock, time::Duration};

use anyhow::{Result, anyhow, bail, ensure};
use clap::ValueEnum;
use trinity_stack::{Descriptor, Fetched, Pointer, Status, accept};

use super::{Network, Stack};
use crate::{
	chain::ChainBackend,
	stack_descriptor::{FetchError, describe_pin_change, fetch, reads_same, unix_now},
};

/// How long a command waits for a built-in stack's published descriptor
/// before it runs on the bundled copy.
pub const BUILT_IN_REFRESH_TIMEOUT: Duration = Duration::from_secs(3);

/// A stack this build ships a signed copy of, so a command starts without the
/// published descriptor when it is unreachable. `scripts/update-stacks.sh`
/// refreshes `stacks/<name>.json` from the link in `stacks/<name>.link`.
struct BuiltInStack {
	link: &'static str,
	bundle: &'static [u8],
}

impl BuiltInStack {
	fn pointer(&self) -> Pointer {
		Pointer::from_link(self.link.trim()).expect("invalid built-in stack link")
	}

	fn open_bundled(&self) -> Descriptor {
		trinity_stack::open(self.bundle, &self.pointer())
			.expect("invalid bundled stack")
			.descriptor
	}
}

const PASEO_NEXT_V2_STACK: BuiltInStack = BuiltInStack {
	link: include_str!("../../stacks/paseo-next-v2.link"),
	bundle: include_bytes!("../../stacks/paseo-next-v2.json"),
};

const PREVIEWNET_STACK: BuiltInStack = BuiltInStack {
	link: include_str!("../../stacks/previewnet.link"),
	bundle: include_bytes!("../../stacks/previewnet.json"),
};

const PCF_DEVNET_STACK: BuiltInStack = BuiltInStack {
	link: include_str!("../../stacks/pcf-devnet.link"),
	bundle: include_bytes!("../../stacks/pcf-devnet.json"),
};

/// Real Polkadot's descriptor, signed by PCF. Not a network the CLI runs on:
/// it lists Polkadot's chains and the RPC nodes that tell a fork of Polkadot
/// from Polkadot itself. Only the bundled copy is read;
/// `scripts/update-stacks.sh` refreshes it with the built-ins.
const POLKADOT_REFERENCE_STACK: BuiltInStack = BuiltInStack {
	link: include_str!("../../stacks/polkadot.link"),
	bundle: include_bytes!("../../stacks/polkadot.json"),
};

/// The bundled copy of real Polkadot's descriptor, opened once per process.
pub fn polkadot_reference() -> &'static Descriptor {
	static REFERENCE: LazyLock<Descriptor> =
		LazyLock::new(|| POLKADOT_REFERENCE_STACK.open_bundled());
	&REFERENCE
}

/// Fails when `descriptor`, fetched from `pointer`, is a built-in network's
/// stack the CLI cannot run on, so a bundled copy cannot start every command
/// with an error.
pub fn ensure_built_in_runnable(pointer: &Pointer, descriptor: &Descriptor) -> Result<()> {
	let stack_id = pointer.stack_id();
	let is_built_in = Network::value_variants()
		.iter()
		.any(|network| network.built_in_stack().pointer().stack_id() == stack_id);
	if is_built_in && let Err(error) = Stack::new(descriptor.clone(), ChainBackend::Rpc) {
		bail!("{} is a built-in network's stack the CLI cannot run: {error}", pointer.url());
	}
	Ok(())
}

impl Network {
	fn built_in_stack(self) -> BuiltInStack {
		match self {
			Self::PaseoNextV2 => PASEO_NEXT_V2_STACK,
			Self::Previewnet => PREVIEWNET_STACK,
			Self::PcfDevnet => PCF_DEVNET_STACK,
		}
	}

	/// The bundled copy of this network's stack, connected to over
	/// `chain_backend`, without fetching anything.
	#[cfg(test)]
	pub fn bundled_stack(self, chain_backend: ChainBackend) -> Stack {
		Stack::new(self.built_in_stack().open_bundled(), chain_backend)
			.expect("invalid bundled stack")
	}

	/// The published descriptor when it is newer than the bundled copy,
	/// otherwise the bundled copy, with which of the two is in use. Logs a
	/// notice when the published descriptor changes a field the CLI reads.
	/// Fails when the descriptor in use is expired or is not a stack the CLI
	/// can run on.
	pub async fn resolve_built_in(
		self,
		fetch_timeout: Duration,
		chain_backend: ChainBackend,
	) -> Result<(Stack, BuiltInOrigin)> {
		let built_in = self.built_in_stack();
		let value = self.to_possible_value().expect("network without a --network value");
		let name = value.get_name();
		let pointer = built_in.pointer();
		let url = pointer.url().as_str();
		let fetched = fetch(url, fetch_timeout).await;
		match &fetched {
			Err(cause @ FetchError::Http(_)) => tracing::warn!(
				"the {name} stack descriptor at {url} returned {cause}; using the bundled copy"
			),
			Err(cause) => tracing::debug!("{name} stack descriptor is unreachable: {cause}"),
			Ok(_) => {},
		}
		let fetched = fetched.as_deref().map_or(Fetched::Unreachable, Fetched::Document);
		let now = unix_now() as i64;
		let refresh = refresh_stack(name, &pointer, built_in.bundle, fetched, now, chain_backend)?;
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
	/// The bundled copy, because the published descriptor could not be
	/// fetched; a later fetch may still return it.
	Unreachable,
	/// The bundled copy, because the published descriptor was refused, a
	/// rollback, a conflict, changed the TLD or the id, or is not a stack the
	/// CLI can run on; fetching it again returns the same.
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
				"the {id} stack restarted from genesis after the bundled descriptor in use was \
				 built, and the published copy at {url} could not be fetched"
			),
			BuiltInCopy::Rejected => format!(
				"the {id} stack restarted from genesis after the bundled descriptor in use was \
				 built, and the published copy at {url} was rejected, as the warning above reports"
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

/// The stack a built-in network runs on: `fetched` when [`accept`] takes it
/// over `bundled`, has the bundled copy's `tld` and `id`, and is a stack the
/// CLI can run on; otherwise `bundled`, with a warning. Carries a notice when
/// the stack in use differs from the bundled one in a field the CLI reads.
/// Fails when the descriptor in use is past `expiresAt`.
fn refresh_stack(
	id: &str,
	pointer: &Pointer,
	bundled: &[u8],
	fetched: Fetched,
	now_unix_seconds: i64,
	chain_backend: ChainBackend,
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
				"the published {id} stack descriptor is refused ({}): {error}; using the bundled \
				 copy",
				error.code()
			);
			BuiltInCopy::Rejected
		},
		status => {
			tracing::warn!(
				"the published {id} stack descriptor is a {}; using the bundled copy",
				status.code()
			);
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
				Stack::new(accepted.stack.descriptor, chain_backend)
					.map_err(|error| format!("cannot run: {error:#}"))
			},
		};
	let (stack, stale, copy, notice) = match published_stack {
		Ok(stack) => (stack, accepted.stale, copy, notice),
		Err(reason) => {
			tracing::warn!(
				"the published {id} stack descriptor at {} {reason}; using the bundled copy",
				pointer.url()
			);
			ensure!(!bundled_copy.expired, "the {id} stack is past expiresAt; it has shut down");
			let bundled = Stack::new(bundled_copy.stack.descriptor, chain_backend)?;
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
