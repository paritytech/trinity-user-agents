mod profile_tests;

use super::*;
use crate::frame_server::ProductRuntimeFactory;
use parity_scale_codec::Encode;
use truapi::{DebugEvent, FrameSink, ProductContext, ProductRuntime};

const SESSION_MNEMONIC: &str =
	"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

fn root_public_key(entropy: &RootEntropy) -> [u8; 32] {
	entropy.derive_root_keypair().unwrap().public.to_bytes()
}

async fn unfinished_session(base_path: &Path, name: &str) -> Result<SigningHostSession> {
	let network = Network::default().bundled_stack(ChainBackend::Rpc);
	let catalog = SessionCatalog::new(base_path.to_path_buf(), network.state_directory())?;
	catalog.set_current(name)?;
	start_signing_host(&SigningHostArgs::default(), catalog, name.to_string(), network, None).await
}

fn finish_session_account(session: &SigningHostSession) -> Result<Vec<u8>> {
	let profile = session.profile.as_ref().unwrap();
	let accounts = serde_json::to_vec(&serde_json::json!({
		"version": 1,
		"accounts": [{
			"name": "auto-1",
			"network": session.network.state_directory(),
			"mnemonic": SESSION_MNEMONIC,
			"lite_username": "pending.01",
			"public_key_hex": "0x00",
			"address": "test",
			"created_at_unix": 1,
			"attested": true
		}]
	}))?;
	std::fs::write(profile.account_base_path.join("accounts.json"), &accounts)?;
	Ok(accounts)
}

#[tokio::test]
async fn session_selection_retries_the_active_unfinished_profile() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let mut session = unfinished_session(temporary.path(), "pending").await?;
	session.network = session.network.with_dub("http://127.0.0.1:1/");
	let runtime = session.runtime.clone();

	let error = switch_session(&mut session, "pending".to_string())
		.await
		.expect_err("an unfinished active session must attempt provisioning");

	assert_eq!(
		(
			error.to_string(),
			Arc::ptr_eq(&runtime, &session.runtime),
			session.catalog.current_session()?,
			session.runtime.has_active_session(),
		),
		(
			"check lite username \"pending\" availability".to_string(),
			true,
			CurrentSession::Pointed("pending".to_string()),
			false,
		)
	);
	Ok(())
}

/// A clear may name the running session by the name it was created under
/// before its promotion. Whether the clear includes the running session is
/// decided from the name it resolves to, so the host warns and stops before
/// that session's data goes, and its messages name the session it clears.
#[tokio::test]
async fn a_clear_by_the_name_the_running_session_was_created_under_includes_it() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let network = Network::default().bundled_stack(ChainBackend::Rpc);
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), network.state_directory())?;
	let provisional = catalog.ensure_profile("worker-0")?;
	let promoted = catalog.promote_to_user(&provisional, "alice.01")?;
	std::fs::write(promoted.path.join("accounts.json"), r#"{"version":1,"accounts":[]}"#)?;
	let session = unfinished_session(temporary.path(), "alice.01").await?;

	assert_eq!(
		validate_session_clear(&session, &SessionClearTarget::Named("worker-0".to_string()))?,
		(SessionClearTarget::Named("alice.01".to_string()), true)
	);
	Ok(())
}

#[tokio::test]
async fn session_selection_activates_a_finished_account_without_reprovisioning() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let mut session = unfinished_session(temporary.path(), "pending").await?;
	let accounts = finish_session_account(&session)?;
	session.network = session.network.with_dub("http://127.0.0.1:1/");
	let mut resets = session.runtime_factory.connection_reset().unwrap();
	let expected_key = root_public_key(&RootEntropy::new([0; 16]));

	switch_session(&mut session, "pending".to_string()).await?;

	assert_eq!(
		(
			session.runtime.has_active_session(),
			session.runtime.statement_renewal_owner_key().ok(),
			session.profile.as_ref().map(|profile| profile.name.as_str()),
			session.catalog.current_session()?,
			*resets.borrow_and_update(),
			std::fs::read(
				session.profile.as_ref().unwrap().account_base_path.join("accounts.json")
			)?,
		),
		(
			true,
			Some(expected_key),
			Some("pending.01"),
			CurrentSession::Pointed("pending.01".to_string()),
			1,
			accounts,
		)
	);
	let runtime = session.runtime.clone();
	switch_session(&mut session, "pending".to_string()).await?;
	assert_eq!((Arc::ptr_eq(&runtime, &session.runtime), resets.has_changed()?), (true, false));
	Ok(())
}

#[tokio::test]
async fn session_selection_preserves_the_active_profile_when_promotion_cannot_commit() -> Result<()>
{
	for (name, slash_command) in
		[(DEFAULT_SESSION_NAME, false), ("pending", true), ("pending", false)]
	{
		let temporary = tempfile::tempdir()?;
		let mut session = unfinished_session(temporary.path(), name).await?;
		let profile = session.profile.clone().unwrap();
		let accounts = finish_session_account(&session)?;
		let contents = [
			(
				profile.path.join("session.json"),
				br#"{"version":1,"last_script":"saved.ts"}"#.to_vec(),
			),
			(profile.path.join("core-storage.json"), br#"{"values":{}}"#.to_vec()),
			(profile.path.join("scripts/saved.ts"), b"console.log('saved');".to_vec()),
			(profile.product_storage_dir.join("saved"), b"product data".to_vec()),
		];
		for (path, bytes) in &contents {
			std::fs::create_dir_all(path.parent().unwrap())?;
			std::fs::write(path, bytes)?;
		}
		let role = session.catalog.profile(DEFAULT_SESSION_NAME)?;
		let blocked_pointer =
			role.path.join(format!(".current-session.{}.tmp", std::process::id()));
		std::fs::create_dir(&blocked_pointer)?;
		let runtime = session.runtime.clone();
		let resets = session.runtime_factory.connection_reset().unwrap();

		let result = if slash_command {
			switch_session(&mut session, "pending".to_string()).await
		} else {
			ensure_signer(&mut session).await
		};
		let error = result.expect_err("failed persistence must leave the active profile usable");

		assert!(error.to_string().contains("write current session"), "{error:#}");
		assert_eq!(
			(
				Arc::ptr_eq(&runtime, &session.runtime),
				session.profile.as_ref(),
				session.catalog.current_session()?,
				resets.has_changed()?,
				std::fs::read(profile.account_base_path.join("accounts.json"))?,
			),
			(
				true,
				Some(&profile),
				CurrentSession::Pointed(name.to_string()),
				false,
				accounts.clone(),
			)
		);
		for (path, bytes) in &contents {
			assert_eq!(
				std::fs::read(path).with_context(|| format!("read restored {}", path.display()))?,
				*bytes,
				"{}",
				path.display()
			);
		}

		std::fs::remove_dir(blocked_pointer)?;
		if slash_command {
			switch_session(&mut session, "pending".to_string()).await?;
		} else {
			ensure_signer(&mut session).await?;
		}
		assert_eq!(
			(
				session.runtime.has_active_session(),
				session.runtime.statement_renewal_owner_key().ok(),
				std::fs::read(
					session.profile.as_ref().unwrap().account_base_path.join("accounts.json")
				)?,
			),
			(true, Some(root_public_key(&RootEntropy::new([0; 16]))), accounts)
		);
	}
	Ok(())
}

#[tokio::test]
async fn session_selection_failure_keeps_the_working_identity_and_responders() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let mut session = unfinished_session(temporary.path(), "pending").await?;
	finish_session_account(&session)?;
	switch_session(&mut session, "pending".to_string()).await?;
	let runtime = session.runtime.clone();
	let owner = runtime.statement_renewal_owner_key().unwrap();
	let resets = session.runtime_factory.connection_reset().unwrap();
	let responder = tokio::spawn(std::future::pending::<()>());
	let responder_abort = responder.abort_handle();
	session.responders.insert([7; 32], responder);
	session.network = session.network.with_dub("http://127.0.0.1:1/");

	session.catalog.ensure_profile("workbench.99")?;
	for name in ["foo", "carol", "workbench.99", "another"] {
		let expected_error = match name {
			"foo" | "carol" => {
				format!(
					"session name {name:?} must contain at least 6 lowercase ASCII letters to create an account; digits and separators do not count"
				)
			},
			"workbench.99" => {
				format!(
					"session {name:?} has no saved account; choose a username base to create one"
				)
			},
			_ => format!("check lite username {name:?} availability"),
		};
		let error = switch_session(&mut session, name.to_string())
			.await
			.expect_err("failed selection must preserve the active signer");

		assert_eq!(
			(
				error.to_string(),
				Arc::ptr_eq(&runtime, &session.runtime),
				session.runtime.statement_renewal_owner_key().ok(),
				session.catalog.current_session()?,
				resets.has_changed()?,
				session.responders.tasks.keys().copied().collect::<Vec<_>>(),
				responder_abort.is_finished(),
				session.catalog.exists(name),
			),
			(
				expected_error,
				true,
				Some(owner),
				CurrentSession::Pointed("pending.01".to_string()),
				false,
				vec![[7; 32]],
				false,
				matches!(name, "workbench.99" | "another"),
			)
		);
	}
	Ok(())
}

/// Without a local session every restart fails the same way, so `/pair`
/// has to end with the failure instead of restarting forever.
#[tokio::test]
async fn a_responder_without_a_local_session_stops_instead_of_restarting() {
	let profile = tempfile::tempdir().unwrap();
	let (runtime, _platform) = build_signing_runtime(
		&crate::network::Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc),
		profile.path().to_path_buf(),
		profile.path().join("storage"),
		ApprovalPolicy::AutoAccept,
		None,
		None,
		None,
	)
	.unwrap();
	let host = PairedHost::new([0x31; 32], [0x42; 32], PairedHostMetadata::default());

	let served =
		tokio::time::timeout(Duration::from_secs(5), supervise_responder(runtime, host, None))
			.await
			.expect("the responder stops instead of restarting");

	assert!(matches!(served, Err(ResumePairingError::CannotServe { .. })), "{served:?}");
}

#[tokio::test]
async fn the_signing_runtime_keeps_its_core_database_in_the_profile_directory() {
	// Each profile keeps its own durable state, so switching users never
	// mixes two ledgers.
	let profile = tempfile::tempdir().unwrap();
	let (runtime, _platform) = build_signing_runtime(
		&crate::network::Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc),
		profile.path().to_path_buf(),
		profile.path().join("storage"),
		ApprovalPolicy::AutoAccept,
		None,
		None,
		None,
	)
	.unwrap();

	let status = runtime.core_database_status().await.unwrap();

	let file = profile.path().canonicalize().unwrap().join(truapi::store::CORE_DB_FILE);
	assert_eq!(
		status.path,
		Some(file.to_string_lossy().into_owned()),
		"the core database lives in the profile directory"
	);
}

#[tokio::test]
async fn a_core_database_this_build_cannot_open_asks_for_cleanup() {
	// Running an older build on a session a newer build migrated must tell
	// the user how to restart cleanly instead of only reporting the failure.
	let root = tempfile::tempdir().unwrap();
	let profile = root.path().join("my session");
	std::fs::create_dir_all(&profile).unwrap();
	let database = profile.join(truapi::store::CORE_DB_FILE);
	rusqlite::Connection::open(&database)
		.unwrap()
		.pragma_update(None, "user_version", 1000)
		.unwrap();
	let build = || {
		build_signing_runtime(
			&crate::network::Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc),
			profile.clone(),
			profile.join("storage"),
			ApprovalPolicy::AutoAccept,
			None,
			None,
			None,
		)
	};

	let error = build().err().expect("a newer schema is rejected");

	let database = database.display();
	let cleanup = format!("rm -f '{database}' '{database}-wal' '{database}-shm'");
	assert_eq!(
		error.to_string(),
		format!(
			"cannot open the core database {database}. A newer truapi-host may have \
			 written it, or it is damaged. Run a newer truapi-host, or delete it to \
			 start clean; the session keeps its accounts and pairings:\n  {cleanup}"
		)
	);
	let status = std::process::Command::new("sh").arg("-c").arg(&cleanup).status().unwrap();
	assert!(status.success());
	build().expect("the session starts cleanly after cleanup");
}

#[test]
fn pairing_deeplink_becomes_a_public_persistable_host_record() {
	use truapi::host_logic::sso::pairing::{
		VersionedHandshakeProposal,
		v2::{Device, MetadataEntry, MetadataKey, Proposal},
	};

	let proposal = VersionedHandshakeProposal::V2(Proposal {
		device: Device { statement_account_id: [1; 32], encryption_public_key: [2; 32] },
		metadata: vec![
			MetadataEntry(
				MetadataKey::HostName,
				" Desk\u{202e}top\u{2066}\nHost\u{061c} ".to_string(),
			),
			MetadataEntry(MetadataKey::HostVersion, "1.2.3".to_string()),
		],
	});
	let deeplink = format!("polkadotapp://pair?handshake={}", hex::encode(proposal.encode()));

	assert_eq!(
		paired_host_from_deeplink(&deeplink).unwrap(),
		PairedHost::new(
			[1; 32],
			[2; 32],
			PairedHostMetadata {
				host_name: Some("DesktopHost".to_string()),
				host_version: Some("1.2.3".to_string()),
				..PairedHostMetadata::default()
			}
		)
	);
}

#[tokio::test]
async fn responder_manager_keeps_unrelated_peers_and_replaces_only_the_same_peer() {
	let mut manager = ResponderManager::default();
	let first = tokio::spawn(std::future::pending::<()>());
	let first_abort = first.abort_handle();
	let second = tokio::spawn(std::future::pending::<()>());
	let second_abort = second.abort_handle();
	manager.insert([1; 32], first);
	manager.insert([2; 32], second);

	let replacement = tokio::spawn(std::future::pending::<()>());
	manager.insert([1; 32], replacement);
	tokio::task::yield_now().await;

	assert_eq!(
		(
			manager.tasks.keys().copied().collect::<std::collections::HashSet<_>>(),
			first_abort.is_finished(),
			second_abort.is_finished(),
		),
		([[1; 32], [2; 32]].into_iter().collect(), true, false,)
	);
}

/// The light client drops a PGAS claim, so a submit there would only wait
/// out its timeout. It is refused before a key is derived or a chain dialed.
#[tokio::test]
async fn pgas_check_refuses_to_submit_on_the_light_client() {
	let network = Network::default().bundled_stack(ChainBackend::LightClient);

	let error = run_pgas_check(String::new(), network, Some("00".repeat(32)), 8, true)
		.await
		.expect_err("a submit on the light client is refused");

	assert_eq!(
		error.to_string(),
		"--submit needs --chain-backend rpc: the light client drops a PGAS claim, because \
		 smoldot refuses the proof Asset Hub's nodes return for its validation"
	);
}

#[tokio::test]
async fn responder_manager_removes_only_the_selected_peer() {
	let mut manager = ResponderManager::default();
	let first = tokio::spawn(std::future::pending::<()>());
	let first_abort = first.abort_handle();
	let second = tokio::spawn(std::future::pending::<()>());
	let second_abort = second.abort_handle();
	manager.insert([1; 32], first);
	manager.insert([2; 32], second);

	assert!(manager.remove(&[1; 32]));
	tokio::task::yield_now().await;

	assert_eq!(
		(
			manager.tasks.keys().copied().collect::<Vec<_>>(),
			first_abort.is_finished(),
			second_abort.is_finished(),
		),
		(vec![[2; 32]], true, false)
	);
}

#[test]
fn paired_device_list_is_sorted_and_includes_available_display_metadata() {
	let named = PairedHost::new(
		[2; 32],
		[22; 32],
		PairedHostMetadata {
			host_name: Some("Desktop".to_string()),
			host_version: Some("1.2.3".to_string()),
			host_icon: None,
			platform_type: Some("macOS".to_string()),
			platform_version: Some("26.1".to_string()),
		},
	);
	let unknown = PairedHost::new([1; 32], [11; 32], PairedHostMetadata::default());

	assert_eq!(
		format_paired_device_list("alice.01", vec![named, unknown]),
		format!(
			"Paired devices for session alice.01\n0x{}  <unknown device>\n0x{}  Desktop · 1.2.3 · macOS 26.1",
			hex::encode([1; 32]),
			hex::encode([2; 32])
		)
	);
}

#[test]
fn paired_device_list_reports_an_empty_session() {
	assert_eq!(
		format_paired_device_list("alice.01", Vec::new()),
		"No paired devices for session alice.01"
	);
}

#[test]
fn a_session_name_promoted_away_still_selects_its_own_session() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let provisional = catalog.ensure_profile("worker-0")?;
	catalog.promote_to_user(&provisional, "alice.01")?;
	let args =
		SigningHostArgs { session: Some("worker-0".to_string()), ..SigningHostArgs::default() };

	assert_eq!(initial_session_name(&args, &catalog)?, "alice.01");
	Ok(())
}

#[test]
fn a_base_path_with_several_provisioned_sessions_and_no_pointer_refuses_to_guess() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	for name in ["alice.01", "bob.02"] {
		let profile = catalog.ensure_profile(name)?;
		std::fs::write(profile.path.join("accounts.json"), "{}")?;
	}

	let error = initial_session_name(&SigningHostArgs::default(), &catalog)
		.expect_err("an ambiguous base path must not select an identity");

	let message = error.to_string();
	assert!(message.contains("alice.01"), "{message}");
	assert!(message.contains("bob.02"), "{message}");
	assert!(message.contains("--session"), "{message}");
	Ok(())
}

#[test]
fn a_lost_pointer_reselects_the_provisioned_session_instead_of_the_bootstrap_profile() -> Result<()>
{
	let temporary = tempfile::tempdir()?;
	let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
	let profile = catalog.ensure_profile("alice.01")?;
	std::fs::write(profile.path.join("accounts.json"), "{}")?;

	assert_eq!(initial_session_name(&SigningHostArgs::default(), &catalog)?, "alice.01");
	Ok(())
}

#[test]
fn allowance_exhaustion_is_recognized_through_operation_context() {
	for (reason, expected) in [
		("no free StatementStore slot in period 42 (max 3)", true),
		("People chain is unavailable", false),
	] {
		let error = anyhow::anyhow!(reason).context("pairing allowance renewal failed");
		assert_eq!(is_statement_slot_exhaustion(&error), expected);
	}
}

#[test]
fn remote_pairing_failures_preserve_exhaustion_without_local_details() {
	let errors = [
		anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
			.context("write paired hosts /home/alice/private/paired-hosts.json"),
		anyhow::anyhow!("no free StatementStore slot in period 42 (max 3)")
			.context("pairing allowance renewal failed"),
	];

	assert_eq!(
		errors.each_ref().map(pairing_failure_reason),
		[
			"Pairing failed. Check the signing host for details.",
			"No free StatementStore slot. Remove a paired device or wait for a new allowance period.",
		]
	);
}

#[test]
fn an_auto_managed_signer_never_rotates_while_a_paired_host_depends_on_it() {
	assert!(signer_identity_may_rotate(true, 0));
	assert!(!signer_identity_may_rotate(true, 1));
	assert!(!signer_identity_may_rotate(true, 2));
	assert!(!signer_identity_may_rotate(false, 0));
}

#[test]
fn trace_log_level_is_available_before_or_after_the_subcommand() {
	let before = Cli::try_parse_from(["truapi-host", "--log-level", "trace", "signing-host"])
		.expect("global log level before subcommand should parse");
	let after = Cli::try_parse_from(["truapi-host", "signing-host", "--log-level", "trace"])
		.expect("global log level after subcommand should parse");

	assert_eq!(before.log_level, Some(LogLevel::Trace));
	assert_eq!(after.log_level, Some(LogLevel::Trace));
	assert_eq!(LogLevel::Trace.scoped_filter(), "warn,truapi=trace,truapi_host=trace");
}

#[test]
fn noisy_transport_targets_are_always_excluded_from_cli_logs() {
	assert!(!log_target_is_visible(terminal_ui::SSO_TRANSCRIPT_TARGET));
	assert!(!log_target_is_visible("rustls"));
	assert!(!log_target_is_visible("rustls::client::tls13"));
	assert!(!log_target_is_visible("tungstenite::protocol"));
	assert!(!log_target_is_visible("tungstenite::protocol::frame::socket"));
	assert!(log_target_is_visible("tungstenite::handshake"));
	assert!(log_target_is_visible("truapi::runtime"));
}

#[test]
fn signing_host_exec_accepts_one_slash_command() {
	let cli =
		Cli::try_parse_from(["truapi-host", "signing-host", "--auto-accept", "exec", "/help"])
			.expect("exec slash command should parse");
	let Command::SigningHost(args) = cli.command else {
		panic!("expected signing-host command");
	};
	let Some(SigningHostAction::Exec { command }) = args.action else {
		panic!("expected exec action");
	};
	assert_eq!(command, "/help");
	assert_eq!(args.frame_listen, None);
}

/// The product id and the frame port are the two values the host and the
/// product must agree on, and `dev` derives both so they cannot drift.
#[test]
fn dev_derives_the_product_id_from_the_app_port_and_wraps_a_command() {
	let cli = Cli::try_parse_from([
		"truapi-host",
		"dev",
		"--app-port",
		"5173",
		"--",
		"pnpm",
		"dev",
		"--host",
	])
	.expect("dev should parse a wrapped command");
	let Command::Dev(args) = cli.command else {
		panic!("expected dev command");
	};

	assert_eq!(args.app_port, 5173);
	assert_eq!(args.port, DEFAULT_DEV_PORT);
	assert_eq!(args.product_id, None);
	assert_eq!(args.command, ["pnpm", "dev", "--host"]);
}

#[test]
fn dev_runs_a_bare_host_when_no_command_is_wrapped() {
	let cli = Cli::try_parse_from(["truapi-host", "dev", "--product-id", "playground.paseo"])
		.expect("dev should parse without a wrapped command");
	let Command::Dev(args) = cli.command else {
		panic!("expected dev command");
	};

	assert_eq!(args.product_id.as_ref().map(ProductId::as_str), Some("playground.paseo"));
	assert_eq!(args.app_port, 3000);
	assert!(args.command.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn script_completion_removes_the_private_frame_socket_before_returning() -> Result<()> {
	struct UnusedRuntimeFactory;

	impl frame_server::ProductRuntimeFactory for UnusedRuntimeFactory {
		fn product_runtime(
			&self,
			_product: truapi::ProductContext,
			_sink: Arc<dyn truapi::FrameSink>,
		) -> truapi::ProductRuntime {
			panic!("the completed script must not open a product connection")
		}
	}

	for script_failed in [false, true] {
		let frame_server = frame_server::bind(None).await?;
		let socket = PathBuf::from(
			frame_server
				.endpoint()
				.strip_prefix("ws+unix:")
				.context("expected a private Unix socket")?,
		);
		let directory = socket.parent().context("socket has no directory")?;
		assert!(socket.exists());
		let product = frame_server::ProductSelection::new(
			ProductId::for_test("script-cleanup.testnet"),
			ExecutableKind::App,
		);
		let result =
			with_frame_server(Arc::new(UnusedRuntimeFactory), product, frame_server, async move {
				anyhow::ensure!(!script_failed, "script failed");
				Ok(())
			})
			.await;

		assert_eq!(
			(result.is_err(), socket.exists(), directory.exists()),
			(script_failed, false, false),
		);
	}
	Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn dev_force_kills_a_term_ignoring_descendant_after_its_launcher_exits() -> Result<()> {
	const READY_PATH_ENV: &str = "TRUAPI_DEV_COMMAND_TEST_READY_PATH";

	if let Some(ready_path) = std::env::var_os(READY_PATH_ENV) {
		let _terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
		let _listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
		std::fs::write(ready_path, format!("{} {}", _listener.local_addr()?, std::process::id()))?;
		return std::future::pending().await;
	}

	let temporary = tempfile::tempdir()?;
	let ready_path = temporary.path().join("descendant-ready");
	let executable = std::env::current_exe()?.to_string_lossy().into_owned();
	let launcher = r#"
"$1" --exact tests::dev_force_kills_a_term_ignoring_descendant_after_its_launcher_exits --nocapture &
attempts=0
while [ ! -s "$TRUAPI_DEV_COMMAND_TEST_READY_PATH" ] && [ "$attempts" -lt 500 ]; do
    attempts=$((attempts + 1))
    sleep 0.01
done
test -s "$TRUAPI_DEV_COMMAND_TEST_READY_PATH"
"#;
	let started = std::time::Instant::now();
	let exit_code = run_dev_command(vec![
		"env".to_string(),
		format!("{READY_PATH_ENV}={}", ready_path.display()),
		"sh".to_string(),
		"-c".to_string(),
		launcher.to_string(),
		"sh".to_string(),
		executable,
	])
	.await?;
	let ready = std::fs::read_to_string(&ready_path)?;
	let (address, pid) =
		ready.split_once(' ').context("descendant readiness record is incomplete")?;
	let address = address.parse::<SocketAddr>()?;
	let port_was_released = std::net::TcpListener::bind(address).is_ok();
	if !port_was_released {
		let pid = pid
			.trim()
			.parse::<i32>()
			.ok()
			.and_then(rustix::process::Pid::from_raw)
			.context("descendant pid is invalid")?;
		let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
	}

	assert_eq!(
		(exit_code, port_was_released, started.elapsed() >= DEV_COMMAND_GRACE,),
		(0, true, true)
	);
	Ok(())
}

#[test]
fn frame_listen_explicitly_selects_tcp_websockets() {
	let cli = Cli::try_parse_from(["truapi-host", "pairing-host", "--frame-listen", "127.0.0.1:0"])
		.expect("explicit TCP frame listener should parse");
	let Command::PairingHost(args) = cli.command else {
		panic!("expected pairing-host command");
	};

	assert_eq!(args.frame_listen, Some("127.0.0.1:0".parse().expect("valid socket address")));
}

#[test]
fn signing_host_rejects_script_and_exec_together() {
	let cli = Cli::try_parse_from([
		"truapi-host",
		"signing-host",
		"--script",
		"smoke.ts",
		"exec",
		"/help",
	])
	.expect("clap should parse parent options before exec");
	let Command::SigningHost(args) = cli.command else {
		panic!("expected signing-host command");
	};
	assert!(
		validate_signing_args(&args)
			.unwrap_err()
			.to_string()
			.contains("--script cannot be combined")
	);
}

#[test]
fn signing_host_serve_needs_no_tty_and_refuses_one_shot_modes() {
	let cli = Cli::try_parse_from([
		"truapi-host",
		"signing-host",
		"--serve",
		"--frame-listen",
		"127.0.0.1:9955",
		"--auto-accept",
	])
	.expect("serve should parse");
	let Command::SigningHost(args) = cli.command else {
		panic!("expected signing-host command");
	};
	assert!(args.serve);
	assert!(validate_signing_args(&args).is_ok());

	let with_script =
		Cli::try_parse_from(["truapi-host", "signing-host", "--serve", "--script", "smoke.ts"])
			.expect("serve with a script should parse");
	let Command::SigningHost(args) = with_script.command else {
		panic!("expected signing-host command");
	};
	assert!(
		validate_signing_args(&args)
			.unwrap_err()
			.to_string()
			.contains("--serve cannot be combined with --script")
	);

	let with_exec =
		Cli::try_parse_from(["truapi-host", "signing-host", "--serve", "exec", "/help"])
			.expect("serve with exec should parse");
	let Command::SigningHost(args) = with_exec.command else {
		panic!("expected signing-host command");
	};
	assert!(
		validate_signing_args(&args)
			.unwrap_err()
			.to_string()
			.contains("--serve cannot be combined with the exec subcommand")
	);
}

#[test]
fn serve_ready_names_the_endpoint_and_the_approval_policy() {
	let prompting = terminal_ui::SystemEvent::ServeReady {
		url: "ws://127.0.0.1:9955".to_string(),
		auto_accept: false,
	}
	.human();
	assert!(prompting.contains("ws://127.0.0.1:9955"));
	assert!(prompting.contains("--auto-accept"));

	let accepting = terminal_ui::SystemEvent::ServeReady {
		url: "ws://127.0.0.1:9955".to_string(),
		auto_accept: true,
	}
	.human();
	assert!(accepting.contains("approved automatically"));
}

#[test]
fn signing_host_accepts_a_startup_session() {
	let cli = Cli::try_parse_from([
		"truapi-host",
		"signing-host",
		"--session",
		"alice",
		"exec",
		"/session",
	])
	.expect("startup session should parse");
	let Command::SigningHost(args) = cli.command else {
		panic!("expected signing-host command");
	};

	assert_eq!(args.session.as_deref(), Some("alice"));
	assert!(validate_signing_args(&args).is_ok());
}

#[test]
fn rerun_without_a_selected_script_does_not_create_or_edit_a_project() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let projects = temporary.path().join("scripts");

	let error = select_script(&ScriptCommand::Run(None), &projects, &mut None).unwrap_err();

	assert_eq!(
		(error.to_string(), projects.exists()),
		(
			"no script selected; use /script to create one or /script <path> to select one"
				.to_string(),
			false
		)
	);
	Ok(())
}

#[test]
fn clearing_sessions_keeps_managed_script_projects() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let base = Some(temporary.path().to_path_buf());
	let catalog = SessionCatalog::new(state_base_path(base.clone()), "testnet")?;
	let profile = catalog.ensure_profile(DEFAULT_SESSION_NAME)?;
	let projects = script_project_directory(base);
	let script = select_script_to_edit(&projects, &mut None)?;
	sessions::store_session_last_script(&profile.path, &script)?;

	catalog.clear(catalog.prepare_clear(&SessionClearTarget::All)?)?;

	assert_eq!(
		(projects, script.is_file(), profile.path.exists()),
		(temporary.path().join("scripts"), true, false)
	);
	Ok(())
}

#[test]
fn bare_script_selection_reuses_the_last_existing_script() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let mut last_script = None;

	let first = select_script_to_edit(temporary.path(), &mut last_script)?;
	let second = select_script_to_edit(temporary.path(), &mut last_script)?;
	assert_eq!(second, first);

	std::fs::remove_file(&first)?;
	let replacement = select_script_to_edit(temporary.path(), &mut last_script)?;
	assert_ne!(replacement, first);
	assert!(replacement.is_file());
	Ok(())
}

#[test]
fn explicit_script_becomes_the_next_bare_script_selection() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let scripts = temporary.path().join("scripts");
	std::fs::create_dir_all(&scripts)?;
	let scratch = scripts.join("scratch.ts");
	std::fs::write(&scratch, "console.log('scratch');")?;
	let mut last_script = Some(scratch);
	let explicit = temporary.path().join("product-script.ts");
	std::fs::write(&explicit, "console.log('product');")?;

	let remembered = remember_script(Some(temporary.path()), &mut last_script, explicit.clone())?;
	let selected = select_script_to_edit(&scripts, &mut last_script)?;

	assert_eq!(remembered, explicit);
	assert_eq!(selected, explicit);
	assert_eq!(
		sessions::session_last_script(temporary.path())?.as_deref(),
		Some(explicit.as_path())
	);
	Ok(())
}

#[test]
fn signing_host_rejects_managed_session_with_mnemonic() {
	let cli = Cli::try_parse_from([
		"truapi-host",
		"signing-host",
		"--mnemonic",
		"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
		"--session",
		"alice",
		"exec",
		"/session",
	])
	.expect("clap should parse conflicting signer options");
	let Command::SigningHost(args) = cli.command else {
		panic!("expected signing-host command");
	};

	assert!(
		validate_signing_args(&args)
			.unwrap_err()
			.to_string()
			.contains("--session cannot be used")
	);
}

#[test]
fn paired_device_management_rejects_an_ephemeral_mnemonic_session() {
	let cli = Cli::try_parse_from([
		"truapi-host",
		"signing-host",
		"--mnemonic",
		"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
		"exec",
		"/devices",
	])
	.expect("clap should parse the command before lifecycle validation");
	let Command::SigningHost(args) = cli.command else {
		panic!("expected signing-host command");
	};

	assert_eq!(
		validate_signing_args(&args).unwrap_err().to_string(),
		"paired-device management is unavailable when launched with --mnemonic"
	);
}

#[test]
fn saved_log_level_is_the_default_but_explicit_selection_wins() {
	assert_eq!(
		[
			resolve_log_level(None, None),
			resolve_log_level(None, Some(LogLevel::Debug)),
			resolve_log_level(Some(LogLevel::Trace), Some(LogLevel::Debug)),
		],
		[LogLevel::Info, LogLevel::Debug, LogLevel::Trace]
	);
}

#[test]
fn log_levels_display_in_lowercase() {
	assert_eq!(
		[LogLevel::Error, LogLevel::Warn, LogLevel::Info, LogLevel::Debug, LogLevel::Trace,]
			.map(|level| level.to_string()),
		["error", "warn", "info", "debug", "trace"]
	);
}

#[test]
fn valid_rust_log_value_replaces_the_resolved_level() {
	let (filter, displayed) = startup_filter(Some(" warn,truapi=trace "), LogLevel::Info);
	let (_, fallback) = startup_filter(Some("truapi=verbose"), LogLevel::Debug);

	assert_eq!(
		(filter.to_string(), displayed, fallback),
		("truapi=trace,warn".to_string(), "warn,truapi=trace".to_string(), "debug".to_string())
	);
}

#[test]
fn log_preference_uses_the_selected_host_base_path() {
	for arguments in [
		vec!["truapi-host", "pairing-host", "--base-path", "custom-state"],
		vec!["truapi-host", "signing-host", "--base-path", "custom-state"],
		vec!["truapi-host", "dev", "--base-path", "custom-state"],
		vec!["truapi-host", "materials", "resolve", "face.json", "--base-path", "custom-state"],
	] {
		let cli = Cli::try_parse_from(arguments).expect("host command should parse");

		assert_eq!(command_base_path(&cli.command), PathBuf::from("custom-state/v2"));
	}
}

#[test]
fn log_level_preference_round_trips() -> Result<()> {
	let temporary = tempfile::tempdir()?;

	assert_eq!(load_log_level(temporary.path())?, None);
	store_log_level(temporary.path(), LogLevel::Debug)?;
	store_log_level(temporary.path(), LogLevel::Trace)?;

	assert_eq!(load_log_level(temporary.path())?, Some(LogLevel::Trace));
	Ok(())
}

#[test]
fn invalid_saved_log_level_is_reported() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	std::fs::write(temporary.path().join(LOG_LEVEL_FILE), "verbose\n")?;

	let error = format!("{:#}", load_log_level(temporary.path()).unwrap_err());

	assert!(error.contains("invalid log level `verbose`"), "{error}");
	Ok(())
}

#[test]
fn log_controller_persists_successful_updates() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let reloaded = Arc::new(std::sync::Mutex::new(None));
	let observed = reloaded.clone();
	let controller = LogController {
		reload: Arc::new(move |level| {
			*observed.lock().expect("reload observation lock") = Some(level);
			Ok(())
		}),
		base_path: temporary.path().to_path_buf(),
	};

	controller.set(LogLevel::Debug)?;

	assert_eq!(
		(*reloaded.lock().expect("reload observation lock"), load_log_level(temporary.path())?,),
		(Some(LogLevel::Debug), Some(LogLevel::Debug))
	);
	Ok(())
}

#[test]
fn log_controller_does_not_reload_when_persistence_fails() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let base_path = temporary.path().join("not-a-directory");
	std::fs::write(&base_path, "occupied")?;
	let reloaded = Arc::new(std::sync::Mutex::new(None));
	let observed = reloaded.clone();
	let controller = LogController {
		reload: Arc::new(move |level| {
			*observed.lock().expect("reload observation lock") = Some(level);
			Ok(())
		}),
		base_path,
	};

	let error = controller.set(LogLevel::Debug).unwrap_err().to_string();

	assert!(error.contains("persist saved log level"), "{error}");
	assert_eq!(*reloaded.lock().expect("reload observation lock"), None);
	Ok(())
}

struct SilentSink;

impl DebugSink for SilentSink {
	fn emit(&self, _event: DebugEvent) {}
}

struct UnusedFactory;

impl frame_server::ProductRuntimeFactory for UnusedFactory {
	fn product_runtime(
		&self,
		_product: ProductContext,
		_sink: Arc<dyn FrameSink>,
	) -> ProductRuntime {
		panic!("this test observes the wrapping decision only")
	}
}

/// What `DebugTappedRuntime` does once installed is covered next to it. This
/// covers whether it is installed at all: a `tap_for_debugger` that returned
/// `factory` in both arms leaves a host that accepts `--debugger`, announces
/// the dial, and streams nothing.
#[test]
fn the_tap_wraps_the_factory_exactly_when_a_sink_was_opened() {
	let factory: Arc<dyn frame_server::ProductRuntimeFactory> = Arc::new(UnusedFactory);
	let tapped = tap_for_debugger(factory.clone(), Some(Arc::new(SilentSink)));
	let untapped = tap_for_debugger(factory.clone(), None);

	assert_eq!((Arc::ptr_eq(&factory, &tapped), Arc::ptr_eq(&factory, &untapped)), (false, true));
}
