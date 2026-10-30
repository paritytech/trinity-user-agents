//! Deposits: opening a received cheque and moving its items into a purse.

use truapi::latest::{
	NftPurseCheque, NftPurseChequeDestination, NftPurseChequeId, NftPurseChequeRecipient,
	NftPurseIdempotencyKey, NftPurseReceiveKey,
};

use super::{
	NftPurses,
	cheque::{CorruptedCheque, cheque_id_for, open_cheque},
	dao,
	operation::operation_key,
	receive_key::ReceiveSecretError,
};
use crate::{
	host_logic::x25519_seal::X25519PrivateKey,
	nft::{ImportRequest, OperationKey, PurseId, StartImportError},
	platform::ProductId,
	runtime::AuthorityError,
	store::DbError,
};

/// Why a deposit did not start. Nothing was registered.
#[derive(Debug, thiserror::Error)]
pub enum DepositRefusal {
	/// The cheque lists no items.
	#[error("the cheque lists no items")]
	EmptyCheque,
	/// No purse has the id the items should go into.
	#[error("purse {} does not exist", .0.0)]
	PurseNotFound(PurseId),
	/// The receive key was not made on this host by the depositing product.
	#[error("receive key {} was not made here by the depositing product", hex::encode(.0.0))]
	UnknownReceiveKey(NftPurseReceiveKey),
	/// The cheque carries an id other than the one its receive key derives.
	#[error("cheque id {}, expected {} derived from its receive key", hex::encode(.id.0), hex::encode(.derived.0))]
	ChequeIdMismatch {
		/// The id the cheque carries.
		id: NftPurseChequeId,
		/// The id its receive key derives.
		derived: NftPurseChequeId,
	},
	/// The receive key sends its cheques into another purse.
	#[error("the receive key goes into purse {}, not {}", .receive_key_purse.0, .to.0)]
	PurseMismatch {
		/// The purse the receive key was made into.
		receive_key_purse: PurseId,
		/// The purse the deposit named.
		to: PurseId,
	},
	/// The cheque does not open into its items.
	#[error(transparent)]
	CorruptedCheque(#[from] CorruptedCheque),
	/// The secret of the receive key could not be read.
	#[error(transparent)]
	ReceiveSecret(#[from] ReceiveSecretError),
	/// The static receive secret is unavailable, as without a session.
	#[error("the static receive secret is unavailable: {0}")]
	StaticSecret(AuthorityError),
	/// The import did not start.
	#[error(transparent)]
	Start(#[from] StartImportError),
	/// The database failed.
	#[error(transparent)]
	Db(#[from] DbError),
}

/// The operation `depositor`'s deposit of cheque `id` is registered as: the
/// cheque id is its idempotency key.
pub fn deposit_operation(depositor: &ProductId, id: &NftPurseChequeId) -> OperationKey {
	operation_key(depositor, NftPurseIdempotencyKey(id.0))
}

impl NftPurses {
	/// Opens `cheque` and starts moving its items into `to`. Returns once the
	/// moves are registered under the cheque id, scoped to `depositor`. An
	/// `Ephemeral` cheque may be deposited only by the product that made its
	/// receive key, and only into that key's purse.
	pub async fn deposit(
		&self,
		depositor: &ProductId,
		cheque: &NftPurseCheque,
		to: PurseId,
	) -> Result<(), DepositRefusal> {
		self.validate_deposit(depositor, cheque, to).await?;
		let secret = self.recipient_secret(&cheque.recipient).await?;
		let keys = open_cheque(&secret, cheque)?;
		let operation = deposit_operation(depositor, &cheque.id);
		self.ledger.start_imports(ImportRequest { operation, into: to, keys }).await?;
		Ok(())
	}

	/// Checks that `cheque` lists items, that `to` exists, and that
	/// `depositor` may deposit `cheque` into it.
	async fn validate_deposit(
		&self,
		depositor: &ProductId,
		cheque: &NftPurseCheque,
		to: PurseId,
	) -> Result<(), DepositRefusal> {
		if cheque.items.is_empty() {
			return Err(DepositRefusal::EmptyCheque);
		}
		if self.ledger.purse(to).await?.is_none() {
			return Err(DepositRefusal::PurseNotFound(to));
		}
		self.validate_recipient(depositor, cheque, to).await
	}

	/// Checks that `depositor` may deposit `cheque` into `to`: any product may
	/// deposit a `Static` cheque anywhere, while an `Ephemeral` one belongs to
	/// its receive key's maker and purse.
	async fn validate_recipient(
		&self,
		depositor: &ProductId,
		cheque: &NftPurseCheque,
		to: PurseId,
	) -> Result<(), DepositRefusal> {
		let NftPurseChequeRecipient::Ephemeral(key) = cheque.recipient else {
			return Ok(());
		};
		let derived = cheque_id_for(&NftPurseChequeDestination::Ephemeral(key));
		if cheque.id != derived {
			return Err(DepositRefusal::ChequeIdMismatch { id: cheque.id, derived });
		}
		let record = self
			.db
			.read(move |conn| dao::find_receive_key(conn, &key))
			.await?
			.filter(|record| record.creator == depositor.as_str())
			.ok_or(DepositRefusal::UnknownReceiveKey(key))?;
		if record.purse != to {
			return Err(DepositRefusal::PurseMismatch { receive_key_purse: record.purse, to });
		}
		Ok(())
	}

	/// The X25519 secret that opens cheques for `recipient`.
	async fn recipient_secret(
		&self,
		recipient: &NftPurseChequeRecipient,
	) -> Result<X25519PrivateKey, DepositRefusal> {
		match recipient {
			NftPurseChequeRecipient::Ephemeral(key) => Ok(self.receive_secret(key).await?),
			NftPurseChequeRecipient::Static => {
				self.static_secret.static_receive_secret().map_err(DepositRefusal::StaticSecret)
			},
		}
	}
}
