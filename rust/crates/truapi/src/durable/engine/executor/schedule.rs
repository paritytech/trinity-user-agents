//! [`Schedule`]: when each waiting row of a domain may be handed to its
//! policy next.

use core::time::Duration;
use std::collections::{HashMap, HashSet};

use super::backoff::{ASK_AGAIN, cooldown};
use crate::durable::model::{DeferredDurableTx, DurableTxId};

/// The time from which each waiting row may be handed to the policy. Held in
/// the worker's memory only: a new worker starts every row afresh.
#[derive(Default)]
pub struct Schedule {
	rows: HashMap<DurableTxId, Due>,
}

/// When one row is due, for the build count it was seen waiting with.
struct Due {
	attempts: u32,
	at: Duration,
	unanswered: bool,
}

impl Schedule {
	/// Forgets rows no longer waiting and schedules each row seen for the
	/// first time with its build count: a first build is due at once, a
	/// rebuild after its cooldown.
	pub fn track(&mut self, waiting: &[DeferredDurableTx], now: Duration) {
		let ids: HashSet<DurableTxId> = waiting.iter().map(|row| row.id).collect();
		self.rows.retain(|id, _| ids.contains(id));
		for row in waiting {
			let seen = self.rows.get(&row.id).is_some_and(|due| due.attempts == row.attempts);
			if !seen {
				let at = now + cooldown(row.attempts);
				self.rows.insert(row.id, Due { attempts: row.attempts, at, unanswered: false });
			}
		}
	}

	/// Whether `id` may be handed to the policy at `now`.
	pub fn is_due(&self, id: DurableTxId, now: Duration) -> bool {
		self.rows.get(&id).is_some_and(|due| due.at <= now)
	}

	/// Asks about `ids` again only after [`ASK_AGAIN`], or sooner on a
	/// change in the domain: they were handed to the policy, or could not
	/// be, and are still waiting.
	pub fn ask_later(&mut self, ids: impl IntoIterator<Item = DurableTxId>, now: Duration) {
		for id in ids {
			if let Some(due) = self.rows.get_mut(&id) {
				due.at = now + ASK_AGAIN;
				due.unanswered = true;
			}
		}
	}

	/// Makes every row asked about without an answer due at once: a change
	/// in the domain may be what its policy was waiting for.
	pub fn domain_changed(&mut self, now: Duration) {
		for due in self.rows.values_mut().filter(|due| due.unanswered) {
			due.at = due.at.min(now);
		}
	}

	/// How long until the next row is due, or `None` while no row waits.
	pub fn until_next(&self, now: Duration) -> Option<Duration> {
		self.rows.values().map(|due| due.at.saturating_sub(now)).min()
	}
}
