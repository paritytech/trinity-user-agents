//! The host's backup statement account: the same for one wallet in every
//! session, and a signer of backup pointers only.

use super::*;
use crate::host_logic::{
	product_account::derive_sr25519_hard_path,
	statement_store::{StatementField, StatementProof, unsigned_statement_signing_payload},
};
use parity_scale_codec::Encode;

const POINTER_MAGIC: &[u8] = b"twhb/ptr/1";

fn activated(entropy: &[u8]) -> Arc<SigningHostRole> {
	let (_, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(entropy.to_vec()))
		.expect("the local session activates");
	authority
}

/// The unsigned signing payload of a statement with `fields`, as the host
/// builds a pointer's.
fn payload(fields: Vec<StatementField>) -> Vec<u8> {
	unsigned_statement_signing_payload(fields).expect("unsigned fields encode")
}

fn now_secs() -> u64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.expect("the clock is past the epoch")
		.as_secs()
}

/// A pointer's data: the magic, then what a sealed body would take.
fn sealed_pointer() -> Vec<u8> {
	[POINTER_MAGIC, &[0x5e; 24 + 16 + 200]].concat()
}

fn fields_lapsing(lapses: u64, data: &[u8]) -> Vec<StatementField> {
	vec![
		StatementField::Expiry((lapses << 32) | 7),
		StatementField::Channel([0x11; 32]),
		StatementField::Topic1([0x22; 32]),
		StatementField::Data(data.to_vec()),
	]
}

fn pointer_fields(data: &[u8]) -> Vec<StatementField> {
	fields_lapsing(now_secs() + 14 * 86_400, data)
}

#[test]
fn the_backup_account_is_the_same_for_one_wallet_in_every_session_and_another_for_another() {
	let first = activated(&ENTROPY).backup_account().expect("the account derives");
	let again = activated(&ENTROPY).backup_account().expect("the account derives");
	let other = activated(&[0xCD; 16]).backup_account().expect("the account derives");

	// A fresh browser finds the pointer only because the same wallet derives
	// the same signer there.
	assert_eq!(first, again);
	assert_ne!(first, other);
	assert_eq!(
		first,
		derive_sr25519_hard_path(&ENTROPY, &["host-backup", "statement"])
			.expect("the path derives")
			.public
			.to_bytes(),
		"the account sits at the reserved junction, outside every product subtree",
	);
}

#[test]
fn the_backup_account_signs_a_pointer_with_a_signature_its_key_verifies() {
	let authority = activated(&ENTROPY);
	let account = authority.backup_account().expect("the account derives");
	let payload = payload(pointer_fields(&sealed_pointer()));

	let signature = authority.sign_backup_pointer(&payload).expect("a pointer is signed");

	let public = schnorrkel::PublicKey::from_bytes(&account).expect("32-byte key");
	let signature = schnorrkel::Signature::from_bytes(&signature).expect("64-byte signature");
	public
		.verify_simple(SR25519_SIGNING_CONTEXT, &payload, &signature)
		.expect("the signature verifies against the backup account");
}

#[test]
fn the_backup_account_signs_nothing_but_a_short_lived_pointer() {
	let authority = activated(&ENTROPY);
	let pointer = sealed_pointer();
	let mut extra_topic = pointer_fields(&pointer);
	extra_topic.push(StatementField::Topic2([0x33; 32]));
	let signed = [
		StatementField::Proof(StatementProof::Sr25519 { signature: [0; 64], signer: [0; 32] })
			.encode(),
		payload(pointer_fields(&pointer)),
	]
	.concat();
	let mut trailing = payload(pointer_fields(&pointer));
	trailing.push(0xff);

	for (what, refused) in [
		(
			"a statement without a channel",
			payload(vec![
				StatementField::Expiry((now_secs() + 86_400) << 32),
				StatementField::Topic1([0x22; 32]),
				StatementField::Data(pointer.clone()),
			]),
		),
		("a second topic", payload(extra_topic)),
		(
			"data that is not a pointer",
			payload(pointer_fields(&[b"twhb/ptr/0", &pointer[10..]].concat())),
		),
		("a pointer too short to be sealed", payload(pointer_fields(&pointer[..40]))),
		(
			"data larger than a pointer",
			payload(pointer_fields(&[pointer.as_slice(), &[0; 1024]].concat())),
		),
		("an expiry already past", payload(fields_lapsing(now_secs() - 1, &pointer))),
		(
			"an expiry over fifteen days ahead",
			payload(fields_lapsing(now_secs() + 16 * 86_400, &pointer)),
		),
		("a statement that is already signed", signed),
		("bytes after the statement", trailing),
		("an empty payload", Vec::new()),
		("an arbitrary message", b"transfer everything".to_vec()),
	] {
		assert!(authority.sign_backup_pointer(&refused).is_err(), "{what} must be refused");
	}
}

#[test]
fn no_backup_call_answers_without_a_session() {
	let (_, authority) = signing_runtime();

	assert!(authority.backup_account().is_err());
	assert!(
		authority
			.sign_backup_pointer(&payload(pointer_fields(&sealed_pointer())))
			.is_err()
	);
}
