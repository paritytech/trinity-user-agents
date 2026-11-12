//! PROTOTYPE: the host's field key bindings, signed as a product account
//! (see [`crate::host_logic::field_key_binding`]).

use truapi::latest::{DerivationIndex, ProductAccountId};

use super::WalletAccountHolder;
use crate::{
	host_logic::field_key_binding::{FIELD_KEY_BINDING_LEN, sign_field_key_binding},
	platform::ProductId,
	runtime::authority::{AuthorityError, AuthoritySession},
};

impl WalletAccountHolder {
	/// Sign `binding` for `product_id` as the product's account at
	/// `Raw(index)`, under the field key binding context. Answers the account
	/// and the signature.
	///
	/// Host only: no product call reaches it, so the host decides which key it
	/// binds. Rejects without the current session.
	pub fn sign_field_key_binding(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		index: [u8; 32],
		binding: &[u8; FIELD_KEY_BINDING_LEN],
	) -> Result<([u8; 32], [u8; 64]), AuthorityError> {
		self.with_keys(session, |keys| {
			let keypair = keys.product_keypair(&ProductAccountId {
				dot_ns_identifier: product_id.clone(),
				derivation_index: DerivationIndex::Raw(index),
			})?;
			Ok((keypair.public.to_bytes(), sign_field_key_binding(&keypair, product_id, binding)))
		})
	}
}
