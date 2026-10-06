//! Withdrawals: the user's CASH paid into a withdrawal account on People,
//! the way getcash pays its withdrawal key.
//!
//! Each outbound session gets its own account under the funding product.
//! The host asks the user to approve a payment into it, under an id taken
//! from the account and the attempt; the watch then reads the account's
//! CASH and the host's word on the payment until the CASH is there.

use core::time::Duration;
use std::sync::Arc;

use futures::StreamExt;
use truapi::latest::{
    GenericError, HostPaymentError, HostPaymentRequest, HostPaymentStatusSubscribeError,
};

use super::conversion::ConversionChains;
use super::{
    CANCEL_CONFIRM, CancelFundingError, FundingRegistry, MAX_USED_ACCOUNTS, within_chain_timeout,
    within_timeout,
};
use crate::host_logic::funding::{
    FundingSessionError, FundingWithdrawal, PaymentReading, PaymentWord, funding_attempt_id,
};
use crate::platform::{CoreStorage, ProductContext};
use crate::runtime::services::RuntimeServices;
use crate::unix_time::current_unix_millis;

/// Longest the host may take to report a payment's current status, as
/// getcash waits.
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);
/// Longest one watch pass waits on a withdrawal account and its payment.
const WATCH_READ: Duration = Duration::from_secs(45);

/// Why a withdrawal could not be started or its payment asked for.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum WithdrawError {
    /// No such session.
    #[display("no such funding session")]
    NotFound,
    /// The session is not an open outbound one naming its amount, without a
    /// withdrawal account.
    #[display("funding session is not awaiting a withdrawal account")]
    NotAwaitingWithdrawal,
    /// No signing host converts funding on this runtime.
    #[display("this host does not convert funding")]
    ConversionUnavailable,
    /// The host has no payment engine.
    #[display("this host does not take payments")]
    PaymentsUnavailable,
    /// Every account tried already holds CASH.
    #[display("every withdrawal account tried already holds CASH")]
    AccountsInUse,
    /// The account could not be derived.
    #[display("{}", _0.reason)]
    Derive(GenericError),
    /// The chain could not be read.
    #[display("{}", _0.reason)]
    Chain(GenericError),
    /// The host or the user refused the payment.
    #[display("{_0}")]
    Refused(HostPaymentError),
    /// The session could not be saved.
    #[display("{_0}")]
    Storage(FundingSessionError),
}

impl From<FundingSessionError> for WithdrawError {
    fn from(error: FundingSessionError) -> Self {
        Self::Storage(error)
    }
}

impl FundingRegistry {
    /// Every session whose withdrawal account is still read, with it.
    fn withdrawing_sessions(&self, now_ms: u64) -> Vec<(String, FundingWithdrawal)> {
        self.lock_sessions()
            .values()
            .filter_map(|session| {
                let withdrawal = session.watched_withdrawal(now_ms)?;
                Some((session.intent.clone(), withdrawal.clone()))
            })
            .collect()
    }

    /// Record a reading of session `intent`'s withdrawal account.
    async fn record_withdrawal(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        intent: &str,
        cash: u128,
        payment: PaymentReading,
    ) -> Result<(), FundingSessionError> {
        let intent = intent.to_string();
        self.commit(storage, now_ms, move |sessions| {
            let changed = sessions
                .get_mut(&intent)
                .is_some_and(|session| session.observe_withdrawal(cash, payment, now_ms));
            ((), if changed { vec![intent] } else { Vec::new() })
        })
        .await
    }
}

impl RuntimeServices {
    /// Give open outbound session `intent` a withdrawal account for
    /// `destination_id`, the next one whose account holds no CASH, and ask
    /// the host to have the user pay the session's amount into it. Returns
    /// the account once the user has decided.
    pub async fn assign_funding_withdrawal(
        self: &Arc<Self>,
        intent: &str,
        destination_id: &str,
        derive: impl Fn(u32) -> Result<[u8; 32], GenericError>,
    ) -> Result<[u8; 32], WithdrawError> {
        let registry = self.funding();
        let session = registry.get(intent).ok_or(WithdrawError::NotFound)?;
        if !session.awaits_withdrawal() {
            return Err(WithdrawError::NotAwaitingWithdrawal);
        }
        let conversion = registry
            .conversion
            .get()
            .ok_or(WithdrawError::ConversionUnavailable)?;
        if self.payment_platform().is_none() {
            return Err(WithdrawError::PaymentsUnavailable);
        }
        let chains = self
            .funding_chains(conversion.network, false)
            .await
            .map_err(|error| WithdrawError::Chain(GenericError {
                reason: error.to_string(),
            }))?;
        let storage = self.platform.as_ref();
        // getcash numbers withdrawals per destination under `wd:`, apart
        // from deposit sources.
        let counter = format!("wd:{destination_id}");
        for _ in 0..MAX_USED_ACCOUNTS {
            let number = registry.next_account_number(storage, &counter).await?;
            let account = derive(number).map_err(WithdrawError::Derive)?;
            let cash = within_chain_timeout(chains.landed(&account))
                .await
                .map_err(WithdrawError::Chain)?
                .map_err(|error| WithdrawError::Chain(GenericError {
                    reason: error.to_string(),
                }))?;
            if cash > 0 {
                continue;
            }
            let withdrawal = FundingWithdrawal {
                destination_id: destination_id.to_string(),
                number,
                account,
                attempt: 0,
                since_ms: current_unix_millis(),
                taken: false,
            };
            let owned = intent.to_string();
            let assigned = registry
                .commit(storage, current_unix_millis(), move |sessions| {
                    let assigned = sessions
                        .get_mut(&owned)
                        .is_some_and(|session| session.assign_withdrawal(withdrawal));
                    (assigned, if assigned { vec![owned] } else { Vec::new() })
                })
                .await?;
            if !assigned {
                return Err(WithdrawError::NotAwaitingWithdrawal);
            }
            // Watched before the request, which waits on the user, so the
            // payment window runs and CASH is seen whatever the host does.
            self.watch_funding_deposits();
            self.request_withdrawal_payment(intent).await?;
            return Ok(account);
        }
        Err(WithdrawError::AccountsInUse)
    }

    /// Ask the host to have the user pay session `intent`'s amount into its
    /// withdrawal account, under the current attempt's id. Resolves once the
    /// user has decided, which can take as long as the host's sheet stays
    /// up. A refusal ends the session for a retry under the next one.
    pub async fn request_withdrawal_payment(self: &Arc<Self>, intent: &str) -> Result<(), WithdrawError> {
        let registry = self.funding();
        let session = registry.get(intent).ok_or(WithdrawError::NotFound)?;
        let (Some(withdrawal), Some(amount)) = (session.withdrawal, session.amount) else {
            return Err(WithdrawError::NotAwaitingWithdrawal);
        };
        let conversion = registry
            .conversion
            .get()
            .ok_or(WithdrawError::ConversionUnavailable)?;
        let platform = self
            .payment_platform()
            .ok_or(WithdrawError::PaymentsUnavailable)?;
        let product = ProductContext {
            product_id: conversion.signer.funding_product_id(),
            execution_kind: Default::default(),
        };
        let request = HostPaymentRequest {
            from: None,
            amount,
            destination: withdrawal.account,
            id: funding_attempt_id(&withdrawal.account, withdrawal.attempt),
        };
        match platform.request_payment(&product, request).await {
            Ok(()) | Err(HostPaymentError::AlreadyExists) => Ok(()),
            Err(error) => {
                let message = error.to_string();
                let owned = intent.to_string();
                registry
                    .commit(self.platform.as_ref(), current_unix_millis(), move |sessions| {
                        let now_ms = current_unix_millis();
                        let changed = sessions
                            .get_mut(&owned)
                            .is_some_and(|session| session.refuse_payment(message, now_ms));
                        ((), if changed { vec![owned] } else { Vec::new() })
                    })
                    .await?;
                Err(WithdrawError::Refused(error))
            }
        }
    }

    /// Read `withdrawal`'s CASH on People and the host's word on its current
    /// payment, within `limit`.
    async fn read_withdrawal(
        self: &Arc<Self>,
        withdrawal: &FundingWithdrawal,
        limit: Duration,
    ) -> Result<(u128, PaymentReading), String> {
        let conversion = self
            .funding()
            .conversion
            .get()
            .ok_or_else(|| "this host does not convert funding".to_string())?;
        let product = ProductContext {
            product_id: conversion.signer.funding_product_id(),
            execution_kind: Default::default(),
        };
        let platform = self.payment_platform();
        within_timeout(limit, async {
            let chains = self
                .funding_chains(conversion.network, false)
                .await
                .map_err(|error| error.to_string())?;
            let cash = chains.landed(&withdrawal.account).await.map_err(|error| error.to_string())?;
            let word = match &platform {
                Some(platform) => {
                    let id = funding_attempt_id(&withdrawal.account, withdrawal.attempt);
                    let mut statuses = platform.subscribe_payment_status(&product, id);
                    match within_timeout(STATUS_TIMEOUT, statuses.next()).await {
                        Ok(Some(Ok(status))) => PaymentWord::Said(status),
                        Ok(Some(Err(HostPaymentStatusSubscribeError::PaymentNotFound))) => {
                            PaymentWord::NotFound
                        }
                        Ok(Some(Err(HostPaymentStatusSubscribeError::Unknown { .. })) | None) | Err(_) => {
                            PaymentWord::Unanswered
                        }
                    }
                }
                None => PaymentWord::Unanswered,
            };
            Ok((
                cash,
                PaymentReading {
                    attempt: withdrawal.attempt,
                    word,
                },
            ))
        })
        .await
        .map_err(|error| error.reason)
        .and_then(|read| read)
    }

    /// Read session `intent`'s withdrawal account and payment fresh, and
    /// record what they show.
    async fn refresh_withdrawal(
        self: &Arc<Self>,
        intent: &str,
        withdrawal: &FundingWithdrawal,
        limit: Duration,
    ) -> Result<(), String> {
        let (cash, payment) = self.read_withdrawal(withdrawal, limit).await?;
        self.funding()
            .record_withdrawal(self.platform.as_ref(), current_unix_millis(), intent, cash, payment)
            .await
            .map_err(|error| error.to_string())
    }

    /// Read session `intent`'s withdrawal account and payment fresh before a
    /// cancel, as getcash does, recording what they show: CASH on the
    /// account or a payment under way then refuses the cancel. Nothing is
    /// cancelled when either cannot be read in time.
    pub async fn confirm_payment_untaken(
        self: &Arc<Self>,
        intent: &str,
        withdrawal: &FundingWithdrawal,
    ) -> Result<(), CancelFundingError> {
        let (cash, payment) = self.read_withdrawal(withdrawal, CANCEL_CONFIRM).await.map_err(|reason| {
            tracing::warn!(%intent, %reason, "confirming a withdrawal cancel failed");
            CancelFundingError::Unconfirmed
        })?;
        // The host must have answered for the cancel to know the payment
        // was not taken.
        if payment.word == PaymentWord::Unanswered {
            return Err(CancelFundingError::Unconfirmed);
        }
        self.funding()
            .record_withdrawal(self.platform.as_ref(), current_unix_millis(), intent, cash, payment)
            .await?;
        Ok(())
    }

    /// One pass over the withdrawal accounts still read: their CASH on
    /// People and the host's word on each payment.
    pub async fn advance_withdrawals(self: &Arc<Self>) {
        for (intent, withdrawal) in self.funding().withdrawing_sessions(current_unix_millis()) {
            if let Err(reason) = self.refresh_withdrawal(&intent, &withdrawal, WATCH_READ).await {
                tracing::warn!(%intent, %reason, "reading a withdrawal account failed");
            }
        }
    }
}
