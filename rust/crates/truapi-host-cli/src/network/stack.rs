//! The checks that make a descriptor a [`Stack`] the CLI can run on.

use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use trinity_stack::Descriptor;

use super::{
	ChainEndpoint, DescriptorRefresh, POLKADOT_NAMESPACE, Stack, built_in::built_in_name,
	namespace_of,
};
use crate::{
	chain::{ChainAccess, StackLightClient},
	stack_descriptor::{ROLE_NAMES, Roles},
};

/// What the CLI reads from a descriptor to reach the stack's chains and
/// services.
pub struct Routes {
	/// People, Asset Hub and Bulletin: each one's genesis hash and first RPC
	/// URL.
	pub chains: Roles<ChainEndpoint>,
	/// The identity backend: the `dub` service's URL with `/api/v1`.
	pub identity_backend: String,
	/// The `dotns.popController` address.
	pub pop_controller: [u8; 20],
}

/// The chains and services the CLI reads from `descriptor`. Fails when
/// People, Asset Hub or Bulletin is missing, when the namespace is not a
/// lowercase label or is the `polkadot` one, and when the stack has no `dub`
/// service or no `dotns.popController`. No two of its chains share a genesis
/// hash, which the descriptor's own rules refuse.
pub fn routes(descriptor: &Descriptor) -> Result<Routes> {
	let id = &descriptor.id;
	ensure!(
		id != POLKADOT_NAMESPACE,
		"a stack cannot run under the {POLKADOT_NAMESPACE} namespace, which would present it as \
		 Polkadot; use a stack with another id"
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
	Ok(Routes { chains, identity_backend, pop_controller })
}

/// The directory under `<base-path>/v2` that holds the state of the stack
/// with `stack_id`: a built-in network's name, or `stack-` and the first 32
/// hex digits of the SHA-256 of `stack_id`.
pub fn state_directory_of(stack_id: &str) -> String {
	match built_in_name(stack_id) {
		Some(network) => network.to_owned(),
		None => {
			let digest = Sha256::digest(stack_id.as_bytes());
			format!("stack-{}", hex::encode(&digest[..16]))
		},
	}
}

impl Stack {
	/// The stack `core` runs, from `descriptor`, connected to over
	/// `chain_access`, and kept current by `refresh` when it is not a local
	/// file. Fails as [`routes`] does, and on the light client as
	/// [`StackLightClient::new`] does.
	pub fn new(
		core: truapi::platform::Stack,
		refresh: Option<DescriptorRefresh>,
		descriptor: Descriptor,
		chain_access: &ChainAccess,
	) -> Result<Self> {
		let Routes { chains, identity_backend, pop_controller } = routes(&descriptor)?;
		let light_client = match chain_access {
			ChainAccess::Rpc => None,
			ChainAccess::LightClient { spec_directory } => {
				Some(Arc::new(StackLightClient::new(&descriptor, &chains, spec_directory.clone())?))
			},
		};
		Ok(Self {
			state_directory: state_directory_of(core.id()),
			core,
			refresh,
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
