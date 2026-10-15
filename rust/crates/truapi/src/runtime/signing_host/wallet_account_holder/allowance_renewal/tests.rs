use super::*;
use crate::{host_logic::product_account::derive_sr25519_hard_path, test_support::StubPlatform};
use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

use truapi::ProductId;

fn wallet_for(storage: Arc<dyn CoreStorage>, entropy: u8) -> WalletAccountHolder {
	let platform = StubPlatform { core_storage_override: Some(storage), ..Default::default() };
	super::super::tests::unlocked_wallet(platform, "paseo", entropy)
}

/// A pass that began under a wallet since replaced must neither prune nor track
/// under that wallet, whether it was replaced while the pass waited for the
/// ledger or after it read it: either would drop entries the new wallet
/// promised.
#[test]
fn stale_renewal_cannot_prune_the_replacement_wallets_targets() {
	use futures::FutureExt;

	for waiting_on_lock in [true, false] {
		let storage = Arc::new(YieldingStorage::default());
		let wallet = wallet_for(storage.clone(), 8);
		let target = StatementRenewalTarget::Account {
			account_id: [9; 32],
			label: "wallet B device".to_string(),
		};
		futures::executor::block_on(wallet.track_statement_renewal_targets(vec![target.clone()]))
			.unwrap();
		let owner = wallet.current_session().unwrap().public_key;
		wallet.install(wallet.prepare_activation(vec![7; 32], None).unwrap());
		let selected = wallet.current_session().unwrap();
		let guard = waiting_on_lock
			.then(|| futures::executor::block_on(wallet.renewal.ledger_lock().lock()));
		let renewal = wallet.renew_statement_allowances();
		futures::pin_mut!(renewal);
		assert!(renewal.as_mut().now_or_never().is_none());
		wallet.install(wallet.prepare_activation(vec![8; 32], None).unwrap());
		drop(guard);
		if waiting_on_lock {
			futures::executor::block_on(
				wallet.track_statement_renewal_targets(vec![target.clone()]),
			)
			.unwrap();
		}
		let result = futures::executor::block_on(renewal).map(|_| ());
		let tracked = futures::executor::block_on(wallet.track_statement_renewal_targets_for(
			&selected,
			vec![StatementRenewalTarget::Account {
				account_id: [8; 32],
				label: "wallet A device".to_string(),
			}],
		));
		assert_eq!(
			(
				result.map_err(|error| error.to_string()),
				tracked.map_err(|error| error.to_string()),
				futures::executor::block_on(read_entries(storage.as_ref())).unwrap()
			),
			(
				Err("Disconnected".to_string()),
				Err("Disconnected".to_string()),
				vec![TrackedStatementRenewalTarget { target, owner: Some(owner) }]
			),
		);
	}
}

#[derive(Default)]
struct MemStorage {
	inner: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
	writes: Mutex<usize>,
}

impl MemStorage {
	fn writes(&self) -> usize {
		*self.writes.lock().expect("write counter mutex poisoned")
	}
}

#[async_trait::async_trait]
impl CoreStorage for MemStorage {
	async fn read_core_storage(
		&self,
		key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		Ok(self.inner.lock().expect("storage mutex poisoned").get(&key.encode()).cloned())
	}

	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError> {
		*self.writes.lock().expect("write counter mutex poisoned") += 1;
		self.inner.lock().expect("storage mutex poisoned").insert(key.encode(), value);
		Ok(())
	}

	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
		self.inner.lock().expect("storage mutex poisoned").remove(&key.encode());
		Ok(())
	}
}

/// Yield once, so a concurrently polled task can run.
async fn yield_once() {
	let mut yielded = false;
	futures::future::poll_fn(move |cx| {
		if yielded {
			core::task::Poll::Ready(())
		} else {
			yielded = true;
			cx.waker().wake_by_ref();
			core::task::Poll::Pending
		}
	})
	.await
}

/// Storage that yields *after* serving a read, so a second reader observes
/// the same value before the first writes its update back. Without
/// something serializing the cycle, one update is lost.
#[derive(Default)]
struct YieldingStorage(MemStorage);

#[async_trait::async_trait]
impl CoreStorage for YieldingStorage {
	async fn read_core_storage(
		&self,
		key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		let value = self.0.read_core_storage(key).await;
		yield_once().await;
		value
	}

	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError> {
		self.0.write_core_storage(key, value).await
	}

	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
		self.0.clear_core_storage(key).await
	}
}

#[test]
fn concurrent_tracks_do_not_drop_an_entry() {
	let storage = Arc::new(YieldingStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;

	futures::executor::block_on(async {
		let (first, second) = futures::join!(
			wallet.track_statement_renewal_targets(vec![product("a.dot")]),
			wallet.track_statement_renewal_targets(vec![product("b.dot")]),
		);
		first.unwrap();
		second.unwrap();

		let mut targets = read_targets(storage.as_ref(), owner).await.unwrap();
		targets.sort_by_key(|target| format!("{target:?}"));
		assert_eq!(targets, vec![product("a.dot"), product("b.dot")]);
	});
}

/// The in-process loop returns nothing, so a host reads what a pass achieved
/// from the recorded report. Recording only in the direct path would leave a
/// loop-driven host unable to see exhaustion at all.
#[test]
fn the_last_report_is_readable_once_a_pass_has_run() {
	let state = RenewalState::default();
	assert!(
		state.last_report().is_none(),
		"no pass has run, which is not the same as a healthy one"
	);

	absorb_tick(
		&state,
		Ok(StatementRenewalReport {
			period: 7,
			outcomes: Vec::new(),
			pruned: Vec::new(),
			slots_exhausted: true,
		}),
	);

	let seen = state.last_report().expect("a pass has run");
	assert_eq!(seen.period, 7);
	assert!(seen.slots_exhausted, "exhaustion is the outcome a host most needs to read back");
}

/// Only the newest matters: a host reads this on resume and wants the state
/// now, not the first exhaustion it ever hit.
#[test]
fn the_last_report_keeps_the_newest_pass() {
	let state = RenewalState::default();
	for period in [7, 8] {
		absorb_tick(
			&state,
			Ok(StatementRenewalReport {
				period,
				outcomes: Vec::new(),
				pruned: Vec::new(),
				slots_exhausted: period == 7,
			}),
		);
	}

	let seen = state.last_report().expect("a pass has run");
	assert_eq!(seen.period, 8);
	assert!(!seen.slots_exhausted, "the older exhaustion should not stick");
}

/// A tick that could not run at all must not erase the last pass a host has
/// not read yet.
#[test]
fn a_failed_tick_leaves_the_previous_report_readable() {
	let state = RenewalState::default();
	absorb_tick(
		&state,
		Ok(StatementRenewalReport {
			period: 7,
			outcomes: Vec::new(),
			pruned: Vec::new(),
			slots_exhausted: true,
		}),
	);

	absorb_tick(&state, Err(AuthorityError::Disconnected.into()));

	let seen = state.last_report().expect("the earlier pass is still there");
	assert_eq!(seen.period, 7);
	assert!(seen.slots_exhausted);
}

/// Pruning rewrites the whole ledger, so it has to hold the lock across its
/// read too. Reading outside it lets a tracking call land in the gap and be
/// overwritten by a view that predates it, leaving that account tracked
/// nowhere and never renewed again.
#[test]
fn a_prune_does_not_overwrite_a_concurrent_track() {
	let storage = Arc::new(YieldingStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;
	let other_wallet = wallet_for(storage.clone(), 8);
	let device =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device".to_string() };

	futures::executor::block_on(async {
		// A foreign entry, so the pass prunes and therefore writes.
		other_wallet.track_statement_renewal_targets(vec![device]).await.unwrap();

		let session = wallet.current_session().unwrap();
		let (pruned, tracked) = futures::join!(
			wallet.owned_targets(&session),
			wallet.track_statement_renewal_targets(vec![product("a.dot")]),
		);
		pruned.unwrap();
		tracked.unwrap();

		assert_eq!(
			read_targets(storage.as_ref(), owner).await.unwrap(),
			vec![product("a.dot")],
			"the concurrently tracked target was overwritten by the prune"
		);
	});
}

/// A raw account promised by a previous identity must not be renewed under a
/// later one: it would spend that identity's slots on an account it never
/// promised.
#[test]
fn a_foreign_target_is_dropped_from_the_ledger() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;
	let other_wallet = wallet_for(storage.clone(), 8);
	let device =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device".to_string() };

	futures::executor::block_on(async {
		other_wallet.track_statement_renewal_targets(vec![device]).await.unwrap();
		wallet.track_statement_renewal_targets(vec![product("a.dot")]).await.unwrap();

		let (targets, pruned) =
			wallet.owned_targets(&wallet.current_session().unwrap()).await.unwrap();

		assert_eq!(targets, vec![product("a.dot")]);
		// Reported, not just dropped: the pass is a host's only view of the
		// ledger, so a silent prune is one it cannot notice or re-track.
		assert_eq!(pruned, vec!["device".to_string()]);
		// Dropped, not merely skipped, so the cost is paid once.
		assert_eq!(
			read_entries(storage.as_ref()).await.unwrap(),
			vec![TrackedStatementRenewalTarget::new(product("a.dot"), owner)]
		);
	});
}

#[test]
fn an_all_foreign_ledger_prunes_to_nothing() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let other_wallet = wallet_for(storage.clone(), 8);
	let device =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device".to_string() };

	futures::executor::block_on(async {
		other_wallet.track_statement_renewal_targets(vec![device]).await.unwrap();

		assert_eq!(
			wallet.owned_targets(&wallet.current_session().unwrap()).await.unwrap(),
			(Vec::new(), vec!["device".to_string()])
		);
		assert_eq!(read_entries(storage.as_ref()).await.unwrap(), Vec::new());
	});
}

#[test]
fn an_all_owned_ledger_is_not_rewritten() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);

	futures::executor::block_on(async {
		wallet.track_statement_renewal_targets(vec![product("a.dot")]).await.unwrap();
		let after_seeding = storage.writes();

		let (targets, _pruned) =
			wallet.owned_targets(&wallet.current_session().unwrap()).await.unwrap();

		assert_eq!(targets, vec![product("a.dot")]);
		// Every tick calls this; rewriting the ledger each time would be waste.
		assert_eq!(storage.writes(), after_seeding);
	});
}

/// The targets in the ledger visible to the identity rooted at `owner`.
async fn read_targets(
	storage: &(impl CoreStorage + ?Sized),
	owner: [u8; 32],
) -> Result<Vec<StatementRenewalTarget>, StatementRenewalError> {
	Ok(read_entries(storage)
		.await?
		.into_iter()
		.filter(|entry| entry.is_owned_by(owner))
		.map(|entry| entry.target)
		.collect())
}

fn product(product_id: &str) -> StatementRenewalTarget {
	StatementRenewalTarget::ProductStatementAllowance {
		product_id: ProductId::for_test(product_id),
	}
}

#[test]
fn ledger_round_trips_dedupes_and_preserves_order() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;

	futures::executor::block_on(async {
		wallet
			.track_statement_renewal_targets(vec![
				StatementRenewalTarget::WalletSso,
				product("a.dot"),
			])
			.await
			.unwrap();
		wallet
			.track_statement_renewal_targets(vec![
				product("a.dot"),
				StatementRenewalTarget::Account {
					account_id: [9; 32],
					label: "device".to_string(),
				},
			])
			.await
			.unwrap();

		assert_eq!(
			read_targets(storage.as_ref(), owner).await.unwrap(),
			vec![
				StatementRenewalTarget::WalletSso,
				product("a.dot"),
				StatementRenewalTarget::Account {
					account_id: [9; 32],
					label: "device".to_string(),
				},
			]
		);
	});
}

#[test]
fn the_ledger_lists_back_every_entry_in_track_order() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;
	let device =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device".to_string() };

	futures::executor::block_on(async {
		wallet
			.track_statement_renewal_targets(vec![
				StatementRenewalTarget::WalletSso,
				device.clone(),
			])
			.await
			.unwrap();

		assert_eq!(
			wallet.statement_renewal_targets().await.unwrap(),
			vec![
				TrackedStatementRenewalTarget {
					target: StatementRenewalTarget::WalletSso,
					owner: None,
				},
				TrackedStatementRenewalTarget { target: device, owner: Some(owner) },
			]
		);
	});
}

// The point of the reader is auditing finite slots, so it reports what is
// stored rather than what the active identity would resolve: an entry another
// identity promised is still occupying a row until a pass prunes it, and the
// owner is what lets a caller tell the two apart.
#[test]
fn a_listing_includes_a_foreign_entry_and_writes_nothing() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let other_wallet = wallet_for(storage.clone(), 8);
	let other_owner = other_wallet.current_session().unwrap().public_key;
	let device =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device".to_string() };

	futures::executor::block_on(async {
		other_wallet
			.track_statement_renewal_targets(vec![device.clone()])
			.await
			.unwrap();
		wallet.track_statement_renewal_targets(vec![product("a.dot")]).await.unwrap();
		let writes_before = storage.writes();

		let listed = wallet.statement_renewal_targets().await.unwrap();

		assert_eq!(
			listed,
			vec![
				TrackedStatementRenewalTarget { target: device, owner: Some(other_owner) },
				TrackedStatementRenewalTarget { target: product("a.dot"), owner: None },
			]
		);
		// A read that rewrote the ledger could not be run without a session.
		assert_eq!(storage.writes(), writes_before);
	});
}

// The reader is the one ledger operation whose whole job is to run while a
// pass may be writing: a scheduled host wakes, lists, and decides. `track`
// is polled first so it holds the lock across its read, which is what makes
// the guarantee observable; without the lock the listing reads the ledger
// the track is in the middle of replacing.
#[test]
fn a_listing_during_a_track_reports_the_settled_ledger() {
	let storage = Arc::new(YieldingStorage::default());
	let wallet = wallet_for(storage.clone(), 7);

	futures::executor::block_on(async {
		let (tracked, listed) = futures::join!(
			wallet.track_statement_renewal_targets(vec![product("a.dot")]),
			wallet.statement_renewal_targets(),
		);
		tracked.unwrap();

		assert_eq!(
			listed.unwrap(),
			vec![TrackedStatementRenewalTarget { target: product("a.dot"), owner: None }],
			"the listing observed the ledger the track was replacing"
		);
	});
}

#[test]
fn an_undecodable_ledger_lists_as_empty() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);

	futures::executor::block_on(async {
		storage
			.write_core_storage(CoreStorageKey::StatementRenewalTargets, vec![0xff; 8])
			.await
			.unwrap();

		assert_eq!(wallet.statement_renewal_targets().await.unwrap(), Vec::new());
	});
}

#[test]
fn ledger_rejects_trailing_bytes() {
	let mut blob = vec![TrackedStatementRenewalTarget::new(product("a.dot"), [1; 32])].encode();
	blob.push(0xff);
	assert!(decode_entries(&blob).is_err());
}

#[test]
fn an_undecodable_ledger_reads_as_empty() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;

	futures::executor::block_on(async {
		storage
			.write_core_storage(CoreStorageKey::StatementRenewalTargets, vec![0xff, 0xff, 0xff])
			.await
			.unwrap();

		assert_eq!(read_targets(storage.as_ref(), owner).await.unwrap(), Vec::new());
	});
}

#[test]
fn tracking_over_an_undecodable_ledger_starts_a_fresh_one() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;

	futures::executor::block_on(async {
		storage
			.write_core_storage(CoreStorageKey::StatementRenewalTargets, vec![0xff; 3])
			.await
			.unwrap();
		wallet.track_statement_renewal_targets(vec![product("a.dot")]).await.unwrap();

		assert_eq!(read_targets(storage.as_ref(), owner).await.unwrap(), vec![product("a.dot")]);
	});
}

#[test]
fn product_target_resolves_to_allocation_derivation() {
	let entropy = [7u8; 32];
	let expected = derive_sr25519_hard_path(&entropy, &["allowance", "statement-store", "a.dot"])
		.unwrap()
		.public
		.to_bytes();

	let resolved =
		resolve_target(&WalletKeys::new(entropy.to_vec(), "paseo".to_string()), &product("a.dot"))
			.unwrap();
	assert_eq!(
		resolved,
		ResolvedRenewalTarget { label: "product:a.dot".to_string(), account_id: expected }
	);
}

#[test]
fn wallet_sso_target_resolves_to_the_responder_identity() {
	let entropy = [7u8; 32];
	let expected = crate::host_logic::product_account::derive_identity_keypair(&entropy, "paseo")
		.unwrap()
		.public
		.to_bytes();

	let resolved = resolve_target(
		&WalletKeys::new(entropy.to_vec(), "paseo".to_string()),
		&StatementRenewalTarget::WalletSso,
	)
	.unwrap();

	assert_eq!(
		resolved,
		ResolvedRenewalTarget { label: "wallet-sso".to_string(), account_id: expected }
	);
}

#[test]
fn untracking_one_device_preserves_wallet_sso_and_other_devices() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;
	let first =
		StatementRenewalTarget::Account { account_id: [8; 32], label: "device:08".to_string() };
	let second =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device:09".to_string() };

	futures::executor::block_on(async {
		wallet
			.track_statement_renewal_targets(vec![
				StatementRenewalTarget::WalletSso,
				first,
				second.clone(),
			])
			.await
			.unwrap();

		assert!(wallet.untrack_statement_renewal_account(&[8; 32]).await.unwrap());
		assert_eq!(
			read_targets(storage.as_ref(), owner).await.unwrap(),
			vec![StatementRenewalTarget::WalletSso, second]
		);
	});
}

#[test]
fn concurrent_track_and_untrack_preserve_an_unrelated_device() {
	let storage = Arc::new(YieldingStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;
	let first =
		StatementRenewalTarget::Account { account_id: [8; 32], label: "device:08".to_string() };
	let second =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device:09".to_string() };

	futures::executor::block_on(async {
		wallet.track_statement_renewal_targets(vec![first]).await.unwrap();
		let (removed, tracked) = futures::join!(
			wallet.untrack_statement_renewal_account(&[8; 32]),
			wallet.track_statement_renewal_targets(vec![second.clone()]),
		);
		assert!(removed.unwrap());
		tracked.unwrap();

		assert_eq!(read_targets(&storage.0, owner).await.unwrap(), vec![second]);
	});
}

#[test]
fn a_raw_account_is_hidden_from_another_identity() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;
	let other_owner = wallet_for(storage.clone(), 8).current_session().unwrap().public_key;
	let device =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device".to_string() };

	futures::executor::block_on(async {
		wallet
			.track_statement_renewal_targets(vec![device.clone(), product("a.dot")])
			.await
			.unwrap();

		// The recipe resolves under any identity; the raw account does not.
		assert_eq!(
			read_targets(storage.as_ref(), owner).await.unwrap(),
			vec![device, product("a.dot")]
		);
		assert_eq!(
			read_targets(storage.as_ref(), other_owner).await.unwrap(),
			vec![product("a.dot")]
		);
	});
}

#[test]
fn the_same_raw_account_can_be_promised_by_two_identities() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;
	let other_wallet = wallet_for(storage.clone(), 8);
	let other_owner = other_wallet.current_session().unwrap().public_key;
	let device =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device".to_string() };

	futures::executor::block_on(async {
		wallet.track_statement_renewal_targets(vec![device.clone()]).await.unwrap();
		other_wallet
			.track_statement_renewal_targets(vec![device.clone()])
			.await
			.unwrap();

		// Distinct owners, so each identity renews it for itself.
		assert_eq!(read_targets(storage.as_ref(), owner).await.unwrap(), vec![device.clone()]);
		assert_eq!(read_targets(storage.as_ref(), other_owner).await.unwrap(), vec![device]);
	});
}

#[test]
fn untracking_a_device_preserves_the_other_identitys_entry() {
	let storage = Arc::new(MemStorage::default());
	let wallet = wallet_for(storage.clone(), 7);
	let owner = wallet.current_session().unwrap().public_key;
	let other_wallet = wallet_for(storage.clone(), 8);
	let other_owner = other_wallet.current_session().unwrap().public_key;
	let device =
		StatementRenewalTarget::Account { account_id: [9; 32], label: "device".to_string() };

	futures::executor::block_on(async {
		wallet.track_statement_renewal_targets(vec![device.clone()]).await.unwrap();
		other_wallet
			.track_statement_renewal_targets(vec![device.clone()])
			.await
			.unwrap();

		assert!(wallet.untrack_statement_renewal_account(&[9; 32]).await.unwrap());
		assert_eq!(
			(
				read_targets(storage.as_ref(), owner).await.unwrap(),
				read_targets(storage.as_ref(), other_owner).await.unwrap(),
			),
			(Vec::new(), vec![device])
		);
	});
}

#[test]
fn owner_key_follows_the_root_entropy() {
	assert_ne!(
		WalletKeys::new(vec![7; 32], "paseo".to_string()).root_public_key().unwrap(),
		WalletKeys::new(vec![8; 32], "paseo".to_string()).root_public_key().unwrap()
	);
}
