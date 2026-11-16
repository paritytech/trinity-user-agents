//! dotNS transport shared by the core's dotNS readers.
//!
//! A lookup reads Asset Hub over the core's legacy JSON-RPC connection at one
//! best block, so a sequence of storage reads and contract views sees one
//! consistent state. It opens no `chainHead_v1_follow`: a node allows only a
//! few of those per connection, and how many dotNS reads run at once must not
//! decide whether the core or a product can follow a chain. No chain metadata
//! is needed.

use core::{
	sync::atomic::{AtomicU64, Ordering},
	time::Duration,
};

use futures::future::join_all;
use subxt::utils::H256;
use tracing::{Instrument, Span, debug, instrument, warn};

use crate::{
	chain::{EvmContractError, RuntimeApiCaller},
	chain_runtime::{ChainRuntime, LegacyConnection},
	evm_contract::read_batch,
	host_logic::dotns_gateway::{DotnsError, DotnsTransport},
	time::within,
};

/// Budget for one step of a lookup: finding the block, one storage read, one
/// contract view. A step that stalls this long is not going to answer.
#[cfg(not(test))]
const OPERATION_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
const OPERATION_TIMEOUT: Duration = Duration::from_millis(100);

/// Step names a timed out lookup reports.
const FINDING_THE_BLOCK: &str = "finding the best Asset Hub block";
const STORAGE_READ: &str = "an Asset Hub storage read";
const RUNTIME_CALL: &str = "an Asset Hub runtime call";

/// Numbers lookups so the lines of concurrent ones can be told apart.
static NEXT_LOOKUP: AtomicU64 = AtomicU64::new(1);

/// One block context for a sequence of dotNS reads, over the core's legacy
/// connection to Asset Hub.
pub struct DotnsLookup {
	legacy: LegacyConnection,
	at: H256,
	span: Span,
}

impl DotnsLookup {
	/// Reads Asset Hub at its best block, so a record is visible as soon as
	/// it is included.
	#[instrument(
		level = "debug",
		target = "truapi::dotns",
		name = "dotns_lookup",
		skip_all,
		fields(lookup = NEXT_LOOKUP.fetch_add(1, Ordering::Relaxed))
	)]
	pub async fn at_best_block(
		chain: &ChainRuntime,
		asset_hub_chain_genesis_hash: [u8; 32],
	) -> Result<Self, DotnsError> {
		let opened = async {
			let legacy = chain
				.legacy_connection(&asset_hub_chain_genesis_hash)
				.await
				.map_err(DotnsError::Connection)?;
			let at = legacy
				.methods
				.chain_get_block_hash(None)
				.await
				.map_err(|source| DotnsError::Rpc { method: "chain_getBlockHash", source })?
				.ok_or(DotnsError::NoBestBlock)?;
			Ok(Self { legacy, at, span: Span::current() })
		};
		// This path does network I/O, can stall for `OPERATION_TIMEOUT`, and
		// fails closed into a refusal a caller cannot tell from "you were not
		// granted this", so say what it did.
		let lookup = within(OPERATION_TIMEOUT, opened)
			.await
			.unwrap_or(Err(DotnsError::TimedOut { step: FINDING_THE_BLOCK }))
			.inspect_err(|error| {
				warn!(
					target: "truapi::dotns",
					reason = %error.reason(),
					"the dotNS lookup could not open at a best Asset Hub \
					 block; every grant behind it is refused"
				);
			})?;
		debug!(target: "truapi::dotns", at = ?lookup.at, "reading dotNS at the best Asset Hub block");
		Ok(lookup)
	}
}

#[async_trait::async_trait]
impl DotnsTransport for DotnsLookup {
	/// Reads one storage value at the lookup's block. `Ok(None)` when absent.
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		let read = self.legacy.methods.state_get_storage(&key, Some(self.at));
		let value = within(OPERATION_TIMEOUT, read)
			.await
			.map(|read| {
				read.map_err(|source| DotnsError::Rpc { method: "state_getStorage", source })
			})
			.unwrap_or(Err(DotnsError::TimedOut { step: STORAGE_READ }))
			.inspect_err(|error| {
				self.span.in_scope(|| {
					debug!(target: "truapi::dotns", reason = %error.reason(), "dotNS storage read failed");
				});
			})?;
		self.span.in_scope(|| {
			debug!(
				target: "truapi::dotns",
				bytes = value.as_ref().map(Vec::len),
				"dotNS storage read returned"
			);
		});
		Ok(value)
	}

	/// Dry-runs a contract view via `ReviveApi_call` at the lookup's block and
	/// returns its data.
	///
	/// Views originate from the synthetic always-mapped account. They work
	/// regardless of the queried account's revive mapping.
	async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, EvmContractError> {
		self.views(dest, vec![input])
			.await
			.pop()
			.expect("one requested view has one result")
	}

	async fn views(
		&mut self,
		dest: &[u8; 20],
		inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, EvmContractError>> {
		let span = self.span.clone();
		read_batch(self, dest, inputs)
			.await
			.inspect(|result| {
				span.in_scope(|| {
					debug!(
						target: "truapi::dotns",
						dest = %hex::encode(dest),
						outcome = match result {
							Ok(data) => format!("{} bytes", data.len()),
							Err(reason) => format!("failed: {reason}"),
						},
						"dotNS contract view returned"
					);
				});
			})
			.collect()
	}
}

#[async_trait::async_trait]
impl RuntimeApiCaller for DotnsLookup {
	async fn call(&mut self, function: &str, input: &[u8]) -> Result<Vec<u8>, subxt::Error> {
		self.call_batch(function, vec![input.to_vec()])
			.await
			.pop()
			.expect("one requested call has one result")
	}

	/// Sends every call at once, all at the lookup's block.
	async fn call_batch(
		&mut self,
		function: &str,
		inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, subxt::Error>> {
		let (methods, at) = (&self.legacy.methods, self.at);
		join_all(inputs.iter().map(|input| async move {
			within(OPERATION_TIMEOUT, methods.state_call(function, Some(input), Some(at)))
				.await
				.ok_or_else(|| subxt::Error::other_str(format!("{RUNTIME_CALL} timed out")))?
				.map_err(Into::into)
		}))
		.instrument(self.span.clone())
		.await
	}
}

#[cfg(test)]
mod tests;
