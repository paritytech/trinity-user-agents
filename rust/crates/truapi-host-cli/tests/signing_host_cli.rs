//! Process-boundary smoke tests for signing-host invocation modes.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_truapi-host"))
}

fn seed_two_paired_hosts(base_path: &Path) -> PathBuf {
    let profile = base_path.join("v2/paseo-next-v2/alice_signing_host");
    std::fs::create_dir_all(&profile).expect("create signing-host profile");
    std::fs::write(
        profile.join("paired-hosts.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "paired_hosts": [
                {
                    "version": 1,
                    "statement_account_id": vec![1_u8; 32],
                    "encryption_public_key": vec![11_u8; 32],
                    "host_name": "First"
                },
                {
                    "version": 1,
                    "statement_account_id": vec![2_u8; 32],
                    "encryption_public_key": vec![22_u8; 32],
                    "host_name": "Second"
                }
            ]
        }))
        .expect("encode paired hosts"),
    )
    .expect("seed paired hosts");
    profile
}

#[test]
fn versioned_state_starts_fresh_and_preserves_previous_state() {
    for source in ["argument", "environment", "default"] {
        let temporary = tempfile::tempdir().expect("create temporary state root");
        let base_path = temporary.path().join("truapi-host");
        let previous_profile = base_path.join("paseo-next-v2/previous_signing_host");
        let previous_role = base_path.join("paseo-next-v2/signing-host");
        std::fs::create_dir_all(&previous_profile).expect("create previous profile");
        std::fs::create_dir_all(&previous_role).expect("create previous role");
        let previous_files = [
            (
                base_path.join("accounts.json"),
                r#"{"version":1,"accounts":[]}"#,
            ),
            (base_path.join("log-level"), "trace\n"),
            (previous_role.join("current-session"), "previous\n"),
            (
                previous_profile.join("session.json"),
                r#"{"version":1,"user_id":"previous.01"}"#,
            ),
            (
                previous_profile.join("core-storage.json"),
                "previous core state",
            ),
            (
                previous_profile.join("paired-hosts.json"),
                "previous pairings",
            ),
        ];
        for (path, contents) in &previous_files {
            std::fs::write(path, contents).expect("seed previous state");
        }

        let mut invocation = command();
        invocation
            .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
            .env_remove("HOST_CLI_SIGNER_MNEMONIC")
            .env_remove("TRUAPI_HOST_BASE_PATH")
            .env_remove("TRUAPI_HOST_LOG")
            .env_remove("RUST_LOG")
            .env("XDG_STATE_HOME", temporary.path())
            .env("TRUAPI_HOST_NO_UPDATE", "1");
        match source {
            "argument" => {
                invocation.arg("--base-path").arg(&base_path);
                invocation.env("TRUAPI_HOST_BASE_PATH", temporary.path().join("unused"));
            }
            "environment" => {
                invocation.env("TRUAPI_HOST_BASE_PATH", &base_path);
            }
            _ => {}
        }
        let output = invocation
            .args(["exec", "/log debug"])
            .stdin(Stdio::null())
            .output()
            .expect("start with versioned state");

        assert!(output.status.success(), "{source}: {output:?}");
        assert_eq!(
            (
                std::fs::read_to_string(base_path.join("v2/log-level"))
                    .expect("read log preference"),
                std::fs::read_to_string(
                    base_path.join("v2/paseo-next-v2/signing-host/current-session")
                )
                .expect("read fresh session selection"),
            ),
            ("debug\n".to_string(), "default\n".to_string())
        );
        for (path, contents) in &previous_files {
            assert_eq!(
                std::fs::read_to_string(path).expect("read previous state"),
                *contents
            );
        }
    }
}

#[test]
fn interactive_mode_rejects_non_tty_stdio_with_usage_exit() {
    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .stdin(Stdio::null())
        .output()
        .expect("run signing-host without a TTY");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("interactive signing-host requires a TTY")
    );
    assert!(!output.stdout.contains(&0x1b));
}

#[test]
fn exec_help_is_plain_and_exits_successfully() {
    let base_path =
        std::env::temp_dir().join(format!("truapi-host-cli-exec-help-{}", std::process::id()));
    let mut invocation = command();
    invocation.arg("signing-host");
    #[cfg(not(unix))]
    invocation.args(["--frame-listen", "127.0.0.1:0"]);
    let output = invocation
        .arg("--base-path")
        .arg(&base_path)
        .args(["exec", "/help"])
        .stdin(Stdio::null())
        .output()
        .expect("run signing-host exec /help");
    let _ = std::fs::remove_dir_all(base_path);

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("/whoami"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/script"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/copy"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/product"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/session"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/devices"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/devices --remove <id> --force"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/approval automatic"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("/session --clear-all"));
    #[cfg(unix)]
    assert!(String::from_utf8_lossy(&output.stdout).contains("ws+unix:"));
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stderr.contains(&0x1b));
}

#[test]
fn exec_rejects_process_local_approval_changes() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["exec", "/approval automatic"])
        .stdin(Stdio::null())
        .output()
        .expect("reject approval mode change outside the terminal UI");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("/approval is only available in the terminal UI")
    );
}

#[test]
fn exec_explains_how_to_pair_when_clipboard_paste_has_no_terminal_ui() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["exec", "/pair"])
        .stdin(Stdio::null())
        .output()
        .expect("reject clipboard image paste outside the terminal UI");

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "clipboard image paste needs an interactive signing host; use /pair <image-path> or /pair <polkadotapp://pair?...>"
    ));
}

#[test]
fn exec_product_reports_the_normalized_selected_product() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let output = command()
        .args([
            "signing-host",
            "--frame-listen",
            "127.0.0.1:0",
            "--product-id",
            "Dotli.DOT",
        ])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["exec", "/product"])
        .stdin(Stdio::null())
        .output()
        .expect("run signing-host exec /product");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("dotli.dot"));
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stderr.contains(&0x1b));
}

#[test]
fn bare_script_in_non_tty_exec_mode_fails_without_opening_an_editor() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["exec", "/script"])
        .stdin(Stdio::null())
        .output()
        .expect("run bare script without a TTY");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("/script without a path requires an interactive terminal")
    );
    assert!(
        !temporary
            .path()
            .join("v2/paseo-next-v2/signing-host/scripts")
            .exists()
    );
}

#[test]
fn startup_session_is_reported_and_restored() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let base_path = temporary.path();
    let first = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(base_path)
        .args(["--session", "alice", "exec", "/session"])
        .stdin(Stdio::null())
        .output()
        .expect("run signing-host in alice session");
    assert!(first.status.success());
    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("Session alice"));
    assert!(first_stdout.contains("alice_signing_host"));
    assert!(first_stdout.contains("User <not provisioned>"));
    assert!(first_stdout.contains("No connected user"));
    assert!(first_stdout.contains("Use /session <name> to start a session."));

    let restored = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(base_path)
        .args(["exec", "/session --list"])
        .stdin(Stdio::null())
        .output()
        .expect("restore signing-host session");
    assert!(restored.status.success());
    let restored_stdout = String::from_utf8_lossy(&restored.stdout);
    assert!(restored_stdout.contains("* alice"));
    assert!(!restored_stdout.contains("default"));
    assert!(!restored.stdout.contains(&0x1b));
}

#[test]
fn exec_clear_removes_the_named_session_without_extra_confirmation() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let session_path = temporary.path().join("v2/paseo-next-v2/alice_signing_host");
    std::fs::create_dir_all(&session_path).expect("seed session");
    std::fs::write(session_path.join("state"), "local state").expect("seed session state");

    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["--session", "alice", "exec", "/session --clear alice"])
        .stdin(Stdio::null())
        .output()
        .expect("clear active signing-host session");

    assert!(output.status.success());
    assert!(!session_path.exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Session alice cleared"));
    assert!(stdout.contains("Signing host stopped"));
    assert!(!output.stdout.contains(&0x1b));
}

#[test]
fn exec_clear_all_removes_every_session_for_the_network() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let network_path = temporary.path().join("v2/paseo-next-v2");
    let role_path = network_path.join("signing-host");
    let alice = network_path.join("alice_signing_host");
    let bob = network_path.join("bob_signing_host");
    for path in [&role_path, &alice, &bob] {
        std::fs::create_dir_all(path).expect("seed session");
        std::fs::write(path.join("state"), "local state").expect("seed session state");
    }
    let unrelated = network_path.join("pairing-host");
    std::fs::create_dir_all(&unrelated).expect("seed unrelated host state");

    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["--session", "bob", "exec", "/session --clear-all"])
        .stdin(Stdio::null())
        .output()
        .expect("clear all signing-host sessions");

    assert!(output.status.success());
    assert!(!role_path.exists());
    assert!(!alice.exists());
    assert!(!bob.exists());
    assert!(unrelated.exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("All sessions cleared for paseo-next-v2"));
    assert!(!output.stdout.contains(&0x1b));
}

#[test]
fn exec_device_removal_preserves_pairings_when_the_local_session_is_inactive() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let profile = seed_two_paired_hosts(temporary.path());
    let pairings_before =
        std::fs::read(profile.join("paired-hosts.json")).expect("read paired hosts");

    let listed = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["--session", "alice", "exec", "/devices"])
        .stdin(Stdio::null())
        .output()
        .expect("list paired devices");
    assert!(listed.status.success());
    let stdout = String::from_utf8_lossy(&listed.stdout);
    assert!(stdout.contains("Paired devices for session alice"));
    assert!(stdout.contains(&format!("0x{}  First", hex::encode([1; 32]))));
    assert!(stdout.contains(&format!("0x{}  Second", hex::encode([2; 32]))));

    let remove_command = format!("/devices --remove 0x{}", hex::encode([1; 32]));
    let removed = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["--session", "alice", "exec", &remove_command])
        .stdin(Stdio::null())
        .output()
        .expect("remove paired device");
    assert_eq!(removed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&removed.stderr).contains("wallet is locked"));

    assert_eq!(
        std::fs::read(profile.join("paired-hosts.json")).expect("read paired hosts"),
        pairings_before,
    );
}

#[test]
fn exec_force_device_removal_removes_exactly_one_pairing_when_notification_fails() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let profile = seed_two_paired_hosts(temporary.path());

    let remove_command = format!("/devices --remove 0x{} --force", hex::encode([1_u8; 32]));
    let removed = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["--session", "alice", "exec", &remove_command])
        .stdin(Stdio::null())
        .output()
        .expect("force remove paired device");

    assert!(removed.status.success());
    let expected = format!(
        "Removed paired device 0x{} from session alice",
        hex::encode([1_u8; 32])
    );
    assert_eq!(
        String::from_utf8_lossy(&removed.stdout).lines().last(),
        Some(expected.as_str())
    );
    let stderr = String::from_utf8_lossy(&removed.stderr);
    assert!(stderr.contains("Paired device removed without notification"));
    assert!(stderr.contains("wallet is locked"));

    let stored: serde_json::Value = serde_json::from_slice(
        &std::fs::read(profile.join("paired-hosts.json")).expect("read paired hosts"),
    )
    .expect("decode paired hosts");
    assert_eq!(
        stored["paired_hosts"],
        serde_json::json!([
            {
                "version": 1,
                "statement_account_id": vec![2_u8; 32],
                "encryption_public_key": vec![22_u8; 32],
                "host_name": "Second"
            }
        ])
    );
}

#[test]
fn default_session_is_not_user_selectable() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["exec", "/session default"])
        .stdin(Stdio::null())
        .output()
        .expect("reject the internal default session");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("session name `default` is reserved for bootstrap state")
    );
}

#[test]
fn existing_local_signer_is_activated_and_cached_at_startup() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let base_path = temporary.path();
    let state_path = base_path.join("v2");
    std::fs::create_dir_all(&state_path).expect("create active state root");
    std::fs::write(
        state_path.join("accounts.json"),
        r#"{
  "version": 1,
  "accounts": [{
    "name": "auto-1",
    "network": "paseo-next-v2",
    "mnemonic": "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    "lite_username": "cachedalice.01",
    "public_key_hex": "0x00",
    "address": "5GrwvaEF5zXb26Fz9rcQpDWSKfwVwqNxyvE9uZunJMtBEw2s",
    "created_at_unix": 1,
    "attested": true
  }]
}"#,
    )
    .expect("seed local account store");

    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(base_path)
        .args(["exec", "/session"])
        .stdin(Stdio::null())
        .output()
        .expect("run signing-host with a cached signer");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Signing host ready"));
    assert!(stdout.contains("User cachedalice.01"));
    let metadata = std::fs::read_to_string(
        base_path.join("v2/paseo-next-v2/cachedalice.01_signing_host/session.json"),
    )
    .expect("read persisted session identity");
    assert!(metadata.contains("cachedalice.01"));
}

#[test]
fn imported_session_restores_the_exact_bound_account() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let network_path = temporary.path().join("v2/paseo-next-v2");
    let profile = network_path.join("importedalice.01_signing_host");
    let role_path = network_path.join("signing-host");
    std::fs::create_dir_all(&profile).expect("create imported profile");
    std::fs::create_dir_all(&role_path).expect("create signing-host state");
    std::fs::write(
        profile.join("accounts.json"),
        r#"{
  "version": 1,
  "accounts": [{
    "name": "imported",
    "network": "paseo-next-v2",
    "mnemonic": "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    "lite_username": "importedalice.01",
    "public_key_hex": "0x00",
    "address": "5GrwvaEF5zXb26Fz9rcQpDWSKfwVwqNxyvE9uZunJMtBEw2s",
    "created_at_unix": 1,
    "attested": true,
    "origin": "imported"
  }]
}"#,
    )
    .expect("seed imported account store");
    std::fs::write(
        profile.join("session.json"),
        r#"{
  "version": 1,
  "user_id": "importedalice.01",
  "account_name": "imported"
}"#,
    )
    .expect("seed signer binding");
    std::fs::write(role_path.join("current-session"), "importedalice.01\n")
        .expect("seed current session");

    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["exec", "/session"])
        .stdin(Stdio::null())
        .output()
        .expect("restore imported signing-host session");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Signing host ready"));
    assert!(stdout.contains("Session importedalice.01"));
    assert!(stdout.contains("User importedalice.01"));
}

#[test]
fn imported_session_without_dotns_username_restores_by_account_binding() {
    let temporary = tempfile::tempdir().expect("create temporary session root");
    let network_path = temporary.path().join("v2/paseo-next-v2");
    let session_name = "imported-0123456789abcdef";
    let profile = network_path.join(format!("{session_name}_signing_host"));
    let role_path = network_path.join("signing-host");
    std::fs::create_dir_all(&profile).expect("create imported profile");
    std::fs::create_dir_all(&role_path).expect("create signing-host state");
    std::fs::write(
        profile.join("accounts.json"),
        r#"{
  "version": 1,
  "accounts": [{
    "name": "imported",
    "network": "paseo-next-v2",
    "mnemonic": "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    "lite_username": "",
    "public_key_hex": "0x00",
    "address": "5GrwvaEF5zXb26Fz9rcQpDWSKfwVwqNxyvE9uZunJMtBEw2s",
    "created_at_unix": 1,
    "attested": true,
    "origin": "imported"
  }]
}"#,
    )
    .expect("seed username-less imported account store");
    std::fs::write(
        profile.join("session.json"),
        r#"{
  "version": 1,
  "account_name": "imported"
}"#,
    )
    .expect("seed account binding");
    std::fs::write(
        role_path.join("current-session"),
        format!("{session_name}\n"),
    )
    .expect("seed current session");

    let output = command()
        .args(["signing-host", "--frame-listen", "127.0.0.1:0"])
        .arg("--base-path")
        .arg(temporary.path())
        .args(["exec", "/session"])
        .stdin(Stdio::null())
        .output()
        .expect("restore username-less imported session");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Signing host ready"));
    assert!(stdout.contains(&format!("Session {session_name}")));
    assert!(stdout.contains("User <no assigned username>"));
}
