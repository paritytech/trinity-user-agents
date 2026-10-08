//! The ids a host sees for the top-ups and payments a product starts.

use parity_scale_codec::Encode;

use crate::platform::ProductContext;

/// The id the host sees for a top-up or payment id `product` chose. Hashing in
/// the product means no product can name another's.
pub fn host_payment_id(product: &ProductContext, id: [u8; 32]) -> [u8; 32] {
    sp_crypto_hashing::blake2_256(&(b"truapi/payment-id", product.product_id.as_str(), id).encode())
}
