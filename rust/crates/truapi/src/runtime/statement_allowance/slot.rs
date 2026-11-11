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

use super::{
	PersonhoodSigner, StatementAllowanceError, collection::PersonhoodCollection,
	extension::Metadata, key_hash::blake2_128_concat, proof::ProofError, rpc::RpcClient, view,
};

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
		error: String,
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

/// A `StatementStore` allowance slot value as the chain stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, scale_decode::DecodeAsType)]
pub struct StatementStoreAllowanceEntry {
	/// Account the slot allows.
	pub account_id: [u8; 32],
	/// Slot index within the period.
	pub seq: u32,
	/// Unix seconds at which the slot was set.
	pub since: u64,
}

/// A slot that is occupied, as observed by a scan.
#[derive(Debug, Clone, PartialEq, Eq)]
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
	u32::try_from(now_seconds / STATEMENT_STORE_PERIOD_SECONDS)
		.expect("allowance periods fit in u32")
}

/// The current long-term-storage period for `now_seconds`.
pub fn current_long_term_storage_period(
	now_seconds: u64,
	period_duration: u32,
) -> Result<u32, StatementAllowanceError> {
	if period_duration == 0 {
		return Err(SlotError::LongTermStoragePeriodDurationZero.into());
	}
	Ok(u32::try_from(now_seconds / u64::from(period_duration)).expect("storage periods fit in u32"))
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

/// The slot alias for the selected collection at `(period, seq)`.
pub fn slot_alias(
	signer: &dyn PersonhoodSigner,
	collection: PersonhoodCollection,
	key_index: usize,
	network_suffix: &[u8],
	period: u32,
	seq: u32,
) -> Result<[u8; 32], StatementAllowanceError> {
	let context = derive_slot_context(network_suffix, period, seq);
	alias_in_context(signer, collection, key_index, &context, "statement-store slot")
}

/// The PGAS claim alias for the selected collection at `(day, slot_index)`.
pub fn pgas_alias(
	signer: &dyn PersonhoodSigner,
	collection: PersonhoodCollection,
	key_index: usize,
	network_suffix: &[u8],
	day: u32,
	slot_index: u32,
) -> Result<[u8; 32], StatementAllowanceError> {
	let context = derive_pgas_context(network_suffix, day, slot_index);
	alias_in_context(signer, collection, key_index, &context, "PGAS claim slot")
}

pub(super) fn long_term_storage_alias(
	signer: &dyn PersonhoodSigner,
	collection: PersonhoodCollection,
	key_index: usize,
	network_suffix: &[u8],
	period: u32,
	counter: u8,
) -> Result<[u8; 32], StatementAllowanceError> {
	let context = derive_long_term_storage_context(network_suffix, period, counter);
	alias_in_context(signer, collection, key_index, &context, "long-term-storage slot")
}

fn alias_in_context(
	signer: &dyn PersonhoodSigner,
	collection: PersonhoodCollection,
	key_index: usize,
	context: &[u8],
	name: &'static str,
) -> Result<[u8; 32], StatementAllowanceError> {
	signer.alias(collection, key_index, context).map_err(|error| match error {
		StatementAllowanceError::Proof(ProofError::Vrf(error)) => {
			SlotError::AliasInContext { context: name, error }.into()
		},
		other => other,
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
	[twox_128(b"Timestamp").as_slice(), twox_128(b"Now").as_slice()].concat()
}

fn network_suffix_key() -> Vec<u8> {
	[twox_128(b"NetworkSuffix").as_slice(), twox_128(b"NetworkSuffix").as_slice()].concat()
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
	alias: &[u8; 32],
	period: u32,
	block_hash: &str,
) -> Result<Option<[u8; 32]>, StatementAllowanceError> {
	let key = statement_store_allowance_key(period, alias);
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

/// Public selection and synchronous alias signing for one slot scan.
pub struct SlotScan<'a> {
	/// Collection whose alias space and slot budget are scanned.
	pub collection: PersonhoodCollection,
	/// Key ordinal within the collection, shared with the selected ring.
	pub key_index: usize,
	/// Authorizes aliases for the selected collection.
	pub signer: &'a dyn PersonhoodSigner,
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

/// Scan slots `0..max` for the scan's period in one round trip, returning the
/// first non-excluded free seq (or detecting that the target already holds one).
pub async fn scan_slot_excluding(
	rpc: &RpcClient,
	metadata: &Metadata,
	scan: SlotScan<'_>,
) -> Result<SlotSelection, StatementAllowanceError> {
	let SlotScan {
		collection,
		key_index,
		signer,
		network_suffix,
		period,
		target,
		excluded,
		reuse_existing,
	} = scan;
	let max = max_slots(rpc, metadata, collection).await?;
	let mut keys = Vec::with_capacity(max as usize);
	for seq in 0..max {
		let alias = slot_alias(signer, collection, key_index, network_suffix, period, seq)?;
		keys.push(statement_store_allowance_key(period, &alias));
	}
	let entries = rpc.get_storage_many(&keys).await?;
	let mut first_free: Option<u32> = None;
	let mut excluded_free = false;
	let mut occupied = Vec::new();
	for (seq, entry) in (0..max).zip(entries) {
		match entry {
			None => {
				if excluded.contains(&seq) {
					excluded_free = true;
				} else if first_free.is_none() {
					first_free = Some(seq);
				}
			},
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
			},
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
#[allow(clippy::too_many_arguments)]
pub async fn scan_pgas_slot_excluding(
	rpc: &RpcClient,
	metadata: &Metadata,
	collection: PersonhoodCollection,
	key_index: usize,
	signer: &dyn PersonhoodSigner,
	network_suffix: &[u8],
	day: u32,
	excluded: &[u32],
) -> Result<u32, StatementAllowanceError> {
	let max = max_pgas_claims(metadata, collection)?;
	scan_pgas_slot_in(rpc, signer, collection, key_index, network_suffix, day, max, excluded).await
}

/// The scan itself, over a known slot count.
///
/// Split from the constant read so it can be exercised without Asset Hub metadata.
#[allow(clippy::too_many_arguments)]
async fn scan_pgas_slot_in(
	rpc: &RpcClient,
	signer: &dyn PersonhoodSigner,
	collection: PersonhoodCollection,
	key_index: usize,
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
		let mut keys = Vec::with_capacity(batch.len());
		for &slot_index in &batch {
			let alias = pgas_alias(signer, collection, key_index, network_suffix, day, slot_index)?;
			keys.push(claimed_gas_alias_key(day, &alias));
		}
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
	alias: &[u8; 32],
	day: u32,
	block_hash: &str,
) -> Result<bool, StatementAllowanceError> {
	let key = claimed_gas_alias_key(day, alias);
	Ok(rpc.get_storage_at(&key, block_hash).await?.is_some())
}

/// Scan long-term-storage aliases `0..max` for `period`, returning the first
/// free counter not listed in `excluded`.
#[allow(clippy::too_many_arguments)]
pub async fn scan_long_term_storage_counter_excluding(
	rpc: &RpcClient,
	metadata: &Metadata,
	signer: &dyn PersonhoodSigner,
	collection: PersonhoodCollection,
	key_index: usize,
	network_suffix: &[u8],
	period: u32,
	excluded: &[u8],
) -> Result<u8, StatementAllowanceError> {
	let max = long_term_storage_claims_per_period(rpc, metadata).await?;
	for counter in 0..max {
		if excluded.contains(&counter) {
			continue;
		}
		let alias = long_term_storage_alias(
			signer,
			collection,
			key_index,
			network_suffix,
			period,
			counter,
		)?;
		let key = spent_long_term_storage_alias_key(period, &alias);
		if rpc.get_storage(&key).await?.is_none() {
			return Ok(counter);
		}
	}
	Err(SlotError::NoFreeLongTermStorageSlot { period, max }.into())
}

#[cfg(test)]
pub mod testing {
	//! Scripted chain answers for slot scans.

	use crate::runtime::statement_allowance::{
		CollectionCandidate, FixedPersonhoodSigner, collection::PersonhoodCollection,
	};

	use super::{slot_alias, statement_store_allowance_key};

	/// The only membership a test signer holds: `entropy` in LitePeople.
	pub fn lite_people(entropy: [u8; 32]) -> [CollectionCandidate; 1] {
		[CollectionCandidate { collection: PersonhoodCollection::LitePeople, entropy }]
	}

	/// A `state_queryStorageAt` answer for `entropy`'s `period` row, where
	/// `entries[seq]` is slot `seq`'s JSON storage value. Like the node, it
	/// lists only the slots that exist.
	pub fn slot_row(
		entropy: [u8; 32],
		network_suffix: &[u8],
		period: u32,
		entries: &[Option<String>],
	) -> String {
		let candidates = lite_people(entropy);
		let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
		let changes: Vec<String> = entries
			.iter()
			.enumerate()
			.filter_map(|(seq, entry)| {
				let alias = slot_alias(
					&signer,
					PersonhoodCollection::LitePeople,
					0,
					network_suffix,
					period,
					u32::try_from(seq).expect("slot sequence fits in u32"),
				)
				.unwrap();
				let key = hex::encode(statement_store_allowance_key(period, &alias));
				Some(format!(r#"["0x{key}",{}]"#, entry.as_ref()?))
			})
			.collect();
		format!(r#"[{{"block":"0xb10c","changes":[{}]}}]"#, changes.join(","))
	}
}

#[cfg(test)]
mod tests;
