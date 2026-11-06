use super::*;
use tempfile::tempdir;

use crate::chain::ChainBackend;

const MNEMONIC: &str =
	"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

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
	let network = crate::network::Network::PaseoNextV2.config(ChainBackend::Rpc);
	let network_id = network.id.as_str();
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
				resolve_cached_signer(directory.path(), &network, None)?
					.map(|signer| signer.entropy),
				serde_json::to_value(reloaded.pending_auto_candidate(network_id))?,
			),
			(original_accounts.clone(), None, serde_json::to_value(&pending)?,)
		);
		Ok(())
	};
	let mut registered = pending.clone();
	registered.lite_username = "pending.01".to_string();

	let mut cancelled =
		Box::pin(finish_account_readiness(&mut store, registered.clone(), std::future::pending()));
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
	let cached = resolve_cached_signer(directory.path(), &network, None)?
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
		(1, true, Some("auto-1".to_string()), vec![0; 16], Some("pending.01".to_string()),)
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
	assert!(!rendered.contains("171"), "entropy bytes leaked: {rendered}");
	assert!(rendered.contains("<redacted>"));
	assert!(rendered.contains("auto-1"), "non-secret fields dropped");

	let config = ResolveSignerConfig {
		base_path: Path::new("/tmp"),
		network: &crate::network::Network::PaseoNextV2.config(ChainBackend::Rpc),
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
	let mut store =
		AccountStore { path: PathBuf::from("accounts.json"), data: AccountStoreData::default() };
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
		assert!(error.contains("lowercase ASCII letters only"), "{prefix:?} reported as {error:?}");
	}
	assert_eq!(lite_username_base("abcdefghijkl").unwrap(), "abcdefghijkl");
}

#[test]
fn auto_candidate_skips_pending_and_exhausted_accounts() {
	let mut store =
		AccountStore { path: PathBuf::from("accounts.json"), data: AccountStoreData::default() };
	let mut exhausted = record("auto-1", "paseo-next-v2", true);
	exhausted.exhausted_statement_periods.insert(7);
	store.upsert(exhausted);
	store.upsert(record("auto-2", "paseo-next-v2", false));
	store.upsert(record("auto-3", "paseo-next-v2", true));

	assert_eq!(
		store.auto_candidate("paseo-next-v2", 7).map(|record| record.name),
		Some("auto-3".to_string())
	);
}

#[test]
fn pending_auto_candidate_reuses_failed_onboarding_record() {
	let mut store =
		AccountStore { path: PathBuf::from("accounts.json"), data: AccountStoreData::default() };
	store.upsert(record("auto-1", "paseo-next-v2", false));
	store.upsert(record("auto-2", "other", false));

	assert_eq!(
		store.pending_auto_candidate("paseo-next-v2").map(|record| record.name),
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
		loaded.get("paseo-next-v2", "auto-1").map(|record| record.name.as_str()),
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

	assert_eq!(remove_managed_accounts(dir.path(), "paseo-next-v2", Some("alicelite.01"))?, 1);
	let loaded = AccountStore::load(dir.path())?;
	assert!(loaded.get("paseo-next-v2", "alice").is_none());
	assert!(loaded.get("paseo-next-v2", "bob").is_some());
	assert!(loaded.get("other", "alice").is_some());

	assert_eq!(remove_managed_accounts(dir.path(), "paseo-next-v2", None)?, 1);
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

	let signer = resolve_cached_signer(
		dir.path(),
		&crate::network::Network::PaseoNextV2.config(ChainBackend::Rpc),
		None,
	)?
	.expect("cached signer");

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

	assert!(
		resolve_cached_signer(
			dir.path(),
			&crate::network::Network::PaseoNextV2.config(ChainBackend::Rpc),
			None
		)?
		.is_none()
	);
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

	let signer = persist_imported_signer(
		dir.path(),
		&crate::network::Network::PaseoNextV2.config(ChainBackend::Rpc),
		&imported,
	)?;

	assert_eq!(signer.account_name.as_deref(), Some(IMPORTED_ACCOUNT_NAME));
	assert_eq!(signer.lite_username.as_deref(), Some("alice.01"));
	assert!(!signer.auto_managed);
	let cached = resolve_cached_signer(
		dir.path(),
		&crate::network::Network::PaseoNextV2.config(ChainBackend::Rpc),
		Some(IMPORTED_ACCOUNT_NAME),
	)?
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
	let signer = persist_imported_signer(
		dir.path(),
		&crate::network::Network::PaseoNextV2.config(ChainBackend::Rpc),
		&imported,
	)?;
	assert_eq!(signer.lite_username, None);
	let cached = resolve_cached_signer(
		dir.path(),
		&crate::network::Network::PaseoNextV2.config(ChainBackend::Rpc),
		Some(IMPORTED_ACCOUNT_NAME),
	)?
	.expect("username-less imported signer is cached");
	assert_eq!(cached.lite_username, None);
	assert!(!cached.auto_managed);
	Ok(())
}
