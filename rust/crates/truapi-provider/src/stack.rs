//! The chains of a Trinity stack, for the light client.
//!
//! A [`LightStack`] is built from a stack's descriptor and filled with the
//! light specs the host fetched, through [`LightStack::add_light_spec`]. It
//! takes the relay wiring and the statement-store placement from the
//! descriptor, and checks every spec against it, so a provider built with
//! [`EmbeddedChainProviderBuilder::stack`](crate::EmbeddedChainProviderBuilder::stack)
//! runs any stack rather than only the bundled ones.

use std::collections::HashMap;

use trinity_stack::{Chain, Descriptor, Pointer};

use crate::{ChainSource, ProviderError};

/// One chain of a stack and its light spec once the host supplied it.
struct StackChain {
	chain: Chain,
	specification: Option<String>,
}

/// What the light client is handed to run one chain of a stack.
pub struct StackSources {
	/// The chain's own source.
	pub chain: ChainSource,
	/// The relay's genesis hash and source, for a parachain.
	pub relay: Option<([u8; 32], ChainSource)>,
}

/// The light-client chains of one stack.
///
/// Every chain other than the relay syncs through the relay, and a chain runs
/// the statement-store protocol only when the descriptor sets `statementStore`
/// on it. A chain whose spec was never added is unknown to the provider, and
/// the rest of the stack runs without it.
pub struct LightStack {
	id: String,
	generation: String,
	relay: [u8; 32],
	chains: HashMap<[u8; 32], StackChain>,
}

impl core::fmt::Debug for LightStack {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("LightStack")
			.field("id", &self.id)
			.field("generation", &self.generation)
			.field("chains", &self.chains.len())
			.finish()
	}
}

impl LightStack {
	/// The chains of `descriptor`, with no light spec yet.
	///
	/// `id` is what identifies the stack, as `trinity_stack::Pointer::stack_id`
	/// gives it. Stored databases are kept under it, so two stacks that share
	/// genesis hashes never read each other's. Fails when two chains of the
	/// descriptor share a genesis hash.
	pub fn new(id: impl Into<String>, descriptor: &Descriptor) -> Result<Self, ProviderError> {
		let mut chains = HashMap::new();
		let mut relay = None;
		for chain in &descriptor.chains {
			let genesis = chain.genesis();
			if chain.para_id.is_none() {
				relay = Some(genesis);
			}
			let entry = StackChain { chain: chain.clone(), specification: None };
			if chains.insert(genesis, entry).is_some() {
				return Err(ProviderError::Stack {
					reason: format!(
						"genesis 0x{} appears on more than one chain",
						hex::encode(genesis)
					),
				});
			}
		}
		let relay = relay.ok_or_else(|| ProviderError::Stack {
			reason: "the stack has no relay chain".into(),
		})?;
		Ok(Self { id: id.into(), generation: descriptor.generation.clone(), relay, chains })
	}

	/// The stack a `link` points to, from its `descriptor` as served.
	///
	/// The descriptor is opened under the link's rules, so a link with a key
	/// accepts only a document signed with it. Fails when the link or the
	/// descriptor is refused.
	pub fn open(link: &str, descriptor: &[u8]) -> Result<Self, ProviderError> {
		let refused =
			|error: trinity_stack::Error| ProviderError::Stack { reason: error.to_string() };
		let pointer = Pointer::from_link(link).map_err(refused)?;
		let verified = trinity_stack::open(descriptor, &pointer).map_err(refused)?;
		Self::new(pointer.stack_id(), &verified.descriptor)
	}

	/// Keep `specification` as the light spec of the chain with `genesis_hash`.
	///
	/// The bytes are checked against the `sha256` the descriptor names and the
	/// genesis hash they imply against the chain's. Fails with
	/// [`ProviderError::LightSpec`] when either differs or the descriptor names
	/// no light spec for the chain, and with [`ProviderError::UnknownGenesis`]
	/// for a chain outside the stack. A refused spec leaves the stack as it was.
	pub fn add_light_spec(
		&mut self,
		genesis_hash: [u8; 32],
		specification: &[u8],
	) -> Result<(), ProviderError> {
		let entry = self
			.chains
			.get_mut(&genesis_hash)
			.ok_or(ProviderError::UnknownGenesis { genesis: genesis_hash })?;
		let refused = |reason: String| ProviderError::LightSpec {
			role: entry.chain.role.clone(),
			genesis: genesis_hash,
			reason,
		};
		entry
			.chain
			.check_light_spec(specification)
			.map_err(|error| refused(error.to_string()))?;
		let specification =
			core::str::from_utf8(specification).map_err(|error| refused(error.to_string()))?;
		entry.specification = Some(specification.to_owned());
		Ok(())
	}

	/// What identifies the stack.
	pub fn id(&self) -> &str {
		&self.id
	}

	/// The descriptor's `generation`.
	pub fn generation(&self) -> &str {
		&self.generation
	}

	/// The relay `genesis_hash` syncs through. The relay itself, and a chain
	/// outside the stack, have none.
	pub fn relay_of(&self, genesis_hash: [u8; 32]) -> Option<[u8; 32]> {
		(genesis_hash != self.relay && self.chains.contains_key(&genesis_hash))
			.then_some(self.relay)
	}

	/// The light-client source of `genesis_hash`, when its spec was added.
	pub fn source(&self, genesis_hash: [u8; 32]) -> Option<ChainSource> {
		let entry = self.chains.get(&genesis_hash)?;
		Some(ChainSource::LightClient {
			specification: entry.specification.clone()?.into(),
			database_content: None,
			statement_protocol: entry.chain.statement_store == Some(true),
		})
	}

	/// What the light client needs to run `genesis_hash`.
	///
	/// Fails with [`ProviderError::UnknownGenesis`] for a chain outside the
	/// stack or without its spec, and with [`ProviderError::UnknownRelay`] for
	/// a parachain whose relay has no spec yet.
	pub fn sources(&self, genesis_hash: [u8; 32]) -> Result<StackSources, ProviderError> {
		let chain = self
			.source(genesis_hash)
			.ok_or(ProviderError::UnknownGenesis { genesis: genesis_hash })?;
		let relay = match self.relay_of(genesis_hash) {
			None => None,
			Some(relay) => {
				let source = self.source(relay).ok_or(ProviderError::UnknownRelay { relay })?;
				Some((relay, source))
			},
		};
		Ok(StackSources { chain, relay })
	}
}

#[cfg(test)]
mod tests;
