//! The built-in networks: each stack's bundled descriptor, its refresh from
//! the published copy, and the notice when that copy is in use.

use std::{
	collections::HashMap,
	fs,
	io::{self, Read},
	path::Path,
	sync::LazyLock,
	time::Duration,
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::ValueEnum;
use fs2::FileExt;
use trinity_stack::{Descriptor, Fetched, Pointer, Status, accept};

use super::{Network, Stack};
use crate::{
	chain::ChainAccess,
	platform::atomic_write,
	stack_descriptor::{
		FetchError, MAX_FETCH_BYTES, describe_pin_change, fetch, light_client_chains, pointer_of,
		reads_same, refused, unix_now,
	},
};

/// How long a command waits for a built-in stack's published descriptor
/// before it runs on the copy a host kept, or else the bundled one.
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
	/// otherwise the copy a host kept or the bundled copy, with
	/// what became of the published one. Logs a notice when the published
	/// descriptor changes a field the CLI reads.
	/// Fails when the descriptor in use is expired or is not a stack the CLI
	/// can run on.
	///
	/// A published descriptor that is in use is kept in `copy_directory`,
	/// unless the copy kept there is the same or later. While the published
	/// one is unreachable, that copy stands in for it, so a host that ran on a
	/// stack after a wipe does not go back to the bundled copy's chains, which
	/// are gone. Fails when the kept copy cannot be read.
	pub async fn resolve_built_in(
		self,
		fetch_timeout: Duration,
		chain_access: &ChainAccess,
		copy_directory: Option<&Path>,
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
		let copy_path = copy_directory.map(|directory| directory.join(format!("{name}.json")));
		let now = unix_now() as i64;
		let published = fetched.as_deref().ok();
		let refresh = refresh_with_kept_copy(
			name,
			&pointer,
			built_in.bundle,
			published,
			copy_path.as_deref(),
			now,
			chain_access,
		)?;
		match &refresh.notice {
			Some(notice) if notice.new_generation => tracing::warn!("{notice}"),
			Some(notice) => tracing::info!("{notice}"),
			None => {},
		}
		let origin = BuiltInOrigin { id: name.to_owned(), url: url.to_owned(), copy: refresh.copy };
		Ok((refresh.stack, origin))
	}
}

/// What became of a built-in stack's published descriptor, which decides the
/// copy the network runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltInCopy {
	/// It is in use, also when it equals the bundled copy.
	Published,
	/// It could not be fetched, so the copy a host kept is in use, or else the
	/// bundled copy; a later fetch may still return it.
	Unreachable,
	/// It was refused, a rollback, a conflict, changed the TLD or the id, or
	/// is not a stack the CLI can run on, so the copy a host kept is in use,
	/// or else the bundled copy; fetching it again returns the same.
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

/// The published descriptor a host kept at `path` on an earlier run, or
/// `None` when none was kept.
///
/// A file larger than any descriptor the CLI fetches is not a copy a host
/// kept, so it is passed over with a warning. Fails as [`read_capped`] does:
/// the kept copy is what tells a published copy served stale from a current
/// one, so starting without it, from the published or the bundled copy, could
/// take the host back to chains a wipe replaced.
fn read_kept_copy(path: &Path) -> Result<Option<Vec<u8>>> {
	let copy = read_capped(path)?;
	if copy.as_ref().is_some_and(|copy| copy.len() > MAX_FETCH_BYTES) {
		let path = path.display();
		tracing::warn!(
			"the stack descriptor kept at {path} is larger than {MAX_FETCH_BYTES} bytes; not using it"
		);
		return Ok(None);
	}
	Ok(copy)
}

/// The file at `path`, read up to one byte past [`MAX_FETCH_BYTES`], or
/// `None` when there is none. Fails when it cannot be read, with an error
/// that says how to recover.
fn read_capped(path: &Path) -> Result<Option<Vec<u8>>> {
	let context = || {
		format!(
			"could not read the stack descriptor kept at {}; fix its permissions, or remove it \
			 to start from the published or the bundled copy",
			path.display()
		)
	};
	let file = match fs::File::open(path) {
		Ok(file) => file,
		Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
		Err(error) => return Err(error).with_context(context),
	};
	let mut copy = Vec::new();
	let limit = u64::try_from(MAX_FETCH_BYTES + 1).expect("the fetch limit fits in a u64");
	file.take(limit).read_to_end(&mut copy).with_context(context)?;
	Ok(Some(copy))
}

/// Keeps `published`, the copy in use, at `path` for a run that cannot fetch
/// it, when [`accept`] takes it over the copy kept there. The same copy, or a
/// later one that another host starting at the same time kept, stays.
///
/// Keeping is best effort: this run has its descriptor, and a later one keeps
/// it again. A failure is a warning, and a lock another host holds, as one
/// stopped in a debugger can, is not waited for.
fn keep_copy(path: &Path, pointer: &Pointer, published: &[u8], now_unix_seconds: i64) {
	if let Err(error) = replace_older_copy(path, pointer, published, now_unix_seconds) {
		let path = path.display();
		tracing::warn!("could not keep the stack descriptor at {path}: {error:#}");
	}
}

/// [`keep_copy`] under a lock on `<path>.lock`, which it holds from its own
/// read of the kept copy to the write, so no other host writes in between.
/// Keeps nothing when another host holds the lock.
fn replace_older_copy(
	path: &Path,
	pointer: &Pointer,
	published: &[u8],
	now_unix_seconds: i64,
) -> Result<()> {
	let lock_path = path.with_extension("json.lock");
	if let Some(directory) = lock_path.parent() {
		fs::create_dir_all(directory)
			.with_context(|| format!("could not create {}", directory.display()))?;
	}
	let lock = fs::OpenOptions::new()
		.create(true)
		.truncate(false)
		.write(true)
		.open(&lock_path)
		.with_context(|| format!("could not open {}", lock_path.display()))?;
	if let Err(error) = lock.try_lock_exclusive() {
		if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
			let lock_path = lock_path.display();
			tracing::info!(
				"another truapi-host holds {lock_path}; not keeping the stack descriptor this run"
			);
			return Ok(());
		}
		return Err(error).with_context(|| format!("could not lock {}", lock_path.display()));
	}
	let kept = read_capped(path)?;
	let newer = accept(pointer, kept.as_deref(), Fetched::Document(published), now_unix_seconds)
		.is_ok_and(|taken| taken.status == Status::Updated);
	if newer {
		atomic_write(path, published)?;
	}
	Ok(())
}

/// Whether `last_run` is older than `bundled`, as after an update of the CLI
/// that ships a later descriptor than the one the host kept.
fn is_older_than_bundled(
	pointer: &Pointer,
	bundled: &[u8],
	last_run: &[u8],
	now_unix_seconds: i64,
) -> bool {
	accept(pointer, Some(bundled), Fetched::Document(last_run), now_unix_seconds)
		.is_ok_and(|taken| taken.status == Status::Rollback)
}

/// How a warning says why a copy [`accept`] gave `status` is not in use, or
/// `None` when it is, or when there was no copy to take.
fn not_taken(status: &Status) -> Option<String> {
	match status {
		Status::Updated | Status::Unchanged | Status::Unreachable => None,
		Status::Conflict | Status::Rollback => Some(format!("is a {}", status.code())),
		Status::Refused(error) => Some(format!("is {}", refused(error.clone()))),
	}
}

/// [`refresh_from_last_run`] with the copy kept at `copy_path` as `last_run`,
/// which then keeps `published` there when it is in use. Without `copy_path`
/// nothing is read or kept. Fails as [`read_kept_copy`] does.
fn refresh_with_kept_copy(
	id: &str,
	pointer: &Pointer,
	bundled: &[u8],
	published: Option<&[u8]>,
	copy_path: Option<&Path>,
	now_unix_seconds: i64,
	chain_access: &ChainAccess,
) -> Result<StackRefresh> {
	let last_run = copy_path.map(read_kept_copy).transpose()?.flatten();
	let copies = Copies { bundled, published, last_run: last_run.as_deref() };
	let refresh = refresh_from_last_run(id, pointer, &copies, now_unix_seconds, chain_access)?;
	if let (Some(path), Some(published), BuiltInCopy::Published) =
		(copy_path, published, refresh.copy)
	{
		keep_copy(path, pointer, published, now_unix_seconds);
	}
	Ok(refresh)
}

/// The copies of a built-in stack's descriptor a refresh chooses between.
struct Copies<'a> {
	/// The copy this build ships.
	bundled: &'a [u8],
	/// The published copy, when it could be fetched.
	published: Option<&'a [u8]>,
	/// The published copy the host kept from an earlier run, when it kept one.
	last_run: Option<&'a [u8]>,
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
	copies: &Copies<'_>,
	now_unix_seconds: i64,
	chain_access: &ChainAccess,
) -> Result<StackRefresh> {
	let &Copies { bundled, published, last_run } = copies;
	let last_run = last_run
		.filter(|last_run| !is_older_than_bundled(pointer, bundled, last_run, now_unix_seconds));
	let refresh_from = |document: Option<&[u8]>, name: &str| {
		let fetched = document.map_or(Fetched::Unreachable, Fetched::Document);
		refresh_stack(id, name, pointer, bundled, fetched, now_unix_seconds, chain_access)
	};
	let from_last_run = |copy: BuiltInCopy| {
		let name = format!("{id} stack descriptor a host kept");
		Ok(StackRefresh { copy, ..refresh_from(last_run, &name)? })
	};
	let Some(published) = published else {
		return from_last_run(BuiltInCopy::Unreachable);
	};
	let over_last_run = last_run.and_then(|last_run| {
		match accept(pointer, Some(last_run), Fetched::Document(published), now_unix_seconds) {
			Ok(taken) => not_taken(&taken.status),
			Err(error) => Some(format!("is {}", refused(error))),
		}
	});
	if let Some(reason) = over_last_run {
		tracing::warn!(
			"the published {id} stack descriptor {reason}; using the copy a host kept instead"
		);
		return from_last_run(BuiltInCopy::Rejected);
	}
	let name = format!("published {id} stack descriptor at {}", pointer.url());
	let refresh = refresh_from(Some(published), &name)?;
	if refresh.copy == BuiltInCopy::Published || last_run.is_none() {
		return Ok(refresh);
	}
	from_last_run(BuiltInCopy::Rejected)
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
	let copy = match not_taken(&accepted.status) {
		Some(reason) => {
			tracing::warn!("the {fetched_name} {reason}; not using it");
			BuiltInCopy::Rejected
		},
		None if accepted.status == Status::Unreachable => BuiltInCopy::Unreachable,
		None => BuiltInCopy::Published,
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
