// SPDX-License-Identifier: AGPL-3.0-only
//! Wallet-owned native Coinage payments. Products receive only public cards;
//! entropy, memos, approval bindings and recovery evidence stay in the Host.

mod engine;
mod inventory;
#[cfg(test)]
mod tests;

use super::NativeChatContext;
use crate::runtime::{
    chat_device, coinage_chain::HostCoinageChain, coinage_store::HostCoinageStore,
};
use engine::Engine;
use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};
use std::sync::Arc;
use truapi::latest::{
    self, HostNativeChatPayment, HostNativeChatPaymentDirection as Direction,
    HostNativeChatPaymentFailure as Failure, HostNativeChatPaymentState as State,
    HostProductDeviceChatError as Error,
};
use truapi_coinage::{
    ClaimPlanStatus, ClaimPlanStore, Clock, CoinageStorageQuery, ExternalMemoClaiming, MemoEntry,
    SystemClock, TransferMemo, TransferPreviewChoice, WalStore,
};
use truapi_platform::{MainPurseChatPaymentReview, UserConfirmationReview, async_trait};
use zeroize::{Zeroize, Zeroizing};

const OPERATION_MAGIC: &[u8; 4] = b"HCP1";

#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub(crate) struct PaymentIntent {
    pub product_id: String,
    pub peer_identity: [u8; 32],
    pub recipient_username: Option<String>,
    pub request_id: String,
    pub amount_cents: u64,
}

#[async_trait]
pub(crate) trait PaymentTransport: Send + Sync {
    /// Idempotently take durable custody of this exact operation's encrypted,
    /// signed native message. An error certifies NO durable acceptance; uncertain
    /// storage/submission MUST return success and retain the message for retry.
    async fn accept(&self, payment: &HostNativeChatPayment, memo: TransferMemo) -> Result<(), ()>;
}

#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum Phase {
    Review,
    Approved,
    HandoffReady,
    Accepted,
    Denied,
    Incoming,
}

#[derive(Clone, Encode, Decode)]
struct Operation {
    product_id: String,
    recipient_username: Option<String>,
    genesis_hash: [u8; 32],
    card: HostNativeChatPayment,
    phase: Phase,
    max_debit_cents: u64,
    denominations: Option<(u128, i16, i16, u8)>,
    source_public: Vec<[u8; 32]>,
    source_exponents: Vec<Option<i16>>,
    source_seen: Vec<bool>,
    source_cleared: Vec<bool>,
    memo_key: Option<[u8; 32]>,
    source_fingerprint: Option<[u8; 32]>,
    detection_anchor: Option<[u8; 32]>,
    delivered: bool,
    // Kept last so partial SCALE decoding never abandons a decoded secret field.
    memo: Vec<u8>,
}

impl Drop for Operation {
    fn drop(&mut self) {
        self.memo.zeroize();
    }
}

impl Operation {
    fn matches(&self, intent: &PaymentIntent, genesis: [u8; 32]) -> bool {
        self.product_id == intent.product_id
            && self.card.request_id == intent.request_id
            && self.card.peer_identity == intent.peer_identity
            && self.card.amount_cents == intent.amount_cents
            && self.genesis_hash == genesis
            && self.card.direction == Direction::Outgoing
    }

    fn review(&self, coinage_instance_id: Option<u32>) -> MainPurseChatPaymentReview {
        MainPurseChatPaymentReview {
            calling_product_id: self.product_id.clone(),
            recipient_identity: self.card.peer_identity,
            recipient_username: self.recipient_username.clone(),
            amount_cents: self.card.amount_cents,
            max_debit_cents: self.max_debit_cents,
            genesis_hash: self.genesis_hash,
            coinage_instance_id,
            operation_id: self.card.operation_id,
        }
    }
}

/// One live wallet/network owner shared across products. Session cache eviction
/// releases it once owned work finishes; no session-bound signer is retained.
pub(crate) struct WalletCoinage {
    root_public_key: [u8; 32],
    genesis_hash: [u8; 32],
    coinage_instance_id: Option<u32>,
    store: Arc<HostCoinageStore>,
    gate: Mutex<()>,
}

impl WalletCoinage {
    pub(crate) async fn open(
        context: &NativeChatContext,
    ) -> Result<Arc<Self>, latest::HostProductDeviceChatError> {
        live(context)?;
        let key = storage_key(context);
        let store = HostCoinageStore::open(
            context.services.platform.clone(),
            context.session.public_key,
            context.genesis_hash,
            key,
            context.services.spawner.clone(),
        )
        .await
        .map_err(|_| Error::StorageUnavailable)?;
        live(context)?;
        store
            .bind_asset_instance(context.coinage_instance_id)
            .await
            .map_err(|error| {
                if error == crate::runtime::coinage_store::StoreError::Conflict {
                    Error::OperationConflict
                } else {
                    Error::StorageUnavailable
                }
            })?;
        // Opening storage is not evidence of an empty native purse. Every send
        // must finish the native persisted recovery scan before selection.
        Ok(Arc::new(Self {
            root_public_key: context.session.public_key,
            genesis_hash: context.genesis_hash,
            coinage_instance_id: context.coinage_instance_id,
            store,
            gate: Mutex::new(()),
        }))
    }

    pub(crate) async fn send(
        self: &Arc<Self>,
        context: &NativeChatContext,
        intent: PaymentIntent,
        transport: Arc<dyn PaymentTransport>,
    ) -> Result<HostNativeChatPayment, Error> {
        self.check(context)?;
        validate_intent(&intent)?;
        let wallet = self.clone();
        let context = context.clone();
        let (tx, rx) = futures::channel::oneshot::channel();
        (context.services.spawner.clone())(Box::pin(async move {
            let result = wallet.send_once(&context, intent, transport).await;
            let _ = tx.send(result);
        }));
        rx.await.map_err(|_| Error::StorageUnavailable)?
    }

    async fn send_once(
        self: &Arc<Self>,
        context: &NativeChatContext,
        intent: PaymentIntent,
        transport: Arc<dyn PaymentTransport>,
    ) -> Result<HostNativeChatPayment, Error> {
        let _gate = self.gate.lock().await;
        self.check(context)?;
        self.store
            .reauthenticate()
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let id = operation_id(
            self.root_public_key,
            self.genesis_hash,
            &intent.product_id,
            &intent.request_id,
        );
        let mut operation = match self.load(id).await? {
            Some(operation) => {
                if !operation.matches(&intent, self.genesis_hash) {
                    return Err(Error::OperationConflict);
                }
                if operation.phase == Phase::Denied {
                    return Err(Error::UserRejected);
                }
                if matches!(operation.card.state, State::Cleared | State::Failed { .. }) {
                    return Ok(operation.card.clone());
                }
                operation
            }
            None => {
                let operation = new_outgoing(id, self.genesis_hash, intent);
                self.save(&operation).await?;
                operation
            }
        };
        let journal_id = hex::encode(id);
        let journals = self
            .store
            .load_operation(&journal_id)
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        if journals
            .iter()
            .any(|entry| entry.operation == truapi_coinage::WalOperation::TransferRejected)
        {
            operation.card.state = State::Failed {
                reason: Failure::Cancelled,
            };
            self.save(&operation).await?;
            return Ok(operation.card.clone());
        }
        // Completed preparation only needs ciphertext repair. An accepted memo
        // can precede split/unload funding, so unfinished preparation must still
        // reach the exact-plan resume path below after renewed review.
        if operation.phase == Phase::Accepted
            || journals.iter().any(|entry| {
                matches!(
                    entry.operation,
                    truapi_coinage::WalOperation::TransferAccepted
                        | truapi_coinage::WalOperation::TransferCompleted
                )
            })
        {
            if operation.phase != Phase::Accepted {
                self.stored_memo(&operation)?;
                operation.phase = Phase::Accepted;
                self.save(&operation).await?;
            }
            if !operation.delivered && operation.card.state != State::Delivered {
                self.replay_handoff(context, &operation, transport.clone())
                    .await?;
            }
            if journals
                .iter()
                .any(|entry| entry.operation == truapi_coinage::WalOperation::TransferCompleted)
            {
                return Ok(operation.card.clone());
            }
        }
        let engine = Engine::new(context, self.store.clone()).await?;
        engine.synchronize(context, &self.store).await?;
        let journals = engine
            .sender
            .operation_wal(&journal_id)
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let denomination_binding = binding(&engine);
        let preview = if journals.is_empty() {
            let amount = engine
                .denominations
                .cash_cents_to_planks(u128::from(operation.card.amount_cents))
                .ok_or(Error::InvalidRequest)?;
            let preview = engine
                .sender
                .preview(amount)
                .await
                .map_err(|error| match error {
                    truapi_coinage::RegularTransferError::Selection(_) => {
                        Error::InsufficientBalance
                    }
                    _ => Error::NetworkUnavailable,
                })?;
            operation.max_debit_cents = engine.debit_cents(preview.max_debit_amount)?;
            operation.denominations = Some(denomination_binding);
            Some(preview)
        } else {
            if operation.denominations != Some(denomination_binding) {
                return Err(Error::NetworkUnavailable);
            }
            None
        };
        self.save(&operation).await?;
        self.check(context)?;
        let approved = review_operation(
            context.services.platform.as_ref(),
            context.session_valid.as_ref(),
            &operation,
            self.coinage_instance_id,
        )
        .await?;
        self.check(context)?;
        if !approved {
            // A resumed, already accepted operation cannot be cancelled or have
            // reservations released: denial means no *new* effects this time.
            if journals.is_empty() && operation.memo_key.is_none() {
                operation.phase = Phase::Denied;
                operation.card.state = State::Failed {
                    reason: Failure::Cancelled,
                };
                self.save(&operation).await?;
            }
            return Err(Error::UserRejected);
        }
        if operation.phase == Phase::Review {
            operation.phase = Phase::Approved;
        }
        operation.card.state = State::Preparing;
        self.save(&operation).await?;
        self.check(context)?;
        let wallet = self.clone();
        let guarded_context = context.clone();
        let handoff = move |memo: TransferMemo| async move {
            wallet.handoff(&guarded_context, id, memo, transport).await
        };
        let result = if let Some(preview) = preview {
            engine
                .sender
                .confirm_operation(
                    &journal_id,
                    &preview.preview_id,
                    TransferPreviewChoice::Full,
                    handoff,
                )
                .await
        } else {
            engine.sender.resume_operation(&journal_id, handoff).await
        };
        let mut operation = self.load(id).await?.ok_or(Error::StorageUnavailable)?;
        if result.is_err() {
            operation.card.state = State::Recovering;
        }
        self.observe_outgoing(&engine, &mut operation).await?;
        self.save(&operation).await?;
        Ok(operation.card.clone())
    }

    async fn handoff(
        &self,
        context: &NativeChatContext,
        id: [u8; 32],
        memo: TransferMemo,
        transport: Arc<dyn PaymentTransport>,
    ) -> Result<(), ()> {
        self.check(context).map_err(|_| ())?;
        let mut operation = self.load(id).await.map_err(|_| ())?.ok_or(())?;
        let amount = operation
            .denominations
            .ok_or(())?
            .0
            .checked_mul(u128::from(operation.card.amount_cents))
            .ok_or(())?;
        if memo.total_value != amount {
            return Err(());
        }
        // Resume after transport accepted but before the journal was advanced.
        if operation.phase == Phase::Accepted {
            return Ok(());
        }
        let public = memo_public(&memo).map_err(|_| ())?;
        if operation
            .memo_key
            .is_some_and(|key| key != memo.identifier())
        {
            return Err(());
        }
        if operation.phase == Phase::HandoffReady {
            // The previous transport write may have committed. Replaying its
            // immutable id resolves that ambiguity; a now-absent source is not
            // grounds for asserting that the peer never accepted the secret.
            transport.accept(&operation.card, memo).await?;
            operation.phase = Phase::Accepted;
            let _ = self.save(&operation).await;
            return Ok(());
        }
        operation.memo.zeroize();
        operation.memo = memo.scale_encoded();
        operation.memo_key = Some(memo.identifier());
        operation.source_fingerprint = Some(source_fingerprint(&public));
        operation.source_public = public;
        operation
            .source_exponents
            .resize(operation.source_public.len(), None);
        operation
            .source_seen
            .resize(operation.source_public.len(), false);
        operation
            .source_cleared
            .resize(operation.source_public.len(), false);
        let chain = Arc::new(HostCoinageChain::new(
            context.services.platform.clone(),
            self.genesis_hash,
            context.coinage_instance_id,
            context.entropy.clone(),
            context.session_valid.clone(),
            context.services.spawner.clone(),
            self.store.clone(),
        ));
        let at = chain.finalized_head().await.map_err(|_| ())?;
        let query = truapi_coinage::CoinOnChainQueryService::new(chain);
        let rows = query
            .fetch_coins(&operation.source_public, Some(at))
            .await
            .map_err(|_| ())?;
        let journals = self
            .store
            .load_operation(&hex::encode(id))
            .await
            .map_err(|_| ())?;
        let parent = journals
            .iter()
            .find(|entry| entry.operation.is_transfer_receipt())
            .ok_or(())?;
        let key_factory = truapi_coinage::CoinKeypairFactory::new(&context.entropy);
        let references = parent
            .payload
            .output_coins
            .iter()
            .map(|coin| {
                key_factory
                    .public_key(coin.derivation_index)
                    .map(|public| (public, coin))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ())?;
        for (i, row) in rows.into_iter().enumerate() {
            let reference = references
                .iter()
                .find(|(public, _)| *public == operation.source_public[i])
                .ok_or(())?
                .1;
            let pass_through = journals
                .iter()
                .filter(|entry| entry.operation == truapi_coinage::WalOperation::SecretHandoff)
                .any(|entry| entry.payload.input_coins.contains(reference));
            operation.source_exponents[i] = Some(reference.exponent);
            match row {
                Some(row) if row.exponent == reference.exponent => operation.source_seen[i] = true,
                None if !pass_through => (),
                _ => return Err(()),
            }
        }
        operation.detection_anchor = Some(at);
        operation.phase = Phase::HandoffReady;
        operation.card.state = State::Delivering;
        // Our local secret record alone is not recipient acceptance. A failure
        // here may safely reject; poisoned WAL still guards reservations.
        self.save(&operation).await.map_err(|_| ())?;
        self.check(context).map_err(|_| ())?;
        transport.accept(&operation.card, memo).await?;
        // Acceptance is irreversible even if our final write is ambiguous.
        operation.phase = Phase::Accepted;
        let _ = self.save(&operation).await;
        Ok(())
    }

    pub(crate) async fn receive_batch(
        self: &Arc<Self>,
        context: &NativeChatContext,
        product_id: &str,
        peer_identity: [u8; 32],
        request_id: &str,
        memos: Vec<chat_device::PaymentMemo>,
    ) -> Result<Vec<HostNativeChatPayment>, Error> {
        self.check(context)?;
        let wallet = self.clone();
        let context = context.clone();
        let product_id = product_id.to_owned();
        let request_id = request_id.to_owned();
        let (tx, rx) = futures::channel::oneshot::channel();
        (context.services.spawner.clone())(Box::pin(async move {
            let result = wallet
                .receive_batch_once(&context, product_id, peer_identity, request_id, memos)
                .await;
            let accepted = result.is_ok();
            let _ = tx.send(result);
            // Every secret and complete claim plan is durable before ACK.
            // Claiming survives a disconnected receive caller.
            if accepted {
                let _ = wallet.reconcile_once(&context).await;
            }
        }));
        rx.await.map_err(|_| Error::StorageUnavailable)?
    }

    async fn receive_batch_once(
        &self,
        context: &NativeChatContext,
        product_id: String,
        peer_identity: [u8; 32],
        request_id: String,
        incoming: Vec<chat_device::PaymentMemo>,
    ) -> Result<Vec<HostNativeChatPayment>, Error> {
        let _gate = self.gate.lock().await;
        self.check(context)?;
        self.store
            .reauthenticate()
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        if product_id.is_empty()
            || product_id.len() > 1024
            || request_id.is_empty()
            || request_id.len() > 1024
        {
            return Err(Error::InvalidRequest);
        }
        struct Admission {
            message_id: String,
            timestamp: u64,
            memo: TransferMemo,
            public: Vec<[u8; 32]>,
            fingerprint: [u8; 32],
            existing: Option<usize>,
            plan_ready: bool,
        }
        let existing = self.operations().await?;
        let mut existing_sources = std::collections::BTreeMap::new();
        let mut existing_messages = std::collections::BTreeMap::new();
        for (index, operation) in existing
            .iter()
            .enumerate()
            .filter(|(_, operation)| operation.card.direction == Direction::Incoming)
        {
            for key in &operation.source_public {
                if existing_sources.insert(*key, index).is_some() {
                    return Err(Error::StorageUnavailable);
                }
            }
            if operation.product_id == product_id
                && operation.card.peer_identity == peer_identity
                && operation.card.request_id == request_id
            {
                existing_messages.insert(operation.card.message_id.as_str(), index);
            }
        }
        let mut admissions: Vec<Admission> = Vec::with_capacity(incoming.len());
        let mut batch_sources = std::collections::BTreeMap::new();
        let mut batch_messages = std::collections::BTreeMap::new();
        for incoming in incoming {
            if incoming.message_id.is_empty()
                || incoming.message_id.len() > 1024
                || incoming.total_value == 0
                || incoming.coin_keys.is_empty()
                || incoming.coin_keys.len() > 4096
            {
                return Err(Error::InvalidRequest);
            }
            let entries = incoming
                .coin_keys
                .iter()
                .map(|bytes| {
                    <[u8; 64]>::try_from(bytes.as_slice())
                        .map(MemoEntry)
                        .map_err(|_| Error::InvalidRequest)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let memo = TransferMemo {
                entries,
                total_value: incoming.total_value,
            };
            let public = memo_public(&memo)?;
            let fingerprint = source_fingerprint(&public);
            let mut duplicate = batch_messages.get(&incoming.message_id).copied();
            for key in &public {
                if let Some(&index) = batch_sources.get(key) {
                    if duplicate.is_some_and(|previous| previous != index) {
                        return Err(Error::OperationConflict);
                    }
                    duplicate = Some(index);
                }
            }
            if let Some(index) = duplicate {
                let previous: &Admission = &admissions[index];
                if previous.fingerprint != fingerprint
                    || previous.memo.total_value != memo.total_value
                {
                    return Err(Error::OperationConflict);
                }
                batch_messages.insert(incoming.message_id, index);
                continue;
            }
            let mut matched = existing_messages.get(incoming.message_id.as_str()).copied();
            for key in &public {
                if let Some(&index) = existing_sources.get(key) {
                    if matched.is_some_and(|previous| previous != index) {
                        return Err(Error::OperationConflict);
                    }
                    matched = Some(index);
                }
            }
            let plan_ready = if let Some(index) = matched {
                let operation = &existing[index];
                let expected = operation
                    .denominations
                    .and_then(|d| d.0.checked_mul(u128::from(operation.card.amount_cents)));
                if operation.source_fingerprint != Some(fingerprint)
                    || expected != Some(memo.total_value)
                    || operation.product_id != product_id
                    || operation.card.peer_identity != peer_identity
                {
                    return Err(Error::OperationConflict);
                }
                let key = operation.memo_key.ok_or(Error::StorageUnavailable)?;
                self.store
                    .plan(&key)
                    .await
                    .map_err(|_| Error::StorageUnavailable)?
                    .is_some()
            } else {
                false
            };
            let index = admissions.len();
            for key in &public {
                batch_sources.insert(*key, index);
            }
            batch_messages.insert(incoming.message_id.clone(), index);
            admissions.push(Admission {
                message_id: incoming.message_id,
                timestamp: incoming.timestamp,
                memo,
                public,
                fingerprint,
                existing: matched,
                plan_ready,
            });
        }
        // Read chain denomination metadata before any custody/claim effects.
        // A fully durable replay needs neither network access nor a new plan.
        let engine = if admissions.iter().any(|entry| !entry.plan_ready) {
            let engine = Engine::new(context, self.store.clone()).await?;
            for entry in &admissions {
                if engine.cents(entry.memo.total_value)? == 0 {
                    return Err(Error::InvalidRequest);
                }
                if let Some(index) = entry.existing {
                    if existing[index].denominations != Some(binding(&engine)) {
                        return Err(Error::NetworkUnavailable);
                    }
                }
            }
            engine.synchronize(context, &self.store).await?;
            Some(engine)
        } else {
            None
        };
        let mut cards = Vec::with_capacity(admissions.len());
        for entry in admissions {
            self.check(context)?;
            let operation = if let Some(index) = entry.existing {
                std::borrow::Cow::Borrowed(&existing[index])
            } else {
                let engine = engine.as_ref().ok_or(Error::StorageUnavailable)?;
                let id = hash(
                    &(
                        b"truapi/main-purse/incoming/v1".as_slice(),
                        self.root_public_key,
                        self.genesis_hash,
                        entry.fingerprint,
                    )
                        .encode(),
                );
                let operation = Operation {
                    product_id: product_id.clone(),
                    recipient_username: None,
                    genesis_hash: self.genesis_hash,
                    card: HostNativeChatPayment {
                        operation_id: id,
                        request_id: request_id.clone(),
                        message_id: entry.message_id,
                        timestamp: entry.timestamp,
                        peer_identity,
                        direction: Direction::Incoming,
                        amount_cents: engine.cents(entry.memo.total_value)?,
                        state: State::Claiming,
                    },
                    phase: Phase::Incoming,
                    max_debit_cents: 0,
                    denominations: Some(binding(engine)),
                    source_exponents: vec![None; entry.public.len()],
                    source_seen: vec![false; entry.public.len()],
                    source_cleared: vec![false; entry.public.len()],
                    source_public: entry.public,
                    memo_key: Some(entry.memo.identifier()),
                    source_fingerprint: Some(entry.fingerprint),
                    detection_anchor: None,
                    delivered: false,
                    memo: entry.memo.scale_encoded(),
                };
                self.save(&operation).await?;
                std::borrow::Cow::Owned(operation)
            };
            if !entry.plan_ready {
                // Reordered retries retain the original memo's canonical plan.
                let canonical;
                let memo = if entry.existing.is_some() {
                    canonical = TransferMemo::from_scale_encoded(&operation.memo)
                        .map_err(|_| Error::StorageUnavailable)?;
                    &canonical
                } else {
                    &entry.memo
                };
                engine
                    .as_ref()
                    .ok_or(Error::StorageUnavailable)?
                    .claimer
                    .prepare_memo(
                        memo,
                        truapi_coinage::external_claim_message_id(&memo.identifier()),
                    )
                    .await
                    .map_err(|_| Error::NetworkUnavailable)?;
            }
            cards.push(operation.card.clone());
        }
        self.check(context)?;
        Ok(cards)
    }

    pub(crate) async fn reconcile(
        self: &Arc<Self>,
        context: &NativeChatContext,
    ) -> Result<(), Error> {
        self.check(context)?;
        let wallet = self.clone();
        let context = context.clone();
        let (tx, rx) = futures::channel::oneshot::channel();
        (context.services.spawner.clone())(Box::pin(async move {
            let _ = tx.send(wallet.reconcile_once(&context).await);
        }));
        rx.await.map_err(|_| Error::StorageUnavailable)?
    }

    async fn reconcile_once(&self, context: &NativeChatContext) -> Result<(), Error> {
        let _gate = self.gate.lock().await;
        self.check(context)?;
        self.store
            .reauthenticate()
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let operations = self.operations().await?;
        if operations
            .iter()
            .all(|operation| matches!(operation.card.state, State::Cleared | State::Failed { .. }))
        {
            return Ok(());
        }
        let engine = Engine::new(context, self.store.clone()).await?;
        engine.synchronize(context, &self.store).await?;
        for mut operation in operations {
            self.check(context)?;
            if matches!(operation.card.state, State::Cleared | State::Failed { .. }) {
                continue;
            }
            if operation.phase == Phase::Incoming {
                if operation.denominations != Some(binding(&engine)) {
                    return Err(Error::NetworkUnavailable);
                }
                let memo = TransferMemo::from_scale_encoded(&operation.memo)
                    .map_err(|_| Error::StorageUnavailable)?;
                let key = memo.identifier();
                // Claim uses exclusively supplied source secrets, never a
                // main-purse fee/input selection and never outgoing resume.
                let outcome = engine
                    .claimer
                    .claim_external_memo(memo, truapi_coinage::external_claim_message_id(&key))
                    .await;
                let plan = self
                    .store
                    .plan(&key)
                    .await
                    .map_err(|_| Error::StorageUnavailable)?;
                let cleared = plan.as_ref().and_then(|p| p.claimed_amount).unwrap_or(0);
                operation.card.state = if plan
                    .as_ref()
                    .is_some_and(|p| p.status == ClaimPlanStatus::Finished)
                    && Some(cleared)
                        == operation
                            .denominations
                            .ok_or(Error::StorageUnavailable)?
                            .0
                            .checked_mul(u128::from(operation.card.amount_cents))
                {
                    State::Cleared
                } else if cleared > 0 {
                    State::PartiallyCleared {
                        cleared_cents: engine.partial_cents(cleared)?,
                    }
                } else if outcome.is_err() {
                    State::Recovering
                } else {
                    State::Claiming
                };
            } else if matches!(
                operation.phase,
                Phase::HandoffReady | Phase::Accepted | Phase::Approved
            ) {
                // Observation only: a new session must obtain a new review
                // through send() before any additional split/unload signature.
                self.observe_outgoing(&engine, &mut operation).await?;
            }
            self.save(&operation).await?;
        }
        Ok(())
    }

    async fn observe_outgoing(
        &self,
        engine: &Engine,
        operation: &mut Operation,
    ) -> Result<(), Error> {
        if operation.source_public.is_empty() {
            return Ok(());
        }
        if operation.denominations != Some(binding(engine)) {
            return Err(Error::NetworkUnavailable);
        }
        let journals = self
            .store
            .load_operation(&hex::encode(operation.card.operation_id))
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        if journals.iter().any(|entry| {
            matches!(
                entry.operation,
                truapi_coinage::WalOperation::TransferAccepted
                    | truapi_coinage::WalOperation::TransferCompleted
            )
        }) {
            operation.phase = Phase::Accepted;
        }
        if journals
            .iter()
            .any(|entry| entry.operation == truapi_coinage::WalOperation::TransferRejected)
        {
            operation.card.state = State::Failed {
                reason: Failure::Cancelled,
            };
            return Ok(());
        }
        // Completed proves each prepared output was finalized (never merely
        // that its inputs disappeared); pass-through coins were independently
        // verified at our durable pre-handoff anchor. This closes the fast-peer
        // race where a recipient claims before the next observation.
        if journals
            .iter()
            .any(|entry| entry.operation == truapi_coinage::WalOperation::TransferCompleted)
        {
            operation.source_seen.fill(true);
        }
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
        let mut cleared = 0u128;
        for (i, row) in rows.into_iter().enumerate() {
            if let Some(row) = row {
                if operation.source_exponents[i].is_some_and(|e| e != row.exponent) {
                    return Err(Error::NetworkUnavailable);
                }
                operation.source_exponents[i] = Some(row.exponent);
                operation.source_seen[i] = true;
            } else if operation.source_seen[i] {
                operation.source_cleared[i] = true;
            }
            if operation.source_cleared[i] {
                cleared = cleared
                    .checked_add(engine.denominations.value_in_planks(
                        operation.source_exponents[i].ok_or(Error::StorageUnavailable)?,
                    ))
                    .ok_or(Error::InvalidRequest)?;
            }
        }
        operation.detection_anchor = Some(at);
        let total = engine
            .denominations
            .cash_cents_to_planks(u128::from(operation.card.amount_cents))
            .ok_or(Error::InvalidRequest)?;
        if cleared > total {
            return Err(Error::NetworkUnavailable);
        }
        if cleared == total {
            operation.card.state = State::Cleared;
        } else if cleared > 0 {
            operation.card.state = State::PartiallyCleared {
                cleared_cents: engine.partial_cents(cleared)?,
            };
        } else if operation.phase == Phase::Accepted {
            operation.card.state = if operation.delivered {
                State::Delivered
            } else {
                State::Delivering
            };
        } else {
            operation.card.state = State::Recovering;
        }
        Ok(())
    }

    pub(crate) async fn views(
        &self,
        product_id: &str,
    ) -> Result<Vec<HostNativeChatPayment>, Error> {
        self.store
            .reauthenticate()
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        Ok(self
            .operations()
            .await?
            .into_iter()
            .filter(|operation| {
                operation.product_id == product_id && operation.phase != Phase::Review
            })
            .map(|operation| operation.card.clone())
            .collect())
    }
    /// Public cards only. The authenticated actor's irreversible custody ledger
    /// can repair a lost wallet acceptance write, but a prepared memo alone cannot.
    pub(crate) async fn pending_handoffs(
        &self,
        context: &NativeChatContext,
        product_id: &str,
        accepted_operations: &[[u8; 32]],
    ) -> Result<Vec<HostNativeChatPayment>, Error> {
        let _gate = self.gate.lock().await;
        self.check(context)?;
        self.store
            .reauthenticate()
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let accepted: std::collections::BTreeSet<_> = accepted_operations.iter().collect();
        let mut cards = Vec::new();
        for mut operation in self.operations().await? {
            self.check(context)?;
            if operation.phase == Phase::HandoffReady
                && operation.product_id == product_id
                && operation.genesis_hash == self.genesis_hash
                && operation.card.direction == Direction::Outgoing
                && accepted.contains(&operation.card.operation_id)
            {
                let journals = self
                    .store
                    .load_operation(&hex::encode(operation.card.operation_id))
                    .await
                    .map_err(|_| Error::StorageUnavailable)?;
                if journals
                    .iter()
                    .any(|entry| entry.operation == truapi_coinage::WalOperation::TransferRejected)
                {
                    return Err(Error::OperationConflict);
                }
                let parent_id = truapi_coinage::wal::operation_entry_id(
                    &hex::encode(operation.card.operation_id),
                    "parent",
                );
                let parent = journals
                    .iter()
                    .find(|entry| entry.entry_id == parent_id)
                    .ok_or(Error::StorageUnavailable)?;
                if !matches!(
                    parent.operation,
                    truapi_coinage::WalOperation::TransferPrepared
                        | truapi_coinage::WalOperation::TransferAccepted
                        | truapi_coinage::WalOperation::TransferCompleted
                ) {
                    return Err(Error::StorageUnavailable);
                }
                self.stored_memo(&operation)?;
                if parent.operation == truapi_coinage::WalOperation::TransferPrepared {
                    let mut receipt = parent.clone();
                    receipt.operation = truapi_coinage::WalOperation::TransferAccepted;
                    self.check(context)?;
                    WalStore::save(self.store.as_ref(), &receipt)
                        .await
                        .map_err(|_| Error::StorageUnavailable)?;
                }
                operation.phase = Phase::Accepted;
                self.check(context)?;
                self.save(&operation).await?;
                // Recording proven custody permits read-only recovery to retire
                // consumed inputs. It does not execute any unfinished funding;
                // signatures still require the reviewed exact-plan resume path.
            }
            if self.pending_memo(&operation, product_id).await?.is_some() {
                cards.push(operation.card.clone());
            }
        }
        self.check(context)?;
        Ok(cards)
    }

    /// Re-encrypt an already accepted memo for the current authenticated roster.
    /// No wallet state changes: failure or cancellation cannot revoke the
    /// original acceptance, release reservations, or claim peer delivery.
    pub(crate) async fn redeliver(
        &self,
        context: &NativeChatContext,
        product_id: &str,
        operation_id: [u8; 32],
        transport: Arc<dyn PaymentTransport>,
    ) -> Result<(), Error> {
        let _gate = self.gate.lock().await;
        self.check(context)?;
        self.store
            .reauthenticate()
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let operation = self
            .load(operation_id)
            .await?
            .ok_or(Error::OperationNotFound)?;
        if operation.product_id != product_id
            || operation.genesis_hash != self.genesis_hash
            || operation.card.direction != Direction::Outgoing
        {
            return Err(Error::OperationNotFound);
        }
        self.replay_handoff(context, &operation, transport).await
    }

    // The caller holds the wallet gate and has reauthenticated storage.
    async fn replay_handoff(
        &self,
        context: &NativeChatContext,
        operation: &Operation,
        transport: Arc<dyn PaymentTransport>,
    ) -> Result<(), Error> {
        self.check(context)?;
        let memo = self
            .pending_memo(operation, &operation.product_id)
            .await?
            .ok_or(Error::OperationConflict)?;
        self.check(context)?;
        let result = transport.accept(&operation.card, memo).await;
        self.check(context)?;
        result.map_err(|_| Error::NetworkUnavailable)
    }

    async fn pending_memo(
        &self,
        operation: &Operation,
        product_id: &str,
    ) -> Result<Option<TransferMemo>, Error> {
        if operation.product_id != product_id
            || operation.genesis_hash != self.genesis_hash
            || operation.card.direction != Direction::Outgoing
            || operation.phase != Phase::Accepted
            || operation.delivered
            || matches!(
                operation.card.state,
                State::Delivered | State::Cleared | State::Failed { .. }
            )
        {
            return Ok(None);
        }
        let journals = self
            .store
            .load_operation(&hex::encode(operation.card.operation_id))
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        if journals
            .iter()
            .any(|entry| entry.operation == truapi_coinage::WalOperation::TransferRejected)
        {
            return Ok(None);
        }
        self.stored_memo(operation).map(Some)
    }

    fn stored_memo(&self, operation: &Operation) -> Result<TransferMemo, Error> {
        let card = &operation.card;
        if operation.genesis_hash != self.genesis_hash
            || card.direction != Direction::Outgoing
            || card.amount_cents == 0
            || operation.product_id.is_empty()
            || operation.product_id.len() > 1024
            || card.request_id.is_empty()
            || card.request_id.len() > 1024
            || card.operation_id
                != operation_id(
                    self.root_public_key,
                    self.genesis_hash,
                    &operation.product_id,
                    &card.request_id,
                )
            || card.message_id != format!("payment-{}", hex::encode(card.operation_id))
        {
            return Err(Error::StorageUnavailable);
        }
        let amount = operation
            .denominations
            .filter(|binding| binding.0 != 0)
            .and_then(|binding| binding.0.checked_mul(u128::from(card.amount_cents)))
            .ok_or(Error::StorageUnavailable)?;
        let memo = TransferMemo::from_scale_encoded(&operation.memo)
            .map_err(|_| Error::StorageUnavailable)?;
        if memo.entries.is_empty()
            || memo.entries.len() > 4096
            || memo.total_value != amount
            || operation.memo_key != Some(memo.identifier())
        {
            return Err(Error::StorageUnavailable);
        }
        let public = memo_public(&memo).map_err(|_| Error::StorageUnavailable)?;
        if public != operation.source_public
            || operation.source_fingerprint != Some(source_fingerprint(&public))
        {
            return Err(Error::StorageUnavailable);
        }
        Ok(memo)
    }

    /// Called only after the parent has durably authenticated the peer ACK.
    /// Delivery is independent from finalized monetary clearing.
    pub(crate) async fn note_delivery(
        &self,
        context: &NativeChatContext,
        product_id: &str,
        operation_id: [u8; 32],
    ) -> Result<(), Error> {
        let _gate = self.gate.lock().await;
        self.check(context)?;
        self.store
            .reauthenticate()
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let mut operation = self
            .load(operation_id)
            .await?
            .ok_or(Error::OperationNotFound)?;
        if operation.product_id != product_id || operation.card.direction != Direction::Outgoing {
            return Err(Error::OperationNotFound);
        }
        if !matches!(operation.phase, Phase::Accepted | Phase::HandoffReady) {
            return Err(Error::OperationConflict);
        }
        operation.phase = Phase::Accepted;
        operation.delivered = true;
        if matches!(
            operation.card.state,
            State::Preparing | State::Delivering | State::Recovering
        ) {
            operation.card.state = State::Delivered;
        }
        self.check(context)?;
        self.save(&operation).await
    }

    fn check(&self, context: &NativeChatContext) -> Result<(), Error> {
        live(context)?;
        if context.coinage_instance_id != self.coinage_instance_id {
            return Err(Error::OperationConflict);
        }
        if context.session.public_key != self.root_public_key
            || context.genesis_hash != self.genesis_hash
        {
            return Err(Error::NotConnected);
        }
        Ok(())
    }

    async fn save(&self, operation: &Operation) -> Result<(), Error> {
        let mut bytes = OPERATION_MAGIC.to_vec();
        operation.encode_to(&mut bytes);
        self.store
            .write_operation(operation.card.operation_id, bytes)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    async fn load(&self, id: [u8; 32]) -> Result<Option<Operation>, Error> {
        self.store
            .read_operation(id)
            .await
            .map_err(|_| Error::StorageUnavailable)?
            .map(|bytes| {
                let operation = decode_operation(&Zeroizing::new(bytes))?;
                if operation.card.operation_id != id {
                    return Err(Error::StorageUnavailable);
                }
                Ok(operation)
            })
            .transpose()
    }

    async fn operations(&self) -> Result<Vec<Operation>, Error> {
        self.store
            .list_operations()
            .await
            .map_err(|_| Error::StorageUnavailable)?
            .into_iter()
            .filter(|(id, _)| *id != inventory::progress_id())
            .map(|(id, bytes)| {
                let operation = decode_operation(&bytes)?;
                if operation.card.operation_id != id {
                    return Err(Error::StorageUnavailable);
                }
                Ok(operation)
            })
            .collect()
    }
}

fn decode_operation(bytes: &[u8]) -> Result<Operation, Error> {
    let mut input = bytes
        .strip_prefix(OPERATION_MAGIC)
        .ok_or(Error::StorageUnavailable)?;
    let operation = Operation::decode(&mut input).map_err(|_| Error::StorageUnavailable)?;
    if !input.is_empty()
        || operation.source_public.len() != operation.source_seen.len()
        || operation.source_public.len() != operation.source_cleared.len()
        || operation.source_public.len() != operation.source_exponents.len()
    {
        return Err(Error::StorageUnavailable);
    }
    Ok(operation)
}

fn new_outgoing(id: [u8; 32], genesis: [u8; 32], intent: PaymentIntent) -> Operation {
    Operation {
        product_id: intent.product_id,
        recipient_username: intent.recipient_username,
        genesis_hash: genesis,
        card: HostNativeChatPayment {
            operation_id: id,
            request_id: intent.request_id,
            message_id: format!("payment-{}", hex::encode(id)),
            timestamp: SystemClock.now_ms().max(0) as u64,
            peer_identity: intent.peer_identity,
            direction: Direction::Outgoing,
            amount_cents: intent.amount_cents,
            state: State::Preparing,
        },
        phase: Phase::Review,
        max_debit_cents: 0,
        denominations: None,
        source_public: Vec::new(),
        source_exponents: Vec::new(),
        source_seen: Vec::new(),
        source_cleared: Vec::new(),
        memo_key: None,
        source_fingerprint: None,
        detection_anchor: None,
        delivered: false,
        memo: Vec::new(),
    }
}

fn validate_intent(intent: &PaymentIntent) -> Result<(), Error> {
    if intent.amount_cents == 0
        || intent.product_id.is_empty()
        || intent.product_id.len() > 1024
        || intent.request_id.is_empty()
        || intent.request_id.len() > 1024
        || intent
            .recipient_username
            .as_ref()
            .is_some_and(|name| name.len() > 1024)
    {
        return Err(Error::InvalidRequest);
    }
    Ok(())
}

fn operation_id(root: [u8; 32], genesis: [u8; 32], product: &str, request: &str) -> [u8; 32] {
    hash(
        &(
            b"truapi/main-purse/outgoing/v1".as_slice(),
            root,
            genesis,
            product,
            request,
        )
            .encode(),
    )
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    sp_crypto_hashing::blake2_256(bytes)
}

fn storage_key(context: &NativeChatContext) -> Zeroizing<[u8; 32]> {
    let mut material = Zeroizing::new(b"truapi/main-purse/encryption/v1".to_vec());
    context.entropy.encode_to(&mut *material);
    context.session.public_key.encode_to(&mut *material);
    context.genesis_hash.encode_to(&mut *material);
    Zeroizing::new(hash(&material))
}

fn binding(engine: &Engine) -> (u128, i16, i16, u8) {
    let d = &engine.denominations;
    (d.asset_unit, d.min_exponent, d.max_exponent, d.precision)
}

fn memo_public(memo: &TransferMemo) -> Result<Vec<[u8; 32]>, Error> {
    let public = memo
        .entries
        .iter()
        .map(|entry| {
            schnorrkel::SecretKey::from_bytes(&entry.0)
                .map(|secret| secret.to_public().to_bytes())
                .map_err(|_| Error::InvalidRequest)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut sorted = public.clone();
    sorted.sort_unstable();
    if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(Error::InvalidRequest);
    }
    Ok(public)
}

fn source_fingerprint(public: &[[u8; 32]]) -> [u8; 32] {
    let mut sorted = public.to_vec();
    sorted.sort_unstable();
    hash(&(b"truapi/main-purse/memo-sources/v1".as_slice(), sorted).encode())
}

fn live(context: &NativeChatContext) -> Result<(), Error> {
    if (context.session_valid)() {
        Ok(())
    } else {
        Err(Error::NotConnected)
    }
}

async fn review_operation(
    platform: &dyn truapi_platform::UserConfirmation,
    session_valid: &(dyn Fn() -> bool + Send + Sync),
    operation: &Operation,
    coinage_instance_id: Option<u32>,
) -> Result<bool, Error> {
    if !session_valid() {
        return Err(Error::NotConnected);
    }
    let approved = platform
        .confirm_user_action(UserConfirmationReview::MainPurseChatPayment(
            operation.review(coinage_instance_id),
        ))
        .await
        .map_err(|_| Error::AccessNotGranted)?;
    if !session_valid() {
        return Err(Error::NotConnected);
    }
    Ok(approved)
}
