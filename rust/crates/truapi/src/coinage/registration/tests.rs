
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

/// A peer's coin is only ever an input: claiming it must never make it one
/// of the wallet's own assets.
#[test]
fn a_peer_coin_input_never_enters_the_wallets_assets() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let peer = Asset::PeerCoin([5; 32]);
	let output = coins(&db, 1)[0];
	let claim = register_rows(&engine, &op(1), Admission::Import, vec![spend(&[peer], &[output])]).unwrap();
	let view = block_on(db.read(snapshot)).unwrap();
	let consumers: Vec<_> = view.registrations().consumers(&peer).map(|facts| facts.record).collect();
	assert_eq!(consumers, claim);
	assert!(view.asset(&peer).is_none());
	assert!(view.all_assets().all(|record| record.asset != peer));
}

#[test]
fn the_same_index_under_two_installations_is_two_assets() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let current = coins(&db, 1)[0];
	let Asset::Coin { purse, index, .. } = current else { unreachable!("allocated as a coin") };
	let earlier = Asset::Coin { purse, installation: InstallationId([4; 32]), index };
	let found = crate::coinage::ledger::AssetRecord { asset: earlier, exponent: Exponent::new(0).unwrap() };
	block_on(db.write(move |tx| crate::coinage::ledger::adopt(tx, &[found]))).unwrap();
	register_rows(&engine, &op(1), Admission::Ordinary, vec![spend(&[current], &[])]).unwrap();
	assert!(register_rows(&engine, &op(2), Admission::Ordinary, vec![spend(&[earlier], &[])]).is_ok(), "holding one leaves the other free");
}

#[test]
fn an_operations_records_are_its_own_in_registration_order_with_what_each_mints() {
	let db = ledger();
	let engine = unbuilt_engine(db.clone());
	let assets = coins(&db, 6);
	let first = register_rows(&engine, &op(1), Admission::Ordinary, vec![spend(&assets[..1], &assets[1..2])]).unwrap();
	register_rows(&engine, &op(2), Admission::Ordinary, vec![spend(&assets[2..3], &[])]).unwrap();
	let third = register_rows(&engine, &op(1), Admission::Ordinary, vec![spend(&assets[3..4], &assets[4..6])]).unwrap();
	let view = block_on(db.read(snapshot)).unwrap();
	let own: Vec<_> = view.registrations().of_operation(&op(1)).map(|facts| (facts.record, facts.rows.outputs.clone())).collect();
	assert_eq!(own, [(first[0], assets[1..2].to_vec()), (third[0], assets[4..6].to_vec())]);
}
