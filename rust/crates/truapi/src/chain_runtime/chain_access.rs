//! [`ChainAccess`]: one chain the core reads, with the runtime that reaches
//! it.

use subxt::utils::H256;

use super::{ChainRuntime, LegacyConnection, RuntimeFailure};

/// One chain, named by its genesis hash, and the runtime that connects to it,
/// so a reader of the chain needs neither passed apart.
#[derive(Clone)]
pub struct ChainAccess {
	/// The runtime that connects to the chain.
	pub runtime: ChainRuntime,
	/// The chain's genesis hash.
	pub genesis: H256,
}

impl ChainAccess {
	/// The chain with `genesis`, reached through `runtime`.
	pub fn new(runtime: ChainRuntime, genesis: H256) -> Self {
		Self { runtime, genesis }
	}

	/// A legacy RPC connection to the chain. Fails when no connection can be
	/// opened.
	pub async fn legacy_connection(&self) -> Result<LegacyConnection, RuntimeFailure> {
		self.runtime.legacy_connection(self.genesis.as_bytes()).await
	}
}
