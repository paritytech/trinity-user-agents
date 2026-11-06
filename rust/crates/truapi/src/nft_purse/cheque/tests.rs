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
	let prepared = block_on(purses.prepare_cheque(&creator(product), FROM, to, items))?;
	block_on(prepared.create())
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
fn resealed(cheque: NftPurseCheque, recipient: &X25519Keypair, plaintext: &[u8]) -> NftPurseCheque {
	let sealed = SealedBox::seal(recipient.public_key(), plaintext).unwrap();
	NftPurseCheque { encrypted_secrets: sealed.to_bytes(), ..cheque }
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
	let to =
		NftPurseChequeDestination::Static { encryption_key: X25519PublicKey(public), cheque_id };

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
	let to =
		NftPurseChequeDestination::Static { encryption_key: X25519PublicKey(public), cheque_id };
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

	assert!(matches!(without_ephemeral_key, Err(CorruptedCheque::Seal(SealError::TooShort(31)))));
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

	assert!(matches!(opened, Err(CorruptedCheque::InvalidSecret { position: 0 })));
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
