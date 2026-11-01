//! Product-scoped identifiers for funding sessions.

use parity_scale_codec::Encode;

use crate::platform::ProductContext;

/// The session id the core keeps for funding id `id` that `product` chose,
/// hashed with the product so no product can name another's session.
pub fn host_funding_intent(product: &ProductContext, id: [u8; 32]) -> String {
	let hash = sp_crypto_hashing::blake2_256(
		&(b"truapi/funding-id", product.product_id.as_str(), id).encode(),
	);
	format!("fs_{}", hex::encode(hash))
}
