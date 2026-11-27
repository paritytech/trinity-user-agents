//! The fields no other copy of a stack's descriptor may change.
//!
//! [`Pinned`] is read from the copy the build bundles, or for a stack the
//! build ships no copy of from the first copy the core held, and
//! [`Pinned::refuse_change`] checks stored and fetched copies against it.

use trinity_stack::Descriptor;

use super::StackError;

/// The bundled descriptor's fields that every other copy must keep.
///
/// The bundled copy ships in the build, so a copy signed with a stolen key
/// cannot move the stack off these values. The TLD is pinned on every stack
/// because accounts derive under it.
#[derive(Clone)]
pub struct Pinned {
	tld: String,
}

impl Pinned {
	/// What `first` pins: the copy the build ships, or the first copy the core
	/// held of a stack the build ships no copy of.
	pub fn new(first: &Descriptor) -> Self {
		Self { tld: first.tld.clone() }
	}

	/// Checks that `copy` keeps every pinned field.
	///
	/// Fails with [`StackError::PinnedFieldChanged`] naming the field.
	pub fn refuse_change(&self, copy: &Descriptor) -> Result<(), StackError> {
		if copy.tld != self.tld {
			return Err(StackError::PinnedFieldChanged { field: "tld" });
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests;
