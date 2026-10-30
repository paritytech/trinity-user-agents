//! Retrying requests while a host redials a halted chain.

use core::{fmt::Debug, future::Future, time::Duration};

use futures::{FutureExt, pin_mut};
use subxt_rpcs::Error as RpcError;
use tracing::warn;
use truapi::CancellationToken;

use crate::time::Timer;

/// The `error.data` dotli sends while it redials a halted chain. dotli
/// documents it as safe to retry.
const CHAIN_HALTED: &str = "dotli:chain-halted";

/// Pauses between attempts after a halt. They double like dotli's redial
/// back-off, so each attempt follows another chance to redial.
pub const HALT_PAUSES: [Duration; 5] = [
	Duration::from_secs(1),
	Duration::from_secs(2),
	Duration::from_secs(4),
	Duration::from_secs(8),
	Duration::from_secs(16),
];

/// Whether `error` says the chain halted, so the request can be retried.
pub fn says_chain_halted(error: &RpcError) -> bool {
	let RpcError::User(error) = error else {
		return false;
	};
	error.data.as_deref().is_some_and(|data| {
		serde_json::from_str::<&str>(data.get()).is_ok_and(|data| data == CHAIN_HALTED)
	})
}

/// Run `attempt`, retrying after each of [`HALT_PAUSES`] while its failure
/// `is_retryable`. Returns the first other outcome, or the last failure.
///
/// Cancelling `until` ends a pause, not an attempt in flight, because a
/// dropped request can leave something open, such as a subscription. Use it
/// only for attempts that are safe to repeat.
pub async fn retry_through_halts<Answer, Failure, Attempt>(
	timer: &dyn Timer,
	until: &CancellationToken,
	mut attempt: impl FnMut() -> Attempt,
	is_retryable: impl Fn(&Failure) -> bool,
) -> Result<Answer, Failure>
where
	Attempt: Future<Output = Result<Answer, Failure>>,
	Failure: Debug,
{
	for pause in HALT_PAUSES {
		let failure = match attempt().await {
			Err(failure) if is_retryable(&failure) => failure,
			answer => return answer,
		};
		warn!(?pause, ?failure, "retrying after a chain halt");
		let paused = timer.sleep(pause).fuse();
		let cancelled = until.cancelled().fuse();
		pin_mut!(paused, cancelled);
		futures::select_biased! {
			_ = cancelled => return Err(failure),
			() = paused => {},
		}
	}
	attempt().await
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
		retry_through_halts(timer, until, attempt, says_chain_halted).await
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

		for (sleep, pause) in HALT_PAUSES[..2].iter().enumerate() {
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

		for (sleep, pause) in HALT_PAUSES.into_iter().enumerate() {
			wait_until(|| timer.sleeps() == sleep + 1, "the next pause starts");
			timer.advance(pause);
		}

		let error = ask.join().unwrap().expect_err("the chain stays halted");
		assert!(says_chain_halted(&error), "{error}");
		assert_eq!(timer.sleeps(), HALT_PAUSES.len());
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
