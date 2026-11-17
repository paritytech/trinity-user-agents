//! The hand-over of a wallet from the coinage engine a shipped app ran
//! (native interface, section 5): what the app knows and the chain does not,
//! recorded once, in the store transaction that mints the installation.
//!
//! [`record`] writes it. A reconstruction scans each handed-over
//! installation up to its high-water marks ([`Handover::marks`]) and adopts
//! the coins of the payments the app sent as handed out
//! ([`adopt_disclosed`]); the layer starts the imports of the payments the
//! app received after its start sequence.

use parity_scale_codec::{Decode, Encode};
use rusqlite::{Connection, Transaction};

use super::{
	Asset, CoinSecret, Exponent, ledger,
	operation::{self, OpKey, OpKind},
};
use crate::{installation::InstallationId, store::DbError};

/// The scope of every operation the app starts through the native
/// interface, and of those the hand-over records for it.
pub const APP_SCOPE: &[u8] = b"coinage/app";

/// What a shipped app hands over: its installations with their high-water
/// marks, the payments it sent that are not settled, and the payments it
/// received and has not claimed. The store keeps it, without the incoming
/// payments whose imports have started.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
#[derive(uniffi::Record)]
pub struct Handover {
	/// Every installation the app allocated under.
	pub installations: Vec<AppInstallation>,
	/// The payments whose secrets the app sent and whose coins it has not
	/// seen claimed or reclaimed.
	pub outstanding: Vec<OutstandingPayment>,
	/// The payments the app received and has not seen claimed.
	pub incoming: Vec<IncomingPayment>,
}

/// One installation of the app, with its two high-water marks: the lowest
/// coin index and the lowest entry index it never allocated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
#[derive(uniffi::Record)]
pub struct AppInstallation {
	/// The installation.
	pub id: InstallationId,
	/// The lowest coin index the app never allocated in the main purse.
	pub next_coin: u32,
	/// The lowest entry index the app never allocated in the main purse.
	pub next_entry: u32,
}

/// A payment the app sent, named by the coin accounts of the secrets in its
/// message.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[derive(uniffi::Record)]
pub struct OutstandingPayment {
	/// The id its export is recorded under, in [`APP_SCOPE`].
	pub id: [u8; 32],
	/// The accounts of its coins.
	pub accounts: Vec<[u8; 32]>,
}

/// A payment the app received and has not seen claimed.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[derive(uniffi::Record)]
pub struct IncomingPayment {
	/// The id its import starts under, in [`APP_SCOPE`].
	pub id: [u8; 32],
	/// Until when its claim is retried, in Unix seconds.
	pub claim_by: u64,
	/// The secrets of its coins.
	pub coins: Vec<CoinSecret>,
}

/// How far the scan of a hand-over has come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(uniffi::Enum)]
pub enum HandOverProgress {
	/// No hand-over was recorded.
	None,
	/// A handed-over installation's first scan has not completed.
	Scanning,
	/// Every handed-over installation has been scanned once.
	Done,
}

impl Handover {
	/// The high-water marks of `installation`, coins then entries, when it
	/// was handed over.
	pub fn marks(&self, installation: InstallationId) -> Option<(u32, u32)> {
		let handed = self.installations.iter().find(|handed| handed.id == installation)?;
		Some((handed.next_coin, handed.next_entry))
	}

	/// The id of the outstanding payment that lists `account`.
	pub fn payment_of(&self, account: &[u8; 32]) -> Option<[u8; 32]> {
		self.outstanding.iter().find(|payment| payment.accounts.contains(account)).map(|payment| payment.id)
	}
}

/// The key of the app's operation `id`.
pub fn app_key(id: [u8; 32]) -> OpKey {
	OpKey { scope: APP_SCOPE.to_vec(), id }
}

/// Records `handover` in `tx`, the transaction that mints the installation:
/// adds its installations, begins the export of each outstanding payment,
/// with no coins yet, and stores the record.
pub fn record(tx: &Transaction<'_>, handover: &Handover) -> Result<(), DbError> {
	let installations: Vec<InstallationId> = handover.installations.iter().map(|handed| handed.id).collect();
	ledger::add_installations(tx, &installations)?;
	for payment in &handover.outstanding {
		if let Err(error) = operation::begin(tx, &app_key(payment.id), OpKind::Export, &payment.accounts.encode())? {
			tracing::warn!(?error, "a handed-over payment's id is listed twice; the first is kept");
		}
	}
	ledger::write_hand_over(tx, &handover.encode())
}

/// The recorded hand-over; none when none was recorded.
pub fn read(conn: &Connection) -> Result<Option<Handover>, DbError> {
	ledger::hand_over(conn)
}

/// Forgets the incoming payment `id` once its import has started, or once
/// it was refused, so its secrets are no longer kept here.
pub fn forget_incoming(tx: &Transaction<'_>, id: [u8; 32]) -> Result<(), DbError> {
	let Some(mut handover) = read(tx)? else { return Ok(()) };
	handover.incoming.retain(|payment| payment.id != id);
	ledger::write_hand_over(tx, &handover.encode())
}

/// Marks `coin`, which a scan has just adopted, as handed out with a
/// committed disclosure mark, and adds it to the export of the outstanding
/// payment `payment`.
pub fn adopt_disclosed(tx: &Transaction<'_>, payment: [u8; 32], coin: Asset, exponent: Exponent) -> Result<(), DbError> {
	if ledger::mark_disclosed(tx, &[coin])?.is_err() {
		return Ok(());
	}
	ledger::commit_disclosed(tx, &[coin])?;
	let key = app_key(payment);
	let mut exported = operation::read(tx, &key)?.map(|export| export.exported).unwrap_or_default();
	exported.push((coin, exponent));
	operation::record_exported(tx, &key, &exported)
}

#[cfg(test)]
mod tests;
