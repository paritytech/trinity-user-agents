//! Secrets products keep through `SecretsStorage`: a row in the core
//! database's `product_secret` table, and the secret itself in the host's
//! [`EncryptedStorage`].
//!
//! The row records which product and account own a secret, so the core can
//! count and clear secrets without the host listing anything. Every secret is
//! written after its row and deleted before it, so a crash in between leaves
//! at most a row whose secret is missing, which reads as absent and the next
//! clear removes. A secret is never left behind without a row.

use std::sync::Arc;

use rusqlite::{OptionalExtension, named_params};
use truapi::latest::GenericError;
use zeroize::Zeroizing;

use crate::{
	encrypted_storage::EncryptedStorage,
	store::{Db, DbError},
};

/// The id of a product's secret, derived from its owner and the product's key.
pub type SecretId = [u8; 32];

/// The product and signed-in account a secret belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecretOwner<'a> {
	/// The product that owns the secret.
	pub product_id: &'a str,
	/// The account the secret was kept for.
	pub account: &'a [u8; 32],
}

/// Products' secrets, over the core database and the host's encrypted
/// storage. Cloning shares both.
#[derive(Clone)]
pub struct ProductSecrets {
	db: Db,
	encrypted: Arc<dyn EncryptedStorage>,
}

const UPSERT: &str = "INSERT INTO product_secret (id, product_id, account)
	VALUES (:id, :product_id, :account)
	ON CONFLICT (id) DO UPDATE SET product_id = :product_id, account = :account";

const EXISTS: &str = "SELECT 1 FROM product_secret WHERE id = :id";

const DELETE: &str = "DELETE FROM product_secret WHERE id = :id";

const COUNT: &str = "SELECT COUNT(*) FROM product_secret
	WHERE product_id = :product_id AND account = :account";

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

	/// Keeps `secret` under `id`, owned by `owner`, replacing any secret
	/// already under it. When the host fails to store a new secret, its row
	/// is removed again, so it does not count toward the owner's secrets.
	pub async fn put(
		&self,
		id: &SecretId,
		owner: SecretOwner<'_>,
		secret: &[u8],
	) -> Result<(), GenericError> {
		let row_id = *id;
		let product_id = owner.product_id.to_owned();
		let account = *owner.account;
		let inserted = self
			.db
			.write(move |tx| {
				let existed = tx
					.prepare_cached(EXISTS)?
					.query_row(named_params! { ":id": row_id }, |_| Ok(()))
					.optional()?
					.is_some();
				tx.prepare_cached(UPSERT)?.execute(named_params! {
					":id": row_id,
					":product_id": product_id,
					":account": account,
				})?;
				Ok(!existed)
			})
			.await
			.map_err(db_failure)?;
		let written = self.encrypted.write_encrypted(&storage_key(id), secret).await;
		if written.is_err() &&
			inserted && let Err(error) = self.delete_row(id).await
		{
			tracing::warn!(reason = %error.reason, "a secret the host did not store left its row behind");
		}
		written
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
		self.delete_row(id).await
	}

	/// How many secrets `owner` has.
	pub async fn count(&self, owner: SecretOwner<'_>) -> Result<u32, GenericError> {
		let product_id = owner.product_id.to_owned();
		let account = *owner.account;
		self.db
			.read(move |conn| {
				Ok(conn.prepare_cached(COUNT)?.query_row(
					named_params! { ":product_id": product_id, ":account": account },
					|row| row.get(0),
				)?)
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

	async fn delete_row(&self, id: &SecretId) -> Result<(), GenericError> {
		let row_id = *id;
		self.db
			.write(move |tx| {
				tx.prepare_cached(DELETE)?.execute(named_params! { ":id": row_id })?;
				Ok(())
			})
			.await
			.map_err(db_failure)
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

	const ALICE: [u8; 32] = [1; 32];
	const BOB: [u8; 32] = [2; 32];

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

	fn owner<'a>(product_id: &'a str, account: &'a [u8; 32]) -> SecretOwner<'a> {
		SecretOwner { product_id, account }
	}

	// Only the secret goes to the host, under the feature's prefix, and it
	// reads back until deleted.
	#[test]
	fn a_kept_secret_reads_back_until_deleted() {
		let (secrets, storage) = secrets();
		block_on(secrets.put(&id(1), owner("a.dot", &ALICE), b"secret")).unwrap();

		let read = block_on(secrets.get(&id(1))).unwrap();
		let host_keys: Vec<_> = storage.stored().into_keys().collect();
		block_on(secrets.delete(&id(1))).unwrap();
		block_on(secrets.delete(&id(1))).unwrap();

		assert_eq!(read.as_deref().map(Vec::as_slice), Some(b"secret".as_slice()));
		assert_eq!(host_keys, vec![storage_key(&id(1))]);
		assert_eq!(block_on(secrets.get(&id(1))).unwrap(), None);
		assert!(storage.stored().is_empty());
	}

	// Counting is per product and account, and replacing a secret does not
	// count it twice.
	#[test]
	fn secrets_are_counted_per_product_and_account() {
		let (secrets, _) = secrets();
		for (id, product, account) in [
			(id(1), "a.dot", &ALICE),
			(id(2), "a.dot", &ALICE),
			(id(3), "a.dot", &BOB),
			(id(4), "b.dot", &ALICE),
		] {
			block_on(secrets.put(&id, owner(product, account), b"s")).unwrap();
		}
		block_on(secrets.put(&id(1), owner("a.dot", &ALICE), b"again")).unwrap();

		let counts = [
			block_on(secrets.count(owner("a.dot", &ALICE))).unwrap(),
			block_on(secrets.count(owner("a.dot", &BOB))).unwrap(),
			block_on(secrets.count(owner("b.dot", &ALICE))).unwrap(),
		];

		assert_eq!(counts, [2, 1, 1]);
	}

	// Clearing one product leaves the others' secrets, and clearing all
	// leaves nothing in the host's storage.
	#[test]
	fn clearing_deletes_one_products_secrets_or_all() {
		let (secrets, storage) = secrets();
		block_on(secrets.put(&id(1), owner("a.dot", &ALICE), b"s")).unwrap();
		block_on(secrets.put(&id(2), owner("b.dot", &ALICE), b"s")).unwrap();

		block_on(secrets.clear(Some("a.dot"))).unwrap();
		let after_product = (
			block_on(secrets.get(&id(1))).unwrap(),
			block_on(secrets.get(&id(2))).unwrap().is_some(),
		);
		block_on(secrets.clear(None)).unwrap();

		assert_eq!(after_product, (None, true));
		assert!(storage.stored().is_empty());
		assert_eq!(block_on(secrets.count(owner("b.dot", &ALICE))).unwrap(), 0);
	}

	/// Encrypted storage the host cannot write or read.
	struct BrokenStorage;

	#[async_trait::async_trait]
	impl EncryptedStorage for BrokenStorage {
		async fn read_encrypted(
			&self,
			_key: &[u8],
		) -> Result<Option<Zeroizing<Vec<u8>>>, GenericError> {
			Err(GenericError { reason: "locked".to_owned() })
		}

		async fn write_encrypted(&self, _key: &[u8], _value: &[u8]) -> Result<(), GenericError> {
			Err(GenericError { reason: "full".to_owned() })
		}

		async fn delete_encrypted(&self, _key: &[u8]) -> Result<(), GenericError> {
			Ok(())
		}
	}

	// A secret the host failed to store must not count toward its owner's
	// secrets, and storage the host cannot read is an error, not an absent
	// secret, so a locked Keychain is never taken for a lost one.
	#[test]
	fn a_failed_store_leaves_no_row_and_an_unreadable_store_is_an_error() {
		let (working, _) = secrets();
		let broken = ProductSecrets::new(working.db.clone(), Arc::new(BrokenStorage));
		block_on(working.put(&id(1), owner("a.dot", &ALICE), b"s")).unwrap();

		let stored = block_on(broken.put(&id(2), owner("a.dot", &ALICE), b"s"));
		let count = block_on(broken.count(owner("a.dot", &ALICE))).unwrap();
		let read = block_on(broken.get(&id(1)));

		assert_eq!(stored, Err(GenericError { reason: "full".to_owned() }));
		assert_eq!(count, 1);
		assert_eq!(read, Err(GenericError { reason: "locked".to_owned() }));
	}

	// A row whose secret the host lost, as a crash between the two writes
	// leaves, reads as absent, and a secret with no row is never served.
	#[test]
	fn a_secret_is_served_only_with_both_its_row_and_its_value() {
		let (secrets, storage) = secrets();
		block_on(secrets.put(&id(1), owner("a.dot", &ALICE), b"s")).unwrap();
		block_on(storage.delete_encrypted(&storage_key(&id(1)))).unwrap();
		block_on(storage.write_encrypted(&storage_key(&id(2)), b"s")).unwrap();

		assert_eq!(block_on(secrets.get(&id(1))).unwrap(), None);
		assert_eq!(block_on(secrets.get(&id(2))).unwrap(), None);
	}
}
