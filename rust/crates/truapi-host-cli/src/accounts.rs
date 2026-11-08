use std::{
	collections::BTreeSet,
	fs::{self, OpenOptions},
	io::Write,
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use bip39::Mnemonic;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use truapi::host_logic::product_account::{
	ProductAccountError, derive_full_person_ring_vrf_entropy, derive_identity_keypair,
	derive_lite_person_ring_vrf_entropy, product_public_key_to_address,
};

use crate::{attestation, network::Stack};
use truapi::{statement_allowance as alloc, statement_allowance::collection::PersonhoodCollection};
use zeroize::ZeroizeOnDrop;

const ACCOUNT_STORE_FILE: &str = "accounts.json";
const ACCOUNT_STORE_LOCK_FILE: &str = "accounts.json.lock";
const DEFAULT_USERNAME_PREFIX: &str = "headless";

/// The account's identity key could not be derived from its entropy.
#[derive(derive_more::Display, derive_more::Error, derive_more::From, Debug)]
#[display("identity account derivation failed")]
pub struct IdentityDerivationError(ProductAccountError);

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
	pub network: &'a Stack,
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
	/// Hex-encoded RFC-0022 `peopl.<suffix>` identity public key on this
	/// account's network.
	pub public_key_hex: String,
	/// SS58 address for the RFC-0022 `peopl.<suffix>` identity public key.
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

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
enum AccountOrigin {
	#[default]
	Auto,
	Imported,
}

#[derive(Default, Serialize, Deserialize, Debug)]
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
			},
			Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
				AccountStoreData { version: 1, accounts: Vec::new() }
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
				record.network == network_id &&
					record.origin == AccountOrigin::Auto &&
					record.attested && !record.exhausted_statement_periods.contains(&period)
			})
			.cloned()
	}

	fn pending_auto_candidate(&self, network_id: &str) -> Option<AccountRecord> {
		self.data
			.accounts
			.iter()
			.find(|record| {
				record.network == network_id &&
					record.origin == AccountOrigin::Auto &&
					!record.attested
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
		record.network != network_id ||
			lite_username.is_some_and(|username| record.lite_username != username)
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
		let record = store.get(config.network.namespace(), &name).cloned().with_context(|| {
			format!("account {name:?} not found for {}", config.network.namespace())
		})?;
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
	if let Some(record) = store.auto_candidate(config.network.namespace(), period) {
		let record = ensure_record_ready(
			&mut store,
			config.network,
			&record,
			config.reserved_username.as_deref(),
		)
		.await?;
		return resolved_from_record(record, true);
	}

	if let Some(record) = store.pending_auto_candidate(config.network.namespace()) {
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
		config.lite_username_prefix.as_deref().unwrap_or(DEFAULT_USERNAME_PREFIX),
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
pub async fn inspect_imported_signer(network: &Stack, mnemonic: &str) -> Result<ImportedSigner> {
	let mnemonic = Mnemonic::parse(mnemonic.trim()).context("invalid BIP-39 mnemonic")?.to_string();
	let identity = identity_from_mnemonic(&mnemonic, network.tld())?;
	let username = attestation::lookup_registered_username(network, &identity.entropy)
		.await
		.with_context(|| {
			format!("look up the mnemonic's optional dotNS username on {}", network.namespace())
		})?;
	let username = match username {
		Some(username) => Some(username),
		None => attestation::lookup_backend_username(network, &identity.entropy, &identity.address)
			.await
			.with_context(|| {
				format!(
					"reverse-resolve the mnemonic's assigned identity-backend username on {}",
					network.namespace()
				)
			})?,
	};
	wait_for_ring_membership(network, &identity.entropy).await.with_context(|| {
		let account = username.as_deref().unwrap_or(&identity.address);
		format!(
			"verify personhood ring membership for account {account:?} on {}",
			network.namespace()
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
	network: &Stack,
	imported: &ImportedSigner,
) -> Result<ResolvedSigner> {
	let network_id = network.namespace();
	let _lock = AccountStoreLock::acquire(base_path)?;
	let mut store = AccountStore::load(base_path)?;
	let public_key_hex = format!("0x{}", hex::encode(imported.public_key));
	if let Some(existing) = store.get(network_id, IMPORTED_ACCOUNT_NAME) &&
		existing.public_key_hex != public_key_hex
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
		.map_or_else(BTreeSet::new, |record| record.exhausted_statement_periods.clone());
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
	network: &Stack,
	account: Option<&str>,
) -> Result<Option<ResolvedSigner>> {
	let network_id = network.namespace();
	let _lock = AccountStoreLock::acquire(base_path)?;
	let store = AccountStore::load(base_path)?;
	let (record, auto_managed) = if let Some(name) = account {
		(store.get(network_id, name).cloned(), false)
	} else {
		let period = current_statement_period()?;
		(store.auto_candidate(network_id, period), true)
	};
	let Some(record) = record.filter(|record| {
		record.attested &&
			(record.origin == AccountOrigin::Imported ||
				resolved_lite_username(&record.lite_username))
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
	network: &Stack,
	username_prefix: &str,
	reserved_username: Option<&str>,
) -> Result<AccountRecord> {
	let lite_username = lite_username_base(username_prefix)?;
	let name = store.next_auto_name(network.namespace());
	let mnemonic = Mnemonic::generate(12).context("generate BIP-39 mnemonic")?.to_string();
	let identity = identity_from_mnemonic(&mnemonic, network.tld())?;

	if !attestation::lite_username_available(network, &identity.entropy, &lite_username)
		.await
		.with_context(|| format!("check lite username {lite_username:?} availability"))?
	{
		bail!("lite username {lite_username:?} is taken; choose a different session name");
	}

	let mut record = AccountRecord {
		name,
		network: network.namespace().to_owned(),
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
	finish_account_readiness(store, record, wait_for_ring_membership(network, &identity.entropy))
		.await
}

async fn ensure_record_ready(
	store: &mut AccountStore,
	network: &Stack,
	record: &AccountRecord,
	reserved_username: Option<&str>,
) -> Result<AccountRecord> {
	let identity = identity_from_mnemonic(&record.mnemonic, network.tld())?;
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
	finish_account_readiness(store, record, wait_for_ring_membership(network, &identity.entropy))
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
	network: &Stack,
	record: &AccountRecord,
	reserved_username: Option<&str>,
) -> Result<String> {
	let entropy = mnemonic_entropy(&record.mnemonic)?;
	let lite_username = attestation::attest(&attest_config(
		network,
		entropy,
		record.lite_username.clone(),
		reserved_username,
	))
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

fn attest_config<'a>(
	network: &'a Stack,
	entropy: Vec<u8>,
	username_base: String,
	reserved_username: Option<&str>,
) -> attestation::AttestConfig<'a> {
	attestation::AttestConfig {
		network,
		entropy,
		username_base,
		reserved_username: reserved_username.map(str::to_string),
	}
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

async fn wait_for_ring_membership(network: &Stack, entropy: &[u8]) -> Result<()> {
	const MAX_ATTEMPTS: usize = 30;
	const SLEEP: Duration = Duration::from_secs(4);

	let chains = crate::chain::CliChainProvider::new(network);
	let candidates = collection_candidates(entropy, network.tld());
	let signer = alloc::FixedPersonhoodSigner::new(&candidates).await?;
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
		let rpc = match chains.rpc_client(network.genesis().people).await {
			Ok(rpc) => alloc::rpc::RpcClient::new(rpc),
			Err(err) => {
				warn!(
					attempt,
					max_attempts = MAX_ATTEMPTS,
					location = chains.location(network.genesis().people),
					error = %err,
					"could not connect while checking personhood ring membership"
				);
				sleep_ring_poll(attempt, MAX_ATTEMPTS, SLEEP).await;
				continue;
			},
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
				},
			}
		}
		let metadata_ref = metadata.as_ref().expect("metadata is initialized");
		// Every ring back to index 0: the signer may sit in an older one.
		match alloc::find_including_rings(
			&rpc,
			metadata_ref,
			&signer,
			&PersonhoodCollection::ALL,
			u32::MAX,
		)
		.await
		{
			Ok(memberships) if !memberships.is_empty() => {
				let held = memberships
					.iter()
					.map(|membership| membership.collection.to_string())
					.collect::<Vec<_>>()
					.join(", ");
				crate::terminal_ui::update_activity(
					"signer",
					"Setting up signer",
					Some(format!("personhood ring membership ready ({held})")),
					crate::terminal_ui::ActivityState::Running,
				);
				return Ok(());
			},
			Ok(_) => {},
			Err(err) => {
				warn!(
					attempt,
					max_attempts = MAX_ATTEMPTS,
					error = %err,
					"could not scan personhood rings"
				);
			},
		}
		sleep_ring_poll(attempt, MAX_ATTEMPTS, SLEEP).await;
	}
	bail!("signer account did not appear in any personhood ring");
}

async fn sleep_ring_poll(attempt: usize, max_attempts: usize, sleep: Duration) {
	if attempt < max_attempts {
		debug!(attempt, max_attempts, "signer account not in any personhood ring yet");
		tokio::time::sleep(sleep).await;
	}
}

fn resolved_from_record(record: AccountRecord, auto_managed: bool) -> Result<ResolvedSigner> {
	let entropy = mnemonic_entropy(&record.mnemonic)?;
	let lite_username = (!record.lite_username.is_empty()).then_some(record.lite_username);
	Ok(ResolvedSigner { entropy, account_name: Some(record.name), lite_username, auto_managed })
}

fn imported_session_name(username: Option<&str>, public_key: &[u8; 32]) -> String {
	username.map_or_else(|| format!("imported-{}", hex::encode(&public_key[..8])), str::to_string)
}

/// The identity [`attest_mnemonic`] registered.
pub struct AttestedIdentity {
	pub address: String,
	pub public_key: [u8; 32],
	pub lite_username: String,
}

/// Attest `mnemonic`'s `peopl.<tld>` identity for `username_base` and wait until
/// it is a personhood ring member. Nothing is written to the account store.
pub async fn attest_mnemonic(
	network: &Stack,
	mnemonic: &str,
	username_base: &str,
) -> Result<AttestedIdentity> {
	let username_base = lite_username_base(username_base)?;
	let identity = identity_from_mnemonic(mnemonic, network.tld())?;
	let lite_username =
		attestation::attest(&attest_config(network, identity.entropy.clone(), username_base, None))
			.await?;
	wait_for_ring_membership(network, &identity.entropy).await?;
	Ok(AttestedIdentity {
		address: identity.address,
		public_key: identity.public_key,
		lite_username,
	})
}

struct SignerIdentity {
	entropy: Vec<u8>,
	public_key: [u8; 32],
	address: String,
}

fn identity_from_mnemonic(mnemonic: &str, network_suffix: &str) -> Result<SignerIdentity> {
	let entropy = mnemonic_entropy(mnemonic)?;
	let candidate =
		derive_identity_keypair(&entropy, network_suffix).map_err(IdentityDerivationError::from)?;
	let public_key = candidate.public.to_bytes();
	Ok(SignerIdentity { entropy, public_key, address: product_public_key_to_address(public_key) })
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
	SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Replaces the file at `path` with `bytes` atomically, readable and
/// writable by its owner only on Unix.
pub fn write_secret_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
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
	let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or(ACCOUNT_STORE_FILE);
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
mod tests;
