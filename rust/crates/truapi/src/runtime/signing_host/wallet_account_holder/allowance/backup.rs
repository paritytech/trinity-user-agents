//! The host's own backup accounts.
//!
//! A web host keeps a backup of one wallet's local state on the network: the
//! encrypted snapshot on Bulletin, and a small pointer to it in the Statement
//! Store. Two accounts serve it, both hard-derived from the wallet root under
//! the reserved `host-backup` junction, which no product subtree and no
//! product allowance account uses:
//!
//! - the statement account, which holds a general Statement Store slot of the wallet's personhood
//!   and signs only backup pointer statements ([`WalletAccountHolder::sign_backup_pointer`]);
//! - the Bulletin account, which holds the long-term storage authorization the snapshot is stored
//!   under ([`WalletAccountHolder::allocate_backup_bulletin`]).
//!
//! Neither secret leaves the core, and neither account signs anything else.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use parity_scale_codec::Decode;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use super::{
	AllowanceAllocationError, BulletinAllowanceAccount, StatementAccountClaim, WalletAccountHolder,
};
use crate::{
	host_internal::sso_messages::OnExistingAllowancePolicy,
	host_logic::{
		product_account::{SR25519_SIGNING_CONTEXT, derive_sr25519_hard_path},
		statement_store::{StatementField, validate_unsigned_statement_signing_payload},
	},
	runtime::{
		allowances::current_unix_secs,
		authority::{AuthorityError, AuthoritySession, BulletinAllowanceKey},
		bulletin_rpc::BulletinSubmitError,
		statement_allowance::ClaimSubmission,
	},
};

/// Junctions from the wallet root to the backup statement account.
const STATEMENT_ACCOUNT: [&str; 2] = ["host-backup", "statement"];
/// Junctions from the wallet root to the backup Bulletin allowance account.
const BULLETIN_ACCOUNT: [&str; 2] = ["host-backup", "bulletin"];

/// The bytes every backup pointer's data starts with. The statement account
/// signs no statement whose data does not.
const BACKUP_POINTER_MAGIC: &[u8] = b"twhb/ptr/1";
/// A sealed pointer's nonce and authentication tag.
const SEALED_OVERHEAD: usize = 24 + 16;
/// The largest pointer data the statement account signs.
const MAX_POINTER_BYTES: usize = 1024;
/// How far ahead a pointer's expiry may lapse: the host gives it fourteen
/// days, the time Bulletin keeps the snapshot it names, and a day of slack.
const MAX_POINTER_LIFETIME: Duration = Duration::from_secs(15 * 86_400);

/// Failure of a backup account operation.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
	/// The session, a key derivation, or an allowance claim failed.
	#[error("{0}")]
	Allowance(#[from] AllowanceAllocationError),
	/// The statement to sign is not a backup pointer.
	#[error("not a backup pointer: {0}")]
	NotAPointer(&'static str),
	/// Bulletin did not store the snapshot.
	#[error("{0}")]
	Store(#[from] BulletinSubmitError),
}

impl From<AuthorityError> for BackupError {
	fn from(error: AuthorityError) -> Self {
		Self::Allowance(error.into())
	}
}

impl WalletAccountHolder {
	/// The backup statement account's public key.
	pub fn backup_account(
		&self,
		session: &AuthoritySession,
	) -> Result<[u8; 32], AllowanceAllocationError> {
		self.with_keys(session, |keys| {
			Ok(derive_sr25519_hard_path(&keys.entropy, &STATEMENT_ACCOUNT)?.public.to_bytes())
		})
	}

	/// Give the backup statement account a general Statement Store slot for the
	/// current period, reusing one it already holds, or only validate the claim.
	pub async fn claim_backup_statement_account(
		&self,
		session: &AuthoritySession,
		submission: ClaimSubmission,
	) -> Result<StatementAccountClaim, AllowanceAllocationError> {
		let account = self.backup_account(session)?;
		self.claim_statement_account_for(session, account, "host backup", submission)
			.await
	}

	/// Sign `payload`, the unsigned signing payload of a backup pointer
	/// statement, as the backup statement account.
	///
	/// Refuses a payload that is not a short-lived pointer statement, so the
	/// account cannot be made to sign anything else.
	pub fn sign_backup_pointer(
		&self,
		session: &AuthoritySession,
		payload: &[u8],
	) -> Result<[u8; 64], BackupError> {
		require_pointer(payload, current_unix_secs()?)?;
		self.with_keys::<_, BackupError>(session, |keys| {
			let keypair = derive_sr25519_hard_path(&keys.entropy, &STATEMENT_ACCOUNT)
				.map_err(AllowanceAllocationError::from)?;
			Ok(keypair
				.secret
				.sign_simple(SR25519_SIGNING_CONTEXT, payload, &keypair.public)
				.to_bytes())
		})
	}

	/// The backup Bulletin account's allowance key, claiming long-term storage
	/// first when its authorization cannot hold `size` bytes.
	pub async fn allocate_backup_bulletin(
		&self,
		session: &AuthoritySession,
		size: u64,
	) -> Result<BulletinAllowanceKey, BackupError> {
		let secret = self
			.allocate_bulletin_allowance_at(
				session,
				BulletinAllowanceAccount {
					junctions: &BULLETIN_ACCOUNT,
					label: "host backup",
					size,
				},
				OnExistingAllowancePolicy::Ignore,
			)
			.await?;
		Ok(BulletinAllowanceKey::from_secret_bytes(secret)?)
	}
}

/// Refuse `payload` unless it is the unsigned signing payload of a pointer:
/// exactly an expiry, a channel, one topic and data, in canonical order; the
/// data a sealed pointer of at most [`MAX_POINTER_BYTES`]; and the expiry
/// still ahead, by at most [`MAX_POINTER_LIFETIME`].
fn require_pointer(payload: &[u8], now_secs: u64) -> Result<(), BackupError> {
	validate_unsigned_statement_signing_payload(payload)
		.map_err(|_| BackupError::NotAPointer("not a canonical unsigned statement"))?;
	let mut input = payload;
	let mut fields = Vec::new();
	while !input.is_empty() {
		fields.push(
			StatementField::decode(&mut input)
				.map_err(|_| BackupError::NotAPointer("not a canonical unsigned statement"))?,
		);
	}
	let [
		StatementField::Expiry(expiry),
		StatementField::Channel(_),
		StatementField::Topic1(_),
		StatementField::Data(data),
	] = fields.as_slice()
	else {
		return Err(BackupError::NotAPointer("it is not an expiry, a channel, one topic and data"));
	};
	if !data.starts_with(BACKUP_POINTER_MAGIC) ||
		data.len() < BACKUP_POINTER_MAGIC.len() + SEALED_OVERHEAD ||
		data.len() > MAX_POINTER_BYTES
	{
		return Err(BackupError::NotAPointer("its data is not a sealed pointer"));
	}
	let lapses = expiry >> 32;
	if lapses <= now_secs || lapses > now_secs + MAX_POINTER_LIFETIME.as_secs() {
		return Err(BackupError::NotAPointer("its expiry is past or too far ahead"));
	}
	Ok(())
}
