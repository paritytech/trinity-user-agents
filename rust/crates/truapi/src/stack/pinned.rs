//! The fields no other copy of a stack's descriptor may change.
//!
//! [`Pinned`] holds the copy the build bundles.
//! [`Pinned::refuse_change`] checks stored and fetched copies against it, and
//! [`Pinned::keep_bundled_relay_light_spec`] gives a copy the core runs the
//! bundled relay light spec.

use std::collections::BTreeMap;

use trinity_stack::{Asset, Descriptor};

use super::{ReservedValuesPolicy, StackError};

/// A stack's bundled descriptor, as the copy whose pinned fields every other
/// copy must leave as they are.
///
/// The bundled copy ships in the build, so a copy signed with a stolen key
/// cannot move the stack off these values. The TLD is pinned on every stack
/// because accounts derive under it.
///
/// A stack its build allows reserved values pins more, and runs its bundled
/// relay light spec, as [`ReservedValuesPolicy`] describes.
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

	/// `copy` with the relay light spec the bundled copy names, or none when
	/// it names none, on a stack `reserved_values` allows reserved values.
	/// Any other stack runs `copy` as it is.
	///
	/// The relay light spec carries the sync checkpoint a light client starts
	/// from. An operator publishes a fresher one from time to time, so a copy
	/// that names another is not refused: the core keeps the bundled spec and
	/// takes the rest of the copy.
	pub fn keep_bundled_relay_light_spec(
		&self,
		mut copy: Descriptor,
		reserved_values: &ReservedValuesPolicy,
	) -> Descriptor {
		if !reserved_values.allowed {
			return copy;
		}
		let Some(bundled_relay) = self.bundled.relay() else { return copy };
		if let Some(relay) = copy.chains.iter_mut().find(|chain| chain.role == bundled_relay.role) {
			relay.light_spec = bundled_relay.light_spec.clone();
		}
		copy
	}

	/// The first field a stack allowed reserved values pins besides its TLD
	/// that `copy` changes, or `None` when it changes none.
	fn changed_reserved_field(&self, copy: &Descriptor) -> Option<&'static str> {
		let bundled = &self.bundled;
		[
			("the chain roles", !has_same_roles(bundled, copy)),
			("a chain's genesisHash", !has_same_genesis_hashes(bundled, copy)),
			("assets", assets_by_role(bundled) != assets_by_role(copy)),
			("contracts", non_empty(&bundled.contracts) != non_empty(&copy.contracts)),
			("dotns", non_empty(&bundled.dotns) != non_empty(&copy.dotns)),
			("coinage", bundled.coinage != copy.coinage),
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

/// The descriptor's `assets` by role, empty when it names none. A descriptor
/// names each asset role once, so the order it lists them in says nothing.
fn assets_by_role(descriptor: &Descriptor) -> BTreeMap<&str, &Asset> {
	descriptor
		.assets
		.iter()
		.flatten()
		.map(|asset| (asset.role.as_str(), asset))
		.collect()
}

/// `map`, or `None` when it has no entries: `{}` names no more than a map
/// left out.
fn non_empty(map: &Option<BTreeMap<String, String>>) -> Option<&BTreeMap<String, String>> {
	map.as_ref().filter(|map| !map.is_empty())
}

#[cfg(test)]
mod tests;
