use super::*;

fn slot(slot_index: u32) -> ClaimSlot {
	ClaimSlot { day: 20_412, slot_index }
}

#[test]
fn a_slot_consumed_within_the_hold_is_still_consumed() {
	let mut recent = RecentSlots::default();
	recent.consume(slot(0), 1_000);
	recent.consume(slot(1), 1_000 + RecentSlots::HOLD_SECS);

	let consumed = recent.consumed_slots(1_000 + RecentSlots::HOLD_SECS);

	assert_eq!(consumed, BTreeSet::from([slot(0), slot(1)]));
}

/// Once an attempt landed or can no longer land, its slot is handed out to
/// other claims again.
#[test]
fn a_slot_consumed_longer_ago_than_the_hold_is_forgotten() {
	let mut recent = RecentSlots::default();
	recent.consume(slot(0), 1_000);
	let later = 1_000 + RecentSlots::HOLD_SECS + 1;

	recent.consume(slot(1), later);

	assert_eq!(recent.consumed_slots(later), BTreeSet::from([slot(1)]));
}
