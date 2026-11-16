//! Where a withdrawal through the native route pays out: the Asset Hub
//! account the user names in the host's address sheet, kept per session in
//! core storage beside the session, whose own stored record does not carry
//! it.

use truapi::latest::FundingDirection;

use crate::{
	host_logic::{funding::FundingSessionError, product_account::public_key_from_address},
	platform::CoreStorageKey,
	runtime::RuntimeServices,
};

impl RuntimeServices {
	/// Record `address`, an SS58 Asset Hub account, as where withdrawal
	/// session `intent` pays out through the native route. Refused with
	/// `InvalidDestination` for an address that does not parse, or a session
	/// the core does not hold or that is not a withdrawal.
	pub async fn set_native_funding_destination(
		&self,
		intent: &str,
		address: &str,
	) -> Result<(), FundingSessionError> {
		let invalid = |reason: &str| FundingSessionError::InvalidDestination { reason: reason.into() };
		let session = self.funding().get(intent).ok_or_else(|| invalid("no such session"))?;
		if session.direction != FundingDirection::Out {
			return Err(invalid("only a withdrawal pays out"));
		}
		let account =
			public_key_from_address(address.trim()).ok_or_else(|| invalid("not an SS58 address"))?;
		self.platform
			.write_core_storage(destination_key(intent), account.to_vec())
			.await
			.map_err(|error| FundingSessionError::Storage { reason: error.reason })
	}

	/// The Asset Hub account withdrawal session `intent` pays out to through
	/// the native route, once the user named one.
	pub async fn native_funding_destination(
		&self,
		intent: &str,
	) -> Result<Option<[u8; 32]>, FundingSessionError> {
		let stored = self
			.platform
			.read_core_storage(destination_key(intent))
			.await
			.map_err(|error| FundingSessionError::Storage { reason: error.reason })?;
		Ok(stored.and_then(|bytes| bytes.try_into().ok()))
	}
}

fn destination_key(intent: &str) -> CoreStorageKey {
	CoreStorageKey::NativeFundingDestination { intent: intent.to_string() }
}
