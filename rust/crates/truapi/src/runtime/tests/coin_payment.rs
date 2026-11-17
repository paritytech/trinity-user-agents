use core::sync::atomic::AtomicU64;

use futures::StreamExt;
use truapi::{
	api::CoinPayment,
	versioned::coin_payment::{
		HostCoinPaymentCreateChequeRequest, HostCoinPaymentCreatePurseRequest, HostCoinPaymentListPursesRequest,
		HostCoinPaymentQueryPurseRequest,
	},
};

mod use_cases;

use super::*;
use crate::coinage::{
	PrivacyPolicy, PurseId,
	ledger::AssetKind,
	payments::testing::{Board, payments},
	testing::{NOW, TestWallet, coin_read, started_layer},
};

const PRODUCT: &str = "shop.dot";

/// A wallet whose product accounts cannot pay a top-up.
struct NoProductAccounts;

impl crate::runtime::ProductFunding for NoProductAccounts {
	fn product_account(
		&self,
		_: &truapi::latest::ProductAccountId,
	) -> Option<Arc<dyn crate::coinage::assembly::Funding>> {
		None
	}
}

/// A product host over a running coinage layer, with the wallet behind it.
async fn coinage_host(platform: Arc<StubPlatform>) -> (TestWallet, ProductRuntimeHost<RemoteAccountHolder>) {
	let (wallet, host, _) = coinage_host_with_layer(platform).await;
	(wallet, host)
}

/// [`coinage_host`], with the layer it serves.
async fn coinage_host_with_layer(
	platform: Arc<StubPlatform>,
) -> (TestWallet, ProductRuntimeHost<RemoteAccountHolder>, crate::coinage::layer::Layer) {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	let services = payment_services_over(platform);
	let clock = Arc::new(AtomicU64::new(NOW * 1000));
	let payments = payments(layer.clone(), wallet.db.clone(), Arc::new(Board::default()), clock);
	let engine = crate::runtime::CoinageEngine {
		payments: payments.clone(),
		platform: services.platform.clone(),
		funding: Arc::new(NoProductAccounts),
	};
	assert!(services.install_coinage_payments(payments));
	assert!(services.install_coinage_engine(Arc::new(engine)));
	let host = payment_host(&services, PRODUCT, true);
	(wallet, host, layer)
}

async fn create(host: &ProductRuntimeHost<RemoteAccountHolder>, name: &str) -> u32 {
	let request = v01::HostCoinPaymentCreatePurseRequest { name: name.to_owned() };
	let response = host.create_purse(&CallContext::default(), HostCoinPaymentCreatePurseRequest::V1(request)).await;
	let truapi::versioned::coin_payment::HostCoinPaymentCreatePurseResponse::V1(created) =
		response.expect("the purse is created");
	created.purse
}

#[test]
fn a_host_without_coinage_answers_every_coin_payment_call_as_unsupported() {
	let services = payment_services_over(Arc::new(StubPlatform::default()));
	let host = payment_host(&services, PRODUCT, true);
	let request = v01::HostCoinPaymentCreatePurseRequest { name: "savings".to_owned() };
	let created = futures::executor::block_on(
		host.create_purse(&CallContext::default(), HostCoinPaymentCreatePurseRequest::V1(request)),
	);
	assert!(matches!(created, Err(CallError::Unsupported)));
}

#[tokio::test]
async fn a_product_sees_the_purse_it_created_and_its_creator_is_the_product() {
	let (_wallet, host) = coinage_host(Arc::new(StubPlatform::default())).await;
	let purse = create(&host, "savings").await;

	let request = v01::HostCoinPaymentQueryPurseRequest { purse };
	let queried = host.query_purse(&CallContext::default(), HostCoinPaymentQueryPurseRequest::V1(request)).await;
	let truapi::versioned::coin_payment::HostCoinPaymentQueryPurseResponse::V1(queried) = queried.unwrap();
	assert_eq!((queried.info.name.as_str(), queried.info.creator.as_str()), ("savings", PRODUCT));

	let listed = host
		.list_purses(&CallContext::default(), HostCoinPaymentListPursesRequest::V1(v01::HostCoinPaymentListPursesRequest {}))
		.await;
	let truapi::versioned::coin_payment::HostCoinPaymentListPursesResponse::V1(listed) = listed.unwrap();
	assert!(listed.purses.iter().any(|listed| listed.purse == purse), "its own purse is listed");
}

#[tokio::test]
async fn a_product_without_the_coin_payment_permission_is_denied() {
	let platform = Arc::new(StubPlatform { remote_permission_denied: true, ..Default::default() });
	let (_wallet, host) = coinage_host(platform).await;
	let request = v01::HostCoinPaymentCreatePurseRequest { name: "savings".to_owned() };
	let created = host.create_purse(&CallContext::default(), HostCoinPaymentCreatePurseRequest::V1(request)).await;
	assert!(matches!(created, Err(CallError::Domain(truapi::versioned::coin_payment::HostCoinPaymentCreatePurseError::V1(
		v01::CoinPaymentError::Denied
	)))));
}

#[tokio::test]
async fn every_spend_asks_the_user_and_a_refusal_is_denied() {
	let platform = Arc::new(StubPlatform::default());
	let (wallet, host) = coinage_host(platform.clone()).await;
	let purse = create(&host, "savings").await;
	wallet.hold(PurseId(purse), AssetKind::Coin, 3, coin_read(3));

	let request = v01::HostCoinPaymentCreateChequeRequest { from: purse, to: [9; 32], amount: 8 };
	let paid = host.create_cheque(&CallContext::default(), HostCoinPaymentCreateChequeRequest::V1(request)).await;
	assert!(paid.is_err(), "the stub platform refuses the spend");
	let reviews = platform.coin_payment_spend_reviews.lock().unwrap().clone();
	assert_eq!(reviews.len(), 1);
	assert_eq!((reviews[0].from_purse, reviews[0].takes_held_back_value), (purse, false));
}

/// The fixture's chain says one cent is 10 000 of the asset's units.
const UNIT: u128 = 10_000;

fn payment_request(amount: u128, id: u8) -> truapi::versioned::payment::HostPaymentRequest {
	truapi::versioned::payment::HostPaymentRequest::V2(truapi::v02::HostPaymentRequest {
		from: None,
		amount,
		destination: [9; 32],
		id: [id; 32],
		funding: None,
	})
}

#[tokio::test]
async fn rfc6_balance_is_the_spendable_value_of_the_main_purse_in_asset_units() {
	let (wallet, host) = coinage_host(Arc::new(StubPlatform::default())).await;
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let request = truapi::versioned::payment::HostPaymentBalanceSubscribeRequest::V1(
		v01::HostPaymentBalanceSubscribeRequest { purse: None },
	);
	let mut balances = truapi::api::Payment::balance_subscribe(&host, &CallContext::default(), request).await;
	let first = balances.next().await.expect("a balance").expect("no failure");
	let truapi::versioned::payment::HostPaymentBalanceSubscribeItem::V1(first) = first;
	assert_eq!(first.available, 8 * UNIT);
}

#[tokio::test]
async fn an_rfc6_payment_the_user_declines_or_below_a_cent_is_refused() {
	let (wallet, host) = coinage_host(Arc::new(StubPlatform::default())).await;
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let declined = truapi::api::Payment::request(&host, &CallContext::default(), payment_request(8 * UNIT, 1)).await;
	assert!(matches!(declined, Err(CallError::Domain(_))), "the stub platform declines the spend");
	let too_small = truapi::api::Payment::request(&host, &CallContext::default(), payment_request(UNIT - 1, 2)).await;
	assert!(matches!(too_small, Err(CallError::Domain(_))), "less than a cent is refused");
}

#[tokio::test]
async fn an_rfc6_payment_runs_once_under_its_id() {
	let platform = Arc::new(StubPlatform { coin_payment_spend_confirmed: true, ..Default::default() });
	let (wallet, host) = coinage_host(platform).await;
	wallet.hold(PurseId::MAIN, AssetKind::Entry, 3, crate::coinage::testing::entry_read(0));
	let first = truapi::api::Payment::request(&host, &CallContext::default(), payment_request(8 * UNIT, 1)).await;
	assert!(first.is_ok(), "the payment starts: {first:?}");
	let again = truapi::api::Payment::request(&host, &CallContext::default(), payment_request(8 * UNIT, 1)).await;
	assert!(matches!(again, Err(CallError::Domain(_))), "a reused id is refused");
}

fn payment_status_request(id: u8) -> truapi::versioned::payment::HostPaymentStatusSubscribeRequest {
	truapi::versioned::payment::HostPaymentStatusSubscribeRequest::V1(v01::HostPaymentStatusSubscribeRequest { id: [id; 32] })
}

#[tokio::test]
async fn an_rfc6_payment_status_ends_with_its_first_terminal_item() {
	let platform = Arc::new(StubPlatform { coin_payment_spend_confirmed: true, ..Default::default() });
	let (wallet, host, layer) = coinage_host_with_layer(platform).await;
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 3, coin_read(3));
	let started = truapi::api::Payment::request(&host, &CallContext::default(), payment_request(8 * UNIT, 1)).await;
	assert!(started.is_ok(), "the payment starts: {started:?}");
	let records: Vec<_> = wallet.view().registrations().records().map(|facts| facts.record).collect();
	for record in records {
		crate::coinage::testing::set_status(&wallet.db, record, "FAILURE", Some("DISPATCH_FAILED"));
	}
	layer.run_due(NOW).await.unwrap();
	let statuses = truapi::api::Payment::status_subscribe(&host, &CallContext::default(), payment_status_request(1)).await;
	let items: Vec<_> = tokio::time::timeout(core::time::Duration::from_secs(5), statuses.collect::<Vec<_>>())
		.await
		.expect("the stream ends after its terminal item");
	let failed = |item: &Result<truapi::versioned::payment::HostPaymentStatusSubscribeItem, _>| {
		matches!(
			item,
			Ok(truapi::versioned::payment::HostPaymentStatusSubscribeItem::V1(
				v01::HostPaymentStatusSubscribeItem::Failed { .. }
			))
		)
	};
	assert_eq!(items.len(), 1, "nothing follows the terminal item: {items:?}");
	assert!(failed(&items[0]), "its only phase failed on chain: {items:?}");
}

#[tokio::test]
async fn an_unknown_rfc6_payment_is_not_found() {
	let (_wallet, host) = coinage_host(Arc::new(StubPlatform::default())).await;
	let mut statuses =
		truapi::api::Payment::status_subscribe(&host, &CallContext::default(), payment_status_request(9)).await;
	let first = statuses.next().await.expect("an answer");
	assert!(matches!(
		first,
		Err(CallError::Domain(truapi::versioned::payment::HostPaymentStatusSubscribeError::V1(
			v01::HostPaymentStatusSubscribeError::PaymentNotFound
		)))
	));
}
