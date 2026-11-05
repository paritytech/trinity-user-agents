//! A stack descriptor read as a [`NetworkConfig`].

use anyhow::{Context, Result, ensure};
use trinity_stack::Descriptor;
use truapi::platform::DOTNS_TLDS;

use super::{ChainEndpoint, NetworkConfig};
use crate::{
	chain::ChainBackend,
	stack_descriptor::{ROLE_NAMES, Roles, StackProjection},
};

/// A `--stack` descriptor's network, reached over `chain_backend`, under the
/// product namespace of the descriptor's `id` without a trailing `-fork`.
pub fn stack_network_config(
	descriptor: &Descriptor,
	chain_backend: ChainBackend,
) -> Result<NetworkConfig> {
	let id = descriptor.id.strip_suffix("-fork").unwrap_or(&descriptor.id);
	ensure!(
		truapi::platform::is_dns_label(id),
		"the stack id {:?} is not a lowercase label to use as the product namespace",
		descriptor.id
	);
	stack_config(descriptor, id.to_owned(), chain_backend)
}

/// A stack's People, Asset Hub and Bulletin chains, identity backend, TLD and
/// dotNS `popController` under the product namespace `id`, with disposable
/// identities, read through its [`StackProjection`]. Fails when a role is
/// missing, when two roles share a genesis hash, and when the TLD is not one
/// the core serves.
pub fn stack_config(
	descriptor: &Descriptor,
	id: String,
	chain_backend: ChainBackend,
) -> Result<NetworkConfig> {
	let projection = StackProjection::of(descriptor);
	let chains = ROLE_NAMES.zip(projection.roles).try_map(|(role, chain)| {
		let (ws, genesis_hash) =
			chain.with_context(|| format!("the {} stack has no {role} chain", descriptor.id))?;
		let mut genesis = [0u8; 32];
		hex::decode_to_slice(genesis_hash.trim_start_matches("0x"), &mut genesis)
			.expect("invalid genesisHash in a validated descriptor");
		anyhow::Ok(ChainEndpoint { genesis, ws: ws.to_owned() })
	})?;
	let network_suffix =
		DOTNS_TLDS.iter().find(|tld| **tld == projection.tld).with_context(|| {
			format!(
				"the {} stack's tld {} is not one the core serves ({})",
				descriptor.id,
				projection.tld,
				DOTNS_TLDS.join(", ")
			)
		})?;
	let identity_backend_base =
		projection.dub.map(|url| format!("{}/api/v1", url.trim_end_matches('/')));
	let dotns_pop_controller = projection.pop_controller.map(|address| {
		let mut bytes = [0u8; 20];
		hex::decode_to_slice(address.trim_start_matches("0x"), &mut bytes)
			.expect("invalid dotns address in a validated descriptor");
		bytes
	});
	ensure_distinct_genesis(&chains)?;
	Ok(NetworkConfig {
		id,
		network_suffix,
		identity_backend_base,
		chains,
		dotns_pop_controller,
		disposable_identities: true,
		chain_backend,
	})
}

/// Fails when two roles share a genesis hash, since the host routes each role
/// by its genesis.
pub fn ensure_distinct_genesis(chains: &Roles<ChainEndpoint>) -> Result<()> {
	let Roles { people, asset_hub, bulletin } = chains;
	ensure!(
		people.genesis != asset_hub.genesis &&
			people.genesis != bulletin.genesis &&
			asset_hub.genesis != bulletin.genesis,
		"People, Asset Hub and Bulletin need distinct genesis hashes: the host routes each role \
		 by its genesis"
	);
	Ok(())
}

#[cfg(test)]
mod tests;
