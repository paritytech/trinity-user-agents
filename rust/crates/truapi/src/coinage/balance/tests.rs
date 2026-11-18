
use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		PrivacyPolicy,
		ledger::{AssetKind, BlockRef, CoinOnChain, EntryOnChain, Facts, allocate, create_purse, mint_installation, observe, snapshot},
		operation::OpKey,
		registration::{Admission, Rows},
		testing::{chain_params, observe_allowance, register_rows, set_status, unbuilt_engine},
	},
	installation::InstallationId,
	store::{Db, open_memory_core_db},
};

const NOW: u64 = 1_000_000;

fn wallet() -> Db {
	let db = open_memory_core_db();
	block_on(db.write(|tx| {
		mint_installation(tx, InstallationId([9; 32]), &PrivacyPolicy::fastest(), None, 0)?.unwrap();
		observe_allowance(tx, 20)?;
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
		unloaded: None,
	}))
}

fn balance_with_allowance(db: &Db, purse: PurseId, allowance: u32) -> Balance {
	block_on(db.write(move |tx| observe_allowance(tx, allowance))).unwrap();
	balance(&block_on(db.read(snapshot)).unwrap(), purse, NOW, &chain_params())
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
	let both = balance_with_allowance(&db, PurseId::MAIN, 2);
	assert_eq!(both, Balance { spendable: 36, held_back: 0, arriving: 0 }, "one token per ring");
	let one = balance_with_allowance(&db, PurseId::MAIN, 1);
	assert_eq!(one, Balance { spendable: 32, held_back: 0, arriving: 4 }, "the larger group gets the token");
	let none = balance_with_allowance(&db, PurseId::MAIN, 0);
	assert_eq!(none, Balance { spendable: 0, held_back: 0, arriving: 36 });
}

#[test]
fn the_main_purse_takes_tokens_before_product_purses() {
	let db = wallet();
	let product = block_on(db.write(|tx| create_purse(tx, "p".into(), Vec::new(), 0))).unwrap();
	holding(&db, product, AssetKind::Entry, 6, entry(0));
	holding(&db, PurseId::MAIN, AssetKind::Entry, 1, entry(3));
	assert_eq!(balance_with_allowance(&db, PurseId::MAIN, 1).spendable, 2);
	assert_eq!(balance_with_allowance(&db, product, 1), Balance { spendable: 0, held_back: 0, arriving: 64 });
}

/// The score both shipped wallets draw for a ring.
#[test]
fn fungibility_is_the_share_of_a_ring_that_unloads_have_not_spent() {
	let scores = [
		fungibility(10, 10, 0),
		fungibility(10, 10, 5),
		fungibility(10, 5, 0),
		fungibility(10, 5, 9),
		fungibility(0, 5, 0),
		fungibility(10, 0, 0),
	];
	assert_eq!(scores, [100, 75, 50, 0, 0, 0]);
}

fn entry_unloaded(ring: u32, unloaded: Option<u32>) -> Read {
	match entry(ring) {
		Read::Entry(Some(on_chain)) => Read::Entry(Some(EntryOnChain { unloaded, ..on_chain })),
		other => other,
	}
}

/// A wallet card draws a row per counted coin and entry next to the balance,
/// so both come from one snapshot, and a coin this wallet unloaded carries
/// the ring it left.
#[test]
fn holdings_carry_a_row_per_counted_holding_with_what_the_card_draws() {
	let db = wallet();
	let params = chain_params();
	let spare = holding(&db, PurseId::MAIN, AssetKind::Coin, 3, coin(3, 2, None));
	let locked = holding(&db, PurseId::MAIN, AssetKind::Coin, 2, coin(2, 0, Some(NOW + 60)));
	let unknown = holding(&db, PurseId::MAIN, AssetKind::Entry, 4, entry_unloaded(1, None));
	let left = holding(&db, PurseId::MAIN, AssetKind::Entry, 5, entry_unloaded(0, Some(5)));
	let unloaded = holding(&db, PurseId::MAIN, AssetKind::Coin, 5, coin(5, 1, None));
	let engine = unbuilt_engine(db.clone());
	let rows = Rows { inputs: vec![left], outputs: vec![unloaded], tokens: Vec::new() };
	let unload = register_rows(&engine, &OpKey::own([1; 32]), Admission::Ordinary, vec![rows]).unwrap()[0];
	set_status(&db, unload, "FINALIZED_SUCCESS", None);

	let view = block_on(db.read(snapshot)).unwrap();
	let row = |asset, exponent, ready, age, batch_unloaded, fungibility| Holding {
		asset,
		exponent: Exponent::new(exponent).unwrap(),
		ready,
		age,
		batch_unloaded,
		fungibility,
	};
	let ring_left = Some(fungibility(params.ring_size(), 10, 5));
	assert_eq!(
		holdings(&view, PurseId::MAIN, NOW, &params),
		Holdings {
			balance: balance(&view, PurseId::MAIN, NOW, &params),
			rows: vec![
				row(spare, 3, true, Some(2), false, None),
				row(locked, 2, false, Some(0), false, None),
				row(unloaded, 5, true, Some(1), true, ring_left),
				row(unknown, 4, true, None, false, None),
			],
		}
	);
}
