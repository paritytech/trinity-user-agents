//! The CLI host's Bulletin connection for preimage lookups. The lookup itself, polling a miss
//! and checking every value against its key, is the core's [`truapi::BulletinLookup`].

use std::{sync::Arc, time::Duration};

use subxt_rpcs::client::RpcClient;
use tokio::sync::Mutex as AsyncMutex;
use truapi::{BlobSource, SourceError, bitswap_answer, bitswap_request};

use crate::chain::CliChainProvider;

/// Bound on one round trip to the node.
const RPC_TIMEOUT: Duration = Duration::from_secs(10);
/// The JSON-RPC code of a node without `bitswap_v1_get`.
const METHOD_NOT_FOUND: i32 = -32601;

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
	async fn get(&self, cid: &str) -> Result<Option<Vec<u8>>, SourceError> {
		let client = self.client().await?;
		let Ok(result) = tokio::time::timeout(RPC_TIMEOUT, bitswap_request(&client, cid)).await
		else {
			self.disconnect().await;
			return Err(SourceError::Transient("bitswap_v1_get timed out".to_string()));
		};
		// A load-balanced endpoint may put the next connection on a node that has it.
		let reconnect = match &result {
			Ok(_) => false,
			Err(subxt_rpcs::Error::User(error)) => error.code == METHOD_NOT_FOUND,
			Err(_) => true,
		};
		if reconnect {
			self.disconnect().await;
		}
		bitswap_answer(result)
	}
}
