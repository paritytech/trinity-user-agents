// SPDX-License-Identifier: AGPL-3.0-only
//! Session-bound effects over the wallet-owned repositories.

use super::{HostCoinageChain, HostCoinageStore, NativeChatContext, live};
use parity_scale_codec::DecodeAll;
use std::sync::Arc;
use truapi::latest::HostProductDeviceChatError as Error;
use truapi_coinage::members::RingPosition;
use truapi_coinage::*;

pub(super) struct Engine {
    pub chain: Arc<HostCoinageChain>,
    pub denominations: DenominationBreakdownContext,
    pub sender: Arc<RegularCoinTransferService>,
    pub claimer: ExternalSecretClaimService,
    pub recovery: TransferRecoveryService,
    pub query: CoinOnChainQueryService,
}

impl Engine {
    pub async fn new(
        context: &NativeChatContext,
        store: Arc<HostCoinageStore>,
    ) -> Result<Self, Error> {
        live(context)?;
        let chain = Arc::new(HostCoinageChain::new(
            context.services.platform.clone(),
            context.genesis_hash,
            context.coinage_instance_id,
            context.entropy.clone(),
            context.session_valid.clone(),
            context.services.spawner.clone(),
            store.clone(),
        ));
        let denominations = chain
            .denomination_context()
            .await
            .map_err(|_| Error::NetworkUnavailable)?;
        let max_consolidation = chain
            .max_consolidation()
            .await
            .map_err(|_| Error::NetworkUnavailable)?;
        let allocator = Arc::new(CoinAllocator::new(store.clone()));
        let sender = Arc::new(RegularCoinTransferService::new(
            &context.entropy,
            RegularCoinTransferParts {
                spawner: context.services.spawner.clone(),
                coins: store.clone(),
                vouchers: store.clone(),
                wal: store.clone(),
                allocator: allocator.clone(),
                submitter: chain.clone(),
                denominations: denominations.clone(),
                max_consolidation,
                clock: Arc::new(SystemClock),
                session_generation: 0,
                committer: Some(store.clone()),
            },
        ));
        let claimer = ExternalSecretClaimService::new(
            &context.entropy,
            allocator,
            store.clone(),
            store.clone(),
            chain.clone(),
        );
        let recovery =
            TransferRecoveryService::new(store.clone(), store.clone(), store, chain.clone());
        let query = CoinOnChainQueryService::new(chain.clone());
        Ok(Self {
            chain,
            denominations,
            sender,
            claimer,
            recovery,
            query,
        })
    }

    /// This runs only under the wallet gate, including the complete recovery
    /// probe pass. A stale session's engine is never retained for a new login.
    pub async fn synchronize(
        &self,
        context: &NativeChatContext,
        store: &Arc<HostCoinageStore>,
    ) -> Result<(), Error> {
        live(context)?;
        super::inventory::refresh_or_resume(
            store.clone(),
            self.chain.clone(),
            context.entropy.clone(),
            self.chain.voucher_crypto(),
            SystemClock.now_ms(),
        )
        .await
        .map_err(|_| Error::NetworkUnavailable)?;
        self.recovery
            .recover()
            .await
            .map_err(|_| Error::NetworkUnavailable)?;
        let at = self
            .chain
            .finalized_head()
            .await
            .map_err(|_| Error::NetworkUnavailable)?;
        let key_factory = CoinKeypairFactory::new(&context.entropy);
        let coins = CoinRepository::list(store.as_ref())
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        for chunk in coins.chunks(500) {
            live(context)?;
            let publics = chunk
                .iter()
                .map(|coin| key_factory.public_key(coin.derivation_index))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| Error::NotConnected)?;
            let rows = self
                .query
                .fetch_coins(&publics, Some(at))
                .await
                .map_err(|_| Error::NetworkUnavailable)?;
            for (coin, row) in chunk.iter().zip(rows) {
                // Never reclaim an accepted but not-yet-claimed outgoing secret.
                if coin.state != CoinState::Available {
                    continue;
                }
                let mut updated = coin.clone();
                match row {
                    Some(row) if row.exponent == coin.exponent => updated.age = Some(row.age),
                    Some(_) => return Err(Error::NetworkUnavailable),
                    None => updated.state = CoinState::Spent,
                }
                if updated != *coin {
                    CoinRepository::upsert(store.as_ref(), &updated)
                        .await
                        .map_err(|_| Error::StorageUnavailable)?;
                }
            }
        }
        let public_chain = self.chain.clone();
        let alias_chain = self.chain.clone();
        let vouchers_query = VoucherOnChainQueryService::new(
            self.chain.clone(),
            Arc::new(move |index| public_chain.voucher_public_key(index)),
            Arc::new(move |index| alias_chain.voucher_alias(index)),
        );
        let vouchers = VoucherRepository::list(store.as_ref())
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        for chunk in vouchers.chunks(500) {
            let ids: Vec<_> = chunk
                .iter()
                .map(|voucher| voucher.derivation_index)
                .collect();
            let rows = vouchers_query
                .fetch_vouchers(&ids, Some(at))
                .await
                .map_err(|_| Error::NetworkUnavailable)?;
            let rings: Vec<_> = rows
                .iter()
                .filter_map(|row| match row {
                    Some(VoucherOnChainInfo {
                        exponent,
                        ring_position: RingPosition::Included { ring_index, .. },
                        is_unloaded: false,
                    }) => Some((*exponent, *ring_index)),
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            let keys: Vec<_> = rings
                .iter()
                .map(
                    |(exponent, index)| truapi_coinage::CoinageStorageKey::RingKeysStatus {
                        exponent: *exponent,
                        ring_index: *index,
                    },
                )
                .collect();
            let values = if keys.is_empty() {
                Vec::new()
            } else {
                self.chain
                    .query(&keys, Some(at))
                    .await
                    .map_err(|_| Error::NetworkUnavailable)?
            };
            if values.len() != rings.len() {
                return Err(Error::NetworkUnavailable);
            }
            let statuses = rings
                .into_iter()
                .zip(values)
                .map(|(ring, bytes)| {
                    let status = bytes
                        .map(|bytes| {
                            members::RingStatus::decode_all(&mut &bytes[..])
                                .map_err(|_| Error::NetworkUnavailable)
                        })
                        .transpose()?;
                    Ok((ring, status))
                })
                .collect::<Result<std::collections::BTreeMap<_, _>, Error>>()?;
            for (voucher, row) in chunk.iter().zip(rows) {
                if voucher.local_state != VoucherLocalState::Available {
                    continue;
                }
                let mut updated = voucher.clone();
                updated.remote_state = match row {
                    Some(row) if row.exponent != voucher.exponent => {
                        return Err(Error::NetworkUnavailable);
                    }
                    Some(row) if row.is_unloaded => VoucherRemoteState::Unloaded,
                    Some(VoucherOnChainInfo {
                        exponent,
                        ring_position:
                            RingPosition::Included {
                                ring_index,
                                ring_position: included_at,
                                ..
                            },
                        ..
                    }) => {
                        match statuses
                            .get(&(exponent, ring_index))
                            .and_then(|status| *status)
                        {
                            Some(status) if status.included > included_at => {
                                updated.privacy = if ring_readiness_upgraded(status.included) {
                                    VoucherPrivacyLevel::Full
                                } else {
                                    VoucherPrivacyLevel::Degraded
                                };
                                VoucherRemoteState::InRecycler {
                                    recycler_index: ring_index,
                                }
                            }
                            _ => VoucherRemoteState::Onboarding,
                        }
                    }
                    Some(_) => VoucherRemoteState::Onboarding,
                    None => VoucherRemoteState::Unlocated,
                };
                if updated != *voucher {
                    VoucherRepository::upsert(store.as_ref(), &updated)
                        .await
                        .map_err(|_| Error::StorageUnavailable)?;
                }
            }
        }
        live(context)
    }

    pub fn partial_cents(&self, amount: u128) -> Result<u64, Error> {
        let unit = self.denominations.asset_unit;
        if unit == 0 {
            return Err(Error::NetworkUnavailable);
        }
        // Public cents cannot represent a fractional-cent cleared prefix.
        // Report only the whole cents proven; never round settlement upward.
        u64::try_from(amount / unit).map_err(|_| Error::InvalidRequest)
    }

    pub fn debit_cents(&self, amount: u128) -> Result<u64, Error> {
        let unit = self.denominations.asset_unit;
        if unit == 0 {
            return Err(Error::NetworkUnavailable);
        }
        u64::try_from(amount.div_ceil(unit)).map_err(|_| Error::InvalidRequest)
    }
}
