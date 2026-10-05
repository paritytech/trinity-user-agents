use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use bip39::Mnemonic;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use truapi::host_logic::product_account::{
    derive_full_person_ring_vrf_entropy, derive_identity_keypair,
    derive_lite_person_ring_vrf_entropy, product_public_key_to_address,
};

use crate::attestation;
use crate::network::NetworkConfig;
use truapi::statement_allowance as alloc;
use truapi::statement_allowance::collection::PersonhoodCollection;
use zeroize::ZeroizeOnDrop;

const ACCOUNT_STORE_FILE: &str = "accounts.json";
const ACCOUNT_STORE_LOCK_FILE: &str = "accounts.json.lock";
const DEFAULT_USERNAME_PREFIX: &str = "headless";

/// Whether an account store has been written under `base_path`.
///
/// This is the judgement "a signer was provisioned here"; the store's filename
/// stays private to this module.
pub fn has_account_store(base_path: &std::path::Path) -> bool {
    base_path.join(ACCOUNT_STORE_FILE).is_file()
}

const IMPORTED_ACCOUNT_NAME: &str = "imported";

/// Signer material selected for a signing-host session.
#[derive(Clone, derive_more::Debug)]
pub struct ResolvedSigner {
    /// BIP-39 entropy backing the selected signer account.
    #[debug("\"<redacted>\"")]
    pub entropy: Vec<u8>,
    /// Stored account name when this came from `accounts.json`.
    pub account_name: Option<String>,
    /// Lite username attested for the signer, when managed by the CLI.
    pub lite_username: Option<String>,
    /// Whether the account was selected from the CLI-managed auto pool.
    pub auto_managed: bool,
}

/// A mnemonic whose existing on-chain identity and personhood membership have
/// been verified, but which has not yet been persisted into a session.
#[derive(derive_more::Debug, ZeroizeOnDrop)]
pub struct ImportedSigner {
    #[debug("\"<redacted>\"")]
    mnemonic: String,
    #[debug("\"<redacted>\"")]
    entropy: Vec<u8>,
    #[zeroize(skip)]
    username: Option<String>,
    #[zeroize(skip)]
    session_name: String,
    #[zeroize(skip)]
    #[debug("0x{}", hex::encode(public_key))]
    public_key: [u8; 32],
    #[zeroize(skip)]
    address: String,
}

impl ImportedSigner {
    /// Existing identity username that owns the durable session.
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    /// Stable session name: the identity username when present, otherwise a
    /// non-secret fingerprint of the canonical identity public key.
    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    /// Borrow the already-derived entropy for off-side runtime activation.
    pub fn entropy(&self) -> &[u8] {
        &self.entropy
    }
}

/// Inputs for resolving a signing-host account.
#[derive(Clone, derive_more::Debug)]
pub struct ResolveSignerConfig<'a> {
    /// Directory containing the local account store.
    pub base_path: &'a Path,
    /// Network whose identity backend and People chain should be used.
    pub network: NetworkConfig,
    /// Explicit mnemonic. When present, the account store is not used.
    #[debug("{:?}", mnemonic.as_ref().map(|_| "<redacted>"))]
    pub mnemonic: Option<String>,
    /// Named stored account. Mutually exclusive with `mnemonic`.
    pub account: Option<String>,
    /// Prefix for generated Lite usernames in auto mode.
    pub lite_username_prefix: Option<String>,
    /// Full-person base name a newly created auto account reserves on dotNS
    /// alongside its lite username.
    pub reserved_username: Option<String>,
}

/// Stored signer account record.
#[derive(Clone, Serialize, Deserialize, derive_more::Debug)]
pub struct AccountRecord {
    /// Stable local account name, for example `auto-1`.
    pub name: String,
    /// Network id this account belongs to.
    pub network: String,
    /// BIP-39 mnemonic for this local test signer.
    #[debug("\"<redacted>\"")]
    pub mnemonic: String,
    /// Lite username registered through the identity backend.
    pub lite_username: String,
    /// Hex-encoded RFC-0022 `uid.<suffix>` identity public key on this
    /// account's network.
    pub public_key_hex: String,
    /// SS58 address for the RFC-0022 `uid.<suffix>` identity public key.
    pub address: String,
    /// Creation timestamp.
    pub created_at_unix: u64,
    /// Whether registration and ring readiness completed.
    #[serde(default)]
    pub attested: bool,
    /// Whether this record was provisioned automatically or imported.
    #[serde(default)]
    origin: AccountOrigin,
    #[serde(default)]
    exhausted_statement_periods: BTreeSet<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AccountOrigin {
    #[default]
    Auto,
    Imported,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AccountStoreData {
    version: u32,
    accounts: Vec<AccountRecord>,
}

/// Local JSON account store for CLI-managed signer accounts.
pub struct AccountStore {
    path: PathBuf,
    data: AccountStoreData,
}

/// Cross-process guard over the account store, held for the whole
/// provisioning flow so a concurrent instance cannot interleave writes.
///
/// The guard is taken with a *non-blocking* `try_lock_exclusive`. A blocking
/// `lock_exclusive` here would be a blocking syscall inside async code, and it
/// is held across `attest_record` and `wait_for_ring_membership`, which poll for
/// up to ~80s (`attestation.rs` and `wait_for_ring_membership` both retry 10x
/// with 4s sleeps, on top of 30s HTTP timeouts). That combination has two
/// failure modes: a second instance waits the full provisioning window with no
/// output, and — once as many waiters as tokio worker threads are parked in the
/// syscall — the holder's timer can never be polled, so it never releases and
/// the wait never ends. Refusing immediately makes the contention visible and
/// keeps the runtime schedulable.
struct AccountStoreLock {
    file: fs::File,
}

impl AccountStoreLock {
    fn acquire(base_path: &Path) -> Result<Self> {
        fs::create_dir_all(base_path).with_context(|| format!("create {}", base_path.display()))?;
        let path = base_path.join(ACCOUNT_STORE_LOCK_FILE);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("open lock {}", path.display()))?;
        file.try_lock_exclusive().map_err(|err| {
            anyhow::anyhow!(
                "another truapi-host is using the account store at {} \
                 ({err}); wait for it to finish or pass a different \
                 --account-base-path",
                base_path.display()
            )
        })?;
        Ok(Self { file })
    }
}

impl Drop for AccountStoreLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

impl AccountStore {
    pub fn load(base_path: &Path) -> Result<Self> {
        let path = base_path.join(ACCOUNT_STORE_FILE);
        let data = match fs::read_to_string(&path) {
            Ok(text) => {
                serde_json::from_str(&text).with_context(|| format!("decode {}", path.display()))?
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => AccountStoreData {
                version: 1,
                accounts: Vec::new(),
            },
            Err(err) => return Err(err).with_context(|| format!("read {}", path.display())),
        };
        Ok(Self { path, data })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(&self.data)?;
        write_secret_file(&self.path, text.as_bytes())
            .with_context(|| format!("write {}", self.path.display()))
    }

    pub fn get(&self, network_id: &str, name: &str) -> Option<&AccountRecord> {
        self.data
            .accounts
            .iter()
            .find(|record| record.network == network_id && record.name == name)
    }

    /// Use the bound account's creation time, or the first identity stored for the network.
    /// A missing bound account is an error rather than a reason to select another identity.
    pub fn created_at(&self, network_id: &str, account_name: Option<&str>) -> Result<Option<u64>> {
        if let Some(name) = account_name {
            let record = self
                .get(network_id, name)
                .with_context(|| format!("account {name:?} not found for {network_id}"))?;
            return Ok(Some(record.created_at_unix));
        }
        Ok(self
            .data
            .accounts
            .iter()
            .filter(|record| record.network == network_id)
            .map(|record| record.created_at_unix)
            .min())
    }

    fn upsert(&mut self, record: AccountRecord) {
        if let Some(existing) = self
            .data
            .accounts
            .iter_mut()
            .find(|existing| existing.network == record.network && existing.name == record.name)
        {
            *existing = record;
            return;
        }
        self.data.accounts.push(record);
    }

    fn auto_candidate(&self, network_id: &str, period: u32) -> Option<AccountRecord> {
        self.data
            .accounts
            .iter()
            .find(|record| {
                record.network == network_id
                    && record.origin == AccountOrigin::Auto
                    && record.attested
                    && !record.exhausted_statement_periods.contains(&period)
            })
            .cloned()
    }

    fn pending_auto_candidate(&self, network_id: &str) -> Option<AccountRecord> {
        self.data
            .accounts
            .iter()
            .find(|record| {
                record.network == network_id
                    && record.origin == AccountOrigin::Auto
                    && !record.attested
            })
            .cloned()
    }

    fn next_auto_name(&self, network_id: &str) -> String {
        let mut index = 1usize;
        loop {
            let name = format!("auto-{index}");
            if !self
                .data
                .accounts
                .iter()
                .any(|record| record.network == network_id && record.name == name)
            {
                return name;
            }
            index += 1;
        }
    }

    pub fn mark_exhausted(&mut self, network_id: &str, name: &str, period: u32) -> Result<()> {
        let Some(record) = self
            .data
            .accounts
            .iter_mut()
            .find(|record| record.network == network_id && record.name == name)
        else {
            return Ok(());
        };
        record.exhausted_statement_periods.insert(period);
        self.save()
    }
}

/// Remove cached local accounts owned by one network and, optionally, one
/// durable Lite username. Records for other networks are always preserved.
pub fn remove_managed_accounts(
    base_path: &Path,
    network_id: &str,
    lite_username: Option<&str>,
) -> Result<usize> {
    if !base_path.join(ACCOUNT_STORE_FILE).is_file() {
        return Ok(0);
    }
    let _lock = AccountStoreLock::acquire(base_path)?;
    let mut store = AccountStore::load(base_path)?;
    let before = store.data.accounts.len();
    store.data.accounts.retain(|record| {
        record.network != network_id
            || lite_username.is_some_and(|username| record.lite_username != username)
    });
    let removed = before - store.data.accounts.len();
    if removed > 0 {
        store.save()?;
    }
    Ok(removed)
}

pub async fn resolve_signer(config: ResolveSignerConfig<'_>) -> Result<ResolvedSigner> {
    if let Some(mnemonic) = config.mnemonic {
        let entropy = mnemonic_entropy(&mnemonic)?;
        return Ok(ResolvedSigner {
            entropy,
            account_name: None,
            lite_username: None,
            auto_managed: false,
        });
    }

    let _lock = AccountStoreLock::acquire(config.base_path)?;
    let mut store = AccountStore::load(config.base_path)?;
    if let Some(name) = config.account {
        let record = store
            .get(config.network.id, &name)
            .cloned()
            .with_context(|| format!("account {name:?} not found for {}", config.network.id))?;
        let record = ensure_record_ready(
            &mut store,
            config.network,
            &record,
            config.reserved_username.as_deref(),
        )
        .await?;
        return resolved_from_record(record, false);
    }

    let period = current_statement_period()?;
    if let Some(record) = store.auto_candidate(config.network.id, period) {
        let record = ensure_record_ready(
            &mut store,
            config.network,
            &record,
            config.reserved_username.as_deref(),
        )
        .await?;
        return resolved_from_record(record, true);
    }

    if let Some(record) = store.pending_auto_candidate(config.network.id) {
        let refreshed = ensure_record_ready(
            &mut store,
            config.network,
            &record,
            config.reserved_username.as_deref(),
        )
        .await?;
        return resolved_from_record(refreshed, true);
    }

    let record = create_auto_account(
        &mut store,
        config.network,
        config
            .lite_username_prefix
            .as_deref()
            .unwrap_or(DEFAULT_USERNAME_PREFIX),
        config.reserved_username.as_deref(),
    )
    .await?;
    resolved_from_record(record, true)
}

/// Resolve the signer's optional dotNS mirror or identity-backend username and
/// validate an existing mnemonic against the personhood rings.
///
/// This is deliberately read-only: it does not submit username registration
/// and does not write the mnemonic until the caller has activated a new runtime.
pub async fn inspect_imported_signer(
    network: NetworkConfig,
    mnemonic: &str,
) -> Result<ImportedSigner> {
    let mnemonic = Mnemonic::parse(mnemonic.trim())
        .context("invalid BIP-39 mnemonic")?
        .to_string();
    let identity = identity_from_mnemonic(&mnemonic, network.network_suffix)?;
    let username = attestation::lookup_registered_username(network, &identity.entropy)
        .await
        .with_context(|| {
            format!(
                "look up the mnemonic's optional dotNS username on {}",
                network.id
            )
        })?;
    let username = match username {
        Some(username) => Some(username),
        None => attestation::lookup_backend_username(network, &identity.entropy, &identity.address)
            .await
            .with_context(|| {
                format!(
                    "reverse-resolve the mnemonic's assigned identity-backend username on {}",
                    network.id
                )
            })?,
    };
    wait_for_ring_membership(network, &identity.entropy)
        .await
        .with_context(|| {
            let account = username.as_deref().unwrap_or(&identity.address);
            format!(
                "verify personhood ring membership for account {account:?} on {}",
                network.id
            )
        })?;
    let session_name = imported_session_name(username.as_deref(), &identity.public_key);
    Ok(ImportedSigner {
        mnemonic,
        entropy: identity.entropy,
        username,
        session_name,
        public_key: identity.public_key,
        address: identity.address,
    })
}

/// Persist a previously inspected signer in one session account store.
/// Existing imports are idempotent, but a different key cannot overwrite the
/// session's imported signer.
pub fn persist_imported_signer(
    base_path: &Path,
    network_id: &str,
    imported: &ImportedSigner,
) -> Result<ResolvedSigner> {
    let _lock = AccountStoreLock::acquire(base_path)?;
    let mut store = AccountStore::load(base_path)?;
    let public_key_hex = format!("0x{}", hex::encode(imported.public_key));
    if let Some(existing) = store.get(network_id, IMPORTED_ACCOUNT_NAME)
        && existing.public_key_hex != public_key_hex
    {
        bail!(
            "session {:?} is already bound to a different imported signer",
            imported.session_name
        );
    }
    let created_at_unix = store
        .get(network_id, IMPORTED_ACCOUNT_NAME)
        .map_or_else(now_unix, |record| record.created_at_unix);
    let exhausted_statement_periods = store
        .get(network_id, IMPORTED_ACCOUNT_NAME)
        .map_or_else(BTreeSet::new, |record| {
            record.exhausted_statement_periods.clone()
        });
    let record = AccountRecord {
        name: IMPORTED_ACCOUNT_NAME.to_string(),
        network: network_id.to_string(),
        mnemonic: imported.mnemonic.clone(),
        lite_username: imported.username.clone().unwrap_or_default(),
        public_key_hex,
        address: imported.address.clone(),
        created_at_unix,
        attested: true,
        origin: AccountOrigin::Imported,
        exhausted_statement_periods,
    };
    store.upsert(record.clone());
    store.save()?;
    resolved_from_record(record, false)
}

/// Resolve an already-provisioned signer from local state without network
/// attestation or ring-membership checks.
pub fn resolve_cached_signer(
    base_path: &Path,
    network_id: &str,
    account: Option<&str>,
) -> Result<Option<ResolvedSigner>> {
    let _lock = AccountStoreLock::acquire(base_path)?;
    let store = AccountStore::load(base_path)?;
    let (record, auto_managed) = if let Some(name) = account {
        (store.get(network_id, name).cloned(), false)
    } else {
        let period = current_statement_period()?;
        (store.auto_candidate(network_id, period), true)
    };
    let Some(record) = record.filter(|record| {
        record.attested
            && (record.origin == AccountOrigin::Imported
                || resolved_lite_username(&record.lite_username))
    }) else {
        return Ok(None);
    };
    resolved_from_record(record, auto_managed).map(Some)
}

pub fn mark_account_exhausted(
    base_path: &Path,
    network_id: &str,
    name: &str,
    period: u32,
) -> Result<()> {
    let _lock = AccountStoreLock::acquire(base_path)?;
    AccountStore::load(base_path)?.mark_exhausted(network_id, name, period)
}

pub fn current_statement_period() -> Result<u32> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before UNIX epoch")?
        .as_secs();
    Ok(alloc::slot::current_period(now))
}

async fn create_auto_account(
    store: &mut AccountStore,
    network: NetworkConfig,
    username_prefix: &str,
    reserved_username: Option<&str>,
) -> Result<AccountRecord> {
    let lite_username = lite_username_base(username_prefix)?;
    let name = store.next_auto_name(network.id);
    let mnemonic = Mnemonic::generate(12)
        .context("generate BIP-39 mnemonic")?
        .to_string();
    let identity = identity_from_mnemonic(&mnemonic, network.network_suffix)?;

    if !attestation::lite_username_available(network, &identity.entropy, &lite_username)
        .await
        .with_context(|| format!("check lite username {lite_username:?} availability"))?
    {
        bail!("lite username {lite_username:?} is taken; choose a different session name");
    }

    let mut record = AccountRecord {
        name,
        network: network.id.to_string(),
        mnemonic,
        lite_username,
        public_key_hex: format!("0x{}", hex::encode(identity.public_key)),
        address: identity.address,
        created_at_unix: now_unix(),
        attested: false,
        origin: AccountOrigin::Auto,
        exhausted_statement_periods: BTreeSet::new(),
    };
    store.upsert(record.clone());
    store.save()?;

    debug!(
        account = %record.name,
        network = %record.network,
        lite_username = %record.lite_username,
        address = %record.address,
        "created auto signer account"
    );

    record.lite_username = attest_record(network, &record, reserved_username).await?;
    finish_account_readiness(
        store,
        record,
        wait_for_ring_membership(network, &identity.entropy),
    )
    .await
}

async fn ensure_record_ready(
    store: &mut AccountStore,
    network: NetworkConfig,
    record: &AccountRecord,
    reserved_username: Option<&str>,
) -> Result<AccountRecord> {
    let identity = identity_from_mnemonic(&record.mnemonic, network.network_suffix)?;
    if record.origin == AccountOrigin::Imported {
        wait_for_ring_membership(network, &identity.entropy).await?;
        return Ok(record.clone());
    }
    let mut record = record.clone();
    if !record.attested {
        record.lite_username = attest_record(network, &record, reserved_username).await?;
    } else {
        record.lite_username = attestation::registered_lite_username(network, &identity.entropy)
            .await
            .with_context(|| format!("resolve Lite username for account {}", record.name))?;
    }
    finish_account_readiness(
        store,
        record,
        wait_for_ring_membership(network, &identity.entropy),
    )
    .await
}

async fn finish_account_readiness(
    store: &mut AccountStore,
    mut record: AccountRecord,
    readiness: impl std::future::Future<Output = Result<()>>,
) -> Result<AccountRecord> {
    readiness.await?;
    record.attested = true;
    if store
        .get(&record.network, &record.name)
        .is_none_or(|stored| stored.lite_username != record.lite_username || !stored.attested)
    {
        store.upsert(record.clone());
        store.save()?;
    }
    Ok(record)
}

async fn attest_record(
    network: NetworkConfig,
    record: &AccountRecord,
    reserved_username: Option<&str>,
) -> Result<String> {
    let entropy = mnemonic_entropy(&record.mnemonic)?;
    let lite_username = attestation::attest(&attestation::AttestConfig {
        backend_base: network.identity_backend_base.to_string(),
        asset_hub_ws: network.asset_hub_ws.to_string(),
        network_suffix: network.network_suffix.to_string(),
        entropy,
        username_base: record.lite_username.clone(),
        reserved_username: reserved_username.map(str::to_string),
    })
    .await
    .with_context(|| format!("attest account {}", record.name))?;
    debug!(
        account = %record.name,
        requested_lite_username = %record.lite_username,
        assigned_lite_username = %lite_username,
        "signer account attested"
    );
    Ok(lite_username)
}

fn resolved_lite_username(username: &str) -> bool {
    username
        .rsplit_once('.')
        .is_some_and(|(name, discriminator)| !name.is_empty() && !discriminator.is_empty())
}

/// Every personhood collection candidate for `entropy` on the network whose
/// dotNS TLD is `network_suffix`, widest slot budget first.
///
/// Both are always offered; membership is settled on chain, not from local state.
pub fn collection_candidates(
    entropy: &[u8],
    network_suffix: &str,
) -> Vec<alloc::CollectionCandidate> {
    vec![
        alloc::CollectionCandidate {
            collection: PersonhoodCollection::People,
            entropy: derive_full_person_ring_vrf_entropy(entropy, network_suffix),
        },
        alloc::CollectionCandidate {
            collection: PersonhoodCollection::LitePeople,
            entropy: derive_lite_person_ring_vrf_entropy(entropy, network_suffix),
        },
    ]
}

async fn wait_for_ring_membership(network: NetworkConfig, entropy: &[u8]) -> Result<()> {
    const MAX_ATTEMPTS: usize = 30;
    const SLEEP: Duration = Duration::from_secs(4);

    let people_ws = network.people_ws;
    let candidates = collection_candidates(entropy, network.network_suffix);
    let mut metadata = None;
    for attempt in 1..=MAX_ATTEMPTS {
        crate::terminal_ui::update_activity(
            "signer",
            "Setting up signer",
            Some(format!(
                "Waiting for personhood ring membership · attempt {attempt}/{MAX_ATTEMPTS}"
            )),
            crate::terminal_ui::ActivityState::Running,
        );
        let rpc = match alloc::rpc::RpcClient::connect(people_ws).await {
            Ok(rpc) => rpc,
            Err(err) => {
                warn!(
                    attempt,
                    max_attempts = MAX_ATTEMPTS,
                    error = %err,
                    "could not connect while checking personhood ring membership"
                );
                sleep_ring_poll(attempt, MAX_ATTEMPTS, SLEEP).await;
                continue;
            }
        };
        if metadata.is_none() {
            match alloc::fetch_metadata(&rpc).await {
                Ok(fetched) => metadata = Some(fetched),
                Err(err) => {
                    warn!(
                        attempt,
                        max_attempts = MAX_ATTEMPTS,
                        error = %err,
                        "could not fetch metadata while checking personhood ring membership"
                    );
                    sleep_ring_poll(attempt, MAX_ATTEMPTS, SLEEP).await;
                    continue;
                }
            }
        }
        let metadata_ref = metadata.as_ref().expect("metadata is initialized");
        // Every ring back to index 0: the signer may sit in an older one.
        match alloc::find_including_rings(&rpc, metadata_ref, &candidates, u32::MAX).await {
            Ok(memberships) if !memberships.is_empty() => {
                let held = memberships
                    .iter()
                    .map(|membership| membership.collection().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                crate::terminal_ui::update_activity(
                    "signer",
                    "Setting up signer",
                    Some(format!("personhood ring membership ready ({held})")),
                    crate::terminal_ui::ActivityState::Running,
                );
                return Ok(());
            }
            Ok(_) => {}
            Err(err) => {
                warn!(
                    attempt,
                    max_attempts = MAX_ATTEMPTS,
                    error = %err,
                    "could not scan personhood rings"
                );
            }
        }
        sleep_ring_poll(attempt, MAX_ATTEMPTS, SLEEP).await;
    }
    bail!("signer account did not appear in any personhood ring");
}

async fn sleep_ring_poll(attempt: usize, max_attempts: usize, sleep: Duration) {
    if attempt < max_attempts {
        debug!(
            attempt,
            max_attempts, "signer account not in any personhood ring yet"
        );
        tokio::time::sleep(sleep).await;
    }
}

fn resolved_from_record(record: AccountRecord, auto_managed: bool) -> Result<ResolvedSigner> {
    let entropy = mnemonic_entropy(&record.mnemonic)?;
    let lite_username = (!record.lite_username.is_empty()).then_some(record.lite_username);
    Ok(ResolvedSigner {
        entropy,
        account_name: Some(record.name),
        lite_username,
        auto_managed,
    })
}

fn imported_session_name(username: Option<&str>, public_key: &[u8; 32]) -> String {
    username.map_or_else(
        || format!("imported-{}", hex::encode(&public_key[..8])),
        str::to_string,
    )
}

struct SignerIdentity {
    entropy: Vec<u8>,
    public_key: [u8; 32],
    address: String,
}

fn identity_from_mnemonic(mnemonic: &str, network_suffix: &str) -> Result<SignerIdentity> {
    let entropy = mnemonic_entropy(mnemonic)?;
    let candidate = derive_identity_keypair(&entropy, network_suffix)
        .map_err(|err| anyhow::anyhow!("uid identity derivation failed: {err}"))?;
    let public_key = candidate.public.to_bytes();
    Ok(SignerIdentity {
        entropy,
        public_key,
        address: product_public_key_to_address(public_key),
    })
}

fn mnemonic_entropy(mnemonic: &str) -> Result<Vec<u8>> {
    Ok(Mnemonic::parse(mnemonic.trim())
        .context("invalid BIP-39 mnemonic")?
        .to_entropy())
}

/// The two rejections are reported apart because they suggest opposite fixes.
/// A prefix is most often made unique by appending digits or a hyphen, and
/// answering that with a length complaint sends the reader to lengthen a prefix
/// that is already long enough.
fn lite_username_base(prefix: &str) -> Result<String> {
    if !prefix.bytes().all(|byte| byte.is_ascii_lowercase()) {
        bail!(
            "lite username base must be lowercase ASCII letters only; \
             digits, hyphens and uppercase are not accepted"
        );
    }
    if prefix.len() < 6 {
        bail!("lite username base must contain at least 6 lowercase ASCII letters");
    }
    Ok(prefix.to_string())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_secret_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp_path = temp_path(path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp_path)?;
        file.write_all(bytes)?;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        sync_parent(path)
    }
    #[cfg(not(unix))]
    {
        fs::write(&tmp_path, bytes)?;
        let _ = fs::remove_file(path);
        fs::rename(&tmp_path, path)
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(ACCOUNT_STORE_FILE);
    path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()))
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn record(name: &str, network: &str, attested: bool) -> AccountRecord {
        AccountRecord {
            name: name.to_string(),
            network: network.to_string(),
            mnemonic: MNEMONIC.to_string(),
            lite_username: format!("{name}lite.01"),
            public_key_hex: "0x00".to_string(),
            address: "5GrwvaEF5zXb26Fz9rcQpDWSKfwVwqNxyvE9uZunJMtBEw2s".to_string(),
            created_at_unix: 1,
            attested,
            origin: AccountOrigin::Auto,
            exhausted_statement_periods: BTreeSet::new(),
        }
    }

    #[tokio::test]
    async fn interrupted_ring_readiness_keeps_the_same_account_pending() -> Result<()> {
        let directory = tempdir()?;
        let network_id = "paseo-next-v2";
        let mut store = AccountStore::load(directory.path())?;
        let mut pending = record("auto-1", network_id, false);
        pending.lite_username = "pending".to_string();
        store.upsert(pending.clone());
        store.save()?;
        let original_accounts = fs::read(&store.path)?;
        let assert_pending = || -> Result<()> {
            let reloaded = AccountStore::load(directory.path())?;
            assert_eq!(
                (
                    fs::read(&reloaded.path)?,
                    resolve_cached_signer(directory.path(), network_id, None)?
                        .map(|signer| signer.entropy),
                    serde_json::to_value(reloaded.pending_auto_candidate(network_id))?,
                ),
                (
                    original_accounts.clone(),
                    None,
                    serde_json::to_value(&pending)?,
                )
            );
            Ok(())
        };
        let mut registered = pending.clone();
        registered.lite_username = "pending.01".to_string();

        let mut cancelled = Box::pin(finish_account_readiness(
            &mut store,
            registered.clone(),
            std::future::pending(),
        ));
        assert!(futures::poll!(cancelled.as_mut()).is_pending());
        drop(cancelled);
        assert_pending()?;

        finish_account_readiness(&mut store, registered.clone(), async {
            bail!("ring lookup failed");
        })
        .await
        .expect_err("failed ring readiness must remain retryable");
        assert_pending()?;

        finish_account_readiness(&mut store, registered, async { Ok(()) }).await?;
        let cached = resolve_cached_signer(directory.path(), network_id, None)?
            .expect("completed ring readiness makes the original account usable");
        let reloaded = AccountStore::load(directory.path())?;
        assert_eq!(
            (
                reloaded.data.accounts.len(),
                reloaded.pending_auto_candidate(network_id).is_none(),
                cached.account_name,
                cached.entropy,
                cached.lite_username,
            ),
            (
                1,
                true,
                Some("auto-1".to_string()),
                vec![0; 16],
                Some("pending.01".to_string()),
            )
        );
        Ok(())
    }

    #[test]
    fn debug_output_redacts_signer_secrets() {
        let signer = ResolvedSigner {
            entropy: vec![0xab; 32],
            account_name: Some("auto-1".to_string()),
            lite_username: None,
            auto_managed: true,
        };
        let rendered = format!("{signer:?}");
        assert!(
            !rendered.contains("171"),
            "entropy bytes leaked: {rendered}"
        );
        assert!(rendered.contains("<redacted>"));
        assert!(rendered.contains("auto-1"), "non-secret fields dropped");

        let config = ResolveSignerConfig {
            base_path: Path::new("/tmp"),
            network: crate::network::Network::PaseoNextV2.config(),
            mnemonic: Some(MNEMONIC.to_string()),
            account: None,
            lite_username_prefix: None,
            reserved_username: None,
        };
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("abandon"), "mnemonic leaked: {rendered}");
        assert!(rendered.contains("<redacted>"));

        // AccountStoreData renders records through AccountRecord's impl, so the
        // whole store is covered by redacting the record.
        let mut store = AccountStore {
            path: PathBuf::from("accounts.json"),
            data: AccountStoreData::default(),
        };
        store.upsert(record("auto-1", "paseo-next-v2", true));
        let rendered = format!("{:?}", store.data);
        assert!(!rendered.contains("abandon"), "mnemonic leaked: {rendered}");
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn numerical_aliases_make_random_username_suffixes_unnecessary() -> Result<()> {
        assert_eq!(lite_username_base("majority")?, "majority");
        Ok(())
    }

    #[test]
    fn lite_username_prefix_requires_the_backend_minimum() {
        assert_eq!(
            lite_username_base("short").unwrap_err().to_string(),
            "lite username base must contain at least 6 lowercase ASCII letters"
        );
    }

    #[test]
    fn a_long_enough_prefix_is_not_refused_for_its_length() {
        // The shape a reader reaches for when told to pass a different prefix:
        // long enough, but made unique with something other than a letter. It
        // must not come back as a length complaint.
        for prefix in ["headless12ab", "headless-run", "Headlessabcd"] {
            let error = lite_username_base(prefix)
                .expect_err("a non-letter prefix is refused")
                .to_string();
            assert!(
                error.contains("lowercase ASCII letters only"),
                "{prefix:?} reported as {error:?}"
            );
        }
        assert_eq!(lite_username_base("abcdefghijkl").unwrap(), "abcdefghijkl");
    }

    #[test]
    fn auto_candidate_skips_pending_and_exhausted_accounts() {
        let mut store = AccountStore {
            path: PathBuf::from("accounts.json"),
            data: AccountStoreData::default(),
        };
        let mut exhausted = record("auto-1", "paseo-next-v2", true);
        exhausted.exhausted_statement_periods.insert(7);
        store.upsert(exhausted);
        store.upsert(record("auto-2", "paseo-next-v2", false));
        store.upsert(record("auto-3", "paseo-next-v2", true));

        assert_eq!(
            store
                .auto_candidate("paseo-next-v2", 7)
                .map(|record| record.name),
            Some("auto-3".to_string())
        );
    }

    #[test]
    fn pending_auto_candidate_reuses_failed_onboarding_record() {
        let mut store = AccountStore {
            path: PathBuf::from("accounts.json"),
            data: AccountStoreData::default(),
        };
        store.upsert(record("auto-1", "paseo-next-v2", false));
        store.upsert(record("auto-2", "other", false));

        assert_eq!(
            store
                .pending_auto_candidate("paseo-next-v2")
                .map(|record| record.name),
            Some("auto-1".to_string())
        );
    }

    #[test]
    fn save_roundtrips_account_store() -> Result<()> {
        let dir = tempdir()?;
        let mut store = AccountStore::load(dir.path())?;
        store.upsert(record("auto-1", "paseo-next-v2", true));
        store.save()?;

        let loaded = AccountStore::load(dir.path())?;

        assert_eq!(
            loaded
                .get("paseo-next-v2", "auto-1")
                .map(|record| record.name.as_str()),
            Some("auto-1")
        );
        assert!(!temp_path(&dir.path().join(ACCOUNT_STORE_FILE)).exists());
        Ok(())
    }

    #[test]
    fn removing_managed_accounts_preserves_other_sessions_and_networks() -> Result<()> {
        let dir = tempdir()?;
        let mut store = AccountStore::load(dir.path())?;
        store.upsert(record("alice", "paseo-next-v2", true));
        store.upsert(record("bob", "paseo-next-v2", true));
        store.upsert(record("alice", "other", true));
        store.save()?;

        assert_eq!(
            remove_managed_accounts(dir.path(), "paseo-next-v2", Some("alicelite.01"))?,
            1
        );
        let loaded = AccountStore::load(dir.path())?;
        assert!(loaded.get("paseo-next-v2", "alice").is_none());
        assert!(loaded.get("paseo-next-v2", "bob").is_some());
        assert!(loaded.get("other", "alice").is_some());

        assert_eq!(
            remove_managed_accounts(dir.path(), "paseo-next-v2", None)?,
            1
        );
        let loaded = AccountStore::load(dir.path())?;
        assert!(loaded.get("paseo-next-v2", "bob").is_none());
        assert!(loaded.get("other", "alice").is_some());
        Ok(())
    }

    #[test]
    fn a_second_store_lock_is_refused_rather_than_waited_on() -> Result<()> {
        let dir = tempdir()?;
        let held = AccountStoreLock::acquire(dir.path())?;

        // With a blocking `lock_exclusive` this call parks the calling thread
        // for as long as the first guard lives — inside async code that is up
        // to the whole ~80s provisioning window, and it deadlocks outright once
        // every tokio worker is parked here. It must fail fast instead.
        let err = match AccountStoreLock::acquire(dir.path()) {
            Ok(_) => panic!("a second exclusive lock must be refused"),
            Err(err) => err,
        };
        let message = format!("{err:#}");
        assert!(
            message.contains("another truapi-host is using the account store"),
            "unexpected error: {message}"
        );

        // Releasing the first guard makes the store available again.
        drop(held);
        let _reacquired = AccountStoreLock::acquire(dir.path())?;
        Ok(())
    }

    #[test]
    fn cached_signer_resolves_without_network_access() -> Result<()> {
        let dir = tempdir()?;
        let mut store = AccountStore::load(dir.path())?;
        store.upsert(record("auto-1", "paseo-next-v2", true));
        store.save()?;

        let signer =
            resolve_cached_signer(dir.path(), "paseo-next-v2", None)?.expect("cached signer");

        assert_eq!(signer.account_name.as_deref(), Some("auto-1"));
        assert_eq!(signer.lite_username.as_deref(), Some("auto-1lite.01"));
        assert!(signer.auto_managed);
        Ok(())
    }

    #[test]
    fn cached_signer_ignores_legacy_username_base() -> Result<()> {
        let dir = tempdir()?;
        let mut store = AccountStore::load(dir.path())?;
        let mut stale = record("auto-1", "paseo-next-v2", true);
        stale.lite_username = "headlessabcdef".to_string();
        store.upsert(stale);
        store.save()?;

        assert!(resolve_cached_signer(dir.path(), "paseo-next-v2", None)?.is_none());
        Ok(())
    }

    #[test]
    fn imported_signer_is_durable_named_and_excluded_from_auto_pool() -> Result<()> {
        let dir = tempdir()?;
        let identity = identity_from_mnemonic(MNEMONIC, "paseo")?;
        let imported = ImportedSigner {
            mnemonic: MNEMONIC.to_string(),
            entropy: identity.entropy,
            username: Some("alice.01".to_string()),
            session_name: "alice.01".to_string(),
            public_key: identity.public_key,
            address: identity.address,
        };

        let signer = persist_imported_signer(dir.path(), "paseo-next-v2", &imported)?;

        assert_eq!(signer.account_name.as_deref(), Some(IMPORTED_ACCOUNT_NAME));
        assert_eq!(signer.lite_username.as_deref(), Some("alice.01"));
        assert!(!signer.auto_managed);
        let cached =
            resolve_cached_signer(dir.path(), "paseo-next-v2", Some(IMPORTED_ACCOUNT_NAME))?
                .expect("imported signer is cached");
        assert_eq!(cached.lite_username.as_deref(), Some("alice.01"));
        let store = AccountStore::load(dir.path())?;
        assert!(store.auto_candidate("paseo-next-v2", 7).is_none());

        let rendered = format!("{imported:?}");
        assert!(!rendered.contains("abandon"), "mnemonic leaked: {rendered}");
        assert!(rendered.contains("<redacted>"));
        Ok(())
    }

    #[test]
    fn imported_signer_without_dotns_username_is_still_cached() -> Result<()> {
        let dir = tempdir()?;
        let identity = identity_from_mnemonic(MNEMONIC, "paseo")?;
        let session_name = imported_session_name(None, &identity.public_key);
        let imported = ImportedSigner {
            mnemonic: MNEMONIC.to_string(),
            entropy: identity.entropy,
            username: None,
            session_name: session_name.clone(),
            public_key: identity.public_key,
            address: identity.address,
        };

        assert!(session_name.starts_with("imported-"));
        let signer = persist_imported_signer(dir.path(), "paseo-next-v2", &imported)?;
        assert_eq!(signer.lite_username, None);
        let cached =
            resolve_cached_signer(dir.path(), "paseo-next-v2", Some(IMPORTED_ACCOUNT_NAME))?
                .expect("username-less imported signer is cached");
        assert_eq!(cached.lite_username, None);
        assert!(!cached.auto_managed);
        Ok(())
    }
}
