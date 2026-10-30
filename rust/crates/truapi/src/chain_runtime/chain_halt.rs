//! Retrying requests while a host redials a halted chain.

use core::{fmt::Display, future::Future, ops::ControlFlow, time::Duration};

use subxt_rpcs::Error as RpcError;
use tracing::warn;
use truapi::CancellationToken;

use crate::time::{Backoff, Timer, retry_on_until};

/// The `error.data` dotli sends while it redials a halted chain. dotli
/// documents it as safe to retry.
const CHAIN_HALTED: &str = "dotli:chain-halted";

/// Pauses between attempts after a halt. They double like dotli's redial
/// back-off, so each attempt follows another chance to redial.
pub const HALT_PAUSES: Backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(16));

/// How many times an attempt is retried after a halt.
pub const HALT_RETRIES: usize = 5;

/// Whether `error` says the chain halted, so the request can be retried.
pub fn says_chain_halted(error: &RpcError) -> bool {
	let RpcError::User(error) = error else {
		return false;
	};
	error.data.as_deref().is_some_and(|data| {
		serde_json::from_str::<&str>(data.get()).is_ok_and(|data| data == CHAIN_HALTED)
	})
}

/// Run `attempt`, retrying up to [`HALT_RETRIES`] times on [`HALT_PAUSES`]
/// while its failure `is_retryable`. Only for attempts safe to repeat.
///
/// Cancelling `until` ends a pause and returns the failure that started it,
/// as a lasting halt does, so check `until` to tell the two apart.
pub async fn retry_while_halted<Done, Failure, AttemptFuture>(
	mut attempt: impl FnMut() -> AttemptFuture,
	is_retryable: impl Fn(&Failure) -> bool,
	timer: &dyn Timer,
	until: &CancellationToken,
) -> Result<Done, Failure>
where
	AttemptFuture: Future<Output = Result<Done, Failure>>,
	Failure: Display,
{
	let is_retryable = &is_retryable;
	let mut attempts = 0;
	let mut pauses = HALT_PAUSES;
	let outcome = retry_on_until(
		timer,
		until,
		|| {
			attempts += 1;
			let may_retry = attempts <= HALT_RETRIES;
			let attempt = attempt();
			async move {
				match attempt.await {
					Err(failure) if may_retry && is_retryable(&failure) => {
						ControlFlow::Continue(failure)
					},
					result => ControlFlow::Break(result),
				}
			}
		},
		|failure| {
			let pause = pauses.next_pause();
			warn!(?pause, %failure, "retrying while the chain recovers from a halt");
			pause
		},
	)
	.await;
	match outcome {
		Ok(result) => result,
		// Cancelled during a pause.
		Err(failure) => Err(failure),
	}
}

/// The pauses [`retry_while_halted`] takes, in order.
#[cfg(test)]
pub fn halt_schedule() -> Vec<Duration> {
	let mut pauses = HALT_PAUSES;
	(0..HALT_RETRIES).map(|_| pauses.next_pause()).collect()
}

#[cfg(test)]
mod tests {
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
		until: &CancellationToken,
		attempt: impl FnMut() -> Attempt,
	) -> Result<Answer, RpcError>
	where
		Attempt: Future<Output = Result<Answer, RpcError>>,
	{
		retry_while_halted(attempt, says_chain_halted, timer, until).await
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
		let until = CancellationToken::default();
		let attempts = Arc::new(AtomicUsize::new(0));
		let ask = {
			let (timer, until, attempts) = (timer.clone(), until.clone(), attempts.clone());
			thread::spawn(move || {
				block_on(retry(&*timer, &until, || {
					attempts.fetch_add(1, Ordering::SeqCst);
					ready(Err::<(), _>(halted()))
				}))
			})
		};
		wait_until(|| timer.sleeps() == 1, "the first pause starts");

		until.cancel();

		let error = ask.join().unwrap().expect_err("the caller gave up");
		assert!(says_chain_halted(&error), "{error}");
		assert_eq!(attempts.load(Ordering::SeqCst), 1);
	}
}
