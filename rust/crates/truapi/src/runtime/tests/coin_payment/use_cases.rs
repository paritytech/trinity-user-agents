//! RFC-6 payments and top-ups as the products' use cases expect them.

use truapi::{
	latest::{
		DerivationIndex, HostPaymentError, HostPaymentRequest, HostPaymentStatusSubscribeError,
		HostPaymentStatusSubscribeItem, HostPaymentTopUpError, HostPaymentTopUpRequest,
		HostPaymentTopUpStatusSubscribeError, HostPaymentTopUpStatusSubscribeItem, PaymentTopUpSource, ProductAccountId,
	},
	versioned::IntoLatest,
};

use super::*;
use crate::{
	coinage::{

		assembly::Funding,
		chain::ChainCoin,
		layer::{Interfaces, start},
		ledger::{BlockRef, Facts, Subject},
		namespace::coin_key,
		operation::{OpKey, Retry},
		plugins::Running,
		payments::records::{self, OperationRecord, Rfc6Kind, Rfc6Record},
		start::RecordParams,
		testing::{FakeChain, SCAN, TestRuntime, chain_config, coin_read, funding::funding, layer_interfaces, set_status},
	},
	durable::DurableTxId,
	installation::InstallationId,
	platform::{ProductContext, ProductId},
	runtime::{CoinageEngine, PaymentEngine, ProductFunding, TopUpEngine},
};

fn spend_reviews(platform: &StubPlatform) -> Vec<crate::platform::CoinPaymentSpendReview> {
	platform.coin_payment_spend_reviews.lock().unwrap().clone()
}

async fn pay(host: &ProductRuntimeHost<RemoteAccountHolder>, amount: u128, id: u8) -> Result<(), HostPaymentError> {
	match truapi::api::Payment::request(host, &CallContext::default(), payment_request(amount, id)).await {
		Ok(_) => Ok(()),
		Err(CallError::Domain(error)) => Err(error.into_latest()),
		Err(other) => panic!("the call itself failed: {other:?}"),
	}
}

/// The engine the product host serves RFC-6 from, over a running layer.
struct Engine {
	wallet: TestWallet,
	engine: CoinageEngine,
	product: ProductContext,
	/// Every spend review the user saw.
	reviews: Arc<std::sync::Mutex<Vec<crate::platform::CoinPaymentSpendReview>>>,
}

async fn engine(platform: StubPlatform, funding: Arc<dyn ProductFunding>) -> Engine {
	engine_over(PrivacyPolicy::fastest(), Arc::new(FakeChain::default()), platform, funding).await
}

/// The engine over a wallet under `policy`, whose chain is `chain`.
async fn engine_over(
	policy: PrivacyPolicy,
	chain: Arc<FakeChain>,
	platform: StubPlatform,
	funding: Arc<dyn ProductFunding>,
) -> Engine {
	let wallet = TestWallet::new(policy);
	let interfaces = Interfaces { chain, ..layer_interfaces(wallet.db.clone(), Arc::new(TestRuntime::default())) };
	let layer = start(interfaces, chain_config(), SCAN, None, Running::new()).await.expect("the layer starts");
	let clock = Arc::new(AtomicU64::new(NOW * 1000));
	let payments = payments(layer, wallet.db.clone(), Arc::new(Board::default()), clock);
	let reviews = platform.coin_payment_spend_reviews.clone();
	let engine = CoinageEngine { payments, platform: Arc::new(platform), funding };
	Engine { wallet, engine, product: ProductContext::new(ProductId::for_test(PRODUCT)), reviews }
}

/// A platform that answers successive spend reviews with `decisions`.
fn deciding(decisions: &[bool]) -> StubPlatform {
	StubPlatform { coin_payment_spend_decisions: std::sync::Mutex::new(decisions.iter().copied().collect()), ..Default::default() }
}

/// Product accounts, each a different funding account for each product
/// and index.
struct ProductAccounts;

impl ProductFunding for ProductAccounts {
	fn product_account(&self, account: &ProductAccountId) -> Option<Arc<dyn Funding>> {
		let DerivationIndex::Index(index) = account.derivation_index else { return None };
		let product = account.dot_ns_identifier.as_str().as_bytes()[0];
		Some(funding(product.wrapping_add(u8::try_from(index).ok()?)))
	}
}

/// A payment from the main purse to an external account.
fn payment(amount: u128, id: u8) -> HostPaymentRequest {
	HostPaymentRequest { from: None, amount, destination: [9; 32], id: [id; 32], funding: None }
}

/// The secret of a peer's coin, as a product hands it over.
fn peer_coin(index: u32) -> [u8; 64] {
	coin_key(&[5; 16], PurseId::MAIN, InstallationId([1; 32]), index).unwrap().0.0
}

fn coins_top_up(coins: &[u32], amount: u128, id: u8) -> HostPaymentTopUpRequest {
	HostPaymentTopUpRequest {
		into: None,
		amount,
		source: PaymentTopUpSource::Coins { sr25519_secret_keys: coins.iter().map(|index| peer_coin(*index)).collect() },
		id: [id; 32],
		funding: None,
	}
}

impl Engine {
	async fn top_up(&self, request: HostPaymentTopUpRequest) -> Result<(), HostPaymentTopUpError> {
		TopUpEngine::top_up(&self.engine, &self.product, request).await
	}

	async fn pay(&self, request: HostPaymentRequest) -> Result<(), HostPaymentError> {
		PaymentEngine::request_payment(&self.engine, &self.product, request).await
	}

	fn payment_status(
		&self,
		id: u8,
	) -> futures::stream::BoxStream<'static, Result<HostPaymentStatusSubscribeItem, HostPaymentStatusSubscribeError>> {
		PaymentEngine::subscribe_payment_status(&self.engine, &self.product, [id; 32])
	}

	/// Whether each spend review the user saw took held-back value.
	fn warnings(&self) -> Vec<bool> {
		self.reviews.lock().unwrap().iter().map(|review| review.takes_held_back_value).collect()
	}

	/// The RFC-6 record the product's `id` names.
	async fn record(&self, id: u8) -> Option<Rfc6Record> {
		self.wallet.db.read(move |conn| records::rfc6(conn, PRODUCT.as_bytes(), [id; 32])).await.unwrap()
	}

	fn top_up_status(
		&self,
		id: u8,
	) -> futures::stream::BoxStream<'static, Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>>
	{
		TopUpEngine::subscribe_top_up_status(&self.engine, &self.product, [id; 32])
	}

	/// The records of the operation the product's `id` names.
	fn records(&self, id: u8) -> Vec<DurableTxId> {
		let key = OpKey { scope: PRODUCT.as_bytes().to_vec(), id: [id; 32] };
		self.wallet.view().registrations().of_operation(&key).map(|facts| facts.record).collect()
	}
}

/// A product that lost the response retries with the same id; asking the
/// user again would confirm a spend the retry cannot make.
#[tokio::test]
async fn an_id_the_product_already_used_is_refused_before_the_user_sees_anything() {
	let platform = Arc::new(StubPlatform { coin_payment_spend_confirmed: true, ..Default::default() });
	let (wallet, host) = coinage_host(platform.clone()).await;
	wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, crate::coinage::testing::entry_read(0));
	assert_eq!(pay(&host, 8 * UNIT, 1).await, Ok(()));

	assert_eq!(pay(&host, 8 * UNIT, 1).await, Err(HostPaymentError::AlreadyExists));
	assert_eq!(spend_reviews(&platform).len(), 1, "only the first call asked the user");
}

/// Only one top-up per source runs at a time (N2 invariant 4); for coins,
/// one shared coin is the same source.
#[tokio::test]
async fn a_coins_top_up_sharing_a_coin_with_a_running_one_is_refused_as_busy() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	assert_eq!(engine.top_up(coins_top_up(&[1, 2], 8 * UNIT, 1)).await, Ok(()));
	assert_eq!(engine.top_up(coins_top_up(&[1, 2], 8 * UNIT, 2)).await, Err(HostPaymentTopUpError::SourceBusy));
	assert_eq!(engine.top_up(coins_top_up(&[2, 3], 8 * UNIT, 3)).await, Err(HostPaymentTopUpError::SourceBusy));
	assert_eq!(engine.top_up(coins_top_up(&[3], 8 * UNIT, 2)).await, Ok(()), "a busy refusal leaves the id unused");
}

#[tokio::test]
async fn a_coins_top_up_no_other_top_up_is_claiming_is_accepted() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	assert_eq!(engine.top_up(coins_top_up(&[1], 8 * UNIT, 1)).await, Ok(()));
	assert_eq!(engine.top_up(coins_top_up(&[2], 8 * UNIT, 2)).await, Ok(()));
}

/// A store failure, or the instant between the record and layer 1's start,
/// says nothing about the top-up's outcome (layer 1, P11).
#[tokio::test]
async fn a_status_layer_one_cannot_read_ends_the_follow_with_a_fixed_reason_not_an_outcome() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	let record = Rfc6Record {
		product: PRODUCT.as_bytes().to_vec(),
		id: [1; 32],
		kind: Rfc6Kind::TopUp,
		amount: 8 * UNIT,
		source: None,
		funding: None,
		operation: OperationRecord { key: OpKey { scope: PRODUCT.as_bytes().to_vec(), id: [1; 32] }, refused: None },
	};
	engine.wallet.db.write(move |tx| records::record_rfc6(tx, &record)).await.unwrap().unwrap();
	let items: Vec<_> = engine.top_up_status(1).collect().await;
	assert_eq!(items, [Err(HostPaymentTopUpStatusSubscribeError::Unknown { reason: "the status is unavailable".into() })]);
}

/// Several layer-1 statuses map to one RFC-6 item; the product hears each
/// item once.
#[tokio::test]
async fn a_status_the_product_already_has_is_not_reported_again() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	engine.top_up(coins_top_up(&[1, 2], 8 * UNIT, 1)).await.unwrap();
	let claims = engine.records(1);
	let mut items = engine.top_up_status(1);
	assert_eq!(items.next().await, Some(Ok(HostPaymentTopUpStatusSubscribeItem::Detecting)));
	set_status(&engine.wallet.db, claims[0], "PENDING", None);
	assert_eq!(items.next().await, Some(Ok(HostPaymentTopUpStatusSubscribeItem::Claiming)));
	set_status(&engine.wallet.db, claims[1], "PENDING_SUCCESS", None);
	tokio::time::sleep(core::time::Duration::from_millis(50)).await;
	for claim in &claims {
		set_status(&engine.wallet.db, *claim, "FAILURE", Some("ABANDONED"));
	}
	assert_eq!(items.next().await, Some(Ok(HostPaymentTopUpStatusSubscribeItem::NotClaimed)));
	assert_eq!(items.next().await, None);
}

/// A top-up of the main purse from one of the product's accounts.
fn account_top_up(index: u32, id: u8) -> HostPaymentTopUpRequest {
	HostPaymentTopUpRequest {
		into: None,
		amount: 8 * UNIT,
		source: PaymentTopUpSource::ProductAccount { derivation_index: DerivationIndex::Index(index) },
		id: [id; 32],
		funding: None,
	}
}

/// A policy that holds a coin back for recycling from its first block.
fn recycling_at_once(policy: PrivacyPolicy) -> PrivacyPolicy {
	PrivacyPolicy { recycle_at: crate::coinage::RecycleAge::Age(0), max_unavailable_share: 100, ..policy }
}

#[tokio::test]
async fn a_payment_asks_the_user_to_confirm_the_spend_once_before_it_starts() {
	let engine = engine(deciding(&[true]), Arc::new(NoProductAccounts)).await;
	engine.wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, crate::coinage::testing::entry_read(0));
	assert_eq!(engine.pay(payment(8 * UNIT, 1)).await, Ok(()));
	let reviews = engine.reviews.lock().unwrap().clone();
	let asked: Vec<_> = reviews.iter().map(|review| (review.from_purse, review.amount, review.takes_held_back_value)).collect();
	assert_eq!(asked, [(PurseId::MAIN.0, 8 * UNIT, false)]);
	assert!(engine.record(1).await.is_some(), "the payment started");
}

/// The fastest policy lets a product's spend take value still gaining
/// privacy without asking, so the user sees the spend confirmation only.
#[tokio::test]
async fn no_privacy_warning_is_raised_under_the_fastest_policy() {
	let policy = recycling_at_once(PrivacyPolicy::fastest());
	let engine = engine_over(policy, Arc::new(FakeChain::default()), deciding(&[true]), Arc::new(NoProductAccounts)).await;
	engine.wallet.coin(3);
	assert_eq!(engine.pay(payment(8 * UNIT, 1)).await, Ok(()));
	assert_eq!(engine.warnings(), [false]);
}

/// Value still gaining privacy is spent only once the user agreed to that
/// separately, after confirming the spend.
#[tokio::test]
async fn spending_value_still_gaining_privacy_warns_the_user_after_the_confirmation() {
	let chain = Arc::new(FakeChain::default());
	let engine = engine_over(PrivacyPolicy::balanced(), chain, deciding(&[true, true]), Arc::new(NoProductAccounts)).await;
	engine.wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, crate::coinage::testing::entry_read(0));
	assert_eq!(engine.pay(payment(8 * UNIT, 1)).await, Ok(()));
	assert_eq!(engine.warnings(), [false, true]);
}

/// Under the shipped wallet, coins in the private balance still take the
/// privacy warning before a product spends them.
#[tokio::test]
async fn private_coins_do_not_spare_the_privacy_warning() {
	let chain = Arc::new(FakeChain::default());
	let engine = engine_over(PrivacyPolicy::balanced(), chain, deciding(&[true, true]), Arc::new(NoProductAccounts)).await;
	engine.wallet.coin(3);
	assert_eq!(engine.pay(payment(8 * UNIT, 1)).await, Ok(()));
	assert_eq!(engine.warnings(), [false, true]);
}

#[tokio::test]
async fn under_the_most_private_policy_a_payment_is_confirmed_then_warned_about() {
	let chain = Arc::new(FakeChain::default());
	let platform = deciding(&[true, true]);
	let engine = engine_over(PrivacyPolicy::most_private(), chain, platform, Arc::new(NoProductAccounts)).await;
	engine.wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, crate::coinage::testing::entry_read(0));
	assert_eq!(engine.pay(payment(8 * UNIT, 1)).await, Ok(()));
	assert_eq!(engine.warnings(), [false, true]);
}

#[tokio::test]
async fn a_user_who_declines_rejects_the_payment_and_nothing_is_recorded() {
	let engine = engine(deciding(&[false]), Arc::new(NoProductAccounts)).await;
	engine.wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, crate::coinage::testing::entry_read(0));
	assert_eq!(engine.pay(payment(8 * UNIT, 1)).await, Err(HostPaymentError::Rejected));
	assert_eq!(engine.record(1).await, None);
}

#[tokio::test]
async fn the_status_of_a_payment_the_product_never_made_is_not_found() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	let items: Vec<_> = engine.payment_status(1).collect().await;
	assert_eq!(items, [Err(HostPaymentStatusSubscribeError::PaymentNotFound)]);
}

#[tokio::test]
async fn a_followed_payment_is_reported_until_it_settles_and_no_further() {
	let engine = engine(deciding(&[true]), Arc::new(NoProductAccounts)).await;
	engine.wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, crate::coinage::testing::entry_read(0));
	engine.pay(payment(8 * UNIT, 1)).await.unwrap();
	let mut items = engine.payment_status(1);
	assert_eq!(items.next().await, Some(Ok(HostPaymentStatusSubscribeItem::Processing)));
	for record in engine.records(1) {
		set_status(&engine.wallet.db, record, "FINALIZED_SUCCESS", None);
	}
	engine.engine.payments.layer().run_due(NOW).await.unwrap();
	assert_eq!(items.next().await, Some(Ok(HostPaymentStatusSubscribeItem::Completed)));
	assert_eq!(items.next().await, None);
}

/// The id names the operation in the product's own scope, so a product
/// follows its top-up by it and no other product can.
#[tokio::test]
async fn a_top_up_is_named_after_its_product_and_its_id() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	engine.top_up(coins_top_up(&[1], 8 * UNIT, 1)).await.unwrap();
	let key = OpKey { scope: PRODUCT.as_bytes().to_vec(), id: [1; 32] };
	assert!(engine.wallet.view().operation(&key).is_some());
}

/// The window is fixed when the top-up starts, so a rebuilt attempt does
/// not extend it.
#[tokio::test]
async fn a_top_ups_window_runs_from_when_it_started() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	let before = crate::unix_time::current_unix_secs();
	engine.top_up(coins_top_up(&[1], 8 * UNIT, 1)).await.unwrap();
	let after = crate::unix_time::current_unix_secs();
	let view = engine.wallet.view();
	let claim = view.registrations().record(engine.records(1)[0]).unwrap();
	let params = RecordParams::from_params(&claim.state.plan.as_ref().unwrap().params).unwrap();
	let Retry::Until(deadline) = params.retry else { panic!("{params:?}") };
	assert!((before + 3600..=after + 3600).contains(&deadline), "{deadline}");
}

#[tokio::test]
async fn a_top_up_id_already_used_is_refused() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	assert_eq!(engine.top_up(coins_top_up(&[1], 8 * UNIT, 1)).await, Ok(()));
	assert_eq!(engine.top_up(coins_top_up(&[2], 8 * UNIT, 1)).await, Err(HostPaymentTopUpError::AlreadyExists));
	assert_eq!(engine.records(1).len(), 1, "the first top-up runs alone");
}

#[tokio::test]
async fn a_source_that_cannot_be_resolved_is_refused_and_nothing_is_recorded() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	assert_eq!(engine.top_up(account_top_up(0, 1)).await, Err(HostPaymentTopUpError::InvalidSource));
	assert_eq!(engine.record(1).await, None);
}

#[tokio::test]
async fn the_same_account_index_of_the_same_product_is_busy() {
	let engine = engine(StubPlatform::default(), Arc::new(ProductAccounts)).await;
	assert_eq!(engine.top_up(account_top_up(0, 1)).await, Ok(()));
	assert_eq!(engine.top_up(account_top_up(0, 2)).await, Err(HostPaymentTopUpError::SourceBusy));
	assert_eq!(engine.top_up(account_top_up(1, 2)).await, Ok(()), "another index is another account");
}

/// A product account is derived under its product, so another product's
/// account at the same index is another source.
#[tokio::test]
async fn the_same_account_index_of_another_product_is_another_source() {
	let engine = engine(StubPlatform::default(), Arc::new(ProductAccounts)).await;
	assert_eq!(engine.top_up(account_top_up(0, 1)).await, Ok(()));
	let other = ProductContext::new(ProductId::for_test("game.dot"));
	assert_eq!(TopUpEngine::top_up(&engine.engine, &other, account_top_up(0, 1)).await, Ok(()));
}

#[tokio::test]
async fn a_private_key_another_top_up_pays_from_is_busy() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	let key = |id: u8| HostPaymentTopUpRequest {
		source: PaymentTopUpSource::PrivateKey { sr25519_secret_key: peer_coin(9) },
		..account_top_up(0, id)
	};
	assert_eq!(engine.top_up(key(1)).await, Ok(()));
	assert_eq!(engine.top_up(key(2)).await, Err(HostPaymentTopUpError::SourceBusy));
}

#[tokio::test]
async fn following_a_top_up_the_product_never_made_is_not_found() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	let items: Vec<_> = engine.top_up_status(1).collect().await;
	assert_eq!(items, [Err(HostPaymentTopUpStatusSubscribeError::TopUpNotFound)]);
}

/// A top-up layer 1 refused to start still has its verdict, and its id
/// stays spent, so a retry cannot start a second one.
#[tokio::test]
async fn a_top_up_layer_one_refused_reports_its_verdict_and_keeps_its_id() {
	let engine = engine(StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	let refused = HostPaymentTopUpRequest {
		source: PaymentTopUpSource::Coins { sr25519_secret_keys: vec![[0xff; 64]] },
		..coins_top_up(&[], 8 * UNIT, 1)
	};
	assert_eq!(engine.top_up(refused).await, Err(HostPaymentTopUpError::InvalidSource));
	let items: Vec<_> = engine.top_up_status(1).collect().await;
	assert_eq!(items, [Ok(HostPaymentTopUpStatusSubscribeItem::NotClaimed)]);
	assert_eq!(engine.top_up(coins_top_up(&[1], 8 * UNIT, 1)).await, Err(HostPaymentTopUpError::AlreadyExists));
}

#[tokio::test]
async fn a_followed_top_up_is_reported_until_it_is_final_and_no_further() {
	let chain = Arc::new(FakeChain::default());
	let account = crate::coinage::planning::account_of(&crate::coinage::CoinSecret(peer_coin(1))).unwrap();
	chain.coins.lock().insert(account, ChainCoin { instance: 0, value: 3, age: 0, locked_until: None });
	let engine = engine_over(PrivacyPolicy::fastest(), chain, StubPlatform::default(), Arc::new(NoProductAccounts)).await;
	engine.top_up(coins_top_up(&[1], 8 * UNIT, 1)).await.unwrap();
	let claim = engine.records(1)[0];
	let mut items = engine.top_up_status(1);
	assert_eq!(items.next().await, Some(Ok(HostPaymentTopUpStatusSubscribeItem::Detecting)));

	let output = engine.wallet.view().registrations().record(claim).unwrap().rows.outputs[0];
	let facts = Facts { at: BlockRef { number: 11, hash: [11; 32] }, reads: vec![(Subject::Asset(output), coin_read(3))] };
	engine.wallet.db.write(move |tx| crate::coinage::ledger::observe(tx, &facts, NOW, 255)).await.unwrap();
	set_status(&engine.wallet.db, claim, "FINALIZED_SUCCESS", None);
	assert_eq!(items.next().await, Some(Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true })));
	assert_eq!(items.next().await, None);
}

/// The engine always names the shortfall; the core masks it for a product
/// without balance access.
#[tokio::test]
async fn a_payment_the_balance_cannot_cover_is_reported_as_insufficient() {
	let engine = engine(deciding(&[true]), Arc::new(NoProductAccounts)).await;
	assert_eq!(engine.pay(payment(8 * UNIT, 1)).await, Err(HostPaymentError::InsufficientBalance));
}
