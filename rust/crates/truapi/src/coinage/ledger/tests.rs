use futures::executor::block_on;

use super::*;
use crate::store::{Db, open_memory_core_db};

const INSTALLATION: InstallationId = InstallationId([9; 32]);

fn exponent(raw: u8) -> Exponent {
	Exponent::new(raw).unwrap()
}

fn block(number: u32) -> BlockRef {
	BlockRef { number, hash: [number as u8; 32] }
}

/// A database with a minted installation and the balanced policy.
fn ledger() -> Db {
	let db = open_memory_core_db();
	block_on(db.write(|tx| {
		mint_installation(tx, INSTALLATION, &PrivacyPolicy::balanced(), None, 0)?.unwrap();
		Ok(())
	}))
	.unwrap();
	db
}

fn view(db: &Db) -> Snapshot {
	block_on(db.read(snapshot)).unwrap()
}

#[test]
fn allocation_never_reissues_an_index() {
	let db = ledger();
	let outputs = [exponent(3), exponent(1)];
	let first = block_on(db.write(move |tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &outputs))).unwrap();
	let second = block_on(db.write(move |tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &outputs))).unwrap();
	let entries = block_on(db.write(move |tx| allocate(tx, PurseId::MAIN, AssetKind::Entry, &outputs))).unwrap();
	let indices = |assets: &[Asset]| -> Vec<u32> {
		assets
			.iter()
			.map(|asset| match asset {
				Asset::Coin { index, .. } | Asset::Entry { index, .. } => *index,
				Asset::PeerCoin(_) => unreachable!(),
			})
			.collect()
	};
	assert_eq!(indices(&first), [0, 1]);
	assert_eq!(indices(&second), [2, 3]);
	assert_eq!(indices(&entries), [0, 1], "coins and entries are separate trees");
	assert!(first.iter().all(|asset| asset.installation() == Some(INSTALLATION)));
	let snapshot = view(&db);
	assert_eq!(snapshot.asset(&second[0]).map(|record| record.exponent), Some(exponent(3)));
	assert_eq!(snapshot.assets(PurseId::MAIN).count(), 6);
}

#[test]
fn product_purses_count_up_and_skip_identifiers_already_known() {
	let db = ledger();
	let first = block_on(db.write(|tx| create_purse(tx, "a".into(), vec![1], 10))).unwrap();
	let second = block_on(db.write(|tx| create_purse(tx, "b".into(), vec![2], 11))).unwrap();
	assert_eq!((first, second), (PurseId(0), PurseId(1)));
	let found = AssetRecord {
		asset: Asset::Coin { purse: PurseId(7), installation: InstallationId([1; 32]), index: 0 },
		exponent: exponent(0),
	};
	block_on(db.write(move |tx| adopt(tx, &[found]))).unwrap();
	let third = block_on(db.write(|tx| create_purse(tx, "c".into(), Vec::new(), 12))).unwrap();
	assert_eq!(third, PurseId(8));
	let record = view(&db).purse(PurseId(1)).cloned().unwrap();
	assert_eq!(record, PurseRecord { id: PurseId(1), name: "b".into(), created: 11, tag: vec![2], state: PurseState::Open });
}

#[test]
fn a_purse_state_only_moves_forward_and_resolves_to_an_open_purse() {
	let db = ledger();
	let a = block_on(db.write(|tx| create_purse(tx, "a".into(), Vec::new(), 0))).unwrap();
	let b = block_on(db.write(|tx| create_purse(tx, "b".into(), Vec::new(), 0))).unwrap();
	let delete = |purse, into| block_on(db.write(move |tx| begin_delete(tx, purse, into))).unwrap();
	assert_eq!(delete(PurseId::MAIN, a), Err(LedgerError::MainPurse));
	assert_eq!(delete(a, a), Err(LedgerError::PurseNotFound));
	assert_eq!(delete(a, PurseId(99)), Err(LedgerError::PurseNotFound));
	assert_eq!(delete(a, b), Ok(()));
	assert_eq!(delete(a, b), Err(LedgerError::DeletionOpen));
	assert_eq!(delete(b, a), Err(LedgerError::PurseNotFound), "a deleting purse is no target");
	block_on(db.write(move |tx| close_purse(tx, a))).unwrap();
	assert_eq!(delete(a, b), Err(LedgerError::PurseNotFound), "a closed purse never reopens");
	assert_eq!(delete(b, PurseId::MAIN), Ok(()));
	let snapshot = view(&db);
	assert_eq!(snapshot.purse(a).map(|purse| purse.state), Some(PurseState::Closed { drained_into: b }));
	assert_eq!(snapshot.resolve_purse(a), PurseId::MAIN);
	assert_eq!(snapshot.resolve_purse(PurseId::MAIN), PurseId::MAIN);
}

#[test]
fn a_failed_or_older_read_never_overwrites_a_stored_value() {
	let db = ledger();
	let coin = block_on(db.write(|tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &[exponent(2)])))
		.unwrap()[0];
	let present = Read::Coin { now: Some(CoinOnChain { exponent: exponent(2), age: 1, locked_until: None }), emptied_by: None };
	let empty = Read::Coin { now: None, emptied_by: None };
	let report = |at: u32, read: Read| {
		let facts = Facts { at: block(at), reads: vec![(Subject::Asset(coin), read)] };
		block_on(db.write(move |tx| observe(tx, &facts, 100, 255))).unwrap();
		view(&db).observation(&Subject::Asset(coin)).cloned().unwrap()
	};
	let seen = report(10, present.clone());
	assert_eq!((seen.observed, seen.value.clone()), (Observed::At(block(10)), Some(present.clone())));
	let failed = report(11, Read::Failed);
	assert_eq!((failed.observed, failed.value), (Observed::FailedSince(block(10)), Some(present.clone())));
	let older = report(9, empty.clone());
	assert_eq!((older.observed, older.value), (Observed::FailedSince(block(10)), Some(present)));
	let newer = report(12, empty.clone());
	assert_eq!((newer.observed, newer.value), (Observed::At(block(12)), Some(empty)));
}

#[test]
fn a_failed_first_read_leaves_an_asset_never_observed() {
	let db = ledger();
	let coin = Asset::PeerCoin([4; 32]);
	let facts = Facts { at: block(5), reads: vec![(Subject::Asset(coin), Read::Failed)] };
	block_on(db.write(move |tx| observe(tx, &facts, 0, 255))).unwrap();
	let stored = view(&db).observation(&Subject::Asset(coin)).cloned().unwrap();
	assert_eq!(stored, Observation { observed: Observed::Never, value: None, included_at: None });
}

#[test]
fn an_entry_records_when_its_key_was_first_seen_included() {
	let db = ledger();
	let entry = block_on(db.write(|tx| allocate(tx, PurseId::MAIN, AssetKind::Entry, &[exponent(4)])))
		.unwrap()[0];
	let in_ring = |included| EntryOnChain {
		ring: 0,
		revision: 1,
		page: 1,
		position: 3,
		members: 300,
		included,
		closed: false,
		expires_at: None,
		locked_until: None,
	};
	let report = |at: u32, now: u64, included: u32| {
		let facts = Facts { at: block(at), reads: vec![(Subject::Asset(entry), Read::Entry(Some(in_ring(included))))] };
		block_on(db.write(move |tx| observe(tx, &facts, now, 255))).unwrap();
		view(&db).observation(&Subject::Asset(entry)).and_then(|observation| observation.included_at)
	};
	assert_eq!(report(1, 100, 4), None, "position 3 of page 1 is place 258, not yet included");
	assert_eq!(report(2, 200, 259), Some(200));
	assert_eq!(report(3, 300, 300), Some(200), "the first inclusion instant stays");
}

#[test]
fn a_committed_mark_is_never_released() {
	let db = ledger();
	let coins = block_on(db.write(|tx| {
		allocate(tx, PurseId::MAIN, AssetKind::Coin, &[exponent(0), exponent(1)])
	}))
	.unwrap();
	let [first, second] = [coins[0], coins[1]];
	assert_eq!(block_on(db.write(move |tx| mark_disclosed(tx, &[first, second]))).unwrap(), Ok(()));
	assert_eq!(block_on(db.write(move |tx| mark_disclosed(tx, &[second]))).unwrap(), Err(second));
	block_on(db.write(move |tx| commit_disclosed(tx, &[first]))).unwrap();
	assert_eq!(block_on(db.read(uncommitted)).unwrap(), [second]);
	block_on(db.write(move |tx| release_disclosed(tx, &[first, second]))).unwrap();
	let snapshot = view(&db);
	assert_eq!((snapshot.mark(&first), snapshot.mark(&second)), (Some(Mark::Committed), None));
}

#[test]
fn a_mark_is_kept_per_asset() {
	let db = ledger();
	let coins = block_on(db.write(|tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &[exponent(0), exponent(1)]))).unwrap();
	let marked = coins[0];
	block_on(db.write(move |tx| mark_disclosed(tx, &[marked]))).unwrap().unwrap();
	let snapshot = view(&db);
	assert_eq!((snapshot.mark(&coins[0]), snapshot.mark(&coins[1])), (Some(Mark::Uncommitted), None));
}

#[test]
fn an_asset_the_ledger_never_held_has_no_record() {
	let db = ledger();
	let unknown = Asset::Coin { purse: PurseId::MAIN, installation: INSTALLATION, index: 99 };
	assert_eq!(view(&db).asset(&unknown), None);
}

/// Adoption records what an earlier instance allocated; the current
/// installation's counter starts from its own allocations alone.
#[test]
fn allocation_counts_per_installation_and_adoption_never_advances_it() {
	let db = ledger();
	let earlier = InstallationId([1; 32]);
	let found = AssetRecord { asset: Asset::Coin { purse: PurseId::MAIN, installation: earlier, index: 40 }, exponent: exponent(0) };
	block_on(db.write(move |tx| adopt(tx, &[found]))).unwrap();
	let allocated = block_on(db.write(|tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &[exponent(0)]))).unwrap();
	assert_eq!(allocated, [Asset::Coin { purse: PurseId::MAIN, installation: INSTALLATION, index: 0 }]);
}

#[test]
fn an_allocation_never_overwrites_an_asset_already_held() {
	let db = ledger();
	let held = Asset::Coin { purse: PurseId::MAIN, installation: INSTALLATION, index: 0 };
	let found = AssetRecord { asset: held, exponent: exponent(5) };
	block_on(db.write(move |tx| adopt(tx, &[found]))).unwrap();
	let refused = block_on(db.write(|tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &[exponent(0)])));
	assert!(refused.is_err(), "index 0 is taken");
	assert_eq!(view(&db).asset(&held), Some(&found));
}

#[test]
fn earlier_installations_count_toward_the_main_purse() {
	let db = ledger();
	let earlier = InstallationId([1; 32]);
	let found = AssetRecord {
		asset: Asset::Entry { purse: PurseId(3), installation: earlier, index: 5 },
		exponent: exponent(2),
	};
	block_on(db.write(move |tx| {
		add_installations(tx, &[earlier])?;
		adopt(tx, &[found])
	}))
	.unwrap();
	let current = block_on(db.write(|tx| allocate(tx, PurseId(3), AssetKind::Coin, &[exponent(0)])))
		.unwrap()[0];
	let snapshot = view(&db);
	let assets = |purse| snapshot.assets(purse).map(|record| record.asset).collect::<Vec<_>>();
	assert_eq!(assets(PurseId::MAIN), [found.asset]);
	assert_eq!(assets(PurseId(3)), [current]);
	let known: Vec<_> = snapshot.installations().iter().map(|(id, _)| *id).collect();
	assert_eq!(known, [INSTALLATION, earlier]);
	assert_eq!(snapshot.current_installation(), Some(INSTALLATION));
}

#[test]
fn the_policy_and_the_purse_backup_round_trip() {
	let db = ledger();
	assert_eq!(view(&db).policy(), &PrivacyPolicy::balanced());
	block_on(db.write(|tx| set_policy(tx, &PrivacyPolicy::most_private()))).unwrap();
	assert_eq!(view(&db).policy(), &PrivacyPolicy::most_private());
	block_on(db.write(|tx| create_purse(tx, "tickets".into(), b"product".to_vec(), 7).map(|_| ()))).unwrap();
	let backup = block_on(db.read(purse_backup)).unwrap();
	let restored = open_memory_core_db();
	block_on(restored.write(move |tx| {
		mint_installation(tx, InstallationId([2; 32]), &PrivacyPolicy::fastest(), Some(&backup), 0)?.unwrap();
		Ok(())
	}))
	.unwrap();
	let purse = view(&restored).purse(PurseId(0)).cloned().unwrap();
	assert_eq!((purse.name.as_str(), purse.tag.as_slice()), ("tickets", b"product".as_slice()));
	let next = block_on(restored.write(|tx| create_purse(tx, "next".into(), Vec::new(), 8))).unwrap();
	assert_eq!(next, PurseId(1), "allocation continues past every restored identifier");
	let refused = block_on(open_memory_core_db().write(|tx| {
		mint_installation(tx, InstallationId([3; 32]), &PrivacyPolicy::fastest(), Some(&[0xff][..]), 0)
	}))
	.unwrap();
	assert_eq!(refused, Err(LedgerError::UndecodableStore));
}

#[test]
fn operation_ids_count_up() {
	let db = ledger();
	let first = block_on(db.write(next_operation_id)).unwrap();
	let second = block_on(db.write(next_operation_id)).unwrap();
	assert_ne!(first, second);
	assert_eq!(second[..8], 1u64.to_le_bytes());
}

#[test]
fn peer_secrets_are_kept_until_forgotten() {
	let db = ledger();
	let secret = super::super::CoinSecret([5; 64]);
	block_on(db.write(move |tx| keep_secrets(tx, &[([1; 32], secret)]))).unwrap();
	assert_eq!(block_on(db.read(|conn| super::secret(conn, &[1; 32]))).unwrap(), Some(super::super::CoinSecret([5; 64])));
	block_on(db.write(|tx| forget_secrets(tx, &[[1; 32]]))).unwrap();
	assert_eq!(block_on(db.read(|conn| super::secret(conn, &[1; 32]))).unwrap(), None);
}

fn found_on_chain(index: u32) -> AssetRecord {
	AssetRecord { asset: Asset::Coin { purse: PurseId::MAIN, installation: INSTALLATION, index }, exponent: exponent(5) }
}

#[test]
fn an_allocation_whose_index_is_taken_fails_instead_of_overwriting_the_row() {
	let db = ledger();
	let taken = found_on_chain(0);
	block_on(db.write(move |tx| adopt(tx, &[taken]))).unwrap();
	let outputs = [exponent(2)];
	assert!(block_on(db.write(move |tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &outputs))).is_err());
	let assets: Vec<AssetRecord> = view(&db).assets(PurseId::MAIN).copied().collect();
	assert_eq!(assets, [taken], "the record found on chain keeps its exponent");
}

#[test]
fn a_batch_whose_indices_are_taken_fails_instead_of_overwriting_the_rows() {
	let db = ledger();
	let taken = found_on_chain(1);
	block_on(db.write(move |tx| adopt(tx, &[taken]))).unwrap();
	let outputs = [exponent(2), exponent(3)];
	assert!(block_on(db.write(move |tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &outputs))).is_err());
	let assets: Vec<AssetRecord> = view(&db).assets(PurseId::MAIN).copied().collect();
	assert_eq!(assets, [taken], "the batch's free first index is rolled back with the rest");
}

#[test]
fn an_allocation_without_a_current_installation_fails() {
	let db = open_memory_core_db();
	for kind in [AssetKind::Coin, AssetKind::Entry] {
		let outputs = [exponent(2)];
		assert!(block_on(db.write(move |tx| allocate(tx, PurseId::MAIN, kind, &outputs))).is_err(), "{kind:?}");
	}
}

/// A released export frees its coin: the mark is gone, so the coin can be
/// disclosed again.
#[test]
fn a_released_mark_can_be_marked_again() {
	let db = ledger();
	let coin = block_on(db.write(|tx| allocate(tx, PurseId::MAIN, AssetKind::Coin, &[exponent(0)]))).unwrap()[0];
	block_on(db.write(move |tx| mark_disclosed(tx, &[coin]))).unwrap().unwrap();
	block_on(db.write(move |tx| release_disclosed(tx, &[coin]))).unwrap();
	assert_eq!(block_on(db.write(move |tx| mark_disclosed(tx, &[coin]))).unwrap(), Ok(()));
	assert_eq!(view(&db).mark(&coin), Some(Mark::Uncommitted));
}
