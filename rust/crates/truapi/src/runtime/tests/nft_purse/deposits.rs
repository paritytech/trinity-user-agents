//! Depositing received cheques through the product-facing trait.

use futures::{StreamExt, executor::block_on};
use sp_crypto_hashing::blake2_256;
use truapi::versioned::nft_purse::{
	HostNftPurseDepositError, HostNftPurseDepositRequest, HostNftPurseDepositResponse,
	HostNftPurseObserveOperationStatusItem, HostNftPurseObserveOperationStatusRequest,
	HostNftPurseRefundError, HostNftPurseRefundRequest, HostNftPurseRefundResponse,
	HostNftPurseStaticReceiveKeyError, HostNftPurseStaticReceiveKeyRequest,
	HostNftPurseStaticReceiveKeyResponse,
};

use super::{
	cheques::{create_cheque, create_receive_key, first_status_under, public_key_of},
	operations::{Purses, product_purses, product_purses_over},
	*,
};
use crate::{
	nft::OperationKey,
	platform::{NftPurseDrainDestination, NftPurseDrainReview},
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

/// A deposit of nothing registers no operation a product could follow.
#[test]
fn a_cheque_of_no_items_answers_empty_cheque() {
	let (purses, cheque) = ephemeral_cheque(1);
	let emptied = v01::NftPurseCheque { items: vec![], ..cheque };

	let refused = deposit(&purses.host, emptied, TICKETS);

	assert_eq!(refused, Err(deposit_refusal(v01::NftPurseDepositError::EmptyCheque)));
}

/// Items go only into a purse that exists.
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

fn refund(
	host: &ProductRuntimeHost,
	cheque: &v01::NftPurseCheque,
) -> Result<HostNftPurseRefundResponse, CallError<HostNftPurseRefundError>> {
	let request = v01::HostNftPurseRefundRequest { cheque_id: cheque.id };
	block_on(host.refund(&CallContext::default(), HostNftPurseRefundRequest::V1(request)))
}

fn refund_refusal(error: v01::NftPurseRefundError) -> CallError<HostNftPurseRefundError> {
	CallError::Domain(HostNftPurseRefundError::V1(error))
}

/// Marks every move still waiting as finalized, as if each landed at a
/// finalized block.
fn finalize_waiting_moves(purses: &Purses) {
	let db = purses.services.core_db().unwrap();
	let finalize = |tx: &rusqlite::Transaction<'_>| {
		tx.execute(
			"UPDATE durable_tx SET status = 'FINALIZED_SUCCESS' WHERE status = 'PENDING_SUBMISSION'",
			[],
		)?;
		Ok(())
	};
	block_on(db.write(finalize)).unwrap();
}

/// The keys the deposit of `cheque` moved its items into, in its order.
fn deposited_items(purses: &Purses, cheque: &v01::NftPurseCheque) -> Vec<v01::NftPurseItemKey> {
	let deposit = OperationKey::new(PRODUCT.as_bytes(), cheque.id.0);
	let mut moves = purses.ledger.observe_operation(&deposit);
	let moves = block_on(moves.next()).unwrap().unwrap();
	moves.iter().map(|transfer| v01::NftPurseItemKey(transfer.minted.0)).collect()
}

/// A cheque of `count` items deposited into [`TICKETS`], with the deposit
/// finalized.
fn deposited_cheque(count: u32) -> (Purses, v01::NftPurseCheque) {
	let (purses, cheque) = ephemeral_cheque(count);
	deposit(&purses.host, cheque.clone(), TICKETS).unwrap();
	finalize_waiting_moves(&purses);
	(purses, cheque)
}

/// The receiver sends back what it deposited from its own purse without
/// being asked, and follows the moves under the key the RFC derives from
/// the cheque id.
#[test]
fn a_settled_deposit_is_refunded_under_its_refund_key() {
	let (purses, cheque) = deposited_cheque(2);

	let refunded = refund(&purses.host, &cheque).unwrap();

	let started = deposited_items(&purses, &cheque);
	let expected = v01::HostNftPurseRefundResponse { started, unavailable: vec![] };
	assert_eq!(refunded, HostNftPurseRefundResponse::V1(expected));
	let refund_key = blake2_256(&[b"nft-purse/refund".as_slice(), &cheque.id.0].concat());
	let status = first_status_under(&purses.host, refund_key);
	assert_eq!(
		status,
		Ok(HostNftPurseObserveOperationStatusItem::V1(v01::NftPurseStatus::InProgress))
	);
}

/// A refund must not race the deposit's own moves, which could still land.
#[test]
fn a_refund_while_the_deposit_runs_answers_deposit_in_progress() {
	let (purses, cheque) = ephemeral_cheque(1);
	deposit(&purses.host, cheque.clone(), TICKETS).unwrap();

	let refused = refund(&purses.host, &cheque);

	assert_eq!(refused, Err(refund_refusal(v01::NftPurseRefundError::DepositInProgress)));
}

/// Only the product that deposited a cheque can refund it.
#[test]
fn a_refund_of_a_cheque_this_product_never_deposited_answers_cheque_not_found() {
	let (purses, cheque) = deposited_cheque(1);

	let refused = refund(&purses.other_product(), &cheque);

	assert_eq!(refused, Err(refund_refusal(v01::NftPurseRefundError::ChequeNotFound)));
}

/// A refund is one operation under its key.
#[test]
fn a_second_refund_answers_operation_exists() {
	let (purses, cheque) = deposited_cheque(1);
	refund(&purses.host, &cheque).unwrap();

	let refused = refund(&purses.host, &cheque);

	assert_eq!(refused, Err(refund_refusal(v01::NftPurseRefundError::OperationExists)));
}

/// Items deposited into Root's purse belong to the user, so they leave only
/// if the user says so, and the user sees that they go back to the sender.
#[test]
fn a_refund_out_of_roots_purse_asks_the_user_and_a_refusal_is_denied() {
	let platform = Arc::new(StubPlatform::default());
	let purses = product_purses_over(platform.clone(), Arc::new(MemoryEncryptedStorage::default()));
	let sender = purses.other_product();
	create(&sender, "Sender").unwrap();
	let items = purses.held_items(PurseId(SENDER_PURSE), 2);
	let cheque = sent_cheque(&sender, to_static_key([9; 32]), items);
	let root = purses.root_purse();
	deposit(&purses.host, cheque.clone(), root.0).unwrap();
	finalize_waiting_moves(&purses);

	let refused = refund(&purses.host, &cheque);

	assert_eq!(refused, Err(refund_refusal(v01::NftPurseRefundError::Denied)));
	let review = NftPurseDrainReview {
		calling_product_id: PRODUCT.parse().unwrap(),
		from_purse: root.0,
		owner: v01::NftPurseOwner::Root,
		item_count: 2,
		destination: NftPurseDrainDestination::Refund,
	};
	assert_eq!(platform.nft_purse_drain_reviews.lock().unwrap().clone(), vec![review]);
}
