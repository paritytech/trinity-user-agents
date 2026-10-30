//! One-time receive keys: the X25519 keys a receiver creates so a sender can
//! encrypt a cheque to it.

use std::sync::Weak;

use truapi::latest::{GenericError, NftPurseReceiveKey, X25519PublicKey};

use super::{NftPurses, dao};
use crate::{
	host_logic::x25519_seal::{X25519Keypair, X25519PrivateKey},
	nft::PurseId,
	platform::ProductId,
	runtime::AuthorityError,
	store::DbError,
};

/// Prefix of the encrypted storage key that holds a receive key's secret.
const SECRET_KEY_PREFIX: &[u8] = b"nft/receive-key/";

/// Why a receive key was not created.
#[derive(Debug, thiserror::Error)]
pub enum CreateReceiveKeyError {
	/// No purse has this id.
	#[error("purse {0:?} does not exist")]
	UnknownPurse(PurseId),
	/// The platform randomness source failed.
	#[error("failed to generate a receive key: {0}")]
	Random(getrandom::Error),
	/// The host did not store the secret.
	#[error("the receive key secret was not stored: {}", .0.reason)]
	SecretNotStored(GenericError),
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// The X25519 secret that opens `Static` cheques, the same for every product.
/// The signing host is its one production implementation, so the key it
/// returns can change in one place.
pub trait StaticReceiveSecret: Send + Sync {
	/// The secret. Fails without a session.
	fn static_receive_secret(&self) -> Result<X25519PrivateKey, AuthorityError>;
}

/// For a holder that owns the purses, which must not keep it alive in turn.
/// Once the holder is gone there is no secret.
impl<Holder: StaticReceiveSecret> StaticReceiveSecret for Weak<Holder> {
	fn static_receive_secret(&self) -> Result<X25519PrivateKey, AuthorityError> {
		let Some(holder) = self.upgrade() else {
			let reason = "the static receive secret holder was dropped".to_string();
			return Err(AuthorityError::Unavailable { reason });
		};
		holder.static_receive_secret()
	}
}

/// Why the secret of a receive key could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ReceiveSecretError {
	/// The host storage failed.
	#[error("reading the receive key secret failed: {}", .0.reason)]
	Read(GenericError),
	/// The key was recorded, but no secret is stored for it.
	#[error("no secret is stored for receive key {}", hex::encode(.0.0))]
	Missing(NftPurseReceiveKey),
	/// The stored secret is not 32 bytes.
	#[error("the stored secret of receive key {} has {length} bytes, expected 32", hex::encode(.key.0))]
	Malformed {
		/// The receive key.
		key: NftPurseReceiveKey,
		/// The length of the stored value.
		length: usize,
	},
}

/// The encrypted storage key under which the secret of `key` is kept.
fn receive_secret_storage_key(key: &NftPurseReceiveKey) -> Vec<u8> {
	[SECRET_KEY_PREFIX, &key.0].concat()
}

impl NftPurses {
	/// A fresh receive key whose cheques go into `into`, made by `creator`.
	/// Stores its secret in encrypted storage before recording the key, so a
	/// recorded key always has its secret. Needs no grant: receiving only adds
	/// items to a purse.
	pub async fn create_receive_key(
		&self,
		creator: &ProductId,
		into: PurseId,
	) -> Result<NftPurseReceiveKey, CreateReceiveKeyError> {
		if self.ledger.purse(into).await?.is_none() {
			return Err(CreateReceiveKeyError::UnknownPurse(into));
		}
		let keypair = X25519Keypair::generate().map_err(CreateReceiveKeyError::Random)?;
		let key = NftPurseReceiveKey(keypair.public_key().0);
		let storage_key = receive_secret_storage_key(&key);
		self.encrypted
			.write_encrypted(&storage_key, keypair.private_key().as_bytes())
			.await
			.map_err(CreateReceiveKeyError::SecretNotStored)?;
		let creator = creator.clone();
		self.db
			.write(move |tx| dao::insert_receive_key(tx, &key, into, &creator))
			.await?;
		Ok(key)
	}

	/// The public half of the static receive secret. Fails without a session.
	pub fn static_receive_key(&self) -> Result<X25519PublicKey, AuthorityError> {
		Ok(self.static_secret.static_receive_secret()?.public_key())
	}

	/// The secret of the receive key `key`, made on this host.
	pub async fn receive_secret(
		&self,
		key: &NftPurseReceiveKey,
	) -> Result<X25519PrivateKey, ReceiveSecretError> {
		let storage_key = receive_secret_storage_key(key);
		let stored = self
			.encrypted
			.read_encrypted(&storage_key)
			.await
			.map_err(ReceiveSecretError::Read)?;
		let stored = stored.ok_or(ReceiveSecretError::Missing(*key))?;
		let secret = <[u8; 32]>::try_from(stored.as_slice())
			.map_err(|_| ReceiveSecretError::Malformed { key: *key, length: stored.len() })?;
		Ok(X25519PrivateKey::from(secret))
	}
}

#[cfg(test)]
mod tests {
	use futures::executor::block_on;

	use super::*;
	use crate::{nft_purse::test_purses, store::open_temporary_core_db};

	/// A deposit must not take a secret it cannot read for one that was never
	/// stored, nor sign with a value that is not a secret.
	#[test]
	fn a_receive_key_secret_that_is_absent_or_malformed_is_an_error() {
		let (_dir, db) = open_temporary_core_db();
		let purses = test_purses(db);
		let missing = NftPurseReceiveKey([0x01; 32]);
		let malformed = NftPurseReceiveKey([0x02; 32]);
		let storage_key = receive_secret_storage_key(&malformed);
		block_on(purses.encrypted.write_encrypted(&storage_key, &[0x03; 16])).unwrap();

		let absent = block_on(purses.receive_secret(&missing));
		let short = block_on(purses.receive_secret(&malformed));

		assert!(matches!(absent, Err(ReceiveSecretError::Missing(key)) if key == missing));
		assert!(matches!(short, Err(ReceiveSecretError::Malformed { length: 16, .. })));
	}
}
