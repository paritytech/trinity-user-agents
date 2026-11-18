use futures::executor::block_on;

use super::*;
use crate::store::{Db, open_memory_core_db};

fn key(id: u8) -> OpKey {
	OpKey { scope: b"coinage/layer-2".to_vec(), id: [id; 32] }
}

fn operation(id: u8) -> OperationRecord {
	OperationRecord { key: key(id), refused: None }
}

fn receivable_record(id: u8) -> ReceivableRecord {
	ReceivableRecord {
		receivable: Receivable([id; 32]),
		secret: ReceivableSecret(Zeroizing::new([id + 1; 32])),
		purse: PurseId(2),
		return_receivable: false,
		deposits: Vec::new(),
	}
}

fn deposit_of(cheque: u8) -> DepositRecord {
	DepositRecord {
		cheque: ChequeHash([cheque; 32]),
		product: "shop".to_owned(),
		amount: 9,
		claim_by: 5_000,
		import: operation(cheque),
		return_to: Some(Receivable([7; 32])),
		refunds: Vec::new(),
		automatic_refund_made: false,
	}
}

fn outgoing_of(cheque: u8, post: bool, claim_by: u64) -> OutgoingCheque {
	OutgoingCheque {
		hash: ChequeHash([cheque; 32]),
		cheque: CoinPaymentCheque { id: [3; 32], amount: 9, encrypted_secrets: vec![1, 2, 3] },
		source: PurseId::MAIN,
		payee: Receivable([3; 32]),
		export: key(cheque),
		return_to: Some(Receivable([4; 32])),
		claim_by,
		product: "shop".to_owned(),
		post,
		posted: false,
		reclaims: Vec::new(),
	}
}

fn write(db: &Db, change: impl FnOnce(&Transaction<'_>) -> Result<(), DbError> + Send + 'static) {
	block_on(db.write(change)).unwrap();
}

#[test]
fn a_receivable_reads_back_with_its_deposits_refunds_and_refusals() {
	let db = open_memory_core_db();
	let record = receivable_record(1);
	let stored = record.clone();
	write(&db, move |tx| record_receivable(tx, &stored));
	write(&db, |tx| record_deposit(tx, Receivable([1; 32]), &deposit_of(5)));
	let refund = RefundRecord { operation: operation(6), cheque: None };
	write(&db, move |tx| record_refund(tx, ChequeHash([5; 32]), &refund));
	write(&db, |tx| record_refund_cheque(tx, ChequeHash([5; 32]), &key(6), ChequeHash([8; 32])));
	write(&db, |tx| record_refusal(tx, &key(6), &CoinPaymentError::BalanceLow));
	write(&db, |tx| mark_automatic_refund(tx, ChequeHash([5; 32])));

	let read = block_on(db.read(|conn| receivable(conn, Receivable([1; 32])))).unwrap().unwrap();
	let expected_deposit = DepositRecord {
		refunds: vec![RefundRecord {
			operation: OperationRecord { key: key(6), refused: Some(CoinPaymentError::BalanceLow) },
			cheque: Some(ChequeHash([8; 32])),
		}],
		automatic_refund_made: true,
		..deposit_of(5)
	};
	assert_eq!(read, ReceivableRecord { deposits: vec![expected_deposit.clone()], ..record });
	let found = block_on(db.read(|conn| deposit(conn, ChequeHash([5; 32])))).unwrap();
	assert_eq!(found, Some((Receivable([1; 32]), expected_deposit)));
}

#[test]
fn only_cheques_layer_2_posts_and_has_not_posted_before_their_deadline_are_unposted() {
	let db = open_memory_core_db();
	write(&db, |tx| record_outgoing(tx, &outgoing_of(1, true, 10_000)));
	write(&db, |tx| record_outgoing(tx, &outgoing_of(2, false, 10_000)));
	write(&db, |tx| record_outgoing(tx, &outgoing_of(3, true, 1_000)));
	write(&db, |tx| record_outgoing(tx, &outgoing_of(4, true, 10_000)));
	write(&db, |tx| mark_posted(tx, ChequeHash([4; 32])));
	write(&db, |tx| record_reclaim(tx, ChequeHash([1; 32]), &key(9)));

	let unposted = block_on(db.read(|conn| unposted(conn, 5_000))).unwrap();
	assert_eq!(unposted.iter().map(|cheque| cheque.hash).collect::<Vec<_>>(), vec![ChequeHash([1; 32])]);
	assert_eq!(unposted[0].reclaims, vec![operation(9)]);
}

#[test]
fn an_rfc6_id_names_one_operation_of_its_product() {
	let db = open_memory_core_db();
	let record = Rfc6Record {
		product: b"shop".to_vec(),
		id: [1; 32],
		kind: Rfc6Kind::TopUp,
		amount: 100,
		source: Some([6; 32]),
		funding: None,
		operation: operation(1),
	};
	let first = record.clone();
	assert_eq!(block_on(db.write(move |tx| record_rfc6(tx, &first))).unwrap(), Ok(()));
	let again = record.clone();
	assert_eq!(block_on(db.write(move |tx| record_rfc6(tx, &again))).unwrap(), Err(AlreadyExists));
	let other_product = Rfc6Record { product: b"other".to_vec(), ..record.clone() };
	assert_eq!(block_on(db.write(move |tx| record_rfc6(tx, &other_product))).unwrap(), Ok(()));
	assert_eq!(block_on(db.read(|conn| rfc6(conn, b"shop", [1; 32]))).unwrap(), Some(record));
	assert_eq!(block_on(db.read(|conn| top_ups_from(conn, [6; 32]))).unwrap().len(), 2);
}
