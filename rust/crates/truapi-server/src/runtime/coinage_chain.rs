// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer core/crates/brevity-ffi/src/{coinage_sender,coinage_transfer}.rs.
// Copyright the Brevity contributors. See truapi-coinage/NOTICE and LICENSE.

//! Wallet-owned Coinage effects. The caller owns user confirmation and persists
//! claim plans before submission; this edge owns chain identity, secret use,
//! mortal transaction preparation and canonical finality verification.

mod crypto;
mod rpc;
mod sender;
#[cfg(test)]
mod tests;
mod transaction;

use crate::{host_rpc_client::HostRpcClient, subscription::Spawner};
use core::time::Duration;
pub(crate) use crypto::HostVoucherCryptography;
use futures::{StreamExt, stream::BoxStream};
use parity_scale_codec::Encode;
use serde_json::json;
use std::sync::{Arc, Mutex};
use subxt_rpcs::RpcClient;
use transaction::{decode_exact, denomination_value, hex0x};
use truapi_coinage::{
    CoinKeypairFactory, CoinageStorageQuery, DenominationBreakdownContext,
    ExternalCoinTransferBackend, ExternalCoinTransferRequest, OnChainCoin, RecoveryChainProbe,
    VoucherCryptography, VoucherKeypairFactory, WalStore,
};
use truapi_platform::{JsonRpcConnection, Platform, async_trait};
use zeroize::Zeroizing;

/// One wallet/network/session's chain edge. All clones share the submission
/// gate and recovery snapshot. Serialize complete recovery sweeps, not each
/// individual probe, so a sweep cannot replace another sweep's pinned head.
#[derive(Clone)]
pub(crate) struct HostCoinageChain {
    inner: Arc<Inner>,
}

struct Inner {
    platform: Arc<dyn Platform>,
    genesis_hash: [u8; 32],
    coinage_instance_id: Option<u32>,
    entropy: Zeroizing<Vec<u8>>,
    coins: CoinKeypairFactory,
    vouchers: VoucherKeypairFactory,
    session_valid: Arc<dyn Fn() -> bool + Send + Sync>,
    spawner: Spawner,
    wal: Arc<dyn WalStore>,
    submission: futures::lock::Mutex<()>,
    recovery: Mutex<Option<([u8; 32], u64)>>,
    crypto: Arc<HostVoucherCryptography>,
    runtime: futures::lock::Mutex<Option<transaction::Snapshot>>,
    denominations: Mutex<Option<DenominationBreakdownContext>>,
}

impl HostCoinageChain {
    /// Keep root entropy Host-private and bind all secret use to the active session.
    pub(crate) fn new(
        platform: Arc<dyn Platform>,
        genesis_hash: [u8; 32],
        coinage_instance_id: Option<u32>,
        entropy: Zeroizing<Vec<u8>>,
        session_valid: Arc<dyn Fn() -> bool + Send + Sync>,
        spawner: Spawner,
        wal: Arc<dyn WalStore>,
    ) -> Self {
        let crypto = Arc::new(HostVoucherCryptography::new(session_valid.clone()));
        let coins = CoinKeypairFactory::new(&entropy);
        let vouchers = VoucherKeypairFactory::new(&entropy);
        Self {
            inner: Arc::new(Inner {
                platform,
                genesis_hash,
                coinage_instance_id,
                entropy,
                coins,
                vouchers,
                session_valid,
                spawner,
                wal,
                submission: futures::lock::Mutex::new(()),
                recovery: Mutex::new(None),
                crypto,
                runtime: futures::lock::Mutex::new(None),
                denominations: Mutex::new(None),
            }),
        }
    }

    fn ensure_session(&self) -> Result<(), String> {
        if (self.inner.session_valid)() {
            Ok(())
        } else {
            Err("Coinage signing session expired".into())
        }
    }

    async fn connect(&self) -> Result<RpcClient, String> {
        self.ensure_session()?;
        let connection: Arc<dyn JsonRpcConnection> = self
            .inner
            .platform
            .connect(self.inner.genesis_hash)
            .await
            .map_err(|_| "configured Coinage chain unavailable")?
            .into();
        if let Err(error) = self.ensure_session() {
            connection.close();
            return Err(error);
        }
        let rpc = RpcClient::new(HostRpcClient::new(connection, self.inner.spawner.clone()));
        let genesis = rpc::hash_value(&rpc::call(&rpc, "chain_getBlockHash", json!([0])).await?)?;
        if genesis != self.inner.genesis_hash {
            return Err("Coinage chain genesis does not match configured network".into());
        }
        self.ensure_session()?;
        Ok(rpc)
    }

    async fn snapshot(
        &self,
        rpc: &RpcClient,
        at: [u8; 32],
    ) -> Result<transaction::Snapshot, String> {
        let (version, number) = futures::try_join!(
            rpc::call(rpc, "state_getRuntimeVersion", json!([hex0x(&at)])),
            rpc::block_number(rpc, at),
        )?;
        let spec = u32::try_from(rpc::number(&version["specVersion"])?)
            .map_err(|_| "invalid runtime spec version")?;
        let tx = u32::try_from(rpc::number(&version["transactionVersion"])?)
            .map_err(|_| "invalid runtime transaction version")?;
        let mut snapshot = {
            let mut cached = self.inner.runtime.lock().await;
            if let Some(snapshot) = cached.as_ref().filter(|snapshot| {
                snapshot.state.spec_version == spec && snapshot.state.transaction_version == tx
            }) {
                let mut snapshot = snapshot.clone();
                snapshot.at = at;
                snapshot.number = number;
                snapshot
            } else {
                let metadata = rpc::metadata(rpc, at).await?;
                let snapshot = transaction::Snapshot::new(
                    metadata,
                    at,
                    number,
                    self.inner.genesis_hash,
                    spec,
                    tx,
                    self.inner.coinage_instance_id,
                )?;
                *cached = Some(snapshot.clone());
                snapshot
            }
        };
        if let Some(key) = snapshot.instance_asset_key()? {
            let row = rpc::query(rpc, &[key], at)
                .await?
                .pop()
                .flatten()
                .ok_or("configured Coinage asset instance does not exist")?;
            snapshot.bind_instance_asset(&row)?;
        }
        snapshot.denomination_context()?;
        self.ensure_session()?;
        Ok(snapshot)
    }

    fn bind_denominations(
        &self,
        snapshot: &transaction::Snapshot,
    ) -> Result<DenominationBreakdownContext, String> {
        let live = snapshot.denomination_context()?;
        let mut expected = self
            .inner
            .denominations
            .lock()
            .map_err(|_| "Coinage denomination snapshot unavailable")?;
        match expected.as_ref() {
            Some(expected) if expected != &live => {
                return Err(
                    "Coinage denominations changed; a new transfer review is required".into(),
                );
            }
            None => *expected = Some(live.clone()),
            _ => (),
        }
        Ok(live)
    }

    /// Read the authoritative denomination constants at one finalized head.
    pub(crate) async fn denomination_context(
        &self,
    ) -> Result<DenominationBreakdownContext, String> {
        let rpc = self.connect().await?;
        let at = rpc::finalized(&rpc).await?;
        let context = self.bind_denominations(&self.snapshot(&rpc, at).await?)?;
        self.ensure_session()?;
        Ok(context)
    }

    /// Runtime-enforced upper bound for one voucher consolidation group.
    pub(crate) async fn max_consolidation(&self) -> Result<usize, String> {
        let rpc = self.connect().await?;
        let at = rpc::finalized(&rpc).await?;
        let snapshot = self.snapshot(&rpc, at).await?;
        let value: u32 = transaction::constant(&snapshot.metadata, "Coinage", "MaxConsolidation")?;
        if value == 0 {
            return Err("Coinage consolidation bound is zero".into());
        }
        self.ensure_session()?;
        Ok(value as usize)
    }

    /// Concrete voucher public/proof effects for the engine's query providers.
    pub(crate) fn voucher_crypto(&self) -> Arc<dyn VoucherCryptography> {
        self.inner.crypto.clone()
    }

    /// Derive public voucher material without exposing the seed to a caller.
    pub(crate) fn voucher_public_key(&self, index: u32) -> Result<[u8; 32], String> {
        self.ensure_session()?;
        self.inner
            .vouchers
            .public_key(index, self.inner.crypto.as_ref())
    }

    /// Derive the protocol recycler alias, never a private voucher key.
    pub(crate) fn voucher_alias(&self, index: u32) -> Result<[u8; 32], String> {
        self.ensure_session()?;
        self.inner.vouchers.alias(
            index,
            truapi_coinage::RECYCLER_ALIAS_CONTEXT,
            self.inner.crypto.as_ref(),
        )
    }

    async fn recovery_at(&self) -> Result<[u8; 32], String> {
        self.ensure_session()?;
        self.inner
            .recovery
            .lock()
            .map_err(|_| "Coinage recovery snapshot unavailable")?
            .as_ref()
            .map(|(hash, _)| *hash)
            .ok_or_else(|| "Coinage recovery pass has no finalized snapshot".into())
    }

    async fn coins_at(
        rpc: &RpcClient,
        owners: &[[u8; 32]],
        snapshot: &transaction::Snapshot,
    ) -> Result<Vec<Option<OnChainCoin>>, String> {
        let keys = owners
            .iter()
            .map(|owner| {
                snapshot
                    .storage
                    .coinage_key(&truapi_coinage::CoinageStorageKey::Coin(*owner))
            })
            .collect::<Result<Vec<_>, _>>()?;
        rpc::query(rpc, &keys, snapshot.at)
            .await?
            .into_iter()
            .zip(owners)
            .map(|(value, owner)| {
                value
                    .map(|mut value| {
                        snapshot.storage.normalize_value(
                            &truapi_coinage::CoinageStorageKey::Coin(*owner),
                            &mut value,
                        )?;
                        let (exponent, age): (i8, u16) = decode_exact(&value)?;
                        Ok(OnChainCoin {
                            exponent: i16::from(exponent),
                            age: i16::try_from(age).map_err(|_| "Coinage age out of range")?,
                        })
                    })
                    .transpose()
            })
            .collect()
    }

    async fn nonce(
        &self,
        rpc: &RpcClient,
        account: &[u8; 32],
        _snapshot: &transaction::Snapshot,
    ) -> Result<u32, String> {
        // AccountId32 serializes a checked SS58 address. The RPC decodes the
        // account bytes independently of the address's display prefix.
        let address = subxt::utils::AccountId32(*account);
        u32::try_from(rpc::number(
            &rpc::call(rpc, "system_accountNextIndex", json!([address])).await?,
        )?)
        .map_err(|_| "Coinage account nonce exceeds u32".into())
    }
}

#[async_trait]
impl CoinageStorageQuery for HostCoinageChain {
    async fn query(
        &self,
        keys: &[truapi_coinage::CoinageStorageKey],
        at: Option<[u8; 32]>,
    ) -> Result<Vec<Option<Vec<u8>>>, String> {
        let rpc = self.connect().await?;
        let at = match at {
            Some(at) => at,
            None => rpc::finalized(&rpc).await?,
        };
        let snapshot = self.snapshot(&rpc, at).await?;
        let physical_keys = keys
            .iter()
            .map(|key| snapshot.storage.coinage_key(key))
            .collect::<Result<Vec<_>, _>>()?;
        let mut rows = rpc::query(&rpc, &physical_keys, snapshot.at).await?;
        for (key, row) in keys.iter().zip(&mut rows) {
            if let Some(row) = row {
                snapshot.storage.normalize_value(key, row)?;
            }
        }
        self.ensure_session()?;
        Ok(rows)
    }
    async fn finalized_head(&self) -> Result<[u8; 32], String> {
        let rpc = self.connect().await?;
        let head = rpc::finalized(&rpc).await?;
        self.ensure_session()?;
        Ok(head)
    }
    fn finalized_heads(&self) -> BoxStream<'static, Result<[u8; 32], String>> {
        let chain = self.clone();
        // A lazy stream owns its connection; dropping the consumer stops all work.
        // Poll the finalized hash rather than depending on author subscriptions,
        // which some configured People endpoints stop at inBlock.
        futures::stream::try_unfold(
            (chain, None::<RpcClient>, None::<[u8; 32]>),
            |(chain, client, previous)| async move {
                let client = match client {
                    Some(client) => client,
                    None => chain.connect().await?,
                };
                loop {
                    chain.ensure_session()?;
                    let head = rpc::finalized(&client).await?;
                    if previous != Some(head) {
                        return Ok(Some((head, (chain, Some(client), Some(head)))));
                    }
                    futures_timer::Delay::new(Duration::from_secs(1)).await;
                }
            },
        )
        .boxed()
    }
}

#[async_trait]
impl ExternalCoinTransferBackend for HostCoinageChain {
    async fn denomination_context(&self) -> Result<DenominationBreakdownContext, String> {
        HostCoinageChain::denomination_context(self).await
    }
    async fn fetch_coins(
        &self,
        public_keys: &[[u8; 32]],
    ) -> Result<Vec<Option<OnChainCoin>>, String> {
        let rpc = self.connect().await?;
        let at = rpc::finalized(&rpc).await?;
        let snapshot = self.snapshot(&rpc, at).await?;
        let values = Self::coins_at(&rpc, public_keys, &snapshot).await?;
        self.ensure_session()?;
        Ok(values)
    }
    async fn submit_transfer(
        &self,
        request: ExternalCoinTransferRequest,
    ) -> Result<OnChainCoin, String> {
        let _gate = self.inner.submission.lock().await;
        self.ensure_session()?;
        let rpc = self.connect().await?;
        let at = rpc::finalized(&rpc).await?;
        let snapshot = self.snapshot(&rpc, at).await?;
        let context = snapshot.denomination_context()?;
        let before =
            Self::coins_at(&rpc, &[request.source_public, request.recipient], &snapshot).await?;
        validate_live_transfer(&request, &context, &before)?;
        self.ensure_session()?;
        let secret = schnorrkel::SecretKey::from_bytes(&request.source_secret.0)
            .map_err(|_| "invalid Coinage source secret")?;
        let public = secret.to_public();
        if public.to_bytes() != request.source_public {
            return Err("Coinage source does not match supplied secret".into());
        }
        let keypair = schnorrkel::Keypair { secret, public };
        let [pallet, call_index] = snapshot
            .metadata
            .call_indices("Coinage", "transfer")
            .map_err(|_| "Coinage transfer call unavailable")?;
        let call = truapi_coinage::pallet::transfer_call(pallet, call_index, &request.recipient);
        let nonce = self.nonce(&rpc, &request.source_public, &snapshot).await?;
        self.ensure_session()?;
        let extrinsic = snapshot.signed(&keypair, &call, nonce)?;
        // The external-claim service persisted the source/destination plan before
        // entering this method. An ambiguous result leaves that plan recoverable.
        let finalized = self.broadcast(&rpc, &extrinsic, &snapshot).await?;
        let finalized_snapshot = self.snapshot(&rpc, finalized).await?;
        let after = Self::coins_at(
            &rpc,
            &[request.source_public, request.recipient],
            &finalized_snapshot,
        )
        .await?;
        validate_finalized_transfer(&request, &after)
    }
}

pub(super) fn validate_live_transfer(
    request: &ExternalCoinTransferRequest,
    context: &DenominationBreakdownContext,
    rows: &[Option<OnChainCoin>],
) -> Result<(), String> {
    if request.source_public == [0; 32]
        || request.recipient == [0; 32]
        || request.source_public == request.recipient
    {
        return Err("invalid Coinage source or destination".into());
    }
    if request.asset_unit != context.asset_unit
        || denomination_value(context, request.exponent)? != request.amount_planks
    {
        return Err("Coinage denomination changed since claim preparation".into());
    }
    if rows.len() != 2
        || rows[0].is_none_or(|coin| coin.exponent != request.exponent)
        || rows[1].is_some()
    {
        return Err("Coinage claim source or destination changed".into());
    }
    Ok(())
}

pub(super) fn validate_finalized_transfer(
    request: &ExternalCoinTransferRequest,
    rows: &[Option<OnChainCoin>],
) -> Result<OnChainCoin, String> {
    if rows.len() != 2 || rows[0].is_some() {
        return Err("finalized Coinage source was not consumed".into());
    }
    rows[1]
        .filter(|coin| coin.exponent == request.exponent)
        .ok_or_else(|| "finalized Coinage destination denomination mismatch".into())
}

#[async_trait]
impl RecoveryChainProbe for HostCoinageChain {
    async fn finalized_block(&self) -> Result<u64, String> {
        let rpc = self.connect().await?;
        let hash = rpc::finalized(&rpc).await?;
        let number = self.snapshot(&rpc, hash).await?.number;
        self.ensure_session()?;
        *self
            .inner
            .recovery
            .lock()
            .map_err(|_| "Coinage recovery snapshot unavailable")? = Some((hash, number));
        Ok(number)
    }
    async fn canonical_hash(&self, height: u64) -> Result<Option<[u8; 32]>, String> {
        let snapshot = self
            .inner
            .recovery
            .lock()
            .map_err(|_| "Coinage recovery snapshot unavailable")?
            .ok_or("Coinage recovery pass has no snapshot")?;
        if height > snapshot.1 {
            return Ok(None);
        }
        let rpc = self.connect().await?;
        let value = rpc::call(&rpc, "chain_getBlockHash", json!([height])).await?;
        if value.is_null() {
            Ok(None)
        } else {
            rpc::hash_value(&value).map(Some)
        }
    }
    async fn coins_present(&self, indices: &[u32]) -> Result<Vec<bool>, String> {
        Ok(self
            .coin_exponents(indices)
            .await?
            .into_iter()
            .map(|coin| coin.is_some())
            .collect())
    }
    async fn coin_exponents(&self, indices: &[u32]) -> Result<Vec<Option<i16>>, String> {
        let at = self.recovery_at().await?;
        self.ensure_session()?;
        let factory = &self.inner.coins;
        let owners = indices
            .iter()
            .map(|index| {
                self.ensure_session()?;
                factory.public_key(*index)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let rpc = self.connect().await?;
        let snapshot = self.snapshot(&rpc, at).await?;
        let values = Self::coins_at(&rpc, &owners, &snapshot).await?;
        self.ensure_session()?;
        Ok(values
            .into_iter()
            .map(|coin| coin.map(|coin| coin.exponent))
            .collect())
    }
    async fn vouchers_present(&self, indices: &[u32]) -> Result<Vec<bool>, String> {
        let at = self.recovery_at().await?;
        let rpc = self.connect().await?;
        let snapshot = self.snapshot(&rpc, at).await?;
        let mut result = Vec::with_capacity(indices.len());
        for index in indices {
            let member = self.voucher_public_key(*index)?;
            let query = truapi_coinage::CoinageStorageKey::Recycler(member);
            let exponent_key = snapshot.storage.coinage_key(&query)?;
            let Some(mut value) = rpc::query(&rpc, &[exponent_key], at).await?.pop().flatten()
            else {
                result.push(false);
                continue;
            };
            snapshot.storage.normalize_value(&query, &mut value)?;
            let exponent: i8 = decode_exact(&value)?;
            let collection = snapshot.storage.collection(i16::from(exponent));
            let position_key = transaction::storage_key(
                &snapshot.storage,
                "Members",
                "Members",
                &[collection.encode(), member.encode()],
            )?;
            let Some(value) = rpc::query(&rpc, &[position_key], at).await?.pop().flatten() else {
                // An existing recycler assignment with missing membership is
                // ambiguous, not proof that a spend consumed the voucher.
                return Err("Coinage voucher membership is inconsistent".into());
            };
            let position: truapi_coinage::members::RingPosition = decode_exact(&value)?;
            let truapi_coinage::members::RingPosition::Included { ring_index, .. } = position
            else {
                result.push(true);
                continue;
            };
            let alias = self.voucher_alias(*index)?;
            let key = snapshot.storage.coinage_key(
                &truapi_coinage::CoinageStorageKey::RecyclerAlias {
                    exponent,
                    ring_index,
                    alias,
                },
            )?;
            let value = rpc::query(&rpc, &[key], at).await?.pop().flatten();
            let state = value
                .as_deref()
                .map(decode_exact::<truapi_coinage::AliasState>)
                .transpose()?;
            result.push(state != Some(truapi_coinage::AliasState::Unloaded));
        }
        self.ensure_session()?;
        Ok(result)
    }
}
