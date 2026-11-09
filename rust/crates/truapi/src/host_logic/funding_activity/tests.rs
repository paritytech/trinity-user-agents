use truapi::latest::FundingQuote;

use super::*;
use crate::host_logic::funding::{FundingChoice, FundingStage};

fn choice() -> FundingChoice {
	FundingChoice {
		quote: FundingQuote {
			quote_id: "q".to_string(),
			send_amount: 5_100,
			receive_amount: 50_000_000,
			provider_fee: 100,
			network_fee: 0,
			eta_secs: None,
			expires_at: None,
			asset_decimals: 2,
		},
		rail: FundingRail::Bank,
		asset: "EUR".to_string(),
		network: None,
	}
}

fn summary(
	intent: &str,
	stage: FundingStage,
	status: FundingStatus,
	refunded: Option<u128>,
) -> FundingSessionSummary {
	FundingSessionSummary {
		intent: intent.to_string(),
		owner_product_id: None,
		direction: FundingDirection::In,
		amount: Some(50_000_000),
		stage,
		opened_at_ms: 1,
		provider_id: None,
		choice: Some(choice()),
		status,
		payout: None,
		refunded,
		transaction_id: None,
		reference: None,
		ended: Default::default(),
	}
}

fn failed(reason: FundingFailure, at: u64) -> FundingStage {
	FundingStage::Failed { reason, settled_at_ms: at }
}

// Divergence "Hide cancelled-before-paying in history": only when nothing
// settled or was refunded, and a cancel that did refund reads as cancelled.
#[test]
fn a_cancel_before_paying_is_hidden_unless_something_came_back() {
	let cancelled = FundingStatus::Failed { reason: FundingFailure::Cancelled };
	let history = [
		summary("unpaid", failed(FundingFailure::Cancelled, 30), cancelled.clone(), None),
		summary("refunded", failed(FundingFailure::Cancelled, 20), cancelled, Some(49_000_000)),
		summary(
			"added",
			FundingStage::Delivered { credited: 50_000_000, settled_at_ms: 40 },
			FundingStatus::ToppedUp,
			None,
		),
		summary(
			"expired",
			failed(FundingFailure::Expired, 10),
			FundingStatus::Failed { reason: FundingFailure::Expired },
			None,
		),
	];
	let view = activity(&[], &history);
	assert_eq!(view.hidden, ["unpaid"]);
	assert_eq!(
		view.activity
			.history
			.iter()
			.map(|row| (row.intent.as_str(), row.status.clone(), row.amount, row.at_ms))
			.collect::<Vec<_>>(),
		vec![
			("added", FundingStatus::ToppedUp, Some(50_000_000), 40),
			(
				"refunded",
				FundingStatus::Cancelled { refunded: Some(49_000_000) },
				Some(50_000_000),
				20
			),
			(
				"expired",
				FundingStatus::Failed { reason: FundingFailure::Expired },
				Some(50_000_000),
				10
			),
		]
	);
}

#[test]
fn sessions_in_flight_lead_and_leave_history() {
	let mut open = FundingSession::new("open".to_string(), None, FundingDirection::Out, None, 5);
	open.choice = Some(choice());
	let progress = open.progress(5);
	let mut older =
		FundingSession::new("older".to_string(), None, FundingDirection::In, Some(7), 1);
	older.choice = Some(choice());
	let older_progress = older.progress(1);
	let history = [summary(
		"open",
		FundingStage::Delivered { credited: 1, settled_at_ms: 9 },
		FundingStatus::ToppedUp,
		None,
	)];
	let entries = [
		FundingBookEntry { session: older, progress: older_progress },
		FundingBookEntry { session: open, progress },
	];
	let view = activity(&entries, &history);
	assert_eq!(
		view.activity
			.in_flight
			.iter()
			.map(|row| (row.intent.as_str(), row.amount, row.status.clone()))
			.collect::<Vec<_>>(),
		vec![
			("open", Some(5_100), FundingStatus::TransactionInitiated),
			("older", Some(7), FundingStatus::WaitingForTransfer),
		]
	);
	assert!(view.activity.history.is_empty());
}
