//! K3 · Refund: return what a deposit cleared to its payer's return
//! receivable, never more (invariant 1), keyed so a retry joins the refund
//! in progress (invariant 2).

use std::collections::BTreeMap;

use futures::stream::{self, BoxStream, StreamExt};
use truapi::latest::{CoinPaymentError, CoinPaymentStatus};

use super::{Payments, derived_key, internal, millis};
use crate::{
	coinage::{
		Initiator,
		classification::ExportStatus,
		layer::{Error, ExportedCoinStatus, Op, commit_export_in},
		operation::Retry,
		payments::{
			cheque::{ChequeContents, ChequeHash, cheque_hash, seal},
			clearing::{self, Leaf},
			receivable::Receivable,
			records::{self, DepositRecord, OperationRecord, OutgoingCheque, RefundRecord},
			status::{cheque_progress, coin_payment_error, refund_status},
		},
	},
	stream_extensions::StreamExtensions,
};

impl Payments {
	/// Refunds everything still refundable under `receivable`, one refund
	/// cheque per deposit, and follows the status the refund cheques' progress
	/// gives (H5 invariant 4). With nothing refundable it ends at once, `Done`
	/// at zero.
	pub async fn refund(
		&self,
		product: &str,
		receivable: Receivable,
	) -> Result<BoxStream<'static, CoinPaymentStatus>, CoinPaymentError> {
		let record = self.0.store.read(move |conn| records::receivable(conn, receivable)).await.map_err(internal)?;
		let record = record.ok_or(CoinPaymentError::ReceivableNotFound)?;
		let mut missing_return_path = false;
		for deposit in &record.deposits {
			if refundable(self, deposit).await.map_err(internal)? == 0 {
				continue;
			}
			missing_return_path |= deposit.return_to.is_none();
			refund_deposit(self, deposit.cheque, product).await.map_err(internal)?;
		}
		let record = self.0.store.read(move |conn| records::receivable(conn, receivable)).await.map_err(internal)?;
		let exports: Vec<_> = record
			.into_iter()
			.flat_map(|record| record.deposits)
			.filter_map(|deposit| deposit.refunds.last().and_then(|refund| refund.cheque))
			.collect();
		let mut cheques = Vec::with_capacity(exports.len());
		for cheque in exports {
			let outgoing = self.0.store.read(move |conn| records::outgoing(conn, cheque)).await.map_err(internal)?;
			cheques.extend(outgoing.map(|outgoing| outgoing.export));
		}
		if cheques.is_empty() {
			let done = if missing_return_path {
				CoinPaymentStatus::Failed { error: CoinPaymentError::Internal, cleared: 0, reference: clearing::build(Vec::new()) }
			} else {
				CoinPaymentStatus::Done { cleared: 0, reference: clearing::build(Vec::new()) }
			};
			return Ok(stream::once(core::future::ready(done)).boxed());
		}
		let mut streams = Vec::with_capacity(cheques.len());
		for (position, export) in cheques.into_iter().enumerate() {
			let statuses = self.0.layer.export_status(export).await.map_err(internal)?;
			streams.push(statuses.map(move |coins| (position, coins)).boxed());
		}
		let count = streams.len();
		let statuses = stream::select_all(streams)
			.scan(BTreeMap::new(), move |latest: &mut BTreeMap<usize, Vec<ExportedCoinStatus>>, (position, coins)| {
				latest.insert(position, coins);
				let status = (latest.len() == count).then(|| {
					let progress: Vec<_> = latest.values().map(|coins| cheque_progress(coins)).collect();
					refund_status(&progress, missing_return_path, clearing::build(payer_claims(latest.values().flatten())))
				});
				core::future::ready(Some(status))
			})
			.filter_map(core::future::ready);
		Ok(statuses.take_until_inclusive(|status| !matches!(status, CoinPaymentStatus::Clearing { .. })))
	}
}

/// Refunds one deposit for `product`, whose statement-store key posts the
/// refund cheque, unless a refund of it is in progress. Returns whether a
/// refund counts as made: its export committed, layer 1 refused to start it,
/// or the deposit has no return path or nothing left to refund.
pub async fn refund_deposit(payments: &Payments, deposit: ChequeHash, product: &str) -> Result<bool, Error> {
	let Some((receivable, record)) = payments.0.store.read(move |conn| records::deposit(conn, deposit)).await? else {
		return Ok(true);
	};
	let refunds: Vec<OperationRecord> = record.refunds.iter().map(|refund| refund.operation.clone()).collect();
	if payments.in_progress(&refunds).await? {
		return Ok(true);
	}
	let key = derived_key("coinage/refund", (deposit, payments.ended(&refunds).await?));
	let Some(return_to) = record.return_to else {
		let refund = RefundRecord { operation: OperationRecord { key: key.clone(), refused: None }, cheque: None };
		payments
			.0
			.store
			.write(move |tx| {
				records::record_refund(tx, deposit, &refund)?;
				records::record_refusal(tx, &key, &CoinPaymentError::Internal)
			})
			.await?;
		return Ok(true);
	};
	let amount = refundable(payments, &record).await?;
	let Ok(amount_cents) = u32::try_from(amount) else { return Ok(true) };
	if amount == 0 {
		return Ok(true);
	}
	let refund = RefundRecord { operation: OperationRecord { key: key.clone(), refused: None }, cheque: None };
	payments.0.store.write(move |tx| records::record_refund(tx, deposit, &refund)).await?;
	let Some(purse) = payments.0.store.read(move |conn| records::receivable(conn, receivable)).await?.map(|r| r.purse)
	else {
		return Ok(true);
	};
	let source = payments.0.layer.resolve_purse(purse).await?;
	let claim_by = payments.now() + millis(payments.0.params.claim_timeout);
	let op = Op { key: key.clone(), retry: Retry::Until(claim_by / 1000) };
	let export = match payments.0.layer.export(op, source, amount, Initiator::User, true).await {
		Ok(export) => export,
		Err(error) => {
			let refusal = coin_payment_error(&error);
			payments.0.store.write(move |tx| records::record_refusal(tx, &key, &refusal)).await?;
			return Ok(true);
		},
	};
	let coins = export.coins.iter().map(|coin| (coin.secret.clone(), coin.exponent)).collect();
	let contents = ChequeContents { coins, return_to: None, claim_by };
	let Ok(cheque) = seal(return_to, amount_cents, &contents, payments.draw(), payments.nonce()) else {
		payments.0.layer.release_export(key).await?;
		return Ok(false);
	};
	let outgoing = OutgoingCheque {
		hash: cheque_hash(&cheque),
		cheque,
		source,
		payee: return_to,
		export: key.clone(),
		return_to: None,
		claim_by,
		product: product.to_owned(),
		post: true,
		posted: false,
		reclaims: Vec::new(),
	};
	let recorded = outgoing.clone();
	let committed = payments
		.0
		.store
		.write_unless_refused(move |tx| {
			records::record_outgoing(tx, &recorded)?;
			records::record_refund_cheque(tx, deposit, &key, recorded.hash)?;
			commit_export_in(tx, &key)
		})
		.await?;
	if committed.is_err() {
		return Ok(false);
	}
	payments.post(&outgoing).await?;
	Ok(true)
}

/// What may still be refunded under a deposit (J2 invariant 1): the value
/// its import settled, less what the payer has stably claimed of its
/// refunds.
async fn refundable(payments: &Payments, deposit: &DepositRecord) -> Result<u64, Error> {
	if deposit.import.refused.is_some() {
		return Ok(0);
	}
	let Some(import) = payments.0.layer.completion(deposit.import.key.clone()).await? else { return Ok(0) };
	let mut claimed = 0;
	for cheque in deposit.refunds.iter().filter_map(|refund| refund.cheque) {
		let Some(outgoing) = payments.0.store.read(move |conn| records::outgoing(conn, cheque)).await? else {
			continue;
		};
		claimed += payments.progress(&outgoing).await?.claimed;
	}
	Ok(import.progress.settled.saturating_sub(claimed))
}

/// One leaf per coin the payer stably claimed, naming its claim.
fn payer_claims<'a>(coins: impl Iterator<Item = &'a ExportedCoinStatus>) -> Vec<Leaf> {
	coins
		.filter_map(|coin| match coin.status {
			ExportStatus::Claimed { stable: true, by } => Some(Leaf { coin: coin.account, transaction: by }),
			_ => None,
		})
		.collect()
}
