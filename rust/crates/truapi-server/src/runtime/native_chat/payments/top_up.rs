// SPDX-License-Identifier: AGPL-3.0-only
//! Generic supplied-secret deposits share the native purse's custody and claim engine.

#[cfg(test)]
mod tests;

use super::*;
use std::collections::BTreeSet;
use truapi::v01::{
    HostPaymentTopUpError as TopUpError, HostPaymentTopUpRequest, PaymentTopUpSource,
};

pub(super) const OPERATION_MAGIC: &[u8; 4] = b"HCT1";
const MAX_SOURCES: usize = 4096;

// No Debug: the final field contains bearer secrets, including while awaiting funding.
#[derive(Encode, Decode)]
pub(super) struct CoinTopUp {
    product_id: String,
    source_public: Vec<[u8; 32]>,
    denominations: Option<(u128, i16, i16, u8)>,
    memo: Vec<u8>,
}

impl Drop for CoinTopUp {
    fn drop(&mut self) {
        self.memo.zeroize();
    }
}

impl CoinTopUp {
    fn memo(&self) -> Result<TransferMemo, Error> {
        TransferMemo::from_scale_encoded(&self.memo).map_err(|_| Error::StorageUnavailable)
    }
}

impl WalletCoinage {
    pub(crate) async fn top_up(
        self: &Arc<Self>,
        context: &NativeChatContext,
        product_id: &str,
        request: HostPaymentTopUpRequest,
    ) -> Result<(), TopUpError> {
        // Wrap all incoming secret material before any early return or suspension.
        let keys = match request.source {
            PaymentTopUpSource::Coins {
                sr25519_secret_keys,
            } => Zeroizing::new(sr25519_secret_keys),
            PaymentTopUpSource::PrivateKey {
                mut sr25519_secret_key,
            } => {
                sr25519_secret_key.zeroize();
                return Err(TopUpError::InvalidSource);
            }
            PaymentTopUpSource::ProductAccount { .. } => return Err(TopUpError::InvalidSource),
        };
        if request
            .into
            .is_some_and(|purse| purse != truapi::v01::MAIN_PURSE)
        {
            return Err(TopUpError::Unknown {
                reason: "unsupported top-up purse".into(),
            });
        }
        if product_id.is_empty()
            || product_id.len() > 1024
            || keys.is_empty()
            || keys.len() > MAX_SOURCES
        {
            return Err(TopUpError::InvalidSource);
        }
        let memo = TransferMemo {
            entries: keys.iter().copied().map(MemoEntry).collect(),
            total_value: 0,
        };
        drop(keys);
        let public = memo_public(&memo).map_err(top_up_error)?;
        self.check(context).map_err(top_up_error)?;
        let wallet = self.clone();
        let context = context.clone();
        let product_id = product_id.to_owned();
        let (tx, rx) = futures::channel::oneshot::channel();
        (context.services.spawner.clone())(Box::pin(async move {
            let result = wallet
                .top_up_once(&context, &product_id, memo, public)
                .await;
            let _ = tx.send(result);
        }));
        let credited = rx
            .await
            .map_err(|_| top_up_error(Error::StorageUnavailable))?
            .map_err(top_up_error)?;
        if request.amount > credited {
            Err(TopUpError::PartialPayment { credited })
        } else {
            Ok(())
        }
    }

    async fn top_up_once(
        &self,
        context: &NativeChatContext,
        product_id: &str,
        memo: TransferMemo,
        public: Vec<[u8; 32]>,
    ) -> Result<u128, Error> {
        let _gate = self.gate.lock().await;
        self.check(context)?;
        self.store
            .reauthenticate()
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let mut operation = self.admit_top_up(context, product_id, memo, public).await?;
        if let Some(credited) = self.top_up_receipt(&operation).await? {
            self.check(context)?;
            return Ok(credited);
        }
        let engine = Engine::new(context, self.store.clone()).await?;
        engine.synchronize(context, &self.store).await?;
        self.resume_top_up(context, &mut operation, &engine).await
    }

    // Source identity is wallet-wide, independent of caller order and requested
    // minimum. A changed amount reads the same receipt, never allocates again.
    async fn admit_top_up(
        &self,
        context: &NativeChatContext,
        product_id: &str,
        mut memo: TransferMemo,
        public: Vec<[u8; 32]>,
    ) -> Result<CoinTopUp, Error> {
        let sources: BTreeSet<_> = public.iter().copied().collect();
        let fingerprint = source_fingerprint(&public);
        for operation in self.top_ups().await? {
            if !operation
                .source_public
                .iter()
                .any(|key| sources.contains(key))
            {
                continue;
            }
            if source_fingerprint(&operation.source_public) != fingerprint
                || operation.product_id != product_id
            {
                return Err(Error::OperationConflict);
            }
            return Ok(operation);
        }
        let mut denominations = None;
        // A native receive may already own this memo and its destination plan.
        // Retain its exact order/value/key, including a partially claimed plan.
        for operation in self.operations().await? {
            if operation.card.direction != Direction::Incoming
                || !operation
                    .source_public
                    .iter()
                    .any(|key| sources.contains(key))
            {
                continue;
            }
            if operation.source_fingerprint != Some(fingerprint)
                || operation.product_id != product_id
            {
                return Err(Error::OperationConflict);
            }
            let canonical = TransferMemo::from_scale_encoded(&operation.memo)
                .map_err(|_| Error::StorageUnavailable)?;
            if operation.memo_key != Some(canonical.identifier())
                || memo_public(&canonical)? != operation.source_public
                || operation.denominations.is_none()
                || operation
                    .denominations
                    .and_then(|d| d.0.checked_mul(u128::from(operation.card.amount_cents)))
                    != Some(canonical.total_value)
            {
                return Err(Error::StorageUnavailable);
            }
            memo = canonical;
            denominations = operation.denominations;
            break;
        }
        let operation = CoinTopUp {
            product_id: product_id.to_owned(),
            source_public: memo_public(&memo)?,
            denominations,
            memo: memo.scale_encoded(),
        };
        self.check(context)?;
        self.save_top_up(&operation).await?;
        Ok(operation)
    }

    async fn resume_top_up(
        &self,
        context: &NativeChatContext,
        operation: &mut CoinTopUp,
        engine: &Engine,
    ) -> Result<u128, Error> {
        self.check(context)?;
        if let Some(denominations) = operation.denominations {
            if denominations != binding(engine) {
                return Err(Error::NetworkUnavailable);
            }
        } else {
            let at = engine
                .chain
                .finalized_head()
                .await
                .map_err(|_| Error::NetworkUnavailable)?;
            let rows = engine
                .query
                .fetch_coins(&operation.source_public, Some(at))
                .await
                .map_err(|_| Error::NetworkUnavailable)?;
            if rows.len() != operation.source_public.len() {
                return Err(Error::NetworkUnavailable);
            }
            let mut total = 0u128;
            for row in rows {
                // An absent source may be an accepted native send still awaiting
                // funding. Never discard custody or call an ambiguous zero settled.
                let row = row.ok_or(Error::NetworkUnavailable)?;
                let d = &engine.denominations;
                if row.exponent < d.min_exponent || row.exponent > d.max_exponent {
                    return Err(Error::NetworkUnavailable);
                }
                let value = d.value_in_planks(row.exponent);
                if value == 0 || value == u128::MAX {
                    return Err(Error::NetworkUnavailable);
                }
                total = total.checked_add(value).ok_or(Error::InvalidRequest)?;
            }
            let mut memo = operation.memo()?;
            memo.total_value = total;
            operation.memo.zeroize();
            operation.memo = memo.scale_encoded();
            operation.denominations = Some(binding(engine));
            self.check(context)?;
            // Commit the canonical value/order before allocating any destination.
            self.save_top_up(operation).await?;
        }
        self.claim_top_up(context, operation, &engine.claimer).await
    }

    async fn claim_top_up(
        &self,
        context: &NativeChatContext,
        operation: &CoinTopUp,
        claimer: &dyn ExternalMemoClaiming,
    ) -> Result<u128, Error> {
        self.check(context)?;
        let memo = operation.memo()?;
        let message_id = truapi_coinage::external_claim_message_id(&memo.identifier());
        // The same engine durably plans before its first supplied-key transfer;
        // it never selects host inventory or invokes outgoing preparation.
        let _outcome = claimer.claim_external_memo(memo, message_id).await;
        let receipt = self.top_up_receipt(operation).await?;
        self.check(context)?;
        // Error can follow a committed final receipt. Conversely, an Ok transport
        // result alone is never settlement evidence. Keep all unresolved work.
        receipt.ok_or(Error::NetworkUnavailable)
    }

    async fn top_up_receipt(&self, operation: &CoinTopUp) -> Result<Option<u128>, Error> {
        let memo = operation.memo()?;
        if operation.denominations.is_none() {
            return Ok(None);
        }
        let key = memo.identifier();
        let Some(plan) = self
            .store
            .plan(&key)
            .await
            .map_err(|_| Error::StorageUnavailable)?
        else {
            return Ok(None);
        };
        if plan.memo_key != key
            || plan.total_value != memo.total_value
            || plan.entries.len() != memo.entries.len()
        {
            return Err(Error::StorageUnavailable);
        }
        if plan.status != ClaimPlanStatus::Finished {
            return Ok(None);
        }
        if plan.claimed_amount != Some(memo.total_value) || memo.total_value == 0 {
            return Err(Error::StorageUnavailable);
        }
        Ok(plan.claimed_amount)
    }

    /// Wallet/session recovery, independent of any product's Chat lifecycle.
    pub(crate) async fn reconcile_top_ups(
        self: &Arc<Self>,
        context: &NativeChatContext,
    ) -> Result<(), Error> {
        self.check(context)?;
        let wallet = self.clone();
        let context = context.clone();
        let (tx, rx) = futures::channel::oneshot::channel();
        (context.services.spawner.clone())(Box::pin(async move {
            let _ = tx.send(wallet.reconcile_top_ups_once(&context).await);
        }));
        rx.await.map_err(|_| Error::StorageUnavailable)?
    }

    async fn reconcile_top_ups_once(&self, context: &NativeChatContext) -> Result<(), Error> {
        let _gate = self.gate.lock().await;
        self.check(context)?;
        self.store
            .reauthenticate()
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let mut pending = Vec::new();
        for operation in self.top_ups().await? {
            if self.top_up_receipt(&operation).await?.is_none() {
                pending.push(operation);
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        let engine = Engine::new(context, self.store.clone()).await?;
        engine.synchronize(context, &self.store).await?;
        let mut failure = None;
        for mut operation in pending {
            self.check(context)?;
            // One missing/unfunded source set must not starve other deposits.
            if let Err(error) = self.resume_top_up(context, &mut operation, &engine).await {
                failure = Some(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    fn top_up_id(&self, public: &[[u8; 32]]) -> [u8; 32] {
        hash(
            &(
                b"truapi/main-purse/coins-top-up/v1".as_slice(),
                self.root_public_key,
                self.genesis_hash,
                source_fingerprint(public),
            )
                .encode(),
        )
    }

    async fn save_top_up(&self, operation: &CoinTopUp) -> Result<(), Error> {
        let mut bytes = OPERATION_MAGIC.to_vec();
        operation.encode_to(&mut bytes);
        self.store
            .write_operation(self.top_up_id(&operation.source_public), bytes)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub(super) async fn top_ups(&self) -> Result<Vec<CoinTopUp>, Error> {
        let mut operations = Vec::new();
        for (id, bytes) in self
            .store
            .list_operations()
            .await
            .map_err(|_| Error::StorageUnavailable)?
        {
            let Some(mut input) = bytes.strip_prefix(OPERATION_MAGIC) else {
                continue;
            };
            let operation = CoinTopUp::decode(&mut input).map_err(|_| Error::StorageUnavailable)?;
            if !input.is_empty()
                || operation.product_id.is_empty()
                || operation.product_id.len() > 1024
                || operation.source_public.is_empty()
                || operation.source_public.len() > MAX_SOURCES
                || self.top_up_id(&operation.source_public) != id
            {
                return Err(Error::StorageUnavailable);
            }
            let memo = operation.memo()?;
            if memo_public(&memo).map_err(|_| Error::StorageUnavailable)? != operation.source_public
                || operation.denominations.is_some() != (memo.total_value != 0)
            {
                return Err(Error::StorageUnavailable);
            }
            operations.push(operation);
        }
        Ok(operations)
    }
}

fn top_up_error(error: Error) -> TopUpError {
    match error {
        Error::InvalidRequest | Error::OperationConflict => TopUpError::InvalidSource,
        Error::NotConnected => TopUpError::Unknown {
            reason: "top-up session is not active".into(),
        },
        Error::StorageUnavailable => TopUpError::Unknown {
            reason: "top-up custody storage is unavailable; retry the same coins".into(),
        },
        _ => TopUpError::Unknown {
            reason: "top-up is unresolved; retry the same coins".into(),
        },
    }
}
