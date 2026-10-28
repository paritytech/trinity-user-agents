//! Receive keys and cheques through the product-facing trait.

use futures::{StreamExt, executor::block_on};
use sp_crypto_hashing::blake2_256;
use truapi::versioned::nft_purse::{
	HostNftPurseCreateChequeError, HostNftPurseCreateChequeRequest,
	HostNftPurseCreateChequeResponse, HostNftPurseCreateReceiveKeyError,
	HostNftPurseCreateReceiveKeyRequest, HostNftPurseCreateReceiveKeyResponse,
	HostNftPurseObserveOperationStatusError, HostNftPurseObserveOperationStatusItem,
	HostNftPurseObserveOperationStatusRequest, HostNftPurseRevokeChequeError,
	HostNftPurseRevokeChequeRequest, HostNftPurseRevokeChequeResponse,
};

use super::{
	operations::{Purses, product_purses, product_purses_over},
	*,
};
use crate::{
	host_logic::x25519_seal::X25519PrivateKey,
	platform::{NftPurseDrainDestination, NftPurseDrainReview, async_trait},
	store::DbError,
};

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

pub fn create_receive_key(
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
	assert_eq!(public_key_of(secret), key.0);
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

pub fn create_cheque(
	host: &ProductRuntimeHost,
	from: u32,
	to: v01::NftPurseChequeDestination,
	items: Vec<v01::NftPurseItemKey>,
) -> Result<v01::NftPurseCheque, CallError<HostNftPurseCreateChequeError>> {
	let request = v01::HostNftPurseCreateChequeRequest { from: v01::NftPurseId(from), to, items };
	let request = HostNftPurseCreateChequeRequest::V1(request);
	let created = block_on(host.create_cheque(&CallContext::default(), request))?;
	let HostNftPurseCreateChequeResponse::V1(response) = created;
	Ok(response.cheque)
}

fn cheque_refusal(
	error: v01::NftPurseCreateChequeError,
) -> CallError<HostNftPurseCreateChequeError> {
	CallError::Domain(HostNftPurseCreateChequeError::V1(error))
}

pub fn public_key_of(secret: [u8; 32]) -> [u8; 32] {
	X25519PrivateKey::from(secret).public_key().0
}

fn to_receive_key(key: v01::NftPurseReceiveKey) -> v01::NftPurseChequeDestination {
	v01::NftPurseChequeDestination::Ephemeral(key)
}

fn drain_reviews(platform: &StubPlatform) -> Vec<NftPurseDrainReview> {
	platform.nft_purse_drain_reviews.lock().unwrap().clone()
}

/// A product hands its own items to a receive key without being asked, gets
/// the cheque to deliver, and the items are no longer its to spend.
#[test]
fn a_cheque_from_an_own_purse_needs_no_confirmation_and_spends_its_items() {
	let platform = Arc::new(StubPlatform::default());
	let purses = product_purses(platform.clone());
	let items = purses.held_items(PurseId(TICKETS), 2);
	let receive_key = create_receive_key(&purses.host, 1).unwrap();

	let cheque = create_cheque(&purses.host, TICKETS, to_receive_key(receive_key), items.clone());

	let cheque = cheque.unwrap();
	let mut sorted = items;
	sorted.sort_by_key(|key| key.0);
	let expected = v01::NftPurseCheque {
		version: 0,
		id: cheque.id,
		recipient: v01::NftPurseChequeRecipient::Ephemeral(receive_key),
		items: sorted,
		encrypted_secrets: cheque.encrypted_secrets.clone(),
	};
	assert_eq!(cheque, expected);
	assert_eq!(query(&purses.host, TICKETS, 10), Ok(empty_page()));
	assert_eq!(drain_reviews(&platform), vec![]);
}

/// A product may not give away items of Root's purse unless the user says so,
/// and the user sees that they leave in a cheque.
#[test]
fn a_cheque_from_roots_purse_asks_the_user_and_a_refusal_is_denied() {
	let platform = Arc::new(StubPlatform::default());
	let purses = product_purses(platform.clone());
	let root = purses.root_purse();
	let items = purses.held_items(root, 1);
	let receive_key = v01::NftPurseReceiveKey(public_key_of([3; 32]));

	let refused = create_cheque(&purses.host, root.0, to_receive_key(receive_key), items);

	assert_eq!(refused, Err(cheque_refusal(v01::NftPurseCreateChequeError::Denied)));
	let review = NftPurseDrainReview {
		calling_product_id: PRODUCT.parse().unwrap(),
		from_purse: root.0,
		owner: v01::NftPurseOwner::Root,
		item_count: 1,
		destination: NftPurseDrainDestination::Cheque,
	};
	assert_eq!(drain_reviews(&platform), vec![review]);
}

/// A second cheque under the same id would hand the same transfer out twice,
/// so the product is told it already made one.
#[test]
fn a_second_cheque_for_the_same_receive_key_answers_operation_exists() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let items = purses.held_items(PurseId(TICKETS), 2);
	let receive_key = v01::NftPurseReceiveKey(public_key_of([3; 32]));
	create_cheque(&purses.host, TICKETS, to_receive_key(receive_key), items[..1].to_vec()).unwrap();

	let again =
		create_cheque(&purses.host, TICKETS, to_receive_key(receive_key), items[1..].to_vec());

	assert_eq!(again, Err(cheque_refusal(v01::NftPurseCreateChequeError::OperationExists)));
}

#[test]
fn a_cheque_of_no_items_answers_no_items() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let receive_key = v01::NftPurseReceiveKey(public_key_of([3; 32]));

	let refused = create_cheque(&purses.host, TICKETS, to_receive_key(receive_key), vec![]);

	assert_eq!(refused, Err(cheque_refusal(v01::NftPurseCreateChequeError::NoItems)));
}

#[test]
fn a_cheque_from_an_unknown_purse_answers_purse_not_found() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let receive_key = v01::NftPurseReceiveKey(public_key_of([3; 32]));
	let item = v01::NftPurseItemKey([0x55; 32]);

	let refused = create_cheque(&purses.host, 7, to_receive_key(receive_key), vec![item]);

	assert_eq!(refused, Err(cheque_refusal(v01::NftPurseCreateChequeError::PurseNotFound)));
}

/// An item already in a cheque may be held by its recipient, so it is never
/// handed out again, and an item the purse does not hold is not found.
#[test]
fn a_cheque_over_items_in_use_or_not_held_is_refused() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let items = purses.held_items(PurseId(TICKETS), 1);
	let first = v01::NftPurseReceiveKey(public_key_of([3; 32]));
	let second = v01::NftPurseReceiveKey(public_key_of([4; 32]));
	create_cheque(&purses.host, TICKETS, to_receive_key(first), items.clone()).unwrap();

	let in_use = create_cheque(&purses.host, TICKETS, to_receive_key(second), items);
	let not_held = vec![v01::NftPurseItemKey([0x55; 32])];
	let missing = create_cheque(&purses.host, TICKETS, to_receive_key(second), not_held);

	assert_eq!(in_use, Err(cheque_refusal(v01::NftPurseCreateChequeError::ItemUnavailable)));
	assert_eq!(missing, Err(cheque_refusal(v01::NftPurseCreateChequeError::ItemNotFound)));
}

/// A key anyone could decrypt to is the product's mistake, and nothing is
/// handed off.
#[test]
fn a_cheque_to_a_low_order_static_key_answers_other_and_spends_nothing() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let items = purses.held_items(PurseId(TICKETS), 1);
	let to = v01::NftPurseChequeDestination::Static {
		encryption_key: v01::X25519PublicKey([0; 32]),
		cheque_id: v01::NftPurseChequeId([9; 32]),
	};

	let refused = create_cheque(&purses.host, TICKETS, to, items.clone());

	let reason = format!(
		"sealing the item secrets failed: invalid X25519 public key {}: a low-order point",
		hex::encode([0; 32])
	);
	let other = v01::NftPurseCreateChequeError::Other(v01::GenericError { reason });
	assert_eq!(refused, Err(cheque_refusal(other)));
	let page = query(&purses.host, TICKETS, 10).unwrap();
	let HostNftPurseQueryPurseResponse::V1(response) = page;
	assert_eq!(response.page.items, items);
}

fn revoke(
	host: &ProductRuntimeHost,
	cheque_id: v01::NftPurseChequeId,
) -> Result<HostNftPurseRevokeChequeResponse, CallError<HostNftPurseRevokeChequeError>> {
	let request =
		HostNftPurseRevokeChequeRequest::V1(v01::HostNftPurseRevokeChequeRequest { cheque_id });
	block_on(host.revoke_cheque(&CallContext::default(), request))
}

/// The first status of the operation `host`'s product registered under
/// `idempotency_key`.
pub fn first_status_under(
	host: &ProductRuntimeHost,
	idempotency_key: [u8; 32],
) -> Result<
	HostNftPurseObserveOperationStatusItem,
	CallError<HostNftPurseObserveOperationStatusError>,
> {
	let idempotency_key = v01::NftPurseIdempotencyKey(idempotency_key);
	let request = v01::HostNftPurseObserveOperationStatusRequest { idempotency_key };
	let request = HostNftPurseObserveOperationStatusRequest::V1(request);
	let mut statuses = block_on(host.observe_operation_status(&CallContext::default(), request));
	block_on(statuses.next()).expect("a status or an interruption")
}

/// The sender takes back an undelivered cheque without being asked, since
/// the items return to its own purse, and follows the moves under the key
/// the RFC derives from the cheque id.
#[test]
fn a_revoke_needs_no_confirmation_and_is_observed_under_its_revoke_key() {
	let platform = Arc::new(StubPlatform::default());
	let purses = product_purses(platform.clone());
	let items = purses.held_items(PurseId(TICKETS), 2);
	let receive_key = create_receive_key(&purses.host, 1).unwrap();
	let cheque = create_cheque(&purses.host, TICKETS, to_receive_key(receive_key), items).unwrap();

	let revoked = revoke(&purses.host, cheque.id);

	assert_eq!(revoked, Ok(HostNftPurseRevokeChequeResponse::V1));
	let revoke_key = blake2_256(&[b"nft-purse/revoke".as_slice(), &cheque.id.0].concat());
	let status = first_status_under(&purses.host, revoke_key);
	assert_eq!(
		status,
		Ok(HostNftPurseObserveOperationStatusItem::V1(v01::NftPurseStatus::InProgress))
	);
	assert_eq!(drain_reviews(&platform), vec![]);
}

/// A product learns nothing about another product's cheques, and cannot take
/// their items.
#[test]
fn revoking_another_products_cheque_answers_cheque_not_found() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let items = purses.held_items(PurseId(TICKETS), 1);
	let receive_key = create_receive_key(&purses.host, 1).unwrap();
	let cheque = create_cheque(&purses.host, TICKETS, to_receive_key(receive_key), items).unwrap();

	let refused = revoke(&purses.other_product(), cheque.id);

	let not_found =
		HostNftPurseRevokeChequeError::V1(v01::NftPurseRevokeChequeError::ChequeNotFound);
	assert_eq!(refused, Err(CallError::Domain(not_found)));
}
