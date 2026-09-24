//! StatementStore allowance slot selection.
//!
//! An allowance is claimed at `(period, seq)`. The slot is bound to a 32-byte
//! product context; occupancy is read from
//! `Resources.StatementStoreAllowances[period][alias]`, where the alias is
//! derived from OUR bandersnatch entropy in that slot context. Mirrors
//! signing-bot `allowance.ts` / `allowance-slots.ts`.

use parity_scale_codec::{Decode, DecodeAll, Encode};
use sp_crypto_hashing::{blake2_256, twox_128};
use thiserror::Error;
use verifiable::Error as VerifiableError;
use verifiable::GenerateVerifiable;
use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;

use super::StatementAllowanceError;
use super::collection::PersonhoodCollection;
use super::extension::Metadata;
use super::ring::blake2_128_concat;
use super::rpc::RpcClient;
use super::view;

/// StatementStore allowance period: one UTC day, in seconds.
pub const STATEMENT_STORE_PERIOD_SECONDS: u64 = 86_400;
const PRODUCT_CONTEXT_PREFIX: &[u8] = b"product/peopl.";
const SYSTEM_CONTEXT_PREFIX: &[u8] = b"sys/";
const STATEMENT_STORE_CONTEXT_FAMILY: u32 = 2;
const LONG_TERM_STORAGE_CONTEXT_FAMILY: u32 = 3;
const PGAS_CONTEXT_FAMILY: u32 = 4;
const MAX_NETWORK_SUFFIX_LENGTH: usize = 16;
/// Slots probed per batched storage read while scanning for a free PGAS slot.
///
/// Reading every slot in one request would cost a round trip flat, but each slot's
/// key needs a bandersnatch alias first, and those are milliseconds each — paying
/// for all of them when the first slot is usually free is the worse trade. A batch
/// keeps the common case to one round trip and bounds the full-table case to a
/// handful.
const PGAS_SCAN_BATCH: u32 = 10;

/// Error while deriving aliases or selecting allowance slots.
#[derive(Debug, Error)]
pub enum SlotError {
    /// Long-term storage period duration constant was zero.
    #[error("Resources.LongTermStoragePeriodDuration is zero")]
    LongTermStoragePeriodDurationZero,
    /// Long-term-storage claim count does not fit the `u8` counter.
    #[error("Resources long-term-storage claim count {value} exceeds u8")]
    LongTermStorageClaimsOverflow {
        /// Value returned by the runtime.
        value: u32,
    },
    /// Bandersnatch alias derivation failed.
    #[error("{context} alias_in_context failed: {error:?}")]
    AliasInContext {
        /// Alias context name.
        context: &'static str,
        /// Alias derivation failure.
        error: VerifiableError,
    },
    /// No free statement-store slot was found.
    #[error("no free StatementStore slot in period {period} (max {max})")]
    NoFreeStatementStoreSlot {
        /// Period scanned.
        period: u32,
        /// Maximum slot count.
        max: u32,
    },
    /// No free long-term-storage slot was found.
    #[error("no free long-term-storage slot in period {period} (max {max})")]
    NoFreeLongTermStorageSlot {
        /// Period scanned.
        period: u32,
        /// Maximum slot count.
        max: u8,
    },
    /// The runtime refused a takeover: the slot was not old enough by its own
    /// clock, or another registration reached it first.
    #[error(
        "runtime refused replacing slot (period {period}, seq {seq}): it is not replaceable yet"
    )]
    ReplacementRefused {
        /// Period the takeover targeted.
        period: u32,
        /// Slot the takeover targeted.
        seq: u32,
    },
    /// No unclaimed PGAS slot was found for the day.
    #[error("no free PGAS slot on day {day} (max {max})")]
    NoFreePgasSlot {
        /// Day scanned.
        day: u32,
        /// Maximum claims per day.
        max: u32,
    },
    /// Free slots exist but all were excluded by this call's own submissions.
    #[error(
        "every free StatementStore slot in period {period} is awaiting one of this call's own submissions"
    )]
    FreeSlotsAwaitingSubmission {
        /// Period scanned.
        period: u32,
    },
    /// No collection membership could be proved, so no alias space is available.
    #[error("no provable personhood collection membership")]
    NoCollectionMembership,
    /// `Timestamp.Now` was absent or undecodable, so slot ages cannot be judged.
    #[error("Timestamp.Now missing from chain state")]
    MissingChainTimestamp,
    /// The runtime-wide product context suffix was absent from chain storage.
    #[error("NetworkSuffix.NetworkSuffix missing from chain state")]
    MissingNetworkSuffix,
    /// The runtime-wide product context suffix did not have its declared SCALE shape.
    #[error("NetworkSuffix.NetworkSuffix is not a valid SCALE byte vector: {0}")]
    NetworkSuffixDecode(#[source] parity_scale_codec::Error),
    /// The runtime-wide product context suffix was outside its declared bounds.
    #[error("NetworkSuffix.NetworkSuffix length {len}, expected 1..={MAX_NETWORK_SUFFIX_LENGTH}")]
    InvalidNetworkSuffixLength {
        /// Actual suffix length.
        len: usize,
    },
    /// Registration reached a block but the slot was not held by the target.
    #[error(
        "registration reached block {block_hash} but slot (period {period}, seq {seq}) is not held by the target account"
    )]
    RegistrationVerificationMismatch {
        /// Block hash the registration reached.
        block_hash: String,
        /// Registration period.
        period: u32,
        /// Slot sequence.
        seq: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, scale_decode::DecodeAsType)]
pub(super) struct StatementStoreAllowanceEntry {
    pub(super) account_id: [u8; 32],
    pub(super) seq: u32,
    pub(super) since: u64,
}

/// A slot that is occupied, as observed by a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OccupiedSlot {
    /// Slot index within the period.
    pub seq: u32,
    /// Account the slot currently allows.
    pub account_id: [u8; 32],
    /// Unix seconds at which the slot was last set.
    pub since: u64,
}

/// The current allowance period for `now_seconds`.
pub fn current_period(now_seconds: u64) -> u32 {
    (now_seconds / STATEMENT_STORE_PERIOD_SECONDS) as u32
}

/// The current long-term-storage period for `now_seconds`.
pub fn current_long_term_storage_period(
    now_seconds: u64,
    period_duration: u32,
) -> Result<u32, StatementAllowanceError> {
    if period_duration == 0 {
        return Err(SlotError::LongTermStoragePeriodDurationZero.into());
    }
    Ok((now_seconds / u64::from(period_duration)) as u32)
}

fn derive_product_context(network_suffix: &[u8], family: u32, first: u32, second: u32) -> [u8; 32] {
    let mut suffix = [0u8; 32];
    suffix[..4].copy_from_slice(SYSTEM_CONTEXT_PREFIX);
    suffix[4..8].copy_from_slice(&family.to_le_bytes());
    suffix[8..12].copy_from_slice(&first.to_le_bytes());
    suffix[12..16].copy_from_slice(&second.to_le_bytes());

    let mut preimage = Vec::with_capacity(
        PRODUCT_CONTEXT_PREFIX.len() + network_suffix.len() + b"/".len() + suffix.len(),
    );
    preimage.extend_from_slice(PRODUCT_CONTEXT_PREFIX);
    preimage.extend_from_slice(network_suffix);
    preimage.push(b'/');
    preimage.extend_from_slice(&suffix);
    blake2_256(&preimage)
}

/// Derive the network-scoped 32-byte StatementStore slot context.
pub fn derive_slot_context(network_suffix: &[u8], period: u32, seq: u32) -> [u8; 32] {
    derive_product_context(network_suffix, STATEMENT_STORE_CONTEXT_FAMILY, period, seq)
}

/// Derive the network-scoped 32-byte Asset Hub PGAS claim context.
pub fn derive_pgas_context(network_suffix: &[u8], day: u32, slot_index: u32) -> [u8; 32] {
    derive_product_context(network_suffix, PGAS_CONTEXT_FAMILY, day, slot_index)
}

/// Derive the network-scoped 32-byte Bulletin long-term-storage context.
pub fn derive_long_term_storage_context(
    network_suffix: &[u8],
    period: u32,
    counter: u8,
) -> [u8; 32] {
    derive_product_context(
        network_suffix,
        LONG_TERM_STORAGE_CONTEXT_FAMILY,
        period,
        u32::from(counter),
    )
}

/// The slot alias for our `entropy` at `(period, seq)`.
pub fn slot_alias(
    entropy: [u8; 32],
    network_suffix: &[u8],
    period: u32,
    seq: u32,
) -> Result<[u8; 32], StatementAllowanceError> {
    let secret = BandersnatchVrfVerifiable::new_secret(entropy);
    let context = derive_slot_context(network_suffix, period, seq);
    BandersnatchVrfVerifiable::alias_in_context(&secret, &context).map_err(|err| {
        SlotError::AliasInContext {
            context: "statement-store slot",
            error: err,
        }
        .into()
    })
}

/// The PGAS claim alias for our `entropy` at `(day, slot_index)`.
pub fn pgas_alias(
    entropy: [u8; 32],
    network_suffix: &[u8],
    day: u32,
    slot_index: u32,
) -> Result<[u8; 32], StatementAllowanceError> {
    let secret = BandersnatchVrfVerifiable::new_secret(entropy);
    let context = derive_pgas_context(network_suffix, day, slot_index);
    BandersnatchVrfVerifiable::alias_in_context(&secret, &context).map_err(|err| {
        SlotError::AliasInContext {
            context: "PGAS claim slot",
            error: err,
        }
        .into()
    })
}

/// The long-term-storage slot alias for our `entropy` at `(period, counter)`.
pub fn long_term_storage_alias(
    entropy: [u8; 32],
    network_suffix: &[u8],
    period: u32,
    counter: u8,
) -> Result<[u8; 32], StatementAllowanceError> {
    let secret = BandersnatchVrfVerifiable::new_secret(entropy);
    let context = derive_long_term_storage_context(network_suffix, period, counter);
    BandersnatchVrfVerifiable::alias_in_context(&secret, &context).map_err(|err| {
        SlotError::AliasInContext {
            context: "long-term-storage slot",
            error: err,
        }
        .into()
    })
}

/// `Resources.StatementStoreAllowances[period][alias]` storage key.
/// key1 = Identity(u32be period); key2 = Blake2_128Concat(alias).
pub(super) fn statement_store_allowance_key(period: u32, alias: &[u8; 32]) -> Vec<u8> {
    [
        twox_128(b"Resources").as_slice(),
        twox_128(b"StatementStoreAllowances").as_slice(),
        &period.to_be_bytes(),
        &blake2_128_concat(alias),
    ]
    .concat()
}

/// `Pgas.ClaimedGasAliases[day][alias]` storage key on Asset Hub.
/// key1 = Identity(u32be day); key2 = Blake2_128Concat(alias). Presence alone
/// marks the slot spent; the value is unit.
pub(super) fn claimed_gas_alias_key(day: u32, alias: &[u8; 32]) -> Vec<u8> {
    [
        twox_128(b"Pgas").as_slice(),
        twox_128(b"ClaimedGasAliases").as_slice(),
        &day.to_be_bytes(),
        &blake2_128_concat(alias),
    ]
    .concat()
}

/// `Resources.SpentLongTermStorageAliases[period][alias]` storage key.
/// key1 = Identity(u32be period); key2 = Blake2_128Concat(alias).
pub(super) fn spent_long_term_storage_alias_key(period: u32, alias: &[u8; 32]) -> Vec<u8> {
    [
        twox_128(b"Resources").as_slice(),
        twox_128(b"SpentLongTermStorageAliases").as_slice(),
        &period.to_be_bytes(),
        &blake2_128_concat(alias),
    ]
    .concat()
}

/// Max StatementStore slots per period for `collection`.
async fn max_slots(
    rpc: &RpcClient,
    metadata: &Metadata,
    collection: PersonhoodCollection,
) -> Result<u32, StatementAllowanceError> {
    collection.slots_per_period(rpc, metadata).await
}

/// Max long-term-storage claims per period.
pub async fn long_term_storage_claims_per_period(
    rpc: &RpcClient,
    metadata: &Metadata,
) -> Result<u8, StatementAllowanceError> {
    let value =
        view::read_resource_u32(rpc, metadata, "get_long_term_storage_claims_per_period").await?;
    u8::try_from(value).map_err(|_| SlotError::LongTermStorageClaimsOverflow { value }.into())
}

/// Long-term-storage period duration in seconds from
/// `Resources.LongTermStoragePeriodDuration`.
pub fn long_term_storage_period_duration(
    metadata: &Metadata,
) -> Result<u32, StatementAllowanceError> {
    metadata.constant_u32("Resources", "LongTermStoragePeriodDuration")
}

/// Seconds that statement-store allowances remain active after their period.
pub async fn statement_store_grace_window(
    rpc: &RpcClient,
    metadata: &Metadata,
) -> Result<u32, StatementAllowanceError> {
    view::read_resource_u32(rpc, metadata, "get_stmt_store_grace_window").await
}

/// Decode a slot entry: `account_id(32) ‖ seq(u32 LE) ‖ since(u64 LE)`.
fn decode_entry(bytes: &[u8]) -> Option<StatementStoreAllowanceEntry> {
    StatementStoreAllowanceEntry::decode(&mut &bytes[..]).ok()
}

/// The slot to replace once no free slot is left: the oldest one that is not
/// the target's own and whose replacement cooldown has elapsed.
///
/// `chain_now_seconds` must come from the chain, not the host: `since` is a chain
/// timestamp and the runtime re-checks the cooldown against its own clock when it
/// validates the extrinsic. A host clock runs ahead of the chain by up to a block,
/// which would offer slots the runtime then refuses.
///
/// The comparison is strict because the runtime requires `now > since + cooldown`;
/// a slot at exactly the cooldown is still refused.
///
/// Takes the candidate list rather than scanning, so a caller can widen the
/// pool without changing this rule. Ties on `since` break to the lowest `seq`
/// so the choice is deterministic.
pub fn replaceable_slot(
    candidates: &[OccupiedSlot],
    target: &[u8; 32],
    chain_now_seconds: u64,
    cooldown_seconds: u64,
    excluded: &[u32],
) -> Option<u32> {
    candidates
        .iter()
        .filter(|slot| slot.account_id != *target)
        .filter(|slot| !excluded.contains(&slot.seq))
        .filter(|slot| chain_now_seconds.saturating_sub(slot.since) > cooldown_seconds)
        .min_by_key(|slot| (slot.since, slot.seq))
        .map(|slot| slot.seq)
}

/// `Timestamp.Now` storage key.
pub(super) fn timestamp_now_key() -> Vec<u8> {
    [
        twox_128(b"Timestamp").as_slice(),
        twox_128(b"Now").as_slice(),
    ]
    .concat()
}

fn network_suffix_key() -> Vec<u8> {
    [
        twox_128(b"NetworkSuffix").as_slice(),
        twox_128(b"NetworkSuffix").as_slice(),
    ]
    .concat()
}

/// Read the runtime-wide suffix used for product-scoped proof contexts.
pub async fn read_network_suffix(rpc: &RpcClient) -> Result<Vec<u8>, StatementAllowanceError> {
    let bytes = rpc
        .get_storage(&network_suffix_key())
        .await?
        .ok_or(SlotError::MissingNetworkSuffix)?;
    let suffix = Vec::<u8>::decode_all(&mut &bytes[..]).map_err(SlotError::NetworkSuffixDecode)?;
    if suffix.is_empty() || suffix.len() > MAX_NETWORK_SUFFIX_LENGTH {
        return Err(SlotError::InvalidNetworkSuffixLength { len: suffix.len() }.into());
    }
    Ok(suffix)
}

/// The chain's clock in unix seconds, decoded from `Timestamp.Now` milliseconds.
///
/// Slot ages are judged against this rather than the host clock, which runs up to
/// one block ahead of it.
pub async fn read_chain_now_seconds(rpc: &RpcClient) -> Result<u64, StatementAllowanceError> {
    let bytes = rpc
        .get_storage(&timestamp_now_key())
        .await?
        .ok_or(SlotError::MissingChainTimestamp)?;
    let millis = u64::decode(&mut &bytes[..]).map_err(|_| SlotError::MissingChainTimestamp)?;
    Ok(millis / 1_000)
}

/// Seconds an occupied slot must age before the runtime allows replacing it.
pub async fn replacement_cooldown(
    rpc: &RpcClient,
    metadata: &Metadata,
) -> Result<u64, StatementAllowanceError> {
    view::read_resource_u32(rpc, metadata, "get_stmt_store_replacement_cooldown")
        .await
        .map(u64::from)
}

/// The account holding our alias slot `(period, seq)`, read pinned to
/// `block_hash` (`None` when the slot entry is absent).
pub async fn read_slot_account_at(
    rpc: &RpcClient,
    entropy: [u8; 32],
    network_suffix: &[u8],
    period: u32,
    seq: u32,
    block_hash: &str,
) -> Result<Option<[u8; 32]>, StatementAllowanceError> {
    let alias = slot_alias(entropy, network_suffix, period, seq)?;
    let key = statement_store_allowance_key(period, &alias);
    Ok(rpc
        .get_storage_at(&key, block_hash)
        .await?
        .and_then(|bytes| decode_entry(&bytes).map(|entry| entry.account_id)))
}

/// Outcome of scanning for a slot to register `target` in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotSelection {
    /// A free `seq` we should claim.
    Free(u32),
    /// `target` already holds `seq` this period; no registration needed.
    AlreadyAllocated(u32),
    /// Every slot in the period is taken and none is reusable. Reported rather
    /// than raised so a caller can ask "is an allowance already in place" and
    /// get an answer instead of an error.
    Full {
        /// Slots the period has, for the caller's error.
        max: u32,
        /// Every occupied slot, so a caller can choose one to replace.
        occupied: Vec<OccupiedSlot>,
    },
    /// Slots are free but every one was excluded by this call's own earlier
    /// submissions. Distinct from [`SlotSelection::Full`]: replacing a live slot
    /// here would destroy capacity that is about to free up, because the
    /// excluded slots are only unavailable until those submissions resolve.
    FreeSlotsExcluded,
}

/// Inputs for one statement-store slot scan. The collection fixes both the slot
/// budget and the alias space, so it travels with the entropy that derives them.
pub struct SlotScan<'a> {
    /// Collection whose alias space and slot budget are scanned.
    pub collection: PersonhoodCollection,
    /// Our bandersnatch entropy for `collection`.
    pub entropy: [u8; 32],
    /// Runtime-wide suffix used for product-scoped aliases.
    pub network_suffix: &'a [u8],
    /// Statement-store period to scan.
    pub period: u32,
    /// Account whose existing slot, if any, should be reported.
    pub target: &'a [u8; 32],
    /// Slots to skip, held back by this call's own in-flight submissions.
    pub excluded: &'a [u32],
    /// Whether an existing slot for `target` may be reused.
    pub reuse_existing: bool,
}

/// Scan slots `0..max` for the scan's period, returning the first non-excluded
/// free seq (or detecting that the target already holds one).
pub async fn scan_slot_excluding(
    rpc: &RpcClient,
    metadata: &Metadata,
    scan: SlotScan<'_>,
) -> Result<SlotSelection, StatementAllowanceError> {
    let SlotScan {
        collection,
        entropy,
        network_suffix,
        period,
        target,
        excluded,
        reuse_existing,
    } = scan;
    let max = max_slots(rpc, metadata, collection).await?;
    let mut first_free: Option<u32> = None;
    let mut excluded_free = false;
    let mut occupied = Vec::new();
    for seq in 0..max {
        let alias = slot_alias(entropy, network_suffix, period, seq)?;
        let key = statement_store_allowance_key(period, &alias);
        match rpc.get_storage(&key).await? {
            None => {
                if excluded.contains(&seq) {
                    excluded_free = true;
                } else if first_free.is_none() {
                    first_free = Some(seq);
                }
            }
            Some(bytes) => {
                let Some(entry) = decode_entry(&bytes) else {
                    continue;
                };
                if reuse_existing && entry.account_id == *target {
                    return Ok(SlotSelection::AlreadyAllocated(seq));
                }
                occupied.push(OccupiedSlot {
                    seq,
                    account_id: entry.account_id,
                    since: entry.since,
                });
            }
        }
    }
    Ok(match (first_free, excluded_free) {
        (Some(seq), _) => SlotSelection::Free(seq),
        (None, true) => SlotSelection::FreeSlotsExcluded,
        (None, false) => SlotSelection::Full { max, occupied },
    })
}

/// Claims `collection` may make per PGAS period, from its `Pgas` claim-budget
/// constant.
pub fn max_pgas_claims(
    metadata: &Metadata,
    collection: PersonhoodCollection,
) -> Result<u32, StatementAllowanceError> {
    metadata.constant_u32("Pgas", collection.pgas_claims_per_period_constant())
}

/// Scan PGAS slots `0..max` for `day`, returning the first whose alias has not
/// been claimed and is not listed in `excluded`.
///
/// `ClaimedGasAliases` records spent aliases with a unit value, so presence is
/// the whole answer and nothing is decoded.
pub async fn scan_pgas_slot_excluding(
    rpc: &RpcClient,
    metadata: &Metadata,
    collection: PersonhoodCollection,
    entropy: [u8; 32],
    network_suffix: &[u8],
    day: u32,
    excluded: &[u32],
) -> Result<u32, StatementAllowanceError> {
    let max = max_pgas_claims(metadata, collection)?;
    scan_pgas_slot_in(rpc, entropy, network_suffix, day, max, excluded).await
}

/// The scan itself, over a known slot count.
///
/// Split from the constant read so it can be exercised without Asset Hub metadata.
async fn scan_pgas_slot_in(
    rpc: &RpcClient,
    entropy: [u8; 32],
    network_suffix: &[u8],
    day: u32,
    max: u32,
    excluded: &[u32],
) -> Result<u32, StatementAllowanceError> {
    let mut probed = 0;
    while probed < max {
        let batch: Vec<u32> = (probed..max.min(probed + PGAS_SCAN_BATCH))
            .filter(|slot_index| !excluded.contains(slot_index))
            .collect();
        probed = max.min(probed + PGAS_SCAN_BATCH);
        if batch.is_empty() {
            continue;
        }
        let keys = batch
            .iter()
            .map(|&slot_index| {
                pgas_alias(entropy, network_suffix, day, slot_index)
                    .map(|alias| claimed_gas_alias_key(day, &alias))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let claimed = rpc.get_storage_many(&keys).await?;
        if let Some(slot_index) = batch
            .iter()
            .zip(claimed)
            .find_map(|(&slot_index, value)| value.is_none().then_some(slot_index))
        {
            return Ok(slot_index);
        }
    }
    Err(SlotError::NoFreePgasSlot { day, max }.into())
}

/// Whether `day`'s PGAS slot for `slot_index` is recorded as claimed at
/// `block_hash`.
///
/// A claim reaching a block does not mean it succeeded: `Pgas.claim_pgas` can
/// dispatch-error and the extrinsic still lands. The pallet marks the alias spent
/// on success, so its presence at the included block is what distinguishes the two.
pub async fn pgas_slot_is_claimed_at(
    rpc: &RpcClient,
    entropy: [u8; 32],
    network_suffix: &[u8],
    day: u32,
    slot_index: u32,
    block_hash: &str,
) -> Result<bool, StatementAllowanceError> {
    let alias = pgas_alias(entropy, network_suffix, day, slot_index)?;
    let key = claimed_gas_alias_key(day, &alias);
    Ok(rpc.get_storage_at(&key, block_hash).await?.is_some())
}

/// Scan long-term-storage aliases `0..max` for `period`, returning the first
/// free counter not listed in `excluded`. `entropy` is our bandersnatch entropy.
pub async fn scan_long_term_storage_counter_excluding(
    rpc: &RpcClient,
    metadata: &Metadata,
    entropy: [u8; 32],
    network_suffix: &[u8],
    period: u32,
    excluded: &[u8],
) -> Result<u8, StatementAllowanceError> {
    let max = long_term_storage_claims_per_period(rpc, metadata).await?;
    for counter in 0..max {
        if excluded.contains(&counter) {
            continue;
        }
        let alias = long_term_storage_alias(entropy, network_suffix, period, counter)?;
        let key = spent_long_term_storage_alias_key(period, &alias);
        if rpc.get_storage(&key).await?.is_none() {
            return Ok(counter);
        }
    }
    Err(SlotError::NoFreeLongTermStorageSlot { period, max }.into())
}

#[cfg(test)]
mod tests {
    use subxt_rpcs::RpcClient as HostRpcClient;

    use super::super::rpc::testing::ScriptedRpc;
    use super::super::test_fixtures;
    use super::*;

    /// Fixture metadata captured from paseo-next-v2; its
    /// `LiteStmtStoreSlotsPerPeriod` is 10.
    const SLOTS: usize = 10;
    const NETWORK_SUFFIX: &[u8] = b"paseo";

    /// `StmtStoreAllowanceEntry { account_id, seq: 0, since: 0 }` as a scripted
    /// JSON storage result.
    fn slot_entry(account: [u8; 32]) -> String {
        entry_with_since(account, 0)
    }

    /// A scripted slot entry that was set at `since`.
    fn entry_with_since(account: [u8; 32], since: u64) -> String {
        format!(r#""0x{}""#, hex::encode((account, 0u32, since).encode()))
    }

    /// An occupied-slot candidate for the replacement rule.
    fn occupied(seq: u32, account_id: [u8; 32], since: u64) -> OccupiedSlot {
        OccupiedSlot {
            seq,
            account_id,
            since,
        }
    }

    /// Run `scan_slot_excluding` for `[0x22; 32]` against a scripted period
    /// whose slot occupancy is `slots`.
    fn scripted_find(slots: &[Option<[u8; 32]>]) -> SlotSelection {
        let metadata = test_fixtures::people();
        let entries: Vec<String> = slots
            .iter()
            .map(|slot| slot.map_or_else(|| "null".to_string(), slot_entry))
            .collect();
        let scripted = ScriptedRpc::new(entries.iter().map(String::as_str));
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        futures::executor::block_on(scan_slot_excluding(
            &rpc,
            metadata,
            SlotScan {
                collection: PersonhoodCollection::LitePeople,
                entropy: [0x11; 32],
                network_suffix: NETWORK_SUFFIX,
                period: 7,
                target: &[0x22; 32],
                excluded: &[],
                reuse_existing: true,
            },
        ))
        .unwrap()
    }

    /// The scan bound is whatever Asset Hub declares, not a compiled-in constant,
    /// and it bounds how many keys a full scan hashes and reads.
    ///
    /// Asset Hub budgets each collection separately, so both are pinned: one
    /// number alone would still pass if the two constants were swapped.
    #[test]
    fn the_asset_hub_fixture_declares_a_daily_pgas_budget_per_collection() {
        let metadata = test_fixtures::asset_hub();

        assert_eq!(
            max_pgas_claims(metadata, PersonhoodCollection::People).unwrap(),
            100,
        );
        assert_eq!(
            max_pgas_claims(metadata, PersonhoodCollection::LitePeople).unwrap(),
            40,
        );
    }

    #[test]
    fn an_empty_period_offers_the_first_slot() {
        assert_eq!(scripted_find(&[None; SLOTS]), SlotSelection::Free(0));
    }

    #[test]
    fn the_slot_the_target_holds_is_found() {
        let mut slots = [None; SLOTS];
        slots[2] = Some([0x22; 32]);

        assert_eq!(scripted_find(&slots), SlotSelection::AlreadyAllocated(2));
    }

    #[test]
    fn a_table_filled_by_other_accounts_reports_full_rather_than_erroring() {
        let SlotSelection::Full { max, occupied } = scripted_find(&[Some([0x99; 32]); SLOTS])
        else {
            panic!("a full table should report Full");
        };

        assert_eq!(max, SLOTS as u32);
        // Every slot is reported, with the age the replacement rule needs.
        assert_eq!(occupied.len(), SLOTS);
        assert_eq!(occupied[0].seq, 0);
        assert_eq!(occupied[0].account_id, [0x99; 32]);
    }

    /// `since` values are what the replacement rule sorts on, so the scan has to
    /// carry them through rather than discard them.
    #[test]
    fn the_scan_reports_each_occupied_slots_age() {
        let metadata = test_fixtures::people();
        let entries: Vec<String> = (0..SLOTS)
            .map(|seq| entry_with_since([0x99; 32], 1_000 + seq as u64))
            .collect();
        let scripted = ScriptedRpc::new(entries.iter().map(String::as_str).collect::<Vec<_>>());
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        let SlotSelection::Full { occupied, .. } =
            futures::executor::block_on(scan_slot_excluding(
                &rpc,
                metadata,
                SlotScan {
                    collection: PersonhoodCollection::LitePeople,
                    entropy: [0x11; 32],
                    network_suffix: NETWORK_SUFFIX,
                    period: 7,
                    target: &[0x22; 32],
                    excluded: &[],
                    reuse_existing: true,
                },
            ))
            .unwrap()
        else {
            panic!("a full table should report Full");
        };

        assert_eq!(
            occupied.iter().map(|slot| slot.since).collect::<Vec<_>>(),
            (0..SLOTS).map(|seq| 1_000 + seq as u64).collect::<Vec<_>>(),
        );
    }

    /// The oldest slot wins, and the target's own slot is never a candidate.
    #[test]
    fn the_oldest_replaceable_slot_is_chosen() {
        let candidates = [
            occupied(0, [0x99; 32], 5_000),
            occupied(1, [0x22; 32], 1_000),
            occupied(2, [0x98; 32], 2_000),
        ];

        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]),
            Some(2),
        );
    }

    #[test]
    fn a_slot_inside_its_cooldown_is_not_replaceable() {
        let candidates = [occupied(0, [0x99; 32], 9_990)];

        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]),
            None,
        );
    }

    /// The runtime requires `now > since + cooldown`, so a slot at exactly the
    /// cooldown is still refused. Offering it means an extrinsic the chain
    /// rejects as invalid, which is not retried.
    #[test]
    fn the_cooldown_boundary_is_strict() {
        let candidates = [occupied(0, [0x99; 32], 1_000)];

        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 1_060, 60, &[]),
            None,
            "age exactly at the cooldown must not be offered",
        );
        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 1_061, 60, &[]),
            Some(0),
            "one second past the cooldown is replaceable",
        );
    }

    /// `Timestamp.Now` is milliseconds; ages are judged in seconds.
    #[test]
    fn the_chain_clock_is_read_in_seconds() {
        let scripted = ScriptedRpc::new(vec![r#""0x60ea000000000000""#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        assert_eq!(
            futures::executor::block_on(read_chain_now_seconds(&rpc)).unwrap(),
            60,
        );
    }

    #[test]
    fn a_table_of_only_the_targets_own_and_cooling_slots_yields_nothing() {
        let candidates = [
            occupied(0, [0x22; 32], 1_000),
            occupied(1, [0x99; 32], 9_999),
        ];

        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]),
            None,
        );
    }

    #[test]
    fn an_excluded_slot_is_skipped_so_a_failed_replacement_moves_on() {
        let candidates = [
            occupied(0, [0x99; 32], 1_000),
            occupied(1, [0x98; 32], 2_000),
        ];

        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[0]),
            Some(1),
        );
    }

    /// Equal ages resolve to the lowest seq, so retries do not oscillate.
    #[test]
    fn equal_ages_break_to_the_lowest_seq() {
        let candidates = [
            occupied(5, [0x99; 32], 1_000),
            occupied(2, [0x98; 32], 1_000),
        ];

        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]),
            Some(2),
        );
    }

    /// A pass that registers several targets protects the slots it has already
    /// claimed, so target N cannot take the slot target N-1 just got. Without
    /// this a pass with more targets than slots undoes its own work forever.
    #[test]
    fn slots_already_claimed_in_this_pass_are_protected() {
        let candidates = [
            occupied(0, [0x01; 32], 1_000),
            occupied(1, [0x02; 32], 2_000),
            occupied(2, [0x03; 32], 3_000),
        ];

        // Nothing claimed yet: the oldest goes.
        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]),
            Some(0),
        );
        // Having claimed 0 and 1 earlier in the pass, only 2 is available.
        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[0, 1]),
            Some(2),
        );
        // Once every slot is one this pass claimed, the honest answer is none.
        assert_eq!(
            replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[0, 1, 2]),
            None,
        );
    }

    /// Excluding a slot because a submission for it is in flight must not read as
    /// "the period is full": the free slot is coming back, and a caller that
    /// treats this as full would replace a live slot for nothing.
    #[test]
    fn an_excluded_free_slot_is_not_reported_as_a_full_period() {
        let metadata = test_fixtures::people();
        // Only seq 9 is free, and the caller already excluded it.
        let mut entries: Vec<String> = (0..SLOTS - 1).map(|_| slot_entry([0x99; 32])).collect();
        entries.push("null".to_string());
        let scripted = ScriptedRpc::new(entries.iter().map(String::as_str).collect::<Vec<_>>());
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        let selection = futures::executor::block_on(scan_slot_excluding(
            &rpc,
            metadata,
            SlotScan {
                collection: PersonhoodCollection::LitePeople,
                entropy: [0x11; 32],
                network_suffix: NETWORK_SUFFIX,
                period: 7,
                target: &[0x22; 32],
                excluded: &[(SLOTS - 1) as u32],
                reuse_existing: true,
            },
        ))
        .unwrap();

        assert_eq!(selection, SlotSelection::FreeSlotsExcluded);
    }

    /// The scan probes a batch per round trip, not a key per round trip, and still
    /// returns the first free slot in order. One round trip per slot cost seconds
    /// against a live chain when the early slots were taken.
    #[test]
    fn the_pgas_scan_reads_a_batch_per_round_trip() {
        const ENTROPY: [u8; 32] = [0x11; 32];
        const DAY: u32 = 20678;

        // Slots 0-2 are claimed; 3 is free. `state_queryStorageAt` reports only the
        // keys that exist, so the absent ones are simply missing from `changes`.
        let claimed: Vec<String> = (0..3u32)
            .map(|slot_index| {
                let alias = pgas_alias(ENTROPY, NETWORK_SUFFIX, DAY, slot_index).unwrap();
                format!(
                    r#"["0x{}","0x"]"#,
                    hex::encode(claimed_gas_alias_key(DAY, &alias))
                )
            })
            .collect();
        let response = format!(
            r#"[{{"block":"0xb10c","changes":[{}]}}]"#,
            claimed.join(",")
        );
        let scripted = ScriptedRpc::new(vec![response.as_str()]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let chosen = futures::executor::block_on(scan_pgas_slot_in(
            &rpc,
            ENTROPY,
            NETWORK_SUFFIX,
            DAY,
            40,
            &[],
        ))
        .unwrap();

        assert_eq!(chosen, 3, "the first free slot, in order");
        let calls = scripted.calls();
        assert_eq!(calls.len(), 1, "one round trip covered the whole batch");
        assert_eq!(calls[0].0, "state_queryStorageAt");
    }

    /// Inclusion is not success: the pallet marks the alias spent only when the
    /// claim dispatches cleanly, so its absence at the included block is how a
    /// dispatch error is caught.
    #[test]
    fn a_claim_is_only_recorded_when_the_alias_is_spent() {
        const ENTROPY: [u8; 32] = [0x11; 32];
        const DAY: u32 = 20678;

        let spent = ScriptedRpc::new(vec![r#""0x""#]);
        let absent = ScriptedRpc::new(vec!["null"]);

        assert!(
            futures::executor::block_on(pgas_slot_is_claimed_at(
                &RpcClient::new(HostRpcClient::new(spent)),
                ENTROPY,
                NETWORK_SUFFIX,
                DAY,
                0,
                "0xb10c",
            ))
            .unwrap()
        );
        assert!(
            !futures::executor::block_on(pgas_slot_is_claimed_at(
                &RpcClient::new(HostRpcClient::new(absent)),
                ENTROPY,
                NETWORK_SUFFIX,
                DAY,
                0,
                "0xb10c",
            ))
            .unwrap()
        );
    }

    #[test]
    fn pgas_context_matches_mobile_clients_and_runtime() {
        let expected: [u8; 32] =
            hex::decode("e47ba2c7eae3b97beabaeef8df599afd53e44ba9c2b851cd80850d3ed95a685b")
                .unwrap()
                .try_into()
                .unwrap();

        assert_eq!(derive_pgas_context(NETWORK_SUFFIX, 100, 3), expected);
    }

    /// `ClaimedGasAliases` is `Identity(u32be day) ‖ Blake2_128Concat(alias)`.
    #[test]
    fn claimed_gas_alias_key_layout() {
        let alias = [0x42; 32];
        let key = claimed_gas_alias_key(0x0102_0304, &alias);

        assert_eq!(key.len(), 16 + 16 + 4 + 16 + 32);
        assert_eq!(&key[32..36], &[0x01, 0x02, 0x03, 0x04], "day is big-endian");
        assert_eq!(&key[52..], &alias, "alias follows its blake2_128 prefix");
    }

    /// The nonzero vector pins field order and endianness. The second separately
    /// pins the period-zero, sequence-zero reference vector.
    #[test]
    fn statement_slot_context_matches_mobile_clients_and_runtime() {
        let vector = |hex: &str| -> [u8; 32] { hex::decode(hex).unwrap().try_into().unwrap() };

        assert_eq!(
            (
                derive_slot_context(NETWORK_SUFFIX, 100, 3),
                derive_slot_context(NETWORK_SUFFIX, 0, 0),
            ),
            (
                vector("b6c21225dcf4c2aeeca32b6db1fc93b6942ca0e8ff5c3cb1b2c5d8f0b4647ee3"),
                vector("deee1c90cf0d31093d318ac6629b4c4ab08650d4a4164511cc2496205f20f067"),
            ),
        );
    }

    #[test]
    fn product_contexts_are_scoped_to_the_network() {
        assert_eq!(
            [
                derive_slot_context(b"paseo", 100, 3) != derive_slot_context(b"polkadot", 100, 3),
                derive_long_term_storage_context(b"paseo", 100, 3)
                    != derive_long_term_storage_context(b"polkadot", 100, 3),
                derive_pgas_context(b"paseo", 100, 3) != derive_pgas_context(b"polkadot", 100, 3),
            ],
            [true; 3],
        );
    }

    #[test]
    fn network_suffix_is_read_from_chain_storage() {
        let scripted = ScriptedRpc::new(vec![r#""0x14706173656f""#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));
        let suffix = futures::executor::block_on(read_network_suffix(&rpc)).unwrap();

        assert_eq!(
            (suffix, scripted.calls()),
            (
                b"paseo".to_vec(),
                vec![(
                    "state_getStorage".to_string(),
                    r#"["0x63c2d4c355d2d188c234d0a0669dc90863c2d4c355d2d188c234d0a0669dc908"]"#
                        .to_string(),
                )],
            ),
        );
    }

    #[test]
    fn missing_network_suffix_is_rejected() {
        let scripted = ScriptedRpc::new(vec!["null"]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted));

        assert!(matches!(
            futures::executor::block_on(read_network_suffix(&rpc)),
            Err(StatementAllowanceError::Slot(
                SlotError::MissingNetworkSuffix
            )),
        ));
    }

    #[test]
    fn malformed_network_suffix_is_rejected() {
        let malformed = ScriptedRpc::new(vec![r#""0x14""#]);
        let malformed_rpc = RpcClient::new(HostRpcClient::new(malformed));

        assert!(matches!(
            futures::executor::block_on(read_network_suffix(&malformed_rpc)),
            Err(StatementAllowanceError::Slot(
                SlotError::NetworkSuffixDecode(_)
            )),
        ));
    }

    #[test]
    fn empty_network_suffix_is_rejected() {
        let empty = ScriptedRpc::new(vec![r#""0x00""#]);
        let empty_rpc = RpcClient::new(HostRpcClient::new(empty));

        assert!(matches!(
            futures::executor::block_on(read_network_suffix(&empty_rpc)),
            Err(StatementAllowanceError::Slot(
                SlotError::InvalidNetworkSuffixLength { len: 0 }
            )),
        ));
    }

    #[test]
    fn oversized_network_suffix_is_rejected() {
        let oversized_response = format!(r#""0x{}""#, hex::encode(vec![0x44; 18]));
        let oversized = ScriptedRpc::new([oversized_response.as_str()]);
        let oversized_rpc = RpcClient::new(HostRpcClient::new(oversized));

        assert!(matches!(
            futures::executor::block_on(read_network_suffix(&oversized_rpc)),
            Err(StatementAllowanceError::Slot(
                SlotError::InvalidNetworkSuffixLength { len: 17 }
            )),
        ));
    }

    #[test]
    fn long_term_storage_context_matches_mobile_clients_and_runtime() {
        let expected: [u8; 32] =
            hex::decode("1b3fbe4dd813ea1e349878c9228c6823db8345207690ca4df656acb7fee81bd1")
                .unwrap()
                .try_into()
                .unwrap();

        assert_eq!(
            derive_long_term_storage_context(NETWORK_SUFFIX, 100, 3),
            expected,
        );
    }

    #[test]
    fn long_term_storage_scan_uses_the_requested_network_suffix() {
        const ENTROPY: [u8; 32] = [0x11; 32];
        const PERIOD: u32 = 7;
        const SUFFIX: &[u8] = b"previewnet";

        let metadata = test_fixtures::people();
        let scripted = ScriptedRpc::new([r#""0x""#, "null"]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let counter = futures::executor::block_on(scan_long_term_storage_counter_excluding(
            &rpc,
            metadata,
            ENTROPY,
            SUFFIX,
            PERIOD,
            &[],
        ))
        .unwrap();
        let calls = (0..=1)
            .map(|counter| {
                let alias = long_term_storage_alias(ENTROPY, SUFFIX, PERIOD, counter).unwrap();
                (
                    "state_getStorage".to_string(),
                    format!(
                        r#"["0x{}"]"#,
                        hex::encode(spent_long_term_storage_alias_key(PERIOD, &alias))
                    ),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!((counter, scripted.calls()), (1, calls));
    }

    #[test]
    fn period_is_utc_day_index() {
        assert_eq!(current_period(86_400 * 20_000 + 5), 20_000);
    }

    #[test]
    fn long_term_storage_period_uses_chain_duration() {
        assert_eq!(
            current_long_term_storage_period(1_209_600 * 20 + 5, 1_209_600).unwrap(),
            20,
        );
    }

    #[test]
    fn allowance_entry_matches_runtime_field_codec() {
        let entry = StatementStoreAllowanceEntry {
            account_id: [0x42; 32],
            seq: 7,
            since: 99,
        };
        let encoded = entry.encode();

        assert_eq!(encoded, (entry.account_id, entry.seq, entry.since).encode());
        assert_eq!(
            decode_entry(&encoded).map(|e| e.account_id),
            Some(entry.account_id)
        );
        assert_eq!(
            StatementStoreAllowanceEntry::decode(&mut encoded.as_slice()).unwrap(),
            entry
        );
    }

    #[test]
    fn truncated_allowance_entry_has_no_account() {
        assert!(decode_entry(&[0x42; 32]).is_none());
    }

    /// Pins `reports_exhausted_period` against the renderings above: rewording
    /// one of these variants fails here, in the file it was reworded in, rather
    /// than silently turning the signing host's account rotation into a retry
    /// loop. The reason it reads is the registration error wrapped in context,
    /// so the match has to survive both the wrapping and the casing.
    #[test]
    fn an_exhausted_period_is_reported_whatever_wraps_it() {
        use crate::runtime::login_failure::reports_exhausted_period;

        let error = SlotError::NoFreeStatementStoreSlot { period: 7, max: 10 };

        assert!(reports_exhausted_period(&error.to_string()));
        assert!(reports_exhausted_period(&format!(
            "allowance registration for device failed: {error}"
        )));
        assert!(reports_exhausted_period(
            &SlotError::NoFreeLongTermStorageSlot { period: 7, max: 4 }.to_string()
        ));
        assert!(!reports_exhausted_period(
            &SlotError::FreeSlotsAwaitingSubmission { period: 7 }.to_string()
        ));
    }
}
