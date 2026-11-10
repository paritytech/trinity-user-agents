//! The checks that make a descriptor a [`Stack`] the CLI can run on.

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result, ensure};
use trinity_stack::{Descriptor, Pointer};
use truapi::platform::DOTNS_TLDS;

use super::{ChainEndpoint, POLKADOT_NAMESPACE, Stack, namespace_of};
use crate::{
	chain::{ChainBackend, StackLightClient},
	stack_descriptor::{ROLE_NAMES, Roles},
};

impl Stack {
	/// The stack `pointer` names, read from `descriptor` and connected to over
	/// `chain_backend`. Fails when People,
	/// Asset Hub or Bulletin is missing, when two of them share a genesis hash,
	/// when the TLD is not one the core serves, when the namespace is not a
	/// lowercase label or is the `polkadot` one, when the stack has
	/// no `dub` service or no `dotns.popController`, and on the light client
	/// as [`light_client`] does.
	pub fn new(
		pointer: &Pointer,
		descriptor: Descriptor,
		chain_backend: ChainBackend,
	) -> Result<Self> {
		let id = &descriptor.id;
		ensure!(
			id != POLKADOT_NAMESPACE,
			"a stack cannot run under the {POLKADOT_NAMESPACE} namespace, which may \
			 contain sessions of real Polkadot identities; use a stack with another id"
		);
		ensure!(
			truapi::platform::is_dns_label(namespace_of(id)),
			"the stack id {id:?} is not a lowercase label to use as the product namespace"
		);
		let chains = ROLE_NAMES.try_map(|role| {
			let chain = descriptor
				.chain(role)
				.with_context(|| format!("the {id} stack has no {role} chain"))?;
			anyhow::Ok(ChainEndpoint { genesis: chain.genesis(), ws: chain.rpc[0].clone() })
		})?;
		let Roles { people, asset_hub, bulletin } = &chains;
		ensure!(
			people.genesis != asset_hub.genesis &&
				people.genesis != bulletin.genesis &&
				asset_hub.genesis != bulletin.genesis,
			"People, Asset Hub and Bulletin need distinct genesis hashes: the host routes each role \
			 by its genesis"
		);
		ensure!(
			DOTNS_TLDS.contains(&descriptor.tld.as_str()),
			"the {id} stack's tld {} is not one the core serves ({})",
			descriptor.tld,
			DOTNS_TLDS.join(", ")
		);
		let dub = descriptor.service("dub").with_context(|| {
			format!("the {id} stack has no dub service, which the CLI uses as its identity backend")
		})?;
		let identity_backend = format!("{}/api/v1", dub.urls[0].trim_end_matches('/'));
		let pop_controller = descriptor.dotns_address("popController").with_context(|| {
			format!(
				"the {id} stack has no dotns.popController, which the CLI resolves usernames through"
			)
		})?;
		let light_client = match chain_backend {
			ChainBackend::Rpc => None,
			ChainBackend::LightClient => {
				Some(Arc::new(light_client(pointer, &descriptor, &chains, light_spec_directory())?))
			},
		};
		Ok(Self { light_client, descriptor, chains, identity_backend, pop_controller })
	}
}

/// Where the CLI keeps checked light specs: under its state directory,
/// outside any `--base-path`, since a spec is the same for every session.
fn light_spec_directory() -> PathBuf {
	crate::default_base_path().join("light-specs")
}

/// The light client of the stack `pointer` names, keeping its specs in
/// `spec_directory`. Fails when two chains of `descriptor` share a genesis
/// hash, or when it names no light spec for a routed chain or its relay.
pub fn light_client(
	pointer: &Pointer,
	descriptor: &Descriptor,
	routed: &Roles<ChainEndpoint>,
	spec_directory: PathBuf,
) -> Result<StackLightClient> {
	let light = StackLightClient::new(&pointer.stack_id(), descriptor, spec_directory)?;
	let Roles { people, asset_hub, bulletin } = routed;
	let relay = light.provider().relay_of(people.genesis);
	let run = [people.genesis, asset_hub.genesis, bulletin.genesis];
	let named = light.provider().light_specs();
	for chain in &descriptor.chains {
		let genesis = chain.genesis();
		ensure!(
			!(run.contains(&genesis) || relay == Some(genesis)) ||
				named.iter().any(|spec| spec.genesis_hash == genesis),
			"--chain-backend light-client cannot run {}: its descriptor names no lightSpec for {}; \
			 use --chain-backend rpc",
			descriptor.id,
			chain.role
		);
	}
	Ok(light)
}

#[cfg(test)]
mod tests;
