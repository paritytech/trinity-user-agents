// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use {parking_lot::Mutex, std::sync::Arc};

use futures::future::AbortHandle;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::claim::{
    ClaimError, ClaimOrchestrator, ClaimStatus, ClaimStatusStore, IncomingClaim, SendConfirmation,
    TransferSendVerifying,
};
use crate::claim_plan::{ClaimPlan, ClaimPlanStatus, ClaimPlanStore};
use crate::constants::SEND_VERIFY_BLOCK_TIMEOUT;
use crate::tasks::ActiveTaskRegistry;

/// One `coinageSend` chat message, already decoded by the chat layer:
/// `memo_key = TransferMemo::identifier` and the memo's declared total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoinageSendMessage {
    pub message_id: String,
    pub memo_key: [u8; 32],
    pub total_value: u128,
}

pub struct TransferRecipientService {
    spawner: crate::Spawner,
    orchestrator: Arc<ClaimOrchestrator>,
    plans: Arc<dyn ClaimPlanStore>,
    verifier: Arc<dyn TransferSendVerifying>,
    statuses: Arc<ClaimStatusStore>,
    registry: Arc<ActiveTaskRegistry>,
    loops: Mutex<Vec<AbortHandle>>,
}

impl TransferRecipientService {
    /// `statuses` must be the same store injected into `orchestrator`, so
    /// incoming and outgoing pushes land on one per-message channel.
    pub fn new(
        orchestrator: Arc<ClaimOrchestrator>,
        plans: Arc<dyn ClaimPlanStore>,
        verifier: Arc<dyn TransferSendVerifying>,
        statuses: Arc<ClaimStatusStore>,
        spawner: crate::Spawner,
    ) -> Self {
        Self {
            orchestrator,
            plans,
            verifier,
            statuses,
            registry: Arc::new(ActiveTaskRegistry::new(Arc::clone(&spawner))),
            spawner,
            loops: Mutex::new(Vec::new()),
        }
    }

    pub fn registry(&self) -> Arc<ActiveTaskRegistry> {
        Arc::clone(&self.registry)
    }

    pub async fn start(
        self: &Arc<Self>,
        incoming: mpsc::Receiver<CoinageSendMessage>,
        outgoing: mpsc::Receiver<CoinageSendMessage>,
    ) -> Result<(), String> {
        self.orchestrator.restore_persisted_statuses().await?;
        let mut loops = self.loops.lock();
        loops.push(self.spawn_loop(incoming, Direction::Incoming));
        loops.push(self.spawn_loop(outgoing, Direction::Outgoing));
        Ok(())
    }

    pub fn throttle(&self) {
        for handle in self.loops.lock().drain(..) {
            handle.abort();
        }
        self.registry.cancel_all();
    }

    fn spawn_loop(
        self: &Arc<Self>,
        mut messages: mpsc::Receiver<CoinageSendMessage>,
        direction: Direction,
    ) -> AbortHandle {
        let service = Arc::clone(self);
        crate::tasks::spawn_abortable(&self.spawner, async move {
            while let Some(message) = messages.recv().await {
                service.dispatch(message, direction);
            }
        })
    }

    fn dispatch(self: &Arc<Self>, message: CoinageSendMessage, direction: Direction) {
        if matches!(
            self.statuses.status(&message.message_id),
            Some(ClaimStatus::Finished { .. })
        ) {
            return;
        }
        let service = Arc::clone(self);
        let id = message.message_id.clone();
        self.registry.try_start(&id, async move {
            match direction {
                Direction::Incoming => service.run_incoming(message).await,
                Direction::Outgoing => service.run_outgoing(message).await,
            }
        });
    }

    async fn run_incoming(&self, message: CoinageSendMessage) {
        let outcome = self
            .orchestrator
            .claim_incoming(IncomingClaim {
                memo_key: message.memo_key,
                message_id: message.message_id.clone(),
                total_value: message.total_value,
            })
            .await;
        match outcome {
            Ok(claimed) => info!(
                message_id = message.message_id,
                claimed, "incoming coinage send claimed"
            ),
            Err(ClaimError::AlreadyClaiming) => {
                info!(
                    message_id = message.message_id,
                    "claim already in flight for this memo"
                );
            }
            Err(ClaimError::Failed(error)) => {
                warn!(
                    error,
                    message_id = message.message_id,
                    "incoming claim failed"
                );
            }
        }
    }

    async fn run_outgoing(&self, message: CoinageSendMessage) {
        if let Err(error) = self.verify_outgoing(&message).await {
            warn!(
                error,
                message_id = message.message_id,
                "outgoing send verification failed"
            );
            if let Err(stamp_error) = self
                .plans
                .update_status(&message.memo_key, ClaimPlanStatus::Error, None)
                .await
            {
                warn!(stamp_error, "outgoing error status stamp failed");
            }
            self.statuses
                .update_status(&message.message_id, ClaimStatus::Error);
        }
    }

    async fn verify_outgoing(&self, message: &CoinageSendMessage) -> Result<(), String> {
        let existing = self.plans.plan(&message.memo_key).await?;
        match &existing {
            // Finished on a prior run: report only.
            Some(plan) if plan.status == ClaimPlanStatus::Finished => {
                self.statuses.update_status(
                    &message.message_id,
                    ClaimStatus::Finished {
                        claimed_amount: plan.claimed_amount.unwrap_or(plan.total_value),
                    },
                );
                return Ok(());
            }
            Some(plan) if plan.status == ClaimPlanStatus::Detected => {
                self.statuses
                    .update_status(&message.message_id, ClaimStatus::Sent);
                self.verifier
                    .await_claim_on_chain(&message.memo_key, SEND_VERIFY_BLOCK_TIMEOUT)
                    .await?;
            }
            other => {
                if other.is_none() {
                    self.plans
                        .save(&ClaimPlan {
                            memo_key: message.memo_key,
                            message_id: Some(message.message_id.clone()),
                            entries: Vec::new(),
                            outgoing_public_keys: Vec::new(),
                            detection_anchor: None,
                            status: ClaimPlanStatus::Processing,
                            claimed_amount: None,
                            total_value: message.total_value,
                        })
                        .await?;
                }
                self.statuses
                    .update_status(&message.message_id, ClaimStatus::Detecting);
                match self
                    .verifier
                    .await_send_or_claimed(&message.memo_key, SEND_VERIFY_BLOCK_TIMEOUT)
                    .await?
                {
                    SendConfirmation::OnChain => {
                        self.plans
                            .update_status(&message.memo_key, ClaimPlanStatus::Detected, None)
                            .await?;
                        self.statuses
                            .update_status(&message.message_id, ClaimStatus::Sent);
                        self.verifier
                            .await_claim_on_chain(&message.memo_key, SEND_VERIFY_BLOCK_TIMEOUT)
                            .await?;
                    }
                    // Consumed before the watch saw them — the recipient
                    // already claimed; terminal.
                    SendConfirmation::AlreadyClaimed => {}
                }
            }
        }
        self.plans
            .update_status(
                &message.memo_key,
                ClaimPlanStatus::Finished,
                Some(message.total_value),
            )
            .await?;
        self.statuses.update_status(
            &message.message_id,
            ClaimStatus::Finished {
                claimed_amount: message.total_value,
            },
        );
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Incoming,
    Outgoing,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::claim::ClaimExecutor;
    use crate::claim_plan::CodableClaimPlanEntry;
    use crate::denomination::DenominationBreakdownContext;

    fn ctx() -> DenominationBreakdownContext {
        DenominationBreakdownContext {
            asset_unit: 10,
            max_exponent: 4,
            min_exponent: 0,
            precision: 10,
        }
    }

    #[derive(Default)]
    struct MemPlans {
        plans: Mutex<HashMap<[u8; 32], ClaimPlan>>,
    }

    #[async_trait]
    impl ClaimPlanStore for MemPlans {
        async fn save(&self, plan: &ClaimPlan) -> Result<(), String> {
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

    struct ScriptedVerifier {
        plans: Arc<MemPlans>,
        send_or_claimed: Result<SendConfirmation, String>,
        claim_outcome: Result<(), String>,
        send_or_claimed_calls: AtomicUsize,
        claim_calls: AtomicUsize,
    }

    impl ScriptedVerifier {
        fn new(
            plans: Arc<MemPlans>,
            send_or_claimed: Result<SendConfirmation, String>,
            claim_outcome: Result<(), String>,
        ) -> Arc<Self> {
            Arc::new(Self {
                plans,
                send_or_claimed,
                claim_outcome,
                send_or_claimed_calls: AtomicUsize::new(0),
                claim_calls: AtomicUsize::new(0),
            })
        }
    }

    #[async_trait]
    impl TransferSendVerifying for ScriptedVerifier {
        async fn await_send_on_chain(
            &self,
            _memo_key: &[u8; 32],
            _block_timeout: u32,
        ) -> Result<(), String> {
            Ok(())
        }
        async fn await_claim_on_chain(
            &self,
            _memo_key: &[u8; 32],
            block_timeout: u32,
        ) -> Result<(), String> {
            assert_eq!(block_timeout, 100, "COINM-024");
            self.claim_calls.fetch_add(1, Ordering::SeqCst);
            self.claim_outcome.clone()
        }
        async fn await_send_or_claimed(
            &self,
            memo_key: &[u8; 32],
            block_timeout: u32,
        ) -> Result<SendConfirmation, String> {
            assert_eq!(block_timeout, 100, "COINM-024");
            self.send_or_claimed_calls.fetch_add(1, Ordering::SeqCst);
            assert!(
                self.plans.plan(memo_key).await.unwrap().is_some(),
                "the .processing placeholder must exist before the send race (COINA-016)"
            );
            self.send_or_claimed.clone()
        }
    }

    struct NoopExecutor;

    #[async_trait]
    impl ClaimExecutor for NoopExecutor {
        async fn claim(
            &self,
            _memo_key: &[u8; 32],
            _message_id: &str,
        ) -> Result<Vec<CodableClaimPlanEntry>, String> {
            Ok(vec![CodableClaimPlanEntry {
                entry_index: 0,
                exponent: 0,
                derivation_index: 1,
            }])
        }
    }

    fn service(
        plans: Arc<MemPlans>,
        verifier: Arc<ScriptedVerifier>,
    ) -> (Arc<TransferRecipientService>, Arc<ClaimStatusStore>) {
        let statuses = Arc::new(ClaimStatusStore::default());
        let orchestrator = Arc::new(ClaimOrchestrator::new(
            Arc::clone(&plans) as Arc<dyn ClaimPlanStore>,
            Arc::clone(&verifier) as Arc<dyn TransferSendVerifying>,
            Arc::new(NoopExecutor),
            Arc::clone(&statuses),
            ctx(),
        ));
        (
            Arc::new(TransferRecipientService::new(
                orchestrator,
                plans,
                verifier,
                Arc::clone(&statuses),
                crate::test_spawner(),
            )),
            statuses,
        )
    }

    fn message() -> CoinageSendMessage {
        CoinageSendMessage {
            message_id: "m1".into(),
            memo_key: [7; 32],
            total_value: 990,
        }
    }

    fn detected_plan() -> ClaimPlan {
        ClaimPlan {
            memo_key: [7; 32],
            message_id: Some("m1".into()),
            entries: Vec::new(),
            outgoing_public_keys: Vec::new(),
            detection_anchor: None,
            status: ClaimPlanStatus::Detected,
            claimed_amount: None,
            total_value: 990,
        }
    }

    async fn settle(statuses: &ClaimStatusStore, id: &str) -> ClaimStatus {
        for _ in 0..1_000 {
            if let Some(status) = statuses.status(id)
                && status.is_terminal()
            {
                return status;
            }
            tokio::task::yield_now().await;
        }
        panic!("status never became terminal");
    }

    #[tokio::test]
    async fn detected_plan_jumps_to_await_claim() {
        let plans = Arc::new(MemPlans::default());
        plans.save(&detected_plan()).await.unwrap();
        let verifier =
            ScriptedVerifier::new(Arc::clone(&plans), Ok(SendConfirmation::OnChain), Ok(()));
        let (service, statuses) = service(Arc::clone(&plans), Arc::clone(&verifier));

        service.run_outgoing(message()).await;
        assert_eq!(
            settle(&statuses, "m1").await,
            ClaimStatus::Finished {
                claimed_amount: 990
            }
        );
        assert_eq!(verifier.send_or_claimed_calls.load(Ordering::SeqCst), 0);
        assert_eq!(verifier.claim_calls.load(Ordering::SeqCst), 1);
        let plan = plans.plan(&[7; 32]).await.unwrap().unwrap();
        assert_eq!(plan.status, ClaimPlanStatus::Finished);
        assert_eq!(plan.claimed_amount, Some(990));
    }

    #[tokio::test]
    async fn fresh_outgoing_send_walks_the_full_path() {
        let plans = Arc::new(MemPlans::default());
        let verifier =
            ScriptedVerifier::new(Arc::clone(&plans), Ok(SendConfirmation::OnChain), Ok(()));
        let (service, statuses) = service(Arc::clone(&plans), Arc::clone(&verifier));

        service.run_outgoing(message()).await;
        assert_eq!(
            settle(&statuses, "m1").await,
            ClaimStatus::Finished {
                claimed_amount: 990
            }
        );
        assert_eq!(verifier.send_or_claimed_calls.load(Ordering::SeqCst), 1);
        assert_eq!(verifier.claim_calls.load(Ordering::SeqCst), 1);
    }

    /// `AlreadyClaimed`: the coins were consumed before the watch — the
    /// claim await is skipped entirely and the send finishes.
    #[tokio::test]
    async fn already_claimed_short_circuits_the_claim_await() {
        let plans = Arc::new(MemPlans::default());
        let verifier = ScriptedVerifier::new(
            Arc::clone(&plans),
            Ok(SendConfirmation::AlreadyClaimed),
            Ok(()),
        );
        let (service, statuses) = service(Arc::clone(&plans), Arc::clone(&verifier));

        service.run_outgoing(message()).await;
        assert_eq!(
            settle(&statuses, "m1").await,
            ClaimStatus::Finished {
                claimed_amount: 990
            }
        );
        assert_eq!(verifier.claim_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn timeout_is_a_terminal_error() {
        let plans = Arc::new(MemPlans::default());
        let verifier = ScriptedVerifier::new(
            Arc::clone(&plans),
            Err("timeout after 100 blocks".into()),
            Ok(()),
        );
        let (service, statuses) = service(Arc::clone(&plans), Arc::clone(&verifier));

        service.run_outgoing(message()).await;
        assert_eq!(settle(&statuses, "m1").await, ClaimStatus::Error);
        assert_eq!(
            plans.plan(&[7; 32]).await.unwrap().unwrap().status,
            ClaimPlanStatus::Error
        );
    }

    #[tokio::test]
    async fn loops_dedup_and_skip_finished_messages() {
        let plans = Arc::new(MemPlans::default());
        let verifier = ScriptedVerifier::new(
            Arc::clone(&plans),
            Ok(SendConfirmation::AlreadyClaimed),
            Ok(()),
        );
        let (service, statuses) = service(Arc::clone(&plans), Arc::clone(&verifier));

        let (incoming_tx, incoming_rx) = mpsc::channel(8);
        let (outgoing_tx, outgoing_rx) = mpsc::channel(8);
        service.start(incoming_rx, outgoing_rx).await.unwrap();

        outgoing_tx.send(message()).await.unwrap();
        outgoing_tx.send(message()).await.unwrap();
        assert_eq!(
            settle(&statuses, "m1").await,
            ClaimStatus::Finished {
                claimed_amount: 990
            }
        );
        // Re-emission after finish: skipped by the terminal guard.
        outgoing_tx.send(message()).await.unwrap();
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            verifier.send_or_claimed_calls.load(Ordering::SeqCst),
            1,
            "one verification for three emissions"
        );

        // Incoming path delegates to the orchestrator.
        incoming_tx
            .send(CoinageSendMessage {
                message_id: "m2".into(),
                memo_key: [8; 32],
                total_value: 10,
            })
            .await
            .unwrap();
        assert_eq!(
            settle(&statuses, "m2").await,
            ClaimStatus::Finished { claimed_amount: 10 }
        );
        service.throttle();
    }

    /// `throttle` stops the loops: messages sent afterwards are never
    /// processed.
    #[tokio::test]
    async fn throttle_stops_the_subscription_loops() {
        let plans = Arc::new(MemPlans::default());
        let verifier = ScriptedVerifier::new(
            Arc::clone(&plans),
            Ok(SendConfirmation::AlreadyClaimed),
            Ok(()),
        );
        let (service, statuses) = service(Arc::clone(&plans), Arc::clone(&verifier));
        let (_incoming_tx, incoming_rx) = mpsc::channel::<CoinageSendMessage>(8);
        let (outgoing_tx, outgoing_rx) = mpsc::channel(8);
        service.start(incoming_rx, outgoing_rx).await.unwrap();
        service.throttle();

        outgoing_tx.send(message()).await.unwrap();
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
        assert_eq!(statuses.status("m1"), None, "loop is dead after throttle");
    }
}
