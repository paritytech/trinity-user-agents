//! Keeps each stack's copy current by a refresh policy, so a descriptor is never a live
//! dependency: a reader serves the copy it holds, and a fetch only refreshes it. The caller
//! fetches, stores and keeps time; [`StackReader`] decides when, and what to use.

use crate::{
	Accepted, DotnsRecord, Error, Fetched, Pointer, Status, Verified, accept, envelope::open,
	format, reader::passed,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// When a [`StackReader`] fetches a stack it already holds a copy of. A stack it holds no copy
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

/// What [`StackReader::get`] says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
pub struct StackRead {
	/// The copy to use. `None` means wait for the stack's fetch, which this call or an earlier
	/// one handed out.
	pub current: Option<CurrentStack>,
	/// A fetch to make. The reader hands out at most one per stack until it is answered.
	pub fetch: Option<FetchRequest>,
}

/// What [`StackReader::fetched`] did with an answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
pub struct Refreshed {
	/// What `accept` decided, and the copy now in use.
	pub accepted: Accepted,
	/// Bytes to store under the pointer's `stack_id`, when what the reader keeps changed. They
	/// go back to [`StackReader::load`] as `stored`.
	#[serde(serialize_with = "format::serialize_optional_bytes")]
	#[cfg_attr(feature = "js", tsify(type = "Uint8Array | undefined"))]
	pub stored: Option<Vec<u8>>,
}

/// What [`StackReader::dotns`] says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase")]
pub struct DotnsRead {
	/// The record to use. `None` means wait for the stack's lookup.
	pub record: Option<DotnsRecord>,
	/// Run a lookup for this `generation` and answer with [`StackReader::looked_up`]. The
	/// reader hands out at most one per stack until it is answered.
	pub look_up: Option<String>,
}

/// What [`StackReader::looked_up`] did with a lookup's result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
pub struct DotnsLookedUp {
	/// The record now in use, `None` when the reader has none for the descriptor in use.
	pub record: Option<DotnsRecord>,
	/// Bytes to store under the pointer's `stack_id`, when what the reader keeps changed.
	#[serde(serialize_with = "format::serialize_optional_bytes")]
	#[cfg_attr(feature = "js", tsify(type = "Uint8Array | undefined"))]
	pub stored: Option<Vec<u8>>,
}

/// Each stack's current copy and dotNS record, and when to refresh them.
///
/// ```
/// use trinity_stack::{Pointer, Refresh, Served, StackReader};
///
/// # fn fetch(_: &trinity_stack::FetchRequest) -> Option<Vec<u8>> { None }
/// # let (build, stored, now) = (None, None, 0);
/// let pointer = Pointer::new("https://stack.example/.well-known/trinity-stack.json", None)?;
/// let mut reader = StackReader::new(Refresh::MaxAge(3600));
/// reader.load(&pointer, build, None, stored)?;
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
	held: Option<Held>,
	fetches: Schedule,
	dotns: Option<KeptDotns>,
	lookups: Schedule,
}

#[derive(Clone, Debug)]
struct Held {
	document: Vec<u8>,
	stack: Verified,
	/// When it was fetched; `None` for a build copy.
	fetched_at: Option<i64>,
	etag: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeptDotns {
	record: DotnsRecord,
	/// When it was looked up; `None` for a build copy.
	read_at: Option<i64>,
}

/// What the store keeps per stack.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
	document: String,
	fetched_at: Option<i64>,
	etag: Option<String>,
	dotns: Option<KeptDotns>,
}

/// When to ask for one thing a stack needs: its descriptor, or its dotNS record.
#[derive(Clone, Debug, Default)]
struct Schedule {
	/// Asked and not answered yet.
	pending: bool,
	asked_at: Option<i64>,
	answered: bool,
	/// The caller asked for it outside the policy, through [`StackReader::refresh`].
	requested: bool,
}

impl Schedule {
	/// Whether to ask now, given the copy in use and when it was read.
	fn wants(&self, refresh: Refresh, has_copy: bool, read_at: Option<i64>, now: i64) -> bool {
		if self.pending {
			return false;
		}
		match refresh {
			_ if !has_copy || self.requested => true,
			Refresh::Never => false,
			Refresh::Once => self.asked_at.is_none(),
			Refresh::MaxAge(age) => {
				// A read time ahead of the clock was kept under a skewed one, so it says nothing.
				let read_at = read_at.filter(|read_at| *read_at <= now);
				read_at.max(self.asked_at).is_none_or(|last| {
					now.saturating_sub(last) >= i64::try_from(age).unwrap_or(i64::MAX)
				})
			},
		}
	}

	/// Whether a caller waits for the answer instead of using the copy.
	fn waits(&self, refresh: Refresh, has_copy: bool) -> bool {
		!has_copy || (refresh == Refresh::Once && !self.answered)
	}

	fn ask(&mut self, now: i64) {
		self.pending = true;
		self.requested = false;
		self.asked_at = Some(now);
	}

	fn answer(&mut self) {
		self.pending = false;
		self.answered = true;
	}
}

impl StackReader {
	/// A reader that refreshes by `refresh`.
	pub fn new(refresh: Refresh) -> Self {
		Self { refresh, stacks: BTreeMap::new() }
	}

	/// Starts a stack from the copy a build bundled and the record the caller stored, the newer
	/// of the two: the store can move a stack or its dotNS record forward from the build's, never
	/// back. Either may be absent, and a stored record that no longer reads is ignored. Fails
	/// when the build copy breaks a rule.
	pub fn load(
		&mut self,
		pointer: &Pointer,
		build: Option<&[u8]>,
		build_dotns: Option<DotnsRecord>,
		stored: Option<&[u8]>,
	) -> Result<(), Error> {
		let build = build
			.map(|bytes| {
				Ok::<_, Error>(Held {
					stack: open(bytes, pointer)?,
					document: bytes.to_vec(),
					fetched_at: None,
					etag: None,
				})
			})
			.transpose()?;
		if let Some(record) = &build_dotns {
			record.check()?;
		}
		// Through `Value`, which the crate links anyway, rather than a typed slice parser.
		let kept = stored.and_then(|bytes| {
			serde_json::from_slice::<serde_json::Value>(bytes)
				.and_then(Kept::deserialize)
				.ok()
		});
		let stored_copy = kept.as_ref().and_then(|kept| {
			Some(Held {
				stack: open(kept.document.as_bytes(), pointer).ok()?,
				document: kept.document.as_bytes().to_vec(),
				fetched_at: kept.fetched_at,
				etag: kept.etag.clone(),
			})
		});
		// By `sequence` even when unsigned, where `accept` would take an older copy.
		let held = match (build, stored_copy) {
			(Some(build), Some(stored)) => {
				let (built, kept) =
					(build.stack.descriptor.sequence, stored.stack.descriptor.sequence);
				let newer =
					kept > built || (kept == built && stored.stack.same_content(&build.stack));
				Some(if newer { stored } else { build })
			},
			(build, stored) => build.or(stored),
		};
		let mut stack = Stack { held, ..Default::default() };
		let stored_dotns =
			kept.and_then(|kept| kept.dotns).filter(|kept| kept.record.check().is_ok());
		let build_dotns = build_dotns.map(|record| KeptDotns { record, read_at: None });
		stack.dotns =
			[stored_dotns, build_dotns]
				.into_iter()
				.flatten()
				.filter(|kept| stack.serves(kept))
				.reduce(|newest, other| {
					if other.record.at.number > newest.record.at.number { other } else { newest }
				});
		self.stacks.insert(pointer.stack_id(), stack);
		Ok(())
	}

	/// The copy to use now, and a fetch to make when the policy says the copy is due, or when
	/// there is none. A pointer [`StackReader::load`] never saw starts with no copy.
	pub fn get(&mut self, pointer: &Pointer, now_unix_seconds: i64) -> StackRead {
		let refresh = self.refresh;
		let stack = self.stack(pointer);
		let fetch = stack.fetch_due(refresh, now_unix_seconds).then(|| {
			stack.fetches.ask(now_unix_seconds);
			stack.request(pointer)
		});
		let current = stack
			.held
			.as_ref()
			.filter(|_| !stack.fetches.waits(refresh, true))
			.map(|held| held.current(now_unix_seconds));
		StackRead { current, fetch }
	}

	/// A fetch of the stack whatever the policy, unless one is out already. The next
	/// [`StackReader::dotns`] reads the stack's dotNS contracts again too.
	pub fn refresh(&mut self, pointer: &Pointer, now_unix_seconds: i64) -> Option<FetchRequest> {
		let stack = self.stack(pointer);
		stack.lookups.requested = true;
		(!stack.fetches.pending).then(|| {
			stack.fetches.ask(now_unix_seconds);
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
				stack.held.is_some() && stack.fetch_due(self.refresh, now_unix_seconds)
			})
			.map(|(id, _)| id.clone())
			.collect()
	}

	/// Takes the answer to the stack's fetch. Fails only when the reader holds no copy and the
	/// answer gives it none, which every caller waiting on the fetch then sees.
	pub fn fetched(
		&mut self,
		pointer: &Pointer,
		served: Served,
		now_unix_seconds: i64,
	) -> Result<Refreshed, Error> {
		let stack = self.stack(pointer);
		stack.fetches.answer();
		let stored = stack.held.as_ref().map(|held| held.document.clone());
		let (fetched, etag) = match (served, &stored) {
			(Served::Document { bytes, etag }, _) => (Fetched::Document(bytes), etag),
			// The server vouches for the copy the reader sent the ETag of.
			(Served::NotModified, Some(stored)) => (Fetched::Document(stored), None),
			(Served::NotModified | Served::Unreachable, _) => (Fetched::Unreachable, None),
		};
		let accepted = accept(pointer, stored.as_deref(), fetched, now_unix_seconds)?;
		let etag = etag.map(str::to_owned);
		match (&accepted.status, &mut stack.held) {
			(Status::Updated, held) => {
				*held = Some(Held {
					document: accepted.document.clone(),
					stack: accepted.stack.clone(),
					fetched_at: Some(now_unix_seconds),
					etag,
				})
			},
			(Status::Unchanged, Some(held)) => {
				held.fetched_at = Some(now_unix_seconds);
				held.etag = etag.or(held.etag.take());
			},
			// A browser cannot send `If-None-Match` cross-origin to a server whose CORS does not
			// allow it, so a conditional fetch that failed is followed by a plain one.
			(Status::Unreachable, Some(held)) if held.etag.is_some() => held.etag = None,
			_ => return Ok(Refreshed { accepted, stored: None }),
		}
		Ok(Refreshed { accepted, stored: stack.kept() })
	}

	/// The dotNS record to use for the descriptor in use, and a lookup to run when the policy
	/// says it is due, or when the reader has no record of that descriptor's `generation`.
	/// `None` until the reader holds a descriptor, and for a stack with no Asset Hub, which has
	/// no dotNS.
	pub fn dotns(&mut self, pointer: &Pointer, now_unix_seconds: i64) -> Option<DotnsRead> {
		let refresh = self.refresh;
		let stack = self.stack(pointer);
		let descriptor = &stack.held.as_ref()?.stack.descriptor;
		descriptor.chain("AssetHub")?;
		let generation = descriptor.generation.clone();
		let usable = stack.usable_dotns().cloned();
		let read_at = usable.as_ref().and_then(|kept| kept.read_at);
		let look_up = stack
			.lookups
			.wants(refresh, usable.is_some(), read_at, now_unix_seconds)
			.then(|| {
				stack.lookups.ask(now_unix_seconds);
				generation
			});
		let record = usable.filter(|_| !stack.lookups.waits(refresh, true)).map(|kept| kept.record);
		Some(DotnsRead { record, look_up })
	}

	/// Takes the result of the stack's lookup, `None` when it failed. A failed lookup, or one
	/// for a `generation` the reader has since moved past, leaves the record in use as it was.
	/// Fails when the record breaks a rule, which also counts as a failed lookup.
	pub fn looked_up(
		&mut self,
		pointer: &Pointer,
		record: Option<DotnsRecord>,
		now_unix_seconds: i64,
	) -> Result<DotnsLookedUp, Error> {
		let stack = self.stack(pointer);
		stack.lookups.answer();
		if let Some(record) = &record {
			record.check()?;
		}
		let fresh = record
			.map(|record| KeptDotns { record, read_at: Some(now_unix_seconds) })
			.filter(|kept| stack.serves(kept));
		let stored = match fresh {
			Some(fresh) => {
				stack.dotns = Some(fresh);
				stack.kept()
			},
			None => None,
		};
		let record = stack.usable_dotns().map(|kept| kept.record.clone());
		Ok(DotnsLookedUp { record, stored })
	}

	fn stack(&mut self, pointer: &Pointer) -> &mut Stack {
		self.stacks.entry(pointer.stack_id()).or_default()
	}
}

impl Stack {
	fn fetch_due(&self, refresh: Refresh, now_unix_seconds: i64) -> bool {
		let read_at = self.held.as_ref().and_then(|held| held.fetched_at);
		self.fetches.wants(refresh, self.held.is_some(), read_at, now_unix_seconds)
	}

	/// Whether `kept` belongs to the descriptor in use: a record never serves another
	/// `generation`.
	fn serves(&self, kept: &KeptDotns) -> bool {
		self.held
			.as_ref()
			.is_some_and(|held| held.stack.descriptor.generation == kept.record.generation)
	}

	fn usable_dotns(&self) -> Option<&KeptDotns> {
		self.dotns.as_ref().filter(|kept| self.serves(kept))
	}

	fn request(&self, pointer: &Pointer) -> FetchRequest {
		FetchRequest {
			url: pointer.url().to_string(),
			if_none_match: self.held.as_ref().and_then(|held| held.etag.clone()),
		}
	}

	/// What the store keeps for this stack; `None` until it holds a descriptor.
	fn kept(&self) -> Option<Vec<u8>> {
		let held = self.held.as_ref()?;
		let kept = Kept {
			document: String::from_utf8(held.document.clone()).expect("a descriptor is UTF-8"),
			fetched_at: held.fetched_at,
			etag: held.etag.clone(),
			dotns: self.dotns.clone(),
		};
		Some(serde_json::to_vec(&kept).expect("a record serialises"))
	}
}

impl Held {
	fn current(&self, now_unix_seconds: i64) -> CurrentStack {
		let descriptor = &self.stack.descriptor;
		CurrentStack {
			stack: self.stack.clone(),
			stale: passed(descriptor.valid_until, now_unix_seconds),
			expired: passed(descriptor.expires_at, now_unix_seconds),
		}
	}
}
