//! The fields no other copy of a stack's descriptor may change.
//!
//! [`Pinned`] is read from the copy the build bundles, and
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
	/// What `bundled`, the copy the build ships, pins.
	pub fn new(bundled: &Descriptor) -> Self {
		Self { tld: bundled.tld.clone() }
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
