//! Layer 2's flows as payments and claims use them.

use super::*;
use crate::{
	chain::HashAndNumber,
	coinage::{
		Asset, CoinSecret,
		chain::ChainCoin,
		layer::{Interfaces, Op, start},
		ledger::{Read, Subject},
		namespace::coin_key,
		operation::Retry,
		plugins::Running,
		start::RecordParams,
		testing::{FakeChain, SCAN, TestRuntime, chain_config, layer_interfaces},
	},
	installation::InstallationId,
};

/// The secret of a peer's coin.
fn peer_coin(index: u32) -> CoinSecret {
	coin_key(&[5; 16], PurseId::MAIN, InstallationId([1; 32]), index).unwrap().0
}

/// Layer 2 over a layer whose chain is `chain`.
async fn payments_over(chain: Arc<FakeChain>) -> (TestWallet, Payments) {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let interfaces = Interfaces { chain, ..layer_interfaces(wallet.db.clone(), Arc::new(TestRuntime::default())) };
	let layer = start(interfaces, chain_config(), SCAN, None, Running::new()).await.expect("the layer starts");
	let payments = payments(layer, wallet.db.clone(), Arc::new(Board::default()), Arc::new(AtomicU64::new(NOW_MILLIS)));
	(wallet, payments)
}

/// Nothing but the chain's new heads drives the layers while they run: a
/// coin that lands after its import started is seen at the next head, and
/// its claim can be built.
#[tokio::test]
async fn a_coin_that_appears_after_its_import_started_is_seen_at_the_next_head() {
	let chain = Arc::new(FakeChain::default());
	let (wallet, payments) = payments_over(chain.clone()).await;
	let op = Op { key: payments.random_key(), retry: Retry::Until(NOW + 3600) };
	payments.layer().import(op, PurseId::MAIN, vec![peer_coin(1)]).await.unwrap();
	let account = crate::coinage::planning::account_of(&peer_coin(1)).unwrap();
	chain.coins.lock().insert(account, ChainCoin { instance: 0, value: 2, age: 0, locked_until: None });

	let head = HashAndNumber { hash: subxt::utils::H256([11; 32]), number: 11 };
	payments.follow(futures::stream::iter([head])).await;
	let observed = wallet.view().observation(&Subject::Asset(Asset::PeerCoin(account))).and_then(|observation| observation.value.clone());
	assert!(matches!(observed, Some(Read::Coin { now: Some(_), .. })), "{observed:?}");
}

/// A payment's minting is retried until the cheque's claim deadline, chat
/// and merchant payments alike, so the payee never finds a coin that could
/// still be minted after the payer has given it up.
#[tokio::test]
async fn a_payment_is_retried_until_its_cheques_claim_deadline() {
	let setup = setup().await;
	setup.wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let to = new_receivable([1; 32]).0;
	let cheque = setup.payments.pay("shop", PurseId::MAIN, to, 4, Initiator::Product, false).await.unwrap();
	let recorded = outgoing(&setup, &cheque).await;
	let view = setup.wallet.view();
	let retries: Vec<Retry> = view
		.registrations()
		.of_operation(&recorded.export)
		.filter_map(|facts| RecordParams::from_params(&facts.state.plan.as_ref()?.params))
		.map(|params| params.retry)
		.collect();
	assert_eq!(retries, [Retry::Until(recorded.claim_by / 1000)]);
}
