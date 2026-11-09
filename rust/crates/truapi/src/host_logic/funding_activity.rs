//! A user's funding activity: sessions in flight, and the history of ended
//! ones, each as one row of facts.

use std::collections::HashSet;

use truapi::latest::{FundingDirection, FundingFailure, FundingRail};

use crate::host_logic::{
	funding::{FundingProgress, FundingSession, FundingSessionSummary, FundingStatus},
	funding_view::status_in_flight,
};

/// A session the host has heard of, with its progress now.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingBookEntry {
	/// The session.
	pub session: FundingSession,
	/// Its progress now.
	pub progress: FundingProgress,
}

/// One session in the host's funding activity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingActivityRow {
	/// The session.
	pub intent: String,
	/// Which way value moves.
	pub direction: FundingDirection,
	/// The rail it went by, once a provider was chosen.
	pub rail: Option<FundingRail>,
	/// The CASH amount, in balance units: in flight, what the session moves;
	/// ended, what was credited or debited, else what was asked.
	pub amount: Option<u128>,
	/// When the session opened, in flight, or ended, in Unix milliseconds.
	pub at_ms: u64,
	/// Where the session stands or how it ended. A cancel before paying that
	/// still settled or refunded something reads as cancelled.
	pub status: FundingStatus,
}

/// The user's funding activity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingActivity {
	/// Sessions in flight, newest opened first.
	pub in_flight: Vec<FundingActivityRow>,
	/// Ended sessions, newest ended first.
	pub history: Vec<FundingActivityRow>,
}

/// The activity, and the ended sessions it leaves out of history.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActivityView {
	/// The activity.
	pub activity: FundingActivity,
	/// Ended sessions left out: cancelled before the user paid, with nothing
	/// settled or refunded.
	pub hidden: Vec<String>,
}

/// The activity from the sessions the host heard of with their progress,
/// and the history of ended ones. A session is in flight while its status
/// says so; history leaves those out, and hides a session cancelled before
/// the user paid that settled and refunded nothing.
pub fn activity(entries: &[FundingBookEntry], history: &[FundingSessionSummary]) -> ActivityView {
	let mut in_flight: Vec<FundingActivityRow> = entries
		.iter()
		.filter(|entry| status_in_flight(&entry.progress.status))
		.map(|FundingBookEntry { session, progress }| FundingActivityRow {
			intent: session.intent.clone(),
			direction: session.direction,
			rail: session.choice.as_ref().map(|choice| choice.rail),
			// In flight, only a released withdrawal has settled an amount.
			amount: session.stage.settled_amount().or(session.amount).or_else(|| {
				session.choice.as_ref().map(|choice| choice.balance_amount(session.direction))
			}),
			at_ms: session.opened_at_ms,
			status: progress.status.clone(),
		})
		.collect();
	in_flight.sort_by_key(|row| core::cmp::Reverse(row.at_ms));
	let flying: HashSet<&str> = in_flight.iter().map(|row| row.intent.as_str()).collect();
	let mut hidden = Vec::new();
	let mut ended = Vec::new();
	for summary in history.iter().filter(|summary| !flying.contains(summary.intent.as_str())) {
		let settled = summary.stage.settled_amount();
		let cancelled =
			summary.status == FundingStatus::Failed { reason: FundingFailure::Cancelled };
		if cancelled && settled.is_none() && summary.refunded.is_none() {
			hidden.push(summary.intent.clone());
			continue;
		}
		let status = if cancelled {
			FundingStatus::Cancelled { refunded: summary.refunded }
		} else {
			summary.status.clone()
		};
		ended.push(FundingActivityRow {
			intent: summary.intent.clone(),
			direction: summary.direction,
			rail: summary.choice.as_ref().map(|choice| choice.rail),
			amount: settled.or(summary.amount),
			at_ms: summary.settled_at_ms().unwrap_or(summary.opened_at_ms),
			status,
		});
	}
	ended.sort_by_key(|row| core::cmp::Reverse(row.at_ms));
	ActivityView { activity: FundingActivity { in_flight, history: ended }, hidden }
}

#[cfg(test)]
mod tests;
