//! PROTOTYPE for the web host's draft field bridge: a host's signature that a
//! field receiving key belongs to one product account.
//!
//! A host makes a receiving key for a product and signs, as that product's
//! account, a message naming the key, the product, the account and the
//! network. It signs under [`FIELD_KEY_BINDING_CONTEXT`], an sr25519 signing
//! context of its own. The context is part of the signature transcript, and
//! every signature a product can ask for is made under `substrate`, so no
//! product request yields a signature that [`verify_field_key_binding`]
//! accepts. Only a host calls [`sign_field_key_binding`]; no product call
//! reaches it.
//!
//! A valid signature shows that a holder of the wallet's keys bound the key
//! to the account. It does not show that the host is honest or that it keeps
//! the key: a modified host, or anyone with the recovery phrase, can sign any
//! binding. Not part of the TrUAPI protocol.

use schnorrkel::{Keypair, PublicKey, Signature};

use crate::platform::ProductId;

/// The sr25519 signing context of a field key binding. No product signing
/// path signs under it.
pub const FIELD_KEY_BINDING_CONTEXT: &[u8] = b"truapi-web-host/field-key-binding/v1";

/// The first bytes of every signed binding message.
const MESSAGE_TAG: &[u8] = b"web-fields/key-binding/v1";

/// The bound key's own bytes: its 16-byte id, its 32-byte X25519 public key
/// and the 32-byte genesis hash of the network's People chain.
pub const FIELD_KEY_BINDING_LEN: usize = 16 + 32 + 32;

/// The bytes a binding signs: the tag, the product id with its one-byte
/// length, the account and `binding`.
fn binding_message(
	product_id: &ProductId,
	account: &[u8; 32],
	binding: &[u8; FIELD_KEY_BINDING_LEN],
) -> Vec<u8> {
	let product = product_id.as_str().as_bytes();
	let mut message =
		Vec::with_capacity(MESSAGE_TAG.len() + 1 + product.len() + 32 + FIELD_KEY_BINDING_LEN);
	message.extend_from_slice(MESSAGE_TAG);
	// A product id is a dotNS name, at most 253 bytes, so its length fits.
	message.push(u8::try_from(product.len()).unwrap_or(u8::MAX));
	message.extend_from_slice(product);
	message.extend_from_slice(account);
	message.extend_from_slice(binding);
	message
}

/// Sign `binding` for `product_id` as `keypair`, the product account it binds
/// to.
pub fn sign_field_key_binding(
	keypair: &Keypair,
	product_id: &ProductId,
	binding: &[u8; FIELD_KEY_BINDING_LEN],
) -> [u8; 64] {
	let account = keypair.public.to_bytes();
	keypair
		.secret
		.sign_simple(
			FIELD_KEY_BINDING_CONTEXT,
			&binding_message(product_id, &account, binding),
			&keypair.public,
		)
		.to_bytes()
}

/// Whether `signature` is `account`'s binding of `binding` for `product_id`.
/// False for a malformed account or signature.
pub fn verify_field_key_binding(
	product_id: &ProductId,
	account: &[u8; 32],
	binding: &[u8; FIELD_KEY_BINDING_LEN],
	signature: &[u8; 64],
) -> bool {
	let (Ok(public), Ok(signature)) =
		(PublicKey::from_bytes(account), Signature::from_bytes(signature))
	else {
		return false;
	};
	public
		.verify_simple(
			FIELD_KEY_BINDING_CONTEXT,
			&binding_message(product_id, account, binding),
			&signature,
		)
		.is_ok()
}

#[cfg(test)]
mod tests;
