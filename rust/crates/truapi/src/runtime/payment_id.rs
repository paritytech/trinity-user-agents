//! The ids a host sees for the top-ups, payments and funding sessions a
//! product starts.

use parity_scale_codec::Encode;

use crate::platform::ProductContext;

/// The id the host sees for a top-up or payment id `product` chose. Hashing in
/// the product means no product can name another's.
pub fn host_payment_id(product: &ProductContext, id: [u8; 32]) -> [u8; 32] {
	sp_crypto_hashing::blake2_256(&(b"truapi/payment-id", product.product_id.as_str(), id).encode())
}

/// The session id the core keeps for funding id `id` that `product` chose,
/// hashed with the product for the same reason.
pub fn host_funding_intent(product: &ProductContext, id: [u8; 32]) -> String {
	let hash = sp_crypto_hashing::blake2_256(
		&(b"truapi/funding-id", product.product_id.as_str(), id).encode(),
	);
	format!("fs_{}", hex::encode(hash))
}
