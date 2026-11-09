//! Funding rules native hosts call, in the shape the bindings take.

use truapi::latest::FundingDirection;

use crate::{
	host_logic::{funding::FundingStatus, funding_providers::FundingAmountLimits, funding_view},
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

/// Whether a session at `status` is still in flight.
#[uniffi::export]
pub fn funding_status_in_flight(status: FundingStatus) -> bool {
	funding_view::status_in_flight(&status)
}
