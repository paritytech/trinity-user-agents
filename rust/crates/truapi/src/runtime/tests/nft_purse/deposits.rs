//! Depositing received cheques through the product-facing trait.

use futures::{StreamExt, executor::block_on};
use truapi::versioned::nft_purse::{
	HostNftPurseDepositError, HostNftPurseDepositRequest, HostNftPurseDepositResponse,
	HostNftPurseObserveOperationStatusItem, HostNftPurseObserveOperationStatusRequest,
	HostNftPurseStaticReceiveKeyError, HostNftPurseStaticReceiveKeyRequest,
	HostNftPurseStaticReceiveKeyResponse,
};

use super::{
	cheques::{create_cheque, create_receive_key, public_key_of},
	operations::{Purses, product_purses},
	*,
};

/// The purse the sending product, the other product, hands its items out of.
const SENDER_PURSE: u32 = 2;

/// Purses of [`PRODUCT`], the receiver, and [`SENDER_PURSE`] of
/// the other product, the sender, which holds `count` items.
fn sender_and_receiver(count: u32) -> (Purses, ProductRuntimeHost, Vec<v01::NftPurseItemKey>) {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let sender = purses.other_product();
	create(&sender, "Sender").unwrap();
	let items = purses.held_items(PurseId(SENDER_PURSE), count);
	(purses, sender, items)
}

/// A cheque of `items` the sender makes for `to`.
fn sent_cheque(
	sender: &ProductRuntimeHost,
	to: v01::NftPurseChequeDestination,
	items: Vec<v01::NftPurseItemKey>,
) -> v01::NftPurseCheque {
	create_cheque(sender, SENDER_PURSE, to, items).expect("the sender makes the cheque")
}

/// A cheque of `count` items sent to a receive key [`PRODUCT`] made into
/// [`TICKETS`].
fn ephemeral_cheque(count: u32) -> (Purses, v01::NftPurseCheque) {
	let (purses, sender, items) = sender_and_receiver(count);
	let receive_key = create_receive_key(&purses.host, TICKETS).unwrap();
	let to = v01::NftPurseChequeDestination::Ephemeral(receive_key);
	let cheque = sent_cheque(&sender, to, items);
	(purses, cheque)
}

/// The destination of a `Static` cheque sealed to the host's static receive key.
fn to_static_key(cheque_id: [u8; 32]) -> v01::NftPurseChequeDestination {
	v01::NftPurseChequeDestination::Static {
		encryption_key: v01::X25519PublicKey(public_key_of(TEST_STATIC_SECRET)),
		cheque_id: v01::NftPurseChequeId(cheque_id),
	}
}

fn deposit(
	host: &ProductRuntimeHost,
	cheque: v01::NftPurseCheque,
	to: u32,
) -> Result<HostNftPurseDepositResponse, CallError<HostNftPurseDepositError>> {
	let request = v01::HostNftPurseDepositRequest { cheque, to: v01::NftPurseId(to) };
	block_on(host.deposit(&CallContext::default(), HostNftPurseDepositRequest::V1(request)))
}

fn deposit_refusal(error: v01::NftPurseDepositError) -> CallError<HostNftPurseDepositError> {
	CallError::Domain(HostNftPurseDepositError::V1(error))
}

/// The first status `host` sees for the operation it registered under `cheque`'s id.
fn first_status_of(host: &ProductRuntimeHost, cheque: &v01::NftPurseCheque) -> v01::NftPurseStatus {
	let idempotency_key = v01::NftPurseIdempotencyKey(cheque.id.0);
	let request = v01::HostNftPurseObserveOperationStatusRequest { idempotency_key };
	let request = HostNftPurseObserveOperationStatusRequest::V1(request);
	let mut statuses = block_on(host.observe_operation_status(&CallContext::default(), request));
	let first = block_on(statuses.next()).expect("a status").expect("the operation exists");
	let HostNftPurseObserveOperationStatusItem::V1(status) = first;
	status
}

/// A product follows its deposit by the cheque id, the only name it has for
/// it, so the deposit is registered under that id once it returns.
#[test]
fn an_ephemeral_cheque_deposits_into_its_receive_keys_purse_and_is_followed_by_its_id() {
	let (purses, cheque) = ephemeral_cheque(2);

	let deposited = deposit(&purses.host, cheque.clone(), TICKETS);

	assert_eq!(deposited, Ok(HostNftPurseDepositResponse::V1));
	assert_eq!(first_status_of(&purses.host, &cheque), v01::NftPurseStatus::InProgress);
}

/// A receive key belongs to the product that made it, so another product
/// cannot take the cheque, and learns nothing about the key from the answer.
#[test]
fn another_products_receive_key_answers_corrupted_cheque() {
	let (purses, cheque) = ephemeral_cheque(1);
	let other = purses.other_product();

	let refused = deposit(&other, cheque, SENDER_PURSE);

	assert_eq!(refused, Err(deposit_refusal(v01::NftPurseDepositError::CorruptedCheque)));
}

/// The receiver chose the purse when it made the receive key, so a cheque
/// for it cannot be taken into another.
#[test]
fn an_ephemeral_cheque_into_another_purse_answers_purse_mismatch() {
	let (purses, cheque) = ephemeral_cheque(1);

	let refused = deposit(&purses.host, cheque, 1);

	assert_eq!(refused, Err(deposit_refusal(v01::NftPurseDepositError::PurseMismatch)));
}

/// The cheque id is the deposit's idempotency key, so it must be the one
/// the receive key derives, not one a sender picked.
#[test]
fn an_ephemeral_cheque_under_another_id_answers_corrupted_cheque() {
	let (purses, cheque) = ephemeral_cheque(1);
	let renamed = v01::NftPurseCheque { id: v01::NftPurseChequeId([4; 32]), ..cheque };

	let refused = deposit(&purses.host, renamed, TICKETS);

	assert_eq!(refused, Err(deposit_refusal(v01::NftPurseDepositError::CorruptedCheque)));
}

/// The static key is the user's, not a product's, so any product may deposit
/// a cheque sealed to it, and each follows its own deposit.
#[test]
fn a_static_cheque_deposits_from_any_product_into_any_purse() {
	let (purses, sender, items) = sender_and_receiver(1);
	let cheque = sent_cheque(&sender, to_static_key([9; 32]), items);
	let other = purses.other_product();

	let by_receiver = deposit(&purses.host, cheque.clone(), 1);
	let by_other = deposit(&other, cheque.clone(), SENDER_PURSE);

	assert_eq!(by_receiver, Ok(HostNftPurseDepositResponse::V1));
	assert_eq!(by_other, Ok(HostNftPurseDepositResponse::V1));
	assert_eq!(first_status_of(&other, &cheque), v01::NftPurseStatus::InProgress);
}

/// Retrying a deposit must not move the items twice.
#[test]
fn a_cheque_deposited_twice_answers_operation_exists() {
	let (purses, cheque) = ephemeral_cheque(1);
	deposit(&purses.host, cheque.clone(), TICKETS).unwrap();

	let again = deposit(&purses.host, cheque, TICKETS);

	assert_eq!(again, Err(deposit_refusal(v01::NftPurseDepositError::OperationExists)));
}

#[test]
fn a_cheque_of_no_items_answers_empty_cheque() {
	let (purses, cheque) = ephemeral_cheque(1);
	let emptied = v01::NftPurseCheque { items: vec![], ..cheque };

	let refused = deposit(&purses.host, emptied, TICKETS);

	assert_eq!(refused, Err(deposit_refusal(v01::NftPurseDepositError::EmptyCheque)));
}

#[test]
fn a_deposit_into_an_unknown_purse_answers_purse_not_found() {
	let (purses, cheque) = ephemeral_cheque(1);

	let refused = deposit(&purses.host, cheque, 7);

	assert_eq!(refused, Err(deposit_refusal(v01::NftPurseDepositError::PurseNotFound)));
}

/// A cheque whose secrets were not sealed to the static key cannot be opened
/// with it.
#[test]
fn a_static_cheque_sealed_to_another_key_answers_corrupted_cheque() {
	let (purses, sender, items) = sender_and_receiver(1);
	let to = v01::NftPurseChequeDestination::Static {
		encryption_key: v01::X25519PublicKey(public_key_of([3; 32])),
		cheque_id: v01::NftPurseChequeId([9; 32]),
	};
	let cheque = sent_cheque(&sender, to, items);

	let refused = deposit(&purses.host, cheque, TICKETS);

	assert_eq!(refused, Err(deposit_refusal(v01::NftPurseDepositError::CorruptedCheque)));
}

fn static_receive_key(
	host: &ProductRuntimeHost,
) -> Result<v01::X25519PublicKey, CallError<HostNftPurseStaticReceiveKeyError>> {
	let request =
		HostNftPurseStaticReceiveKeyRequest::V1(v01::HostNftPurseStaticReceiveKeyRequest {});
	let HostNftPurseStaticReceiveKeyResponse::V1(response) =
		block_on(host.static_receive_key(&CallContext::default(), request))?;
	Ok(response.key)
}

/// The key belongs to the user, so every product publishes the same one.
#[test]
fn every_product_gets_the_static_receive_key() {
	let purses = product_purses(Arc::new(StubPlatform::default()));

	let by_product = static_receive_key(&purses.host);
	let by_other = static_receive_key(&purses.other_product());

	let expected = v01::X25519PublicKey(public_key_of(TEST_STATIC_SECRET));
	assert_eq!(by_product, Ok(expected));
	assert_eq!(by_other, Ok(expected));
}

/// Without a session there is no user whose key it is.
#[test]
fn the_static_receive_key_is_denied_without_a_session() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), false);

	let refused = static_receive_key(&host);

	assert_eq!(refused, Err(CallError::Denied));
}
