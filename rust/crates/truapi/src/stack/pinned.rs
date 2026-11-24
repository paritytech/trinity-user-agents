//! The fields no other copy of a stack's descriptor may change.
//!
//! [`Pinned`] holds the copy the build bundles, and
//! [`Pinned::refuse_change`] checks stored and fetched copies against it.

use trinity_stack::Descriptor;

use super::{ReservedValuesPolicy, StackError};

/// A stack's bundled descriptor, as the copy whose pinned fields every other
/// copy must leave as they are.
///
/// The bundled copy ships in the build, so a copy signed with a stolen key
/// cannot move the stack off these values. The TLD is pinned on every stack
/// because accounts derive under it.
///
/// A stack its build allows reserved values pins more of what its descriptor
/// names: the chain roles and each one's genesis hash, `assets`, `contracts`,
/// `dotns`, `coinage`, and the digest of the relay's light spec, the file
/// that carries the relay's sync checkpoint. The pins fix what the descriptor
/// says, not what a chain answers: the core does not check a genesis hash
/// against the chain it reads.
#[derive(Clone)]
pub struct Pinned {
	bundled: Descriptor,
}

impl Pinned {
	/// The pins of `bundled`, the copy the build ships.
	pub fn new(bundled: Descriptor) -> Self {
		Self { bundled }
	}

	/// Checks that `copy` leaves every pinned field as the bundled copy has
	/// it. `reserved_values` is the build's policy, which decides whether
	/// the stack pins more than its TLD.
	///
	/// Fails with [`StackError::PinnedFieldChanged`] naming the first field
	/// that differs.
	pub fn refuse_change(
		&self,
		copy: &Descriptor,
		reserved_values: &ReservedValuesPolicy,
	) -> Result<(), StackError> {
		if copy.tld != self.bundled.tld {
			return Err(StackError::PinnedFieldChanged { field: "tld" });
		}
		if !reserved_values.allowed {
			return Ok(());
		}
		match self.changed_reserved_field(copy) {
			Some(field) => Err(StackError::PinnedFieldChanged { field }),
			None => Ok(()),
		}
	}

	/// The first field a stack allowed reserved values pins besides its TLD
	/// that `copy` changes, or `None` when it changes none.
	fn changed_reserved_field(&self, copy: &Descriptor) -> Option<&'static str> {
		let bundled = &self.bundled;
		[
			("the chain roles", !has_same_roles(bundled, copy)),
			("a chain's genesisHash", !has_same_genesis_hashes(bundled, copy)),
			("assets", bundled.assets != copy.assets),
			("contracts", bundled.contracts != copy.contracts),
			("dotns", bundled.dotns != copy.dotns),
			("coinage", bundled.coinage != copy.coinage),
			(
				"the relay's lightSpec.sha256",
				relay_light_spec_digest(bundled) != relay_light_spec_digest(copy),
			),
		]
		.into_iter()
		.find_map(|(field, changed)| changed.then_some(field))
	}
}

/// Whether `copy` names the chain roles `bundled` names, no more and no
/// fewer. A descriptor names each role once.
fn has_same_roles(bundled: &Descriptor, copy: &Descriptor) -> bool {
	bundled.chains.len() == copy.chains.len() &&
		bundled.chains.iter().all(|chain| copy.chain(&chain.role).is_some())
}

/// Whether every role both copies name has the same genesis hash in both. A
/// descriptor writes a hash in one way, as lowercase hex, so equal hashes
/// are equal strings.
fn has_same_genesis_hashes(bundled: &Descriptor, copy: &Descriptor) -> bool {
	bundled.chains.iter().all(|chain| {
		copy.chain(&chain.role)
			.is_none_or(|other| other.genesis_hash == chain.genesis_hash)
	})
}

/// The digest of the relay's light spec, or `None` when the relay names no
/// light spec. The spec's URL is not pinned: the digest fixes what any server
/// may serve.
fn relay_light_spec_digest(descriptor: &Descriptor) -> Option<&str> {
	Some(descriptor.relay()?.light_spec.as_ref()?.sha256.as_str())
}

#[cfg(test)]
mod tests;
