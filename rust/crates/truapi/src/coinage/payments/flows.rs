//! Band K · Flows: pay (K1), receive (K2), refund (K3), cancel (K4) and the
//! outgoing cheques' scheduled work (K5), over layer 1 and the channel.
//!
//! None stores anything of its own: every record a flow writes is J2's, and
//! is written before the layer-1 operation it names starts, under a key
//! derived from the operation's own inputs, so a crash never produces a
//! second operation (layer 2, section 1). [`Payments::run_due`] is the one
//! entry point that drives both layers.

mod cancel;
mod pay;
mod receive;
mod refund;

use core::time::Duration;
use std::sync::Arc;

use futures::StreamExt;
use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use truapi::latest::CoinPaymentError;

pub use self::pay::PayError;
use super::{
	channel::{self, Transport},
	clearing::Leaf,
	receivable::{Receivable, new_receivable},
	records::{self, OperationRecord, OutgoingCheque, ReceivableRecord},
	status::{ChequeProgress, cheque_progress},
	topic::{payment_topic, refund_topic},
};
use crate::{
	coinage::{
		PurseId,
		layer::{Error, ExportedCoinStatus, Layer},
		maintenance::Due,
		operation::OpKey,
		status::Completion,
	},
	store::{Db, DbError},
};

/// The layer-1 scope of every operation layer 2 starts under a derived or
/// random key.
pub const SCOPE: &[u8] = b"coinage/layer-2";

/// The parameters each implementation sets within the protocol's bounds
/// (RFC deviations, gap 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaymentParams {
	/// How long after sending a payee may claim a cheque.
	pub claim_timeout: Duration,
	/// How long after a cheque's claim deadline its payer collects a refund.
	pub refund_window: Duration,
	/// How far this user agent's clock may be from network time.
	pub clock_tolerance: Duration,
	/// The protocol's maximum claim timeout, which bounds an incoming
	/// cheque's deadline.
	pub max_claim_timeout: Duration,
	/// The protocol's maximum claim validity window, which a claim submitted
	/// just before the deadline may still land within.
	pub max_claim_validity: Duration,
}

impl PaymentParams {
	/// The parameters the core runs with, until the protocol fixes its
	/// maximums: a day to claim, a week to collect refunds, five minutes of
	/// clock tolerance, two days as the longest claim timeout accepted, and
	/// 256 six-second blocks as the longest claim validity window.
	pub const SHIPPED: Self = Self {
		claim_timeout: Duration::from_secs(24 * 3600),
		refund_window: Duration::from_secs(7 * 24 * 3600),
		clock_tolerance: Duration::from_secs(5 * 60),
		max_claim_timeout: Duration::from_secs(48 * 3600),
		max_claim_validity: Duration::from_secs(256 * 6),
	};
}

/// Layer 2's draws from §1's randomness.
pub trait Draws: Send + Sync {
	/// 32 fresh random bytes.
	fn draw(&self) -> [u8; 32];
}

/// Draws from the operating system.
pub struct SystemDraws;

impl Draws for SystemDraws {
	fn draw(&self) -> [u8; 32] {
		let mut draw = [0u8; 32];
		getrandom::getrandom(&mut draw).expect("the operating system provides randomness");
		draw
	}
}

/// What the flows run on.
pub struct PaymentDeps {
	/// The running layer 1.
	pub layer: Layer,
	/// The shared store, which holds layer 2's records too.
	pub store: Db,
	/// The statement store.
	pub transport: Arc<dyn Transport>,
	/// Receivables, ephemeral keys, nonces and random operation keys.
	pub draws: Arc<dyn Draws>,
	/// Network time, in Unix milliseconds.
	pub clock: Arc<dyn Fn() -> u64 + Send + Sync>,
	/// The parameters.
	pub params: PaymentParams,
}

/// Layer 2's flows.
#[derive(Clone)]
pub struct Payments(Arc<PaymentDeps>);

impl Payments {
	/// The flows over `deps`.
	pub fn new(deps: PaymentDeps) -> Self {
		Self(Arc::new(deps))
	}

	/// The running layer 1.
	pub fn layer(&self) -> &Layer {
		&self.0.layer
	}

	/// The shared store.
	pub fn store(&self) -> &Db {
		&self.0.store
	}

	/// Runs everything due at `now`, in Unix seconds: reclaims and refund
	/// collection (K5), automatic refunds (K2), posts that have not reached
	/// the transport (K1, K3), then layer 1's maintenance.
	pub async fn run_due(&self, now: u64) -> Result<Due, Error> {
		let reclaim_due = cancel::run_outgoing_due(self).await?;
		receive::refund_failed_deposits(self).await?;
		self.post_unposted().await?;
		let mut due = self.0.layer.run_due(now).await?;
		if let Some(at) = reclaim_due {
			let wait = Duration::from_millis(at.saturating_sub(self.now()));
			due.call_again_within = Some(due.call_again_within.map_or(wait, |layer| layer.min(wait)));
		}
		Ok(due)
	}

	fn now(&self) -> u64 {
		(self.0.clock)()
	}

	fn draw(&self) -> [u8; 32] {
		self.0.draws.draw()
	}

	fn nonce(&self) -> [u8; 12] {
		let draw = self.draw();
		draw[..12].try_into().expect("a draw has 32 bytes")
	}

	/// A key for an operation that runs under a random key: a payment's
	/// export, a rebalance or a deletion (layer 2, section 1).
	pub fn random_key(&self) -> OpKey {
		OpKey { scope: SCOPE.to_vec(), id: self.draw() }
	}

	/// A fresh receivable whose deposits go to `purse` (H1, recorded in J2).
	pub async fn create_receivable(&self, purse: PurseId) -> Result<Receivable, DbError> {
		let (receivable, secret) = new_receivable(self.draw());
		let record = ReceivableRecord { receivable, secret, purse, return_receivable: false, deposits: Vec::new() };
		self.0.store.write(move |tx| records::record_receivable(tx, &record)).await?;
		Ok(receivable)
	}

	/// How many of `records` ended: refused by layer 1, or completed there.
	async fn ended(&self, records: &[OperationRecord]) -> Result<u32, Error> {
		let mut ended = 0;
		for record in records {
			if record.refused.is_some() || self.0.layer.completion(record.key.clone()).await?.is_some() {
				ended += 1;
			}
		}
		Ok(ended)
	}

	/// Whether the last of `records` has not ended.
	async fn in_progress(&self, records: &[OperationRecord]) -> Result<bool, Error> {
		match records.last() {
			Some(last) => Ok(self.ended(core::slice::from_ref(last)).await? == 0),
			None => Ok(false),
		}
	}

	/// The current export status of every coin of `export`.
	async fn export_statuses(&self, export: &OpKey) -> Result<Vec<ExportedCoinStatus>, Error> {
		let mut statuses = self.0.layer.export_status(export.clone()).await?;
		Ok(statuses.next().await.unwrap_or_default())
	}

	/// The current progress of `cheque`.
	async fn progress(&self, cheque: &OutgoingCheque) -> Result<ChequeProgress, Error> {
		Ok(cheque_progress(&self.export_statuses(&cheque.export).await?))
	}

	/// Posts `cheque` to its topic as its product, retained until its claim
	/// deadline, and records the post once it succeeded.
	async fn post(&self, cheque: &OutgoingCheque) -> Result<(), DbError> {
		let topic = match cheque.return_to {
			Some(_) => payment_topic(cheque.payee),
			None => refund_topic(cheque.payee),
		};
		let transport = self.0.transport.as_ref();
		let posted = channel::post(transport, &cheque.product, topic, &cheque.cheque, cheque.claim_by / 1000).await;
		if posted.is_ok() {
			let hash = cheque.hash;
			self.0.store.write(move |tx| records::mark_posted(tx, hash)).await?;
		}
		Ok(())
	}

	/// Posts every refund cheque that no post has reached yet, before its
	/// claim deadline (K3 `run_due`).
	async fn post_unposted(&self) -> Result<(), DbError> {
		let now = self.now();
		for cheque in self.0.store.read(move |conn| records::unposted(conn, now)).await? {
			self.post(&cheque).await?;
		}
		Ok(())
	}
}

/// A duration in milliseconds.
fn millis(duration: Duration) -> u64 {
	u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// `OpKey { scope: "coinage/layer-2", id: BLAKE2b-256(label ‖ SCALE(inputs)) }`.
fn derived_key(label: &str, inputs: impl Encode) -> OpKey {
	OpKey { scope: SCOPE.to_vec(), id: blake2_256(&[label.as_bytes(), &inputs.encode()].concat()) }
}

/// One leaf per coin a completion's transactions consumed, naming the
/// transaction that moved it.
fn leaves(completion: &Completion) -> Vec<Leaf> {
	completion
		.transactions
		.iter()
		.flat_map(|settled| settled.consumed.iter().map(|coin| Leaf { coin: *coin, transaction: settled.hash }))
		.collect()
}

/// A failure of the store or layer 1 while serving a product, as RFC-17
/// reports it.
fn internal<E>(_: E) -> CoinPaymentError {
	CoinPaymentError::Internal
}

#[cfg(test)]
mod tests;
