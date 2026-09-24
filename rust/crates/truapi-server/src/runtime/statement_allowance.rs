//! On-chain statement-store allowance registration (`set_statement_store_account`).
//!
//! Mirrors how iOS and browser account holders obtain statement-store
//! allowance from the real People chain: build the
//! `Resources.set_statement_store_account` call, prove personhood ring
//! membership with the caller's registry-selected ring-VRF key, and submit the
//! resulting unsigned General (v5) extrinsic.
//!
//! These helpers accept host-backed RPC clients on native and browser targets;
//! the host supplies the chain connections and controls which claims it offers.
//! Opening a direct RPC URL is only available on native targets.

pub mod collection;
pub mod extension;
pub mod extrinsic;
pub mod inspection;
pub mod pgas;
pub mod proof;
pub mod renewal;
pub mod ring;
pub mod rpc;
pub mod slot;
#[cfg(test)]
pub(crate) mod test_fixtures;
pub mod view;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use futures::FutureExt;
use parity_scale_codec::{Decode, Encode};
use serde_json::{Value, json};
use sp_crypto_hashing::twox_128;
use thiserror::Error;
use tracing::{debug, warn};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use collection::PersonhoodCollection;
use extension::{ChainState, Metadata, MetadataError};
use ring::RingParams;
use rpc::RpcClient;
use slot::{SlotError, SlotSelection};

/// Error while reading chain state, building allowance extrinsics, or waiting
/// for allowance authorization.
#[derive(Debug, Error)]
pub enum StatementAllowanceError {
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
    /// Bulletin allowance polling timed out.
    #[error("timed out waiting for Bulletin authorization")]
    BulletinAuthorizationTimeout,
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
    /// Bulletin authorization storage failed to decode a field.
    #[error("authorization {field}: {source}")]
    AuthorizationFieldDecode {
        /// Field name.
        field: &'static str,
        /// SCALE decode failure.
        #[source]
        source: parity_scale_codec::Error,
    },
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
        }
        Err(reason) => {
            debug!(
                version = PREFERRED_METADATA_VERSION,
                %reason,
                "metadata runtime call failed; using state_getMetadata"
            );
        }
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
        .call(
            "state_call",
            json!(["Metadata_metadata_at_version", argument]),
        )
        .await?;
    let hex_str = value
        .as_str()
        .ok_or(MetadataError::MetadataResultNotString)?;
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
    let hex_str = value
        .as_str()
        .ok_or(MetadataError::MetadataResultNotString)?;
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
    Ok((
        json_u32(&runtime, "specVersion")?,
        json_u32(&runtime, "transactionVersion")?,
    ))
}

/// Read the chain's genesis block hash.
pub async fn fetch_genesis_hash(rpc: &RpcClient) -> Result<[u8; 32], StatementAllowanceError> {
    let genesis_hex = rpc.call("chain_getBlockHash", json!([0])).await?;
    let genesis_str = genesis_hex
        .as_str()
        .ok_or(ChainStateError::GenesisHashNotString)?;
    let genesis = hex::decode(genesis_str.strip_prefix("0x").unwrap_or(genesis_str))
        .map_err(ChainStateError::GenesisHex)?;
    let len = genesis.len();
    genesis
        .try_into()
        .map_err(|_| ChainStateError::GenesisHashLength { len }.into())
}

/// Fetch the chain state needed to fill the signed extensions.
pub async fn fetch_chain_state(rpc: &RpcClient) -> Result<ChainState, StatementAllowanceError> {
    let genesis_hash = fetch_genesis_hash(rpc).await?;
    let (spec_version, transaction_version) = fetch_runtime_version(rpc).await?;
    Ok(ChainState {
        spec_version,
        transaction_version,
        genesis_hash,
        nonce: 0,
        restrict_origins: false,
    })
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
        Self {
            rpc,
            configured_genesis_hash,
        }
    }

    /// The underlying client, for reads that do not need the chain's identity.
    pub fn rpc(&self) -> &RpcClient {
        &self.rpc
    }
}

/// Runtime metadata and signed-extension chain state for one chain.
#[derive(Clone)]
pub struct ChainContext {
    /// Decoded runtime metadata.
    pub metadata: Arc<Metadata>,
    /// Chain state filling the standard signed extensions.
    pub state: ChainState,
}

/// Runtime metadata and chain state cached per chain.
///
/// Both are fixed for a given runtime, and a full `state_getMetadata` response
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
    /// Metadata and chain state for the chain reached over `rpc`, read from the
    /// chain only when no entry describes the chain as it is now.
    ///
    /// The client carries the caller's identity for the chain — the hash it
    /// routes connections by — and that keys the cache. The genesis hash
    /// placed in [`ChainState`], and therefore signed into every allowance
    /// extrinsic, is always the one the chain itself reports: a host whose
    /// configured constant has gone stale (a wiped testnet) still produces
    /// valid extrinsics. A divergence is logged, since it means the host's
    /// chain configuration needs refreshing — see RFC-0026, which lets hosts
    /// discover these hashes instead of hard-coding them.
    ///
    /// An entry is reused only when both the spec version **and** the reported
    /// genesis hash still match. Spec version alone cannot see a chain that was
    /// wiped and redeployed from the same runtime, which keeps its spec version
    /// and gets a new genesis; an entry held across that would sign
    /// `CheckGenesis` for a chain that no longer exists. Both reads are issued
    /// together, so validating on the pair costs no extra round trip.
    pub async fn get(&self, client: &ChainClient) -> Result<ChainContext, StatementAllowanceError> {
        let rpc = client.rpc();
        let configured_genesis_hash = client.configured_genesis_hash;
        let ((spec_version, transaction_version), genesis_hash) =
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
                "chain reports a different genesis than the host configured; using the chain's"
            );
        }
        let context = ChainContext {
            metadata: Arc::new(fetch_metadata(rpc).await?),
            state: ChainState {
                spec_version,
                transaction_version,
                genesis_hash,
                nonce: 0,
                restrict_origins: false,
            },
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
                cached.state.spec_version == spec_version
                    && cached.state.genesis_hash == genesis_hash
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preselected {
    /// A slot observed free; claiming it revokes nothing.
    Free(u32),
    /// A live slot the caller judged replaceable.
    Takeover(u32),
}

impl Preselected {
    /// The chosen slot sequence.
    pub fn seq(self) -> u32 {
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

/// Target and slot-selection inputs for one statement-store registration.
pub struct RegistrationParams<'a> {
    /// Account that should receive the statement-store registration.
    pub target: &'a [u8; 32],
    /// Statement-store period for which the registration is requested.
    pub period: u32,
    /// Runtime-wide suffix used for product-scoped aliases and proofs.
    pub network_suffix: &'a [u8],
    /// Ring parameters used to build the membership proof.
    pub ring: &'a RingParams,
    /// Whether an existing registration for this period may be reused.
    pub reuse_existing: bool,
    /// A slot the caller's own scan already selected, used for the first attempt
    /// so the scan is not repeated. The duplicate-submit retry rescans, so this
    /// only ever shortcuts the first submission.
    pub preselected: Option<Preselected>,
    /// Slots the caller has already claimed in this batch and must not lose.
    /// A multi-target pass would otherwise take a slot back off a target it
    /// registered moments earlier and never settle.
    pub protected: &'a [u32],
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
    /// People signed-extension state.
    pub chain_state: &'a ChainState,
    /// Our ring-VRF entropy for the collection `ring` names.
    pub entropy: [u8; 32],
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
#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum BulletinAuthorizationScope {
    Account([u8; 32]),
}

impl BulletinAllowanceInfo {
    /// Returns whether the snapshot still permits at least one submission.
    pub fn available(self) -> bool {
        self.remained_size > 0
            && self.remained_transactions > 0
            && self.fetched_at < self.expires_in
    }
}

/// A collection this device can derive aliases for, with the entropy backing
/// them. Each collection has its own entropy, so the pair travels together.
#[derive(Debug, Clone, Copy)]
pub struct CollectionCandidate {
    /// Collection to look for membership in.
    pub collection: PersonhoodCollection,
    /// Ring-VRF entropy for this collection.
    pub entropy: [u8; 32],
}

/// Our provable ring membership in one collection.
#[derive(Debug)]
pub struct CollectionMembership {
    /// Entropy whose member key is included in `ring`.
    pub entropy: [u8; 32],
    /// Ring snapshot the membership proof is built against.
    pub ring: RingParams,
}

impl CollectionMembership {
    /// The collection this membership proves.
    pub fn collection(&self) -> PersonhoodCollection {
        self.ring.collection
    }
}

/// Find the newest ring in `collection` (scanning up to `lookback` back from the
/// current index) that includes our member key. Reads the ring exponent once and
/// stops at the first match. Every read is pinned to one finalized block so the
/// snapshot is internally consistent; the pinned hash is recorded on the
/// returned [`RingParams`].
pub async fn find_including_ring(
    rpc: &RpcClient,
    metadata: &Metadata,
    collection: PersonhoodCollection,
    entropy: [u8; 32],
    lookback: u32,
) -> Result<Option<RingParams>, StatementAllowanceError> {
    let member = proof::member_key(entropy);
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
    candidates: &[CollectionCandidate],
    lookback: u32,
) -> Result<Vec<CollectionMembership>, StatementAllowanceError> {
    let mut memberships = Vec::new();
    let mut first_error = None;
    for candidate in candidates {
        let collection = candidate.collection;
        match find_including_ring(rpc, metadata, collection, candidate.entropy, lookback).await {
            Ok(Some(ring)) => {
                // A ring whose exponent has no proof domain cannot be proved
                // against, so it must not enter the set: selecting it would fail
                // after the collection was already chosen, with no fallback left.
                if let Err(err) = proof::domain_for_ring_exponent(ring.exponent) {
                    warn!(%collection, %err, "unusable ring exponent; skipping collection");
                    continue;
                }
                memberships.push(CollectionMembership {
                    entropy: candidate.entropy,
                    ring,
                });
            }
            Ok(None) => debug!(%collection, "no ring includes our member key"),
            // One collection's failure must not take down the others. A device
            // that can only prove light personhood should still get its
            // allowance when the full-personhood storage is unreadable.
            Err(err) => {
                warn!(%collection, %err, "could not resolve this collection");
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }
    // Every candidate erroring is an outage, not an answer: reporting "not a
    // member" there would let a caller conclude the person has no personhood.
    match (memberships.is_empty(), first_error) {
        (true, Some(err)) => Err(err),
        _ => Ok(memberships),
    }
}

/// Register statement-store allowance for `target`, proving membership in the
/// already-located `ring`, at UTC-day `period`.
pub async fn register_statement_account(
    rpc: &RpcClient,
    metadata: &Metadata,
    chain_state: &ChainState,
    entropy: [u8; 32],
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
                    entropy,
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
                }
                SlotSelection::Free(seq) => seq,
                SlotSelection::FreeSlotsExcluded => {
                    // A free slot exists; it is only held back by one of this
                    // call's own in-flight submissions. Evicting a live slot
                    // instead would revoke an allowance for no reason.
                    return Err(SlotError::FreeSlotsAwaitingSubmission {
                        period: params.period,
                    }
                    .into());
                }
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
                        }
                        None => {
                            return Err(SlotError::NoFreeStatementStoreSlot {
                                period: params.period,
                                max,
                            }
                            .into());
                        }
                    }
                }
            },
        };

        let context = slot::derive_slot_context(params.network_suffix, params.period, seq);
        let call = extrinsic::build_set_statement_store_account_call(
            metadata,
            params.period,
            seq,
            params.target,
        )?;
        let message = extension::build_proof_message(metadata, &call, chain_state)?;
        let domain = proof::domain_for_ring_exponent(params.ring.exponent)?;
        let ring_proof =
            proof::ring_vrf_proof(domain, entropy, &params.ring.members, &context, &message)?;
        let as_resources_extra = extrinsic::build_as_resources_extra(
            metadata,
            &ring_proof,
            params.ring.ring_index,
            revision,
            collection,
        )?;
        let extrinsic =
            extrinsic::build_unsigned_extrinsic(metadata, chain_state, &call, &as_resources_extra)?;

        match rpc.submit_and_watch(&extrinsic).await {
            Ok(block_hash) => {
                if slot::read_slot_account_at(
                    rpc,
                    entropy,
                    params.network_suffix,
                    params.period,
                    seq,
                    &block_hash,
                )
                .await?
                    != Some(*params.target)
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
            }
            Err(err) if duplicate_submit_error(&err.to_string()) => {
                skipped_duplicate_slots.push(seq);
            }
            Err(err) if took_over_a_slot && invalid_transaction_error(&err.to_string()) => {
                // The runtime re-checks the replacement cooldown against its own
                // clock and rejects at validation, so a takeover can lose a race
                // it looked eligible for. Name that rather than surfacing a raw
                // RPC failure the caller cannot act on.
                return Err(SlotError::ReplacementRefused {
                    period: params.period,
                    seq,
                }
                .into());
            }
            Err(err) => return Err(err),
        }
    }
}

/// One collection's scan of a period's slot table.
pub struct CollectionScan {
    /// Collection scanned.
    pub collection: PersonhoodCollection,
    /// Entropy whose aliases were read.
    pub entropy: [u8; 32],
    /// What the scan found.
    pub selection: SlotSelection,
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
    candidates: &[CollectionCandidate],
    network_suffix: &[u8],
    period: u32,
    target: &[u8; 32],
    reuse_existing: bool,
) -> Result<Vec<CollectionScan>, StatementAllowanceError> {
    let mut scans = Vec::new();
    for candidate in candidates {
        let collection = candidate.collection;
        if !collection.is_supported(metadata) {
            debug!(%collection, "chain declares no slot budget for this collection");
            continue;
        }
        let selection = slot::scan_slot_excluding(
            rpc,
            metadata,
            slot::SlotScan {
                collection,
                entropy: candidate.entropy,
                network_suffix,
                period,
                target,
                excluded: &[],
                reuse_existing,
            },
        )
        .await;
        match selection {
            Ok(selection) => {
                // An allowance already held settles the question, so the
                // remaining collections are reads nobody needs.
                let settled = matches!(selection, SlotSelection::AlreadyAllocated(_));
                scans.push(CollectionScan {
                    collection,
                    entropy: candidate.entropy,
                    selection,
                });
                if settled {
                    break;
                }
            }
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
    chain_state: &ChainState,
    scans: &[CollectionScan],
    memberships: &[CollectionMembership],
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
        let collection = membership.collection();
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
            }
            SlotSelection::Full { occupied, .. } => full.push((index, occupied)),
            // A free slot held back by this call's own in-flight submission.
            // Replacing a live slot instead would revoke an allowance for
            // capacity that is about to come back.
            SlotSelection::FreeSlotsExcluded => {
                return Err(SlotError::FreeSlotsAwaitingSubmission {
                    period: params.period,
                }
                .into());
            }
            // Handled above, before any collection was chosen.
            SlotSelection::AlreadyAllocated(seq) => {
                return Ok(RegistrationOutcome::AlreadyAllocated {
                    seq: *seq,
                    collection,
                });
            }
        }
    }
    if usable == 0 {
        return Err(SlotError::NoCollectionMembership.into());
    }

    let exhausted = || SlotError::NoFreeStatementStoreSlot {
        period: params.period,
        max: budget,
    };

    let (index, choice) = match free {
        Some((index, seq)) => (index, Preselected::Free(seq)),
        None if !params.allow_eviction => return Err(exhausted().into()),
        None => {
            let cooldown = slot::replacement_cooldown(rpc, metadata).await?;
            let chain_now = slot::read_chain_now_seconds(rpc).await?;
            let mut oldest: Option<(usize, u32, u64)> = None;
            for (index, occupied) in &full {
                let protected = protected_in(memberships[*index].collection());
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
        }
    };

    let membership = &memberships[index];
    let protected = protected_in(membership.collection());
    register_statement_account(
        rpc,
        metadata,
        chain_state,
        membership.entropy,
        RegistrationParams {
            target: params.target,
            period: params.period,
            network_suffix: params.network_suffix,
            ring: &membership.ring,
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
        }
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
        chain_state,
        entropy,
        network_suffix,
        target,
        period,
        ring,
    } = params;
    let revision = ring::read_ring_revision(
        rpc,
        metadata,
        ring.collection,
        ring.ring_index,
        &ring.block_hash,
    )
    .await?;
    let mut skipped_duplicate_counters = Vec::new();
    loop {
        let counter = slot::scan_long_term_storage_counter_excluding(
            rpc,
            metadata,
            entropy,
            network_suffix,
            period,
            &skipped_duplicate_counters,
        )
        .await?;

        let context = slot::derive_long_term_storage_context(network_suffix, period, counter);
        let call =
            extrinsic::build_claim_long_term_storage_call(metadata, period, counter, target)?;
        let message = extension::build_proof_message(metadata, &call, chain_state)?;
        let domain = proof::domain_for_ring_exponent(ring.exponent)?;
        let ring_proof = proof::ring_vrf_proof(domain, entropy, &ring.members, &context, &message)?;
        let as_resources_extra = extrinsic::build_long_term_storage_extra(
            metadata,
            &ring_proof,
            ring.ring_index,
            revision,
            ring.collection,
        )?;
        let extrinsic =
            extrinsic::build_unsigned_extrinsic(metadata, chain_state, &call, &as_resources_extra)?;
        debug!(
            period,
            counter,
            ring_index = ring.ring_index,
            revision,
            "submitting Bulletin long-term-storage claim"
        );

        match rpc.submit_and_watch(&extrinsic).await {
            Ok(block_hash) => {
                return Ok(LongTermStorageOutcome::Claimed {
                    block_hash,
                    counter,
                    ring_index: ring.ring_index,
                });
            }
            Err(err) if duplicate_submit_error(&err.to_string()) => {
                skipped_duplicate_counters.push(counter);
            }
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
            }
        }
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

/// Wait until Bulletin authorization is available and fresher than `current`.
pub async fn wait_bulletin_authorization(
    rpc: &RpcClient,
    target: &[u8; 32],
    current: Option<BulletinAllowanceInfo>,
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
            info.remained_transactions > current.remained_transactions
                || info.remained_size > current.remained_size
                || info.expires_in > current.expires_in
        }
    }
}

/// `TransactionStorage.Authorizations[AuthorizationScope::Account(target)]`.
fn bulletin_authorization_key(target: &[u8; 32]) -> Vec<u8> {
    let scope = BulletinAuthorizationScope::Account(*target).encode();
    [
        twox_128(b"TransactionStorage").as_slice(),
        twox_128(b"Authorizations").as_slice(),
        &ring::blake2_128_concat(&scope),
    ]
    .concat()
}

fn decode_bulletin_allowance(
    bytes: &[u8],
    fetched_at: u32,
) -> Result<BulletinAllowanceInfo, StatementAllowanceError> {
    let mut input = bytes;
    let transactions =
        u32::decode(&mut input).map_err(|err| ChainStateError::AuthorizationFieldDecode {
            field: "transactions",
            source: err,
        })?;
    let transactions_allowance =
        u32::decode(&mut input).map_err(|err| ChainStateError::AuthorizationFieldDecode {
            field: "transactions_allowance",
            source: err,
        })?;
    let bytes_used =
        u64::decode(&mut input).map_err(|err| ChainStateError::AuthorizationFieldDecode {
            field: "bytes",
            source: err,
        })?;
    let _bytes_permanent =
        u64::decode(&mut input).map_err(|err| ChainStateError::AuthorizationFieldDecode {
            field: "bytes_permanent",
            source: err,
        })?;
    let bytes_allowance =
        u64::decode(&mut input).map_err(|err| ChainStateError::AuthorizationFieldDecode {
            field: "bytes_allowance",
            source: err,
        })?;
    let expires_in =
        u32::decode(&mut input).map_err(|err| ChainStateError::AuthorizationFieldDecode {
            field: "expiration",
            source: err,
        })?;
    Ok(BulletinAllowanceInfo {
        remained_size: bytes_allowance.saturating_sub(bytes_used),
        remained_transactions: transactions_allowance.saturating_sub(transactions),
        expires_in,
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
mod tests {
    use subxt_rpcs::RpcClient as HostRpcClient;

    use super::rpc::testing::ScriptedRpc;
    use super::*;

    /// Fixture metadata captured from paseo-next-v2 (raw `RuntimeMetadataPrefixed`).
    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/paseo-next-v2-metadata.scale");

    fn allowance(
        remained_size: u64,
        remained_transactions: u32,
        expires_in: u32,
    ) -> BulletinAllowanceInfo {
        BulletinAllowanceInfo {
            remained_size,
            remained_transactions,
            expires_in,
            fetched_at: 10,
        }
    }

    #[test]
    fn bulletin_refresh_accepts_available_state_when_baseline_was_unusable() {
        let exhausted_by_size = allowance(0, 4, 100);
        let refreshed_same_transactions = allowance(4096, 4, 100);

        assert!(!exhausted_by_size.available());
        assert!(authorization_refreshed(
            refreshed_same_transactions,
            Some(exhausted_by_size).filter(|info| info.available()),
        ));
    }

    #[test]
    fn bulletin_refresh_accepts_size_only_increase() {
        let baseline = allowance(128, 4, 100);
        let refreshed = allowance(4096, 4, 100);

        assert!(authorization_refreshed(refreshed, Some(baseline)));
    }

    #[test]
    fn bulletin_refresh_rejects_unchanged_available_state() {
        let baseline = allowance(128, 4, 100);

        assert!(!authorization_refreshed(baseline, Some(baseline)));
    }

    #[test]
    fn banned_submissions_are_not_classified_as_duplicates() {
        let classified: Vec<bool> = [
            "Priority is too low: (100 vs 100)",
            "Transaction Already Imported",
            "Transaction is temporarily banned",
            "Invalid Transaction",
        ]
        .into_iter()
        .map(duplicate_submit_error)
        .collect();

        assert_eq!(classified, vec![true, true, false, false]);
    }

    #[test]
    fn bulletin_account_scope_matches_runtime_enum_codec() {
        let scope = BulletinAuthorizationScope::Account([0x42; 32]);
        let encoded = scope.encode();

        assert_eq!(encoded, [vec![0x00], vec![0x42; 32]].concat());
        assert_eq!(
            BulletinAuthorizationScope::decode(&mut encoded.as_slice()).unwrap(),
            scope
        );
    }

    /// A `state_getRuntimeVersion` result for `spec_version`.
    fn runtime_version(spec_version: u32) -> String {
        format!(r#"{{"specVersion":{spec_version},"transactionVersion":1}}"#)
    }

    /// The fixture metadata as a `state_getMetadata` hex result.
    fn metadata_result() -> String {
        format!(r#""0x{}""#, hex::encode(FIXTURE))
    }

    /// `Metadata_metadata_at_version(16)` answering `None`, so the caller falls
    /// back to `state_getMetadata`. These tests are about caching, not about
    /// which metadata version a runtime serves.
    fn metadata_version_unavailable() -> String {
        r#""0x00""#.to_string()
    }

    /// A `chain_getBlockHash(0)` result for `genesis_hash`.
    fn genesis_result(genesis_hash: [u8; 32]) -> String {
        format!(r#""0x{}""#, hex::encode(genesis_hash))
    }

    /// Method names the scripted transport saw, in order.
    fn methods(scripted: &ScriptedRpc) -> Vec<String> {
        scripted
            .calls()
            .into_iter()
            .map(|(method, _)| method)
            .collect()
    }

    /// The requests one cache miss makes, in order. The two validation reads
    /// are issued together, so both happen whether or not the entry is reused.
    const MISS: [&str; 4] = [
        "state_getRuntimeVersion",
        "chain_getBlockHash",
        // The V16 runtime call is tried first; these scripts answer it as absent,
        // so the legacy fetch follows.
        "state_call",
        "state_getMetadata",
    ];
    /// The requests one cache hit makes: validation only, no metadata download.
    const HIT: [&str; 2] = ["state_getRuntimeVersion", "chain_getBlockHash"];

    /// One call's worth of scripted responses: the two validation reads plus,
    /// when the entry has to be built, the metadata download.
    fn call_script(spec_version: u32, reported: [u8; 32], downloads: bool) -> Vec<String> {
        let mut script = vec![runtime_version(spec_version), genesis_result(reported)];
        if downloads {
            script.push(metadata_version_unavailable());
            script.push(metadata_result());
        }
        script
    }

    /// Drive `ChainContextCache::get` over `responses`, fetching each entry in
    /// `chains` in turn, and report the methods the transport saw.
    fn scripted_cache_run(responses: &[String], chains: &[[u8; 32]]) -> Vec<String> {
        let scripted = ScriptedRpc::new(responses.iter().map(String::as_str));
        let cache = ChainContextCache::default();

        futures::executor::block_on(async {
            for genesis_hash in chains {
                let client = ChainClient::new(
                    RpcClient::new(HostRpcClient::new(scripted.clone())),
                    *genesis_hash,
                );
                cache
                    .get(&client)
                    .await
                    .expect("scripted chain context fetch succeeds");
            }
        });
        methods(&scripted)
    }

    /// Transport that answers by method rather than in order, and yields inside
    /// every request so concurrent cache misses genuinely interleave. Counts the
    /// metadata downloads, which is the cost the cache exists to avoid.
    #[derive(Clone, Default)]
    struct CountingRpc(std::sync::Arc<CountingState>);

    #[derive(Default)]
    struct CountingState {
        metadata_downloads: std::sync::atomic::AtomicUsize,
    }

    impl CountingRpc {
        fn metadata_downloads(&self) -> usize {
            self.0
                .metadata_downloads
                .load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    impl subxt_rpcs::client::RpcClientT for CountingRpc {
        fn request_raw<'a>(
            &'a self,
            method: &'a str,
            _params: Option<Box<subxt_rpcs::client::RawValue>>,
        ) -> subxt_rpcs::client::RawRpcFuture<'a, Box<subxt_rpcs::client::RawValue>> {
            let body = match method {
                "state_getRuntimeVersion" => runtime_version(1_000_000),
                "chain_getBlockHash" => genesis_result([0xaa; 32]),
                "state_call" => metadata_version_unavailable(),
                "state_getMetadata" => {
                    self.0
                        .metadata_downloads
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    metadata_result()
                }
                other => panic!("unexpected request `{other}`"),
            };
            Box::pin(async move {
                // Yield so a concurrently polled caller reaches the same point.
                let mut yielded = false;
                futures::future::poll_fn(move |cx| {
                    if yielded {
                        core::task::Poll::Ready(())
                    } else {
                        yielded = true;
                        cx.waker().wake_by_ref();
                        core::task::Poll::Pending
                    }
                })
                .await;
                Ok(subxt_rpcs::client::RawValue::from_string(body)
                    .expect("scripted response is valid JSON"))
            })
        }

        fn subscribe_raw<'a>(
            &'a self,
            _sub: &'a str,
            _params: Option<Box<subxt_rpcs::client::RawValue>>,
            _unsub: &'a str,
        ) -> subxt_rpcs::client::RawRpcFuture<'a, subxt_rpcs::client::RawRpcSubscription> {
            unreachable!("the chain context cache does not subscribe")
        }
    }

    #[test]
    fn concurrent_misses_download_the_metadata_once() {
        let counting = CountingRpc::default();
        let client = ChainClient::new(
            RpcClient::new(HostRpcClient::new(counting.clone())),
            [0xaa; 32],
        );
        let cache = ChainContextCache::default();

        futures::executor::block_on(async {
            let (first, second) = futures::join!(cache.get(&client), cache.get(&client));
            let first = first.expect("first read");
            let second = second.expect("second read");
            // Both callers end up on the same entry.
            assert!(Arc::ptr_eq(&first.metadata, &second.metadata));
        });

        assert_eq!(counting.metadata_downloads(), 1);
    }

    #[test]
    fn a_chain_is_read_once_per_spec_version() {
        let seen = scripted_cache_run(
            &[
                call_script(1_000_000, [0xaa; 32], true),
                call_script(1_000_000, [0xaa; 32], false),
            ]
            .concat(),
            &[[0xaa; 32], [0xaa; 32]],
        );

        assert_eq!(seen, [MISS.as_slice(), HIT.as_slice()].concat());
    }

    #[test]
    fn a_wipe_that_keeps_the_spec_version_refreshes_the_entry() {
        // A chain redeployed from the same runtime keeps its spec version and
        // gets a new genesis. Validating on the spec version alone would reuse
        // the entry and sign `CheckGenesis` for the chain that no longer exists.
        let seen = scripted_cache_run(
            &[
                call_script(1_000_000, [0xaa; 32], true),
                call_script(1_000_000, [0xcc; 32], true),
            ]
            .concat(),
            &[[0xaa; 32], [0xaa; 32]],
        );

        assert_eq!(seen, [MISS, MISS].concat());
    }

    #[test]
    fn a_wipe_is_reflected_in_the_signed_genesis_hash() {
        let responses = [
            call_script(1_000_000, [0xaa; 32], true),
            call_script(1_000_000, [0xcc; 32], true),
        ]
        .concat();
        let scripted = ScriptedRpc::new(responses.iter().map(String::as_str));
        let client = ChainClient::new(RpcClient::new(HostRpcClient::new(scripted)), [0xaa; 32]);
        let cache = ChainContextCache::default();

        futures::executor::block_on(async {
            let before = cache.get(&client).await.expect("first read");
            assert_eq!(before.state.genesis_hash, [0xaa; 32]);

            let after = cache.get(&client).await.expect("read after a wipe");
            assert_eq!(after.state.genesis_hash, [0xcc; 32]);
        });
    }

    #[test]
    fn a_spec_version_bump_refreshes_the_entry() {
        let seen = scripted_cache_run(
            &[
                call_script(1_000_000, [0xaa; 32], true),
                call_script(1_000_001, [0xaa; 32], true),
            ]
            .concat(),
            &[[0xaa; 32], [0xaa; 32]],
        );

        assert_eq!(seen, [MISS, MISS].concat());
    }

    #[test]
    fn chains_do_not_share_a_cache_entry() {
        let seen = scripted_cache_run(
            &[
                call_script(1_000_000, [0xaa; 32], true),
                call_script(1_000_000, [0xbb; 32], true),
            ]
            .concat(),
            &[[0xaa; 32], [0xbb; 32]],
        );

        assert_eq!(seen, [MISS, MISS].concat());
    }

    #[test]
    fn a_stale_configured_genesis_still_yields_the_chains_own_hash() {
        let responses = call_script(1_000_000, [0xbb; 32], true);
        let scripted = ScriptedRpc::new(responses.iter().map(String::as_str));
        let client = ChainClient::new(RpcClient::new(HostRpcClient::new(scripted)), [0xaa; 32]);
        let cache = ChainContextCache::default();

        let context = futures::executor::block_on(cache.get(&client))
            .expect("a stale configured genesis is not fatal");

        // `CheckGenesis` is signed over this, so it must be the chain's value,
        // not the caller's possibly-stale constant.
        assert_eq!(context.state.genesis_hash, [0xbb; 32]);
    }

    #[test]
    fn a_stale_configured_genesis_still_keys_the_cache() {
        let seen = scripted_cache_run(
            &[
                call_script(1_000_000, [0xbb; 32], true),
                call_script(1_000_000, [0xbb; 32], false),
            ]
            .concat(),
            &[[0xaa; 32], [0xaa; 32]],
        );

        assert_eq!(seen, [MISS.as_slice(), HIT.as_slice()].concat());
    }

    /// `StmtStoreAllowanceEntry { account_id, seq: 0, since: 0 }` as a scripted
    /// JSON storage result.
    /// Fixed clock for the scripted registration tests.
    const NOW: u64 = 10_000_000;

    fn slot_entry(account: [u8; 32]) -> String {
        slot_entry_since(account, 0)
    }

    /// A scripted slot entry that was set at `since`.
    fn slot_entry_since(account: [u8; 32], since: u64) -> String {
        let entry = (account, 0u32, since).encode();
        format!(r#""0x{}""#, hex::encode(entry))
    }

    /// `Timestamp.Now` as a scripted storage result: unix seconds in millis.
    fn chain_clock(seconds: u64) -> String {
        format!(r#""0x{}""#, hex::encode((seconds * 1_000).encode()))
    }

    /// Run `register_statement_account` against a scripted chain: all ten
    /// slots free, the extrinsic reaches block `0xb10c`, and the verification
    /// read at that block returns `verified_entry`.
    fn scripted_registration(
        verified_entry: &str,
    ) -> (
        Result<RegistrationOutcome, StatementAllowanceError>,
        ScriptedRpc,
    ) {
        scripted_registration_with(verified_entry, None, 10)
    }

    /// Run `register_statement_account` against a scripted chain: `free_slots`
    /// slots answered as free, the extrinsic reaching block `0xb10c`, and the
    /// verification read at that block returning `verified_entry`.
    fn scripted_registration_with(
        verified_entry: &str,
        preselected: Option<Preselected>,
        free_slots: usize,
    ) -> (
        Result<RegistrationOutcome, StatementAllowanceError>,
        ScriptedRpc,
    ) {
        let metadata = test_fixtures::people();
        let chain_state = ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        };
        let entropy = [0x11; 32];
        let ring = RingParams {
            collection: PersonhoodCollection::LitePeople,
            members: vec![proof::member_key(entropy)],
            exponent: 9,
            ring_index: 0,
            block_hash: "0xfinal".to_string(),
        };

        // The ring-revision read comes first (absent => revision 0), then the
        // slot scan, then the post-submit verification read.
        let mut responses = vec!["null"; free_slots + 1];
        responses.push(verified_entry);
        let scripted = ScriptedRpc::new(responses);
        scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let outcome = futures::executor::block_on(register_statement_account(
            &rpc,
            metadata,
            &chain_state,
            entropy,
            RegistrationParams {
                target: &[0x22; 32],
                period: 7,
                network_suffix: b"paseo",
                ring: &ring,
                reuse_existing: true,
                preselected,
                protected: &[],
            },
        ));
        (outcome, scripted)
    }

    /// Both collections, People first, with a distinct entropy each.
    fn pooled_memberships() -> [CollectionMembership; 2] {
        PersonhoodCollection::ALL.map(|collection| {
            let entropy = match collection {
                PersonhoodCollection::People => [0x31; 32],
                PersonhoodCollection::LitePeople => [0x11; 32],
            };
            CollectionMembership {
                entropy,
                ring: RingParams {
                    collection,
                    members: vec![proof::member_key(entropy)],
                    exponent: 9,
                    ring_index: 0,
                    block_hash: "0xfinal".to_string(),
                },
            }
        })
    }

    /// One occupied slot entry.
    fn occupied(account: [u8; 32], since: u64) -> String {
        format!(r#""0x{}""#, hex::encode((account, 0u32, since).encode()))
    }

    /// Candidates matching [`pooled_memberships`], for the scan pass.
    fn pooled_candidates() -> [CollectionCandidate; 2] {
        pooled_memberships().map(|membership| CollectionCandidate {
            collection: membership.collection(),
            entropy: membership.entropy,
        })
    }

    /// Scan both collections then register from that scan, over `responses`.
    fn scripted_pooled(
        responses: Vec<String>,
        target: [u8; 32],
        protected: &[(PersonhoodCollection, u32)],
        allow_eviction: bool,
    ) -> (
        Result<RegistrationOutcome, StatementAllowanceError>,
        ScriptedRpc,
    ) {
        let metadata = test_fixtures::people();
        let chain_state = ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        };
        let memberships = pooled_memberships();
        let candidates = pooled_candidates();
        let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
        scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let outcome = futures::executor::block_on(async {
            let scans =
                scan_collections(&rpc, metadata, &candidates, b"paseo", 7, &target, true).await?;
            register_statement_account_pooled(
                &rpc,
                metadata,
                &chain_state,
                &scans,
                &memberships,
                PooledRegistrationParams {
                    target: &target,
                    period: 7,
                    network_suffix: b"paseo",
                    reuse_existing: true,
                    allow_eviction,
                    protected,
                },
            )
            .await
        });
        (outcome, scripted)
    }

    /// Scan then register, with the submission failing with `submit_error`.
    fn scripted_pooled_with_submit_error(
        responses: Vec<String>,
        target: [u8; 32],
        submit_error: &str,
    ) -> Result<RegistrationOutcome, StatementAllowanceError> {
        let metadata = test_fixtures::people();
        let chain_state = ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        };
        let memberships = pooled_memberships();
        let candidates = pooled_candidates();
        let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
        scripted.script_subscription_errors(submit_error, 1);
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        futures::executor::block_on(async {
            let scans =
                scan_collections(&rpc, metadata, &candidates, b"paseo", 7, &target, true).await?;
            register_statement_account_pooled(
                &rpc,
                metadata,
                &chain_state,
                &scans,
                &memberships,
                PooledRegistrationParams {
                    target: &target,
                    period: 7,
                    network_suffix: b"paseo",
                    reuse_existing: true,
                    allow_eviction: true,
                    protected: &[],
                },
            )
            .await
        })
    }

    /// Slot tables for both collections: `people` then `lite`, each entry either
    /// free or occupied.
    fn tables(people: Vec<Option<String>>, lite: Vec<Option<String>>) -> Vec<String> {
        assert_eq!(people.len(), 20, "People declares 20 slots in the fixture");
        assert_eq!(
            lite.len(),
            10,
            "LitePeople declares 10 slots in the fixture"
        );
        people
            .into_iter()
            .chain(lite)
            .map(|entry| entry.unwrap_or_else(|| "null".to_string()))
            .collect()
    }

    /// The capacity this change exists for: a full People table must not report
    /// exhaustion while LitePeople still has a free seq.
    #[test]
    fn a_full_people_table_falls_back_to_a_free_lite_people_slot() {
        let target = [0x22; 32];
        let mut responses = tables(
            (0..20)
                .map(|seq| Some(occupied([0x99; 32], 1_000 + seq)))
                .collect(),
            (0..10).map(|_| None).collect(),
        );
        // Ring revision, then the post-submit verification read.
        responses.push("null".to_string());
        responses.push(occupied(target, 5_000));

        let (outcome, _) = scripted_pooled(responses, target, &[], true);

        let RegistrationOutcome::Registered {
            seq, collection, ..
        } = outcome.expect("a free LitePeople slot is registrable")
        else {
            panic!("expected a registration");
        };
        assert_eq!(collection, PersonhoodCollection::LitePeople);
        assert_eq!(seq, 0);
    }

    /// A free People slot wins over a free LitePeople one, so a full person
    /// spends the wider budget first and keeps the light one as headroom.
    #[test]
    fn a_free_people_slot_is_preferred_to_a_free_lite_people_slot() {
        let target = [0x22; 32];
        let mut responses = tables(
            (0..20).map(|_| None).collect(),
            (0..10).map(|_| None).collect(),
        );
        responses.push("null".to_string());
        responses.push(occupied(target, 5_000));

        let (outcome, _) = scripted_pooled(responses, target, &[], true);

        let RegistrationOutcome::Registered { collection, .. } =
            outcome.expect("a free People slot is registrable")
        else {
            panic!("expected a registration");
        };
        assert_eq!(collection, PersonhoodCollection::People);
    }

    /// Holding a slot in one collection must not earn a second one in another,
    /// even though every People slot here is free.
    #[test]
    fn an_allocation_in_lite_people_blocks_a_second_one_in_people() {
        let target = [0x22; 32];
        let responses = tables(
            (0..20).map(|_| None).collect(),
            (0..10)
                .map(|seq| (seq == 3).then(|| occupied(target, 4_000)))
                .collect(),
        );

        let (outcome, scripted) = scripted_pooled(responses, target, &[], true);

        assert!(
            matches!(
                outcome,
                Ok(RegistrationOutcome::AlreadyAllocated {
                    seq: 3,
                    collection: PersonhoodCollection::LitePeople
                })
            ),
            "expected the existing LitePeople slot to be reported: {outcome:?}"
        );
        assert!(
            scripted
                .calls()
                .iter()
                .all(|(method, _)| method != "author_submitAndWatchExtrinsic"),
            "nothing should have been submitted"
        );
    }

    /// With every collection full, the victim is the globally oldest replaceable
    /// slot, not the oldest inside whichever collection was scanned first.
    #[test]
    fn eviction_takes_the_globally_oldest_slot_across_collections() {
        let target = [0x22; 32];
        // Every People slot is newer than every LitePeople slot.
        let mut responses = tables(
            (0..20)
                .map(|seq| Some(occupied([0x99; 32], 9_000 + seq)))
                .collect(),
            (0..10)
                .map(|seq| Some(occupied([0x98; 32], 1_000 + seq)))
                .collect(),
        );
        responses.push(chain_clock(10_000_000));
        responses.push("null".to_string());
        responses.push(occupied(target, 10_000_000));

        let (outcome, _) = scripted_pooled(responses, target, &[], true);

        let RegistrationOutcome::Registered {
            seq, collection, ..
        } = outcome.expect("an old LitePeople slot is replaceable")
        else {
            panic!("expected a registration");
        };
        assert_eq!(collection, PersonhoodCollection::LitePeople);
        assert_eq!(seq, 0, "seq 0 is the oldest LitePeople slot");
    }

    /// `protected` is per collection: the same `seq` in two collections is two
    /// unrelated slots, so protecting one must leave the other evictable.
    #[test]
    fn protection_does_not_leak_across_collections() {
        let target = [0x22; 32];
        // People holds the oldest slots, so the victim should be People seq 0.
        // LitePeople seq 0 is protected, and that must not shield People seq 0.
        let mut responses = tables(
            (0..20)
                .map(|seq| Some(occupied([0x99; 32], 1_000 + seq)))
                .collect(),
            (0..10)
                .map(|seq| Some(occupied([0x98; 32], 9_000 + seq)))
                .collect(),
        );
        responses.push(chain_clock(10_000_000));
        responses.push("null".to_string());
        responses.push(occupied(target, 10_000_000));

        let (outcome, _) = scripted_pooled(
            responses,
            target,
            &[(PersonhoodCollection::LitePeople, 0)],
            true,
        );

        let RegistrationOutcome::Registered {
            seq, collection, ..
        } = outcome.expect("People seq 0 is still replaceable")
        else {
            panic!("expected a registration");
        };
        assert_eq!(collection, PersonhoodCollection::People);
        assert_eq!(
            seq, 0,
            "protecting LitePeople seq 0 must not protect People seq 0"
        );
    }

    /// Exhaustion has to name the pooled budget. Reporting one collection's share
    /// is what made a full person look capped at ten.
    #[test]
    fn exhaustion_names_the_summed_budget() {
        let target = [0x22; 32];
        // Everything full and everything inside its cooldown, so nothing is
        // replaceable and the error is raised.
        let mut responses = tables(
            (0..20)
                .map(|_| Some(occupied([0x99; 32], 9_999_999)))
                .collect(),
            (0..10)
                .map(|_| Some(occupied([0x98; 32], 9_999_999)))
                .collect(),
        );
        responses.push(chain_clock(10_000_000));

        let (outcome, _) = scripted_pooled(responses, target, &[], true);

        let err = outcome.expect_err("a full pool cannot register");
        assert!(
            matches!(
                err,
                StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
                    period: 7,
                    max: 30
                })
            ),
            "expected the pooled budget of 20 + 10: {err:?}"
        );
    }

    /// A takeover chosen by the pooled caller still spends this call's single
    /// revocation, so a refused submission has to be named rather than surfacing
    /// as a raw RPC string. The pooled path is the shipping path, so testing this
    /// only through a direct call with no preselection misses it entirely.
    #[test]
    fn a_refused_takeover_is_named_on_the_pooled_path() {
        let target = [0x22; 32];
        // Everything full and old enough to replace, so the pool evicts.
        let mut responses = tables(
            (0..20)
                .map(|seq| Some(occupied([0x99; 32], 1_000 + seq)))
                .collect(),
            (0..10)
                .map(|seq| Some(occupied([0x98; 32], 5_000 + seq)))
                .collect(),
        );
        responses.push(chain_clock(10_000_000));
        responses.push("null".to_string());

        let err = scripted_pooled_with_submit_error(
            responses,
            target,
            "User error: Invalid Transaction (1010)",
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("not replaceable yet"),
            "a refused pooled takeover should be named, got: {err}"
        );
    }

    /// One registration revokes at most one allowance. A duplicate-submit retry
    /// after a pooled takeover must not evict a second slot, because the first
    /// submission may still land.
    #[test]
    fn a_pooled_takeover_does_not_evict_a_second_slot_on_retry() {
        let target = [0x22; 32];
        let mut responses = tables(
            (0..20)
                .map(|seq| Some(occupied([0x99; 32], 1_000 + seq)))
                .collect(),
            (0..10)
                .map(|seq| Some(occupied([0x98; 32], 5_000 + seq)))
                .collect(),
        );
        responses.push(chain_clock(10_000_000));
        responses.push("null".to_string());
        // The retry rescans this collection and finds it still full.
        responses.extend((0..20).map(|seq| occupied([0x99; 32], 1_000 + seq)));

        // A duplicate-submit rejection drives the retry.
        let err = scripted_pooled_with_submit_error(responses, target, "Priority is too low")
            .unwrap_err();

        // Exhaustion, not a second eviction, and it names the pooled budget.
        assert!(
            matches!(
                err,
                StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
                    period: 7,
                    max: 30
                })
            ),
            "a retry after a takeover should give up rather than evict again: {err:?}"
        );
    }

    /// A real `Members.Collections[LitePeople]` value from the live People chain.
    /// Hand-encoding one means encoding `CollectionOwner` and `RingMode` too, and
    /// a wrong guess would make the test pass for the wrong reason.
    const LIVE_LITE_COLLECTION: &str = r#""0x000001043e000900""#;

    /// The regression this guards: a device that can only prove light personhood
    /// must still get its allowance when the full-personhood storage is broken.
    /// Resolving every collection in one fallible pass made any People-side
    /// failure abort the whole allowance path, including for lite-only devices
    /// that never needed People at all.
    #[test]
    fn a_broken_people_collection_does_not_discard_a_lite_people_membership() {
        let metadata = test_fixtures::people();
        let candidates = pooled_memberships().map(|membership| CollectionCandidate {
            collection: membership.collection(),
            entropy: membership.entropy,
        });
        let lite_entropy = candidates[1].entropy;
        let page = format!(r#""0x04{}""#, hex::encode(proof::member_key(lite_entropy)));

        let responses = [
            // People: a finalized head, then an undecodable Collections value.
            r#""0xfinal""#.to_string(),
            r#""0x00""#.to_string(),
            // LitePeople: resolves normally.
            r#""0xfinal""#.to_string(),
            LIVE_LITE_COLLECTION.to_string(),
            r#""0x00000000""#.to_string(),
            page,
            "null".to_string(),
            "null".to_string(),
        ];
        let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        let memberships = futures::executor::block_on(find_including_rings(
            &rpc,
            metadata,
            &candidates,
            u32::MAX,
        ))
        .expect("a broken People collection must not fail the whole resolution");

        assert_eq!(memberships.len(), 1, "LitePeople should still resolve");
        assert_eq!(
            memberships[0].collection(),
            PersonhoodCollection::LitePeople
        );
        assert_eq!(memberships[0].entropy, lite_entropy);
    }

    /// Every candidate failing is an outage, not an answer. Reporting it as "no
    /// membership" would let a caller conclude the person has no personhood.
    #[test]
    fn every_collection_failing_is_reported_as_an_error() {
        let metadata = test_fixtures::people();
        let candidates = pooled_memberships().map(|membership| CollectionCandidate {
            collection: membership.collection(),
            entropy: membership.entropy,
        });

        let responses = [
            r#""0xfinal""#.to_string(),
            r#""0x00""#.to_string(),
            r#""0xfinal""#.to_string(),
            r#""0x00""#.to_string(),
        ];
        let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        let err = futures::executor::block_on(find_including_rings(
            &rpc,
            metadata,
            &candidates,
            u32::MAX,
        ))
        .expect_err("no collection resolved, so this is a failure not an empty answer");
        assert!(
            matches!(err, StatementAllowanceError::Ring(_)),
            "unexpected error: {err:?}"
        );
    }

    /// Exhaustion raised by the inner rescan has to name the device's budget on
    /// the free-slot path too. Summing only the collections that reported `Full`
    /// makes that figure empty here, so the retry reported a budget of zero.
    #[test]
    fn a_retry_from_a_free_slot_still_names_the_summed_budget() {
        let target = [0x22; 32];
        // Both tables free, so the first choice is a free People slot.
        let mut responses = tables(
            (0..20).map(|_| None).collect(),
            (0..10).map(|_| None).collect(),
        );
        // Ring revision, then the rescan the duplicate-submit retry performs:
        // People is now full and every slot is inside its cooldown.
        responses.push("null".to_string());
        responses.extend((0..20).map(|_| occupied([0x99; 32], 9_999_999)));
        responses.push(chain_clock(10_000_000));

        let err = scripted_pooled_with_submit_error(responses, target, "Priority is too low")
            .unwrap_err();

        assert!(
            matches!(
                err,
                StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
                    period: 7,
                    max: 30
                })
            ),
            "expected the pooled budget of 20 + 10, not one branch's share: {err:?}"
        );
    }

    /// Connecting a product must not revoke another product's allowance. A full
    /// period is exhaustion, and reclaiming space is the renewal pass's job.
    #[test]
    fn on_demand_allocation_reports_exhaustion_rather_than_evicting() {
        let target = [0x22; 32];
        // Everything full and old enough that eviction would succeed if allowed.
        let responses = tables(
            (0..20)
                .map(|seq| Some(occupied([0x99; 32], 1_000 + seq)))
                .collect(),
            (0..10)
                .map(|seq| Some(occupied([0x98; 32], 1_000 + seq)))
                .collect(),
        );

        let (outcome, scripted) = scripted_pooled(responses, target, &[], false);

        let err = outcome.expect_err("a full pool cannot allocate without evicting");
        assert!(
            matches!(
                err,
                StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
                    period: 7,
                    max: 30
                })
            ),
            "unexpected error: {err:?}"
        );
        assert!(
            scripted
                .calls()
                .iter()
                .all(|(method, _)| method != "author_submitAndWatchExtrinsic"),
            "nothing should have been submitted"
        );
    }

    /// The scan pass isolates per-collection failures too. It runs before the
    /// membership resolution, so a failed People read here would fail the whole
    /// allocation for a device that could never hold a slot in People.
    #[test]
    fn a_failed_people_scan_still_finds_the_lite_people_allocation() {
        let metadata = test_fixtures::people();
        let candidates = pooled_candidates();
        let target = [0x22; 32];

        let responses = [
            // People: an undecodable storage value fails this collection's scan.
            r#""zz""#.to_string(),
            // LitePeople: the target holds seq 3.
            "null".to_string(),
            "null".to_string(),
            "null".to_string(),
            occupied(target, 4_000),
        ];
        let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        let scans = futures::executor::block_on(scan_collections(
            &rpc,
            metadata,
            &candidates,
            b"paseo",
            7,
            &target,
            true,
        ))
        .expect("a broken People scan must not fail the pass");

        assert_eq!(
            allocated_in(&scans),
            Some((PersonhoodCollection::LitePeople, 3)),
        );
    }

    /// Storage reads the scripted transport served.
    fn storage_reads(scripted: &ScriptedRpc) -> usize {
        scripted
            .calls()
            .iter()
            .filter(|(method, _)| method == "state_getStorage")
            .count()
    }

    /// A full table is no longer a dead end: the oldest slot the runtime allows
    /// taking is replaced, and the registration proceeds.
    #[test]
    fn a_full_table_replaces_the_oldest_replaceable_slot() {
        let metadata = test_fixtures::people();
        let chain_state = ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        };
        let entropy = [0x11; 32];
        let ring = RingParams {
            collection: PersonhoodCollection::LitePeople,
            members: vec![proof::member_key(entropy)],
            exponent: 9,
            ring_index: 0,
            block_hash: "0xfinal".to_string(),
        };

        // Revision read, then ten occupied slots where seq 3 is the oldest, then
        // the post-submit verification read.
        let mut owned = vec!["null".to_string()];
        owned.extend((0..10u64).map(|seq| {
            let since = if seq == 3 { 1_000 } else { NOW - 1_000 };
            slot_entry_since([0x99; 32], since)
        }));
        owned.push(chain_clock(NOW));
        owned.push(slot_entry([0x22; 32]));
        let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
        let scripted = ScriptedRpc::new(responses);
        scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let outcome = futures::executor::block_on(register_statement_account(
            &rpc,
            metadata,
            &chain_state,
            entropy,
            RegistrationParams {
                target: &[0x22; 32],
                period: 7,
                network_suffix: b"paseo",
                ring: &ring,
                reuse_existing: true,
                preselected: None,
                protected: &[],
            },
        ));

        assert!(
            matches!(
                outcome.unwrap(),
                RegistrationOutcome::Registered { seq: 3, .. }
            ),
            "the oldest occupied slot should have been replaced",
        );
    }

    /// A duplicate-submit retry must not revoke a second allowance: the first
    /// submission can still land, so two takeovers for one call can cost two.
    #[test]
    fn a_duplicate_submit_retry_does_not_take_over_a_second_slot() {
        let metadata = test_fixtures::people();
        let chain_state = ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        };
        let entropy = [0x11; 32];
        let ring = RingParams {
            collection: PersonhoodCollection::LitePeople,
            members: vec![proof::member_key(entropy)],
            exponent: 9,
            ring_index: 0,
            block_hash: "0xfinal".to_string(),
        };

        // Revision, ten occupied slots of differing age, the chain clock. The
        // submission then fails as a duplicate, and the rescan would otherwise
        // pick a second victim.
        let mut owned = vec!["null".to_string()];
        owned.extend((0..10u64).map(|seq| slot_entry_since([0x99; 32], 1_000 + seq)));
        owned.push(chain_clock(NOW));
        // Second pass through the loop: revision is cached, so scan then clock.
        owned.extend((0..10u64).map(|seq| slot_entry_since([0x99; 32], 1_000 + seq)));
        owned.push(chain_clock(NOW));
        let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
        let scripted = ScriptedRpc::new(responses);
        scripted.script_subscription_errors("already imported", 2);
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        let err = futures::executor::block_on(register_statement_account(
            &rpc,
            metadata,
            &chain_state,
            entropy,
            RegistrationParams {
                target: &[0x22; 32],
                period: 7,
                network_suffix: b"paseo",
                ring: &ring,
                reuse_existing: true,
                preselected: None,
                protected: &[],
            },
        ))
        .unwrap_err();

        assert!(
            err.to_string().contains("no free"),
            "a retry after a takeover should stop, not replace again: {err}"
        );
    }

    /// A takeover the runtime refuses is reported as such, not as a bare RPC
    /// failure: the host loses the race whenever the chain's clock disagrees.
    #[test]
    fn a_refused_takeover_is_named() {
        let metadata = test_fixtures::people();
        let chain_state = ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        };
        let entropy = [0x11; 32];
        let ring = RingParams {
            collection: PersonhoodCollection::LitePeople,
            members: vec![proof::member_key(entropy)],
            exponent: 9,
            ring_index: 0,
            block_hash: "0xfinal".to_string(),
        };

        let mut owned = vec!["null".to_string()];
        owned.extend((0..10u64).map(|seq| slot_entry_since([0x99; 32], 1_000 + seq)));
        owned.push(chain_clock(NOW));
        let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
        let scripted = ScriptedRpc::new(responses);
        scripted.script_subscription_errors("User error: Invalid Transaction (1010)", 1);
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        let err = futures::executor::block_on(register_statement_account(
            &rpc,
            metadata,
            &chain_state,
            entropy,
            RegistrationParams {
                target: &[0x22; 32],
                period: 7,
                network_suffix: b"paseo",
                ring: &ring,
                reuse_existing: true,
                preselected: None,
                protected: &[],
            },
        ))
        .unwrap_err();

        assert!(
            err.to_string().contains("not replaceable yet"),
            "unexpected error: {err}"
        );
    }

    /// Everything occupied and still inside the cooldown stays an error.
    #[test]
    fn a_full_table_within_the_cooldown_still_fails() {
        let metadata = test_fixtures::people();
        let chain_state = ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        };
        let entropy = [0x11; 32];
        let ring = RingParams {
            collection: PersonhoodCollection::LitePeople,
            members: vec![proof::member_key(entropy)],
            exponent: 9,
            ring_index: 0,
            block_hash: "0xfinal".to_string(),
        };

        let mut owned = vec!["null".to_string()];
        owned.extend((0..10).map(|_| slot_entry_since([0x99; 32], NOW - 10)));
        owned.push(chain_clock(NOW));
        let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
        let scripted = ScriptedRpc::new(responses);
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        let err = futures::executor::block_on(register_statement_account(
            &rpc,
            metadata,
            &chain_state,
            entropy,
            RegistrationParams {
                target: &[0x22; 32],
                period: 7,
                network_suffix: b"paseo",
                ring: &ring,
                reuse_existing: true,
                preselected: None,
                protected: &[],
            },
        ))
        .unwrap_err();

        assert!(
            err.to_string().contains("no free"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn registration_is_verified_at_the_included_block() {
        let (outcome, scripted) = scripted_registration(&slot_entry([0x22; 32]));

        assert!(matches!(
            outcome.unwrap(),
            RegistrationOutcome::Registered {
                block_hash,
                seq: 0,
                ring_index: 0,
                collection: PersonhoodCollection::LitePeople,
            }
                if block_hash == "0xb10c"
        ));
        let (method, params) = scripted.calls().last().cloned().unwrap();
        assert_eq!(method, "state_getStorage");
        assert!(
            params.ends_with(r#","0xb10c"]"#),
            "verification read not pinned to the included block: {params}"
        );
    }

    #[test]
    fn a_preselected_slot_is_not_rescanned() {
        // No slots are scripted as free: if the caller's selection were ignored
        // and the slots rescanned, the scripted transport would run dry.
        let (outcome, scripted) =
            scripted_registration_with(&slot_entry([0x22; 32]), Some(Preselected::Free(0)), 0);

        assert!(matches!(
            outcome.unwrap(),
            RegistrationOutcome::Registered { seq: 0, .. }
        ));
        // The ring revision and the verification read at the included block,
        // and nothing else.
        assert_eq!(storage_reads(&scripted), 2);
    }

    #[test]
    fn a_scan_without_a_preselection_reads_every_slot() {
        let (outcome, scripted) = scripted_registration(&slot_entry([0x22; 32]));

        assert!(outcome.is_ok());
        // The ring revision, ten slots scanned, and the verification read.
        assert_eq!(storage_reads(&scripted), 12);
    }

    #[test]
    fn registration_fails_when_the_included_block_lacks_the_slot() {
        let (outcome, _scripted) = scripted_registration(&slot_entry([0x99; 32]));

        let err = outcome.unwrap_err();
        assert!(
            err.to_string().contains("0xb10c"),
            "unexpected error: {err}"
        );
    }
}
