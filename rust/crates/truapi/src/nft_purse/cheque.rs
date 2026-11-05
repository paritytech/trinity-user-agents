//! Cheques: item secrets sealed to a recipient's X25519 key, with the items
//! in the clear so the recipient can see what it receives.

use std::collections::BTreeSet;

use parity_scale_codec::{Decode, Encode};
use sp_crypto_hashing::blake2_256;
use truapi::latest::{
	NftPurseCheque, NftPurseChequeDestination, NftPurseChequeId, NftPurseChequeRecipient,
	NftPurseItemKey, X25519PublicKey,
};
use zeroize::Zeroizing;

use super::{
	NftPurses,
	operation::{cheque_idempotency_key, operation_key},
};
use crate::{
	durable::RegistrationError,
	host_logic::x25519_seal::{SealError, SealedBox, X25519PrivateKey},
	nft::{
		HandoffError, HandoffRequest, HandoffTag, ImportedItemKey, ItemKey, ItemSecret, NftLedger,
		OperationKey, PurseId, ReclaimRequest, StartOperationError,
	},
	platform::ProductId,
};

/// Version of the cheque format this host writes.
const CHEQUE_VERSION: u8 = 0;

/// Domain of the hash that derives an ephemeral cheque's id from its receive key.
const CHEQUE_ID_DOMAIN: &[u8] = b"nft-purse/cheque-id";

/// Separates the creating product from the cheque id in a handoff tag.
const HANDOFF_TAG_SEPARATOR: &[u8] = b".";

/// Domain of the hash that derives the idempotency key of a cheque's revoke.
const REVOKE_KEY_DOMAIN: &[u8] = b"nft-purse/revoke";

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

/// A cheque nothing refused when it was prepared: its id was unused by its
/// creator and its items idle. Creating it checks both again.
pub struct PreparedCheque {
	ledger: NftLedger,
	to: NftPurseChequeDestination,
	id: NftPurseChequeId,
	request: HandoffRequest,
}

impl PreparedCheque {
	/// Hands the items off in the cheque. They are spent from then on.
	pub async fn create(self) -> Result<NftPurseCheque, HandoffError<SealError>> {
		let Self { ledger, to, id, request } = self;
		let items = request.keys.clone();
		ledger.hand_off(request, |secrets| seal_cheque(&to, id, &items, secrets)).await
	}
}

impl NftPurses {
	/// Prepares handing `items` of `from` off in a cheque for `to`, made by
	/// `creator`, unless it is refused already. The served handler has
	/// already answered `NoItems` and `PurseNotFound`. A second cheque with
	/// the same id from the same creator is refused as `TagInUse`.
	pub async fn prepare_cheque(
		&self,
		creator: &ProductId,
		from: PurseId,
		to: &NftPurseChequeDestination,
		items: BTreeSet<ItemKey>,
	) -> Result<PreparedCheque, HandoffError<SealError>> {
		let id = cheque_id_for(to);
		let request = HandoffRequest { from, keys: items, tag: handoff_tag(creator, &id) };
		self.ledger.validate_can_handoff(&request).await?;
		Ok(PreparedCheque { ledger: self.ledger.clone(), to: to.clone(), id, request })
	}

	/// Starts moving what sits at the keys of `creator`'s cheque `id` back
	/// into fresh keys of the purse they left. Each call moves only the keys
	/// with no move yet or whose latest move failed, and moves nothing when
	/// every key is moving or moved. Refuses a cheque `creator` did not make
	/// with `TagNotFound`.
	pub async fn revoke_cheque(
		&self,
		creator: &ProductId,
		id: &NftPurseChequeId,
	) -> Result<(), RegistrationError<StartOperationError>> {
		let operation = revoke_operation(creator, id);
		let tag = handoff_tag(creator, id);
		self.ledger.start_reclaims(ReclaimRequest { operation, tag }).await
	}
}

/// The operation `creator`'s revoke of cheque `id` is registered as.
fn revoke_operation(creator: &ProductId, id: &NftPurseChequeId) -> OperationKey {
	operation_key(creator, cheque_idempotency_key(REVOKE_KEY_DOMAIN, id))
}

/// The tag that names the handoff of the cheque `id` by `creator`, so cheque
/// ids are unique per creating product. The id has a fixed length, so a
/// creator that contains the separator cannot make two tags collide.
fn handoff_tag(creator: &ProductId, id: &NftPurseChequeId) -> HandoffTag {
	HandoffTag([creator.as_str().as_bytes(), HANDOFF_TAG_SEPARATOR, &id.0].concat())
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
	let sealed = SealedBox::seal(recipient_key(to), &plaintext)?;
	Ok(NftPurseCheque {
		version: CHEQUE_VERSION,
		id,
		recipient: cheque_recipient(to),
		items: items.iter().map(|key| NftPurseItemKey(key.0)).collect(),
		encrypted_secrets: sealed.to_bytes(),
	})
}

/// The X25519 key a cheque for `to` is sealed to.
fn recipient_key(to: &NftPurseChequeDestination) -> X25519PublicKey {
	match to {
		NftPurseChequeDestination::Ephemeral(key) => X25519PublicKey(key.0),
		NftPurseChequeDestination::Static { encryption_key, .. } => *encryption_key,
	}
}

/// What the recipient of a cheque for `to` needs to find its decryption key.
fn cheque_recipient(to: &NftPurseChequeDestination) -> NftPurseChequeRecipient {
	match to {
		NftPurseChequeDestination::Ephemeral(key) => NftPurseChequeRecipient::Ephemeral(*key),
		NftPurseChequeDestination::Static { .. } => NftPurseChequeRecipient::Static,
	}
}

/// Why a cheque did not open into one valid secret per listed item.
#[derive(Debug, thiserror::Error)]
pub enum CorruptedCheque {
	/// The cheque is in a format this host does not read.
	#[error("cheque version {0}, expected {CHEQUE_VERSION}")]
	UnknownVersion(u8),
	/// An item is listed twice.
	#[error("item {} is listed twice", hex::encode(.0.0))]
	DuplicateItem(NftPurseItemKey),
	/// The secrets do not decrypt with the recipient's key.
	#[error(transparent)]
	Seal(#[from] SealError),
	/// The decrypted secrets are not a list of 64-byte secrets.
	#[error("the cheque secrets do not decode: {0}")]
	Encoding(#[from] parity_scale_codec::Error),
	/// The cheque holds a different number of secrets than it lists items.
	#[error("the cheque holds {secrets} secrets for {items} items")]
	SecretCount {
		/// Secrets in the encrypted part.
		secrets: usize,
		/// Items listed.
		items: usize,
	},
	/// A secret is not an sr25519 secret key.
	#[error("the secret at position {position} is not a secret key")]
	InvalidSecret {
		/// Its position among the cheque's secrets.
		position: usize,
	},
	/// A secret signs for a key other than the item listed at its position.
	#[error("the secret listed for item {} signs for {}", hex::encode(.listed.0), hex::encode(.signs_for.0))]
	KeyMismatch {
		/// The item listed at the secret's position.
		listed: NftPurseItemKey,
		/// The key the secret signs for.
		signs_for: ItemKey,
	},
}

/// Each item of `cheque` with its secret, opened with the recipient's X25519
/// `secret`. Fails unless every listed item comes with the secret that signs
/// for it, in the listed order.
pub fn open_cheque(
	secret: &X25519PrivateKey,
	cheque: &NftPurseCheque,
) -> Result<Vec<ImportedItemKey>, CorruptedCheque> {
	if cheque.version != CHEQUE_VERSION {
		return Err(CorruptedCheque::UnknownVersion(cheque.version));
	}
	reject_duplicate_items(&cheque.items)?;
	let secrets = decrypt_secrets(secret, &cheque.encrypted_secrets)?;
	let keys = secrets
		.iter()
		.enumerate()
		.map(|(position, secret)| imported_key_at(position, secret))
		.collect::<Result<Vec<_>, _>>()?;
	validate_metadata_matches_secrets(&cheque.items, &keys)?;
	Ok(keys)
}

/// Fails on the first item of `items` listed twice.
fn reject_duplicate_items(items: &[NftPurseItemKey]) -> Result<(), CorruptedCheque> {
	let mut seen = BTreeSet::new();
	let duplicate = items.iter().find(|item| !seen.insert(item.0));
	duplicate.map_or(Ok(()), |item| Err(CorruptedCheque::DuplicateItem(*item)))
}

/// The secrets sealed in `encrypted_secrets`, which is `ephemeral public key ‖
/// nonce ‖ ciphertext`.
fn decrypt_secrets(
	secret: &X25519PrivateKey,
	encrypted_secrets: &[u8],
) -> Result<Zeroizing<Vec<[u8; 64]>>, CorruptedCheque> {
	let sealed = SealedBox::from_bytes(encrypted_secrets)?;
	let plaintext = Zeroizing::new(sealed.open(secret)?);
	Ok(Zeroizing::new(Vec::<[u8; 64]>::decode(&mut plaintext.as_slice())?))
}

/// The key the secret at `position` of a cheque signs for, with that secret.
fn imported_key_at(position: usize, secret: &[u8; 64]) -> Result<ImportedItemKey, CorruptedCheque> {
	let secret = ItemSecret::from_bytes(Zeroizing::new(*secret));
	ImportedItemKey::from_secret(secret).map_err(|_| CorruptedCheque::InvalidSecret { position })
}

/// Checks that the cleartext `items` of a cheque are the `keys` its secrets
/// sign for, in the same order. The items are not bound to the ciphertext,
/// so this is what ties them together.
fn validate_metadata_matches_secrets(
	items: &[NftPurseItemKey],
	keys: &[ImportedItemKey],
) -> Result<(), CorruptedCheque> {
	if keys.len() != items.len() {
		return Err(CorruptedCheque::SecretCount { secrets: keys.len(), items: items.len() });
	}
	let mismatch = items.iter().zip(keys).find(|(item, imported)| item.0 != imported.key().0);
	mismatch.map_or(Ok(()), |(item, imported)| {
		Err(CorruptedCheque::KeyMismatch { listed: *item, signs_for: imported.key() })
	})
}

#[cfg(test)]
mod tests;
