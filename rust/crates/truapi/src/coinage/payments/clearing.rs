//! H4 · Clearing reference: RFC-17's `CoinPaymentClearingReference`, one
//! leaf per claimed coin and a binary Merkle root over the leaves.
//!
//! A leaf is `SCALE((coin account, transaction hash))`. Leaves are sorted by
//! their encoding so every party computes the same root, and the tree is the
//! one Polkadot SDK's `binary-merkle-tree` builds over BLAKE2b-256, so one
//! leaf can be proved to a third party without the others (invariant 3).
//! The reference of no leaves has an all-zero root.

use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use truapi::latest::CoinPaymentClearingReference;

/// One claimed coin and the transaction that moved it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Leaf {
	/// The coin's account.
	pub coin: [u8; 32],
	/// The transaction that moved it.
	pub transaction: [u8; 32],
}

impl Leaf {
	fn encoded(&self) -> Vec<u8> {
		(self.coin, self.transaction).encode()
	}
}

/// What proves one leaf against a root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InclusionProof {
	/// The leaf's position among the sorted leaves.
	pub position: u32,
	/// How many leaves the tree has.
	pub leaf_count: u32,
	/// The sibling node at each row where the leaf's node has one.
	pub siblings: Vec<[u8; 32]>,
}

/// The reference over `leaves`: sorted by their encoding, with their root.
pub fn build(mut leaves: Vec<Leaf>) -> CoinPaymentClearingReference {
	leaves.sort_by_key(Leaf::encoded);
	let root = if leaves.is_empty() {
		[0; 32]
	} else {
		binary_merkle_tree::merkle_root::<Blake2, _>(leaves.iter().map(Leaf::encoded))
	};
	CoinPaymentClearingReference { root, leaves: leaves.iter().map(|leaf| (leaf.coin, leaf.transaction)).collect() }
}

/// The inclusion proof of the leaf for `coin`; none when the reference has
/// no leaf for it.
pub fn prove(reference: &CoinPaymentClearingReference, coin: [u8; 32]) -> Option<InclusionProof> {
	let leaves: Vec<Vec<u8>> = reference.leaves.iter().map(|leaf| leaf.encode()).collect();
	let position = reference.leaves.iter().position(|(leaf_coin, _)| *leaf_coin == coin)?;
	let proof = binary_merkle_tree::merkle_proof::<Blake2, _, _>(leaves, u32::try_from(position).ok()?);
	Some(InclusionProof {
		position: proof.leaf_index,
		leaf_count: proof.number_of_leaves,
		siblings: proof.proof,
	})
}

/// Whether `leaf` and `proof` recompute `root`, as a third party checks it.
/// That the leaf's transaction is finalized and moved the leaf's coin is the
/// third party's own check against the chain (invariant 2).
pub fn verify(root: [u8; 32], leaf: Leaf, proof: &InclusionProof) -> bool {
	binary_merkle_tree::verify_proof::<Blake2, _, _>(
		&root,
		proof.siblings.clone(),
		proof.leaf_count,
		proof.position,
		&leaf.encoded(),
	)
}

/// BLAKE2b-256 as the hasher `binary-merkle-tree` builds the tree with.
struct Blake2;

impl hash_db::Hasher for Blake2 {
	type Out = [u8; 32];
	type StdHasher = std::hash::DefaultHasher;
	const LENGTH: usize = 32;

	fn hash(bytes: &[u8]) -> [u8; 32] {
		blake2_256(bytes)
	}
}

#[cfg(test)]
mod tests;
