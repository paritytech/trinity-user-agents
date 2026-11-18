//! K1 · Pay: export, seal and hand over a cheque, and follow its progress.

use futures::stream::{BoxStream, StreamExt};
use truapi::latest::{CoinPaymentCheque, CoinPaymentError};

use super::{Payments, internal, millis};
use crate::{
	coinage::{
		Initiator, PurseId,
		layer::{Error, Op, commit_export_in},
		operation::Retry,
		payments::{
			cheque::{ChequeContents, ChequeHash, accepts, cheque_hash, seal},
			receivable::{Receivable, new_receivable},
			records::{self, OutgoingCheque, ReceivableRecord},
			status::{ChequeProgress, cheque_progress, coin_payment_error},
		},
	},
	stream_extensions::StreamExtensions,
};

/// Why a payment did not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PayError {
	/// Layer 1 refused, or the cheque could not be sealed.
	Refused(CoinPaymentError),
	/// Only held-back value covers the payment; the caller asks the user
	/// and pays again with `confirmed`.
	NeedsConfirmation,
}

impl Payments {
	/// Pays `amount` cents from `source` to `to` for `product` and returns
	/// the sealed cheque, which the product hands over. The export is
	/// committed in the store transaction that records the outgoing cheque,
	/// before the cheque exists anywhere else (invariant 1); it waits for no
	/// chain transaction (invariant 3).
	pub async fn pay(
		&self,
		product: &str,
		source: PurseId,
		to: Receivable,
		amount: u32,
		initiator: Initiator,
		confirmed: bool,
	) -> Result<CoinPaymentCheque, PayError> {
		if !accepts(to) {
			return Err(PayError::Refused(CoinPaymentError::ReceivableNotFound));
		}
		let claim_by = self.now() + millis(self.0.params.claim_timeout);
		let key = self.random_key();
		let op = Op { key: key.clone(), retry: Retry::Until(claim_by / 1000) };
		let export = self.0.layer.export(op, source, u64::from(amount), initiator, confirmed).await.map_err(|error| {
			match error {
				Error::NeedsConfirmation => PayError::NeedsConfirmation,
				error => PayError::Refused(coin_payment_error(&error)),
			}
		})?;
		let (return_to, return_secret) = new_receivable(self.draw());
		let coins = export.coins.iter().map(|coin| (coin.secret.clone(), coin.exponent)).collect();
		let contents = ChequeContents { coins, return_to: Some(return_to), claim_by };
		let Ok(cheque) = seal(to, amount, &contents, self.draw(), self.nonce()) else {
			let _ = self.0.layer.release_export(key).await;
			return Err(PayError::Refused(CoinPaymentError::Internal));
		};
		let outgoing = OutgoingCheque {
			hash: cheque_hash(&cheque),
			cheque: cheque.clone(),
			source,
			payee: to,
			export: key.clone(),
			return_to: Some(return_to),
			claim_by,
			product: product.to_owned(),
			post: false,
			posted: false,
			reclaims: Vec::new(),
		};
		let return_record = ReceivableRecord {
			receivable: return_to,
			secret: return_secret,
			purse: source,
			return_receivable: true,
			deposits: Vec::new(),
		};
		let recorded = outgoing.clone();
		let export_key = key.clone();
		let committed = self
			.0
			.store
			.write_unless_refused(move |tx| {
				records::record_receivable(tx, &return_record)?;
				records::record_outgoing(tx, &recorded)?;
				commit_export_in(tx, &export_key)
			})
			.await;
		if !matches!(committed, Ok(Ok(()))) {
			let _ = self.0.layer.release_export(key).await;
			return Err(PayError::Refused(CoinPaymentError::Internal));
		}
		Ok(cheque)
	}

	/// The progress of one of this user agent's outgoing cheques: the current
	/// value at once, then each change, closing when nothing is minting or
	/// claimable.
	pub async fn cheque_status(
		&self,
		cheque: ChequeHash,
	) -> Result<BoxStream<'static, ChequeProgress>, CoinPaymentError> {
		let outgoing = self.0.store.read(move |conn| records::outgoing(conn, cheque)).await.map_err(internal)?;
		let outgoing = outgoing.ok_or(CoinPaymentError::ReceivableNotFound)?;
		let statuses = self.0.layer.export_status(outgoing.export).await.map_err(internal)?;
		Ok(statuses.map(|coins| cheque_progress(&coins)).take_until_inclusive(ChequeProgress::is_settled))
	}
}
