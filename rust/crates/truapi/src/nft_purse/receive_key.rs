//! One-time receive keys: the X25519 keys a receiver creates so a sender can
//! encrypt a cheque to it.

use truapi::latest::{GenericError, NftPurseReceiveKey};

use super::{NftPurses, dao};
use crate::{
	host_logic::x25519_seal::X25519Keypair, nft::PurseId, platform::ProductId, store::DbError,
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
}
