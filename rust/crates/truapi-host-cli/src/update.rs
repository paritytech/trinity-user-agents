//! Self-update for installs made by `scripts/truapi-host-installer.sh`.
//!
//! The installer lays out `<root>/versions/<version>/truapi-host` and points
//! `<root>/current` at the active version, with the user's `PATH` entry
//! symlinked to `<root>/current/truapi-host`. Updating therefore only has to
//! unpack a new version directory and move `current`, which leaves the running
//! process untouched and takes effect on the next run.
//!
//! Anything not laid out that way — a `cargo install` copy, a `cargo build`
//! target directory, a distro package — is left alone.

use std::{
	fs::{self, File, OpenOptions},
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Version this binary was built at.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Installed file name, matching what the release archives contain.
const BINARY: &str = "truapi-host";

/// Release host. `TRUAPI_HOST_RELEASE_BASE_URL` overrides it for mirrors and tests.
const DEFAULT_BASE_URL: &str = "https://github.com/paritytech/trinity-user-agents";

/// Rolling release whose `version` asset names the current stable version.
const STABLE_TAG: &str = "truapi-host-cli-stable";

/// How long a recorded check suppresses the next one.
const CHECK_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);

/// Budget for each release request. Generous, because the archive is several MB
/// and may be fetched over a slow link.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// How long the process waits at exit for a check it started. A command holds
/// itself open so the download it began actually lands, but a stalled network
/// must not keep a CLI open indefinitely.
const FINISH_BUDGET: Duration = Duration::from_secs(150);

/// Where the throttle timestamp lives, relative to the install root.
const CHECK_STATE_FILE: &str = "update-check.json";

/// Cross-process update lock, relative to the install root.
const LOCK_FILE: &str = "update.lock";

/// One-liner that installs or repairs a managed install.
const INSTALLER_URL: &str = "https://raw.githubusercontent.com/paritytech/trinity-user-agents/main/scripts/truapi-host-installer.sh";

/// An install laid out by the installer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedInstall {
	root: PathBuf,
}

impl ManagedInstall {
	/// Resolve from the running executable, or `None` when this binary was not
	/// put in place by the installer.
	fn detect() -> Option<Self> {
		Self::from_executable(&std::env::current_exe().ok()?)
	}

	/// The install root of `executable`, when it sits in a version directory.
	///
	/// The path is resolved first. Linux reports `current_exe` through
	/// `/proc/self/exe`, which is already resolved, but macOS reports the path
	/// as invoked, so a binary started through the `PATH` symlink names that
	/// symlink instead of the version directory it points into.
	fn from_executable(executable: &Path) -> Option<Self> {
		let resolved = fs::canonicalize(executable).unwrap_or_else(|_| executable.to_path_buf());
		let versions = resolved.parent()?.parent()?;
		if versions.file_name()? != "versions" {
			return None;
		}
		Some(Self { root: versions.parent()?.to_path_buf() })
	}

	/// Version `current` selects, which is the version the next run will use.
	fn active_version(&self) -> Option<String> {
		let target = fs::read_link(self.current_link()).ok()?;
		Some(target.file_name()?.to_str()?.to_owned())
	}

	fn versions_dir(&self) -> PathBuf {
		self.root.join("versions")
	}

	fn version_dir(&self, version: &str) -> PathBuf {
		self.versions_dir().join(version)
	}

	fn current_link(&self) -> PathBuf {
		self.root.join("current")
	}

	/// Seconds since the Unix epoch of the last attempted check. Unreadable or
	/// malformed state reads as "never checked" so a corrupt file cannot wedge
	/// updates off permanently.
	fn last_check(&self) -> u64 {
		fs::read_to_string(self.root.join(CHECK_STATE_FILE))
			.ok()
			.and_then(|raw| serde_json::from_str::<CheckState>(&raw).ok())
			.map_or(0, |state| state.last_check)
	}

	/// Record a check attempt. Called *before* the network request, so an
	/// unreachable release host is not retried on every invocation.
	fn record_check(&self, now: u64) -> Result<()> {
		fs::create_dir_all(&self.root)?;
		let state = CheckState { last_check: now };
		fs::write(self.root.join(CHECK_STATE_FILE), serde_json::to_vec(&state)?)
			.context("record the update check timestamp")
	}
}

/// Throttle state persisted across runs.
#[derive(Debug, Default, Serialize, Deserialize)]
struct CheckState {
	/// Seconds since the Unix epoch when a check was last attempted.
	last_check: u64,
}

/// What a check did. The background task and the `update` command report the
/// same outcomes differently.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
	/// Not installed by the installer, so there is nothing to manage.
	Unmanaged,
	/// Turned off through `TRUAPI_HOST_NO_UPDATE`.
	Disabled,
	/// Checked recently enough that this run skipped the network.
	Throttled,
	/// Another process holds the update lock.
	Busy,
	/// The published version is already the one the next run will use.
	UpToDate(String),
	/// A new version was installed and takes effect on the next run.
	Installed(String),
}

/// Prebuilt target triple for this machine, or `None` where no binary is
/// published. Derived from OS and architecture alone, exactly as the shell
/// installer derives it from `uname`, so a locally built binary sitting in a
/// managed install still updates to the published artifact.
fn target_triple() -> Option<&'static str> {
	match (std::env::consts::OS, std::env::consts::ARCH) {
		("macos", "aarch64") => Some("aarch64-apple-darwin"),
		("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
		("linux", "aarch64") => Some("aarch64-unknown-linux-musl"),
		_ => None,
	}
}

fn base_url() -> String {
	std::env::var("TRUAPI_HOST_RELEASE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned())
}

/// Archive name published for `version` on `target`.
fn archive_name(version: &str, target: &str) -> String {
	format!("{BINARY}-{version}-{target}.tar.gz")
}

/// URL of a release asset. The tag is `@parity/truapi@<version>`, whose `@` and
/// `/` have to be percent-encoded to address its assets.
fn asset_url(base: &str, version: &str, name: &str) -> String {
	format!("{base}/releases/download/%40parity%2Ftruapi%40{version}/{name}")
}

/// URL of the asset naming the current stable version.
///
/// Carries a changing query parameter because GitHub's asset CDN keeps serving
/// a replaced asset for some minutes, and a `Cache-Control` request header does
/// not defeat it. Without this a release goes unnoticed until the edge expires.
fn stable_url(base: &str, cache_buster: u64) -> String {
	format!("{base}/releases/download/{STABLE_TAG}/version?t={cache_buster}")
}

/// Whether a check is due, given the last attempt and the current time, both in
/// seconds since the Unix epoch. A timestamp in the future means the clock
/// moved backwards, which counts as stale rather than blocking checks until the
/// clock catches up.
fn should_check(last_check: u64, now: u64) -> bool {
	now < last_check || now - last_check >= CHECK_INTERVAL.as_secs()
}

fn now_seconds() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// Unpack a verified archive into `versions/<version>` and point `current` at it.
fn install_archive(
	install: &ManagedInstall,
	version: &str,
	archive: &[u8],
	expected_sha256: &str,
) -> Result<()> {
	let actual = hex::encode(Sha256::digest(archive));
	if !actual.eq_ignore_ascii_case(expected_sha256) {
		bail!("checksum mismatch: expected {expected_sha256}, got {actual}");
	}

	let versions = install.versions_dir();
	fs::create_dir_all(&versions)?;

	// Unpack beside the destination so publishing it is a rename within one
	// filesystem, and a killed process leaves only this staging directory.
	let staging = versions.join(format!(".{version}.incoming"));
	let _ = fs::remove_dir_all(&staging);
	tar::Archive::new(flate2::read::GzDecoder::new(archive))
		.unpack(&staging)
		.with_context(|| format!("unpack the {version} archive"))?;

	let unpacked = staging.join(BINARY);
	if !unpacked.is_file() {
		let _ = fs::remove_dir_all(&staging);
		bail!("the {version} archive does not contain {BINARY}");
	}
	make_executable(&unpacked)?;

	let destination = install.version_dir(version);
	let _ = fs::remove_dir_all(&destination);
	fs::rename(&staging, &destination)
		.with_context(|| format!("publish {}", destination.display()))?;

	point_current_at(install, version)?;
	prune_versions(install, &[version, CURRENT_VERSION]);
	Ok(())
}

/// Move `current` onto `version`. `rename` over the existing symlink is atomic,
/// so a concurrent process always sees one version or the other.
fn point_current_at(install: &ManagedInstall, version: &str) -> Result<()> {
	let staged_link = install.root.join(format!(".current.{version}"));
	let _ = fs::remove_file(&staged_link);
	symlink(&Path::new("versions").join(version), &staged_link)?;
	fs::rename(&staged_link, install.current_link()).context("activate the new version")
}

/// Delete every installed version except those named in `keep`.
fn prune_versions(install: &ManagedInstall, keep: &[&str]) {
	let Ok(entries) = fs::read_dir(install.versions_dir()) else {
		return;
	};
	for entry in entries.flatten() {
		let name = entry.file_name();
		if !keep.iter().any(|kept| name == **kept) {
			let _ = fs::remove_dir_all(entry.path());
		}
	}
}

#[cfg(unix)]
fn symlink(original: &Path, link: &Path) -> Result<()> {
	std::os::unix::fs::symlink(original, link).with_context(|| format!("link {}", link.display()))
}

#[cfg(not(unix))]
fn symlink(_original: &Path, _link: &Path) -> Result<()> {
	bail!("self-update needs a platform with POSIX symlinks")
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
	use std::os::unix::fs::PermissionsExt;

	let mut permissions = fs::metadata(path)?.permissions();
	permissions.set_mode(permissions.mode() | 0o755);
	fs::set_permissions(path, permissions)
		.with_context(|| format!("mark {} executable", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
	Ok(())
}

/// Cross-process guard so concurrent hosts do not download the same release.
struct UpdateLock {
	file: File,
}

impl UpdateLock {
	/// `None` when another process is already updating; that process's result
	/// is picked up on the next run, so waiting here would buy nothing.
	fn try_acquire(install: &ManagedInstall) -> Result<Option<Self>> {
		fs::create_dir_all(&install.root)?;
		let file = OpenOptions::new()
			.create(true)
			.truncate(false)
			.read(true)
			.write(true)
			.open(install.root.join(LOCK_FILE))?;
		match file.try_lock_exclusive() {
			Ok(()) => Ok(Some(Self { file })),
			Err(_) => Ok(None),
		}
	}
}

impl Drop for UpdateLock {
	fn drop(&mut self) {
		let _ = self.file.unlock();
	}
}

async fn fetch_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
	let response = client
		.get(url)
		.send()
		.await
		.with_context(|| format!("request {url}"))?
		.error_for_status()
		.with_context(|| format!("fetch {url}"))?;
	Ok(response.bytes().await?.to_vec())
}

async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String> {
	Ok(String::from_utf8(fetch_bytes(client, url).await?)?)
}

/// Check the published version and install it when the next run would not
/// already use it. `force` skips the throttle.
async fn check_and_install(force: bool) -> Result<Outcome> {
	if std::env::var_os("TRUAPI_HOST_NO_UPDATE").is_some() {
		return Ok(Outcome::Disabled);
	}
	let (Some(install), Some(target)) = (ManagedInstall::detect(), target_triple()) else {
		return Ok(Outcome::Unmanaged);
	};
	let Some(_lock) = UpdateLock::try_acquire(&install)? else {
		return Ok(Outcome::Busy);
	};

	let now = now_seconds();
	if !force && !should_check(install.last_check(), now) {
		return Ok(Outcome::Throttled);
	}
	install.record_check(now)?;

	let base = base_url();
	let client = reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build()?;
	let published = fetch_text(&client, &stable_url(&base, now)).await?;
	let published = published.trim();
	if published.is_empty() {
		bail!("the stable release pointer is empty");
	}

	// The active version, not the running one, decides: it is what the next run
	// executes, and comparing against the pointer rather than ordering versions
	// lets a bad release be rolled back by rewriting the pointer.
	if install.active_version().as_deref() == Some(published) {
		return Ok(Outcome::UpToDate(published.to_owned()));
	}

	// An earlier run may have unpacked this version without activating it.
	if install.version_dir(published).join(BINARY).is_file() {
		point_current_at(&install, published)?;
		prune_versions(&install, &[published, CURRENT_VERSION]);
		return Ok(Outcome::Installed(published.to_owned()));
	}

	// Announced because the process waits for this before exiting, and a silent
	// pause on an otherwise quick command reads as a hang.
	tracing::info!("downloading truapi-host {published}");
	let name = archive_name(published, target);
	let archive = fetch_bytes(&client, &asset_url(&base, published, &name)).await?;
	let checksum =
		fetch_text(&client, &asset_url(&base, published, &format!("{name}.sha256"))).await?;
	let expected = checksum
		.split_whitespace()
		.next()
		.context("the published checksum is empty")?
		.to_owned();

	// Verifying and unpacking the archive is enough filesystem work to stall a
	// runtime worker, and the terminal UI is driven by the same runtime.
	let version = published.to_owned();
	let target_install = install.clone();
	tokio::task::spawn_blocking(move || {
		install_archive(&target_install, &version, &archive, &expected)
	})
	.await
	.context("install the downloaded release")??;

	Ok(Outcome::Installed(published.to_owned()))
}

/// Throttled background check, spawned alongside every other command.
///
/// Failures are logged at debug level on purpose: an unreachable release host
/// must never disrupt the command the user actually asked for.
pub async fn run_background_check() {
	match check_and_install(false).await {
		Ok(Outcome::Installed(version)) => {
			tracing::info!("truapi-host {version} installed; restart to use it");
		},
		Ok(outcome) => tracing::debug!("update check: {outcome:?}"),
		Err(error) => tracing::debug!("update check failed: {error:#}"),
	}
}

/// Wait for a check spawned by `run_background_check` to finish.
///
/// A short command would otherwise exit while its own download was still in
/// flight, so an install could take several attempts to land. Bounded, because
/// a stalled network must not hold the CLI open; a download cut short here
/// leaves only a staging directory that the next attempt clears.
pub async fn finish_background_check(check: tokio::task::JoinHandle<()>) {
	if tokio::time::timeout(FINISH_BUDGET, check).await.is_err() {
		tracing::info!("update still downloading; leaving it for the next run");
	}
}

/// `truapi-host update`: the same work, reported on stdout and failing loudly.
pub async fn run_update_command() -> Result<()> {
	match check_and_install(true).await? {
		Outcome::Unmanaged => {
			bail!(
				"truapi-host {CURRENT_VERSION} was not installed by the installer, so it \
             cannot update itself.\nInstall a managed copy with:\n  curl -fsSL \
             {INSTALLER_URL} | bash"
			)
		},
		Outcome::Disabled => {
			println!("Updates are disabled by TRUAPI_HOST_NO_UPDATE.");
		},
		Outcome::Busy => {
			println!("Another truapi-host is already updating; try again shortly.");
		},
		// A forced check never throttles; handled so a CLI cannot panic.
		Outcome::Throttled => println!("Checked recently; try again later."),
		Outcome::UpToDate(version) => println!("truapi-host {version} is up to date."),
		Outcome::Installed(version) => {
			println!("Installed truapi-host {version}; restart to use it.");
		},
	}
	Ok(())
}

/// One line at startup describing this install.
///
/// A local build says so and shows the way back to a prebuilt copy, because it
/// silently never updates and is otherwise indistinguishable from one.
pub fn report_install() {
	let Some(install) = ManagedInstall::detect() else {
		tracing::info!(
			"truapi-host {CURRENT_VERSION} (local build), not auto-updating. \
             Prebuilt release: curl -fsSL {INSTALLER_URL} | bash"
		);
		return;
	};
	if let Some(version) = install.active_version() &&
		version != CURRENT_VERSION
	{
		tracing::info!("truapi-host {version} is installed; restart to use it");
	}
}

#[cfg(all(test, unix))]
mod tests;
