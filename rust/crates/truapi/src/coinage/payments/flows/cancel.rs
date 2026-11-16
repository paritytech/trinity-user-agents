//! K4 · Cancel and K5 · Outgoing cheques: reclaim what a payee has not
//! claimed, on request or after the claim deadline, and collect refunds
//! until the refund deadline.
//!
//! A reclaim is keyed by the cheque and the number of its reclaims that
//! ended, so a cancellation and K5's reclaim of one cheque join one
//! operation (K4 invariant 3).

use futures::stream::{BoxStream, StreamExt};
use truapi::latest::{CoinPaymentError, CoinPaymentStatus};

use super::{Payments, derived_key, internal, millis};
use crate::{
	coinage::{
		classification::ExportStatus,
		layer::{Error, Op},
		operation::Retry,
		payments::{
			channel,
			cheque::ChequeHash,
			records::{self, OutgoingCheque},
			status::{cancellation_status, cheque_progress},
			topic::refund_topic,
		},
	},
	stream_extensions::StreamExtensions,
};

impl Payments {
	/// Reclaims what is still unclaimed of one of this user agent's outgoing
	/// cheques, without waiting for its claim deadline (K4 invariant 2), and
	/// follows the status its progress gives.
	pub async fn cancel(&self, cheque: ChequeHash) -> Result<BoxStream<'static, CoinPaymentStatus>, CoinPaymentError> {
		let outgoing = self.0.store.read(move |conn| records::outgoing(conn, cheque)).await.map_err(internal)?;
		let outgoing = outgoing.ok_or(CoinPaymentError::ReceivableNotFound)?;
		start_reclaim(self, &outgoing).await.map_err(internal)?;
		let statuses = self.0.layer.export_status(outgoing.export).await.map_err(internal)?;
		Ok(statuses
			.map(|coins| cancellation_status(cheque_progress(&coins)))
			.take_until_inclusive(|status| !matches!(status, CoinPaymentStatus::Clearing { .. })))
	}
}

/// K5 `run_due`: for every outgoing cheque, starts the next reclaim once its
/// claim deadline, the maximum claim validity window and the clock tolerance
/// have passed, while some coin is not yet stably claimed, reclaimed or never
/// minted; and collects the refunds of a payment cheque until its refund
/// deadline. Returns the earliest time, in Unix milliseconds, at which a
/// reclaim next comes due.
pub async fn run_outgoing_due(payments: &Payments) -> Result<Option<u64>, Error> {
	let now = payments.now();
	let params = payments.0.params;
	let mut next: Option<u64> = None;
	for outgoing in payments.0.store.read(records::all_outgoing).await? {
		let reclaim_at = outgoing.claim_by + millis(params.max_claim_validity) + millis(params.clock_tolerance);
		if now < reclaim_at {
			next = Some(next.map_or(reclaim_at, |earliest| earliest.min(reclaim_at)));
		} else if !settled(payments, &outgoing).await? {
			start_reclaim(payments, &outgoing).await?;
		}
		let collecting = outgoing.claim_by + millis(params.refund_window);
		if let Some(return_to) = outgoing.return_to.filter(|_| now < collecting) {
			for refund in channel::fetch(payments.0.transport.as_ref(), refund_topic(return_to)).await {
				if refund.id == return_to.0 {
					let _ = payments.deposit(&outgoing.product, refund).await;
				}
			}
		}
	}
	Ok(next)
}

/// Starts the next reclaim of `outgoing` into its source purse, resolved
/// through any deletion, unless one is in progress.
async fn start_reclaim(payments: &Payments, outgoing: &OutgoingCheque) -> Result<(), Error> {
	if payments.in_progress(&outgoing.reclaims).await? {
		return Ok(());
	}
	let hash = outgoing.hash;
	let key = derived_key("coinage/reclaim", (hash, payments.ended(&outgoing.reclaims).await?));
	let recorded = key.clone();
	payments.0.store.write(move |tx| records::record_reclaim(tx, hash, &recorded)).await?;
	let into = payments.0.layer.resolve_purse(outgoing.source).await?;
	let op = Op { key: key.clone(), retry: Retry::ExpiryOnly };
	if let Err(error) = payments.0.layer.reclaim(op, outgoing.export.clone(), into).await {
		let refusal = crate::coinage::payments::status::coin_payment_error(&error);
		payments.0.store.write(move |tx| records::record_refusal(tx, &key, &refusal)).await?;
	}
	Ok(())
}

/// Whether every coin of `outgoing` is stably claimed, reclaimed or never
/// minted.
async fn settled(payments: &Payments, outgoing: &OutgoingCheque) -> Result<bool, Error> {
	Ok(payments.export_statuses(&outgoing.export).await?.iter().all(|coin| {
		matches!(
			coin.status,
			ExportStatus::Claimed { stable: true, .. } | ExportStatus::Reclaimed { stable: true } | ExportStatus::NeverMinted
		)
	}))
}
