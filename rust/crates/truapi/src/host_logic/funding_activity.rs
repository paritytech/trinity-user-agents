//! A user's funding activity: sessions in flight, and the history of ended
//! ones, each as one row of facts.

use std::collections::HashSet;

use truapi::latest::{FundingDeposit, FundingDirection, FundingFailure, FundingRail};

use crate::host_logic::{
	funding::{
		FundingChoice, FundingProgress, FundingSession, FundingSessionSummary, FundingStage,
		FundingStatus,
	},
	funding_view::{status_delayed, status_in_flight},
};

/// What the user pays a provider with, or receives from it, in the
/// provider's asset.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingPaid {
	/// The amount, in the asset's smallest unit: what the user pays in, what
	/// the user receives out.
	pub amount: u128,
	/// The asset symbol.
	pub asset: String,
	/// The asset's decimals.
	pub decimals: u8,
	/// The network a crypto asset moves on.
	pub network: Option<String>,
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
	/// Whether the session is still in flight.
	pub in_flight: bool,
	/// Whether it is held up: retrying, taking longer, or waiting on a
	/// cancel the provider answered.
	pub delayed: bool,
	/// What the user paid or receives in the provider's asset.
	pub paid: Option<FundingPaid>,
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

/// `choice`'s amount in the provider's asset, for `direction`.
fn paid(
	choice: &FundingChoice,
	direction: FundingDirection,
	network: Option<String>,
) -> FundingPaid {
	FundingPaid {
		amount: match direction {
			FundingDirection::In => choice.quote.send_amount,
			FundingDirection::Out => choice.quote.receive_amount,
		},
		asset: choice.asset.clone(),
		decimals: choice.quote.asset_decimals,
		network: network.or_else(|| choice.network.clone()),
	}
}

/// The activity from the sessions the host heard of with their progress,
/// and the history of ended ones. A session is in flight while its status
/// says so; history leaves those out, and hides a session cancelled before
/// the user paid that settled and refunded nothing.
pub fn activity(
	sessions: &[(FundingSession, FundingProgress)],
	history: &[FundingSessionSummary],
) -> ActivityView {
	let mut in_flight: Vec<FundingActivityRow> = sessions
		.iter()
		.filter(|(_, progress)| status_in_flight(&progress.status))
		.map(|(session, progress)| {
			let debited = match session.stage {
				FundingStage::Released { debited, .. } => Some(debited),
				_ => None,
			};
			let network = match &progress.deposit {
				Some(FundingDeposit::Crypto { network, .. }) => Some(network.clone()),
				_ => None,
			};
			FundingActivityRow {
				intent: session.intent.clone(),
				direction: session.direction,
				rail: session.choice.as_ref().map(|choice| choice.rail),
				amount: debited.or(session.amount).or_else(|| {
					session.choice.as_ref().map(|choice| choice.balance_amount(session.direction))
				}),
				at_ms: session.opened_at_ms,
				status: progress.status.clone(),
				in_flight: true,
				delayed: status_delayed(&progress.status),
				paid: session
					.choice
					.as_ref()
					.map(|choice| paid(choice, session.direction, network)),
			}
		})
		.collect();
	in_flight.sort_by_key(|row| core::cmp::Reverse(row.at_ms));
	let flying: HashSet<&str> = in_flight.iter().map(|row| row.intent.as_str()).collect();
	let mut hidden = Vec::new();
	let mut ended = Vec::new();
	for summary in history.iter().filter(|summary| !flying.contains(summary.intent.as_str())) {
		let settled = match summary.stage {
			FundingStage::Delivered { credited, .. } => Some(credited),
			FundingStage::Released { debited, .. } => Some(debited),
			_ => None,
		};
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
			delayed: status_delayed(&status),
			status,
			in_flight: false,
			paid: summary.choice.as_ref().map(|choice| paid(choice, summary.direction, None)),
		});
	}
	ended.sort_by_key(|row| core::cmp::Reverse(row.at_ms));
	ActivityView { activity: FundingActivity { in_flight, history: ended }, hidden }
}

#[cfg(test)]
mod tests;
