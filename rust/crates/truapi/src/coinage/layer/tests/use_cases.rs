//! The primitives as top-ups, claims and payments use them.

use super::*;
use crate::{
	coinage::{
		namespace::coin_key,
		planning::Call,
		registration::RecordFacts,
		start::RecordParams,
		testing::{funding::funding, set_status},
	},
	durable::DurableTxId,
};

/// The secret of a peer's coin.
fn peer_coin(index: u32) -> CoinSecret {
	coin_key(&[5; 16], PurseId::MAIN, InstallationId([1; 32]), index).unwrap().0
}

fn account(secret: &CoinSecret) -> [u8; 32] {
	crate::coinage::planning::account_of(secret).unwrap()
}

fn until(id: u8, deadline: u64) -> Op {
	Op { retry: Retry::Until(deadline), ..op(id) }
}

async fn records(layer: &Layer, key: &OpKey) -> Vec<RecordFacts> {
	layer.view().await.unwrap().registrations().of_operation(key).cloned().collect()
}

fn params(facts: &RecordFacts) -> RecordParams {
	let plan = facts.state.plan.as_ref().expect("a deferred record keeps its plan");
	RecordParams::from_params(&plan.params).expect("E3 wrote the parameters")
}

fn ids(records: &[RecordFacts]) -> Vec<DurableTxId> {
	records.iter().map(|facts| facts.record).collect()
}

/// Every coin is claimed by its own transfer under the caller's key, in one
/// registration, with the caller's window, and the secret that signs every
/// attempt is kept until the claim is final.
#[tokio::test]
async fn an_import_claims_each_coin_under_the_callers_key_with_its_window_and_secret() {
	let (layer, ..) = started_layer(crate::store::open_memory_core_db()).await;
	let coins = vec![peer_coin(1), peer_coin(2)];
	let deadline = NOW + 3600;
	assert_eq!(layer.import(until(1, deadline), PurseId::MAIN, coins.clone()).await.unwrap(), Started::New);

	let claims = records(&layer, &op(1).key).await;
	let claimed: Vec<_> = claims.iter().map(|facts| (facts.rows.inputs.clone(), facts.rows.outputs.len(), params(facts))).collect();
	let transfer = RecordParams { call: Call::Transfer, retry: Retry::Until(deadline) };
	assert_eq!(claimed, [
		(vec![Asset::PeerCoin(account(&coins[0]))], 1, transfer),
		(vec![Asset::PeerCoin(account(&coins[1]))], 1, transfer)
	]);
	for coin in &coins {
		let kept = layer.0.store.read({
			let account = account(coin);
			move |conn| ledger::secret(conn, &account)
		});
		assert_eq!(kept.await.unwrap(), Some(coin.clone()));
	}
}

/// A coin a live claim holds is refused to a second import: two claims of
/// one coin would compete.
#[tokio::test]
async fn a_second_import_of_a_coin_a_claim_holds_is_refused() {
	let (layer, ..) = started_layer(crate::store::open_memory_core_db()).await;
	layer.import(op(1), PurseId::MAIN, vec![peer_coin(1)]).await.unwrap();
	let refused = layer.import(op(2), PurseId::MAIN, vec![peer_coin(1)]).await;
	assert!(matches!(refused, Err(Error::InputTaken)), "{refused:?}");
	assert!(layer.view().await.unwrap().operation(&op(2).key).is_none(), "a refused start records nothing");
}

/// A top-up needs no follower: its loads are registered before `top_up`
/// returns, and the durability layer builds them.
#[tokio::test]
async fn a_top_up_registers_its_loads_before_it_returns() {
	let (layer, ..) = started_layer(crate::store::open_memory_core_db()).await;
	let paying = funding(1);
	layer.top_up(op(1), PurseId::MAIN, 8, paying.clone()).await.unwrap();
	let loads = records(&layer, &op(1).key).await;
	let planned: Vec<_> = loads.iter().map(|facts| (params(facts).call, facts.rows.outputs.len())).collect();
	assert_eq!(planned, [(Call::Load(paying.account()), 1)]);
}

/// Imported secrets and the claims they sign live in the store, so a claim
/// a previous process registered is still there to be built.
#[tokio::test]
async fn a_coins_import_is_still_there_to_build_after_a_restart() {
	let store = crate::store::open_memory_core_db();
	let (layer, ..) = started_layer(store.clone()).await;
	layer.import(op(1), PurseId::MAIN, vec![peer_coin(1)]).await.unwrap();
	let before = ids(&records(&layer, &op(1).key).await);
	drop(layer);

	let (layer, ..) = started_layer(store).await;
	let after = records(&layer, &op(1).key).await;
	assert_eq!(ids(&after), before);
	assert_eq!(after[0].state.status, crate::durable::DurableTxStatus::PendingSubmission);
	let account = account(&peer_coin(1));
	assert!(layer.0.store.read(move |conn| ledger::secret(conn, &account)).await.unwrap().is_some());
}

/// The balance is re-read whenever a registration changes what a coin is
/// for, and only a balance that differs from the last one is reported.
#[tokio::test]
async fn a_balance_is_reported_again_when_a_registration_takes_a_coin_and_not_otherwise() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let mut balances = layer.balance(PurseId::MAIN).await.unwrap();
	assert_eq!(balances.next().await.unwrap().spendable, 8);

	layer.create_purse("unrelated".into(), Vec::new()).await.unwrap();
	layer.export(op(1), PurseId::MAIN, 8, Initiator::User, false).await.unwrap();
	assert_eq!(balances.next().await.unwrap().spendable, 0, "the next balance reported is the changed one");
}

/// A capability can hold a key a product handed over; it is held while the
/// top-up may still build, and no longer.
#[tokio::test]
async fn the_funding_of_a_top_up_is_kept_until_its_verdict_and_dropped_then() {
	let (layer, running, _) = started_layer(crate::store::open_memory_core_db()).await;
	layer.top_up(op(1), PurseId::MAIN, 8, funding(1)).await.unwrap();
	let held = || running.state().unwrap().funding.contains_key(&op(1).key);
	let loads = ids(&records(&layer, &op(1).key).await);

	set_status(&layer.0.store, loads[0], "PENDING_SUCCESS", None);
	layer.run_due(NOW).await.unwrap();
	assert!(held(), "a load only included in a block may still be retracted and rebuilt");

	set_status(&layer.0.store, loads[0], "FINALIZED_SUCCESS", None);
	layer.run_due(NOW).await.unwrap();
	assert!(!held());
}
