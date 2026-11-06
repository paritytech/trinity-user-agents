//! Retained allowance scopes and paired-session storage encoding.

use crate::platform::{CoreStorage, CoreStorageKey, ProductId};
use parity_scale_codec::{Decode, Encode};
use truapi::latest::GenericError;

use super::{authority::AuthorityError, sso_remote::SsoSessionKey};
use crate::host_logic::session::{SessionInfo, SsoSessionInfo};

/// Chain resource an allowance key grants access to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum AllowanceResource {
	/// Bulletin-chain transaction storage.
	Bulletin,
	/// People-chain statement store.
	StatementStore,
}

/// Canonical wallet and optional paired channel owning in-memory grants.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct GrantScope {
	root_public_key: [u8; 32],
	channel: Option<SsoSessionKey>,
}

impl GrantScope {
	/// Use session identity directly, without interpreting an authority token.
	pub fn from_session(session: &SessionInfo) -> Self {
		Self {
			root_public_key: session.public_key,
			channel: session.sso.as_ref().map(SsoSessionKey::from_session),
		}
	}

	/// Whether this owner remains selected by the canonical session.
	pub fn matches(self, state: &crate::host_logic::session::SessionState) -> bool {
		state
			.current()
			.as_ref()
			.is_some_and(|session| Self::from_session(session) == self)
	}
}

/// Memory-cache key: `(session, product_id, resource)`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct AllowanceCacheKey {
	session: GrantScope,
	product_id: ProductId,
	resource: AllowanceResource,
}

impl AllowanceCacheKey {
	/// Key for the wallet and authenticated channel that received this grant.
	pub fn new(session: &SessionInfo, product_id: &ProductId, resource: AllowanceResource) -> Self {
		Self {
			session: GrantScope::from_session(session),
			product_id: product_id.clone(),
			resource,
		}
	}

	/// Whether this key belongs to the given SSO session.
	pub fn is_for_session(&self, session: GrantScope) -> bool {
		self.session == session
	}

	/// Whether this key belongs to the given product.
	pub fn is_for_product(&self, product_id: &ProductId) -> bool {
		&self.product_id == product_id
	}
}

#[derive(Clone, Encode, Decode)]
struct StoredAllowanceEntry {
	product_id: ProductId,
	resource: AllowanceResource,
	slot_account_key: Vec<u8>,
}

/// Read the persisted allowance key for `(product_id, resource)`, if any.
pub async fn read_allowance_key(
	storage: &(impl CoreStorage + ?Sized),
	session: &SessionInfo,
	product_id: &ProductId,
	resource: AllowanceResource,
) -> Result<Option<Vec<u8>>, AuthorityError> {
	let entries = read_entries(storage, session).await?;
	Ok(entries
		.into_iter()
		.find(|entry| &entry.product_id == product_id && entry.resource == resource)
		.map(|entry| entry.slot_account_key))
}

/// Persist an allowance key, replacing any prior key for the same
/// `(product_id, resource)`.
pub async fn write_allowance_key(
	storage: &(impl CoreStorage + ?Sized),
	session: &SessionInfo,
	product_id: &ProductId,
	resource: AllowanceResource,
	slot_account_key: Vec<u8>,
) -> Result<(), AuthorityError> {
	let mut entries = read_entries(storage, session).await?;
	entries.retain(|entry| !(&entry.product_id == product_id && entry.resource == resource));
	entries.push(StoredAllowanceEntry {
		product_id: product_id.clone(),
		resource,
		slot_account_key,
	});
	storage
		.write_core_storage(storage_key(session)?, encode_entries(entries))
		.await
		.map_err(storage_error)
}

/// Remove the persisted allowance key for `(product_id, resource)`; a miss is
/// not an error.
pub async fn remove_allowance_key(
	storage: &(impl CoreStorage + ?Sized),
	session: &SessionInfo,
	product_id: &ProductId,
	resource: AllowanceResource,
) -> Result<(), AuthorityError> {
	let mut entries = read_entries(storage, session).await?;
	let before = entries.len();
	entries.retain(|entry| !(&entry.product_id == product_id && entry.resource == resource));
	if entries.len() == before {
		return Ok(());
	}
	storage
		.write_core_storage(storage_key(session)?, encode_entries(entries))
		.await
		.map_err(storage_error)
}

/// Remove every persisted allowance key for `product_id` in the active SSO
/// session while preserving entries owned by other products.
pub async fn clear_product_allowance_keys(
	storage: &(impl CoreStorage + ?Sized),
	session: &SessionInfo,
	product_id: &ProductId,
) -> Result<(), AuthorityError> {
	let key = storage_key(session)?;
	let Some(blob) = storage.read_core_storage(key.clone()).await.map_err(storage_error)? else {
		return Ok(());
	};
	let mut entries = match decode_entries(&blob) {
		Ok(entries) => entries,
		Err(_) => {
			return storage.clear_core_storage(key).await.map_err(storage_error);
		},
	};
	let before = entries.len();
	entries.retain(|entry| &entry.product_id != product_id);
	if entries.len() == before {
		return Ok(());
	}
	if entries.is_empty() {
		storage.clear_core_storage(key).await.map_err(storage_error)
	} else {
		storage
			.write_core_storage(key, encode_entries(entries))
			.await
			.map_err(storage_error)
	}
}

/// Drop every persisted allowance key belonging to the session.
pub async fn clear_session_allowance_keys(
	storage: &(impl CoreStorage + ?Sized),
	session: &SessionInfo,
) -> Result<(), AuthorityError> {
	storage.clear_core_storage(storage_key(session)?).await.map_err(storage_error)
}

async fn read_entries(
	storage: &(impl CoreStorage + ?Sized),
	session: &SessionInfo,
) -> Result<Vec<StoredAllowanceEntry>, AuthorityError> {
	let Some(blob) =
		storage.read_core_storage(storage_key(session)?).await.map_err(storage_error)?
	else {
		return Ok(Vec::new());
	};
	decode_entries(&blob)
}

fn encode_entries(entries: Vec<StoredAllowanceEntry>) -> Vec<u8> {
	entries.encode()
}

fn decode_entries(blob: &[u8]) -> Result<Vec<StoredAllowanceEntry>, AuthorityError> {
	let mut input = blob;
	let entries = Vec::<StoredAllowanceEntry>::decode(&mut input).map_err(|err| {
		AuthorityError::Unknown { reason: format!("invalid persisted allowance keys: {err}") }
	})?;
	if !input.is_empty() {
		return Err(AuthorityError::Unknown {
			reason: "invalid persisted allowance keys: trailing bytes".to_string(),
		});
	}
	Ok(entries)
}

fn storage_key(session: &SessionInfo) -> Result<CoreStorageKey, AuthorityError> {
	Ok(CoreStorageKey::AllowanceKeys {
		session_id: session_storage_id(session.sso.as_ref().ok_or(AuthorityError::Disconnected)?),
	})
}

pub fn session_storage_id(session: &SsoSessionInfo) -> String {
	let mut bytes = Vec::with_capacity(64);
	bytes.extend_from_slice(&session.session_id_own);
	bytes.extend_from_slice(&session.session_id_peer);
	hex::encode(bytes)
}

fn storage_error(err: GenericError) -> AuthorityError {
	AuthorityError::Unknown { reason: format!("allowance storage failed: {}", err.reason) }
}

#[cfg(test)]
mod tests;
