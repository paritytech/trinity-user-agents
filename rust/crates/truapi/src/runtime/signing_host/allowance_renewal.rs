//! Ledger and driver for automatic statement-store allowance renewal.
//!
//! The ledger records which accounts this signing host promised to keep
//! allowed, as derivation recipes where possible so entries stay valid when
//! the host rotates to a new root entropy. The driver resolves them against
//! the active session and runs the chain-pure pass in
//! `statement_allowance::renewal`, either once (`renew_now`) or on a periodic
//! tick (`start_renewal_loop`).

#[cfg(not(target_arch = "wasm32"))]
mod driver;

#[cfg(not(target_arch = "wasm32"))]
pub use driver::{
	active_owner_key, list, renew_now, start_renewal_loop, untrack_account_for_signing_host,
};

use crate::{
	host_logic::product_account::{ProductAccountError, derive_root_keypair_from_entropy},
	platform::{CoreStorage, CoreStorageKey, ProductId},
	runtime::{
		authority::AuthorityError, statement_allowance::StatementAllowanceError,
		statement_store_rpc::StatementStoreRpcClientError,
	},
};
use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};
use tracing::warn;

use super::{SigningHost, allowance::AllowanceAllocationError};
use truapi::latest::GenericError;

/// Failure reading renewal targets or preparing a statement-store renewal pass.
#[derive(Debug, thiserror::Error)]
pub enum StatementRenewalError {
	/// The active authority could not supply the renewal identity.
	#[error(transparent)]
	Authority(#[from] AuthorityError),
	/// A tracked account could not be derived.
	#[error(transparent)]
	ProductAccount(#[from] ProductAccountError),
	/// The clock could not supply the current allowance period.
	#[error(transparent)]
	Clock(#[from] AllowanceAllocationError),
	/// The chain could not be opened for a renewal pass.
	#[error(transparent)]
	RpcClient(#[from] StatementStoreRpcClientError),
	/// Chain state or allowance preparation failed.
	#[error(transparent)]
	Allowance(#[from] StatementAllowanceError),
	/// The persisted targets could not be read.
	#[error("renewal ledger read failed: {0}")]
	ReadLedger(#[source] GenericError),
	/// The persisted targets could not be written.
	#[error("renewal ledger write failed: {0}")]
	WriteLedger(#[source] GenericError),
	/// Persisted targets are not valid SCALE values.
	#[error("invalid persisted renewal targets: {0}")]
	DecodeLedger(#[from] parity_scale_codec::Error),
	/// Bytes after a complete target list make the ledger invalid.
	#[error("invalid persisted renewal targets: trailing bytes")]
	TrailingLedgerBytes,
	/// The session ended before ring membership could be resolved.
	#[error("no active session for statement-store renewal")]
	NoActiveSession,
	/// Renewal requires a personhood membership for its allowance proof.
	#[error(
		"signing account is not a member of any personhood ring; cannot renew statement-store allowances"
	)]
	MissingPersonhoodMembership,
}

impl From<StatementRenewalError> for GenericError {
	fn from(error: StatementRenewalError) -> Self {
		Self { reason: error.to_string() }
	}
}

/// A statement account or derivation recipe the wallet keeps renewed.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum StatementRenewalTarget {
	/// `//allowance//statement-store//{product_id}` from the active root entropy.
	ProductStatementAllowance {
		/// Product the allowance account belongs to.
		product_id: ProductId,
	},
	/// `//wallet//sso` from the active root entropy.
	WalletSso,
	/// A fixed account, e.g. a pairing peer's device statement key.
	Account {
		/// Account to keep allowed.
		account_id: [u8; 32],
		/// Human-readable name used in logs and reports.
		label: String,
	},
}

/// Renewal recipes follow the active wallet; fixed accounts belong to their recorded owner.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct TrackedStatementRenewalTarget {
	/// The account, or the recipe for one, that the host promised to renew.
	pub target: StatementRenewalTarget,
	/// Root public key that promised a raw account id. A recipe carries none
	/// and resolves under whichever identity is active.
	pub owner: Option<[u8; 32]>,
}

impl TrackedStatementRenewalTarget {
	/// Record `target` under `owner`, which only raw account ids retain.
	fn new(target: StatementRenewalTarget, owner: [u8; 32]) -> Self {
		let owner = match &target {
			StatementRenewalTarget::Account { .. } => Some(owner),
			StatementRenewalTarget::ProductStatementAllowance { .. } |
			StatementRenewalTarget::WalletSso => None,
		};
		Self { target, owner }
	}
}

/// Root public key of the identity rooted at `entropy`, used to own raw ledger
/// entries.
fn owner_key(entropy: &[u8]) -> Result<[u8; 32], StatementRenewalError> {
	Ok(derive_root_keypair_from_entropy(entropy)?.public.to_bytes())
}

/// Renewal coordination state owned by [`SigningHost`].
#[derive(Default)]
pub struct RenewalState {
	/// Serializes slot registrations between the renewal pass and on-demand
	/// allocation so both cannot race for the same free slot.
	registration_lock: Mutex<()>,
	/// Serializes read-modify-write cycles on the ledger so a concurrent
	/// allocation cannot drop another's entry.
	ledger_lock: Mutex<()>,
	#[cfg(not(target_arch = "wasm32"))]
	driver: driver::DriverState,
}

impl RenewalState {
	/// Serialize registration scans with competing issuers.
	pub fn registration_lock(&self) -> &Mutex<()> {
		&self.registration_lock
	}

	fn ledger_lock(&self) -> &Mutex<()> {
		&self.ledger_lock
	}
}

/// The persisted renewal state.
///
/// Encoded as the entry list, followed by `renewed` only when it is non-empty,
/// so a ledger written before products had a renewal history still decodes.
#[derive(Debug, Default, PartialEq, Eq)]
struct Ledger {
	/// Promised targets, in track order.
	entries: Vec<TrackedStatementRenewalTarget>,
	/// The last period each product target held a slot in after a pass.
	renewed: Vec<(ProductId, u32)>,
}

impl Ledger {
	fn to_bytes(&self) -> Vec<u8> {
		let mut blob = self.entries.encode();
		if !self.renewed.is_empty() {
			self.renewed.encode_to(&mut blob);
		}
		blob
	}

	fn from_bytes(blob: &[u8]) -> Result<Self, StatementRenewalError> {
		let mut input = blob;
		let entries = Vec::<TrackedStatementRenewalTarget>::decode(&mut input)?;
		let renewed = if input.is_empty() {
			Vec::new()
		} else {
			Vec::<(ProductId, u32)>::decode(&mut input)?
		};
		if !input.is_empty() {
			return Err(StatementRenewalError::TrailingLedgerBytes);
		}
		Ok(Self { entries, renewed })
	}
}

/// Discard an unreadable ledger; later allocations rebuild it.
async fn read_ledger(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Ledger, StatementRenewalError> {
	let Some(blob) = storage
		.read_core_storage(CoreStorageKey::StatementRenewalTargets)
		.await
		.map_err(StatementRenewalError::ReadLedger)?
	else {
		return Ok(Ledger::default());
	};
	match Ledger::from_bytes(&blob) {
		Ok(ledger) => Ok(ledger),
		Err(reason) => {
			warn!(%reason, "discarding an undecodable renewal ledger");
			Ok(Ledger::default())
		},
	}
}

/// Append `new_targets` to the ledger, preserving order and skipping entries
/// already present.
async fn track_targets(
	storage: &(impl CoreStorage + ?Sized),
	ledger_lock: &Mutex<()>,
	owner: [u8; 32],
	new_targets: Vec<StatementRenewalTarget>,
) -> Result<(), StatementRenewalError> {
	let _guard = ledger_lock.lock().await;
	let mut ledger = read_ledger(storage).await?;
	let mut changed = false;
	for target in new_targets {
		let entry = TrackedStatementRenewalTarget::new(target, owner);
		if !ledger.entries.contains(&entry) {
			ledger.entries.push(entry);
			changed = true;
		}
	}
	if !changed {
		return Ok(());
	}
	write_ledger(storage, &ledger).await
}

async fn write_ledger(
	storage: &(impl CoreStorage + ?Sized),
	ledger: &Ledger,
) -> Result<(), StatementRenewalError> {
	storage
		.write_core_storage(CoreStorageKey::StatementRenewalTargets, ledger.to_bytes())
		.await
		.map_err(StatementRenewalError::WriteLedger)
}

/// Record `targets` in the ledger under the active identity.
pub async fn track(
	signing_host: &SigningHost,
	targets: Vec<StatementRenewalTarget>,
) -> Result<(), StatementRenewalError> {
	let entropy = signing_host.root_entropy()?;
	track_targets(
		signing_host.platform.as_ref(),
		signing_host.renewal.ledger_lock(),
		owner_key(&entropy)?,
		targets,
	)
	.await
}
