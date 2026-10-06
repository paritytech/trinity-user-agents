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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use core::time::Duration;

use futures::channel::mpsc;
use futures::lock::Mutex as AsyncMutex;
use core::future::Future;

use futures::future::{BoxFuture, FutureExt};
use futures::stream::{self, BoxStream, StreamExt};
use truapi::latest::{
    ChainIdentifier, FundingDirection, GenericError, HostFundingStatusSubscribeItem,
};

mod conversion;
mod credit;
mod withdraw;

use conversion::{Chains, ConversionChains, ConversionError, SigningChain};
#[cfg(test)]
use conversion::Prepared;
pub use conversion::{FundingNetwork, FundingSigner};

use super::services::RuntimeServices;

use super::statement_allowance::{ChainClient, ChainContext};
use super::statement_allowance::rpc::RpcClient;
use crate::host_logic::features;
use crate::host_logic::funding::{
    AcceptRefusal, CancelRefusal, ConversionRoute, ConversionStep, ConversionSubmission, CreditProgress, CreditStep,
    DepositAsset, DepositHolding, DepositMismatch, DepositQuote, DepositRequest,
    FundingDeposit, FundingSession, FundingSessionError, FundingStage,
    load_sessions, next_account_number, retained, store_sessions,
};
use crate::platform::{
    CoreStorage, FundingPlatform, FundingPresentOutcome, FundingPresentation, Platform,
    ProductContext,
};
use crate::unix_time::current_unix_millis;

/// Wait before retrying an expiry sweep whose write failed.
const SWEEP_RETRY: Duration = Duration::from_secs(30);
/// Wait between reads of the awaited deposits: two Asset Hub blocks.
const DEPOSIT_POLL: Duration = Duration::from_secs(12);
/// How often the deposit accounts of sessions that already ended are read.
const LATE_READ_MS: u64 = 5 * 60 * 1_000;
/// Longest a chain read may take before the pass gives up on it.
const CHAIN_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a conversion that took the deposit on Asset Hub may take to
/// reach People before it counts as stalled.
const STALL_AFTER_MS: u64 = 30 * 60 * 1_000;
/// Longest a cancel waits to confirm the deposit account is empty, as
/// getcash waits.
const CANCEL_CONFIRM: Duration = Duration::from_secs(8);
/// The chain a pass signs on, whose signed-extension metadata it loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignOn {
    /// It only reads.
    Nowhere,
    /// It signs a conversion on Asset Hub.
    AssetHub,
    /// It signs a withdrawal on People.
    People,
}

/// Numbered accounts skipped for already holding funds before assignment
/// gives up.
const MAX_USED_ACCOUNTS: usize = 16;

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
    /// Whether a task is polling the awaited deposits.
    watching: AtomicBool,
    /// When sessions that already ended were last read, in Unix milliseconds.
    late_read_ms: AtomicU64,
    /// What converts deposits, once a signing host provides it.
    conversion: OnceLock<Conversion>,
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

    /// Let deposits be converted on `network`, signed by `signer`. Set-once;
    /// returns whether this call installed it.
    ///
    /// The assets a deposit can arrive in, `deposit_asset_ids` from the
    /// network's `Assets` pallet with the native token, are read on every
    /// watched deposit account, so a wrong or short deposit is seen.
    pub fn install_conversion(
        &self,
        network: FundingNetwork,
        deposit_asset_ids: Vec<u32>,
        signer: Arc<dyn FundingSigner>,
    ) -> bool {
        let mut deposit_assets: Vec<_> = deposit_asset_ids.into_iter().map(DepositAsset::Asset).collect();
        deposit_assets.push(DepositAsset::Native);
        self.conversion
            .set(Conversion {
                network,
                deposit_assets,
                signer,
            })
            .is_ok()
    }

    /// Snapshot one session.
    pub fn get(&self, intent: &str) -> Option<FundingSession> {
        self.lock_sessions().get(intent).cloned()
    }

    /// Every session the core keeps, for the host's progress and history
    /// views: those in flight first, a held one among them, then the ended
    /// ones, each newest first, as getcash lists its requests.
    pub fn sessions(&self) -> Vec<FundingSession> {
        let mut sessions: Vec<_> = self.lock_sessions().values().cloned().collect();
        sessions.sort_by_key(|session| {
            (
                !session.in_flight(),
                core::cmp::Reverse(session.settled_at_ms().unwrap_or(session.opened_at_ms)),
            )
        });
        sessions
    }

    /// Cancel session `intent`, first recording `holdings`, a fresh reading
    /// of its deposit account when it has one. `confirmed_withdrawal` says
    /// its withdrawal account and payment were read fresh just before.
    pub async fn cancel(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        intent: &str,
        holdings: Option<Vec<DepositHolding>>,
        confirmed_withdrawal: bool,
    ) -> Result<(), CancelFundingError> {
        let intent = intent.to_string();
        self.commit(storage, now_ms, move |sessions| {
            let Some(session) = sessions.get_mut(&intent) else {
                return (Err(CancelFundingError::NotFound), Vec::new());
            };
            // An account assigned since the read, or funds a watch pass
            // recorded since, are not covered by it.
            if (holdings.is_none() && session.deposit.is_some())
                || (!confirmed_withdrawal && session.payment_pending())
            {
                return (Err(CancelFundingError::Unconfirmed), Vec::new());
            }
            if session.deposit.as_ref().is_some_and(|deposit| !deposit.holdings.is_empty()) {
                return (Err(CancelFundingError::Refused(CancelRefusal::FundsArrived)), Vec::new());
            }
            let observed = holdings.is_some_and(|holdings| session.observe_holdings(holdings, now_ms));
            let cancelled = session.cancel(now_ms).map_err(CancelFundingError::Refused);
            let changed = if observed || cancelled.is_ok() { vec![intent] } else { Vec::new() };
            (cancelled, changed)
        })
        .await?
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
                .filter(|session| !session.in_flight() && !session.acknowledged)
                .map(|session| session.acknowledged = true)
                .is_some();
            (acknowledged, Vec::new())
        })
        .await
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
        // The host also hears when only a deposit account's holdings change;
        // a subscriber sees each status once.
        let mut shown = None;
        Some(
            current
                .chain(receiver)
                .filter(move |item| {
                    let fresh = shown.as_ref() != Some(item);
                    if fresh {
                        shown = Some(item.clone());
                    }
                    futures::future::ready(fresh)
                })
                .boxed(),
        )
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
            session.stamp(now_ms);
            // A reopened or held session ends later, with a new outcome.
            if session.in_flight() {
                session.acknowledged = false;
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

    /// Reserve the next account number for `source_id`, serialised with every
    /// other funding write.
    pub async fn next_account_number(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        source_id: &str,
    ) -> Result<u32, FundingSessionError> {
        let _writes = self.writes.lock().await;
        next_account_number(storage, source_id).await
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
            .filter(|session| session.expires_by_sweep())
            .map(|session| session.deadline_ms)
            .min()
    }

    /// Give an open inbound session the first numbered account for the
    /// request's source that holds none of its asset, so a seed restored on a new
    /// install never reuses an account a provider may still pay into.
    /// `derive` maps an account number to its public key.
    pub async fn assign_empty_deposit(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        balances: &dyn DepositBalances,
        now_ms: u64,
        intent: &str,
        plan: DepositPlan,
        derive: impl Fn(u32) -> Result<[u8; 32], GenericError>,
    ) -> Result<[u8; 32], AssignDepositError> {
        let target = self
            .get(intent)
            .ok_or(AssignDepositError::NotFound)
            .and_then(|session| assignable(&session).map(|()| session.amount))?;
        let DepositPlan { request, route } = plan;
        for _ in 0..MAX_USED_ACCOUNTS {
            let number = self
                .next_account_number(storage, &request.source_id)
                .await?;
            let account = derive(number).map_err(AssignDepositError::Derive)?;
            let held = balances
                .balance(request.asset, &account)
                .await
                .map_err(AssignDepositError::Chain)?;
            if held > 0 {
                continue;
            }
            let deposit = FundingDeposit {
                source_id: request.source_id,
                number,
                asset: request.asset,
                account,
                expected: request.expected,
                route,
                target,
                holdings: Vec::new(),
            };
            let intent = intent.to_string();
            return self
                .commit(storage, now_ms, move |sessions| {
                    let assigned = match sessions.get_mut(&intent) {
                        None => Err(AssignDepositError::NotFound),
                        Some(session) => assignable(session).map(|()| {
                            session.deposit = Some(deposit);
                            account
                        }),
                    };
                    (assigned, Vec::new())
                })
                .await?;
        }
        Err(AssignDepositError::AccountsInUse)
    }

    /// Read every watched deposit account once, for its requested asset and
    /// every asset in `deposit_assets`, and record what each holds, so a
    /// wrong, short or late deposit is seen. Sessions that already ended are
    /// read once per [`LATE_READ_MS`]. A failed read of the requested asset
    /// leaves its session for the next pass; a failed read of another asset
    /// keeps that asset's last reading.
    pub async fn observe_deposits(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        balances: &dyn DepositBalances,
        deposit_assets: &[DepositAsset],
    ) -> Result<(), FundingSessionError> {
        let read_late = now_ms.saturating_sub(self.late_read_ms.load(Ordering::Acquire)) >= LATE_READ_MS;
        let watched: Vec<_> = self
            .lock_sessions()
            .values()
            .filter(|session| read_late || !session.is_terminal())
            .filter_map(|session| Some((session.intent.clone(), session.watched_deposit(now_ms)?.clone())))
            .collect();
        if read_late {
            self.late_read_ms.store(now_ms, Ordering::Release);
        }
        let mut readings = Vec::new();
        'sessions: for (intent, deposit) in watched {
            let mut holdings = Vec::new();
            for asset in assets_to_read(&deposit, deposit_assets) {
                let balance = match balances.balance(asset, &deposit.account).await {
                    Ok(balance) => balance,
                    Err(error) if asset != deposit.asset => {
                        tracing::warn!(%intent, reason = %error.reason, "reading a funding deposit failed");
                        deposit.held(asset)
                    }
                    Err(error) => {
                        tracing::warn!(%intent, reason = %error.reason, "reading a funding deposit failed");
                        continue 'sessions;
                    }
                };
                if balance > 0 {
                    holdings.push(DepositHolding { asset, balance });
                }
            }
            readings.push((intent, holdings));
        }
        self.commit(storage, now_ms, move |sessions| {
            let changed = readings
                .into_iter()
                .filter_map(|(intent, holdings)| {
                    let session = sessions.get_mut(&intent)?;
                    session.observe_holdings(holdings, now_ms).then_some(intent)
                })
                .collect();
            ((), changed)
        })
        .await
    }

    /// Whether a polling task should keep going, clearing the watching flag
    /// once no deposit is awaited.
    fn still_watching(registry: &Weak<Self>) -> bool {
        let Some(live) = registry.upgrade() else {
            return false;
        };
        loop {
            if live.awaits_deposit() {
                return true;
            }
            live.watching.store(false, Ordering::Release);
            // A deposit assigned after the check would otherwise wait for the
            // next caller to arm the watch.
            if !live.awaits_deposit() || live.watching.swap(true, Ordering::AcqRel) {
                return false;
            }
        }
    }

    fn awaits_deposit(&self) -> bool {
        self.lock_sessions()
            .values()
            .any(|session| {
                session.watched_deposit(current_unix_millis()).is_some()
                    || session.watched_withdrawal(current_unix_millis()).is_some()
                    || session.withdrawing().is_some()
                    || session.paying_out().is_some()
                    || session.converting().is_some()
                    || session.crediting().is_some()
            })
    }

    /// Every session being converted, with its deposit and submission.
    fn converting_sessions(&self) -> Vec<(String, FundingDeposit, Option<ConversionSubmission>)> {
        self.lock_sessions()
            .values()
            .filter_map(|session| {
                let (deposit, submission) = session.converting()?;
                Some((session.intent.clone(), deposit.clone(), submission))
            })
            .collect()
    }

    /// Every session awaiting or being credited, with its deposit and what
    /// its top-ups claimed so far.
    fn crediting_sessions(&self) -> Vec<CreditingSession> {
        self.lock_sessions()
            .values()
            .filter_map(|session| {
                let (deposit, progress) = session.crediting()?;
                Some(CreditingSession {
                    intent: session.intent.clone(),
                    deposit: deposit.clone(),
                    progress,
                })
            })
            .collect()
    }

    /// Apply one credit `step` to session `intent`.
    async fn record_credit(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        intent: &str,
        step: CreditStep,
    ) -> Result<(), FundingSessionError> {
        let intent = intent.to_string();
        self.commit(storage, now_ms, move |sessions| {
            let changed = sessions
                .get_mut(&intent)
                .is_some_and(|session| session.advance_credit(step, now_ms));
            ((), if changed { vec![intent] } else { Vec::new() })
        })
        .await
    }

    /// Apply one conversion `step` to session `intent`.
    async fn record_conversion(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        intent: &str,
        step: ConversionStep,
    ) -> Result<(), FundingSessionError> {
        let intent = intent.to_string();
        self.commit(storage, now_ms, move |sessions| {
            let changed = sessions
                .get_mut(&intent)
                .is_some_and(|session| session.advance_conversion(step, now_ms));
            ((), if changed { vec![intent] } else { Vec::new() })
        })
        .await
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

/// A session awaiting or being credited, as one credit pass reads it.
struct CreditingSession {
    intent: String,
    deposit: FundingDeposit,
    progress: Option<CreditProgress>,
}

impl CreditingSession {
    /// Whether the next top-up has yet to be sized from what the account
    /// holds.
    fn sizing(&self) -> bool {
        self.progress.is_none_or(|progress| progress.claim.is_none())
    }
}

/// A deposit request with the route core chose for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositPlan {
    /// What the provider delivers.
    pub request: DepositRequest,
    /// How the deposit becomes CASH on People.
    pub route: ConversionRoute,
}

/// What converts deposits: the network's constants and the deposit keys.
struct Conversion {
    network: FundingNetwork,
    /// What a deposit account is read for, the native token last.
    deposit_assets: Vec<DepositAsset>,
    signer: Arc<dyn FundingSigner>,
}

/// What a conversion pass decided for one session.
#[derive(Debug, PartialEq, Eq)]
enum PlannedStep {
    /// Record a step.
    Record(ConversionStep),
    /// Record the submission, then submit `extrinsic`.
    Submit {
        submission: ConversionSubmission,
        extrinsic: Vec<u8>,
    },
}

/// Decide the next step for `deposit`, given its `submission` so far.
///
/// A submitted conversion is judged only by what it did: landed once People
/// shows its CASH on top of what was there before; dropped once its era has
/// passed unincluded; refused once it was included and the deposit is still
/// on Asset Hub, so a conversion failing on chain is bounded like one a dry
/// run refuses; stalled once it took the deposit and nothing arrived in time.
/// A fresh conversion is signed only by the account the deposit sits on.
async fn plan_conversion(
    chains: &dyn ConversionChains,
    signer: &dyn FundingSigner,
    deposit: &FundingDeposit,
    submission: Option<ConversionSubmission>,
    now_ms: u64,
) -> Result<Option<PlannedStep>, ConversionError> {
    let account = &deposit.account;
    if let Some(submission) = submission {
        let landed = chains
            .landed(account)
            .await?
            .saturating_sub(submission.people_before);
        if landed >= submission.landing {
            return Ok(Some(PlannedStep::Record(ConversionStep::Landed { landed })));
        }
        let step = if chains.nonce(account).await? > submission.nonce {
            let held = chains
                .balance(deposit.asset, account)
                .await
                .map_err(|error| ConversionError::Chain(error.reason))?;
            if held >= submission.spent {
                // Included without taking the deposit: it failed on chain,
                // and resubmitting it as is costs a fee each time.
                Some(ConversionStep::Refused {
                    reason: "the conversion was included and failed".into(),
                    psm: None,
                })
            } else if now_ms.saturating_sub(submission.submitted_at_ms) > STALL_AFTER_MS {
                // What did land on People is the user's: credit it rather
                // than fail, where getcash fails the job as a shortfall.
                Some(match landed {
                    0 => ConversionStep::Stalled,
                    landed => ConversionStep::Landed { landed },
                })
            } else {
                None
            }
        } else if chains.finalized_block() > submission.valid_until_block {
            Some(ConversionStep::Dropped)
        } else {
            None
        };
        return Ok(step.map(PlannedStep::Record));
    }
    let keypair = signer
        .deposit_keypair(&deposit.source_id, deposit.number)
        .map_err(|error| ConversionError::Chain(error.reason))?;
    let Some(keypair) = keypair.filter(|keypair| keypair.public.to_bytes() == *account) else {
        return Ok(None);
    };
    let people_before = chains.landed(account).await?;
    let nonce = chains.nonce(account).await?;
    let prepared = chains.prepare(deposit, &keypair, nonce).await?;
    Ok(Some(PlannedStep::Submit {
        submission: ConversionSubmission {
            nonce,
            submitted_at_ms: now_ms,
            valid_until_block: prepared.valid_until_block,
            people_before,
            landing: prepared.landing,
            spent: prepared.spent,
        },
        extrinsic: prepared.extrinsic,
    }))
}

/// Reads deposit-account balances on Asset Hub.
pub trait DepositBalances: Send + Sync {
    /// `account`'s balance of `asset`; zero when the account does not exist.
    fn balance<'a>(
        &'a self,
        asset: DepositAsset,
        account: &'a [u8; 32],
    ) -> BoxFuture<'a, Result<u128, GenericError>>;
}

/// Run a chain read, giving up after [`CHAIN_TIMEOUT`] so a stalled
/// connection cannot park the deposit watch.
async fn within_chain_timeout<T>(read: impl Future<Output = T>) -> Result<T, GenericError> {
    within_timeout(CHAIN_TIMEOUT, read).await
}

/// The assets `deposit`'s account is read for: its own, then the others in
/// `deposit_assets`, the native token last, so it is the stray of last
/// resort.
fn assets_to_read(deposit: &FundingDeposit, deposit_assets: &[DepositAsset]) -> Vec<DepositAsset> {
    let mut assets = vec![deposit.asset];
    assets.extend(deposit_assets.iter().filter(|other| **other != deposit.asset));
    assets.sort_by_key(|asset| *asset == DepositAsset::Native);
    assets
}

/// Everything `deposit`'s account holds of `deposit_assets`, failing if any
/// read fails.
async fn read_holdings(
    balances: &dyn DepositBalances,
    deposit: &FundingDeposit,
    deposit_assets: &[DepositAsset],
) -> Result<Vec<DepositHolding>, GenericError> {
    let mut holdings = Vec::new();
    for asset in assets_to_read(deposit, deposit_assets) {
        let balance = balances.balance(asset, &deposit.account).await?;
        if balance > 0 {
            holdings.push(DepositHolding { asset, balance });
        }
    }
    Ok(holdings)
}

/// Why a funding session could not be cancelled.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum CancelFundingError {
    /// No such session.
    #[display("no such funding session")]
    NotFound,
    /// The session cannot be cancelled now.
    #[display("{_0}")]
    Refused(CancelRefusal),
    /// The deposit account could not be read in time, so nothing was
    /// cancelled.
    #[display("could not confirm the deposit account is empty; try again")]
    Unconfirmed,
    /// The cancel could not be saved.
    #[display("{_0}")]
    Storage(FundingSessionError),
}

impl From<FundingSessionError> for CancelFundingError {
    fn from(error: FundingSessionError) -> Self {
        Self::Storage(error)
    }
}

/// Run `work`, giving up after `limit`.
async fn within_timeout<T>(limit: Duration, work: impl Future<Output = T>) -> Result<T, GenericError> {
    let work = work.fuse();
    let timeout = futures_timer::Delay::new(limit).fuse();
    futures::pin_mut!(work, timeout);
    futures::select! {
        value = work => Ok(value),
        () = timeout => Err(GenericError {
            reason: format!("timed out after {}s", limit.as_secs()),
        }),
    }
}

/// Why a deposit account could not be assigned.
#[derive(Debug, derive_more::Display)]
pub enum AssignDepositError {
    /// No such session.
    #[display("no such funding session")]
    NotFound,
    /// The session is not an open inbound one without a deposit.
    #[display("funding session is not awaiting a deposit account")]
    NotAwaitingDeposit,
    /// No signing host converts deposits on this runtime.
    #[display("this host does not convert funding deposits")]
    ConversionUnavailable,
    /// No route turns this asset into CASH.
    #[display("no route converts this deposit into CASH")]
    NoRoute,
    /// The chain refused to price the deposit.
    #[display("{_0}")]
    Refused(String),
    /// The session names no amount to quote a deposit for.
    #[display("the funding session names no amount")]
    NoAmount,
    /// The provider would deliver less than it takes to credit the session's
    /// amount.
    #[display("the deposit must be at least {needed} to credit the amount")]
    DepositTooSmall {
        /// Least deposit, in the asset's units.
        needed: u128,
    },
    /// Every account tried already holds funds.
    #[display("every funding account tried already holds funds")]
    AccountsInUse,
    /// The account could not be derived.
    #[display("{}", _0.reason)]
    Derive(GenericError),
    /// Asset Hub could not be read.
    #[display("{}", _0.reason)]
    Chain(GenericError),
    /// The session could not be stored.
    #[display("{_0}")]
    Session(FundingSessionError),
}

impl AssignDepositError {
    /// A conversion error as an assignment error: a refusal stays one.
    fn from_conversion(error: ConversionError) -> Self {
        match error {
            ConversionError::Refused(reason) | ConversionError::PsmRefused { reason, .. } => {
                Self::Refused(reason)
            }
            ConversionError::Chain(reason) => Self::Chain(GenericError { reason }),
        }
    }
}

impl From<FundingSessionError> for AssignDepositError {
    fn from(error: FundingSessionError) -> Self {
        Self::Session(error)
    }
}

/// Whether `session` can take a deposit account.
fn assignable(session: &FundingSession) -> Result<(), AssignDepositError> {
    let awaiting = session.direction == FundingDirection::In
        && session.stage == FundingStage::Open
        && session.deposit.is_none();
    awaiting
        .then_some(())
        .ok_or(AssignDepositError::NotAwaitingDeposit)
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
                        // Ended sessions the host has not recorded are handed
                        // over again, so its history gets every outcome.
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
                Ok(()) => {
                    registry.keep_expiring(&services);
                    services.watch_funding_deposits();
                }
                Err(error) => tracing::warn!(%error, "loading funding sessions failed"),
            }
        }));
    }

    /// Give an open inbound session its deposit account for the request's
    /// source, fix the route that will convert it, and watch the account until
    /// the expected balance arrives. Returns the account the provider pays
    /// into.
    pub async fn assign_funding_deposit(
        self: &Arc<Self>,
        intent: &str,
        request: DepositRequest,
        derive: impl Fn(u32) -> Result<[u8; 32], GenericError>,
    ) -> Result<[u8; 32], AssignDepositError> {
        let session = self
            .funding()
            .get(intent)
            .ok_or(AssignDepositError::NotFound)?;
        assignable(&session)?;
        let network = self.funding_network()?;
        let quoting = session.amount.is_some() && session.quote.is_none();
        let chains = self
            .funding_chains(network, if quoting { SignOn::AssetHub } else { SignOn::Nowhere })
            .await
            .map_err(AssignDepositError::from_conversion)?;
        // A session that names its amount must be paid enough to credit it,
        // judged against the quote the provider was given; one that does not
        // takes what the provider delivers.
        let route = match (session.amount, session.quoted_route(request.asset, request.expected)) {
            (_, Some(judged)) => {
                judged.map_err(|needed| AssignDepositError::DepositTooSmall { needed })?
            }
            (Some(target), None) => {
                let quote = self.record_quote(intent, &chains, request.asset, target).await?;
                if request.expected < quote.deposit {
                    return Err(AssignDepositError::DepositTooSmall {
                        needed: quote.deposit,
                    });
                }
                quote.route
            }
            (None, None) => within_chain_timeout(chains.choose_route(request.asset, request.expected))
                .await
                .map_err(AssignDepositError::Chain)?
                .map_err(AssignDepositError::from_conversion)?
                .ok_or(AssignDepositError::NoRoute)?,
        };
        let account = self
            .funding()
            .assign_empty_deposit(
                self.platform.as_ref(),
                &chains,
                current_unix_millis(),
                intent,
                DepositPlan { request, route },
                derive,
            )
            .await?;
        self.watch_funding_deposits();
        Ok(account)
    }

    /// Cancel session `intent` while nothing has arrived, as getcash does:
    /// a session with a deposit account has it read first, and is not
    /// cancelled unless the read confirms it holds nothing. The read is at
    /// finalized blocks, as every funding read is; a payment still short of
    /// finality when the cancel goes through is converted by the late watch
    /// once it reaches the gate, and shown for accepting if it does not.
    pub async fn cancel_funding(self: &Arc<Self>, intent: &str) -> Result<(), CancelFundingError> {
        let registry = self.funding();
        let session = registry.get(intent).ok_or(CancelFundingError::NotFound)?;
        let confirmed_withdrawal = match session.withdrawal.as_ref().filter(|_| session.payment_pending()) {
            Some(withdrawal) => {
                self.confirm_payment_untaken(intent, withdrawal).await?;
                true
            }
            None => false,
        };
        let holdings = match (&session.deposit, registry.conversion.get()) {
            (None, _) => None,
            (Some(_), None) => return Err(CancelFundingError::Unconfirmed),
            (Some(deposit), Some(conversion)) => {
                let read = within_timeout(CANCEL_CONFIRM, async {
                    let chains = self
                        .funding_chains(conversion.network, SignOn::Nowhere)
                        .await
                        .map_err(|error| GenericError {
                            reason: error.to_string(),
                        })?;
                    read_holdings(&chains, deposit, &conversion.deposit_assets).await
                })
                .await
                .and_then(|read| read);
                match read {
                    Ok(holdings) => Some(holdings),
                    Err(GenericError { reason }) => {
                        tracing::warn!(%intent, %reason, "confirming a funding cancel failed");
                        return Err(CancelFundingError::Unconfirmed);
                    }
                }
            }
        };
        registry
            .cancel(
                self.platform.as_ref(),
                current_unix_millis(),
                intent,
                holdings,
                confirmed_withdrawal,
            )
            .await
    }

    /// End session `intent` with `stage` as though its funds had moved, for
    /// test hosts that drive the flow with no chain behind it. Returns
    /// whether it was still in flight.
    #[cfg(feature = "test-host")]
    pub async fn settle_funding_for_test(
        &self,
        intent: &str,
        stage: FundingStage,
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

    /// Try a failed session `intent` again from where its funds are: its
    /// conversion if the deposit is still on Asset Hub, its crediting if the
    /// CASH is on People.
    pub async fn retry_funding(self: &Arc<Self>, intent: &str) -> Result<(), AssignDepositError> {
        let intent = intent.to_string();
        let now_ms = current_unix_millis();
        let retrying = intent.clone();
        self.funding()
            .commit(self.platform.as_ref(), now_ms, move |sessions| {
                let retried = sessions
                    .get_mut(&retrying)
                    .ok_or(AssignDepositError::NotFound)
                    .and_then(|session| {
                        session
                            .retry(now_ms)
                            .map_err(|refusal| AssignDepositError::Refused(refusal.to_string()))
                    });
                let changed = if retried.is_ok() { vec![retrying] } else { Vec::new() };
                (retried, changed)
            })
            .await??;
        // A payment asked for again needs the user's approval again.
        let asks_payment = self
            .funding()
            .get(&intent)
            .is_some_and(|session| session.stage == FundingStage::Open && session.withdrawal.is_some());
        self.watch_funding_deposits();
        if asks_payment {
            self.request_withdrawal_payment(&intent)
                .await
                .map_err(|error| AssignDepositError::Refused(error.to_string()))?;
        }
        Ok(())
    }

    /// Convert what arrived of `asset` on session `intent`'s deposit account,
    /// by the route that serves that much of it.
    pub async fn accept_funding_deposit(
        self: &Arc<Self>,
        intent: &str,
        asset: DepositAsset,
    ) -> Result<(), AssignDepositError> {
        let arrived = match self
            .funding()
            .get(intent)
            .ok_or(AssignDepositError::NotFound)?
            .deposit_mismatch()
        {
            Some(DepositMismatch::Short { asset: arrived, amount } | DepositMismatch::WrongAsset { asset: arrived, amount })
                if arrived == asset =>
            {
                amount
            }
            Some(_) => return Err(AssignDepositError::Refused(AcceptRefusal::NotMismatched.to_string())),
            None => return Err(AssignDepositError::Refused(AcceptRefusal::NothingArrived.to_string())),
        };
        let chains = self
            .funding_chains(self.funding_network()?, SignOn::Nowhere)
            .await
            .map_err(AssignDepositError::from_conversion)?;
        let route = within_chain_timeout(chains.choose_route(asset, arrived))
            .await
            .map_err(AssignDepositError::Chain)?
            .map_err(AssignDepositError::from_conversion)?
            .ok_or(AssignDepositError::NoRoute)?;
        let intent = intent.to_string();
        self.funding()
            .commit(self.platform.as_ref(), current_unix_millis(), move |sessions| {
                let accepted = sessions
                    .get_mut(&intent)
                    .ok_or(AssignDepositError::NotFound)
                    .and_then(|session| {
                        session
                            .accept_arrival(asset, arrived, route, current_unix_millis())
                            .map_err(|refusal| AssignDepositError::Refused(refusal.to_string()))
                    });
                let changed = if accepted.is_ok() { vec![intent] } else { Vec::new() };
                (accepted, changed)
            })
            .await??;
        self.watch_funding_deposits();
        Ok(())
    }

    /// The deposit of `asset` a provider must deliver to credit the amount
    /// session `intent` names.
    pub async fn quote_funding_deposit(
        self: &Arc<Self>,
        intent: &str,
        asset: DepositAsset,
    ) -> Result<u128, AssignDepositError> {
        let session = self
            .funding()
            .get(intent)
            .ok_or(AssignDepositError::NotFound)?;
        let target = session.amount.ok_or(AssignDepositError::NoAmount)?;
        let chains = self
            .funding_chains(self.funding_network()?, SignOn::AssetHub)
            .await
            .map_err(AssignDepositError::from_conversion)?;
        self.record_quote(intent, &chains, asset, target)
            .await
            .map(|quote| quote.deposit)
    }

    /// Quote `target` in `asset` and freeze the quote on session `intent`,
    /// so the deposit is later held to the figure the provider was given.
    async fn record_quote(
        &self,
        intent: &str,
        chains: &Chains,
        asset: DepositAsset,
        target: u128,
    ) -> Result<DepositQuote, AssignDepositError> {
        let quote = within_chain_timeout(chains.deposit_quote(asset, target))
            .await
            .map_err(AssignDepositError::Chain)?
            .map_err(AssignDepositError::from_conversion)?
            .ok_or(AssignDepositError::NoRoute)?;
        let intent = intent.to_string();
        self.funding()
            .commit(self.platform.as_ref(), current_unix_millis(), move |sessions| {
                if let Some(session) = sessions.get_mut(&intent) {
                    session.quote = Some(quote);
                }
                ((), Vec::new())
            })
            .await?;
        Ok(quote)
    }

    fn funding_network(&self) -> Result<FundingNetwork, AssignDepositError> {
        self.funding()
            .conversion
            .get()
            .map(|conversion| conversion.network)
            .ok_or(AssignDepositError::ConversionUnavailable)
    }

    /// Keep one task polling the awaited deposits while any is awaited. The
    /// task ends once none is, or the services are dropped.
    pub fn watch_funding_deposits(self: &Arc<Self>) {
        let registry = self.funding();
        if registry.watching.swap(true, Ordering::AcqRel) {
            return;
        }
        let watched = Arc::downgrade(registry);
        let services = Arc::downgrade(self);
        (self.spawner)(Box::pin(async move {
            while FundingRegistry::still_watching(&watched) {
                futures_timer::Delay::new(DEPOSIT_POLL).await;
                let Some(services) = services.upgrade() else {
                    return;
                };
                if let Err(reason) = services.advance_chain_sessions().await {
                    tracing::warn!(%reason, "funding deposit watch failed");
                }
                if let Err(reason) = services.advance_credits().await {
                    tracing::warn!(%reason, "funding credit pass failed");
                }
                services.advance_withdrawals().await;
                if let Err(reason) = services.advance_withdrawal_moves().await {
                    tracing::warn!(%reason, "funding withdrawal move failed");
                }
                if let Err(reason) = services.advance_payouts().await {
                    tracing::warn!(%reason, "funding payout pass failed");
                }
            }
        }));
    }

    /// Asset Hub and People, pinned at their latest finalized blocks, with the
    /// signed-extension metadata when `signing`.
    async fn funding_chains(
        &self,
        network: FundingNetwork,
        signing: SignOn,
    ) -> Result<Chains, ConversionError> {
        within_chain_timeout(async {
            let failed = |error: &dyn core::fmt::Display| ConversionError::Chain(error.to_string());
            let chains = features::supported_chains(self.platform.as_ref())
                .await
                .map_err(|error| ConversionError::Chain(error.reason))?;
            let genesis = |chain: ChainIdentifier| {
                features::genesis_for(&chains, chain)
                    .ok_or_else(|| ConversionError::Chain(format!("the host serves no {chain:?}")))
            };
            let (asset_hub_genesis, people_genesis) =
                (genesis(ChainIdentifier::AssetHub)?, genesis(ChainIdentifier::People)?);
            let asset_hub = self
                .chain
                .online_client(&asset_hub_genesis)
                .await
                .map_err(|error| failed(&error))?;
            let people = self
                .chain
                .online_client(&people_genesis)
                .await
                .map_err(|error| failed(&error))?;
            let signing = match signing {
                SignOn::Nowhere => None,
                SignOn::AssetHub => Some(SigningChain::AssetHub(self.signing_metadata(asset_hub_genesis).await?)),
                SignOn::People => Some(SigningChain::People(self.signing_metadata(people_genesis).await?)),
            };
            Chains::at_finalized(&asset_hub, &people, network, signing).await
        })
        .await
        .map_err(|error| ConversionError::Chain(error.reason))?
    }

    /// A chain's signed-extension metadata from the per-chain cache the
    /// allowance path keeps, so signing never downloads it again.
    async fn signing_metadata(&self, genesis: [u8; 32]) -> Result<ChainContext, ConversionError> {
        let failed = |error: &dyn core::fmt::Display| ConversionError::Chain(error.to_string());
        let rpc = RpcClient::new(subxt_rpcs::RpcClient::new(
            self.chain
                .rpc_client("funding conversion", &genesis)
                .await
                .map_err(|error| failed(&error))?,
        ));
        self.chain_context
            .get(&ChainClient::new(rpc, genesis))
            .await
            .map_err(|error| failed(&error))
    }

    /// One pass over the sessions whose CASH landed: register top-ups and
    /// record what they credited. Waits while the host has no top-up.
    async fn advance_credits(self: &Arc<Self>) -> Result<(), String> {
        let registry = self.funding();
        let (Some(conversion), Some(top_up)) = (registry.conversion.get(), self.top_up_platform())
        else {
            return Ok(());
        };
        let crediting = registry.crediting_sessions();
        if crediting.is_empty() {
            return Ok(());
        }
        let product = ProductContext {
            product_id: conversion.signer.funding_product_id(),
            execution_kind: Default::default(),
        };
        let credit = credit::Credit {
            top_up: top_up.as_ref(),
            signer: conversion.signer.as_ref(),
            product: &product,
        };
        // Each top-up is sized from what the account holds on People when
        // it starts, as getcash sizes its claims.
        let chains = if crediting.iter().any(CreditingSession::sizing) {
            match self.funding_chains(conversion.network, SignOn::Nowhere).await {
                Ok(chains) => Some(chains),
                Err(error) => {
                    tracing::warn!(%error, "reading funding accounts on People failed");
                    None
                }
            }
        } else {
            None
        };
        for session in crediting {
            let held = match &chains {
                Some(chains) if session.sizing() => {
                    let read = within_chain_timeout(chains.landed(&session.deposit.account)).await;
                    match read
                        .map_err(|error| error.reason)
                        .and_then(|held| held.map_err(|error| error.to_string()))
                    {
                        Ok(held) => Some(held),
                        Err(reason) => {
                            tracing::warn!(intent = %session.intent, %reason, "reading a funding account on People failed");
                            None
                        }
                    }
                }
                _ => None,
            };
            let CreditingSession {
                intent,
                deposit,
                progress,
            } = session;
            let now_ms = current_unix_millis();
            match credit.plan(&deposit, progress, held, now_ms).await {
                Ok(Some(step)) => registry
                    .record_credit(self.platform.as_ref(), now_ms, &intent, step)
                    .await
                    .map_err(|error| error.to_string())?,
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%intent, reason = %error.reason, "funding credit pass failed");
                }
            }
        }
        Ok(())
    }

    /// One pass over the sessions that need the chains: read the awaited
    /// deposits, then advance the conversions, against one pair of finalized
    /// blocks.
    async fn advance_chain_sessions(self: &Arc<Self>) -> Result<(), String> {
        let registry = self.funding();
        let Some(conversion) = registry.conversion.get() else {
            return Ok(());
        };
        let (pending, signing) = {
            let sessions = registry.lock_sessions();
            let now_ms = current_unix_millis();
            let pending = sessions.values().any(|session| {
                session.watched_deposit(now_ms).is_some() || session.converting().is_some()
            });
            let signing = sessions
                .values()
                .any(|session| matches!(session.converting(), Some((_, None))));
            (pending, signing)
        };
        if !pending {
            return Ok(());
        }
        let chains = self
            .funding_chains(conversion.network, if signing { SignOn::AssetHub } else { SignOn::Nowhere })
            .await
            .map_err(|error| error.to_string())?;
        let observed = registry
            .observe_deposits(
                self.platform.as_ref(),
                current_unix_millis(),
                &chains,
                &conversion.deposit_assets,
            )
            .await;
        if let Err(error) = observed {
            tracing::warn!(%error, "recording funding deposits failed");
        }
        self.advance_conversions(&chains, conversion).await
    }

    /// One pass over the sessions being converted: record what landed,
    /// what was dropped or stalled, and submit what is ready.
    async fn advance_conversions(
        self: &Arc<Self>,
        chains: &Chains,
        conversion: &Conversion,
    ) -> Result<(), String> {
        let registry = self.funding();
        let converting = registry.converting_sessions();
        let storage = self.platform.as_ref();
        for (intent, deposit, submission) in converting {
            let now_ms = current_unix_millis();
            let planned = within_chain_timeout(plan_conversion(
                chains,
                conversion.signer.as_ref(),
                &deposit,
                submission,
                now_ms,
            ))
            .await;
            let planned = match planned {
                Ok(Ok(planned)) => planned,
                Ok(Err(ConversionError::Refused(reason))) => {
                    Some(PlannedStep::Record(ConversionStep::Refused { reason, psm: None }))
                }
                Ok(Err(ConversionError::PsmRefused { reason, refusal })) => {
                    Some(PlannedStep::Record(ConversionStep::Refused {
                        reason,
                        psm: Some(refusal),
                    }))
                }
                Ok(Err(ConversionError::Chain(reason))) | Err(GenericError { reason }) => {
                    tracing::warn!(%intent, %reason, "funding conversion pass failed");
                    continue;
                }
            };
            let recorded = match planned {
                None => Ok(()),
                Some(PlannedStep::Record(step)) => {
                    registry.record_conversion(storage, now_ms, &intent, step).await
                }
                Some(PlannedStep::Submit {
                    submission,
                    extrinsic,
                }) => {
                    let submitted = ConversionStep::Submitted(submission);
                    registry
                        .record_conversion(storage, now_ms, &intent, submitted)
                        .await
                        .map_err(|error| error.to_string())?;
                    match chains.submit(extrinsic).await {
                        Ok(()) => Ok(()),
                        Err(ConversionError::Refused(reason) | ConversionError::PsmRefused { reason, .. }) => {
                            let refused = ConversionStep::Refused { reason, psm: None };
                            registry
                                .record_conversion(storage, current_unix_millis(), &intent, refused)
                                .await
                        }
                        // It may have reached the chain anyway; the
                        // submission stays, and its era or the nonce decides.
                        Err(ConversionError::Chain(reason)) => {
                            tracing::warn!(%intent, %reason, "submitting a funding conversion failed");
                            Ok(())
                        }
                    }
                }
            };
            recorded.map_err(|error| error.to_string())?;
        }
        Ok(())
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

    use crate::host_logic::funding::{FundingStage, FundingStamp, FundingStep};
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
                resume: None,
            })
        );
    }

    // The host's history must get each outcome exactly once: an ended
    // session is handed over until the host records it, and one whose late
    // deposit reopens it is handed over again when it ends.
    #[test]
    fn an_ended_session_is_handed_over_until_the_host_records_it() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        let mut ended = FundingSession {
            deposit: Some(converting_deposit()),
            ..session("fs_ended", NOW - DAY_MS)
        };
        ended.fail(FundingFailure::Expired, NOW);
        insert(&registry, storage.as_ref(), ended);
        insert(&registry, storage.as_ref(), session("fs_live", NOW));
        let acknowledge = |intent| block_on(registry.acknowledge(storage.as_ref(), NOW, intent)).expect("saved");
        let handed_over = || {
            let mut pending: Vec<_> = registry
                .sessions()
                .into_iter()
                .filter(FundingSession::needs_handoff)
                .map(|session| session.intent)
                .collect();
            pending.sort();
            pending
        };

        let before = handed_over();
        let first = (acknowledge("fs_ended"), acknowledge("fs_ended"), acknowledge("fs_live"));
        let after = handed_over();
        // The full deposit arrives late, so the session converts after all.
        let late = AssetBalances(vec![(USDT, converting_deposit().account, 50)]);
        block_on(registry.observe_deposits(storage.as_ref(), NOW + 1, &late, &[USDT])).expect("reopened");

        assert_eq!(
            (before, first, after, registry.get("fs_ended").map(|session| session.acknowledged)),
            (
                vec!["fs_ended".to_string(), "fs_live".to_string()],
                (true, false, false),
                vec!["fs_live".to_string()],
                Some(false),
            )
        );
    }

    // Progress is drawn from when each step was first reached, recorded as
    // the session moves, and going back through a step, as a resubmitted
    // conversion does, keeps its first time.
    #[test]
    fn each_step_is_stamped_as_the_session_reaches_it() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        insert(
            &registry,
            storage.as_ref(),
            FundingSession {
                deposit: Some(converting_deposit()),
                ..session("fs_1", NOW)
            },
        );
        let usdt = AssetBalances(vec![(USDT, converting_deposit().account, 50)]);
        block_on(registry.observe_deposits(storage.as_ref(), NOW + 1, &usdt, &[USDT])).expect("seen");
        for (at_ms, step) in [
            (NOW + 2, ConversionStep::Submitted(SUBMITTED)),
            (NOW + 3, ConversionStep::Dropped),
            (NOW + 4, ConversionStep::Submitted(SUBMITTED)),
            (NOW + 5, ConversionStep::Landed { landed: 49 }),
        ] {
            block_on(registry.record_conversion(storage.as_ref(), at_ms, "fs_1", step)).expect("recorded");
        }

        assert_eq!(
            registry.get("fs_1").map(|session| (session.stamps, session.landed)),
            Some((
                [
                    (FundingStep::AwaitingDeposit, NOW),
                    (FundingStep::DepositSeen, NOW + 1),
                    (FundingStep::Converting, NOW + 2),
                    (FundingStep::Landed, NOW + 5),
                ]
                .map(|(step, at_ms)| FundingStamp { step, at_ms })
                .to_vec(),
                Some(49)
            ))
        );
    }

    // The fresh read a cancel takes is recorded first, so funds that arrived
    // since the last pass refuse the cancel and show on the session.
    #[test]
    fn a_cancel_is_refused_by_what_the_fresh_read_finds() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        insert(
            &registry,
            storage.as_ref(),
            FundingSession {
                deposit: Some(converting_deposit()),
                ..session("fs_1", NOW)
            },
        );
        let stray = DepositHolding {
            asset: DepositAsset::Asset(1337),
            balance: 3,
        };

        let refused = block_on(registry.cancel(storage.as_ref(), NOW, "fs_1", Some(vec![stray]), false));
        let holdings = || registry.get("fs_1").and_then(|session| session.deposit).map(|deposit| deposit.holdings);
        let after_fresh_read = holdings();
        // A read taken before the watch recorded the stray must not erase it.
        let stale = block_on(registry.cancel(storage.as_ref(), NOW, "fs_1", Some(Vec::new()), false));

        assert_eq!(
            (refused, after_fresh_read, stale, holdings()),
            (
                Err(CancelFundingError::Refused(CancelRefusal::FundsArrived)),
                Some(vec![stray]),
                Err(CancelFundingError::Refused(CancelRefusal::FundsArrived)),
                Some(vec![stray]),
            )
        );
    }

    // A full deposit the cancel's read finds is converted, not cancelled,
    // and an account assigned after the read is not cancelled unread.
    #[test]
    fn a_cancel_converts_a_deposit_it_finds_and_never_cancels_unread() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        insert(
            &registry,
            storage.as_ref(),
            FundingSession {
                deposit: Some(converting_deposit()),
                ..session("fs_1", NOW)
            },
        );
        insert(
            &registry,
            storage.as_ref(),
            FundingSession {
                deposit: Some(converting_deposit()),
                ..session("fs_2", NOW)
            },
        );
        let paid = DepositHolding {
            asset: USDT,
            balance: 50,
        };

        assert_eq!(
            (
                block_on(registry.cancel(storage.as_ref(), NOW, "fs_1", Some(vec![paid]), false)),
                registry.get("fs_1").map(|session| session.step()),
                block_on(registry.cancel(storage.as_ref(), NOW, "fs_2", None, false)),
            ),
            (
                Err(CancelFundingError::Refused(CancelRefusal::Underway)),
                Some(FundingStep::DepositSeen),
                Err(CancelFundingError::Unconfirmed),
            )
        );
    }

    // A withdrawal is cancelled only after its account and payment were read
    // fresh, so one assigned after the cancel looked is not cancelled unread.
    #[test]
    fn a_withdrawal_is_never_cancelled_unread() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        let mut withdrawing = FundingSession {
            direction: FundingDirection::Out,
            amount: Some(1_000),
            ..session("fs_1", NOW)
        };
        withdrawing.assign_withdrawal(crate::host_logic::funding::FundingWithdrawal {
            destination_id: "dot-assethub".into(),
            number: 1,
            account: [4; 32],
            attempt: 0,
            since_ms: NOW,
            taken: false,
            payout: None,
        });
        insert(&registry, storage.as_ref(), withdrawing);

        assert_eq!(
            (
                block_on(registry.cancel(storage.as_ref(), NOW, "fs_1", None, false)),
                block_on(registry.cancel(storage.as_ref(), NOW, "fs_1", None, true)),
            ),
            (Err(CancelFundingError::Unconfirmed), Ok(()))
        );
    }

    // The host's list shows what is in flight first, then what ended, each
    // newest first, as getcash lists its requests.
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

    const USDT: DepositAsset = DepositAsset::Asset(1984);

    /// Balances keyed by account; any other account is empty.
    struct Balances(HashMap<[u8; 32], u128>);

    impl DepositBalances for Balances {
        fn balance<'a>(
            &'a self,
            _asset: DepositAsset,
            account: &'a [u8; 32],
        ) -> BoxFuture<'a, Result<u128, GenericError>> {
            Box::pin(async move { Ok(self.0.get(account).copied().unwrap_or(0)) })
        }
    }

    fn plan(expected: u128) -> DepositPlan {
        DepositPlan {
            request: request(expected),
            route: ConversionRoute::Teleport,
        }
    }

    fn request(expected: u128) -> DepositRequest {
        DepositRequest {
            source_id: "usdt-assethub".to_string(),
            asset: USDT,
            expected,
        }
    }

    fn account(number: u32) -> [u8; 32] {
        [u8::try_from(number).expect("small"); 32]
    }

    fn assign(
        registry: &FundingRegistry,
        storage: &dyn CoreStorage,
        balances: &Balances,
        intent: &str,
    ) -> Result<[u8; 32], String> {
        block_on(registry.assign_empty_deposit(storage, balances, NOW, intent, plan(50), |n| {
            Ok(account(n))
        }))
        .map_err(|error| error.to_string())
    }

    // A restored seed starts its counters again, and a provider may still pay
    // into an account handed out before, so assignment passes over any
    // account that already holds the asset.
    #[test]
    fn assignment_skips_accounts_that_already_hold_funds() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        insert(&registry, storage.as_ref(), session("fs_1", NOW));
        let balances = Balances(HashMap::from([(account(1), 7), (account(2), 1)]));

        let assigned = assign(&registry, storage.as_ref(), &balances, "fs_1");

        assert_eq!(
            (
                assigned,
                registry.get("fs_1").and_then(|session| session.deposit)
            ),
            (
                Ok(account(3)),
                Some(FundingDeposit {
                    source_id: "usdt-assethub".to_string(),
                    number: 3,
                    asset: USDT,
                    account: account(3),
                    expected: 50,
                    route: ConversionRoute::Teleport,
                    target: Some(100),
                    holdings: Vec::new(),
                })
            )
        );
    }

    // Numbers are never reused, so one burned on a session that cannot take
    // an account is gone for good.
    #[test]
    fn assignment_refuses_without_spending_a_number() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        let outbound = FundingSession {
            direction: FundingDirection::Out,
            ..session("fs_out", NOW)
        };
        insert(&registry, storage.as_ref(), outbound);
        insert(&registry, storage.as_ref(), session("fs_in", NOW));
        let empty = Balances(HashMap::new());

        let refused = [
            assign(&registry, storage.as_ref(), &empty, "fs_missing"),
            assign(&registry, storage.as_ref(), &empty, "fs_out"),
        ];
        let first = assign(&registry, storage.as_ref(), &empty, "fs_in");
        let again = assign(&registry, storage.as_ref(), &empty, "fs_in");

        assert_eq!(
            (refused, first, again),
            (
                [
                    Err("no such funding session".to_string()),
                    Err("funding session is not awaiting a deposit account".to_string()),
                ],
                Ok(account(1)),
                Err("funding session is not awaiting a deposit account".to_string()),
            )
        );
    }

    // Converting starts only once the whole expected balance is on chain, and
    // the stage survives a restart, so a deposit is converted exactly once.
    #[test]
    fn a_covering_deposit_moves_the_session_to_converting() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        insert(&registry, storage.as_ref(), session("fs_1", NOW));
        assign(
            &registry,
            storage.as_ref(),
            &Balances(HashMap::new()),
            "fs_1",
        )
        .expect("assigned");
        let stream = registry.subscribe("fs_1").expect("session exists");

        for held in [49, 50] {
            let balances = Balances(HashMap::from([(account(1), held)]));
            block_on(registry.observe_deposits(storage.as_ref(), NOW, &balances, &[]))
                .expect("observed");
        }
        let restarted = FundingRegistry::default();
        block_on(restarted.commit(storage.as_ref(), NOW, |_| ((), Vec::new()))).expect("loaded");

        assert_eq!(
            (
                block_on(stream.take(2).collect::<Vec<_>>()),
                restarted.get("fs_1").map(|session| session.stage),
            ),
            (
                vec![
                    HostFundingStatusSubscribeItem::AwaitingDeposit {
                        expires_at: Some(NOW + DAY_MS),
                    },
                    HostFundingStatusSubscribeItem::Converting,
                ],
                Some(FundingStage::Converting {
                    deposited: 50,
                    refusals: 0,
                    submission: None,
                }),
            )
        );
    }

    // A provider may pay moments before the deadline, and finality and the
    // poll both lag, so a session with a deposit account ends only on a read
    // taken after the deadline: converting if the funds made it, expired if
    // not. The sweep alone never strands a payment.
    #[test]
    fn a_deposit_session_expires_only_on_a_read_after_its_deadline() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        let empty = Balances(HashMap::new());
        for intent in ["fs_late", "fs_never"] {
            insert(&registry, storage.as_ref(), session(intent, NOW));
            assign(&registry, storage.as_ref(), &empty, intent).expect("assigned");
        }
        let past_deadline = NOW + DAY_MS;

        block_on(registry.commit(storage.as_ref(), past_deadline, |_| ((), Vec::new())))
            .expect("swept");
        let after_sweep = [registry.get("fs_late"), registry.get("fs_never")]
            .map(|session| session.map(|session| session.stage));
        let late_payment = Balances(HashMap::from([(account(1), 50)]));
        block_on(registry.observe_deposits(storage.as_ref(), past_deadline, &late_payment, &[]))
            .expect("observed");

        assert_eq!(
            (
                after_sweep,
                [registry.get("fs_late"), registry.get("fs_never")]
                    .map(|session| session.map(|session| session.stage)),
            ),
            (
                [Some(FundingStage::Open), Some(FundingStage::Open)],
                [
                    Some(FundingStage::Converting {
                    deposited: 50,
                    refusals: 0,
                    submission: None,
                }),
                    Some(FundingStage::Failed {
                        reason: FundingFailure::Expired,
                        settled_at_ms: past_deadline,
                        resume: None,
                    }),
                ],
            )
        );
    }

    // The deposit is already on chain, so the deposit window no longer
    // applies, and the expiry sweep must not wait on a deadline that passed.
    #[test]
    fn a_converting_session_does_not_expire() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        let converting = FundingSession {
            stage: FundingStage::Converting {
                    deposited: 50,
                    refusals: 0,
                    submission: None,
                },
            ..session("fs_1", NOW - DAY_MS)
        };
        insert(&registry, storage.as_ref(), converting.clone());

        block_on(registry.commit(storage.as_ref(), NOW, |_| ((), Vec::new()))).expect("swept");

        assert_eq!(
            (registry.get("fs_1").map(|session| session.stage), registry.next_deadline()),
            (Some(converting.stage), None)
        );
    }

    /// Chains answering fixed reads, and preparing a fixed conversion.
    struct Scripted {
        landed: u128,
        nonce: u32,
        balance: u128,
        block: u64,
    }

    const PREPARED: Prepared = Prepared {
        extrinsic: Vec::new(),
        valid_until_block: 164,
        landing: 40,
        spent: 50,
    };

    impl DepositBalances for Scripted {
        fn balance<'a>(
            &'a self,
            _: DepositAsset,
            _: &'a [u8; 32],
        ) -> BoxFuture<'a, Result<u128, GenericError>> {
            Box::pin(async move { Ok(self.balance) })
        }
    }

    impl ConversionChains for Scripted {
        fn landed<'a>(&'a self, _: &'a [u8; 32]) -> BoxFuture<'a, Result<u128, ConversionError>> {
            Box::pin(async move { Ok(self.landed) })
        }

        fn nonce<'a>(&'a self, _: &'a [u8; 32]) -> BoxFuture<'a, Result<u32, ConversionError>> {
            Box::pin(async move { Ok(self.nonce) })
        }

        fn finalized_block(&self) -> u64 {
            self.block
        }

        fn prepare<'a>(
            &'a self,
            _: &'a FundingDeposit,
            _: &'a schnorrkel::Keypair,
            _: u32,
        ) -> BoxFuture<'a, Result<Prepared, ConversionError>> {
            Box::pin(async { Ok(PREPARED) })
        }
    }

    struct Keys(Option<schnorrkel::Keypair>);

    impl FundingSigner for Keys {
        fn deposit_keypair(
            &self,
            _: &str,
            _: u32,
        ) -> Result<Option<schnorrkel::Keypair>, GenericError> {
            Ok(self.0.clone())
        }

        fn withdrawal_keypair(
            &self,
            _: &str,
            _: u32,
        ) -> Result<Option<schnorrkel::Keypair>, GenericError> {
            Ok(self.0.clone())
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

    fn converting_deposit() -> FundingDeposit {
        FundingDeposit {
            source_id: "usdt-assethub".to_string(),
            number: 1,
            asset: USDT,
            account: keypair(1).public.to_bytes(),
            expected: 50,
            route: ConversionRoute::Teleport,
            target: None,
            holdings: Vec::new(),
        }
    }

    const SUBMITTED: ConversionSubmission = ConversionSubmission {
        nonce: 4,
        submitted_at_ms: NOW,
        valid_until_block: 164,
        people_before: 10,
        landing: 40,
        spent: 50,
    };

    fn next_step(chains: Scripted, submission: Option<ConversionSubmission>, now_ms: u64) -> Option<PlannedStep> {
        block_on(plan_conversion(
            &chains,
            &Keys(Some(keypair(1))),
            &converting_deposit(),
            submission,
            now_ms,
        ))
        .expect("planned")
    }

    // Anyone can send CASH to the account on People, so only CASH on top of
    // what was there before, and at least what the conversion lands, ends it.
    #[test]
    fn only_the_conversions_own_cash_counts_as_landed() {
        let reading = |landed| Scripted {
            landed,
            nonce: 4,
            balance: 50,
            block: 100,
        };

        assert_eq!(
            [
                next_step(reading(11), Some(SUBMITTED), NOW),
                next_step(reading(55), Some(SUBMITTED), NOW),
            ],
            [
                None,
                Some(PlannedStep::Record(ConversionStep::Landed { landed: 45 })),
            ]
        );
    }

    // Resubmitting while the first transaction can still land would convert
    // twice, so a submission is dropped only once it provably cannot: its era
    // passed unincluded, or it was included and left the deposit in place,
    // which counts as a refusal so a failing conversion is not paid for
    // forever.
    #[test]
    fn a_submission_is_dropped_only_once_it_cannot_convert() {
        let chains = |nonce, balance, block| Scripted {
            landed: 10,
            nonce,
            balance,
            block,
        };
        let late = NOW + STALL_AFTER_MS + 1;

        assert_eq!(
            [
                next_step(chains(4, 50, 164), Some(SUBMITTED), late),
                next_step(chains(4, 50, 165), Some(SUBMITTED), NOW),
                next_step(chains(5, 50, 100), Some(SUBMITTED), NOW),
                next_step(chains(5, 1, 100), Some(SUBMITTED), NOW),
                next_step(chains(5, 1, 100), Some(SUBMITTED), late),
            ],
            [
                None,
                Some(PlannedStep::Record(ConversionStep::Dropped)),
                Some(PlannedStep::Record(ConversionStep::Refused {
                    reason: "the conversion was included and failed".into(),
                    psm: None,
                })),
                None,
                Some(PlannedStep::Record(ConversionStep::Stalled)),
            ]
        );
    }

    /// Balances per asset and account; anything else is empty.
    struct AssetBalances(Vec<(DepositAsset, [u8; 32], u128)>);

    impl DepositBalances for AssetBalances {
        fn balance<'a>(
            &'a self,
            asset: DepositAsset,
            account: &'a [u8; 32],
        ) -> BoxFuture<'a, Result<u128, GenericError>> {
            let balance = self
                .0
                .iter()
                .find(|(held, holder, _)| *held == asset && holder == account)
                .map_or(0, |(_, _, balance)| *balance);
            Box::pin(async move { Ok(balance) })
        }
    }

    // A wrong or short deposit is only seen if every asset a deposit can
    // arrive in is read, and a late one only if an expired session keeps
    // being read; past the watch window it is left alone.
    #[test]
    fn the_watch_reads_every_deposit_asset_and_late_sessions_within_the_window() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        let usdc = DepositAsset::Asset(1337);
        let now = NOW + crate::host_logic::funding::LATE_WATCH_MS;
        let expired = |intent: &str, settled_at_ms, holder| FundingSession {
            stage: FundingStage::Failed {
                reason: FundingFailure::Expired,
                settled_at_ms,
                resume: None,
            },
            deposit: Some(FundingDeposit {
                account: holder,
                ..converting_deposit()
            }),
            ..session(intent, settled_at_ms - DAY_MS)
        };
        insert(&registry, storage.as_ref(), expired("fs_recent", NOW, account(1)));
        insert(&registry, storage.as_ref(), expired("fs_old", NOW - 1, account(2)));
        let balances = AssetBalances(vec![
            (DepositAsset::Native, account(1), 3),
            (usdc, account(1), 9),
            (usdc, account(2), 9),
        ]);

        block_on(registry.observe_deposits(
            storage.as_ref(),
            now,
            &balances,
            &[DepositAsset::Native, usdc],
        ))
        .expect("observed");
        let holdings = |intent| {
            registry
                .get(intent)
                .and_then(|session| session.deposit)
                .map(|deposit| deposit.holdings)
        };

        assert_eq!(
            (holdings("fs_recent"), holdings("fs_old")),
            (
                Some(vec![
                    DepositHolding {
                        asset: usdc,
                        balance: 9
                    },
                    DepositHolding {
                        asset: DepositAsset::Native,
                        balance: 3
                    },
                ]),
                Some(Vec::new())
            )
        );
    }

    // An ended session is only read for a late deposit, which can wait, so
    // its account is read at the slower cadence rather than every pass.
    #[test]
    fn an_ended_session_is_read_at_the_slower_cadence() {
        let storage = stub_platform();
        let registry = FundingRegistry::default();
        let usdc = DepositAsset::Asset(1337);
        insert(
            &registry,
            storage.as_ref(),
            FundingSession {
                stage: FundingStage::Failed {
                    reason: FundingFailure::Expired,
                    settled_at_ms: NOW,
                    resume: None,
                },
                deposit: Some(converting_deposit()),
                ..session("fs_ended", NOW - DAY_MS)
            },
        );
        let read = |at, balance| {
            let balances = AssetBalances(vec![(usdc, converting_deposit().account, balance)]);
            block_on(registry.observe_deposits(storage.as_ref(), at, &balances, &[usdc])).expect("observed");
            registry
                .get("fs_ended")
                .and_then(|session| session.deposit)
                .map(|deposit| deposit.held(usdc))
        };

        assert_eq!(
            [read(NOW + 1, 9), read(NOW + 2, 7), read(NOW + 1 + LATE_READ_MS, 7)],
            [Some(9), Some(9), Some(7)]
        );
    }

    // What did land on People is the user's even when the conversion took
    // the deposit and less arrived than it should have: it is credited
    // rather than the session failing, as getcash fails it.
    #[test]
    fn a_stalled_conversion_credits_what_did_land() {
        let late = NOW + STALL_AFTER_MS + 1;
        let chains = |landed| Scripted {
            landed,
            nonce: 5,
            balance: 1,
            block: 100,
        };

        assert_eq!(
            [
                next_step(chains(30), Some(SUBMITTED), late),
                next_step(chains(10), Some(SUBMITTED), late),
            ],
            [
                Some(PlannedStep::Record(ConversionStep::Landed { landed: 20 })),
                Some(PlannedStep::Record(ConversionStep::Stalled)),
            ]
        );
    }

    // A session outlives a sign-out, so after switching identity the key on
    // hand is not the deposit account's; signing with it would dry-run one
    // account and pay from another.
    #[test]
    fn a_conversion_is_signed_only_by_the_deposit_account() {
        let chains = || Scripted {
            landed: 10,
            nonce: 7,
            balance: 50,
            block: 100,
        };
        let planned = |keys: Keys| {
            block_on(plan_conversion(&chains(), &keys, &converting_deposit(), None, NOW))
                .expect("planned")
        };

        assert_eq!(
            [planned(Keys(None)), planned(Keys(Some(keypair(2)))), planned(Keys(Some(keypair(1))))],
            [
                None,
                None,
                Some(PlannedStep::Submit {
                    submission: ConversionSubmission {
                        nonce: 7,
                        people_before: 10,
                        ..SUBMITTED
                    },
                    extrinsic: Vec::new(),
                }),
            ]
        );
    }
}
