use crate::{Error, Pointer, Verified, envelope::open};
use serde::{Serialize, Serializer};

/// The result of fetching a stack's descriptor.
#[derive(Clone, Copy, Debug)]
pub enum Fetched<'a> {
	/// The bytes the URL served.
	Document(&'a [u8]),
	/// The fetch failed.
	Unreachable,
}

/// What [`accept`] did with a fetched descriptor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, strum::IntoStaticStr)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
#[serde(tag = "status", content = "refused", rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Status {
	/// The fetched descriptor replaces the stored one.
	Updated,
	/// The fetched descriptor carries the same content as the stored one.
	Unchanged,
	/// Same `sequence`, different content. The stored copy is kept.
	Conflict,
	/// A signed descriptor with a lower `sequence`. Refused; the stored copy is kept.
	Rollback,
	/// Nothing was fetched. The stored copy is kept.
	Unreachable,
	/// The fetched bytes break a rule, named by its [`Error::code`]. The stored copy is kept.
	Refused(
		#[serde(serialize_with = "code")]
		#[cfg_attr(feature = "js", tsify(type = "string"))]
		Error,
	),
}

fn code<S: Serializer>(error: &Error, serializer: S) -> Result<S::Ok, S::Error> {
	serializer.serialize_str(error.code())
}

impl Status {
	/// The status's name, as the conformance vectors spell it.
	pub fn code(&self) -> &'static str {
		self.into()
	}
}

/// The descriptor a reader uses next, and what changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase")]
pub struct Accepted {
	/// What happened to the fetched descriptor.
	// Named as an override so tsify writes an intersection: an interface cannot extend a union.
	#[serde(flatten)]
	#[cfg_attr(feature = "js", tsify(type = "Status"))]
	pub status: Status,
	/// The bytes to store and pass back as `stored` next time.
	#[serde(serialize_with = "crate::format::serialize_bytes")]
	#[cfg_attr(feature = "js", tsify(type = "Uint8Array"))]
	pub document: Vec<u8>,
	/// The descriptor to use.
	#[serde(flatten)]
	pub stack: Verified,
	/// The chains restarted: drop cached chain data such as a light client's database.
	pub new_generation: bool,
	/// Past `validUntil`: still usable, but shown as stale.
	pub stale: bool,
	/// Past `expiresAt`: the stack itself is gone.
	pub expired: bool,
}

/// Decides what a reader does with a fetched descriptor, given the copy it stored last.
///
/// `stored` is what the previous call returned as [`Accepted::document`], or a bundled copy. A
/// reader with no store passes `None` every time and gets no rollback protection. A stored copy
/// that no longer passes the rules counts as absent. Fails only when nothing usable is left.
pub fn accept(
	pointer: &Pointer,
	stored: Option<&[u8]>,
	fetched: Fetched,
	now_unix_seconds: i64,
) -> Result<Accepted, Error> {
	let stored = stored.and_then(|bytes| Some((bytes, open(bytes, pointer).ok()?)));
	let fetched = match fetched {
		Fetched::Document(bytes) => open(bytes, pointer).map(|stack| (bytes, stack)),
		Fetched::Unreachable => Err(Error::Unreachable),
	};
	let (status, (document, stack)) = match (stored, fetched) {
		(None, Ok(fetched)) => (Status::Updated, fetched),
		(None, Err(error)) => return Err(error),
		(Some(stored), Err(Error::Unreachable)) => (Status::Unreachable, stored),
		(Some(stored), Err(error)) => (Status::Refused(error), stored),
		(Some(stored), Ok(fetched)) => {
			let (old, new) = (stored.1.descriptor.sequence, fetched.1.descriptor.sequence);
			if new == old && fetched.1.same_content(&stored.1) {
				(Status::Unchanged, stored)
			} else if new == old {
				(Status::Conflict, stored)
			} else if new < old && pointer.key().is_some() {
				(Status::Rollback, stored)
			} else {
				let new_generation =
					fetched.1.descriptor.generation != stored.1.descriptor.generation;
				return Ok(finish(Status::Updated, fetched, new_generation, now_unix_seconds));
			}
		},
	};
	Ok(finish(status, (document, stack), false, now_unix_seconds))
}

fn finish(
	status: Status,
	(document, stack): (&[u8], Verified),
	new_generation: bool,
	now_unix_seconds: i64,
) -> Accepted {
	let passed = |time: Option<u32>| time.is_some_and(|time| now_unix_seconds >= i64::from(time));
	Accepted {
		stale: passed(stack.descriptor.valid_until),
		expired: passed(stack.descriptor.expires_at),
		status,
		document: document.to_vec(),
		stack,
		new_generation,
	}
}
