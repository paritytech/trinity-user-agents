//! Secrets products keep through `SecretsStorage`: a row in the core
//! database's `product_secret` table, and the secret itself in the host's
//! [`EncryptedStorage`].
//!
//! The row records which product owns a secret, so the core can clear secrets
//! without the host listing anything. Every secret is written after its row
//! and deleted before it, so a crash in between leaves at most a row whose
//! secret is missing, which reads as absent and the next clear removes. A
//! secret is never left behind without a row.

use std::sync::Arc;

use rusqlite::{OptionalExtension, named_params};
use truapi::latest::GenericError;
use zeroize::Zeroizing;

use crate::{
	encrypted_storage::EncryptedStorage,
	store::{Db, DbError},
};

/// The id of a product's secret, derived from the account, the product and
/// the product's key.
pub type SecretId = [u8; 32];

/// Products' secrets, over the core database and the host's encrypted
/// storage. Cloning shares both.
#[derive(Clone)]
pub struct ProductSecrets {
	db: Db,
	encrypted: Arc<dyn EncryptedStorage>,
}

const UPSERT: &str = "INSERT INTO product_secret (id, product_id) VALUES (:id, :product_id)
	ON CONFLICT (id) DO UPDATE SET product_id = :product_id";

const EXISTS: &str = "SELECT 1 FROM product_secret WHERE id = :id";

const DELETE: &str = "DELETE FROM product_secret WHERE id = :id";

const IDS: &str = "SELECT id FROM product_secret
	WHERE :product_id IS NULL OR product_id = :product_id";

/// The encrypted storage key of the secret with `id`.
fn storage_key(id: &SecretId) -> Vec<u8> {
	[b"product-secret/".as_slice(), id].concat()
}

impl ProductSecrets {
	/// The secrets indexed in `db` and kept in `encrypted`.
	pub fn new(db: Db, encrypted: Arc<dyn EncryptedStorage>) -> Self {
		Self { db, encrypted }
	}

	/// Keeps `secret` under `id`, owned by `product_id`, replacing any secret
	/// already under it.
	pub async fn put(
		&self,
		id: &SecretId,
		product_id: &str,
		secret: &[u8],
	) -> Result<(), GenericError> {
		let row_id = *id;
		let product_id = product_id.to_owned();
		self.db
			.write(move |tx| {
				tx.prepare_cached(UPSERT)?
					.execute(named_params! { ":id": row_id, ":product_id": product_id })?;
				Ok(())
			})
			.await
			.map_err(db_failure)?;
		self.encrypted.write_encrypted(&storage_key(id), secret).await
	}

	/// The secret under `id`, or `None` when none is kept or the host lost
	/// it. A host that cannot read its storage is an error.
	pub async fn get(&self, id: &SecretId) -> Result<Option<Zeroizing<Vec<u8>>>, GenericError> {
		let row_id = *id;
		let indexed = self
			.db
			.read(move |conn| {
				Ok(conn
					.prepare_cached(EXISTS)?
					.query_row(named_params! { ":id": row_id }, |_| Ok(()))
					.optional()?
					.is_some())
			})
			.await
			.map_err(db_failure)?;
		if !indexed {
			return Ok(None);
		}
		self.encrypted.read_encrypted(&storage_key(id)).await
	}

	/// Deletes the secret under `id`. Deleting none succeeds.
	pub async fn delete(&self, id: &SecretId) -> Result<(), GenericError> {
		self.encrypted.delete_encrypted(&storage_key(id)).await?;
		let row_id = *id;
		self.db
			.write(move |tx| {
				tx.prepare_cached(DELETE)?.execute(named_params! { ":id": row_id })?;
				Ok(())
			})
			.await
			.map_err(db_failure)
	}

	/// Deletes every secret `product_id` owns, or every product's when it is
	/// `None`. Stops at the first failure; what is left is still indexed, so
	/// calling it again finishes the job.
	pub async fn clear(&self, product_id: Option<&str>) -> Result<(), GenericError> {
		let product_id = product_id.map(str::to_owned);
		let ids: Vec<SecretId> = self
			.db
			.read(move |conn| {
				let mut statement = conn.prepare_cached(IDS)?;
				let ids = statement
					.query_map(named_params! { ":product_id": product_id }, |row| row.get(0))?
					.collect::<Result<_, _>>()?;
				Ok(ids)
			})
			.await
			.map_err(db_failure)?;
		for id in ids {
			self.delete(&id).await?;
		}
		Ok(())
	}
}

/// A core database failure, as the reason a secret operation failed.
fn db_failure(error: DbError) -> GenericError {
	GenericError { reason: format!("core database: {error}") }
}

#[cfg(test)]
mod tests {
	use futures::executor::block_on;

	use super::*;
	use crate::{
		encrypted_storage::MemoryEncryptedStorage,
		store::{DbConfig, DbLocation},
	};

	fn id(n: u8) -> SecretId {
		[n; 32]
	}

	fn secrets() -> (ProductSecrets, Arc<MemoryEncryptedStorage>) {
		let db = block_on(Db::open(DbConfig {
			location: DbLocation::Memory,
			migrations: crate::store::core_migrations,
			readers: 1,
		}))
		.expect("database opens");
		let storage = Arc::new(MemoryEncryptedStorage::default());
		(ProductSecrets::new(db, storage.clone()), storage)
	}

	// Only the secret goes to the host, under the feature's prefix. Clearing
	// one product leaves the others' secrets, and clearing all leaves nothing
	// in the host's storage.
	#[test]
	fn secrets_are_kept_per_product_until_cleared() {
		let (secrets, storage) = secrets();
		block_on(secrets.put(&id(1), "a.dot", b"secret")).unwrap();
		block_on(secrets.put(&id(2), "b.dot", b"secret")).unwrap();
		let host_keys = storage.stored().len();

		block_on(secrets.clear(Some("a.dot"))).unwrap();
		let after_product = (
			block_on(secrets.get(&id(1))).unwrap(),
			block_on(secrets.get(&id(2))).unwrap().as_deref().cloned(),
		);
		block_on(secrets.clear(None)).unwrap();

		assert_eq!(host_keys, 2);
		assert_eq!(after_product, (None, Some(b"secret".to_vec())));
		assert!(storage.stored().is_empty());
	}

	/// Encrypted storage the host cannot read.
	struct LockedStorage;

	#[async_trait::async_trait]
	impl EncryptedStorage for LockedStorage {
		async fn read_encrypted(
			&self,
			_key: &[u8],
		) -> Result<Option<Zeroizing<Vec<u8>>>, GenericError> {
			Err(GenericError { reason: "locked".to_owned() })
		}

		async fn write_encrypted(&self, _key: &[u8], _value: &[u8]) -> Result<(), GenericError> {
			Ok(())
		}

		async fn delete_encrypted(&self, _key: &[u8]) -> Result<(), GenericError> {
			Ok(())
		}
	}

	// Storage the host cannot read is an error, not an absent secret, so a
	// locked Keychain is never taken for a lost one.
	#[test]
	fn an_unreadable_store_is_an_error() {
		let (working, _) = secrets();
		let locked = ProductSecrets::new(working.db.clone(), Arc::new(LockedStorage));
		block_on(working.put(&id(1), "a.dot", b"s")).unwrap();

		let read = block_on(locked.get(&id(1)));

		assert_eq!(read, Err(GenericError { reason: "locked".to_owned() }));
	}

	// A row whose secret the host lost, as a crash between the two writes
	// leaves, reads as absent, and a secret with no row is never served.
	#[test]
	fn a_secret_is_served_only_with_both_its_row_and_its_value() {
		let (secrets, storage) = secrets();
		block_on(secrets.put(&id(1), "a.dot", b"s")).unwrap();
		block_on(storage.delete_encrypted(&storage_key(&id(1)))).unwrap();
		block_on(storage.write_encrypted(&storage_key(&id(2)), b"s")).unwrap();

		assert_eq!(block_on(secrets.get(&id(1))).unwrap(), None);
		assert_eq!(block_on(secrets.get(&id(2))).unwrap(), None);
	}
}
