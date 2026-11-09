//! Stateless funding rules native hosts call, so both apps treat amounts,
//! fees and statuses the same way.

use truapi::latest::{FundingDirection, FundingQuote};

use crate::{
	host_logic::{
		funding::FundingStatus,
		funding_providers::FundingAmountLimits,
		funding_view::{self, FundingFeeBreakdown},
	},
	platform::{FundingAmountIssue, FundingQuoteList},
};

/// Why `amount` (balance units) cannot go ahead, if it cannot: more than
/// `spendable` on a withdrawal, then what every provider's refusal of this
/// same amount in `quotes` said, then the amount limits from the session's
/// options.
#[uniffi::export]
pub fn funding_amount_issue(
	direction: FundingDirection,
	limits: FundingAmountLimits,
	quotes: Option<FundingQuoteList>,
	amount: u128,
	spendable: Option<u128>,
) -> Option<FundingAmountIssue> {
	funding_view::amount_issue(
		direction,
		&limits,
		quotes.as_ref().map(|quotes| (quotes.amount, quotes.issue.as_ref())),
		amount,
		spendable,
	)
}

/// What `quote`'s fees come to in `direction`, with a withdrawal's fee
/// also rounded up to the cent, as it is charged.
#[uniffi::export]
pub fn funding_fee_breakdown(
	quote: FundingQuote,
	direction: FundingDirection,
) -> FundingFeeBreakdown {
	funding_view::fee_breakdown(&quote, direction)
}

/// `text`, an amount typed with `.` as its separator, in units of
/// `decimals` decimals, rounded down. `None` when it is not a plain decimal
/// number or does not fit.
#[uniffi::export]
pub fn funding_units(text: String, decimals: u8) -> Option<u128> {
	funding_view::units(&text, decimals)
}

/// Whether asset `code` is a fiat currency, by ISO 4217, rather than a
/// crypto asset.
#[uniffi::export]
pub fn funding_asset_is_fiat(code: String) -> bool {
	funding_view::asset_is_fiat(&code)
}

/// The canonical English name of a crypto network id, which hosts may
/// localise.
#[uniffi::export]
pub fn funding_network_name(id: String) -> String {
	funding_view::network_name(&id)
}

/// Whether a session at `status` is still in flight.
#[uniffi::export]
pub fn funding_status_in_flight(status: FundingStatus) -> bool {
	funding_view::status_in_flight(&status)
}

/// Whether a session at `status` is held up: retrying, taking longer, or
/// waiting on a cancel the provider answered.
#[uniffi::export]
pub fn funding_status_delayed(status: FundingStatus) -> bool {
	funding_view::status_delayed(&status)
}
