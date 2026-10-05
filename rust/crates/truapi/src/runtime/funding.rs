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
use core::future::Future;

use futures::future::{BoxFuture, FutureExt};
use futures::stream::{self, BoxStream, StreamExt};
use truapi::latest::{
    ChainIdentifier, FundingDirection, GenericError, HostFundingStatusSubscribeItem,
};

mod conversion;

use conversion::{Chains, ConversionChains, ConversionError};
#[cfg(test)]
use conversion::Prepared;
pub use conversion::{FundingNetwork, FundingSigner};

use super::services::RuntimeServices;
use parity_scale_codec::Decode;
use sp_crypto_hashing::twox_128;

use super::statement_allowance::blake2_128_concat;
use super::statement_allowance::rpc::RpcClient;
use crate::host_logic::features;
use crate::host_logic::funding::{
    ConversionRoute, ConversionStep, ConversionSubmission, DepositAsset, DepositRequest,
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
/// Longest a chain read may take before the pass gives up on it.
const CHAIN_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a conversion that took the deposit on Asset Hub may take to
/// reach People before it counts as stalled.
const STALL_AFTER_MS: u64 = 30 * 60 * 1_000;
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
    pub fn install_conversion(&self, network: FundingNetwork, signer: Arc<dyn FundingSigner>) -> bool {
        self.conversion.set(Conversion { network, signer }).is_ok()
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
        self.get(intent)
            .ok_or(AssignDepositError::NotFound)
            .and_then(|session| assignable(&session))?;
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

    /// Read every awaited deposit once: a session whose deposit arrived
    /// moves to converting, one past its deadline without it expires. A
    /// failed read leaves its session for the next pass.
    pub async fn observe_deposits(
        &self,
        storage: &(impl CoreStorage + ?Sized),
        now_ms: u64,
        balances: &dyn DepositBalances,
    ) -> Result<(), FundingSessionError> {
        let awaited: Vec<_> = self
            .lock_sessions()
            .values()
            .filter_map(|session| {
                let deposit = session.awaited_deposit()?;
                Some((session.intent.clone(), deposit.asset, deposit.account))
            })
            .collect();
        let mut readings = Vec::new();
        for (intent, asset, account) in awaited {
            match balances.balance(asset, &account).await {
                Ok(balance) => readings.push((intent, balance)),
                Err(error) => {
                    tracing::warn!(%intent, reason = %error.reason, "reading a funding deposit failed")
                }
            }
        }
        self.commit(storage, now_ms, move |sessions| {
            let arrived = readings
                .into_iter()
                .filter(|(intent, balance)| {
                    sessions
                        .get_mut(intent)
                        .is_some_and(|session| session.observe_deposit(*balance, now_ms))
                })
                .map(|(intent, _)| intent)
                .collect();
            ((), arrived)
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
            .any(|session| session.awaited_deposit().is_some() || session.converting().is_some())
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
/// passed unincluded, or once it was included and the deposit is still on
/// Asset Hub; stalled once it took the deposit and nothing arrived in time.
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
            if chains.deposit_balance(deposit.asset, account).await? >= submission.spent {
                Some(ConversionStep::Dropped)
            } else if now_ms.saturating_sub(submission.submitted_at_ms) > STALL_AFTER_MS {
                Some(ConversionStep::Stalled)
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

/// Asset Hub balances read at one finalized block, so a deposit counts only
/// once it cannot be reverted.
struct FinalizedAssetHubBalances {
    rpc: RpcClient,
    finalized: String,
}

impl FinalizedAssetHubBalances {
    async fn connect(services: &RuntimeServices) -> Result<Self, GenericError> {
        within_chain_timeout(Self::connect_unbounded(services)).await?
    }

    async fn connect_unbounded(services: &RuntimeServices) -> Result<Self, GenericError> {
        let failed = |reason: String| GenericError { reason };
        let chains = features::supported_chains(services.platform.as_ref()).await?;
        let genesis = features::genesis_for(&chains, ChainIdentifier::AssetHub)
            .ok_or_else(|| failed("the host serves no Asset Hub".into()))?;
        let rpc = RpcClient::new(subxt_rpcs::RpcClient::new(
            services
                .chain
                .rpc_client("funding deposit watch", &genesis)
                .await
                .map_err(|err| failed(err.to_string()))?,
        ));
        let finalized = rpc
            .finalized_head()
            .await
            .map_err(|err| failed(err.to_string()))?;
        Ok(Self { rpc, finalized })
    }
}

impl DepositBalances for FinalizedAssetHubBalances {
    fn balance<'a>(
        &'a self,
        asset: DepositAsset,
        account: &'a [u8; 32],
    ) -> BoxFuture<'a, Result<u128, GenericError>> {
        Box::pin(async move {
            let value = within_chain_timeout(
                self.rpc
                    .get_storage_at(&balance_key(asset, account), &self.finalized),
            )
            .await?
            .map_err(|err| GenericError {
                reason: err.to_string(),
            })?;
            decode_balance(asset, value.as_deref()).ok_or_else(|| GenericError {
                reason: "undecodable deposit balance".into(),
            })
        })
    }
}

/// Run a chain read, giving up after [`CHAIN_TIMEOUT`] so a stalled
/// connection cannot park the deposit watch.
async fn within_chain_timeout<T>(read: impl Future<Output = T>) -> Result<T, GenericError> {
    let read = read.fuse();
    let timeout = futures_timer::Delay::new(CHAIN_TIMEOUT).fuse();
    futures::pin_mut!(read, timeout);
    futures::select! {
        value = read => Ok(value),
        () = timeout => Err(GenericError {
            reason: "Asset Hub read timed out".into(),
        }),
    }
}

/// Asset Hub storage key holding `account`'s balance of `asset`:
/// `System.Account` for the native token, `Assets.Account` otherwise.
fn balance_key(asset: DepositAsset, account: &[u8; 32]) -> Vec<u8> {
    match asset {
        DepositAsset::Native => [
            twox_128(b"System").as_slice(),
            &twox_128(b"Account"),
            &blake2_128_concat(account),
        ]
        .concat(),
        DepositAsset::Asset(id) => [
            twox_128(b"Assets").as_slice(),
            &twox_128(b"Account"),
            &blake2_128_concat(&id.to_le_bytes()),
            &blake2_128_concat(account),
        ]
        .concat(),
    }
}

/// The balance in a value read from [`balance_key`]. An absent value is a
/// zero balance. The native balance is the free balance after
/// `AccountInfo`'s four `u32` counters; an asset account leads with it.
fn decode_balance(asset: DepositAsset, value: Option<&[u8]>) -> Option<u128> {
    let Some(mut value) = value else {
        return Some(0);
    };
    if asset == DepositAsset::Native {
        value = value.get(16..)?;
    }
    u128::decode(&mut value).ok()
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
        self.funding()
            .get(intent)
            .ok_or(AssignDepositError::NotFound)
            .and_then(|session| assignable(&session))?;
        let network = self
            .funding()
            .conversion
            .get()
            .ok_or(AssignDepositError::ConversionUnavailable)?
            .network;
        let chains = self
            .funding_chains(network)
            .await
            .map_err(|error| AssignDepositError::Chain(GenericError { reason: error.to_string() }))?;
        let route = chains
            .choose_route(request.asset, request.expected)
            .await
            .map_err(|error| AssignDepositError::Chain(GenericError { reason: error.to_string() }))?
            .ok_or(AssignDepositError::NoRoute)?;
        let balances = FinalizedAssetHubBalances::connect(self)
            .await
            .map_err(AssignDepositError::Chain)?;
        let account = self
            .funding()
            .assign_empty_deposit(
                self.platform.as_ref(),
                &balances,
                current_unix_millis(),
                intent,
                DepositPlan { request, route },
                derive,
            )
            .await?;
        self.watch_funding_deposits();
        Ok(account)
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
                let observed = match FinalizedAssetHubBalances::connect(&services).await {
                    Ok(balances) => services
                        .funding()
                        .observe_deposits(
                            services.platform.as_ref(),
                            current_unix_millis(),
                            &balances,
                        )
                        .await
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.reason),
                };
                if let Err(reason) = observed {
                    tracing::warn!(%reason, "funding deposit watch failed");
                }
                if let Err(reason) = services.advance_conversions().await {
                    tracing::warn!(%reason, "funding conversion pass failed");
                }
            }
        }));
    }

    /// Asset Hub and People, pinned at their latest finalized blocks.
    async fn funding_chains(&self, network: FundingNetwork) -> Result<Chains, ConversionError> {
        within_chain_timeout(async {
            let chains = features::supported_chains(self.platform.as_ref())
                .await
                .map_err(|error| ConversionError::Chain(error.reason))?;
            let client = |chain: ChainIdentifier| {
                let genesis = features::genesis_for(&chains, chain);
                async move {
                    let genesis = genesis
                        .ok_or_else(|| ConversionError::Chain(format!("the host serves no {chain:?}")))?;
                    self.chain
                        .online_client(&genesis)
                        .await
                        .map_err(|error| ConversionError::Chain(error.to_string()))
                }
            };
            let asset_hub = client(ChainIdentifier::AssetHub).await?;
            let people = client(ChainIdentifier::People).await?;
            Chains::at_finalized(&asset_hub, &people, network).await
        })
        .await
        .map_err(|error| ConversionError::Chain(error.reason))?
    }

    /// One pass over the sessions being converted: record what landed,
    /// what was dropped or stalled, and submit what is ready.
    async fn advance_conversions(self: &Arc<Self>) -> Result<(), String> {
        let registry = self.funding();
        let converting = registry.converting_sessions();
        let Some(conversion) = registry.conversion.get().filter(|_| !converting.is_empty()) else {
            return Ok(());
        };
        let chains = self
            .funding_chains(conversion.network)
            .await
            .map_err(|error| error.to_string())?;
        let storage = self.platform.as_ref();
        for (intent, deposit, submission) in converting {
            let now_ms = current_unix_millis();
            let planned = within_chain_timeout(plan_conversion(
                &chains,
                conversion.signer.as_ref(),
                &deposit,
                submission,
                now_ms,
            ))
            .await;
            let planned = match planned {
                Ok(Ok(planned)) => planned,
                Ok(Err(ConversionError::Refused(reason))) => {
                    Some(PlannedStep::Record(ConversionStep::Refused { reason }))
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
                        Err(ConversionError::Refused(reason)) => {
                            let refused = ConversionStep::Refused { reason };
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
    use parity_scale_codec::Encode;
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
            block_on(registry.observe_deposits(storage.as_ref(), NOW, &balances))
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
        block_on(registry.observe_deposits(storage.as_ref(), past_deadline, &late_payment))
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
            (registry.get("fs_1"), registry.next_deadline()),
            (Some(converting), None)
        );
    }

    // A wrong key reads an empty account forever and no deposit is ever
    // seen, so the keys are pinned to the pallets' well-known prefixes.
    #[test]
    fn balance_keys_address_system_and_assets_accounts() {
        let account = [7u8; 32];
        let hashed_account = [sp_crypto_hashing::blake2_128(&account).as_slice(), &account].concat();
        let hashed_id = [sp_crypto_hashing::blake2_128(&1984u32.to_le_bytes()).as_slice(), &1984u32.to_le_bytes()].concat();

        assert_eq!(
            (
                hex::encode(balance_key(DepositAsset::Native, &account)),
                hex::encode(balance_key(USDT, &account)),
            ),
            (
                format!(
                    "26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9{}",
                    hex::encode(&hashed_account)
                ),
                format!(
                    "682a59d51ab9e48a8c8cc418ff9708d2b99d880ec681799c0cf30e8886371da9{}{}",
                    hex::encode(hashed_id),
                    hex::encode(&hashed_account)
                ),
            )
        );
    }

    // The native balance sits behind `AccountInfo`'s counters, while an asset
    // account leads with it; reading the wrong offset would see a counter.
    #[test]
    fn balances_decode_from_each_account_layout() {
        let account_info = (1u32, 2u32, 3u32, 4u32, 500u128, 9u128).encode();
        let asset_account = (70u128, 0u8).encode();

        assert_eq!(
            [
                decode_balance(DepositAsset::Native, Some(&account_info)),
                decode_balance(USDT, Some(&asset_account)),
                decode_balance(USDT, None),
                decode_balance(DepositAsset::Native, Some(&account_info[..12])),
            ],
            [Some(500), Some(70), Some(0), None]
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

    impl ConversionChains for Scripted {
        fn landed<'a>(&'a self, _: &'a [u8; 32]) -> BoxFuture<'a, Result<u128, ConversionError>> {
            Box::pin(async move { Ok(self.landed) })
        }

        fn nonce<'a>(&'a self, _: &'a [u8; 32]) -> BoxFuture<'a, Result<u32, ConversionError>> {
            Box::pin(async move { Ok(self.nonce) })
        }

        fn deposit_balance<'a>(
            &'a self,
            _: DepositAsset,
            _: &'a [u8; 32],
        ) -> BoxFuture<'a, Result<u128, ConversionError>> {
            Box::pin(async move { Ok(self.balance) })
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
    // passed unincluded, or it was included and left the deposit in place.
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
                Some(PlannedStep::Record(ConversionStep::Dropped)),
                None,
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
