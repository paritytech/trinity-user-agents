//! Cheques: item secrets sealed to a recipient's X25519 key, with the items
//! in the clear so the recipient can see what it receives.

use std::collections::BTreeSet;

use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use truapi::latest::{
	NftPurseCheque, NftPurseChequeDestination, NftPurseChequeId, NftPurseChequeRecipient,
	NftPurseItemKey,
};
use zeroize::Zeroizing;

use super::NftPurses;
use crate::{
	host_logic::x25519_seal::{SealError, SealedBox, seal_to},
	nft::{HandoffError, HandoffRequest, HandoffTag, ItemKey, ItemSecret, PurseId},
	platform::ProductId,
};

/// Version of the cheque format this host writes.
const CHEQUE_VERSION: u8 = 0;

/// Domain of the hash that derives an ephemeral cheque's id from its receive key.
const CHEQUE_ID_DOMAIN: &[u8] = b"nft-purse/cheque-id";

/// The id a cheque for `destination` carries: derived from the receive key,
/// or the static destination's own.
pub fn cheque_id_for(destination: &NftPurseChequeDestination) -> NftPurseChequeId {
	match destination {
		NftPurseChequeDestination::Ephemeral(key) => {
			let preimage = [CHEQUE_ID_DOMAIN, &key.0].concat();
			NftPurseChequeId(blake2_256(&preimage))
		},
		NftPurseChequeDestination::Static { cheque_id, .. } => *cheque_id,
	}
}

impl NftPurses {
	/// Hands `items` of `from` off in a cheque for `to`, made by `creator`.
	/// The served handler has already answered `NoItems`, `PurseNotFound` and
	/// `Denied`. A second cheque with the same id from the same creator is
	/// refused as `TagInUse`.
	pub async fn create_cheque(
		&self,
		creator: &ProductId,
		from: PurseId,
		to: &NftPurseChequeDestination,
		items: BTreeSet<ItemKey>,
	) -> Result<NftPurseCheque, HandoffError<SealError>> {
		let id = cheque_id_for(to);
		let tag = handoff_tag(creator, &id);
		let request = HandoffRequest { from, keys: items.clone(), tag };
		let seal = |secrets: &[ItemSecret]| seal_cheque(to, id, &items, secrets);
		self.ledger.hand_off(request, seal).await
	}
}

/// The tag that names the handoff of the cheque `id` by `creator`, so cheque
/// ids are unique per creating product.
fn handoff_tag(creator: &ProductId, id: &NftPurseChequeId) -> HandoffTag {
	HandoffTag((creator, id.0).encode())
}

/// The cheque for `to` carrying `items`, whose `secrets` are in the same order.
fn seal_cheque(
	to: &NftPurseChequeDestination,
	id: NftPurseChequeId,
	items: &BTreeSet<ItemKey>,
	secrets: &[ItemSecret],
) -> Result<NftPurseCheque, SealError> {
	let secret_bytes: Zeroizing<Vec<[u8; 64]>> =
		Zeroizing::new(secrets.iter().map(|secret| *secret.as_bytes()).collect());
	let plaintext = Zeroizing::new(secret_bytes.encode());
	let sealed = seal_to(recipient_key(to), &plaintext)?;
	Ok(NftPurseCheque {
		version: CHEQUE_VERSION,
		id,
		recipient: cheque_recipient(to),
		items: items.iter().map(|key| NftPurseItemKey(key.0)).collect(),
		encrypted_secrets: encrypted_secrets(sealed),
	})
}

/// The X25519 key a cheque for `to` is sealed to.
fn recipient_key(to: &NftPurseChequeDestination) -> [u8; 32] {
	match to {
		NftPurseChequeDestination::Ephemeral(key) => key.0,
		NftPurseChequeDestination::Static { encryption_key, .. } => encryption_key.0,
	}
}

/// What the recipient of a cheque for `to` needs to find its decryption key.
pub fn cheque_recipient(to: &NftPurseChequeDestination) -> NftPurseChequeRecipient {
	match to {
		NftPurseChequeDestination::Ephemeral(key) => NftPurseChequeRecipient::Ephemeral(*key),
		NftPurseChequeDestination::Static { .. } => NftPurseChequeRecipient::Static,
	}
}

/// `ephemeral public key ‖ nonce ‖ ciphertext`.
fn encrypted_secrets(sealed: SealedBox) -> Vec<u8> {
	[sealed.ephemeral_public.as_slice(), &sealed.encrypted].concat()
}

/// The item key each secret in `cheque` rebuilds, opened with the
/// recipient's X25519 `secret`, as a receiver's deposit does.
#[cfg(test)]
pub fn keys_in_cheque(secret: [u8; 32], cheque: &NftPurseCheque) -> Vec<ItemKey> {
	use parity_scale_codec::Decode;

	let (ephemeral, encrypted) = cheque.encrypted_secrets.split_at(32);
	let ephemeral_public = ephemeral.try_into().expect("an ephemeral key has 32 bytes");
	let sealed = SealedBox { ephemeral_public, encrypted: encrypted.to_vec() };
	let plaintext = crate::host_logic::x25519_seal::open_sealed(secret, &sealed)
		.expect("the recipient opens the cheque");
	let secrets = Vec::<[u8; 64]>::decode(&mut plaintext.as_slice()).expect("secrets decode");
	let key_of = |secret: &[u8; 64]| {
		let public = schnorrkel::SecretKey::from_bytes(secret).expect("a secret key").to_public();
		ItemKey(public.to_bytes())
	};
	secrets.iter().map(key_of).collect()
}

#[cfg(test)]
mod tests;
