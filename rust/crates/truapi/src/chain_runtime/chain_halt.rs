//! Retrying requests while a host redials a halted chain.

use core::{fmt::Display, future::Future, ops::ControlFlow, time::Duration};

use subxt_rpcs::Error as RpcError;
use tracing::warn;
use truapi::CancellationToken;

use crate::time::{Backoff, Timer, retry_until_cancelled};

/// The `error.data` dotli sends while it redials a halted chain. dotli
/// documents it as safe to retry.
const CHAIN_HALTED_DATA: &str = "dotli:chain-halted";

/// Pauses between attempts after a halt: 1, 2, 4, 8, 16 and 16 seconds. Once
/// dotli's redial back-off has grown, it redials a chain at most every 30
/// seconds, so the last two attempts both come after that wait, the second
/// with time left for the redial to finish.
pub const HALT_PAUSES: Backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(16));

/// How many times an attempt is retried after a halt.
pub const HALT_RETRIES: usize = 6;

/// Whether `error` says the chain halted, so the request can be retried.
pub fn says_chain_halted(error: &RpcError) -> bool {
	let RpcError::User(error) = error else {
		return false;
	};
	error.data.as_deref().is_some_and(|data| {
		serde_json::from_str::<&str>(data.get()).is_ok_and(|data| data == CHAIN_HALTED_DATA)
	})
}

/// Run `attempt`, retrying up to [`HALT_RETRIES`] times on [`HALT_PAUSES`]
/// while its failure `is_retryable`. Only for attempts safe to repeat.
///
/// `cancel` firing ends a pause and returns the failure that started it, as
/// a lasting halt does, so check `cancel` to tell the two apart.
pub async fn retry_while_halted<Done, Failure, AttemptFuture>(
	mut attempt: impl FnMut() -> AttemptFuture,
	is_retryable: impl Fn(&Failure) -> bool,
	timer: &dyn Timer,
	cancel: &CancellationToken,
) -> Result<Done, Failure>
where
	AttemptFuture: Future<Output = Result<Done, Failure>>,
	Failure: Display,
{
	let is_retryable = &is_retryable;
	let mut attempts = 0;
	let mut pauses = HALT_PAUSES;
	let outcome = retry_until_cancelled(
		timer,
		cancel,
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
mod tests;
