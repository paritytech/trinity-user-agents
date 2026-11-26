//! The checks that make a descriptor a [`Stack`] the CLI can run on.

use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use trinity_stack::Descriptor;

use super::{ChainEndpoint, POLKADOT_NAMESPACE, Stack, built_in::built_in_name, namespace_of};
use crate::{
	chain::{ChainAccess, StackLightClient},
	stack_descriptor::ROLE_NAMES,
};

impl Stack {
	/// The stack `descriptor` describes, read from the pointer whose identity
	/// is `stack_id`, connected to over `chain_access`. Fails when People,
	/// Asset Hub or Bulletin is missing, when the namespace is not a lowercase
	/// label or is the `polkadot` one, when the stack has no `dub` service or
	/// no `dotns.popController`, and on the light client as
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
			"a stack cannot run under the {POLKADOT_NAMESPACE} namespace, which would \
			 present it as Polkadot; use a stack with another id"
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
		let dub = descriptor.service("dub").with_context(|| {
			format!("the {id} stack has no dub service, which the CLI uses as its identity backend")
		})?;
		let identity_backend = format!("{}/api/v1", dub.urls[0].trim_end_matches('/'));
		let pop_controller = descriptor.dotns_address("popController").with_context(|| {
			format!(
				"the {id} stack has no dotns.popController, which the CLI resolves usernames through"
			)
		})?;
		let state_directory = match built_in_name(&stack_id) {
			Some(network) => network.to_owned(),
			None => {
				let digest = Sha256::digest(stack_id.as_bytes());
				format!("stack-{}", hex::encode(&digest[..16]))
			},
		};
		let light_client = match chain_access {
			ChainAccess::Rpc => None,
			ChainAccess::LightClient { spec_directory } => {
				Some(Arc::new(StackLightClient::new(&descriptor, &chains, spec_directory.clone())?))
			},
		};
		Ok(Self {
			stack_id,
			state_directory,
			light_client,
			descriptor,
			chains,
			identity_backend,
			pop_controller,
		})
	}
}

#[cfg(test)]
mod tests;
