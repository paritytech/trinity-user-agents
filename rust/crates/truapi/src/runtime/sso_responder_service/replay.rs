use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, Weak};

use crate::platform::{CoreStorage, CoreStorageKey};
use futures::lock::Mutex as AsyncMutex;
use parity_scale_codec::{Decode, Encode};

use crate::runtime::sso_remote::DEFAULT_SSO_STATEMENT_EXPIRY_SECS;

pub const MAX_REQUEST_LEDGER_ENTRIES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SsoReplayScope {
    pub root_public_key: [u8; 32],
    pub peer_statement_account_id: [u8; 32],
    pub peer_encryption_public_key: [u8; 32],
}

impl SsoReplayScope {
    fn storage_key(self) -> CoreStorageKey {
        CoreStorageKey::SsoResponderRequestLedger {
            root_public_key: self.root_public_key,
            peer_statement_account_id: self.peer_statement_account_id,
            peer_encryption_public_key: self.peer_encryption_public_key,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReplayExecution<T> {
    Duplicate,
    Executed(T),
}

#[derive(Default)]
pub struct SsoReplayLocks {
    scopes: Mutex<HashMap<SsoReplayScope, Weak<AsyncMutex<()>>>>,
}

impl SsoReplayLocks {
    fn lock_for(&self, scope: SsoReplayScope) -> Arc<AsyncMutex<()>> {
        let mut scopes = self.scopes.lock().expect("SSO replay lock map poisoned");
        scopes.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = scopes.get(&scope).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(AsyncMutex::new(()));
        scopes.insert(scope, Arc::downgrade(&lock));
        lock
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
struct RequestLedger {
    entries: Vec<RequestLedgerEntry>,
}

impl RequestLedger {
    fn contains(&mut self, request_id: &str, now_unix_secs: u64) -> bool {
        self.prune(now_unix_secs);
        self.entries
            .iter()
            .any(|entry| entry.request_id == request_id)
    }

    fn start(
        &mut self,
        request_id: String,
        expires_at_unix_secs: Option<u64>,
        now_unix_secs: u64,
    ) -> Result<(), String> {
        self.prune(now_unix_secs);
        if self
            .entries
            .iter()
            .any(|entry| entry.request_id == request_id)
        {
            return Ok(());
        }
        if self.entries.len() >= MAX_REQUEST_LEDGER_ENTRIES {
            return Err(format!(
                "SSO replay ledger is full with {MAX_REQUEST_LEDGER_ENTRIES} unexpired requests"
            ));
        }
        let maximum_expiry = now_unix_secs.saturating_add(DEFAULT_SSO_STATEMENT_EXPIRY_SECS);
        let expires_at_unix_secs = Some(
            expires_at_unix_secs
                .unwrap_or(maximum_expiry)
                .min(maximum_expiry),
        );
        self.entries.push(RequestLedgerEntry {
            request_id,
            expires_at_unix_secs,
            state: RequestLedgerState::Started,
        });
        Ok(())
    }

    fn complete(&mut self, request_id: &str) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.request_id == request_id)
        {
            entry.state = RequestLedgerState::Completed;
        }
    }

    fn prune(&mut self, now_unix_secs: u64) -> bool {
        let previous_len = self.entries.len();
        self.entries.retain(|entry| {
            entry
                .expires_at_unix_secs
                .is_some_and(|expiry| expiry >= now_unix_secs)
        });
        self.entries.len() != previous_len
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
struct RequestLedgerEntry {
    request_id: String,
    expires_at_unix_secs: Option<u64>,
    state: RequestLedgerState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
enum RequestLedgerState {
    #[codec(index = 0)]
    Started,
    #[codec(index = 1)]
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
enum VersionedRequestLedger {
    #[codec(index = 0)]
    V1(RequestLedger),
}

pub async fn execute_once<T, F, Fut>(
    storage: &(impl CoreStorage + ?Sized),
    locks: &SsoReplayLocks,
    scope: SsoReplayScope,
    request_id: &str,
    expires_at_unix_secs: Option<u64>,
    now_unix_secs: u64,
    execute: F,
) -> Result<ReplayExecution<T>, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let request_lock = locks.lock_for(scope);
    let _guard = request_lock.lock().await;
    let mut ledger = read_ledger(storage, scope).await?;
    let previous_len = ledger.entries.len();
    if ledger.contains(request_id, now_unix_secs) {
        if ledger.entries.len() != previous_len {
            write_ledger(storage, scope, &ledger).await?;
        }
        return Ok(ReplayExecution::Duplicate);
    }

    ledger.start(request_id.to_string(), expires_at_unix_secs, now_unix_secs)?;
    write_ledger(storage, scope, &ledger).await?;
    // This durable marker prevents a crash from replaying arbitrary side effects.
    // A crash after this write can drop the request or its response.
    let output = execute().await?;
    ledger.complete(request_id);
    write_ledger(storage, scope, &ledger).await?;
    Ok(ReplayExecution::Executed(output))
}

async fn read_ledger(
    storage: &(impl CoreStorage + ?Sized),
    scope: SsoReplayScope,
) -> Result<RequestLedger, String> {
    let Some(blob) = storage
        .read_core_storage(scope.storage_key())
        .await
        .map_err(|err| format!("SSO replay ledger read failed: {}", err.reason))?
    else {
        return Ok(RequestLedger::default());
    };
    let mut input = blob.as_slice();
    let VersionedRequestLedger::V1(ledger) = VersionedRequestLedger::decode(&mut input)
        .map_err(|err| format!("invalid SSO replay ledger: {err}"))?;
    if !input.is_empty() {
        return Err("invalid SSO replay ledger: trailing bytes".to_string());
    }
    Ok(ledger)
}

async fn write_ledger(
    storage: &(impl CoreStorage + ?Sized),
    scope: SsoReplayScope,
    ledger: &RequestLedger,
) -> Result<(), String> {
    storage
        .write_core_storage(
            scope.storage_key(),
            VersionedRequestLedger::V1(ledger.clone()).encode(),
        )
        .await
        .map_err(|err| format!("SSO replay ledger write failed: {}", err.reason))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::test_support::StubPlatform;

    fn scope(root: u8, peer: u8, encryption: u8) -> SsoReplayScope {
        SsoReplayScope {
            root_public_key: [root; 32],
            peer_statement_account_id: [peer; 32],
            peer_encryption_public_key: [encryption; 32],
        }
    }

    #[test]
    fn ledger_prunes_expired_entries() {
        let mut ledger = RequestLedger::default();
        ledger.start("expired".to_string(), Some(99), 50).unwrap();
        ledger.start("live".to_string(), Some(100), 50).unwrap();
        ledger.start("unbounded".to_string(), None, 50).unwrap();

        ledger.prune(100);

        assert_eq!(
            ledger,
            RequestLedger {
                entries: vec![
                    RequestLedgerEntry {
                        request_id: "live".to_string(),
                        expires_at_unix_secs: Some(100),
                        state: RequestLedgerState::Started,
                    },
                    RequestLedgerEntry {
                        request_id: "unbounded".to_string(),
                        expires_at_unix_secs: Some(50 + DEFAULT_SSO_STATEMENT_EXPIRY_SECS),
                        state: RequestLedgerState::Started,
                    },
                ],
            }
        );
    }

    #[test]
    fn ledger_bounds_missing_and_distant_expiry() {
        let mut ledger = RequestLedger::default();
        ledger.start("missing".to_string(), None, 100).unwrap();
        ledger
            .start("distant".to_string(), Some(u64::MAX), 100)
            .unwrap();
        ledger.start("near".to_string(), Some(200), 100).unwrap();

        assert_eq!(
            ledger,
            RequestLedger {
                entries: vec![
                    RequestLedgerEntry {
                        request_id: "missing".to_string(),
                        expires_at_unix_secs: Some(100 + DEFAULT_SSO_STATEMENT_EXPIRY_SECS),
                        state: RequestLedgerState::Started,
                    },
                    RequestLedgerEntry {
                        request_id: "distant".to_string(),
                        expires_at_unix_secs: Some(100 + DEFAULT_SSO_STATEMENT_EXPIRY_SECS),
                        state: RequestLedgerState::Started,
                    },
                    RequestLedgerEntry {
                        request_id: "near".to_string(),
                        expires_at_unix_secs: Some(200),
                        state: RequestLedgerState::Started,
                    },
                ],
            }
        );
    }

    #[test]
    fn legacy_unbounded_entries_do_not_block_new_requests() {
        futures::executor::block_on(async {
            let platform = StubPlatform::default();
            let request_scope = scope(1, 2, 3);
            let ledger = RequestLedger {
                entries: (0..MAX_REQUEST_LEDGER_ENTRIES)
                    .map(|index| RequestLedgerEntry {
                        request_id: format!("legacy-{index}"),
                        expires_at_unix_secs: None,
                        state: RequestLedgerState::Completed,
                    })
                    .collect(),
            };
            write_ledger(&platform, request_scope, &ledger)
                .await
                .unwrap();

            let result = execute_once(
                &platform,
                &SsoReplayLocks::default(),
                request_scope,
                "fresh",
                Some(200),
                100,
                || async { Ok::<_, String>("executed") },
            )
            .await
            .unwrap();

            assert_eq!(result, ReplayExecution::Executed("executed"));
            assert_eq!(
                read_ledger(&platform, request_scope).await.unwrap(),
                RequestLedger {
                    entries: vec![RequestLedgerEntry {
                        request_id: "fresh".to_string(),
                        expires_at_unix_secs: Some(200),
                        state: RequestLedgerState::Completed,
                    }],
                }
            );
        });
    }

    #[test]
    fn ledger_rejects_a_new_entry_at_its_bound_without_eviction() {
        let mut ledger = RequestLedger::default();
        for index in 0..MAX_REQUEST_LEDGER_ENTRIES {
            ledger.start(format!("request-{index}"), None, 0).unwrap();
        }
        let error = ledger.start("overflow".to_string(), None, 0).unwrap_err();

        assert_eq!(ledger.entries.len(), MAX_REQUEST_LEDGER_ENTRIES);
        assert!(ledger.contains("request-0", 0));
        assert!(ledger.contains(&format!("request-{}", MAX_REQUEST_LEDGER_ENTRIES - 1), 0));
        assert_eq!(
            error,
            "SSO replay ledger is full with 1024 unexpired requests"
        );
    }

    #[test]
    fn full_ledger_rejects_before_executing_the_request() {
        futures::executor::block_on(async {
            let platform = StubPlatform::default();
            let request_scope = scope(1, 2, 3);
            let mut ledger = RequestLedger::default();
            for index in 0..MAX_REQUEST_LEDGER_ENTRIES {
                ledger.start(format!("request-{index}"), None, 0).unwrap();
            }
            write_ledger(&platform, request_scope, &ledger)
                .await
                .unwrap();
            let executions = Arc::new(AtomicUsize::new(0));
            let request_executions = executions.clone();

            let error = execute_once(
                &platform,
                &SsoReplayLocks::default(),
                request_scope,
                "overflow",
                None,
                0,
                move || async move {
                    request_executions.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, String>(())
                },
            )
            .await
            .unwrap_err();

            assert_eq!(
                error,
                "SSO replay ledger is full with 1024 unexpired requests"
            );
            assert_eq!(executions.load(Ordering::SeqCst), 0);
        });
    }

    #[test]
    fn completed_request_survives_store_reconstruction() {
        futures::executor::block_on(async {
            let platform = StubPlatform::default();
            let locks = SsoReplayLocks::default();
            let request_scope = scope(1, 2, 3);
            let first = execute_once(
                &platform,
                &locks,
                request_scope,
                "request-1",
                Some(200),
                100,
                || async { Ok::<_, String>("executed") },
            )
            .await
            .unwrap();
            assert_eq!(first, ReplayExecution::Executed("executed"));
            assert_eq!(
                read_ledger(&platform, request_scope).await.unwrap(),
                RequestLedger {
                    entries: vec![RequestLedgerEntry {
                        request_id: "request-1".to_string(),
                        expires_at_unix_secs: Some(200),
                        state: RequestLedgerState::Completed,
                    }],
                }
            );

            let reconstructed_locks = SsoReplayLocks::default();
            let replay = execute_once(
                &platform,
                &reconstructed_locks,
                request_scope,
                "request-1",
                Some(200),
                101,
                || async { Ok::<_, String>("executed twice") },
            )
            .await
            .unwrap();

            assert_eq!(replay, ReplayExecution::Duplicate);
        });
    }

    #[test]
    fn started_request_is_not_executed_after_store_reconstruction() {
        futures::executor::block_on(async {
            let platform = StubPlatform::default();
            let request_scope = scope(1, 2, 3);
            let failure = execute_once(
                &platform,
                &SsoReplayLocks::default(),
                request_scope,
                "request-1",
                Some(200),
                100,
                || async { Err::<(), _>("simulated process failure".to_string()) },
            )
            .await
            .unwrap_err();
            assert_eq!(failure, "simulated process failure");
            assert_eq!(
                read_ledger(&platform, request_scope).await.unwrap(),
                RequestLedger {
                    entries: vec![RequestLedgerEntry {
                        request_id: "request-1".to_string(),
                        expires_at_unix_secs: Some(200),
                        state: RequestLedgerState::Started,
                    }],
                }
            );

            let executions = Arc::new(AtomicUsize::new(0));
            let replay_executions = executions.clone();
            let replay = execute_once(
                &platform,
                &SsoReplayLocks::default(),
                request_scope,
                "request-1",
                Some(200),
                101,
                move || async move {
                    replay_executions.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, String>(())
                },
            )
            .await
            .unwrap();

            assert_eq!(replay, ReplayExecution::Duplicate);
            assert_eq!(executions.load(Ordering::SeqCst), 0);
        });
    }

    #[test]
    fn request_ids_are_isolated_by_root_and_peer() {
        futures::executor::block_on(async {
            let platform = StubPlatform::default();
            let locks = SsoReplayLocks::default();
            let executions = Arc::new(AtomicUsize::new(0));

            for request_scope in [
                scope(1, 2, 3),
                scope(2, 2, 3),
                scope(1, 4, 3),
                scope(1, 2, 4),
            ] {
                let executions = executions.clone();
                let outcome = execute_once(
                    &platform,
                    &locks,
                    request_scope,
                    "same-request-id",
                    Some(200),
                    100,
                    move || async move {
                        executions.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, String>(())
                    },
                )
                .await
                .unwrap();
                assert_eq!(outcome, ReplayExecution::Executed(()));
            }

            assert_eq!(executions.load(Ordering::SeqCst), 4);
        });
    }

    #[test]
    fn duplicate_request_does_not_execute_side_effect_twice() {
        futures::executor::block_on(async {
            let platform = StubPlatform::default();
            let locks = SsoReplayLocks::default();
            let executions = Arc::new(AtomicUsize::new(0));

            for expected in [ReplayExecution::Executed(()), ReplayExecution::Duplicate] {
                let executions = executions.clone();
                let actual = execute_once(
                    &platform,
                    &locks,
                    scope(1, 2, 3),
                    "request-1",
                    Some(200),
                    100,
                    move || async move {
                        executions.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, String>(())
                    },
                )
                .await
                .unwrap();
                assert_eq!(actual, expected);
            }

            assert_eq!(executions.load(Ordering::SeqCst), 1);
        });
    }
}
