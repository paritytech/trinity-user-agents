//! Receiving cheques over the statement store: the topic a receiver watches,
//! and the envelopes it accepts there.
//!
//! An envelope is the statement data a sender publishes: a sealed box over
//! `SCALE(version, topic, cheque)`, sealed to the key that opens the cheque.

use core::future::ready;

use futures::stream::{self, BoxStream, StreamExt};
use parity_scale_codec::{Decode, DecodeAll, Encode};
use sp_crypto_hashing::blake2_256;
use truapi::latest::{
	NftPurseCheque, NftPurseChequeDestination, NftPurseChequeId, NftPurseChequeRecipient,
	NftPurseReceiveKey, StatementTopic,
};

use super::{NftPurses, cheque::cheque_id_for, receive_key::ReceiveSecretError};
use crate::{
	host_logic::{
		statement_store::{
			StatementBatches, StatementStoreParseError, StatementWatchError, TopicFilter,
			decode_statement_data,
		},
		x25519_seal::{SealError, SealedBox, X25519PrivateKey},
	},
	platform::ProductId,
	runtime::AuthorityError,
	store::DbError,
	stream_extensions::StreamExtensions as _,
};

/// The version of the envelope this host reads and writes.
pub const ENVELOPE_VERSION: u8 = 0;

/// Domain of the topic a one-time payment is published under.
const ONE_TIME_TOPIC_DOMAIN: &[u8] = b"nft-purse/payment-channel";

/// Domain of the topic a payment inbox is published under.
const INBOX_TOPIC_DOMAIN: &[u8] = b"nft-purse/payment-inbox";

/// A started payment listen: the statement topic the sender publishes under,
/// and what arrives on it.
pub struct PaymentListen<T> {
	/// The topic the host watches.
	pub statement_topic: StatementTopic,
	/// What arrives. Ends with an `Err` when the watch fails or ends.
	pub arrivals: BoxStream<'static, Result<T, PaymentListenError>>,
}

/// Why a payment listen did not start, or stopped.
#[derive(Debug, thiserror::Error)]
pub enum PaymentListenError {
	/// The receive key was not made on this host by the listening product.
	#[error("receive key {} was not made here by the listening product", hex::encode(.0.0))]
	UnknownReceiveKey(NftPurseReceiveKey),
	/// The secret of the receive key could not be read.
	#[error(transparent)]
	ReceiveSecret(#[from] ReceiveSecretError),
	/// The static receive secret is unavailable, as without a session.
	#[error("the static receive secret is unavailable: {0}")]
	StaticSecret(AuthorityError),
	/// The statement watch failed.
	#[error(transparent)]
	Watch(#[from] StatementWatchError),
	/// The statement watch ended while the listen was still waiting.
	#[error("the statement subscription ended")]
	WatchEnded,
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// The plaintext of an envelope.
#[derive(Encode, Decode)]
pub struct EnvelopeContents {
	/// [`ENVELOPE_VERSION`] for an envelope this host reads.
	pub version: u8,
	/// The topic the sender published the envelope under.
	pub topic: StatementTopic,
	/// The cheque the envelope carries.
	pub cheque: NftPurseCheque,
}

/// What a listen accepts: cheques for `recipient` published under `topic`,
/// with the id `id` when the listen names one.
struct ExpectedCheque {
	topic: StatementTopic,
	recipient: NftPurseChequeRecipient,
	id: Option<NftPurseChequeId>,
}

/// Why a statement under a watched topic is not a cheque for the listen.
#[derive(Debug, thiserror::Error)]
enum RejectedEnvelope {
	#[error("the statement does not decode: {0}")]
	Statement(#[from] StatementStoreParseError),
	#[error("the envelope does not open: {0}")]
	Seal(#[from] SealError),
	#[error("the envelope does not decode: {0}")]
	Contents(#[from] parity_scale_codec::Error),
	#[error("the envelope has version {0}, expected {ENVELOPE_VERSION}")]
	Version(u8),
	#[error("the envelope names another topic or recipient")]
	Mismatch,
}

impl NftPurses {
	/// Starts watching the statement topic of `key` for its first cheque. Fails before
	/// watching when `creator` did not make `key`.
	pub async fn start_one_time_payment_listen(
		&self,
		creator: &ProductId,
		key: NftPurseReceiveKey,
	) -> Result<PaymentListen<NftPurseCheque>, PaymentListenError> {
		self.check_receive_key_owner(creator, key).await?;
		let secret = self.receive_secret(&key).await?;
		let statement_topic = one_time_payment_topic(&key);
		let batches = self.start_topic_watch(statement_topic).await?;
		let id = cheque_id_for(&NftPurseChequeDestination::Ephemeral(key));
		let recipient = NftPurseChequeRecipient::Ephemeral(key);
		let expected = ExpectedCheque { topic: statement_topic, recipient, id: Some(id) };
		let arrivals = arriving_cheque_batches(batches, secret, expected)
			.flat_map(|batch| {
				let cheques = transpose_batch(batch);
				stream::iter(cheques)
			})
			.take(1)
			.boxed();
		Ok(PaymentListen { statement_topic, arrivals })
	}

	/// Starts watching `creator`'s inbox `inbox_domain` of the identity
	/// `account`. Each arrival is a batch of `Static` cheques: the first holds
	/// every cheque already stored, possibly none, and no later one is empty.
	pub async fn start_payment_inbox_listen(
		&self,
		account: [u8; 32],
		creator: &ProductId,
		inbox_domain: [u8; 32],
	) -> Result<PaymentListen<Vec<NftPurseCheque>>, PaymentListenError> {
		let secret = self
			.static_secret
			.static_receive_secret()
			.map_err(PaymentListenError::StaticSecret)?;
		let statement_topic = payment_inbox_topic(account, creator, inbox_domain);
		let batches = self.start_topic_watch(statement_topic).await?;
		let recipient = NftPurseChequeRecipient::Static;
		let expected = ExpectedCheque { topic: statement_topic, recipient, id: None };
		let batches = arriving_cheque_batches(batches, secret, expected);
		let arrivals = drop_empty_updates(batches);
		Ok(PaymentListen { statement_topic, arrivals })
	}

	/// Fails when `creator` did not make `key` on this host.
	async fn check_receive_key_owner(
		&self,
		creator: &ProductId,
		key: NftPurseReceiveKey,
	) -> Result<(), PaymentListenError> {
		let purse = self.receive_key_purse(creator, key).await?;
		purse.map(|_| ()).ok_or(PaymentListenError::UnknownReceiveKey(key))
	}

	/// Starts watching the statements published under `topic`.
	async fn start_topic_watch(
		&self,
		topic: StatementTopic,
	) -> Result<StatementBatches, StatementWatchError> {
		let filter = TopicFilter::MatchAll(vec![topic.0]);
		self.statements.start_statement_watch(filter).await
	}
}

/// The topic a one-time payment to `key` is published under.
fn one_time_payment_topic(key: &NftPurseReceiveKey) -> StatementTopic {
	let preimage = [ONE_TIME_TOPIC_DOMAIN, &key.0].concat();
	StatementTopic(blake2_256(&preimage))
}

/// The topic of `creator`'s inbox `inbox_domain` of the identity `account`.
fn payment_inbox_topic(
	account: [u8; 32],
	creator: &ProductId,
	inbox_domain: [u8; 32],
) -> StatementTopic {
	let product = blake2_256(creator.as_str().as_bytes());
	let preimage = [INBOX_TOPIC_DOMAIN, &account, &product, &inbox_domain].concat();
	StatementTopic(blake2_256(&preimage))
}

/// `batches` without the empty updates after the first batch, which holds
/// what was already stored.
fn drop_empty_updates<E: Send + 'static>(
	batches: BoxStream<'static, Result<Vec<NftPurseCheque>, E>>,
) -> BoxStream<'static, Result<Vec<NftPurseCheque>, E>> {
	batches
		.enumerate()
		.filter(|(index, batch)| {
			ready(*index == 0 || !matches!(batch, Ok(cheques) if cheques.is_empty()))
		})
		.map(|(_, batch)| batch)
		.boxed()
}

/// The cheques `expected` accepts in each batch of `batches`, opened with
/// `secret`, ending with an `Err` when the watch fails or ends.
fn arriving_cheque_batches(
	batches: StatementBatches,
	secret: X25519PrivateKey,
	expected: ExpectedCheque,
) -> BoxStream<'static, Result<Vec<NftPurseCheque>, PaymentListenError>> {
	batches
		.map(move |batch| {
			let statements = batch?;
			Ok(accepted_cheques(&statements, &secret, &expected))
		})
		.chain(stream::once(ready(Err(PaymentListenError::WatchEnded))))
		.take_until_inclusive(Result::is_err)
}

/// The cheques of `batch` one by one, or its error.
fn transpose_batch<E>(batch: Result<Vec<NftPurseCheque>, E>) -> Vec<Result<NftPurseCheque, E>> {
	match batch {
		Ok(cheques) => cheques.into_iter().map(Ok).collect(),
		Err(error) => vec![Err(error)],
	}
}

/// The cheques among `statements` that open with `secret` and that `expected`
/// accepts.
fn accepted_cheques(
	statements: &[Vec<u8>],
	secret: &X25519PrivateKey,
	expected: &ExpectedCheque,
) -> Vec<NftPurseCheque> {
	statements
		.iter()
		.filter_map(|statement| accepted_cheque(statement, secret, expected))
		.collect()
}

/// The cheque in `statement` when it opens with `secret` and `expected`
/// accepts it, or `None`, logged, for any other statement.
fn accepted_cheque(
	statement: &[u8],
	secret: &X25519PrivateKey,
	expected: &ExpectedCheque,
) -> Option<NftPurseCheque> {
	open_envelope(statement, secret)
		.and_then(|contents| cheque_matching_expected(contents, expected))
		.inspect_err(|reason| tracing::debug!("skipped a payment statement: {reason}"))
		.ok()
}

/// The contents of the envelope in the data of `statement`, opened with
/// `secret`.
fn open_envelope(
	statement: &[u8],
	secret: &X25519PrivateKey,
) -> Result<EnvelopeContents, RejectedEnvelope> {
	let data = decode_statement_data(statement)?;
	let plaintext = SealedBox::from_bytes(&data)?.open(secret)?;
	Ok(EnvelopeContents::decode_all(&mut plaintext.as_slice())?)
}

/// The cheque in `contents`, when its version is known and it matches
/// `expected`.
fn cheque_matching_expected(
	contents: EnvelopeContents,
	expected: &ExpectedCheque,
) -> Result<NftPurseCheque, RejectedEnvelope> {
	if contents.version != ENVELOPE_VERSION {
		return Err(RejectedEnvelope::Version(contents.version));
	}
	let cheque = contents.cheque;
	let id_matches = expected.id.is_none_or(|id| id == cheque.id);
	if contents.topic != expected.topic || cheque.recipient != expected.recipient || !id_matches {
		return Err(RejectedEnvelope::Mismatch);
	}
	Ok(cheque)
}

#[cfg(test)]
mod tests;
