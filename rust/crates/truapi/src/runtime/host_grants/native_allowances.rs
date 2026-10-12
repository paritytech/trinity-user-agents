use super::{
	AccountGrant, AuthorityError, BulletinAllowanceKey, CoreStorageKey, HostGrantPersistence,
	StatementStoreAllowanceKey,
};
use crate::{platform::normalize_product_identifier, runtime::allowances::AllowanceResource};
use parity_scale_codec::DecodeAll;
use zeroize::Zeroizing;

/// Durable cleanup independent of the currently selected wallet or stored values.
#[derive(Clone, PartialEq, Eq)]
pub enum NativeAllowanceDeletion {
	/// Remove grants when a product is uninstalled.
	Product(String),
	/// Remove grants when a wallet is deleted.
	Owner([u8; 32]),
}

impl NativeAllowanceDeletion {
	/// Whether a pending deletion prevents use of this grant.
	pub fn includes(&self, owner: [u8; 32], product_id: &str) -> bool {
		match self {
			Self::Product(product) => product == product_id,
			Self::Owner(root) => *root == owner,
		}
	}

	fn matches(&self, key: &CoreStorageKey) -> bool {
		let CoreStorageKey::NativeAllowanceKey { root_public_key, product_id, .. } = key else {
			return false;
		};
		match self {
			Self::Product(product) => product_id == product,
			Self::Owner(owner) => root_public_key == owner,
		}
	}
}

fn storage_key(
	owner: [u8; 32],
	product_id: &str,
	resource: AllowanceResource,
) -> Result<CoreStorageKey, AuthorityError> {
	if normalize_product_identifier(product_id).ok().as_deref() != Some(product_id) {
		return Err(AuthorityError::Unavailable {
			reason: "invalid allowance product identifier".to_string(),
		});
	}
	Ok(CoreStorageKey::NativeAllowanceKey {
		root_public_key: owner,
		product_id: product_id.to_string(),
		resource,
	})
}

/// Load only the selected wallet, product and resource, without reading other grants.
pub async fn native_allowance(
	storage: &HostGrantPersistence<'_>,
	owner: [u8; 32],
	product_id: &str,
	resource: AllowanceResource,
) -> Result<Option<AccountGrant>, AuthorityError> {
	let Some(secret) = storage
		.store
		.storage
		.read_core_storage(storage_key(owner, product_id, resource)?)
		.await
		.map_err(storage_error)?
	else {
		return Ok(None);
	};
	let secret = Zeroizing::new(secret);
	match resource {
		AllowanceResource::StatementStore => Ok(Some(AccountGrant::StatementStore {
			key: StatementStoreAllowanceKey::from_secret_bytes(secret.to_vec())?,
			period: None,
		})),
		AllowanceResource::Bulletin => Ok(Some(AccountGrant::Bulletin(
			BulletinAllowanceKey::from_secret_bytes(secret.to_vec())?,
		))),
	}
}

/// Replace one grant only after the host has validated the issuing activation.
pub async fn retain_native_allowance(
	storage: &HostGrantPersistence<'_>,
	session_state: &crate::host_logic::session::SessionState,
	session: &crate::host_logic::session::SessionInfo,
	revision: u64,
	product_id: &str,
	allowance: &AccountGrant,
) -> Result<(), AuthorityError> {
	let (resource, secret) = match allowance {
		AccountGrant::StatementStore { key, .. } => {
			(AllowanceResource::StatementStore, key.as_secret_bytes())
		},
		AccountGrant::Bulletin(key) => (AllowanceResource::Bulletin, key.as_secret_bytes()),
		_ => {
			return Err(AuthorityError::Unavailable {
				reason: "expected an allowance grant".to_string(),
			});
		},
	};
	let key = storage_key(session.public_key, product_id, resource)?;
	if !storage
		.store
		.session_secret_allocation_is_current(session_state, session, revision)
	{
		return Err(AuthorityError::Disconnected);
	}
	storage
		.store
		.storage
		.write_core_storage(key, secret.to_vec())
		.await
		.map_err(storage_error)
}

/// Clear matching slots even when their values cannot be decoded or decrypted.
pub async fn delete_native_allowances(
	storage: &HostGrantPersistence<'_>,
	deletion: &NativeAllowanceDeletion,
) -> Result<(), AuthorityError> {
	let keys = storage.store.storage.core_storage_keys().await.map_err(storage_error)?;
	let mut first_error = None;
	for encoded in keys.encoded_keys {
		let Ok(key) = CoreStorageKey::decode_all(&mut encoded.as_slice()) else {
			continue;
		};
		if deletion.matches(&key) &&
			let Err(error) = storage.store.storage.clear_core_storage(key).await &&
			first_error.is_none()
		{
			first_error = Some(storage_error(error));
		}
	}
	first_error.map_or(Ok(()), Err)
}

fn storage_error(error: crate::latest::GenericError) -> AuthorityError {
	AuthorityError::Unavailable { reason: error.reason }
}
