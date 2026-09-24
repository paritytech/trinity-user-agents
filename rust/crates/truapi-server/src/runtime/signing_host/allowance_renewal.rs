//! Ledger and driver for automatic statement-store allowance renewal.
//!
//! The ledger records which accounts this signing host promised to keep
//! allowed, as derivation recipes where possible so entries stay valid when
//! the host rotates to a new root entropy. The driver resolves them against
//! the active session and runs the chain-pure pass in
//! `statement_allowance::renewal`, either once (`renew_now`) or on a periodic
//! tick (`start_renewal_loop`).
//!
//! All signing hosts record the ledger during allocation. The resident renewal
//! driver belongs to the native host API; browser hosts currently allocate on
//! demand without starting that driver.

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};
#[cfg(not(target_arch = "wasm32"))]
use tracing::debug;
#[cfg(any(test, not(target_arch = "wasm32")))]
use tracing::info;
use tracing::warn;
use truapi_platform::{CoreStorage, CoreStorageKey};

use super::SigningHost;
#[cfg(not(target_arch = "wasm32"))]
use super::sso_responder::current_unix_secs;
use crate::host_logic::product_account::derive_root_keypair_from_entropy;
use crate::host_logic::product_account::{derive_identity_keypair, derive_sr25519_hard_path};
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::RuntimeServices;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::authority::ProductAuthority;
use crate::runtime::statement_allowance::renewal::ResolvedRenewalTarget;
#[cfg(any(test, not(target_arch = "wasm32")))]
use crate::runtime::statement_allowance::renewal::StatementRenewalReport;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::statement_allowance::renewal::{
    RenewalChainContext, next_tick_delay, renew_targets,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::statement_allowance::{
    self, fetch_chain_state, fetch_metadata, find_including_rings,
};

/// Fallback tick delay when the system clock is unusable.
#[cfg(not(target_arch = "wasm32"))]
const CLOCK_FAILURE_TICK_DELAY: Duration = Duration::from_secs(3_600);

/// A statement-store account the signing host promised to keep renewed.
///
/// Entropy-derived variants are recipes, not raw account ids, so the ledger
/// survives root-entropy rotation (the CLI rotates auto-managed accounts on
/// slot exhaustion).
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum StatementRenewalTarget {
    /// `//allowance//statement-store//{product_id}` from the active root entropy.
    ProductStatementAllowance {
        /// Product the allowance account belongs to.
        product_id: String,
    },
    /// `//wallet//sso` from the active root entropy.
    WalletSso,
    /// A fixed account, e.g. a pairing peer's device statement key.
    Account {
        /// Account to keep allowed.
        account_id: [u8; 32],
        /// Human-readable name used in logs and reports.
        label: String,
    },
}

/// One persisted ledger entry.
///
/// A derivation recipe resolves under whatever root entropy is active, so it
/// carries no owner and keeps working across a rotation. A raw account id does
/// not re-derive, so it records the root public key that promised it and is
/// ignored under any other identity: without that, a later account would spend
/// its own slot-table capacity keeping a previous account's peer allowed.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
struct LedgerEntry {
    target: StatementRenewalTarget,
    owner: Option<[u8; 32]>,
}

/// One ledger entry as a host reads it back.
///
/// Mirrors the persisted entry rather than resolving it: resolution needs root
/// entropy, and a host inspecting its slots may hold none.
#[cfg(any(test, not(target_arch = "wasm32")))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedStatementRenewalTarget {
    /// The account, or the recipe for one, that the host promised to renew.
    pub target: StatementRenewalTarget,
    /// Root public key that promised a raw account id. A recipe carries none
    /// and resolves under whichever identity is active.
    pub owner: Option<[u8; 32]>,
}

impl LedgerEntry {
    /// Record `target` under `owner`, which only raw account ids retain.
    fn new(target: StatementRenewalTarget, owner: [u8; 32]) -> Self {
        let owner = match &target {
            StatementRenewalTarget::Account { .. } => Some(owner),
            StatementRenewalTarget::ProductStatementAllowance { .. }
            | StatementRenewalTarget::WalletSso => None,
        };
        Self { target, owner }
    }

    /// Whether this entry belongs to the identity rooted at `owner`.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    fn is_owned_by(&self, owner: [u8; 32]) -> bool {
        self.owner.is_none_or(|recorded| recorded == owner)
    }
}

/// Root public key of the identity rooted at `entropy`, used to own raw ledger
/// entries.
fn owner_key(entropy: &[u8]) -> Result<[u8; 32], String> {
    derive_root_keypair_from_entropy(entropy)
        .map(|pair| pair.public.to_bytes())
        .map_err(|err| err.to_string())
}

/// Renewal coordination state owned by [`SigningHost`].
#[derive(Default)]
pub(super) struct RenewalState {
    /// Serializes slot registrations between the renewal pass and on-demand
    /// allocation so both cannot race for the same free slot.
    registration_lock: Mutex<()>,
    /// Serializes read-modify-write cycles on the ledger so a concurrent
    /// allocation cannot drop another's entry.
    ledger_lock: Mutex<()>,
    #[cfg(not(target_arch = "wasm32"))]
    loop_started: AtomicBool,
    /// The most recent pass, so a host that drives the in-process loop can read
    /// what it achieved. The loop computes a report on every tick and has no
    /// caller to hand it to, and exhaustion is the outcome a host most needs to
    /// act on. A blocking lock rather than an async one: every use is a clone or
    /// a store with no await in between.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    last_report: std::sync::Mutex<Option<StatementRenewalReport>>,
}

impl RenewalState {
    pub(super) fn registration_lock(&self) -> &Mutex<()> {
        &self.registration_lock
    }

    fn ledger_lock(&self) -> &Mutex<()> {
        &self.ledger_lock
    }

    /// Record the pass a host has not seen the return value of.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    fn record_report(&self, report: &StatementRenewalReport) {
        if let Ok(mut last) = self.last_report.lock() {
            *last = Some(report.clone());
        }
    }

    /// The most recent pass the loop ran. `None` until one has run, which a host
    /// should read as "not yet" rather than as healthy.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn last_report(&self) -> Option<StatementRenewalReport> {
        self.last_report.lock().ok().and_then(|last| last.clone())
    }
}

/// Read the renewal ledger; an absent or undecodable slot is an empty ledger.
///
/// A ledger this build cannot decode is treated as empty rather than failing
/// the pass: the entries are recipes and raw account ids that
/// [`track_targets`] rebuilds on the next allocation or pairing, so refusing to
/// renew anything is strictly worse than starting over.
async fn read_entries(storage: &(impl CoreStorage + ?Sized)) -> Result<Vec<LedgerEntry>, String> {
    let Some(blob) = storage
        .read_core_storage(CoreStorageKey::StatementRenewalTargets)
        .await
        .map_err(|err| format!("renewal ledger read failed: {}", err.reason))?
    else {
        return Ok(Vec::new());
    };
    match decode_entries(&blob) {
        Ok(entries) => Ok(entries),
        Err(reason) => {
            warn!(%reason, "discarding an undecodable renewal ledger");
            Ok(Vec::new())
        }
    }
}

/// Append `new_targets` to the ledger, preserving order and skipping entries
/// already present.
async fn track_targets(
    storage: &(impl CoreStorage + ?Sized),
    ledger_lock: &Mutex<()>,
    owner: [u8; 32],
    new_targets: Vec<StatementRenewalTarget>,
) -> Result<(), String> {
    let _guard = ledger_lock.lock().await;
    let mut entries = read_entries(storage).await?;
    let mut changed = false;
    for target in new_targets {
        let entry = LedgerEntry::new(target, owner);
        if !entries.contains(&entry) {
            entries.push(entry);
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    write_entries(storage, &entries).await
}

/// Read the ledger without resolving anything.
///
/// Takes the ledger lock so a listing taken while a track or a prune is running
/// reports the settled ledger rather than the state it is replacing.
#[cfg(any(test, not(target_arch = "wasm32")))]
async fn list_entries(
    storage: &(impl CoreStorage + ?Sized),
    ledger_lock: &Mutex<()>,
) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
    let _guard = ledger_lock.lock().await;
    Ok(read_entries(storage)
        .await?
        .into_iter()
        .map(|entry| TrackedStatementRenewalTarget {
            target: entry.target,
            owner: entry.owner,
        })
        .collect())
}

#[cfg(any(test, not(target_arch = "wasm32")))]
async fn untrack_account(
    storage: &(impl CoreStorage + ?Sized),
    ledger_lock: &Mutex<()>,
    owner: [u8; 32],
    account_id: &[u8; 32],
) -> Result<bool, String> {
    let _guard = ledger_lock.lock().await;
    let mut entries = read_entries(storage).await?;
    let original_len = entries.len();
    entries.retain(|entry| {
        entry.owner != Some(owner)
            || !matches!(
                &entry.target,
                StatementRenewalTarget::Account {
                    account_id: existing,
                    ..
                } if existing == account_id
            )
    });
    if entries.len() == original_len {
        return Ok(false);
    }
    write_entries(storage, &entries).await?;
    Ok(true)
}

async fn write_entries(
    storage: &(impl CoreStorage + ?Sized),
    entries: &[LedgerEntry],
) -> Result<(), String> {
    storage
        .write_core_storage(CoreStorageKey::StatementRenewalTargets, entries.encode())
        .await
        .map_err(|err| format!("renewal ledger write failed: {}", err.reason))
}

fn decode_entries(blob: &[u8]) -> Result<Vec<LedgerEntry>, String> {
    let mut input = blob;
    let entries = Vec::<LedgerEntry>::decode(&mut input)
        .map_err(|err| format!("invalid persisted renewal targets: {err}"))?;
    if !input.is_empty() {
        return Err("invalid persisted renewal targets: trailing bytes".to_string());
    }
    Ok(entries)
}

/// Resolve a ledger entry into a concrete account for this session's entropy.
/// The label a target reports under, derivable without an active session so a
/// pruned entry reads the same as a renewed one.
fn target_label(target: &StatementRenewalTarget) -> String {
    match target {
        StatementRenewalTarget::ProductStatementAllowance { product_id } => {
            format!("product:{product_id}")
        }
        StatementRenewalTarget::WalletSso => "wallet-sso".to_string(),
        StatementRenewalTarget::Account { label, .. } => label.clone(),
    }
}

fn resolve_target(
    entropy: &[u8],
    network_suffix: &str,
    target: &StatementRenewalTarget,
) -> Result<ResolvedRenewalTarget, String> {
    let label = target_label(target);
    match target {
        StatementRenewalTarget::ProductStatementAllowance { product_id } => {
            let pair = derive_sr25519_hard_path(
                entropy,
                &["allowance", "statement-store", product_id.as_str()],
            )
            .map_err(|err| err.to_string())?;
            Ok(ResolvedRenewalTarget {
                label,
                account_id: pair.public.to_bytes(),
            })
        }
        StatementRenewalTarget::WalletSso => {
            let pair =
                derive_identity_keypair(entropy, network_suffix).map_err(|err| err.to_string())?;
            Ok(ResolvedRenewalTarget {
                label,
                account_id: pair.public.to_bytes(),
            })
        }
        StatementRenewalTarget::Account { account_id, .. } => Ok(ResolvedRenewalTarget {
            label,
            account_id: *account_id,
        }),
    }
}

/// Resolve attribution without refreshing, pruning, or rewriting the ledger.
pub(super) async fn inspection_labels(
    signing_host: &SigningHost,
    entropy: &[u8],
    owner: [u8; 32],
) -> Result<crate::runtime::statement_allowance::inspection::AccountLabels, String> {
    let _guard = signing_host.renewal.ledger_lock().lock().await;
    let bytes = signing_host
        .platform
        .read_core_storage(CoreStorageKey::StatementRenewalTargets)
        .await
        .map_err(|err| format!("renewal ledger read failed: {}", err.reason))?;
    let entries = bytes
        .as_deref()
        .map(decode_entries)
        .transpose()?
        .unwrap_or_default();
    let mut labels = crate::runtime::statement_allowance::inspection::AccountLabels::new();
    for entry in entries {
        if entry.owner.is_some_and(|recorded| recorded != owner) {
            continue;
        }
        let resolved = resolve_target(entropy, &signing_host.network_suffix, &entry.target)?;
        let product = match entry.target {
            StatementRenewalTarget::ProductStatementAllowance { product_id } => Some(product_id),
            _ => None,
        };
        labels.insert(resolved.account_id, (product, resolved.label));
    }
    Ok(labels)
}

/// Record `targets` in the ledger under the active identity.
pub(super) async fn track(
    signing_host: &SigningHost,
    targets: Vec<StatementRenewalTarget>,
) -> Result<(), String> {
    let entropy = signing_host.root_entropy().map_err(|err| err.to_string())?;
    track_targets(
        signing_host.platform.as_ref(),
        signing_host.renewal.ledger_lock(),
        owner_key(&entropy)?,
        targets,
    )
    .await
}

/// Every entry the ledger holds, in the order it was tracked.
///
/// Reads storage alone. A host that has not unlocked an identity still gets
/// the list, which is the case a scheduled task runs in: it wakes, asks what
/// its finite slots are spent on, and decides whether to renew at all.
#[cfg(any(test, not(target_arch = "wasm32")))]
pub(super) async fn list(
    signing_host: &SigningHost,
) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
    list_entries(
        signing_host.platform.as_ref(),
        signing_host.renewal.ledger_lock(),
    )
    .await
}

/// Root public key the active identity records its fixed entries under.
///
/// Pairs with [`list`], which reports each entry's owner as stored: comparing
/// the two is how a host tells the entries it will actually renew from the ones
/// a pass will prune.
#[cfg(any(test, not(target_arch = "wasm32")))]
pub(super) fn active_owner_key(signing_host: &SigningHost) -> Result<[u8; 32], String> {
    let entropy = signing_host.root_entropy().map_err(|err| err.to_string())?;
    owner_key(&entropy)
}

/// Stop renewing one fixed statement account for the active identity.
#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn untrack_account_for_signing_host(
    signing_host: &SigningHost,
    account_id: &[u8; 32],
) -> Result<bool, String> {
    let entropy = signing_host.root_entropy().map_err(|err| err.to_string())?;
    untrack_account(
        signing_host.platform.as_ref(),
        signing_host.renewal.ledger_lock(),
        owner_key(&entropy)?,
        account_id,
    )
    .await
}

/// Resolve every ledger target under `entropy`, skipping any that cannot be
/// resolved.
///
/// A target is skipped rather than failing the pass: one unusable entry must
/// not stop every other target from being renewed.
#[cfg(any(test, not(target_arch = "wasm32")))]
fn resolve_targets(
    entropy: &[u8],
    network_suffix: &str,
    targets: &[StatementRenewalTarget],
) -> Vec<ResolvedRenewalTarget> {
    targets
        .iter()
        .filter_map(
            |target| match resolve_target(entropy, network_suffix, target) {
                Ok(resolved) => Some(resolved),
                Err(reason) => {
                    warn!(?target, %reason, "skipping an unresolvable renewal target");
                    None
                }
            },
        )
        .collect()
}

/// The ledger targets `owner` promised, dropping any it did not.
///
/// An entry promised by a different identity would consume this one's slots for
/// an account it never promised, so it is removed rather than skipped — the cost
/// is paid once per identity change instead of on every tick. The ledger is only
/// rewritten when something was actually dropped.
///
/// The lock covers the read as well as the write, because this is a
/// read-modify-write of the whole ledger: reading outside it lets a
/// [`track_targets`] call land in the gap and be overwritten by a view that
/// predates it, leaving that account tracked nowhere and never renewed again.
///
/// The labels are logged here rather than only returned. Every step between this
/// and the report can fail, and `run_tick` does not read the report at all, so
/// the log is the one place a prune is recorded unconditionally.
#[cfg(any(test, not(target_arch = "wasm32")))]
async fn owned_targets(
    storage: &(impl CoreStorage + ?Sized),
    ledger_lock: &Mutex<()>,
    owner: [u8; 32],
) -> Result<(Vec<StatementRenewalTarget>, Vec<String>), String> {
    let _guard = ledger_lock.lock().await;
    let (owned, foreign): (Vec<_>, Vec<_>) = read_entries(storage)
        .await?
        .into_iter()
        .partition(|entry| entry.is_owned_by(owner));
    let pruned: Vec<String> = foreign
        .iter()
        .map(|entry| target_label(&entry.target))
        .collect();
    if !foreign.is_empty() {
        warn!(
            dropped = ?pruned,
            "pruning renewal targets promised by a previous identity"
        );
        write_entries(storage, &owned).await?;
    }
    Ok((
        owned.into_iter().map(|entry| entry.target).collect(),
        pruned,
    ))
}

/// One renewal pass: resolve the ledger against the active session and renew
/// every target for the current period.
#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn renew_now(
    services: &Arc<RuntimeServices>,
    signing_host: &SigningHost,
) -> Result<StatementRenewalReport, String> {
    let entropy = signing_host.root_entropy().map_err(|err| err.to_string())?;
    let period = statement_allowance::slot::current_period(
        current_unix_secs().map_err(|err| err.to_string())?,
    );
    let (targets, pruned) = owned_targets(
        signing_host.platform.as_ref(),
        signing_host.renewal.ledger_lock(),
        owner_key(&entropy)?,
    )
    .await?;
    let resolved = resolve_targets(&entropy, signing_host.network_suffix(), &targets);
    if resolved.is_empty() {
        return Ok(StatementRenewalReport {
            period,
            outcomes: Vec::new(),
            pruned,
            slots_exhausted: false,
        });
    }

    // The same accessor on-demand allocation uses, so a change to the reserved
    // key reaches renewal too — and it revalidates the session first.
    let session = signing_host
        .current_session()
        .ok_or_else(|| "no active session for statement-store renewal".to_string())?;
    let candidates = signing_host
        .reserved_person_collection_candidates(&session)
        .map_err(|err| err.to_string())?;
    let rpc = statement_allowance::rpc::RpcClient::new(
        services
            .statement_store
            .client("statement-allowance renewal")
            .await
            .map_err(|err| err.to_string())?,
    );
    let metadata = fetch_metadata(&rpc).await.map_err(|err| err.to_string())?;
    let chain_state = fetch_chain_state(&rpc)
        .await
        .map_err(|err| err.to_string())?;
    let network_suffix = statement_allowance::slot::read_network_suffix(&rpc)
        .await
        .map_err(|err| err.to_string())?;
    // Every ring back to index 0, because a membership that stopped being
    // re-included still proves against the ring that holds it.
    let memberships = find_including_rings(&rpc, &metadata, &candidates, u32::MAX)
        .await
        .map_err(|err| err.to_string())?;
    if memberships.is_empty() {
        return Err(
            "signing account is not a member of any personhood ring; cannot renew statement-store allowances"
                .to_string(),
        );
    }
    let context = RenewalChainContext {
        rpc: &rpc,
        metadata: &metadata,
        chain_state: &chain_state,
        network_suffix: &network_suffix,
        candidates: &candidates,
        memberships: &memberships,
    };
    let mut report = renew_targets(
        &context,
        period,
        &resolved,
        signing_host.renewal.registration_lock(),
    )
    .await;
    report.pruned = pruned;
    Ok(report)
}

/// Spawn the periodic renewal loop; repeated calls are no-ops. The loop holds
/// only weak references, so it exits when the owning runtime is dropped.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn start_renewal_loop(services: &Arc<RuntimeServices>, signing_host: &Arc<SigningHost>) {
    if signing_host
        .renewal
        .loop_started
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    let weak_services = Arc::downgrade(services);
    let weak_host = Arc::downgrade(signing_host);
    let spawner = services.spawner.clone();
    spawner(Box::pin(async move {
        loop {
            {
                let (Some(services), Some(signing_host)) =
                    (weak_services.upgrade(), weak_host.upgrade())
                else {
                    return;
                };
                run_tick(&services, &signing_host).await;
            }
            let delay = match current_unix_secs() {
                Ok(now) => next_tick_delay(now),
                Err(_) => CLOCK_FAILURE_TICK_DELAY,
            };
            futures_timer::Delay::new(delay).await;
        }
    }));
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_tick(services: &Arc<RuntimeServices>, signing_host: &SigningHost) {
    if signing_host.root_entropy().is_err() {
        debug!("skipping statement-store renewal tick; no active session");
        return;
    }
    absorb_tick(
        &signing_host.renewal,
        renew_now(services, signing_host).await,
    );
}

/// Record and log what one tick achieved.
///
/// Split out from [`run_tick`] so the recording is reachable without a runtime: a
/// loop that logged but forgot to record would leave a host unable to see
/// exhaustion, and that is exactly the wiring worth a test.
#[cfg(any(test, not(target_arch = "wasm32")))]
fn absorb_tick(state: &RenewalState, result: Result<StatementRenewalReport, String>) {
    match result {
        Ok(report) => {
            if report.slots_exhausted {
                warn!(
                    period = report.period,
                    "statement-store renewal hit slot exhaustion"
                );
            } else {
                info!(
                    period = report.period,
                    targets = report.outcomes.len(),
                    "statement-store renewal pass complete"
                );
            }
            state.record_report(&report);
        }
        // A tick that could not run leaves the previous pass readable rather than
        // replacing it with nothing: "the last thing we know" beats "no idea".
        Err(reason) => warn!(%reason, "statement-store renewal tick failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use truapi::latest::GenericError;

    #[derive(Default)]
    struct MemStorage {
        inner: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
        writes: Mutex<usize>,
    }

    impl MemStorage {
        fn writes(&self) -> usize {
            *self.writes.lock().expect("write counter mutex poisoned")
        }
    }

    #[truapi_platform::async_trait]
    impl CoreStorage for MemStorage {
        async fn read_core_storage(
            &self,
            key: CoreStorageKey,
        ) -> Result<Option<Vec<u8>>, GenericError> {
            Ok(self
                .inner
                .lock()
                .expect("storage mutex poisoned")
                .get(&key.encode())
                .cloned())
        }

        async fn write_core_storage(
            &self,
            key: CoreStorageKey,
            value: Vec<u8>,
        ) -> Result<(), GenericError> {
            *self.writes.lock().expect("write counter mutex poisoned") += 1;
            self.inner
                .lock()
                .expect("storage mutex poisoned")
                .insert(key.encode(), value);
            Ok(())
        }

        async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
            self.inner
                .lock()
                .expect("storage mutex poisoned")
                .remove(&key.encode());
            Ok(())
        }
    }

    /// Root public key standing in for the active identity.
    const OWNER: [u8; 32] = [1; 32];
    /// A different identity's root public key.
    const OTHER_OWNER: [u8; 32] = [2; 32];

    /// Yield once, so a concurrently polled task can run.
    async fn yield_once() {
        let mut yielded = false;
        futures::future::poll_fn(move |cx| {
            if yielded {
                core::task::Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                core::task::Poll::Pending
            }
        })
        .await
    }

    /// Storage that yields *after* serving a read, so a second reader observes
    /// the same value before the first writes its update back. Without
    /// something serializing the cycle, one update is lost.
    #[derive(Default)]
    struct YieldingStorage(MemStorage);

    #[truapi_platform::async_trait]
    impl CoreStorage for YieldingStorage {
        async fn read_core_storage(
            &self,
            key: CoreStorageKey,
        ) -> Result<Option<Vec<u8>>, GenericError> {
            let value = self.0.read_core_storage(key).await;
            yield_once().await;
            value
        }

        async fn write_core_storage(
            &self,
            key: CoreStorageKey,
            value: Vec<u8>,
        ) -> Result<(), GenericError> {
            self.0.write_core_storage(key, value).await
        }

        async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
            self.0.clear_core_storage(key).await
        }
    }

    #[test]
    fn concurrent_tracks_do_not_drop_an_entry() {
        let storage = YieldingStorage::default();
        let ledger_lock = lock();

        futures::executor::block_on(async {
            let (first, second) = futures::join!(
                track_targets(&storage, &ledger_lock, OWNER, vec![product("a.dot")]),
                track_targets(&storage, &ledger_lock, OWNER, vec![product("b.dot")]),
            );
            first.unwrap();
            second.unwrap();

            let mut targets = read_targets(&storage, OWNER).await.unwrap();
            targets.sort_by_key(|target| format!("{target:?}"));
            assert_eq!(targets, vec![product("a.dot"), product("b.dot")]);
        });
    }

    /// The in-process loop returns nothing, so a host reads what a pass achieved
    /// from the recorded report. Recording only in the direct path would leave a
    /// loop-driven host unable to see exhaustion at all.
    #[test]
    fn the_last_report_is_readable_once_a_pass_has_run() {
        let state = RenewalState::default();
        assert!(
            state.last_report().is_none(),
            "no pass has run, which is not the same as a healthy one"
        );

        absorb_tick(
            &state,
            Ok(StatementRenewalReport {
                period: 7,
                outcomes: Vec::new(),
                pruned: Vec::new(),
                slots_exhausted: true,
            }),
        );

        let seen = state.last_report().expect("a pass has run");
        assert_eq!(seen.period, 7);
        assert!(
            seen.slots_exhausted,
            "exhaustion is the outcome a host most needs to read back"
        );
    }

    /// Only the newest matters: a host reads this on resume and wants the state
    /// now, not the first exhaustion it ever hit.
    #[test]
    fn the_last_report_keeps_the_newest_pass() {
        let state = RenewalState::default();
        for period in [7, 8] {
            absorb_tick(
                &state,
                Ok(StatementRenewalReport {
                    period,
                    outcomes: Vec::new(),
                    pruned: Vec::new(),
                    slots_exhausted: period == 7,
                }),
            );
        }

        let seen = state.last_report().expect("a pass has run");
        assert_eq!(seen.period, 8);
        assert!(
            !seen.slots_exhausted,
            "the older exhaustion should not stick"
        );
    }

    /// A tick that could not run at all must not erase the last pass a host has
    /// not read yet.
    #[test]
    fn a_failed_tick_leaves_the_previous_report_readable() {
        let state = RenewalState::default();
        absorb_tick(
            &state,
            Ok(StatementRenewalReport {
                period: 7,
                outcomes: Vec::new(),
                pruned: Vec::new(),
                slots_exhausted: true,
            }),
        );

        absorb_tick(&state, Err("no active session".to_string()));

        let seen = state
            .last_report()
            .expect("the earlier pass is still there");
        assert_eq!(seen.period, 7);
        assert!(seen.slots_exhausted);
    }

    /// Pruning rewrites the whole ledger, so it has to hold the lock across its
    /// read too. Reading outside it lets a `track_targets` land in the gap and be
    /// overwritten by a view that predates it, leaving that account tracked
    /// nowhere and never renewed again.
    #[test]
    fn a_prune_does_not_overwrite_a_concurrent_track() {
        let storage = YieldingStorage::default();
        let ledger_lock = lock();
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            // A foreign entry, so the pass prunes and therefore writes.
            track_targets(&storage, &ledger_lock, OTHER_OWNER, vec![device])
                .await
                .unwrap();

            let (pruned, tracked) = futures::join!(
                owned_targets(&storage, &ledger_lock, OWNER),
                track_targets(&storage, &ledger_lock, OWNER, vec![product("a.dot")]),
            );
            pruned.unwrap();
            tracked.unwrap();

            assert_eq!(
                read_targets(&storage, OWNER).await.unwrap(),
                vec![product("a.dot")],
                "the concurrently tracked target was overwritten by the prune"
            );
        });
    }

    /// A raw account promised by a previous identity must not be renewed under a
    /// later one: it would spend that identity's slots on an account it never
    /// promised.
    #[test]
    fn a_foreign_target_is_dropped_from_the_ledger() {
        let storage = MemStorage::default();
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_targets(&storage, &lock(), OTHER_OWNER, vec![device])
                .await
                .unwrap();
            track_targets(&storage, &lock(), OWNER, vec![product("a.dot")])
                .await
                .unwrap();

            let (targets, pruned) = owned_targets(&storage, &lock(), OWNER).await.unwrap();

            assert_eq!(targets, vec![product("a.dot")]);
            // Reported, not just dropped: the pass is a host's only view of the
            // ledger, so a silent prune is one it cannot notice or re-track.
            assert_eq!(pruned, vec!["device".to_string()]);
            // Dropped, not merely skipped, so the cost is paid once.
            assert_eq!(
                read_entries(&storage).await.unwrap(),
                vec![LedgerEntry::new(product("a.dot"), OWNER)]
            );
        });
    }

    #[test]
    fn an_all_foreign_ledger_prunes_to_nothing() {
        let storage = MemStorage::default();
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_targets(&storage, &lock(), OTHER_OWNER, vec![device])
                .await
                .unwrap();

            assert_eq!(
                owned_targets(&storage, &lock(), OWNER).await.unwrap(),
                (Vec::new(), vec!["device".to_string()])
            );
            assert_eq!(read_entries(&storage).await.unwrap(), Vec::new());
        });
    }

    #[test]
    fn an_all_owned_ledger_is_not_rewritten() {
        let storage = MemStorage::default();

        futures::executor::block_on(async {
            track_targets(&storage, &lock(), OWNER, vec![product("a.dot")])
                .await
                .unwrap();
            let after_seeding = storage.writes();

            let (targets, _pruned) = owned_targets(&storage, &lock(), OWNER).await.unwrap();

            assert_eq!(targets, vec![product("a.dot")]);
            // Every tick calls this; rewriting the ledger each time would be waste.
            assert_eq!(storage.writes(), after_seeding);
        });
    }

    #[test]
    fn an_unresolvable_target_is_skipped_not_fatal() {
        // An all-digit product id past `u64::MAX` fails junction derivation with
        // `NumericJunctionOutOfRange`.
        let unresolvable = product(&"9".repeat(25));
        let entropy = [7u8; 32];

        assert!(resolve_target(&entropy, "paseo", &unresolvable).is_err());
        let targets = [unresolvable, product("a.dot")];

        // Resolving strictly loses the healthy target with the broken one.
        assert!(
            targets
                .iter()
                .map(|target| resolve_target(&entropy, "paseo", target))
                .collect::<Result<Vec<_>, _>>()
                .is_err()
        );

        let resolved = resolve_targets(&entropy, "paseo", &targets);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].label, "product:a.dot");
    }

    /// A fresh ledger lock; each test drives one ledger in isolation.
    fn lock() -> futures::lock::Mutex<()> {
        futures::lock::Mutex::new(())
    }

    /// The targets in the ledger visible to the identity rooted at `owner`.
    async fn read_targets(
        storage: &(impl CoreStorage + ?Sized),
        owner: [u8; 32],
    ) -> Result<Vec<StatementRenewalTarget>, String> {
        Ok(read_entries(storage)
            .await?
            .into_iter()
            .filter(|entry| entry.is_owned_by(owner))
            .map(|entry| entry.target)
            .collect())
    }

    fn product(product_id: &str) -> StatementRenewalTarget {
        StatementRenewalTarget::ProductStatementAllowance {
            product_id: product_id.to_string(),
        }
    }

    #[test]
    fn ledger_round_trips_dedupes_and_preserves_order() {
        let storage = MemStorage::default();

        futures::executor::block_on(async {
            track_targets(
                &storage,
                &lock(),
                OWNER,
                vec![StatementRenewalTarget::WalletSso, product("a.dot")],
            )
            .await
            .unwrap();
            track_targets(
                &storage,
                &lock(),
                OWNER,
                vec![
                    product("a.dot"),
                    StatementRenewalTarget::Account {
                        account_id: [9; 32],
                        label: "device".to_string(),
                    },
                ],
            )
            .await
            .unwrap();

            assert_eq!(
                read_targets(&storage, OWNER).await.unwrap(),
                vec![
                    StatementRenewalTarget::WalletSso,
                    product("a.dot"),
                    StatementRenewalTarget::Account {
                        account_id: [9; 32],
                        label: "device".to_string(),
                    },
                ]
            );
        });
    }

    #[test]
    fn the_ledger_lists_back_every_entry_in_track_order() {
        let storage = MemStorage::default();
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_targets(
                &storage,
                &lock(),
                OWNER,
                vec![StatementRenewalTarget::WalletSso, device.clone()],
            )
            .await
            .unwrap();

            assert_eq!(
                list_entries(&storage, &lock()).await.unwrap(),
                vec![
                    TrackedStatementRenewalTarget {
                        target: StatementRenewalTarget::WalletSso,
                        owner: None,
                    },
                    TrackedStatementRenewalTarget {
                        target: device,
                        owner: Some(OWNER),
                    },
                ]
            );
        });
    }

    // The point of the reader is auditing finite slots, so it reports what is
    // stored rather than what the active identity would resolve: an entry another
    // identity promised is still occupying a row until a pass prunes it, and the
    // owner is what lets a caller tell the two apart.
    #[test]
    fn a_listing_includes_a_foreign_entry_and_writes_nothing() {
        let storage = MemStorage::default();
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_targets(&storage, &lock(), OTHER_OWNER, vec![device.clone()])
                .await
                .unwrap();
            track_targets(&storage, &lock(), OWNER, vec![product("a.dot")])
                .await
                .unwrap();
            let writes_before = storage.writes();

            let listed = list_entries(&storage, &lock()).await.unwrap();

            assert_eq!(
                listed,
                vec![
                    TrackedStatementRenewalTarget {
                        target: device,
                        owner: Some(OTHER_OWNER),
                    },
                    TrackedStatementRenewalTarget {
                        target: product("a.dot"),
                        owner: None,
                    },
                ]
            );
            // A read that rewrote the ledger could not be run without a session.
            assert_eq!(storage.writes(), writes_before);
        });
    }

    // The reader is the one ledger operation whose whole job is to run while a
    // pass may be writing: a scheduled host wakes, lists, and decides. `track`
    // is polled first so it holds the lock across its read, which is what makes
    // the guarantee observable; without the lock the listing reads the ledger
    // the track is in the middle of replacing.
    #[test]
    fn a_listing_during_a_track_reports_the_settled_ledger() {
        let storage = YieldingStorage::default();
        let ledger_lock = lock();

        futures::executor::block_on(async {
            let (tracked, listed) = futures::join!(
                track_targets(&storage, &ledger_lock, OWNER, vec![product("a.dot")]),
                list_entries(&storage, &ledger_lock),
            );
            tracked.unwrap();

            assert_eq!(
                listed.unwrap(),
                vec![TrackedStatementRenewalTarget {
                    target: product("a.dot"),
                    owner: None,
                }],
                "the listing observed the ledger the track was replacing"
            );
        });
    }

    #[test]
    fn an_undecodable_ledger_lists_as_empty() {
        let storage = MemStorage::default();

        futures::executor::block_on(async {
            storage
                .write_core_storage(CoreStorageKey::StatementRenewalTargets, vec![0xff; 8])
                .await
                .unwrap();

            assert_eq!(list_entries(&storage, &lock()).await.unwrap(), Vec::new());
        });
    }

    #[test]
    fn ledger_rejects_trailing_bytes() {
        let mut blob = vec![LedgerEntry::new(product("a.dot"), OWNER)].encode();
        blob.push(0xff);
        assert!(decode_entries(&blob).is_err());
    }

    #[test]
    fn an_undecodable_ledger_reads_as_empty() {
        let storage = MemStorage::default();

        futures::executor::block_on(async {
            storage
                .write_core_storage(
                    CoreStorageKey::StatementRenewalTargets,
                    vec![0xff, 0xff, 0xff],
                )
                .await
                .unwrap();

            assert_eq!(read_targets(&storage, OWNER).await.unwrap(), Vec::new());
        });
    }

    #[test]
    fn tracking_over_an_undecodable_ledger_starts_a_fresh_one() {
        let storage = MemStorage::default();

        futures::executor::block_on(async {
            storage
                .write_core_storage(CoreStorageKey::StatementRenewalTargets, vec![0xff; 3])
                .await
                .unwrap();
            track_targets(&storage, &lock(), OWNER, vec![product("a.dot")])
                .await
                .unwrap();

            assert_eq!(
                read_targets(&storage, OWNER).await.unwrap(),
                vec![product("a.dot")]
            );
        });
    }

    #[test]
    fn product_target_resolves_to_allocation_derivation() {
        let entropy = [7u8; 32];
        let expected =
            derive_sr25519_hard_path(&entropy, &["allowance", "statement-store", "a.dot"])
                .unwrap()
                .public
                .to_bytes();

        let resolved = resolve_target(&entropy, "paseo", &product("a.dot")).unwrap();
        assert_eq!(
            resolved,
            ResolvedRenewalTarget {
                label: "product:a.dot".to_string(),
                account_id: expected,
            }
        );
    }

    #[test]
    fn wallet_sso_target_resolves_to_the_responder_identity() {
        let entropy = [7u8; 32];
        let expected =
            crate::host_logic::product_account::derive_identity_keypair(&entropy, "paseo")
                .unwrap()
                .public
                .to_bytes();

        let resolved =
            resolve_target(&entropy, "paseo", &StatementRenewalTarget::WalletSso).unwrap();

        assert_eq!(
            resolved,
            ResolvedRenewalTarget {
                label: "wallet-sso".to_string(),
                account_id: expected,
            }
        );
    }

    #[test]
    fn untracking_one_device_preserves_wallet_sso_and_other_devices() {
        let storage = MemStorage::default();
        let first = StatementRenewalTarget::Account {
            account_id: [8; 32],
            label: "device:08".to_string(),
        };
        let second = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device:09".to_string(),
        };

        futures::executor::block_on(async {
            track_targets(
                &storage,
                &lock(),
                OWNER,
                vec![StatementRenewalTarget::WalletSso, first, second.clone()],
            )
            .await
            .unwrap();

            assert!(
                untrack_account(&storage, &lock(), OWNER, &[8; 32])
                    .await
                    .unwrap()
            );
            assert_eq!(
                read_targets(&storage, OWNER).await.unwrap(),
                vec![StatementRenewalTarget::WalletSso, second]
            );
        });
    }

    #[test]
    fn concurrent_track_and_untrack_preserve_an_unrelated_device() {
        let storage = YieldingStorage::default();
        let ledger_lock = lock();
        let first = StatementRenewalTarget::Account {
            account_id: [8; 32],
            label: "device:08".to_string(),
        };
        let second = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device:09".to_string(),
        };

        futures::executor::block_on(async {
            track_targets(&storage, &ledger_lock, OWNER, vec![first])
                .await
                .unwrap();
            let (removed, tracked) = futures::join!(
                untrack_account(&storage, &ledger_lock, OWNER, &[8; 32]),
                track_targets(&storage, &ledger_lock, OWNER, vec![second.clone()]),
            );
            assert!(removed.unwrap());
            tracked.unwrap();

            assert_eq!(read_targets(&storage.0, OWNER).await.unwrap(), vec![second]);
        });
    }

    #[test]
    fn a_raw_account_is_hidden_from_another_identity() {
        let storage = MemStorage::default();
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_targets(
                &storage,
                &lock(),
                OWNER,
                vec![device.clone(), product("a.dot")],
            )
            .await
            .unwrap();

            // The recipe resolves under any identity; the raw account does not.
            assert_eq!(
                read_targets(&storage, OWNER).await.unwrap(),
                vec![device, product("a.dot")]
            );
            assert_eq!(
                read_targets(&storage, OTHER_OWNER).await.unwrap(),
                vec![product("a.dot")]
            );
        });
    }

    #[test]
    fn the_same_raw_account_can_be_promised_by_two_identities() {
        let storage = MemStorage::default();
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_targets(&storage, &lock(), OWNER, vec![device.clone()])
                .await
                .unwrap();
            track_targets(&storage, &lock(), OTHER_OWNER, vec![device.clone()])
                .await
                .unwrap();

            // Distinct owners, so each identity renews it for itself.
            assert_eq!(
                read_targets(&storage, OWNER).await.unwrap(),
                vec![device.clone()]
            );
            assert_eq!(
                read_targets(&storage, OTHER_OWNER).await.unwrap(),
                vec![device]
            );
        });
    }

    #[test]
    fn untracking_a_device_preserves_the_other_identitys_entry() {
        let storage = MemStorage::default();
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_targets(&storage, &lock(), OWNER, vec![device.clone()])
                .await
                .unwrap();
            track_targets(&storage, &lock(), OTHER_OWNER, vec![device.clone()])
                .await
                .unwrap();

            assert!(
                untrack_account(&storage, &lock(), OWNER, &[9; 32])
                    .await
                    .unwrap()
            );
            assert_eq!(
                (
                    read_targets(&storage, OWNER).await.unwrap(),
                    read_targets(&storage, OTHER_OWNER).await.unwrap(),
                ),
                (Vec::new(), vec![device])
            );
        });
    }

    #[test]
    fn owner_key_follows_the_root_entropy() {
        assert_ne!(
            owner_key(&[7u8; 32]).unwrap(),
            owner_key(&[8u8; 32]).unwrap()
        );
    }
}
