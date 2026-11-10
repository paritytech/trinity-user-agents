use super::*;

use std::os::unix::fs::PermissionsExt;

/// A release archive holding one executable `truapi-host`.
fn archive_of(body: &str) -> Vec<u8> {
	let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
	let mut builder = tar::Builder::new(encoder);
	let mut header = tar::Header::new_gnu();
	header.set_size(body.len() as u64);
	header.set_mode(0o755);
	builder.append_data(&mut header, BINARY, body.as_bytes()).unwrap();
	builder.into_inner().unwrap().finish().unwrap()
}

fn digest_of(bytes: &[u8]) -> String {
	hex::encode(Sha256::digest(bytes))
}

fn install_at(root: &Path) -> ManagedInstall {
	ManagedInstall { root: root.to_path_buf() }
}

#[test]
fn asset_urls_percent_encode_the_release_tag() {
	assert_eq!(
		asset_url("https://example.test", "0.10.0", "archive.tar.gz"),
		"https://example.test/releases/download/%40parity%2Ftruapi%400.10.0/archive.tar.gz"
	);
}

/// The query parameter is what makes a pointer move visible promptly; the
/// CDN otherwise serves the previous release for minutes.
#[test]
fn the_stable_pointer_is_read_past_the_cdn_cache() {
	assert_eq!(
		stable_url("https://example.test", 1_700_000_000),
		"https://example.test/releases/download/truapi-host-cli-stable/version\
         ?t=1700000000"
	);
}

/// The updater and `scripts/truapi-host-installer.sh` have to agree on this
/// name, or an update downloads a 404 page.
#[test]
fn archive_names_match_the_installer() {
	assert_eq!(
		archive_name("0.10.0", "aarch64-apple-darwin"),
		"truapi-host-0.10.0-aarch64-apple-darwin.tar.gz"
	);
}

#[test]
fn the_installer_layout_is_recognized_as_managed() {
	assert_eq!(
		ManagedInstall::from_executable(Path::new(
			"/home/dev/.local/share/truapi-host/versions/0.10.0/truapi-host"
		)),
		Some(install_at(Path::new("/home/dev/.local/share/truapi-host")))
	);
}

/// The layout the installer actually creates: the PATH entry points at
/// `current/truapi-host`, and `current` points at a version directory. On
/// macOS `current_exe` hands back that PATH symlink verbatim, so detection
/// has to resolve it or every installed copy looks unmanaged.
#[test]
fn a_binary_reached_through_the_path_symlink_is_managed() {
	let install = tempfile::tempdir().unwrap();
	let root = fs::canonicalize(install.path()).unwrap();
	let version_dir = root.join("versions").join(CURRENT_VERSION);
	fs::create_dir_all(&version_dir).unwrap();
	fs::write(version_dir.join(BINARY), "binary").unwrap();
	symlink(&Path::new("versions").join(CURRENT_VERSION), &root.join("current")).unwrap();

	let bin = root.join("bin");
	fs::create_dir_all(&bin).unwrap();
	symlink(&root.join("current").join(BINARY), &bin.join(BINARY)).unwrap();

	assert_eq!(ManagedInstall::from_executable(&bin.join(BINARY)), Some(ManagedInstall { root }));
}

/// A `cargo install` or `cargo build` binary must never be replaced.
#[test]
fn unmanaged_binaries_are_left_alone() {
	for path in [
		"/home/dev/.cargo/bin/truapi-host",
		"/home/dev/src/trinity-user-agents/target/release/truapi-host",
		"/usr/local/bin/truapi-host",
	] {
		assert_eq!(
			ManagedInstall::from_executable(Path::new(path)),
			None,
			"{path} must not be treated as a managed install"
		);
	}
}

#[test]
fn installing_publishes_the_version_and_activates_it() {
	let root = tempfile::tempdir().unwrap();
	let install = install_at(root.path());
	let archive = archive_of("binary 0.10.0");

	install_archive(&install, "0.10.0", &archive, &digest_of(&archive)).unwrap();

	let binary = install.version_dir("0.10.0").join(BINARY);
	assert_eq!(fs::read_to_string(&binary).unwrap(), "binary 0.10.0");
	assert!(fs::metadata(&binary).unwrap().permissions().mode() & 0o111 != 0);
	assert_eq!(fs::read_link(install.current_link()).unwrap(), Path::new("versions/0.10.0"));
	assert_eq!(install.active_version().as_deref(), Some("0.10.0"));
}

/// Named after the running version rather than a literal, so this keeps
/// testing the real invariant after the crate version is bumped.
#[test]
fn installing_a_second_version_swaps_current_but_keeps_the_running_one() {
	let root = tempfile::tempdir().unwrap();
	let install = install_at(root.path());
	let running = archive_of("running");
	let next = archive_of("next");

	install_archive(&install, CURRENT_VERSION, &running, &digest_of(&running)).unwrap();
	install_archive(&install, "99.0.0", &next, &digest_of(&next)).unwrap();

	assert_eq!(install.active_version().as_deref(), Some("99.0.0"));
	// The running process keeps executing out of its own version directory,
	// so activating a new one must not delete the version it came from.
	assert!(
		install.version_dir(CURRENT_VERSION).join(BINARY).is_file(),
		"the running version must survive an update"
	);
}

#[test]
fn a_digest_mismatch_installs_nothing_and_keeps_the_active_version() {
	let root = tempfile::tempdir().unwrap();
	let install = install_at(root.path());
	let good = archive_of("binary 0.10.0");
	install_archive(&install, "0.10.0", &good, &digest_of(&good)).unwrap();

	let tampered = archive_of("malicious 0.10.1");
	let error = install_archive(&install, "0.10.1", &tampered, &digest_of(&good))
		.expect_err("a mismatched digest must be refused");

	assert!(error.to_string().contains("checksum"), "{error}");
	assert!(!install.version_dir("0.10.1").exists());
	assert_eq!(install.active_version().as_deref(), Some("0.10.0"));
}

#[test]
fn pruning_keeps_only_the_named_versions() {
	let root = tempfile::tempdir().unwrap();
	let install = install_at(root.path());
	for version in ["0.9.0", "0.10.0", "0.10.1"] {
		fs::create_dir_all(install.version_dir(version)).unwrap();
	}

	prune_versions(&install, &["0.10.1", "0.10.0"]);

	assert!(!install.version_dir("0.9.0").exists());
	assert!(install.version_dir("0.10.0").exists());
	assert!(install.version_dir("0.10.1").exists());
}

#[test]
fn checks_are_throttled_to_the_interval() {
	let interval = CHECK_INTERVAL.as_secs();
	assert!(!should_check(1_000, 1_000 + interval - 1));
	assert!(should_check(1_000, 1_000 + interval));
	// A never-checked install checks on its first run.
	assert!(should_check(0, now_seconds()));
}

/// A clock that jumped backwards would otherwise wedge checks off for as
/// long as the skew lasts.
#[test]
fn a_last_check_in_the_future_is_treated_as_stale() {
	assert!(should_check(9_000, 1_000));
}

#[test]
fn a_recorded_check_round_trips_through_the_state_file() {
	let root = tempfile::tempdir().unwrap();
	let install = install_at(root.path());

	assert_eq!(install.last_check(), 0, "an empty root has never checked");
	install.record_check(1_234).unwrap();
	assert_eq!(install.last_check(), 1_234);

	fs::write(root.path().join(CHECK_STATE_FILE), "not json").unwrap();
	assert_eq!(install.last_check(), 0, "corrupt state must not wedge updates off");
}
