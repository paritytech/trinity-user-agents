
use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		PrivacyPolicy,
		ledger::{AssetKind, BlockRef, CoinOnChain, EntryOnChain, Facts, allocate, mint_installation, observe, snapshot},
		testing::{chain_params, observe_allowance},
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

fn hold(db: &Db, kind: AssetKind, exponent: u8, read: Read) -> Asset {
	let outputs = [Exponent::new(exponent).unwrap()];
	let asset = block_on(db.write(move |tx| allocate(tx, PurseId::MAIN, kind, &outputs))).unwrap()[0];
	let facts = Facts { at: BlockRef { number: 1, hash: [1; 32] }, reads: vec![(Subject::Asset(asset), read)] };
	block_on(db.write(move |tx| observe(tx, &facts, NOW - 100, 255))).unwrap();
	asset
}

fn coin(db: &Db, exponent: u8) -> Asset {
	hold(db, AssetKind::Coin, exponent, coin_read(exponent, None))
}

fn coin_read(exponent: u8, locked_until: Option<u64>) -> Read {
	Read::Coin { now: Some(CoinOnChain { exponent: Exponent::new(exponent).unwrap(), age: 0, locked_until }), emptied_by: None }
}

fn entry(db: &Db, exponent: u8, ring: u32) -> Asset {
	let read = Read::Entry(Some(EntryOnChain {
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
	}));
	hold(db, AssetKind::Entry, exponent, read)
}

fn request(want: Want) -> SelectRequest {
	SelectRequest { purse: PurseId::MAIN, want, forms: Forms::Any, entries_first: false, held_back: false }
}

fn select_on(db: &Db, request: SelectRequest, params: &ChainParams) -> Result<Selection, SelectError> {
	select(&block_on(db.read(snapshot)).unwrap(), request, NOW, params)
}

fn raw(exponents: &[Exponent]) -> Vec<u8> {
	exponents.iter().map(|exponent| exponent.get()).collect()
}

fn allow(db: &Db, allowance: u32) {
	block_on(db.write(move |tx| observe_allowance(tx, allowance))).unwrap();
}

#[test]
fn coins_that_already_sum_are_spent_as_they_are() {
	let db = wallet();
	let four = coin(&db, 2);
	let one = coin(&db, 0);
	coin(&db, 3);
	let selection = select_on(&db, request(Want::Amount(5)), &chain_params()).unwrap();
	assert_eq!(selection.inputs, [four, one]);
	assert_eq!(raw(&selection.outputs), [2, 0]);
	assert!(selection.splits.is_empty());
}

#[test]
fn coins_are_reshaped_by_the_split_plan_when_none_sum() {
	let db = wallet();
	let thirty_two = coin(&db, 5);
	let four = coin(&db, 2);
	let selection = select_on(&db, request(Want::Amount(27)), &chain_params()).unwrap();
	let splits: Vec<(Asset, Vec<u8>)> = selection.splits.iter().map(|(coin, produce)| (*coin, raw(produce))).collect();
	assert_eq!(splits, [(four, vec![0, 0, 1]), (thirty_two, vec![3, 3, 4])]);
	assert_eq!(selection.outputs.iter().map(|e| e.value()).sum::<u64>(), 27);
	assert_eq!(selection.inputs, [four, thirty_two]);
}

#[test]
fn a_spend_for_a_product_takes_entries_first() {
	let db = wallet();
	coin(&db, 4);
	let held = entry(&db, 4, 0);
	let product = SelectRequest { entries_first: true, ..request(Want::Amount(16)) };
	assert_eq!(select_on(&db, product, &chain_params()).unwrap().inputs, [held]);
	let user = select_on(&db, request(Want::Amount(16)), &chain_params()).unwrap();
	assert!(matches!(user.inputs[..], [Asset::Coin { .. }]));
}

#[test]
fn usable_now_comes_before_usable_later() {
	let db = wallet();
	let locked = hold(&db, AssetKind::Coin, 3, coin_read(3, Some(NOW + 60)));
	let free = coin(&db, 3);
	assert_eq!(select_on(&db, request(Want::Amount(8)), &chain_params()).unwrap().inputs, [free]);
	let both = select_on(&db, request(Want::Amount(16)), &chain_params()).unwrap();
	assert_eq!(both.inputs.len(), 2);
	assert!(both.inputs.contains(&locked), "a locked coin is taken when nothing usable now covers it");
}

#[test]
fn selection_fails_only_for_lack_of_value_or_tokens() {
	let db = wallet();
	coin(&db, 1);
	entry(&db, 3, 0);
	assert_eq!(
		select_on(&db, request(Want::Amount(11)), &chain_params()),
		Err(SelectError::InsufficientFunds { available: 10 })
	);
	let only_entries = SelectRequest { forms: Forms::EntriesOnly, ..request(Want::Amount(8)) };
	allow(&db, 0);
	assert_eq!(select_on(&db, only_entries, &chain_params()), Err(SelectError::NoUnloadToken));
	allow(&db, 1);
	assert!(select_on(&db, only_entries, &chain_params()).is_ok());
}

#[test]
fn coins_and_entries_together_cover_what_neither_does_alone() {
	let db = wallet();
	let two = coin(&db, 1);
	let eight = entry(&db, 3, 0);
	let selection = select_on(&db, request(Want::Amount(10)), &chain_params()).unwrap();
	assert_eq!(selection.inputs, [two, eight]);
	assert_eq!(selection.outputs.iter().map(|e| e.value()).sum::<u64>(), 10);
}

#[test]
fn a_drain_takes_everything_that_is_not_never() {
	let db = wallet();
	let a = coin(&db, 0);
	let b = entry(&db, 2, 0);
	let selection = select_on(&db, request(Want::Everything), &chain_params()).unwrap();
	assert_eq!(selection.inputs.len(), 2);
	assert!(selection.inputs.contains(&a) && selection.inputs.contains(&b));
	assert_eq!(selection.outputs.iter().map(|e| e.value()).sum::<u64>(), 5);
}

#[test]
fn outputs_sum_to_the_request_on_generated_purses() {
	let mut state = 0x2545_f491_u32;
	let mut next = move || {
		state ^= state << 13;
		state ^= state >> 17;
		state ^= state << 5;
		state
	};
	for _ in 0..12 {
		let db = wallet();
		let mut total = 0;
		for _ in 0..(next() % 6 + 1) {
			let exponent = (next() % 8) as u8;
			total += 1u64 << exponent;
			if next() % 3 == 0 { entry(&db, exponent, next() % 2) } else { coin(&db, exponent) };
		}
		for amount in [1, total / 2, total] {
			let selection = select_on(&db, request(Want::Amount(amount.max(1))), &chain_params());
			if let Ok(selection) = selection {
				assert_eq!(selection.outputs.iter().map(|e| e.value()).sum::<u64>(), amount.max(1));
				let again = select_on(&db, request(Want::Amount(amount.max(1))), &chain_params()).unwrap();
				assert_eq!(selection, again, "selection is deterministic");
			}
		}
	}
}
