//! Polkadot mainnet's values, pinned per build, and the check that rejects
//! them on a disposable-identity network.
//!
//! Every route is host-required: session identity (dotNS usernames) resolves
//! through Asset Hub, SSO through People, preimages through Bulletin.

use anyhow::{Result, ensure};

use super::{ChainEndpoint, NetworkConfig};
use crate::{chain::ChainBackend, stack_descriptor::Roles};

/// Polkadot's product namespace.
pub const ID: &str = "polkadot";

const TLD: &str = "dot";

/// A Polkadot chain pinned in this build.
struct PinnedChain {
	genesis: [u8; 32],
	ws: &'static str,
}

const ASSET_HUB: PinnedChain = PinnedChain {
	genesis: hex_literal_genesis(
		"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f",
	),
	ws: "wss://polkadot-asset-hub-rpc.polkadot.io",
};

const PEOPLE: PinnedChain = PinnedChain {
	genesis: hex_literal_genesis(
		"67fa177a097bfa18f77ea95ab56e9bcdfeb0e5b8a40e46298bb93e16b6fc5008",
	),
	ws: "wss://polkadot-people-rpc.polkadot.io",
};

const BULLETIN: PinnedChain = PinnedChain {
	genesis: hex_literal_genesis(
		"2761c95259d59e55ae3daf756c1413b46e45a5a2987299f8ef8e5d8e4776cbc4",
	),
	ws: "wss://bulletin-rpc.polkadot.io",
};

const CHAINS: Roles<PinnedChain> =
	Roles { people: PEOPLE, asset_hub: ASSET_HUB, bulletin: BULLETIN };

/// The dotNS deployment on Polkadot Asset Hub, copied verbatim from
/// `deployments/polkadot/420420419.json` in paritytech/dotns at
/// `c3b291d287a28874f14f906ed61fb43162fc7caa` (paritytech/dotns#340). Replace
/// the file rather than editing it.
const DOTNS_DEPLOYMENT: &str = include_str!("../../deployments/polkadot-420420419.json");

/// `DotnsPopController`, read out of [`DOTNS_DEPLOYMENT`] at compile time.
/// Polkadot carries it because its Asset Hub can leave
/// `DotnsGateway.DispatcherAddress` unset, and discovery then finds nothing.
const DOTNS_POP_CONTROLLER: [u8; 20] = manifest_address(DOTNS_DEPLOYMENT, "DotnsPopController");

/// Polkadot's pinned values, reached over `chain_backend`.
pub fn config(chain_backend: ChainBackend) -> NetworkConfig {
	NetworkConfig {
		id: ID.to_owned(),
		network_suffix: TLD,
		identity_backend_base: None,
		chains: CHAINS
			.map(|chain| ChainEndpoint { genesis: chain.genesis, ws: chain.ws.to_owned() }),
		dotns_pop_controller: Some(DOTNS_POP_CONTROLLER),
		disposable_identities: false,
		chain_backend,
	}
}

/// Fails for a disposable-identity network that carries Polkadot's genesis
/// hashes, its `dot` TLD or its product namespace. A fork of mainnet has
/// mainnet's genesis, and a signature made against it is valid on mainnet
/// itself; mainnet's namespace contains its real-identity sessions.
pub fn ensure_no_real_identity_values(config: &NetworkConfig) -> Result<()> {
	if !config.disposable_identities {
		return Ok(());
	}
	ensure!(
		config.id != ID,
		"a stack cannot run under the {ID} namespace, whose identities are real; use a stack \
		 with another id"
	);
	ensure!(
		config.network_suffix != TLD,
		"{} uses Polkadot's TLD .{}, whose identities are real; point it at a test network",
		config.id,
		config.network_suffix
	);
	for endpoint in config.chains.iter() {
		ensure!(
			!CHAINS.iter().any(|chain| chain.genesis == endpoint.genesis),
			"{} would route to {ID}'s chain at {}, whose identities are real; point it at a test \
			 network",
			config.id,
			endpoint.ws
		);
	}
	Ok(())
}

/// Decode a 64-char hex genesis at compile time.
const fn hex_literal_genesis(hex: &str) -> [u8; 32] {
	let bytes = hex.as_bytes();
	let mut out = [0u8; 32];
	let mut i = 0;
	while i < 32 {
		out[i] = hex_nibble(bytes[i * 2]) << 4 | hex_nibble(bytes[i * 2 + 1]);
		i += 1;
	}
	out
}

/// The H160 a dotNS deployment manifest records under `name`, decoded at
/// compile time from its `"<name>":"0x<40 hex>"` entry. Accepts the EIP-55
/// mixed case the manifests use, and fails the build when the entry is missing.
const fn manifest_address(manifest: &str, name: &str) -> [u8; 20] {
	let json = manifest.as_bytes();
	let name = name.as_bytes();
	let mut start = 0;
	while start + name.len() + 46 <= json.len() {
		let mut matches = json[start] == b'"';
		let mut i = 0;
		while matches && i < name.len() {
			matches = json[start + 1 + i] == name[i];
			i += 1;
		}
		let value = start + 1 + name.len();
		if matches &&
			json[value] == b'"' &&
			json[value + 1] == b':' &&
			json[value + 2] == b'"' &&
			json[value + 3] == b'0' &&
			json[value + 4] == b'x'
		{
			let hex = value + 5;
			let mut out = [0u8; 20];
			let mut byte = 0;
			while byte < 20 {
				out[byte] =
					hex_nibble(json[hex + byte * 2]) << 4 | hex_nibble(json[hex + byte * 2 + 1]);
				byte += 1;
			}
			return out;
		}
		start += 1;
	}
	panic!("the deployment manifest has no entry with that name");
}

const fn hex_nibble(c: u8) -> u8 {
	match c {
		b'0'..=b'9' => c - b'0',
		b'a'..=b'f' => c - b'a' + 10,
		b'A'..=b'F' => c - b'A' + 10,
		_ => panic!("invalid hex digit in a compile-time hex literal"),
	}
}

#[cfg(test)]
mod tests;
