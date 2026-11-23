//! [`GenesisCheck`]: where a chain runtime reports the genesis hash of each
//! chain it connects to.

/// The genesis hash a connection was opened for, and the one its chain gave.
#[cfg_attr(test, derive(Clone, Debug, PartialEq))]
pub struct GenesisHashes {
	/// The hash the connection was opened for.
	pub named: [u8; 32],
	/// The hash the chain gave.
	pub answered: [u8; 32],
}

/// Is told the genesis hash each new connection's chain gives.
///
/// Set on a runtime with
/// [`ChainRuntime::checking_genesis`](super::ChainRuntime::checking_genesis).
pub trait GenesisCheck: Send + Sync {
	/// A new connection's chain gave its genesis hash.
	///
	/// Not called for a chain that gave no hash, and may be called from
	/// several connections at once.
	fn chain_answered(&self, hashes: GenesisHashes);
}
