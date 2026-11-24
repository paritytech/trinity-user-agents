//! Call arguments for allowance extrinsics, which the core's
//! `ExtrinsicService` assembles. They encode by name against metadata, so a
//! renamed or reshaped call fails loudly instead of encoding a wrong one.

use subxt::ext::scale_encode::EncodeAsType;

/// `Resources.set_statement_store_account` arguments.
#[derive(Clone, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct SetStatementStoreAccountCallArgs {
	/// Statement-store period.
	pub period: u32,
	/// Slot within the period.
	pub seq: u32,
	/// Account the allowance is for.
	pub target_account: [u8; 32],
}

/// `Resources.claim_long_term_storage` arguments.
#[derive(Clone, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct ClaimLongTermStorageCallArgs {
	/// Long-term-storage period.
	pub period: u32,
	/// Claim counter within the period.
	pub counter: u8,
	/// Account the storage is for.
	pub account_id: [u8; 32],
}

/// `Pgas.claim_pgas` arguments.
#[derive(Clone, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct ClaimPgasCallArgs {
	/// PGAS slot of the day.
	pub slot_index: u32,
	/// Account credited.
	pub target: [u8; 32],
}

#[cfg(test)]
mod tests;
