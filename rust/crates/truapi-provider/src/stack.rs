//! The chains of a Trinity stack, for the light client.
//!
//! A [`LightStack`] is built from a stack's descriptor. The host fetches the
//! light specs [`LightStack::light_specs`] lists and hands each over through
//! [`LightStack::add_light_spec`], which checks it against the descriptor.
//! The descriptor also gives the relay every other chain syncs through and
//! the chains that run the statement-store protocol.

use std::{
	collections::HashMap,
	sync::{Mutex, MutexGuard, PoisonError},
};

use trinity_stack::{Chain, Descriptor, Error, Pointer};

use crate::{ChainSource, ProviderError};

/// Where a chain's light spec is published.
#[derive(Debug, PartialEq, Eq)]
pub struct LightSpecSource {
	/// The chain's genesis hash.
	pub genesis_hash: [u8; 32],
	/// The chain's role in the descriptor.
	pub role: String,
	/// The `lightSpec.url` the descriptor names.
	pub url: String,
	/// The `lightSpec.sha256` the spec's bytes are checked against, as the
	/// descriptor writes it. A host can keep a checked spec under it.
	pub sha256: String,
}

/// The light-client chains of one stack.
///
/// A chain is unknown to the provider until its spec is added, and a
/// parachain also until the relay's is. The chains that have what they need
/// run meanwhile. Build one provider per stack: the light client keys a chain
/// by its genesis hash, which a fork shares with its source.
pub struct LightStack {
	id: String,
	generation: String,
	relay: [u8; 32],
	chains: Vec<([u8; 32], Chain)>,
	specs: Mutex<HashMap<[u8; 32], String>>,
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

fn refused(source: Error) -> ProviderError {
	ProviderError::Stack { source }
}

impl LightStack {
	/// The chains of `descriptor`, with no light spec yet.
	///
	/// `id` is what identifies the stack, as `trinity_stack::Pointer::stack_id`
	/// gives it. Fails when the descriptor breaks a rule of its format, as one
	/// built in code can.
	pub fn new(id: impl Into<String>, descriptor: &Descriptor) -> Result<Self, ProviderError> {
		descriptor.validate().map_err(refused)?;
		let relay = descriptor.relay().expect("a valid descriptor has a relay").genesis();
		Ok(Self {
			id: id.into(),
			generation: descriptor.generation.clone(),
			relay,
			chains: descriptor
				.chains
				.iter()
				.map(|chain| (chain.genesis(), chain.clone()))
				.collect(),
			specs: Mutex::new(HashMap::new()),
		})
	}

	/// The stack a `link` points to, from its `descriptor` as served.
	///
	/// The descriptor is opened under the link's rules, so a link with a key
	/// accepts only a document signed with it. Its `validUntil`, `expiresAt`
	/// and `sequence` are not read: hand over the copy the stack's reader
	/// accepted. Fails when the link or the descriptor is refused.
	pub fn open(link: &str, descriptor: &[u8]) -> Result<Self, ProviderError> {
		let pointer = Pointer::from_link(link).map_err(refused)?;
		let verified = trinity_stack::open(descriptor, &pointer).map_err(refused)?;
		Self::new(pointer.stack_id(), &verified.descriptor)
	}

	/// Where the host fetches each chain's light spec, in the descriptor's
	/// order. A chain the descriptor names no light spec for is left out.
	pub fn light_specs(&self) -> Vec<LightSpecSource> {
		self.chains
			.iter()
			.filter_map(|(genesis_hash, chain)| {
				let named = chain.light_spec.as_ref()?;
				Some(LightSpecSource {
					genesis_hash: *genesis_hash,
					role: chain.role.clone(),
					url: named.url.clone(),
					sha256: named.sha256.clone(),
				})
			})
			.collect()
	}

	/// Keep `specification` as the light spec of the chain with `genesis_hash`.
	///
	/// The bytes are checked against the `sha256` the descriptor names, and
	/// the genesis hash they imply against the chain's. Fails with
	/// [`ProviderError::LightSpec`] when either differs or the descriptor
	/// names no light spec for the chain, and with
	/// [`ProviderError::UnknownGenesis`] for a chain outside the stack. A
	/// refused spec changes nothing.
	pub fn add_light_spec(
		&self,
		genesis_hash: [u8; 32],
		specification: &[u8],
	) -> Result<(), ProviderError> {
		let chain = self
			.chain(genesis_hash)
			.ok_or(ProviderError::UnknownGenesis { genesis: genesis_hash })?;
		let refused = |source| ProviderError::LightSpec {
			role: chain.role.clone(),
			genesis: genesis_hash,
			source,
		};
		chain.check_light_spec(specification).map_err(refused)?;
		let specification = core::str::from_utf8(specification)
			.map_err(|error| refused(Error::MalformedSpec(error.to_string())))?;
		self.specs().insert(genesis_hash, specification.to_owned());
		Ok(())
	}

	/// Whether the spec of `genesis_hash` was added.
	pub fn has_light_spec(&self, genesis_hash: [u8; 32]) -> bool {
		self.specs().contains_key(&genesis_hash)
	}

	/// What identifies the stack.
	pub fn id(&self) -> &str {
		&self.id
	}

	/// The descriptor's `generation`.
	pub fn generation(&self) -> &str {
		&self.generation
	}

	fn chain(&self, genesis_hash: [u8; 32]) -> Option<&Chain> {
		self.chains
			.iter()
			.find(|(genesis, _)| *genesis == genesis_hash)
			.map(|(_, chain)| chain)
	}

	fn specs(&self) -> MutexGuard<'_, HashMap<[u8; 32], String>> {
		self.specs.lock().unwrap_or_else(PoisonError::into_inner)
	}
}

/// What the provider reads of a stack to connect to its chains.
///
/// A trait of this private module, so a host that holds a [`LightStack`]
/// sees only what it hands over.
pub trait StackChains {
	/// The relay `genesis_hash` syncs through. The relay itself, and a chain
	/// outside the stack, have none.
	fn relay_of(&self, genesis_hash: [u8; 32]) -> Option<[u8; 32]>;

	/// What the light client runs `genesis_hash` from, once its spec was
	/// added.
	fn source(&self, genesis_hash: [u8; 32]) -> Option<ChainSource>;
}

impl StackChains for LightStack {
	fn relay_of(&self, genesis_hash: [u8; 32]) -> Option<[u8; 32]> {
		(genesis_hash != self.relay && self.chain(genesis_hash).is_some()).then_some(self.relay)
	}

	fn source(&self, genesis_hash: [u8; 32]) -> Option<ChainSource> {
		let specification = self.specs().get(&genesis_hash)?.clone();
		Some(ChainSource::LightClient {
			specification: specification.into(),
			database_content: None,
			statement_protocol: self.chain(genesis_hash)?.statement_store == Some(true),
		})
	}
}

#[cfg(test)]
mod tests;
