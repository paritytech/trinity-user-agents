use super::*;

fn leaf(coin: u8, transaction: u8) -> Leaf {
	Leaf { coin: [coin; 32], transaction: [transaction; 32] }
}

#[test]
fn the_empty_reference_has_an_all_zero_root() {
	assert_eq!(build(Vec::new()), CoinPaymentClearingReference { root: [0; 32], leaves: Vec::new() });
}

#[test]
fn every_party_computes_the_same_root_whatever_the_order() {
	let forward = build(vec![leaf(1, 4), leaf(2, 5), leaf(3, 6)]);
	let backward = build(vec![leaf(3, 6), leaf(1, 4), leaf(2, 5)]);
	assert_eq!(forward, backward);
	assert_ne!(forward.root, build(vec![leaf(1, 4), leaf(2, 5)]).root);
}

#[test]
fn a_single_leaf_is_its_own_hashed_encoding() {
	assert_eq!(build(vec![leaf(1, 4)]).root, blake2_256(&leaf(1, 4).encoded()));
}

#[test]
fn each_leaf_is_proved_without_the_others() {
	let leaves = vec![leaf(1, 4), leaf(2, 5), leaf(3, 6)];
	let reference = build(leaves.clone());
	for one in leaves {
		let proof = prove(&reference, one.coin).unwrap();
		assert_eq!(proof.leaf_count, 3);
		assert!(verify(reference.root, one, &proof), "the leaf of coin {:?} verifies", one.coin[0]);
		assert!(!verify(reference.root, leaf(one.coin[0], 9), &proof), "another transaction does not");
	}
	assert_eq!(prove(&reference, [8; 32]), None);
}
