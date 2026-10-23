//! Ledger and driver for automatic statement-store allowance renewal.
//!
//! The ledger records which accounts this signing host promised to keep
//! allowed, as derivation recipes where possible so entries stay valid when
//! the host rotates to a new root entropy. The driver resolves them against
//! the active session and runs the chain-pure pass in
//! `statement_allowance::renewal`, either once (`renew_now`) or on a periodic
//! tick (`start_renewal_loop`).
//!
//! Only host-owned wallet/paired-device targets are renewed in the background.
//! Product allowance decisions belong to product connection storage, which may
//! be artifact-scoped and is unavailable here. They are ensured on demand by an
//! authorized product request; old unscoped product ledger entries are pruned.

#[cfg(not(target_arch = "wasm32"))]
mod driver;

#[cfg(not(target_arch = "wasm32"))]
pub use driver::{
	active_owner_key, list, renew_now, start_renewal_loop, track, untrack_account_for_signing_host,
};

use crate::{
	host_logic::product_account::{
		ProductAccountError, derive_identity_keypair, derive_sr25519_hard_path,
	},
	platform::CoreStorageKey,
	runtime::{
		authority::AuthorityError,
		statement_allowance::{StatementAllowanceError, renewal::ResolvedRenewalTarget},
		statement_store_rpc::StatementStoreRpcClientError,
	},
};
use futures::lock::Mutex;
use parity_scale_codec::{Decode, Encode};

use super::{SigningHost, sso_responder::AllowanceAllocationError};
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
	/// Product authorization is artifact-scoped and unavailable to background renewal.
	#[error(
		"Product statement allowances require an authorized product request; background renewal cannot access artifact-scoped permissions"
	)]
	ProductTargetRequiresAuthorizedRequest,
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
		product_id: String,
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

fn decode_entries(
	blob: &[u8],
) -> Result<Vec<TrackedStatementRenewalTarget>, StatementRenewalError> {
	let mut input = blob;
	let entries = Vec::<TrackedStatementRenewalTarget>::decode(&mut input)?;
	if !input.is_empty() {
		return Err(StatementRenewalError::TrailingLedgerBytes);
	}
	Ok(entries)
}

/// Stable labels identify pruned entries even without an active wallet.
fn target_label(target: &StatementRenewalTarget) -> String {
	match target {
		StatementRenewalTarget::ProductStatementAllowance { product_id } => {
			format!("product:{product_id}")
		},
		StatementRenewalTarget::WalletSso => "wallet-sso".to_string(),
		StatementRenewalTarget::Account { label, .. } => label.clone(),
	}
}

fn resolve_target(
	entropy: &[u8],
	network_suffix: &str,
	target: &StatementRenewalTarget,
) -> Result<ResolvedRenewalTarget, String> {
	let label = target_label(target);
	match target {
		StatementRenewalTarget::ProductStatementAllowance { product_id } => {
			let pair = derive_sr25519_hard_path(
				entropy,
				&["allowance", "statement-store", product_id.as_str()],
			)
			.map_err(|err| err.to_string())?;
			Ok(ResolvedRenewalTarget { label, account_id: pair.public.to_bytes() })
		},
		StatementRenewalTarget::WalletSso => {
			let pair =
				derive_identity_keypair(entropy, network_suffix).map_err(|err| err.to_string())?;
			Ok(ResolvedRenewalTarget { label, account_id: pair.public.to_bytes() })
		},
		StatementRenewalTarget::Account { account_id, .. } => {
			Ok(ResolvedRenewalTarget { label, account_id: *account_id })
		},
	}
}

/// Resolve attribution without refreshing, pruning, or rewriting the ledger.
pub async fn inspection_labels(
	signing_host: &SigningHost,
	entropy: &[u8],
	owner: [u8; 32],
) -> Result<crate::runtime::statement_allowance::inspection::AccountLabels, String> {
	let _guard = signing_host.renewal.ledger_lock().lock().await;
	let bytes = signing_host
		.platform
		.read_core_storage(CoreStorageKey::StatementRenewalTargets)
		.await
		.map_err(|err| format!("renewal ledger read failed: {}", err.reason))?;
	let entries = bytes
		.as_deref()
		.map(decode_entries)
		.transpose()
		.map_err(|error| error.to_string())?
		.unwrap_or_default();
	let mut labels = crate::runtime::statement_allowance::inspection::AccountLabels::new();
	for entry in entries {
		if entry.owner.is_some_and(|recorded| recorded != owner) {
			continue;
		}
		let resolved = resolve_target(entropy, &signing_host.network_suffix, &entry.target)?;
		let product = match entry.target {
			StatementRenewalTarget::ProductStatementAllowance { product_id } => Some(product_id),
			_ => None,
		};
		labels.insert(resolved.account_id, (product, resolved.label));
	}
	Ok(labels)
}
