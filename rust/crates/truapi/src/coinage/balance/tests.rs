
use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		ChainConfig, PrivacyPolicy,
		ledger::{AssetKind, BlockRef, CoinOnChain, EntryOnChain, Facts, allocate, create_purse, mint_installation, observe, snapshot},
		operation::OpKey,
		params::validate,
		registration::{Admission, Rows},
		testing::{
			TestWallet, chain_config, chain_params, instance_record, people_metadata, register_rows, set_status,
			unbuilt_engine,
			planning::{allocated, in_ring},
		},
	},
	durable::DurableTxId,
	installation::InstallationId,
	store::{Db, open_memory_core_db},
};

const NOW: u64 = 1_000_000;

fn wallet() -> Db {
	let db = open_memory_core_db();
	block_on(db.write(|tx| {
		mint_installation(tx, InstallationId([9; 32]), &PrivacyPolicy::fastest(), None, 0)?.unwrap();
		Ok(())
	}))
	.unwrap();
	db
}

fn holding(db: &Db, purse: PurseId, kind: AssetKind, exponent: u8, read: Read) -> Asset {
	let outputs = [Exponent::new(exponent).unwrap()];
	let asset = block_on(db.write(move |tx| allocate(tx, purse, kind, &outputs))).unwrap()[0];
	let facts = Facts { at: BlockRef { number: 1, hash: [1; 32] }, reads: vec![(Subject::Asset(asset), read)] };
	block_on(db.write(move |tx| observe(tx, &facts, NOW - 100, 255))).unwrap();
	asset
}

fn coin(exponent: u8, age: u32, locked_until: Option<u64>) -> Read {
	Read::Coin {
		now: Some(CoinOnChain { exponent: Exponent::new(exponent).unwrap(), age, locked_until }),
		emptied_by: None,
	}
}

fn entry(ring: u32) -> Read {
	Read::Entry(Some(EntryOnChain {
		ring,
		revision: 1,
		page: 0,
		position: 0,
		members: 10,
		included: 10,
		closed: false,
		expires_at: None,
		locked_until: None,
	}))
}

fn params_with_allowance(allowance: u32) -> ChainParams {
	let config = ChainConfig { unload_allowance: allowance, ..chain_config() };
	validate(&people_metadata(), Some(&instance_record()), Some(16), config).unwrap()
}

#[test]
fn every_holding_falls_in_exactly_one_bucket() {
	let db = wallet();
	holding(&db, PurseId::MAIN, AssetKind::Coin, 3, coin(3, 0, None));
	holding(&db, PurseId::MAIN, AssetKind::Coin, 2, coin(2, 0, Some(NOW + 60)));
	holding(&db, PurseId::MAIN, AssetKind::Coin, 1, coin(1, 14, None));
	let view = block_on(db.read(snapshot)).unwrap();
	let balance = balance(&view, PurseId::MAIN, NOW, &chain_params());
	assert_eq!(balance, Balance { spendable: 8, held_back: 0, arriving: 4 + 2 });
}

#[test]
fn an_entry_is_spendable_only_while_a_token_can_unload_it() {
	let db = wallet();
	holding(&db, PurseId::MAIN, AssetKind::Entry, 4, entry(0));
	holding(&db, PurseId::MAIN, AssetKind::Entry, 4, entry(0));
	holding(&db, PurseId::MAIN, AssetKind::Entry, 2, entry(5));
	let view = block_on(db.read(snapshot)).unwrap();
	let both = balance(&view, PurseId::MAIN, NOW, &params_with_allowance(2));
	assert_eq!(both, Balance { spendable: 36, held_back: 0, arriving: 0 }, "one token per ring");
	let one = balance(&view, PurseId::MAIN, NOW, &params_with_allowance(1));
	assert_eq!(one, Balance { spendable: 32, held_back: 0, arriving: 4 }, "the larger group gets the token");
	let none = balance(&view, PurseId::MAIN, NOW, &params_with_allowance(0));
	assert_eq!(none, Balance { spendable: 0, held_back: 0, arriving: 36 });
}

#[test]
fn the_main_purse_takes_tokens_before_product_purses() {
	let db = wallet();
	let product = block_on(db.write(|tx| create_purse(tx, "p".into(), Vec::new(), 0))).unwrap();
	holding(&db, product, AssetKind::Entry, 6, entry(0));
	holding(&db, PurseId::MAIN, AssetKind::Entry, 1, entry(3));
	let view = block_on(db.read(snapshot)).unwrap();
	let params = params_with_allowance(1);
	assert_eq!(balance(&view, PurseId::MAIN, NOW, &params).spendable, 2);
	assert_eq!(balance(&view, product, NOW, &params), Balance { spendable: 0, held_back: 0, arriving: 64 });
}

/// Registers the record that mints `assets` and gives it `status`.
fn minted_by(db: &Db, id: u8, assets: Vec<Asset>, status: &'static str, failure: Option<&'static str>) -> DurableTxId {
	let operation = OpKey { scope: b"mint".to_vec(), id: [id; 32] };
	let rows = Rows { outputs: assets, ..Rows::default() };
	let record = register_rows(&unbuilt_engine(db.clone()), &operation, Admission::Ordinary, vec![rows]).unwrap()[0];
	set_status(db, record, status, failure);
	record
}

fn main_balance(db: &Db) -> Balance {
	balance(&block_on(db.read(snapshot)).unwrap(), PurseId::MAIN, NOW, &chain_params())
}

const NOTHING: Balance = Balance { spendable: 0, held_back: 0, arriving: 0 };

#[test]
fn a_coin_absent_from_chain_is_arriving_until_its_mint_fails() {
	let db = wallet();
	let coin = allocated(&db, PurseId::MAIN, AssetKind::Coin, 2);
	let record = minted_by(&db, 1, vec![coin], "PENDING", None);
	assert_eq!(main_balance(&db), Balance { arriving: 4, ..NOTHING });
	set_status(&db, record, "FAILURE", Some("EXPIRED"));
	assert_eq!(main_balance(&db), NOTHING);
}

#[test]
fn a_coin_whose_mint_finalized_before_it_was_read_is_still_arriving() {
	let db = wallet();
	let coin = allocated(&db, PurseId::MAIN, AssetKind::Coin, 2);
	minted_by(&db, 1, vec![coin], "FINALIZED_SUCCESS", None);
	assert_eq!(main_balance(&db), Balance { arriving: 4, ..NOTHING });
}

#[test]
fn a_coin_counts_in_the_total_for_every_mint_status_but_failure() {
	for (id, status) in [(1, "PENDING_SUBMISSION"), (2, "PENDING"), (3, "PENDING_SUCCESS"), (4, "FINALIZED_SUCCESS")] {
		let db = wallet();
		let present = holding(&db, PurseId::MAIN, AssetKind::Coin, 2, coin(2, 0, None));
		let absent = allocated(&db, PurseId::MAIN, AssetKind::Coin, 1);
		minted_by(&db, id, vec![present, absent], status, None);
		let total = main_balance(&db);
		assert_eq!(total.spendable + total.held_back + total.arriving, 6, "{status}");
	}
}

#[test]
fn a_coin_whose_mint_failed_is_in_no_bucket() {
	let db = wallet();
	let read_before_the_failure = holding(&db, PurseId::MAIN, AssetKind::Coin, 2, coin(2, 0, None));
	minted_by(&db, 1, vec![read_before_the_failure], "FAILURE", Some("REJECTED"));
	assert_eq!(main_balance(&db), NOTHING);
}

#[test]
fn an_entry_whose_mint_finalized_before_its_place_was_read_is_still_arriving() {
	let db = wallet();
	let entry = allocated(&db, PurseId::MAIN, AssetKind::Entry, 2);
	minted_by(&db, 1, vec![entry], "FINALIZED_SUCCESS", None);
	assert_eq!(main_balance(&db), Balance { arriving: 4, ..NOTHING });
}

#[test]
fn an_entry_whose_mint_failed_is_in_no_bucket() {
	let db = wallet();
	let entry = allocated(&db, PurseId::MAIN, AssetKind::Entry, 2);
	minted_by(&db, 1, vec![entry], "FAILURE", Some("EXPIRED"));
	assert_eq!(main_balance(&db), NOTHING);
}

#[test]
fn entries_still_gaining_privacy_are_counted_apart_from_spendable_ones() {
	let wallet = TestWallet::new(PrivacyPolicy::balanced());
	wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, in_ring(0, 767));
	wallet.hold(PurseId::MAIN, AssetKind::Entry, 2, in_ring(1, 20));
	assert_eq!(main_balance(&wallet.db), Balance { spendable: 8, held_back: 4, arriving: 0 });
}
