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

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        Some(Self {
            root: versions.parent()?.to_path_buf(),
        })
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
        fs::write(
            self.root.join(CHECK_STATE_FILE),
            serde_json::to_vec(&state)?,
        )
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
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
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()?;
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
    let checksum = fetch_text(
        &client,
        &asset_url(&base, published, &format!("{name}.sha256")),
    )
    .await?;
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
        }
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
        Outcome::Unmanaged => bail!(
            "truapi-host {CURRENT_VERSION} was not installed by the installer, so it \
             cannot update itself.\nInstall a managed copy with:\n  curl -fsSL \
             {INSTALLER_URL} | bash"
        ),
        Outcome::Disabled => {
            println!("Updates are disabled by TRUAPI_HOST_NO_UPDATE.");
        }
        Outcome::Busy => {
            println!("Another truapi-host is already updating; try again shortly.");
        }
        // A forced check never throttles; handled so a CLI cannot panic.
        Outcome::Throttled => println!("Checked recently; try again later."),
        Outcome::UpToDate(version) => println!("truapi-host {version} is up to date."),
        Outcome::Installed(version) => {
            println!("Installed truapi-host {version}; restart to use it.");
        }
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
    if let Some(version) = install.active_version()
        && version != CURRENT_VERSION
    {
        tracing::info!("truapi-host {version} is installed; restart to use it");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    use std::os::unix::fs::PermissionsExt;

    /// A release archive holding one executable `truapi-host`.
    fn archive_of(body: &str) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        builder
            .append_data(&mut header, BINARY, body.as_bytes())
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn digest_of(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn install_at(root: &Path) -> ManagedInstall {
        ManagedInstall {
            root: root.to_path_buf(),
        }
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
        symlink(
            &Path::new("versions").join(CURRENT_VERSION),
            &root.join("current"),
        )
        .unwrap();

        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        symlink(&root.join("current").join(BINARY), &bin.join(BINARY)).unwrap();

        assert_eq!(
            ManagedInstall::from_executable(&bin.join(BINARY)),
            Some(ManagedInstall { root })
        );
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
        assert_eq!(
            fs::read_link(install.current_link()).unwrap(),
            Path::new("versions/0.10.0")
        );
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
        assert_eq!(
            install.last_check(),
            0,
            "corrupt state must not wedge updates off"
        );
    }
}
