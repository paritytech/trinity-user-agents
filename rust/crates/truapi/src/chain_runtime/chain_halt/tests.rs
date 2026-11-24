use std::{
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	},
	thread,
};

use futures::{executor::block_on, future::ready};

use super::*;
use crate::{
	test_support::{chain_halted_rpc_error as halted, rpc_refusal, wait_until},
	time::ManualTimer,
};

/// Retry while the chain says it halted, as the core's callers do.
async fn retry<Answer, Attempt>(
	timer: &dyn Timer,
	cancel: &CancellationToken,
	attempt: impl FnMut() -> Attempt,
) -> Result<Answer, RpcError>
where
	Attempt: Future<Output = Result<Answer, RpcError>>,
{
	retry_while_halted(attempt, says_chain_halted, timer, cancel).await
}

/// A long halt grows dotli's redial back-off to 30 seconds, so only the
/// attempts after that wait can reach a redialed chain. One that lands
/// just after it may find the redial still running.
#[test]
fn two_attempts_come_after_dotlis_longest_redial_wait() {
	let longest_redial_wait = Duration::from_secs(30);

	let attempts_after_it = halt_schedule()
		.into_iter()
		.scan(Duration::ZERO, |elapsed, pause| {
			*elapsed += pause;
			Some(*elapsed)
		})
		.filter(|elapsed| *elapsed > longest_redial_wait)
		.count();

	assert_eq!(attempts_after_it, 2);
}

/// Only dotli's marker counts. The same error code without it does not.
#[test]
fn only_the_halt_marker_says_the_chain_halted() {
	let answers = [
		halted(),
		rpc_refusal(None),
		rpc_refusal(Some(serde_json::json!("other"))),
		rpc_refusal(Some(serde_json::json!(7))),
	];

	let halts: Vec<bool> = answers.iter().map(says_chain_halted).collect();

	assert_eq!(halts, vec![true, false, false, false]);
}

/// Retries through each pause and stops at the first answer.
#[test]
fn a_request_is_asked_again_after_each_halt_until_it_is_answered() {
	let timer = Arc::new(ManualTimer::default());
	let attempts = Arc::new(AtomicUsize::new(0));
	let ask = {
		let (timer, attempts) = (timer.clone(), attempts.clone());
		thread::spawn(move || {
			block_on(retry(&*timer, &CancellationToken::default(), || {
				let attempt = attempts.fetch_add(1, Ordering::SeqCst);
				ready(if attempt < 2 { Err(halted()) } else { Ok(attempt) })
			}))
		})
	};

	for (sleep, pause) in halt_schedule()[..2].iter().enumerate() {
		wait_until(|| timer.sleeps() == sleep + 1, "the next pause starts");
		assert_eq!(timer.remaining(), vec![*pause]);
		timer.advance(*pause);
	}

	assert_eq!(ask.join().unwrap().unwrap(), 2);
}

/// A chain that stays halted is reported once the pauses run out.
#[test]
fn a_halt_that_outlasts_every_pause_is_returned() {
	let timer = Arc::new(ManualTimer::default());
	let ask = {
		let timer = timer.clone();
		thread::spawn(move || {
			block_on(retry(&*timer, &CancellationToken::default(), || {
				ready(Err::<(), _>(halted()))
			}))
		})
	};

	for (sleep, pause) in halt_schedule().into_iter().enumerate() {
		wait_until(|| timer.sleeps() == sleep + 1, "the next pause starts");
		timer.advance(pause);
	}

	wait_until(|| ask.is_finished(), "no pause follows the last retry");
	let error = ask.join().unwrap().expect_err("the chain stays halted");
	assert!(says_chain_halted(&error), "{error}");
	assert_eq!(timer.sleeps(), HALT_RETRIES);
}

/// Any other failure is returned at once.
#[test]
fn a_failure_other_than_a_halt_is_not_asked_again() {
	let timer = ManualTimer::default();
	let attempts = AtomicUsize::new(0);

	let result = block_on(retry(&timer, &CancellationToken::default(), || {
		attempts.fetch_add(1, Ordering::SeqCst);
		ready(Err::<(), _>(rpc_refusal(None)))
	}));

	assert!(result.is_err());
	assert_eq!((attempts.into_inner(), timer.sleeps()), (1, 0));
}

/// Cancelling during a pause returns at once and sends nothing more.
#[test]
fn a_cancellation_during_a_pause_returns_the_halt_without_asking_again() {
	let timer = Arc::new(ManualTimer::default());
	let cancel = CancellationToken::default();
	let attempts = Arc::new(AtomicUsize::new(0));
	let ask = {
		let (timer, cancel, attempts) = (timer.clone(), cancel.clone(), attempts.clone());
		thread::spawn(move || {
			block_on(retry(&*timer, &cancel, || {
				attempts.fetch_add(1, Ordering::SeqCst);
				ready(Err::<(), _>(halted()))
			}))
		})
	};
	wait_until(|| timer.sleeps() == 1, "the first pause starts");

	cancel.cancel();

	let error = ask.join().unwrap().expect_err("the caller gave up");
	assert!(says_chain_halted(&error), "{error}");
	assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
