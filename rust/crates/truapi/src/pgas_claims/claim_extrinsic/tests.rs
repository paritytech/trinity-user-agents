use super::*;

/// A slot excluded yesterday is free again today, so only today's
/// exclusions reach the scan.
#[test]
fn only_the_days_own_excluded_slots_are_skipped() {
	let excluded = BTreeSet::from([
		ClaimSlot { day: 9, slot_index: 0 },
		ClaimSlot { day: 10, slot_index: 2 },
		ClaimSlot { day: 10, slot_index: 5 },
		ClaimSlot { day: 11, slot_index: 1 },
	]);

	assert_eq!(slot_indices_on_day(&excluded, 10), vec![2, 5]);
}
