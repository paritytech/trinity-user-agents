//! The descriptor copies behind a running stack: the one the core started
//! from and what each fetch did to it.
//!
//! [`StackSource`] does no I/O. Its caller fetches, stores and tracks time,
//! and [`StackSource`] decides which copy the core runs and what the host is
//! told.

use trinity_stack::{
	Descriptor, FetchRequest, Pointer, Refresh, Served, StackReader, Status, Verified,
};

use super::{Stack, StackError};

/// How old a copy gets before the core fetches the descriptor again.
const REFRESH_MAX_AGE_SECS: u64 = 60 * 60;

/// A fetch of the stack's descriptor for the host to make.
///
/// Send a `GET` to `url`. Do not follow redirects: the descriptor's URL rules
/// hold only for the URL the stack's link names.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct StackFetchRequest {
	/// The descriptor's URL.
	pub url: String,
	/// The `ETag` to send as `If-None-Match`, when the core holds one.
	pub if_none_match: Option<String>,
	/// The largest body to read. Stop reading and answer `Unreachable` once a
	/// body passes it.
	pub max_bytes: u32,
}

/// What a fetch of the stack's descriptor got.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum StackFetchResponse {
	/// `200 OK`.
	Document {
		/// The body as served.
		bytes: Vec<u8>,
		/// The response's `ETag`.
		etag: Option<String>,
	},
	/// `304 Not Modified`.
	NotModified,
	/// Anything else: a transport error, a timeout, a redirect, another
	/// status, or a body larger than `max_bytes`.
	Unreachable {
		/// What went wrong, for the core's log.
		reason: String,
	},
}

/// What the last finished fetch of the stack's descriptor came to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum StackRefreshOutcome {
	/// The published descriptor is the copy the core holds.
	Current,
	/// The descriptor could not be fetched. The core keeps the copy it holds.
	Unreachable,
	/// The published descriptor was refused. The core keeps the copy it holds.
	Refused {
		/// The rule the published descriptor breaks.
		reason: String,
	},
}

/// The stack the core runs, and what the host shows and does about it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct StackStatus {
	/// The verified descriptor the core runs, as plain descriptor JSON with
	/// the fields this core knows. The host's own features read their chains
	/// and services from it, so they run the same stack as the core.
	pub descriptor: String,
	/// The newest copy is past `validUntil`: show the stack as stale and keep
	/// running on it.
	pub stale: bool,
	/// The newest copy is past `expiresAt`: the stack has shut down.
	pub expired: bool,
	/// The core holds a newer copy that changes a chain, the network or the
	/// TLD. It runs that copy from its next start, so build the runtime again
	/// to follow it.
	pub restart_required: bool,
	/// The stack restarted its chains in that newer copy: drop cached chain
	/// data, such as a light client's database, before the next start.
	pub new_generation: bool,
	/// The last finished fetch, or `None` before the first one.
	pub last_refresh: Option<StackRefreshOutcome>,
}

/// One verified copy of the descriptor, and the stack it names.
struct Copy {
	descriptor: Descriptor,
	stack: Stack,
}

impl Copy {
	fn new(verified: Verified, allows_reserved_values: bool) -> Result<Self, StackError> {
		let stack = Stack::from_descriptor(&verified.descriptor, allows_reserved_values)?;
		Ok(Self { descriptor: verified.descriptor, stack })
	}

	/// Whether the core can switch to `newer` while it runs: nothing it built
	/// its services from differs, and the chains did not restart.
	fn runs_as(&self, newer: &Self) -> bool {
		self.stack == newer.stack && self.descriptor.generation == newer.descriptor.generation
	}
}

/// A reader loaded with the build copy and a stored record, and the copy it
/// starts from.
struct Opened {
	reader: StackReader,
	running: Copy,
	first_fetch: Option<FetchRequest>,
	stale: bool,
	expired: bool,
}

impl Opened {
	fn new(
		pointer: &Pointer,
		bundled: &[u8],
		stored: Option<&[u8]>,
		allows_reserved_values: bool,
		now_unix_seconds: i64,
	) -> Result<Self, StackError> {
		let mut reader = StackReader::new(Refresh::MaxAge(REFRESH_MAX_AGE_SECS));
		reader.load(pointer, Some(bundled), stored).map_err(StackError::Descriptor)?;
		let read = reader.get(pointer, now_unix_seconds);
		let current = read.current.expect("a reader loaded with a build copy has a copy");
		Ok(Self {
			running: Copy::new(current.stack, allows_reserved_values)?,
			reader,
			first_fetch: read.fetch,
			stale: current.stale,
			expired: current.expired,
		})
	}
}

/// The stack a host build runs: the copy the core started from, and each
/// newer copy a fetch brings.
pub struct StackSource {
	pointer: Pointer,
	allows_reserved_values: bool,
	reader: StackReader,
	/// The fetch the reader handed out when it was loaded.
	first_fetch: Option<FetchRequest>,
	running: Copy,
	/// A newer copy the core runs from its next start.
	next_start: Option<Copy>,
	stale: bool,
	expired: bool,
	last_refresh: Option<StackRefreshOutcome>,
}

impl StackSource {
	/// Starts from the newer of `bundled`, the signed copy the build ships,
	/// and `stored`, the record the last [`Self::fetched`] returned.
	///
	/// Fails when `bundled` does not verify under the pointer's key or is not
	/// a stack the core can run. A stored record that fails either is ignored.
	pub fn load(
		pointer: Pointer,
		bundled: &[u8],
		stored: Option<&[u8]>,
		allows_reserved_values: bool,
		now_unix_seconds: i64,
	) -> Result<Self, StackError> {
		let open = |stored| {
			Opened::new(&pointer, bundled, stored, allows_reserved_values, now_unix_seconds)
		};
		let opened = match (open(stored), stored) {
			(Err(error), Some(_)) => {
				tracing::warn!("starting from the bundled stack descriptor: {error}");
				open(None)?
			},
			(opened, _) => opened?,
		};
		let Opened { reader, running, first_fetch, stale, expired } = opened;
		Ok(Self {
			pointer,
			allows_reserved_values,
			reader,
			first_fetch,
			running,
			next_start: None,
			stale,
			expired,
			last_refresh: None,
		})
	}

	/// What identifies the stack. The caller stores its record under it.
	pub fn stack_id(&self) -> String {
		self.pointer.stack_id()
	}

	/// The stack the core runs.
	pub fn stack(&self) -> &Stack {
		&self.running.stack
	}

	/// What the host is told about the stack.
	pub fn status(&self) -> StackStatus {
		StackStatus {
			descriptor: serde_json::to_string(&self.running.descriptor)
				.expect("a descriptor always serialises"),
			stale: self.stale,
			expired: self.expired,
			restart_required: self.next_start.is_some(),
			new_generation: self.next_start.as_ref().is_some_and(|next| {
				next.descriptor.generation != self.running.descriptor.generation
			}),
			last_refresh: self.last_refresh.clone(),
		}
	}

	/// The fetch to make now, when the copy is old enough and no fetch is
	/// out. Answer it with [`Self::fetched`].
	pub fn due_fetch(&mut self, now_unix_seconds: i64) -> Option<FetchRequest> {
		self.first_fetch
			.take()
			.or_else(|| self.reader.get(&self.pointer, now_unix_seconds).fetch)
	}

	/// A fetch to make now whatever the copy's age, unless one is out. Answer
	/// it with [`Self::fetched`].
	pub fn forced_fetch(&mut self, now_unix_seconds: i64) -> Option<FetchRequest> {
		self.first_fetch
			.take()
			.or_else(|| self.reader.refresh(&self.pointer, now_unix_seconds))
	}

	/// Takes the answer to a fetch, and returns the record to store when it
	/// changed. The record goes back to [`Self::load`] as `stored`.
	///
	/// A copy the core cannot run is refused like one that breaks a
	/// descriptor rule: it is not stored, so the core never starts from it.
	pub fn fetched(&mut self, served: Served, now_unix_seconds: i64) -> Option<Vec<u8>> {
		let refreshed = match self.reader.fetched(&self.pointer, served, now_unix_seconds) {
			Ok(refreshed) => refreshed,
			Err(error) => {
				self.last_refresh =
					Some(StackRefreshOutcome::Refused { reason: error.to_string() });
				return None;
			},
		};
		let accepted = refreshed.accepted;
		let outcome = match &accepted.status {
			Status::Updated | Status::Unchanged => StackRefreshOutcome::Current,
			Status::Unreachable => StackRefreshOutcome::Unreachable,
			Status::Refused(error) => StackRefreshOutcome::Refused { reason: error.to_string() },
			status @ (Status::Conflict | Status::Rollback) => {
				StackRefreshOutcome::Refused { reason: status.code().to_string() }
			},
		};
		let held = match Copy::new(accepted.stack, self.allows_reserved_values) {
			Ok(held) => held,
			Err(error) => {
				self.last_refresh = Some(match outcome {
					StackRefreshOutcome::Current => {
						StackRefreshOutcome::Refused { reason: error.to_string() }
					},
					outcome => outcome,
				});
				return None;
			},
		};
		self.last_refresh = Some(outcome);
		self.stale = accepted.stale;
		self.expired = accepted.expired;
		if self.running.runs_as(&held) {
			self.running = held;
			self.next_start = None;
		} else {
			self.next_start = Some(held);
		}
		refreshed.stored
	}
}

#[cfg(test)]
mod tests;
