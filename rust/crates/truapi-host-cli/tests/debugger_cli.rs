//! Process-boundary coverage for the `--debugger` startup report.
//!
//! The report is what stands in for a build gate on the released binary: a host
//! that streams frames says so where the developer is already looking, and a
//! host that does not says that too. Whether a host role still makes the report
//! is only visible from a real invocation, so these run the binary.

use std::process::{Command, Stdio};

/// Which headless host role to start. `dev` reaches the report through
/// `signing-host`, so it is covered by the signing-host case.
#[derive(Clone, Copy, Debug)]
enum Role {
	PairingHost,
	SigningHost,
}

/// How the debugger URL is offered to the process. Both spellings may be set at
/// once, which is the case where clap's answer and a plain string comparison of
/// the two values disagree.
#[derive(Clone, Copy)]
struct Switch<'a> {
	flag: Option<&'a str>,
	variable: Option<&'a str>,
}

const DEBUGGER_URL: &str = "ws://127.0.0.1:9231";

const OFF: Switch<'static> = Switch { flag: None, variable: None };

/// Start `role` with `switch` and return the dial report it printed.
///
/// Both roles are driven to an immediate exit and neither reaches a network: the
/// signing host through `exec`, the pairing host through a script path that does
/// not resolve. Both print the report before that.
fn dial_report(role: Role, switch: Switch<'_>) -> Vec<String> {
	let temporary = tempfile::tempdir().expect("create temporary state root");
	let mut command = Command::new(env!("CARGO_BIN_EXE_truapi-host"));
	command
		.env_remove("TRUAPI_DEBUGGER_URL")
		.env_remove("TRUAPI_HOST_BASE_PATH")
		.env_remove("TRUAPI_HOST_LOG")
		.env_remove("RUST_LOG")
		.env("TRUAPI_HOST_NO_UPDATE", "1")
		.stdin(Stdio::null());
	if let Some(url) = switch.flag {
		command.args(["--debugger", url]);
	}
	if let Some(url) = switch.variable {
		command.env("TRUAPI_DEBUGGER_URL", url);
	}
	match role {
		Role::PairingHost => command
			.args(["pairing-host", "--frame-listen", "127.0.0.1:0"])
			.arg("--base-path")
			.arg(temporary.path())
			.arg("--script")
			.arg(temporary.path().join("no-such-script.ts")),
		Role::SigningHost => command
			.args(["signing-host", "--frame-listen", "127.0.0.1:0"])
			.arg("--base-path")
			.arg(temporary.path())
			.args(["exec", "/session"]),
	};
	let output = command.output().expect("run a headless host role");

	String::from_utf8_lossy(&output.stdout)
		.lines()
		.map(|line| line.trim().trim_start_matches(['•', '!', ' ']).to_string())
		.skip_while(|line| {
			!line.starts_with("Wire debugger") && !line.starts_with("Streaming wire frames")
		})
		.take(2)
		.collect()
}

#[test]
fn a_host_that_dials_no_debugger_says_so() {
	for role in [Role::PairingHost, Role::SigningHost] {
		assert_eq!(
			dial_report(role, OFF),
			vec![
				"Wire debugger off".to_string(),
				"No --debugger and no TRUAPI_DEBUGGER_URL, so no frames leave this host"
					.to_string()
			],
			"{role:?}"
		);
	}
}

#[test]
fn a_dialling_host_names_the_endpoint_and_the_switch_clap_read_it_from() {
	let cases = [
		(Switch { flag: Some(DEBUGGER_URL), variable: None }, "--debugger"),
		(Switch { flag: None, variable: Some(DEBUGGER_URL) }, "TRUAPI_DEBUGGER_URL"),
		// An explicit flag wins, so the variable is not what a developer would
		// have to clear, whether or not it holds the same string.
		(Switch { flag: Some(DEBUGGER_URL), variable: Some(DEBUGGER_URL) }, "--debugger"),
		(Switch { flag: Some(DEBUGGER_URL), variable: Some("ws://127.0.0.1:9300") }, "--debugger"),
	];
	for role in [Role::PairingHost, Role::SigningHost] {
		for (switch, source) in cases {
			assert_eq!(
				dial_report(role, switch),
				vec![
					"Streaming wire frames to a debugger".to_string(),
					format!("{DEBUGGER_URL} (from {source})")
				],
				"{role:?} from {source}"
			);
		}
	}
}

#[test]
fn a_debugger_url_that_is_not_loopback_fails_startup() {
	let temporary = tempfile::tempdir().expect("create temporary state root");
	let output = Command::new(env!("CARGO_BIN_EXE_truapi-host"))
		.env_remove("TRUAPI_DEBUGGER_URL")
		.env_remove("TRUAPI_HOST_BASE_PATH")
		.env("TRUAPI_HOST_NO_UPDATE", "1")
		.stdin(Stdio::null())
		.args(["--debugger", "ws://example.com:9231"])
		.args(["signing-host", "--frame-listen", "127.0.0.1:0"])
		.arg("--base-path")
		.arg(temporary.path())
		.args(["exec", "/session"])
		.output()
		.expect("run a signing host with an off-loopback debugger");

	assert_eq!(output.status.code(), Some(1));
	assert!(
		String::from_utf8_lossy(&output.stderr).contains(
			"--debugger ws://example.com:9231 must be a ws:// URL on 127.0.0.1, localhost, or [::1]"
		),
		"{output:?}"
	);
}
