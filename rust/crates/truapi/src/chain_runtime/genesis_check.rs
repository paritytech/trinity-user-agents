//! [`GenesisCheck`]: where a chain runtime reports the genesis hash of each
//! chain it connects to.

/// Is told the genesis hash each new connection's chain gives.
///
/// A [`ChainRuntime`](super::ChainRuntime) given one asks every connection it
/// opens for its chain's genesis hash before anything else uses the
/// connection, and hands out only a connection whose chain gives the hash it
/// was opened for.
pub trait GenesisCheck: Send + Sync {
	/// A connection opened for `named` reached a chain with genesis hash `answered`.
	///
	/// Not called for a chain that gave no hash, and may be called from
	/// several connections at once.
	fn chain_answered(&self, named: [u8; 32], answered: [u8; 32]);
}
