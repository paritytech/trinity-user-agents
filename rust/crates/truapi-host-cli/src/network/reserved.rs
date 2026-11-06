//! The reserved values, Polkadot's genesis hashes and its `dot` TLD, and the
//! check that rejects them where they do not belong.

use anyhow::{Result, ensure};
use clap::ValueEnum;

use super::{Network, NetworkConfig};

/// The genesis hashes of Polkadot's People, Asset Hub and Bulletin chains.
const POLKADOT_GENESIS: [&str; 3] = [
	"67fa177a097bfa18f77ea95ab56e9bcdfeb0e5b8a40e46298bb93e16b6fc5008",
	"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f",
	"2761c95259d59e55ae3daf756c1413b46e45a5a2987299f8ef8e5d8e4776cbc4",
];

const POLKADOT_TLD: &str = "dot";

/// The namespace of `--network polkadot`, which earlier releases stored
/// real-identity sessions under.
const RETIRED_POLKADOT_NAMESPACE: &str = "polkadot";

/// Fails for any network that routes to one of Polkadot's chains, and for a
/// disposable-identity network that uses the `dot` TLD, the namespace of a
/// built-in network whose identities are real, or the retired `polkadot`
/// namespace. A fork of mainnet has mainnet's genesis, and a signature made
/// against it is valid on mainnet itself; a `dot` identity derives the same
/// keys as on mainnet; a real-identity namespace contains real-identity
/// sessions.
pub fn ensure_no_reserved_values(config: &NetworkConfig) -> Result<()> {
	for endpoint in config.chains.iter() {
		ensure!(
			!POLKADOT_GENESIS.contains(&hex::encode(endpoint.genesis).as_str()),
			"{} would route to Polkadot's chain at {}, whose identities are real; point it at a \
			 test network",
			config.id,
			endpoint.ws
		);
	}
	if !config.disposable_identities {
		return Ok(());
	}
	let is_real_identity_namespace = config.id == RETIRED_POLKADOT_NAMESPACE ||
		Network::value_variants()
			.iter()
			.any(|network| !network.has_disposable_identities() && network.id() == config.id);
	ensure!(
		!is_real_identity_namespace,
		"a stack cannot run under the {} namespace, whose identities are real; use a stack with \
		 another id",
		config.id
	);
	ensure!(
		config.network_suffix != POLKADOT_TLD,
		"{} uses Polkadot's TLD .{}, whose identities are real; point it at a test network",
		config.id,
		config.network_suffix
	);
	Ok(())
}

#[cfg(test)]
mod tests;
