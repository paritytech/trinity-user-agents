// SPDX-License-Identifier: AGPL-3.0-only
//! Dispatch to exactly one custody owner. This module never allocates native coins
//! or records a second ledger: native persistence and finality belong to the service.

use std::{collections::BTreeSet, sync::Arc};

use futures::{channel::oneshot, lock::Mutex};
use parity_scale_codec::Encode;
use truapi::latest::{
    HostNativeChatPayment as Payment, HostNativeChatPaymentDirection as Direction,
    HostNativeChatPaymentState as State, HostProductDeviceChatError as Error,
};
use truapi::v01::{
    HostPaymentTopUpError as TopUpError, HostPaymentTopUpRequest, PaymentTopUpSource,
};
use truapi_coinage::{MemoEntry, TransferMemo};
use truapi_platform::{
    CoinageWalletHost, NativeCoinageFailure, NativeCoinageMemo,
    NativeCoinageOperation as Operation, NativeCoinagePaymentIntent, NativeCoinageRequest,
    NativeCoinageResponse as Response, NativeCoinageScope, NativeCoinageTopUpOutcome,
};
use zeroize::{Zeroize, Zeroizing};

use super::{
    NativeChatContext,
    payments::{self, PaymentIntent, PaymentTransport, WalletCoinage},
};

const MAX_SOURCES: usize = 4096;
const MAX_PAYMENTS: usize = 4096;

pub(crate) struct WalletBinding {
    native_wallet: Arc<dyn CoinageWalletHost>,
    scope: NativeCoinageScope,
}

impl WalletBinding {
    pub(super) fn new(
        context: &NativeChatContext,
        native_wallet: Arc<dyn CoinageWalletHost>,
    ) -> Self {
        Self {
            native_wallet,
            scope: NativeCoinageScope {
                root_public_key: context.session.public_key,
                genesis_hash: context.genesis_hash,
                coinage_instance_id: context.coinage_instance_id,
            },
        }
    }

    pub(super) fn check(&self, context: &NativeChatContext) -> Result<(), Error> {
        context.require_current()?;
        if self.scope.root_public_key != context.session.public_key
            || self.scope.genesis_hash != context.genesis_hash
        {
            return Err(Error::NotConnected);
        }
        if self.scope.coinage_instance_id != context.coinage_instance_id {
            return Err(Error::OperationConflict);
        }
        Ok(())
    }

    async fn call(
        &self,
        context: &NativeChatContext,
        operation: Operation,
    ) -> Result<Zeroizing<Response>, Error> {
        // Guard before checking authority: TopUp owns secrets even on rejection.
        let mut request = Zeroizing::new(NativeCoinageRequest {
            scope: self.scope.clone(),
            operation,
        });
        self.check(context)?;
        let operation = std::mem::replace(&mut request.operation, Operation::Reconcile);
        let response = self
            .native_wallet
            .native_coinage(NativeCoinageRequest {
                scope: request.scope.clone(),
                operation,
            })
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let response = Zeroizing::new(response);
        self.check(context)?;
        if let Response::Failed { reason } = &*response {
            return Err(failure(*reason));
        }
        Ok(response)
    }

    pub(super) async fn denomination(&self, context: &NativeChatContext) -> Result<u128, Error> {
        self.check(context)?;
        let response = self.call(context, Operation::Denomination).await?;
        match &*response {
            Response::Denomination { cents_unit_raw } => decimal(cents_unit_raw)
                .filter(|value| *value > 0)
                .ok_or(Error::StorageUnavailable),
            _ => Err(Error::StorageUnavailable),
        }
    }

    fn validate_card(&self, product: &str, card: &Payment) -> Result<(), Error> {
        // Incoming generic top-ups have no Chat card; this boundary exposes only
        // authenticated outgoing intents, never source keys or native error text.
        if card.direction != Direction::Outgoing
            || card.amount_cents == 0
            || card.request_id.is_empty()
            || card.request_id.len() > 1024
            || card.operation_id
                != payments::operation_id(
                    self.scope.root_public_key,
                    self.scope.genesis_hash,
                    product,
                    &card.request_id,
                )
            || card.message_id != format!("payment-{}", hex::encode(card.operation_id))
            || matches!(card.state, State::Claiming)
            || matches!(card.state, State::PartiallyCleared { cleared_cents }
                if cleared_cents == 0 || cleared_cents >= card.amount_cents)
        {
            return Err(Error::StorageUnavailable);
        }
        Ok(())
    }

    fn cards(&self, product: &str, response: &Response) -> Result<Vec<Payment>, Error> {
        let Response::Payments { payments } = response else {
            return Err(Error::StorageUnavailable);
        };
        if payments.len() > MAX_PAYMENTS {
            return Err(Error::StorageUnavailable);
        }
        let mut ids = BTreeSet::new();
        for card in payments {
            self.validate_card(product, card)?;
            if !ids.insert(card.operation_id) {
                return Err(Error::StorageUnavailable);
            }
        }
        Ok(payments.clone())
    }
}

pub(crate) enum SelectedWallet {
    Rust(Arc<WalletCoinage>),
    Native {
        binding: WalletBinding,
        gate: Mutex<()>,
    },
}

impl SelectedWallet {
    pub(super) fn native(binding: WalletBinding) -> Self {
        Self::Native {
            binding,
            gate: Mutex::new(()),
        }
    }

    pub(super) fn check(&self, context: &NativeChatContext) -> Result<(), Error> {
        match self {
            Self::Rust(wallet) => wallet.check(context),
            Self::Native { binding, .. } => binding.check(context),
        }
    }

    pub(super) async fn denomination(&self, context: &NativeChatContext) -> Result<u128, Error> {
        self.check(context)?;
        match self {
            Self::Rust(_) => payments::coinage_cents_unit(context).await,
            Self::Native { binding, .. } => binding.denomination(context).await,
        }
    }

    pub(super) fn is_native(&self) -> bool {
        matches!(self, Self::Native { .. })
    }

    pub(super) async fn views(
        &self,
        context: &NativeChatContext,
        product: &str,
    ) -> Result<Vec<Payment>, Error> {
        context.require_current()?;
        match self {
            Self::Rust(wallet) => wallet.views(product).await,
            Self::Native { binding, .. } => {
                validate_product(product)?;
                let response = binding
                    .call(
                        context,
                        Operation::Views {
                            product_id: product.into(),
                        },
                    )
                    .await?;
                binding.cards(product, &response)
            }
        }
    }

    pub(super) async fn send(
        self: &Arc<Self>,
        context: &NativeChatContext,
        intent: PaymentIntent,
        transport: Arc<dyn PaymentTransport>,
    ) -> Result<Payment, Error> {
        match self.as_ref() {
            Self::Rust(wallet) => wallet.send(context, intent, transport).await,
            Self::Native { binding, .. } => {
                binding.check(context)?;
                payments::validate_intent(&intent)?;
                let wallet = self.clone();
                let context = context.clone();
                let (tx, rx) = oneshot::channel();
                (context.services.spawner.clone())(Box::pin(async move {
                    let _ = tx.send(wallet.send_native(&context, intent, transport).await);
                }));
                rx.await.map_err(|_| Error::StorageUnavailable)?
            }
        }
    }

    async fn send_native(
        &self,
        context: &NativeChatContext,
        intent: PaymentIntent,
        transport: Arc<dyn PaymentTransport>,
    ) -> Result<Payment, Error> {
        let Self::Native { binding, gate } = self else {
            unreachable!()
        };
        let _gate = gate.lock().await;
        binding.check(context)?;
        let operation_id = payments::operation_id(
            binding.scope.root_public_key,
            binding.scope.genesis_hash,
            &intent.product_id,
            &intent.request_id,
        );
        let response = binding
            .call(
                context,
                Operation::PreparePayment {
                    intent: NativeCoinagePaymentIntent {
                        operation_id,
                        product_id: intent.product_id.clone(),
                        request_id: intent.request_id.clone(),
                        peer_identity: intent.peer_identity,
                        recipient_username: intent.recipient_username.clone(),
                        amount_cents: intent.amount_cents,
                    },
                },
            )
            .await?;
        let Response::Prepared { payment, memo } = &*response else {
            return Err(Error::StorageUnavailable);
        };
        binding.validate_card(&intent.product_id, payment)?;
        if payment.operation_id != operation_id
            || payment.request_id != intent.request_id
            || payment.peer_identity != intent.peer_identity
            || payment.amount_cents != intent.amount_cents
        {
            return Err(Error::OperationConflict);
        }
        if let Some(memo) = memo {
            self.accept_native(context, &intent.product_id, payment, memo, transport)
                .await?;
        } else if !matches!(
            payment.state,
            State::Delivered | State::Cleared | State::Failed { .. }
        ) {
            return Err(Error::StorageUnavailable);
        }
        Ok(payment.clone())
    }

    async fn accept_native(
        &self,
        context: &NativeChatContext,
        product: &str,
        payment: &Payment,
        memo: &NativeCoinageMemo,
        transport: Arc<dyn PaymentTransport>,
    ) -> Result<(), Error> {
        let Self::Native { binding, .. } = self else {
            unreachable!()
        };
        if matches!(
            payment.state,
            State::Delivered | State::Cleared | State::Failed { .. }
        ) {
            return Err(Error::StorageUnavailable);
        }
        let unit = binding.denomination(context).await?;
        let expected = unit
            .checked_mul(u128::from(payment.amount_cents))
            .ok_or(Error::StorageUnavailable)?;
        let memo = transfer_memo(memo, expected)?;
        binding.check(context)?;
        transport
            .accept(payment, memo)
            .await
            .map_err(|_| Error::NetworkUnavailable)?;
        binding.check(context)?;
        // Ambiguous commit is never rollback. On restart the actor passes its
        // durable accepted ids to PendingHandoffs, repairing only proven custody.
        let response = binding
            .call(
                context,
                Operation::CommitHandoff {
                    product_id: product.into(),
                    operation_id: payment.operation_id,
                },
            )
            .await?;
        done(&response)
    }

    pub(super) async fn pending_handoffs(
        &self,
        context: &NativeChatContext,
        product: &str,
        accepted: &[[u8; 32]],
    ) -> Result<Vec<Payment>, Error> {
        match self {
            Self::Rust(wallet) => wallet.pending_handoffs(context, product, accepted).await,
            Self::Native { binding, gate } => {
                let _gate = gate.lock().await;
                validate_product(product)?;
                if accepted.len() > MAX_PAYMENTS {
                    return Err(Error::InvalidRequest);
                }
                let response = binding
                    .call(
                        context,
                        Operation::PendingHandoffs {
                            product_id: product.into(),
                            accepted_operations: accepted.to_vec(),
                        },
                    )
                    .await?;
                let cards = binding.cards(product, &response)?;
                if cards.iter().any(|card| {
                    !accepted.contains(&card.operation_id)
                        || matches!(
                            card.state,
                            State::Delivered | State::Cleared | State::Failed { .. }
                        )
                }) {
                    return Err(Error::StorageUnavailable);
                }
                Ok(cards)
            }
        }
    }

    pub(super) async fn redeliver(
        &self,
        context: &NativeChatContext,
        product: &str,
        operation_id: [u8; 32],
        transport: Arc<dyn PaymentTransport>,
    ) -> Result<(), Error> {
        match self {
            Self::Rust(wallet) => {
                wallet
                    .redeliver(context, product, operation_id, transport)
                    .await
            }
            Self::Native { binding, gate } => {
                let _gate = gate.lock().await;
                validate_product(product)?;
                let response = binding
                    .call(
                        context,
                        Operation::ReadHandoff {
                            product_id: product.into(),
                            operation_id,
                        },
                    )
                    .await?;
                let Response::Prepared {
                    payment,
                    memo: Some(memo),
                } = &*response
                else {
                    return Err(Error::StorageUnavailable);
                };
                binding.validate_card(product, payment)?;
                if payment.operation_id != operation_id {
                    return Err(Error::OperationConflict);
                }
                self.accept_native(context, product, payment, memo, transport)
                    .await
            }
        }
    }

    pub(super) async fn note_delivery(
        &self,
        context: &NativeChatContext,
        product: &str,
        operation_id: [u8; 32],
    ) -> Result<(), Error> {
        match self {
            Self::Rust(wallet) => wallet.note_delivery(context, product, operation_id).await,
            Self::Native { binding, gate } => {
                let _gate = gate.lock().await;
                validate_product(product)?;
                let response = binding
                    .call(
                        context,
                        Operation::NoteDelivery {
                            product_id: product.into(),
                            operation_id,
                        },
                    )
                    .await?;
                done(&response)
            }
        }
    }

    pub(super) async fn reconcile(
        self: &Arc<Self>,
        context: &NativeChatContext,
    ) -> Result<(), Error> {
        match self.as_ref() {
            Self::Rust(wallet) => wallet.reconcile(context).await,
            Self::Native { binding, gate } => {
                let _gate = gate.lock().await;
                let response = binding.call(context, Operation::Reconcile).await?;
                done(&response)
            }
        }
    }

    pub(super) async fn recover(
        self: &Arc<Self>,
        context: &NativeChatContext,
    ) -> Result<(), Error> {
        match self.as_ref() {
            Self::Rust(wallet) => {
                let imports = wallet.reconcile_top_ups(context).await;
                let outgoing = wallet.reconcile(context).await;
                imports?;
                outgoing
            }
            Self::Native { .. } => self.reconcile(context).await,
        }
    }

    pub(super) async fn top_up(
        self: &Arc<Self>,
        context: &NativeChatContext,
        product: &str,
        request: HostPaymentTopUpRequest,
    ) -> Result<(), TopUpError> {
        if let Self::Rust(wallet) = self.as_ref() {
            return wallet.top_up(context, product, request).await;
        }
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
            || validate_product(product).is_err()
            || keys.is_empty()
            || keys.len() > MAX_SOURCES
        {
            return Err(TopUpError::InvalidSource);
        }
        let mut public = Vec::with_capacity(keys.len());
        for key in keys.iter() {
            let source = schnorrkel::SecretKey::from_bytes(key)
                .map_err(|_| TopUpError::InvalidSource)?
                .to_public()
                .to_bytes();
            public.push(source);
        }
        public.sort_unstable();
        if public.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(TopUpError::InvalidSource);
        }
        let Self::Native { binding, .. } = self.as_ref() else {
            unreachable!()
        };
        binding.check(context).map_err(top_up_error)?;
        let operation_id = sp_crypto_hashing::blake2_256(
            &(
                b"truapi/main-purse/native-coins-top-up/v1".as_slice(),
                &binding.scope,
                product,
                public,
            )
                .encode(),
        );
        let wallet = self.clone();
        let context = context.clone();
        let product = product.to_owned();
        let minimum = request.amount;
        let (tx, rx) = oneshot::channel();
        (context.services.spawner.clone())(Box::pin(async move {
            let Self::Native { binding, gate } = wallet.as_ref() else {
                unreachable!()
            };
            let _gate = gate.lock().await;
            let response = binding
                .call(
                    &context,
                    Operation::TopUp {
                        product_id: product,
                        operation_id,
                        minimum_amount_raw: minimum.to_string(),
                        secret_keys: keys.iter().map(|key| key.to_vec()).collect(),
                    },
                )
                .await;
            let result = response.map_err(top_up_error).and_then(|response| {
                match &*response {
                    Response::TopUp {
                        outcome: NativeCoinageTopUpOutcome::Cleared,
                    } => Ok(()),
                    Response::TopUp {
                        outcome:
                            NativeCoinageTopUpOutcome::Partial {
                                credited_amount_raw,
                            },
                    } => {
                        let credited = decimal(credited_amount_raw)
                            .filter(|credited| *credited > 0 && *credited < minimum)
                            .ok_or_else(|| top_up_error(Error::StorageUnavailable))?;
                        Err(TopUpError::PartialPayment { credited })
                    }
                    Response::TopUp {
                        outcome: NativeCoinageTopUpOutcome::NotClaimed,
                    } => Err(TopUpError::InsufficientFunds),
                    // Pending, malformed and unrelated responses cannot certify final credit.
                    _ => Err(top_up_error(Error::NetworkUnavailable)),
                }
            });
            let _ = tx.send(result);
        }));
        rx.await
            .map_err(|_| top_up_error(Error::StorageUnavailable))?
    }
}

fn validate_product(product: &str) -> Result<(), Error> {
    if product.is_empty() || product.len() > 1024 {
        Err(Error::InvalidRequest)
    } else {
        Ok(())
    }
}

fn decimal(value: &str) -> Option<u128> {
    if value.is_empty()
        || value.len() > 39
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}

fn transfer_memo(memo: &NativeCoinageMemo, expected: u128) -> Result<TransferMemo, Error> {
    if decimal(&memo.total_value_raw) != Some(expected)
        || expected == 0
        || memo.secret_keys.is_empty()
        || memo.secret_keys.len() > MAX_SOURCES
    {
        return Err(Error::StorageUnavailable);
    }
    let mut entries = Vec::with_capacity(memo.secret_keys.len());
    let mut public = BTreeSet::new();
    for bytes in &memo.secret_keys {
        let key = Zeroizing::new(
            <[u8; 64]>::try_from(bytes.as_slice()).map_err(|_| Error::StorageUnavailable)?,
        );
        let secret =
            schnorrkel::SecretKey::from_bytes(&*key).map_err(|_| Error::StorageUnavailable)?;
        if !public.insert(secret.to_public().to_bytes()) {
            return Err(Error::StorageUnavailable);
        }
        entries.push(MemoEntry(*key));
    }
    Ok(TransferMemo {
        entries,
        total_value: expected,
    })
}

fn done(response: &Response) -> Result<(), Error> {
    if matches!(response, Response::Done) {
        Ok(())
    } else {
        Err(Error::StorageUnavailable)
    }
}

fn failure(reason: NativeCoinageFailure) -> Error {
    match reason {
        NativeCoinageFailure::Unavailable => Error::StorageUnavailable,
        NativeCoinageFailure::InvalidRequest | NativeCoinageFailure::InvalidSource => {
            Error::InvalidRequest
        }
        NativeCoinageFailure::OperationConflict => Error::OperationConflict,
        NativeCoinageFailure::OperationNotFound => Error::OperationNotFound,
        NativeCoinageFailure::InsufficientBalance => Error::InsufficientBalance,
        NativeCoinageFailure::UserRejected => Error::UserRejected,
    }
}

fn top_up_error(error: Error) -> TopUpError {
    match error {
        Error::InvalidRequest | Error::OperationConflict => TopUpError::InvalidSource,
        Error::InsufficientBalance => TopUpError::InsufficientFunds,
        _ => TopUpError::Unknown {
            reason: "Native top-up is unresolved; retry the same coins".into(),
        },
    }
}
