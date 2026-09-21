// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use std::collections::{HashMap, HashSet};
use {parking_lot::Mutex, std::sync::Arc};

use async_trait::async_trait;
use tokio::sync::watch;
use tracing::warn;

use crate::claim_plan::{ClaimPlan, ClaimPlanStatus, ClaimPlanStore, CodableClaimPlanEntry};
use crate::constants::SEND_VERIFY_BLOCK_TIMEOUT;
use crate::denomination::DenominationBreakdownContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimStatus {
    /// Waiting for the send to appear on-chain.
    Detecting,
    /// Coins confirmed on-chain, awaiting claim.
    Sent,
    /// Claim extrinsic in flight.
    Claiming,
    Finished {
        claimed_amount: u128,
    },
    Error,
}

impl ClaimStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, ClaimStatus::Finished { .. } | ClaimStatus::Error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendConfirmation {
    OnChain,
    AlreadyClaimed,
}

#[async_trait]
pub trait TransferSendVerifying: Send + Sync {
    /// Resolves when the memo's coins are on-chain; errors on timeout.
    async fn await_send_on_chain(
        &self,
        memo_key: &[u8; 32],
        block_timeout: u32,
    ) -> Result<(), String>;

    /// Resolves when the memo's coins have left the chain (claimed).
    async fn await_claim_on_chain(
        &self,
        memo_key: &[u8; 32],
        block_timeout: u32,
    ) -> Result<(), String>;

    async fn await_send_or_claimed(
        &self,
        memo_key: &[u8; 32],
        block_timeout: u32,
    ) -> Result<SendConfirmation, String>;
}

#[async_trait]
pub trait ClaimExecutor: Send + Sync {
    async fn claim(
        &self,
        memo_key: &[u8; 32],
        message_id: &str,
    ) -> Result<Vec<CodableClaimPlanEntry>, String>;
}

pub fn claimed_amount_from_plan(plan: &ClaimPlan, context: &DenominationBreakdownContext) -> u128 {
    if plan.entries.is_empty() {
        return plan.total_value;
    }
    plan.entries.iter().fold(0u128, |acc, entry| {
        acc.saturating_add(context.value_in_planks(entry.exponent))
    })
}

#[derive(Default)]
pub struct ClaimStatusStore {
    subjects: Mutex<HashMap<String, watch::Sender<ClaimStatus>>>,
    terminal: Mutex<HashMap<String, ClaimStatus>>,
}

impl ClaimStatusStore {
    /// Emits the current status immediately (the watch receiver's seed
    /// value), then streams updates for live claims. For a terminal
    /// message the receiver completes right after that seed read.
    /// Returns `None` for a message id that was never seen.
    pub fn watch_status(&self, message_id: &str) -> Option<watch::Receiver<ClaimStatus>> {
        if let Some(sender) = self.subjects.lock().get(message_id) {
            return Some(sender.subscribe());
        }
        let terminal = self.terminal.lock();
        let status = terminal.get(message_id)?;
        // One-shot: seed a fresh channel and drop the sender so
        // `changed` completes immediately after the seed read.
        let (tx, rx) = watch::channel(status.clone());
        drop(tx);
        Some(rx)
    }

    /// Current status snapshot.
    pub fn status(&self, message_id: &str) -> Option<ClaimStatus> {
        if let Some(sender) = self.subjects.lock().get(message_id) {
            return Some(sender.borrow().clone());
        }
        self.terminal.lock().get(message_id).cloned()
    }

    pub fn update_status(&self, message_id: &str, status: ClaimStatus) {
        let mut subjects = self.subjects.lock();
        if status.is_terminal() {
            if let Some(sender) = subjects.remove(message_id) {
                let _ = sender.send(status.clone());
                // Sender drops here → subscriber streams close.
            }
            self.terminal.lock().insert(message_id.to_string(), status);
            return;
        }
        match subjects.get(message_id) {
            Some(sender) => {
                let _ = sender.send(status);
            }
            None => {
                let (sender, _) = watch::channel(status);
                subjects.insert(message_id.to_string(), sender);
            }
        }
    }
}

pub fn restore_persisted_statuses(plans: &[ClaimPlan], store: &ClaimStatusStore) {
    for plan in plans {
        let Some(message_id) = &plan.message_id else {
            continue;
        };
        let status = match plan.status {
            ClaimPlanStatus::Processing => ClaimStatus::Detecting,
            ClaimPlanStatus::Detected => ClaimStatus::Sent,
            ClaimPlanStatus::Finished => ClaimStatus::Finished {
                claimed_amount: plan.claimed_amount.unwrap_or(plan.total_value),
            },
            ClaimPlanStatus::Error => ClaimStatus::Error,
        };
        store.update_status(message_id, status);
    }
}

/// Claim orchestration errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimError {
    AlreadyClaiming,
    Failed(String),
}

impl std::fmt::Display for ClaimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaimError::AlreadyClaiming => write!(f, "already claiming this memo"),
            ClaimError::Failed(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ClaimError {}

/// One incoming `coinageSend` to claim.
#[derive(Debug, Clone)]
pub struct IncomingClaim {
    pub memo_key: [u8; 32],
    pub message_id: String,
    pub total_value: u128,
}

pub struct ClaimOrchestrator {
    plans: Arc<dyn ClaimPlanStore>,
    verifier: Arc<dyn TransferSendVerifying>,
    executor: Arc<dyn ClaimExecutor>,
    statuses: Arc<ClaimStatusStore>,
    context: DenominationBreakdownContext,
    claiming_memos: Mutex<HashSet<[u8; 32]>>,
}

impl ClaimOrchestrator {
    pub fn new(
        plans: Arc<dyn ClaimPlanStore>,
        verifier: Arc<dyn TransferSendVerifying>,
        executor: Arc<dyn ClaimExecutor>,
        statuses: Arc<ClaimStatusStore>,
        context: DenominationBreakdownContext,
    ) -> Self {
        Self {
            plans,
            verifier,
            executor,
            statuses,
            context,
            claiming_memos: Mutex::new(HashSet::new()),
        }
    }

    pub async fn restore_persisted_statuses(&self) -> Result<(), String> {
        let plans = self.plans.load_all().await?;
        restore_persisted_statuses(&plans, &self.statuses);
        Ok(())
    }

    /// Claims one incoming memo; resolves the claimed planks.
    pub async fn claim_incoming(&self, incoming: IncomingClaim) -> Result<u128, ClaimError> {
        {
            let mut claiming = self.claiming_memos.lock();
            if !claiming.insert(incoming.memo_key) {
                return Err(ClaimError::AlreadyClaiming);
            }
        }
        let _guard = ClaimingGuard {
            memo_key: incoming.memo_key,
            claiming: &self.claiming_memos,
        };

        let outcome = self.claim_inner(&incoming).await;
        if let Err(ClaimError::Failed(message)) = &outcome {
            warn!(message, message_id = incoming.message_id, "claim failed");
            if let Err(error) = self
                .plans
                .update_status(&incoming.memo_key, ClaimPlanStatus::Error, None)
                .await
            {
                warn!(error, "claim error status stamp failed");
            }
            self.statuses
                .update_status(&incoming.message_id, ClaimStatus::Error);
        }
        outcome
    }

    async fn claim_inner(&self, incoming: &IncomingClaim) -> Result<u128, ClaimError> {
        let existing = self
            .plans
            .plan(&incoming.memo_key)
            .await
            .map_err(ClaimError::Failed)?;

        match &existing {
            Some(plan) if plan.status == ClaimPlanStatus::Finished => {
                let claimed = plan
                    .claimed_amount
                    .unwrap_or_else(|| claimed_amount_from_plan(plan, &self.context));
                self.statuses.update_status(
                    &incoming.message_id,
                    ClaimStatus::Finished {
                        claimed_amount: claimed,
                    },
                );
                return Ok(claimed);
            }
            Some(_) => {}
            None => {
                self.statuses
                    .update_status(&incoming.message_id, ClaimStatus::Detecting);
                self.verifier
                    .await_send_on_chain(&incoming.memo_key, SEND_VERIFY_BLOCK_TIMEOUT)
                    .await
                    .map_err(ClaimError::Failed)?;
                self.plans
                    .save(&ClaimPlan {
                        memo_key: incoming.memo_key,
                        message_id: Some(incoming.message_id.clone()),
                        entries: Vec::new(),
                        outgoing_public_keys: Vec::new(),
                        detection_anchor: None,
                        status: ClaimPlanStatus::Processing,
                        claimed_amount: None,
                        total_value: incoming.total_value,
                    })
                    .await
                    .map_err(ClaimError::Failed)?;
            }
        }

        self.statuses
            .update_status(&incoming.message_id, ClaimStatus::Claiming);
        let entries = self
            .executor
            .claim(&incoming.memo_key, &incoming.message_id)
            .await
            .map_err(ClaimError::Failed)?;

        let finished = ClaimPlan {
            memo_key: incoming.memo_key,
            message_id: Some(incoming.message_id.clone()),
            entries,
            outgoing_public_keys: Vec::new(),
            detection_anchor: None,
            status: ClaimPlanStatus::Finished,
            claimed_amount: None,
            total_value: incoming.total_value,
        };
        let claimed = claimed_amount_from_plan(&finished, &self.context);
        self.plans
            .update_status(&incoming.memo_key, ClaimPlanStatus::Finished, Some(claimed))
            .await
            .map_err(ClaimError::Failed)?;
        self.statuses.update_status(
            &incoming.message_id,
            ClaimStatus::Finished {
                claimed_amount: claimed,
            },
        );
        Ok(claimed)
    }
}

struct ClaimingGuard<'a> {
    memo_key: [u8; 32],
    claiming: &'a Mutex<HashSet<[u8; 32]>>,
}

impl Drop for ClaimingGuard<'_> {
    fn drop(&mut self) {
        self.claiming.lock().remove(&self.memo_key);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn ctx() -> DenominationBreakdownContext {
        DenominationBreakdownContext {
            asset_unit: 10,
            max_exponent: 4,
            min_exponent: 0,
            precision: 10,
        }
    }

    fn entry(exponent: i16) -> CodableClaimPlanEntry {
        CodableClaimPlanEntry {
            entry_index: 0,
            exponent,
            derivation_index: 1,
        }
    }

    fn plan(entries: Vec<CodableClaimPlanEntry>, status: ClaimPlanStatus) -> ClaimPlan {
        ClaimPlan {
            memo_key: [7; 32],
            message_id: Some("m1".into()),
            entries,
            outgoing_public_keys: Vec::new(),
            detection_anchor: None,
            status,
            claimed_amount: None,
            total_value: 990,
        }
    }

    #[test]
    fn claimed_amount_sums_entry_denominations() {
        let plan = plan(
            vec![entry(4), entry(2), entry(0)],
            ClaimPlanStatus::Finished,
        );
        assert_eq!(claimed_amount_from_plan(&plan, &ctx()), 210);
    }

    #[test]
    fn claimed_amount_falls_back_to_total_value() {
        let plan = plan(Vec::new(), ClaimPlanStatus::Finished);
        assert_eq!(claimed_amount_from_plan(&plan, &ctx()), 990);
    }

    #[tokio::test]
    async fn watch_emits_current_then_streams_updates() {
        let store = ClaimStatusStore::default();
        store.update_status("m1", ClaimStatus::Detecting);
        let mut rx = store.watch_status("m1").unwrap();
        assert_eq!(
            *rx.borrow(),
            ClaimStatus::Detecting,
            "immediate current value"
        );
        store.update_status("m1", ClaimStatus::Claiming);
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), ClaimStatus::Claiming);
    }

    #[tokio::test]
    async fn terminal_status_closes_and_cleans_up() {
        let store = ClaimStatusStore::default();
        store.update_status("m1", ClaimStatus::Claiming);
        let mut live = store.watch_status("m1").unwrap();

        store.update_status("m1", ClaimStatus::Finished { claimed_amount: 5 });
        live.changed().await.unwrap();
        assert_eq!(*live.borrow(), ClaimStatus::Finished { claimed_amount: 5 });
        assert!(
            live.changed().await.is_err(),
            "stream completes after terminal"
        );
        assert!(
            store.subjects.lock().is_empty(),
            "subject removed after terminal send"
        );

        // Late watcher: value once, then complete.
        let mut late = store.watch_status("m1").unwrap();
        assert_eq!(*late.borrow(), ClaimStatus::Finished { claimed_amount: 5 });
        assert!(late.changed().await.is_err());
        assert!(store.watch_status("unknown").is_none());
    }

    #[test]
    fn restore_maps_plan_statuses_to_claim_statuses() {
        let store = ClaimStatusStore::default();
        let mut processing = plan(Vec::new(), ClaimPlanStatus::Processing);
        processing.message_id = Some("p".into());
        let mut detected = plan(Vec::new(), ClaimPlanStatus::Detected);
        detected.message_id = Some("d".into());
        let mut finished = plan(Vec::new(), ClaimPlanStatus::Finished);
        finished.message_id = Some("f".into());
        finished.claimed_amount = Some(123);
        let mut finished_no_amount = plan(Vec::new(), ClaimPlanStatus::Finished);
        finished_no_amount.message_id = Some("f2".into());
        let mut errored = plan(Vec::new(), ClaimPlanStatus::Error);
        errored.message_id = Some("e".into());
        let mut no_message = plan(Vec::new(), ClaimPlanStatus::Processing);
        no_message.message_id = None;

        restore_persisted_statuses(
            &[
                processing,
                detected,
                finished,
                finished_no_amount,
                errored,
                no_message,
            ],
            &store,
        );
        assert_eq!(store.status("p"), Some(ClaimStatus::Detecting));
        assert_eq!(store.status("d"), Some(ClaimStatus::Sent));
        assert_eq!(
            store.status("f"),
            Some(ClaimStatus::Finished {
                claimed_amount: 123
            })
        );
        assert_eq!(
            store.status("f2"),
            Some(ClaimStatus::Finished {
                claimed_amount: 990
            }),
            "missing claimed_amount falls back to total_value"
        );
        assert_eq!(store.status("e"), Some(ClaimStatus::Error));
    }

    // — orchestrator plumbing —

    #[derive(Default)]
    struct MemPlans {
        plans: Mutex<HashMap<[u8; 32], ClaimPlan>>,
        saves: AtomicUsize,
    }

    #[async_trait]
    impl ClaimPlanStore for MemPlans {
        async fn save(&self, plan: &ClaimPlan) -> Result<(), String> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            self.plans.lock().insert(plan.memo_key, plan.clone());
            Ok(())
        }
        async fn plan(&self, memo_key: &[u8; 32]) -> Result<Option<ClaimPlan>, String> {
            Ok(self.plans.lock().get(memo_key).cloned())
        }
        async fn load_all(&self) -> Result<Vec<ClaimPlan>, String> {
            Ok(self.plans.lock().values().cloned().collect())
        }
        async fn update_status(
            &self,
            memo_key: &[u8; 32],
            status: ClaimPlanStatus,
            claimed_amount: Option<u128>,
        ) -> Result<(), String> {
            let mut plans = self.plans.lock();
            let plan = plans.get_mut(memo_key).ok_or("plan not found")?;
            plan.status = status;
            plan.claimed_amount = claimed_amount;
            Ok(())
        }
        async fn remove(&self, memo_key: &[u8; 32]) -> Result<(), String> {
            self.plans.lock().remove(memo_key);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockVerifier {
        send_awaits: AtomicUsize,
    }

    #[async_trait]
    impl TransferSendVerifying for MockVerifier {
        async fn await_send_on_chain(
            &self,
            _memo_key: &[u8; 32],
            block_timeout: u32,
        ) -> Result<(), String> {
            assert_eq!(block_timeout, 100, "COINM-024: 100-block timeout");
            self.send_awaits.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn await_claim_on_chain(
            &self,
            _memo_key: &[u8; 32],
            _block_timeout: u32,
        ) -> Result<(), String> {
            Ok(())
        }
        async fn await_send_or_claimed(
            &self,
            _memo_key: &[u8; 32],
            _block_timeout: u32,
        ) -> Result<SendConfirmation, String> {
            Ok(SendConfirmation::OnChain)
        }
    }

    struct MockExecutor {
        plans: Arc<MemPlans>,
        saves_seen_at_claim: AtomicUsize,
        calls: AtomicUsize,
        outcome: Result<Vec<CodableClaimPlanEntry>, String>,
        release: Option<watch::Receiver<bool>>,
    }

    #[async_trait]
    impl ClaimExecutor for MockExecutor {
        async fn claim(
            &self,
            memo_key: &[u8; 32],
            _message_id: &str,
        ) -> Result<Vec<CodableClaimPlanEntry>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.saves_seen_at_claim
                .store(self.plans.saves.load(Ordering::SeqCst), Ordering::SeqCst);
            assert!(
                self.plans.plan(memo_key).await.unwrap().is_some(),
                "plan must be persisted before the claim executor runs (COINA-016)"
            );
            if let Some(release) = &self.release {
                let mut release = release.clone();
                while !*release.borrow() {
                    if release.changed().await.is_err() {
                        break;
                    }
                }
            }
            self.outcome.clone()
        }
    }

    fn orchestrator(
        plans: Arc<MemPlans>,
        verifier: Arc<MockVerifier>,
        executor: Arc<MockExecutor>,
    ) -> ClaimOrchestrator {
        ClaimOrchestrator::new(
            plans,
            verifier,
            executor,
            Arc::new(ClaimStatusStore::default()),
            ctx(),
        )
    }

    fn incoming() -> IncomingClaim {
        IncomingClaim {
            memo_key: [7; 32],
            message_id: "m1".into(),
            total_value: 990,
        }
    }

    #[tokio::test]
    async fn fresh_claim_saves_plan_before_submitting() {
        let plans = Arc::new(MemPlans::default());
        let verifier = Arc::new(MockVerifier::default());
        let executor = Arc::new(MockExecutor {
            plans: Arc::clone(&plans),
            saves_seen_at_claim: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            outcome: Ok(vec![entry(2), entry(0)]), // 40 + 10
            release: None,
        });
        let orchestrator = orchestrator(
            Arc::clone(&plans),
            Arc::clone(&verifier),
            Arc::clone(&executor),
        );

        let claimed = orchestrator.claim_incoming(incoming()).await.unwrap();
        assert_eq!(claimed, 50);
        assert_eq!(verifier.send_awaits.load(Ordering::SeqCst), 1);
        assert!(
            executor.saves_seen_at_claim.load(Ordering::SeqCst) >= 1,
            "save-before-submit (COINA-016)"
        );
        let stored = plans.plan(&[7; 32]).await.unwrap().unwrap();
        assert_eq!(stored.status, ClaimPlanStatus::Finished);
        assert_eq!(stored.claimed_amount, Some(50));
        assert!(
            stored.entries.is_empty(),
            "finish is the status-only path — entries_data untouched (COINM-006)"
        );
        assert_eq!(
            orchestrator.statuses.status("m1"),
            Some(ClaimStatus::Finished { claimed_amount: 50 })
        );
    }

    #[tokio::test]
    async fn existing_plan_short_circuits_the_send_await() {
        let plans = Arc::new(MemPlans::default());
        plans
            .save(&plan(Vec::new(), ClaimPlanStatus::Processing))
            .await
            .unwrap();
        let verifier = Arc::new(MockVerifier::default());
        let executor = Arc::new(MockExecutor {
            plans: Arc::clone(&plans),
            saves_seen_at_claim: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            outcome: Ok(vec![entry(0)]),
            release: None,
        });
        let orchestrator = orchestrator(
            Arc::clone(&plans),
            Arc::clone(&verifier),
            Arc::clone(&executor),
        );

        let claimed = orchestrator.claim_incoming(incoming()).await.unwrap();
        assert_eq!(claimed, 10);
        assert_eq!(
            verifier.send_awaits.load(Ordering::SeqCst),
            0,
            "crash-recovery safety: input coins may already be spent (COINM-022)"
        );
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1, "claim still runs");
    }

    #[tokio::test]
    async fn finished_plan_is_reported_without_reclaiming() {
        let plans = Arc::new(MemPlans::default());
        let mut finished = plan(Vec::new(), ClaimPlanStatus::Finished);
        finished.claimed_amount = Some(321);
        plans.save(&finished).await.unwrap();
        let verifier = Arc::new(MockVerifier::default());
        let executor = Arc::new(MockExecutor {
            plans: Arc::clone(&plans),
            saves_seen_at_claim: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            outcome: Ok(Vec::new()),
            release: None,
        });
        let orchestrator = orchestrator(plans, verifier.clone(), Arc::clone(&executor));

        assert_eq!(orchestrator.claim_incoming(incoming()).await.unwrap(), 321);
        assert_eq!(verifier.send_awaits.load(Ordering::SeqCst), 0);
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn concurrent_claim_for_the_same_memo_is_rejected() {
        let (release_tx, release_rx) = watch::channel(false);
        let plans = Arc::new(MemPlans::default());
        plans
            .save(&plan(Vec::new(), ClaimPlanStatus::Processing))
            .await
            .unwrap();
        let executor = Arc::new(MockExecutor {
            plans: Arc::clone(&plans),
            saves_seen_at_claim: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            outcome: Ok(Vec::new()),
            release: Some(release_rx),
        });
        let orchestrator = Arc::new(orchestrator(
            plans,
            Arc::new(MockVerifier::default()),
            executor,
        ));

        let first = tokio::spawn({
            let orchestrator = Arc::clone(&orchestrator);
            async move { orchestrator.claim_incoming(incoming()).await }
        });
        // Let the first claim reach the stalled executor.
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            orchestrator.claim_incoming(incoming()).await,
            Err(ClaimError::AlreadyClaiming)
        );
        release_tx.send(true).unwrap();
        first.await.unwrap().unwrap();
        assert!(orchestrator.claim_incoming(incoming()).await.is_ok());
    }

    #[tokio::test]
    async fn failed_claim_stamps_error() {
        let plans = Arc::new(MemPlans::default());
        plans
            .save(&plan(Vec::new(), ClaimPlanStatus::Processing))
            .await
            .unwrap();
        let executor = Arc::new(MockExecutor {
            plans: Arc::clone(&plans),
            saves_seen_at_claim: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            outcome: Err("CoinPayment: Unavailable".into()),
            release: None,
        });
        let orchestrator = orchestrator(
            Arc::clone(&plans),
            Arc::new(MockVerifier::default()),
            executor,
        );

        let outcome = orchestrator.claim_incoming(incoming()).await;
        assert_eq!(
            outcome,
            Err(ClaimError::Failed("CoinPayment: Unavailable".into()))
        );
        assert_eq!(
            plans.plan(&[7; 32]).await.unwrap().unwrap().status,
            ClaimPlanStatus::Error
        );
        assert_eq!(orchestrator.statuses.status("m1"), Some(ClaimStatus::Error));
    }
}
