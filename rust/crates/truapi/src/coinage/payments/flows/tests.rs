use core::sync::atomic::AtomicU64;

use futures::StreamExt;
use truapi::latest::{CoinPaymentCheque, CoinPaymentStatus};
use zeroize::Zeroizing;

use super::*;
use crate::coinage::{
	Initiator, PrivacyPolicy, PurseId,
	ledger::{AssetKind, Mark},
	payments::{
		cheque::{ChequeContents, ChequeHash, seal},
		receivable::{Receivable, ReceivableSecret, new_receivable},
		records::ReceivableRecord,
		testing::{Board, payments},
	},
	testing::{NOW, TestWallet, coin_read, started_layer},
};

/// Network time in the tests: the layer's clock, in milliseconds.
const NOW_MILLIS: u64 = NOW * 1000;

struct Setup {
	wallet: TestWallet,
	payments: Payments,
	board: Arc<Board>,
	clock: Arc<AtomicU64>,
}

async fn setup() -> Setup {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	let board = Arc::new(Board::default());
	let clock = Arc::new(AtomicU64::new(NOW_MILLIS));
	let payments = payments(layer, wallet.db.clone(), board.clone(), clock.clone());
	Setup { wallet, payments, board, clock }
}

/// A receivable of this user agent, into the main purse.
async fn receivable(setup: &Setup, draw: u8) -> (Receivable, ReceivableSecret) {
	let (receivable, secret) = new_receivable([draw; 32]);
	let record = ReceivableRecord {
		receivable,
		secret: ReceivableSecret(Zeroizing::new(*secret.0)),
		purse: PurseId::MAIN,
		return_receivable: false,
		deposits: Vec::new(),
	};
	setup.wallet.db.write(move |tx| records::record_receivable(tx, &record)).await.unwrap();
	(receivable, secret)
}

async fn outgoing(setup: &Setup, cheque: &CoinPaymentCheque) -> OutgoingCheque {
	let hash = crate::coinage::payments::cheque::cheque_hash(cheque);
	setup.wallet.db.read(move |conn| records::outgoing(conn, hash)).await.unwrap().unwrap()
}

#[tokio::test]
async fn a_paid_cheque_is_recorded_and_committed_before_it_is_returned() {
	let setup = setup().await;
	setup.wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let to = new_receivable([1; 32]).0;
	let cheque =
		setup.payments.pay("shop", PurseId::MAIN, to, 8, Initiator::Product, false).await.expect("the payment starts");
	assert_eq!((cheque.id, cheque.amount), (to.0, 8));

	let recorded = outgoing(&setup, &cheque).await;
	assert_eq!(recorded.cheque, cheque);
	assert!(!recorded.post && !recorded.posted, "the product hands a payment cheque over");
	let view = setup.wallet.view();
	let exported = view.operation(&recorded.export).unwrap().exported.clone();
	assert!(exported.iter().all(|(coin, _)| view.mark(coin) == Some(Mark::Committed)), "the export is committed");
	assert!(setup.board.posts.lock().is_empty(), "layer 2 posts no payment cheque");
	let return_to = recorded.return_to.expect("a payment carries a return receivable");
	let return_record = setup.wallet.db.read(move |conn| records::receivable(conn, return_to)).await.unwrap().unwrap();
	assert!(return_record.return_receivable);
}

#[tokio::test]
async fn a_payment_layer_one_refuses_records_nothing() {
	let setup = setup().await;
	let to = new_receivable([1; 32]).0;
	let refused = setup.payments.pay("shop", PurseId::MAIN, to, 8, Initiator::Product, false).await;
	assert_eq!(refused, Err(PayError::Refused(CoinPaymentError::BalanceLow)));
	assert!(setup.wallet.db.read(records::all_outgoing).await.unwrap().is_empty());
	let low_order = setup.payments.pay("shop", PurseId::MAIN, Receivable([0; 32]), 8, Initiator::User, true).await;
	assert_eq!(low_order, Err(PayError::Refused(CoinPaymentError::ReceivableNotFound)));
}

#[tokio::test]
async fn a_cheque_is_deposited_once_whatever_how_often_it_arrives() {
	let setup = setup().await;
	setup.wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let (to, _) = receivable(&setup, 2).await;
	let cheque = setup.payments.pay("shop", PurseId::MAIN, to, 8, Initiator::User, true).await.unwrap();
	let topic = crate::coinage::payments::topic::payment_topic(to);
	channel::post(setup.board.as_ref(), "shop", topic, &cheque, 0).await.expect("the product hands the cheque over");
	let heard: Vec<_> = setup.payments.listen(to).await.unwrap().collect().await;
	assert_eq!(heard, vec![cheque.clone()]);

	let mut first = setup.payments.deposit("shop", cheque.clone()).await.expect("the deposit starts");
	assert!(matches!(first.next().await, Some(CoinPaymentStatus::Clearing { .. })));
	let _joined = setup.payments.deposit("shop", cheque.clone()).await.expect("depositing again joins");
	let deposits = setup.wallet.db.read(records::deposits).await.unwrap();
	assert_eq!(deposits.len(), 1);
	assert_eq!(deposits[0].1.import.key, derived_key("coinage/deposit", crate::coinage::payments::cheque::cheque_hash(&cheque)));
}

#[tokio::test]
async fn a_cheque_that_does_not_check_out_is_refused_and_leaves_no_record() {
	let setup = setup().await;
	let (to, _) = receivable(&setup, 2).await;
	let contents = |claim_by| ChequeContents {
		coins: vec![(crate::coinage::CoinSecret([1; 64]), crate::coinage::Exponent::new(3).unwrap())],
		return_to: Some(Receivable([9; 32])),
		claim_by,
	};
	let unknown = seal(new_receivable([3; 32]).0, 8, &contents(NOW_MILLIS), [4; 32], [5; 12]).unwrap();
	assert_eq!(setup.payments.deposit("shop", unknown).await.err(), Some(CoinPaymentError::ReceivableNotFound));
	let far = NOW_MILLIS + 3 * 3600 * 1000;
	let too_late = seal(to, 8, &contents(far), [4; 32], [5; 12]).unwrap();
	assert_eq!(setup.payments.deposit("shop", too_late).await.err(), Some(CoinPaymentError::BadCoins));
	let mut tampered = seal(to, 8, &contents(NOW_MILLIS), [4; 32], [5; 12]).unwrap();
	tampered.amount = 9;
	assert_eq!(setup.payments.deposit("shop", tampered).await.err(), Some(CoinPaymentError::BadCoins));
	assert!(setup.wallet.db.read(records::deposits).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_cancellation_and_the_scheduled_reclaim_join_one_operation() {
	let setup = setup().await;
	setup.wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let to = new_receivable([1; 32]).0;
	let cheque = setup.payments.pay("shop", PurseId::MAIN, to, 8, Initiator::User, true).await.unwrap();
	let hash = crate::coinage::payments::cheque::cheque_hash(&cheque);
	let _cancellation = setup.payments.cancel(hash).await.expect("the cancellation starts");
	assert_eq!(outgoing(&setup, &cheque).await.reclaims.len(), 1);

	let reclaim_at = outgoing(&setup, &cheque).await.claim_by + 700_000;
	setup.clock.store(reclaim_at, core::sync::atomic::Ordering::SeqCst);
	setup.payments.run_due(NOW).await.unwrap();
	assert_eq!(outgoing(&setup, &cheque).await.reclaims.len(), 1, "the reclaim in progress is joined");
	assert_eq!(setup.payments.cancel(ChequeHash([0; 32])).await.err(), Some(CoinPaymentError::ReceivableNotFound));
}

#[tokio::test]
async fn a_receivable_with_nothing_to_refund_ends_done_at_zero() {
	let setup = setup().await;
	let (to, _) = receivable(&setup, 2).await;
	let statuses: Vec<_> = setup.payments.refund("shop", to).await.unwrap().collect().await;
	assert_eq!(statuses, vec![CoinPaymentStatus::Done { cleared: 0, reference: crate::coinage::payments::clearing::build(Vec::new()) }]);
	assert_eq!(setup.payments.refund("shop", Receivable([8; 32])).await.err(), Some(CoinPaymentError::ReceivableNotFound));
}

#[tokio::test]
async fn the_cheque_status_follows_its_coins() {
	let setup = setup().await;
	setup.wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let to = new_receivable([1; 32]).0;
	let cheque = setup.payments.pay("shop", PurseId::MAIN, to, 8, Initiator::User, true).await.unwrap();
	let hash = crate::coinage::payments::cheque::cheque_hash(&cheque);
	let mut progress = setup.payments.cheque_status(hash).await.unwrap();
	assert_eq!(progress.next().await, Some(ChequeProgress { claimable: 8, ..Default::default() }));
}

mod use_cases;
