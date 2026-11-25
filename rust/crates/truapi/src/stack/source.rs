//! The descriptor copies behind a running stack: the one the core started
//! from and what each fetch did to it.
//!
//! [`StackSource`] does no I/O. Its caller fetches, stores and tracks time,
//! and [`StackSource`] decides which copy the core runs and what the host is
//! told.

use core::time::Duration;

use trinity_stack::{FetchRequest, Pointer, Refresh, Served, StackReader, Status, open};

use super::{
	ReplaceCheck, Stack, StackError,
	held::{CopyRules, HeldCopy},
};
use crate::chain_runtime::{GenesisHashes, encode_hex};

/// How old a copy gets before the core fetches the descriptor again.
const REFRESH_MAX_AGE_SECS: u64 = 60 * 60;

/// How old a copy gets before the next fetch while one of its chains gives a
/// different genesis hash from the one it names.
///
/// Chains that restart from genesis get new hashes, which the operator
/// publishes. Until the core has them the chain is out of use.
const MISMATCH_REFRESH_MAX_AGE_SECS: u64 = 5 * 60;

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
	/// The published descriptor is newer, but the core could not store it.
	/// The next start would not read it, so the core asks for no restart and
	/// stores it again after the next fetch.
	NotStored {
		/// Why the store failed.
		reason: String,
	},
}

/// A difference between the stack's descriptor and what one of its chains answered.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum StackMismatch {
	/// The chain's last connection reached one with a different genesis hash
	/// from the descriptor's, as happens after a chain restarts from genesis.
	/// The core refused that connection.
	#[display(
		"the {role} chain gave genesis hash {}, not {}",
		encode_hex(answered),
		encode_hex(named)
	)]
	GenesisHash {
		/// The chain's role in the descriptor.
		role: String,
		/// The genesis hash the descriptor names.
		named: [u8; 32],
		/// The genesis hash the chain gave.
		answered: [u8; 32],
	},
	/// People's `NetworkSuffix.NetworkSuffix` is not the descriptor's `tld`.
	///
	/// The core derives its identities under `peopl.<tld>` and builds proof
	/// contexts with People's suffix, so an identity derived under one does
	/// not match the other. The core keeps using People.
	#[display("People's NetworkSuffix is \"{answered}\", not the stack's tld \"{named}\"")]
	NetworkSuffix {
		/// The descriptor's `tld`.
		named: String,
		/// The suffix People gave, escaped as [`u8::escape_ascii`] escapes
		/// it: printable ASCII stays as it is, apart from a backslash, single
		/// quote or double quote, which gets a backslash before it, and any
		/// other byte is written as `\t`, `\r`, `\n` or `\xNN`.
		answered: String,
	},
}

/// What an answer from a chain did to [`StackStatus::mismatches`].
#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum Listing {
	/// Nothing: the list is as it was.
	Unchanged,
	/// The entry the list had is gone, because the answer agrees with the
	/// descriptor.
	Cleared,
	/// The list has this entry now.
	Listed(StackMismatch),
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
	/// The core stored a newer copy that changes a chain, the network or the
	/// generation. It runs that copy from its next start, so build the
	/// runtime again to follow it. A copy on another TLD is refused and never
	/// sets it.
	pub restart_required: bool,
	/// The last finished fetch, or `None` before the first one. A fetch that
	/// changed the stored record finishes once that record is stored or its
	/// store fails; the whole status shows it only from then on.
	pub last_refresh: Option<StackRefreshOutcome>,
	/// Where a chain's last answer differed from the descriptor the core
	/// runs: its chains in the descriptor's order, then People's
	/// `NetworkSuffix`. The suffix is listed only once this runtime has read
	/// it, so a suffix that is not listed may not have been read yet. While a
	/// chain is listed, a refresh that was started fetches the descriptor
	/// every five minutes instead of every hour, until a newer copy sets
	/// `restart_required`.
	pub mismatches: Vec<StackMismatch>,
}

/// The stack a host build runs: the copy the core started from, and each
/// newer copy a fetch brings.
pub struct StackSource {
	/// What the core checks every copy against.
	rules: CopyRules,
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
	/// The last answer of each chain that gave a different genesis hash from
	/// the one the running copy names.
	wrong_genesis: Vec<GenesisHashes>,
	/// People's `NetworkSuffix` as last read, or `None` before any read. It is
	/// not stored, so each runtime starts without one.
	people_suffix: Option<Vec<u8>>,
	/// The status from before the last [`Self::fetched`], shown until the
	/// record it returned is stored or fails to store, so that the host is
	/// never told `Current` for a copy whose store may still fail.
	before_store: Option<StackStatus>,
}

impl StackSource {
	/// Starts from the newer of `bundled`, the signed copy the build ships,
	/// and `stored`, the record the last [`Self::fetched`] returned.
	///
	/// Fails when `bundled` does not verify under the pointer's key or is not
	/// a stack the core can run. A stored record the core would refuse is
	/// ignored until the next stored record replaces it.
	pub fn load(
		pointer: Pointer,
		bundled: &[u8],
		stored: Option<&[u8]>,
		allows_reserved_values: bool,
		now_unix_seconds: i64,
	) -> Result<Self, StackError> {
		let rules = CopyRules::new(pointer, bundled, allows_reserved_values)?;
		let stored = stored.filter(|record| match rules.stored_copy(record) {
			Ok(copy) => copy.is_some(),
			Err(error) => {
				tracing::warn!("starting from the bundled stack descriptor: {error}");
				false
			},
		});
		let mut reader = StackReader::new(Refresh::MaxAge(REFRESH_MAX_AGE_SECS));
		reader
			.load(rules.pointer(), Some(bundled), stored)
			.map_err(StackError::Descriptor)?;
		let read = reader.get(rules.pointer(), now_unix_seconds);
		let current = read.current.expect("a reader loaded with a build copy has a copy");
		Ok(Self {
			running: rules.hold(current.stack)?,
			rules,
			reader,
			first_fetch: read.fetch,
			unstored: None,
			next_start: None,
			stale: current.stale,
			expired: current.expired,
			last_refresh: None,
			wrong_genesis: Vec::new(),
			people_suffix: None,
			before_store: None,
		})
	}

	/// What identifies the stack. The caller stores its record under it.
	pub fn stack_id(&self) -> String {
		self.rules.pointer().stack_id()
	}

	/// The stack the core runs.
	pub fn stack(&self) -> &Stack {
		&self.running.stack
	}

	/// What the host is told about the stack.
	///
	/// While the record a [`Self::fetched`] returned is being stored, this is
	/// the status from before that fetch, with the chains' answers since.
	pub fn status(&self) -> StackStatus {
		let mismatches = self.mismatches();
		if let Some(before) = &self.before_store {
			return StackStatus { mismatches, ..before.clone() };
		}
		StackStatus {
			descriptor: serde_json::to_string(&self.running.descriptor)
				.expect("a descriptor always serialises"),
			generation: self.running.descriptor.generation.clone(),
			stale: self.stale,
			expired: self.expired,
			restart_required: self.next_start.is_some(),
			last_refresh: self.last_refresh.clone(),
			mismatches,
		}
	}

	fn mismatches(&self) -> Vec<StackMismatch> {
		self.wrong_chains().chain(self.wrong_suffix()).collect()
	}

	/// People's suffix as last read, when it is not the running copy's `tld`.
	fn wrong_suffix(&self) -> Option<StackMismatch> {
		let tld = self.running.stack.tld();
		let answered =
			self.people_suffix.as_deref().filter(|answered| *answered != tld.as_bytes())?;
		Some(StackMismatch::NetworkSuffix {
			named: tld.to_owned(),
			answered: answered.escape_ascii().to_string(),
		})
	}

	/// The chains of the running copy whose last answer was a different
	/// genesis hash from the one it names, in the copy's order.
	fn wrong_chains(&self) -> impl Iterator<Item = StackMismatch> {
		self.running.descriptor.chains.iter().filter_map(|chain| {
			let named = chain.genesis();
			let wrong = self.wrong_genesis.iter().find(|wrong| wrong.named == named)?;
			let answered = wrong.answered;
			Some(StackMismatch::GenesisHash { role: chain.role.clone(), named, answered })
		})
	}

	/// Takes People's `NetworkSuffix.NetworkSuffix` as read from the chain.
	///
	/// Returns what that did to [`Self::status`], which lists the last suffix
	/// read when it is not the running copy's `tld`. The core refuses nothing
	/// over a suffix, so unlike a wrong genesis hash it does not bring the
	/// next fetch forward.
	pub fn people_suffix_answered(&mut self, answered: &[u8]) -> Listing {
		let listed = self.wrong_suffix();
		self.people_suffix = Some(answered.to_vec());
		match self.wrong_suffix() {
			wrong if wrong == listed => Listing::Unchanged,
			Some(wrong) => Listing::Listed(wrong),
			None => Listing::Cleared,
		}
	}

	/// Records the genesis hash a new connection's chain gave.
	///
	/// Returns what that did to [`Self::status`]. A chain the running copy
	/// does not name changes nothing. While a chain's last answer is wrong
	/// and no newer copy waits for the next start, [`Self::next_fetch_in`]
	/// counts to five minutes instead of an hour.
	pub fn chain_answered(&mut self, hashes: GenesisHashes) -> Listing {
		let GenesisHashes { named, answered } = hashes;
		let chains = &self.running.descriptor.chains;
		let Some(chain) = chains.iter().find(|chain| chain.genesis() == named) else {
			return Listing::Unchanged;
		};
		let entry = self.wrong_genesis.iter().find(|entry| entry.named == named);
		let listed = entry.map(|entry| entry.answered);
		let wrong = (answered != named).then_some(answered);
		if listed == wrong {
			return Listing::Unchanged;
		}
		self.wrong_genesis.retain(|entry| entry.named != named);
		if answered == named {
			return Listing::Cleared;
		}
		self.wrong_genesis.push(hashes);
		Listing::Listed(StackMismatch::GenesisHash { role: chain.role.clone(), named, answered })
	}

	/// The fetch to make now, when the copy is old enough and no fetch is
	/// out. Answer it with [`Self::fetched`].
	#[must_use = "a fetch handed out stays pending until `fetched` answers it"]
	pub fn due_fetch(&mut self, now_unix_seconds: i64) -> Option<FetchRequest> {
		match self.next_fetch_in(now_unix_seconds) {
			Some(Duration::ZERO) => self.forced_fetch(now_unix_seconds),
			_ => None,
		}
	}

	/// How long until [`Self::due_fetch`] has a fetch to make, or `None` while
	/// one is out.
	#[must_use]
	pub fn next_fetch_in(&self, now_unix_seconds: i64) -> Option<Duration> {
		if self.first_fetch.is_some() {
			return Some(Duration::ZERO);
		}
		let until_an_hour_old = self.reader.due_in(self.rules.pointer(), now_unix_seconds)?;
		// Once a newer copy waits for the next start, fetching sooner has
		// done its job.
		let chain_is_wrong = self.wrong_chains().next().is_some();
		let max_age = if chain_is_wrong && self.next_start.is_none() {
			MISMATCH_REFRESH_MAX_AGE_SECS
		} else {
			REFRESH_MAX_AGE_SECS
		};
		let sooner_by = REFRESH_MAX_AGE_SECS - max_age;
		Some(Duration::from_secs(until_an_hour_old.saturating_sub(sooner_by)))
	}

	/// A fetch to make now whatever the copy's age, unless one is out. Answer
	/// it with [`Self::fetched`].
	#[must_use = "a fetch handed out stays pending until `fetched` answers it"]
	pub fn forced_fetch(&mut self, now_unix_seconds: i64) -> Option<FetchRequest> {
		self.first_fetch
			.take()
			.or_else(|| self.reader.refresh(self.rules.pointer(), now_unix_seconds))
	}

	/// Takes the answer to a fetch, or why the fetch failed, and returns the
	/// record to store when it changed. Store it, then call
	/// [`Self::record_stored`], or [`Self::record_not_stored`] when the store
	/// failed. Until then [`Self::status`] shows the status from before this
	/// fetch. The record goes back to [`Self::load`] as `stored`.
	///
	/// A copy the core cannot run, or that changes a pinned field, is refused
	/// and not stored. After a refused or failed fetch the record holds the
	/// copy the core already had, without its `ETag`.
	#[must_use = "the record is the copy the next start reads"]
	pub fn fetched(
		&mut self,
		answer: Result<Served, String>,
		now_unix_seconds: i64,
	) -> Option<Vec<u8>> {
		// The reader judges every later fetch against a copy it takes, so one
		// the core refuses never reaches it.
		let (served, unusable) = match answer {
			Ok(served) => match self.refusal(served) {
				Some(reason) => {
					(Served::Unreachable, Some(StackRefreshOutcome::Refused { reason }))
				},
				None => (served, None),
			},
			Err(reason) => (Served::Unreachable, Some(StackRefreshOutcome::Unreachable { reason })),
		};
		let refreshed = match self.reader.fetched(self.rules.pointer(), served, now_unix_seconds) {
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
		let held = match self.rules.hold(accepted.stack) {
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
		if refreshed.stored.is_some() {
			self.before_store = Some(self.status());
		}
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
		self.before_store = None;
		if let Some(copy) = self.unstored.take() {
			self.next_start = Some(copy);
		}
	}

	/// Says the record the last [`Self::fetched`] returned could not be
	/// stored, and why.
	///
	/// It shows in [`StackStatus::last_refresh`] only after a fetch that came
	/// to `Current`. A refused or failed fetch stays reported as such.
	pub fn record_not_stored(&mut self, reason: String) {
		self.before_store = None;
		if self.last_refresh == Some(StackRefreshOutcome::Current) {
			self.last_refresh = Some(StackRefreshOutcome::NotStored { reason });
		}
	}

	/// The check for storing `record`, which [`Self::fetched`] returned.
	pub fn replace_check(&self, record: &[u8]) -> ReplaceCheck {
		ReplaceCheck::new(self.rules.clone(), record)
	}

	/// Why the core refuses a served document that verifies under the
	/// pointer. `None` when the core takes it or the reader refuses it.
	fn refusal(&self, served: Served) -> Option<String> {
		let Served::Document { bytes, .. } = served else { return None };
		let verified = open(bytes, self.rules.pointer()).ok()?;
		self.rules.hold(verified).err().map(|error| error.to_string())
	}
}

#[cfg(test)]
mod tests;
