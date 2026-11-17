//! H5 · Status and error mapping: layer 1's statuses, failures and per-coin
//! results in RFC-17's vocabulary.
//!
//! The mapping never reports more progress than layer 1 did: nothing maps to
//! `Done` unless layer 1 reported `Done` (invariant 1), a cheque's progress
//! counts a claim only once it is stable (invariant 3), and a refusal never
//! reveals a balance to a product without balance access (invariant 2).

use truapi::latest::{
	CoinPaymentClearingReference, CoinPaymentError, CoinPaymentStatus, HostPaymentStatusSubscribeItem,
	HostPaymentTopUpStatusSubscribeItem,
};

use super::clearing;
use crate::coinage::{
	classification::ExportStatus,
	layer::{Error, ExportedCoinStatus},
	status::{CoinResult, Progress, Status, StatusItem},
};

/// A cheque's value by what became of its coins, in cents (RFC deviations,
/// entry 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChequeProgress {
	/// Not yet claimable.
	pub minting: u64,
	/// Minted, not claimed.
	pub claimable: u64,
	/// Claimed by the payee, stably.
	pub claimed: u64,
	/// Reclaimed, cancelled, or never minted.
	pub returned: u64,
}

impl ChequeProgress {
	/// Whether nothing is minting or claimable any more, so the progress can
	/// no longer change.
	pub fn is_settled(&self) -> bool {
		self.minting == 0 && self.claimable == 0
	}
}

/// A cheque's progress from its coins' export statuses.
pub fn cheque_progress(coins: &[ExportedCoinStatus]) -> ChequeProgress {
	coins.iter().fold(ChequeProgress::default(), |progress, coin| {
		let value = coin.exponent.value();
		match coin.status {
			ExportStatus::NotYetMinted => ChequeProgress { minting: progress.minting + value, ..progress },
			ExportStatus::AwaitingPayee |
			ExportStatus::Claimed { stable: false, .. } |
			ExportStatus::Reclaimed { stable: false } => ChequeProgress { claimable: progress.claimable + value, ..progress },
			ExportStatus::Claimed { stable: true, .. } => ChequeProgress { claimed: progress.claimed + value, ..progress },
			ExportStatus::Reclaimed { stable: true } | ExportStatus::NeverMinted => {
				ChequeProgress { returned: progress.returned + value, ..progress }
			},
		}
	})
}

/// A deposit's status: `Done` only at layer 1's `Done` with the expected
/// amount settled (K2 invariant 4); otherwise, once ended, `Failed` with the
/// error its coins' results name.
pub fn deposit_status(
	item: StatusItem,
	coins: &[([u8; 32], CoinResult)],
	expected: u64,
	reference: CoinPaymentClearingReference,
) -> CoinPaymentStatus {
	let cleared = cents(item.progress.settled);
	match item.status {
		Status::Done if item.progress.settled == expected => CoinPaymentStatus::Done { cleared, reference },
		status if status.is_terminal() => {
			let error = if coins.iter().any(|(_, result)| *result == CoinResult::TakenByOther) {
				CoinPaymentError::SnipedCoins
			} else {
				CoinPaymentError::BadCoins
			};
			CoinPaymentStatus::Failed { error, cleared, reference }
		},
		_ => clearing(item),
	}
}

/// A rebalance's or a deletion's status, with the empty reference.
pub fn own_move_status(item: StatusItem) -> CoinPaymentStatus {
	let cleared = cents(item.progress.settled);
	match item.status {
		Status::Done => CoinPaymentStatus::Done { cleared, reference: clearing::build(Vec::new()) },
		status if status.is_terminal() => {
			CoinPaymentStatus::Failed { error: CoinPaymentError::Internal, cleared, reference: clearing::build(Vec::new()) }
		},
		_ => clearing(item),
	}
}

/// A refund's status from the progress of its cheques (invariant 4): `Done`
/// once the payer claimed all of it, `Failed` with `Internal` when some
/// deposit had no return path (K3 invariant 3).
pub fn refund_status(
	cheques: &[ChequeProgress],
	missing_return_path: bool,
	reference: CoinPaymentClearingReference,
) -> CoinPaymentStatus {
	let total = cheques.iter().fold(ChequeProgress::default(), |sum, cheque| ChequeProgress {
		minting: sum.minting + cheque.minting,
		claimable: sum.claimable + cheque.claimable,
		claimed: sum.claimed + cheque.claimed,
		returned: sum.returned + cheque.returned,
	});
	let cleared = cents(total.claimed);
	if !total.is_settled() {
		return CoinPaymentStatus::Clearing { clearing: cents(total.minting + total.claimable), cleared };
	}
	if missing_return_path || total.returned > 0 {
		return CoinPaymentStatus::Failed { error: CoinPaymentError::Internal, cleared, reference };
	}
	CoinPaymentStatus::Done { cleared, reference }
}

/// A cancellation's status from its cheque's progress (invariant 4): `Done`
/// once every coin came back, `Failed` with `SnipedCoins` when the payee
/// claimed some first.
pub fn cancellation_status(cheque: ChequeProgress) -> CoinPaymentStatus {
	let cleared = cents(cheque.returned);
	let reference = clearing::build(Vec::new());
	if !cheque.is_settled() {
		CoinPaymentStatus::Clearing { clearing: cents(cheque.minting + cheque.claimable), cleared }
	} else if cheque.claimed == 0 {
		CoinPaymentStatus::Done { cleared, reference }
	} else {
		CoinPaymentStatus::Failed { error: CoinPaymentError::SnipedCoins, cleared, reference }
	}
}

/// A layer-1 refusal as RFC-17 reports it.
pub fn coin_payment_error(error: &Error) -> CoinPaymentError {
	match error {
		Error::InsufficientFunds { .. } => CoinPaymentError::BalanceLow,
		Error::PurseNotFound(_) | Error::DeletionOpen(_) | Error::MainPurse => CoinPaymentError::PurseNotFound,
		Error::NeedsConfirmation => CoinPaymentError::Denied,
		Error::InvalidSecret(_) => CoinPaymentError::BadCoins,
		Error::UnknownOperation(_) | Error::NotAnExport(_) => CoinPaymentError::ReceivableNotFound,
		Error::UnsupportedRuntime { .. } | Error::Unavailable => CoinPaymentError::UserAgentCapabilityUnavailable,
		_ => CoinPaymentError::Internal,
	}
}

/// A payment's RFC-6 status, with its amount and the cent both in the
/// asset's units: `Completed` only at layer 1's `Done` with the full amount
/// settled, so an amount rounded down to whole cents ends partial (N2
/// invariant 5).
pub fn payment_status(item: StatusItem, amount: u128, unit: u128) -> HostPaymentStatusSubscribeItem {
	let claimed = in_units(item.progress.settled, unit);
	match item.status {
		Status::Done if claimed >= amount => HostPaymentStatusSubscribeItem::Completed,
		status if status.is_terminal() && claimed > 0 => {
			HostPaymentStatusSubscribeItem::ClaimedPartially { actual_claimed: claimed }
		},
		Status::Failed(failure) => HostPaymentStatusSubscribeItem::Failed { reason: format!("{failure:?}") },
		status if status.is_terminal() => HostPaymentStatusSubscribeItem::Failed { reason: "nothing moved".into() },
		_ => HostPaymentStatusSubscribeItem::Processing,
	}
}

/// A top-up's RFC-6 status, with its amount and the cent both in the asset's
/// units: `Claimed { finalized: false }` once the value that succeeded, on
/// the best chain or finally, reaches the amount, otherwise `Detecting`
/// while it waits for the source and `Claiming` while it is built or
/// submitted; and `Claimed { finalized: true }` at layer 1's `Done` with the
/// full amount, so an amount rounded down to whole cents ends partial (N2
/// invariant 5).
pub fn top_up_status(item: StatusItem, amount: u128, unit: u128) -> HostPaymentTopUpStatusSubscribeItem {
	let Progress { settled, provisional, .. } = item.progress;
	let claimed = in_units(settled, unit);
	match item.status {
		Status::Done if claimed >= amount => HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true },
		status if status.is_terminal() && claimed > 0 => {
			HostPaymentTopUpStatusSubscribeItem::ClaimedPartially { actual_claimed: claimed }
		},
		status if status.is_terminal() => HostPaymentTopUpStatusSubscribeItem::NotClaimed,
		_ if in_units(settled + provisional, unit) >= amount => HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false },
		Status::Waiting => HostPaymentTopUpStatusSubscribeItem::Detecting,
		_ => HostPaymentTopUpStatusSubscribeItem::Claiming,
	}
}

/// `cents` in the asset's units, saturating.
fn in_units(cents: u64, unit: u128) -> u128 {
	u128::from(cents).saturating_mul(unit)
}

fn clearing(item: StatusItem) -> CoinPaymentStatus {
	CoinPaymentStatus::Clearing { clearing: cents(item.progress.in_flight), cleared: cents(item.progress.settled) }
}

/// An amount in cents as RFC-17's `u32` balance, saturating.
fn cents(amount: u64) -> u32 {
	u32::try_from(amount).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
