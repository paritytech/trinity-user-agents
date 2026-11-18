//! What no newer copy of a stack's descriptor may change.
//!
//! [`Pinned`] is read from the copy a host build bundles.
//! [`Pinned::refuse_change`] holds every copy the core stored or fetched to
//! it.

use trinity_stack::Descriptor;

use super::StackError;

/// The fields of a stack's bundled descriptor that every other copy must
/// leave as they are.
///
/// The bundled copy ships inside the build, so the key that signs the
/// stack's descriptors cannot change it. A copy signed with a stolen key
/// therefore cannot move the stack off a pinned value. The TLD is pinned on
/// every stack: the signing role derives its accounts under it.
#[derive(Clone)]
pub struct Pinned {
	tld: String,
}

impl Pinned {
	/// What `bundled`, the copy the build ships, pins.
	pub fn new(bundled: &Descriptor) -> Self {
		Self { tld: bundled.tld.clone() }
	}

	/// Checks that `copy` leaves every pinned field as the bundled copy has
	/// it.
	///
	/// Fails with [`StackError::PinnedFieldChanged`] naming the field that
	/// differs.
	pub fn refuse_change(&self, copy: &Descriptor) -> Result<(), StackError> {
		if copy.tld != self.tld {
			return Err(StackError::PinnedFieldChanged { field: "tld" });
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests;
