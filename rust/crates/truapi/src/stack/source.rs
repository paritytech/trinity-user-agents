//! The descriptor copies behind a running stack: the one the core started
//! from and what each fetch did to it.
//!
//! [`StackSource`] does no I/O. Its caller fetches, stores and tracks time,
//! and [`StackSource`] decides which copy the core runs and what the host is
//! told.

use core::time::Duration;

use trinity_stack::{
	Descriptor, FetchRequest, Pointer, Refresh, Served, StackReader, Status, Verified, open,
};

use super::{Stack, StackError};

/// How old a copy gets before the core fetches the descriptor again.
const REFRESH_MAX_AGE_SECS: u64 = 60 * 60;

/// A fetch of the stack's descriptor for the host to make.
///
/// Send a `GET` to `url`. Do not follow redirects: the descriptor's URL rules
/// hold only for the URL the stack's link names.
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct StackFetchRequest {
	/// The descriptor's URL.
	pub url: String,
	/// The `ETag` to send as `If-None-Match`, when the core holds one.
	pub if_none_match: Option<String>,
	/// The largest body to read. Stop reading and fail the fetch once a body
	/// passes it.
	pub max_bytes: u32,
}

/// What a fetch of the stack's descriptor got. Anything but these two is a
/// failed fetch: a transport error, a timeout, a redirect, another status, or
/// a body larger than [`StackFetchRequest::max_bytes`].
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
}

/// What the last finished fetch of the stack's descriptor came to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum StackRefreshOutcome {
	/// The published descriptor is the copy the core holds.
	Current,
	/// The descriptor could not be fetched. The core keeps the copy it holds.
	Unreachable {
		/// Why the fetch failed: the host's error, or the limit of the core's
		/// that the answer broke.
		reason: String,
	},
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
	/// and services from it.
	pub descriptor: String,
	/// The descriptor's `generation`, which changes when the stack restarts
	/// its chains. Key cached chain data by it, such as a light client's
	/// database: data kept under another generation is for chains that are
	/// gone.
	pub generation: String,
	/// The newest copy was past `validUntil` when the core last looked, at
	/// start and after each fetch: show the stack as stale and keep running
	/// on it.
	pub stale: bool,
	/// The newest copy was past `expiresAt` when the core last looked, at
	/// start and after each fetch: the stack has shut down.
	pub expired: bool,
	/// The core stored a newer copy that changes a chain, the network, the
	/// TLD or the generation. It runs that copy from its next start, so build
	/// the runtime again to follow it.
	pub restart_required: bool,
	/// The last finished fetch, or `None` before the first one.
	pub last_refresh: Option<StackRefreshOutcome>,
}

/// One verified copy of the descriptor, and the stack it names.
struct HeldCopy {
	descriptor: Descriptor,
	stack: Stack,
}

impl HeldCopy {
	fn try_new(verified: Verified, allows_reserved_values: bool) -> Result<Self, StackError> {
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
	running: HeldCopy,
	first_fetch: Option<FetchRequest>,
	stale: bool,
	expired: bool,
}

impl Opened {
	fn try_new(
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
			running: HeldCopy::try_new(current.stack, allows_reserved_values)?,
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
	running: HeldCopy,
	/// A newer copy the core runs from its next start, once its record is
	/// stored.
	unstored: Option<HeldCopy>,
	/// A newer copy, stored, that the core runs from its next start.
	next_start: Option<HeldCopy>,
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
			Opened::try_new(&pointer, bundled, stored, allows_reserved_values, now_unix_seconds)
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
			unstored: None,
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
			generation: self.running.descriptor.generation.clone(),
			stale: self.stale,
			expired: self.expired,
			restart_required: self.next_start.is_some(),
			last_refresh: self.last_refresh.clone(),
		}
	}

	/// The fetch to make now, when the copy is old enough and no fetch is
	/// out. Answer it with [`Self::fetched`].
	#[must_use = "a fetch handed out stays pending until `fetched` answers it"]
	pub fn due_fetch(&mut self, now_unix_seconds: i64) -> Option<FetchRequest> {
		self.first_fetch
			.take()
			.or_else(|| self.reader.get(&self.pointer, now_unix_seconds).fetch)
	}

	/// How long until [`Self::due_fetch`] has a fetch to make, or `None` while
	/// one is out.
	#[must_use]
	pub fn next_fetch_in(&self, now_unix_seconds: i64) -> Option<Duration> {
		match self.first_fetch {
			Some(_) => Some(Duration::ZERO),
			None => self.reader.due_in(&self.pointer, now_unix_seconds).map(Duration::from_secs),
		}
	}

	/// A fetch to make now whatever the copy's age, unless one is out. Answer
	/// it with [`Self::fetched`].
	#[must_use = "a fetch handed out stays pending until `fetched` answers it"]
	pub fn forced_fetch(&mut self, now_unix_seconds: i64) -> Option<FetchRequest> {
		self.first_fetch
			.take()
			.or_else(|| self.reader.refresh(&self.pointer, now_unix_seconds))
	}

	/// Takes the answer to a fetch, or why the fetch failed, and returns the
	/// record to store when it changed. Store it, then call
	/// [`Self::record_stored`]. The record goes back to [`Self::load`] as
	/// `stored`.
	///
	/// A copy the core cannot run is refused like one that breaks a
	/// descriptor rule: it is not stored, so the core never starts from it.
	#[must_use = "the record is the copy the next start reads"]
	pub fn fetched(
		&mut self,
		answer: Result<Served, String>,
		now_unix_seconds: i64,
	) -> Option<Vec<u8>> {
		// The reader judges every later fetch against a copy it takes, so one
		// the core cannot run never reaches it.
		let (served, unusable) = match answer {
			Ok(served) => match self.refusal(served) {
				Some(reason) => {
					(Served::Unreachable, Some(StackRefreshOutcome::Refused { reason }))
				},
				None => (served, None),
			},
			Err(reason) => (Served::Unreachable, Some(StackRefreshOutcome::Unreachable { reason })),
		};
		let refreshed = match self.reader.fetched(&self.pointer, served, now_unix_seconds) {
			Ok(refreshed) => refreshed,
			Err(error) => {
				self.last_refresh =
					Some(StackRefreshOutcome::Refused { reason: error.to_string() });
				return None;
			},
		};
		let accepted = refreshed.accepted;
		let outcome = unusable.unwrap_or_else(|| match &accepted.status {
			Status::Updated | Status::Unchanged => StackRefreshOutcome::Current,
			status @ Status::Unreachable => {
				StackRefreshOutcome::Unreachable { reason: status.code().to_string() }
			},
			Status::Refused(error) => StackRefreshOutcome::Refused { reason: error.to_string() },
			status @ (Status::Conflict | Status::Rollback) => {
				StackRefreshOutcome::Refused { reason: status.code().to_string() }
			},
		});
		let held = match HeldCopy::try_new(accepted.stack, self.allows_reserved_values) {
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
			self.unstored = None;
			self.next_start = None;
		} else if refreshed.stored.is_some() {
			self.unstored = Some(held);
		}
		refreshed.stored
	}

	/// Says the record the last [`Self::fetched`] returned is stored. A newer
	/// copy that waits for the next start is reported only from here on: a
	/// host told to restart before the record is stored would start from the
	/// old copy again.
	pub fn record_stored(&mut self) {
		if let Some(copy) = self.unstored.take() {
			self.next_start = Some(copy);
		}
	}

	/// Why the core cannot run a served document that verifies under the
	/// pointer. `None` for one it can run, and for one the reader refuses.
	fn refusal(&self, served: Served) -> Option<String> {
		let Served::Document { bytes, .. } = served else { return None };
		let verified = open(bytes, &self.pointer).ok()?;
		Stack::from_descriptor(&verified.descriptor, self.allows_reserved_values)
			.err()
			.map(|error| error.to_string())
	}
}

#[cfg(test)]
mod tests;
