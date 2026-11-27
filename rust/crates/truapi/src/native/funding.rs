//! Funding rules native hosts call, in the shape the bindings take.

use crate::host_logic::{funding::FundingStatus, funding_view};

/// Whether a session at `status` is still in flight.
#[uniffi::export]
pub fn funding_status_in_flight(status: FundingStatus) -> bool {
	funding_view::status_in_flight(&status)
}
