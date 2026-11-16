use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;

use super::*;
use crate::coinage::{
	ledger::AssetKind,
	status::{Failure, Status},
	testing::{BLOCK, NOW, SCAN, TestRuntime, TestWallet, chain_config, coin_read, layer_interfaces, started_layer},
};

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
	let runtime = Arc::new(TestRuntime { failing: AtomicBool::new(true) });
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
async fn an_import_refuses_a_secret_that_is_not_a_canonical_key() {
	let (layer, ..) = started_layer(crate::store::open_memory_core_db()).await;
	let canonical = crate::coinage::namespace::coin_key(&[3; 16], PurseId::MAIN, InstallationId([1; 32]), 0).unwrap().0;
	let not_canonical = CoinSecret([0xff; 64]);
	let refused = layer.import(op(1), PurseId::MAIN, vec![canonical, not_canonical]).await;
	assert!(matches!(refused, Err(Error::InvalidSecret(1))));
}

mod use_cases;
