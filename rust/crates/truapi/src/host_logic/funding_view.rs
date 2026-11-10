//! Funding rules every host shares: which rails and assets a session can
//! use, whether an amount can go ahead, whether a session can be cancelled,
//! and how an ended session went.
//!
//! These are facts and rules only. Each host decides how to present them.

use truapi::latest::{
	FundingDirection, FundingFailure, FundingPayout, FundingQuoteRefusal, FundingRail,
};

use crate::{
	host_logic::{
		funding::{FundingSessionSummary, FundingStage, FundingStatus, FundingStep},
		funding_providers::{FundingAmountLimits, FundingCandidate},
	},
	platform::{
		FundingAmountIssue, FundingFrameContext, FundingQuoteRow, FundingQuoteState,
		FundingQuoteUnavailable,
	},
};

/// The frame facts for a session moving value in `direction` by `rail`:
/// the user makes the transfer themselves, outside the frame, only on a bank
/// top-up.
pub fn frame_context(
	direction: FundingDirection,
	rail: Option<FundingRail>,
) -> FundingFrameContext {
	FundingFrameContext {
		awaits_user_transfer: rail == Some(FundingRail::Bank) && direction == FundingDirection::In,
	}
}

/// The fiat asset asked for when neither the user's country nor any provider
/// names one.
const DEFAULT_FIAT: &str = "USD";

/// Whether `candidate` routes `rail` in the direction its routes were picked
/// for. A provider that declares no routes may serve any rail, so only its
/// quotes say what it serves.
pub fn routes_rail(candidate: &FundingCandidate, rail: FundingRail) -> bool {
	candidate.routes.is_empty() || candidate.routes.iter().any(|route| route.rail == rail)
}

/// The rails some candidate routes, in core order (crypto, card, bank): the
/// only ones a session can use.
pub fn routed_rails(candidates: &[FundingCandidate]) -> Vec<FundingRail> {
	[FundingRail::Crypto, FundingRail::Card, FundingRail::Bank]
		.into_iter()
		.filter(|rail| candidates.iter().any(|candidate| routes_rail(candidate, *rail)))
		.collect()
}

/// The fiat asset a card or bank ask names: the user's currency when a
/// provider routing `rail` lists it or none lists any, otherwise the first one
/// listed, then the user's currency, then USD.
pub fn fiat_asset(
	candidates: &[FundingCandidate],
	rail: FundingRail,
	currency: Option<&str>,
) -> String {
	let listed: Vec<&String> = candidates
		.iter()
		.flat_map(|candidate| &candidate.routes)
		.filter(|route| route.rail == rail)
		.flat_map(|route| &route.assets)
		.collect();
	if let Some(currency) = currency &&
		(listed.is_empty() || listed.iter().any(|asset| asset.eq_ignore_ascii_case(currency)))
	{
		return currency.to_string();
	}
	listed
		.first()
		.map(|asset| asset.to_string())
		.or(currency.map(str::to_string))
		.unwrap_or_else(|| DEFAULT_FIAT.to_string())
}

/// The crypto tokens a session can use on `network`, or on any network
/// before one is picked: each asset a candidate's crypto route lists, once,
/// in the order first listed. A route that declares no networks counts for
/// every network.
pub fn crypto_tokens(candidates: &[FundingCandidate], network: Option<&str>) -> Vec<String> {
	let mut tokens: Vec<String> = Vec::new();
	let routes = candidates
		.iter()
		.flat_map(|candidate| &candidate.routes)
		.filter(|route| route.rail == FundingRail::Crypto)
		.filter(|route| {
			network.is_none_or(|network| {
				route
					.networks
					.as_ref()
					.is_none_or(|networks| networks.iter().any(|n| n == network))
			})
		});
	for asset in routes.flat_map(|route| &route.assets) {
		if !tokens.contains(asset) {
			tokens.push(asset.clone());
		}
	}
	tokens
}

/// The canonical English name of a crypto network id, which hosts may
/// localise. Known ids have their proper name; any other is title-cased word
/// by word.
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), uniffi::export)]
pub fn funding_network_name(id: String) -> String {
	match id.as_str() {
		"polkadot" => "Polkadot".to_string(),
		"bitcoin" => "Bitcoin".to_string(),
		"ethereum" => "Ethereum".to_string(),
		"tron" => "Tron".to_string(),
		"solana" => "Solana".to_string(),
		other => title_case(other),
	}
}

/// Each word of `text` with its first letter upper-cased and the rest
/// lower-cased.
fn title_case(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	let mut word_start = true;
	for ch in text.chars() {
		if ch.is_alphanumeric() {
			if word_start {
				out.extend(ch.to_uppercase());
			} else {
				out.extend(ch.to_lowercase());
			}
			word_start = false;
		} else {
			out.push(ch);
			word_start = true;
		}
	}
	out
}

/// Whether every row has answered and none quoted.
fn all_unavailable(rows: &[FundingQuoteRow]) -> bool {
	!rows.is_empty() &&
		rows.iter()
			.all(|row| matches!(row.state, FundingQuoteState::Unavailable { .. }))
}

/// A row's reason for not quoting, when it is one every provider can share:
/// a timeout, an unsupported country, or a non-empty message.
fn shared_reason(row: &FundingQuoteRow) -> Option<&FundingQuoteUnavailable> {
	let FundingQuoteState::Unavailable { reason } = &row.state else {
		return None;
	};
	match reason {
		FundingQuoteUnavailable::Timeout |
		FundingQuoteUnavailable::Refused { reason: FundingQuoteRefusal::CountryUnsupported } => {
			Some(reason)
		},
		FundingQuoteUnavailable::Refused { reason: FundingQuoteRefusal::Other { message } }
			if !message.trim().is_empty() =>
		{
			Some(reason)
		},
		_ => None,
	}
}

/// The reason every provider gave for not quoting, once all answered without
/// a quote and each gave the same one. A limit or a bare
/// unavailability is not such a reason: the amount check covers limits.
pub fn quotes_refusal(rows: &[FundingQuoteRow]) -> Option<FundingQuoteUnavailable> {
	if !all_unavailable(rows) {
		return None;
	}
	let first = shared_reason(&rows[0])?;
	rows[1..]
		.iter()
		.all(|row| shared_reason(row) == Some(first))
		.then(|| first.clone())
}

/// What the providers' refusals say about the amount asked, once all
/// answered without a quote: the lowest minimum any showed, otherwise the
/// highest maximum.
pub fn quotes_issue(rows: &[FundingQuoteRow]) -> Option<FundingAmountIssue> {
	if !all_unavailable(rows) {
		return None;
	}
	let refusals = rows.iter().filter_map(|row| match &row.state {
		FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Refused { reason } } => {
			Some(reason)
		},
		_ => None,
	});
	let min = refusals
		.clone()
		.filter_map(|reason| match reason {
			FundingQuoteRefusal::BelowMinimum { min } => Some(*min),
			_ => None,
		})
		.min();
	if let Some(min) = min {
		return Some(FundingAmountIssue::BelowMinimum { min });
	}
	refusals
		.filter_map(|reason| match reason {
			FundingQuoteRefusal::AboveMaximum { max } => Some(*max),
			_ => None,
		})
		.max()
		.map(|max| FundingAmountIssue::AboveMaximum { max })
}

/// Why `amount` cannot go ahead, if it cannot: more than `spendable` on a
/// withdrawal, then what every provider's refusal of this same amount said,
/// then the amount limits across providers.
pub fn amount_issue(
	direction: FundingDirection,
	limits: &FundingAmountLimits,
	quoted: Option<(u128, Option<&FundingAmountIssue>)>,
	amount: u128,
	spendable: Option<u128>,
) -> Option<FundingAmountIssue> {
	if direction == FundingDirection::Out && spendable.is_some_and(|spendable| amount > spendable) {
		return Some(FundingAmountIssue::NotEnoughBalance);
	}
	if let Some((asked, Some(issue))) = quoted &&
		asked == amount
	{
		return Some(issue.clone());
	}
	if let Some(min) = limits.min &&
		amount > 0 &&
		amount < min
	{
		return Some(FundingAmountIssue::BelowMinimum { min });
	}
	if let Some(max) = limits.max &&
		amount > max
	{
		return Some(FundingAmountIssue::AboveMaximum { max });
	}
	None
}

/// Whether the user can cancel a session now, and what a cancel does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingCancelAvailability {
	/// No cancel is possible: the session ended, or an offer or an accepted
	/// cancel is already in.
	Unavailable,
	/// A cancel is possible.
	Available,
	/// A cancel is already with the provider and no money moved, so a
	/// further cancel ends the session.
	AvailableAgain,
}

/// Whether `status` is a session still in flight: every status but the
/// endings.
pub fn status_in_flight(status: &FundingStatus) -> bool {
	!matches!(
		status,
		FundingStatus::ToppedUp |
			FundingStatus::Sent |
			FundingStatus::Refunded |
			FundingStatus::PayoutFailed { .. } |
			FundingStatus::Failed { .. } |
			FundingStatus::Cancelled { .. } |
			FundingStatus::PaidOutAfterCancel
	)
}

/// Whether a session at `status` can be cancelled: not once it ended or
/// while an offer or an accepted cancel is in, again while a cancel waits on
/// the provider and no money moved, and otherwise yes.
pub fn cancel_availability(status: &FundingStatus, money_moved: bool) -> FundingCancelAvailability {
	if !status_in_flight(status) {
		return FundingCancelAvailability::Unavailable;
	}
	match status {
		FundingStatus::CancelRequested if !money_moved => FundingCancelAvailability::AvailableAgain,
		FundingStatus::CancelRequested |
		FundingStatus::CancelOffered { .. } |
		FundingStatus::Cancelling => FundingCancelAvailability::Unavailable,
		_ => FundingCancelAvailability::Available,
	}
}

/// How an ended session went.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingEndedOutcome {
	/// Not ended yet.
	#[default]
	Open,
	/// The balance was credited, or the withdrawal's payout went out or is
	/// still on its way.
	Succeeded,
	/// Ended without moving the money it was for.
	Failed {
		/// Why.
		reason: FundingFailure,
	},
	/// The withdrawal's payout failed after the balance was debited.
	PayoutFailed {
		/// Why, as the provider put it.
		reason: String,
	},
}

/// Where a released withdrawal's payout stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingPayoutState {
	/// The provider has not said yet.
	Pending,
	/// The funds reached the user.
	PaidOut,
	/// The payout failed.
	Failed,
}

/// The facts about how a session ended.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingEnding {
	/// When it ended, else when it opened, in Unix milliseconds: what history
	/// is ordered by.
	pub at_ms: u64,
	/// How it went.
	pub outcome: FundingEndedOutcome,
	/// On a released withdrawal, where its payout stands.
	pub payout_state: Option<FundingPayoutState>,
	/// The CASH amount the session moved, in balance units: what was
	/// credited or debited, else what was asked, else what the quote moved.
	pub cash_amount: Option<u128>,
	/// What was asked, when it differs from what was credited or debited.
	pub differing_requested: Option<u128>,
	/// The step the session stopped before: the first one it did not
	/// reach. `None` when it succeeded.
	pub stopped_at: Option<FundingStep>,
	/// The steps a session in its direction goes through, in order.
	pub steps: Vec<FundingStep>,
	/// The step a released withdrawal waits at while its payout is pending.
	pub waiting_at: Option<FundingStep>,
}

/// Whether a summary's status or stage says the withdrawal was released,
/// and how its record ended otherwise.
enum EndKind {
	Delivered,
	Released,
	Failed(FundingFailure),
}

/// `summary`'s ending: its status decides, its stage when the status says
/// nothing about it.
fn end_kind(summary: &FundingSessionSummary) -> Option<EndKind> {
	match &summary.status {
		FundingStatus::ToppedUp => return Some(EndKind::Delivered),
		FundingStatus::Sent |
		FundingStatus::PaidOutAfterCancel |
		FundingStatus::PayoutFailed { .. } => {
			return Some(EndKind::Released);
		},
		FundingStatus::Refunded => return Some(EndKind::Failed(FundingFailure::Refunded)),
		FundingStatus::Cancelled { .. } => return Some(EndKind::Failed(FundingFailure::Cancelled)),
		FundingStatus::Failed { reason } => return Some(EndKind::Failed(reason.clone())),
		_ => {},
	}
	match &summary.stage {
		FundingStage::Open => None,
		FundingStage::Delivered { .. } => Some(EndKind::Delivered),
		FundingStage::Released { .. } => Some(EndKind::Released),
		FundingStage::Failed { reason, .. } => Some(EndKind::Failed(reason.clone())),
	}
}

/// How `summary` ended. The steps it reached are told from
/// how it ended, as a summary keeps no updates: a credit or a release got
/// past Received, and a refund too, since the provider had the payment; a
/// payout reached Sent only once paid out.
pub fn ending(summary: &FundingSessionSummary) -> FundingEnding {
	let at_ms = summary.settled_at_ms().unwrap_or(summary.opened_at_ms);
	let Some(ending) = end_kind(summary) else {
		return FundingEnding {
			at_ms,
			steps: FundingStep::of(summary.direction).to_vec(),
			..FundingEnding::default()
		};
	};
	let payout_state = match ending {
		EndKind::Released if summary.direction == FundingDirection::Out => {
			Some(match &summary.payout {
				None => FundingPayoutState::Pending,
				Some(FundingPayout::PaidOut) => FundingPayoutState::PaidOut,
				Some(FundingPayout::Failed { .. }) => FundingPayoutState::Failed,
			})
		},
		_ => None,
	};
	let outcome = match (&ending, &summary.payout) {
		(EndKind::Released, Some(FundingPayout::Failed { reason })) => {
			FundingEndedOutcome::PayoutFailed { reason: reason.clone() }
		},
		(EndKind::Delivered | EndKind::Released, _) => FundingEndedOutcome::Succeeded,
		(EndKind::Failed(reason), _) => FundingEndedOutcome::Failed { reason: reason.clone() },
	};
	let settled = summary.stage.settled_amount();
	let quoted = summary.choice.as_ref().map(|choice| choice.balance_amount(summary.direction));
	let last = match summary.direction {
		FundingDirection::In => FundingStep::Added,
		FundingDirection::Out => FundingStep::Sent,
	};
	let received = matches!(
		summary.stage,
		FundingStage::Delivered { .. } |
			FundingStage::Released { .. } |
			FundingStage::Failed { reason: FundingFailure::Refunded, .. }
	);
	let completed = match summary.stage {
		FundingStage::Delivered { .. } => true,
		FundingStage::Released { .. } => matches!(summary.payout, Some(FundingPayout::PaidOut)),
		_ => false,
	};
	let first_unreached = if !received {
		Some(FundingStep::Received)
	} else if !completed {
		Some(last)
	} else {
		None
	};
	let stopped_at = match outcome {
		FundingEndedOutcome::Open | FundingEndedOutcome::Succeeded => None,
		FundingEndedOutcome::PayoutFailed { .. } => Some(last),
		FundingEndedOutcome::Failed { .. } => first_unreached.or(Some(FundingStep::Received)),
	};
	FundingEnding {
		at_ms,
		outcome,
		payout_state,
		cash_amount: settled.or(summary.amount).or(quoted),
		differing_requested: summary
			.amount
			.filter(|requested| settled.is_some_and(|settled| settled != *requested)),
		stopped_at,
		steps: FundingStep::of(summary.direction).to_vec(),
		waiting_at: (payout_state == Some(FundingPayoutState::Pending)).then_some(last),
	}
}

#[cfg(test)]
mod tests;
