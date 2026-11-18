//! What no newer copy of a stack's descriptor may change.
//!
//! [`Pinned`] is read from the copy a host build bundles.
//! [`Pinned::refuse_change`] holds every copy the core stored or fetched to
//! it.

use std::collections::BTreeMap;

use trinity_stack::{Asset, Chain, Coinage, Descriptor};

use super::{ReservedValuesPolicy, StackError};

/// The fields of a stack's bundled descriptor that every other copy must
/// leave as they are.
///
/// The bundled copy ships inside the build, so the key that signs the
/// stack's descriptors cannot change it. A copy signed with a stolen key
/// therefore cannot move the stack off a pinned value. The TLD is pinned on
/// every stack: the signing role derives its accounts under it.
///
/// A stack its build allows reserved values runs on Polkadot or a fork of
/// it, so it also pins what decides which chains, assets and contracts the
/// host uses: every chain's genesis hash, `assets`, `contracts`, `dotns`,
/// `coinage` and the digest of the relay's light spec, the file that
/// carries the relay's sync checkpoint.
#[derive(Clone)]
pub struct Pinned {
	tld: String,
	/// What a stack allowed reserved values pins besides its TLD, and `None`
	/// for any other stack.
	reserved: Option<ReservedStackFields>,
}

impl Pinned {
	/// What `bundled`, the copy the build ships, pins under the build's
	/// `reserved_values` policy.
	pub fn new(bundled: &Descriptor, reserved_values: &ReservedValuesPolicy) -> Self {
		Self {
			tld: bundled.tld.clone(),
			reserved: reserved_values.allowed.then(|| ReservedStackFields::of(bundled)),
		}
	}

	/// Checks that `copy` leaves every pinned field as the bundled copy has
	/// it.
	///
	/// Fails with [`StackError::PinnedFieldChanged`] naming the first field
	/// that differs.
	pub fn refuse_change(&self, copy: &Descriptor) -> Result<(), StackError> {
		if copy.tld != self.tld {
			return Err(StackError::PinnedFieldChanged { field: "tld" });
		}
		let changed = self
			.reserved
			.as_ref()
			.and_then(|reserved| reserved.changed_field(&ReservedStackFields::of(copy)));
		match changed {
			Some(field) => Err(StackError::PinnedFieldChanged { field }),
			None => Ok(()),
		}
	}
}

/// The fields a stack allowed reserved values pins besides its TLD, as one
/// copy of its descriptor has them.
#[derive(Clone)]
struct ReservedStackFields {
	/// Each chain's genesis hash by its role, for every role the descriptor
	/// names. A role added or left out is a change like another hash is.
	genesis_hashes: BTreeMap<String, [u8; 32]>,
	assets: Option<Vec<Asset>>,
	contracts: Option<BTreeMap<String, String>>,
	dotns: Option<BTreeMap<String, String>>,
	coinage: Option<Coinage>,
	/// The digest of the relay's light spec, or `None` when the relay names
	/// no light spec. The spec's URL is not pinned: the digest fixes what any
	/// server may serve.
	relay_light_spec_sha256: Option<String>,
}

impl ReservedStackFields {
	fn of(descriptor: &Descriptor) -> Self {
		let genesis_of = |chain: &Chain| (chain.role.clone(), chain.genesis());
		let relay_light_spec = descriptor.relay().and_then(|relay| relay.light_spec.as_ref());
		Self {
			genesis_hashes: descriptor.chains.iter().map(genesis_of).collect(),
			assets: descriptor.assets.clone(),
			contracts: descriptor.contracts.clone(),
			dotns: descriptor.dotns.clone(),
			coinage: descriptor.coinage.clone(),
			relay_light_spec_sha256: relay_light_spec.map(|spec| spec.sha256.clone()),
		}
	}

	/// The first field `copy` has another value in, as the descriptor spells
	/// it, or `None` when all are equal.
	fn changed_field(&self, copy: &Self) -> Option<&'static str> {
		let Self { genesis_hashes, assets, contracts, dotns, coinage, relay_light_spec_sha256 } =
			self;
		[
			("a chain's genesisHash", *genesis_hashes != copy.genesis_hashes),
			("assets", *assets != copy.assets),
			("contracts", *contracts != copy.contracts),
			("dotns", *dotns != copy.dotns),
			("coinage", *coinage != copy.coinage),
			(
				"the relay's lightSpec.sha256",
				*relay_light_spec_sha256 != copy.relay_light_spec_sha256,
			),
		]
		.into_iter()
		.find_map(|(field, changed)| changed.then_some(field))
	}
}

#[cfg(test)]
mod tests;
