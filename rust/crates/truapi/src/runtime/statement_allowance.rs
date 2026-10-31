//! On-chain statement-store allowance registration (`set_statement_store_account`).
//!
//! Mirrors how an iOS/web client obtains statement-store allowance from the real
//! People chain: build the `Resources.set_statement_store_account` call, prove
//! personhood ring membership with the caller's registry-selected ring-VRF key,
//! and submit the resulting unsigned General (v5) extrinsic. Native only
//! (needs the `verifiable` prover and live chain reads).

mod authorizers;
pub mod collection;
pub mod extension;
pub mod extrinsic;
mod key_hash;
pub mod pgas;
pub mod proof;
pub mod renewal;
pub mod ring;
pub mod rpc;
mod signer;
pub mod slot;
#[cfg(test)]
mod test_fixtures;
/// Captured chain metadata for wallet allowance tests.
#[cfg(test)]
pub use test_fixtures::{
	ASSET_HUB_METADATA as ASSET_HUB_TEST_METADATA_BYTES, asset_hub as asset_hub_test_metadata,
	people as people_test_metadata, people_extrinsics as people_test_extrinsics,
	people_target as people_test_target,
};
mod view;
mod view_cache;

pub use authorizers::{AsPgasExtension, AsResourcesExtension, ExtrinsicTarget, RingProof};
pub use signer::{FixedPersonhoodSigner, PersonhoodSigner};
pub use view::ViewFunctionError;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
	time::Duration,
};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use futures::FutureExt;
use parity_scale_codec::{Decode, Encode};
use serde_json::{Value, json};
use sp_crypto_hashing::twox_128;
use thiserror::Error;
use tracing::{debug, warn};

use collection::PersonhoodCollection;
use subxt::utils::H256;

use crate::chain::ExtrinsicService;
use extension::{Metadata, MetadataError};
use ring::RingParams;
use rpc::RpcClient;
use slot::{SlotError, SlotSelection};

/// The pallet that grants statement-store and long-term-storage allowances.
const RESOURCES_PALLET: &str = "Resources";

/// Error while reading chain state, building allowance extrinsics, or waiting
/// for allowance authorization.
#[derive(Debug, Error)]
pub enum StatementAllowanceError {
	/// The selected wallet can no longer authorize this operation.
	#[error(transparent)]
	Authority(#[from] crate::runtime::authority::AuthorityError),
	/// JSON-RPC transport, request, subscription, or storage hex failure.
	#[error(transparent)]
	Rpc(#[from] rpc::RpcError),
	/// Runtime metadata was missing an expected pallet, call, type, extension,
	/// constant, or could not be decoded.
	#[error(transparent)]
	Metadata(#[from] extension::MetadataError),
	/// Chain RPC returned a value with an unexpected shape.
	#[error(transparent)]
	ChainState(#[from] ChainStateError),
	/// Ring lookup or ring storage decoding failed.
	#[error(transparent)]
	Ring(#[from] ring::RingError),
	/// Slot context, alias, or free-slot selection failed.
	#[error(transparent)]
	Slot(#[from] slot::SlotError),
	/// Ring-VRF proof construction failed.
	#[error(transparent)]
	Proof(#[from] proof::ProofError),
	/// Asset Hub PGAS claim failed.
	#[error(transparent)]
	Pgas(#[from] pgas::PgasError),
	/// Runtime view-function dispatch or response decoding failed.
	#[error(transparent)]
	ViewFunction(#[from] view::ViewFunctionError),
	/// The allowance extrinsic could not be built.
	#[error(transparent)]
	Extrinsic(crate::chain::ExtrinsicBuildError),
	/// Bulletin allowance polling timed out.
	#[error("timed out waiting for Bulletin authorization")]
	BulletinAuthorizationTimeout,
	/// The extended Bulletin authorization still cannot hold the payload.
	#[error("Bulletin authorization holds {remained_size} bytes after extension, {size} needed")]
	BulletinAuthorizationTooSmall {
		/// Payload size in bytes.
		size: u64,
		/// Bytes the extended authorization holds.
		remained_size: u64,
	},
}

/// Error while decoding generic chain state used by allowance registration.
#[derive(Debug, Error)]
pub enum ChainStateError {
	/// `chain_getBlockHash(0)` did not return a hex string.
	#[error("chain_getBlockHash returned non-string")]
	GenesisHashNotString,
	/// `chain_getBlockHash(0)` returned invalid hex.
	#[error("genesis hex: {0}")]
	GenesisHex(#[source] hex::FromHexError),
	/// Decoded genesis hash was not 32 bytes.
	#[error("genesis hash is {len} bytes, expected 32")]
	GenesisHashLength {
		/// Actual decoded length.
		len: usize,
	},
	/// Runtime JSON lacked an expected u32 field.
	#[error("missing/invalid {field}")]
	MissingU32Field {
		/// Field name.
		field: &'static str,
	},
	/// Bulletin authorization storage failed to decode.
	#[error("authorization: {0}")]
	AuthorizationDecode(#[source] parity_scale_codec::Error),
	/// `chain_getHeader` did not contain a block number string.
	#[error("chain_getHeader returned no number")]
	HeaderNumberMissing,
	/// `chain_getHeader.number` was not valid hex.
	#[error("chain_getHeader number: {0}")]
	HeaderNumberParse(#[source] std::num::ParseIntError),
}

/// Metadata version to ask the runtime for: the first that carries a
/// transaction-extension version map.
const PREFERRED_METADATA_VERSION: u32 = 16;

/// Fetch and decode the runtime metadata, preferring V16.
///
/// The legacy `state_getMetadata` RPC answers with whatever version the node
/// serves — V14 on paseo-next-v2 — and V14 declares no transaction-extension
/// version map at all, so the pipeline version cannot be resolved from it. V16 is
/// only reachable through the `Metadata_metadata_at_version` runtime API, so ask
/// for it first and fall back for runtimes that do not offer it.
pub async fn fetch_metadata(rpc: &RpcClient) -> Result<Metadata, StatementAllowanceError> {
	match fetch_metadata_at_version(rpc, PREFERRED_METADATA_VERSION).await {
		Ok(Some(metadata)) => return Ok(metadata),
		Ok(None) => {
			debug!(
				version = PREFERRED_METADATA_VERSION,
				"runtime does not offer this metadata version; using state_getMetadata"
			);
		},
		Err(reason) => {
			debug!(
				version = PREFERRED_METADATA_VERSION,
				%reason,
				"metadata runtime call failed; using state_getMetadata"
			);
		},
	}
	fetch_legacy_metadata(rpc).await
}

/// Ask the runtime for one metadata version through
/// `Metadata_metadata_at_version`, which answers `Option<OpaqueMetadata>`.
async fn fetch_metadata_at_version(
	rpc: &RpcClient,
	version: u32,
) -> Result<Option<Metadata>, StatementAllowanceError> {
	let argument = format!("0x{}", hex::encode(version.encode()));
	let value = rpc
		.call("state_call", json!(["Metadata_metadata_at_version", argument]))
		.await?;
	let hex_str = value.as_str().ok_or(MetadataError::MetadataResultNotString)?;
	let bytes = hex::decode(hex_str.strip_prefix("0x").unwrap_or(hex_str))
		.map_err(MetadataError::MetadataHex)?;
	let Some(opaque) =
		Option::<Vec<u8>>::decode(&mut &bytes[..]).map_err(MetadataError::OpaqueMetadata)?
	else {
		return Ok(None);
	};
	Metadata::decode(&opaque).map(Some)
}

/// Fetch and decode the runtime metadata through the legacy `state_getMetadata`.
async fn fetch_legacy_metadata(rpc: &RpcClient) -> Result<Metadata, StatementAllowanceError> {
	let value = rpc.call("state_getMetadata", json!([])).await?;
	let hex_str = value.as_str().ok_or(MetadataError::MetadataResultNotString)?;
	let bytes = hex::decode(hex_str.strip_prefix("0x").unwrap_or(hex_str))
		.map_err(MetadataError::MetadataHex)?;
	// `state_getMetadata` may return either the raw `RuntimeMetadataPrefixed`
	// (starts with the `meta` magic) or an OpaqueMetadata wrapper
	// (`Vec<u8>` = compact(len) ‖ bytes). Strip the wrapper only when present.
	const META_MAGIC: [u8; 4] = *b"meta";
	if bytes.get(..4) == Some(&META_MAGIC) {
		Metadata::decode(&bytes)
	} else {
		let inner = Vec::<u8>::decode(&mut &bytes[..]).map_err(MetadataError::OpaqueMetadata)?;
		Metadata::decode(&inner)
	}
}

/// Read the chain's runtime `(specVersion, transactionVersion)`.
pub async fn fetch_runtime_version(rpc: &RpcClient) -> Result<(u32, u32), StatementAllowanceError> {
	let runtime = rpc.call("state_getRuntimeVersion", json!([])).await?;
	Ok((json_u32(&runtime, "specVersion")?, json_u32(&runtime, "transactionVersion")?))
}

/// Read the chain's genesis block hash.
pub async fn fetch_genesis_hash(rpc: &RpcClient) -> Result<[u8; 32], StatementAllowanceError> {
	let genesis_hex = rpc.call("chain_getBlockHash", json!([0])).await?;
	let genesis_str = genesis_hex.as_str().ok_or(ChainStateError::GenesisHashNotString)?;
	let genesis = hex::decode(genesis_str.strip_prefix("0x").unwrap_or(genesis_str))
		.map_err(ChainStateError::GenesisHex)?;
	let len = genesis.len();
	genesis
		.try_into()
		.map_err(|_| ChainStateError::GenesisHashLength { len }.into())
}

/// An RPC client together with the genesis hash the host routes it by.
///
/// Bundled because the two have to agree and nothing at a call site shows when
/// they do not: that hash keys the chain-context cache and is what a reported
/// divergence is measured against, so pairing a connection with another chain's
/// hash silently attributes one chain's metadata to another.
pub struct ChainClient {
	rpc: RpcClient,
	configured_genesis_hash: [u8; 32],
}

impl ChainClient {
	/// Scope `rpc` to the chain the host routes by `configured_genesis_hash`.
	pub fn new(rpc: RpcClient, configured_genesis_hash: [u8; 32]) -> Self {
		Self { rpc, configured_genesis_hash }
	}

	/// The underlying client, for reads that do not need the chain's identity.
	pub fn rpc(&self) -> &RpcClient {
		&self.rpc
	}

	/// Where to build extrinsics for this chain: `extrinsics`, over the
	/// genesis the host configured.
	pub fn extrinsic_target<'a>(&self, extrinsics: &'a ExtrinsicService) -> ExtrinsicTarget<'a> {
		ExtrinsicTarget { extrinsics, genesis: H256(self.configured_genesis_hash) }
	}
}

/// Runtime metadata for one chain, with the runtime it was read from.
#[derive(Clone)]
pub struct ChainContext {
	/// Decoded runtime metadata.
	pub metadata: Arc<Metadata>,
	/// Spec version the metadata belongs to.
	spec_version: u32,
	/// Genesis hash the chain reported when the metadata was read.
	reported_genesis_hash: [u8; 32],
}

/// Runtime metadata cached per chain.
///
/// Metadata is fixed for a given runtime, and a full `state_getMetadata` response
/// is large, so entries are keyed by genesis hash and revalidated with a
/// concurrent `state_getRuntimeVersion` + `chain_getBlockHash(0)` — two small
/// requests in place of a metadata download on every allowance call.
///
/// One entry per chain the host is configured for, so the map needs no eviction
/// policy: it is bounded by that chain set, not by call volume.
#[derive(Default)]
pub struct ChainContextCache {
	entries: Mutex<HashMap<[u8; 32], ChainContext>>,
	/// Held across a miss so concurrent callers do not each download the same
	/// metadata. `chain_runtime` shares one in-flight future for this, which is
	/// tidier, but `Shared` needs a `Clone` output and `StatementAllowanceError`
	/// is not — so the waiters re-check the entry instead.
	downloads: futures::lock::Mutex<()>,
}

impl ChainContextCache {
	/// Metadata for the chain reached over `rpc`, read from the chain only
	/// when no entry describes the chain as it is now.
	///
	/// The client carries the caller's identity for the chain, the hash it
	/// routes connections by, and that keys the cache. An entry is reused only
	/// when both the spec version **and** the reported genesis hash still
	/// match, so a chain wiped and redeployed from the same runtime is read
	/// again. Extrinsics are always signed with the configured genesis, never
	/// the reported one; a divergence is logged, since such extrinsics will be
	/// rejected until the host's chain configuration is refreshed.
	pub async fn get(&self, client: &ChainClient) -> Result<ChainContext, StatementAllowanceError> {
		let rpc = client.rpc();
		let configured_genesis_hash = client.configured_genesis_hash;
		let ((spec_version, _transaction_version), genesis_hash) =
			futures::try_join!(fetch_runtime_version(rpc), fetch_genesis_hash(rpc))?;
		if let Some(cached) = self.cached(configured_genesis_hash, spec_version, genesis_hash) {
			return Ok(cached);
		}

		let _download = self.downloads.lock().await;
		// Another caller may have finished the download while this one waited.
		if let Some(cached) = self.cached(configured_genesis_hash, spec_version, genesis_hash) {
			return Ok(cached);
		}
		if genesis_hash != configured_genesis_hash {
			warn!(
				configured = %hex::encode(configured_genesis_hash),
				reported = %hex::encode(genesis_hash),
				"chain reports a different genesis than the host configured; extrinsics signed \
				 for the configured one will be rejected"
			);
		}
		let context = ChainContext {
			metadata: Arc::new(fetch_metadata(rpc).await?),
			spec_version,
			reported_genesis_hash: genesis_hash,
		};
		self.entries
			.lock()
			.expect("chain context cache mutex poisoned")
			.insert(configured_genesis_hash, context.clone());
		Ok(context)
	}

	fn cached(
		&self,
		configured_genesis_hash: [u8; 32],
		spec_version: u32,
		genesis_hash: [u8; 32],
	) -> Option<ChainContext> {
		self.entries
			.lock()
			.expect("chain context cache mutex poisoned")
			.get(&configured_genesis_hash)
			.filter(|cached| {
				cached.spec_version == spec_version && cached.reported_genesis_hash == genesis_hash
			})
			.cloned()
	}
}

/// Read a u32 field from a JSON object.
fn json_u32(value: &Value, field: &'static str) -> Result<u32, StatementAllowanceError> {
	value
		.get(field)
		.and_then(Value::as_u64)
		.and_then(|v| u32::try_from(v).ok())
		.ok_or_else(|| ChainStateError::MissingU32Field { field }.into())
}

/// A slot a caller's own scan already chose, and what claiming it costs.
///
/// The distinction is not cosmetic: claiming a free slot revokes nothing, while a
/// takeover revokes somebody's allowance. Registration limits itself to one
/// revocation per call, so it has to know which kind it was handed.
#[derive(Clone)]
enum Preselected {
	/// A slot observed free; claiming it revokes nothing.
	Free(u32),
	/// A live slot the caller judged replaceable.
	Takeover(u32),
}

impl Preselected {
	fn seq(self) -> u32 {
		match self {
			Self::Free(seq) | Self::Takeover(seq) => seq,
		}
	}
}

/// Result of a statement-store allowance registration attempt.
#[derive(Debug)]
pub enum RegistrationOutcome {
	/// The extrinsic reached a block and the slot entry was verified at that
	/// block: the target now holds slot `seq`.
	Registered {
		/// Block hash the extrinsic landed in.
		block_hash: String,
		/// Claimed slot sequence.
		seq: u32,
		/// Ring index the proof was built against.
		ring_index: u32,
		/// Collection whose alias space holds the slot.
		collection: PersonhoodCollection,
	},
	/// The target already held a slot this period; nothing submitted.
	AlreadyAllocated {
		/// Existing slot sequence.
		seq: u32,
		/// Collection whose alias space holds the slot.
		collection: PersonhoodCollection,
	},
}

struct RegistrationParams<'a> {
	target: &'a [u8; 32],
	period: u32,
	network_suffix: &'a [u8],
	ring: &'a RingParams,
	reuse_existing: bool,
	// Reuse the caller's first scan; duplicate submissions still need a fresh scan.
	preselected: Option<Preselected>,
	// A batch must not revoke allowances it just registered for earlier targets.
	protected: &'a [u32],
}

/// Result of a long-term storage claim attempt.
pub enum LongTermStorageOutcome {
	/// The extrinsic reached a block; the target should receive Bulletin
	/// authorization once XCM/chain propagation completes.
	Claimed {
		/// Block hash the extrinsic landed in.
		block_hash: String,
		/// Claimed counter within the long-term storage period.
		counter: u8,
		/// Ring index the proof was built against.
		ring_index: u32,
	},
}

/// Everything one long-term-storage claim needs.
pub struct LongTermStorageClaim<'a> {
	/// People connection the claim is submitted on.
	pub rpc: &'a RpcClient,
	/// People runtime metadata.
	pub metadata: &'a Metadata,
	/// Where the claim extrinsic is built.
	pub target_chain: ExtrinsicTarget<'a>,
	/// Authorizes synchronous personhood operations for the selected wallet.
	pub signer: &'a dyn PersonhoodSigner,
	/// People suffix used for the product-scoped alias and proof.
	pub network_suffix: &'a [u8],
	/// Account whose Bulletin allowance is authorized.
	pub target: &'a [u8; 32],
	/// People long-term-storage period.
	pub period: u32,
	/// Ring the membership proof is built against.
	pub ring: &'a RingParams,
}

/// Bulletin authorization state for one account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BulletinAllowanceInfo {
	/// Number of preimage bytes that remain available.
	pub remained_size: u64,
	/// Number of preimage submissions that remain available.
	pub remained_transactions: u32,
	/// Block at which the allowance expires.
	pub expires_in: u32,
	/// Block at which this allowance snapshot was fetched.
	pub fetched_at: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
enum BulletinAuthorizationScope {
	Account([u8; 32]),
}

/// A `TransactionStorage.Authorizations` value: what the account has used and
/// may use, and the block the authorization expires at.
#[derive(Decode)]
#[cfg_attr(test, derive(Encode))]
pub struct BulletinAuthorization {
	/// Submissions made.
	pub transactions: u32,
	/// Submissions allowed.
	pub transactions_allowance: u32,
	/// Bytes stored.
	pub bytes: u64,
	/// Bytes renewed.
	pub bytes_permanent: u64,
	/// Bytes allowed.
	pub bytes_allowance: u64,
	/// Block at which the authorization expires.
	pub expiration: u32,
}

impl BulletinAllowanceInfo {
	/// Returns whether the snapshot still permits at least one submission.
	pub fn available(self) -> bool {
		self.can_store(1)
	}

	/// Require enough bytes and a live transaction allowance for this payload.
	pub fn can_store(self, size: u64) -> bool {
		self.remained_size >= size &&
			self.remained_transactions > 0 &&
			self.fetched_at < self.expires_in
	}
}

/// A fixed collection key supplied by standalone CLI operations.
#[derive(Clone)]
pub struct CollectionCandidate {
	/// Collection to look for membership in.
	pub collection: PersonhoodCollection,
	/// Ring-VRF entropy for this collection.
	pub entropy: [u8; 32],
}

// Pin all reads to one finalized block so the membership snapshot is consistent.
async fn find_including_ring(
	rpc: &RpcClient,
	metadata: &Metadata,
	collection: PersonhoodCollection,
	signer: &dyn PersonhoodSigner,
	lookback: u32,
) -> Result<Option<RingParams>, StatementAllowanceError> {
	let member = signer.member(collection)?;
	let at = rpc.finalized_head().await?;
	let exponent = ring::read_ring_exponent(rpc, metadata, collection, &at).await?;
	let current = ring::read_current_ring_index_at(rpc, collection, &at).await?;
	let oldest = current.saturating_sub(lookback);
	for ring_index in (oldest..=current).rev() {
		let members = ring::read_ring_members_at(rpc, collection, ring_index, &at).await?;
		if members.contains(&member) {
			return Ok(Some(RingParams {
				collection,
				members,
				exponent,
				ring_index,
				block_hash: at,
			}));
		}
	}
	Ok(None)
}

/// Locate our including ring in each candidate collection, in the order given.
///
/// Membership in the ring is the availability test: a candidate whose member key
/// no ring includes is dropped, so the result is exactly the set of collections
/// this device can prove right now. A collection this chain does not run is
/// skipped rather than raised, because a chain without full personhood is not a
/// failure for a light-personhood device.
pub async fn find_including_rings(
	rpc: &RpcClient,
	metadata: &Metadata,
	signer: &dyn PersonhoodSigner,
	collections: &[PersonhoodCollection],
	lookback: u32,
) -> Result<Vec<RingParams>, StatementAllowanceError> {
	let mut memberships = Vec::new();
	let mut first_error = None;
	for &collection in collections {
		match find_including_ring(rpc, metadata, collection, signer, lookback).await {
			Ok(Some(ring)) => {
				// A ring whose exponent has no proof domain cannot be proved
				// against, so it must not enter the set: selecting it would fail
				// after the collection was already chosen, with no fallback left.
				if let Err(err) = proof::domain_for_ring_exponent(ring.exponent) {
					warn!(%collection, %err, "unusable ring exponent; skipping collection");
					continue;
				}
				memberships.push(ring);
			},
			Ok(None) => debug!(%collection, "no ring includes our member key"),
			// One collection's failure must not take down the others. A device
			// that can only prove light personhood should still get its
			// allowance when the full-personhood storage is unreadable.
			Err(err @ StatementAllowanceError::Authority(_)) => return Err(err),
			Err(err) => {
				warn!(%collection, %err, "could not resolve this collection");
				if first_error.is_none() {
					first_error = Some(err);
				}
			},
		}
	}
	// Every candidate erroring is an outage, not an answer: reporting "not a
	// member" there would let a caller conclude the person has no personhood.
	match (memberships.is_empty(), first_error) {
		(true, Some(err)) => Err(err),
		_ => Ok(memberships),
	}
}

async fn register_statement_account(
	rpc: &RpcClient,
	metadata: &Metadata,
	target_chain: ExtrinsicTarget<'_>,
	signer: &dyn PersonhoodSigner,
	params: RegistrationParams<'_>,
) -> Result<RegistrationOutcome, StatementAllowanceError> {
	let collection = params.ring.collection;
	let revision = ring::read_ring_revision(
		rpc,
		metadata,
		collection,
		params.ring.ring_index,
		&params.ring.block_hash,
	)
	.await?;
	let mut skipped_duplicate_slots = Vec::new();
	let mut preselected = params.preselected;
	// One registration revokes at most one allowance. The duplicate-submit retry
	// was written for free slots, where trying another costs nothing; on a full
	// period each attempt is a revocation, and the earlier submission may still
	// land, so a second takeover can leave two allowances revoked for one call.
	//
	// A preselected takeover counts: the caller already spent this call's one
	// revocation, so a retry must not spend another.
	let mut took_over_a_slot = matches!(preselected, Some(Preselected::Takeover(_)));
	loop {
		let seq = match preselected.take() {
			Some(preselected) => preselected.seq(),
			None => match slot::scan_slot_excluding(
				rpc,
				metadata,
				slot::SlotScan {
					collection,
					signer,
					network_suffix: params.network_suffix,
					period: params.period,
					target: params.target,
					excluded: &skipped_duplicate_slots,
					reuse_existing: params.reuse_existing,
				},
			)
			.await?
			{
				SlotSelection::AlreadyAllocated(seq) => {
					return Ok(RegistrationOutcome::AlreadyAllocated { seq, collection });
				},
				SlotSelection::Free(seq) => seq,
				SlotSelection::FreeSlotsExcluded => {
					// A free slot exists; it is only held back by one of this
					// call's own in-flight submissions. Evicting a live slot
					// instead would revoke an allowance for no reason.
					return Err(
						SlotError::FreeSlotsAwaitingSubmission { period: params.period }.into()
					);
				},
				SlotSelection::Full { max, occupied } => {
					if took_over_a_slot {
						return Err(SlotError::NoFreeStatementStoreSlot {
							period: params.period,
							max,
						}
						.into());
					}
					// Nothing free: replace the oldest slot the runtime will
					// let us take, and only then give up.
					let cooldown = slot::replacement_cooldown(rpc, metadata).await?;
					let chain_now = slot::read_chain_now_seconds(rpc).await?;
					match slot::replaceable_slot(
						&occupied,
						params.target,
						chain_now,
						cooldown,
						params.protected,
					) {
						Some(seq) => {
							took_over_a_slot = true;
							seq
						},
						None => {
							return Err(SlotError::NoFreeStatementStoreSlot {
								period: params.period,
								max,
							}
							.into());
						},
					}
				},
			},
		};

		let context = slot::derive_slot_context(params.network_suffix, params.period, seq);
		let alias =
			slot::slot_alias(signer, collection, params.network_suffix, params.period, seq)?;
		let proof = RingProof { signer, ring: params.ring, context, revision };
		let call = extrinsic::SetStatementStoreAccountCallArgs {
			period: params.period,
			seq,
			target_account: *params.target,
		};
		let extrinsic = target_chain
			.extrinsics
			.build_mortal_extrinsic(target_chain.genesis)
			.with_call_args(RESOURCES_PALLET, "set_statement_store_account", call)
			.with_extension(AsResourcesExtension::statement_store_allowance(proof))
			.build()
			.await?;

		match rpc.submit_and_watch(extrinsic.extrinsic.bytes()).await {
			Ok(block_hash) => {
				if slot::read_slot_account_at(rpc, &alias, params.period, &block_hash).await? !=
					Some(*params.target)
				{
					return Err(SlotError::RegistrationVerificationMismatch {
						block_hash,
						period: params.period,
						seq,
					}
					.into());
				}
				return Ok(RegistrationOutcome::Registered {
					block_hash,
					seq,
					ring_index: params.ring.ring_index,
					collection,
				});
			},
			Err(err) if duplicate_submit_error(&err.to_string()) => {
				skipped_duplicate_slots.push(seq);
			},
			Err(err) if took_over_a_slot && invalid_transaction_error(&err.to_string()) => {
				// The runtime re-checks the replacement cooldown against its own
				// clock and rejects at validation, so a takeover can lose a race
				// it looked eligible for. Name that rather than surfacing a raw
				// RPC failure the caller cannot act on.
				return Err(SlotError::ReplacementRefused { period: params.period, seq }.into());
			},
			Err(err) => return Err(err),
		}
	}
}

/// One collection's scan of a period's slot table.
pub struct CollectionScan {
	/// Collection scanned.
	pub collection: PersonhoodCollection,
	/// What the scan found.
	pub selection: SlotSelection,
}

/// Public target and period shared by a collection scan.
pub struct CollectionScanParams<'a> {
	/// Runtime suffix used for product-scoped aliases.
	pub network_suffix: &'a [u8],
	/// Statement-store period to scan.
	pub period: u32,
	/// Account whose existing slot may be reused.
	pub target: &'a [u8; 32],
	/// Whether to accept a slot already held by the target.
	pub reuse_existing: bool,
}

/// Scan the period's slot table in every supported candidate collection.
///
/// Alias derivation and slot reads only, so this runs before any ring snapshot
/// is fetched. That ordering matters twice over: a ring snapshot pages in every
/// member key, and the common case for an established product is that an
/// allowance is already in place and no proof is needed at all.
///
/// The result is handed to [`register_statement_account_pooled`] so the table is
/// read once per period rather than once per question asked about it.
///
/// A candidate whose read fails is logged and skipped: one unreadable collection
/// must not fail an allocation the device could satisfy from another.
pub async fn scan_collections(
	rpc: &RpcClient,
	metadata: &Metadata,
	signer: &dyn PersonhoodSigner,
	collections: &[PersonhoodCollection],
	params: CollectionScanParams<'_>,
) -> Result<Vec<CollectionScan>, StatementAllowanceError> {
	let supported = collections.iter().copied().filter(|collection| {
		let supported = collection.is_supported(metadata);
		if !supported {
			debug!(%collection, "chain declares no slot budget for this collection");
		}
		supported
	});
	let selections = futures::future::join_all(supported.map(|collection| async move {
		let selection = slot::scan_slot_excluding(
			rpc,
			metadata,
			slot::SlotScan {
				collection,
				signer,
				network_suffix: params.network_suffix,
				period: params.period,
				target: params.target,
				excluded: &[],
				reuse_existing: params.reuse_existing,
			},
		)
		.await;
		(collection, selection)
	}))
	.await;
	let mut scans = Vec::new();
	for (collection, selection) in selections {
		let selection = match selection {
			Err(err @ StatementAllowanceError::Authority(_)) => return Err(err),
			other => other,
		};
		if allocated_in(&scans).is_some() {
			continue;
		}
		match selection {
			Ok(selection) => scans.push(CollectionScan { collection, selection }),
			Err(err) => warn!(%collection, %err, "could not scan this collection's slots"),
		}
	}
	Ok(scans)
}

/// The slot `target` already holds, according to `scans`.
///
/// Covers every collection that was scanned, including ones whose ring the device
/// cannot currently prove: the allowance is live regardless of whether a fresh
/// proof could be built, so a second slot must not be claimed for the same target.
pub fn allocated_in(scans: &[CollectionScan]) -> Option<(PersonhoodCollection, u32)> {
	scans.iter().find_map(|scan| match scan.selection {
		SlotSelection::AlreadyAllocated(seq) => Some((scan.collection, seq)),
		_ => None,
	})
}

/// Target and slot-selection inputs for a registration pooled across
/// collections.
pub struct PooledRegistrationParams<'a> {
	/// Account that should receive the statement-store registration.
	pub target: &'a [u8; 32],
	/// Statement-store period for which the registration is requested.
	pub period: u32,
	/// Runtime-wide suffix used for product-scoped aliases and proofs.
	pub network_suffix: &'a [u8],
	/// Whether an existing registration for this period may be reused.
	pub reuse_existing: bool,
	/// Whether a live slot may be replaced once every collection is full.
	///
	/// Off for on-demand allocation: connecting a product must not revoke another
	/// product's allowance, and with a replacement cooldown of a minute nearly
	/// every occupied slot would qualify. On for the renewal pass, whose job is
	/// to keep the ledger's own targets alive across period boundaries.
	pub allow_eviction: bool,
	/// Slots the caller has already claimed in this batch and must not lose.
	/// Scoped per collection, because the same `seq` in two collections is two
	/// unrelated slots and protecting one must not protect the other.
	pub protected: &'a [(PersonhoodCollection, u32)],
}

/// Register statement-store allowance for `target`, pooling slots across every
/// collection in `memberships`, using the slot tables `scans` already read.
///
/// Each collection is a separate alias space with its own budget, so a device
/// that can prove two memberships has the sum of both. Collections are tried in
/// the order the memberships are given.
///
/// A free slot in any collection is always preferred to replacing a live one. A
/// replacement is only considered when `allow_eviction` is set and every
/// collection is full, and the candidate is then the globally oldest replaceable
/// slot across all collections rather than the oldest within the first full one.
///
/// `scans` may cover collections absent from `memberships`. That is deliberate:
/// an allowance the target already holds counts even where a fresh proof could
/// not be built, so it is reported instead of claiming a second slot.
pub async fn register_statement_account_pooled(
	rpc: &RpcClient,
	metadata: &Metadata,
	target_chain: ExtrinsicTarget<'_>,
	signer: &dyn PersonhoodSigner,
	scans: &[CollectionScan],
	memberships: &[RingParams],
	params: PooledRegistrationParams<'_>,
) -> Result<RegistrationOutcome, StatementAllowanceError> {
	// Answered across every scanned collection before anything is submitted.
	if let Some((collection, seq)) = allocated_in(scans) {
		return Ok(RegistrationOutcome::AlreadyAllocated { seq, collection });
	}

	let protected_in = |collection: PersonhoodCollection| -> Vec<u32> {
		params
			.protected
			.iter()
			.filter(|(candidate, _)| *candidate == collection)
			.map(|(_, seq)| *seq)
			.collect()
	};

	// Only collections we can both prove and have read a table for are usable.
	let mut free = None;
	let mut full = Vec::new();
	let mut budget: u32 = 0;
	let mut usable = 0;
	for (index, membership) in memberships.iter().enumerate() {
		let collection = membership.collection;
		let Some(scan) = scans.iter().find(|scan| scan.collection == collection) else {
			continue;
		};
		usable += 1;
		// Summed from the collections in play rather than from the full ones, so
		// the figure is the device's budget whichever branch reports it.
		budget = budget.saturating_add(collection.slots_per_period(rpc, metadata).await?);
		match &scan.selection {
			SlotSelection::Free(seq) => {
				if free.is_none() {
					free = Some((index, *seq));
				}
			},
			SlotSelection::Full { occupied, .. } => full.push((index, occupied)),
			// A free slot held back by this call's own in-flight submission.
			// Replacing a live slot instead would revoke an allowance for
			// capacity that is about to come back.
			SlotSelection::FreeSlotsExcluded => {
				return Err(SlotError::FreeSlotsAwaitingSubmission { period: params.period }.into());
			},
			// Handled above, before any collection was chosen.
			SlotSelection::AlreadyAllocated(seq) => {
				return Ok(RegistrationOutcome::AlreadyAllocated { seq: *seq, collection });
			},
		}
	}
	if usable == 0 {
		return Err(SlotError::NoCollectionMembership.into());
	}

	let exhausted = || SlotError::NoFreeStatementStoreSlot { period: params.period, max: budget };

	let (index, choice) = match free {
		Some((index, seq)) => (index, Preselected::Free(seq)),
		None if !params.allow_eviction => return Err(exhausted().into()),
		None => {
			let cooldown = slot::replacement_cooldown(rpc, metadata).await?;
			let chain_now = slot::read_chain_now_seconds(rpc).await?;
			let mut oldest: Option<(usize, u32, u64)> = None;
			for (index, occupied) in &full {
				let protected = protected_in(memberships[*index].collection);
				let Some(seq) = slot::replaceable_slot(
					occupied,
					params.target,
					chain_now,
					cooldown,
					&protected,
				) else {
					continue;
				};
				let since = occupied
					.iter()
					.find(|slot| slot.seq == seq)
					.map(|slot| slot.since)
					.unwrap_or(u64::MAX);
				let better = match oldest {
					Some((_, _, best)) => since < best,
					None => true,
				};
				if better {
					oldest = Some((*index, seq, since));
				}
			}
			match oldest {
				Some((index, seq, _)) => (index, Preselected::Takeover(seq)),
				None => return Err(exhausted().into()),
			}
		},
	};

	let membership = &memberships[index];
	let protected = protected_in(membership.collection);
	register_statement_account(
		rpc,
		metadata,
		target_chain,
		signer,
		RegistrationParams {
			target: params.target,
			period: params.period,
			network_suffix: params.network_suffix,
			ring: membership,
			reuse_existing: params.reuse_existing,
			preselected: Some(choice),
			// A duplicate-submit retry rescans within this collection only. It
			// is a rare race on a slot we just saw free, and staying put keeps
			// the retry from quietly moving collections mid-call.
			protected: &protected,
		},
	)
	.await
	.map_err(|err| match err {
		// The retry rescans one collection, so its own exhaustion names that
		// collection's share. Restate it as the device's pooled budget so the
		// same error never means two different things to a caller.
		StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot { .. }) => {
			exhausted().into()
		},
		other => other,
	})
}

/// Claim long-term Bulletin storage authorization for `target`, proving
/// membership in the already-located `ring`, at People-chain `period`.
pub async fn claim_long_term_storage(
	params: LongTermStorageClaim<'_>,
) -> Result<LongTermStorageOutcome, StatementAllowanceError> {
	let LongTermStorageClaim {
		rpc,
		metadata,
		target_chain,
		signer,
		network_suffix,
		target,
		period,
		ring,
	} = params;
	let revision =
		ring::read_ring_revision(rpc, metadata, ring.collection, ring.ring_index, &ring.block_hash)
			.await?;
	let mut skipped_duplicate_counters = Vec::new();
	loop {
		let counter = slot::scan_long_term_storage_counter_excluding(
			rpc,
			metadata,
			signer,
			ring.collection,
			network_suffix,
			period,
			&skipped_duplicate_counters,
		)
		.await?;

		let context = slot::derive_long_term_storage_context(network_suffix, period, counter);
		let proof = RingProof { signer, ring, context, revision };
		let call = extrinsic::ClaimLongTermStorageCallArgs { period, counter, account_id: *target };
		let extrinsic = target_chain
			.extrinsics
			.build_mortal_extrinsic(target_chain.genesis)
			.with_call_args(RESOURCES_PALLET, "claim_long_term_storage", call)
			.with_extension(AsResourcesExtension::long_term_storage(proof))
			.build()
			.await?;
		debug!(
			period,
			counter,
			ring_index = ring.ring_index,
			revision,
			"submitting Bulletin long-term-storage claim"
		);

		match rpc.submit_and_watch(extrinsic.extrinsic.bytes()).await {
			Ok(block_hash) => {
				return Ok(LongTermStorageOutcome::Claimed {
					block_hash,
					counter,
					ring_index: ring.ring_index,
				});
			},
			Err(err) if duplicate_submit_error(&err.to_string()) => {
				skipped_duplicate_counters.push(counter);
			},
			Err(err) => {
				warn!(
					period,
					counter,
					ring_index = ring.ring_index,
					revision,
					%err,
					"Bulletin long-term-storage claim failed"
				);
				return Err(err);
			},
		}
	}
}

/// What a notification claim does once it is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimSubmission {
	/// Submit it and wait for it to reach a block.
	Submit,
	/// Ask the runtime whether it would accept it, and submit nothing.
	ValidateOnly,
}

/// Result of a notification-account claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationAccountOutcome {
	/// The claim reached a block and the slot was verified to allow the target.
	Registered {
		/// Block hash the claim landed in.
		block_hash: String,
		/// Claimed slot sequence.
		seq: u8,
		/// Collection whose alias space holds the slot.
		collection: PersonhoodCollection,
	},
	/// The target already holds a registration in this period; nothing built.
	AlreadyRegistered {
		/// Its slot sequence.
		seq: u8,
	},
	/// The runtime would accept the claim; nothing was submitted.
	Validated {
		/// Slot sequence the claim names.
		seq: u8,
		/// Collection whose alias space holds the slot.
		collection: PersonhoodCollection,
	},
}

/// The notification slot `target` holds in `period`, or `None` when it holds
/// none. An account registered in another period cannot be registered again.
pub async fn notification_registration_in(
	rpc: &RpcClient,
	target: &[u8; 32],
	period: u32,
) -> Result<Option<u8>, StatementAllowanceError> {
	match slot::read_notification_registration_of(rpc, target).await? {
		None => Ok(None),
		Some(registration) if registration.period == period => Ok(Some(registration.seq)),
		Some(registration) => {
			Err(SlotError::NotificationAccountUsed { registered: registration.period }.into())
		},
	}
}

/// Everything one notification-account claim needs.
pub struct NotificationAccountClaim<'a> {
	/// People connection the claim is read and submitted on.
	pub rpc: &'a RpcClient,
	/// People runtime metadata.
	pub metadata: &'a Metadata,
	/// Where the claim extrinsic is built.
	pub target_chain: ExtrinsicTarget<'a>,
	/// Authorizes synchronous personhood operations for the selected wallet.
	pub signer: &'a dyn PersonhoodSigner,
	/// People suffix used for the slot context and alias.
	pub network_suffix: &'a [u8],
	/// Account that should get the notification allowance.
	pub target: &'a [u8; 32],
	/// Notification period: the current UTC day.
	pub period: u32,
	/// Rings the person is a member of, in the order to try them.
	pub memberships: &'a [RingParams],
	/// Whether to submit the claim or only validate it.
	pub submission: ClaimSubmission,
}

/// Register `target` as a notification statement account for `period`, in
/// the first free slot of the first collection in `memberships` that has one.
///
/// The runtime registers an account once, for one slot, and grants it one
/// statement, so a caller asks [`notification_registration_in`] first and
/// claims only for an account that holds none, one claim at a time.
///
/// A submission that fails, or that the pool already holds an equivalent of,
/// is settled by what the chain holds, never by trying another slot while
/// this one may still land: a target the chain now registers is reported
/// registered; a slot the chain gave someone else moves the claim on; and
/// anything else is [`SlotError::NotificationClaimUncertain`] or the error.
/// One claim may still land after it was reported uncertain; a later call
/// then finds the target registered and claims nothing.
///
/// `signer` refuses once its owner's session ends: its last use is the
/// proof, made with no wait before the submission, so nothing is submitted
/// after a refusal.
pub async fn claim_notification_account(
	params: NotificationAccountClaim<'_>,
) -> Result<NotificationAccountOutcome, StatementAllowanceError> {
	let NotificationAccountClaim {
		rpc,
		metadata,
		target_chain,
		signer,
		network_suffix,
		target,
		period,
		memberships,
		submission,
	} = params;
	if memberships.is_empty() {
		return Err(SlotError::NoCollectionMembership.into());
	}
	let mut budget: u32 = 0;
	for ring in memberships {
		let collection = ring.collection;
		budget = budget.saturating_add(
			u32::from(slot::notification_max_seq(rpc, metadata, collection).await?) + 1,
		);
		let revision =
			ring::read_ring_revision(rpc, metadata, collection, ring.ring_index, &ring.block_hash)
				.await?;
		loop {
			let Some(seq) = slot::scan_notification_slot_excluding(
				rpc,
				metadata,
				signer,
				collection,
				network_suffix,
				period,
				&[],
			)
			.await?
			else {
				break;
			};
			let context = slot::derive_notification_context(network_suffix, period, seq);
			let proof = RingProof { signer, ring, context, revision };
			let call = extrinsic::SetNotificationAccountCallArgs {
				reference: (period, seq),
				account_id: *target,
			};
			let built = target_chain
				.extrinsics
				.build_mortal_extrinsic(target_chain.genesis)
				.with_call_args(
					RESOURCES_PALLET,
					"set_notification_statement_account_for_sequence",
					call,
				)
				.with_extension(AsResourcesExtension::notification(proof))
				.build()
				.await?;
			let extrinsic = built.extrinsic.bytes();
			debug!(period, seq, %collection, ring_index = ring.ring_index, ?submission, "notification claim built");

			if submission == ClaimSubmission::ValidateOnly {
				return match validate_transaction(rpc, extrinsic).await? {
					Ok(()) => Ok(NotificationAccountOutcome::Validated { seq, collection }),
					Err(validity) => {
						Err(SlotError::NotificationClaimInvalid { period, seq, validity }.into())
					},
				};
			}
			match rpc.submit_and_watch(extrinsic).await {
				Ok(block_hash) => {
					let alias =
						slot::notification_alias(signer, collection, network_suffix, period, seq)?;
					if slot::read_notification_account_at(rpc, &alias, &block_hash).await? !=
						Some(*target)
					{
						return Err(SlotError::RegistrationVerificationMismatch {
							block_hash,
							period,
							seq: u32::from(seq),
						}
						.into());
					}
					return Ok(NotificationAccountOutcome::Registered {
						block_hash,
						seq,
						collection,
					});
				},
				Err(err) => {
					if let Some(held) = notification_registration_in(rpc, target, period).await? {
						return Ok(NotificationAccountOutcome::AlreadyRegistered { seq: held });
					}
					if !duplicate_submit_error(&err.to_string()) {
						return Err(err);
					}
					let alias =
						slot::notification_alias(signer, collection, network_suffix, period, seq)?;
					match slot::read_notification_account(rpc, &alias).await? {
						// Another claim took the slot, so this one cannot land: claim the next.
						Some(_) => continue,
						None => {
							return Err(
								SlotError::NotificationClaimUncertain { period, seq }.into()
							);
						},
					}
				},
			}
		}
	}
	Err(SlotError::NoFreeNotificationSlot { period, max: budget }.into())
}

/// Ask the runtime, at the finalized head, whether it would accept
/// `extrinsic` from outside the node: `Ok` when valid, or its validity answer.
async fn validate_transaction(
	rpc: &RpcClient,
	extrinsic: &[u8],
) -> Result<Result<(), String>, StatementAllowanceError> {
	let at = rpc.finalized_head().await?;
	let bytes = rpc.validate_transaction(extrinsic, &at).await?;
	Ok(match bytes.first() {
		Some(0) => Ok(()),
		_ => Err(transaction_validity_error(&bytes)),
	})
}

/// Name a `TransactionValidity` error the way the runtime's enums do.
fn transaction_validity_error(bytes: &[u8]) -> String {
	const INVALID: [&str; 13] = [
		"Call",
		"Payment",
		"Future",
		"Stale",
		"BadProof",
		"AncientBirthBlock",
		"ExhaustsResources",
		"Custom",
		"BadMandatory",
		"MandatoryValidation",
		"BadSigner",
		"IndeterminateImplicit",
		"UnknownOrigin",
	];
	const UNKNOWN: [&str; 3] = ["CannotLookup", "NoUnsignedValidator", "Custom"];
	let named = match bytes {
		[1, 0, variant, rest @ ..] => INVALID
			.get(usize::from(*variant))
			.map(|name| ("Invalid", *name, rest.first().filter(|_| *variant == 7))),
		[1, 1, variant, rest @ ..] => UNKNOWN
			.get(usize::from(*variant))
			.map(|name| ("Unknown", *name, rest.first().filter(|_| *variant == 2))),
		_ => None,
	};
	match named {
		Some((kind, name, Some(code))) => format!("{kind}::{name}({code})"),
		Some((kind, name, None)) => format!("{kind}::{name}"),
		None => format!("0x{}", hex::encode(bytes)),
	}
}

/// Fetch Bulletin `TransactionStorage.Authorizations[Account(target)]`.
pub async fn fetch_bulletin_allowance(
	rpc: &RpcClient,
	target: &[u8; 32],
) -> Result<Option<BulletinAllowanceInfo>, StatementAllowanceError> {
	let Some(bytes) = rpc.get_storage(&bulletin_authorization_key(target)).await? else {
		return Ok(None);
	};
	let fetched_at = fetch_block_number(rpc).await?;
	decode_bulletin_allowance(&bytes, fetched_at).map(Some)
}

/// Wait until Bulletin authorization is available and fresher than `current`,
/// then require it to hold `size` bytes.
pub async fn wait_bulletin_authorization(
	rpc: &RpcClient,
	target: &[u8; 32],
	current: Option<BulletinAllowanceInfo>,
	size: u64,
	timeout: Duration,
) -> Result<BulletinAllowanceInfo, StatementAllowanceError> {
	let started = Instant::now();
	let baseline = current.filter(|info| info.available());
	loop {
		let Some(info) = fetch_bulletin_allowance(rpc, target).await? else {
			wait_before_next_bulletin_authorization_poll(started, timeout).await?;
			continue;
		};
		if authorization_refreshed(info, baseline) {
			if !info.can_store(size) {
				return Err(StatementAllowanceError::BulletinAuthorizationTooSmall {
					size,
					remained_size: info.remained_size,
				});
			}
			return Ok(info);
		}
		wait_before_next_bulletin_authorization_poll(started, timeout).await?;
	}
}

async fn wait_before_next_bulletin_authorization_poll(
	started: Instant,
	timeout: Duration,
) -> Result<(), StatementAllowanceError> {
	let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
		return Err(StatementAllowanceError::BulletinAuthorizationTimeout);
	};
	let delay = futures_timer::Delay::new(remaining.min(Duration::from_secs(2))).fuse();
	futures::pin_mut!(delay);
	delay.await;
	Ok(())
}

fn authorization_refreshed(
	info: BulletinAllowanceInfo,
	baseline: Option<BulletinAllowanceInfo>,
) -> bool {
	if !info.available() {
		return false;
	}
	match baseline {
		None => true,
		Some(current) => {
			info.remained_transactions > current.remained_transactions ||
				info.remained_size > current.remained_size ||
				info.expires_in > current.expires_in
		},
	}
}

/// `TransactionStorage.Authorizations[AuthorizationScope::Account(target)]`.
fn bulletin_authorization_key(target: &[u8; 32]) -> Vec<u8> {
	let scope = BulletinAuthorizationScope::Account(*target).encode();
	[
		twox_128(b"TransactionStorage").as_slice(),
		twox_128(b"Authorizations").as_slice(),
		&key_hash::blake2_128_concat(&scope),
	]
	.concat()
}

fn decode_bulletin_allowance(
	bytes: &[u8],
	fetched_at: u32,
) -> Result<BulletinAllowanceInfo, StatementAllowanceError> {
	let authorization = BulletinAuthorization::decode(&mut &bytes[..])
		.map_err(ChainStateError::AuthorizationDecode)?;
	Ok(BulletinAllowanceInfo {
		remained_size: authorization.bytes_allowance.saturating_sub(authorization.bytes),
		remained_transactions: authorization
			.transactions_allowance
			.saturating_sub(authorization.transactions),
		expires_in: authorization.expiration,
		fetched_at,
	})
}

async fn fetch_block_number(rpc: &RpcClient) -> Result<u32, StatementAllowanceError> {
	let header = rpc.call("chain_getHeader", json!([])).await?;
	let number = header
		.get("number")
		.and_then(Value::as_str)
		.ok_or(ChainStateError::HeaderNumberMissing)?;
	u32::from_str_radix(number.trim_start_matches("0x"), 16)
		.map_err(|err| ChainStateError::HeaderNumberParse(err).into())
}

/// Pool responses meaning an equivalent claim already occupies the pool, so
/// the scan should move to the next slot. Bans and validity failures are hard
/// errors for the caller.
/// Whether the node rejected the extrinsic as invalid rather than accepting it
/// into the pool. The runtime's `AsResources` validity check answers this way
/// when a slot is not replaceable yet.
fn invalid_transaction_error(message: &str) -> bool {
	message.to_ascii_lowercase().contains("invalid transaction")
}

fn duplicate_submit_error(message: &str) -> bool {
	let message = message.to_ascii_lowercase();
	message.contains("priority is too low") || message.contains("already imported")
}

#[cfg(test)]
mod tests;
