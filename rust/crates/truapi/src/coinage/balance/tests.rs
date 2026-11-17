
use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		ChainConfig, PrivacyPolicy,
		ledger::{AssetKind, BlockRef, CoinOnChain, EntryOnChain, Facts, allocate, create_purse, mint_installation, observe, snapshot},
		params::validate,
		testing::{chain_config, chain_params, instance_record, people_metadata},
	},
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

mod use_cases;
