//! dotNS transport shared by the core's dotNS readers.
//!
//! A lookup reads Asset Hub over the core's legacy JSON-RPC connection at one
//! best block, so a sequence of storage reads and contract views sees one
//! consistent state. It opens no `chainHead_v1_follow`: a node allows only a
//! few of those per connection, and how many dotNS reads run at once must not
//! decide whether the core or a product can follow a chain. No chain metadata
//! is needed.

use core::{future::Future, time::Duration};

use async_lock::Semaphore;
use futures::{FutureExt, future::join_all, pin_mut};
use subxt::utils::H256;
use tracing::{debug, info};

use crate::{
	chain::{EvmContractError, RuntimeApiCaller},
	chain_runtime::{ChainRuntime, LegacyConnection},
	evm_contract::read_batch,
	host_logic::dotns_gateway::{DotnsError, DotnsTransport},
	time::{RealTimer, Timer},
};

/// Budget for one step of a lookup: finding the block, one storage read, one
/// contract view. A step that stalls this long is not going to answer.
#[cfg(not(test))]
const OPERATION_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
const OPERATION_TIMEOUT: Duration = Duration::from_millis(100);

/// Most dotNS requests in flight at once, across every lookup. The core's
/// connection queues about a thousand requests before it drops or refuses
/// them, and its transaction client shares that connection, so a burst of
/// lookups waits here instead. Waiting does not count against
/// [`OPERATION_TIMEOUT`].
const MAX_REQUESTS_IN_FLIGHT: usize = 256;

static REQUESTS_IN_FLIGHT: Semaphore = Semaphore::new(MAX_REQUESTS_IN_FLIGHT);

/// One block context for a sequence of dotNS reads, over the core's legacy
/// connection to Asset Hub.
pub struct DotnsLookup {
	legacy: LegacyConnection,
	at: H256,
}

impl DotnsLookup {
	/// Reads Asset Hub at its best block, so a record is visible as soon as
	/// it is included.
	pub async fn at_best_block(
		chain: &ChainRuntime,
		asset_hub_chain_genesis_hash: [u8; 32],
	) -> Result<Self, DotnsError> {
		let _permit = REQUESTS_IN_FLIGHT.acquire().await;
		let opened = async {
			let legacy = chain
				.legacy_connection(&asset_hub_chain_genesis_hash)
				.await
				.map_err(|failure| failure.to_string())?;
			let at = legacy
				.methods
				.chain_get_block_hash(None)
				.await
				.map_err(|error| error.to_string())?
				.ok_or("the node named no best block")?;
			Ok::<_, String>(Self { legacy, at })
		};
		// This path does network I/O, can stall for `OPERATION_TIMEOUT`, and
		// fails closed into a refusal a caller cannot tell from "you were not
		// granted this", so say what it did.
		let lookup = within(opened, "finding the best Asset Hub block")
			.await
			.and_then(core::convert::identity)
			.inspect_err(|reason| {
				info!(
					target: "truapi::dotns",
					%reason,
					"no best Asset Hub block to read dotNS at; every grant \
					 behind this lookup is refused"
				);
			})
			.map_err(DotnsError::Chain)?;
		debug!(target: "truapi::dotns", at = ?lookup.at, "reading dotNS at the best Asset Hub block");
		Ok(lookup)
	}
}

/// Runs `step`, or names it as timed out after [`OPERATION_TIMEOUT`].
async fn within<T>(step: impl Future<Output = T>, name: &str) -> Result<T, String> {
	let step = step.fuse();
	let timeout = RealTimer.sleep(OPERATION_TIMEOUT).fuse();
	pin_mut!(step, timeout);
	futures::select! {
		output = step => Ok(output),
		() = timeout => Err(format!("{name} timed out")),
	}
}

#[async_trait::async_trait]
impl DotnsTransport for DotnsLookup {
	/// Reads one storage value at the lookup's block. `Ok(None)` when absent.
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		let _permit = REQUESTS_IN_FLIGHT.acquire().await;
		let read = self.legacy.methods.state_get_storage(&key, Some(self.at));
		let value = within(read, "an Asset Hub storage read")
			.await?
			.map_err(|error| DotnsError::Storage(error.to_string()))?;
		debug!(
			target: "truapi::dotns",
			bytes = value.as_ref().map(Vec::len),
			"dotNS storage read returned"
		);
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
		read_batch(self, dest, inputs)
			.await
			.inspect(|result| {
				debug!(
					target: "truapi::dotns",
					dest = %hex::encode(dest),
					outcome = match result {
						Ok(data) => format!("{} bytes", data.len()),
						Err(reason) => format!("failed: {reason}"),
					},
					"dotNS contract view returned"
				);
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
			let _permit = REQUESTS_IN_FLIGHT.acquire().await;
			within(methods.state_call(function, Some(input), Some(at)), "an Asset Hub runtime call")
				.await
				.map_err(subxt::Error::other_str)?
				.map_err(Into::into)
		}))
		.await
	}
}

#[cfg(test)]
mod tests;
