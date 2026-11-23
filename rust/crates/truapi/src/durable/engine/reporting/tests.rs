use std::sync::Arc;

use futures::{FutureExt, StreamExt, executor::block_on};

use crate::{
	durable::{
		engine::DurableRequest,
		model::DomainId,
		testing::{FakeChain, extrinsic, insert, test_engine, without_plan},
	},
	test_support::wait_until,
};

/// A watch that hands its transaction back while no recovery runs must
/// ask the host for recovery again, although the ledger stayed live.
#[test]
fn a_watch_ending_with_no_recovery_running_asks_the_host_again() {
	let chain = FakeChain::new(130, 140);
	let events = chain.script_watch();
	let (_dir, engine, _timer) = test_engine(&chain);
	let mut live = engine.live_work();
	assert!(!block_on(live.next()).unwrap().unwrap());
	let request = DurableRequest::presigned(
		DomainId::new("test"),
		None,
		without_plan(vec![extrinsic(1, 100, 64)]),
	)
	.unwrap();
	block_on(engine.execute(request)).unwrap();
	assert!(block_on(live.next()).unwrap().unwrap());

	drop(events);

	wait_until(
		|| matches!(live.next().now_or_never(), Some(Some(Ok(true)))),
		"the host is asked for recovery again",
	);
}

/// The reporter must not outlive the engine: it would keep the database
/// and its connection threads alive after the runtime is gone.
#[test]
fn work_reporting_stops_when_the_engine_is_dropped() {
	let chain = FakeChain::new(130, 140);
	let (_dir, engine, _timer) = test_engine(&chain);
	let reported = Arc::new(parking_lot::Mutex::new(Vec::new()));
	let sink = reported.clone();
	engine.report_work_to(move |pending| sink.lock().push(pending));
	wait_until(|| *reported.lock() == vec![false], "the current state is reported");

	drop(engine);

	wait_until(|| Arc::strong_count(&reported) == 1, "the reporter is dropped with the engine");
}

/// A domain may be missing from the registry for one start. Its live rows
/// are left alone until it is back, so they must not keep the host running
/// recovery that returns at once.
#[test]
fn live_rows_of_an_unregistered_domain_are_no_work() {
	let chain = FakeChain::new(130, 140);
	let (_dir, engine, _timer) = test_engine(&chain);
	insert(&engine.db, &DomainId::new("orphan"), extrinsic(1, 100, 64));

	assert!(!block_on(engine.live_work().next()).unwrap().unwrap());
}
