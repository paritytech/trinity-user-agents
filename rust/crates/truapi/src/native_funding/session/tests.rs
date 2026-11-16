//! A session's flow against a fake chain and a fake registry.

use std::sync::{Arc, Mutex};

use super::*;
use crate::{
	installation::InstallationId,
	native_funding::{
		keys::session_key_from_entropy,
		testing::{ENTROPY, FakeChain, FixedInstallation, FixedKeys},
	},
};

const INTENT: &str = "fs_native";
const SEND: u128 = 25_000_000_000;
const CREDIT: u128 = 10_000_000;

/// A registry that records what the route reports and starts, and serves a
/// session until it is delivered or failed.
#[derive(Default)]
struct FakeSessions {
	reports: Mutex<Vec<FundingUpdate>>,
	top_ups: Mutex<Vec<HostPaymentTopUpRequest>>,
}

impl FakeSessions {
	fn reports(&self) -> Vec<FundingUpdate> {
		self.reports.lock().unwrap().clone()
	}

	fn top_ups(&self) -> Vec<HostPaymentTopUpRequest> {
		self.top_ups.lock().unwrap().clone()
	}
}

#[async_trait::async_trait]
impl NativeSessions for FakeSessions {
	async fn report(
		&self,
		_intent: &str,
		update: FundingUpdate,
	) -> Result<(), HostFundingReportError> {
		self.reports.lock().unwrap().push(update);
		Ok(())
	}

	async fn start_top_up(
		&self,
		_intent: &str,
		request: HostPaymentTopUpRequest,
	) -> Result<(), CallError<HostPaymentTopUpError>> {
		self.top_ups.lock().unwrap().push(request);
		Ok(())
	}

	fn is_serving(&self, _intent: &str) -> bool {
		!self.reports.lock().unwrap().iter().any(|update| {
			matches!(
				update,
				FundingUpdate::In(FundingInboundUpdate::Delivered) | FundingUpdate::Failed { .. }
			)
		})
	}
}

struct Harness {
	chain: Arc<FakeChain>,
	sessions: Arc<FakeSessions>,
	route: Arc<NativeFunding>,
}

fn harness() -> Harness {
	let chain = Arc::new(FakeChain::default());
	let sessions = Arc::new(FakeSessions::default());
	let route = NativeFunding::new(NativeFundingDeps {
		sessions: sessions.clone(),
		chain: chain.clone(),
		keys: Arc::new(FixedKeys),
		installation: Arc::new(FixedInstallation),
		spawner: crate::spawner::Spawner::new(tokio::runtime::Handle::current()),
		poll: Duration::from_millis(5),
	});
	Harness { chain, sessions, route }
}

fn key() -> SessionKey {
	let slot = SessionKeySlot {
		installation: InstallationId([9; 32]),
		intent: INTENT.to_string(),
		direction: FundingDirection::In,
	};
	session_key_from_entropy(&ENTROPY, &slot).expect("derives")
}

fn dot_quote() -> FundingQuote {
	FundingQuote {
		quote_id: "native:DOT:pool".to_string(),
		send_amount: SEND,
		receive_amount: CREDIT,
		provider_fee: 0,
		network_fee: 1_000,
		eta_secs: Some(60),
		expires_at: None,
		asset_decimals: 10,
	}
}

fn assignment(last_update: Option<FundingUpdate>) -> FundingAssignment {
	FundingAssignment {
		intent: INTENT.to_string(),
		direction: FundingDirection::In,
		amount: Some(CREDIT),
		expires_at: None,
		last_update,
		quote: Some(dot_quote()),
	}
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
	let waited = tokio::time::timeout(Duration::from_secs(5), async {
		while !done() {
			tokio::time::sleep(Duration::from_millis(2)).await;
		}
	})
	.await;
	assert!(waited.is_ok(), "timed out waiting for {what}");
}

fn inbound(update: FundingInboundUpdate) -> FundingUpdate {
	FundingUpdate::In(update)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_top_up_runs_from_the_deposit_address_to_delivered() {
	let Harness { chain, sessions, route } = harness();
	let link = NativeLink(route);
	let key = key();

	assert!(link.assign(assignment(None)).await);
	until("the deposit instructions", || sessions.reports().len() >= 2).await;
	assert_eq!(
		sessions.reports(),
		vec![
			inbound(FundingInboundUpdate::AwaitingPayment),
			inbound(FundingInboundUpdate::Deposit {
				deposit: FundingDeposit::Crypto {
					address: product_public_key_to_address(key.public),
					network: "polkadot".to_string(),
					asset: "DOT".to_string(),
					amount: SEND,
					decimals: 10,
					exact: false,
					uri: None,
					expires_at: None,
				},
			}),
		]
	);

	chain.with(|state| {
		state.deposit = SEND;
		state.landing = 10_049_999;
	});
	until("delivery", || !sessions.is_serving(INTENT)).await;

	assert_eq!(chain.converted(), vec![(NativeToken::Dot, Tier::Pool, SEND, CREDIT)]);
	assert_eq!(
		sessions.reports()[2..],
		[
			inbound(FundingInboundUpdate::PaymentReceived { finalized: true, mismatch: None }),
			inbound(FundingInboundUpdate::Converting),
			inbound(FundingInboundUpdate::Crediting { top_up_id: key.public, amount: 10_040_000 }),
			inbound(FundingInboundUpdate::Delivered),
		]
	);
	assert_eq!(
		sessions.top_ups(),
		vec![HostPaymentTopUpRequest {
			into: None,
			amount: 10_040_000,
			source: PaymentTopUpSource::PrivateKey { sr25519_secret_key: *key.secret },
			id: key.public,
			funding: Some(INTENT.to_string()),
		}]
	);
}

/// A key that has signed is past its conversion, whatever the restart lost:
/// the route waits for the CASH on People and credits it, never converting
/// again or repeating where to pay.
#[tokio::test(flavor = "multi_thread")]
async fn a_resumed_session_credits_without_converting_again() {
	let Harness { chain, sessions, route } = harness();
	chain.with(|state| state.nonce = 1);
	let link = NativeLink(route);

	assert!(link.assign(assignment(Some(inbound(FundingInboundUpdate::Converting)))).await);
	tokio::time::sleep(Duration::from_millis(30)).await;
	assert!(sessions.reports().is_empty());

	chain.with(|state| state.on_people = CREDIT);
	until("delivery", || !sessions.is_serving(INTENT)).await;

	assert!(chain.converted().is_empty());
	assert_eq!(sessions.top_ups().len(), 1);
	assert_eq!(sessions.reports().last(), Some(&inbound(FundingInboundUpdate::Delivered)));
}

/// A short deposit is waited on: the route converts only once the key holds
/// what the quote asked.
#[tokio::test(flavor = "multi_thread")]
async fn a_short_deposit_waits_for_the_rest() {
	let Harness { chain, sessions, route } = harness();
	let link = NativeLink(route);
	chain.with(|state| state.deposit = SEND - 1);

	assert!(link.assign(assignment(None)).await);
	tokio::time::sleep(Duration::from_millis(40)).await;
	assert!(chain.converted().is_empty());

	chain.with(|state| {
		state.deposit = SEND;
		state.landing = CREDIT;
	});
	until("delivery", || !sessions.is_serving(INTENT)).await;
	assert_eq!(chain.converted().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_stops_a_session_until_its_deposit_arrives() {
	let Harness { chain, sessions, route } = harness();
	let link = NativeLink(route);

	assert!(link.assign(assignment(None)).await);
	until("the deposit instructions", || sessions.reports().len() >= 2).await;
	assert_eq!(link.cancel(INTENT.to_string()).await, Some(FundingCancelAnswer::Stopped));

	let Harness { chain: paid_chain, sessions: paid_sessions, route: paid_route } = harness();
	let paid = NativeLink(paid_route);
	paid_chain.with(|state| state.deposit = 1);
	assert!(paid.assign(assignment(None)).await);
	until("the deposit instructions", || paid_sessions.reports().len() >= 2).await;
	tokio::time::sleep(Duration::from_millis(30)).await;
	assert!(matches!(
		paid.cancel(INTENT.to_string()).await,
		Some(FundingCancelAnswer::Refused { .. })
	));

	// The stopped session neither converts nor reports again.
	chain.with(|state| state.deposit = SEND);
	tokio::time::sleep(Duration::from_millis(30)).await;
	assert!(chain.converted().is_empty());
	assert_eq!(sessions.reports().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_withdrawal_or_a_foreign_quote_fails_the_session() {
	for assignment in [
		FundingAssignment { direction: FundingDirection::Out, ..assignment(None) },
		FundingAssignment {
			quote: Some(FundingQuote { quote_id: "q_other".to_string(), ..dot_quote() }),
			..assignment(None)
		},
	] {
		let Harness { sessions, route, .. } = harness();
		assert!(NativeLink(route).assign(assignment).await);
		until("the failure", || !sessions.is_serving(INTENT)).await;
		assert!(matches!(sessions.reports()[..], [FundingUpdate::Failed { .. }]));
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn quotes_price_served_tokens_on_the_stacks_network_with_no_provider_fee() {
	let Harness { route, .. } = harness();
	let ask = |asset: &str, network: Option<&str>, direction| FundingQuoteAsk {
		direction,
		rail: FundingRail::Crypto,
		asset: asset.to_string(),
		network: network.map(str::to_string),
		amount: CREDIT,
		country: None,
	};

	let FundingQuoteAnswer::Quoted { quote } =
		route.quote_ask(ask("dotusd", Some("polkadot"), FundingDirection::In)).await
	else {
		panic!("dotUSD is served");
	};
	assert_eq!(quote.quote_id, "native:dotUSD:direct");
	assert_eq!((quote.send_amount, quote.receive_amount), (CREDIT + 50_000, CREDIT));
	assert_eq!((quote.provider_fee, quote.network_fee, quote.asset_decimals), (0, 50_000, 6));
	assert!(quote.expires_at.is_some());

	let unavailable = FundingQuoteAnswer::Refused { reason: FundingQuoteRefusal::Unavailable };
	for refused in [
		ask("USDT", Some("polkadot"), FundingDirection::In),
		ask("DOT", Some("ethereum"), FundingDirection::In),
		ask("DOT", Some("polkadot"), FundingDirection::Out),
		ask("BTC", None, FundingDirection::In),
	] {
		assert_eq!(route.quote_ask(refused).await, unavailable);
	}
}

#[test]
fn quote_ids_name_the_token_and_tier() {
	let route = QuotedRoute { token: NativeToken::Usdt, tier: Tier::Psm { fee_rate: 5_000 } };
	assert_eq!(route.quote_id(), "native:USDT:psm-5000");
	assert_eq!(QuotedRoute::from_quote_id(&route.quote_id()), Some(route));
	for other in ["native:USDT", "native:USDT:psm-5000:x", "fundtest:USDT:pool", "native:BTC:pool"] {
		assert_eq!(QuotedRoute::from_quote_id(other), None);
	}
}
