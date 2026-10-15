//! Wallet keys, account operations and resource issuance for the signing host.

mod account;
pub mod allowance;
pub mod allowance_renewal;

use crate::{
	host_internal::sso_messages::RingVrfError,
	host_logic::product_account::derive_ring_vrf_entropy,
	runtime::authority::{AuthorityError, AuthoritySession},
};
use truapi::v01;
use zeroize::Zeroizing;

use super::SigningHost;

impl SigningHost {
	/// Current root entropy, or [`AuthorityError::Disconnected`] when no local
	/// session is active.
	pub fn root_entropy(&self) -> Result<Zeroizing<Vec<u8>>, AuthorityError> {
		self.root_entropy
			.lock()
			.expect("signing host entropy mutex poisoned")
			.clone()
			.ok_or(AuthorityError::Disconnected)
	}

	fn ring_vrf_entropy(
		&self,
		session: &AuthoritySession,
		handle: &v01::ProductAccountId,
	) -> Result<Zeroizing<[u8; 32]>, RingVrfError> {
		self.require_current_session(session)?;
		let root = self.root_entropy()?;
		derive_ring_vrf_entropy(&root, &handle.dot_ns_identifier, &handle.derivation_index)
			.map(Zeroizing::new)
			.map_err(|err| RingVrfError::Unknown { reason: err.to_string() })
	}
}
