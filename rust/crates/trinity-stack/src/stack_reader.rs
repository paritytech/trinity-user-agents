//! Keeps each stack's copy current by a refresh policy, so a descriptor is never a live
//! dependency: a reader serves the copy it has, and a fetch only refreshes it. The caller
//! fetches, stores and tracks time; [`StackReader`] decides when, and what to use.

use crate::{
	Accepted, Error, Fetched, Pointer, Status, Verified, accept, envelope::open, format,
	reader::passed,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// When a [`StackReader`] fetches a stack it already has a copy of. A stack it has no copy
/// of is fetched on first use under every policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
	/// Only when the caller asks, through [`StackReader::refresh`].
	Never,
	/// On first use in the process, which waits for the answer, and not again.
	Once,
	/// In the background, once the copy is this many seconds old.
	MaxAge(u64),
}

/// A fetch for the caller to make and answer with [`StackReader::fetched`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase")]
pub struct FetchRequest {
	/// The descriptor's URL.
	pub url: String,
	/// The `ETag` to send as `If-None-Match`, so an unchanged stack costs a `304`.
	pub if_none_match: Option<String>,
}

/// What a fetch got.
#[derive(Clone, Copy, Debug)]
pub enum Served<'a> {
	/// A document, with the `ETag` it was served with.
	Document {
		/// The bytes as served.
		bytes: &'a [u8],
		/// The response's `ETag`.
		etag: Option<&'a str>,
	},
	/// `304 Not Modified`.
	NotModified,
	/// The fetch failed.
	Unreachable,
}

/// The copy of a stack a reader uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
pub struct CurrentStack {
	/// The descriptor.
	#[serde(flatten)]
	pub stack: Verified,
	/// Past `validUntil`: still usable, but shown as stale.
	pub stale: bool,
	/// Past `expiresAt`: the stack itself is gone.
	pub expired: bool,
}

/// What [`StackReader::get`] returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
pub struct StackRead {
	/// The copy to use. `None` means wait for the stack's fetch, which this call or an earlier
	/// one handed out.
	pub current: Option<CurrentStack>,
	/// A fetch to make. The reader hands out at most one per stack until it is completed.
	pub fetch: Option<FetchRequest>,
}

/// What [`StackReader::fetched`] did with an answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
pub struct Refreshed {
	/// What `accept` decided, and the copy now in use.
	pub accepted: Accepted,
	/// Bytes to store under the pointer's `stack_id`, when what the reader stores changed. They
	/// go back to [`StackReader::load`] as `stored`.
	#[serde(serialize_with = "format::serialize_optional_bytes")]
	#[cfg_attr(feature = "js", tsify(type = "Uint8Array | undefined"))]
	pub stored: Option<Vec<u8>>,
}

/// Each stack's current copy, and when to refresh it.
///
/// ```
/// use trinity_stack::{Pointer, Refresh, Served, StackReader};
///
/// # fn fetch(_: &trinity_stack::FetchRequest) -> Option<Vec<u8>> { None }
/// # let (build, stored, now) = (None, None, 0);
/// let pointer = Pointer::new("https://stack.example/.well-known/trinity-stack.json", None)?;
/// let mut reader = StackReader::new(Refresh::MaxAge(3600));
/// reader.load(&pointer, build, stored)?;
/// let read = reader.get(&pointer, now);
/// if let Some(request) = &read.fetch {
///     let served = fetch(request);
///     let served = served.as_deref().map_or(Served::Unreachable, |bytes| Served::Document {
///         bytes,
///         etag: None,
///     });
///     let refreshed = reader.fetched(&pointer, served, now);
/// }
/// # Ok::<(), trinity_stack::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct StackReader {
	refresh: Refresh,
	stacks: BTreeMap<String, Stack>,
}

#[derive(Clone, Debug, Default)]
struct Stack {
	copy: Option<StackCopy>,
	fetches: FetchState,
}

#[derive(Clone, Debug)]
struct StackCopy {
	document: Vec<u8>,
	stack: Verified,
	/// When it was fetched; `None` for a build copy.
	fetched_at: Option<i64>,
	etag: Option<String>,
}

/// What the store has per stack.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRecord {
	document: String,
	fetched_at: Option<i64>,
	etag: Option<String>,
}

/// When to fetch a stack.
#[derive(Clone, Debug, Default)]
struct FetchState {
	/// Started and not finished yet.
	pending: bool,
	requested_at: Option<i64>,
	completed: bool,
}

impl FetchState {
	/// Whether a caller waits for the answer instead of using the copy it has.
	fn waits(&self, refresh: Refresh) -> bool {
		refresh == Refresh::Once && !self.completed
	}

	fn start(&mut self, now: i64) {
		self.pending = true;
		self.requested_at = Some(now);
	}

	fn finish(&mut self) {
		self.pending = false;
		self.completed = true;
	}
}

impl StackReader {
	/// A reader that refreshes by `refresh`.
	pub fn new(refresh: Refresh) -> Self {
		Self { refresh, stacks: BTreeMap::new() }
	}

	/// Starts a stack from the copy a build bundled and the record the caller stored, the newer
	/// of the two: the store can move a stack forward from the build's copy, never back. Either
	/// may be absent, and a stored record that no longer reads is ignored. Fails when the build
	/// copy breaks a rule.
	pub fn load(
		&mut self,
		pointer: &Pointer,
		build: Option<&[u8]>,
		stored: Option<&[u8]>,
	) -> Result<(), Error> {
		let build = build
			.map(|bytes| {
				Ok::<_, Error>(StackCopy {
					stack: open(bytes, pointer)?,
					document: bytes.to_vec(),
					fetched_at: None,
					etag: None,
				})
			})
			.transpose()?;
		// Through `Value`, which the crate links anyway, rather than a typed slice parser.
		let record = stored.and_then(|bytes| {
			serde_json::from_slice::<serde_json::Value>(bytes)
				.and_then(StoredRecord::deserialize)
				.ok()
		});
		let stored_copy = record.and_then(|record| {
			Some(StackCopy {
				stack: open(record.document.as_bytes(), pointer).ok()?,
				document: record.document.into_bytes(),
				fetched_at: record.fetched_at,
				etag: record.etag,
			})
		});
		// By `sequence` even when unsigned, where `accept` would take an older copy.
		let copy = match (build, stored_copy) {
			(Some(build), Some(stored)) => {
				let (built, stored_sequence) =
					(build.stack.descriptor.sequence, stored.stack.descriptor.sequence);
				let newer = stored_sequence > built ||
					(stored_sequence == built && stored.stack.same_content(&build.stack));
				Some(if newer { stored } else { build })
			},
			(build, stored) => build.or(stored),
		};
		self.stacks.insert(pointer.stack_id(), Stack { copy, ..Default::default() });
		Ok(())
	}

	/// The copy to use now, and a fetch to make when the policy makes the copy due, or when
	/// there is none. A pointer [`StackReader::load`] never saw starts with no copy.
	pub fn get(&mut self, pointer: &Pointer, now_unix_seconds: i64) -> StackRead {
		let refresh = self.refresh;
		let stack = self.stack(pointer);
		let fetch = stack.fetch_due(refresh, now_unix_seconds).then(|| {
			stack.fetches.start(now_unix_seconds);
			stack.request(pointer)
		});
		let current = stack
			.copy
			.as_ref()
			.filter(|_| !stack.fetches.waits(refresh))
			.map(|copy| copy.current(now_unix_seconds));
		StackRead { current, fetch }
	}

	/// A fetch of the stack whatever the policy, unless one is out already.
	pub fn refresh(&mut self, pointer: &Pointer, now_unix_seconds: i64) -> Option<FetchRequest> {
		let stack = self.stack(pointer);
		(!stack.fetches.pending).then(|| {
			stack.fetches.start(now_unix_seconds);
			stack.request(pointer)
		})
	}

	/// The stacks whose background refresh is due, by their `stack_id`. A caller on a timer
	/// passes each to [`StackReader::refresh`].
	pub fn due(&self, now_unix_seconds: i64) -> Vec<String> {
		let Refresh::MaxAge(_) = self.refresh else { return Vec::new() };
		self.stacks
			.iter()
			.filter(|(_, stack)| {
				stack.copy.is_some() && stack.fetch_due(self.refresh, now_unix_seconds)
			})
			.map(|(id, _)| id.clone())
			.collect()
	}

	/// Takes the answer to the stack's fetch. Fails only when the reader has no copy and the
	/// answer gives it none, which every caller waiting on the fetch then sees.
	pub fn fetched(
		&mut self,
		pointer: &Pointer,
		served: Served,
		now_unix_seconds: i64,
	) -> Result<Refreshed, Error> {
		let stack = self.stack(pointer);
		stack.fetches.finish();
		let stored = stack.copy.as_ref().map(|copy| copy.document.as_slice());
		let (fetched, etag) = match (served, stored) {
			(Served::Document { bytes, etag }, _) => (Fetched::Document(bytes), etag),
			// The server vouches for the copy the reader sent the ETag of.
			(Served::NotModified, Some(stored)) => (Fetched::Document(stored), None),
			(Served::NotModified | Served::Unreachable, _) => (Fetched::Unreachable, None),
		};
		let accepted = accept(pointer, stored, fetched, now_unix_seconds)?;
		let etag = etag.map(str::to_owned);
		match (&accepted.status, &mut stack.copy) {
			(Status::Updated, copy) => {
				*copy = Some(StackCopy {
					document: accepted.document.clone(),
					stack: accepted.stack.clone(),
					fetched_at: Some(now_unix_seconds),
					etag,
				})
			},
			(Status::Unchanged, Some(copy)) => {
				copy.fetched_at = Some(now_unix_seconds);
				copy.etag = etag.or(copy.etag.take());
			},
			// A browser cannot send `If-None-Match` cross-origin to a server whose CORS does not
			// allow it, so a conditional fetch that failed is followed by a plain one.
			(Status::Unreachable, Some(copy)) if copy.etag.is_some() => copy.etag = None,
			_ => return Ok(Refreshed { accepted, stored: None }),
		}
		Ok(Refreshed { accepted, stored: stack.stored_record() })
	}

	fn stack(&mut self, pointer: &Pointer) -> &mut Stack {
		self.stacks.entry(pointer.stack_id()).or_default()
	}
}

impl Stack {
	fn fetch_due(&self, refresh: Refresh, now_unix_seconds: i64) -> bool {
		if self.fetches.pending {
			return false;
		}
		let Some(copy) = &self.copy else { return true };
		match refresh {
			Refresh::Never => false,
			Refresh::Once => self.fetches.requested_at.is_none(),
			Refresh::MaxAge(age) => {
				// A fetch time ahead of the clock was recorded under a skewed one, so it is
				// ignored.
				let fetched_at = copy.fetched_at.filter(|at| *at <= now_unix_seconds);
				fetched_at.max(self.fetches.requested_at).is_none_or(|last| {
					now_unix_seconds.saturating_sub(last) >= i64::try_from(age).unwrap_or(i64::MAX)
				})
			},
		}
	}

	fn request(&self, pointer: &Pointer) -> FetchRequest {
		FetchRequest {
			url: pointer.url().to_string(),
			if_none_match: self.copy.as_ref().and_then(|copy| copy.etag.clone()),
		}
	}

	/// What the store has for this stack; `None` until the reader has a descriptor.
	fn stored_record(&self) -> Option<Vec<u8>> {
		let copy = self.copy.as_ref()?;
		let record = StoredRecord {
			document: String::from_utf8(copy.document.clone()).expect("descriptor is not UTF-8"),
			fetched_at: copy.fetched_at,
			etag: copy.etag.clone(),
		};
		Some(serde_json::to_vec(&record).expect("failed to serialize the stored record"))
	}
}

impl StackCopy {
	fn current(&self, now_unix_seconds: i64) -> CurrentStack {
		let descriptor = &self.stack.descriptor;
		CurrentStack {
			stack: self.stack.clone(),
			stale: passed(descriptor.valid_until, now_unix_seconds),
			expired: passed(descriptor.expires_at, now_unix_seconds),
		}
	}
}
