use truapi::platform::ProductId;

use super::WorkerLaunchTable;

// A launch's exit is reported only while it is the live one: a stopped
// launch, or one a later launch replaced, exits quietly.
#[test]
fn only_the_live_launch_reports_its_exit() {
	let table = WorkerLaunchTable::default();
	let ramp = || ProductId::for_test("ramp.dot");
	let first = table.begin(ramp()).expect("first launch");
	let while_live = table.begin(ramp());
	let stopped = table.stop(ramp());
	let first_exit = table.ended(ramp(), first);
	let second = table.begin(ramp()).expect("second launch");
	let stale_exit = table.ended(ramp(), first);
	let second_exit = table.ended(ramp(), second);
	let again = table.ended(ramp(), second);

	assert_eq!(
		(while_live, stopped, first_exit, second > first, stale_exit, second_exit, again),
		(None, Some(first), false, true, false, true, false)
	);
}
