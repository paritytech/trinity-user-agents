use super::*;

// The battery seeds one outcome per request it makes, in order. An entry
// this host cannot read is skipped, and the entries after it keep their
// order.
#[test]
fn outcomes_are_read_in_order_and_unknown_entries_dropped() {
	assert_eq!(
		parse_outcomes("deliver:900, release:500,fail,bogus,dismiss,deliver:x"),
		VecDeque::from([
			Outcome::Deliver(900),
			Outcome::Release(500),
			Outcome::Fail,
			Outcome::Dismiss,
		])
	);
}
