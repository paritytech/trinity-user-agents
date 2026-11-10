use super::*;

use std::sync::atomic::{AtomicUsize, Ordering};

use futures::{channel::oneshot, executor::block_on};
use v01::{
	HostPaymentBalanceSubscribeError as BalanceError,
	HostPaymentTopUpStatusSubscribeError as TopUpError,
	HostPaymentTopUpStatusSubscribeItem as TopUp,
};

use crate::subscription::thread_per_subscription_spawner;

/// A host engine whose follows run until the test ends them, in the order
/// the core asked for them.
struct Engine<E> {
	asked: AtomicUsize,
	ends: Mutex<Vec<oneshot::Sender<Result<(), E>>>>,
}

impl<E: Send + 'static> Engine<E> {
	fn new() -> Arc<Self> {
		Arc::new(Self { asked: AtomicUsize::new(0), ends: Mutex::new(Vec::new()) })
	}

	/// Start one follow, returning when the test ends it.
	fn follow(&self) -> BoxFuture<'static, Result<(), E>> {
		self.asked.fetch_add(1, Ordering::SeqCst);
		let (end, ended) = oneshot::channel();
		self.ends.lock().expect("ends").push(end);
		async move { ended.await.unwrap_or(Ok(())) }.boxed()
	}

	fn asked(&self) -> usize {
		self.asked.load(Ordering::SeqCst)
	}

	/// End follow `index` with `result`.
	fn end(&self, index: usize, result: Result<(), E>) {
		let end =
			core::mem::replace(&mut self.ends.lock().expect("ends")[index], oneshot::channel().0);
		let _ = end.send(result);
	}

	/// Wait until the core stopped follow `index`.
	fn wait_stopped(&self, index: usize) {
		for _ in 0..200 {
			if self.ends.lock().expect("ends")[index].is_canceled() {
				return;
			}
			std::thread::sleep(Duration::from_millis(5));
		}
		panic!("follow {index} was not stopped");
	}
}

fn top_ups(engine: &Arc<Engine<TopUpError>>, spawner: Spawner) -> StatusFollower<TopUp> {
	let engine = engine.clone();
	StatusFollower::new(Arc::new(move |_, _| engine.follow()), spawner)
}

/// Never runs what it is given, for tests that only count what was asked.
fn idle() -> Spawner {
	Arc::new(|_| {})
}

fn product() -> ProductId {
	ProductId::for_test("fund.dot")
}

// A product following a top-up the engine is slow to report sees the pending
// status after the first-status wait, then each later status once, never
// stepping back, and its stream ends at the final one.
#[test]
fn a_slow_top_up_shows_pending_then_each_status_up_to_the_final_one() {
	let engine = Engine::new();
	let follower = top_ups(&engine, idle());
	let mut stream = follower.subscribe(product(), [1; 32]);
	let first = block_on(stream.next());
	for status in [
		TopUp::Detecting,
		TopUp::Claiming,
		TopUp::Claimed { finalized: false },
		TopUp::Claiming,
		TopUp::Claimed { finalized: true },
	] {
		follower.notify(product(), [1; 32], status);
	}

	assert_eq!(
		(first, block_on(stream.collect::<Vec<_>>()), engine.asked()),
		(
			Some(Ok(TopUp::Detecting)),
			vec![
				Ok(TopUp::Claiming),
				Ok(TopUp::Claimed { finalized: false }),
				Ok(TopUp::Claimed { finalized: true }),
			],
			1,
		)
	);
}

// A final status is kept, so a later subscription or follow of the same id
// answers it at once without asking the host again.
#[test]
fn a_final_status_answers_later_subscriptions_without_the_host() {
	let engine = Engine::new();
	let follower = top_ups(&engine, idle());
	follower.follow(product(), [1; 32]);
	follower.notify(product(), [1; 32], TopUp::NotClaimed);
	follower.follow(product(), [1; 32]);

	assert_eq!(
		(block_on(follower.subscribe(product(), [1; 32]).collect::<Vec<_>>()), engine.asked()),
		(vec![Ok(TopUp::NotClaimed)], 1)
	);
}

// Only the most recent final statuses are kept: the oldest one is asked of
// the host again, the newest one is not.
#[test]
fn only_the_most_recent_final_statuses_are_kept() {
	let engine = Engine::new();
	let follower = top_ups(&engine, idle());
	let id = |index: usize| {
		let mut id = [0; 32];
		id[..8].copy_from_slice(&(index as u64).to_le_bytes());
		id
	};
	for index in 0..=FINALS_KEPT {
		follower.follow(product(), id(index));
		follower.notify(product(), id(index), TopUp::NotClaimed);
	}
	let asked = engine.asked();
	follower.follow(product(), id(FINALS_KEPT));
	let newest_asked = engine.asked() - asked;
	follower.follow(product(), id(0));

	assert_eq!((newest_asked, engine.asked() - asked), (0, 1));
}

// An engine that stops before a final status: a stream that saw a status ends
// with the abandoned one; one that saw none gets the engine's not found, or the
// fixed unavailable error for any other failure. The id is then asked again.
#[test]
fn an_engine_ending_early_abandons_or_fails_its_streams() {
	let engine = Engine::new();
	let follower = top_ups(&engine, thread_per_subscription_spawner());

	let shown = follower.subscribe(product(), [1; 32]);
	follower.notify(product(), [1; 32], TopUp::Claiming);
	engine.end(0, Ok(()));
	let not_found = follower.subscribe(product(), [2; 32]);
	engine.end(1, Err(TopUpError::TopUpNotFound));
	let failed = follower.subscribe(product(), [3; 32]);
	engine.end(2, Err(TopUpError::Unknown { reason: "engine down".into() }));
	let outcomes = (
		block_on(shown.collect::<Vec<_>>()),
		block_on(not_found.collect::<Vec<_>>()),
		block_on(failed.collect::<Vec<_>>()),
	);
	drop(follower.subscribe(product(), [2; 32]));

	assert_eq!(
		(outcomes, engine.asked()),
		(
			(
				vec![Ok(TopUp::Claiming), Ok(TopUp::NotClaimed)],
				vec![Err(TopUpError::TopUpNotFound)],
				vec![Err(TopUpError::Unknown { reason: STATUS_UNAVAILABLE.into() })],
			),
			4,
		)
	);
}

fn balance(engine: &Arc<Engine<HostRejection>>) -> BalanceFollower {
	let engine = engine.clone();
	BalanceFollower::new(Arc::new(move || engine.follow()), thread_per_subscription_spawner())
}

// The balance is followed once while any product listens: a later product
// starts from the latest balance, an unchanged one is not repeated, and the
// follow stops when the last product stream is dropped and starts again with
// the next one.
#[test]
fn the_balance_is_followed_once_while_a_product_listens() {
	let engine = Engine::new();
	let follower = balance(&engine);
	let mut first = follower.subscribe();
	follower.notify(10);
	let first_seen = block_on(first.next());
	let mut second = follower.subscribe();
	follower.notify(10);
	follower.notify(25);
	let seen = (
		first_seen,
		block_on(first.next()),
		block_on(second.next()),
		block_on(second.next()),
		engine.asked(),
	);
	drop((first, second));
	engine.wait_stopped(0);
	drop(follower.subscribe());

	assert_eq!(
		(seen, engine.asked()),
		((Some(Ok(10)), Some(Ok(25)), Some(Ok(10)), Some(Ok(25)), 1), 2)
	);
}

// A balance follow that ends ends every product stream with the fixed
// unavailable error; the core does not retry, and the next product asks
// again.
#[test]
fn an_ended_balance_follow_fails_its_streams_until_asked_again() {
	let engine = Engine::new();
	let follower = balance(&engine);
	let stream = follower.subscribe();
	engine.end(0, Ok(()));
	let ended = block_on(stream.collect::<Vec<_>>());
	drop(follower.subscribe());

	assert_eq!(
		(ended, engine.asked()),
		(vec![Err(BalanceError::Unknown { reason: BALANCE_UNAVAILABLE.into() })], 2)
	);
}
