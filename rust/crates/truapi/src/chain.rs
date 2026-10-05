//! Chain capabilities shared by host services: heads, block data, and
//! transaction validation and submission.
//!
//! Each capability is its own trait, and a consumer takes only the ones it
//! uses as `Arc<dyn …>`. Every method names the chain by its genesis hash, so
//! one implementation serves every chain. The runtime's `ChainRuntime`
//! implements all four over its per-chain subxt clients: reads and validation
//! go through the legacy JSON-RPC methods, which reach any block the node
//! still keeps, and submission goes through the shared chainHead client.
//!
//! `Ok(None)` means the node knows the block, or the extrinsic in it, does
//! not exist. A read the node cannot serve, such as the body or events of a
//! block it has pruned, or any read over a closed connection, is an `Err`.
//! Blocks and extrinsics are hashed with Blake2-256, the `BlakeTwo256`
//! hasher of every chain the core talks to; a chain with another hasher needs
//! a different implementation.

use sp_crypto_hashing::blake2_256;
use subxt::utils::H256;

mod block_backend;
mod heads;
mod subxt_chain;
mod tx_submitter;
mod tx_validator;

pub use block_backend::{BlockBackend, DispatchOutcome};
pub use heads::{ChainHeads, HeadEvent, Heads};
pub use tx_submitter::{TxSubmitter, WatchEvent};
pub use tx_validator::TxValidator;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

/// An encoded extrinsic, ready to submit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedExtrinsic {
    bytes: Vec<u8>,
}

impl EncodedExtrinsic {
    /// Wrap a fully encoded extrinsic, including its compact length prefix.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// The encoded extrinsic.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The extrinsic's Blake2-256 hash, as reported in blocks and
    /// transaction pools.
    pub fn hash(&self) -> H256 {
        H256(blake2_256(&self.bytes))
    }
}

/// A block identified by both its hash and its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HashAndNumber {
    /// Block hash.
    pub hash: H256,
    /// Block number.
    pub number: u64,
}
