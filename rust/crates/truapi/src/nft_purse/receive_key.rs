//! One-time receive keys: the X25519 keys a receiver creates so a sender can
//! encrypt a cheque to it.

use std::sync::Weak;

use truapi::latest::{GenericError, NftPurseReceiveKey};

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
/// It has one implementation, so the key it returns can change in one place.
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
	/// The key was recorded, but no 32-byte secret is stored for it.
	#[error("no secret is stored for receive key {}", hex::encode(.0.0))]
	Missing(NftPurseReceiveKey),
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
		let secret = stored.and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok());
		secret.map(X25519PrivateKey::from).ok_or(ReceiveSecretError::Missing(*key))
	}
}
