//! Preimage lookup for the CLI host: the blob behind a key, read from a Bulletin node by CID.
//!
//! Transaction storage serves every blob a node retains over bitswap, and the node exposes that
//! on its JSON-RPC as `bitswap_v1_get(cid)`. The CID is fixed by the key: CIDv1, the `raw`
//! codec, and the blake2b-256 multihash that is the preimage key itself. A lookup is therefore
//! one round trip to the network's Bulletin chain, and the host needs no IPFS stack of its own.
//!
//! A subscription emits the current answer at once. On a miss it keeps asking every
//! [`POLL_INTERVAL`] until the blob appears, since a submission from another host lands within
//! a block or two, and it ends once it has delivered a value. A node that cannot be reached is
//! reported as a miss as well, never as the end of the subscription, so a product waiting on a
//! blob survives a transport failure. A request the node can never answer, such as an invalid
//! CID or a node without `bitswap_v1_get`, ends the subscription with an error after the miss.
//! Every value is checked against the key before it is emitted.

use std::{sync::Arc, time::Duration};

use futures::{
	future::{self, FutureExt},
	stream::{self, BoxStream, StreamExt},
};
use subxt_rpcs::client::RpcClient;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, info, warn};
use truapi::{BitswapError, BulletinCid, bitswap_get, latest as api};

use crate::chain::CliChainProvider;

/// How long a miss waits before asking the node again: about one Bulletin block.
const POLL_INTERVAL: Duration = Duration::from_secs(6);
/// Bound on one round trip to the node.
const RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a source could not answer.
pub enum SourceError {
	/// Worth asking again: the node was unreachable, busy, or dropped the connection.
	Transient(String),
	/// The node can never answer this request, so the lookup stops.
	Permanent(String),
}

/// Where blobs are fetched from, by CID. The seam the subscription logic is tested through.
#[async_trait::async_trait]
pub trait BlobSource: Send + Sync {
	/// The blob stored under `cid`, or `None` when the source does not hold it.
	async fn get(&self, cid: &BulletinCid) -> Result<Option<Vec<u8>>, SourceError>;
}

/// `bitswap_v1_get` on the Bulletin chain's JSON-RPC. The connection is opened on first use and
/// dropped after a transport failure, so the next call reconnects.
pub struct BitswapRpc {
	chains: Arc<CliChainProvider>,
	bulletin_genesis: [u8; 32],
	client: AsyncMutex<Option<RpcClient>>,
}

impl BitswapRpc {
	/// A source for the Bulletin chain `chains` routes `bulletin_genesis` to. Nothing connects
	/// before the first lookup.
	pub fn new(chains: Arc<CliChainProvider>, bulletin_genesis: [u8; 32]) -> Self {
		Self { chains, bulletin_genesis, client: AsyncMutex::new(None) }
	}

	async fn client(&self) -> Result<RpcClient, SourceError> {
		let mut slot = self.client.lock().await;
		if let Some(client) = slot.as_ref() {
			return Ok(client.clone());
		}
		let location = self.chains.location(self.bulletin_genesis);
		let client =
			tokio::time::timeout(RPC_TIMEOUT, self.chains.rpc_client(self.bulletin_genesis))
				.await
				.map_err(|_| {
					SourceError::Transient(format!("connecting to Bulletin ({location}) timed out"))
				})?
				.map_err(|err| {
					SourceError::Transient(format!("connecting to Bulletin ({location}): {err}"))
				})?;
		*slot = Some(client.clone());
		Ok(client)
	}

	async fn disconnect(&self) {
		*self.client.lock().await = None;
	}
}

#[async_trait::async_trait]
impl BlobSource for BitswapRpc {
	async fn get(&self, cid: &BulletinCid) -> Result<Option<Vec<u8>>, SourceError> {
		let client = self.client().await?;
		match tokio::time::timeout(RPC_TIMEOUT, bitswap_get(&client, cid)).await {
			Ok(Ok(blob)) => Ok(blob),
			Ok(Err(BitswapError::Unanswerable(reason))) => Err(SourceError::Permanent(reason)),
			Ok(Err(BitswapError::MethodMissing(reason))) => {
				// A load-balanced endpoint may put the next connection on a node that has it.
				self.disconnect().await;
				Err(SourceError::Permanent(reason))
			},
			Ok(Err(BitswapError::Connection(reason))) => {
				self.disconnect().await;
				Err(SourceError::Transient(reason))
			},
			Ok(Err(BitswapError::Failed(reason))) => Err(SourceError::Transient(reason)),
			Err(_) => {
				self.disconnect().await;
				Err(SourceError::Transient("bitswap_v1_get timed out".to_string()))
			},
		}
	}
}

/// Lookups over one blob source: the CID for each key, and the check that what comes back
/// hashes to it.
pub struct BulletinLookup<S> {
	source: S,
	poll_interval: Duration,
}

impl<S: BlobSource + 'static> BulletinLookup<S> {
	/// Lookups over `source` that ask again every [`POLL_INTERVAL`] after a miss.
	pub fn new(source: S) -> Self {
		Self::with_poll_interval(source, POLL_INTERVAL)
	}

	fn with_poll_interval(source: S, poll_interval: Duration) -> Self {
		Self { source, poll_interval }
	}

	/// The stream `PreimageHost::lookup_preimage` returns for `key`: the current answer at once,
	/// then the value once it can be read, then the end.
	pub fn subscribe(
		self: &Arc<Self>,
		key: [u8; 32],
	) -> BoxStream<'static, Result<Option<Vec<u8>>, api::GenericError>> {
		let lookup = Arc::clone(self);
		async move {
			let cid = BulletinCid::from(key);
			match lookup.read(&cid).await {
				Ok(Some(value)) => return stream::once(future::ready(Ok(Some(value)))).boxed(),
				Ok(None) => {},
				Err(SourceError::Transient(reason)) => {
					warn!(%cid, %reason, "preimage lookup failed, still trying")
				},
				Err(SourceError::Permanent(reason)) => {
					return stream::iter([Ok(None), Err(api::GenericError { reason })]).boxed();
				},
			}
			let held = lookup.until_held(cid).map(|held| held.map(Some));
			stream::once(future::ready(Ok(None))).chain(stream::once(held)).boxed()
		}
		.flatten_stream()
		.boxed()
	}

	/// Asks again every poll interval until the source holds the blob, or fails once it never
	/// can.
	async fn until_held(self: Arc<Self>, cid: BulletinCid) -> Result<Vec<u8>, api::GenericError> {
		loop {
			tokio::time::sleep(self.poll_interval).await;
			match self.read(&cid).await {
				Ok(Some(value)) => return Ok(value),
				Ok(None) => {},
				Err(SourceError::Transient(reason)) => {
					debug!(%cid, %reason, "preimage lookup failed, still trying")
				},
				Err(SourceError::Permanent(reason)) => return Err(api::GenericError { reason }),
			}
		}
	}

	/// One read: the blob if the source holds it and it hashes to `cid`.
	async fn read(&self, cid: &BulletinCid) -> Result<Option<Vec<u8>>, SourceError> {
		let Some(value) = self.source.get(cid).await? else {
			return Ok(None);
		};
		if !cid.matches(&value) {
			warn!(%cid, "preimage source returned bytes that do not hash to the CID");
			return Ok(None);
		}
		info!(%cid, size = value.len(), "preimage read from Bulletin");
		Ok(Some(value))
	}
}

#[cfg(test)]
mod tests;
