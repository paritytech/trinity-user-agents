//! The CLI host's Bulletin connection for preimage lookups. The lookup itself, polling a miss
//! and checking every value against its key, is the core's [`truapi::BulletinLookup`].

use std::{sync::Arc, time::Duration};

use subxt_rpcs::client::RpcClient;
use tokio::sync::Mutex as AsyncMutex;
use truapi::{BlobSource, SourceError, bitswap_get};

use crate::chain::CliChainProvider;

/// Bound on one round trip to the node.
const RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// `bitswap_v1_get` on the Bulletin chain's JSON-RPC. The connection is opened on first use and
/// dropped after a failure, so the next call reconnects, possibly to a load-balanced node that
/// can answer.
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
}

#[async_trait::async_trait]
impl BlobSource for BitswapRpc {
	async fn get(&self, cid: &str) -> Result<Option<Vec<u8>>, SourceError> {
		let client = self.client().await?;
		let answer = tokio::time::timeout(RPC_TIMEOUT, bitswap_get(&client, cid))
			.await
			.unwrap_or_else(|_| {
				Err(SourceError::Transient("bitswap_v1_get timed out".to_string()))
			});
		if answer.is_err() {
			*self.client.lock().await = None;
		}
		answer
	}
}
