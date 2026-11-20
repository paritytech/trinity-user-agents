//! Product hard-subtree public keys persisted per paired SSO session.
//!
//! The Account Holder's answer is fixed for a `(session, product)` pair, so
//! persisting it keeps a restart from waking the wallet once per product for a
//! value that cannot have changed.
//!
//! One slot per product, holding the 32-byte key unframed. A host can read the
//! slot it already stores and derive product account addresses from it without
//! decoding anything this module owns.

use crate::platform::{CoreStorage, CoreStorageKey, ProductId, StackStorage};
use truapi::latest::GenericError;

use super::{allowances::session_storage_id, authority::AuthorityError};
use crate::host_logic::session::SessionInfo;

/// Read `product_id`'s persisted subtree public key, if any.
///
/// A slot that is not exactly 32 bytes reads as a miss, so a truncated or
/// over-written entry re-asks the wallet rather than deriving a wrong account.
pub async fn read_product_subtree(
	storage: &StackStorage<'_, impl CoreStorage + ?Sized>,
	session: &SessionInfo,
	product_id: &ProductId,
) -> Result<Option<[u8; 32]>, AuthorityError> {
	let stored = storage
		.read(product_subtree_key(session, product_id)?)
		.await
		.map_err(storage_error)?;
	Ok(stored.and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok()))
}

/// Persist `product_id`'s subtree public key.
pub async fn write_product_subtree(
	storage: &StackStorage<'_, impl CoreStorage + ?Sized>,
	session: &SessionInfo,
	product_id: &ProductId,
	public_key: [u8; 32],
) -> Result<(), AuthorityError> {
	storage
		.write(product_subtree_key(session, product_id)?, public_key.to_vec())
		.await
		.map_err(storage_error)
}

/// Drop `product_id`'s persisted subtree key.
pub async fn remove_product_subtree(
	storage: &StackStorage<'_, impl CoreStorage + ?Sized>,
	session: &SessionInfo,
	product_id: &ProductId,
) -> Result<(), AuthorityError> {
	storage
		.clear(product_subtree_key(session, product_id)?)
		.await
		.map_err(storage_error)
}

fn product_subtree_key(
	session: &SessionInfo,
	product_id: &ProductId,
) -> Result<CoreStorageKey, AuthorityError> {
	Ok(CoreStorageKey::ProductSubtree {
		session_id: session_storage_id(session.sso.as_ref().ok_or(AuthorityError::Disconnected)?),
		product_id: product_id.clone(),
	})
}

fn storage_error(err: GenericError) -> AuthorityError {
	AuthorityError::Unknown { reason: format!("product subtree storage failed: {}", err.reason) }
}

#[cfg(test)]
mod tests;
