//! Which copies of a stack's descriptor the core runs.
//!
//! A [`HeldCopy`] is a copy that passed [`CopyRules`]. Start, fetch and the
//! record store all ask the same rules, so they agree on which copies count.

use trinity_stack::{Descriptor, Pointer, Refresh, StackReader, Verified, open};

use super::{ReservedValuesPolicy, Stack, StackError, pinned::Pinned};

/// One verified copy of the descriptor, and the stack it names.
pub struct HeldCopy {
	/// The copy's descriptor.
	pub descriptor: Descriptor,
	/// The stack the descriptor names.
	pub stack: Stack,
}

impl HeldCopy {
	/// Whether the core can switch to `newer` while it runs: nothing it built
	/// its services from differs, including the generation, so the chains did
	/// not restart.
	pub fn runs_as(&self, newer: &Self) -> bool {
		self.stack == newer.stack
	}
}

/// What a copy must pass before the core runs it: it verifies under the
/// stack's pointer, names a stack the core can run, and keeps the pinned
/// fields.
#[derive(Clone)]
pub struct CopyRules {
	pointer: Pointer,
	reserved_values: ReservedValuesPolicy,
	pinned: Pinned,
}

impl CopyRules {
	/// The rules for `pointer`'s stack, pinned to `bundled`, the signed copy
	/// the build ships, under `reserved_values`, the build's policy, already
	/// checked.
	///
	/// Fails when `bundled` does not verify under the pointer or is not a
	/// stack the core can run under `reserved_values`, even when a newer stored
	/// copy exists.
	pub fn new(
		pointer: Pointer,
		bundled: &[u8],
		reserved_values: ReservedValuesPolicy,
	) -> Result<Self, StackError> {
		let bundled = open(bundled, &pointer).map_err(StackError::Descriptor)?;
		Stack::from_descriptor(&pointer, &bundled.descriptor, &reserved_values)?;
		let pinned = Pinned::new(&bundled.descriptor);
		Ok(Self { pointer, reserved_values, pinned })
	}

	/// The stack's pointer.
	pub fn pointer(&self) -> &Pointer {
		&self.pointer
	}

	/// Checks `verified`, a copy already verified under the pointer.
	///
	/// Fails with the rule it breaks.
	pub fn hold(&self, verified: Verified) -> Result<HeldCopy, StackError> {
		let stack =
			Stack::from_descriptor(&self.pointer, &verified.descriptor, &self.reserved_values)?;
		self.pinned.refuse_change(&verified.descriptor)?;
		Ok(HeldCopy { descriptor: verified.descriptor, stack })
	}

	/// The copy in `record`, a record from the stack's store.
	///
	/// `None` when the record does not read or its copy does not verify under
	/// the pointer. Fails like [`Self::hold`] when the core refuses the copy.
	pub fn stored_copy(&self, record: &[u8]) -> Result<Option<HeldCopy>, StackError> {
		let mut reader = StackReader::new(Refresh::Never);
		reader.load(&self.pointer, None, Some(record)).map_err(StackError::Descriptor)?;
		let stored = reader.get(&self.pointer, 0).current;
		stored.map(|stored| self.hold(stored.stack)).transpose()
	}
}
