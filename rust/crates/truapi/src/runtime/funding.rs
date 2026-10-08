//! Host-global registry of funding sessions.
//!
//! A session outlives the surface that opened it and is not scoped to one
//! product connection: the Balance card opens one, and a product that reloads
//! attaches to it with `status_subscribe`. So the registry hangs off
//! [`RuntimeServices`] rather than off a product runtime.
//!
//! Every change goes through [`FundingRegistry::commit`], which serialises
//! writes so persisted snapshots land in order, persists before anyone hears of
//! the change, and then notifies subscribers and the host.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use core::time::Duration;

use futures::channel::mpsc;
use futures::lock::Mutex as AsyncMutex;
use futures::stream::{self, BoxStream, StreamExt};
use truapi::latest::{
    FundingDirection, FundingUpdate, GenericError, HostFundingServeSubscribeItem,
    HostFundingStatusSubscribeItem, HostPaymentStatusSubscribeError,
    HostPaymentStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError,
    HostPaymentTopUpStatusSubscribeItem,
};

use super::services::RuntimeServices;
use crate::host_logic::funding::{
    CancelOutcome, FundingSession, FundingSessionError, ReportRefusal, Settlement, load_sessions,
    retained, store_sessions,
};
use crate::platform::{
    CoreStorage, FundingPlatform, FundingPresentOutcome, FundingPresentation, Platform,
    ProductContext, ProductExecutionKind,
};
use crate::runtime::payment_id::host_payment_id;
use crate::unix_time::current_unix_millis;

/// Wait before retrying an expiry sweep whose write failed.
const SWEEP_RETRY: Duration = Duration::from_secs(30);

type Subscribers = HashMap<String, Vec<mpsc::UnboundedSender<HostFundingStatusSubscribeItem>>>;
type Servers = HashMap<String, Vec<mpsc::UnboundedSender<HostFundingServeSubscribeItem>>>;

/// Host-global funding sessions, their subscribers and the host surface.
#[derive(Default)]
pub struct FundingRegistry {
    sessions: Mutex<HashMap<String, FundingSession>>,
    subscribers: Mutex<Subscribers>,
    /// Held across every load and write; the flag records whether persisted
    /// sessions have been loaded.
    writes: AsyncMutex<bool>,
    /// Whether a task is waiting on the next deadline.
    sweeping: AtomicBool,
    platform: OnceLock<Arc<dyn FundingPlatform>>,
    /// Each provider's open `serve_subscribe` streams, by product id.
    servers: Mutex<Servers>,
    /// Sessions holding a reference on their provider's worker, with that
    /// provider.
    holding: Mutex<HashMap<String, String>>,
    /// Sessions whose top-ups or payment are being followed to an outcome.
    following: Mutex<HashSet<String>>,
    /// The services the registry belongs to, for worker references and
    /// following top-ups and payments.
    services: OnceLock<Weak<RuntimeServices>>,
}

impl FundingRegistry {
    /// Install the host's funding surface. Set-once; returns whether this call
    /// installed it.
    pub fn install_platform(&self, platform: Arc<dyn FundingPlatform>) -> bool {
        self.platform.set(platform).is_ok()
    }

    /// The host's funding surface, when one is installed.
    pub fn platform(&self) -> Option<Arc<dyn FundingPlatform>> {
        self.platform.get().cloned()
    }

    /// Attach the registry to the services it belongs to. Set-once.
    pub fn bind(&self, services: &Arc<RuntimeServices>) {
        let _ = self.services.set(Arc::downgrade(services));
    }

    /// Assign open session `intent` to the provider the user chose, and hand
    /// it to that provider. Returns whether it was open and unassigned.
    pub async fn select_provider(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        intent: &str,
        provider_id: &str,
    ) -> Result<bool, FundingSessionError> {
        let intent = intent.to_string();
        let provider = provider_id.to_string();
        let assigned = self
            .commit(storage, now_ms, move |sessions| {
                let assigned = sessions
                    .get_mut(&intent)
                    .and_then(|session| session.assign(&provider).then(|| session.assignment()));
                let changed = assigned.iter().map(|session| session.intent.clone()).collect();
                (assigned, changed)
            })
            .await?;
        let Some(session) = assigned else {
            return Ok(false);
        };
        self.serve_item(provider_id, HostFundingServeSubscribeItem::Assigned { session });
        Ok(true)
    }

    /// The sessions assigned to `provider_id` and requests to cancel them:
    /// every one still in flight first, then each later one. An assignment
    /// may arrive more than once.
    pub fn serve(&self, provider_id: &str) -> BoxStream<'static, HostFundingServeSubscribeItem> {
        let mut servers = self.lock_servers();
        let mut assigned: Vec<_> = self
            .lock_sessions()
            .values()
            .filter(|session| {
                !session.is_terminal() && session.provider_id.as_deref() == Some(provider_id)
            })
            .cloned()
            .collect();
        assigned.sort_by_key(|session| session.opened_at_ms);
        let replay: Vec<_> = assigned
            .into_iter()
            .flat_map(|session| {
                let cancel = session
                    .cancel_requested
                    .then(|| HostFundingServeSubscribeItem::Cancel {
                        intent: session.intent.clone(),
                    });
                std::iter::once(HostFundingServeSubscribeItem::Assigned {
                    session: session.assignment(),
                })
                .chain(cancel)
            })
            .collect();
        let (sender, receiver) = mpsc::unbounded();
        servers
            .entry(provider_id.to_string())
            .or_default()
            .push(sender);
        stream::iter(replay).chain(receiver).boxed()
    }

    /// Store `update` from `provider_id` on session `intent`.
    pub async fn report(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        provider_id: &str,
        intent: &str,
        update: FundingUpdate,
    ) -> Result<Result<(), ReportRefusal>, FundingSessionError> {
        let intent = intent.to_string();
        let provider = provider_id.to_string();
        self.commit(storage, now_ms, move |sessions| {
            // Ids are the provider's own, so one named by another of its
            // sessions would settle two sessions from a single claim.
            let duplicate = FundingSession::named_id(&update).is_some_and(|id| {
                sessions.values().any(|other| {
                    other.intent != intent
                        && other.provider_id.as_deref() == Some(provider.as_str())
                        && other.names(&id)
                })
            });
            let reported = match sessions.get_mut(&intent) {
                Some(_) if duplicate => Err(ReportRefusal::DuplicateId),
                Some(session) => session.report(&provider, update, now_ms),
                None => Err(ReportRefusal::NotFound),
            };
            let changed = if reported.is_ok() { vec![intent] } else { Vec::new() };
            (reported, changed)
        })
        .await
    }

    /// Whether open session `intent` is assigned to `provider_id`.
    pub fn is_serving(&self, provider_id: &str, intent: &str) -> bool {
        self.get(intent).is_some_and(|session| {
            !session.is_terminal() && session.provider_id.as_deref() == Some(provider_id)
        })
    }

    /// Snapshot one session.
    pub fn get(&self, intent: &str) -> Option<FundingSession> {
        self.lock_sessions().get(intent).cloned()
    }

    /// Every session the core keeps, for the host's progress and history
    /// views: those in flight first, then the ended ones, each newest first.
    pub fn sessions(&self) -> Vec<FundingSession> {
        let mut sessions: Vec<_> = self.lock_sessions().values().cloned().collect();
        sessions.sort_by_key(|session| {
            (
                session.is_terminal(),
                core::cmp::Reverse(session.settled_at_ms().unwrap_or(session.opened_at_ms)),
            )
        });
        sessions
    }

    /// Record that the host has written ended session `intent` into its own
    /// history, so it is no longer handed over and can age out. Returns
    /// whether the session was ended and not yet acknowledged.
    pub async fn acknowledge(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        intent: &str,
    ) -> Result<bool, FundingSessionError> {
        let intent = intent.to_string();
        self.commit(storage, now_ms, move |sessions| {
            let acknowledged = sessions
                .get_mut(&intent)
                .filter(|session| session.is_terminal() && !session.acknowledged)
                .map(|session| session.acknowledged = true)
                .is_some();
            (acknowledged, Vec::new())
        })
        .await
    }

    /// Cancel session `intent` at the user's request: end it if no provider
    /// serves it, otherwise ask the provider to stop.
    pub async fn cancel(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        intent: &str,
    ) -> Result<CancelOutcome, FundingSessionError> {
        let target = intent.to_string();
        let (outcome, provider) = self
            .commit(storage, now_ms, move |sessions| {
                let Some(session) = sessions.get_mut(&target) else {
                    return ((CancelOutcome::Refused, None), Vec::new());
                };
                let outcome = session.cancel(now_ms);
                let provider = session.provider_id.clone();
                let changed = if outcome == CancelOutcome::Refused { Vec::new() } else { vec![target] };
                ((outcome, provider), changed)
            })
            .await?;
        if let (CancelOutcome::Requested, Some(provider)) = (outcome, provider) {
            self.serve_item(
                &provider,
                HostFundingServeSubscribeItem::Cancel {
                    intent: intent.to_string(),
                },
            );
        }
        Ok(outcome)
    }

    /// Watch one session, receiving its current stage immediately. A terminal
    /// session yields that one item and then ends.
    pub fn subscribe(
        &self,
        intent: &str,
    ) -> Option<BoxStream<'static, HostFundingStatusSubscribeItem>> {
        // A commit fans out under this lock after writing, so holding it from
        // the read to the registration means no change is missed.
        let mut subscribers = self.lock_subscribers();
        let session = self.get(intent)?;
        let terminal = session.is_terminal();
        let current = stream::once(async move { session.wire_item() });
        if terminal {
            return Some(current.boxed());
        }
        let (sender, receiver) = mpsc::unbounded();
        subscribers
            .entry(intent.to_string())
            .or_default()
            .push(sender);
        Some(current.chain(receiver).boxed())
    }

    /// Apply `edit` to a copy of the sessions, persist the result, then make it
    /// current and notify every session `edit` names.
    ///
    /// Persisted sessions are loaded first on the first call, and every
    /// commit expires the sessions past their deadline, so a long suspend
    /// cannot leave an overdue session open.
    pub async fn commit<R>(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        edit: impl FnOnce(&mut HashMap<String, FundingSession>) -> (R, Vec<String>),
    ) -> Result<R, FundingSessionError> {
        let mut loaded = self.writes.lock().await;
        let before = self.lock_sessions().clone();
        let mut working = before.clone();
        let mut changed = Vec::new();
        if !*loaded {
            for session in load_sessions(storage).await? {
                working.entry(session.intent.clone()).or_insert(session);
            }
        }
        let (result, edited) = edit(&mut working);
        changed.extend(edited);
        for session in working.values_mut() {
            if session.expire_if_due(now_ms) {
                changed.push(session.intent.clone());
            }
        }
        if !*loaded || working != before {
            let kept = retained(working.into_values());
            store_sessions(storage, &kept).await?;
            *self.lock_sessions() = kept
                .into_iter()
                .map(|session| (session.intent.clone(), session))
                .collect();
        }
        *loaded = true;
        // Still under the write lock, so notifications arrive in commit order,
        // each session once even when an edit and its expiry both name it.
        let mut announced = HashSet::new();
        changed.retain(|intent| announced.insert(intent.clone()));
        for intent in changed {
            if let Some(session) = self.get(&intent) {
                self.fan_out(&session);
            }
        }
        drop(loaded);
        self.follow_sessions();
        Ok(result)
    }

    /// Bring worker references and followers in line with the sessions:
    /// every assigned open session holds its provider's worker, and every
    /// session whose outcome is due has its top-ups or payment followed.
    fn follow_sessions(&self) {
        let Some(services) = self.services.get().and_then(Weak::upgrade) else {
            return;
        };
        let sessions = self.lock_sessions().clone();
        let (acquire, release) = {
            let mut holding = self.lock_holding();
            let mut acquire = Vec::new();
            for session in sessions.values().filter(|session| !session.is_terminal()) {
                if let Some(provider) = &session.provider_id
                    && !holding.contains_key(&session.intent)
                {
                    holding.insert(session.intent.clone(), provider.clone());
                    acquire.push(provider.clone());
                }
            }
            let ended: Vec<String> = holding
                .keys()
                .filter(|intent| sessions.get(*intent).is_none_or(FundingSession::is_terminal))
                .cloned()
                .collect();
            let release: Vec<String> = ended
                .iter()
                .filter_map(|intent| holding.remove(intent))
                .collect();
            (acquire, release)
        };
        for provider in &acquire {
            services.worker_ledger.acquire(provider);
        }
        for provider in &release {
            services.worker_ledger.release(provider);
        }
        for session in sessions.values() {
            let (Some(settlement), Some(provider)) =
                (session.settlement_due(), session.provider_id.clone())
            else {
                continue;
            };
            if self.lock_following().insert(session.intent.clone()) {
                services
                    .clone()
                    .follow_settlement(session.intent.clone(), provider, settlement);
            }
        }
    }

    /// Send `item` to every open stream of `provider_id`.
    fn serve_item(&self, provider_id: &str, item: HostFundingServeSubscribeItem) {
        if let Some(senders) = self.lock_servers().get_mut(provider_id) {
            senders.retain(|sender| sender.unbounded_send(item.clone()).is_ok());
        }
    }

    /// Keep one task waiting on the earliest deadline while any session can
    /// expire, so a session expires on time whether or not anyone asks. The
    /// task ends once none can or the registry is dropped.
    pub fn keep_expiring(self: &Arc<Self>, services: &RuntimeServices) {
        if self.sweeping.swap(true, Ordering::AcqRel) {
            return;
        }
        let registry = Arc::downgrade(self);
        let storage: Arc<dyn Platform> = services.platform.clone();
        (services.spawner)(Box::pin(async move {
            while let Some(wait_ms) = Self::next_wait(&registry) {
                futures_timer::Delay::new(Duration::from_millis(wait_ms)).await;
                let Some(live) = registry.upgrade() else {
                    return;
                };
                let swept = live
                    .commit(storage.as_ref(), current_unix_millis(), |_| {
                        ((), Vec::new())
                    })
                    .await;
                if let Err(error) = swept {
                    tracing::warn!(%error, "funding expiry sweep failed");
                    futures_timer::Delay::new(SWEEP_RETRY).await;
                }
            }
        }));
    }

    /// Milliseconds until the earliest open deadline, or `None` once nothing
    /// is open, which also clears the sweeping flag.
    fn next_wait(registry: &Weak<Self>) -> Option<u64> {
        let live = registry.upgrade()?;
        loop {
            if let Some(deadline_ms) = live.next_deadline() {
                return Some(deadline_ms.saturating_sub(current_unix_millis()));
            }
            live.sweeping.store(false, Ordering::Release);
            // A session opened after the check would otherwise wait for the
            // next caller to arm a sweep.
            if live.next_deadline().is_none() || live.sweeping.swap(true, Ordering::AcqRel) {
                return None;
            }
        }
    }

    fn next_deadline(&self) -> Option<u64> {
        self.lock_sessions()
            .values()
            .filter(|session| session.can_expire())
            .map(|session| session.deadline_ms)
            .min()
    }

    /// Tell subscribers and the host about a session's current stage. A
    /// terminal stage ends the subscriber streams.
    fn fan_out(&self, session: &FundingSession) {
        let item = session.wire_item();
        {
            let mut subscribers = self.lock_subscribers();
            if let Some(senders) = subscribers.get_mut(&session.intent) {
                senders.retain(|sender| sender.unbounded_send(item.clone()).is_ok());
            }
            if session.is_terminal() {
                subscribers.remove(&session.intent);
            }
        }
        if let Some(platform) = self.platform() {
            platform.funding_session_changed(session.intent.clone(), item);
        }
    }

    fn lock_sessions(&self) -> std::sync::MutexGuard<'_, HashMap<String, FundingSession>> {
        self.sessions
            .lock()
            .expect("funding sessions mutex poisoned")
    }

    fn lock_subscribers(&self) -> std::sync::MutexGuard<'_, Subscribers> {
        self.subscribers
            .lock()
            .expect("funding subscribers mutex poisoned")
    }

    fn lock_servers(&self) -> std::sync::MutexGuard<'_, Servers> {
        self.servers.lock().expect("funding servers mutex poisoned")
    }

    fn lock_holding(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.holding.lock().expect("funding holding mutex poisoned")
    }

    fn lock_following(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.following
            .lock()
            .expect("funding following mutex poisoned")
    }
}

/// Why a session could not be opened.
#[derive(Debug, derive_more::Display)]
pub enum OpenFundingError {
    /// The host has no funding surface.
    #[display("host has no funding surface")]
    Unsupported,
    /// The user closed the overlay before starting.
    #[display("funding dismissed")]
    Dismissed,
    /// The session could not be stored.
    #[display("{_0}")]
    Session(FundingSessionError),
    /// The host failed to show the overlay.
    #[display("{}", _0.reason)]
    Present(GenericError),
}

/// First wait before retrying a failed load of the sessions on resume.
const RESUME_RETRY_FIRST: Duration = Duration::from_secs(1);
/// Longest wait between retries of that load.
const RESUME_RETRY_LONGEST: Duration = Duration::from_secs(60);

impl RuntimeServices {
    /// Load persisted sessions and arm expiry for them, so sessions from
    /// before a restart end on time and the host hears of them.
    pub fn resume_funding(self: &Arc<Self>) {
        self.funding().bind(self);
        let services = self.clone();
        (self.spawner)(Box::pin(async move {
            let registry = services.funding();
            // The host's session views read what the core has loaded, so a
            // failed load is retried until the sessions are in.
            let mut retry_in = RESUME_RETRY_FIRST;
            loop {
                let loaded = registry
                    .commit(
                        services.platform.as_ref(),
                        current_unix_millis(),
                        |sessions| {
                            // Ended sessions the host has not recorded are
                            // handed over again, so its history gets every
                            // outcome.
                            let pending = sessions
                                .values()
                                .filter(|session| session.needs_handoff())
                                .map(|session| session.intent.clone())
                                .collect();
                            ((), pending)
                        },
                    )
                    .await;
                match loaded {
                    Ok(()) => break registry.keep_expiring(&services),
                    Err(error) => {
                        tracing::warn!(%error, ?retry_in, "loading funding sessions failed");
                        futures_timer::Delay::new(retry_in).await;
                        retry_in = (retry_in * 2).min(RESUME_RETRY_LONGEST);
                    }
                }
            }
        }));
    }

    /// End session `intent` with `stage` as though its funds had moved, for
    /// test hosts that drive the flow with no chain behind it. Returns
    /// whether it was still in flight.
    #[cfg(feature = "test-host")]
    pub async fn settle_funding_for_test(
        &self,
        intent: &str,
        stage: crate::host_logic::funding::FundingStage,
    ) -> Result<bool, FundingSessionError> {
        let intent = intent.to_string();
        self.funding()
            .commit(self.platform.as_ref(), current_unix_millis(), move |sessions| {
                let settled = sessions
                    .get_mut(&intent)
                    .filter(|session| !session.is_terminal())
                    .map(|session| session.stage = stage)
                    .is_some();
                (settled, if settled { vec![intent] } else { Vec::new() })
            })
            .await
    }

    /// Record that the host wrote ended session `intent` into its own
    /// history. Returns whether it was ended and not yet acknowledged.
    pub async fn acknowledge_funding_session(&self, intent: &str) -> Result<bool, FundingSessionError> {
        self.funding()
            .acknowledge(self.platform.as_ref(), current_unix_millis(), intent)
            .await
    }

    /// Cancel session `intent` at the user's request. Returns whether the
    /// cancel was taken: the session ended, or its provider was asked to
    /// stop.
    pub async fn cancel_funding(&self, intent: &str) -> Result<bool, FundingSessionError> {
        let outcome = self
            .funding()
            .cancel(self.platform.as_ref(), current_unix_millis(), intent)
            .await?;
        Ok(outcome != CancelOutcome::Refused)
    }

    /// Hand open session `intent` to the provider the user chose. Returns
    /// whether it was open and not yet assigned.
    pub async fn select_funding_provider(
        self: &Arc<Self>,
        intent: &str,
        provider_id: &str,
    ) -> Result<bool, FundingSessionError> {
        let provider = ProductContext::new_with_execution(
            provider_id.to_string(),
            ProductExecutionKind::Worker,
        )
        .map_err(|error| FundingSessionError::InvalidProvider {
            reason: error.to_string(),
        })?;
        let registry = self.funding();
        registry.bind(self);
        registry
            .select_provider(
                self.platform.as_ref(),
                current_unix_millis(),
                intent,
                &provider.product_id,
            )
            .await
    }

    /// Follow a session's top-ups or payment to the end, then settle it with
    /// what moved. A stream that ends without an outcome leaves the session
    /// to be followed again on the next change to the sessions.
    fn follow_settlement(self: Arc<Self>, intent: String, provider_id: String, settlement: Settlement) {
        let spawner = self.spawner.clone();
        spawner(Box::pin(async move {
            let registry = self.funding();
            let provider =
                match ProductContext::new_with_execution(provider_id, ProductExecutionKind::Worker) {
                    Ok(provider) => provider,
                    Err(error) => {
                        tracing::warn!(%error, "a funding session names an invalid provider");
                        registry.lock_following().remove(&intent);
                        return;
                    }
                };
            let moved = match settlement {
                Settlement::TopUps(top_ups) => self.claimed(&provider, top_ups).await,
                Settlement::Payment(id, amount) => self.paid(&provider, id, amount).await,
            };
            if let Some(moved) = moved {
                let settling = intent.clone();
                let settled = registry
                    .commit(self.platform.as_ref(), current_unix_millis(), move |sessions| {
                        let now_ms = current_unix_millis();
                        let settled = sessions
                            .get_mut(&settling)
                            .is_some_and(|session| session.settle(moved, now_ms));
                        ((), if settled { vec![settling] } else { Vec::new() })
                    })
                    .await;
                if let Err(error) = settled {
                    tracing::warn!(%error, "settling a funding session failed");
                }
            }
            registry.lock_following().remove(&intent);
        }));
    }

    /// What `top_ups` credited: the full amount of each one claimed, what a
    /// partial claim reports, nothing for one not claimed or unknown to the
    /// host. `None` if a status could not be read to its end.
    async fn claimed(&self, provider: &ProductContext, top_ups: Vec<([u8; 32], u128)>) -> Option<u128> {
        let Some(platform) = self.top_up_platform() else {
            tracing::warn!("no top-up platform to follow a funding session's claims");
            return None;
        };
        let mut credited: u128 = 0;
        for (id, amount) in top_ups {
            let mut claimed = None;
            let mut statuses = platform.subscribe_top_up_status(provider, host_payment_id(provider, id));
            while let Some(status) = statuses.next().await {
                match status {
                    Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized }) => {
                        claimed = Some(amount);
                        if finalized {
                            break;
                        }
                    }
                    Ok(HostPaymentTopUpStatusSubscribeItem::ClaimedPartially { actual_claimed }) => {
                        claimed = Some(actual_claimed);
                        break;
                    }
                    Ok(HostPaymentTopUpStatusSubscribeItem::NotClaimed)
                    | Err(HostPaymentTopUpStatusSubscribeError::NotFound) => {
                        claimed = Some(0);
                        break;
                    }
                    Ok(_) => {}
                    Err(HostPaymentTopUpStatusSubscribeError::Unknown { reason }) => {
                        tracing::warn!(%reason, "reading a top-up's status failed");
                        return None;
                    }
                }
            }
            credited = credited.saturating_add(claimed?);
        }
        Some(credited)
    }

    /// What payment `id` moved: its amount once completed, what a partial
    /// claim reports, nothing once failed or unknown to the host. `None` if
    /// its status could not be read to its end.
    async fn paid(&self, provider: &ProductContext, id: [u8; 32], amount: u128) -> Option<u128> {
        let Some(platform) = self.payment_platform() else {
            tracing::warn!("no payment platform to follow a funding session's payment");
            return None;
        };
        let mut statuses = platform.subscribe_payment_status(provider, host_payment_id(provider, id));
        while let Some(status) = statuses.next().await {
            match status {
                Ok(HostPaymentStatusSubscribeItem::Completed) => return Some(amount),
                Ok(HostPaymentStatusSubscribeItem::PartiallyClaimed { actual_claimed }) => {
                    return Some(actual_claimed);
                }
                Ok(HostPaymentStatusSubscribeItem::Failed { .. })
                | Err(HostPaymentStatusSubscribeError::PaymentNotFound) => return Some(0),
                Ok(HostPaymentStatusSubscribeItem::Processing) => {}
                Err(HostPaymentStatusSubscribeError::Unknown { reason }) => {
                    tracing::warn!(%reason, "reading a payment's status failed");
                    return None;
                }
            }
        }
        None
    }

    /// Open a session and show the host's funding overlay for it: the one
    /// path a product's `request` and the host's own Balance card both take.
    ///
    /// A session the user never starts, or that the host fails to show, is
    /// discarded rather than left open.
    pub async fn open_funding(
        self: &Arc<Self>,
        product: Option<&ProductContext>,
        direction: FundingDirection,
        amount: Option<u128>,
    ) -> Result<FundingSession, OpenFundingError> {
        let registry = self.funding();
        registry.bind(self);
        let platform = registry.platform().ok_or(OpenFundingError::Unsupported)?;
        let now_ms = current_unix_millis();
        let session = FundingSession::new(
            format!("fs_{}", nanoid::nanoid!(10)),
            product.map(|product| product.product_id.clone()),
            direction,
            amount,
            now_ms,
        );
        // Nothing is stored until the user starts the session, so an overlay
        // the app is killed under leaves no session behind.
        let presented = platform
            .present_funding(
                product,
                FundingPresentation {
                    intent: session.intent.clone(),
                    direction,
                    amount,
                },
            )
            .await;
        match presented {
            Ok(FundingPresentOutcome::Started) => {
                let started = session.clone();
                registry
                    .commit(self.platform.as_ref(), current_unix_millis(), move |sessions| {
                        let intent = started.intent.clone();
                        sessions.insert(intent.clone(), started);
                        ((), vec![intent])
                    })
                    .await
                    .map_err(OpenFundingError::Session)?;
                registry.keep_expiring(self);
                Ok(session)
            }
            Ok(_) => Err(OpenFundingError::Dismissed),
            Err(error) => Err(OpenFundingError::Present(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::executor::block_on;
    use truapi::latest::FundingFailure;

    use crate::host_logic::funding::{FundingStage, ReportRefusal};
    use crate::test_support::stub_platform;

    const NOW: u64 = 1_700_000_000_000;
    const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

    fn session(intent: &str, opened_at_ms: u64) -> FundingSession {
        FundingSession::new(
            intent.to_string(),
            Some("wallet.dot".to_string()),
            FundingDirection::In,
            Some(100),
            opened_at_ms,
        )
    }

    fn insert(registry: &FundingRegistry, storage: &dyn CoreStorage, session: FundingSession) {
        block_on(registry.commit(storage, NOW, move |sessions| {
            sessions.insert(session.intent.clone(), session);
            ((), Vec::new())
        }))
        .expect("inserted");
    }

    #[test]
    fn a_settled_session_survives_later_saves() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        insert(&registry, storage.as_ref(), session("fs_old", NOW - DAY_MS));
        block_on(registry.commit(storage.as_ref(), NOW, |_| ((), Vec::new()))).expect("swept");

        insert(&registry, storage.as_ref(), session("fs_new", NOW));

        let persisted: Vec<String> = block_on(load_sessions(storage.as_ref()))
            .expect("loaded")
            .into_iter()
            .map(|session| session.intent)
            .collect();
        assert_eq!(persisted, ["fs_new", "fs_old"]);
    }

    #[test]
    fn a_subscriber_sees_the_current_stage_then_expiry_and_the_end() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        insert(&registry, storage.as_ref(), session("fs_1", NOW));
        let stream = registry.subscribe("fs_1").expect("session exists");

        block_on(registry.commit(storage.as_ref(), NOW + DAY_MS, |_| ((), Vec::new())))
            .expect("swept");

        assert_eq!(
            block_on(stream.collect::<Vec<_>>()),
            vec![
                HostFundingStatusSubscribeItem::InProgress {
                    expires_at: Some(NOW + DAY_MS),
                },
                HostFundingStatusSubscribeItem::Failed {
                    reason: FundingFailure::Expired,
                    moved: 0,
                },
            ]
        );
    }

    #[test]
    fn a_restart_restores_open_sessions_and_expires_overdue_ones() {
        let storage = stub_platform();
        let before = FundingRegistry::default();
        insert(&before, storage.as_ref(), session("fs_live", NOW));
        insert(
            &before,
            storage.as_ref(),
            session("fs_overdue", NOW - DAY_MS),
        );

        let after = FundingRegistry::default();
        block_on(after.commit(storage.as_ref(), NOW, |_| ((), Vec::new()))).expect("loaded");

        assert_eq!(after.get("fs_live"), Some(session("fs_live", NOW)));
        assert_eq!(
            after.get("fs_overdue").map(|session| session.stage),
            Some(FundingStage::Failed {
                reason: FundingFailure::Expired,
                settled_at_ms: NOW,
            })
        );
    }

    // The host's list shows what is in flight first, then what ended, each
    // newest first.
    #[test]
    fn sessions_list_in_flight_first_then_ended_newest_first() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        let ended = |intent, settled_at_ms| {
            let mut session = session(intent, settled_at_ms - 1);
            session.fail(FundingFailure::Expired, settled_at_ms);
            session
        };
        for session in [
            ended("fs_old", NOW - 2),
            session("fs_live_old", NOW - 5),
            ended("fs_new", NOW - 1),
            session("fs_live_new", NOW),
        ] {
            insert(&registry, storage.as_ref(), session);
        }

        assert_eq!(
            registry.sessions().into_iter().map(|session| session.intent).collect::<Vec<_>>(),
            ["fs_live_new", "fs_live_old", "fs_new", "fs_old"]
        );
    }

    // A provider's top-up and payment ids are its own, so naming one in two
    // sessions would settle both from a single claim and show the money twice
    // in the user's history. Another provider's ids are a different namespace.
    #[test]
    fn a_provider_cannot_name_one_top_up_in_two_sessions() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        let served = |intent: &str, provider: &str| {
            let mut session = session(intent, NOW);
            assert!(session.assign(provider));
            session
        };
        for session in [
            served("fs_a", "ramp.dot"),
            served("fs_b", "ramp.dot"),
            served("fs_c", "other.dot"),
        ] {
            insert(&registry, storage.as_ref(), session);
        }
        let crediting = FundingUpdate::Crediting {
            top_up_id: [9; 32],
            amount: 100,
        };
        let report = |provider: &str, intent: &str| {
            block_on(registry.report(storage.as_ref(), NOW, provider, intent, crediting.clone()))
                .expect("stored")
        };

        assert_eq!(
            (
                report("ramp.dot", "fs_a"),
                report("ramp.dot", "fs_b"),
                report("other.dot", "fs_c"),
            ),
            (Ok(()), Err(ReportRefusal::DuplicateId), Ok(()))
        );
    }

    #[test]
    fn a_failed_save_changes_nothing_in_memory() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        insert(&registry, storage.as_ref(), session("fs_1", NOW));
        let failing = crate::test_support::StubPlatform {
            local_storage_error: Some("disk full"),
            ..Default::default()
        };

        let failed = block_on(registry.commit(&failing, NOW, |sessions| {
            sessions.clear();
            ((), Vec::new())
        }));

        assert!(failed.is_err());
        assert_eq!(registry.get("fs_1"), Some(session("fs_1", NOW)));
    }
}
