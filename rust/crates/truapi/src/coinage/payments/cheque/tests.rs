use super::*;
use crate::coinage::payments::receivable::new_receivable;

fn contents() -> ChequeContents {
	ChequeContents {
		coins: vec![(CoinSecret([1; 64]), Exponent::new(3).unwrap()), (CoinSecret([2; 64]), Exponent::new(0).unwrap())],
		return_to: Some(Receivable([9; 32])),
		claim_by: 1_700_000_000_000,
	}
}

fn sealed_to(to: Receivable) -> CoinPaymentCheque {
	seal(to, 9, &contents(), [7; 32], [8; 12]).unwrap()
}

#[test]
fn only_the_receivables_secret_opens_its_cheque() {
	let (to, secret) = new_receivable([1; 32]);
	let cheque = sealed_to(to);
	assert_eq!(cheque.id, to.0);
	assert_eq!(cheque.encrypted_secrets[0], VERSION);
	assert_eq!(open(&secret, &cheque), Ok(contents()));
	let (_, other) = new_receivable([2; 32]);
	assert_eq!(open(&other, &cheque), Err(OpenError::NotAuthentic));
}

#[test]
fn neither_the_receivable_nor_the_amount_can_be_changed() {
	let (to, secret) = new_receivable([1; 32]);
	let raised = CoinPaymentCheque { amount: 10, ..sealed_to(to) };
	assert_eq!(open(&secret, &raised), Err(OpenError::NotAuthentic));
	let mut tampered = sealed_to(to);
	*tampered.encrypted_secrets.last_mut().unwrap() ^= 1;
	assert_eq!(open(&secret, &tampered), Err(OpenError::NotAuthentic));
}

#[test]
fn coins_that_do_not_add_up_to_the_amount_are_not_sealed() {
	let (to, _) = new_receivable([1; 32]);
	assert_eq!(seal(to, 8, &contents(), [7; 32], [8; 12]), Err(SealError::AmountMismatch));
}

#[test]
fn a_low_order_receivable_is_refused() {
	assert_eq!(seal(Receivable([0; 32]), 9, &contents(), [7; 32], [8; 12]), Err(SealError::LowOrderReceivable));
	let (to, secret) = new_receivable([1; 32]);
	let mut cheque = sealed_to(to);
	cheque.encrypted_secrets[1..33].copy_from_slice(&[0; 32]);
	assert_eq!(open(&secret, &cheque), Err(OpenError::LowOrderReceivable));
}

#[test]
fn an_unknown_version_does_not_open() {
	let (to, secret) = new_receivable([1; 32]);
	let mut cheque = sealed_to(to);
	cheque.encrypted_secrets[0] = 2;
	assert_eq!(open(&secret, &cheque), Err(OpenError::UnknownVersion));
}

#[test]
fn a_cheque_is_named_by_the_hash_of_its_encoding() {
	let (to, _) = new_receivable([1; 32]);
	let cheque = sealed_to(to);
	assert_eq!(cheque_hash(&cheque), ChequeHash(blake2_256(&cheque.encode())));
	assert_ne!(cheque_hash(&cheque), cheque_hash(&seal(to, 9, &contents(), [7; 32], [9; 12]).unwrap()));
}
