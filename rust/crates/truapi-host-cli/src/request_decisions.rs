//! Confirmations decided by another process through a directory
//! (`TRUAPI_DECISIONS_DIR`), one request at a time.
//!
//! A headless serve-mode host has no terminal to ask, so without
//! `--auto-accept` it denies every confirmation. With a decisions directory
//! each confirmation is published as `<id>.request.json`
//! (`{"id","action","kind","detail","at"}`) and waits for `<id>.decision`,
//! whose text is the answer the terminal prompt would take (`y`/`n` for an
//! action; `o`/`a`/`n` for a permission). Anything else denies, as the
//! terminal does. When the wait ends (answered, unreadable or timed out) the
//! host writes `<id>.decided.json` (the request plus `decision` and `reason`)
//! and removes the request and decision files, so the pending requests are
//! exactly the `*.request.json` files present.
//!
//! Files are written to a temporary name and renamed, so a reader never sees
//! half a request. Nothing here opens a socket: the directory is as private as
//! its permissions.
//!
//! A script that answers prompts can approve a signature without a person, so
//! the directory is refused at startup on any preset not listed as a test
//! network ([`ensure_test_network`]).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use truapi::platform::PermissionDecision;

use crate::network::NetworkConfig;
use crate::terminal_ui::ApprovalKind;

/// Environment variable naming the decisions directory.
pub const DECISIONS_DIR_ENV: &str = "TRUAPI_DECISIONS_DIR";
/// Environment variable bounding one wait, in milliseconds (default 10 min).
pub const DECISIONS_TIMEOUT_ENV: &str = "TRUAPI_DECISIONS_TIMEOUT_MS";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Presets on which another process may answer confirmations. `Network`'s
/// invariant (every preset is a test network with disposable identities, held
/// by `every_preset_is_a_test_network`) makes that every preset today. Naming
/// them keeps the directory off a preset added later until someone decides,
/// here, that its identities are disposable.
const TEST_NETWORK_PRESETS: &[&str] = &["paseo-next-v2", "previewnet"];

/// Refuse `TRUAPI_DECISIONS_DIR` on a preset that is not a listed test network,
/// before the host starts.
pub fn ensure_test_network(network: &NetworkConfig) -> anyhow::Result<()> {
    if std::env::var_os(DECISIONS_DIR_ENV).is_none() {
        return Ok(());
    }
    ensure_test_preset(network.id)
}

fn ensure_test_preset(id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        TEST_NETWORK_PRESETS.contains(&id),
        "{DECISIONS_DIR_ENV} is only available on the test network presets ({}); on `{id}` \
         every confirmation must be answered by hand, so unset {DECISIONS_DIR_ENV}",
        TEST_NETWORK_PRESETS.join(", ")
    );
    Ok(())
}
const POLL: Duration = Duration::from_millis(20);

/// Where and how long confirmations wait for an outside decision.
#[derive(Debug, Clone)]
pub struct RequestDecisions {
    dir: PathBuf,
    timeout: Duration,
    next_id: std::sync::Arc<AtomicU32>,
}

/// How one outside decision ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decided {
    pub id: u32,
    pub decision: PermissionDecision,
    /// `answered`, `unrecognized answer` or `timed out`.
    pub reason: &'static str,
}

impl RequestDecisions {
    /// Read `TRUAPI_DECISIONS_DIR` (and its timeout); `None` when unset.
    pub fn from_env() -> Option<Self> {
        let dir = std::env::var_os(DECISIONS_DIR_ENV).map(PathBuf::from)?;
        let timeout = std::env::var(DECISIONS_TIMEOUT_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT);
        Some(Self::new(dir, timeout))
    }

    pub fn new(dir: PathBuf, timeout: Duration) -> Self {
        Self {
            dir,
            timeout,
            next_id: std::sync::Arc::new(AtomicU32::new(1)),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Publish one confirmation and wait for its decision. A request that
    /// cannot be published is denied (the caller learns why from the log).
    pub async fn decide(&self, action: &str, detail: &str, kind: ApprovalKind) -> Decided {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = serde_json::json!({
            "id": id,
            "action": action,
            "kind": match kind {
                ApprovalKind::Action => "action",
                ApprovalKind::Permission => "permission",
            },
            "detail": detail,
            "at": unix_ms(),
        });
        let request_path = self.dir.join(format!("{id}.request.json"));
        let decision_path = self.dir.join(format!("{id}.decision"));
        if let Err(err) = fs::create_dir_all(&self.dir)
            .and_then(|()| write_atomically(&request_path, request.to_string().as_bytes()))
        {
            tracing::warn!(path = %request_path.display(), %err, "could not publish a confirmation; denying it");
            return Decided {
                id,
                decision: PermissionDecision::Deny,
                reason: "could not publish the request",
            };
        }
        let deadline = Instant::now() + self.timeout;
        let (decision, reason) = loop {
            match fs::read_to_string(&decision_path) {
                Ok(answer) => match kind.parse(&answer) {
                    Some(decision) => break (decision, "answered"),
                    None => break (PermissionDecision::Deny, "unrecognized answer"),
                },
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    tracing::warn!(path = %decision_path.display(), %err, "could not read a decision; denying");
                    break (PermissionDecision::Deny, "unreadable answer");
                }
            }
            if Instant::now() >= deadline {
                break (PermissionDecision::Deny, "timed out");
            }
            tokio::time::sleep(POLL).await;
        };
        let mut decided = request;
        decided["decision"] = serde_json::Value::from(match decision {
            PermissionDecision::AllowOnce => "allowed once",
            PermissionDecision::AllowAlways => "approved",
            PermissionDecision::Deny => "denied",
        });
        decided["reason"] = serde_json::Value::from(reason);
        let done = self.dir.join(format!("{id}.decided.json"));
        if let Err(err) = write_atomically(&done, decided.to_string().as_bytes()) {
            tracing::warn!(path = %done.display(), %err, "could not record a decided confirmation");
        }
        let _ = fs::remove_file(&request_path);
        let _ = fs::remove_file(&decision_path);
        Decided {
            id,
            decision,
            reason,
        }
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Write `bytes` to a sibling temporary file, then rename it into place.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.tmp"));
    {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn pending(dir: &Path) -> Vec<serde_json::Value> {
        let mut found: Vec<serde_json::Value> = fs::read_dir(dir)
            .expect("dir")
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let name = path.file_name()?.to_str()?.to_string();
                name.ends_with(".request.json").then(|| {
                    serde_json::from_str(&fs::read_to_string(&path).expect("request"))
                        .expect("json")
                })
            })
            .collect();
        found.sort_by_key(|value| value["id"].as_u64());
        found
    }

    async fn answer_when_pending(dir: PathBuf, answer: &'static str) -> serde_json::Value {
        for _ in 0..500 {
            if let Some(request) = pending(&dir).into_iter().next() {
                let id = request["id"].as_u64().expect("id");
                fs::write(dir.join(format!("{id}.decision")), answer).expect("answer");
                return request;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("no request was published");
    }

    #[tokio::test]
    async fn an_action_waits_for_its_answer_and_records_it() {
        let dir = tempdir().expect("tempdir");
        let decisions = RequestDecisions::new(dir.path().to_path_buf(), Duration::from_secs(5));
        let answering = tokio::spawn(answer_when_pending(dir.path().to_path_buf(), "y\n"));
        let decided = decisions
            .decide("sign raw data", "hidden", ApprovalKind::Action)
            .await;
        let request = answering.await.expect("answer task");
        assert_eq!(request["action"], "sign raw data");
        assert_eq!(request["kind"], "action");
        assert_eq!(decided.decision, PermissionDecision::AllowAlways);
        assert_eq!(decided.reason, "answered");
        assert!(
            pending(dir.path()).is_empty(),
            "the request file is removed"
        );
        assert!(
            !dir.path().join("1.decision").exists(),
            "the answer file is removed"
        );
        let done: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("1.decided.json")).expect("decided"),
        )
        .expect("json");
        assert_eq!(done["decision"], "approved");
    }

    #[tokio::test]
    async fn one_request_is_denied_while_the_next_is_approved() {
        let dir = tempdir().expect("tempdir");
        let decisions = RequestDecisions::new(dir.path().to_path_buf(), Duration::from_secs(5));
        let deny = tokio::spawn(answer_when_pending(dir.path().to_path_buf(), "n"));
        let first = decisions
            .decide("sign raw data", "hidden", ApprovalKind::Action)
            .await;
        deny.await.expect("deny task");
        let allow = tokio::spawn(answer_when_pending(dir.path().to_path_buf(), "o"));
        let second = decisions
            .decide(
                "submit chain transactions",
                "remote",
                ApprovalKind::Permission,
            )
            .await;
        let request = allow.await.expect("allow task");
        assert_eq!(first.decision, PermissionDecision::Deny);
        assert_eq!(second.decision, PermissionDecision::AllowOnce);
        assert_eq!(request["kind"], "permission");
        assert_eq!((first.id, second.id), (1, 2));
    }

    #[tokio::test]
    async fn an_unrecognized_answer_denies() {
        let dir = tempdir().expect("tempdir");
        let decisions = RequestDecisions::new(dir.path().to_path_buf(), Duration::from_secs(5));
        let answering = tokio::spawn(answer_when_pending(dir.path().to_path_buf(), "maybe"));
        let decided = decisions
            .decide("sign raw data", "hidden", ApprovalKind::Action)
            .await;
        answering.await.expect("answer task");
        assert_eq!(decided.decision, PermissionDecision::Deny);
        assert_eq!(decided.reason, "unrecognized answer");
    }

    #[tokio::test]
    async fn an_unanswered_request_times_out_denied() {
        let dir = tempdir().expect("tempdir");
        let decisions = RequestDecisions::new(dir.path().to_path_buf(), Duration::from_millis(60));
        let decided = decisions
            .decide("sign raw data", "hidden", ApprovalKind::Action)
            .await;
        assert_eq!(decided.decision, PermissionDecision::Deny);
        assert_eq!(decided.reason, "timed out");
        assert!(pending(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn an_unwritable_directory_denies_without_waiting() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("not-a-directory");
        fs::write(&file, "x").expect("file");
        let decisions = RequestDecisions::new(file.join("decisions"), Duration::from_secs(60));
        let started = Instant::now();
        let decided = decisions
            .decide("sign raw data", "hidden", ApprovalKind::Action)
            .await;
        assert_eq!(decided.decision, PermissionDecision::Deny);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn the_directory_is_refused_off_the_test_presets() {
        use clap::ValueEnum;

        for network in crate::network::Network::value_variants() {
            ensure_test_preset(network.config().id)
                .expect("every shipped preset is a test network");
        }
        let error = ensure_test_preset("polkadot").expect_err("an unlisted preset is refused");
        let message = error.to_string();
        assert!(
            message.contains(DECISIONS_DIR_ENV) && message.contains("`polkadot`"),
            "{message}"
        );
    }
}
