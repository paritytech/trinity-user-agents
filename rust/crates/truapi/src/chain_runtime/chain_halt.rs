//! Asking a host again while it redials a halted chain.

use core::{future::Future, time::Duration};

use futures::{FutureExt, pin_mut};
use subxt_rpcs::Error as RpcError;
use tracing::warn;
use truapi::CancellationToken;

use crate::time::Timer;

/// The `error.data` dotli answers a request with while it waits to redial a
/// halted chain, which it documents as worth asking again.
const CHAIN_HALTED: &str = "dotli:chain-halted";

/// Pauses before each new attempt after a halt. dotli lets the first redial
/// of a halted chain through at once, then waits one second and doubles the
/// wait each time. These pauses double the same way, so each attempt comes
/// after another chance to redial.
pub const HALT_PAUSES: [Duration; 5] = [
	Duration::from_secs(1),
	Duration::from_secs(2),
	Duration::from_secs(4),
	Duration::from_secs(8),
	Duration::from_secs(16),
];

/// Whether `error` is a host saying the chain halted, so the same request can
/// succeed once the chain is redialed.
pub fn says_chain_halted(error: &RpcError) -> bool {
	let RpcError::User(error) = error else {
		return false;
	};
	error.data.as_deref().is_some_and(|data| {
		serde_json::from_str::<&str>(data.get()).is_ok_and(|data| data == CHAIN_HALTED)
	})
}

/// Run `request`, and again after each of [`HALT_PAUSES`] while the host says
/// the chain halted. Any other outcome, and a halt that outlasts every pause,
/// is returned as it is.
///
/// Cancelling `until` ends a pause and returns the halt that started it. It
/// never drops a request in flight, whose answer may open something only that
/// answer can close, such as a subscription.
///
/// Only for a request that changes nothing when a halt answers it, since the
/// host may have passed it on before the chain halted.
pub async fn ask_through_halts<Answer, Request>(
	timer: &dyn Timer,
	until: &CancellationToken,
	mut request: impl FnMut() -> Request,
) -> Result<Answer, RpcError>
where
	Request: Future<Output = Result<Answer, RpcError>>,
{
	for pause in HALT_PAUSES {
		let halt = match request().await {
			Err(error) if says_chain_halted(&error) => error,
			answer => return answer,
		};
		warn!(?pause, error = %halt, "the chain halted; asking again");
		let paused = timer.sleep(pause).fuse();
		let cancelled = until.cancelled().fuse();
		pin_mut!(paused, cancelled);
		futures::select_biased! {
			_ = cancelled => return Err(halt),
			() = paused => {},
		}
	}
	request().await
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
	use serde_json::value::RawValue;
	use subxt_rpcs::UserError;

	use super::*;
	use crate::{test_support::wait_until, time::ManualTimer};

	fn refusal(data: Option<&str>) -> RpcError {
		RpcError::User(UserError {
			code: -32603,
			message: "refused".to_string(),
			data: data.map(|data| RawValue::from_string(data.to_string()).unwrap()),
		})
	}

	fn halted() -> RpcError {
		refusal(Some(r#""dotli:chain-halted""#))
	}

	/// Only dotli's marker says a later attempt can succeed. The same
	/// internal-error code without it, or with other data, says nothing.
	#[test]
	fn only_the_halt_marker_says_the_chain_halted() {
		let answers = [halted(), refusal(None), refusal(Some(r#""other""#)), refusal(Some("7"))];

		let halts: Vec<bool> = answers.iter().map(says_chain_halted).collect();

		assert_eq!(halts, vec![true, false, false, false]);
	}

	/// Asks while the halts last, sleeping each pause in turn, and stops at
	/// the first answer that is not a halt.
	#[test]
	fn a_request_is_asked_again_after_each_halt_until_it_is_answered() {
		let timer = Arc::new(ManualTimer::default());
		let attempts = Arc::new(AtomicUsize::new(0));
		let ask = {
			let (timer, attempts) = (timer.clone(), attempts.clone());
			thread::spawn(move || {
				block_on(ask_through_halts(&*timer, &CancellationToken::default(), || {
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

	/// A chain that stays halted is reported, so the caller is not held
	/// longer than the pauses add up to.
	#[test]
	fn a_halt_that_outlasts_every_pause_is_returned() {
		let timer = Arc::new(ManualTimer::default());
		let ask = {
			let timer = timer.clone();
			thread::spawn(move || {
				block_on(ask_through_halts(&*timer, &CancellationToken::default(), || {
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

	/// Any other failure is final, so it reaches the caller at once.
	#[test]
	fn a_failure_other_than_a_halt_is_not_asked_again() {
		let timer = ManualTimer::default();
		let attempts = AtomicUsize::new(0);

		let result = block_on(ask_through_halts(&timer, &CancellationToken::default(), || {
			attempts.fetch_add(1, Ordering::SeqCst);
			ready(Err::<(), _>(refusal(None)))
		}));

		assert!(result.is_err());
		assert_eq!((attempts.into_inner(), timer.sleeps()), (1, 0));
	}

	/// A caller that gives up during a pause is answered at once, with the
	/// halt it was waiting out, and nothing more is sent for it.
	#[test]
	fn a_cancellation_during_a_pause_returns_the_halt_without_asking_again() {
		let timer = Arc::new(ManualTimer::default());
		let until = CancellationToken::default();
		let attempts = Arc::new(AtomicUsize::new(0));
		let ask = {
			let (timer, until, attempts) = (timer.clone(), until.clone(), attempts.clone());
			thread::spawn(move || {
				block_on(ask_through_halts(&*timer, &until, || {
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
