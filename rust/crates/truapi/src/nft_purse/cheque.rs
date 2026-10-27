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

use super::NftPurses;
use crate::{
	host_logic::x25519_seal::{SealError, SealedBox, X25519PrivateKey},
	nft::{
		HandoffError, HandoffRequest, HandoffTag, ImportedItemKey, ItemKey, ItemSecret, PurseId,
	},
	platform::ProductId,
};

/// Version of the cheque format this host writes.
const CHEQUE_VERSION: u8 = 0;

/// Domain of the hash that derives an ephemeral cheque's id from its receive key.
const CHEQUE_ID_DOMAIN: &[u8] = b"nft-purse/cheque-id";

/// Separates the creating product from the cheque id in a handoff tag.
const HANDOFF_TAG_SEPARATOR: &[u8] = b".";

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
		encrypted_secrets: encrypted_secrets(sealed),
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

/// `ephemeral public key ‖ nonce ‖ ciphertext`.
fn encrypted_secrets(sealed: SealedBox) -> Vec<u8> {
	[sealed.ephemeral_public.0.as_slice(), &sealed.encrypted].concat()
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
	#[error("the secret of item {} is not a secret key", hex::encode(.0.0))]
	InvalidSecret(NftPurseItemKey),
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
	if secrets.len() != cheque.items.len() {
		return Err(CorruptedCheque::SecretCount {
			secrets: secrets.len(),
			items: cheque.items.len(),
		});
	}
	cheque.items.iter().zip(secrets.iter()).map(imported_key_listed_as).collect()
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
	let Some((ephemeral, encrypted)) = encrypted_secrets.split_first_chunk::<32>() else {
		return Err(SealError::TooShort(encrypted_secrets.len()).into());
	};
	let ephemeral_public = X25519PublicKey(*ephemeral);
	let sealed = SealedBox { ephemeral_public, encrypted: encrypted.to_vec() };
	let plaintext = Zeroizing::new(sealed.open(secret)?);
	Ok(Zeroizing::new(Vec::<[u8; 64]>::decode(&mut plaintext.as_slice())?))
}

/// The key `secret` signs for, with that secret, when it is the listed `item`.
fn imported_key_listed_as(
	(item, secret): (&NftPurseItemKey, &[u8; 64]),
) -> Result<ImportedItemKey, CorruptedCheque> {
	let secret = ItemSecret::from_bytes(Zeroizing::new(*secret));
	let imported =
		ImportedItemKey::from_secret(secret).map_err(|_| CorruptedCheque::InvalidSecret(*item))?;
	if imported.key().0 != item.0 {
		return Err(CorruptedCheque::KeyMismatch { listed: *item, signs_for: imported.key() });
	}
	Ok(imported)
}

#[cfg(test)]
mod tests {
	use core::num::NonZeroU32;

	use futures::executor::block_on;
	use tempfile::TempDir;
	use truapi::latest::{NftPurseReceiveKey, X25519PublicKey};

	use super::*;
	use crate::{
		host_logic::x25519_seal::X25519Keypair,
		nft::{HeldItem, ItemRefusal, PageRequest},
		nft_purse::test_purses,
		store::open_temporary_core_db,
	};

	const FROM: PurseId = PurseId(0);

	/// Purse calls over a fresh database whose purse 0 holds `count` items.
	fn purses_holding(count: u32) -> (TempDir, NftPurses, Vec<ItemKey>) {
		let (dir, db) = open_temporary_core_db();
		let purses = test_purses(db);
		block_on(purses.create_purse(&creator("tickets.dot"), "Tickets".to_owned())).unwrap();
		let count = NonZeroU32::new(count).unwrap();
		let keys = block_on(purses.ledger.allocate_items(FROM, count)).unwrap();
		let holding = |(index, key): (usize, &ItemKey)| {
			(*key, Some(HeldItem { collection: 1, item: 1, instance: 100 + index as u64 }))
		};
		block_on(purses.ledger.record_holdings(keys.iter().enumerate().map(holding).collect()))
			.unwrap();
		(dir, purses, keys)
	}

	fn creator(id: &str) -> ProductId {
		ProductId::for_test(id)
	}

	fn create_test_cheque(
		purses: &NftPurses,
		product: &str,
		to: &NftPurseChequeDestination,
		keys: &[ItemKey],
	) -> Result<NftPurseCheque, HandoffError<SealError>> {
		let items = keys.iter().copied().collect();
		block_on(purses.create_cheque(&creator(product), FROM, to, items))
	}

	/// The keys `cheque` opens to with `secret`.
	fn opened_keys(secret: &X25519PrivateKey, cheque: &NftPurseCheque) -> Vec<ItemKey> {
		let opened = open_cheque(secret, cheque).expect("the recipient opens the cheque");
		opened.iter().map(ImportedItemKey::key).collect()
	}

	/// A cheque of `count` items sealed to a fresh ephemeral key, with that key.
	fn sealed_test_cheque(count: u32) -> (TempDir, NftPurseCheque, X25519Keypair) {
		let (dir, purses, keys) = purses_holding(count);
		let recipient = X25519Keypair::generate().unwrap();
		let receive_key = NftPurseReceiveKey(recipient.public_key().0);
		let to = NftPurseChequeDestination::Ephemeral(receive_key);
		let cheque = create_test_cheque(&purses, "tickets.dot", &to, &keys).unwrap();
		(dir, cheque, recipient)
	}

	/// `cheque` with `plaintext` sealed to `recipient` in place of its secrets.
	fn resealed(
		cheque: NftPurseCheque,
		recipient: &X25519Keypair,
		plaintext: &[u8],
	) -> NftPurseCheque {
		let sealed = SealedBox::seal(recipient.public_key(), plaintext).unwrap();
		NftPurseCheque { encrypted_secrets: encrypted_secrets(sealed), ..cheque }
	}

	fn available_keys(purses: &NftPurses) -> Vec<ItemKey> {
		let page = PageRequest { index: 0, size: NonZeroU32::new(100).unwrap() };
		block_on(purses.ledger.available_items(FROM, page)).unwrap().items
	}

	/// Other hosts derive the same id from the receive key to find the cheque,
	/// so the derivation must never change.
	#[test]
	fn an_ephemeral_cheque_id_is_the_domain_separated_hash_of_its_receive_key() {
		let to = NftPurseChequeDestination::Ephemeral(NftPurseReceiveKey([7; 32]));

		let id = cheque_id_for(&to);

		assert_eq!(
			hex::encode(id.0),
			"df0f719003b10f758a31933626292bad8b1318e1b59dc84eb10102d978f27162"
		);
	}

	/// Tags are stored, so a cheque id stays used across updates only while
	/// the tag of a creator and an id never changes.
	#[test]
	fn a_handoff_tag_is_the_creator_a_dot_and_the_cheque_id() {
		let tag = handoff_tag(&creator("tickets.dot"), &NftPurseChequeId([9; 32]));

		assert_eq!(tag, HandoffTag([b"tickets.dot.".as_slice(), &[9; 32]].concat()));
	}

	/// The receiver deposits by rebuilding every item key from its secret, so the
	/// holder of the receive key must open the cheque and get one secret per
	/// listed item, in the listed order.
	#[test]
	fn the_receive_key_holder_opens_the_cheque_and_rebuilds_every_listed_key() {
		let (_dir, purses, keys) = purses_holding(3);
		let recipient = X25519Keypair::generate().unwrap();
		let public = recipient.public_key().0;
		let to = NftPurseChequeDestination::Ephemeral(NftPurseReceiveKey(public));

		let cheque = create_test_cheque(&purses, "tickets.dot", &to, &keys).unwrap();

		let mut sorted = keys.clone();
		sorted.sort();
		let expected = NftPurseCheque {
			version: 0,
			id: cheque_id_for(&to),
			recipient: NftPurseChequeRecipient::Ephemeral(NftPurseReceiveKey(public)),
			items: sorted.iter().map(|key| NftPurseItemKey(key.0)).collect(),
			encrypted_secrets: cheque.encrypted_secrets.clone(),
		};
		assert_eq!(cheque, expected);
		assert_eq!(opened_keys(recipient.private_key(), &cheque), sorted);
		assert_eq!(available_keys(&purses), vec![]);
	}

	/// A static receiver names the cheque itself and decrypts with its
	/// long-lived key, which the cheque does not repeat.
	#[test]
	fn a_static_cheque_keeps_its_given_id_and_opens_with_the_static_key() {
		let (_dir, purses, keys) = purses_holding(1);
		let recipient = X25519Keypair::generate().unwrap();
		let public = recipient.public_key().0;
		let cheque_id = NftPurseChequeId([9; 32]);
		let to = NftPurseChequeDestination::Static {
			encryption_key: X25519PublicKey(public),
			cheque_id,
		};

		let cheque = create_test_cheque(&purses, "tickets.dot", &to, &keys).unwrap();

		let expected = NftPurseCheque {
			version: 0,
			id: cheque_id,
			recipient: NftPurseChequeRecipient::Static,
			items: vec![NftPurseItemKey(keys[0].0)],
			encrypted_secrets: cheque.encrypted_secrets.clone(),
		};
		assert_eq!(cheque, expected);
		assert_eq!(opened_keys(recipient.private_key(), &cheque), keys);
	}

	/// A cheque id names one cheque of one product: a second cheque under it is
	/// refused, while another product may use the same static id.
	#[test]
	fn a_cheque_id_is_unique_per_creating_product() {
		let (_dir, purses, keys) = purses_holding(3);
		let public = X25519Keypair::generate().unwrap().public_key().0;
		let cheque_id = NftPurseChequeId([9; 32]);
		let to = NftPurseChequeDestination::Static {
			encryption_key: X25519PublicKey(public),
			cheque_id,
		};
		create_test_cheque(&purses, "tickets.dot", &to, &keys[..1]).unwrap();

		let again = create_test_cheque(&purses, "tickets.dot", &to, &keys[1..2]);
		let other_product = create_test_cheque(&purses, "other.dot", &to, &keys[2..]);

		assert!(matches!(again, Err(HandoffError::TagInUse(_))));
		assert!(other_product.is_ok());
	}

	/// A cheque to a low-order key could be opened by anyone, so none is made and
	/// the items stay spendable.
	#[test]
	fn a_cheque_to_a_low_order_key_is_refused_and_spends_nothing() {
		let (_dir, purses, keys) = purses_holding(1);
		let to = NftPurseChequeDestination::Static {
			encryption_key: X25519PublicKey([0; 32]),
			cheque_id: NftPurseChequeId([9; 32]),
		};

		let refused = create_test_cheque(&purses, "tickets.dot", &to, &keys);

		assert!(matches!(refused, Err(HandoffError::Seal(SealError::LowOrderKey(_)))));
		assert_eq!(available_keys(&purses), keys);
	}

	/// An item already handed off may be held by someone else, so it is never put
	/// in a second cheque.
	#[test]
	fn an_item_in_a_cheque_cannot_go_into_another() {
		let (_dir, purses, keys) = purses_holding(1);
		let public = X25519Keypair::generate().unwrap().public_key().0;
		let first = NftPurseChequeDestination::Ephemeral(NftPurseReceiveKey(public));
		let second = NftPurseChequeDestination::Ephemeral(NftPurseReceiveKey([5; 32]));
		create_test_cheque(&purses, "tickets.dot", &first, &keys).unwrap();

		let refused = create_test_cheque(&purses, "tickets.dot", &second, &keys);

		assert!(
			matches!(refused, Err(HandoffError::Item(ItemRefusal::Unavailable(key))) if key == keys[0])
		);
	}

	/// Only this format is understood, so a cheque in another is not guessed at.
	#[test]
	fn a_cheque_of_another_version_is_corrupted() {
		let (_dir, cheque, recipient) = sealed_test_cheque(1);
		let other_version = NftPurseCheque { version: 1, ..cheque };

		let opened = open_cheque(recipient.private_key(), &other_version);

		assert!(matches!(opened, Err(CorruptedCheque::UnknownVersion(1))));
	}

	/// One key cannot be imported twice into the same deposit.
	#[test]
	fn a_cheque_listing_an_item_twice_is_corrupted() {
		let (_dir, cheque, recipient) = sealed_test_cheque(1);
		let items = vec![cheque.items[0], cheque.items[0]];
		let doubled = NftPurseCheque { items, ..cheque };

		let opened = open_cheque(recipient.private_key(), &doubled);

		assert!(matches!(opened, Err(CorruptedCheque::DuplicateItem(_))));
	}

	/// What a cheque holds must come from its sender, so anything that does
	/// not decrypt with the recipient's key is refused.
	#[test]
	fn a_cheque_that_does_not_decrypt_is_corrupted() {
		let (_dir, cheque, recipient) = sealed_test_cheque(1);
		let truncated = |length: usize| NftPurseCheque {
			encrypted_secrets: cheque.encrypted_secrets[..length].to_vec(),
			..cheque.clone()
		};
		let mut low_order = cheque.clone();
		low_order.encrypted_secrets[..32].fill(0);
		let other_recipient = X25519Keypair::generate().unwrap();

		let without_ephemeral_key = open_cheque(recipient.private_key(), &truncated(31));
		let without_tag = open_cheque(recipient.private_key(), &truncated(32 + 12 + 15));
		let to_low_order = open_cheque(recipient.private_key(), &low_order);
		let to_another_key = open_cheque(other_recipient.private_key(), &cheque);

		assert!(matches!(
			without_ephemeral_key,
			Err(CorruptedCheque::Seal(SealError::TooShort(31)))
		));
		assert!(matches!(without_tag, Err(CorruptedCheque::Seal(SealError::TooShort(_)))));
		assert!(matches!(to_low_order, Err(CorruptedCheque::Seal(SealError::LowOrderKey(_)))));
		assert!(matches!(to_another_key, Err(CorruptedCheque::Seal(SealError::Decrypt))));
	}

	/// Only a list of 64-byte secrets can be moved from, so anything else that
	/// decrypts is refused.
	#[test]
	fn a_cheque_whose_secrets_do_not_decode_is_corrupted() {
		let (_dir, cheque, recipient) = sealed_test_cheque(1);
		let garbled = resealed(cheque, &recipient, &[0x05, 0x01]);

		let opened = open_cheque(recipient.private_key(), &garbled);

		assert!(matches!(opened, Err(CorruptedCheque::Encoding(_))));
	}

	/// Every listed item must come with its secret, or the deposit would
	/// promise items it cannot move.
	#[test]
	fn a_cheque_listing_more_items_than_it_holds_secrets_is_corrupted() {
		let (_dir, cheque, recipient) = sealed_test_cheque(1);
		let items = vec![cheque.items[0], NftPurseItemKey([0x0c; 32])];
		let extended = NftPurseCheque { items, ..cheque };

		let opened = open_cheque(recipient.private_key(), &extended);

		assert!(matches!(opened, Err(CorruptedCheque::SecretCount { secrets: 1, items: 2 })));
	}

	/// Bytes that are not a secret key could never sign a move, so the deposit
	/// is refused before anything is registered.
	#[test]
	fn a_cheque_whose_secret_is_not_a_secret_key_is_corrupted() {
		let (_dir, cheque, recipient) = sealed_test_cheque(1);
		let not_a_key = resealed(cheque, &recipient, &vec![[0xff_u8; 64]].encode());

		let opened = open_cheque(recipient.private_key(), &not_a_key);

		assert!(matches!(opened, Err(CorruptedCheque::InvalidSecret(_))));
	}

	/// The items are in the clear and not bound to the ciphertext, so a
	/// cheque whose listed items were reordered or replaced must not open.
	#[test]
	fn a_cheque_whose_secrets_sign_for_other_items_is_corrupted() {
		let (_dir, cheque, recipient) = sealed_test_cheque(2);
		let swapped = NftPurseCheque { items: vec![cheque.items[1], cheque.items[0]], ..cheque };

		let opened = open_cheque(recipient.private_key(), &swapped);

		assert!(
			matches!(opened, Err(CorruptedCheque::KeyMismatch { listed, .. }) if listed == swapped.items[0])
		);
	}
}
