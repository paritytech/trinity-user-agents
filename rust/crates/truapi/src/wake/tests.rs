use super::*;

#[test]
fn wakes_sent_while_one_is_pending_collapse_into_it() {
	let signal = WakeSignal::default();
	let mut listener = signal.subscribe();

	signal.wake();
	signal.wake();

	assert!(listener.try_recv().is_ok());
	assert!(listener.try_recv().is_err());
}

#[test]
fn a_wake_with_no_listener_left_reports_that_nobody_heard_it() {
	let signal = WakeSignal::default();
	drop(signal.subscribe());

	assert!(!signal.wake());
}
