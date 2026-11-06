//! The checks that make a descriptor a [`Stack`] the CLI can run on.

use anyhow::{Context, Result, ensure};
use trinity_stack::Descriptor;
use truapi::platform::DOTNS_TLDS;

use super::Stack;
use crate::{
	chain::ChainBackend,
	stack_descriptor::{ROLE_NAMES, Roles},
};

/// The namespace of `--network polkadot`, which earlier releases stored
/// mainnet sessions under.
const RETIRED_POLKADOT_NAMESPACE: &str = "polkadot";

/// The product namespace of the stack `id`: `id` without a trailing `-fork`,
/// except that `polkadot-fork` stays whole rather than map to the retired
/// `polkadot` namespace.
pub fn namespace_of(id: &str) -> &str {
	id.strip_suffix("-fork")
		.filter(|source| *source != RETIRED_POLKADOT_NAMESPACE)
		.unwrap_or(id)
}

impl Stack {
	/// `descriptor` connected to over `chain_backend`. Fails when People,
	/// Asset Hub or Bulletin is missing, when two of them share a genesis hash,
	/// when the TLD is not one the core serves, when the namespace is not a
	/// lowercase label, and when the stack has no `dub` service or no
	/// `dotns.popController`.
	pub fn new(descriptor: Descriptor, chain_backend: ChainBackend) -> Result<Self> {
		let id = &descriptor.id;
		let Roles { people, asset_hub, bulletin } = ROLE_NAMES.try_map(|role| {
			let chain = descriptor
				.chain(role)
				.with_context(|| format!("the {id} stack has no {role} chain"))?;
			anyhow::Ok(chain.genesis())
		})?;
		ensure!(
			people != asset_hub && people != bulletin && asset_hub != bulletin,
			"People, Asset Hub and Bulletin need distinct genesis hashes: the host routes each \
			 role by its genesis"
		);
		ensure!(
			DOTNS_TLDS.contains(&descriptor.tld.as_str()),
			"the {id} stack's tld {} is not one the core serves ({})",
			descriptor.tld,
			DOTNS_TLDS.join(", ")
		);
		ensure!(
			truapi::platform::is_dns_label(namespace_of(id)),
			"the stack id {id:?} is not a lowercase label to use as the product namespace"
		);
		ensure!(
			descriptor.service("dub").is_some(),
			"the {id} stack has no dub service, which the CLI uses as its identity backend"
		);
		ensure!(
			descriptor.dotns_address("popController").is_some(),
			"the {id} stack has no dotns.popController, which the CLI resolves usernames through"
		);
		Ok(Self { descriptor, chain_backend })
	}

	/// A `--stack` descriptor's network. Fails as [`Stack::new`] does, and for
	/// the id `polkadot`, the retired namespace of Polkadot mainnet sessions.
	pub fn from_stack_option(descriptor: Descriptor, chain_backend: ChainBackend) -> Result<Self> {
		ensure!(
			descriptor.id != RETIRED_POLKADOT_NAMESPACE,
			"a stack cannot run under the {RETIRED_POLKADOT_NAMESPACE} namespace, which earlier \
			 releases used for Polkadot mainnet; use a stack with another id"
		);
		Self::new(descriptor, chain_backend)
	}
}

#[cfg(test)]
mod tests;
