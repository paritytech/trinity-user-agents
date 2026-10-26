//! Receive keys and cheques through the product-facing trait.

use futures::executor::block_on;
use truapi::versioned::nft_purse::{
	HostNftPurseCreateReceiveKeyError, HostNftPurseCreateReceiveKeyRequest,
	HostNftPurseCreateReceiveKeyResponse,
};

use super::{
	operations::{Purses, product_purses, product_purses_over},
	*,
};
use crate::{host_logic::x25519_seal::x25519_public_key, platform::async_trait, store::DbError};

/// Encrypted storage that refuses every write, as a host without the callback does.
struct RejectingStorage;

#[async_trait]
impl EncryptedStorage for RejectingStorage {
	async fn read_encrypted(
		&self,
		_key: &[u8],
	) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>, v01::GenericError> {
		Ok(None)
	}

	async fn write_encrypted(&self, _key: &[u8], _value: &[u8]) -> Result<(), v01::GenericError> {
		Err(v01::GenericError { reason: "this host keeps no encrypted storage".to_owned() })
	}
}

fn create_receive_key(
	host: &ProductRuntimeHost,
	into: u32,
) -> Result<v01::NftPurseReceiveKey, CallError<HostNftPurseCreateReceiveKeyError>> {
	let request = v01::HostNftPurseCreateReceiveKeyRequest { into: v01::NftPurseId(into) };
	let request = HostNftPurseCreateReceiveKeyRequest::V1(request);
	let created = block_on(host.create_receive_key(&CallContext::default(), request))?;
	let HostNftPurseCreateReceiveKeyResponse::V1(response) = created;
	Ok(response.key)
}

fn receive_key_refusal(
	error: v01::NftPurseCreateReceiveKeyError,
) -> CallError<HostNftPurseCreateReceiveKeyError> {
	CallError::Domain(HostNftPurseCreateReceiveKeyError::V1(error))
}

/// Each recorded receive key as (key, purse, creator).
fn recorded_receive_keys(purses: &Purses) -> Vec<([u8; 32], u32, String)> {
	let db = purses.services.core_db().unwrap();
	let rows = |conn: &rusqlite::Connection| {
		let mut statement = conn.prepare("SELECT key, purse, creator FROM nft_receive_key")?;
		let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
		Ok(rows.collect::<Result<Vec<_>, _>>()?)
	};
	block_on(db.read(rows)).map_err(|error: DbError| error.to_string()).unwrap()
}

/// The receiver opens a cheque with the secret behind its receive key, after
/// a restart too, so the secret must be in encrypted storage under the key it
/// is looked up by, and the key recorded with its purse and creator.
#[test]
fn a_receive_key_keeps_its_secret_in_encrypted_storage_and_records_its_purse() {
	let encrypted = Arc::new(MemoryEncryptedStorage::default());
	let purses = product_purses_over(Arc::new(StubPlatform::default()), encrypted.clone());

	let key = create_receive_key(&purses.host, TICKETS).unwrap();

	let storage_key = [b"nft/receive-key/".as_slice(), &key.0].concat();
	let secret: [u8; 32] = encrypted.stored()[&storage_key].clone().try_into().unwrap();
	assert_eq!(x25519_public_key(secret), key.0);
	assert_eq!(recorded_receive_keys(&purses), vec![(key.0, TICKETS, PRODUCT.to_owned())]);
}

/// Receiving only adds items, so a product may receive into Root's purses
/// without the user's grant, and nobody is asked.
#[test]
fn a_receive_key_into_roots_purse_needs_no_grant() {
	let platform = Arc::new(StubPlatform { remote_permission_denied: true, ..Default::default() });
	let purses = product_purses(platform.clone());
	let root = purses.root_purse();

	let created = create_receive_key(&purses.host, root.0);

	assert!(created.is_ok());
	assert_eq!(requested_permissions(&platform), vec![]);
}

#[test]
fn a_receive_key_into_an_unknown_purse_is_refused() {
	let encrypted = Arc::new(MemoryEncryptedStorage::default());
	let purses = product_purses_over(Arc::new(StubPlatform::default()), encrypted.clone());

	let refused = create_receive_key(&purses.host, 7);

	let expected = receive_key_refusal(v01::NftPurseCreateReceiveKeyError::PurseNotFound);
	assert_eq!(refused, Err(expected));
	assert!(encrypted.stored().is_empty());
}

/// A recorded key without its secret could never open a cheque, so a host
/// that does not keep the secret gets no key.
#[test]
fn a_receive_key_whose_secret_was_not_stored_is_not_recorded() {
	let purses = product_purses_over(Arc::new(StubPlatform::default()), Arc::new(RejectingStorage));

	let refused = create_receive_key(&purses.host, TICKETS);

	let reason = "the receive key secret was not stored: this host keeps no encrypted storage";
	let other = v01::GenericError { reason: reason.to_owned() };
	let expected = receive_key_refusal(v01::NftPurseCreateReceiveKeyError::Other(other));
	assert_eq!(refused, Err(expected));
	assert_eq!(recorded_receive_keys(&purses), vec![]);
}
