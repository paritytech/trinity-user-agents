//! How long a worker waits: after a failed read, before asking its policy
//! again about a row it did not answer, and before building a row again.

use core::time::Duration;

/// The wait after the first failed read.
pub const INITIAL_BACKOFF: Duration = Duration::from_secs(2);
/// The longest wait between failed reads.
pub const MAX_BACKOFF: Duration = Duration::from_secs(120);
/// How long a row the policy did not answer waits before it is asked about
/// again, unless a change in its domain comes first.
pub const ASK_AGAIN: Duration = Duration::from_secs(30);

/// The wait before building a row again after its first attempt.
pub const REBUILD_COOLDOWN: Duration = Duration::from_secs(5);
/// The longest wait before building a row again.
pub const MAX_REBUILD_COOLDOWN: Duration = Duration::from_secs(600);
/// Enough doublings for every base here to pass its cap, and few enough that
/// the multiplication cannot overflow.
const MAX_DOUBLINGS: u32 = 10;

/// The wait after `rounds` failed reads in a row.
pub fn backoff(rounds: u32) -> Duration {
	let doublings = rounds.saturating_sub(1);
	exponential(INITIAL_BACKOFF, MAX_BACKOFF, doublings)
}

/// The wait before building a row that was already built into `attempts`
/// attempts.
pub fn cooldown(attempts: u32) -> Duration {
	if attempts == 0 {
		return Duration::ZERO;
	}
	exponential(REBUILD_COOLDOWN, MAX_REBUILD_COOLDOWN, attempts - 1)
}

/// `base` doubled `doublings` times, capped at `max`.
fn exponential(base: Duration, max: Duration, doublings: u32) -> Duration {
	let doubled = base * (1 << doublings.min(MAX_DOUBLINGS));
	doubled.min(max)
}

#[cfg(test)]
mod tests;
