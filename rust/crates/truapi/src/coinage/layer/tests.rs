use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;

use super::*;
use crate::coinage::{
	ledger::AssetKind,
	status::{Failure, Status},
	testing::{
		BLOCK, ENTROPY, NOW, SCAN, TestRuntime, TestWallet, chain_config, coin_read, layer_interfaces, set_status,
		started_layer,
		scans::{ListedRegistry, RecordingChain},
	},
};
use crate::durable::DurableTxId;

fn op(id: u8) -> Op {
	Op { key: OpKey { scope: b"product".to_vec(), id: [id; 32] }, retry: Retry::ExpiryOnly }
}

#[tokio::test]
async fn a_fresh_store_gets_an_installation_and_its_registration_is_started_layer() {
	let store = crate::store::open_memory_core_db();
	let (layer, running, _) = started_layer(store.clone()).await;
	assert!(running.state().is_some(), "the start sequence opens the running handle");
	let view = layer.view().await.unwrap();
	assert_eq!(view.current_installation(), Some(InstallationId([7; 32])));
	let registrations: Vec<_> =
		view.operations().filter(|operation| operation.kind == OpKind::RegisterInstallation).collect();
	assert_eq!(registrations.len(), 1);
	assert!(registrations[0].key.is_own());
	assert_eq!(layer.recoverable().await.unwrap(), Recoverability::Pending { since: NOW });

	drop(layer);
	let (layer, ..) = started_layer(store).await;
	let view = layer.view().await.unwrap();
	let again = view.operations().filter(|operation| operation.kind == OpKind::RegisterInstallation).count();
	assert_eq!(again, 1, "an open registration is not started twice");
}

#[tokio::test]
async fn an_unreadable_runtime_stops_the_layer_from_coming_up() {
	let runtime = Arc::new(TestRuntime { failing: AtomicBool::new(true), ..Default::default() });
	let running = Running::new();
	let store = crate::store::open_memory_core_db();
	let refused = start(layer_interfaces(store, runtime), chain_config(), SCAN, None, running.clone()).await;
	assert!(matches!(refused, Err(StartError::UnsupportedRuntime { .. })));
	assert!(running.state().is_none(), "nothing is built");
}

#[tokio::test]
async fn an_export_left_uncommitted_is_released_at_the_next_start() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let export = layer.export(op(1), PurseId::MAIN, 8, Initiator::User, false).await.unwrap();
	assert_eq!(export.started, Started::New);
	assert_eq!(export.coins.len(), 1);
	let joined = layer.export(op(1), PurseId::MAIN, 8, Initiator::User, false).await.unwrap();
	let coins = |export: &Export| -> Vec<_> { export.coins.iter().map(|coin| (coin.account, coin.exponent)).collect() };
	assert_eq!(joined.started, Started::Joined);
	assert_eq!(coins(&joined), coins(&export), "a joined export returns the same coins");

	drop(layer);
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	let view = layer.view().await.unwrap();
	assert_eq!(status(&view, &op(1).key, NOW, &layer.params()).unwrap().status, Status::Failed(Failure::NotCommitted));
	assert!(uncommitted_marks(&wallet.db).await.is_empty());
	assert!(matches!(layer.commit_export(op(1).key).await, Err(Error::ExportClosed(_))));
}

async fn uncommitted_marks(db: &Db) -> Vec<Asset> {
	db.read(ledger::uncommitted).await.unwrap()
}

#[tokio::test]
async fn a_committed_export_cannot_be_released_and_an_uncommitted_one_cannot_be_reclaimed() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 2, coin_read(2));
	layer.export(op(1), PurseId::MAIN, 8, Initiator::User, false).await.unwrap();
	layer.export(op(2), PurseId::MAIN, 4, Initiator::User, false).await.unwrap();

	assert!(matches!(layer.reclaim(op(3), op(1).key, PurseId::MAIN).await, Err(Error::ExportNotCommitted(_))));
	layer.commit_export(op(1).key).await.unwrap();
	layer.commit_export(op(1).key).await.expect("committing again succeeds");
	assert!(matches!(layer.release_export(op(1).key).await, Err(Error::ExportClosed(_))));

	layer.release_export(op(2).key).await.unwrap();
	assert!(matches!(layer.commit_export(op(2).key).await, Err(Error::ExportClosed(_))));
	assert!(matches!(layer.commit_export(op(9).key).await, Err(Error::UnknownOperation(_))));
}

#[tokio::test]
async fn the_empty_scope_is_the_layers_own() {
	let (layer, ..) = started_layer(crate::store::open_memory_core_db()).await;
	let own = Op { key: OpKey::own([1; 32]), retry: Retry::Never };
	let refused = layer.rebalance(own, PurseId::MAIN, PurseId::MAIN, 1).await;
	assert!(matches!(refused, Err(Error::ReservedScope)));
}

#[tokio::test]
async fn while_the_runtime_check_fails_no_operation_starts() {
	let (layer, running, runtime) = started_layer(crate::store::open_memory_core_db()).await;
	let purse = layer.create_purse("savings".into(), Vec::new()).await.unwrap();
	runtime.failing.store(true, Ordering::SeqCst);
	layer.revalidate(BLOCK).await;
	assert!(running.state().unwrap().check_failed, "prepare answers no record");
	let refused = layer.rebalance(op(1), PurseId::MAIN, purse, 1).await;
	assert!(matches!(refused, Err(Error::UnsupportedRuntime { .. })));
	assert!(layer.query_purse(purse).await.is_ok(), "reads keep the last parameters");

	runtime.failing.store(false, Ordering::SeqCst);
	layer.revalidate(BLOCK).await;
	let refused = layer.rebalance(op(1), PurseId::MAIN, purse, 1).await;
	assert!(matches!(refused, Err(Error::InsufficientFunds { .. })), "the check passes again");
}

#[tokio::test]
async fn a_status_stream_starts_with_the_current_item_and_closes_after_the_terminal_one() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	layer.export(op(1), PurseId::MAIN, 8, Initiator::User, false).await.unwrap();
	let mut items = layer.status(op(1).key).await.unwrap();
	assert_eq!(items.next().await.unwrap().status, Status::Preparing);
	layer.commit_export(op(1).key).await.unwrap();
	assert_eq!(items.next().await.unwrap().status, Status::Done);
	assert_eq!(items.next().await, None, "the stream closes after its terminal item");
}

#[tokio::test]
async fn the_event_stream_starts_resynced_and_reports_a_new_purse() {
	let (layer, ..) = started_layer(crate::store::open_memory_core_db()).await;
	let mut events = layer.events().await.unwrap();
	assert_eq!(events.next().await, Some(Event::Resynced));
	let purse = layer.create_purse("savings".into(), Vec::new()).await.unwrap();
	assert_eq!(events.next().await, Some(Event::PurseCreated(purse)));
	assert_eq!(events.next().await, Some(Event::PurseMetadataChanged));
}

#[tokio::test]
async fn deleting_an_empty_purse_closes_it_at_once() {
	let (layer, ..) = started_layer(crate::store::open_memory_core_db()).await;
	let purse = layer.create_purse("old".into(), Vec::new()).await.unwrap();
	assert!(matches!(layer.delete_purse(op(1), PurseId::MAIN, purse).await, Err(Error::MainPurse)));
	assert_eq!(layer.delete_purse(op(1), purse, PurseId::MAIN).await.unwrap(), Started::New);
	assert!(matches!(layer.query_purse(purse).await, Err(Error::PurseNotFound(_))));
	assert_eq!(layer.resolve_purse(purse).await.unwrap(), PurseId::MAIN);
	let view = layer.view().await.unwrap();
	assert_eq!(status(&view, &op(1).key, NOW, &layer.params()).unwrap().status, Status::Done);
}

#[tokio::test]
async fn an_offload_of_nothing_is_refused() {
	let (layer, ..) = started_layer(crate::store::open_memory_core_db()).await;
	let refused = layer.offload(op(1), PurseId::MAIN, 0, [7; 32], Initiator::User, false).await;
	assert!(matches!(refused, Err(Error::EmptyRequest)));
}

#[tokio::test]
async fn an_offload_begun_earlier_is_joined_whatever_its_inputs_look_like_now() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	wallet.entry(4, 0);
	let first = layer.offload(op(1), PurseId::MAIN, 10, [7; 32], Initiator::User, false).await.unwrap();
	assert_eq!(first, Started::New);
	let again = layer.offload(op(1), PurseId::MAIN, 10, [7; 32], Initiator::User, false).await.unwrap();
	assert_eq!(again, Started::Joined, "the entry its offboard holds does not stop the join");
	let view = layer.view().await.unwrap();
	assert_eq!(view.registrations().of_operation(&op(1).key).count(), 1, "nothing is offboarded twice");
}

#[tokio::test]
async fn an_offload_only_coins_and_held_back_entries_cover_asks_the_user_first() {
	let dwell = crate::coinage::policy::ReadyRule {
		members: crate::coinage::policy::MemberThreshold::Count(1),
		dwell: Duration::from_secs(600),
	};
	let wallet = TestWallet::new(PrivacyPolicy { ready_when: vec![dwell], ..PrivacyPolicy::fastest() });
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	wallet.coin(3);
	wallet.entry(3, 0);
	let unconfirmed = layer.offload(op(1), PurseId::MAIN, 8, [7; 32], Initiator::User, false).await;
	assert!(matches!(unconfirmed, Err(Error::NeedsConfirmation)), "private coins do not spare the warning");
	let confirmed = layer.offload(op(1), PurseId::MAIN, 8, [7; 32], Initiator::User, true).await;
	assert_eq!(confirmed.unwrap(), Started::New);
}

/// The current installation's registrations, with their first record.
async fn registrations(layer: &Layer) -> Vec<(OpKey, DurableTxId)> {
	let view = layer.view().await.unwrap();
	view.operations()
		.filter(|operation| operation.kind == OpKind::RegisterInstallation)
		.map(|operation| {
			let record = view.registrations().of_operation(&operation.key).next().expect("a registered record").record;
			(operation.key.clone(), record)
		})
		.collect()
}

#[tokio::test]
async fn a_final_registration_is_not_started_again_and_reads_registered() {
	let store = crate::store::open_memory_core_db();
	let (layer, ..) = started_layer(store.clone()).await;
	let (_, record) = registrations(&layer).await[0].clone();
	set_status(&store, record, "FINALIZED_SUCCESS", None);
	assert_eq!(layer.recoverable().await.unwrap(), Recoverability::Registered);

	drop(layer);
	let (layer, ..) = started_layer(store).await;
	assert_eq!(registrations(&layer).await.len(), 1, "nothing is attempted for a final registration");
	assert_eq!(layer.recoverable().await.unwrap(), Recoverability::Registered);
}

#[tokio::test]
async fn a_registration_that_failed_earlier_is_started_again_at_once() {
	let store = crate::store::open_memory_core_db();
	let (layer, ..) = started_layer(store.clone()).await;
	let (_, record) = registrations(&layer).await[0].clone();
	set_status(&store, record, "FAILURE", Some("EXPIRED"));
	assert_eq!(layer.recoverable().await.unwrap(), Recoverability::Pending { since: NOW });

	drop(layer);
	let (layer, ..) = started_layer(store).await;
	assert_eq!(registrations(&layer).await.len(), 2, "the next start registers again");
	assert_eq!(layer.recoverable().await.unwrap(), Recoverability::Pending { since: NOW });
}

#[tokio::test]
async fn the_event_stream_reports_when_the_installation_becomes_recoverable() {
	let store = crate::store::open_memory_core_db();
	let (layer, ..) = started_layer(store.clone()).await;
	let (_, record) = registrations(&layer).await[0].clone();
	let mut events = layer.events().await.unwrap();
	assert_eq!(events.next().await, Some(Event::Resynced));
	set_status(&store, record, "FINALIZED_SUCCESS", None);
	let mut recoverability = events.filter(|event| core::future::ready(matches!(event, Event::Recoverability(_))));
	let reported =
		tokio::time::timeout(Duration::from_secs(5), recoverability.next()).await.expect("the change is reported");
	assert_eq!(reported, Some(Event::Recoverability(Recoverability::Registered)));
}

#[tokio::test]
async fn an_import_refuses_a_secret_that_is_not_a_canonical_key() {
	let (layer, ..) = started_layer(crate::store::open_memory_core_db()).await;
	let canonical = crate::coinage::namespace::coin_key(&[3; 16], PurseId::MAIN, InstallationId([1; 32]), 0).unwrap().0;
	let not_canonical = CoinSecret([0xff; 64]);
	let refused = layer.import(op(1), PurseId::MAIN, vec![canonical, not_canonical]).await;
	assert!(matches!(refused, Err(Error::InvalidSecret(1))));
}

const EARLIER: InstallationId = InstallationId([4; 32]);

/// One longest validity window of the fixture's chain: 64 blocks of six
/// seconds.
const WINDOW: u64 = 64 * 6;

/// A layer over `store` that reads `chain` and whose registry lists
/// `registered`.
async fn layer_over(store: Db, chain: Arc<RecordingChain>, registered: Vec<InstallationId>) -> Layer {
	let mut interfaces = layer_interfaces(store, Arc::new(TestRuntime::default()));
	interfaces.chain = chain;
	interfaces.registry = Arc::new(ListedRegistry(registered));
	start(interfaces, chain_config(), SCAN, None, Running::new()).await.expect("the layer starts")
}

/// Waits until no reconstruction scan runs in this process.
async fn scans_stopped(layer: &Layer) {
	let stopped = async {
		while !layer.0.reconstructing.lock().is_empty() {
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	};
	tokio::time::timeout(Duration::from_secs(30), stopped).await.expect("the scans stop");
}

/// Each reconstruction, with whether it ended.
async fn reconstructions(layer: &Layer) -> Vec<(OpKey, bool)> {
	let view = layer.view().await.unwrap();
	view.operations()
		.filter(|operation| operation.kind == OpKind::Reconstruct)
		.map(|operation| (operation.key.clone(), operation.end.is_some()))
		.collect()
}

fn earlier_coin_chain(index: u32) -> Arc<RecordingChain> {
	let chain = Arc::new(RecordingChain::default());
	let (_, account) = crate::coinage::namespace::coin_key(&ENTROPY, PurseId::MAIN, EARLIER, index).unwrap();
	let coin = crate::coinage::chain::ChainCoin { instance: 0, value: 5, age: 1, locked_until: None };
	chain.chain.coins.lock().insert(account, coin);
	chain
}

/// A wipe is recovered without any caller, but only once every transaction
/// from before it has landed or died.
#[tokio::test]
async fn the_entry_point_starts_the_default_reconstruction_once_it_is_due() {
	let store = crate::store::open_memory_core_db();
	let layer = layer_over(store.clone(), earlier_coin_chain(3), vec![EARLIER]).await;
	let early = layer.run_due(NOW).await.unwrap();
	assert_eq!((early.started.clone(), early.call_again_within), (Vec::new(), Some(Duration::from_secs(WINDOW))));
	assert!(reconstructions(&layer).await.is_empty());

	let due = layer.run_due(NOW + WINDOW).await.unwrap();
	scans_stopped(&layer).await;
	let started = reconstructions(&layer).await;
	assert_eq!(started, [(due.started[0].clone(), true)], "the layer's own reconstruction ran to its end");
	assert!(due.started[0].is_own());
	let view = layer.view().await.unwrap();
	let found = Asset::Coin { purse: PurseId::MAIN, installation: EARLIER, index: 3 };
	assert!(view.asset(&found).is_some(), "the registered installation's coin is adopted");
	assert!(view.installations().iter().all(|(_, progress)| progress.first_scan_done));

	layer.run_due(NOW + WINDOW).await.unwrap();
	assert_eq!(reconstructions(&layer).await.len(), 1, "nothing is due once the registry is read and all is scanned");
}

#[tokio::test]
async fn an_installation_registered_since_the_last_launch_is_scanned_and_no_other() {
	let store = crate::store::open_memory_core_db();
	let layer = layer_over(store.clone(), Arc::new(RecordingChain::default()), Vec::new()).await;
	layer.run_due(NOW + WINDOW).await.unwrap();
	scans_stopped(&layer).await;
	drop(layer);

	let chain = earlier_coin_chain(3);
	let layer = layer_over(store, chain.clone(), vec![EARLIER]).await;
	let due = layer.run_due(NOW + WINDOW).await.unwrap();
	assert_eq!(due.started.len(), 1, "each launch reads the registry again");
	scans_stopped(&layer).await;
	let earlier: Vec<[u8; 32]> = (0..64)
		.map(|index| crate::coinage::namespace::coin_key(&ENTROPY, PurseId::MAIN, EARLIER, index).unwrap().1)
		.collect();
	assert!(!chain.accounts_read().is_empty());
	assert!(chain.accounts_read().iter().all(|account| earlier.contains(account)), "the current installation is not scanned again");
	let found = Asset::Coin { purse: PurseId::MAIN, installation: EARLIER, index: 3 };
	assert!(layer.view().await.unwrap().asset(&found).is_some());
}

#[tokio::test]
async fn a_reconstruction_whose_scan_failed_is_resumed_by_the_next_run() {
	let chain = earlier_coin_chain(3);
	chain.heads_failing.store(true, Ordering::SeqCst);
	let layer = layer_over(crate::store::open_memory_core_db(), chain.clone(), vec![EARLIER]).await;
	let due = layer.run_due(NOW + WINDOW).await.unwrap();
	scans_stopped(&layer).await;
	assert_eq!(reconstructions(&layer).await, [(due.started[0].clone(), false)], "the failed scan leaves it open");

	chain.heads_failing.store(false, Ordering::SeqCst);
	let again = layer.run_due(NOW + WINDOW).await.unwrap();
	assert!(again.started.is_empty(), "the open reconstruction is resumed, not a new one started");
	scans_stopped(&layer).await;
	assert_eq!(reconstructions(&layer).await, [(due.started[0].clone(), true)]);
	let view = layer.view().await.unwrap();
	assert!(view.asset(&Asset::Coin { purse: PurseId::MAIN, installation: EARLIER, index: 3 }).is_some());
}

mod use_cases;
