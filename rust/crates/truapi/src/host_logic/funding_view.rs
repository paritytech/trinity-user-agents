//! Funding rules every host shares: which rails and assets a session can
//! use, whether an amount can go ahead, whether a session can be cancelled,
//! how an ended session went, and the arithmetic behind fees and typed
//! amounts.
//!
//! These are facts and rules only. Each host decides how to present them.

use truapi::latest::{
	FundingDirection, FundingFailure, FundingPayout, FundingQuote, FundingQuoteRefusal, FundingRail,
};

use crate::{
	host_logic::{
		funding::{FundingSessionSummary, FundingStage, FundingStatus, FundingStep},
		funding_providers::{CASH_DECIMALS, FundingAmountLimits, FundingCandidate},
	},
	platform::{
		FundingAmountIssue, FundingQuoteRow, FundingQuoteState, FundingQuoteUnavailable,
		FundingQuotesRefusal,
	},
};

/// The rails in core order, which decides the default one.
pub const FUNDING_RAIL_ORDER: [FundingRail; 3] =
	[FundingRail::Crypto, FundingRail::Card, FundingRail::Bank];

/// The fiat asset asked for when neither the user's country nor any provider
/// names one.
const DEFAULT_FIAT: &str = "USD";

/// Whether `candidate` routes `rail` in the direction its routes were picked
/// for. A provider that declares no routes may serve any rail, so only its
/// quotes say what it serves.
pub fn routes_rail(candidate: &FundingCandidate, rail: FundingRail) -> bool {
	candidate.routes.is_empty() || candidate.routes.iter().any(|route| route.rail == rail)
}

/// The rails some candidate routes, in [`FUNDING_RAIL_ORDER`]: the only
/// ones a session can use.
pub fn routed_rails(candidates: &[FundingCandidate]) -> Vec<FundingRail> {
	FUNDING_RAIL_ORDER
		.into_iter()
		.filter(|rail| candidates.iter().any(|candidate| routes_rail(candidate, *rail)))
		.collect()
}

/// The default rail: the first of `rails`, which are in
/// [`FUNDING_RAIL_ORDER`], or card when none is routed.
pub fn default_rail(rails: &[FundingRail]) -> FundingRail {
	rails.first().copied().unwrap_or(FundingRail::Card)
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

/// The canonical English name of a network id, which hosts may localise.
/// Known ids have their proper name; any other is title-cased word by word.
pub fn network_name(id: &str) -> String {
	match id {
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

/// The reason every provider gave for not quoting, once all answered without
/// a quote and each gave the same one. A limit or a bare
/// unavailability is not such a reason: the amount check covers limits.
pub fn quotes_refusal(rows: &[FundingQuoteRow]) -> Option<FundingQuotesRefusal> {
	if !all_unavailable(rows) {
		return None;
	}
	let reason = |row: &FundingQuoteRow| match &row.state {
		FundingQuoteState::Unavailable { reason: FundingQuoteUnavailable::Timeout } => {
			Some(FundingQuotesRefusal::Timeout)
		},
		FundingQuoteState::Unavailable {
			reason:
				FundingQuoteUnavailable::Refused { reason: FundingQuoteRefusal::CountryUnsupported },
		} => Some(FundingQuotesRefusal::CountryUnsupported),
		FundingQuoteState::Unavailable {
			reason:
				FundingQuoteUnavailable::Refused { reason: FundingQuoteRefusal::Other { message } },
		} if !message.trim().is_empty() => Some(FundingQuotesRefusal::Other { message: message.clone() }),
		_ => None,
	};
	let first = reason(&rows[0])?;
	rows[1..]
		.iter()
		.all(|row| reason(row).as_ref() == Some(&first))
		.then_some(first)
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

/// Whether a session at `status` is held up: retrying, taking longer, or
/// waiting on a cancel the provider offered terms on or refused.
pub fn status_delayed(status: &FundingStatus) -> bool {
	matches!(
		status,
		FundingStatus::Retrying |
			FundingStatus::RetryingPayout |
			FundingStatus::TakingLonger |
			FundingStatus::CancelOffered { .. } |
			FundingStatus::CancelRefused { .. }
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
pub struct FundingEndedView {
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
}

/// Whether a summary's status or stage says the withdrawal was released,
/// and how its record ended otherwise.
enum Ending {
	Delivered,
	Released,
	Failed(FundingFailure),
}

/// `summary`'s ending: its status decides, its stage when the status says
/// nothing about it.
fn ending(summary: &FundingSessionSummary) -> Option<Ending> {
	match &summary.status {
		FundingStatus::ToppedUp => return Some(Ending::Delivered),
		FundingStatus::Sent |
		FundingStatus::PaidOutAfterCancel |
		FundingStatus::PayoutFailed { .. } => {
			return Some(Ending::Released);
		},
		FundingStatus::Refunded => return Some(Ending::Failed(FundingFailure::Refunded)),
		FundingStatus::Cancelled { .. } => return Some(Ending::Failed(FundingFailure::Cancelled)),
		FundingStatus::Failed { reason } => return Some(Ending::Failed(reason.clone())),
		_ => {},
	}
	match &summary.stage {
		FundingStage::Open => None,
		FundingStage::Delivered { .. } => Some(Ending::Delivered),
		FundingStage::Released { .. } => Some(Ending::Released),
		FundingStage::Failed { reason, .. } => Some(Ending::Failed(reason.clone())),
	}
}

/// How `summary` ended. The steps it reached are told from
/// how it ended, as a summary keeps no updates: a credit or a release got
/// past Received, and a refund too, since the provider had the payment; a
/// payout reached Sent only once paid out.
pub fn ended_view(summary: &FundingSessionSummary) -> FundingEndedView {
	let Some(ending) = ending(summary) else {
		return FundingEndedView::default();
	};
	let payout_state = match ending {
		Ending::Released if summary.direction == FundingDirection::Out => {
			Some(match &summary.payout {
				None => FundingPayoutState::Pending,
				Some(FundingPayout::PaidOut) => FundingPayoutState::PaidOut,
				Some(FundingPayout::Failed { .. }) => FundingPayoutState::Failed,
			})
		},
		_ => None,
	};
	let outcome = match (&ending, &summary.payout) {
		(Ending::Released, Some(FundingPayout::Failed { reason })) => {
			FundingEndedOutcome::PayoutFailed { reason: reason.clone() }
		},
		(Ending::Delivered | Ending::Released, _) => FundingEndedOutcome::Succeeded,
		(Ending::Failed(reason), _) => FundingEndedOutcome::Failed { reason: reason.clone() },
	};
	let settled = match summary.stage {
		FundingStage::Delivered { credited, .. } => Some(credited),
		FundingStage::Released { debited, .. } => Some(debited),
		_ => None,
	};
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
	FundingEndedView {
		outcome,
		payout_state,
		cash_amount: settled.or(summary.amount).or(quoted),
		differing_requested: summary
			.amount
			.filter(|requested| settled.is_some_and(|settled| settled != *requested)),
		stopped_at,
	}
}

/// What a quote's fees come to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingFeeBreakdown {
	/// What the user pays less the fees, in the unit of `send_amount`.
	pub net: u128,
	/// The provider's and the network's fees together, in the unit of
	/// `send_amount`.
	pub total_fee: u128,
	/// `total_fee` as the user is charged it: on a withdrawal, whose fees are
	/// in CASH, rounded up to the cent so the cost is never under-stated.
	pub total_fee_rounded_up: u128,
	/// The rate's numerator: the provider's asset, in its smallest unit, per
	/// `rate_den` CASH balance units.
	pub rate_num: u128,
	/// The rate's denominator, in CASH balance units. Zero when the quote
	/// moves no CASH, so there is no rate.
	pub rate_den: u128,
}

/// What `quote`'s fees come to in `direction`. The rate sets the provider's
/// asset against CASH: inbound, what the user pays less fees per CASH
/// credited; outbound, what the user receives per CASH paid less fees.
pub fn fee_breakdown(quote: &FundingQuote, direction: FundingDirection) -> FundingFeeBreakdown {
	let total_fee = quote.provider_fee.saturating_add(quote.network_fee);
	let net = quote.send_amount.saturating_sub(total_fee);
	let (rate_num, rate_den) = match direction {
		FundingDirection::In => (net, quote.receive_amount),
		FundingDirection::Out => (quote.receive_amount, net),
	};
	let total_fee_rounded_up = match direction {
		FundingDirection::In => total_fee,
		FundingDirection::Out => {
			let cent = 10u128.pow(u32::from(CASH_DECIMALS.saturating_sub(2)));
			total_fee.div_ceil(cent).saturating_mul(cent)
		},
	};
	FundingFeeBreakdown { net, total_fee, total_fee_rounded_up, rate_num, rate_den }
}

/// `text`, a decimal amount the user typed with `.` as its separator, in
/// units of `decimals` decimals, rounded down. `None` for anything but ASCII
/// digits with at most one `.`, for no digits at all, and for an amount too
/// large to hold.
pub fn units(text: &str, decimals: u8) -> Option<u128> {
	let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
	let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
	if (whole.is_empty() && fraction.is_empty()) || !digits(whole) || !digits(fraction) {
		return None;
	}
	let scale = 10u128.checked_pow(u32::from(decimals))?;
	let whole: u128 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
	let mut fractional: u128 = 0;
	let mut place = scale;
	for digit in fraction.bytes() {
		place /= 10;
		if place == 0 {
			break;
		}
		fractional += u128::from(digit - b'0') * place;
	}
	whole.checked_mul(scale)?.checked_add(fractional)
}

/// Whether `code` names a fiat currency, by its ISO 4217 code in any case.
/// Anything else a provider lists is a crypto asset.
pub fn asset_is_fiat(code: &str) -> bool {
	let upper = code.to_ascii_uppercase();
	ISO_4217.binary_search(&upper.as_str()).is_ok()
}

/// Active ISO 4217 currency codes, sorted.
const ISO_4217: &[&str] = &[
	"AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT",
	"BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD", "CAD",
	"CDF", "CHF", "CLP", "CNY", "COP", "CRC", "CUP", "CVE", "CZK", "DJF", "DKK", "DOP", "DZD",
	"EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP", "GEL", "GHS", "GIP", "GMD", "GNF", "GTQ",
	"GYD", "HKD", "HNL", "HTG", "HUF", "IDR", "ILS", "INR", "IQD", "IRR", "ISK", "JMD", "JOD",
	"JPY", "KES", "KGS", "KHR", "KMF", "KPW", "KRW", "KWD", "KYD", "KZT", "LAK", "LBP", "LKR",
	"LRD", "LSL", "LYD", "MAD", "MDL", "MGA", "MKD", "MMK", "MNT", "MOP", "MRU", "MUR", "MVR",
	"MWK", "MXN", "MYR", "MZN", "NAD", "NGN", "NIO", "NOK", "NPR", "NZD", "OMR", "PAB", "PEN",
	"PGK", "PHP", "PKR", "PLN", "PYG", "QAR", "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR",
	"SDG", "SEK", "SGD", "SHP", "SLE", "SOS", "SRD", "SSP", "STN", "SVC", "SYP", "SZL", "THB",
	"TJS", "TMT", "TND", "TOP", "TRY", "TTD", "TWD", "TZS", "UAH", "UGX", "USD", "UYU", "UZS",
	"VES", "VND", "VUV", "WST", "XAF", "XCD", "XCG", "XOF", "XPF", "YER", "ZAR", "ZMW", "ZWG",
];

#[cfg(test)]
mod tests;
