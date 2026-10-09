//! Live People-chain checks for the native allowance chain reads.
//!
//! Ignored by default: these need network access and a reachable testnet, so
//! `cargo test` stays offline and deterministic. Run them explicitly:
//!
//! ```bash
//! cargo +nightly test -p truapi-host-cli --test live_people_chain -- --ignored --nocapture
//! ```
//!
//! `TRUAPI_LIVE_PEOPLE_WS` overrides the endpoint. The default is the
//! `paseo-next-v2` People chain, matching `network.rs`.
//!
//! These read chain state only; nothing here submits an extrinsic.

use std::{
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

use truapi::statement_allowance::{
	self as alloc, ChainContextCache, collection::PersonhoodCollection,
};

/// Default People-chain endpoint, kept in step with `network.rs`.
const DEFAULT_PEOPLE_WS: &str = "wss://paseo-people-next-system-rpc.polkadot.io";

/// The `paseo-next-v2` preset's `network_suffix`, kept in step with `network.rs`
/// (the binary crate exposes no library for tests to read it from).
const DEFAULT_NETWORK_SUFFIX: &str = "paseo";

/// A genesis hash no chain will report, standing in for a host whose configured
/// constant has gone stale after a testnet wipe.
const STALE_CONFIGURED_GENESIS: [u8; 32] = [0xff; 32];

fn people_ws() -> String {
	std::env::var("TRUAPI_LIVE_PEOPLE_WS").unwrap_or_else(|_| DEFAULT_PEOPLE_WS.to_string())
}

async fn connect() -> alloc::rpc::RpcClient {
	alloc::rpc::RpcClient::connect(&people_ws())
		.await
		.expect("connect to the live People chain")
}

/// A live client scoped to the stand-in stale genesis, which is what the cache
/// keys by.
async fn stale_scoped_client() -> alloc::ChainClient {
	alloc::ChainClient::new(connect().await, STALE_CONFIGURED_GENESIS)
}

fn current_period() -> u32 {
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("system clock after UNIX epoch")
		.as_secs();
	alloc::slot::current_period(now)
}

/// The preset's network suffix is what the reserved `uid.<suffix>` and
/// `peopl.<suffix>` derivations end in, so it has to be the suffix the chain
/// itself scopes People contexts with, or the CLI derives a person no other
/// host on this network recognises.
#[tokio::test]
#[ignore = "needs network access to a live People chain"]
async fn network_suffix_matches_the_preset() {
	let rpc = connect().await;
	let live = alloc::slot::read_network_suffix(&rpc)
		.await
		.expect("read the live network suffix");
	assert_eq!(
		String::from_utf8(live).expect("network suffix is UTF-8"),
		DEFAULT_NETWORK_SUFFIX,
		"the paseo-next-v2 preset's network suffix must match the chain"
	);
}

/// The genesis hash signed into allowance extrinsics must be the one the chain
/// reports, not the caller's constant, and the entry must still be keyed by that
/// constant so the cache actually hits.
#[tokio::test]
#[ignore = "needs network access to a live People chain"]
async fn chain_context_reports_the_chains_genesis_and_caches_by_the_configured_hash() {
	let rpc = connect().await;
	let live = alloc::fetch_genesis_hash(&rpc).await.expect("read the live genesis hash");
	assert_ne!(
		live, STALE_CONFIGURED_GENESIS,
		"the stand-in stale hash must not collide with the real chain"
	);

	let cache = ChainContextCache::default();
	let client = alloc::ChainClient::new(connect().await, STALE_CONFIGURED_GENESIS);
	let first = cache.get(&client).await.expect("a stale configured genesis is not fatal");

	assert_eq!(
		first.state.genesis_hash, live,
		"CheckGenesis is signed over this; it must come from the chain"
	);
	assert!(first.state.spec_version > 0);
	println!(
		"live People chain: spec_version={} transaction_version={} genesis=0x{}",
		first.state.spec_version,
		first.state.transaction_version,
		hex::encode(live)
	);

	let second = cache.get(&client).await.expect("second read succeeds");
	assert!(
		Arc::ptr_eq(&first.metadata, &second.metadata),
		"second read re-downloaded metadata; the entry is keyed by the wrong hash"
	);
}

/// `scan_slot_excluding` must answer for a live period whatever the table's
/// occupancy — never erroring — since that single answer is what lets the steady
/// state skip ring resolution.
#[tokio::test]
#[ignore = "needs network access to a live People chain"]
async fn scanning_a_live_period_answers_without_erroring() {
	let rpc = connect().await;
	let cache = ChainContextCache::default();
	let chain = cache
		.get(&stale_scoped_client().await)
		.await
		.expect("read the live chain context");
	let period = current_period();
	let network_suffix = alloc::slot::read_network_suffix(&rpc)
		.await
		.expect("read the live network suffix");

	// Entropy and target are throwaway: no alias derived from them owns a slot,
	// so the scan must offer a free one or report the table full — never error.
	let selection = alloc::slot::scan_slot_excluding(
		&rpc,
		&chain.metadata,
		alloc::slot::SlotScan {
			collection: PersonhoodCollection::LitePeople,
			entropy: [0x11; 32],
			network_suffix: &network_suffix,
			period,
			target: &[0x22; 32],
			excluded: &[],
			reuse_existing: true,
		},
	)
	.await
	.expect("scanning a live period is not an error");

	assert!(
		!matches!(selection, alloc::slot::SlotSelection::AlreadyAllocated(_)),
		"a throwaway target cannot already hold a slot: {selection:?}"
	);
	println!("scanned live period {period}: {selection:?}");
}

/// The live runtime must still expose the metadata shape the allowance path
/// decodes. The offline fixture is pinned to one spec version, so this is what
/// catches a runtime upgrade that moves the `AsResources` extension.
#[tokio::test]
#[ignore = "needs network access to a live People chain"]
async fn live_metadata_still_exposes_the_allowance_extension_shape() {
	let cache = ChainContextCache::default();
	let chain = cache
		.get(&stale_scoped_client().await)
		.await
		.expect("read the live chain context");

	let register = chain
		.metadata
		.as_resources_variant_indices(
			"RegisterStatementStoreAllowance",
			PersonhoodCollection::LitePeople,
		)
		.expect("live runtime exposes RegisterStatementStoreAllowance");
	let claim = chain
		.metadata
		.as_resources_variant_indices("ClaimLongTermStorage", PersonhoodCollection::LitePeople)
		.expect("live runtime exposes ClaimLongTermStorage");
	let period_duration = alloc::slot::long_term_storage_period_duration(&chain.metadata)
		.expect("live runtime exposes Resources.LongTermStoragePeriodDuration");

	// Preferring V16 has to actually reach V16: the legacy RPC answers V14, which
	// declares no pipeline map at all, so a silent fallback would leave the
	// version unresolvable while looking fine.
	assert_eq!(
		chain.metadata.metadata_version(),
		16,
		"the runtime API should have served V16; a fallback to the legacy RPC would \
         resolve pipeline 0 by default and look identical"
	);
	println!(
		"live metadata V{} pipeline version {}",
		chain.metadata.metadata_version(),
		chain.metadata.extension_version()
	);

	// Indices alone are not enough. A runtime upgrade added a `revision` field to
	// `RegisterStatementStoreAllowance` while its index stayed at 2, so the
	// encoded payload went one field short and the runtime panicked in
	// `validate_transaction`. Assert the arity the encoders actually write.
	for (variant, encoded_fields) in
		[("RegisterStatementStoreAllowance", 4usize), ("ClaimLongTermStorage", 4usize)]
	{
		let declared = chain
			.metadata
			.as_resources_info_field_count(variant)
			.unwrap_or_else(|err| panic!("live runtime declares `{variant}`: {err}"));
		assert_eq!(
			declared, encoded_fields,
			"`{variant}`: the encoder writes {encoded_fields} fields, the live runtime declares {declared}"
		);
	}

	assert!(period_duration > 0);
	println!(
		"live spec {}: RegisterStatementStoreAllowance={register:?} ClaimLongTermStorage={claim:?} \
         long-term-storage period={period_duration}s",
		chain.state.spec_version,
	);
}

/// The renewal docs tell hosts one scheduled pass per period is enough because
/// an ended period's allowances stay active for `Resources.StmtStoreGraceWindow`.
/// That number is quoted in four places and read by no code, so this is what
/// notices if the runtime shrinks it and the guidance stops being true.
#[tokio::test]
#[ignore = "needs network access to a live People chain"]
async fn live_grace_window_still_leaves_a_full_period_of_slack() {
	let rpc = connect().await;
	let metadata = alloc::fetch_metadata(&rpc).await.expect("live People metadata");
	let grace = alloc::slot::statement_store_grace_window(&rpc, &metadata)
		.await
		.expect("the runtime declares a statement-store grace window");
	let period = alloc::slot::STATEMENT_STORE_PERIOD_SECONDS;

	assert!(
		u64::from(grace) >= period,
		"grace window is {grace}s, under one {period}s period: a host waking once \
         per period can now miss it, so the scheduling guidance in the host \
         READMEs and on next_statement_renewal_delay needs revisiting"
	);
	println!("live StmtStoreGraceWindow={grace}s");
}

/// Both personhood collections have to be real on the live chain: the identifier
/// must address an existing `Members.Collections` entry, the slot budget must be
/// declared, and the extension must name the collection.
///
/// This is the only part of the full-personhood path a device without full
/// personhood can prove. Registering into the `People` ring still needs a
/// full-personhood account, which needs attestation this test cannot perform.
#[tokio::test]
#[ignore = "needs network access to a live People chain"]
async fn both_personhood_collections_resolve_on_the_live_chain() {
	let rpc = connect().await;
	let cache = ChainContextCache::default();
	let chain = cache
		.get(&stale_scoped_client().await)
		.await
		.expect("read the live chain context");
	let at = rpc.finalized_head().await.expect("read the finalized head");

	for collection in PersonhoodCollection::ALL {
		// `Collections[identifier].ring_size` only decodes if the identifier
		// addresses a real collection, so this is what catches a wrong
		// identifier or wrong padding.
		let exponent = alloc::ring::read_ring_exponent(&rpc, &chain.metadata, collection, &at)
			.await
			.unwrap_or_else(|err| panic!("{collection} collection is absent on chain: {err}"));
		let slots = collection
			.slots_per_period(&rpc, &chain.metadata)
			.await
			.unwrap_or_else(|err| panic!("{collection} declares no slot budget: {err}"));
		let ring_index = alloc::ring::read_current_ring_index(&rpc, collection)
			.await
			.unwrap_or_else(|err| panic!("{collection} has no current ring: {err}"));
		let (_, variant) = chain
			.metadata
			.as_resources_variant_indices("RegisterStatementStoreAllowance", collection)
			.unwrap_or_else(|err| panic!("{collection} has no extension variant: {err}"));

		println!(
			"live {collection}: slots={slots} ring_exponent={exponent} \
             current_ring_index={ring_index} extension_variant={variant}"
		);
		assert!(slots > 0, "{collection} declares a zero slot budget");
	}

	// The pooled budget is what the fix delivers, so assert the two differ
	// rather than silently reading the same constant twice.
	let people = PersonhoodCollection::People
		.slots_per_period(&rpc, &chain.metadata)
		.await
		.expect("People slot budget");
	let lite = PersonhoodCollection::LitePeople
		.slots_per_period(&rpc, &chain.metadata)
		.await
		.expect("LitePeople slot budget");
	assert!(
		people > lite,
		"full personhood should carry the wider budget: People={people} LitePeople={lite}"
	);
	println!("live pooled budget={} (People {people} + LitePeople {lite})", people + lite);
}

#[tokio::test]
#[ignore = "needs network access to a live People chain"]
async fn dynamic_resources_values_resolve_on_the_live_chain() {
	let rpc = connect().await;
	let metadata = alloc::fetch_metadata(&rpc).await.expect("live People metadata");

	let cooldown = alloc::slot::replacement_cooldown(&rpc, &metadata)
		.await
		.expect("replacement cooldown view");
	let long_term_storage_claims =
		alloc::slot::long_term_storage_claims_per_period(&rpc, &metadata)
			.await
			.expect("long-term-storage claims view");

	assert!(cooldown > 0, "replacement cooldown must be positive");
	assert!(long_term_storage_claims > 0, "long-term-storage claims must be positive");
	println!(
		"live Resources views: replacement_cooldown={cooldown}s \
         long_term_storage_claims_per_period={long_term_storage_claims}"
	);
}
