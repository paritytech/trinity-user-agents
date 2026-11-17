use super::*;
use std::{
	sync::{Arc, Barrier, mpsc},
	time::Duration,
};
use tempfile::tempdir;

#[test]
fn rejects_names_that_could_escape_the_session_root() {
	for invalid in ["", ".", "..", "Alice", "two words", "../escape", "a/b"] {
		assert!(validate_name(invalid).is_err(), "accepted {invalid:?}");
	}
	assert!(validate_name("alice-2.test").is_ok());
}

#[test]
fn default_is_internal_and_not_user_selectable() {
	assert!(validate_name(DEFAULT_SESSION_NAME).is_ok());
	assert!(validate_selectable_name(DEFAULT_SESSION_NAME).is_err());
	assert!(validate_selectable_name("alice").is_ok());
}

#[test]
fn derives_lite_username_prefix_from_session_name() {
	assert_eq!(lite_username_prefix("pgtest").as_deref(), Some("pgtest"));
	assert_eq!(lite_username_prefix("pg-test_2").as_deref(), Some("pgtest"));
	assert_eq!(lite_username_prefix("bob").as_deref(), Some("bob"));
	assert_eq!(lite_username_prefix("foo").as_deref(), Some("foo"));
	assert_eq!(lite_username_prefix("123").as_deref(), Some(""));
	assert_eq!(lite_username_prefix(DEFAULT_SESSION_NAME), None);
}

#[test]
fn persists_and_lists_the_current_network_session() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	catalog.set_current("alice")?;

	assert_eq!(catalog.current_session()?, CurrentSession::Pointed("alice".to_string()));
	assert_eq!(catalog.list()?, vec!["alice"]);
	let profile = catalog.profile("alice")?;
	assert!(profile.path.ends_with("testnet/alice_signing_host"));
	assert!(profile.product_storage_dir.ends_with("testnet/alice_signing_host/storage"));
	assert_eq!(profile.path, profile.account_base_path);
	Ok(())
}

#[test]
fn signer_binding_roundtrips_in_one_session_metadata_record() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice.01")?;

	catalog.store_signer_binding(&profile, "alice.01", "imported")?;

	assert_eq!(catalog.cached_user_id(&profile)?.as_deref(), Some("alice.01"));
	assert_eq!(catalog.cached_account_name(&profile)?.as_deref(), Some("imported"));
	Ok(())
}

#[test]
fn account_binding_does_not_require_a_dotns_username() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("imported-0123456789abcdef")?;

	catalog.store_account_binding(&profile, "imported")?;

	assert_eq!(catalog.cached_user_id(&profile)?, None);
	assert_eq!(catalog.cached_account_name(&profile)?.as_deref(), Some("imported"));
	Ok(())
}

#[test]
fn paired_hosts_are_persisted_in_statement_order_and_preserve_metadata() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice")?;
	catalog.store_signer_binding(&profile, "alice.dot", "imported")?;
	let session_metadata = fs::read(profile.path.join(SESSION_INFO_FILE))?;
	let first = paired_host(1, 11, "first");
	let second = paired_host(2, 22, "second");

	catalog.store_paired_host(&profile, second.clone())?;
	catalog.store_paired_host(&profile, first.clone())?;

	assert_eq!(catalog.paired_hosts(&profile)?, vec![first, second]);
	assert_eq!(
		(catalog.cached_user_id(&profile)?, catalog.cached_account_name(&profile)?,),
		(Some("alice.dot".to_string()), Some("imported".to_string()))
	);
	assert_eq!(fs::read(profile.path.join(SESSION_INFO_FILE))?, session_metadata);
	let metadata: serde_json::Value =
		serde_json::from_slice(&fs::read(profile.path.join(PAIRED_HOSTS_FILE))?)?;
	assert_eq!(metadata["version"], 1);
	assert_eq!(
		metadata["paired_hosts"][0],
		serde_json::json!({
			"version": 1,
			"statement_account_id": vec![1_u8; 32],
			"encryption_public_key": vec![11_u8; 32],
			"host_name": "first",
			"host_version": "1.0.0",
			"host_icon": "https://example.invalid/first.png",
			"platform_type": "web",
			"platform_version": "test",
		})
	);
	Ok(())
}

#[test]
fn repairing_a_host_updates_its_public_key_without_creating_a_duplicate() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice")?;
	let original = paired_host(1, 11, "original");
	let replacement = paired_host(1, 33, "replacement");
	let other = paired_host(2, 22, "other");

	catalog.store_paired_host(&profile, original)?;
	catalog.store_paired_host(&profile, other.clone())?;
	catalog.store_paired_host(&profile, replacement.clone())?;

	assert_eq!(catalog.paired_hosts(&profile)?, vec![replacement, other]);
	Ok(())
}

#[test]
fn removing_a_paired_host_is_exact_and_preserves_session_metadata() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice")?;
	catalog.store_signer_binding(&profile, "alice.dot", "imported")?;
	let session_metadata = fs::read(profile.path.join(SESSION_INFO_FILE))?;
	let first = paired_host(1, 11, "first");
	let second = paired_host(2, 22, "second");
	catalog.store_paired_host(&profile, first)?;
	catalog.store_paired_host(&profile, second.clone())?;

	assert!(catalog.remove_paired_host(&profile, &[1; 32])?);
	assert!(!catalog.remove_paired_host(&profile, &[1; 32])?);

	assert_eq!(catalog.paired_hosts(&profile)?, vec![second]);
	assert_eq!(
		(catalog.cached_user_id(&profile)?, catalog.cached_account_name(&profile)?,),
		(Some("alice.dot".to_string()), Some("imported".to_string()))
	);
	assert_eq!(fs::read(profile.path.join(SESSION_INFO_FILE))?, session_metadata);
	Ok(())
}

#[test]
fn concurrent_upsert_and_removal_preserve_unrelated_hosts() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice")?;
	let removed = paired_host(1, 11, "removed");
	let preserved = paired_host(2, 22, "preserved");
	let inserted = paired_host(3, 33, "inserted");
	catalog.store_paired_host(&profile, removed)?;
	catalog.store_paired_host(&profile, preserved.clone())?;
	let held_lock = PairedHostStoreLock::acquire(&profile.path)?;
	let ready = Arc::new(Barrier::new(3));
	let (done_tx, done_rx) = mpsc::channel();
	let inserted_for_thread = inserted.clone();

	std::thread::scope(|scope| -> Result<()> {
		let insert_catalog = catalog.clone();
		let insert_profile = profile.clone();
		let insert_ready = Arc::clone(&ready);
		let insert_done = done_tx.clone();
		scope.spawn(move || {
			insert_ready.wait();
			let result = insert_catalog.store_paired_host(&insert_profile, inserted_for_thread);
			insert_done.send(result).unwrap();
		});

		let remove_catalog = catalog.clone();
		let remove_profile = profile.clone();
		let remove_ready = Arc::clone(&ready);
		let remove_done = done_tx.clone();
		scope.spawn(move || {
			remove_ready.wait();
			let result = remove_catalog
				.remove_paired_host(&remove_profile, &[1; 32])
				.map(|removed| assert!(removed));
			remove_done.send(result).unwrap();
		});

		drop(done_tx);
		ready.wait();
		assert!(done_rx.recv_timeout(Duration::from_millis(50)).is_err());
		drop(held_lock);
		done_rx.recv()??;
		done_rx.recv()??;
		Ok(())
	})?;

	assert_eq!(catalog.paired_hosts(&profile)?, vec![preserved, inserted]);
	Ok(())
}

#[test]
fn legacy_version_one_metadata_starts_with_no_paired_hosts() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice")?;
	let legacy_metadata = br#"{"version":1,"user_id":"alice.dot","account_name":"imported"}"#;
	fs::write(profile.path.join(SESSION_INFO_FILE), legacy_metadata)?;

	assert_eq!(catalog.paired_hosts(&profile)?, Vec::<PairedHost>::new());
	catalog.store_paired_host(&profile, paired_host(1, 11, "first"))?;

	assert_eq!(
		(catalog.cached_user_id(&profile)?, catalog.cached_account_name(&profile)?,),
		(Some("alice.dot".to_string()), Some("imported".to_string()))
	);
	assert_eq!(fs::read(profile.path.join(SESSION_INFO_FILE))?, legacy_metadata);
	Ok(())
}

#[test]
fn unsupported_paired_host_records_are_not_overwritten() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice")?;
	let path = profile.path.join(PAIRED_HOSTS_FILE);
	let unsupported = serde_json::to_vec_pretty(&serde_json::json!({
		"version": 1,
		"paired_hosts": [{
			"version": 2,
			"statement_account_id": vec![1_u8; 32],
			"encryption_public_key": vec![11_u8; 32]
		}]
	}))?;
	fs::write(&path, &unsupported)?;

	assert!(catalog.paired_hosts(&profile).is_err());
	assert!(catalog.store_paired_host(&profile, paired_host(2, 22, "second")).is_err());
	assert_eq!(fs::read(path)?, unsupported);
	Ok(())
}

fn paired_host(statement_byte: u8, encryption_byte: u8, name: &str) -> PairedHost {
	PairedHost::new(
		[statement_byte; 32],
		[encryption_byte; 32],
		PairedHostMetadata {
			host_name: Some(name.to_string()),
			host_version: Some("1.0.0".to_string()),
			host_icon: Some(format!("https://example.invalid/{name}.png")),
			platform_type: Some("web".to_string()),
			platform_version: Some("test".to_string()),
		},
	)
}

#[test]
fn clears_one_named_session_and_resets_its_current_pointer() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let alice = catalog.ensure_profile("alice")?;
	let bob = catalog.ensure_profile("bob")?;
	fs::write(alice.path.join("state"), "alice")?;
	fs::write(bob.path.join("state"), "bob")?;
	catalog.set_current("alice")?;

	assert_eq!(catalog.clear(&SessionClearTarget::Named("alice".to_string()))?, vec!["alice"]);

	assert!(!alice.path.exists());
	assert!(bob.path.exists());
	assert_eq!(catalog.current_session()?, CurrentSession::Fresh);
	assert_eq!(catalog.list()?, vec!["bob"]);

	// The pointer is cleared, not merely left unresolvable: recreating the
	// name must not silently re-select the session that was just wiped.
	catalog.ensure_profile("alice")?;
	assert_eq!(catalog.current_session()?, CurrentSession::Fresh);
	Ok(())
}

#[test]
fn clearing_all_sessions_removes_default_and_identity_state_only() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let default = catalog.ensure_profile(DEFAULT_SESSION_NAME)?;
	let alice = catalog.ensure_profile("alice")?;
	fs::write(default.path.join("core-storage.json"), "{}")?;
	fs::write(alice.path.join("state"), "alice")?;
	let unrelated = catalog.network_path.join("pairing-host");
	fs::create_dir_all(&unrelated)?;
	fs::write(unrelated.join("state"), "keep")?;

	let cleared = catalog.clear(&SessionClearTarget::All)?;

	assert_eq!(cleared, vec!["alice"]);
	assert!(!catalog.role_path.exists());
	assert!(!alice.path.exists());
	// The other host role on the same network is not session data.
	assert!(unrelated.join("state").is_file());
	assert!(catalog.list()?.is_empty());
	Ok(())
}

#[test]
fn clearing_a_missing_session_has_an_actionable_error() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;

	let error = catalog.clear(&SessionClearTarget::Named("alice".to_string())).unwrap_err();

	assert_eq!(error.to_string(), "session `alice` does not exist; use /session --list");
	Ok(())
}

#[test]
fn promotes_a_provisional_profile_to_the_username_host_root() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let provisional = catalog.ensure_profile("pgtest")?;
	fs::write(provisional.path.join("accounts.json"), "{}")?;
	fs::create_dir_all(&provisional.product_storage_dir)?;
	fs::write(provisional.product_storage_dir.join("product.json"), "{}")?;

	let promoted = catalog.promote_to_user(&provisional, "alice.dot")?;

	assert_eq!(promoted.name, "alice.dot");
	assert!(promoted.path.ends_with("testnet/alice.dot_signing_host"));
	assert!(promoted.path.join("accounts.json").is_file());
	assert!(promoted.product_storage_dir.join("product.json").is_file());
	assert!(!provisional.path.exists());
	Ok(())
}

#[test]
fn default_profile_uses_the_bootstrap_storage_locations() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.profile(DEFAULT_SESSION_NAME)?;

	assert_eq!(profile.account_base_path, temporary.path());
	assert!(profile.path.ends_with("testnet/signing-host"));
	assert!(profile.product_storage_dir.ends_with("testnet/signing-host/storage/default"));
	Ok(())
}

#[test]
fn promoting_the_default_profile_preserves_paired_hosts() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let default_profile = catalog.ensure_profile(DEFAULT_SESSION_NAME)?;
	let host = paired_host(1, 11, "first");
	catalog.store_paired_host(&default_profile, host.clone())?;

	let promoted = catalog.promote_to_user(&default_profile, "alice.dot")?;

	assert_eq!(catalog.paired_hosts(&promoted)?, vec![host]);
	assert!(!default_profile.path.join(PAIRED_HOSTS_FILE).exists());
	Ok(())
}

#[test]
fn session_metadata_preserves_user_id_and_last_script() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice")?;
	let scripts = profile.path.join("scripts");
	fs::create_dir_all(&scripts)?;
	let script = scripts.join("script.ts");
	fs::write(&script, "console.log('test');")?;

	assert_eq!(catalog.cached_user_id(&profile)?, None);
	assert_eq!(catalog.last_script(&profile)?, None);
	store_session_last_script(&profile.path, &script)?;
	catalog.store_user_id(&profile, "alice.dot")?;
	let replacement = scripts.join("replacement.ts");
	fs::write(&replacement, "console.log('replacement');")?;
	store_session_last_script(&profile.path, &replacement)?;

	assert_eq!(catalog.cached_user_id(&profile)?.as_deref(), Some("alice.dot"));
	assert_eq!(catalog.last_script(&profile)?.as_deref(), Some(replacement.as_path()));
	let metadata = fs::read_to_string(profile.path.join(SESSION_INFO_FILE))?;
	assert!(metadata.contains("\"user_id\": \"alice.dot\""));
	assert!(metadata.contains("\"last_script\": \"replacement.ts\""));
	assert!(profile.path.join(SESSION_INFO_FILE).is_file());
	Ok(())
}

#[test]
fn stale_or_escaping_last_script_is_never_opened() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice")?;
	let scripts = profile.path.join("scripts");
	fs::create_dir_all(&scripts)?;
	let stale = scripts.join("missing.ts");
	store_session_last_script(&profile.path, &stale)?;

	assert_eq!(catalog.last_script(&profile)?, None);
	fs::write(
		profile.path.join(SESSION_INFO_FILE),
		r#"{"version":1,"last_script":"../outside.ts"}"#,
	)?;
	assert!(catalog.last_script(&profile).is_err());
	Ok(())
}

#[test]
fn session_metadata_preserves_an_explicit_script_outside_scratch_storage() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice")?;
	let script = temporary.path().join("product-script.ts");
	fs::write(&script, "console.log('product');")?;

	store_session_last_script(&profile.path, &script)?;

	assert_eq!(catalog.last_script(&profile)?.as_deref(), Some(script.as_path()));
	let metadata = fs::read_to_string(profile.path.join(SESSION_INFO_FILE))?;
	assert!(metadata.contains(script.to_str().context("temporary path is not UTF-8")?));
	Ok(())
}

#[test]
fn a_lost_pointer_recovers_the_only_provisioned_session() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let alice = catalog.ensure_profile("alice.01")?;
	fs::write(alice.path.join("accounts.json"), "{}")?;
	catalog.set_current("alice.01")?;
	fs::remove_file(catalog.role_path.join(CURRENT_SESSION_FILE))?;

	assert_eq!(catalog.current_session()?, CurrentSession::Recovered("alice.01".to_string()));
	Ok(())
}

#[test]
fn a_pointer_naming_a_removed_session_recovers_the_remaining_one() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let alice = catalog.ensure_profile("alice.01")?;
	fs::write(alice.path.join("accounts.json"), "{}")?;
	catalog.set_current("alice.01")?;
	fs::write(catalog.role_path.join(CURRENT_SESSION_FILE), "gone.99\n")?;

	assert_eq!(catalog.current_session()?, CurrentSession::Recovered("alice.01".to_string()));
	Ok(())
}

#[test]
fn a_lost_pointer_with_several_provisioned_sessions_is_ambiguous() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	for name in ["alice.01", "bob.02"] {
		let profile = catalog.ensure_profile(name)?;
		fs::write(profile.path.join("accounts.json"), "{}")?;
	}

	assert_eq!(
		catalog.current_session()?,
		CurrentSession::Ambiguous {
			candidates: vec!["alice.01".to_string(), "bob.02".to_string()],
		}
	);
	Ok(())
}

#[test]
fn an_unprovisioned_session_directory_is_not_recovered() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	catalog.ensure_profile("alice.01")?;

	assert_eq!(catalog.current_session()?, CurrentSession::Fresh);
	assert_eq!(catalog.current_session()?, CurrentSession::Fresh);
	Ok(())
}

#[test]
fn a_username_prefix_selects_creation_order_instead_of_the_numerical_suffix() -> Result<()> {
	for (first, second) in [("workbench.42", "workbench.73"), ("workbench.73", "workbench.42")] {
		let temporary = tempdir()?;
		let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
		saved_session(&catalog, first, None, 100)?;
		saved_session(&catalog, second, None, 200)?;
		saved_session(&catalog, "workbenchextra.99", None, 300)?;
		catalog.ensure_profile("workbench")?;

		assert_eq!(
			(
				catalog.resolve_session_name("workbench")?,
				catalog.resolve_session_name(first)?,
				catalog.resolve_session_name(second)?,
			),
			(second.to_string(), first.to_string(), second.to_string())
		);
	}
	Ok(())
}

#[test]
fn a_broken_binding_cannot_silently_select_an_older_identity() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	saved_session(&catalog, "workbench.42", None, 100)?;
	let broken = saved_session(&catalog, "workbench.73", None, 200)?;
	catalog.store_signer_binding(&broken, "workbench.73", "missing")?;

	for remove_store in [false, true] {
		if remove_store {
			fs::remove_file(broken.account_base_path.join("accounts.json"))?;
		}
		assert_eq!(
			catalog.resolve_session_name("workbench").unwrap_err().to_string(),
			"account \"missing\" not found for testnet"
		);
	}
	Ok(())
}

#[test]
fn equal_creation_times_require_an_exact_session_name() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	saved_session(&catalog, "workbench.42", None, 100)?;
	saved_session(&catalog, "workbench.73", None, 100)?;

	assert_eq!(
		catalog.resolve_session_name("workbench").unwrap_err().to_string(),
		"sessions workbench.42, workbench.73 have the same creation time; select an exact session name"
	);
	assert_eq!(catalog.resolve_session_name("workbench.42")?, "workbench.42");
	Ok(())
}

fn saved_session(
	catalog: &SessionCatalog,
	name: &str,
	created_as: Option<&str>,
	created_at: u64,
) -> Result<SessionProfile> {
	let profile = catalog.ensure_profile(name)?;
	fs::write(
		profile.account_base_path.join("accounts.json"),
		serde_json::to_vec(&serde_json::json!({
			"version": 1,
			"accounts": [{
				"name": "auto-1",
				"network": catalog.network_id,
				"mnemonic": "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
				"lite_username": name,
				"public_key_hex": "0x00",
				"address": "test",
				"created_at_unix": created_at,
				"attested": true
			}]
		}))?,
	)?;
	write_session_info(
		&profile.path,
		&SessionInfo {
			user_id: Some(name.to_string()),
			account_name: Some("auto-1".to_string()),
			created_as: created_as.map(str::to_string),
			..SessionInfo::default()
		},
	)?;
	Ok(profile)
}

#[test]
fn a_promoted_session_still_resolves_under_the_name_that_created_it() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let provisional = catalog.ensure_profile("worker-0")?;

	let promoted = catalog.promote_to_user(&provisional, "alice.01")?;

	assert_eq!(promoted.name, "alice.01");
	assert_eq!(catalog.resolve_session_name("worker-0")?, "alice.01");
	Ok(())
}

#[test]
fn clearing_by_the_name_that_created_a_session_clears_the_promoted_one() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let provisional = catalog.ensure_profile("worker-0")?;
	let promoted = catalog.promote_to_user(&provisional, "alice.01")?;
	fs::write(promoted.path.join("accounts.json"), r#"{"version":1,"accounts":[]}"#)?;

	let cleared = catalog.clear(&SessionClearTarget::Named("worker-0".to_string()))?;

	assert_eq!(cleared, vec!["alice.01"]);
	assert!(!promoted.path.exists());
	Ok(())
}

#[test]
fn an_unpromoted_name_resolves_to_itself() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;

	assert_eq!(catalog.resolve_session_name("worker-0")?, "worker-0");
	Ok(())
}

#[test]
fn an_alias_whose_target_was_cleared_resolves_to_itself_again() -> Result<()> {
	let temporary = tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let provisional = catalog.ensure_profile("worker-0")?;
	catalog.promote_to_user(&provisional, "alice.01")?;
	catalog.clear(&SessionClearTarget::Named("alice.01".to_string()))?;

	assert_eq!(catalog.resolve_session_name("worker-0")?, "worker-0");
	Ok(())
}
