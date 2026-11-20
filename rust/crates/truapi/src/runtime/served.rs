//! Proof that the stack serves every product a value names.
//!
//! [`serve`] checks a value against the stack and wraps it in [`Served`]. A
//! handler serves a request that names another product before it acts, and
//! `HostAccounts` and the signing requests the core builds take their products
//! as `Served`, so no key is resolved for a product the stack does not serve.

use truapi::latest::{
	HostAccountCreateProofRequest, HostAccountGetAliasRequest, HostAccountListRingVrfKeysRequest,
	HostAccountRingVrfSignRequest, HostAccountSignVrfRequest, HostSignPayloadRequest,
	HostSignRawRequest, NftPurseOwner, ProductAccountId, ProductAccountTxPayload, RemotePermission,
	RemotePermissionRequest,
};

use crate::{
	ProductId,
	stack::{Stack, UnservedProduct},
};

/// A value that names only products the stack serves.
///
/// Only [`serve`] creates one, so a function that takes `Served` cannot be
/// called without the check. It derefs to the value. It does not record
/// which stack checked it, so create and use it within one runtime.
#[derive(Clone)]
pub struct Served<T>(T);

impl<T> Served<T> {
	/// Unwraps the value for a callee that does not need the proof.
	pub fn into_inner(self) -> T {
		self.0
	}
}

impl<T> core::ops::Deref for Served<T> {
	type Target = T;

	fn deref(&self) -> &T {
		&self.0
	}
}

impl Served<ProductAccountTxPayload> {
	/// Replaces the call data. Call data names no product, so the payload
	/// stays served.
	pub fn with_call_data(mut self, call_data: Vec<u8>) -> Self {
		self.0.call_data = call_data;
		self
	}
}

/// A value that names products. The core acts on it only if the stack serves
/// all of them.
pub trait NamesProducts {
	/// Every product the value names.
	fn products(&self) -> Vec<&ProductId>;
}

/// `value` as [`Served`] once `stack` serves every product it names. Fails
/// with the first one it does not.
pub fn serve<T: NamesProducts>(stack: &Stack, value: T) -> Result<Served<T>, UnservedProduct> {
	for product_id in value.products() {
		stack.require_served(product_id)?;
	}
	Ok(Served(value))
}

impl NamesProducts for ProductId {
	fn products(&self) -> Vec<&ProductId> {
		vec![self]
	}
}

impl NamesProducts for ProductAccountId {
	fn products(&self) -> Vec<&ProductId> {
		vec![&self.dot_ns_identifier]
	}
}

impl NamesProducts for NftPurseOwner {
	fn products(&self) -> Vec<&ProductId> {
		match self {
			Self::Product(product_id) => vec![product_id],
			Self::Root => vec![],
		}
	}
}

impl NamesProducts for RemotePermissionRequest {
	fn products(&self) -> Vec<&ProductId> {
		match &self.permission {
			RemotePermission::NftPurseRead { owner } => owner.products(),
			RemotePermission::Remote { .. } |
			RemotePermission::WebRtc |
			RemotePermission::ChainSubmit |
			RemotePermission::PreimageSubmit |
			RemotePermission::StatementSubmit |
			RemotePermission::BalanceAccess => vec![],
		}
	}
}

impl NamesProducts for HostSignPayloadRequest {
	fn products(&self) -> Vec<&ProductId> {
		self.account.products()
	}
}

impl NamesProducts for HostSignRawRequest {
	fn products(&self) -> Vec<&ProductId> {
		self.account.products()
	}
}

impl NamesProducts for ProductAccountTxPayload {
	fn products(&self) -> Vec<&ProductId> {
		self.signer.products()
	}
}

impl NamesProducts for HostAccountSignVrfRequest {
	fn products(&self) -> Vec<&ProductId> {
		self.account.products()
	}
}

impl NamesProducts for HostAccountGetAliasRequest {
	fn products(&self) -> Vec<&ProductId> {
		vec![&self.key_handle.dot_ns_identifier, &self.context.product_id]
	}
}

impl NamesProducts for HostAccountCreateProofRequest {
	fn products(&self) -> Vec<&ProductId> {
		vec![&self.key_handle.dot_ns_identifier, &self.context.product_id]
	}
}

impl NamesProducts for HostAccountListRingVrfKeysRequest {
	fn products(&self) -> Vec<&ProductId> {
		vec![&self.owner]
	}
}

impl NamesProducts for HostAccountRingVrfSignRequest {
	fn products(&self) -> Vec<&ProductId> {
		self.key_handle.products()
	}
}

#[cfg(test)]
mod tests;
