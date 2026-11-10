//! Live Asset Hub checks for the PGAS claim path.
//!
//! Ignored by default; these need network access to paseo-next-v2's Asset Hub.
//!
//! ```sh
//! cargo +nightly test -p truapi-host-cli --test live_asset_hub -- --ignored --nocapture
//! ```
//!
//! Connects directly rather than through the host's `ChainProvider`, so the checks
//! are about the chain rather than about host wiring. The provider routes Asset Hub
//! now that the preset serves it as a role.

#[path = "common/bundled.rs"]
mod bundled;
#[path = "common/live.rs"]
mod live;

use truapi::statement_allowance::{self as alloc, collection::PersonhoodCollection, pgas};

/// `LIVE_ASSET_HUB_WS` points the dotNS checks at another Asset Hub, for
/// instance previewnet's.
fn asset_hub_ws() -> String {
	std::env::var("LIVE_ASSET_HUB_WS").unwrap_or_else(|_| live::paseo_next_v2_rpc("AssetHub"))
}

/// `LIVE_TLD`, or the bundled paseo-next-v2 stack's `tld`.
fn live_tld() -> String {
	std::env::var("LIVE_TLD").unwrap_or_else(|_| bundled::paseo_next_v2().tld)
}

/// Must stay active so the ignored live tests can resolve its mirrored roots.
const RING_INDEX: u32 = 1;
/// The ring this fixture's index belongs to.
const COLLECTION: PersonhoodCollection = PersonhoodCollection::LitePeople;

async fn asset_hub() -> (alloc::rpc::RpcClient, alloc::extension::Metadata) {
	let rpc = alloc::rpc::RpcClient::connect(&asset_hub_ws())
		.await
		.expect("connect to Asset Hub");
	let metadata = alloc::fetch_metadata(&rpc).await.expect("Asset Hub metadata");
	(rpc, metadata)
}

#[tokio::test]
#[ignore = "needs network access to a live Asset Hub"]
async fn live_asset_hub_reports_the_product_context_suffix() {
	let (rpc, _metadata) = asset_hub().await;
	let expected = live_tld();

	assert_eq!(
		alloc::slot::read_network_suffix(&rpc)
			.await
			.expect("read Asset Hub NetworkSuffix"),
		expected.as_bytes(),
	);
}

/// Both collections have a PGAS claim budget on the live Asset Hub: a full
/// person proves against the `People` ring and must not be scanned against the
/// light one's share.
#[tokio::test]
#[ignore = "needs network access to a live Asset Hub"]
async fn live_asset_hub_declares_both_pgas_claim_budgets() {
	let (_rpc, metadata) = asset_hub().await;

	assert_eq!(metadata.metadata_version(), 16, "the runtime API should serve V16");
	assert!(
		alloc::slot::max_pgas_claims(&metadata, PersonhoodCollection::LitePeople).unwrap() > 0,
		"a lite person must be allowed at least one claim per day"
	);
	// A full person has their own budget on Asset Hub; scanning them against the
	// light one's share would hide claims they are entitled to.
	let people = alloc::slot::max_pgas_claims(&metadata, PersonhoodCollection::People)
		.expect("Asset Hub declares a full-person claim budget");
	let lite = alloc::slot::max_pgas_claims(&metadata, PersonhoodCollection::LitePeople).unwrap();
	println!("live PGAS claims per day: People={people} LitePeople={lite}");
	assert!(people >= lite, "People={people} LitePeople={lite}");
}

/// Asset Hub learns People's rings through `MembersSubscriber`. A claim can only
/// be authorized against a revision it has imported, so decode that storage for
/// real: the record projection is the part no offline fixture covers.
#[tokio::test]
#[ignore = "needs network access to a live Asset Hub and People chain"]
async fn live_asset_hub_has_imported_the_current_people_ring_revision() {
	let (asset_hub_rpc, asset_hub_metadata) = asset_hub().await;
	let people_rpc = alloc::rpc::RpcClient::connect(&live::paseo_next_v2_rpc("People"))
		.await
		.expect("connect to the People chain");
	let people_metadata = alloc::fetch_metadata(&people_rpc).await.expect("People metadata");
	let at = people_rpc.finalized_head().await.expect("finalized head");
	let revision =
		alloc::ring::read_ring_revision(&people_rpc, &people_metadata, COLLECTION, RING_INDEX, &at)
			.await
			.expect("People reports a ring revision");
	println!("People ring {RING_INDEX} is at revision {revision}");

	pgas::await_ring_revision(
		&asset_hub_rpc,
		&asset_hub_metadata,
		COLLECTION,
		RING_INDEX,
		revision,
	)
	.await
	.expect("Asset Hub has imported the current revision");
}

/// A revision Asset Hub has moved past can never be verified, and has to be told
/// apart from one that simply has not arrived: the first means rebuild the proof,
/// the second means wait.
#[tokio::test]
#[ignore = "needs network access to a live Asset Hub"]
async fn live_asset_hub_reports_a_pruned_revision_rather_than_waiting() {
	let (rpc, metadata) = asset_hub().await;

	let err = pgas::await_ring_revision(&rpc, &metadata, COLLECTION, RING_INDEX, 1)
		.await
		.expect_err("revision 1 is long pruned");

	assert!(
		err.to_string().contains("pruned"),
		"a pruned revision should not be waited out: {err}"
	);
}

/// The already-funded check reads a real asset account. The test identity claimed
/// PGAS earlier, so it must read as funded, and an account that never claimed must
/// not — a check that answered the same either way would silently disable itself.
#[tokio::test]
#[ignore = "needs network access to a live Asset Hub"]
async fn live_asset_hub_reports_whether_an_account_holds_a_full_claim() {
	let (rpc, metadata) = asset_hub().await;

	let claim_amount = metadata.constant_u128("Pgas", "PgasClaimAmount").unwrap();
	let asset_id = metadata.constant_u32("Pgas", "PgasAssetId").unwrap();
	println!("PGAS asset {asset_id}, claim amount {claim_amount}");

	// The onboarded test identity's own account, credited by earlier claims.
	let funded: [u8; 32] =
		hex::decode("ba7ec9e74688af5ae483a3f2c9421443f02a688b2928a1e6f43cc06692c5293c")
			.unwrap()
			.try_into()
			.unwrap();
	assert!(
		pgas::holds_a_full_claim(&rpc, &metadata, &funded).await.expect("balance read"),
		"the test identity has claimed PGAS, so it should read as funded"
	);

	assert!(
		!pgas::holds_a_full_claim(&rpc, &metadata, &[0xcd; 32])
			.await
			.expect("balance read"),
		"an account that never claimed holds nothing"
	);
}

/// A revision the window skipped is as unreachable as one that fell off the front,
/// and has to report as pruned rather than waiting out the timeout.
///
/// The rule itself is covered offline, against a frozen copy of this window, by
/// `revision_status` and `a_skipped_revision_reports_as_pruned_against_captured_roots`.
/// What this adds is drift: it only passes while lite-people ring 5 still skips
/// revision 107 on the live chain, so it fails once the window moves on. That is
/// chain state changing, not the code breaking.
#[tokio::test]
#[ignore = "needs network access to a live Asset Hub"]
async fn live_asset_hub_reports_a_skipped_revision_as_pruned() {
	let (rpc, metadata) = asset_hub().await;

	let err = pgas::await_ring_revision(&rpc, &metadata, COLLECTION, 5, 107)
		.await
		.expect_err("revision 107 was skipped for ring 5");

	assert!(
		err.to_string().contains("pruned"),
		"a skipped revision should not be waited out: {err}"
	);
}

// ---------------------------------------------------------------------------
// dotNS gateway: the register_name authorization shape and the username reads.
// ---------------------------------------------------------------------------

use alloy_primitives::U256;
use alloy_sol_types::SolCall;
use truapi::{
	chain::{EvmContractCaller, EvmContractError, RuntimeApiCaller},
	host_logic::dotns_gateway::{
		DotnsError, DotnsRegistrar, DotnsTransport, classify_labels, discover_pop_controller,
		is_dotted_lite_username, is_pop_issued, label_available, namehash_under, resolve_labels,
	},
};

/// The `DotnsRegistrar` (ERC721) from the bundled paseo-next-v2 stack, the same
/// CREATE3 address on every network per dotns `DEPLOYMENTS.md`. Only used to
/// find an account that holds a settled name.
fn dotns_registrar() -> [u8; 20] {
	let stack = bundled::paseo_next_v2();
	let address = &stack.dotns.as_ref().expect("bundled stack lacks dotns")["registrar"];
	let mut bytes = [0u8; 20];
	hex::decode_to_slice(address.trim_start_matches("0x"), &mut bytes)
		.expect("invalid registrar address");
	bytes
}

/// A label minted through the public registrar on the checked network, whose
/// owner therefore has a settled `LabelStore`. `LIVE_MINTED_LABEL` overrides
/// the paseo-next-v2 default (`e2epoolns01`); `LIVE_TLD` names the network TLD.
fn minted_label() -> String {
	std::env::var("LIVE_MINTED_LABEL").unwrap_or_else(|_| "e2epoolns01".to_string())
}

/// `namehash(<minted label>.<tld>)`, the registrar's token id.
fn minted_node() -> [u8; 32] {
	let tld = live_tld();
	namehash_under(&namehash_under(&[0u8; 32], &tld), &minted_label())
}

/// `DotnsTransport` over plain RPC, the same two primitives the CLI uses.
struct PlainRpc(alloc::rpc::RpcClient);

#[async_trait::async_trait]
impl DotnsTransport for PlainRpc {
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		self.0
			.get_storage(&key)
			.await
			.map_err(|err| DotnsError::Storage(err.to_string()))
	}

	async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, EvmContractError> {
		EvmContractCaller::read(self, dest, &input).await
	}
}

#[async_trait::async_trait]
impl RuntimeApiCaller for PlainRpc {
	async fn call(&mut self, function: &str, input: &[u8]) -> Result<Vec<u8>, subxt::Error> {
		let output = self
			.0
			.call("state_call", serde_json::json!([function, format!("0x{}", hex::encode(input))]))
			.await
			.map_err(subxt::Error::other)?;
		let output = output
			.as_str()
			.ok_or_else(|| subxt::Error::other_str("state_call output is not a string"))?;
		hex::decode(output.trim_start_matches("0x")).map_err(subxt::Error::other)
	}
}

/// End to end over the same resolution steps the CLI and the in-core runtime
/// share: pallet storage → controller (directly, or through a dispatcher's
/// `TARGET()`) → registry →
/// store factory → the owner's `LabelStore` → bare labels. The owner is found
/// through the registrar, so this covers the warm path (a settled store, whose
/// labels carry the network TLD) rather than only pending claims.
#[tokio::test]
#[ignore = "needs network access to a live Asset Hub"]
async fn live_asset_hub_resolves_a_settled_store_over_dotns_discovery() {
	let (rpc, _metadata) = asset_hub().await;
	let mut transport = PlainRpc(rpc);

	let controller = discover_pop_controller(&mut transport)
		.await
		.expect("discovery")
		.expect("the gateway names a controller");
	println!("live DotnsPopController: 0x{}", hex::encode(controller));

	// ownerOf(namehash) on the ERC721 registrar. Eth-derived owners map back
	// to an AccountId32 by `0xEE` padding, which `account_to_h160` truncates.
	let owner_of = DotnsRegistrar::ownerOfCall { tokenId: U256::from_be_bytes(minted_node()) };
	let owner_output = transport
		.view(&dotns_registrar(), owner_of.abi_encode())
		.await
		.expect("registrar ownerOf");
	let owner = DotnsRegistrar::ownerOfCall::abi_decode_returns_validate(&owner_output)
		.expect("owner address");
	let mut account = [0xEE; 32];
	account[..20].copy_from_slice(owner.as_slice());

	let labels = resolve_labels(&mut transport, &controller, &account)
		.await
		.expect("resolve labels");
	println!("live labels of 0x{}: {labels:?}", hex::encode(owner));
	assert!(
		labels.iter().any(|label| *label == minted_label()),
		"the settled store label loses its network TLD: {labels:?}"
	);
	assert!(
		labels
			.iter()
			.all(|label| !label.contains('.') || is_dotted_lite_username(label)),
		"no TLD or subname survives (dotted lite names may): {labels:?}"
	);
	// A dotted survivor must also carry gateway provenance, so a subname under
	// a digit-only parent cannot pass as a lite name.
	for label in labels.iter().filter(|label| label.contains('.')) {
		assert!(
			is_pop_issued(&mut transport, &controller, label).await.expect("isPopIssued"),
			"dotted label {label:?} is not pop-issued"
		);
	}
	// The default owner's store holds far more than one `getLabels` page, so
	// this also proves the store is paged rather than read once.
	if std::env::var("LIVE_MINTED_LABEL").is_err() {
		assert!(
			labels.len() > 16,
			"expected the e2e pool store to span several pages, got {}",
			labels.len()
		);
	}
	let identity = classify_labels(&mut transport, &controller, &labels)
		.await
		.expect("classify labels");
	assert!(identity.is_some(), "labels classify into a username");
}

/// A reservation or registration for a name the registrar already minted can
/// never succeed, so the CLI asks the registrar first. `available` is read for
/// the name's node under the network TLD (`namehash(label.tld)`), the same node the
/// registrar minted.
#[tokio::test]
#[ignore = "needs network access to a live Asset Hub"]
async fn live_asset_hub_reports_registered_names_as_unavailable() {
	let (rpc, _metadata) = asset_hub().await;
	let mut transport = PlainRpc(rpc);
	let controller = discover_pop_controller(&mut transport)
		.await
		.expect("discovery")
		.expect("gateway deployed");

	assert!(
		!label_available(&mut transport, &controller, &minted_label())
			.await
			.expect("available view"),
		"a minted name is not available"
	);
	assert!(
		label_available(&mut transport, &controller, "truapihostrebasecheckzz")
			.await
			.expect("available view"),
		"an unminted name is available"
	);
}
