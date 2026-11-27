//! A browser host has no core database, so it keeps no confirmed allowance
//! and confirms on chain in every activation.

use super::WalletAccountHolder;
use crate::{
	platform::ProductId,
	runtime::{allowances::AllowanceResource, authority::AuthoritySession},
};

impl WalletAccountHolder {
	/// Nothing is confirmed, so the chain is asked.
	pub async fn allowance_is_confirmed(
		&self,
		_session: &AuthoritySession,
		_product_id: &ProductId,
		_resource: AllowanceResource,
		_period: Option<u32>,
	) -> bool {
		false
	}

	/// Confirmations are not kept.
	pub async fn record_confirmed_allowance(
		&self,
		_session: &AuthoritySession,
		_product_id: &ProductId,
		_resource: AllowanceResource,
		_period: Option<u32>,
	) {
	}

	/// Nothing was recorded.
	pub async fn forget_confirmed_allowance(
		&self,
		_product_id: &ProductId,
		_resource: AllowanceResource,
	) {
	}
}
