//! The built-in networks: each stack's bundled descriptor, its refresh from
//! the published copy, and the notice when that copy is in use.

use std::time::Duration;

use anyhow::{Result, anyhow, ensure};
use trinity_stack::{Descriptor, Fetched, Pointer, Status, accept};

use super::{Network, NetworkConfig, stack::stack_config};
use crate::{
	chain::ChainBackend,
	stack_descriptor::{FetchError, StackProjection, fetch, unix_now},
};

/// How long a built-in stack's published descriptor may take before the
/// bundled copy is used instead.
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

	fn bundled(&self) -> Descriptor {
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

/// The bundled copy of real Polkadot's descriptor.
pub fn polkadot_reference() -> Descriptor {
	POLKADOT_REFERENCE_STACK.bundled()
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
	pub fn config(self, chain_backend: ChainBackend) -> NetworkConfig {
		stack_config(&self.built_in_stack().bundled(), self.id().to_owned(), chain_backend)
			.expect("invalid bundled stack")
	}

	/// The published descriptor when it is newer than the bundled copy,
	/// otherwise the bundled copy, with which of the two is in use. Logs a
	/// notice when the published descriptor changes a field the CLI reads.
	/// Fails when the descriptor in use is expired or lacks a role.
	pub async fn resolve_built_in(
		self,
		fetch_timeout: Duration,
		chain_backend: ChainBackend,
	) -> Result<(NetworkConfig, BuiltInOrigin)> {
		let built_in = self.built_in_stack();
		let id = self.id();
		let pointer = built_in.pointer();
		let url = pointer.url().as_str();
		let fetched = fetch(url, fetch_timeout).await;
		match &fetched {
			Err(cause @ FetchError::Http(_)) => tracing::warn!(
				"the {id} stack descriptor at {url} returned {cause}; using the bundled copy"
			),
			Err(cause) => tracing::debug!("{id} stack descriptor is unreachable: {cause}"),
			Ok(_) => {},
		}
		let fetched = fetched.as_deref().map_or(Fetched::Unreachable, Fetched::Document);
		let now = unix_now() as i64;
		let refresh = refresh_stack(id, &pointer, built_in.bundle, fetched, now)?;
		match &refresh.notice {
			Some(notice) if notice.new_generation => tracing::warn!("{notice}"),
			Some(notice) => tracing::info!("{notice}"),
			None => {},
		}
		let origin = BuiltInOrigin { id, url: url.to_owned(), copy: refresh.copy };
		let config = stack_config(&refresh.descriptor, id.to_owned(), chain_backend)?;
		Ok((config, origin))
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
	/// rollback, a conflict, or changed the TLD; fetching it again returns the
	/// same.
	Rejected,
}

/// The descriptor a built-in stack runs on.
pub struct BuiltInOrigin {
	/// The built-in network's product namespace.
	pub id: &'static str,
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

/// The descriptor a built-in stack runs on and the notice to print about it.
struct StackRefresh {
	descriptor: Descriptor,
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

/// The descriptor a built-in stack runs on: `fetched` when [`accept`] takes it
/// over `bundled` and it has the bundled copy's `tld`, otherwise `bundled`,
/// with a notice when the descriptor in use changes a field the CLI reads.
/// Fails when that descriptor is past `expiresAt`.
fn refresh_stack(
	id: &str,
	pointer: &Pointer,
	bundled: &[u8],
	fetched: Fetched,
	now_unix_seconds: i64,
) -> Result<StackRefresh> {
	let bundled_copy = accept(pointer, None, Fetched::Document(bundled), now_unix_seconds)
		.map_err(|error| anyhow!("the bundled {id} stack does not open: {error}"))?;
	let accepted = accept(pointer, Some(bundled), fetched, now_unix_seconds)
		.map_err(|error| anyhow!("the {id} stack has no usable descriptor: {error}"))?;
	match &accepted.status {
		Status::Updated | Status::Unchanged | Status::Unreachable => {},
		Status::Refused(error) => tracing::warn!(
			"the published {id} stack descriptor is refused ({}): {error}; using the bundled copy",
			error.code()
		),
		status => tracing::warn!(
			"the published {id} stack descriptor is a {}; using the bundled copy",
			status.code()
		),
	}
	let bundled_descriptor = &bundled_copy.stack.descriptor;
	let is_same_tld = accepted.stack.descriptor.tld == bundled_descriptor.tld;
	if !is_same_tld {
		tracing::warn!(
			"the published {id} stack descriptor changes the TLD from {} to {}, which would \
			 change every account derived under it; using the bundled copy",
			bundled_descriptor.tld,
			accepted.stack.descriptor.tld
		);
	}
	let is_projection_change = is_same_tld &&
		StackProjection::of(&accepted.stack.descriptor) !=
			StackProjection::of(bundled_descriptor);
	let notice = is_projection_change.then(|| StackNotice {
		id: id.to_owned(),
		bundle_sequence: bundled_descriptor.sequence,
		remote_sequence: accepted.stack.descriptor.sequence,
		new_generation: accepted.new_generation,
	});
	let (accepted, copy) = match (is_same_tld, &accepted.status) {
		(true, Status::Updated | Status::Unchanged) => (accepted, BuiltInCopy::Published),
		(true, Status::Unreachable) => (accepted, BuiltInCopy::Unreachable),
		(true, _) => (accepted, BuiltInCopy::Rejected),
		(false, _) => (bundled_copy, BuiltInCopy::Rejected),
	};
	ensure!(!accepted.expired, "the {id} stack is past expiresAt; it has shut down");
	if accepted.stale {
		tracing::warn!("the {id} stack descriptor in use is past validUntil");
	}
	Ok(StackRefresh { descriptor: accepted.stack.descriptor, notice, copy })
}

#[cfg(test)]
mod tests;
