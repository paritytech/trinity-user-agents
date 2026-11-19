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
