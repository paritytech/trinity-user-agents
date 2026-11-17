use super::*;
use crate::coinage::{
	PrivacyPolicy,
	selection::{Forms, SelectRequest, Want, select},
	testing::{NOW, TestWallet, chain_params, planning::params_with_unload_cap},
};

fn spend_of(wallet: &TestWallet, amount: u64, to: Destination, forms: Forms) -> PlanInput {
	let request =
		SelectRequest { purse: PurseId::MAIN, want: Want::Amount(amount), forms, entries_first: false, held_back: false };
	let selection = select(&wallet.view(), request, NOW, &chain_params()).expect("covered");
	PlanInput::Spend { selection, to }
}

fn value_of(wallet: &TestWallet, assets: &[Asset]) -> u64 {
	let view = wallet.view();
	assets.iter().filter_map(|asset| view.asset(asset)).map(|record| record.exponent.value()).sum()
}

fn outputs_value(planned: &Planned) -> u64 {
	planned.outputs.iter().map(|output| output.exponent.value()).sum()
}

fn delivered(plan: &Plan) -> u64 {
	plan.transactions
		.iter()
		.flat_map(|planned| &planned.outputs)
		.filter(|output| output.delivered)
		.map(|output| output.exponent.value())
		.sum()
}

#[test]
fn every_split_and_transfer_conserves_value_and_delivers_the_amount() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.coin(5);
	wallet.coin(2);
	let input = spend_of(&wallet, 27, Destination::Purse(PurseId(4)), Forms::Any);
	let plan = plan(&input, &[], &wallet.view(), &chain_params());
	for planned in &plan.transactions {
		assert_eq!(outputs_value(planned), value_of(&wallet, &planned.inputs), "{planned:?}");
		assert!(planned.depends_on.is_empty(), "within one operation there are no dependencies");
	}
	assert_eq!(delivered(&plan), 27);
	let delivered_purses: Vec<PurseId> = plan
		.transactions
		.iter()
		.flat_map(|planned| &planned.outputs)
		.map(|output| if output.delivered { output.purse } else { PurseId(99) })
		.filter(|purse| *purse != PurseId(99))
		.collect();
	assert!(delivered_purses.iter().all(|purse| *purse == PurseId(4)));
}

#[test]
fn an_export_hands_out_coins_already_in_shape_without_a_transaction() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let four = wallet.coin(2);
	wallet.coin(3);
	let input = spend_of(&wallet, 4, Destination::Export(PurseId::MAIN), Forms::Any);
	let plan = plan(&input, &[], &wallet.view(), &chain_params());
	assert_eq!(plan.handed_out, [four]);
	assert!(plan.transactions.is_empty());
}

#[test]
fn each_unload_group_presents_its_own_token_and_conserves_value() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.entry(4, 0);
	wallet.entry(4, 1);
	let input = spend_of(&wallet, 20, Destination::External([7; 32]), Forms::EntriesOnly);
	let tokens = [TokenId { period: 1, slot: 0 }, TokenId { period: 1, slot: 1 }];
	assert_eq!(tokens_needed(&input, &wallet.view(), &chain_params()), 2, "one per ring");
	let plan = plan(&input, &tokens, &wallet.view(), &chain_params());
	let presented: Vec<_> = plan.transactions.iter().map(|planned| planned.token).collect();
	assert_eq!(presented, [Some(tokens[0]), Some(tokens[1])]);
	let change: u64 = plan.transactions.iter().map(outputs_value).sum();
	assert_eq!(change, 32 - 20, "the surplus is reloaded as entries");
	for planned in &plan.transactions {
		assert_eq!(planned.call, Call::UnloadToExternal([7; 32]));
		assert!(planned.outputs.iter().all(|output| output.kind == AssetKind::Entry && !output.delivered));
	}
}

#[test]
fn an_offboard_reloads_its_surplus_as_entries_of_the_source_purse() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let eight = wallet.entry(3, 0);
	wallet.entry(1, 1);
	let input = spend_of(&wallet, 6, Destination::External([7; 32]), Forms::EntriesOnly);
	let plan = plan(&input, &[TokenId { period: 1, slot: 0 }], &wallet.view(), &chain_params());
	let change = Output { kind: AssetKind::Entry, purse: PurseId::MAIN, exponent: Exponent::new(1).unwrap(), delivered: false };
	assert_eq!(plan.transactions.len(), 1);
	assert_eq!((plan.transactions[0].inputs.clone(), plan.transactions[0].outputs.clone()), (vec![eight], vec![change]));
}

#[test]
fn an_offboard_with_no_surplus_mints_nothing() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.entry(3, 0);
	wallet.entry(3, 0);
	wallet.entry(2, 1);
	let input = spend_of(&wallet, 20, Destination::External([7; 32]), Forms::EntriesOnly);
	let tokens = [TokenId { period: 1, slot: 0 }, TokenId { period: 1, slot: 1 }];
	let plan = plan(&input, &tokens, &wallet.view(), &chain_params());
	let inputs_per_ring: Vec<usize> = plan.transactions.iter().map(|planned| planned.inputs.len()).collect();
	assert_eq!(inputs_per_ring, [1, 2], "one transaction per ring, each with its ring's entries");
	assert!(plan.transactions.iter().all(|planned| planned.outputs.is_empty()));
}

#[test]
fn a_load_is_cut_at_the_per_load_cap() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let input = PlanInput::Load { from: [3; 32], amount: (1 << 12) - 1, into: PurseId::MAIN };
	let plan = plan(&input, &[], &wallet.view(), &chain_params());
	let sizes: Vec<usize> = plan.transactions.iter().map(|planned| planned.outputs.len()).collect();
	assert_eq!(sizes, [10, 2], "twelve entries, ten per load");
	assert_eq!(plan.transactions.iter().map(outputs_value).sum::<u64>(), (1 << 12) - 1);
}

#[test]
fn a_recycle_turns_each_coin_into_an_entry_of_its_value() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let coin = wallet.coin(6);
	let plan = plan(&PlanInput::Recycle { coins: vec![coin], into: PurseId(2) }, &[], &wallet.view(), &chain_params());
	assert_eq!(plan.transactions.len(), 1);
	let planned = &plan.transactions[0];
	assert_eq!((planned.call, planned.inputs.clone()), (Call::Recycle, vec![coin]));
	assert_eq!(planned.outputs, [Output { kind: AssetKind::Entry, purse: PurseId(2), exponent: Exponent::new(6).unwrap(), delivered: true }]);
}

/// The plan of a payment of `amount` from entries to another purse.
fn unload_plan(wallet: &TestWallet, amount: u64, params: &ChainParams) -> Plan {
	let request = SelectRequest {
		purse: PurseId::MAIN,
		want: Want::Amount(amount),
		forms: Forms::EntriesOnly,
		entries_first: false,
		held_back: false,
	};
	let selection = select(&wallet.view(), request, NOW, params).expect("covered");
	plan(&PlanInput::Spend { selection, to: Destination::Purse(PurseId(4)) }, &[], &wallet.view(), params)
}

/// The delivered and the change exponents of each unload, in plan order.
fn unload_outputs(wallet: &TestWallet, amount: u64, params: &ChainParams) -> Vec<(Vec<u8>, Vec<u8>)> {
	let plan = unload_plan(wallet, amount, params);
	let raw = |planned: &Planned, delivered: bool| -> Vec<u8> {
		planned.outputs.iter().filter(|output| output.delivered == delivered).map(|output| output.exponent.get()).collect()
	};
	plan.transactions.iter().map(|planned| (raw(planned, true), raw(planned, false))).collect()
}

#[test]
fn a_payment_larger_than_any_unload_group_is_split_across_groups() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.entry(5, 0);
	wallet.entry(5, 1);
	assert_eq!(unload_outputs(&wallet, 64, &chain_params()), [(vec![5], vec![]), (vec![5], vec![])]);
}

#[test]
fn a_single_unload_group_delivers_the_fewest_coins_of_the_payment() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.entry(6, 0);
	assert_eq!(unload_outputs(&wallet, 48, &chain_params()), [(vec![5, 4], vec![4])]);
}

#[test]
fn a_payment_spills_into_a_second_group_whose_rest_is_change() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.entry(5, 0);
	wallet.entry(5, 1);
	assert_eq!(unload_outputs(&wallet, 40, &chain_params()), [(vec![5], vec![]), (vec![3], vec![4, 3])]);
}

#[test]
fn entries_of_one_ring_beyond_the_unload_cap_are_unloaded_in_several_groups() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	for _ in 0..3 {
		wallet.entry(5, 0);
	}
	let params = params_with_unload_cap(2);
	let sizes: Vec<usize> = unload_plan(&wallet, 96, &params).transactions.iter().map(|planned| planned.inputs.len()).collect();
	assert_eq!(sizes, [2, 1]);
	assert_eq!(unload_outputs(&wallet, 96, &params), [(vec![6], vec![]), (vec![5], vec![])]);
}
