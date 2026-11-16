//! The native route's reads against the live Paseo Next Asset Hub and People.
//! Ignored by default; run with
//! `cargo test -p truapi --lib native_funding::live -- --ignored --nocapture`.
//!
//! Read-only: they check what the route serves and price each token, and
//! neither sign nor submit anything.

use std::sync::Arc;

use truapi_provider::{ChainSource, EmbeddedChainProvider, platform::ChainProvider};

use super::*;
use crate::{
	chain::tokio_spawner,
	chain_runtime::{RuntimeChainProvider, RuntimeFailure},
	platform::JsonRpcConnection,
};

const ASSET_HUB_WS: &str = "wss://paseo-asset-hub-next-rpc.polkadot.io";
const PEOPLE_WS: &str = "wss://paseo-people-next-system-rpc.polkadot.io";
const ASSET_HUB: [u8; 32] = hex_literal("4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a");
const PEOPLE: [u8; 32] = hex_literal("4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48");
const PAYMENT: StackPayment =
	StackPayment { asset_hub_para_id: 1500, people_para_id: 1502, asset_id: 50_000_413 };

const fn hex_literal(hex: &str) -> [u8; 32] {
	let bytes = hex.as_bytes();
	let mut out = [0; 32];
	let mut index = 0;
	while index < 32 {
		out[index] = nibble(bytes[2 * index]) << 4 | nibble(bytes[2 * index + 1]);
		index += 1;
	}
	out
}

const fn nibble(byte: u8) -> u8 {
	match byte {
		b'0'..=b'9' => byte - b'0',
		_ => byte - b'a' + 10,
	}
}

struct LiveProvider(EmbeddedChainProvider);

#[async_trait::async_trait]
impl RuntimeChainProvider for LiveProvider {
	async fn connect(
		&self,
		genesis_hash: Vec<u8>,
	) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
		let genesis: [u8; 32] = genesis_hash.try_into().expect("a 32-byte genesis hash");
		let connection = self.0.connect(genesis).await.map_err(|error| {
			RuntimeFailure::unavailable_with_reason("live_connect", format!("{error:?}"))
		})?;
		Ok(Arc::from(connection))
	}
}

fn live_chain() -> LiveNativeChain {
	let source = |url: &str| ChainSource::rpc_node(url.parse().expect("a URL"));
	let provider = EmbeddedChainProvider::builder()
		.chain(ASSET_HUB, source(ASSET_HUB_WS))
		.chain(PEOPLE, source(PEOPLE_WS))
		.build();
	let chain = ChainRuntime::new(Arc::new(LiveProvider(provider)), tokio_spawner());
	let versions = Arc::new(crate::chain::FixedExtensionVersions::one_chain(H256(ASSET_HUB), 0));
	let extrinsics = Arc::new(ExtrinsicService::over_chain(chain.clone(), versions));
	LiveNativeChain::new(chain, extrinsics, ASSET_HUB, PEOPLE, PAYMENT)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs network access to Paseo Next"]
async fn the_live_chain_serves_and_prices_tokens() {
	let chain = live_chain();
	let tokens = chain.tokens_in().await.expect("reads what it serves");
	println!("served: {:?}", tokens.iter().map(|token| token.symbol()).collect::<Vec<_>>());
	assert!(tokens.contains(&NativeToken::DotUsd), "the payment asset is always served");

	for token in tokens {
		let price = chain.price_in(token, 10_000_000).await;
		println!("{}: {price:?}", token.symbol());
		let price = price.expect("prices a served token");
		assert!(price.send > price.network_fee && price.network_fee > 0);
	}
	let empty = chain.balance(NativeToken::DotUsd, [0x42; 32]).await.expect("reads a balance");
	let on_people = chain.cash_on_people([0x42; 32]).await.expect("reads People");
	let nonce = chain.nonce([0x42; 32]).await.expect("reads a nonce");
	assert_eq!((empty, on_people, nonce), (0, 0, 0));
}

/// Each served token's conversion builds against the live runtime, is
/// priced by it, and reaches its dry run, which an empty key fails on funds
/// rather than on the call's shape.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs network access to Paseo Next"]
async fn each_conversion_builds_prices_and_dry_runs_live() {
	let chain = live_chain();
	let reads = Reads { at: chain.at(chain.asset_hub).await.expect("reads Asset Hub") };
	let transfer = chain.cash_transfer(&reads).await.expect("reads the transfer");
	println!("CASH moves to People by {transfer:?}");
	let empty_key = [0x42; 32];
	for token in chain.tokens_in().await.expect("reads what it serves") {
		let plan = chain.plan(&reads, token).await.expect("plans").expect("served");
		let leg = chain.leg(transfer, empty_key, 10_000_000, 11_000);
		let deposit = chain.price_in(token, 10_000_000).await.expect("priced").send;
		let (fees, dispatch, weight) =
			chain.fees(&reads, &plan, &leg, deposit).await.expect("fees read");
		let call = chain
			.conversion_call(&reads, &plan, &leg, deposit, fees, dispatch, weight)
			.await
			.expect("builds")
			.expect("the deposit covers the fees");
		let refused = reads.dry_run(empty_key, call.encoded()).await;
		println!("{}: fees {fees}, dispatch {dispatch}, {weight:?}, dry run {refused:?}", token.symbol());
		let reason = refused.expect_err("an empty key cannot convert").reason;
		assert!(reason.contains("would refuse"), "{reason}");
	}
	println!("People fee: {}", chain.people_fee(&chain.leg(transfer, empty_key, 10_000_000, 0), 10_000_000).await);
}
