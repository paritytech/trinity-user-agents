use super::*;

fn secret() -> [u8; 32] {
	let mut secret = [0u8; 32];
	for (i, byte) in secret.iter_mut().enumerate() {
		*byte = i as u8;
	}
	secret
}

#[test]
fn product_entropy_cases() {
	struct SuccessCase {
		name: &'static str,
		product_id: &'static str,
		key: Vec<u8>,
		expected_hex: &'static str,
	}

	let success_cases = vec![
		SuccessCase {
			name: "single byte key",
			product_id: "myapp.dot",
			key: vec![1],
			expected_hex: "4bafd6a34182959bad8914dcff88c6b6842d551d6f0067afbd407e9584223404",
		},
		SuccessCase {
			name: "text key",
			product_id: "myapp.dot",
			key: b"product-key".to_vec(),
			expected_hex: "ab1887248c9de3cf4b8c5a255782796d3d35a98c8eb2d7df61a410db8b14da36",
		},
		SuccessCase {
			name: "localhost product",
			product_id: "localhost:3000",
			key: (0..32).map(|i| 255 - i).collect(),
			expected_hex: "437d0a6236c51fe114cf6a16b79c9c2b5f95b1e105e2d5269cc254a8c593925f",
		},
	];

	for case in success_cases {
		let entropy =
			derive_product_entropy(&secret(), &ProductId::for_test(case.product_id), &case.key)
				.unwrap();
		assert_eq!(hex::encode(entropy), case.expected_hex, "{}", case.name);
	}

	// Byte-for-byte vectors from polkadot-app-ios-v2
	// (ProductRootEntropyDeriverTests): raw BIP-39 entropy of 16 * 0xAB.
	let ios_entropy = [0xABu8; 16];
	let ios_cases = [
		(
			"test.product.dot",
			b"my-key".as_slice(),
			"479d5b9ecce19615397c9f160ee95e2f00c579837a5afb111132dd0da5fd472a",
		),
		(
			"test.product.dot",
			b"other-key".as_slice(),
			"0d576d5d77cb179bf94b85cb1d644b7879315e74d9e69791fb9cbe94df3c7c39",
		),
		(
			"other.product.dot",
			b"my-key".as_slice(),
			"e2f25271c106593c2977d5965f52fa1d2227da0fc110d682c8cb8f30b2ba21c8",
		),
	];
	for (product_id, key, expected_hex) in ios_cases {
		let entropy =
			derive_product_entropy(&ios_entropy, &ProductId::for_test(product_id), key).unwrap();
		assert_eq!(hex::encode(entropy), expected_hex, "ios vector {product_id}");
	}

	let error_cases = vec![
		(Vec::new(), ProductEntropyError::InvalidKeyLength(0)),
		(vec![0u8; 33], ProductEntropyError::InvalidKeyLength(33)),
	];
	for (key, expected) in error_cases {
		assert_eq!(
			derive_product_entropy(&secret(), &ProductId::for_test("myapp.dot"), &key).unwrap_err(),
			expected,
		);
	}
}
