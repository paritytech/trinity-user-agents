//! A chain and a registry for the reconstruction tests, which see what each
//! scan reads.

use core::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use subxt::utils::H256;

use super::{ENTROPY, FakeChain};
use crate::{
	chain::{HashAndNumber, Heads, StorageWatchError},
	chain_runtime::RuntimeFailure,
	coinage::{
		chain::{AliasState, ChainCoin, CoinageChain, RingPlace},
		namespace::datastore_key,
		reconstruction::{InstallationRegistry, RegistryKey},
	},
	installation::InstallationId,
};

/// The finalized head of a [`RecordingChain`].
pub const FINALIZED: HashAndNumber = HashAndNumber { hash: H256([0xf1; 32]), number: 8 };

/// The best head of a [`RecordingChain`], ahead of [`FINALIZED`].
pub const BEST: HashAndNumber = HashAndNumber { hash: H256([0xb1; 32]), number: 10 };

/// A [`FakeChain`] whose heads differ and which records the block and the
/// accounts of every coin read. Its head read fails while `heads_failing`
/// is set, and its coin reads fail once `coin_reads_left` reaches zero.
#[derive(Default)]
pub struct RecordingChain {
	/// The coins and ring places it serves.
	pub chain: FakeChain,
	/// The head read fails.
	pub heads_failing: AtomicBool,
	/// How many more coin reads succeed; unlimited when none.
	pub coin_reads_left: Mutex<Option<usize>>,
	/// The block and the accounts of every coin read that succeeded.
	pub coin_reads: Mutex<Vec<(H256, Vec<[u8; 32]>)>>,
}

impl RecordingChain {
	/// Every account a coin read asked for, in order.
	pub fn accounts_read(&self) -> Vec<[u8; 32]> {
		self.coin_reads.lock().iter().flat_map(|(_, accounts)| accounts.iter().copied()).collect()
	}
}

fn failure() -> StorageWatchError {
	StorageWatchError::Chain(RuntimeFailure::host_failure("recording_chain", "the test fails this read"))
}

#[async_trait::async_trait]
impl CoinageChain for RecordingChain {
	async fn heads(&self) -> Result<Heads, StorageWatchError> {
		if self.heads_failing.load(Ordering::SeqCst) {
			return Err(failure());
		}
		Ok(Heads { finalized: FINALIZED, best: BEST })
	}

	async fn coins_at(&self, at: H256, accounts: &[[u8; 32]]) -> Result<Vec<Option<ChainCoin>>, StorageWatchError> {
		if let Some(left) = self.coin_reads_left.lock().as_mut() {
			if *left == 0 {
				return Err(failure());
			}
			*left -= 1;
		}
		self.coin_reads.lock().push((at, accounts.to_vec()));
		self.chain.coins_at(at, accounts).await
	}

	async fn members_at(&self, at: H256, members: &[[u8; 32]]) -> Result<Vec<Option<(u32, i8)>>, StorageWatchError> {
		self.chain.members_at(at, members).await
	}

	async fn places_at(&self, at: H256, exponent: i8, members: &[[u8; 32]]) -> Result<Vec<Option<RingPlace>>, StorageWatchError> {
		self.chain.places_at(at, exponent, members).await
	}

	async fn aliases_at(
		&self,
		at: H256,
		exponent: i8,
		ring: u32,
		aliases: &[[u8; 32]],
	) -> Result<Vec<Option<AliasState>>, StorageWatchError> {
		self.chain.aliases_at(at, exponent, ring, aliases).await
	}

	async fn ring_members_at(&self, at: H256, exponent: i8, ring: u32) -> Result<Vec<[u8; 32]>, StorageWatchError> {
		self.chain.ring_members_at(at, exponent, ring).await
	}

	async fn tokens_consumed_at(&self, at: H256, tokens: &[(u32, [u8; 32])]) -> Result<Vec<bool>, StorageWatchError> {
		self.chain.tokens_consumed_at(at, tokens).await
	}

	async fn nonce_at(&self, at: H256, account: [u8; 32]) -> Result<u32, StorageWatchError> {
		self.chain.nonce_at(at, account).await
	}
}

/// A registry that lists the given installations, sealed under the key of
/// [`ENTROPY`]'s seed.
pub struct ListedRegistry(pub Vec<InstallationId>);

#[async_trait::async_trait]
impl InstallationRegistry for ListedRegistry {
	async fn records(&self) -> Option<Vec<Vec<u8>>> {
		let key = RegistryKey::of(&datastore_key(&ENTROPY).ok()?.0)?;
		Some(self.0.iter().map(|installation| key.seal(*installation).to_vec()).collect())
	}
}
