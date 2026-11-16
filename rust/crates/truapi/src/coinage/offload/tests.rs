
use std::sync::Arc;

use futures::executor::block_on;
use parity_scale_codec::Encode;

use super::*;
use crate::{
	coinage::{
		Exponent, PrivacyPolicy,
		ledger::{AssetKind, EntryOnChain},
		operation::OpKind,
		policy::{MemberThreshold, ReadyRule},
		registration::{Admission, Rows},
		status::{Failure, Progress, Status, StatusItem, status},
		testing::{ENTROPY, NOW, TestWallet, chain_params, entry_read, register_rows, set_status, unbuilt_engine},
	},
	durable::{DurableTxEngine, DurableTxId},
};

fn offload(amount: u64) -> Offload {
	Offload { from: PurseId::MAIN, amount, to: [7; 32], initiator: Initiator::User, confirmed: false }
}

fn key() -> OpKey {
	OpKey { scope: b"p".to_vec(), id: [1; 32] }
}

fn phase_of(wallet: &TestWallet, amount: u64) -> Phase {
	plan(&wallet.view(), &key(), &offload(amount), NOW, &chain_params())
}

#[test]
fn usable_entries_that_cover_the_amount_are_offboarded() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let entry = wallet.entry(4, 0);
	let Phase::Offboard(selection) = phase_of(&wallet, 10) else { panic!("an offboard") };
	assert_eq!(selection.inputs, [entry]);
}

#[test]
fn usable_coins_are_recycled_to_cover_the_deficit() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let large = wallet.coin(4);
	wallet.coin(1);
	assert_eq!(phase_of(&wallet, 9), Phase::Recycle(vec![large]), "the largest coin covers it");
}

#[test]
fn value_still_arriving_is_waited_for() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let outputs = [Exponent::new(4).unwrap()];
	block_on(wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, AssetKind::Entry, &outputs))).unwrap();
	assert_eq!(phase_of(&wallet, 10), Phase::Wait(None));
}

#[test]
fn an_offload_nothing_covers_fails() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.coin(1);
	assert_eq!(phase_of(&wallet, 10), Phase::Failed);
}

#[test]
fn an_offloaded_amount_is_done() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	assert_eq!(phase_of(&wallet, 0), Phase::Done);
}

/// Entries ready once 20 members are included, or once ten minutes have
/// passed since their key was included in a ring of at least one member.
fn dwell_policy(held_back: HeldBackSpend) -> PrivacyPolicy {
	PrivacyPolicy {
		ready_when: vec![
			ReadyRule { members: MemberThreshold::Count(20), dwell: core::time::Duration::ZERO },
			ReadyRule { members: MemberThreshold::Count(1), dwell: core::time::Duration::from_secs(600) },
		],
		held_back_for_user: held_back,
		..PrivacyPolicy::fastest()
	}
}

/// An entry of the main purse in `ring`, whose root includes `included`
/// members, first seen included a little before [`NOW`].
fn entry_among(wallet: &TestWallet, exponent: u8, ring: u32, included: u32) -> Asset {
	let Read::Entry(Some(place)) = entry_read(ring) else { unreachable!("an entry read") };
	let read = Read::Entry(Some(EntryOnChain { included, members: included, ..place }));
	wallet.hold(PurseId::MAIN, AssetKind::Entry, exponent, read)
}

/// The offload's records, each with the call it was registered for.
fn records_of(wallet: &TestWallet) -> Vec<(DurableTxId, Call)> {
	wallet
		.view()
		.registrations()
		.of_operation(&key())
		.filter_map(|facts| {
			let params = crate::coinage::start::RecordParams::from_params(&facts.state.plan.as_ref()?.params)?;
			Some((facts.record, params.call))
		})
		.collect()
}

/// Begins the offload of `amount` and runs its first Plan, as `offload`
/// does.
fn begin_offload(wallet: &TestWallet, engine: &Arc<DurableTxEngine>, amount: u64) -> Phase {
	let arguments = (offload(amount), Retry::ExpiryOnly).encode();
	block_on(wallet.db.write(move |tx| operation::begin(tx, &key(), OpKind::Offload, &arguments))).unwrap().unwrap();
	let params = chain_params();
	let launcher = Launcher { engine, params: &params, entropy: &ENTROPY };
	block_on(advance(&launcher, &wallet.db, &key(), &offload(amount), Retry::ExpiryOnly, NOW)).unwrap()
}

/// Runs F2's step 3 once, returning the offloads it advanced.
fn resume(wallet: &TestWallet, engine: &Arc<DurableTxEngine>) -> Vec<OpKey> {
	let params = chain_params();
	let launcher = Launcher { engine, params: &params, entropy: &ENTROPY };
	block_on(resume_due(&launcher, &wallet.db, NOW)).unwrap()
}

fn offload_status(wallet: &TestWallet) -> StatusItem {
	status(&wallet.view(), &key(), NOW, &chain_params()).expect("the offload is recorded")
}

#[test]
fn private_entries_that_cover_the_amount_are_offboarded_without_the_others() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let eight = wallet.entry(3, 0);
	wallet.entry(1, 1);
	let Phase::Offboard(selection) = phase_of(&wallet, 6) else { panic!("an offboard") };
	assert_eq!(selection.inputs, [eight], "the largest entry covers it; the surplus comes back as change");
}

#[test]
fn entries_still_gaining_privacy_pay_before_any_coin_is_recycled() {
	for (held_back, expected_offboard) in [(HeldBackSpend::Always, true), (HeldBackSpend::Never, false)] {
		let wallet = TestWallet::new(dwell_policy(held_back));
		entry_among(&wallet, 2, 0, 20);
		let dwelling = entry_among(&wallet, 4, 1, 10);
		wallet.coin(3);
		let phase = phase_of(&wallet, 10);
		if expected_offboard {
			let Phase::Offboard(selection) = phase else { panic!("held-back entries are usable: {phase:?}") };
			assert_eq!(selection.inputs, [dwelling]);
		} else {
			assert_eq!(phase, Phase::Wait(Some(NOW - 100 + 600)), "the dwelling entry is waited for, no coin recycled");
		}
	}
}

#[test]
fn recyclable_coins_make_up_what_all_entries_lack() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.entry(1, 0);
	wallet.entry(2, 1);
	let eight = wallet.coin(3);
	wallet.coin(1);
	assert_eq!(phase_of(&wallet, 12), Phase::Recycle(vec![eight]), "the entries lack 6, which the coin of 8 covers");
}

#[test]
fn entries_another_operation_took_are_never_offboarded_for_less() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	wallet.entry(3, 0);
	let taken = wallet.entry(2, 1);
	let other = OpKey { scope: b"p".to_vec(), id: [2; 32] };
	let spend = vec![Rows { inputs: vec![taken], ..Rows::default() }];
	let record = register_rows(&engine, &other, Admission::Ordinary, spend).unwrap()[0];
	assert_eq!(phase_of(&wallet, 12), Phase::Wait(None), "the taken entry may come back; nothing less than 12 leaves");
	set_status(&wallet.db, record, "FINALIZED_SUCCESS", None);
	assert_eq!(phase_of(&wallet, 12), Phase::Failed, "once it is spent, what is left no longer covers the offload");
}

#[test]
fn an_entry_the_chain_shows_in_no_ring_is_never_offboarded() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	wallet.hold(PurseId::MAIN, AssetKind::Entry, 4, Read::Entry(None));
	assert_eq!(phase_of(&wallet, 10), Phase::Wait(None));
}

#[test]
fn coins_to_load_are_recycled_under_the_offloads_key_and_then_waited_on() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	let coin = wallet.coin(4);
	assert_eq!(begin_offload(&wallet, &engine, 10), Phase::Recycle(vec![coin]));
	let calls: Vec<Call> = records_of(&wallet).into_iter().map(|(_, call)| call).collect();
	assert_eq!(calls, [Call::Recycle]);
	assert!(resume(&wallet, &engine).is_empty(), "nothing is planned while the recycle is in flight");
}

#[test]
fn an_offboard_that_finalized_completes_the_offload() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	wallet.entry(4, 0);
	assert!(matches!(begin_offload(&wallet, &engine, 10), Phase::Offboard(_)));
	let records = records_of(&wallet);
	let calls: Vec<Call> = records.iter().map(|(_, call)| *call).collect();
	assert_eq!(calls, [Call::UnloadToExternal([7; 32])]);
	set_status(&wallet.db, records[0].0, "FINALIZED_SUCCESS", None);
	assert_eq!(resume(&wallet, &engine), [key()], "the next Plan finds the amount offboarded");
	assert_eq!(offload_status(&wallet), StatusItem { status: Status::Done, progress: Progress { settled: 10, in_flight: 0 } });
}

#[test]
fn a_recycle_that_failed_on_chain_ends_the_offload() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	wallet.coin(4);
	begin_offload(&wallet, &engine, 10);
	set_status(&wallet.db, records_of(&wallet)[0].0, "FAILURE", Some("DISPATCH_FAILED"));
	resume(&wallet, &engine);
	assert_eq!(records_of(&wallet).len(), 1, "the coin is not recycled again");
	assert_eq!(offload_status(&wallet).status, Status::Failed(Failure::GivenUp));
	assert!(resume(&wallet, &engine).is_empty(), "nothing runs after the stored end");
}

#[test]
fn an_offboard_that_partly_executed_reports_what_reached_the_destination() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	wallet.entry(3, 0);
	wallet.entry(3, 1);
	begin_offload(&wallet, &engine, 16);
	let records = records_of(&wallet);
	assert_eq!(records.len(), 2, "one unload per ring");
	set_status(&wallet.db, records[0].0, "FINALIZED_SUCCESS", None);
	set_status(&wallet.db, records[1].0, "FAILURE", Some("DISPATCH_FAILED"));
	resume(&wallet, &engine);
	assert_eq!(records_of(&wallet).len(), 2, "no further token is spent on the rest");
	let partial = StatusItem { status: Status::PartiallyDone, progress: Progress { settled: 8, in_flight: 0 } };
	assert_eq!(offload_status(&wallet), partial);
}

#[test]
fn an_offboard_where_nothing_executed_fails_the_offload() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	wallet.entry(3, 0);
	wallet.entry(3, 1);
	begin_offload(&wallet, &engine, 16);
	for (record, _) in records_of(&wallet) {
		set_status(&wallet.db, record, "FAILURE", Some("DISPATCH_FAILED"));
	}
	resume(&wallet, &engine);
	assert_eq!(records_of(&wallet).len(), 2);
	let failed = StatusItem { status: Status::Failed(Failure::GivenUp), progress: Progress::default() };
	assert_eq!(offload_status(&wallet), failed);
}

#[test]
fn an_offboard_reports_live_records_as_in_flight_and_no_records_as_waiting() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	wallet.entry(3, 0);
	wallet.entry(3, 1);
	let arguments = (offload(16), Retry::ExpiryOnly).encode();
	block_on(wallet.db.write(move |tx| operation::begin(tx, &key(), OpKind::Offload, &arguments))).unwrap().unwrap();
	assert_eq!(offload_status(&wallet).status, Status::Waiting, "no records yet is not an end");
	let params = chain_params();
	let launcher = Launcher { engine: &engine, params: &params, entropy: &ENTROPY };
	block_on(advance(&launcher, &wallet.db, &key(), &offload(16), Retry::ExpiryOnly, NOW)).unwrap();
	let records = records_of(&wallet);
	set_status(&wallet.db, records[0].0, "PENDING_SUCCESS", None);
	set_status(&wallet.db, records[1].0, "FINALIZED_SUCCESS", None);
	let settling = StatusItem { status: Status::Settling, progress: Progress { settled: 8, in_flight: 8 } };
	assert_eq!(offload_status(&wallet), settling);
}

#[test]
fn the_value_offboarded_is_the_inputs_less_their_reloaded_change() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let engine = unbuilt_engine(wallet.db.clone());
	wallet.entry(3, 0);
	wallet.entry(2, 1);
	begin_offload(&wallet, &engine, 10);
	let records = records_of(&wallet);
	assert_eq!(records.len(), 2);
	for (record, _) in &records {
		set_status(&wallet.db, *record, "FINALIZED_SUCCESS", None);
	}
	assert_eq!(offboarded(&wallet.view(), &key()), 10, "8 and 4 unloaded, 2 reloaded as change");
}

#[test]
fn only_entries_selectable_now_spare_the_confirmation() {
	let unconfirmed = |amount| Offload { amount, ..offload(0) };
	let params = chain_params();
	let wallet = TestWallet::new(dwell_policy(HeldBackSpend::AfterConfirmation));
	wallet.coin(3);
	entry_among(&wallet, 3, 1, 10);
	let view = wallet.view();
	assert!(needs_confirmation(&view, &unconfirmed(8), NOW, &params), "a coin and a dwelling entry need the user's word");
	let confirmed = Offload { confirmed: true, ..unconfirmed(8) };
	assert!(!needs_confirmation(&view, &confirmed, NOW, &params));

	entry_among(&wallet, 3, 0, 20);
	assert!(!needs_confirmation(&wallet.view(), &unconfirmed(8), NOW, &params), "an entry selectable now covers it");

	let always = TestWallet::new(dwell_policy(HeldBackSpend::Always));
	always.coin(3);
	assert!(!needs_confirmation(&always.view(), &unconfirmed(8), NOW, &params), "only AfterConfirmation asks");
}
