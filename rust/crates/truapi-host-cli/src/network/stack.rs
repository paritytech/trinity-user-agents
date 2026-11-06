//! The checks that make a descriptor a [`Stack`] the CLI can run on.

use anyhow::{Context, Result, ensure};
use trinity_stack::Descriptor;
use truapi::platform::DOTNS_TLDS;

use super::{ChainEndpoint, RETIRED_POLKADOT_NAMESPACE, Stack, namespace_of};
use crate::{
	chain::ChainBackend,
	stack_descriptor::{ROLE_NAMES, Roles},
};

impl Stack {
	/// `descriptor` connected to over `chain_backend`. Fails when People,
	/// Asset Hub or Bulletin is missing, when two of them share a genesis hash,
	/// when the TLD is not one the core serves, when the namespace is not a
	/// lowercase label or is the retired `polkadot` one, and when the stack has
	/// no `dub` service or no `dotns.popController`.
	pub fn new(descriptor: Descriptor, chain_backend: ChainBackend) -> Result<Self> {
		let id = &descriptor.id;
		ensure!(
			id != RETIRED_POLKADOT_NAMESPACE,
			"a stack cannot run under the {RETIRED_POLKADOT_NAMESPACE} namespace, which earlier \
			 releases used for Polkadot mainnet; use a stack with another id"
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
		Ok(Self { descriptor, chain_backend, chains, identity_backend, pop_controller })
	}

	/// Fails as [`Stack::new`] does.
	pub fn check(descriptor: &Descriptor) -> Result<()> {
		Self::new(descriptor.clone(), ChainBackend::Rpc).map(drop)
	}
}

#[cfg(test)]
mod tests;
