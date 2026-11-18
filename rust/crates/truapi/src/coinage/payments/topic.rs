//! H2 · Topics: where a payee listens for a cheque, and where a payer
//! listens for a refund.
//!
//! Each topic hashes the receivable under its own label, so a payment topic
//! never equals a refund topic (invariant 1), and a topic reveals nothing
//! the receivable does not (invariant 2).

use sp_crypto_hashing::blake2_256;

use super::receivable::Receivable;

/// A statement-store topic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Topic(pub [u8; 32]);

/// Where a payee listens for the cheque sealed to `receivable`:
/// `BLAKE2b-256("coinage/payment-topic" ‖ receivable)`.
pub fn payment_topic(receivable: Receivable) -> Topic {
	Topic(blake2_256(&[b"coinage/payment-topic".as_slice(), &receivable.0].concat()))
}

/// Where a payer listens for a refund to `return_receivable`:
/// `BLAKE2b-256("coinage/refund-topic" ‖ return_receivable)`.
pub fn refund_topic(return_receivable: Receivable) -> Topic {
	Topic(blake2_256(&[b"coinage/refund-topic".as_slice(), &return_receivable.0].concat()))
}

#[cfg(test)]
mod tests;
