use std::sync::Arc;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use futures::stream::{self, BoxStream, StreamExt};
use parity_scale_codec::{Decode, Encode};
use rusqlite::{OptionalExtension, params};
use zeroize::Zeroizing;

use super::{Db, DbError};
use crate::host_internal::permissions::{PermissionRecord, saved_permission_updates};
use crate::latest::{GenericError, HostLocalStorageChangeItem};
use crate::platform::{
    CoreStorage, CoreStorageKey, ProductStorage, ProductStorageKey, SecretCoreStorage,
    SecretCoreStorageKey, async_trait,
};
use crate::platform::{PermissionAuthorizationRequest, PermissionAuthorizationStatus};
use crate::v01::HostLocalStorageReadError;

const ENVELOPE_VERSION: u8 = 1;
const NONCE_LENGTH: usize = 12;
const PRODUCT_VALUE: &str = "SELECT value FROM product_storage WHERE product_id = ?1 AND key = ?2";

/// Installation-owned encrypted product and core records over one shared database.
#[derive(Clone)]
pub struct RuntimeStore {
    database: Db,
    encryption_key: Arc<Zeroizing<[u8; 32]>>,
}

impl RuntimeStore {
    /// The process owner serializes initialization before publishing its runtime.
    pub async fn open(database: Db, secrets: &dyn SecretCoreStorage) -> Result<Self, DbError> {
        let stored = secrets
            .read_secret_core_storage(SecretCoreStorageKey::StorageEncryptionKey)
            .await
            .map_err(|error| DbError::Protection(error.reason))?
            .map(Zeroizing::new);
        let encryption_key = match stored {
            Some(bytes) => Zeroizing::new(bytes.as_slice().try_into().map_err(|_| {
                DbError::Protection("installation key must contain 32 bytes".to_string())
            })?),
            None => {
                let has_records = database.read(|connection| {
                    Ok(connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM product_storage) OR EXISTS(SELECT 1 FROM core_state)",
                        [],
                        |row| row.get::<_, bool>(0),
                    )?)
                }).await?;
                if has_records {
                    return Err(DbError::Protection(
                        "installation key is missing for existing records".to_string(),
                    ));
                }
                let mut key = Zeroizing::new([0; 32]);
                getrandom::getrandom(&mut *key)
                    .map_err(|error| DbError::Protection(error.to_string()))?;
                secrets
                    .write_secret_core_storage(
                        SecretCoreStorageKey::StorageEncryptionKey,
                        key.to_vec(),
                    )
                    .await
                    .map_err(|error| DbError::Protection(error.reason))?;
                key
            }
        };
        Ok(Self {
            database,
            encryption_key: Arc::new(encryption_key),
        })
    }

    /// Saved permission records observed from the same committed core state.
    pub fn permission_records(
        &self,
        product_id: Option<String>,
        network_suffix: String,
    ) -> BoxStream<'static, Result<Vec<PermissionRecord>, GenericError>> {
        let product_id = match product_id
            .map(|product| crate::platform::normalize_product_identifier(&product))
            .transpose()
        {
            Ok(product) => product,
            Err(error) => {
                return stream::once(async move {
                    Err(GenericError {
                        reason: error.to_string(),
                    })
                })
                .boxed();
            }
        };
        let store = self.clone();
        self.database
            .observe(
                "SELECT key, value FROM core_state ORDER BY key",
                move |statement| {
                    let rows = statement.query_map([], |row| {
                        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                    })?;
                    let mut records = Vec::new();
                    for row in rows {
                        let (encoded, value) = row;
                        let mut input = encoded.as_slice();
                        let key = CoreStorageKey::decode(&mut input)
                            .map_err(|error| DbError::Protection(error.to_string()))?;
                        if !input.is_empty() {
                            return Err(DbError::Protection(
                                "core key contains trailing bytes".to_string(),
                            ));
                        }
                        let CoreStorageKey::PermissionAuthorization {
                            product_id: owner,
                            request,
                        } = &key
                        else {
                            continue;
                        };
                        if let Some(product) = &product_id {
                            let filter = if matches!(
                                request,
                                PermissionAuthorizationRequest::AccountAccess { .. }
                            ) {
                                crate::host_internal::product_manifest::bare_product_label(product)
                            } else {
                                product.as_str()
                            };
                            if owner != filter {
                                continue;
                            }
                        }
                        let value = store.decrypt(&core_identity(&encoded), &value)?;
                        if let Some(record) =
                            PermissionRecord::decode(&key, &value, &network_suffix)
                                .map_err(|error| DbError::Protection(error.reason))?
                        {
                            records.push(record);
                        }
                    }
                    Ok(records)
                },
            )
            .map(|result| result.map_err(core_error))
            .boxed()
    }

    /// Commit one settings edit, including domain fan-out, as one visible change.
    pub async fn set_permission_record(
        &self,
        product_id: &str,
        request: &PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), GenericError> {
        let updates = saved_permission_updates(product_id, request, status)?
            .into_iter()
            .map(|(key, value)| {
                let key = key.encode();
                let value = value
                    .map(|value| self.encrypt(&core_identity(&key), &value))
                    .transpose()?;
                Ok((key, value))
            })
            .collect::<Result<Vec<_>, DbError>>()
            .map_err(core_error)?;
        self.database.write(move |transaction| {
            for (key, value) in updates {
                if let Some(value) = value {
                    transaction.execute("INSERT INTO core_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])?;
                } else {
                    transaction.execute("DELETE FROM core_state WHERE key = ?1", params![key])?;
                }
            }
            Ok(())
        }).await.map_err(core_error)
    }

    fn encrypt(&self, identity: &[u8], value: &[u8]) -> Result<Vec<u8>, DbError> {
        let mut nonce = [0; NONCE_LENGTH];
        getrandom::getrandom(&mut nonce).map_err(|error| DbError::Protection(error.to_string()))?;
        let cipher = ChaCha20Poly1305::new((&**self.encryption_key).into());
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: value,
                    aad: identity,
                },
            )
            .map_err(|_| DbError::Protection("value encryption failed".to_string()))?;
        let mut envelope = Vec::with_capacity(1 + NONCE_LENGTH + ciphertext.len());
        envelope.push(ENVELOPE_VERSION);
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&ciphertext);
        Ok(envelope)
    }

    fn decrypt(&self, identity: &[u8], envelope: &[u8]) -> Result<Vec<u8>, DbError> {
        if envelope.first() != Some(&ENVELOPE_VERSION) || envelope.len() < 1 + NONCE_LENGTH + 16 {
            return Err(DbError::Protection(
                "invalid encrypted value envelope".to_string(),
            ));
        }
        let cipher = ChaCha20Poly1305::new((&**self.encryption_key).into());
        cipher
            .decrypt(
                Nonce::from_slice(&envelope[1..1 + NONCE_LENGTH]),
                Payload {
                    msg: &envelope[1 + NONCE_LENGTH..],
                    aad: identity,
                },
            )
            .map_err(|_| DbError::Protection("value authentication failed".to_string()))
    }
}

fn product_identity(key: &ProductStorageKey) -> Vec<u8> {
    (
        ENVELOPE_VERSION,
        "product_storage",
        key.product_id(),
        key.key(),
    )
        .encode()
}

fn core_identity(key: &[u8]) -> Vec<u8> {
    (ENVELOPE_VERSION, "core_state", key).encode()
}

fn product_error(error: impl ToString) -> HostLocalStorageReadError {
    HostLocalStorageReadError::Unknown {
        reason: error.to_string(),
    }
}

fn core_error(error: DbError) -> GenericError {
    GenericError {
        reason: error.to_string(),
    }
}

#[async_trait]
impl ProductStorage for RuntimeStore {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, HostLocalStorageReadError> {
        let key = ProductStorageKey::decode(&key).map_err(product_error)?;
        let identity = product_identity(&key);
        let value = self
            .database
            .read(move |connection| {
                Ok(connection
                    .query_row(PRODUCT_VALUE, params![key.product_id(), key.key()], |row| {
                        row.get::<_, Vec<u8>>(0)
                    })
                    .optional()?)
            })
            .await
            .map_err(product_error)?;
        value
            .map(|value| self.decrypt(&identity, &value))
            .transpose()
            .map_err(product_error)
    }

    async fn write(&self, key: String, value: Vec<u8>) -> Result<(), HostLocalStorageReadError> {
        let key = ProductStorageKey::decode(&key).map_err(product_error)?;
        let value = self
            .encrypt(&product_identity(&key), &value)
            .map_err(product_error)?;
        self.database.write(move |transaction| {
            transaction.execute(
                "INSERT INTO product_storage (product_id, key, value) VALUES (?1, ?2, ?3) ON CONFLICT(product_id, key) DO UPDATE SET value = excluded.value",
                params![key.product_id(), key.key(), value],
            )?;
            Ok(())
        }).await.map_err(product_error)
    }

    async fn clear(&self, key: String) -> Result<(), HostLocalStorageReadError> {
        let key = ProductStorageKey::decode(&key).map_err(product_error)?;
        self.database
            .write(move |transaction| {
                transaction.execute(
                    "DELETE FROM product_storage WHERE product_id = ?1 AND key = ?2",
                    params![key.product_id(), key.key()],
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
        let key = match ProductStorageKey::decode(&key) {
            Ok(key) => key,
            Err(reason) => return stream::once(async { Err(GenericError { reason }) }).boxed(),
        };
        let store = self.clone();
        let identity = product_identity(&key);
        self.database
            .observe(PRODUCT_VALUE, move |statement| {
                let value: Option<Vec<u8>> = statement
                    .query_optional(params![key.product_id(), key.key()], |row| row.get(0))?;
                Ok(HostLocalStorageChangeItem {
                    value: value
                        .map(|value| store.decrypt(&identity, &value))
                        .transpose()?,
                })
            })
            .map(|result| result.map_err(core_error))
            .boxed()
    }
}

#[async_trait]
impl CoreStorage for RuntimeStore {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, GenericError> {
        let key = key.encode();
        let identity = core_identity(&key);
        let read = move |connection: &rusqlite::Connection| {
            Ok(connection
                .query_row(
                    "SELECT value FROM core_state WHERE key = ?1",
                    params![key],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?)
        };
        let value = self
            .database
            .read_after_writes(read)
            .await
            .map_err(core_error)?;
        value
            .map(|value| self.decrypt(&identity, &value))
            .transpose()
            .map_err(core_error)
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), GenericError> {
        let key = key.encode();
        let value = self
            .encrypt(&core_identity(&key), &value)
            .map_err(core_error)?;
        self.database.write(move |transaction| {
            transaction.execute("INSERT INTO core_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])?;
            Ok(())
        }).await.map_err(core_error)
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
        let key = key.encode();
        self.database
            .write(move |transaction| {
                transaction.execute("DELETE FROM core_state WHERE key = ?1", params![key])?;
                Ok(())
            })
            .await
            .map_err(core_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::core_db_config;
    use crate::test_support::{StubPlatform, secret_core_storage_test_key};
    use futures::{FutureExt, StreamExt};

    #[test]
    fn saved_permission_edits_preserve_bundle_identity_and_observe_committed_records() {
        futures::executor::block_on(async {
            use crate::latest::{
                HostDevicePermissionRequest, RemotePermission, RemotePermissionRequest,
            };
            let directory = tempfile::tempdir().unwrap();
            let database = Db::open(core_db_config(directory.path())).await.unwrap();
            let store = RuntimeStore::open(database.clone(), &StubPlatform::default())
                .await
                .unwrap();
            let remote = |domains: &[&str]| {
                PermissionAuthorizationRequest::Remote(RemotePermissionRequest {
                    permission: RemotePermission::Remote {
                        domains: domains.iter().map(|domain| domain.to_string()).collect(),
                    },
                })
            };
            let first = remote(&["a.example.com"]);
            let second = remote(&["b.example.com"]);
            let bundle = remote(&["a.example.com", "b.example.com"]);
            let mut records =
                store.permission_records(Some("product.paseo".to_string()), "paseo".to_string());
            assert_eq!(records.next().await, Some(Ok(vec![])));
            store
                .set_permission_record(
                    "product.paseo",
                    &first,
                    PermissionAuthorizationStatus::Authorized,
                )
                .await
                .unwrap();
            records.next().await.unwrap().unwrap();
            store
                .set_permission_record(
                    "product.paseo",
                    &bundle,
                    PermissionAuthorizationStatus::Denied,
                )
                .await
                .unwrap();
            records.next().await.unwrap().unwrap();
            store
                .set_permission_record(
                    "product.paseo",
                    &bundle,
                    PermissionAuthorizationStatus::NotDetermined,
                )
                .await
                .unwrap();
            assert_eq!(
                records.next().await,
                Some(Ok(vec![PermissionRecord {
                    product_id: "product.paseo".to_string(),
                    request: first.clone(),
                    status: PermissionAuthorizationStatus::Authorized
                }]))
            );
            store
                .set_permission_record(
                    "product.paseo",
                    &bundle,
                    PermissionAuthorizationStatus::Denied,
                )
                .await
                .unwrap();
            records.next().await.unwrap().unwrap();
            store
                .set_permission_record(
                    "product.paseo",
                    &bundle,
                    PermissionAuthorizationStatus::Authorized,
                )
                .await
                .unwrap();
            assert_eq!(
                records.next().await,
                Some(Ok(vec![
                    PermissionRecord {
                        product_id: "product.paseo".to_string(),
                        request: first,
                        status: PermissionAuthorizationStatus::Authorized
                    },
                    PermissionRecord {
                        product_id: "product.paseo".to_string(),
                        request: second,
                        status: PermissionAuthorizationStatus::Authorized
                    },
                ]))
            );
            let account = PermissionAuthorizationRequest::AccountAccess {
                target_product_id: "peer".to_string(),
            };
            let camera =
                PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera);
            store
                .set_permission_record(
                    "calendar.paseo",
                    &account,
                    PermissionAuthorizationStatus::Authorized,
                )
                .await
                .unwrap();
            let mut global = store.permission_records(None, "paseo".to_string());
            let account_only = global.next().await.unwrap().unwrap();
            assert!(account_only.contains(&PermissionRecord {
                product_id: "calendar.paseo".to_string(),
                request: account.clone(),
                status: PermissionAuthorizationStatus::Authorized
            }));
            store
                .set_permission_record(
                    "calendar.paseo",
                    &camera,
                    PermissionAuthorizationStatus::Authorized,
                )
                .await
                .unwrap();
            let mixed = global.next().await.unwrap().unwrap();
            assert_eq!(
                mixed
                    .iter()
                    .filter(|record| record.product_id == "calendar.paseo")
                    .count(),
                2
            );
            store
                .set_permission_record(
                    "calendar.paseo",
                    &account,
                    PermissionAuthorizationStatus::NotDetermined,
                )
                .await
                .unwrap();
            let mut selected =
                store.permission_records(Some("calendar.paseo".to_string()), "paseo".to_string());
            assert_eq!(
                selected.next().await,
                Some(Ok(vec![PermissionRecord {
                    product_id: "calendar.paseo".to_string(),
                    request: camera.clone(),
                    status: PermissionAuthorizationStatus::Authorized
                }]))
            );
            store
                .write_core_storage(
                    crate::host_internal::permissions::permission_key("calendar.paseo", &camera),
                    vec![255],
                )
                .await
                .unwrap();
            assert!(selected.next().await.unwrap().is_err());
        });
    }

    #[test]
    fn permission_reads_wait_for_a_started_commit_after_its_caller_is_dropped() {
        futures::executor::block_on(async {
            use crate::host_internal::permissions::PermissionsService;
            use crate::latest::HostDevicePermissionRequest;
            use crate::platform::{PermissionDecision, ProductContext};
            for status in [
                PermissionAuthorizationStatus::Denied,
                PermissionAuthorizationStatus::NotDetermined,
                PermissionAuthorizationStatus::Authorized,
            ] {
                let directory = tempfile::tempdir().unwrap();
                let mut config = core_db_config(directory.path());
                config.readers = 1;
                let database = Db::open(config).await.unwrap();
                let store = RuntimeStore::open(database.clone(), &StubPlatform::default())
                    .await
                    .unwrap();
                let request =
                    PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera);
                let prompt = StubPlatform::default();
                if status == PermissionAuthorizationStatus::Authorized {
                    prompt
                        .device_permission_decisions
                        .lock()
                        .unwrap()
                        .push_back(PermissionDecision::AllowAlways);
                }
                prompt
                    .device_permission_decisions
                    .lock()
                    .unwrap()
                    .push_back(PermissionDecision::AllowOnce);
                let product = ProductContext::new("product.dot".to_string()).unwrap();
                let service = PermissionsService::new(&store, &prompt, &product, Arc::default());
                if status == PermissionAuthorizationStatus::NotDetermined {
                    service
                        .set_authorization_status(&request, PermissionAuthorizationStatus::Denied)
                        .await
                        .unwrap();
                }
                let (started, entered) = futures::channel::oneshot::channel();
                let (release, finish) = std::sync::mpsc::channel();
                database
                    .read_after_writes(move |connection| {
                        let mut started = Some(started);
                        connection.commit_hook(Some(move || {
                            if let Some(started) = started.take() {
                                started.send(()).unwrap();
                                finish.recv().unwrap();
                            }
                            false
                        }))?;
                        Ok(())
                    })
                    .await
                    .unwrap();
                let mut write = Box::pin(async {
                    if status == PermissionAuthorizationStatus::Authorized {
                        service
                            .authorize_device(HostDevicePermissionRequest::Camera)
                            .await
                            .map(|_| ())
                    } else {
                        service.set_authorization_status(&request, status).await
                    }
                });
                assert!(matches!(
                    futures::future::select(entered, &mut write).await,
                    futures::future::Either::Left((Ok(()), _))
                ));
                drop(write);
                let mut authorization =
                    Box::pin(service.authorize_device(HostDevicePermissionRequest::Camera));
                let initial = futures::poll!(&mut authorization);
                database
                    .read(|connection| {
                        Ok(
                            connection.query_row("SELECT COUNT(*) FROM core_state", [], |row| {
                                row.get::<_, i64>(0)
                            })?,
                        )
                    })
                    .await
                    .unwrap();
                let before_commit = match initial {
                    core::task::Poll::Pending => futures::poll!(&mut authorization),
                    ready => ready,
                };
                let still_waiting = before_commit.is_pending();
                release.send(()).unwrap();
                let outcome = match before_commit {
                    core::task::Poll::Ready(result) => result,
                    core::task::Poll::Pending => authorization.await,
                }
                .unwrap();
                let expected = if status == PermissionAuthorizationStatus::Denied {
                    PermissionAuthorizationStatus::Denied
                } else {
                    PermissionAuthorizationStatus::Authorized
                };
                assert_eq!(
                    (
                        still_waiting,
                        outcome,
                        prompt.device_permission_requests.lock().unwrap().len()
                    ),
                    (
                        true,
                        expected,
                        usize::from(status != PermissionAuthorizationStatus::Denied)
                    )
                );
            }
        });
    }

    #[test]
    fn encrypted_records_reopen_and_reject_other_rows_or_damaged_envelopes() {
        futures::executor::block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let secrets = StubPlatform::default();
            let database = Db::open(core_db_config(directory.path())).await.unwrap();
            let store = RuntimeStore::open(database.clone(), &secrets)
                .await
                .unwrap();
            let first = ProductStorageKey::new("first.dot", "same")
                .unwrap()
                .encode();
            let second = ProductStorageKey::new("second.dot", "same")
                .unwrap()
                .encode();
            let core = CoreStorageKey::StatementRenewalTargets;
            store
                .write(first.clone(), b"private product value".to_vec())
                .await
                .unwrap();
            store.write(second.clone(), Vec::new()).await.unwrap();
            store
                .write_core_storage(core.clone(), b"private core value".to_vec())
                .await
                .unwrap();
            let (product_ciphertext, core_ciphertext) = database
                .read(|connection| {
                    Ok((
                        connection.query_row(PRODUCT_VALUE, ["first.dot", "same"], |row| {
                            row.get::<_, Vec<u8>>(0)
                        })?,
                        connection.query_row("SELECT value FROM core_state", [], |row| {
                            row.get::<_, Vec<u8>>(0)
                        })?,
                    ))
                })
                .await
                .unwrap();
            assert!(
                !product_ciphertext
                    .windows(b"private product value".len())
                    .any(|bytes| bytes == b"private product value")
            );
            assert!(
                !core_ciphertext
                    .windows(b"private core value".len())
                    .any(|bytes| bytes == b"private core value")
            );
            drop(store);
            database.close().await.unwrap();
            let database = Db::open(core_db_config(directory.path())).await.unwrap();
            let store = RuntimeStore::open(database.clone(), &secrets)
                .await
                .unwrap();
            assert_eq!(
                (
                    store.read(first.clone()).await.unwrap(),
                    store.read(second.clone()).await.unwrap(),
                    store.read_core_storage(core.clone()).await.unwrap(),
                ),
                (
                    Some(b"private product value".to_vec()),
                    Some(Vec::new()),
                    Some(b"private core value".to_vec())
                )
            );
            let copied = product_ciphertext.clone();
            database
                .write(move |transaction| {
                    transaction.execute(
                        "UPDATE product_storage SET value = ?1 WHERE product_id = 'second.dot'",
                        [copied.clone()],
                    )?;
                    transaction.execute("UPDATE core_state SET value = ?1", [copied])?;
                    Ok(())
                })
                .await
                .unwrap();
            assert!(store.read(second).await.is_err());
            assert!(store.read_core_storage(core.clone()).await.is_err());
            let mut tampered = product_ciphertext.clone();
            *tampered.last_mut().unwrap() ^= 1;
            let mut unknown_version = product_ciphertext.clone();
            unknown_version[0] = 2;
            for invalid in [tampered, unknown_version, vec![1, 2], b"plaintext".to_vec()] {
                database
                    .write(move |transaction| {
                        transaction.execute(
                            "UPDATE product_storage SET value = ?1 WHERE product_id = 'first.dot'",
                            [invalid],
                        )?;
                        Ok(())
                    })
                    .await
                    .unwrap();
                assert!(store.read(first.clone()).await.is_err());
                assert!(
                    store
                        .subscribe_storage(first.clone())
                        .next()
                        .await
                        .unwrap()
                        .is_err()
                );
            }
            store.clear(first.clone()).await.unwrap();
            store.clear_core_storage(core.clone()).await.unwrap();
            assert_eq!(
                (
                    store.read(first).await.unwrap(),
                    store.read_core_storage(core).await.unwrap()
                ),
                (None, None)
            );
        });
    }

    #[test]
    fn a_started_commit_notifies_product_subscribers_after_its_waiter_is_dropped() {
        futures::executor::block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let database = Db::open(core_db_config(directory.path())).await.unwrap();
            let store = RuntimeStore::open(database.clone(), &StubPlatform::default())
                .await
                .unwrap();
            let key = ProductStorageKey::new("product.dot", "progress").unwrap();
            let mut subscription = store.subscribe_storage(key.encode());
            assert_eq!(
                subscription.next().await,
                Some(Ok(HostLocalStorageChangeItem { value: None }))
            );
            let encrypted = store.encrypt(&product_identity(&key), &[7]).unwrap();
            let (started, entered) = futures::channel::oneshot::channel();
            let (release, finish) = std::sync::mpsc::channel();
            let mut write = Box::pin(database.write(move |transaction| {
                transaction.execute(
                    "INSERT INTO product_storage VALUES (?1, ?2, ?3)",
                    params![key.product_id(), key.key(), encrypted],
                )?;
                started.send(()).unwrap();
                finish.recv().unwrap();
                Ok(())
            }));
            assert!(futures::poll!(&mut write).is_pending());
            entered.await.unwrap();
            drop(write);
            release.send(()).unwrap();
            let next = subscription.next();
            let timeout = futures_timer::Delay::new(std::time::Duration::from_secs(5));
            match futures::future::select(next, timeout).await {
                futures::future::Either::Left((value, _)) => assert_eq!(
                    value,
                    Some(Ok(HostLocalStorageChangeItem {
                        value: Some(vec![7])
                    }))
                ),
                futures::future::Either::Right(_) => {
                    panic!("a committed write with a dropped waiter did not notify its observer")
                }
            }
        });
    }

    #[test]
    fn protected_key_failures_never_replace_existing_records() {
        futures::executor::block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let database = Db::open(core_db_config(directory.path())).await.unwrap();
            struct RefusedWrite;
            #[async_trait]
            impl SecretCoreStorage for RefusedWrite {
                async fn read_secret_core_storage(
                    &self,
                    _: SecretCoreStorageKey,
                ) -> Result<Option<Vec<u8>>, GenericError> {
                    Ok(None)
                }
                async fn write_secret_core_storage(
                    &self,
                    _: SecretCoreStorageKey,
                    _: Vec<u8>,
                ) -> Result<(), GenericError> {
                    Err(GenericError {
                        reason: "protected write refused".to_string(),
                    })
                }
                async fn clear_secret_core_storage(
                    &self,
                    _: SecretCoreStorageKey,
                ) -> Result<(), GenericError> {
                    unreachable!()
                }
            }
            assert_eq!(
                RuntimeStore::open(database.clone(), &RefusedWrite)
                    .await
                    .err()
                    .unwrap()
                    .to_string(),
                "database protection failed: protected write refused"
            );
            let secrets = StubPlatform::default();
            let store = RuntimeStore::open(database.clone(), &secrets)
                .await
                .unwrap();
            let core = CoreStorageKey::StatementRenewalTargets;
            store
                .write_core_storage(core.clone(), vec![7])
                .await
                .unwrap();
            let slot = secret_core_storage_test_key(SecretCoreStorageKey::StorageEncryptionKey);
            let original = secrets.local_storage.lock().unwrap().remove(&slot).unwrap();
            assert!(matches!(
                RuntimeStore::open(database.clone(), &secrets).await,
                Err(DbError::Protection(_))
            ));
            assert!(!secrets.local_storage.lock().unwrap().contains_key(&slot));
            secrets
                .local_storage
                .lock()
                .unwrap()
                .insert(slot.clone(), vec![0; 31]);
            assert!(matches!(
                RuntimeStore::open(database.clone(), &secrets).await,
                Err(DbError::Protection(_))
            ));
            let unavailable = StubPlatform {
                local_storage_error: Some("protected storage unavailable"),
                ..StubPlatform::default()
            };
            assert_eq!(
                RuntimeStore::open(database.clone(), &unavailable)
                    .await
                    .err()
                    .unwrap()
                    .to_string(),
                "database protection failed: protected storage unavailable"
            );
            secrets
                .local_storage
                .lock()
                .unwrap()
                .insert(slot.clone(), vec![0; 32]);
            let wrong_key = RuntimeStore::open(database.clone(), &secrets)
                .await
                .unwrap();
            assert!(wrong_key.read_core_storage(core.clone()).await.is_err());
            secrets.local_storage.lock().unwrap().insert(slot, original);
            let recovered = RuntimeStore::open(database, &secrets).await.unwrap();
            assert_eq!(
                recovered.read_core_storage(core).await.unwrap(),
                Some(vec![7])
            );
        });
    }

    #[test]
    fn initialization_waits_for_protected_key_persistence_and_reuses_it() {
        futures::executor::block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let database = Db::open(core_db_config(directory.path())).await.unwrap();
            let (release, wait) = futures::channel::oneshot::channel();
            let secrets = StubPlatform::default();
            *secrets.secret_core_storage_write_gate.lock().unwrap() = Some(wait);
            let mut opening = Box::pin(RuntimeStore::open(database.clone(), &secrets));
            // The database query must finish before the protected write acknowledgement is awaited.
            let slot = secret_core_storage_test_key(SecretCoreStorageKey::StorageEncryptionKey);
            while !secrets.local_storage.lock().unwrap().contains_key(&slot) {
                assert!(futures::poll!(&mut opening).is_pending());
                futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
            }
            assert!(opening.as_mut().now_or_never().is_none());
            let persisted = secrets.local_storage.lock().unwrap().get(&slot).cloned();
            release.send(()).unwrap();
            let store = opening.await.unwrap();
            store
                .write_core_storage(CoreStorageKey::StatementRenewalTargets, vec![1])
                .await
                .unwrap();
            let reopened = RuntimeStore::open(database, &secrets).await.unwrap();
            let retained = secrets.local_storage.lock().unwrap().get(&slot).cloned();
            assert_eq!(
                (
                    retained,
                    reopened
                        .read_core_storage(CoreStorageKey::StatementRenewalTargets)
                        .await
                        .unwrap()
                ),
                (persisted, Some(vec![1]))
            );
        });
    }
}
