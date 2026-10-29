use crate::{Chain, Error, SpecFile, format};
use blake2b_simd::Params;
use sha2::{Digest, Sha256};

impl SpecFile {
	/// Checks a spec's bytes, as served and before HTTP compression, against its `sha256`.
	pub fn check(&self, bytes: &[u8]) -> Result<(), Error> {
		let expected = format::hex_bytes::<32>("sha256", &self.sha256)?;
		if Sha256::digest(bytes).as_slice() == expected { Ok(()) } else { Err(Error::SpecDigest) }
	}
}

impl Chain {
	/// Checks a light spec's bytes against this chain's `lightSpec` digest and genesis hash.
	pub fn check_light_spec(&self, bytes: &[u8]) -> Result<(), Error> {
		let spec = self
			.light_spec
			.as_ref()
			.ok_or_else(|| Error::MalformedSpec(format!("{} has no lightSpec", self.role)))?;
		spec.check(bytes)?;
		let implied = light_spec_genesis(bytes)?;
		if implied == format::hex_bytes::<32>("genesisHash", &self.genesis_hash)? {
			Ok(())
		} else {
			Err(Error::SpecGenesis(format::to_hex(&implied)))
		}
	}
}

/// The genesis hash a light spec implies: the hash of the genesis header built from the spec's
/// state root, with number 0, a zero parent hash, the empty extrinsics root and an empty digest.
pub fn light_spec_genesis(bytes: &[u8]) -> Result<[u8; 32], Error> {
	let spec: serde_json::Value =
		serde_json::from_slice(bytes).map_err(|error| Error::MalformedSpec(error.to_string()))?;
	let state_root = spec
		.pointer("/genesis/stateRootHash")
		.and_then(|root| root.as_str())
		.ok_or_else(|| Error::MalformedSpec("no genesis.stateRootHash".into()))?;
	let state_root = format::hex_bytes::<32>("genesis.stateRootHash", state_root)?;
	let mut header = Vec::with_capacity(98);
	header.extend_from_slice(&[0; 32]);
	header.push(0); // SCALE compact block number 0
	header.extend_from_slice(&state_root);
	header.extend_from_slice(&blake2_256(&[0])); // root of the empty trie
	header.push(0); // SCALE compact length of an empty digest
	Ok(blake2_256(&header))
}

fn blake2_256(data: &[u8]) -> [u8; 32] {
	Params::new()
		.hash_length(32)
		.hash(data)
		.as_bytes()
		.try_into()
		.expect("32-byte hash")
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A real light spec smoldot synced from, frozen here so a testnet wipe cannot break the test,
	/// with the genesis hash its chain reported.
	const PASEO_NEXT_V2_PEOPLE: &str =
		include_str!("../tests/fixtures/paseo-next-v2-people.smol.json");
	const PASEO_NEXT_V2_PEOPLE_GENESIS: &str =
		"0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48";

	#[test]
	fn a_light_spec_implies_its_chains_genesis_hash() {
		let implied = light_spec_genesis(PASEO_NEXT_V2_PEOPLE.as_bytes()).unwrap();
		assert_eq!(format::to_hex(&implied), PASEO_NEXT_V2_PEOPLE_GENESIS);
	}
}
