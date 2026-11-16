use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		PrivacyPolicy,
		ledger::{BlockRef, Facts, mint_installation, observe, snapshot},
		operation::OpKey,
		params::validate,
		registration::{Admission, Rows},
		testing::{
			chain_config, chain_params, instance_record, people_metadata, register_rows, set_status, unbuilt_engine,
		},
	},
	installation::InstallationId,
	store::{Db, open_memory_core_db},
};

/// A time in period 3 of the fixture's one-day periods.
const NOW: u64 = 3 * 86_400 + 100;

fn ledger() -> Db {
	let db = open_memory_core_db();
	block_on(db.write(|tx| {
		mint_installation(tx, InstallationId([9; 32]), &PrivacyPolicy::fastest(), None, 0)?.unwrap();
		Ok(())
	}))
	.unwrap();
	db
}

fn token(slot: u32) -> TokenId {
	TokenId { period: 3, slot }
}

fn present(db: &Db, id: u8, slot: u32) -> crate::durable::DurableTxId {
	let engine = unbuilt_engine(db.clone());
	let operation = OpKey { scope: b"p".to_vec(), id: [id; 32] };
	let rows = Rows { tokens: vec![token(slot)], ..Rows::default() };
	register_rows(&engine, &operation, Admission::Ordinary, vec![rows]).unwrap()[0]
}

fn view(db: &Db) -> Snapshot {
	block_on(db.read(snapshot)).unwrap()
}

#[test]
fn a_token_state_follows_the_records_that_present_it() {
	let db = ledger();
	assert_eq!(token_state(token(0), &view(&db)), TokenState::NotApplied);
	let record = present(&db, 1, 0);
	let state = |status, failure| {
		set_status(&db, record, status, failure);
		token_state(token(0), &view(&db))
	};
	assert_eq!(state("PENDING_SUBMISSION", None), TokenState::Unknown);
	assert_eq!(state("PENDING", None), TokenState::Unknown);
	assert_eq!(state("FAILURE", Some("EXPIRED")), TokenState::NotApplied);
	assert_eq!(state("FAILURE", Some("REJECTED")), TokenState::NotApplied);
	assert_eq!(state("FAILURE", Some("ABANDONED")), TokenState::NotApplied);
	assert_eq!(state("FAILURE", Some("DISPATCH_FAILED")), TokenState::Applied);
	assert_eq!(state("FINALIZED_SUCCESS", None), TokenState::Applied);
}

#[test]
fn a_token_the_chain_shows_consumed_is_applied() {
	let db = ledger();
	let facts = Facts {
		at: BlockRef { number: 1, hash: [1; 32] },
		reads: vec![(Subject::Token(token(2)), Read::Token { consumed: true })],
	};
	block_on(db.write(move |tx| observe(tx, &facts, NOW, 255))).unwrap();
	assert_eq!(token_state(token(2), &view(&db)), TokenState::Applied);
}

fn observe_tokens(db: &Db, block: u32, reads: Vec<(Subject, Read)>) {
	let facts = Facts { at: BlockRef { number: block, hash: [block as u8; 32] }, reads };
	block_on(db.write(move |tx| observe(tx, &facts, NOW, 255))).unwrap();
}

#[test]
fn a_period_the_chain_shows_fully_consumed_leaves_no_free_token() {
	let db = ledger();
	let consumed = (0..20).map(|slot| (Subject::Token(token(slot)), Read::Token { consumed: true })).collect();
	observe_tokens(&db, 1, consumed);
	let supply = supply(NOW, &view(&db), &chain_params());
	assert_eq!(supply.free().count(), 0);
	assert_eq!(resolve(1, &supply), None);
}

#[test]
fn an_empty_allowance_offers_no_token() {
	let db = ledger();
	let config = crate::coinage::ChainConfig { unload_allowance: 0, ..chain_config() };
	let params = validate(&people_metadata(), Some(&instance_record()), Some(16), config).expect("an empty allowance is valid");
	let supply = supply(NOW, &view(&db), &params);
	assert_eq!(supply.free().count(), 0);
	assert_eq!(resolve(1, &supply), None);
}

/// A failed read is no evidence that a consumed token became free again.
#[test]
fn a_failed_token_read_keeps_what_was_last_observed() {
	let db = ledger();
	observe_tokens(&db, 1, vec![(Subject::Token(token(2)), Read::Token { consumed: true })]);
	observe_tokens(&db, 2, vec![(Subject::Token(token(2)), Read::Failed)]);
	assert_eq!(token_state(token(2), &view(&db)), TokenState::Applied);
}

#[test]
fn resolution_takes_free_slots_lowest_first_and_refuses_a_short_plan() {
	let db = ledger();
	let held = present(&db, 1, 0);
	let applied = present(&db, 2, 2);
	set_status(&db, applied, "FINALIZED_SUCCESS", None);
	let supply = supply(NOW, &view(&db), &chain_params());
	assert_eq!((supply.period, supply.allowance, supply.reserve), (3, 20, 4));
	assert_eq!(resolve(3, &supply), Some(vec![token(1), token(3), token(4)]));
	assert_eq!(resolve(0, &supply), Some(Vec::new()));
	assert_eq!(resolve(19, &supply), None, "only 18 of 20 slots are free");
	set_status(&db, held, "FAILURE", Some("REJECTED"));
	let freed = super::supply(NOW, &view(&db), &chain_params());
	assert_eq!(resolve(1, &freed), Some(vec![token(0)]));
}

#[test]
fn the_supply_runs_low_at_the_reserve() {
	let db = ledger();
	for slot in 0..15 {
		let record = present(&db, slot as u8 + 1, slot);
		set_status(&db, record, "FINALIZED_SUCCESS", None);
	}
	let five_free = supply(NOW, &view(&db), &chain_params());
	assert!(!running_low(&five_free));
	let record = present(&db, 99, 15);
	set_status(&db, record, "FINALIZED_SUCCESS", None);
	assert!(running_low(&supply(NOW, &view(&db), &chain_params())), "four free tokens are the reserve");
}
