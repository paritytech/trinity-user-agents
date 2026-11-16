
use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		ChainConfig, PrivacyPolicy, RecycleAge,
		ledger::{AssetKind, BlockRef, CoinOnChain, EntryOnChain, Facts, allocate, mint_installation, observe, snapshot},
		params::validate,
		testing::{
			TestWallet, chain_config, chain_params, instance_record, people_metadata,
			planning::{aged_coin, in_ring},
		},
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

fn params_with_allowance(allowance: u32) -> ChainParams {
	let config = ChainConfig { unload_allowance: allowance, ..chain_config() };
	validate(&people_metadata(), Some(&instance_record()), Some(16), config).unwrap()
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
	assert_eq!(select_on(&db, only_entries, &params_with_allowance(0)), Err(SelectError::NoUnloadToken));
	assert!(select_on(&db, only_entries, &params_with_allowance(1)).is_ok());
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

fn exponents(raw: &[u8]) -> Vec<Exponent> {
	raw.iter().map(|exponent| Exponent::new(*exponent).unwrap()).collect()
}

fn spend(inputs: Vec<Asset>, splits: Vec<(Asset, Vec<Exponent>)>, outputs: &[u8]) -> Selection {
	Selection { purse: PurseId::MAIN, inputs, splits, outputs: exponents(outputs), depends_on: Vec::new() }
}

#[test]
fn a_single_larger_coin_is_split_into_the_payment_and_its_change() {
	let db = wallet();
	let eight = coin(&db, 3);
	let selection = select_on(&db, request(Want::Amount(5)), &chain_params()).unwrap();
	assert_eq!(selection, spend(vec![eight], vec![(eight, exponents(&[0, 0, 1, 2]))], &[2, 0]));
}

#[test]
fn coins_that_fit_are_spent_whole_and_one_coin_is_split_for_the_rest() {
	let db = wallet();
	let eight = coin(&db, 3);
	let four = coin(&db, 2);
	coin(&db, 2);
	let selection = select_on(&db, request(Want::Amount(9)), &chain_params()).unwrap();
	assert_eq!(selection, spend(vec![four, eight], vec![(four, exponents(&[0, 0, 1]))], &[3, 0]));
}

#[test]
fn a_split_coin_delivers_the_amount_and_keeps_the_rest_as_change() {
	let db = wallet();
	let four = coin(&db, 2);
	let selection = select_on(&db, request(Want::Amount(3)), &chain_params()).unwrap();
	assert_eq!(selection, spend(vec![four], vec![(four, exponents(&[0, 0, 1]))], &[1, 0]), "3 delivered, 1 kept");
}

#[test]
fn coins_are_spent_whole_and_one_entry_is_unloaded_for_the_rest() {
	let db = wallet();
	let four = coin(&db, 2);
	let first = entry(&db, 1, 0);
	entry(&db, 1, 1);
	let selection = select_on(&db, request(Want::Amount(5)), &chain_params()).unwrap();
	assert_eq!(selection, spend(vec![four, first], Vec::new(), &[2, 0]), "one unload, one token, change of 1");
}

#[test]
fn one_entry_of_a_shared_ring_is_unloaded_for_the_rest() {
	let db = wallet();
	let four = coin(&db, 2);
	let first = entry(&db, 1, 0);
	entry(&db, 1, 0);
	let selection = select_on(&db, request(Want::Amount(5)), &chain_params()).unwrap();
	assert_eq!(selection, spend(vec![four, first], Vec::new(), &[2, 0]));
}

#[test]
fn the_first_entry_is_unloaded_when_either_alone_covers_the_rest() {
	let db = wallet();
	let four = coin(&db, 2);
	let first = entry(&db, 1, 1);
	entry(&db, 1, 0);
	let selection = select_on(&db, request(Want::Amount(5)), &chain_params()).unwrap();
	assert_eq!(selection.inputs, [four, first], "the tie is broken by the asset, not by the ring");
}

#[test]
fn a_coin_past_the_age_cutoff_is_ignored_and_the_rest_comes_from_an_entry() {
	let db = wallet();
	hold(&db, AssetKind::Coin, 2, aged_coin(2, 14));
	let two = coin(&db, 1);
	let four = entry(&db, 2, 0);
	let selection = select_on(&db, request(Want::Amount(6)), &chain_params()).unwrap();
	assert_eq!(selection, spend(vec![two, four], Vec::new(), &[1, 2]));
}

#[test]
fn a_coin_past_the_age_cutoff_does_not_count_towards_the_funds() {
	let db = wallet();
	hold(&db, AssetKind::Coin, 2, aged_coin(2, 14));
	coin(&db, 1);
	assert_eq!(select_on(&db, request(Want::Amount(6)), &chain_params()), Err(SelectError::InsufficientFunds { available: 2 }));
}

#[test]
fn only_as_many_entries_of_a_ring_are_unloaded_as_cover_the_amount() {
	let db = wallet();
	let first = entry(&db, 1, 0);
	entry(&db, 1, 0);
	let selection = select_on(&db, request(Want::Amount(1)), &chain_params()).unwrap();
	assert_eq!(selection, spend(vec![first], Vec::new(), &[0]));
}

#[test]
fn the_largest_entries_are_unloaded_first() {
	let db = wallet();
	entry(&db, 1, 0);
	let four = entry(&db, 2, 1);
	let eight = entry(&db, 3, 2);
	let selection = select_on(&db, request(Want::Amount(9)), &chain_params()).unwrap();
	assert_eq!(selection, spend(vec![eight, four], Vec::new(), &[3, 0]));
}

#[test]
fn an_amount_far_above_the_purse_fails_as_insufficient_before_any_strategy() {
	let db = wallet();
	coin(&db, 2);
	assert_eq!(
		select_on(&db, request(Want::Amount(u64::MAX)), &chain_params()),
		Err(SelectError::InsufficientFunds { available: 4 })
	);
}

#[test]
fn a_plain_spend_takes_only_what_is_spendable_outright() {
	let policy = PrivacyPolicy { recycle_at: RecycleAge::Age(5), max_unavailable_share: 100, ..PrivacyPolicy::fastest() };
	let wallet = TestWallet::new(policy);
	let spendable = wallet.hold(PurseId::MAIN, AssetKind::Coin, 0, aged_coin(0, 0));
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 1, aged_coin(1, 5));
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 2, aged_coin(2, 14));
	let select_now = |want| select(&wallet.view(), request(want), NOW, &chain_params());
	assert_eq!(select_now(Want::Amount(3)), Err(SelectError::InsufficientFunds { available: 1 }));
	assert_eq!(select_now(Want::Amount(1)).unwrap().inputs, [spendable]);
}

#[test]
fn a_coin_past_the_age_cutoff_is_never_selectable_even_with_held_back_value() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, aged_coin(3, 14));
	for held_back in [false, true] {
		let request = SelectRequest { held_back, ..request(Want::Amount(8)) };
		assert_eq!(
			select(&wallet.view(), request, NOW, &chain_params()),
			Err(SelectError::InsufficientFunds { available: 0 }),
			"held back: {held_back}"
		);
	}
}

#[test]
fn held_back_value_adds_entries_still_gaining_privacy() {
	let wallet = TestWallet::new(PrivacyPolicy::balanced());
	let ready = wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, in_ring(0, 767));
	let gaining = wallet.hold(PurseId::MAIN, AssetKind::Entry, 2, in_ring(1, 20));
	let plain = request(Want::Amount(12));
	assert_eq!(select(&wallet.view(), plain, NOW, &chain_params()), Err(SelectError::InsufficientFunds { available: 8 }));
	let held_back = SelectRequest { held_back: true, ..plain };
	assert_eq!(select(&wallet.view(), held_back, NOW, &chain_params()).unwrap().inputs, [ready, gaining]);
}

#[test]
fn entries_already_private_are_unloaded_before_larger_ones_still_gaining_privacy() {
	let wallet = TestWallet::new(PrivacyPolicy::balanced());
	wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, in_ring(1, 20));
	let ready = wallet.hold(PurseId::MAIN, AssetKind::Entry, 2, in_ring(0, 767));
	let held_back = SelectRequest { held_back: true, ..request(Want::Amount(4)) };
	assert_eq!(select(&wallet.view(), held_back, NOW, &chain_params()).unwrap().inputs, [ready]);
}
