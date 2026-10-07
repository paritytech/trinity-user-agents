//! Wallet-owned allowance renewal ledger and chain operations.

use crate::platform::{CoreStorage, CoreStorageKey, normalize_product_identifier};
use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};
use tracing::{debug, info, warn};

use super::allowance::current_unix_secs;
use super::{WalletAccountHolder, WalletKeys, require_current_session};
use crate::runtime::authority::{AccountHolder, AuthorityError, AuthoritySession};
use crate::runtime::statement_allowance::renewal::{
    RenewalChainContext, ResolvedRenewalTarget, StatementRenewalReport, renew_targets,
};
use crate::runtime::statement_allowance::{
    self, fetch_chain_state, fetch_metadata, find_including_rings,
};

/// A statement account or derivation recipe the wallet keeps renewed.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
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

/// Renewal recipes follow the active wallet; fixed accounts belong to their recorded owner.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct TrackedStatementRenewalTarget {
    /// The account, or the recipe for one, that the host promised to renew.
    pub target: StatementRenewalTarget,
    /// Root public key that promised a raw account id. A recipe carries none
    /// and resolves under whichever identity is active.
    pub owner: Option<[u8; 32]>,
}

impl StatementRenewalTarget {
    /// Match the product identifier used by account derivation.
    fn normalized(self) -> Result<Self, String> {
        match self {
            Self::ProductStatementAllowance { product_id } => {
                let normalized = normalize_product_identifier(&product_id).map_err(|_| {
                    format!("product_id {product_id} is not a valid product identifier")
                })?;
                Ok(Self::ProductStatementAllowance {
                    product_id: normalized,
                })
            }
            target @ (Self::WalletSso | Self::Account { .. }) => Ok(target),
        }
    }
}

impl TrackedStatementRenewalTarget {
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
    fn is_owned_by(&self, owner: [u8; 32]) -> bool {
        self.owner.is_none_or(|recorded| recorded == owner)
    }
}

/// Shared wallet coordination for on-demand allowance issuance and renewal.
#[derive(Default)]
pub struct RenewalState {
    /// Serializes slot registrations between the renewal pass and on-demand
    /// allocation so both cannot race for the same free slot.
    registration_lock: Mutex<()>,
    /// Serializes read-modify-write cycles on the ledger so a concurrent
    /// allocation cannot drop another's entry.
    ledger_lock: Mutex<()>,
    /// Retain loop results until the host reads them.
    last_report: std::sync::Mutex<Option<StatementRenewalReport>>,
}

impl RenewalState {
    /// Serialize registration scans with competing issuers.
    pub fn registration_lock(&self) -> &Mutex<()> {
        &self.registration_lock
    }

    fn ledger_lock(&self) -> &Mutex<()> {
        &self.ledger_lock
    }

    /// Record the pass a host has not seen the return value of.
    fn record_report(&self, report: &StatementRenewalReport) {
        if let Ok(mut last) = self.last_report.lock() {
            *last = Some(report.clone());
        }
    }

    /// The most recent pass the loop ran. `None` until one has run, which a host
    /// should read as "not yet" rather than as healthy.
    fn last_report(&self) -> Option<StatementRenewalReport> {
        self.last_report.lock().ok().and_then(|last| last.clone())
    }
}

/// Unreadable entries are discarded; later allocations rebuild the ledger.
async fn read_entries(
    storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
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

/// Read a settled ledger while competing writes hold the same lock.
async fn list_entries(
    storage: &(impl CoreStorage + ?Sized),
    ledger_lock: &Mutex<()>,
) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
    let _guard = ledger_lock.lock().await;
    read_entries(storage).await
}

async fn write_entries(
    storage: &(impl CoreStorage + ?Sized),
    entries: &[TrackedStatementRenewalTarget],
) -> Result<(), String> {
    storage
        .write_core_storage(CoreStorageKey::StatementRenewalTargets, entries.encode())
        .await
        .map_err(|err| format!("renewal ledger write failed: {}", err.reason))
}

fn decode_entries(blob: &[u8]) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
    let mut input = blob;
    let entries = Vec::<TrackedStatementRenewalTarget>::decode(&mut input)
        .map_err(|err| format!("invalid persisted renewal targets: {err}"))?;
    if !input.is_empty() {
        return Err("invalid persisted renewal targets: trailing bytes".to_string());
    }
    Ok(entries)
}

/// Stable labels identify pruned entries even without an active wallet.
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
    keys: &WalletKeys,
    target: &StatementRenewalTarget,
) -> Result<ResolvedRenewalTarget, String> {
    let label = target_label(target);
    match target {
        StatementRenewalTarget::ProductStatementAllowance { product_id } => {
            let pair = keys
                .statement_allowance_key(product_id)
                .map_err(|err| err.to_string())?;
            Ok(ResolvedRenewalTarget {
                label,
                account_id: pair.public.to_bytes(),
            })
        }
        StatementRenewalTarget::WalletSso => {
            let pair = keys.identity_keypair().map_err(|err| err.to_string())?;
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

/// Record `targets` in the ledger under the active identity.
pub async fn track_statement_renewal_targets(
    wallet: &WalletAccountHolder,
    targets: Vec<StatementRenewalTarget>,
) -> Result<(), String> {
    let session = wallet
        .current_session()
        .ok_or_else(|| AuthorityError::Disconnected.to_string())?;
    track_statement_renewal_targets_for(wallet, &session, targets).await
}

/// Keep an allocation's renewal record bound to its original wallet.
pub async fn track_statement_renewal_targets_for(
    wallet: &WalletAccountHolder,
    session: &AuthoritySession,
    targets: Vec<StatementRenewalTarget>,
) -> Result<(), String> {
    let targets = targets
        .into_iter()
        .map(StatementRenewalTarget::normalized)
        .collect::<Result<Vec<_>, _>>()?;
    let _guard = wallet.renewal.ledger_lock().lock().await;
    require_current_session(wallet, session).map_err(|error| error.to_string())?;
    let mut entries = read_entries(wallet.services.platform.as_ref()).await?;
    require_current_session(wallet, session).map_err(|error| error.to_string())?;
    let mut changed = false;
    for target in targets {
        let entry = TrackedStatementRenewalTarget::new(target, session.public_key);
        if !entries.contains(&entry) {
            entries.push(entry);
            changed = true;
        }
    }
    if changed {
        require_current_session(wallet, session).map_err(|error| error.to_string())?;
        write_entries(wallet.services.platform.as_ref(), &entries).await?;
        require_current_session(wallet, session).map_err(|error| error.to_string())?;
    }
    Ok(())
}

impl WalletAccountHolder {
    // A stale owner must not prune entries tracked by a newer wallet.
    async fn owned_targets(
        &self,
        session: &AuthoritySession,
    ) -> Result<(Vec<StatementRenewalTarget>, Vec<String>), String> {
        let _guard = self.renewal.ledger_lock().lock().await;
        require_current_session(self, session).map_err(|error| error.to_string())?;
        let entries = read_entries(self.services.platform.as_ref()).await?;
        require_current_session(self, session).map_err(|error| error.to_string())?;
        let (owned, foreign): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|entry| entry.is_owned_by(session.public_key));
        let pruned: Vec<String> = foreign
            .iter()
            .map(|entry| target_label(&entry.target))
            .collect();
        if !foreign.is_empty() {
            warn!(dropped = ?pruned, "pruning renewal targets promised by a previous identity");
            require_current_session(self, session).map_err(|error| error.to_string())?;
            write_entries(self.services.platform.as_ref(), &owned).await?;
            require_current_session(self, session).map_err(|error| error.to_string())?;
        }
        Ok((
            owned.into_iter().map(|entry| entry.target).collect(),
            pruned,
        ))
    }
}

/// List tracked entries without requiring wallet unlock.
pub async fn statement_renewal_targets(
    wallet: &WalletAccountHolder,
) -> Result<Vec<TrackedStatementRenewalTarget>, String> {
    list_entries(
        wallet.services.platform.as_ref(),
        wallet.renewal.ledger_lock(),
    )
    .await
}

/// Identify which fixed ledger entries belong to the active wallet.
pub fn statement_renewal_owner_key(wallet: &WalletAccountHolder) -> Result<[u8; 32], String> {
    wallet
        .current_session()
        .map(|session| session.public_key)
        .ok_or_else(|| AuthorityError::Disconnected.to_string())
}

/// Stop renewing one fixed statement account for the active identity.
pub async fn untrack_statement_renewal_account(
    wallet: &WalletAccountHolder,
    account_id: &[u8; 32],
) -> Result<bool, String> {
    let session = wallet
        .current_session()
        .ok_or_else(|| AuthorityError::Disconnected.to_string())?;
    let _guard = wallet.renewal.ledger_lock().lock().await;
    require_current_session(wallet, &session).map_err(|error| error.to_string())?;
    let mut entries = read_entries(wallet.services.platform.as_ref()).await?;
    require_current_session(wallet, &session).map_err(|error| error.to_string())?;
    let original_len = entries.len();
    entries.retain(|entry| {
        entry.owner != Some(session.public_key)
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
    require_current_session(wallet, &session).map_err(|error| error.to_string())?;
    write_entries(wallet.services.platform.as_ref(), &entries).await?;
    require_current_session(wallet, &session).map_err(|error| error.to_string())?;
    Ok(true)
}

/// One renewal pass: resolve the ledger against the active session and renew
/// every target for the current period.
pub async fn renew_statement_allowances(
    wallet: &WalletAccountHolder,
) -> Result<StatementRenewalReport, String> {
    let session = wallet
        .current_session()
        .ok_or_else(|| AuthorityError::Disconnected.to_string())?;
    let period = statement_allowance::slot::current_period(
        current_unix_secs().map_err(|err| err.to_string())?,
    );
    let (targets, pruned) = wallet.owned_targets(&session).await?;
    require_current_session(wallet, &session).map_err(|err| err.to_string())?;
    let resolved = wallet
        .with_keys::<_, AuthorityError>(&session, |keys| Ok(resolve_targets(keys, &targets)))
        .map_err(|error| error.to_string())?;
    if resolved.is_empty() {
        return Ok(StatementRenewalReport {
            period,
            outcomes: Vec::new(),
            pruned,
            slots_exhausted: false,
        });
    }

    let signer = wallet
        .personhood_signer(&session)
        .await
        .map_err(|error| error.to_string())?;
    let rpc = statement_allowance::rpc::RpcClient::new(
        wallet
            .services
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
    let memberships = find_including_rings(
        &rpc,
        &metadata,
        &signer,
        &super::PersonhoodCollection::ALL,
        u32::MAX,
    )
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
        signer: &signer,
        collections: &super::PersonhoodCollection::ALL,
        memberships: &memberships,
    };
    require_current_session(wallet, &session).map_err(|err| err.to_string())?;
    let mut report = renew_targets(
        &context,
        period,
        &resolved,
        wallet.renewal.registration_lock(),
    )
    .await
    .map_err(|error| error.to_string())?;
    require_current_session(wallet, &session).map_err(|err| err.to_string())?;
    report.pruned = pruned;
    Ok(report)
}

/// Renew the active wallet and record the periodic pass result.
pub async fn renewal_tick(wallet: &WalletAccountHolder) {
    if wallet.current_session().is_none() {
        debug!("skipping statement-store renewal tick; no active session");
        return;
    }
    absorb_tick(&wallet.renewal, renew_statement_allowances(wallet).await);
}

/// Most recent result from the host's periodic renewal loop.
pub fn last_statement_renewal_report(
    wallet: &WalletAccountHolder,
) -> Option<StatementRenewalReport> {
    wallet.renewal.last_report()
}

/// Skip unusable entries so they cannot prevent renewal of other targets.
fn resolve_targets(
    keys: &WalletKeys,
    targets: &[StatementRenewalTarget],
) -> Vec<ResolvedRenewalTarget> {
    targets
        .iter()
        .filter_map(|target| match resolve_target(keys, target) {
            Ok(resolved) => Some(resolved),
            Err(reason) => {
                warn!(?target, %reason, "skipping an unresolvable renewal target");
                None
            }
        })
        .collect()
}

/// Preserve the last completed pass when a later tick fails.
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
    use crate::host_logic::product_account::derive_sr25519_hard_path;
    use crate::platform::HostInfo;
    use crate::runtime::RuntimeServices;
    use crate::runtime::signing_host::wallet_account_holder;
    use crate::test_support::{StubPlatform, test_spawner};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use truapi::latest::{GenericError, HostPlatform};

    fn wallet_for(storage: Arc<dyn CoreStorage>, entropy: u8) -> WalletAccountHolder {
        let services = RuntimeServices::new(
            Arc::new(StubPlatform {
                core_storage_override: Some(storage),
                ..Default::default()
            }),
            HostInfo {
                name: "renewal test".to_string(),
                icon: None,
                version: None,
                platform: HostPlatform::Unknown,
            },
            [0; 32],
            [0; 32],
            [0; 32],
            test_spawner(),
        );
        let wallet = WalletAccountHolder::new(services, "paseo".to_string());
        wallet_account_holder::install(
            &wallet,
            wallet_account_holder::prepare_activation(&wallet, vec![entropy; 32], None).unwrap(),
        );
        wallet
    }

    #[test]
    fn stale_renewal_cannot_prune_the_replacement_wallets_targets() {
        use futures::FutureExt;

        for waiting_on_lock in [true, false] {
            let storage = Arc::new(YieldingStorage::default());
            let wallet = wallet_for(storage.clone(), 8);
            let target = StatementRenewalTarget::Account {
                account_id: [9; 32],
                label: "wallet B device".to_string(),
            };
            futures::executor::block_on(track_statement_renewal_targets(
                &wallet,
                vec![target.clone()],
            ))
            .unwrap();
            let owner = wallet.current_session().unwrap().public_key;
            wallet_account_holder::install(
                &wallet,
                wallet_account_holder::prepare_activation(&wallet, vec![7; 32], None).unwrap(),
            );
            let selected = wallet.current_session().unwrap();
            let guard = waiting_on_lock
                .then(|| futures::executor::block_on(wallet.renewal.ledger_lock().lock()));
            let renewal = renew_statement_allowances(&wallet);
            futures::pin_mut!(renewal);
            assert!(renewal.as_mut().now_or_never().is_none());
            wallet_account_holder::install(
                &wallet,
                wallet_account_holder::prepare_activation(&wallet, vec![8; 32], None).unwrap(),
            );
            drop(guard);
            if waiting_on_lock {
                futures::executor::block_on(track_statement_renewal_targets(
                    &wallet,
                    vec![target.clone()],
                ))
                .unwrap();
            }
            let result = futures::executor::block_on(renewal);
            let tracked = futures::executor::block_on(track_statement_renewal_targets_for(
                &wallet,
                &selected,
                vec![StatementRenewalTarget::Account {
                    account_id: [8; 32],
                    label: "wallet A device".to_string(),
                }],
            ));
            assert_eq!(
                (
                    result,
                    tracked,
                    futures::executor::block_on(read_entries(storage.as_ref())).unwrap()
                ),
                (
                    Err("Disconnected".to_string()),
                    Err("Disconnected".to_string()),
                    vec![TrackedStatementRenewalTarget {
                        target,
                        owner: Some(owner)
                    }]
                ),
            );
        }
    }

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

    #[crate::platform::async_trait]
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

    #[crate::platform::async_trait]
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
        let storage = Arc::new(YieldingStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;

        futures::executor::block_on(async {
            let (first, second) = futures::join!(
                track_statement_renewal_targets(&wallet, vec![product("a.dot")]),
                track_statement_renewal_targets(&wallet, vec![product("b.dot")]),
            );
            first.unwrap();
            second.unwrap();

            let mut targets = read_targets(storage.as_ref(), owner).await.unwrap();
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
    /// read too. Reading outside it lets a tracking call land in the gap and be
    /// overwritten by a view that predates it, leaving that account tracked
    /// nowhere and never renewed again.
    #[test]
    fn a_prune_does_not_overwrite_a_concurrent_track() {
        let storage = Arc::new(YieldingStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;
        let other_wallet = wallet_for(storage.clone(), 8);
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            // A foreign entry, so the pass prunes and therefore writes.
            track_statement_renewal_targets(&other_wallet, vec![device])
                .await
                .unwrap();

            let session = wallet.current_session().unwrap();
            let (pruned, tracked) = futures::join!(
                wallet.owned_targets(&session),
                track_statement_renewal_targets(&wallet, vec![product("a.dot")]),
            );
            pruned.unwrap();
            tracked.unwrap();

            assert_eq!(
                read_targets(storage.as_ref(), owner).await.unwrap(),
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
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;
        let other_wallet = wallet_for(storage.clone(), 8);
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_statement_renewal_targets(&other_wallet, vec![device])
                .await
                .unwrap();
            track_statement_renewal_targets(&wallet, vec![product("a.dot")])
                .await
                .unwrap();

            let (targets, pruned) = wallet
                .owned_targets(&wallet.current_session().unwrap())
                .await
                .unwrap();

            assert_eq!(targets, vec![product("a.dot")]);
            // Reported, not just dropped: the pass is a host's only view of the
            // ledger, so a silent prune is one it cannot notice or re-track.
            assert_eq!(pruned, vec!["device".to_string()]);
            // Dropped, not merely skipped, so the cost is paid once.
            assert_eq!(
                read_entries(storage.as_ref()).await.unwrap(),
                vec![TrackedStatementRenewalTarget::new(product("a.dot"), owner)]
            );
        });
    }

    #[test]
    fn an_all_foreign_ledger_prunes_to_nothing() {
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let other_wallet = wallet_for(storage.clone(), 8);
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_statement_renewal_targets(&other_wallet, vec![device])
                .await
                .unwrap();

            assert_eq!(
                wallet
                    .owned_targets(&wallet.current_session().unwrap())
                    .await
                    .unwrap(),
                (Vec::new(), vec!["device".to_string()])
            );
            assert_eq!(read_entries(storage.as_ref()).await.unwrap(), Vec::new());
        });
    }

    #[test]
    fn an_all_owned_ledger_is_not_rewritten() {
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);

        futures::executor::block_on(async {
            track_statement_renewal_targets(&wallet, vec![product("a.dot")])
                .await
                .unwrap();
            let after_seeding = storage.writes();

            let (targets, _pruned) = wallet
                .owned_targets(&wallet.current_session().unwrap())
                .await
                .unwrap();

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
        let keys = WalletKeys::new(vec![7; 32], "paseo".to_string());

        assert!(resolve_target(&keys, &unresolvable).is_err());
        let targets = [unresolvable, product("a.dot")];

        // Resolving strictly loses the healthy target with the broken one.
        assert!(
            targets
                .iter()
                .map(|target| resolve_target(&keys, target))
                .collect::<Result<Vec<_>, _>>()
                .is_err()
        );

        let resolved = resolve_targets(&keys, &targets);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].label, "product:a.dot");
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
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;

        futures::executor::block_on(async {
            track_statement_renewal_targets(
                &wallet,
                vec![StatementRenewalTarget::WalletSso, product("a.dot")],
            )
            .await
            .unwrap();
            track_statement_renewal_targets(
                &wallet,
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
                read_targets(storage.as_ref(), owner).await.unwrap(),
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
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_statement_renewal_targets(
                &wallet,
                vec![StatementRenewalTarget::WalletSso, device.clone()],
            )
            .await
            .unwrap();

            assert_eq!(
                statement_renewal_targets(&wallet).await.unwrap(),
                vec![
                    TrackedStatementRenewalTarget {
                        target: StatementRenewalTarget::WalletSso,
                        owner: None,
                    },
                    TrackedStatementRenewalTarget {
                        target: device,
                        owner: Some(owner),
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
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let other_wallet = wallet_for(storage.clone(), 8);
        let other_owner = other_wallet.current_session().unwrap().public_key;
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_statement_renewal_targets(&other_wallet, vec![device.clone()])
                .await
                .unwrap();
            track_statement_renewal_targets(&wallet, vec![product("a.dot")])
                .await
                .unwrap();
            let writes_before = storage.writes();

            let listed = statement_renewal_targets(&wallet).await.unwrap();

            assert_eq!(
                listed,
                vec![
                    TrackedStatementRenewalTarget {
                        target: device,
                        owner: Some(other_owner),
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
        let storage = Arc::new(YieldingStorage::default());
        let wallet = wallet_for(storage.clone(), 7);

        futures::executor::block_on(async {
            let (tracked, listed) = futures::join!(
                track_statement_renewal_targets(&wallet, vec![product("a.dot")]),
                statement_renewal_targets(&wallet),
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
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);

        futures::executor::block_on(async {
            storage
                .write_core_storage(CoreStorageKey::StatementRenewalTargets, vec![0xff; 8])
                .await
                .unwrap();

            assert_eq!(
                statement_renewal_targets(&wallet).await.unwrap(),
                Vec::new()
            );
        });
    }

    #[test]
    fn ledger_rejects_trailing_bytes() {
        let mut blob = vec![TrackedStatementRenewalTarget::new(
            product("a.dot"),
            [1; 32],
        )]
        .encode();
        blob.push(0xff);
        assert!(decode_entries(&blob).is_err());
    }

    #[test]
    fn an_undecodable_ledger_reads_as_empty() {
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;

        futures::executor::block_on(async {
            storage
                .write_core_storage(
                    CoreStorageKey::StatementRenewalTargets,
                    vec![0xff, 0xff, 0xff],
                )
                .await
                .unwrap();

            assert_eq!(
                read_targets(storage.as_ref(), owner).await.unwrap(),
                Vec::new()
            );
        });
    }

    #[test]
    fn tracking_over_an_undecodable_ledger_starts_a_fresh_one() {
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;

        futures::executor::block_on(async {
            storage
                .write_core_storage(CoreStorageKey::StatementRenewalTargets, vec![0xff; 3])
                .await
                .unwrap();
            track_statement_renewal_targets(&wallet, vec![product("a.dot")])
                .await
                .unwrap();

            assert_eq!(
                read_targets(storage.as_ref(), owner).await.unwrap(),
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

        let resolved = resolve_target(
            &WalletKeys::new(entropy.to_vec(), "paseo".to_string()),
            &product("a.dot"),
        )
        .unwrap();
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

        let resolved = resolve_target(
            &WalletKeys::new(entropy.to_vec(), "paseo".to_string()),
            &StatementRenewalTarget::WalletSso,
        )
        .unwrap();

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
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;
        let first = StatementRenewalTarget::Account {
            account_id: [8; 32],
            label: "device:08".to_string(),
        };
        let second = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device:09".to_string(),
        };

        futures::executor::block_on(async {
            track_statement_renewal_targets(
                &wallet,
                vec![StatementRenewalTarget::WalletSso, first, second.clone()],
            )
            .await
            .unwrap();

            assert!(
                untrack_statement_renewal_account(&wallet, &[8; 32])
                    .await
                    .unwrap()
            );
            assert_eq!(
                read_targets(storage.as_ref(), owner).await.unwrap(),
                vec![StatementRenewalTarget::WalletSso, second]
            );
        });
    }

    #[test]
    fn concurrent_track_and_untrack_preserve_an_unrelated_device() {
        let storage = Arc::new(YieldingStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;
        let first = StatementRenewalTarget::Account {
            account_id: [8; 32],
            label: "device:08".to_string(),
        };
        let second = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device:09".to_string(),
        };

        futures::executor::block_on(async {
            track_statement_renewal_targets(&wallet, vec![first])
                .await
                .unwrap();
            let (removed, tracked) = futures::join!(
                untrack_statement_renewal_account(&wallet, &[8; 32]),
                track_statement_renewal_targets(&wallet, vec![second.clone()]),
            );
            assert!(removed.unwrap());
            tracked.unwrap();

            assert_eq!(read_targets(&storage.0, owner).await.unwrap(), vec![second]);
        });
    }

    #[test]
    fn a_raw_account_is_hidden_from_another_identity() {
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;
        let other_wallet = wallet_for(storage.clone(), 8);
        let other_owner = other_wallet.current_session().unwrap().public_key;
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_statement_renewal_targets(&wallet, vec![device.clone(), product("a.dot")])
                .await
                .unwrap();

            // The recipe resolves under any identity; the raw account does not.
            assert_eq!(
                read_targets(storage.as_ref(), owner).await.unwrap(),
                vec![device, product("a.dot")]
            );
            assert_eq!(
                read_targets(storage.as_ref(), other_owner).await.unwrap(),
                vec![product("a.dot")]
            );
        });
    }

    #[test]
    fn the_same_raw_account_can_be_promised_by_two_identities() {
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;
        let other_wallet = wallet_for(storage.clone(), 8);
        let other_owner = other_wallet.current_session().unwrap().public_key;
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_statement_renewal_targets(&wallet, vec![device.clone()])
                .await
                .unwrap();
            track_statement_renewal_targets(&other_wallet, vec![device.clone()])
                .await
                .unwrap();

            // Distinct owners, so each identity renews it for itself.
            assert_eq!(
                read_targets(storage.as_ref(), owner).await.unwrap(),
                vec![device.clone()]
            );
            assert_eq!(
                read_targets(storage.as_ref(), other_owner).await.unwrap(),
                vec![device]
            );
        });
    }

    #[test]
    fn untracking_a_device_preserves_the_other_identitys_entry() {
        let storage = Arc::new(MemStorage::default());
        let wallet = wallet_for(storage.clone(), 7);
        let owner = wallet.current_session().unwrap().public_key;
        let other_wallet = wallet_for(storage.clone(), 8);
        let other_owner = other_wallet.current_session().unwrap().public_key;
        let device = StatementRenewalTarget::Account {
            account_id: [9; 32],
            label: "device".to_string(),
        };

        futures::executor::block_on(async {
            track_statement_renewal_targets(&wallet, vec![device.clone()])
                .await
                .unwrap();
            track_statement_renewal_targets(&other_wallet, vec![device.clone()])
                .await
                .unwrap();

            assert!(
                untrack_statement_renewal_account(&wallet, &[9; 32])
                    .await
                    .unwrap()
            );
            assert_eq!(
                (
                    read_targets(storage.as_ref(), owner).await.unwrap(),
                    read_targets(storage.as_ref(), other_owner).await.unwrap(),
                ),
                (Vec::new(), vec![device])
            );
        });
    }

    #[test]
    fn owner_key_follows_the_root_entropy() {
        assert_ne!(
            WalletKeys::new(vec![7; 32], "paseo".to_string())
                .root_public_key()
                .unwrap(),
            WalletKeys::new(vec![8; 32], "paseo".to_string())
                .root_public_key()
                .unwrap()
        );
    }

    // A product connection derives its allowance account from the normalized
    // id, so tracking any other spelling renews an account no product uses.
    #[test]
    fn a_product_target_normalizes_its_identifier() {
        for supplied in [
            "  truapi-playground.dot  ",
            "TruAPI-Playground.dot",
            "TRUAPI-PLAYGROUND.DOT",
        ] {
            assert_eq!(
                product(supplied).normalized(),
                Ok(product("truapi-playground.dot")),
                "{supplied:?} did not normalize"
            );
        }
        assert_eq!(
            product("not a product").normalized(),
            Err("product_id not a product is not a valid product identifier".to_string())
        );
        for target in [
            StatementRenewalTarget::WalletSso,
            StatementRenewalTarget::Account {
                account_id: [0x11; 32],
                label: "device".to_string(),
            },
        ] {
            assert_eq!(target.clone().normalized(), Ok(target));
        }
    }
}
