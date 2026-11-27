//! [`RecentSlots`]: the slots this process consumed lately, kept from its
//! other claims until their attempts can no longer land.

use std::collections::{BTreeSet, HashMap};

use crate::pgas_claims::ClaimSlot;

/// The slots this process built claims in lately, with the unix time each
/// was consumed. Held in memory only: after a restart, a slot reused too soon
/// costs one refused attempt and a rebuild.
#[derive(Default)]
pub struct RecentSlots(HashMap<ClaimSlot, u64>);

impl RecentSlots {
	/// How long a consumed slot stays off limits to other claims, in seconds.
	/// Longer than the 64-block era an attempt is signed with, so a slot is
	/// handed out again only once its attempt landed or can no longer land.
	pub const HOLD_SECS: u64 = 15 * 60;

	/// Records that a claim was built in `slot` at unix time `now`, and
	/// forgets every slot consumed more than [`Self::HOLD_SECS`] before it.
	pub fn consume(&mut self, slot: ClaimSlot, now: u64) {
		let cutoff = now.saturating_sub(Self::HOLD_SECS);
		self.0.retain(|_, consumed_at| *consumed_at >= cutoff);
		self.0.insert(slot, now);
	}

	/// The slots still consumed at unix time `now`.
	pub fn consumed_slots(&self, now: u64) -> BTreeSet<ClaimSlot> {
		let cutoff = now.saturating_sub(Self::HOLD_SECS);
		self.0
			.iter()
			.filter(|(_, consumed_at)| **consumed_at >= cutoff)
			.map(|(slot, _)| *slot)
			.collect()
	}
}

#[cfg(test)]
mod tests;
