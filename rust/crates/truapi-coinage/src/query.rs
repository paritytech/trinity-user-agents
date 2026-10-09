// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::members::{self, RingPosition, RingRoot};
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use parity_scale_codec::{Decode, DecodeAll, Encode};
use tokio::sync::mpsc;
use tracing::warn;

use crate::claim::SendConfirmation;
use crate::repo::VoucherRepository;
use crate::selection::RecyclerKey;
use crate::sync::OnChainCoin;
use crate::voucher_location::{
    RingPosition as VoucherRingPosition, RingStatus as VoucherRingStatus,
    VoucherLocationSubscriber, VoucherLocationUpdate,
};

/// Semantic Coinage storage requests. The Host resolves physical keys
/// from runtime metadata pinned to the query block and the configured asset
/// instance. Recycler collections are never encoded by the portable engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoinageStorageKey {
    Coin([u8; 32]),
    Recycler([u8; 32]),
    RecyclerAlias {
        exponent: i8,
        ring_index: u32,
        alias: [u8; 32],
    },
    Member {
        exponent: i16,
        member: [u8; 32],
    },
    Root {
        exponent: i16,
        ring_index: u32,
    },
    RingKeysStatus {
        exponent: i16,
        ring_index: u32,
    },
}

impl CoinageStorageKey {
    fn recycler_alias(
        exponent: i16,
        ring_index: u32,
        alias: [u8; 32],
    ) -> Result<Self, CoinageQueryError> {
        let exponent =
            i8::try_from(exponent).map_err(|_| CoinageQueryError::InvalidExponent(exponent))?;
        Ok(Self::RecyclerAlias {
            exponent,
            ring_index,
            alias,
        })
    }
}

/// The storage/finalized-head effect. Production uses a Host RPC
/// adapter; tests provide a deterministic batch source.
#[async_trait]
pub trait CoinageStorageQuery: Send + Sync {
    /// One head-pinned query, resolving semantic keys against that head's
    /// runtime metadata and configured asset instance, preserving request order.
    ///
    /// Values are canonical engine SCALE rows, not raw runtime storage bytes:
    /// `Coin` is `(i8, u16)`, `Recycler` is `i8`, `RecyclerAlias` is `AliasState`,
    /// `Member` is `RingPosition`, `Root` is `RingRoot`, and `RingKeysStatus` is
    /// `members::RingStatus`. The Host must validate each complete runtime value
    /// against metadata and the selected asset before removing runtime-specific
    /// instance fields and returning the canonical row.
    async fn query(
        &self,
        keys: &[CoinageStorageKey],
        at: Option<[u8; 32]>,
    ) -> Result<Vec<Option<Vec<u8>>>, String>;

    /// The canonical finalized head used to pin a coherent multi-stage
    /// query. Test/deterministic sources may supply it through their
    /// finalized stream; the RPC adapter uses the direct one-shot method.
    async fn finalized_head(&self) -> Result<[u8; 32], String> {
        let mut heads = self.finalized_heads();
        heads
            .next()
            .await
            .ok_or_else(|| "finalized-head subscription terminated".to_string())?
    }

    /// A fresh finalized-head stream. Consumers race this against their
    /// storage condition and block timeout.
    fn finalized_heads(&self) -> BoxStream<'static, Result<[u8; 32], String>>;

    fn observation_heads(&self) -> BoxStream<'static, Result<[u8; 32], String>> {
        self.finalized_heads()
    }
}

/// Typed failures from the query layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoinageQueryError {
    Storage(String),
    Decode {
        storage: &'static str,
        message: String,
    },
    ResponseLength {
        storage: &'static str,
        expected: usize,
        actual: usize,
    },
    SubscriptionTerminated,
    Timeout {
        finalized_heads: u32,
    },
    InvalidExponent(i16),
}

impl fmt::Display for CoinageQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(message) => formatter.write_str(message),
            Self::Decode { storage, message } => {
                write!(formatter, "{storage} SCALE decode failed: {message}")
            }
            Self::ResponseLength {
                storage,
                expected,
                actual,
            } => write!(
                formatter,
                "{storage} returned {actual} values for {expected} keys"
            ),
            Self::SubscriptionTerminated => {
                formatter.write_str("finalized-head subscription terminated")
            }
            Self::Timeout { finalized_heads } => {
                write!(
                    formatter,
                    "coin query timed out after {finalized_heads} finalized heads"
                )
            }
            Self::InvalidExponent(exponent) => {
                write!(
                    formatter,
                    "coinage exponent {exponent} does not fit the runtime i8"
                )
            }
        }
    }
}

impl std::error::Error for CoinageQueryError {}

/// `Coinage.RecyclerAliasStates` value
/// (`indiv_pallet_coinage::pallet::AliasState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum AliasState {
    /// Load/unload is locked until the carried seconds timestamp
    /// (`LockInfo { reason: LockReason::FailedDispatch(u8), until: u64 }`).
    #[codec(index = 0)]
    Locked(LockInfo),
    #[codec(index = 1)]
    Unloaded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct LockInfo {
    pub reason: LockReason,
    pub until: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum LockReason {
    #[codec(index = 0)]
    FailedDispatch(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
struct RawOnChainCoin {
    value: i8,
    age: u16,
}

fn decode_coin(bytes: &[u8]) -> Result<OnChainCoin, CoinageQueryError> {
    let raw =
        RawOnChainCoin::decode_all(&mut &bytes[..]).map_err(|error| CoinageQueryError::Decode {
            storage: "Coinage.CoinsByOwner",
            message: error.to_string(),
        })?;
    let age = i16::try_from(raw.age).map_err(|_| CoinageQueryError::Decode {
        storage: "Coinage.CoinsByOwner",
        message: format!("age {} does not fit i16", raw.age),
    })?;
    Ok(OnChainCoin {
        exponent: i16::from(raw.value),
        age,
    })
}

fn decode_optional<T: DecodeAll>(
    bytes: Option<Vec<u8>>,
    storage: &'static str,
) -> Result<Option<T>, CoinageQueryError> {
    bytes
        .map(|bytes| {
            T::decode_all(&mut &bytes[..]).map_err(|error| CoinageQueryError::Decode {
                storage,
                message: error.to_string(),
            })
        })
        .transpose()
}

fn ensure_response_len(
    storage: &'static str,
    expected: usize,
    actual: usize,
) -> Result<(), CoinageQueryError> {
    if expected == actual {
        Ok(())
    } else {
        Err(CoinageQueryError::ResponseLength {
            storage,
            expected,
            actual,
        })
    }
}

/// Batch coin fetch + finalized-head-raced presence/claim waits.
pub struct CoinOnChainQueryService {
    storage: Arc<dyn CoinageStorageQuery>,
}

impl CoinOnChainQueryService {
    pub fn new(storage: Arc<dyn CoinageStorageQuery>) -> Self {
        Self { storage }
    }

    /// Fetches N `CoinsByOwner` entries in one RPC, preserving input
    /// order. `at` enables the historical anchor probe used after a fast
    /// claim.
    pub async fn fetch_coins(
        &self,
        public_keys: &[[u8; 32]],
        at: Option<[u8; 32]>,
    ) -> Result<Vec<Option<OnChainCoin>>, CoinageQueryError> {
        if public_keys.is_empty() {
            return Ok(Vec::new());
        }
        let keys = public_keys
            .iter()
            .copied()
            .map(CoinageStorageKey::Coin)
            .collect::<Vec<_>>();
        let values = self
            .storage
            .query(&keys, at)
            .await
            .map_err(CoinageQueryError::Storage)?;
        ensure_response_len("Coinage.CoinsByOwner", keys.len(), values.len())?;
        values
            .into_iter()
            .map(|value| value.map(|bytes| decode_coin(&bytes)).transpose())
            .collect()
    }

    /// Resolves once every key has been observed present. Presence is
    /// accumulated across finalized heads because a storage subscription
    /// may emit only the keys changed in a block, and an early coin may
    /// already be claimed by the time a later one appears.
    pub async fn await_all_coins_on_chain(
        &self,
        public_keys: &[[u8; 32]],
        block_timeout: u32,
    ) -> Result<(), CoinageQueryError> {
        self.await_all_coins_sent_or_claimed(public_keys, block_timeout)
            .await
            .map(|_| ())
    }

    /// Resolves once every key is absent (claimed/spent).
    pub async fn await_all_coins_off_chain(
        &self,
        public_keys: &[[u8; 32]],
        block_timeout: u32,
    ) -> Result<(), CoinageQueryError> {
        self.await_condition(public_keys, block_timeout, |coins| {
            coins.iter().all(Option::is_none).then_some(())
        })
        .await
    }

    /// Accumulates `seen` across partial lifecycle observations: once all
    /// keys have appeared at least once, returns whether any remains
    /// present. This distinguishes `OnChain` from an ultra-fast
    /// `AlreadyClaimed` completion and races the finalized-head count.
    pub async fn await_all_coins_sent_or_claimed(
        &self,
        public_keys: &[[u8; 32]],
        block_timeout: u32,
    ) -> Result<bool, CoinageQueryError> {
        if public_keys.is_empty() {
            return Ok(true);
        }
        let mut heads = self.storage.finalized_heads();
        let mut seen = vec![false; public_keys.len()];
        let mut present = vec![false; public_keys.len()];
        let limit = block_timeout.max(1);
        let mut count = 0u32;
        while let Some(head) = heads.next().await {
            let head = head.map_err(CoinageQueryError::Storage)?;
            count = count.saturating_add(1);
            let coins = self.fetch_coins(public_keys, Some(head)).await?;
            for (index, coin) in coins.into_iter().enumerate() {
                present[index] = coin.is_some();
                if present[index] {
                    seen[index] = true;
                }
            }
            if seen.iter().all(|value| *value) {
                return Ok(present.iter().any(|value| *value));
            }
            if count >= limit {
                return Err(CoinageQueryError::Timeout {
                    finalized_heads: limit,
                });
            }
        }
        Err(CoinageQueryError::SubscriptionTerminated)
    }

    pub async fn await_send_or_claimed(
        &self,
        public_keys: &[[u8; 32]],
        anchor_hash: Option<[u8; 32]>,
        block_timeout: u32,
    ) -> Result<SendConfirmation, CoinageQueryError> {
        if public_keys.is_empty() {
            return Ok(SendConfirmation::OnChain);
        }
        let mut confirmed = vec![false; public_keys.len()];
        let current = self.fetch_coins(public_keys, None).await?;
        let mut any_present = false;
        for (index, coin) in current.into_iter().enumerate() {
            if coin.is_some() {
                confirmed[index] = true;
                any_present = true;
            }
        }

        if confirmed.iter().any(|value| !*value)
            && let Some(anchor_hash) = anchor_hash
        {
            let absent_offsets = confirmed
                .iter()
                .enumerate()
                .filter_map(|(index, confirmed)| (!confirmed).then_some(index))
                .collect::<Vec<_>>();
            let absent_keys = absent_offsets
                .iter()
                .map(|index| public_keys[*index])
                .collect::<Vec<_>>();
            let historical = self.fetch_coins(&absent_keys, Some(anchor_hash)).await?;
            for (offset, coin) in historical.into_iter().enumerate() {
                if coin.is_some() {
                    confirmed[absent_offsets[offset]] = true;
                }
            }
        }

        if confirmed.iter().all(|value| *value) {
            return Ok(if any_present {
                SendConfirmation::OnChain
            } else {
                SendConfirmation::AlreadyClaimed
            });
        }

        let remaining = confirmed
            .iter()
            .enumerate()
            .filter_map(|(index, confirmed)| (!confirmed).then_some(public_keys[index]))
            .collect::<Vec<_>>();
        let any_remaining_present = self
            .await_all_coins_sent_or_claimed(&remaining, block_timeout)
            .await?;
        Ok(if any_present || any_remaining_present {
            SendConfirmation::OnChain
        } else {
            SendConfirmation::AlreadyClaimed
        })
    }

    async fn await_condition<T>(
        &self,
        public_keys: &[[u8; 32]],
        block_timeout: u32,
        condition: impl Fn(&[Option<OnChainCoin>]) -> Option<T>,
    ) -> Result<T, CoinageQueryError> {
        if public_keys.is_empty() {
            return condition(&[]).ok_or(CoinageQueryError::SubscriptionTerminated);
        }
        let mut heads = self.storage.finalized_heads();
        let limit = block_timeout.max(1);
        let mut count = 0u32;
        while let Some(head) = heads.next().await {
            let head = head.map_err(CoinageQueryError::Storage)?;
            count = count.saturating_add(1);
            let coins = self.fetch_coins(public_keys, Some(head)).await?;
            if let Some(output) = condition(&coins) {
                return Ok(output);
            }
            if count >= limit {
                return Err(CoinageQueryError::Timeout {
                    finalized_heads: limit,
                });
            }
        }
        Err(CoinageQueryError::SubscriptionTerminated)
    }
}

/// Complete on-chain voucher observation after the four query stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoucherOnChainInfo {
    pub exponent: i16,
    pub ring_position: RingPosition,
    pub is_unloaded: bool,
}

type KeyProvider = dyn Fn(u32) -> Result<[u8; 32], String> + Send + Sync;

#[derive(Debug, Clone)]
enum VoucherLocationRequest {
    Position { derivation_index: u32 },
    Status { derivation_index: u32 },
}

pub struct QueryVoucherLocationSubscriber {
    spawner: crate::Spawner,
    storage: Arc<dyn CoinageStorageQuery>,
    vouchers: Arc<dyn VoucherRepository>,
    public_key_provider: Arc<KeyProvider>,
    observation_refresh_interval: Duration,
}

/// A head notification is the fast path, while this current-best refresh is
/// the recovery path. In particular, a rejected/stopped `chainHead` follow
/// must not strand vouchers whose ring proof set advances after their member
/// position was first observed.
const VOUCHER_LOCATION_OBSERVATION_REFRESH: Duration = Duration::from_secs(8);

impl QueryVoucherLocationSubscriber {
    pub fn new(
        storage: Arc<dyn CoinageStorageQuery>,
        vouchers: Arc<dyn VoucherRepository>,
        public_key_provider: Arc<KeyProvider>,
        spawner: crate::Spawner,
    ) -> Self {
        Self {
            storage,
            vouchers,
            public_key_provider,
            observation_refresh_interval: VOUCHER_LOCATION_OBSERVATION_REFRESH,
            spawner,
        }
    }

    #[cfg(test)]
    fn with_observation_refresh_interval(mut self, interval: Duration) -> Self {
        self.observation_refresh_interval = interval;
        self
    }

    async fn requests(
        &self,
        pending: Vec<u32>,
        included: Vec<(u32, u32)>,
        degraded: Vec<(u32, u32)>,
    ) -> Result<Vec<(VoucherLocationRequest, CoinageStorageKey)>, String> {
        let vouchers = self
            .vouchers
            .list()
            .await?
            .into_iter()
            .map(|voucher| (voucher.derivation_index, voucher))
            .collect::<HashMap<_, _>>();
        let voucher = |index: u32| {
            vouchers
                .get(&index)
                .ok_or_else(|| format!("voucher {index} disappeared before location subscribe"))
        };

        let mut requests = Vec::with_capacity(pending.len() + included.len() + degraded.len());
        for derivation_index in pending {
            let voucher = voucher(derivation_index)?;
            let member = (self.public_key_provider)(derivation_index)?;
            requests.push((
                VoucherLocationRequest::Position { derivation_index },
                CoinageStorageKey::Member {
                    exponent: voucher.exponent,
                    member,
                },
            ));
        }
        for (derivation_index, ring_index) in included.into_iter().chain(degraded) {
            let voucher = voucher(derivation_index)?;
            requests.push((
                VoucherLocationRequest::Status { derivation_index },
                CoinageStorageKey::RingKeysStatus {
                    exponent: voucher.exponent,
                    ring_index,
                },
            ));
        }
        Ok(requests)
    }
}

async fn fetch_voucher_location_update(
    storage: &dyn CoinageStorageQuery,
    requests: &[(VoucherLocationRequest, CoinageStorageKey)],
    at: Option<[u8; 32]>,
) -> Result<VoucherLocationUpdate, CoinageQueryError> {
    let keys = requests.iter().map(|(_, key)| *key).collect::<Vec<_>>();
    let values = storage
        .query(&keys, at)
        .await
        .map_err(CoinageQueryError::Storage)?;
    ensure_response_len(
        "Members voucher-location batch",
        requests.len(),
        values.len(),
    )?;

    let mut update = VoucherLocationUpdate::default();
    for ((request, _), value) in requests.iter().zip(values) {
        match request {
            VoucherLocationRequest::Position { derivation_index } => {
                let position =
                    decode_optional::<RingPosition>(value, "Members.Members")?.map(|position| {
                        match position {
                            RingPosition::Included {
                                ring_index,
                                ring_position,
                                ..
                            } => VoucherRingPosition::Included {
                                ring_index,
                                included_at: ring_position,
                            },
                            RingPosition::Onboarding { .. } | RingPosition::Suspended => {
                                VoucherRingPosition::Onboarding
                            }
                        }
                    });
                update.ring_positions.push((*derivation_index, position));
            }
            VoucherLocationRequest::Status { derivation_index } => {
                if let Some(status) =
                    decode_optional::<members::RingStatus>(value, "Members.RingKeysStatus")?
                {
                    update.ring_statuses.push((
                        *derivation_index,
                        VoucherRingStatus {
                            included_members: status.included,
                        },
                    ));
                }
            }
        }
    }
    Ok(update)
}

#[async_trait]
impl VoucherLocationSubscriber for QueryVoucherLocationSubscriber {
    async fn subscribe(
        &self,
        pending: Vec<u32>,
        included: Vec<(u32, u32)>,
        degraded: Vec<(u32, u32)>,
    ) -> Result<mpsc::Receiver<VoucherLocationUpdate>, String> {
        let requests = self.requests(pending, included, degraded).await?;
        let storage = Arc::clone(&self.storage);
        let observation_refresh_interval = self.observation_refresh_interval;
        let (sender, receiver) = mpsc::channel(4);
        (self.spawner)(Box::pin(async move {
            // Start the live observer before the initial snapshot so a best
            // change racing the query is buffered rather than lost. `None`
            // makes `state_queryStorageAt` read the current best state, which
            // is the same visibility used by the iOS storage subscription.
            let mut heads = storage.observation_heads();
            match fetch_voucher_location_update(storage.as_ref(), &requests, None).await {
                Ok(update) => {
                    if sender.send(update).await.is_err() {
                        return;
                    }
                }
                Err(error) => warn!(%error, "initial voucher location query failed"),
            }

            let mut heads_open = true;
            let refresh = crate::timer::sleep(observation_refresh_interval).fuse();
            futures::pin_mut!(refresh);
            loop {
                let open = heads_open;
                let next_head = async {
                    if open {
                        heads.next().await
                    } else {
                        futures::future::pending().await
                    }
                }
                .fuse();
                let closed = sender.closed().fuse();
                futures::pin_mut!(next_head, closed);
                let at = futures::select! {
                    head = next_head => match head {
                        Some(Ok(head)) => Some(head),
                        Some(Err(error)) => {
                            warn!(%error, "voucher location head update failed");
                            continue;
                        }
                        None => {
                            heads_open = false;
                            continue;
                        }
                    },
                    _ = refresh => {
                        refresh.set(crate::timer::sleep(observation_refresh_interval).fuse());
                        None
                    },
                    _ = closed => return,
                };

                if sender.is_closed() {
                    return;
                }
                match fetch_voucher_location_update(storage.as_ref(), &requests, at).await {
                    Ok(update) => {
                        if sender.send(update).await.is_err() {
                            return;
                        }
                    }
                    Err(error) => warn!(%error, "voucher location query failed"),
                }
            }
        }));
        Ok(receiver)
    }
}

/// Four-stage voucher query (`RecyclersCoinToRecycler` → `Members` →
/// `RecyclerAliasStates`), preserving the original derivation-index order.
pub struct VoucherOnChainQueryService {
    storage: Arc<dyn CoinageStorageQuery>,
    public_key_provider: Arc<KeyProvider>,
    alias_provider: Arc<KeyProvider>,
}

impl VoucherOnChainQueryService {
    pub fn new(
        storage: Arc<dyn CoinageStorageQuery>,
        public_key_provider: Arc<KeyProvider>,
        alias_provider: Arc<KeyProvider>,
    ) -> Self {
        Self {
            storage,
            public_key_provider,
            alias_provider,
        }
    }

    pub async fn fetch_vouchers(
        &self,
        derivation_indices: &[u32],
        at: Option<[u8; 32]>,
    ) -> Result<Vec<Option<VoucherOnChainInfo>>, CoinageQueryError> {
        self.fetch_voucher_observations(derivation_indices, at, false)
            .await
    }

    /// Recovery must observe every assigned derivation index, not only keys
    /// already included in a recycler ring. Otherwise onboarding/suspended
    /// keys could be allocated again after restoring the wallet.
    ///
    /// Every stage is pinned to the caller's finalized recovery snapshot.
    /// An assignment without a Members row is ambiguous, not an empty index.
    pub async fn fetch_recovery_vouchers(
        &self,
        derivation_indices: &[u32],
        at: [u8; 32],
    ) -> Result<Vec<Option<VoucherOnChainInfo>>, CoinageQueryError> {
        self.fetch_voucher_observations(derivation_indices, Some(at), true)
            .await
    }

    async fn fetch_voucher_observations(
        &self,
        derivation_indices: &[u32],
        at: Option<[u8; 32]>,
        recovery: bool,
    ) -> Result<Vec<Option<VoucherOnChainInfo>>, CoinageQueryError> {
        if derivation_indices.is_empty() {
            return Ok(Vec::new());
        }
        let mut output = vec![None; derivation_indices.len()];

        struct Candidate {
            output_index: usize,
            derivation_index: u32,
            exponent: i16,
            position: RingPosition,
            ring_index: u32,
        }

        // Step 1: key derivation.
        let indexed_keys = derivation_indices
            .iter()
            .copied()
            .enumerate()
            .map(|(output_index, derivation_index)| {
                (self.public_key_provider)(derivation_index)
                    .map(|public_key| (output_index, derivation_index, public_key))
                    .map_err(CoinageQueryError::Storage)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Step 2: recycler denomination.
        let exponent_keys = indexed_keys
            .iter()
            .map(|(_, _, public_key)| CoinageStorageKey::Recycler(*public_key))
            .collect::<Vec<_>>();
        let exponent_values = self
            .storage
            .query(&exponent_keys, at)
            .await
            .map_err(CoinageQueryError::Storage)?;
        ensure_response_len(
            "Coinage.RecyclersCoinToRecycler",
            exponent_keys.len(),
            exponent_values.len(),
        )?;
        let mut with_exponents = Vec::new();
        for ((output_index, derivation_index, public_key), value) in
            indexed_keys.into_iter().zip(exponent_values)
        {
            if let Some(exponent) = decode_optional::<i8>(value, "Coinage.RecyclersCoinToRecycler")?
            {
                with_exponents.push((
                    output_index,
                    derivation_index,
                    public_key,
                    i16::from(exponent),
                ));
            }
        }
        if with_exponents.is_empty() {
            return Ok(output);
        }

        // Step 3: Members position under the denomination's recycler
        // collection. Normal unload queries exclude onboarding/suspended
        // rows; recovery retains them as used, non-unloadable indices.
        let position_keys = with_exponents
            .iter()
            .map(|(_, _, public_key, exponent)| CoinageStorageKey::Member {
                exponent: *exponent,
                member: *public_key,
            })
            .collect::<Vec<_>>();
        let position_values = self
            .storage
            .query(&position_keys, at)
            .await
            .map_err(CoinageQueryError::Storage)?;
        ensure_response_len(
            "Members.Members",
            position_keys.len(),
            position_values.len(),
        )?;
        let mut with_positions = Vec::<Candidate>::new();
        for ((output_index, derivation_index, _public_key, exponent), value) in
            with_exponents.into_iter().zip(position_values)
        {
            let Some(position) = decode_optional::<RingPosition>(value, "Members.Members")? else {
                if recovery {
                    return Err(CoinageQueryError::Storage(
                        "Coinage recovery found a recycler assignment without a Members row".into(),
                    ));
                }
                continue;
            };
            let RingPosition::Included { ring_index, .. } = position else {
                if recovery {
                    output[output_index] = Some(VoucherOnChainInfo {
                        exponent,
                        ring_position: position,
                        is_unloaded: false,
                    });
                }
                continue;
            };
            with_positions.push(Candidate {
                output_index,
                derivation_index,
                exponent,
                position,
                ring_index,
            });
        }
        if with_positions.is_empty() {
            return Ok(output);
        }

        // Step 4: alias state. A missing row is a loaded, unlocked alias;
        // only an explicit `AliasState::Unloaded` counts as unloaded (a
        // `Locked` row is still loaded — the lock only gates dispatch).
        let state_keys = with_positions
            .iter()
            .map(|candidate| {
                let alias = (self.alias_provider)(candidate.derivation_index)
                    .map_err(CoinageQueryError::Storage)?;
                CoinageStorageKey::recycler_alias(candidate.exponent, candidate.ring_index, alias)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let state_values = self
            .storage
            .query(&state_keys, at)
            .await
            .map_err(CoinageQueryError::Storage)?;
        ensure_response_len(
            "Coinage.RecyclerAliasStates",
            state_keys.len(),
            state_values.len(),
        )?;

        for (candidate, state) in with_positions.into_iter().zip(state_values) {
            let state = decode_optional::<AliasState>(state, "Coinage.RecyclerAliasStates")?;
            output[candidate.output_index] = Some(VoucherOnChainInfo {
                exponent: candidate.exponent,
                ring_position: candidate.position,
                is_unloaded: state == Some(AliasState::Unloaded),
            });
        }
        Ok(output)
    }
}

pub struct RecyclerReadinessLoader {
    storage: Arc<dyn CoinageStorageQuery>,
}

/// One coherent recycler-readiness snapshot. Every revision was read from
/// the same finalized block, and callers carry that block into unload
/// preparation rather than mixing roots observed at different heads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecyclerRevisionSnapshot {
    pub block_hash: [u8; 32],
    pub revisions: HashMap<RecyclerKey, u32>,
}

impl RecyclerReadinessLoader {
    pub fn new(storage: Arc<dyn CoinageStorageQuery>) -> Self {
        Self { storage }
    }

    /// Captures the canonical finalized head and pins the complete revision
    /// batch to it. This is the production entry point for unload
    /// preparation: a missing root stays missing and must fail the caller
    /// rather than being replaced by a guessed revision.
    pub async fn fetch_finalized_revisions(
        &self,
        recycler_keys: &[RecyclerKey],
    ) -> Result<RecyclerRevisionSnapshot, CoinageQueryError> {
        let block_hash = self
            .storage
            .finalized_head()
            .await
            .map_err(CoinageQueryError::Storage)?;
        let revisions = self
            .fetch_revisions(recycler_keys, Some(block_hash))
            .await?;
        Ok(RecyclerRevisionSnapshot {
            block_hash,
            revisions,
        })
    }

    pub async fn fetch_revisions(
        &self,
        recycler_keys: &[RecyclerKey],
        at: Option<[u8; 32]>,
    ) -> Result<HashMap<RecyclerKey, u32>, CoinageQueryError> {
        if recycler_keys.is_empty() {
            return Ok(HashMap::new());
        }
        let keys = recycler_keys
            .iter()
            .map(|recycler| CoinageStorageKey::Root {
                exponent: recycler.exponent,
                ring_index: recycler.index,
            })
            .collect::<Vec<_>>();
        let values = self
            .storage
            .query(&keys, at)
            .await
            .map_err(CoinageQueryError::Storage)?;
        ensure_response_len("Members.Root", keys.len(), values.len())?;
        let mut revisions = HashMap::new();
        for (recycler, value) in recycler_keys.iter().copied().zip(values) {
            if let Some(root) = decode_optional::<RingRoot>(value, "Members.Root")? {
                revisions.insert(recycler, root.revision);
            }
        }
        Ok(revisions)
    }
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;
    use std::collections::VecDeque;
    use std::time::Duration;

    use futures::stream;

    use super::*;

    type QueryRecord = (Vec<CoinageStorageKey>, Option<[u8; 32]>);
    type QueryResponse = Result<Vec<Option<Vec<u8>>>, String>;
    type HeadBatch = Vec<Result<[u8; 32], String>>;

    #[derive(Default)]
    struct ScriptedStorage {
        queries: Mutex<Vec<QueryRecord>>,
        responses: Mutex<VecDeque<QueryResponse>>,
        heads: Mutex<VecDeque<HeadBatch>>,
    }

    impl ScriptedStorage {
        fn push_response(&self, response: Vec<Option<Vec<u8>>>) {
            self.responses.lock().push_back(Ok(response));
        }

        fn push_heads(&self, heads: impl IntoIterator<Item = u8>) {
            self.heads
                .lock()
                .push_back(heads.into_iter().map(|byte| Ok([byte; 32])).collect());
        }
    }

    #[async_trait]
    impl CoinageStorageQuery for ScriptedStorage {
        async fn query(
            &self,
            keys: &[CoinageStorageKey],
            at: Option<[u8; 32]>,
        ) -> Result<Vec<Option<Vec<u8>>>, String> {
            self.queries.lock().push((keys.to_vec(), at));
            self.responses
                .lock()
                .pop_front()
                .expect("missing scripted response")
        }

        fn finalized_heads(&self) -> BoxStream<'static, Result<[u8; 32], String>> {
            Box::pin(stream::iter(
                self.heads
                    .lock()
                    .pop_front()
                    .expect("missing scripted heads"),
            ))
        }
    }

    fn raw_coin(value: i8, age: u16) -> Vec<u8> {
        RawOnChainCoin { value, age }.encode()
    }

    fn location_voucher(derivation_index: u32, exponent: i16) -> crate::Voucher {
        crate::Voucher {
            exponent,
            derivation_index,
            allocated_at_ms: 0,
            ready_at_ms: 0,
            remote_state: crate::VoucherRemoteState::Unlocated,
            local_state: crate::VoucherLocalState::Available,
            privacy: crate::VoucherPrivacyLevel::Degraded,
        }
    }

    #[test]
    fn alias_query_rejects_out_of_range_exponent() {
        assert!(matches!(
            CoinageStorageKey::recycler_alias(500, 0, [0; 32]),
            Err(CoinageQueryError::InvalidExponent(500))
        ));
    }

    #[test]
    fn alias_state_scale_wire_boundary() {
        // AliasState wire shape: Unloaded is the bare variant 1; Locked
        // carries reason + until.
        assert_eq!(AliasState::Unloaded.encode(), vec![1]);
        let locked = AliasState::Locked(LockInfo {
            reason: LockReason::FailedDispatch(2),
            until: 99,
        });
        let encoded = locked.encode();
        assert_eq!(encoded[0], 0);
        assert_eq!(encoded.len(), 1 + 1 + 1 + 8);
        assert_eq!(AliasState::decode(&mut &encoded[..]).unwrap(), locked);
    }

    #[tokio::test]
    async fn voucher_location_batch_decodes_included_position_and_coverage() {
        let storage = ScriptedStorage::default();
        storage.push_response(vec![
            Some(
                RingPosition::Included {
                    ring_index: 7,
                    ring_page: 2,
                    ring_position: 4,
                }
                .encode(),
            ),
            Some(
                members::RingStatus {
                    total: 9,
                    included: 5,
                    immutable_since: None,
                }
                .encode(),
            ),
        ]);
        let requests = vec![
            (
                VoucherLocationRequest::Position {
                    derivation_index: 11,
                },
                CoinageStorageKey::Member {
                    exponent: 3,
                    member: [11; 32],
                },
            ),
            (
                VoucherLocationRequest::Status {
                    derivation_index: 11,
                },
                CoinageStorageKey::RingKeysStatus {
                    exponent: 3,
                    ring_index: 7,
                },
            ),
        ];

        assert_eq!(
            fetch_voucher_location_update(&storage, &requests, Some([9; 32]))
                .await
                .unwrap(),
            VoucherLocationUpdate {
                ring_positions: vec![(
                    11,
                    Some(VoucherRingPosition::Included {
                        ring_index: 7,
                        included_at: 4,
                    }),
                )],
                ring_statuses: vec![(
                    11,
                    VoucherRingStatus {
                        included_members: 5,
                    },
                )],
            }
        );
    }

    #[tokio::test]
    async fn voucher_location_reads_current_best_then_follows_live_best_changes() {
        let storage = Arc::new(ScriptedStorage::default());
        storage.push_response(vec![Some(
            RingPosition::Onboarding {
                queue_page: 0,
                queued_at: 10,
            }
            .encode(),
        )]);
        storage.push_response(vec![Some(
            RingPosition::Included {
                ring_index: 4,
                ring_page: 0,
                ring_position: 2,
            }
            .encode(),
        )]);
        storage.push_heads([0xA5]);
        let vouchers = Arc::new(crate::InMemoryVoucherRepository::with_vouchers([
            location_voucher(7, 3),
        ]));
        let subscriber = QueryVoucherLocationSubscriber::new(
            Arc::clone(&storage) as Arc<dyn CoinageStorageQuery>,
            vouchers,
            Arc::new(|_| Ok([7; 32])),
            crate::test_spawner(),
        );

        let mut updates = subscriber.subscribe(vec![7], vec![], vec![]).await.unwrap();
        assert_eq!(
            updates.recv().await.unwrap().ring_positions,
            vec![(7, Some(VoucherRingPosition::Onboarding))]
        );
        assert_eq!(
            updates.recv().await.unwrap().ring_positions,
            vec![(
                7,
                Some(VoucherRingPosition::Included {
                    ring_index: 4,
                    included_at: 2,
                }),
            )]
        );

        let queries = storage.queries.lock();
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0].1, None, "initial snapshot is current best");
        assert_eq!(
            queries[1].1,
            Some([0xA5; 32]),
            "live best change pins the follow-up batch"
        );
    }

    /// A `chainHead` follow can be rejected or terminate after the first
    /// snapshot. The location observer must still re-read current best state,
    /// otherwise a member position seen before `RingKeysStatus.included`
    /// advances remains pending forever.
    #[tokio::test]
    async fn voucher_location_periodically_reconciles_after_head_stream_ends() {
        let storage = Arc::new(ScriptedStorage::default());
        storage.push_response(vec![Some(
            RingPosition::Onboarding {
                queue_page: 0,
                queued_at: 10,
            }
            .encode(),
        )]);
        storage.push_response(vec![Some(
            RingPosition::Included {
                ring_index: 4,
                ring_page: 0,
                ring_position: 2,
            }
            .encode(),
        )]);
        storage.push_heads([]);
        let vouchers = Arc::new(crate::InMemoryVoucherRepository::with_vouchers([
            location_voucher(7, 3),
        ]));
        let subscriber = QueryVoucherLocationSubscriber::new(
            Arc::clone(&storage) as Arc<dyn CoinageStorageQuery>,
            vouchers,
            Arc::new(|_| Ok([7; 32])),
            crate::test_spawner(),
        )
        .with_observation_refresh_interval(Duration::from_millis(10));

        let mut updates = subscriber.subscribe(vec![7], vec![], vec![]).await.unwrap();
        assert_eq!(
            updates.recv().await.unwrap().ring_positions,
            vec![(7, Some(VoucherRingPosition::Onboarding))]
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), updates.recv())
                .await
                .expect("periodic current-best reconciliation timed out")
                .unwrap()
                .ring_positions,
            vec![(
                7,
                Some(VoucherRingPosition::Included {
                    ring_index: 4,
                    included_at: 2,
                }),
            )]
        );

        let queries = storage.queries.lock();
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0].1, None);
        assert_eq!(queries[1].1, None, "fallback re-reads current best");
    }

    #[tokio::test]
    async fn coin_batch_fetch_preserves_order_and_strictly_decodes() {
        let storage = Arc::new(ScriptedStorage::default());
        storage.push_response(vec![Some(raw_coin(4, 12)), None, Some(raw_coin(-2, 1))]);
        let service = CoinOnChainQueryService::new(storage.clone());
        let keys = [[1; 32], [2; 32], [3; 32]];
        assert_eq!(
            service.fetch_coins(&keys, Some([9; 32])).await.unwrap(),
            vec![
                Some(OnChainCoin {
                    exponent: 4,
                    age: 12
                }),
                None,
                Some(OnChainCoin {
                    exponent: -2,
                    age: 1
                })
            ]
        );
        let calls = storage.queries.lock();
        assert_eq!(calls.len(), 1, "one batch RPC");
        assert_eq!(calls[0].0.len(), 3);
        assert_eq!(calls[0].1, Some([9; 32]));
    }

    #[tokio::test]
    async fn sent_or_claimed_accumulates_seen_after_an_earlier_coin_is_claimed() {
        let storage = Arc::new(ScriptedStorage::default());
        storage.push_heads([1, 2, 3]);
        storage.push_response(vec![Some(raw_coin(1, 0)), None]);
        storage.push_response(vec![None, Some(raw_coin(1, 0))]);
        let service = CoinOnChainQueryService::new(storage);
        let keys = [[1; 32], [2; 32]];
        assert!(
            service
                .await_all_coins_sent_or_claimed(&keys, 3)
                .await
                .unwrap(),
            "the first key stays confirmed after it is spent; the second remains present"
        );
    }

    #[tokio::test]
    async fn sent_or_claimed_races_the_finalized_head_timeout() {
        let storage = Arc::new(ScriptedStorage::default());
        storage.push_heads([1, 2]);
        storage.push_response(vec![None]);
        storage.push_response(vec![None]);
        let service = CoinOnChainQueryService::new(storage);
        assert_eq!(
            service
                .await_all_coins_sent_or_claimed(&[[1; 32]], 2)
                .await
                .unwrap_err(),
            CoinageQueryError::Timeout { finalized_heads: 2 }
        );
    }

    #[tokio::test]
    async fn historical_probe_confirms_a_fast_claim_without_a_live_wait() {
        let storage = Arc::new(ScriptedStorage::default());
        // Current: both absent. Anchor: both were present.
        storage.push_response(vec![None, None]);
        storage.push_response(vec![Some(raw_coin(1, 0)), Some(raw_coin(2, 0))]);
        let service = CoinOnChainQueryService::new(storage.clone());
        let result = service
            .await_send_or_claimed(&[[1; 32], [2; 32]], Some([7; 32]), 10)
            .await
            .unwrap();
        assert_eq!(result, SendConfirmation::AlreadyClaimed);
        let calls = storage.queries.lock();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1, None);
        assert_eq!(calls[1].1, Some([7; 32]));
    }

    #[tokio::test]
    async fn voucher_query_is_four_stages_and_preserves_nil_slots() {
        let storage = Arc::new(ScriptedStorage::default());
        // Exponents: index 11 is not a recycler member.
        storage.push_response(vec![Some(3i8.encode()), None, Some(5i8.encode())]);
        // Positions for indices 10 and 12: index 12 is onboarding.
        storage.push_response(vec![
            Some(
                RingPosition::Included {
                    ring_index: 7,
                    ring_page: 0,
                    ring_position: 4,
                }
                .encode(),
            ),
            Some(
                RingPosition::Onboarding {
                    queue_page: 1,
                    queued_at: 2,
                }
                .encode(),
            ),
        ]);
        // An explicit `AliasState::Unloaded` row means unloaded.
        storage.push_response(vec![Some(AliasState::Unloaded.encode())]);

        let public_key_provider: Arc<KeyProvider> =
            Arc::new(|index| Ok([u8::try_from(index).unwrap(); 32]));
        let alias_provider: Arc<KeyProvider> =
            Arc::new(|index| Ok([u8::try_from(index + 1).unwrap(); 32]));
        let service =
            VoucherOnChainQueryService::new(storage.clone(), public_key_provider, alias_provider);
        let result = service
            .fetch_vouchers(&[10, 11, 12], Some([4; 32]))
            .await
            .unwrap();
        assert_eq!(
            result,
            vec![
                Some(VoucherOnChainInfo {
                    exponent: 3,
                    ring_position: RingPosition::Included {
                        ring_index: 7,
                        ring_page: 0,
                        ring_position: 4,
                    },
                    is_unloaded: true,
                }),
                None,
                None,
            ]
        );
        let calls = storage.queries.lock();
        assert_eq!(calls.len(), 3, "derivation plus three serial RPC batches");
        assert_eq!(
            calls.iter().map(|call| call.0.len()).collect::<Vec<_>>(),
            [3, 2, 1]
        );
        assert!(calls.iter().all(|call| call.1 == Some([4; 32])));
    }

    #[tokio::test]
    async fn voucher_query_short_circuits_when_no_recycler_rows_exist() {
        let storage = Arc::new(ScriptedStorage::default());
        storage.push_response(vec![None, None]);
        let service = VoucherOnChainQueryService::new(
            storage.clone(),
            Arc::new(|index| Ok([index as u8; 32])),
            Arc::new(|index| Ok([(index + 1) as u8; 32])),
        );
        assert_eq!(
            service.fetch_vouchers(&[1, 2], None).await.unwrap(),
            vec![None, None]
        );
        assert_eq!(
            storage.queries.lock().len(),
            1,
            "an empty first stage must not issue empty RPC batches"
        );
    }

    #[tokio::test]
    async fn recycler_revision_fetch_omits_missing_roots() {
        let storage = Arc::new(ScriptedStorage::default());
        let root = RingRoot {
            root: [1; 288],
            revision: 42,
            intermediate: [2; 848],
        };
        storage.push_response(vec![Some(root.encode()), None]);
        let loader = RecyclerReadinessLoader::new(storage.clone());
        let first = RecyclerKey {
            exponent: 3,
            index: 7,
        };
        let second = RecyclerKey {
            exponent: 5,
            index: 9,
        };
        let revisions = loader
            .fetch_revisions(&[first, second], Some([8; 32]))
            .await
            .unwrap();
        assert_eq!(revisions, HashMap::from([(first, 42)]));
        let calls = storage.queries.lock();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0.len(), 2);
        assert_eq!(calls[0].1, Some([8; 32]));
    }

    #[tokio::test]
    async fn recycler_readiness_uses_one_finalized_head_for_the_batch() {
        let storage = Arc::new(ScriptedStorage::default());
        storage.push_heads([0xA5]);
        storage.push_response(vec![
            Some(
                RingRoot {
                    root: [1; 288],
                    revision: 17,
                    intermediate: [2; 848],
                }
                .encode(),
            ),
            Some(
                RingRoot {
                    root: [3; 288],
                    revision: 23,
                    intermediate: [4; 848],
                }
                .encode(),
            ),
        ]);
        let loader = RecyclerReadinessLoader::new(storage.clone());
        let first = RecyclerKey {
            exponent: 2,
            index: 4,
        };
        let second = RecyclerKey {
            exponent: 7,
            index: 9,
        };

        let snapshot = loader
            .fetch_finalized_revisions(&[first, second])
            .await
            .unwrap();

        assert_eq!(snapshot.block_hash, [0xA5; 32]);
        assert_eq!(
            snapshot.revisions,
            HashMap::from([(first, 17), (second, 23)])
        );
        let calls = storage.queries.lock();
        assert_eq!(calls.len(), 1, "one Members.Root batch");
        assert_eq!(calls[0].0.len(), 2);
        assert_eq!(calls[0].1, Some([0xA5; 32]));
    }
}
