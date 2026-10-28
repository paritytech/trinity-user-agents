//! Secrets the core keeps for its features: an index row in the core
//! database, and the secret itself in the host's [`EncryptedStorage`].
//!
//! The row records which feature a secret belongs to and which product and
//! account own it, so the core can count and clear secrets without the host
//! listing anything. Every secret is written after its row and deleted before
//! it, so a crash in between leaves at most a row whose secret is missing,
//! which reads as absent and the next clear removes. A secret is never left
//! behind without a row.

use std::sync::Arc;

use rusqlite::{OptionalExtension, named_params};
use truapi::latest::GenericError;
use zeroize::Zeroizing;

use crate::{
	encrypted_storage::EncryptedStorage,
	store::{Db, DbError},
};

/// The feature a secret belongs to. Its name prefixes the secret's key in
/// encrypted storage, so two features never share a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
	/// A secret a product keeps through `SecretsStorage`.
	ProductSecret,
}

impl SecretKind {
	fn name(self) -> &'static str {
		match self {
			Self::ProductSecret => "product-secret",
		}
	}

	/// The encrypted storage key of the secret with `id`.
	fn storage_key(self, id: &[u8]) -> Vec<u8> {
		[self.name().as_bytes(), b"/", id].concat()
	}
}

/// The product, and optionally the signed-in account, a secret belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecretOwner<'a> {
	/// The product that owns the secret.
	pub product_id: &'a str,
	/// The account the secret was kept for, when it belongs to one.
	pub account: Option<&'a [u8; 32]>,
}

/// The core's secrets, over the core database and the host's encrypted
/// storage. Cloning shares both.
#[derive(Clone)]
pub struct CoreSecrets {
	db: Db,
	encrypted: Arc<dyn EncryptedStorage>,
}

const UPSERT: &str = "INSERT INTO core_secret (kind, id, product_id, account)
	VALUES (:kind, :id, :product_id, :account)
	ON CONFLICT (kind, id) DO UPDATE SET product_id = :product_id, account = :account";

const EXISTS: &str = "SELECT 1 FROM core_secret WHERE kind = :kind AND id = :id";

const DELETE: &str = "DELETE FROM core_secret WHERE kind = :kind AND id = :id";

const COUNT: &str = "SELECT COUNT(*) FROM core_secret
	WHERE kind = :kind AND product_id = :product_id AND account IS :account";

const IDS: &str = "SELECT id FROM core_secret
	WHERE kind = :kind AND (:product_id IS NULL OR product_id = :product_id)";

impl CoreSecrets {
	/// The secrets indexed in `db` and kept in `encrypted`.
	pub fn new(db: Db, encrypted: Arc<dyn EncryptedStorage>) -> Self {
		Self { db, encrypted }
	}

	/// Keeps `secret` under `id`, owned by `owner`, replacing any secret
	/// already under it. When the host fails to store a new secret, its row
	/// is removed again, so it does not count toward the owner's secrets.
	pub async fn put(
		&self,
		kind: SecretKind,
		id: &[u8],
		owner: SecretOwner<'_>,
		secret: &[u8],
	) -> Result<(), GenericError> {
		let row_id = id.to_vec();
		let product_id = owner.product_id.to_owned();
		let account = owner.account.copied();
		let inserted = self
			.db
			.write(move |tx| {
				let existed = tx
					.prepare_cached(EXISTS)?
					.query_row(named_params! { ":kind": kind.name(), ":id": row_id }, |_| Ok(()))
					.optional()?
					.is_some();
				tx.prepare_cached(UPSERT)?.execute(named_params! {
					":kind": kind.name(),
					":id": row_id,
					":product_id": product_id,
					":account": account,
				})?;
				Ok(!existed)
			})
			.await
			.map_err(db_failure)?;
		let written = self.encrypted.write_encrypted(&kind.storage_key(id), secret).await;
		if written.is_err() &&
			inserted && let Err(error) = self.delete_row(kind, id).await
		{
			tracing::warn!(reason = %error.reason, "a secret the host did not store left its row behind");
		}
		written
	}

	/// The secret under `id`, or `None` when the core keeps none or the host
	/// lost it. A host that cannot read its storage is an error.
	pub async fn get(
		&self,
		kind: SecretKind,
		id: &[u8],
	) -> Result<Option<Zeroizing<Vec<u8>>>, GenericError> {
		let row_id = id.to_vec();
		let indexed = self
			.db
			.read(move |conn| {
				Ok(conn
					.prepare_cached(EXISTS)?
					.query_row(named_params! { ":kind": kind.name(), ":id": row_id }, |_| Ok(()))
					.optional()?
					.is_some())
			})
			.await
			.map_err(db_failure)?;
		if !indexed {
			return Ok(None);
		}
		self.encrypted.read_encrypted(&kind.storage_key(id)).await
	}

	/// Deletes the secret under `id`. Deleting none succeeds.
	pub async fn delete(&self, kind: SecretKind, id: &[u8]) -> Result<(), GenericError> {
		self.encrypted.delete_encrypted(&kind.storage_key(id)).await?;
		self.delete_row(kind, id).await
	}

	/// How many secrets of `kind` `owner` has.
	pub async fn count(
		&self,
		kind: SecretKind,
		owner: SecretOwner<'_>,
	) -> Result<u32, GenericError> {
		let product_id = owner.product_id.to_owned();
		let account = owner.account.copied();
		self.db
			.read(move |conn| {
				Ok(conn.prepare_cached(COUNT)?.query_row(
					named_params! {
						":kind": kind.name(),
						":product_id": product_id,
						":account": account,
					},
					|row| row.get(0),
				)?)
			})
			.await
			.map_err(db_failure)
	}

	/// Deletes every secret of `kind` that `product_id` owns, or every secret
	/// of `kind` when it is `None`. Stops at the first failure; what is left
	/// is still indexed, so calling it again finishes the job.
	pub async fn clear(
		&self,
		kind: SecretKind,
		product_id: Option<&str>,
	) -> Result<(), GenericError> {
		let product_id = product_id.map(str::to_owned);
		let ids: Vec<Vec<u8>> = self
			.db
			.read(move |conn| {
				let mut statement = conn.prepare_cached(IDS)?;
				let ids = statement
					.query_map(
						named_params! { ":kind": kind.name(), ":product_id": product_id },
						|row| row.get(0),
					)?
					.collect::<Result<_, _>>()?;
				Ok(ids)
			})
			.await
			.map_err(db_failure)?;
		for id in ids {
			self.delete(kind, &id).await?;
		}
		Ok(())
	}

	async fn delete_row(&self, kind: SecretKind, id: &[u8]) -> Result<(), GenericError> {
		let row_id = id.to_vec();
		self.db
			.write(move |tx| {
				tx.prepare_cached(DELETE)?
					.execute(named_params! { ":kind": kind.name(), ":id": row_id })?;
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

	fn secrets() -> (CoreSecrets, Arc<MemoryEncryptedStorage>) {
		let db = block_on(Db::open(DbConfig {
			location: DbLocation::Memory,
			migrations: crate::store::core_migrations,
			readers: 1,
		}))
		.expect("database opens");
		let storage = Arc::new(MemoryEncryptedStorage::default());
		(CoreSecrets::new(db, storage.clone()), storage)
	}

	fn owner<'a>(product_id: &'a str, account: &'a [u8; 32]) -> SecretOwner<'a> {
		SecretOwner { product_id, account: Some(account) }
	}

	// Only the secret goes to the host, under the feature's prefix, and it
	// reads back until deleted.
	#[test]
	fn a_kept_secret_reads_back_until_deleted() {
		let (secrets, storage) = secrets();
		let kind = SecretKind::ProductSecret;
		block_on(secrets.put(kind, b"id", owner("a.dot", &ALICE), b"secret")).unwrap();

		let read = block_on(secrets.get(kind, b"id")).unwrap();
		let host_keys: Vec<_> = storage.stored().into_keys().collect();
		block_on(secrets.delete(kind, b"id")).unwrap();
		block_on(secrets.delete(kind, b"id")).unwrap();

		assert_eq!(read.as_deref().map(Vec::as_slice), Some(b"secret".as_slice()));
		assert_eq!(host_keys, vec![b"product-secret/id".to_vec()]);
		assert_eq!(block_on(secrets.get(kind, b"id")).unwrap(), None);
		assert!(storage.stored().is_empty());
	}

	// Counting is per product and account, and replacing a secret does not
	// count it twice.
	#[test]
	fn secrets_are_counted_per_product_and_account() {
		let (secrets, _) = secrets();
		let kind = SecretKind::ProductSecret;
		for (id, product, account) in [
			(&b"1"[..], "a.dot", &ALICE),
			(b"2", "a.dot", &ALICE),
			(b"3", "a.dot", &BOB),
			(b"4", "b.dot", &ALICE),
		] {
			block_on(secrets.put(kind, id, owner(product, account), b"s")).unwrap();
		}
		block_on(secrets.put(kind, b"1", owner("a.dot", &ALICE), b"again")).unwrap();

		let counts = [
			block_on(secrets.count(kind, owner("a.dot", &ALICE))).unwrap(),
			block_on(secrets.count(kind, owner("a.dot", &BOB))).unwrap(),
			block_on(secrets.count(kind, owner("b.dot", &ALICE))).unwrap(),
			block_on(secrets.count(kind, SecretOwner { product_id: "a.dot", account: None }))
				.unwrap(),
		];

		assert_eq!(counts, [2, 1, 1, 0]);
	}

	// Clearing one product leaves the others' secrets, and clearing all
	// leaves nothing in the host's storage.
	#[test]
	fn clearing_deletes_one_products_secrets_or_all() {
		let (secrets, storage) = secrets();
		let kind = SecretKind::ProductSecret;
		block_on(secrets.put(kind, b"1", owner("a.dot", &ALICE), b"s")).unwrap();
		block_on(secrets.put(kind, b"2", owner("b.dot", &ALICE), b"s")).unwrap();

		block_on(secrets.clear(kind, Some("a.dot"))).unwrap();
		let after_product = (
			block_on(secrets.get(kind, b"1")).unwrap(),
			block_on(secrets.get(kind, b"2")).unwrap().is_some(),
		);
		block_on(secrets.clear(kind, None)).unwrap();

		assert_eq!(after_product, (None, true));
		assert!(storage.stored().is_empty());
		assert_eq!(block_on(secrets.count(kind, owner("b.dot", &ALICE))).unwrap(), 0);
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
		let broken = CoreSecrets::new(working.db.clone(), Arc::new(BrokenStorage));
		let kind = SecretKind::ProductSecret;
		block_on(working.put(kind, b"kept", owner("a.dot", &ALICE), b"s")).unwrap();

		let stored = block_on(broken.put(kind, b"new", owner("a.dot", &ALICE), b"s"));
		let count = block_on(broken.count(kind, owner("a.dot", &ALICE))).unwrap();
		let read = block_on(broken.get(kind, b"kept"));

		assert_eq!(stored, Err(GenericError { reason: "full".to_owned() }));
		assert_eq!(count, 1);
		assert_eq!(read, Err(GenericError { reason: "locked".to_owned() }));
	}

	// A row whose secret the host lost, as a crash between the two writes
	// leaves, reads as absent, and a secret with no row is never served.
	#[test]
	fn a_secret_is_served_only_with_both_its_row_and_its_value() {
		let (secrets, storage) = secrets();
		let kind = SecretKind::ProductSecret;
		block_on(secrets.put(kind, b"lost", owner("a.dot", &ALICE), b"s")).unwrap();
		block_on(storage.delete_encrypted(b"product-secret/lost")).unwrap();
		block_on(storage.write_encrypted(b"product-secret/unindexed", b"s")).unwrap();

		assert_eq!(block_on(secrets.get(kind, b"lost")).unwrap(), None);
		assert_eq!(block_on(secrets.get(kind, b"unindexed")).unwrap(), None);
	}
}
