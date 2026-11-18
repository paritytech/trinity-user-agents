use super::*;
use crate::coinage::{
	Exponent,
	status::{CoinOutcome, Failure, Progress},
};

fn item(status: Status, settled: u64, in_flight: u64) -> StatusItem {
	StatusItem { status, progress: Progress { settled, in_flight, provisional: 0 } }
}

fn coin(exponent: u8, status: ExportStatus) -> ExportedCoinStatus {
	ExportedCoinStatus { account: [exponent; 32], exponent: Exponent::new(exponent).unwrap(), status }
}

fn empty() -> CoinPaymentClearingReference {
	clearing::build(Vec::new())
}

#[test]
fn a_deposit_is_done_only_at_layer_ones_done_with_the_expected_amount() {
	assert_eq!(deposit_status(item(Status::Done, 9, 0), &[], 9, empty()), CoinPaymentStatus::Done { cleared: 9, reference: empty() });
	assert_eq!(
		deposit_status(item(Status::Done, 8, 0), &[], 9, empty()),
		CoinPaymentStatus::Failed { error: CoinPaymentError::BadCoins, cleared: 8, reference: empty() },
		"a coin worth less than declared fails the deposit"
	);
	let sniped = [
		CoinOutcome { account: [1; 32], result: CoinResult::TakenByOther },
		CoinOutcome { account: [2; 32], result: CoinResult::Moved },
	];
	assert_eq!(
		deposit_status(item(Status::PartiallyDone, 1, 0), &sniped, 9, empty()),
		CoinPaymentStatus::Failed { error: CoinPaymentError::SnipedCoins, cleared: 1, reference: empty() }
	);
	assert_eq!(
		deposit_status(item(Status::Settling, 4, 5), &[], 9, empty()),
		CoinPaymentStatus::Clearing { clearing: 5, cleared: 4 }
	);
}

#[test]
fn a_cheques_progress_counts_a_claim_only_once_it_is_stable() {
	let coins = [
		coin(0, ExportStatus::NotYetMinted),
		coin(1, ExportStatus::Claimed { stable: false, by: [0; 32] }),
		coin(2, ExportStatus::Claimed { stable: true, by: [0; 32] }),
		coin(3, ExportStatus::NeverMinted),
	];
	assert_eq!(cheque_progress(&coins), ChequeProgress { minting: 1, claimable: 2, claimed: 4, returned: 8 });
}

#[test]
fn a_cancellation_is_done_once_every_coin_came_back_and_sniped_otherwise() {
	let back = ChequeProgress { returned: 8, ..Default::default() };
	assert_eq!(cancellation_status(back), CoinPaymentStatus::Done { cleared: 8, reference: empty() });
	let sniped = ChequeProgress { returned: 4, claimed: 4, ..Default::default() };
	assert_eq!(
		cancellation_status(sniped),
		CoinPaymentStatus::Failed { error: CoinPaymentError::SnipedCoins, cleared: 4, reference: empty() }
	);
	let open = ChequeProgress { claimable: 4, returned: 4, ..Default::default() };
	assert_eq!(cancellation_status(open), CoinPaymentStatus::Clearing { clearing: 4, cleared: 4 });
}

#[test]
fn a_refund_without_a_return_path_fails_even_when_every_cheque_was_claimed() {
	let claimed = [ChequeProgress { claimed: 8, ..Default::default() }];
	assert_eq!(refund_status(&claimed, false, empty()), CoinPaymentStatus::Done { cleared: 8, reference: empty() });
	assert_eq!(
		refund_status(&claimed, true, empty()),
		CoinPaymentStatus::Failed { error: CoinPaymentError::Internal, cleared: 8, reference: empty() }
	);
}

#[test]
fn a_refusal_never_reveals_more_than_rfc_17_names() {
	assert_eq!(coin_payment_error(&Error::InsufficientFunds { available: 3 }), CoinPaymentError::BalanceLow);
	assert_eq!(coin_payment_error(&Error::PurseNotFound(crate::coinage::PurseId(1))), CoinPaymentError::PurseNotFound);
	assert_eq!(coin_payment_error(&Error::NoUnloadToken), CoinPaymentError::Internal);
	assert_eq!(own_move_status(item(Status::Failed(Failure::Refused), 0, 0)), CoinPaymentStatus::Failed {
		error: CoinPaymentError::Internal,
		cleared: 0,
		reference: empty()
	});
}

#[test]
fn a_payment_still_being_worked_on_is_processing() {
	for status in [Status::Waiting, Status::Preparing, Status::Submitted, Status::Settling] {
		assert_eq!(payment_status(item(status, 4, 4), 8, 10_000), HostPaymentStatusSubscribeItem::Processing, "{status:?}");
	}
}

#[test]
fn a_partially_completed_payment_reports_what_reached_the_destination() {
	let partial = HostPaymentStatusSubscribeItem::ClaimedPartially { actual_claimed: 4 * 10_000 };
	assert_eq!(payment_status(item(Status::PartiallyDone, 4, 0), 8, 10_000), partial);
}

mod use_cases;
