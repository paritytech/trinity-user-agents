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

use super::conversion::{
    ConversionChains, ConversionError, PreparedWithdrawal, WithdrawCall, WithdrawChains,
    landing_floor,
};
use super::{
    FundingSigner, STALL_AFTER_MS, SignOn, CANCEL_CONFIRM, CancelFundingError, FundingRegistry, MAX_USED_ACCOUNTS, within_chain_timeout,
    within_timeout,
};
use crate::host_logic::funding::{
    FundingSessionError, FundingWithdrawal, PaymentReading, PaymentWord, WithdrawStep,
    WithdrawSubmission, funding_attempt_id,
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

/// What a pass decided for one withdrawal being moved to Asset Hub.
#[derive(Debug, PartialEq, Eq)]
enum PlannedWithdrawal {
    /// Record a step.
    Record(WithdrawStep),
    /// Record the submission, then submit `extrinsic` on People.
    Submit {
        submission: WithdrawSubmission,
        extrinsic: Vec<u8>,
    },
}

/// Decide the next step for `withdrawal`, given the transaction on its way,
/// as getcash's withdrawal tick does, from balances alone. A submitted XCM
/// has landed once the account holds no CASH and Asset Hub shows the PAS;
/// one included that left the CASH failed; one whose era passed unincluded
/// is dropped; one that took the CASH but landed nothing in time stalled. A
/// swap went through once it left PAS on the account. With nothing on its
/// way, the account's next transaction is prepared: the XCM when it holds
/// the PAS its fees need, the swap otherwise.
async fn plan_withdrawal(
    chains: &dyn WithdrawChains,
    signer: &dyn FundingSigner,
    withdrawal: &FundingWithdrawal,
    submission: Option<WithdrawSubmission>,
    now_ms: u64,
) -> Result<Option<PlannedWithdrawal>, ConversionError> {
    let account = &withdrawal.account;
    let (cash, pas) = chains.people_holdings(account).await?;
    let record = |step| Ok(Some(PlannedWithdrawal::Record(step)));
    match submission {
        Some(WithdrawSubmission::Transfer {
            nonce,
            valid_until_block,
            submitted_at_ms,
            landing_before,
            expected_landing,
        }) => {
            let landed = chains.asset_hub_native(account).await?.saturating_sub(landing_before);
            if cash == 0 && landed >= landing_floor(expected_landing) {
                return record(WithdrawStep::Landed { landed });
            }
            if chains.people_nonce(account).await? > nonce {
                if cash > 0 {
                    return record(WithdrawStep::Rejected {
                        reason: "the withdrawal was included and failed".into(),
                    });
                }
                if now_ms.saturating_sub(submitted_at_ms) > STALL_AFTER_MS {
                    // What did land is the user's: pay it out rather than
                    // fail, where getcash holds the run until its bound.
                    return record(match landed {
                        0 => WithdrawStep::Stalled,
                        landed => WithdrawStep::Landed { landed },
                    });
                }
                return Ok(None);
            }
            if chains.people_block() > valid_until_block {
                return record(WithdrawStep::Dropped);
            }
            return Ok(None);
        }
        Some(WithdrawSubmission::Swap {
            nonce,
            valid_until_block,
            pas_before,
        }) => {
            if chains.people_nonce(account).await? > nonce {
                return record(if pas > pas_before {
                    WithdrawStep::Swapped
                } else {
                    WithdrawStep::Rejected {
                        reason: "the swap for the withdrawal's fees was included and failed".into(),
                    }
                });
            }
            if chains.people_block() > valid_until_block {
                return record(WithdrawStep::Dropped);
            }
            return Ok(None);
        }
        None => {}
    }
    if cash == 0 {
        return Ok(None);
    }
    let keypair = signer
        .withdrawal_keypair(&withdrawal.destination_id, withdrawal.number)
        .map_err(|error| ConversionError::Chain(error.reason))?;
    let Some(keypair) = keypair.filter(|keypair| keypair.public.to_bytes() == *account) else {
        return Ok(None);
    };
    let nonce = chains.people_nonce(account).await?;
    let PreparedWithdrawal {
        extrinsic,
        valid_until_block,
        call,
    } = chains.prepare_withdrawal(&keypair, nonce, *account).await?;
    let submission = match call {
        WithdrawCall::Swap => WithdrawSubmission::Swap {
            nonce,
            valid_until_block,
            pas_before: pas,
        },
        WithdrawCall::Transfer { expected_landing } => WithdrawSubmission::Transfer {
            nonce,
            valid_until_block,
            submitted_at_ms: now_ms,
            landing_before: chains.asset_hub_native(account).await?,
            expected_landing,
        },
    };
    Ok(Some(PlannedWithdrawal::Submit {
        submission,
        extrinsic,
    }))
}

impl FundingRegistry {
    /// Every session whose CASH is being moved to Asset Hub, with its
    /// withdrawal account and the transaction on its way.
    fn moving_withdrawals(&self) -> Vec<(String, FundingWithdrawal, Option<WithdrawSubmission>)> {
        self.lock_sessions()
            .values()
            .filter_map(|session| {
                let (withdrawal, submission) = session.withdrawing()?;
                Some((session.intent.clone(), withdrawal.clone(), submission))
            })
            .collect()
    }

    /// Apply one step of a withdrawal's move to session `intent`.
    async fn record_withdraw_step(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        intent: &str,
        step: WithdrawStep,
    ) -> Result<(), FundingSessionError> {
        let intent = intent.to_string();
        self.commit(storage, now_ms, move |sessions| {
            let changed = sessions
                .get_mut(&intent)
                .is_some_and(|session| session.advance_withdrawal(step, now_ms));
            ((), if changed { vec![intent] } else { Vec::new() })
        })
        .await
    }

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
            .funding_chains(conversion.network, SignOn::Nowhere)
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
                .funding_chains(conversion.network, SignOn::Nowhere)
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

    /// One pass over the withdrawals being moved to Asset Hub: record what
    /// landed, was swapped, dropped or rejected, and submit what is ready.
    pub async fn advance_withdrawal_moves(self: &Arc<Self>) -> Result<(), String> {
        let registry = self.funding();
        let moving = registry.moving_withdrawals();
        let Some(conversion) = registry.conversion.get() else {
            return Ok(());
        };
        if moving.is_empty() {
            return Ok(());
        }
        let signing = moving.iter().any(|(_, _, submission)| submission.is_none());
        let chains = self
            .funding_chains(conversion.network, if signing { SignOn::People } else { SignOn::Nowhere })
            .await
            .map_err(|error| error.to_string())?;
        let storage = self.platform.as_ref();
        for (intent, withdrawal, submission) in moving {
            let now_ms = current_unix_millis();
            let planned = within_chain_timeout(plan_withdrawal(
                &chains,
                conversion.signer.as_ref(),
                &withdrawal,
                submission,
                now_ms,
            ))
            .await;
            let planned = match planned {
                Ok(Ok(planned)) => planned,
                Ok(Err(ConversionError::Refused(reason) | ConversionError::PsmRefused { reason, .. })) => {
                    Some(PlannedWithdrawal::Record(WithdrawStep::Refused { reason }))
                }
                Ok(Err(ConversionError::Chain(reason))) | Err(GenericError { reason }) => {
                    tracing::warn!(%intent, %reason, "funding withdrawal pass failed");
                    continue;
                }
            };
            match planned {
                None => {}
                Some(PlannedWithdrawal::Record(step)) => {
                    if let Err(error) = registry.record_withdraw_step(storage, now_ms, &intent, step).await {
                        tracing::warn!(%intent, %error, "recording a funding withdrawal failed");
                    }
                }
                Some(PlannedWithdrawal::Submit {
                    submission,
                    extrinsic,
                }) => {
                    let submitted = WithdrawStep::Submitted(submission);
                    // Never submitted unless recorded, so a lost answer is
                    // still judged by its nonce.
                    if let Err(error) = registry.record_withdraw_step(storage, now_ms, &intent, submitted).await {
                        tracing::warn!(%intent, %error, "recording a funding withdrawal failed");
                        continue;
                    }
                    match chains.submit_on_people(extrinsic).await {
                        Ok(()) => {}
                        Err(ConversionError::Refused(reason) | ConversionError::PsmRefused { reason, .. }) => {
                            let refused = WithdrawStep::Refused { reason };
                            if let Err(error) = registry
                                .record_withdraw_step(storage, current_unix_millis(), &intent, refused)
                                .await
                            {
                                tracing::warn!(%intent, %error, "recording a funding withdrawal failed");
                            }
                        }
                        // It may have reached the chain anyway; its era or
                        // the nonce decides.
                        Err(ConversionError::Chain(reason)) => {
                            tracing::warn!(%intent, %reason, "submitting a funding withdrawal failed");
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use futures::future::BoxFuture;

    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    /// Chains answering fixed reads and preparing a fixed transaction.
    struct Scripted {
        cash: u128,
        pas: u128,
        nonce: u32,
        block: u64,
        landing: u128,
        prepares: WithdrawCall,
    }

    impl WithdrawChains for Scripted {
        fn people_holdings<'a>(&'a self, _: &'a [u8; 32]) -> BoxFuture<'a, Result<(u128, u128), ConversionError>> {
            Box::pin(async { Ok((self.cash, self.pas)) })
        }

        fn people_nonce<'a>(&'a self, _: &'a [u8; 32]) -> BoxFuture<'a, Result<u32, ConversionError>> {
            Box::pin(async { Ok(self.nonce) })
        }

        fn people_block(&self) -> u64 {
            self.block
        }

        fn asset_hub_native<'a>(&'a self, _: &'a [u8; 32]) -> BoxFuture<'a, Result<u128, ConversionError>> {
            Box::pin(async { Ok(self.landing) })
        }

        fn prepare_withdrawal<'a>(
            &'a self,
            _: &'a schnorrkel::Keypair,
            _: u32,
            _: [u8; 32],
        ) -> BoxFuture<'a, Result<PreparedWithdrawal, ConversionError>> {
            Box::pin(async {
                Ok(PreparedWithdrawal {
                    extrinsic: vec![1, 2, 3],
                    valid_until_block: 164,
                    call: self.prepares,
                })
            })
        }
    }

    struct Keys(schnorrkel::Keypair);

    impl FundingSigner for Keys {
        fn deposit_keypair(&self, _: &str, _: u32) -> Result<Option<schnorrkel::Keypair>, GenericError> {
            Ok(None)
        }

        fn withdrawal_keypair(&self, _: &str, _: u32) -> Result<Option<schnorrkel::Keypair>, GenericError> {
            Ok(Some(self.0.clone()))
        }

        fn funding_product_id(&self) -> String {
            "fund.dot".into()
        }
    }

    fn keypair(seed: u8) -> schnorrkel::Keypair {
        schnorrkel::MiniSecretKey::from_bytes(&[seed; 32])
            .expect("seed")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
    }

    fn withdrawal() -> FundingWithdrawal {
        FundingWithdrawal {
            destination_id: "dot-assethub".into(),
            number: 1,
            account: keypair(1).public.to_bytes(),
            attempt: 0,
            since_ms: NOW,
            taken: true,
        }
    }

    fn chains(cash: u128, pas: u128, nonce: u32, block: u64, landing: u128) -> Scripted {
        Scripted {
            cash,
            pas,
            nonce,
            block,
            landing,
            prepares: WithdrawCall::Swap,
        }
    }

    fn plan(chains: &Scripted, submission: Option<WithdrawSubmission>, now_ms: u64) -> Option<PlannedWithdrawal> {
        block_on(plan_withdrawal(chains, &Keys(keypair(1)), &withdrawal(), submission, now_ms)).expect("planned")
    }

    const SWAP: WithdrawSubmission = WithdrawSubmission::Swap {
        nonce: 4,
        valid_until_block: 164,
        pas_before: 30,
    };
    const TRANSFER: WithdrawSubmission = WithdrawSubmission::Transfer {
        nonce: 5,
        valid_until_block: 164,
        submitted_at_ms: NOW,
        landing_before: 10,
        expected_landing: 1_000,
    };

    fn record(step: WithdrawStep) -> Option<PlannedWithdrawal> {
        Some(PlannedWithdrawal::Record(step))
    }

    // With nothing on its way, the account's next transaction goes out, as
    // getcash's tick acts once per reading: the XCM is measured from where
    // the landing account stood just before, so its arrival can be told.
    #[test]
    fn the_next_withdrawal_transaction_is_prepared_from_what_the_account_holds() {
        let transfer = Scripted {
            prepares: WithdrawCall::Transfer { expected_landing: 900 },
            ..chains(500, 30, 6, 100, 7)
        };

        assert_eq!(
            [
                plan(&chains(0, 0, 4, 100, 0), None, NOW),
                plan(&chains(500, 0, 4, 100, 0), None, NOW),
                plan(&transfer, None, NOW),
            ],
            [
                None,
                Some(PlannedWithdrawal::Submit {
                    submission: WithdrawSubmission::Swap {
                        nonce: 4,
                        valid_until_block: 164,
                        pas_before: 0,
                    },
                    extrinsic: vec![1, 2, 3],
                }),
                Some(PlannedWithdrawal::Submit {
                    submission: WithdrawSubmission::Transfer {
                        nonce: 6,
                        valid_until_block: 164,
                        submitted_at_ms: NOW,
                        landing_before: 7,
                        expected_landing: 900,
                    },
                    extrinsic: vec![1, 2, 3],
                }),
            ]
        );
    }

    // A swap is judged by what it added: more PAS on the account than
    // before means it went through, the same after inclusion means it failed
    // and cost a fee, even when PAS from an earlier swap is there; one whose
    // era passed unincluded is dropped; until then it is waited for.
    #[test]
    fn a_swap_is_judged_by_the_pas_it_added() {
        let failed = || {
            record(WithdrawStep::Rejected {
                reason: "the swap for the withdrawal's fees was included and failed".into(),
            })
        };

        assert_eq!(
            [
                plan(&chains(400, 60, 5, 100, 0), Some(SWAP), NOW),
                plan(&chains(500, 30, 5, 100, 0), Some(SWAP), NOW),
                plan(&chains(500, 0, 5, 100, 0), Some(SWAP), NOW),
                plan(&chains(500, 30, 4, 165, 0), Some(SWAP), NOW),
                plan(&chains(500, 30, 4, 164, 0), Some(SWAP), NOW),
            ],
            [record(WithdrawStep::Swapped), failed(), failed(), record(WithdrawStep::Dropped), None]
        );
    }

    // getcash counts an arrival only on both signals: the account holds no
    // CASH, which only the XCM takes in full, and the landing account gained
    // at least what a sale within the slippage lands. An XCM included with
    // the CASH still there failed; one that took it and landed nothing in
    // time stalled, and what did land is paid out rather than lost.
    #[test]
    fn a_transfer_lands_on_both_signals() {
        let late = NOW + STALL_AFTER_MS + 1;

        assert_eq!(
            [
                plan(&chains(0, 0, 6, 100, 960), Some(TRANSFER), NOW),
                plan(&chains(0, 0, 6, 100, 950), Some(TRANSFER), NOW),
                plan(&chains(500, 0, 6, 100, 0), Some(TRANSFER), NOW),
                plan(&chains(0, 0, 6, 100, 10), Some(TRANSFER), late),
                plan(&chains(0, 0, 6, 100, 410), Some(TRANSFER), late),
                plan(&chains(500, 30, 5, 165, 0), Some(TRANSFER), NOW),
            ],
            [
                record(WithdrawStep::Landed { landed: 950 }),
                None,
                record(WithdrawStep::Rejected {
                    reason: "the withdrawal was included and failed".into(),
                }),
                record(WithdrawStep::Stalled),
                record(WithdrawStep::Landed { landed: 400 }),
                record(WithdrawStep::Dropped),
            ]
        );
    }

    // A withdrawal outlives a sign-out; another identity's key must not
    // sign for this account.
    #[test]
    fn only_the_withdrawal_accounts_key_signs() {
        let wrong = block_on(plan_withdrawal(
            &chains(500, 0, 4, 100, 0),
            &Keys(keypair(2)),
            &withdrawal(),
            None,
            NOW,
        ))
        .expect("planned");

        assert_eq!(wrong, None);
    }
}
