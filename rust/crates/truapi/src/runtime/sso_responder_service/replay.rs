use std::{
	collections::HashMap,
	future::Future,
	sync::{Arc, Mutex, Weak},
};

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
		self.entries.iter().any(|entry| entry.request_id == request_id)
	}

	fn start(
		&mut self,
		request_id: String,
		expires_at_unix_secs: Option<u64>,
		now_unix_secs: u64,
	) -> Result<(), String> {
		self.prune(now_unix_secs);
		if self.entries.iter().any(|entry| entry.request_id == request_id) {
			return Ok(());
		}
		if self.entries.len() >= MAX_REQUEST_LEDGER_ENTRIES {
			return Err(format!(
				"SSO replay ledger is full with {MAX_REQUEST_LEDGER_ENTRIES} unexpired requests"
			));
		}
		let maximum_expiry = now_unix_secs.saturating_add(DEFAULT_SSO_STATEMENT_EXPIRY_SECS);
		let expires_at_unix_secs =
			Some(expires_at_unix_secs.unwrap_or(maximum_expiry).min(maximum_expiry));
		self.entries.push(RequestLedgerEntry {
			request_id,
			expires_at_unix_secs,
			state: RequestLedgerState::Started,
		});
		Ok(())
	}

	fn complete(&mut self, request_id: &str) {
		if let Some(entry) = self.entries.iter_mut().find(|entry| entry.request_id == request_id) {
			entry.state = RequestLedgerState::Completed;
		}
	}

	fn prune(&mut self, now_unix_secs: u64) -> bool {
		let previous_len = self.entries.len();
		self.entries.retain(|entry| {
			entry.expires_at_unix_secs.is_some_and(|expiry| expiry >= now_unix_secs)
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
mod tests;
