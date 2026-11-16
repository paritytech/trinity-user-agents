use futures::executor::block_on;

use super::*;
use crate::{
	coinage::{
		Exponent,
		ledger::{AssetKind, BlockRef, Facts, TokenId, allocate, mark_disclosed, mint_installation, observe, snapshot},
		registration::{Admission, Rows},
		testing::{chain_params, register_rows, set_status, unbuilt_engine},
	},
	durable::DurableTxId,
	installation::InstallationId,
	store::{Db, open_memory_core_db},
};

const NOW: u64 = 1_000_000;
const DAY: u64 = 86_400;

struct Wallet {
	db: Db,
	operations: u8,
}

impl Wallet {
	fn new(policy: PrivacyPolicy) -> Self {
		let db = open_memory_core_db();
		block_on(db.write(move |tx| {
			mint_installation(tx, InstallationId([9; 32]), &policy, None, 0)?.unwrap();
			Ok(())
		}))
		.unwrap();
		Self { db, operations: 0 }
	}

	fn allocate(&self, kind: AssetKind, exponent: u8) -> Asset {
		let outputs = [Exponent::new(exponent).unwrap()];
		block_on(self.db.write(move |tx| allocate(tx, PurseId::MAIN, kind, &outputs))).unwrap()[0]
	}

	fn observe(&self, asset: Asset, read: Read, now: u64) {
		let facts = Facts { at: BlockRef { number: 1, hash: [1; 32] }, reads: vec![(Subject::Asset(asset), read)] };
		block_on(self.db.write(move |tx| observe(tx, &facts, now, 255))).unwrap();
	}

	fn coin(&self, exponent: u8, age: u32) -> Asset {
		let coin = self.allocate(AssetKind::Coin, exponent);
		self.observe(coin, present(exponent, age, None), NOW);
		coin
	}

	fn register(&mut self, rows: Rows, status: &'static str, failure: Option<&'static str>) -> DurableTxId {
		self.operations += 1;
		let operation = OpKey { scope: b"p".to_vec(), id: [self.operations; 32] };
		let engine = unbuilt_engine(self.db.clone());
		let record = register_rows(&engine, &operation, Admission::Ordinary, vec![rows]).unwrap()[0];
		set_status(&self.db, record, status, failure);
		record
	}

	fn state(&self, asset: &Asset) -> AssetState {
		classify(asset, &block_on(self.db.read(snapshot)).unwrap(), NOW, &chain_params())
	}
}

fn present(exponent: u8, age: u32, locked_until: Option<u64>) -> Read {
	Read::Coin {
		now: Some(CoinOnChain { exponent: Exponent::new(exponent).unwrap(), age, locked_until }),
		emptied_by: None,
	}
}

fn spend(input: Asset) -> Rows {
	Rows { inputs: vec![input], ..Rows::default() }
}

fn mint(output: Asset) -> Rows {
	Rows { outputs: vec![output], ..Rows::default() }
}

fn in_ring(included: u32, members: u32, closed: bool, expires_at: Option<u64>) -> Read {
	Read::Entry(Some(EntryOnChain {
		ring: 0,
		revision: 1,
		page: 0,
		position: 0,
		members,
		included,
		closed,
		expires_at,
		locked_until: None,
	}))
}

#[test]
fn a_coin_takes_the_first_class_line_that_applies() {
	let mut wallet = Wallet::new(PrivacyPolicy::fastest());
	let available = wallet.coin(2, 0);
	assert_eq!(wallet.state(&available).class, Class::Coin(CoinClass::Available));

	let held = wallet.coin(2, 0);
	wallet.register(spend(held), "PENDING", None);
	assert!(matches!(wallet.state(&held).class, Class::Coin(CoinClass::InUse(_))));

	let spent = wallet.coin(2, 0);
	wallet.register(spend(spent), "FINALIZED_SUCCESS", None);
	assert_eq!(wallet.state(&spent).class, Class::Coin(CoinClass::Spent));

	let freed = wallet.coin(2, 0);
	wallet.register(spend(freed), "FAILURE", Some("REJECTED"));
	assert_eq!(wallet.state(&freed).class, Class::Coin(CoinClass::Available), "a failed consumer frees it");

	let minting = wallet.allocate(AssetKind::Coin, 1);
	wallet.register(mint(minting), "PENDING_SUCCESS", None);
	assert_eq!(wallet.state(&minting).class, Class::Coin(CoinClass::Minting), "best-chain success settles nothing");

	let void = wallet.allocate(AssetKind::Coin, 1);
	wallet.register(mint(void), "FAILURE", Some("EXPIRED"));
	assert_eq!(wallet.state(&void).class, Class::Coin(CoinClass::Void));

	let exported = wallet.coin(3, 0);
	block_on(wallet.db.write(move |tx| mark_disclosed(tx, &[exported]))).unwrap().unwrap();
	let state = wallet.state(&exported);
	assert_eq!((state.class, state.selectability), (Class::Coin(CoinClass::Exported), Selectability::Never));
	assert_eq!(state.export, Some(ExportStatus::AwaitingPayee));
}

#[test]
fn a_coin_is_arriving_before_it_is_held_back() {
	let wallet = Wallet::new(PrivacyPolicy::fastest());
	let unseen = wallet.allocate(AssetKind::Coin, 0);
	assert_eq!(wallet.state(&unseen).selectability, Selectability::Arriving(Arriving::NeverObserved));
	assert_eq!(wallet.state(&unseen).observed, Observed::Never, "never read as zero");

	let locked = wallet.allocate(AssetKind::Coin, 0);
	wallet.observe(locked, present(0, 0, Some(NOW + 1)), NOW);
	assert_eq!(wallet.state(&locked).selectability, Selectability::Arriving(Arriving::Locked));
	let unlocked = wallet.allocate(AssetKind::Coin, 0);
	wallet.observe(unlocked, present(0, 0, Some(NOW)), NOW);
	assert_eq!(wallet.state(&unlocked).selectability, Selectability::Now);

	let old = wallet.coin(0, 14);
	assert_eq!(wallet.state(&old).selectability, Selectability::Arriving(Arriving::AgeCutoff));
	let young = wallet.coin(0, 13);
	assert_eq!(wallet.state(&young).selectability, Selectability::Now, "fastest recycles only at the cutoff");
}

#[test]
fn recycling_admits_the_oldest_coins_while_the_share_allows() {
	let policy = PrivacyPolicy { recycle_at: RecycleAge::Age(2), max_unavailable_share: 20, ..PrivacyPolicy::fastest() };
	let wallet = Wallet::new(policy);
	let young = wallet.coin(3, 1);
	let oldest = wallet.coin(1, 9);
	let older = wallet.coin(1, 5);
	let old = wallet.coin(1, 3);
	let view = block_on(wallet.db.read(snapshot)).unwrap();
	// Total 14 cents; the cap is 2.8: the first coin takes the unavailable value to 2, the second to 4.
	assert_eq!(recycle_due(&view, PurseId::MAIN, NOW, &chain_params()), [oldest, older]);
	assert_eq!(wallet.state(&oldest).selectability, Selectability::HeldBack(HeldBack::RecycleDue));
	assert_eq!(wallet.state(&old).selectability, Selectability::Now, "the cap holds it back from recycling");
	assert_eq!(wallet.state(&young).selectability, Selectability::Now);
}

#[test]
fn an_entry_walks_from_arriving_through_held_back_to_ready() {
	let wallet = Wallet::new(PrivacyPolicy::balanced());
	let entry = wallet.allocate(AssetKind::Entry, 4);
	// The key is first seen included at NOW.
	let at = |read: Read, now: u64| {
		wallet.observe(entry, read, NOW);
		classify(&entry, &block_on(wallet.db.read(snapshot)).unwrap(), now, &chain_params()).selectability
	};
	assert_eq!(wallet.state(&entry).selectability, Selectability::Arriving(Arriving::NeverObserved));
	assert_eq!(at(Read::Entry(None), NOW), Selectability::Arriving(Arriving::Minting));
	assert_eq!(at(in_ring(0, 1, false, None), NOW), Selectability::Arriving(Arriving::NotIncluded));
	let later = NOW + 61;
	assert_eq!(at(in_ring(10, 10, false, None), later), Selectability::HeldBack(HeldBack::RingBelowThreshold));
	assert_eq!(at(in_ring(40, 40, false, None), later), Selectability::HeldBack(HeldBack::Dwell));
	assert_eq!(at(in_ring(40, 40, false, None), NOW + 600), Selectability::Now, "32 members and ten minutes");
	assert_eq!(at(in_ring(154, 154, false, None), later), Selectability::Now, "20 percent of 767 members");
	let view = block_on(wallet.db.read(snapshot)).unwrap();
	let state = classify(&entry, &view, later, &chain_params());
	assert_eq!(state.class, Class::Entry { ring: RingAxis::Ready, local: EntryLocal::Available });
}

#[test]
fn an_entry_near_expiry_is_left_to_the_rescue_and_an_expired_one_is_gone() {
	let wallet = Wallet::new(PrivacyPolicy::fastest());
	let entry = wallet.allocate(AssetKind::Entry, 4);
	wallet.observe(entry, in_ring(5, 5, true, Some(NOW + DAY)), NOW - 10);
	assert_eq!(wallet.state(&entry).selectability, Selectability::Arriving(Arriving::RescueMargin));
	wallet.observe(entry, in_ring(5, 5, true, Some(NOW + 30 * DAY)), NOW);
	assert_eq!(wallet.state(&entry).selectability, Selectability::Now);
	wallet.observe(entry, Read::Entry(None), NOW);
	let state = wallet.state(&entry);
	assert_eq!(state.class, Class::Entry { ring: RingAxis::Expired, local: EntryLocal::Available });
	assert_eq!(state.selectability, Selectability::Never);
}

#[test]
fn a_ring_closed_below_every_rule_is_degraded_and_held_back() {
	let wallet = Wallet::new(PrivacyPolicy::balanced());
	let entry = wallet.allocate(AssetKind::Entry, 4);
	wallet.observe(entry, in_ring(12, 12, true, Some(NOW + 30 * DAY)), NOW - 1000);
	let state = wallet.state(&entry);
	assert_eq!(state.class, Class::Entry { ring: RingAxis::Degraded(12), local: EntryLocal::Available });
	assert_eq!(state.selectability, Selectability::HeldBack(HeldBack::RingBelowThreshold));
}

#[test]
fn an_exported_coin_reports_what_became_of_it() {
	let mut wallet = Wallet::new(PrivacyPolicy::fastest());
	let minting = wallet.allocate(AssetKind::Coin, 2);
	let record = wallet.register(mint(minting), "PENDING", None);
	block_on(wallet.db.write(move |tx| mark_disclosed(tx, &[minting]))).unwrap().unwrap();
	assert_eq!(wallet.state(&minting).export, Some(ExportStatus::NotYetMinted));
	set_status(&wallet.db, record, "FAILURE", Some("ABANDONED"));
	assert_eq!(wallet.state(&minting).export, Some(ExportStatus::NeverMinted));

	let claimed = wallet.coin(2, 0);
	block_on(wallet.db.write(move |tx| mark_disclosed(tx, &[claimed]))).unwrap().unwrap();
	wallet.observe(claimed, Read::Coin { now: None, emptied_by: Some([7; 32]) }, NOW);
	assert_eq!(wallet.state(&claimed).export, Some(ExportStatus::Claimed { stable: true, by: [7; 32] }));
}

#[test]
fn an_entry_in_use_or_consumed_is_never_selectable() {
	let mut wallet = Wallet::new(PrivacyPolicy::fastest());
	let entry = wallet.allocate(AssetKind::Entry, 4);
	wallet.observe(entry, in_ring(5, 5, false, None), NOW - 10);
	let record = wallet.register(spend(entry), "PENDING", None);
	let state = wallet.state(&entry);
	assert!(matches!(state.class, Class::Entry { local: EntryLocal::InUse(_), .. }));
	assert_eq!(state.selectability, Selectability::Never);
	set_status(&wallet.db, record, "FINALIZED_SUCCESS", None);
	assert!(matches!(wallet.state(&entry).class, Class::Entry { local: EntryLocal::Consumed, .. }));
}

impl Wallet {
	/// An entry whose ring's root includes `included` members, its own key
	/// first seen included at [`NOW`].
	fn entry_in_ring(&self, exponent: u8, included: u32) -> Asset {
		let entry = self.allocate(AssetKind::Entry, exponent);
		self.observe(entry, in_ring(included, included, false, None), NOW);
		entry
	}

	fn selectability_at(&self, asset: &Asset, now: u64) -> Selectability {
		classify(asset, &block_on(self.db.read(snapshot)).unwrap(), now, &chain_params()).selectability
	}

	fn due(&self, now: u64) -> Vec<Asset> {
		recycle_due(&block_on(self.db.read(snapshot)).unwrap(), PurseId::MAIN, now, &chain_params())
	}
}

#[test]
fn an_unreadable_token_allowance_does_not_stop_recycling() {
	let wallet = Wallet::new(PrivacyPolicy::most_private());
	let coin = wallet.coin(2, 5);
	let period = tokens::period_of(NOW, &chain_params());
	let reads = (0..20).map(|slot| (Subject::Token(TokenId { period, slot }), Read::Failed)).collect();
	let facts = Facts { at: BlockRef { number: 1, hash: [1; 32] }, reads };
	block_on(wallet.db.write(move |tx| observe(tx, &facts, NOW, 255))).unwrap();
	assert_eq!(wallet.state(&coin).selectability, Selectability::HeldBack(HeldBack::RecycleDue));
}

#[test]
fn the_age_cutoff_and_the_policy_give_different_verdicts() {
	let wallet = Wallet::new(PrivacyPolicy::most_private());
	let at_cutoff = wallet.coin(2, 14);
	let policy_due = wallet.coin(2, 5);
	assert_eq!(wallet.state(&at_cutoff).selectability, Selectability::Arriving(Arriving::AgeCutoff));
	assert_eq!(wallet.state(&policy_due).selectability, Selectability::HeldBack(HeldBack::RecycleDue));
}

#[test]
fn most_private_holds_back_every_settled_coin() {
	let wallet = Wallet::new(PrivacyPolicy::most_private());
	let young = wallet.coin(1, 1);
	let old = wallet.coin(1, 9);
	assert_eq!(wallet.due(NOW), [old, young]);
}

#[test]
fn balanced_leaves_a_coin_below_its_age_threshold_alone() {
	let wallet = Wallet::new(PrivacyPolicy::balanced());
	let coin = wallet.coin(3, 3);
	assert_eq!(wallet.state(&coin).selectability, Selectability::Now, "balanced recycles from age 14 / 3 = 4");
}

#[test]
fn balanced_holds_back_a_coin_at_its_age_threshold() {
	let wallet = Wallet::new(PrivacyPolicy::balanced());
	let coin = wallet.coin(3, 4);
	assert_eq!(wallet.state(&coin).selectability, Selectability::HeldBack(HeldBack::RecycleDue));
}

#[test]
fn a_coin_larger_than_the_whole_recycling_budget_is_still_admitted() {
	let wallet = Wallet::new(PrivacyPolicy::balanced());
	let coin = wallet.coin(3, 4);
	assert_eq!(wallet.due(NOW), [coin], "20 percent of 8 cents admits the whole coin");
}

#[test]
fn an_empty_purse_has_nothing_to_recycle() {
	let wallet = Wallet::new(PrivacyPolicy::most_private());
	assert_eq!(wallet.due(NOW), []);
}

#[test]
fn a_coin_never_read_is_never_held_back_under_any_preset() {
	for policy in [PrivacyPolicy::fastest(), PrivacyPolicy::balanced(), PrivacyPolicy::most_private()] {
		let wallet = Wallet::new(policy.clone());
		let unread = wallet.allocate(AssetKind::Coin, 2);
		assert_eq!(wallet.state(&unread).selectability, Selectability::Arriving(Arriving::NeverObserved), "{policy:?}");
		assert_eq!(wallet.due(NOW), [], "{policy:?}");
	}
}

#[test]
fn most_private_holds_an_entry_below_ninety_percent_back_until_its_dwell_passes() {
	let wallet = Wallet::new(PrivacyPolicy::most_private());
	let entry = wallet.entry_in_ring(4, 690);
	assert_eq!(wallet.selectability_at(&entry, NOW), Selectability::HeldBack(HeldBack::Dwell));
}

#[test]
fn most_private_releases_an_entry_at_ninety_percent_of_the_ring() {
	let wallet = Wallet::new(PrivacyPolicy::most_private());
	let below = wallet.entry_in_ring(4, 690);
	let at = wallet.entry_in_ring(4, 691);
	assert_eq!(
		(wallet.selectability_at(&below, NOW), wallet.selectability_at(&at, NOW)),
		(Selectability::HeldBack(HeldBack::Dwell), Selectability::Now)
	);
}

#[test]
fn balanced_releases_an_entry_at_twenty_percent_of_the_ring() {
	let wallet = Wallet::new(PrivacyPolicy::balanced());
	let below = wallet.entry_in_ring(4, 153);
	let at = wallet.entry_in_ring(4, 154);
	assert_eq!(
		(wallet.selectability_at(&below, NOW), wallet.selectability_at(&at, NOW)),
		(Selectability::HeldBack(HeldBack::Dwell), Selectability::Now)
	);
}

#[test]
fn timed_readiness_needs_thirty_two_members_and_ten_full_minutes() {
	for policy in [PrivacyPolicy::balanced(), PrivacyPolicy::most_private()] {
		let wallet = Wallet::new(policy.clone());
		let short = wallet.entry_in_ring(4, 31);
		let enough = wallet.entry_in_ring(4, 32);
		let verdicts = [
			wallet.selectability_at(&short, NOW + 600),
			wallet.selectability_at(&enough, NOW + 599),
			wallet.selectability_at(&enough, NOW + 600),
		];
		let expected = [
			Selectability::HeldBack(HeldBack::RingBelowThreshold),
			Selectability::HeldBack(HeldBack::Dwell),
			Selectability::Now,
		];
		assert_eq!(verdicts, expected, "{policy:?}");
	}
}

#[test]
fn a_full_ring_releases_an_entry_before_any_dwell() {
	let wallet = Wallet::new(PrivacyPolicy::most_private());
	let entry = wallet.entry_in_ring(4, chain_params().ring_size());
	assert_eq!(wallet.selectability_at(&entry, NOW), Selectability::Now);
}

#[test]
fn an_entry_finishing_its_dwell_frees_recycling_budget() {
	let wallet = Wallet::new(PrivacyPolicy::balanced());
	wallet.entry_in_ring(3, 40);
	let coin = wallet.coin(1, 5);
	assert_eq!(wallet.due(NOW), [], "8 of 10 cents are held back, above the 20 percent share");
	assert_eq!(wallet.due(NOW + 600), [coin], "the entry is ready, nothing else is unavailable");
}

#[test]
fn readiness_follows_the_clock_both_ways() {
	let wallet = Wallet::new(PrivacyPolicy::balanced());
	let entry = wallet.entry_in_ring(4, 40);
	let verdicts = [NOW, NOW + 600, NOW, NOW + 600].map(|now| wallet.selectability_at(&entry, now));
	let (held, ready) = (Selectability::HeldBack(HeldBack::Dwell), Selectability::Now);
	assert_eq!(verdicts, [held, ready, held, ready]);
}

#[test]
fn maturity_makes_an_entry_spendable_without_confirmation_under_both_presets() {
	for policy in [PrivacyPolicy::balanced(), PrivacyPolicy::most_private()] {
		let wallet = Wallet::new(policy.clone());
		let entry = wallet.entry_in_ring(4, 32);
		let verdicts = [wallet.selectability_at(&entry, NOW), wallet.selectability_at(&entry, NOW + 600)];
		assert_eq!(verdicts, [Selectability::HeldBack(HeldBack::Dwell), Selectability::Now], "{policy:?}");
	}
}
