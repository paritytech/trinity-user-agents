//! Process-boundary coverage for `truapi-host stack`: what an operator publishes must open under
//! the key it printed, and `bundle` must never ship an expired copy or lower `sequence`.

use std::{
	fs,
	io::{Read, Write},
	net::TcpListener,
	path::Path,
	process::{Command, Output},
	thread,
};

fn stack(args: &[&str]) -> Output {
	Command::new(env!("CARGO_BIN_EXE_truapi-host"))
		.arg("stack")
		.args(args)
		.env("TRUAPI_HOST_NO_UPDATE", "1")
		.output()
		.expect("run truapi-host")
}

fn stdout(output: &Output) -> String {
	String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
	String::from_utf8_lossy(&output.stderr).into_owned()
}

/// 2020-01-01T00:00:00Z, long past.
const LONG_AGO: u32 = 1_577_836_800;

fn write_model(dir: &Path, name: &str, expires_at: Option<u32>) -> String {
	let mut model = serde_json::json!({
		"id": "local", "name": name, "network": "local", "tld": "local", "services": [],
		"chains": [{
			"role": "Relay",
			"genesisHash": "0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92",
			"rpc": ["ws://127.0.0.1:9944"],
		}],
	});
	if let Some(expires_at) = expires_at {
		model["expiresAt"] = expires_at.into();
	}
	let path = dir.join(format!("{name}.json"));
	fs::write(&path, model.to_string()).unwrap();
	path.to_str().unwrap().to_owned()
}

/// Publishes a model and returns the bytes written.
fn publish(dir: &Path, model: &str, key_file: &str, state: &str) -> Vec<u8> {
	let site = dir.join("site");
	let output = stack(&[
		"publish",
		model,
		"--generation",
		"g1",
		"--out",
		site.to_str().unwrap(),
		"--signing-key-file",
		key_file,
		"--state",
		state,
	]);
	assert!(output.status.success(), "{}", stderr(&output));
	fs::read(site.join(".well-known/trinity-stack.json")).unwrap()
}

/// Serves `document` to one request on a loopback port and returns its URL.
fn serve_once(document: Vec<u8>) -> String {
	respond_once("200 OK", "", document)
}

/// Answers one request with `status`, extra `headers` and `body`, and returns the URL.
fn respond_once(status: &'static str, headers: &'static str, body: Vec<u8>) -> String {
	let length = format!("Content-Length: {}\r\n", body.len());
	respond_with_head(format!("HTTP/1.1 {status}\r\n{headers}{length}"), body)
}

/// Answers one request with `head` and `body`, closing the connection to end the body.
fn respond_with_head(head: String, body: Vec<u8>) -> String {
	let listener = TcpListener::bind("127.0.0.1:0").unwrap();
	let url = format!("http://{}/stack.json", listener.local_addr().unwrap());
	thread::spawn(move || {
		let (mut socket, _) = listener.accept().unwrap();
		let mut request = [0; 4096];
		let _ = socket.read(&mut request);
		socket.write_all(format!("{head}Connection: close\r\n\r\n").as_bytes()).unwrap();
		let _ = socket.write_all(&body);
	});
	url
}

#[test]
fn a_redirect_is_refused_and_says_where_it_led() {
	let url =
		respond_once("301 Moved Permanently", "Location: http://elsewhere.example/\r\n", vec![]);
	let output = stack(&["verify", &url]);
	assert!(!output.status.success());
	assert!(stderr(&output).contains("redirects are not followed"), "{}", stderr(&output));
	assert!(stderr(&output).contains("http://elsewhere.example/"), "{}", stderr(&output));
}

#[test]
fn publish_checks_the_url_before_writing_anything() {
	let dir = tempfile::tempdir().unwrap();
	let model = write_model(dir.path(), "Local", None);
	let site = dir.path().join("site");
	let output = stack(&[
		"publish",
		&model,
		"--generation",
		"g1",
		"--out",
		site.to_str().unwrap(),
		"--url",
		"http://stack.example/s.json",
	]);
	assert!(stderr(&output).contains("invalid-url"), "{}", stderr(&output));
	assert!(!site.exists(), "nothing is published when the link would be refused");
}

#[test]
fn verify_fails_on_an_expired_stack_but_still_prints_it() {
	let dir = tempfile::tempdir().unwrap();
	let model = write_model(dir.path(), "Gone", Some(LONG_AGO));
	let site = dir.path().join("site");
	let published =
		stack(&["publish", &model, "--generation", "g1", "--out", site.to_str().unwrap()]);
	assert!(published.status.success(), "{}", stderr(&published));
	let output = stack(&["verify", site.join(".well-known/trinity-stack.json").to_str().unwrap()]);
	assert!(!output.status.success());
	let report: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
	assert_eq!(report["expired"], true);
}

#[test]
fn a_published_stack_opens_under_the_printed_key_only() {
	let dir = tempfile::tempdir().unwrap();
	let key_file = dir.path().join("key");
	let keygen = stack(&["keygen", "--out", key_file.to_str().unwrap()]);
	assert!(keygen.status.success(), "{}", stderr(&keygen));
	let key = stdout(&keygen).trim().to_owned();

	let model = write_model(dir.path(), "Local", None);
	let state = dir.path().join("state.json");
	publish(dir.path(), &model, key_file.to_str().unwrap(), state.to_str().unwrap());
	let served = dir.path().join("site/.well-known/trinity-stack.json");
	let served = served.to_str().unwrap();

	let verified = stack(&["verify", served, "--key", &key]);
	assert!(verified.status.success(), "{}", stderr(&verified));
	let report: serde_json::Value = serde_json::from_str(&stdout(&verified)).unwrap();
	assert_eq!((report["signed"].clone(), report["expired"].clone()), (true.into(), false.into()));

	let other = stack(&["keygen", "--out", dir.path().join("other").to_str().unwrap()]);
	let wrong = stack(&["verify", served, "--key", stdout(&other).trim()]);
	assert!(!wrong.status.success());
	assert!(stderr(&wrong).contains("bad-signature"), "{}", stderr(&wrong));

	let unchecked: serde_json::Value =
		serde_json::from_str(&stdout(&stack(&["verify", served]))).unwrap();
	assert_eq!(unchecked["signed"], false, "without a key nothing checks the signature");
	assert_eq!(unchecked["envelope"], true, "but the file still says it carries one");
}

#[test]
fn a_descriptor_past_the_size_cap_is_unreachable() {
	let too_large = vec![b' '; 1024 * 1024 + 1];
	let declared = respond_once("200 OK", "", too_large.clone());
	let streamed = respond_with_head("HTTP/1.1 200 OK\r\n".into(), too_large);
	for url in [declared, streamed] {
		let output = stack(&["verify", &url]);
		assert!(!output.status.success());
		assert!(stderr(&output).contains("larger than"), "{}", stderr(&output));
	}
}

#[cfg(unix)]
#[test]
fn a_published_descriptor_is_readable_by_the_web_server() {
	use std::os::unix::fs::PermissionsExt;
	let dir = tempfile::tempdir().unwrap();
	let model = write_model(dir.path(), "Local", None);
	let site = dir.path().join("site");
	let output = stack(&["publish", &model, "--generation", "g1", "--out", site.to_str().unwrap()]);
	assert!(output.status.success(), "{}", stderr(&output));
	let served = fs::metadata(site.join(".well-known/trinity-stack.json")).unwrap();
	assert_ne!(served.permissions().mode() & 0o004, 0, "a web server as another user reads it");
}

#[test]
fn keygen_creates_the_directory_it_writes_into() {
	let dir = tempfile::tempdir().unwrap();
	let key = dir.path().join("fresh/stack.key");
	let output = stack(&["keygen", "--out", key.to_str().unwrap()]);
	assert!(output.status.success(), "{}", stderr(&output));
	assert!(key.exists());
}

#[test]
fn bundle_refuses_an_expired_stack_and_never_lowers_sequence() {
	let dir = tempfile::tempdir().unwrap();
	let key_file = dir.path().join("key");
	let key = stdout(&stack(&["keygen", "--out", key_file.to_str().unwrap()]))
		.trim()
		.to_owned();
	let key_file = key_file.to_str().unwrap();
	let state = dir.path().join("state.json");
	let state = state.to_str().unwrap();
	let older = publish(dir.path(), &write_model(dir.path(), "Older", None), key_file, state);
	let newer = publish(dir.path(), &write_model(dir.path(), "Newer", None), key_file, state);
	let expired =
		publish(dir.path(), &write_model(dir.path(), "Expired", Some(LONG_AGO)), key_file, state);
	let out = dir.path().join("bundled.json");
	let out = out.to_str().unwrap();

	let first = stack(&["bundle", &serve_once(newer.clone()), "--key", &key, "--out", out]);
	assert!(first.status.success(), "{}", stderr(&first));

	let rollback = stack(&["bundle", &serve_once(older), "--key", &key, "--out", out]);
	assert!(!rollback.status.success());
	assert!(stderr(&rollback).contains("rollback"), "{}", stderr(&rollback));

	let gone = stack(&["bundle", &serve_once(expired), "--key", &key, "--out", out]);
	assert!(!gone.status.success());
	assert!(stderr(&gone).contains("past expiresAt"), "{}", stderr(&gone));

	assert_eq!(fs::read(out).unwrap(), newer, "the bundled copy is still the newer one");
}

#[test]
fn bundle_refuses_an_unsigned_stack() {
	let output = stack(&["bundle", "https://stack.example/s.json", "--out", "/dev/null"]);
	assert!(!output.status.success());
	assert!(stderr(&output).contains("signed stack"), "{}", stderr(&output));
}
