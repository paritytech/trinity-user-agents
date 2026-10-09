//! Product hard-subtree public keys persisted per paired SSO session.
//!
//! The Account Holder's answer is fixed for a `(session, product)` pair, so
//! persisting it keeps a restart from waking the wallet once per product for a
//! value that cannot have changed.
//!
//! One slot per product, holding the 32-byte key unframed. A host can read the
//! slot it already stores and derive product account addresses from it without
//! decoding anything this module owns.

use crate::platform::{CoreStorage, CoreStorageKey};
use truapi::latest::GenericError;

use super::allowances::session_storage_id;
use super::authority::AuthorityError;
use crate::host_logic::session::SessionInfo;

/// Read `product_id`'s persisted subtree public key, if any.
///
/// A slot that is not exactly 32 bytes reads as a miss, so a truncated or
/// over-written entry re-asks the wallet rather than deriving a wrong account.
pub async fn read_product_subtree(
    storage: &(impl CoreStorage + ?Sized),
    session: &SessionInfo,
    product_id: &str,
) -> Result<Option<[u8; 32]>, AuthorityError> {
    let stored = storage
        .read_core_storage(product_subtree_key(session, product_id)?)
        .await
        .map_err(storage_error)?;
    Ok(stored.and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok()))
}

/// Persist `product_id`'s subtree public key.
pub async fn write_product_subtree(
    storage: &(impl CoreStorage + ?Sized),
    session: &SessionInfo,
    product_id: &str,
    public_key: [u8; 32],
) -> Result<(), AuthorityError> {
    storage
        .write_core_storage(
            product_subtree_key(session, product_id)?,
            public_key.to_vec(),
        )
        .await
        .map_err(storage_error)
}

/// Drop `product_id`'s persisted subtree key.
pub async fn remove_product_subtree(
    storage: &(impl CoreStorage + ?Sized),
    session: &SessionInfo,
    product_id: &str,
) -> Result<(), AuthorityError> {
    storage
        .clear_core_storage(product_subtree_key(session, product_id)?)
        .await
        .map_err(storage_error)
}

fn product_subtree_key(
    session: &SessionInfo,
    product_id: &str,
) -> Result<CoreStorageKey, AuthorityError> {
    Ok(CoreStorageKey::ProductSubtree {
        session_id: session_storage_id(session.sso.as_ref().ok_or(AuthorityError::Disconnected)?),
        product_id: product_id.to_string(),
    })
}

fn storage_error(err: GenericError) -> AuthorityError {
    AuthorityError::Unknown {
        reason: format!("product subtree storage failed: {}", err.reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use parity_scale_codec::Encode;

    use crate::test_support::sso_session_info;

    #[derive(Default)]
    struct MemStorage {
        inner: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
        changes: parking_lot::Mutex<Vec<CoreStorageKey>>,
    }

    #[crate::platform::async_trait]
    impl CoreStorage for MemStorage {
        async fn read_core_storage(
            &self,
            key: CoreStorageKey,
        ) -> Result<Option<Vec<u8>>, GenericError> {
            Ok(self
                .inner
                .lock()
                .expect("storage mutex poisoned")
                .get(&key.encode())
                .cloned())
        }

        async fn write_core_storage(
            &self,
            key: CoreStorageKey,
            value: Vec<u8>,
        ) -> Result<(), GenericError> {
            self.inner
                .lock()
                .expect("storage mutex poisoned")
                .insert(key.encode(), value);
            Ok(())
        }

        async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
            self.inner
                .lock()
                .expect("storage mutex poisoned")
                .remove(&key.encode());
            Ok(())
        }

        async fn compare_exchange_core_storage(
            &self, key: CoreStorageKey, expected: Option<Vec<u8>>,
            replacement: Vec<u8>, notify_on_success: bool,
        ) -> Result<bool, GenericError> {
            let encoded = key.encode();
            let mut storage = self.inner.lock().expect("storage mutex poisoned");
            if storage.get(&encoded) != expected.as_ref() {
                return Ok(false);
            }
            storage.insert(encoded, replacement);
            if notify_on_success {
                self.core_storage_changed(key);
            }
            Ok(true)
        }

        fn core_storage_changed(&self, key: CoreStorageKey) {
            self.changes.lock().push(key);
        }
    }

    #[test]
    fn each_product_keeps_its_own_slot() {
        let storage = MemStorage::default();
        let session = sso_session_info();

        futures::executor::block_on(async {
            write_product_subtree(&storage, &session, "one.dot", [1; 32])
                .await
                .unwrap();
            write_product_subtree(&storage, &session, "two.dot", [2; 32])
                .await
                .unwrap();
            write_product_subtree(&storage, &session, "one.dot", [3; 32])
                .await
                .unwrap();

            assert_eq!(
                read_product_subtree(&storage, &session, "one.dot")
                    .await
                    .unwrap(),
                Some([3; 32])
            );
            assert_eq!(
                read_product_subtree(&storage, &session, "two.dot")
                    .await
                    .unwrap(),
                Some([2; 32])
            );
            assert_eq!(
                read_product_subtree(&storage, &session, "absent.dot")
                    .await
                    .unwrap(),
                None
            );
        });
    }

    #[test]
    fn the_value_is_the_bare_key_a_host_can_read() {
        let storage = MemStorage::default();
        let session = sso_session_info();

        futures::executor::block_on(async {
            write_product_subtree(&storage, &session, "one.dot", [7; 32])
                .await
                .unwrap();

            // Hosts derive addresses straight from this slot, so the value must
            // stay 32 unframed bytes rather than anything this module encodes.
            let raw = storage
                .read_core_storage(product_subtree_key(&session, "one.dot").unwrap())
                .await
                .unwrap();
            assert_eq!(raw, Some([7u8; 32].to_vec()));
        });
    }

    #[test]
    fn removing_one_product_leaves_the_others() {
        let storage = MemStorage::default();
        let session = sso_session_info();

        futures::executor::block_on(async {
            write_product_subtree(&storage, &session, "one.dot", [1; 32])
                .await
                .unwrap();
            write_product_subtree(&storage, &session, "two.dot", [2; 32])
                .await
                .unwrap();

            // Backs the rollback in `persist_product_subtree_if_current`.
            remove_product_subtree(&storage, &session, "one.dot")
                .await
                .unwrap();

            assert_eq!(
                read_product_subtree(&storage, &session, "one.dot")
                    .await
                    .unwrap(),
                None
            );
            assert_eq!(
                read_product_subtree(&storage, &session, "two.dot")
                    .await
                    .unwrap(),
                Some([2; 32])
            );
        });
    }

    #[test]
    fn a_wrong_length_slot_reads_as_empty() {
        let storage = MemStorage::default();
        let session = sso_session_info();

        futures::executor::block_on(async {
            storage
                .write_core_storage(
                    product_subtree_key(&session, "one.dot").unwrap(),
                    vec![0xff; 31],
                )
                .await
                .unwrap();

            assert_eq!(
                read_product_subtree(&storage, &session, "one.dot")
                    .await
                    .unwrap(),
                None
            );
        });
    }
}
