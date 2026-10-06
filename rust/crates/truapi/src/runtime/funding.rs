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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use core::time::Duration;

use futures::channel::mpsc;
use futures::lock::Mutex as AsyncMutex;
use futures::stream::{self, BoxStream, StreamExt};
use truapi::latest::{FundingDirection, GenericError, HostFundingStatusSubscribeItem};

use super::services::RuntimeServices;
use crate::host_logic::funding::{
    FundingSession, FundingSessionError, load_sessions, retained, store_sessions,
};
use crate::platform::{
    CoreStorage, FundingPlatform, FundingPresentOutcome, FundingPresentation, Platform,
    ProductContext,
};
use crate::unix_time::current_unix_millis;

/// Wait before retrying an expiry sweep whose write failed.
const SWEEP_RETRY: Duration = Duration::from_secs(30);

type Subscribers = HashMap<String, Vec<mpsc::UnboundedSender<HostFundingStatusSubscribeItem>>>;

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

    /// Snapshot one session.
    pub fn get(&self, intent: &str) -> Option<FundingSession> {
        self.lock_sessions().get(intent).cloned()
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
        // Still under the write lock, so notifications arrive in commit order.
        for intent in changed {
            if let Some(session) = self.get(&intent) {
                self.fan_out(&session);
            }
        }
        Ok(result)
    }

    /// Keep one task waiting on the earliest open deadline while any session
    /// is open, so a session expires on time whether or not anyone asks. The
    /// task ends once no session is open or the registry is dropped.
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
            .filter(|session| !session.is_terminal())
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

impl RuntimeServices {
    /// Load persisted sessions and arm expiry for them, so sessions from
    /// before a restart end on time and the host hears of them.
    pub fn resume_funding(self: &Arc<Self>) {
        let services = self.clone();
        (self.spawner)(Box::pin(async move {
            let registry = services.funding();
            let loaded = registry
                .commit(
                    services.platform.as_ref(),
                    current_unix_millis(),
                    |sessions| {
                        let open = sessions
                            .values()
                            .filter(|session| !session.is_terminal())
                            .map(|session| session.intent.clone())
                            .collect();
                        ((), open)
                    },
                )
                .await;
            match loaded {
                Ok(()) => registry.keep_expiring(&services),
                Err(error) => tracing::warn!(%error, "loading funding sessions failed"),
            }
        }));
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
        let platform = registry.platform().ok_or(OpenFundingError::Unsupported)?;
        let now_ms = current_unix_millis();
        let session = FundingSession::new(
            format!("fs_{}", nanoid::nanoid!(10)),
            product.map(|product| product.product_id.clone()),
            direction,
            amount,
            now_ms,
        );
        let storage = self.platform.as_ref();
        let opened = session.clone();
        registry
            .commit(storage, now_ms, move |sessions| {
                sessions.insert(opened.intent.clone(), opened);
                ((), Vec::new())
            })
            .await
            .map_err(OpenFundingError::Session)?;
        registry.keep_expiring(self);

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
                // The host hears of a session only once the user has started it.
                let intent = session.intent.clone();
                let announced = registry
                    .commit(storage, current_unix_millis(), move |_| ((), vec![intent]))
                    .await;
                if let Err(error) = announced {
                    tracing::warn!(%error, "announcing a started funding session failed");
                }
                Ok(session)
            }
            outcome => {
                let intent = session.intent.clone();
                let discarded = registry
                    .commit(storage, current_unix_millis(), move |sessions| {
                        sessions.remove(&intent);
                        ((), Vec::new())
                    })
                    .await;
                if let Err(error) = discarded {
                    tracing::warn!(%error, "discarding an unstarted funding session failed");
                }
                Err(match outcome {
                    Ok(_) => OpenFundingError::Dismissed,
                    Err(error) => OpenFundingError::Present(error),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::executor::block_on;
    use truapi::latest::FundingFailure;

    use crate::host_logic::funding::FundingStage;
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
                HostFundingStatusSubscribeItem::AwaitingDeposit {
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
