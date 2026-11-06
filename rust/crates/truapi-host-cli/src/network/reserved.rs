//! The reserved values, Polkadot's genesis hashes and its `dot` TLD, and the
//! checks that reject them where they do not belong.

use anyhow::{Result, ensure};

use super::{Network, NetworkConfig};

/// The genesis hashes of Polkadot's People, Asset Hub and Bulletin chains.
const POLKADOT_GENESIS: [&str; 3] = [
	"67fa177a097bfa18f77ea95ab56e9bcdfeb0e5b8a40e46298bb93e16b6fc5008",
	"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f",
	"2761c95259d59e55ae3daf756c1413b46e45a5a2987299f8ef8e5d8e4776cbc4",
];

const POLKADOT_TLD: &str = "dot";

/// The namespace of `--network polkadot`, which earlier releases stored
/// mainnet sessions under.
const RETIRED_POLKADOT_NAMESPACE: &str = "polkadot";

/// Fails for a network that routes to one of Polkadot's chains or runs under
/// the retired `polkadot` namespace. A fork of mainnet has mainnet's genesis,
/// and a signature made against it is valid on mainnet itself.
pub fn ensure_no_reserved_values(config: &NetworkConfig) -> Result<()> {
	for endpoint in config.chains.iter() {
		ensure!(
			!POLKADOT_GENESIS.contains(&hex::encode(endpoint.genesis).as_str()),
			"{} would route to Polkadot's chain at {}; point it at a test network",
			config.id,
			endpoint.ws
		);
	}
	ensure!(
		config.id != RETIRED_POLKADOT_NAMESPACE,
		"a stack cannot run under the {RETIRED_POLKADOT_NAMESPACE} namespace, which earlier \
		 releases used for Polkadot mainnet; use a stack with another id"
	);
	Ok(())
}

/// Fails for a `--stack` network that uses the `dot` TLD, which only the
/// bundled pcf-devnet copy may use, or that runs under pcf-devnet's
/// namespace, so a stack cannot pose as that built-in network. A `dot`
/// identity derives the same keys as on Polkadot.
pub fn ensure_stack_not_reserved(config: &NetworkConfig) -> Result<()> {
	let pcf_devnet = Network::PcfDevnet.id();
	ensure!(
		config.id != pcf_devnet,
		"a stack cannot run under the {pcf_devnet} namespace of the built-in network; use a \
		 stack with another id"
	);
	ensure!(
		config.network_suffix != POLKADOT_TLD,
		"{} uses Polkadot's TLD .{}, which only the built-in {pcf_devnet} network may use; \
		 point it at a test network",
		config.id,
		config.network_suffix
	);
	Ok(())
}

#[cfg(test)]
mod tests;
