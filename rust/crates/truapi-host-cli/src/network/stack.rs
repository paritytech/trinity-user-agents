//! The checks that make a descriptor a [`Stack`] the CLI can run on.

use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use trinity_stack::Descriptor;
use truapi::platform::DOTNS_TLDS;

use super::{ChainEndpoint, POLKADOT_NAMESPACE, Stack, namespace_of};
use crate::{
	chain::{ChainAccess, StackLightClient},
	stack_descriptor::ROLE_NAMES,
};

impl Stack {
	/// The stack `descriptor` describes, read from the pointer whose identity
	/// is `stack_id`, connected to over `chain_access`. Fails when People,
	/// Asset Hub or Bulletin is missing, when the TLD is not one the core
	/// serves, when the namespace is not a lowercase label or is the
	/// `polkadot` one, when the stack has no `dub` service or no
	/// `dotns.popController`, and on the light client as
	/// [`StackLightClient::new`] does. No two of its chains share a genesis
	/// hash, which the descriptor's own rules refuse.
	pub fn new(
		stack_id: String,
		descriptor: Descriptor,
		chain_access: &ChainAccess,
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
		let light_client = match chain_access {
			ChainAccess::Rpc => None,
			ChainAccess::LightClient { spec_directory } => {
				Some(Arc::new(StackLightClient::new(&descriptor, &chains, spec_directory.clone())?))
			},
		};
		Ok(Self {
			stack_id,
			keyed: true,
			light_client,
			descriptor,
			chains,
			identity_backend,
			pop_controller,
		})
	}

	/// This stack, read from a pointer with no key or from a local file, so
	/// its id is not its key and nothing makes its `sequence` grow.
	pub fn without_key(self) -> Self {
		Self { keyed: false, ..self }
	}
}

#[cfg(test)]
mod tests;
