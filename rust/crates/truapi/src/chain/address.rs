//! Account addresses as seen by EVM contracts.

use sp_crypto_hashing::keccak_256;

/// Maps an AccountId32 to the H160 the contracts see.
///
/// Mirrors `pallet_revive::AccountId32Mapper::to_address`. Eth-derived
/// accounts — trailing 12 bytes all `0xEE` — truncate. Everything else hashes.
pub fn account_to_h160(public_key: &[u8; 32]) -> [u8; 20] {
	if public_key[20..].iter().all(|byte| *byte == 0xEE) {
		public_key[..20]
			.try_into()
			.expect("slice of first 20 bytes from a 32-byte array; qed")
	} else {
		keccak_256(public_key)[12..]
			.try_into()
			.expect("keccak_256 output is 32 bytes, tail slice is 20; qed")
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn account_to_h160_follows_the_revive_mapper() {
		let mut eth_derived = [0x0Fu8; 32];
		eth_derived[20..].fill(0xEE);
		assert_eq!(account_to_h160(&eth_derived), [0x0F; 20]);

		let hashed = account_to_h160(&[0x11; 32]);
		assert_eq!(hashed, keccak_256(&[0x11; 32])[12..]);
	}
}
