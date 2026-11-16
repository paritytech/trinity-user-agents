//! RFC-6 payments and top-ups as the products' use cases expect them.

use truapi::{latest::HostPaymentError, versioned::IntoLatest};

use super::*;

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
