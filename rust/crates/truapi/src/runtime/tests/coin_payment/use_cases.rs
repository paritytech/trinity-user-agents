//! RFC-6 payments and top-ups as the products' use cases expect them.

use truapi::{
	latest::{
		HostPaymentError, HostPaymentTopUpError, HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError,
		HostPaymentTopUpStatusSubscribeItem, PaymentTopUpSource,
	},
	versioned::IntoLatest,
};

use super::*;
use crate::{
	coinage::{
		namespace::coin_key,
		operation::OpKey,
		payments::records::{self, OperationRecord, Rfc6Kind, Rfc6Record},
		testing::set_status,
	},
	durable::DurableTxId,
	installation::InstallationId,
	platform::{ProductContext, ProductId, TopUpPlatform},
	runtime::CoinageEngine,
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
}

async fn engine(platform: StubPlatform, funding: Arc<dyn crate::runtime::ProductFunding>) -> Engine {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	let clock = Arc::new(AtomicU64::new(NOW * 1000));
	let payments = payments(layer, wallet.db.clone(), Arc::new(Board::default()), clock);
	let engine = CoinageEngine { payments, platform: Arc::new(platform), funding };
	Engine { wallet, engine, product: ProductContext::new(ProductId::for_test(PRODUCT)) }
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
		TopUpPlatform::top_up(&self.engine, &self.product, request).await
	}

	fn top_up_status(
		&self,
		id: u8,
	) -> futures::stream::BoxStream<'static, Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>>
	{
		TopUpPlatform::subscribe_top_up_status(&self.engine, &self.product, [id; 32])
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
