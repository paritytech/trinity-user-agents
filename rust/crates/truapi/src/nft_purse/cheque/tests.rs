use core::num::NonZeroU32;

use futures::executor::block_on;
use parity_scale_codec::Decode;
use tempfile::TempDir;
use truapi::latest::{NftPurseReceiveKey, X25519PublicKey};

use super::*;
use crate::{
	host_logic::x25519_seal::{AEAD_NONCE_LEN, SealedBox, generate_x25519_keypair, open_sealed},
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

fn cheque_of(
	purses: &NftPurses,
	product: &str,
	to: &NftPurseChequeDestination,
	keys: &[ItemKey],
) -> Result<NftPurseCheque, HandOffError<SealError>> {
	let items = keys.iter().copied().collect();
	block_on(purses.create_cheque(&creator(product), FROM, to, items))
}

fn available_keys(purses: &NftPurses) -> Vec<ItemKey> {
	let page = PageRequest { index: 0, size: NonZeroU32::new(100).unwrap() };
	block_on(purses.ledger.available_items(FROM, page)).unwrap().items
}

/// What the receiver does in `deposit`: splits off the ephemeral key, opens
/// the rest with its secret and decodes one 64-byte secret per item.
fn open_cheque(secret: [u8; 32], cheque: &NftPurseCheque) -> Vec<[u8; 64]> {
	let (ephemeral, encrypted) = cheque.encrypted_secrets.split_at(32);
	let sealed = SealedBox {
		ephemeral_public: ephemeral.try_into().unwrap(),
		encrypted: encrypted.to_vec(),
	};
	let plaintext = open_sealed(secret, &sealed).unwrap();
	Vec::<[u8; 64]>::decode(&mut plaintext.as_slice()).unwrap()
}

/// The key each secret rebuilds.
fn keys_of(secrets: &[[u8; 64]]) -> Vec<ItemKey> {
	let key_of = |secret: &[u8; 64]| {
		let public = schnorrkel::SecretKey::from_bytes(secret).unwrap().to_public();
		ItemKey(public.to_bytes())
	};
	secrets.iter().map(key_of).collect()
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

/// The receiver deposits by rebuilding every item key from its secret, so the
/// holder of the receive key must open the cheque and get one secret per
/// listed item, in the listed order.
#[test]
fn the_receive_key_holder_opens_the_cheque_and_rebuilds_every_listed_key() {
	let (_dir, purses, keys) = purses_holding(3);
	let (secret, public) = generate_x25519_keypair().unwrap();
	let to = NftPurseChequeDestination::Ephemeral(NftPurseReceiveKey(public));

	let cheque = cheque_of(&purses, "tickets.dot", &to, &keys).unwrap();

	let listed: Vec<ItemKey> = cheque.items.iter().map(|key| ItemKey(key.0)).collect();
	let mut sorted = keys.clone();
	sorted.sort();
	assert_eq!(listed, sorted);
	assert_eq!(keys_of(&open_cheque(secret, &cheque)), listed);
	assert_eq!(cheque.id, cheque_id_for(&to));
	assert_eq!(cheque.recipient, NftPurseChequeRecipient::Ephemeral(NftPurseReceiveKey(public)));
	assert_eq!(cheque.version, 0);
	assert_eq!(available_keys(&purses), vec![]);
}

/// The format other hosts parse: the ephemeral key, the nonce, then the
/// ciphertext of the SCALE-encoded secrets and its 16-byte tag.
#[test]
fn encrypted_secrets_are_the_ephemeral_key_the_nonce_and_the_ciphertext() {
	let (_dir, purses, keys) = purses_holding(2);
	let (_, public) = generate_x25519_keypair().unwrap();
	let to = NftPurseChequeDestination::Ephemeral(NftPurseReceiveKey(public));

	let cheque = cheque_of(&purses, "tickets.dot", &to, &keys).unwrap();

	let plaintext_len = 1 + 2 * 64;
	assert_eq!(cheque.encrypted_secrets.len(), 32 + AEAD_NONCE_LEN + plaintext_len + 16);
}

/// A static receiver names the cheque itself and decrypts with its
/// long-lived key, which the cheque does not repeat.
#[test]
fn a_static_cheque_keeps_its_given_id_and_opens_with_the_static_key() {
	let (_dir, purses, keys) = purses_holding(1);
	let (secret, public) = generate_x25519_keypair().unwrap();
	let cheque_id = NftPurseChequeId([9; 32]);
	let to =
		NftPurseChequeDestination::Static { encryption_key: X25519PublicKey(public), cheque_id };

	let cheque = cheque_of(&purses, "tickets.dot", &to, &keys).unwrap();

	assert_eq!(cheque.id, cheque_id);
	assert_eq!(cheque.recipient, NftPurseChequeRecipient::Static);
	assert_eq!(keys_of(&open_cheque(secret, &cheque)), keys);
}

/// A cheque id names one cheque of one product: a second cheque under it is
/// refused, while another product may use the same static id.
#[test]
fn a_cheque_id_is_unique_per_creating_product() {
	let (_dir, purses, keys) = purses_holding(3);
	let (_, public) = generate_x25519_keypair().unwrap();
	let cheque_id = NftPurseChequeId([9; 32]);
	let to =
		NftPurseChequeDestination::Static { encryption_key: X25519PublicKey(public), cheque_id };
	cheque_of(&purses, "tickets.dot", &to, &keys[..1]).unwrap();

	let again = cheque_of(&purses, "tickets.dot", &to, &keys[1..2]);
	let other_product = cheque_of(&purses, "other.dot", &to, &keys[2..]);

	assert!(matches!(again, Err(HandOffError::TagInUse)));
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

	let refused = cheque_of(&purses, "tickets.dot", &to, &keys);

	assert!(matches!(refused, Err(HandOffError::Seal(SealError::LowOrderKey))));
	assert_eq!(available_keys(&purses), keys);
}

/// An item already handed off may be held by someone else, so it is never put
/// in a second cheque.
#[test]
fn an_item_in_a_cheque_cannot_go_into_another() {
	let (_dir, purses, keys) = purses_holding(1);
	let (_, public) = generate_x25519_keypair().unwrap();
	let first = NftPurseChequeDestination::Ephemeral(NftPurseReceiveKey(public));
	let second = NftPurseChequeDestination::Ephemeral(NftPurseReceiveKey([5; 32]));
	cheque_of(&purses, "tickets.dot", &first, &keys).unwrap();

	let refused = cheque_of(&purses, "tickets.dot", &second, &keys);

	assert!(
		matches!(refused, Err(HandOffError::Item(ItemRefusal::Unavailable(key))) if key == keys[0])
	);
}
