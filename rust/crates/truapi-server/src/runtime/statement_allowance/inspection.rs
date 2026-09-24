//! Read-only, finalized wallet allowance observations. No allocation or proof submission.

use std::collections::HashMap;

use parity_scale_codec::{DecodeAll, Encode};
use scale_decode::DecodeAsType;
use serde::Serialize;
use serde_json::{Value, json};
use sp_crypto_hashing::twox_128;

use super::{
    CollectionCandidate, collection::PersonhoodCollection, extension::Metadata, pgas, proof, ring,
    rpc::RpcClient, slot, view,
};

const STORAGE_BATCH: usize = 32;
// Bound work even if a runtime advertises an unexpectedly large policy.
const MAX_INSPECTION_SLOTS: u32 = 4096;
const JS_MAX_INTEGER: u64 = 9_007_199_254_740_991;

/// Finalized chain state used for one observation, not an atomic cross-chain time.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowanceObservation {
    /// Lowercase hex genesis hash identifying the configured chain.
    pub genesis_hash: String,
    /// Lowercase hex hash pinning metadata, storage, and runtime calls.
    pub block_hash: String,
    /// Height of the pinned finalized block.
    pub block_number: u32,
    /// Runtime version used to decode the pinned state.
    pub spec_version: u32,
    /// Pinned chain time in Unix seconds, never the browser clock.
    pub chain_timestamp: u64,
}

/// Independent resource result; unavailable data never implies zero capacity.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum AllowanceSection<T> {
    /// A complete, decoded observation from the stated chain block.
    Available {
        /// Provenance for this section's chain-local values.
        observation: AllowanceObservation,
        /// Resource-specific quantities and policy.
        value: T,
    },
    /// No capacity can be established for this section.
    Unavailable {
        /// Diagnostic explaining the failed observation.
        reason: String,
    },
}

impl<T> AllowanceSection<T> {
    pub(crate) fn unavailable(reason: impl ToString) -> Self {
        Self::Unavailable {
            reason: reason.to_string(),
        }
    }
}

/// One occupied period slot; recipient attribution may not exist on chain.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowanceSlot {
    /// Index in this collection's allowance-slot namespace.
    pub index: u32,
    /// Public recipient when the storage record exposes one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// Product identifier only when native derivation verifies the mapping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    /// Trusted local device or allocation description, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// On-chain assignment time in Unix seconds, where available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
}

/// Collection-specific occupancy and usable capacity under native allocation policy.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowancePool {
    /// Personhood collection: `People` or `LitePeople`.
    pub collection: String,
    /// Finalized membership result: `verified` or `not-found`.
    pub membership: &'static str,
    /// Whether the current native allocator selects this collection.
    pub selected: bool,
    /// Runtime policy limit, retained even for a nonmember.
    pub limit: u32,
    /// Observed occupied slots, including allocations surviving lost membership.
    pub used: u32,
    /// Saturated unused capacity; zero when membership is absent.
    pub remaining: u32,
    /// Occupied entries only; missing recipient fields do not mean unused slots.
    pub slots: Vec<AllowanceSlot>,
}

/// Period boundaries and distinct collection budgets; pools are not implicitly pooled.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowanceClaims {
    /// Runtime allowance period containing the pinned chain time.
    pub period: u32,
    /// Next period boundary in Unix seconds.
    pub resets_at: u64,
    /// Separately observed collection budgets and allocator selection.
    pub pools: Vec<AllowancePool>,
}

/// Statement publishing assignments, not message counts or stored-byte usage.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementAllowanceSnapshot {
    /// Collection budgets and occupied publishing slots for this period.
    #[serde(flatten)]
    pub claims: AllowanceClaims,
    /// Runtime grace window in seconds.
    pub grace_seconds: u32,
    /// Runtime delay before an existing assignment may be replaced.
    pub replacement_cooldown_seconds: u32,
}

/// Asset Hub claim opportunities, separate from any recipient's PGAS balance.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PgasClaimsSnapshot {
    /// Asset Hub period and claim-slot occupancy.
    #[serde(flatten)]
    pub claims: AllowanceClaims,
    /// Runtime-configured PGAS asset identifier as a decimal integer.
    pub asset_id: String,
    /// Amount per claim in exact decimal base units.
    pub claim_amount: String,
    /// Separate finalized People block used to verify collection membership.
    pub membership_observation: AllowanceObservation,
}

/// Total asset balance for one explicitly scoped product account.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PgasBalance {
    /// Canonical trusted-shell product hint.
    pub product_id: String,
    /// Native-derived public product account in lowercase hex.
    pub account_id: String,
    /// Product derivation index; the first inspector supports only zero.
    pub derivation_index: u32,
    /// Exact decimal base units, or unknown on a failed read; not spendability.
    pub balance: Option<String>,
    /// Per-account failure explaining an unknown balance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// PGAS balances and metadata read at the same finalized Asset Hub block.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PgasBalancesSnapshot {
    /// Runtime-configured asset identifier as a decimal integer.
    pub asset_id: String,
    /// Asset metadata precision, absent when metadata cannot be established.
    pub decimals: Option<u8>,
    /// Asset metadata symbol, absent rather than guessed.
    pub symbol: Option<String>,
    /// One result per requested product; other derivations are outside scope.
    pub accounts: Vec<PgasBalance>,
}

/// Bulletin authorization for a dedicated product storage key.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulletinQuota {
    /// Canonical trusted-shell product hint.
    pub product_id: String,
    /// Dedicated Bulletin allowance account, not the PGAS product account.
    pub account_id: String,
    /// `active`, `expired`, `missing`, or `unavailable` at the source block.
    pub status: &'static str,
    /// Raw charged-byte counter in exact decimal units; may exceed the quota.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_used: Option<String>,
    /// Granted byte allowance in exact decimal units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_limit: Option<String>,
    /// Saturated byte remainder; not usable after authorization expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_remaining: Option<String>,
    /// Raw submission counter, which can exceed a soft quota.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions_used: Option<u32>,
    /// Granted submission allowance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions_limit: Option<u32>,
    /// Saturated submission remainder; not usable after expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions_remaining: Option<u32>,
    /// First Bulletin block at which the authorization is expired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at_block: Option<u32>,
    /// Per-account read or decode failure, distinct from missing authorization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Independently decoded Bulletin authorizations for the requested products.
#[derive(Debug, Serialize)]
pub struct BulletinQuotasSnapshot {
    /// Complete requested scope, including missing or unavailable accounts.
    pub accounts: Vec<BulletinQuota>,
}

/// Public, read-only wallet inspection bound to one native identity activation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletAllowanceSnapshot {
    /// DTO schema version, currently one.
    pub schema_version: u32,
    /// Native-derived identity account in lowercase hex; never key material.
    pub identity_account_id: String,
    /// Native network derivation suffix, not a display label.
    pub network_suffix: String,
    /// Validated canonical product hints defining balance and quota scope.
    pub product_ids: Vec<String>,
    /// People-chain publishing-slot occupancy and policy.
    pub statement_store: AllowanceSection<StatementAllowanceSnapshot>,
    /// Asset Hub claims with separately pinned People membership provenance.
    pub pgas_claims: AllowanceSection<PgasClaimsSnapshot>,
    /// Current product Index(0) balances, independent of claim availability.
    pub pgas_balances: AllowanceSection<PgasBalancesSnapshot>,
    /// People-chain storage-claim opportunities under native collection policy.
    pub bulletin_claims: AllowanceSection<AllowanceClaims>,
    /// Bulletin storage authorizations, byte/submission counters, and expiry.
    pub bulletin_quotas: AllowanceSection<BulletinQuotasSnapshot>,
}

pub(crate) type ActivationGuard<'a> = &'a (dyn Fn() -> Result<(), String> + Sync);
pub(crate) type AccountLabels = HashMap<[u8; 32], (Option<String>, String)>;

/// Only public account identifiers, derived before any network request.
pub(crate) struct ProductAccounts {
    pub product_id: String,
    pub pgas: [u8; 32],
    pub bulletin: [u8; 32],
}

pub(crate) struct ReadOnlyChain<'a> {
    rpc: RpcClient,
    metadata: Metadata,
    pub observation: AllowanceObservation,
    guard: ActivationGuard<'a>,
}

fn reason(error: impl ToString) -> String {
    error.to_string()
}

async fn checked_call(
    rpc: &RpcClient,
    guard: ActivationGuard<'_>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    guard()?;
    let result = rpc.call(method, params).await;
    guard()?;
    result.map_err(reason)
}

fn hex_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let text = value
        .as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .ok_or_else(|| "RPC result is not 0x-prefixed hex".to_string())?;
    hex::decode(text).map_err(reason)
}

fn hash(value: &Value) -> Result<String, String> {
    let bytes = hex_bytes(value)?;
    if bytes.len() != 32 {
        return Err("RPC block hash is not 32 bytes".to_string());
    }
    Ok(format!("0x{}", hex::encode(bytes)))
}

fn decode_all<T: DecodeAll>(bytes: &[u8], context: &str) -> Result<T, String> {
    T::decode_all(&mut &bytes[..]).map_err(|error| format!("{context}: {error}"))
}

fn decode_metadata(bytes: &[u8]) -> Result<Metadata, String> {
    // Metadata::decode is also used by allocators. Keep its compatibility
    // behaviour there, but inspection must reject trailing/incomplete data.
    let _: frame_metadata::RuntimeMetadataPrefixed = decode_all(bytes, "metadata")?;
    Metadata::decode(bytes).map_err(reason)
}

impl<'a> ReadOnlyChain<'a> {
    pub(crate) async fn open_client(
        client: super::ChainClient,
        guard: ActivationGuard<'a>,
    ) -> Result<Self, String> {
        Self::open(client.rpc, client.configured_genesis_hash, guard).await
    }

    pub(crate) async fn open(
        rpc: RpcClient,
        expected_genesis: [u8; 32],
        guard: ActivationGuard<'a>,
    ) -> Result<Self, String> {
        let genesis = hash(&checked_call(&rpc, guard, "chain_getBlockHash", json!([0])).await?)?;
        if genesis != format!("0x{}", hex::encode(expected_genesis)) {
            return Err("RPC genesis differs from the configured chain".to_string());
        }
        let at = hash(&checked_call(&rpc, guard, "chain_getFinalizedHead", json!([])).await?)?;
        let header = checked_call(&rpc, guard, "chain_getHeader", json!([at])).await?;
        let number = header
            .get("number")
            .and_then(Value::as_str)
            .and_then(|s| s.strip_prefix("0x"))
            .ok_or_else(|| "finalized header has no hex block number".to_string())?;
        let block_number = u32::from_str_radix(number, 16).map_err(reason)?;
        let runtime = checked_call(&rpc, guard, "state_getRuntimeVersion", json!([at])).await?;
        let spec_version = runtime
            .get("specVersion")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| "runtime has no valid specVersion".to_string())?;
        let versioned = checked_call(
            &rpc,
            guard,
            "state_call",
            json!([
                "Metadata_metadata_at_version",
                format!("0x{}", hex::encode(16u32.encode())),
                at
            ]),
        )
        .await;
        let metadata = match versioned {
            Ok(value) => {
                let bytes = hex_bytes(&value)?;
                match decode_all::<Option<Vec<u8>>>(&bytes, "Metadata_metadata_at_version")? {
                    Some(bytes) => decode_metadata(&bytes)?,
                    None => Self::legacy_metadata(&rpc, guard, &at).await?,
                }
            }
            // Older runtimes do not implement the metadata runtime API. The
            // fallback still names this exact block; malformed success is never
            // accepted or reinterpreted as absence.
            Err(_) => Self::legacy_metadata(&rpc, guard, &at).await?,
        };
        let mut chain = Self {
            rpc,
            metadata,
            guard,
            observation: AllowanceObservation {
                genesis_hash: genesis,
                block_hash: at,
                block_number,
                spec_version,
                chain_timestamp: 0,
            },
        };
        let bytes = chain
            .storage(slot::timestamp_now_key())
            .await?
            .ok_or_else(|| "Timestamp.Now is absent at the finalized block".to_string())?;
        chain.observation.chain_timestamp =
            chain.decode_storage::<u64>("Timestamp", "Now", &bytes)? / 1000;
        if chain.observation.chain_timestamp > JS_MAX_INTEGER {
            return Err("chain timestamp exceeds the snapshot integer range".to_string());
        }
        Ok(chain)
    }

    async fn legacy_metadata(
        rpc: &RpcClient,
        guard: ActivationGuard<'_>,
        at: &str,
    ) -> Result<Metadata, String> {
        let bytes = hex_bytes(&checked_call(rpc, guard, "state_getMetadata", json!([at])).await?)?;
        if bytes.starts_with(b"meta") {
            decode_metadata(&bytes)
        } else {
            decode_metadata(&decode_all::<Vec<u8>>(&bytes, "opaque metadata")?)
        }
    }

    pub(crate) fn section<T>(&self, result: Result<T, String>) -> AllowanceSection<T> {
        match result {
            Ok(value) => AllowanceSection::Available {
                observation: self.observation.clone(),
                value,
            },
            Err(reason) => AllowanceSection::Unavailable { reason },
        }
    }

    async fn storage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
        Ok(self.storage_many(&[key]).await?.remove(0))
    }

    async fn storage_many(&self, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>, String> {
        let mut values = Vec::with_capacity(keys.len());
        for chunk in keys.chunks(STORAGE_BATCH) {
            (self.guard)()?;
            let result = self
                .rpc
                .get_storage_many_at(chunk, &self.observation.block_hash)
                .await;
            (self.guard)()?;
            values.extend(result.map_err(reason)?);
        }
        Ok(values)
    }

    fn storage_type(&self, pallet: &str, entry: &str) -> Result<u32, String> {
        self.metadata
            .storage_value_type(pallet, entry)
            .ok_or_else(|| format!("runtime does not expose {pallet}.{entry}"))
    }

    fn decode_storage<T: DecodeAsType>(
        &self,
        pallet: &str,
        entry: &str,
        bytes: &[u8],
    ) -> Result<T, String> {
        let mut input = bytes;
        let value = T::decode_as_type(
            &mut input,
            self.storage_type(pallet, entry)?,
            self.metadata.registry(),
        )
        .map_err(|error| format!("{pallet}.{entry}: {error}"))?;
        if !input.is_empty() {
            return Err(format!("{pallet}.{entry}: trailing storage bytes"));
        }
        Ok(value)
    }

    fn constant<T: DecodeAsType>(&self, pallet: &str, name: &str) -> Result<T, String> {
        let bytes = self
            .metadata
            .constant(pallet, name)
            .ok_or_else(|| format!("runtime does not expose {pallet}.{name}"))?;
        let type_id = self
            .metadata
            .constant_type(pallet, name)
            .ok_or_else(|| format!("runtime does not expose the type of {pallet}.{name}"))?;
        let mut input = bytes;
        let value = T::decode_as_type(&mut input, type_id, self.metadata.registry())
            .map_err(|error| format!("{pallet}.{name}: {error}"))?;
        if !input.is_empty() {
            return Err(format!("{pallet}.{name}: trailing constant bytes"));
        }
        Ok(value)
    }

    async fn policy(&self, name: &'static str) -> Result<u32, String> {
        (self.guard)()?;
        let result = view::read_resource_u32_at(
            &self.rpc,
            &self.metadata,
            name,
            &self.observation.block_hash,
        )
        .await;
        (self.guard)()?;
        result.map_err(reason)
    }

    async fn require_suffix(&self, expected: &[u8]) -> Result<(), String> {
        let key = [
            twox_128(b"NetworkSuffix").as_slice(),
            twox_128(b"NetworkSuffix").as_slice(),
        ]
        .concat();
        let bytes = self
            .storage(key)
            .await?
            .ok_or_else(|| "NetworkSuffix is absent".to_string())?;
        let actual: Vec<u8> = decode_all(&bytes, "NetworkSuffix")?;
        if actual != expected {
            return Err("chain NetworkSuffix differs from the active wallet".to_string());
        }
        Ok(())
    }

    /// Match the allocator's newest-to-oldest baked-ring search. A member's
    /// current record can point at an unbaked ring while an older proof works.
    pub(crate) async fn memberships(
        &self,
        candidates: &[CollectionCandidate],
    ) -> Result<Vec<bool>, String> {
        self.storage_type("Members", "Collections")?;
        self.storage_type("Members", "CurrentRingIndex")?;
        self.storage_type("Members", "RingKeysStatus")?;
        self.storage_type("Members", "RingKeys")?;
        let mut result = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let values = self
                .storage_many(&[
                    ring::collections_key(candidate.collection),
                    ring::current_ring_index_key(candidate.collection),
                ])
                .await?;
            let Some(info) = &values[0] else {
                result.push(false);
                continue;
            };
            let info: ring::CollectionInfo = self.decode_storage("Members", "Collections", info)?;
            let exponent = info.ring_size.exponent();
            proof::domain_for_ring_exponent(exponent).map_err(reason)?;
            let current = values[1]
                .as_deref()
                .map(|bytes| self.decode_storage("Members", "CurrentRingIndex", bytes))
                .transpose()?
                .unwrap_or(0);
            result.push(
                self.has_including_ring(
                    candidate.collection,
                    &proof::member_key(candidate.entropy),
                    exponent,
                    current,
                )
                .await?,
            );
        }
        Ok(result)
    }

    async fn has_including_ring(
        &self,
        collection: PersonhoodCollection,
        member: &[u8; 32],
        exponent: u8,
        current: u32,
    ) -> Result<bool, String> {
        let capacity = 1u32 << exponent;
        for ring_index in (0..=current).rev() {
            let status = self
                .storage(ring::ring_keys_status_key(collection, ring_index))
                .await?
                .map(|bytes| self.decode_storage::<RingStatus>("Members", "RingKeysStatus", &bytes))
                .transpose()?;
            if status
                .as_ref()
                .is_some_and(|status| status.included > status.total || status.total > capacity)
            {
                return Err("ring included/total exceeds its domain".to_string());
            }
            // The allocator treats a missing status as an entirely baked ring.
            let total = status.as_ref().map(|status| status.total);
            let included = status.as_ref().map_or(capacity, |status| status.included);
            let mut count = 0u32;
            let mut found = false;
            for page in 0..=capacity {
                if total == Some(count) {
                    break;
                }
                let value = self
                    .storage(ring::ring_keys_key(collection, ring_index, page))
                    .await?;
                let Some(bytes) = value else { break };
                let members: Vec<[u8; 32]> = decode_all(&bytes, "Members.RingKeys")?;
                if members.is_empty() {
                    break;
                }
                for key in members {
                    if count >= total.unwrap_or(capacity) {
                        return Err("ring key pages exceed total".to_string());
                    }
                    found |= count < included && &key == member;
                    count += 1;
                }
            }
            if total.is_some_and(|total| count != total) {
                return Err("ring key pages are incomplete".to_string());
            }
            if found {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) async fn statement(
        &self,
        candidates: &[CollectionCandidate],
        memberships: &[bool],
        suffix: &[u8],
        labels: &AccountLabels,
    ) -> Result<StatementAllowanceSnapshot, String> {
        self.require_suffix(suffix).await?;
        self.storage_type("Resources", "StatementStoreAllowances")?;
        let (period, resets_at) = period(
            self.observation.chain_timestamp,
            slot::STATEMENT_STORE_PERIOD_SECONDS,
        )?;
        let grace_seconds = self.policy("get_stmt_store_grace_window").await?;
        let replacement_cooldown_seconds =
            self.policy("get_stmt_store_replacement_cooldown").await?;
        let mut pools = Vec::with_capacity(candidates.len());
        for (candidate, &member) in candidates.iter().zip(memberships) {
            let limit = bounded_limit(
                self.policy(candidate.collection.slots_per_period_view())
                    .await?,
            )?;
            let mut slots = Vec::new();
            for start in (0..limit).step_by(STORAGE_BATCH) {
                (self.guard)()?;
                let end = (start + STORAGE_BATCH as u32).min(limit);
                let keys = (start..end)
                    .map(|index| {
                        slot::slot_alias(candidate.entropy, suffix, period, index)
                            .map(|alias| slot::statement_store_allowance_key(period, &alias))
                            .map_err(reason)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for (index, bytes) in (start..end).zip(self.storage_many(&keys).await?) {
                    let Some(bytes) = bytes else {
                        continue;
                    };
                    let entry: slot::StatementStoreAllowanceEntry =
                        self.decode_storage("Resources", "StatementStoreAllowances", &bytes)?;
                    if entry.seq != index || entry.since > self.observation.chain_timestamp {
                        return Err("statement slot index or timestamp contradicts its snapshot"
                            .to_string());
                    }
                    let label = labels.get(&entry.account_id);
                    slots.push(AllowanceSlot {
                        index,
                        account_id: Some(account_hex(&entry.account_id)),
                        since: Some(entry.since),
                        product_id: label.and_then(|label| label.0.clone()),
                        label: label.map(|label| label.1.clone()),
                    });
                }
            }
            pools.push(pool(candidate.collection, member, member, limit, slots)?);
        }
        Ok(StatementAllowanceSnapshot {
            claims: AllowanceClaims {
                period,
                resets_at,
                pools,
            },
            grace_seconds,
            replacement_cooldown_seconds,
        })
    }

    async fn claimed_slots(
        &self,
        candidate: &CollectionCandidate,
        suffix: &[u8],
        period: u32,
        limit: u32,
        bulletin: bool,
    ) -> Result<Vec<AllowanceSlot>, String> {
        let limit = bounded_limit(limit)?;
        let (pallet, entry) = if bulletin {
            ("Resources", "SpentLongTermStorageAliases")
        } else {
            ("Pgas", "ClaimedGasAliases")
        };
        self.storage_type(pallet, entry)?;
        let mut slots = Vec::new();
        for start in (0..limit).step_by(STORAGE_BATCH) {
            (self.guard)()?;
            let end = (start + STORAGE_BATCH as u32).min(limit);
            let keys = (start..end)
                .map(|index| {
                    if bulletin {
                        let counter = u8::try_from(index).map_err(reason)?;
                        let alias = slot::long_term_storage_alias(
                            candidate.entropy,
                            suffix,
                            period,
                            counter,
                        )
                        .map_err(reason)?;
                        Ok(slot::spent_long_term_storage_alias_key(period, &alias))
                    } else {
                        let alias = slot::pgas_alias(candidate.entropy, suffix, period, index)
                            .map_err(reason)?;
                        Ok(slot::claimed_gas_alias_key(period, &alias))
                    }
                })
                .collect::<Result<Vec<_>, String>>()?;
            for (index, bytes) in (start..end).zip(self.storage_many(&keys).await?) {
                if let Some(bytes) = bytes {
                    self.decode_storage::<()>(pallet, entry, &bytes)?;
                    slots.push(AllowanceSlot {
                        index,
                        account_id: None,
                        product_id: None,
                        label: None,
                        since: None,
                    });
                }
            }
        }
        Ok(slots)
    }

    pub(crate) async fn pgas_claims(
        &self,
        candidates: &[CollectionCandidate],
        memberships: &[bool],
        membership_observation: &AllowanceObservation,
        suffix: &[u8],
    ) -> Result<PgasClaimsSnapshot, String> {
        self.require_suffix(suffix).await?;
        let asset_id: u32 = self.constant("Pgas", "PgasAssetId")?;
        let claim_amount: u128 = self.constant("Pgas", "PgasClaimAmount")?;
        let (period, resets_at) = period(
            self.observation.chain_timestamp,
            slot::STATEMENT_STORE_PERIOD_SECONDS,
        )?;
        let selected = memberships.iter().position(|member| *member);
        let mut pools = Vec::with_capacity(candidates.len());
        for (index, (candidate, &member)) in candidates.iter().zip(memberships).enumerate() {
            let limit: u32 = self.constant(
                "Pgas",
                candidate.collection.pgas_claims_per_period_constant(),
            )?;
            let slots = self
                .claimed_slots(candidate, suffix, period, limit, false)
                .await?;
            pools.push(pool(
                candidate.collection,
                member,
                selected == Some(index),
                limit,
                slots,
            )?);
        }
        Ok(PgasClaimsSnapshot {
            claims: AllowanceClaims {
                period,
                resets_at,
                pools,
            },
            asset_id: asset_id.to_string(),
            claim_amount: claim_amount.to_string(),
            membership_observation: membership_observation.clone(),
        })
    }

    pub(crate) async fn bulletin_claims(
        &self,
        candidates: &[CollectionCandidate],
        memberships: &[bool],
        suffix: &[u8],
    ) -> Result<AllowanceClaims, String> {
        self.require_suffix(suffix).await?;
        let duration: u32 = self.constant("Resources", "LongTermStoragePeriodDuration")?;
        let (period, resets_at) = period(self.observation.chain_timestamp, u64::from(duration))?;
        let limit = self
            .policy("get_long_term_storage_claims_per_period")
            .await?;
        u8::try_from(limit)
            .map_err(|_| "Bulletin claim limit exceeds the native counter width".to_string())?;
        let selected = candidates
            .iter()
            .zip(memberships)
            .position(|(c, m)| *m && c.collection == PersonhoodCollection::LitePeople)
            .or_else(|| memberships.iter().position(|member| *member));
        let mut pools = Vec::with_capacity(candidates.len());
        for (index, (candidate, &member)) in candidates.iter().zip(memberships).enumerate() {
            let slots = self
                .claimed_slots(candidate, suffix, period, limit, true)
                .await?;
            pools.push(pool(
                candidate.collection,
                member,
                selected == Some(index),
                limit,
                slots,
            )?);
        }
        Ok(AllowanceClaims {
            period,
            resets_at,
            pools,
        })
    }

    pub(crate) async fn pgas_balances(
        &self,
        products: &[ProductAccounts],
    ) -> Result<PgasBalancesSnapshot, String> {
        let asset_id: u32 = self.constant("Pgas", "PgasAssetId")?;
        self.storage_type("Assets", "Metadata")?;
        self.storage_type("Assets", "Account")?;
        let key = [
            twox_128(b"Assets").as_slice(),
            twox_128(b"Metadata").as_slice(),
            &ring::blake2_128_concat(&asset_id.to_le_bytes()),
        ]
        .concat();
        let metadata = self.storage(key).await?;
        let (decimals, symbol) = match metadata {
            Some(bytes) => {
                let value: AssetMetadata = self.decode_storage("Assets", "Metadata", &bytes)?;
                (
                    Some(value.decimals),
                    String::from_utf8(value.symbol)
                        .ok()
                        .filter(|symbol| !symbol.is_empty()),
                )
            }
            None => (None, None),
        };
        let keys = products
            .iter()
            .map(|p| pgas::pgas_balance_key(asset_id, &p.pgas))
            .collect::<Vec<_>>();
        let values = self.storage_many(&keys).await?;
        let accounts = products
            .iter()
            .zip(values)
            .map(|(product, value)| {
                let balance = match value {
                    Some(bytes) => self
                        .decode_storage::<AssetAccount>("Assets", "Account", &bytes)
                        .map(|a| a.balance.to_string()),
                    None => Ok("0".to_string()),
                };
                let (balance, error) = match balance {
                    Ok(balance) => (Some(balance), None),
                    Err(error) => (None, Some(error)),
                };
                PgasBalance {
                    product_id: product.product_id.clone(),
                    account_id: account_hex(&product.pgas),
                    derivation_index: 0,
                    balance,
                    error,
                }
            })
            .collect();
        Ok(PgasBalancesSnapshot {
            asset_id: asset_id.to_string(),
            decimals,
            symbol,
            accounts,
        })
    }

    pub(crate) async fn bulletin_quotas(
        &self,
        products: &[ProductAccounts],
    ) -> Result<BulletinQuotasSnapshot, String> {
        self.storage_type("TransactionStorage", "Authorizations")?;
        let keys = products
            .iter()
            .map(|p| super::bulletin_authorization_key(&p.bulletin))
            .collect::<Vec<_>>();
        let values = self.storage_many(&keys).await?;
        let accounts = products
            .iter()
            .zip(values)
            .map(|(product, bytes)| {
                let mut quota = BulletinQuota {
                    product_id: product.product_id.clone(),
                    account_id: account_hex(&product.bulletin),
                    status: "missing",
                    bytes_used: None,
                    bytes_limit: None,
                    bytes_remaining: None,
                    transactions_used: None,
                    transactions_limit: None,
                    transactions_remaining: None,
                    expires_at_block: None,
                    error: None,
                };
                if let Some(bytes) = bytes {
                    let result = self
                        .decode_storage::<BulletinAuthorization>(
                            "TransactionStorage",
                            "Authorizations",
                            &bytes,
                        )
                        .map(|value| value.apply(&mut quota, self.observation.block_number));
                    if let Err(error) = result {
                        quota.status = "unavailable";
                        quota.error = Some(error);
                    }
                }
                quota
            })
            .collect();
        Ok(BulletinQuotasSnapshot { accounts })
    }
}

#[derive(DecodeAsType)]
struct RingStatus {
    total: u32,
    included: u32,
}
#[derive(DecodeAsType)]
struct AssetMetadata {
    decimals: u8,
    symbol: Vec<u8>,
}
#[derive(DecodeAsType)]
struct AssetAccount {
    balance: u128,
}
#[derive(DecodeAsType)]
struct BulletinAuthorization {
    extent: BulletinExtent,
    expiration: u32,
}

#[derive(DecodeAsType)]
struct BulletinExtent {
    transactions: u32,
    transactions_allowance: u32,
    bytes: u64,
    bytes_allowance: u64,
}

impl BulletinAuthorization {
    fn apply(self, quota: &mut BulletinQuota, at: u32) {
        // These counters are soft consumption signals, not hard validity caps.
        // Preserve over-limit usage; the native allocator saturates remaining.
        let extent = self.extent;
        quota.status = if at >= self.expiration {
            "expired"
        } else {
            "active"
        };
        quota.bytes_used = Some(extent.bytes.to_string());
        quota.bytes_limit = Some(extent.bytes_allowance.to_string());
        quota.bytes_remaining = Some(
            extent
                .bytes_allowance
                .saturating_sub(extent.bytes)
                .to_string(),
        );
        quota.transactions_used = Some(extent.transactions);
        quota.transactions_limit = Some(extent.transactions_allowance);
        quota.transactions_remaining = Some(
            extent
                .transactions_allowance
                .saturating_sub(extent.transactions),
        );
        quota.expires_at_block = Some(self.expiration);
    }
}

pub(crate) fn account_hex(account: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(account))
}

fn period(now: u64, duration: u64) -> Result<(u32, u64), String> {
    if duration == 0 {
        return Err("allowance period duration is zero".to_string());
    }
    let period = u32::try_from(now / duration).map_err(reason)?;
    let resets_at = (u64::from(period) + 1)
        .checked_mul(duration)
        .filter(|value| *value <= JS_MAX_INTEGER)
        .ok_or_else(|| "allowance reset timestamp overflow".to_string())?;
    Ok((period, resets_at))
}

fn bounded_limit(limit: u32) -> Result<u32, String> {
    if limit > MAX_INSPECTION_SLOTS {
        return Err(format!(
            "runtime allowance limit {limit} exceeds the bounded inspection limit"
        ));
    }
    Ok(limit)
}

fn pool(
    collection: PersonhoodCollection,
    member: bool,
    selected: bool,
    limit: u32,
    slots: Vec<AllowanceSlot>,
) -> Result<AllowancePool, String> {
    let used = u32::try_from(slots.len()).map_err(reason)?;
    let remaining = limit
        .checked_sub(used)
        .ok_or_else(|| "occupied slots exceed the runtime limit".to_string())?;
    Ok(AllowancePool {
        collection: collection.metadata_variant().to_string(),
        membership: if member { "verified" } else { "not-found" },
        selected: member && selected,
        limit,
        used,
        remaining: if member { remaining } else { 0 },
        slots,
    })
}

#[cfg(test)]
mod tests {
    use super::super::rpc::testing::ScriptedRpc;
    use super::*;
    use subxt_rpcs::RpcClient as HostRpcClient;

    fn empty_quota() -> BulletinQuota {
        BulletinQuota {
            product_id: "app.dot".to_string(),
            account_id: account_hex(&[1; 32]),
            status: "missing",
            bytes_used: None,
            bytes_limit: None,
            bytes_remaining: None,
            transactions_used: None,
            transactions_limit: None,
            transactions_remaining: None,
            expires_at_block: None,
            error: None,
        }
    }

    #[test]
    fn quota_expiry_is_the_block_boundary_and_soft_overuse_is_preserved() {
        let authorization = || BulletinAuthorization {
            extent: BulletinExtent {
                transactions: 3,
                transactions_allowance: 5,
                bytes: 10,
                bytes_allowance: 15,
            },
            expiration: 20,
        };
        let mut quota = empty_quota();
        authorization().apply(&mut quota, 19);
        assert_eq!(quota.status, "active");
        assert_eq!(quota.bytes_remaining.as_deref(), Some("5"));
        assert_eq!(quota.transactions_remaining, Some(2));
        authorization().apply(&mut quota, 20);
        assert_eq!(quota.status, "expired");
        let mut excessive = authorization();
        excessive.extent.bytes = 16;
        excessive.extent.transactions = 6;
        excessive.apply(&mut quota, 19);
        assert_eq!(quota.bytes_used.as_deref(), Some("16"));
        assert_eq!(quota.bytes_remaining.as_deref(), Some("0"));
        assert_eq!(quota.transactions_used, Some(6));
        assert_eq!(quota.transactions_remaining, Some(0));
    }

    #[test]
    fn bulletin_storage_uses_its_metadata_extent_and_rejects_truncation() {
        fn active() -> Result<(), String> {
            Ok(())
        }
        let fixture = include_bytes!("../../../tests/fixtures/bulletin_paseo_metadata.scale");
        let chain = ReadOnlyChain {
            rpc: RpcClient::new(HostRpcClient::new(ScriptedRpc::new(
                std::iter::empty::<&str>(),
            ))),
            metadata: decode_metadata(fixture).unwrap(),
            observation: AllowanceObservation {
                genesis_hash: account_hex(&[0; 32]),
                block_hash: account_hex(&[1; 32]),
                block_number: 19,
                spec_version: 1,
                chain_timestamp: 0,
            },
            guard: &active,
        };
        let bytes = (3u32, 5u32, 10u64, 0u64, 15u64, 20u32).encode();
        let authorization: BulletinAuthorization = chain
            .decode_storage("TransactionStorage", "Authorizations", &bytes)
            .unwrap();
        let mut quota = empty_quota();
        authorization.apply(&mut quota, 19);
        assert_eq!(quota.bytes_remaining.as_deref(), Some("5"));
        assert_eq!(quota.transactions_remaining, Some(2));
        assert!(
            chain
                .decode_storage::<BulletinAuthorization>(
                    "TransactionStorage",
                    "Authorizations",
                    &bytes[..bytes.len() - 1]
                )
                .is_err()
        );
        let mut trailing = bytes;
        trailing.push(0);
        assert!(
            chain
                .decode_storage::<BulletinAuthorization>(
                    "TransactionStorage",
                    "Authorizations",
                    &trailing
                )
                .is_err()
        );
    }

    #[test]
    fn a_lost_membership_preserves_occupancy_but_never_advertises_capacity() {
        let slot = AllowanceSlot {
            index: 0,
            account_id: Some(account_hex(&[1; 32])),
            product_id: None,
            label: None,
            since: Some(7),
        };
        let result = pool(PersonhoodCollection::People, false, true, 10, vec![slot]).unwrap();
        assert_eq!(
            (result.limit, result.used, result.remaining, result.selected),
            (10, 1, 0, false)
        );
    }

    #[test]
    fn periods_reject_zero_duration_and_unrepresentable_resets() {
        assert_eq!(period(86_399, 86_400).unwrap(), (0, 86_400));
        assert_eq!(period(86_400, 86_400).unwrap(), (1, 172_800));
        assert!(period(1, 0).is_err());
        assert!(period(u64::MAX, u64::MAX).is_err());
    }

    #[test]
    fn stale_activation_is_rejected_after_a_successful_rpc_read() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let checks = AtomicUsize::new(0);
        let guard = || {
            if checks.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(())
            } else {
                Err("activation changed".to_string())
            }
        };
        let scripted = ScriptedRpc::new(["null"]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted));
        let result =
            futures::executor::block_on(checked_call(&rpc, &guard, "chain_getHeader", json!([])));
        assert_eq!(result.unwrap_err(), "activation changed");
    }

    #[test]
    fn membership_can_use_an_older_baked_ring_when_the_current_ring_is_unbaked() {
        fn active() -> Result<(), String> {
            Ok(())
        }
        let collection = PersonhoodCollection::LitePeople;
        let member = proof::member_key([7; 32]);
        let block = account_hex(&[1; 32]);
        let response = |key: Vec<u8>, bytes: Vec<u8>| {
            json!([{
                "block": block,
                "changes": [[
                    format!("0x{}", hex::encode(key)),
                    format!("0x{}", hex::encode(bytes)),
                ]],
            }])
            .to_string()
        };
        let replies = [
            response(
                ring::ring_keys_status_key(collection, 1),
                (1u32, 0u32, None::<u64>).encode(),
            ),
            response(ring::ring_keys_key(collection, 1, 0), vec![member].encode()),
            response(
                ring::ring_keys_status_key(collection, 0),
                (1u32, 1u32, None::<u64>).encode(),
            ),
            response(ring::ring_keys_key(collection, 0, 0), vec![member].encode()),
        ];
        let chain = ReadOnlyChain {
            rpc: RpcClient::new(HostRpcClient::new(ScriptedRpc::new(
                replies.iter().map(String::as_str),
            ))),
            metadata: decode_metadata(include_bytes!(
                "../../../tests/fixtures/paseo-next-v2-metadata-v16.scale"
            ))
            .unwrap(),
            observation: AllowanceObservation {
                genesis_hash: account_hex(&[0; 32]),
                block_hash: block,
                block_number: 19,
                spec_version: 1,
                chain_timestamp: 0,
            },
            guard: &active,
        };
        assert!(
            futures::executor::block_on(chain.has_including_ring(collection, &member, 12, 1))
                .unwrap()
        );
    }

    #[test]
    fn metadata_decoding_rejects_trailing_bytes() {
        let fixture = include_bytes!("../../../tests/fixtures/bulletin_paseo_metadata.scale");
        let mut corrupted = fixture.to_vec();
        corrupted.push(0);
        assert!(decode_metadata(&corrupted).is_err());
    }
}
