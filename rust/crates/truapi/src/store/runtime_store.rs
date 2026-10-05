use core::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[path = "records.rs"]
mod records;
pub use records::*;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use futures::StreamExt;
use futures::stream::{self, BoxStream};
use parity_scale_codec::Encode;
use rusqlite::{OptionalExtension, params};
use zeroize::Zeroizing;

use super::{Db, DbError};
use crate::latest::{GenericError, HostLocalStorageChangeItem};
use crate::platform::{
    CoreStorage, CoreStorageKey, ProductStorage, ProductStorageKey, SecretCoreStorage,
    SecretCoreStorageKey, async_trait,
};

/// Account-isolated runtime records sharing one durable database and secret key.
#[derive(Clone)]
pub struct RuntimeStore {
    database: Db,
    owner: [u8; 32],
    encryption_key: Arc<Zeroizing<[u8; 32]>>,
    active: Arc<AtomicBool>,
    products: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    product_guards: Vec<Arc<AtomicBool>>,
}

impl RuntimeStore {
    /// Open one owner's repositories with serialized installation-key creation.
    pub async fn open(
        database: Db,
        secrets: Arc<dyn SecretCoreStorage>,
        owner: [u8; 32],
    ) -> Result<Self, DbError> {
        static KEY_INITIALIZATION: futures::lock::Mutex<()> = futures::lock::Mutex::new(());
        let initialization = KEY_INITIALIZATION.lock().await;
        let stored = secrets
            .read_secret_core_storage(SecretCoreStorageKey::StorageEncryptionKey)
            .await
            .map_err(|error| DbError::SecretStorage(error.reason))?;
        let encryption_key = match stored {
            Some(bytes) => {
                Zeroizing::new(Zeroizing::new(bytes).as_slice().try_into().map_err(|_| {
                    DbError::InvalidRecord("storage encryption key must be 32 bytes".into())
                })?)
            }
            None => {
                let mut key = Zeroizing::new([0; 32]);
                getrandom::getrandom(key.as_mut())
                    .map_err(|error| DbError::SecretStorage(error.to_string()))?;
                secrets
                    .write_secret_core_storage(
                        SecretCoreStorageKey::StorageEncryptionKey,
                        key.to_vec(),
                    )
                    .await
                    .map_err(|error| DbError::SecretStorage(error.reason))?;
                key
            }
        };
        drop(initialization);
        database
            .write(move |transaction| {
                let owners = transaction
                    .prepare("SELECT wallet FROM account_owner")?
                    .query_map([], |row| row.get::<_, Vec<u8>>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                if owners.is_empty() {
                    transaction
                        .execute("INSERT INTO account_owner (wallet) VALUES (?1)", [owner])?;
                } else if owners != [owner.to_vec()] {
                    return Err(DbError::InvalidRecord(
                        "database belongs to another wallet".into(),
                    ));
                }
                Ok(())
            })
            .await?;
        Ok(Self {
            database,
            owner,
            encryption_key: Arc::new(encryption_key),
            active: Arc::new(AtomicBool::new(true)),
            products: Arc::new(Mutex::new(HashMap::new())),
            product_guards: Vec::new(),
        })
    }

    /// Capture the current product lifetime before starting asynchronous work.
    pub fn for_product(&self, product: &str) -> Result<Self, DbError> {
        self.ensure_active()?;
        let product = crate::platform::normalize_product_identifier(product)
            .map_err(|error| DbError::InvalidRecord(error.to_string()))?;
        let guard = self
            .products
            .lock()
            .expect("product lifecycle mutex poisoned")
            .entry(product)
            .or_insert_with(|| Arc::new(AtomicBool::new(true)))
            .clone();
        ensure_active(&guard)?;
        let mut scoped = self.clone();
        if !scoped
            .product_guards
            .iter()
            .any(|existing| Arc::ptr_eq(existing, &guard))
        {
            scoped.product_guards.push(guard);
        }
        Ok(scoped)
    }

    /// Block new work and drain writes before product cleanup starts.
    pub async fn revoke_product(&self, product: &str) -> Result<(), DbError> {
        self.ensure_active()?;
        let product = crate::platform::normalize_product_identifier(product)
            .map_err(|error| DbError::InvalidRecord(error.to_string()))?;
        self.products
            .lock()
            .expect("product lifecycle mutex poisoned")
            .entry(product)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(false, Ordering::SeqCst);
        self.write_records(|_| Ok(())).await
    }

    /// Admit a new product lifetime only after its cleanup completed.
    pub fn finish_product_removal(&self, product: &str) -> Result<(), DbError> {
        self.ensure_active()?;
        let product = crate::platform::normalize_product_identifier(product)
            .map_err(|error| DbError::InvalidRecord(error.to_string()))?;
        let mut products = self
            .products
            .lock()
            .expect("product lifecycle mutex poisoned");
        if products
            .get(&product)
            .is_some_and(|guard| guard.load(Ordering::SeqCst))
        {
            return Err(DbError::InvalidRecord(
                "product cleanup has not started".into(),
            ));
        }
        products.remove(&product);
        Ok(())
    }

    fn ensure_active(&self) -> Result<(), DbError> {
        ensure_active(&self.active)?;
        for guard in &self.product_guards {
            if !guard.load(Ordering::SeqCst) {
                return Err(DbError::InvalidRecord(
                    "product execution was revoked".into(),
                ));
            }
        }
        Ok(())
    }

    fn for_core_key(&self, key: &CoreStorageKey) -> Result<Self, DbError> {
        match key {
            CoreStorageKey::PermissionAuthorization { product_id, .. }
            | CoreStorageKey::ProductSubtree { product_id, .. }
            | CoreStorageKey::ProductManifest { product_id } => self.for_product(product_id),
            _ => Ok(self.clone()),
        }
    }

    /// Fence queued writes before a wallet lock or switch completes.
    pub async fn deactivate(&self) -> Result<(), DbError> {
        let active = self.active.clone();
        self.database
            .write(move |_| {
                active.store(false, Ordering::SeqCst);
                Ok(())
            })
            .await
    }

    async fn read_records<T, F>(&self, query: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError> + Send + 'static,
        T: Send + 'static,
    {
        let store = self.clone();
        let result = self
            .database
            .read(move |connection| {
                store.ensure_active()?;
                query(connection)
            })
            .await;
        self.ensure_active()?;
        result
    }

    async fn write_records<T, F>(&self, query: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T, DbError> + Send + 'static,
        T: Send + 'static,
    {
        let store = self.clone();
        self.database
            .write(move |transaction| {
                store.ensure_active()?;
                query(transaction)
            })
            .await
    }

    /// Stable owner that every wallet-scoped repository uses.
    pub fn owner(&self) -> [u8; 32] {
        self.owner
    }

    fn associated_data(&self, table: &str, key: &[u8]) -> Vec<u8> {
        (1u8, self.owner, table, key).encode()
    }

    fn encrypt(&self, table: &str, key: &[u8], value: &[u8]) -> Result<Vec<u8>, DbError> {
        let mut nonce = [0; 12];
        getrandom::getrandom(&mut nonce)
            .map_err(|error| DbError::SecretStorage(error.to_string()))?;
        let cipher = ChaCha20Poly1305::new((&**self.encryption_key).into());
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: value,
                    aad: &self.associated_data(table, key),
                },
            )
            .map_err(|_| DbError::InvalidRecord("payload encryption failed".into()))?;
        let mut envelope = Vec::with_capacity(13 + ciphertext.len());
        envelope.push(1);
        envelope.extend(nonce);
        envelope.extend(ciphertext);
        Ok(envelope)
    }

    fn decrypt(&self, table: &str, key: &[u8], envelope: &[u8]) -> Result<Vec<u8>, DbError> {
        if envelope.first() != Some(&1) || envelope.len() < 29 {
            return Err(DbError::InvalidRecord(
                "invalid encrypted payload envelope".into(),
            ));
        }
        let cipher = ChaCha20Poly1305::new((&**self.encryption_key).into());
        cipher
            .decrypt(
                Nonce::from_slice(&envelope[1..13]),
                Payload {
                    msg: &envelope[13..],
                    aad: &self.associated_data(table, key),
                },
            )
            .map_err(|_| DbError::InvalidRecord("payload authentication failed".into()))
    }
}

fn ensure_active(active: &AtomicBool) -> Result<(), DbError> {
    if active.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err(DbError::InvalidRecord("wallet database is inactive".into()))
    }
}

fn generic(error: impl ToString) -> GenericError {
    GenericError {
        reason: error.to_string(),
    }
}

fn product_error(error: impl ToString) -> crate::v01::HostLocalStorageReadError {
    crate::v01::HostLocalStorageReadError::Unknown {
        reason: error.to_string(),
    }
}

#[async_trait]
impl CoreStorage for RuntimeStore {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, GenericError> {
        let encoded = key.encode();
        let query_key = encoded.clone();
        let envelope: Option<Vec<u8>> = self
            .read_records(move |connection| {
                Ok(connection
                    .query_row(
                        "SELECT value FROM core_state WHERE key = ?1",
                        [query_key],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await
            .map_err(generic)?;
        envelope
            .map(|bytes| self.decrypt("core_state", &encoded, &bytes))
            .transpose()
            .map_err(generic)
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), GenericError> {
        let store = self.for_core_key(&key).map_err(generic)?;
        let encoded = key.encode();
        let envelope = self
            .encrypt("core_state", &encoded, &value)
            .map_err(generic)?;
        store.write_records(move |transaction| {
            transaction.execute(
                "
                INSERT INTO core_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value =
                excluded.value
            ",
                params![encoded, envelope],
            )?;
            Ok(())
        })
        .await
        .map_err(generic)
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
        let encoded = key.encode();
        self.write_records(move |transaction| {
            transaction.execute("DELETE FROM core_state WHERE key = ?1", [encoded])?;
            Ok(())
        })
        .await
        .map_err(generic)
    }
}

#[async_trait]
impl ProductStorage for RuntimeStore {
    async fn read(
        &self,
        key: String,
    ) -> Result<Option<Vec<u8>>, crate::v01::HostLocalStorageReadError> {
        let decoded = ProductStorageKey::decode(&key).map_err(product_error)?;
        let owner = decoded.product_id().to_owned();
        let local_key = decoded.key().to_owned();
        let envelope: Option<Vec<u8>> = self
            .read_records(move |connection| {
                Ok(connection
                    .query_row(
                        "SELECT value FROM product_storage WHERE product_id = ?1 AND key = ?2",
                        params![owner, local_key],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await
            .map_err(product_error)?;
        envelope
            .map(|bytes| self.decrypt("product_storage", decoded.encode().as_bytes(), &bytes))
            .transpose()
            .map_err(product_error)
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), crate::v01::HostLocalStorageReadError> {
        let decoded = ProductStorageKey::decode(&key).map_err(product_error)?;
        let envelope = self
            .encrypt("product_storage", decoded.encode().as_bytes(), &value)
            .map_err(product_error)?;
        let owner = decoded.product_id().to_owned();
        let local_key = decoded.key().to_owned();
        self.for_product(&owner)
            .map_err(product_error)?
            .write_records(move |transaction| {
                transaction.execute(
                    "
                INSERT INTO product_storage (product_id, key, value) VALUES (?1, ?2, ?3) ON
                CONFLICT(product_id, key) DO UPDATE SET value = excluded.value
            ",
                    params![owner, local_key, envelope],
                )?;
                Ok(())
            })
            .await
            .map_err(product_error)
    }

    async fn clear(&self, key: String) -> Result<(), crate::v01::HostLocalStorageReadError> {
        let decoded = ProductStorageKey::decode(&key).map_err(product_error)?;
        let owner = decoded.product_id().to_owned();
        let local_key = decoded.key().to_owned();
        self.write_records(move |transaction| {
            transaction.execute(
                "DELETE FROM product_storage WHERE product_id = ?1 AND key = ?2",
                params![owner, local_key],
            )?;
            Ok(())
        })
        .await
        .map_err(product_error)
    }

    fn subscribe_storage(
        &self,
        key: String,
    ) -> BoxStream<'static, Result<HostLocalStorageChangeItem, GenericError>> {
        let decoded = match ProductStorageKey::decode(&key) {
            Ok(decoded) => decoded,
            Err(error) => return stream::once(core::future::ready(Err(generic(error)))).boxed(),
        };
        let store = self.clone();
        let owner = decoded.product_id().to_owned();
        let local_key = decoded.key().to_owned();
        self.database
            .observe(
                "SELECT value FROM product_storage WHERE product_id = ?1 AND key = ?2",
                move |query| {
                    query.query_optional(params![owner, local_key], |row| row.get::<_, Vec<u8>>(0))
                },
            )
            .map(move |result| {
                store.ensure_active().map_err(generic)?;
                let envelope = result.map_err(generic)?;
                let value = envelope
                    .map(|bytes| {
                        store.decrypt("product_storage", decoded.encode().as_bytes(), &bytes)
                    })
                    .transpose()
                    .map_err(generic)?;
                Ok(HostLocalStorageChangeItem { value })
            })
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::store::{DbConfig, DbLocation, core_migrations};
    use futures::FutureExt;
    use futures::executor::block_on;

    #[derive(Default)]
    struct Secrets(Mutex<HashMap<String, Vec<u8>>>);

    #[async_trait]
    impl SecretCoreStorage for Secrets {
        async fn read_secret_core_storage(
            &self,
            key: SecretCoreStorageKey,
        ) -> Result<Option<Vec<u8>>, GenericError> {
            Ok(self.0.lock().unwrap().get(&key.storage_key()).cloned())
        }
        async fn write_secret_core_storage(
            &self,
            key: SecretCoreStorageKey,
            value: Vec<u8>,
        ) -> Result<(), GenericError> {
            let mut pending = true;
            futures::future::poll_fn(|context| {
                if core::mem::take(&mut pending) {
                    context.waker().wake_by_ref();
                    core::task::Poll::Pending
                } else {
                    core::task::Poll::Ready(())
                }
            })
            .await;
            self.0.lock().unwrap().insert(key.storage_key(), value);
            Ok(())
        }
        async fn clear_secret_core_storage(
            &self,
            key: SecretCoreStorageKey,
        ) -> Result<(), GenericError> {
            self.0.lock().unwrap().remove(&key.storage_key());
            Ok(())
        }
    }

    async fn memory_store() -> RuntimeStore {
        let database = Db::open(DbConfig {
            location: DbLocation::Memory,
            migrations: core_migrations,
            readers: 1,
        })
        .await
        .unwrap();
        RuntimeStore::open(database, Arc::new(Secrets::default()), [1; 32])
            .await
            .unwrap()
    }

    fn product(identifier: &str) -> ProductRecord {
        ProductRecord {
            product_id: identifier.into(),
            name: identifier.into(),
            icon_cid: None,
            icon_format: None,
            worker_url_override: None,
        }
    }

    #[test]
    fn simultaneous_owner_opens_keep_one_persisted_encryption_key() {
        block_on(async {
            let config = || DbConfig {
                location: DbLocation::Memory,
                migrations: core_migrations,
                readers: 1,
            };
            let first_database = Db::open(config()).await.unwrap();
            let second_database = Db::open(config()).await.unwrap();
            let secrets = Arc::new(Secrets::default());
            let (first, second) = futures::join!(
                RuntimeStore::open(first_database.clone(), secrets.clone(), [1; 32]),
                RuntimeStore::open(second_database.clone(), secrets.clone(), [2; 32]),
            );
            let key = ProductStorageKey::new("chess.dot", "save")
                .unwrap()
                .encode();
            first.unwrap().write(key.clone(), vec![1]).await.unwrap();
            second.unwrap().write(key.clone(), vec![2]).await.unwrap();
            let first = RuntimeStore::open(first_database, secrets.clone(), [1; 32])
                .await
                .unwrap();
            let second = RuntimeStore::open(second_database, secrets, [2; 32])
                .await
                .unwrap();
            assert_eq!(
                (
                    first.read(key.clone()).await.unwrap(),
                    second.read(key).await.unwrap()
                ),
                (Some(vec![1]), Some(vec![2]))
            );
        });
    }

    #[test]
    fn record_changes_follow_commits_and_ignore_rolled_back_updates() {
        block_on(async {
            let store = memory_store().await;
            let mut changes = store.observe_records();
            changes.next().await.unwrap().unwrap();
            store.ensure_product("chess.dot".into()).await.unwrap();
            changes.next().await.unwrap().unwrap();
            let failed: Result<(), DbError> = store
                .database
                .write(|transaction| {
                    transaction.execute("UPDATE products SET name='uncommitted'", [])?;
                    Err(DbError::InvalidRecord("abort".into()))
                })
                .await;
            assert!(failed.is_err());
            assert!(changes.next().now_or_never().is_none());
            let mut renamed = product("chess.dot");
            renamed.name = "Chess".into();
            store.save_product(renamed.clone()).await.unwrap();
            changes.next().await.unwrap().unwrap();
            assert_eq!(store.products().await.unwrap(), vec![renamed]);
            store
                .write_core_storage(CoreStorageKey::StatementRenewalTargets, vec![1])
                .await
                .unwrap();
            changes.next().await.unwrap().unwrap();
        });
    }

    #[test]
    fn encrypted_product_and_core_records_survive_reopen_without_plaintext() {
        block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let secrets = Arc::new(Secrets::default());
            let config = crate::store::core_db_config(directory.path());
            let database = Db::open(config.clone()).await.unwrap();
            let store = RuntimeStore::open(database.clone(), secrets.clone(), [1; 32])
                .await
                .unwrap();
            let key = ProductStorageKey::new("chess.dot", "save")
                .unwrap()
                .encode();
            let value = b"private product payload".to_vec();
            store.write(key.clone(), value.clone()).await.unwrap();
            store
                .write_core_storage(CoreStorageKey::StatementRenewalTargets, value.clone())
                .await
                .unwrap();
            let encrypted: Vec<Vec<u8>> = database.read(|connection| Ok(connection.prepare("SELECT value FROM product_storage UNION ALL SELECT value FROM core_state")?.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?)).await.unwrap();
            assert!(
                encrypted
                    .iter()
                    .all(|bytes| bytes.windows(value.len()).all(|window| window != value))
            );
            database.close().await.unwrap();
            let reopened = RuntimeStore::open(Db::open(config).await.unwrap(), secrets, [1; 32])
                .await
                .unwrap();
            assert_eq!(
                (
                    reopened.read(key).await.unwrap(),
                    reopened
                        .read_core_storage(CoreStorageKey::StatementRenewalTargets)
                        .await
                        .unwrap()
                ),
                (Some(value.clone()), Some(value))
            );
        });
    }

    #[test]
    fn row_substitution_and_wrong_wallet_are_rejected() {
        block_on(async {
            let store = memory_store().await;
            let first = ProductStorageKey::new("chess.dot", "first")
                .unwrap()
                .encode();
            let second = ProductStorageKey::new("chess.dot", "second")
                .unwrap()
                .encode();
            store.write(first.clone(), vec![1, 2, 3]).await.unwrap();
            store
                .database
                .write(|transaction| {
                    transaction.execute("INSERT INTO product_storage SELECT product_id,'second',value FROM product_storage", [])?;
                    Ok(())
                })
                .await
                .unwrap();
            assert!(store.read(second).await.is_err());
            assert!(
                RuntimeStore::open(
                    store.database.clone(),
                    Arc::new(Secrets::default()),
                    [2; 32]
                )
                .await
                .is_err()
            );
            store
                .database
                .write(|transaction| {
                    transaction.execute("UPDATE product_storage SET value=X'00'", [])?;
                    Ok(())
                })
                .await
                .unwrap();
            assert!(store.read(first).await.is_err());
        });
    }

    #[test]
    fn writes_use_fresh_nonces_and_subscribers_observe_committed_values() {
        block_on(async {
            let store = memory_store().await;
            let key = ProductStorageKey::new("chess.dot", "save")
                .unwrap()
                .encode();
            let mut changes = store.subscribe_storage(key.clone());
            assert_eq!(
                changes.next().await.unwrap().unwrap(),
                HostLocalStorageChangeItem { value: None }
            );
            store.write(key.clone(), vec![1]).await.unwrap();
            let first: Vec<u8> = store
                .database
                .read(|connection| {
                    Ok(connection
                        .query_row("SELECT value FROM product_storage", [], |row| row.get(0))?)
                })
                .await
                .unwrap();
            assert_eq!(
                changes.next().await.unwrap().unwrap(),
                HostLocalStorageChangeItem {
                    value: Some(vec![1])
                }
            );
            store.write(key.clone(), vec![1]).await.unwrap();
            let second: Vec<u8> = store
                .database
                .read(|connection| {
                    Ok(connection
                        .query_row("SELECT value FROM product_storage", [], |row| row.get(0))?)
                })
                .await
                .unwrap();
            assert_ne!(first, second);
            store.clear(key).await.unwrap();
            assert_eq!(
                changes.next().await.unwrap().unwrap(),
                HostLocalStorageChangeItem { value: None }
            );
        });
    }

    #[test]
    fn wallet_deactivation_fences_old_reads_and_writes() {
        block_on(async {
            let store = memory_store().await;
            let key = ProductStorageKey::new("chess.dot", "save")
                .unwrap()
                .encode();
            store.write(key.clone(), vec![1]).await.unwrap();
            store.deactivate().await.unwrap();
            assert!(store.write(key.clone(), vec![2]).await.is_err());
            assert!(store.save_product(product("chess.dot")).await.is_err());
            assert!(store.read(key).await.is_err());
        });
    }

    #[test]
    fn reset_fences_old_execution_writes_before_the_empty_store_is_observable() {
        block_on(async {
            let store = memory_store().await;
            let old_execution = store.clone();
            let key = ProductStorageKey::new("chess.dot", "save")
                .unwrap()
                .encode();
            store.write(key.clone(), vec![1]).await.unwrap();
            store.save_product(product("chess.dot")).await.unwrap();
            store.reset_and_deactivate().await.unwrap();
            assert!(old_execution.write(key, vec![2]).await.is_err());
            assert!(
                old_execution
                    .save_product(product("chess.dot"))
                    .await
                    .is_err()
            );
            let rows: (u32, u32) = store
                .database
                .read(|connection| {
                    Ok((
                        connection.query_row(
                            "SELECT COUNT(*) FROM product_storage",
                            [],
                            |row| row.get(0),
                        )?,
                        connection
                            .query_row("SELECT COUNT(*) FROM products", [], |row| row.get(0))?,
                    ))
                })
                .await
                .unwrap();
            assert_eq!(rows, (0, 0));
        });
    }

    #[test]
    fn product_removal_fences_old_work_and_retries_cleanup_before_reopening() {
        block_on(async {
            let store = memory_store().await;
            store.ensure_product("chess.dot".into()).await.unwrap();
            let old_execution = store.for_product("chess.dot").unwrap();
            let other_execution = store.for_product("other.dot").unwrap();
            let key = ProductStorageKey::new("chess.dot", "save")
                .unwrap()
                .encode();
            old_execution.write(key.clone(), vec![1]).await.unwrap();
            let notification = old_execution
                .prepare_notification("chess.dot".into(), "retry".into(), None, Some(1), 64)
                .await
                .unwrap();

            store.revoke_product("chess.dot").await.unwrap();
            assert_eq!(
                (
                    store.for_product("chess.dot").is_err(),
                    store.ensure_product("chess.dot".into()).await.is_err(),
                    store
                        .prepare_notification("chess.dot".into(), "late".into(), None, None, 64)
                        .await
                        .is_err(),
                    old_execution.write(key.clone(), vec![2]).await.is_err(),
                    store.remove_product("chess.dot".into()).await.is_err(),
                ),
                (true, true, true, true, true),
            );
            other_execution
                .ensure_product("other.dot".into())
                .await
                .unwrap();
            store
                .cancel_product_notifications("chess.dot".into())
                .await
                .unwrap();
            store
                .acknowledge_notification(
                    notification.notification_id,
                    1,
                    NotificationState::Cancel,
                )
                .await
                .unwrap();
            store.remove_product("chess.dot".into()).await.unwrap();
            store.finish_product_removal("chess.dot").unwrap();

            let reopened = store.for_product("chess.dot").unwrap();
            reopened.ensure_product("chess.dot".into()).await.unwrap();
            reopened.write(key.clone(), vec![3]).await.unwrap();
            assert_eq!(
                (
                    old_execution.read(key.clone()).await.is_err(),
                    old_execution.write(key.clone(), vec![4]).await.is_err(),
                    old_execution
                        .save_product(product("chess.dot"))
                        .await
                        .is_err(),
                    old_execution
                        .begin_worker_operation("chess.dot".into(), None, 1)
                        .await
                        .is_err(),
                    reopened.read(key).await.unwrap(),
                ),
                (true, true, true, true, Some(vec![3])),
            );
        });
    }

    #[test]
    fn account_switching_keeps_identical_product_keys_and_permissions_isolated() {
        block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let secrets = Arc::new(Secrets::default());
            let key = ProductStorageKey::new("chess.dot", "save")
                .unwrap()
                .encode();
            let permission = CoreStorageKey::identity_disclosure_authorization("chess.dot");
            for (owner, value) in [([1; 32], vec![1]), ([2; 32], vec![2])] {
                let config = crate::store::account_core_db_config(directory.path(), &owner);
                if let DbLocation::File(path) = &config.location {
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                }
                let database = Db::open(config).await.unwrap();
                let store = RuntimeStore::open(database.clone(), secrets.clone(), owner)
                    .await
                    .unwrap();
                store.write(key.clone(), value.clone()).await.unwrap();
                store
                    .write_core_storage(permission.clone(), value)
                    .await
                    .unwrap();
                store.deactivate().await.unwrap();
                database.close().await.unwrap();
            }
            let store = RuntimeStore::open(
                Db::open(crate::store::account_core_db_config(
                    directory.path(),
                    &[1; 32],
                ))
                .await
                .unwrap(),
                secrets,
                [1; 32],
            )
            .await
            .unwrap();
            assert_eq!(
                (
                    store.read(key).await.unwrap(),
                    store.read_core_storage(permission).await.unwrap()
                ),
                (Some(vec![1]), Some(vec![1]))
            );
        });
    }

    #[test]
    fn failed_os_cancellation_retains_rows_and_late_registration_cannot_revive_them() {
        block_on(async {
            let store = memory_store().await;
            store.save_product(product("chess.dot")).await.unwrap();
            let registration = store
                .prepare_notification("chess.dot".into(), "ready".into(), None, None, 2)
                .await
                .unwrap();
            store
                .cancel_product_notifications("chess.dot".into())
                .await
                .unwrap();
            assert!(
                !store
                    .acknowledge_notification(
                        registration.notification_id,
                        registration.revision,
                        NotificationState::Register
                    )
                    .await
                    .unwrap()
            );
            assert!(store.remove_product("chess.dot".into()).await.is_err());
            assert!(store.reset().await.is_err());
            let pending = store.notifications().await.unwrap();
            assert_eq!(
                pending,
                vec![ScheduledNotificationRecord {
                    state: NotificationState::Cancel,
                    revision: 1,
                    ..registration.clone()
                }]
            );
            assert!(
                store
                    .acknowledge_notification(
                        registration.notification_id,
                        1,
                        NotificationState::Cancel
                    )
                    .await
                    .unwrap()
            );
            let next = store
                .prepare_notification("chess.dot".into(), "later".into(), None, None, 2)
                .await
                .unwrap();
            assert_ne!(next.notification_id, registration.notification_id);
            assert!(
                !store
                    .acknowledge_notification(
                        registration.notification_id,
                        1,
                        NotificationState::Cancel
                    )
                    .await
                    .unwrap()
            );
        });
    }

    #[test]
    fn notification_capacity_is_shared_and_immediate_delivery_does_not_occupy_it() {
        block_on(async {
            let store = memory_store().await;
            store.save_product(product("chess.dot")).await.unwrap();
            store.save_product(product("chat.dot")).await.unwrap();
            let scheduled = store
                .prepare_notification("chess.dot".into(), "future".into(), None, Some(100), 1)
                .await
                .unwrap();
            assert!(matches!(
                store
                    .prepare_notification("chat.dot".into(), "other".into(), None, Some(200), 1)
                    .await,
                Err(DbError::NotificationLimitReached)
            ));
            let immediate = store
                .prepare_notification("chat.dot".into(), "now".into(), None, None, 1)
                .await
                .unwrap();
            store
                .acknowledge_notification(immediate.notification_id, 0, NotificationState::Register)
                .await
                .unwrap();
            store
                .acknowledge_notification(scheduled.notification_id, 0, NotificationState::Register)
                .await
                .unwrap();
            store
                .reconcile_pending_notifications(vec![], 50)
                .await
                .unwrap();
            assert_eq!(
                store.notifications().await.unwrap(),
                vec![ScheduledNotificationRecord {
                    revision: 1,
                    ..scheduled.clone()
                }]
            );
            store
                .acknowledge_notification(scheduled.notification_id, 1, NotificationState::Register)
                .await
                .unwrap();
            store
                .reconcile_pending_notifications(vec![], 100)
                .await
                .unwrap();
            assert_eq!(store.notifications().await.unwrap(), vec![]);
        });
    }

    #[test]
    fn worker_operations_retain_unlabeled_android_work_and_cleanup_cascades() {
        block_on(async {
            let store = memory_store().await;
            store.save_product(product("chess.dot")).await.unwrap();
            store
                .save_worker(ProductWorkerRecord {
                    product_id: "chess.dot".into(),
                    content_hash: vec![1],
                    pending_hash: None,
                    manifest: vec![2],
                    checked_at: 0,
                    failure_count: 0,
                    last_error: None,
                })
                .await
                .unwrap();
            store
                .add_worker_reason("chess.dot".into(), WorkerReason::Chat, 1)
                .await
                .unwrap();
            let operation = store
                .begin_worker_operation("chess.dot".into(), None, 2)
                .await
                .unwrap();
            store
                .remove_worker_reason("chess.dot".into(), WorkerReason::Chat)
                .await
                .unwrap();
            assert!(
                !store
                    .remove_worker_if_unused("chess.dot".into(), false)
                    .await
                    .unwrap()
            );
            assert_eq!(
                store.worker_operations().await.unwrap(),
                vec![operation.clone()]
            );
            store
                .end_worker_operation("chess.dot".into(), operation.operation_id)
                .await
                .unwrap();
            assert!(
                !store
                    .remove_worker_if_unused("chess.dot".into(), true)
                    .await
                    .unwrap()
            );
            assert!(
                store
                    .remove_worker_if_unused("chess.dot".into(), false)
                    .await
                    .unwrap()
            );
            store.remove_product("chess.dot".into()).await.unwrap();
            assert_eq!(
                (
                    store.products().await.unwrap(),
                    store.worker_operations().await.unwrap()
                ),
                (vec![], vec![])
            );
        });
    }

    #[test]
    fn detailed_slots_keep_individual_priorities_and_clear_duplicate_aggregates() {
        block_on(async {
            let store = memory_store().await;
            let allowance = AllowanceRecord {
                chain: [2; 32],
                resource: "statement-store".into(),
                account: [3; 32],
                allocated_at: 1,
                priority: Some(20),
                last_renewed_period: Some(4),
            };
            let first = StatementSlotRecord {
                chain: [2; 32],
                collection: "app".into(),
                period: 4,
                slot: 0,
                account: [3; 32],
                priority: 2,
                last_allocated_or_renewed_at: 10,
            };
            let second = StatementSlotRecord {
                slot: 1,
                priority: 9,
                ..first.clone()
            };
            store
                .record_allowance(allowance.clone(), vec![first.clone(), second.clone()])
                .await
                .unwrap();
            assert_eq!(
                (
                    store.allowances().await.unwrap(),
                    store.statement_slots().await.unwrap()
                ),
                (
                    vec![AllowanceRecord {
                        priority: None,
                        last_renewed_period: None,
                        ..allowance.clone()
                    }],
                    vec![second.clone(), first.clone()]
                )
            );
            store
                .record_allowance(
                    allowance.clone(),
                    vec![StatementSlotRecord {
                        priority: 0,
                        ..first.clone()
                    }],
                )
                .await
                .unwrap();
            assert_eq!(
                store.statement_slots().await.unwrap(),
                vec![second.clone(), first.clone()]
            );
            let renewed = StatementSlotRecord {
                period: 5,
                slot: 8,
                priority: 0,
                last_allocated_or_renewed_at: 20,
                ..first.clone()
            };
            store
                .renew_statement_slot(first.clone(), renewed.clone())
                .await
                .unwrap();
            assert!(
                store
                    .renew_statement_slot(first.clone(), renewed.clone())
                    .await
                    .is_err()
            );
            assert_eq!(
                store.statement_slots().await.unwrap(),
                vec![
                    second,
                    StatementSlotRecord {
                        priority: first.priority,
                        ..renewed
                    }
                ]
            );
            let retained_slot = StatementSlotRecord {
                account: [4; 32],
                slot: 9,
                last_allocated_or_renewed_at: 30,
                ..first
            };
            let retained_allowance = AllowanceRecord {
                account: retained_slot.account,
                priority: None,
                last_renewed_period: None,
                ..allowance.clone()
            };
            let bulletin = AllowanceRecord {
                resource: "bulletin".into(),
                priority: None,
                last_renewed_period: None,
                ..allowance.clone()
            };
            store
                .record_allowance(retained_allowance.clone(), vec![retained_slot.clone()])
                .await
                .unwrap();
            store.record_allowance(bulletin.clone(), vec![]).await.unwrap();
            assert_eq!(store.statement_slots().await.unwrap()[1], retained_slot);
            store.remove_statement_allowance(allowance.account).await.unwrap();
            assert_eq!(
                (store.allowances().await.unwrap(), store.statement_slots().await.unwrap()),
                (vec![bulletin, retained_allowance], vec![retained_slot])
            );
        });
    }
}
