use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		PrivacyPolicy, PurseId,
		classification::{CoinClass, Class, classify},
		ledger::{AssetKind, Mark},
		testing::{
			NOW, TestWallet, chain_params, unbuilt_engine,
			planning::{aged_coin, in_ring},
		},
	},
	durable::DurableTxStatus,
};

const ENTROPY: [u8; 16] = [3; 16];

fn key(id: u8) -> OpKey {
	OpKey { scope: b"product".to_vec(), id: [id; 32] }
}

fn started(wallet: &TestWallet, key: &OpKey, request: Request) -> Result<Registered, StartError> {
	let engine = unbuilt_engine(wallet.db.clone());
	let params = chain_params();
	let launcher = Launcher { engine: &engine, params: &params, entropy: &ENTROPY };
	block_on(start(&launcher, key, Retry::ExpiryOnly, request, NOW))
}

fn pay(amount: u64, to: Destination) -> Request {
	Request::Select { from: PurseId::MAIN, amount, to, initiator: Some(Initiator::User), confirmed: false }
}

#[test]
fn a_spend_registers_its_transactions_holding_their_inputs() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let coin = wallet.coin(5);
	let registered = started(&wallet, &key(1), pay(27, Destination::Purse(PurseId(4)))).unwrap();
	assert_eq!(registered.records.len(), 1, "one split of the 32 cent coin");
	let view = wallet.view();
	let record = view.registrations().record(registered.records[0]).unwrap();
	assert_eq!(record.rows.inputs, [coin]);
	assert_eq!(record.state.status, DurableTxStatus::PendingSubmission);
	let params = RecordParams::from_params(&record.state.plan.clone().unwrap().params).unwrap();
	assert_eq!(params, RecordParams { call: Call::Split, retry: Retry::ExpiryOnly });
	assert!(matches!(classify(&coin, &view, NOW, &chain_params()).class, Class::Coin(CoinClass::InUse(_))));
	let delivered: u64 = record.rows.outputs.iter().filter(|output| output.purse() == Some(PurseId(4))).map(|output| view.asset(output).unwrap().exponent.value()).sum();
	assert_eq!(delivered, 27);
}

#[test]
fn an_export_hands_out_its_coins_with_their_secrets_and_marks_them() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let four = wallet.coin(2);
	let two = wallet.coin(1);
	wallet.coin(4);
	let registered = started(&wallet, &key(1), pay(6, Destination::Export(PurseId::MAIN))).unwrap();
	let total: u64 = registered.exported.iter().map(|coin| coin.exponent.value()).sum();
	assert_eq!(total, 6);
	assert!(registered.records.is_empty(), "coins already in shape need no transaction");
	let view = wallet.view();
	assert_eq!((view.mark(&four), view.mark(&two)), (Some(Mark::Uncommitted), Some(Mark::Uncommitted)));
	for coin in &registered.exported {
		let secret = schnorrkel::SecretKey::from_bytes(&coin.secret.0).unwrap();
		assert_eq!(secret.to_public().to_bytes(), coin.account);
	}
}

#[test]
fn a_start_that_cannot_be_covered_writes_nothing() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.coin(1);
	let refused = started(&wallet, &key(1), pay(3, Destination::Purse(PurseId(4))));
	assert!(matches!(refused, Err(StartError::InsufficientFunds { available: 2 })));
	assert_eq!(wallet.view().registrations().records().count(), 0);
}

#[test]
fn an_input_a_caller_chose_is_refused_once_another_registration_took_it() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let coin = wallet.coin(3);
	let recycle = || Request::Given(PlanInput::Recycle { coins: vec![coin], into: PurseId::MAIN });
	assert!(started(&wallet, &key(1), recycle()).is_ok());
	assert!(matches!(started(&wallet, &key(2), recycle()), Err(StartError::Refused(RegisterError::InputTaken(taken))) if taken == coin));
}

#[test]
fn held_back_value_needs_the_users_confirmation_under_balanced() {
	let policy = PrivacyPolicy { recycle_at: crate::coinage::RecycleAge::Age(0), max_unavailable_share: 100, ..PrivacyPolicy::balanced() };
	let wallet = TestWallet::new(policy);
	wallet.coin(3);
	let view = wallet.view();
	let request = pay(8, Destination::Purse(PurseId(4)));
	assert!(needs_confirmation(&view, &request, NOW, &chain_params()), "the only coin is due for recycling");
	let confirmed = Request::Select { from: PurseId::MAIN, amount: 8, to: Destination::Purse(PurseId(4)), initiator: Some(Initiator::User), confirmed: true };
	assert!(!needs_confirmation(&view, &confirmed, NOW, &chain_params()));
	assert!(started(&wallet, &key(1), confirmed).is_ok(), "confirmed, it spends the held-back coin");
}

fn confirmed_user_payment(amount: u64) -> Request {
	Request::Select { from: PurseId::MAIN, amount, to: Destination::Purse(PurseId(4)), initiator: Some(Initiator::User), confirmed: true }
}

#[test]
fn most_private_keeps_coins_due_for_recycling_from_the_user_even_when_confirmed() {
	let wallet = TestWallet::new(PrivacyPolicy::most_private());
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, aged_coin(3, 5));
	let refused = started(&wallet, &key(1), confirmed_user_payment(8));
	assert!(matches!(refused, Err(StartError::InsufficientFunds { available: 0 })), "{refused:?}");
}

#[test]
fn most_private_keeps_entries_gaining_privacy_from_the_user_even_when_confirmed() {
	let wallet = TestWallet::new(PrivacyPolicy::most_private());
	wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, in_ring(0, 20));
	let refused = started(&wallet, &key(1), confirmed_user_payment(8));
	assert!(matches!(refused, Err(StartError::InsufficientFunds { available: 0 })), "{refused:?}");
}

/// The value and disclosure mark of every output the start registered,
/// smallest first.
fn marked_outputs(wallet: &TestWallet, registered: &Registered) -> Vec<(u64, Option<Mark>)> {
	let view = wallet.view();
	let mut outputs: Vec<(u64, Option<Mark>)> = registered
		.records
		.iter()
		.filter_map(|record| view.registrations().record(*record))
		.flat_map(|record| record.rows.outputs.clone())
		.map(|output| (view.asset(&output).unwrap().exponent.value(), view.mark(&output)))
		.collect();
	outputs.sort_by_key(|(value, mark)| (*value, mark.is_some()));
	outputs
}

#[test]
fn an_export_marks_the_delivered_outputs_of_its_split_and_keeps_the_change() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let eight = wallet.coin(3);
	let registered = started(&wallet, &key(1), pay(5, Destination::Export(PurseId::MAIN))).unwrap();
	let uncommitted = Some(Mark::Uncommitted);
	assert_eq!(marked_outputs(&wallet, &registered), [(1, None), (1, uncommitted), (2, None), (4, uncommitted)]);
	assert_eq!(wallet.view().mark(&eight), None, "the split coin is consumed, not handed out");
	assert_eq!(registered.exported.iter().map(|coin| coin.exponent.value()).sum::<u64>(), 5);
}

#[test]
fn an_export_marks_the_delivered_outputs_of_its_unload_and_keeps_the_change() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.entry(3, 0);
	let registered = started(&wallet, &key(1), pay(5, Destination::Export(PurseId::MAIN))).unwrap();
	let uncommitted = Some(Mark::Uncommitted);
	assert_eq!(marked_outputs(&wallet, &registered), [(1, None), (1, uncommitted), (2, None), (4, uncommitted)]);
	assert_eq!(registered.exported.iter().map(|coin| coin.exponent.value()).sum::<u64>(), 5);
}

#[test]
fn record_params_carry_their_layout_version() {
	let params = RecordParams { call: Call::Transfer, retry: Retry::Until(9) }.to_params();
	assert_eq!(params.0[0], PARAMS_VERSION);
	assert_eq!(RecordParams::from_params(&params), Some(RecordParams { call: Call::Transfer, retry: Retry::Until(9) }));
	let mut unknown = params.clone();
	unknown.0[0] = PARAMS_VERSION + 1;
	assert_eq!(RecordParams::from_params(&unknown), None);
}

mod use_cases;
