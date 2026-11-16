use core::sync::atomic::AtomicU64;

use truapi::{
	api::CoinPayment,
	versioned::coin_payment::{
		HostCoinPaymentCreateChequeRequest, HostCoinPaymentCreatePurseRequest, HostCoinPaymentListPursesRequest,
		HostCoinPaymentQueryPurseRequest,
	},
};

use super::*;
use crate::coinage::{
	PrivacyPolicy, PurseId,
	ledger::AssetKind,
	payments::testing::{Board, payments},
	testing::{NOW, TestWallet, coin_read, started_layer},
};

const PRODUCT: &str = "shop.dot";

/// A product host over a running coinage layer, with the wallet behind it.
async fn coinage_host(platform: Arc<StubPlatform>) -> (TestWallet, ProductRuntimeHost<RemoteAccountHolder>) {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	let services = payment_services_over(platform);
	let clock = Arc::new(AtomicU64::new(NOW * 1000));
	let payments = payments(layer, wallet.db.clone(), Arc::new(Board::default()), clock);
	assert!(services.install_coinage_payments(payments));
	let host = payment_host(&services, PRODUCT, true);
	(wallet, host)
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

