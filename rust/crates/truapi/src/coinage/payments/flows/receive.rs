//! K2 · Receive: listen for cheques, deposit them, and refund a failed
//! deposit automatically, once.

use futures::stream::{self, BoxStream, StreamExt};
use truapi::latest::{CoinPaymentCheque, CoinPaymentError, CoinPaymentStatus};

use super::{Payments, derived_key, internal, leaves, millis, refund::refund_deposit};
use crate::{
	coinage::{
		layer::{Error, Op},
		operation::Retry,
		payments::{
			channel,
			cheque::{ChequeHash, cheque_hash, open},
			clearing,
			receivable::Receivable,
			records::{self, DepositRecord, OperationRecord},
			status::{coin_payment_error, deposit_status},
			topic::payment_topic,
		},
		status::Status,
	},
	stream_extensions::StreamExtensions,
};

impl Payments {
	/// The cheques that arrive for a receivable this user agent holds. The
	/// same cheque may arrive more than once; depositing it again joins the
	/// first deposit.
	pub async fn listen(&self, receivable: Receivable) -> Result<BoxStream<'static, CoinPaymentCheque>, CoinPaymentError> {
		let record = self.0.store.read(move |conn| records::receivable(conn, receivable)).await.map_err(internal)?;
		record.ok_or(CoinPaymentError::ReceivableNotFound)?;
		let cheques = channel::subscribe(self.0.transport.as_ref(), payment_topic(receivable));
		Ok(cheques.filter(move |cheque| core::future::ready(cheque.id == receivable.0)).boxed())
	}

	/// Deposits `cheque` for `product` into its receivable's purse, or joins the deposit
	/// its hash already names (invariant 1), and follows its status. A cheque
	/// that does not open, whose coins do not add up to its amount, or whose
	/// claim deadline lies beyond the protocol's bounds is refused with
	/// `BadCoins`, and leaves no record.
	pub async fn deposit(
		&self,
		product: &str,
		cheque: CoinPaymentCheque,
	) -> Result<BoxStream<'static, CoinPaymentStatus>, CoinPaymentError> {
		let hash = cheque_hash(&cheque);
		if let Some((_, deposit)) = self.0.store.read(move |conn| records::deposit(conn, hash)).await.map_err(internal)? {
			return self.deposit_status(deposit).await;
		}
		let receivable = Receivable(cheque.id);
		let record = self.0.store.read(move |conn| records::receivable(conn, receivable)).await.map_err(internal)?;
		let record = record.ok_or(CoinPaymentError::ReceivableNotFound)?;
		let contents = open(&record.secret, &cheque).map_err(|_| CoinPaymentError::BadCoins)?;
		let declared: u64 = contents.coins.iter().map(|(_, exponent)| exponent.value()).sum();
		let params = self.0.params;
		let latest_deadline = self.now() + millis(params.max_claim_timeout) + millis(params.clock_tolerance);
		if declared != u64::from(cheque.amount) || contents.claim_by > latest_deadline {
			return Err(CoinPaymentError::BadCoins);
		}
		let key = derived_key("coinage/deposit", hash);
		let deposit = DepositRecord {
			cheque: hash,
			product: product.to_owned(),
			amount: u64::from(cheque.amount),
			claim_by: contents.claim_by,
			import: OperationRecord { key: key.clone(), refused: None },
			return_to: contents.return_to,
			refunds: Vec::new(),
			automatic_refund_made: false,
		};
		let recorded = deposit.clone();
		self.0.store.write(move |tx| records::record_deposit(tx, receivable, &recorded)).await.map_err(internal)?;
		let into = self.0.layer.resolve_purse(record.purse).await.map_err(internal)?;
		let coins = contents.coins.into_iter().map(|(secret, _)| secret).collect();
		let op = Op { key: key.clone(), retry: Retry::Until(contents.claim_by / 1000) };
		let deposit = match self.0.layer.import(op, into, coins).await {
			Ok(_) => deposit,
			Err(error) => {
				let refusal = coin_payment_error(&error);
				let refused = refusal.clone();
				self.0.store.write(move |tx| records::record_refusal(tx, &key, &refused)).await.map_err(internal)?;
				DepositRecord { import: OperationRecord { refused: Some(refusal), ..deposit.import }, ..deposit }
			},
		};
		self.deposit_status(deposit).await
	}

	/// A deposit's status, as H5 maps its import's: the current status at
	/// once, then each change, ending at `Done` or `Failed`.
	async fn deposit_status(
		&self,
		deposit: DepositRecord,
	) -> Result<BoxStream<'static, CoinPaymentStatus>, CoinPaymentError> {
		if let Some(error) = deposit.import.refused {
			let failed = CoinPaymentStatus::Failed { error, cleared: 0, reference: clearing::build(Vec::new()) };
			return Ok(stream::once(core::future::ready(failed)).boxed());
		}
		let key = deposit.import.key.clone();
		let items = self.0.layer.status(key.clone()).await.map_err(internal)?;
		let layer = self.0.layer.clone();
		let expected = deposit.amount;
		let statuses = items.then(move |item| {
			let (layer, key) = (layer.clone(), key.clone());
			async move {
				if !item.status.is_terminal() {
					return deposit_status(item, &[], expected, clearing::build(Vec::new()));
				}
				match layer.completion(key).await {
					Ok(Some(completion)) => deposit_status(item, &completion.coins, expected, clearing::build(leaves(&completion))),
					_ => deposit_status(item, &[], expected, clearing::build(Vec::new())),
				}
			}
		});
		Ok(statuses.take_until_inclusive(|status| !matches!(status, CoinPaymentStatus::Clearing { .. })))
	}
}

/// Starts the automatic refund of every deposit that ended other than `Done`
/// at its amount and whose automatic refund is not yet recorded as made (K2
/// `run_due`). A deposit under a return receivable is a refund, which is
/// never refunded.
pub async fn refund_failed_deposits(payments: &Payments) -> Result<(), Error> {
	let deposits = payments.0.store.read(records::deposits).await?;
	for (receivable, deposit) in deposits {
		if deposit.automatic_refund_made {
			continue;
		}
		let record = payments.0.store.read(move |conn| records::receivable(conn, receivable)).await?;
		if record.is_none_or(|record| record.return_receivable) {
			continue;
		}
		let ended_well = match &deposit.import.refused {
			Some(_) => false,
			None => match payments.0.layer.completion(deposit.import.key.clone()).await? {
				None => continue,
				Some(completion) => completion.status == Status::Done && completion.progress.settled == deposit.amount,
			},
		};
		if ended_well {
			continue;
		}
		if refund_deposit(payments, deposit.cheque, &deposit.product).await? {
			let cheque: ChequeHash = deposit.cheque;
			payments.0.store.write(move |tx| records::mark_automatic_refund(tx, cheque)).await?;
		}
	}
	Ok(())
}
