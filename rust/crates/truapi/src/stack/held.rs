//! Which copies of a stack's descriptor the core runs.
//!
//! [`CopyRules`] holds what the core checks a copy against, and a
//! [`HeldCopy`] is a copy that passed. The start, each fetch and the record
//! store all ask the same [`CopyRules`], so they agree on which copies count.

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
	/// its services from differs, and the chains did not restart.
	pub fn runs_as(&self, newer: &Self) -> bool {
		self.stack == newer.stack && self.descriptor.generation == newer.descriptor.generation
	}
}

/// What the core checks a copy of the stack's descriptor against before it
/// runs it: the copy verifies under the stack's pointer, names a stack the
/// core can run, and leaves every field the bundled copy pins as it is.
#[derive(Clone)]
pub struct CopyRules {
	pointer: Pointer,
	reserved_values: ReservedValuesPolicy,
	pinned: Pinned,
}

impl CopyRules {
	/// The rules for the stack `pointer` names, with the pins of `bundled`,
	/// the signed copy the build ships. `reserved_values` is the build's
	/// policy on reserved values, already checked.
	///
	/// Fails when `bundled` does not verify under the pointer's key.
	pub fn new(
		pointer: Pointer,
		bundled: &[u8],
		reserved_values: ReservedValuesPolicy,
	) -> Result<Self, StackError> {
		let bundled = open(bundled, &pointer).map_err(StackError::Descriptor)?;
		Ok(Self { pointer, reserved_values, pinned: Pinned::new(bundled.descriptor) })
	}

	/// The stack's pointer.
	pub fn pointer(&self) -> &Pointer {
		&self.pointer
	}

	/// The copy `verified` carries, a descriptor already verified under the
	/// pointer.
	///
	/// Fails with the rule it breaks when the core cannot run it or it
	/// changes a field the bundled copy pins.
	pub fn hold(&self, verified: Verified) -> Result<HeldCopy, StackError> {
		let stack = Stack::from_descriptor(&verified.descriptor, &self.reserved_values)?;
		self.pinned.refuse_change(&verified.descriptor, &self.reserved_values)?;
		Ok(HeldCopy { descriptor: verified.descriptor, stack })
	}

	/// The copy `record` holds, a record of the stack's store, when that copy
	/// passes these rules. `None` when the record does not read or holds no
	/// copy that verifies under the pointer.
	///
	/// Fails like [`Self::hold`] when the record holds a copy the core
	/// refuses. The core starts from neither kind of record, and neither
	/// keeps a copy the core takes out of the store. A copy that passes is
	/// one the core can start from, and it does so unless the bundled copy
	/// is newer.
	pub fn stored_copy(&self, record: &[u8]) -> Result<Option<HeldCopy>, StackError> {
		let mut reader = StackReader::new(Refresh::Never);
		reader.load(&self.pointer, None, Some(record)).map_err(StackError::Descriptor)?;
		let stored = reader.get(&self.pointer, 0).current;
		stored.map(|stored| self.hold(stored.stack)).transpose()
	}
}
