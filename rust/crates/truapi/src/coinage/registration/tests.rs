
use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		Exponent, PrivacyPolicy, PurseId,
		ledger::{AssetKind, allocate, mark_disclosed, mint_installation, snapshot},
		testing::{register_rows, set_status, unbuilt_engine},
	},
	installation::InstallationId,
	store::{Db, open_memory_core_db},
};

fn ledger() -> Db {
	let db = open_memory_core_db();
	block_on(db.write(|tx| {
		mint_installation(tx, InstallationId([9; 32]), &PrivacyPolicy::fastest(), None, 0)?.unwrap();
		Ok(())
	}))
	.unwrap();
	db
}

fn coins(db: &Db, count: usize) -> Vec<Asset> {
	let outputs = vec![Exponent::new(0).unwrap(); count];
	block_on(db.write(move |tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &outputs))).unwrap()
}

fn op(id: u8) -> OpKey {
	OpKey { scope: b"product".to_vec(), id: [id; 32] }
}

fn spend(inputs: &[Asset], outputs: &[Asset]) -> Rows {
	Rows { inputs: inputs.to_vec(), outputs: outputs.to_vec(), tokens: Vec::new() }
}

#[test]
fn an_input_is_held_by_at_most_one_transaction_that_has_not_failed() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let assets = coins(&db, 3);
	let first = register_rows(&engine, &op(1), Admission::Ordinary, vec![spend(&assets[..1], &assets[1..2])]).unwrap();
	let again = register_rows(&engine, &op(2), Admission::Ordinary, vec![spend(&assets[..1], &assets[2..3])]);
	assert_eq!(again, Err(RegisterError::InputTaken(assets[0])));
	set_status(&db, first[0], "FINALIZED_SUCCESS", None);
	let after_success = register_rows(&engine, &op(2), Admission::Ordinary, vec![spend(&assets[..1], &assets[2..3])]);
	assert_eq!(after_success, Err(RegisterError::InputTaken(assets[0])), "a spent input stays held");
	set_status(&db, first[0], "FAILURE", Some("REJECTED"));
	assert!(register_rows(&engine, &op(2), Admission::Ordinary, vec![spend(&assets[..1], &assets[2..3])]).is_ok());
}

#[test]
fn a_refused_registration_writes_nothing() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let assets = coins(&db, 3);
	let refused = register_rows(
		&engine,
		&op(1),
		Admission::Ordinary,
		vec![spend(&assets[..1], &assets[1..2]), spend(&assets[..1], &assets[2..3])],
	);
	assert_eq!(refused, Err(RegisterError::InputTaken(assets[0])));
	let view = block_on(db.read(snapshot)).unwrap();
	assert_eq!(view.registrations().records().count(), 0);
	assert!(block_on(engine.group(&coinage_domain(), &op(1).group())).unwrap().is_empty());
}

#[test]
fn every_output_is_fresh() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let assets = coins(&db, 3);
	register_rows(&engine, &op(1), Admission::Ordinary, vec![spend(&assets[..1], &assets[1..2])]).unwrap();
	let twice = register_rows(&engine, &op(2), Admission::Ordinary, vec![spend(&assets[2..3], &assets[1..2])]);
	assert_eq!(twice, Err(RegisterError::OutputNotFresh(assets[1])));
	let peer = Asset::PeerCoin([1; 32]);
	let peer_output = register_rows(&engine, &op(2), Admission::Ordinary, vec![spend(&assets[2..3], &[peer])]);
	assert_eq!(peer_output, Err(RegisterError::OutputNotFresh(peer)));
}

#[test]
fn a_token_is_presented_by_at_most_one_transaction_that_has_not_failed() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let token = TokenId { period: 7, slot: 0 };
	let rows = || vec![Rows { tokens: vec![token], ..Rows::default() }];
	let first = register_rows(&engine, &op(1), Admission::Ordinary, rows()).unwrap();
	assert_eq!(register_rows(&engine, &op(2), Admission::Ordinary, rows()), Err(RegisterError::TokenTaken(token)));
	set_status(&db, first[0], "FAILURE", Some("EXPIRED"));
	assert!(register_rows(&engine, &op(2), Admission::Ordinary, rows()).is_ok());
}

#[test]
fn a_disclosed_input_is_taken_only_by_an_import_or_a_reclaim() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let assets = coins(&db, 4);
	let marked = assets[0];
	block_on(db.write(move |tx| mark_disclosed(tx, &[marked]))).unwrap().unwrap();
	let ordinary = register_rows(&engine, &op(1), Admission::Ordinary, vec![spend(&[marked], &assets[1..2])]);
	assert_eq!(ordinary, Err(RegisterError::Disclosed(marked)));
	assert!(register_rows(&engine, &op(1), Admission::Reclaim, vec![spend(&[marked], &assets[2..3])]).is_ok());
}

#[test]
fn a_held_coin_cannot_be_marked_for_export() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let assets = coins(&db, 2);
	register_rows(&engine, &op(1), Admission::Ordinary, vec![spend(&assets[..1], &assets[1..2])]).unwrap();
	let held = assets[0];
	assert_eq!(block_on(db.write(move |tx| mark_disclosed(tx, &[held]))).unwrap(), Err(held));
}

#[test]
fn the_snapshot_indexes_consumers_creators_presenters_and_dependencies() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let assets = coins(&db, 4);
	let token = TokenId { period: 1, slot: 2 };
	let mint = register_rows(&engine, &op(1), Admission::Ordinary, vec![spend(&assets[..1], &assets[1..2])]).unwrap();
	let spend_minted = Rows { inputs: vec![assets[1]], outputs: vec![assets[2]], tokens: vec![token] };
	let next = register_rows(&engine, &op(2), Admission::Ordinary, vec![spend_minted]).unwrap();
	let view = block_on(db.read(snapshot)).unwrap();
	let registrations = view.registrations();
	let record = registrations.record(next[0]).unwrap();
	assert_eq!(record.depends_on, mint);
	assert_eq!(record.operation, op(2));
	assert_eq!(record.state.status, DurableTxStatus::PendingSubmission);
	assert_eq!(registrations.creator(&assets[1]).map(|facts| facts.record), Some(mint[0]));
	let consumers: Vec<_> = registrations.consumers(&assets[1]).map(|facts| facts.record).collect();
	assert_eq!(consumers, next);
	let presenters: Vec<_> = registrations.presenters(&token).map(|facts| facts.record).collect();
	assert_eq!(presenters, next);
	assert_eq!(registrations.of_operation(&op(1)).count(), 1);
}

/// Two records of one batch minting the same address would both be its
/// creator, so the batch is refused whole.
#[test]
fn a_batch_minting_one_output_twice_is_refused() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let assets = coins(&db, 3);
	let batch = vec![spend(&assets[..1], &assets[2..]), spend(&assets[1..2], &assets[2..])];
	let refused = register_rows(&engine, &op(1), Admission::Ordinary, batch);
	let view = block_on(db.read(snapshot)).unwrap();
	assert_eq!((refused, view.registrations().records().count()), (Err(RegisterError::OutputNotFresh(assets[2])), 0));
}
