//! [`OfflineSource`]: a build source that answers from fixtures.

use std::sync::atomic::{AtomicUsize, Ordering};

use subxt::utils::{AccountId32, H256};

use crate::chain::extrinsic::{
	ExtrinsicBuildError, ExtrinsicBuilder,
	builder::{BuildSource, RuntimeAtBirth, start_build},
};

/// Answers a build from a fixture runtime, a fixed host pipeline and a fixed
/// account nonce, and counts how often the host and the nonce were read.
pub struct OfflineSource {
	genesis: H256,
	runtime: RuntimeAtBirth,
	host_extension_version: Option<u8>,
	account_nonce: u64,
	host_reads: AtomicUsize,
	nonce_reads: AtomicUsize,
}

impl OfflineSource {
	/// A source for the chain `genesis` whose finalized head runs `runtime`,
	/// which the host builds as V4 and whose accounts are all at nonce 0.
	pub fn new(genesis: H256, runtime: RuntimeAtBirth) -> Self {
		Self {
			genesis,
			runtime,
			host_extension_version: None,
			account_nonce: 0,
			host_reads: AtomicUsize::new(0),
			nonce_reads: AtomicUsize::new(0),
		}
	}

	/// The host builds the chain with `extension_version`, or as V4 for `None`.
	pub fn with_host_extension_version(mut self, extension_version: Option<u8>) -> Self {
		self.host_extension_version = extension_version;
		self
	}

	/// Every account's next nonce is `nonce`.
	pub fn with_account_nonce(mut self, nonce: u64) -> Self {
		self.account_nonce = nonce;
		self
	}

	/// A builder for an extrinsic on this source's chain.
	pub fn start_build(&self) -> ExtrinsicBuilder<'_> {
		start_build(self, self.genesis)
	}

	/// How many times a build asked for the host's pipeline.
	pub fn host_reads(&self) -> usize {
		self.host_reads.load(Ordering::SeqCst)
	}

	/// How many times a build asked for an account nonce.
	pub fn nonce_reads(&self) -> usize {
		self.nonce_reads.load(Ordering::SeqCst)
	}
}

#[async_trait::async_trait]
impl BuildSource for OfflineSource {
	async fn runtime_at_birth(
		&self,
		_genesis: H256,
	) -> Result<RuntimeAtBirth, ExtrinsicBuildError> {
		Ok(self.runtime.clone())
	}

	async fn host_extension_version(
		&self,
		_genesis: H256,
	) -> Result<Option<u8>, ExtrinsicBuildError> {
		self.host_reads.fetch_add(1, Ordering::SeqCst);
		Ok(self.host_extension_version)
	}

	async fn account_nonce(
		&self,
		_genesis: H256,
		_account: AccountId32,
	) -> Result<u64, ExtrinsicBuildError> {
		self.nonce_reads.fetch_add(1, Ordering::SeqCst);
		Ok(self.account_nonce)
	}
}
