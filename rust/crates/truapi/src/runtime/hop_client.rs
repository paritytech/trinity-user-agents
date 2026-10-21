//! A client of the HOP pool of the person chat: submit, claim, ack, and the
//! Bulletin fallback for entries promoted on chain, against one node. The
//! wire calls are those of the iOS and Android apps, with positional params.

use std::sync::Arc;

use serde_json::{Value, json};
use subxt::{config::substrate::SubstrateConfig, tx::Signer};

use crate::{
	host_internal::extrinsic::Sr25519Signer,
	host_logic::{
		hop::{
			self, HopPoolEntryV1, TicketKeys, ack_payload, claim_payload, entry_identifier,
			submit_payload,
		},
		product_account::SR25519_SIGNING_CONTEXT,
	},
};

/// `hop_claim` for an entry the node does not hold.
const NOT_FOUND: i32 = 1004;
/// `bitswap_v1_get` for a CID Bulletin does not hold, and for a malformed one.
const BITSWAP_NOT_FOUND: i32 = -32810;
const BITSWAP_INVALID_CID: i32 = -32602;

/// Why a call to a node failed.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum HopCallError {
	/// The node answered with a JSON-RPC error.
	#[display("{message} ({code})")]
	Rpc {
		/// The JSON-RPC error code.
		code: i32,
		/// Its message.
		message: String,
	},
	/// The node could not be reached or answered something unreadable.
	#[display("{_0}")]
	Transport(String),
}

/// One JSON-RPC connection to a HOP node.
#[async_trait::async_trait]
pub trait HopTransport: Send + Sync {
	/// Calls `method` with positional `params`.
	async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value, HopCallError>;
}

/// Opens connections to HOP nodes by URL.
#[async_trait::async_trait]
pub trait HopConnector: Send + Sync {
	/// A connection to the node at `url`.
	async fn connect(&self, url: &str) -> Result<Arc<dyn HopTransport>, HopCallError>;
}

/// How long a node may take to accept a connection.
#[cfg(not(target_arch = "wasm32"))]
const CONNECT_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(20);

/// How long a node may take to answer one call, a 2 MB submit included.
#[cfg(not(target_arch = "wasm32"))]
const CALL_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(60);

/// `future`, or a transport error once `timeout` passes, so a node that stops
/// answering fails the call instead of holding it.
#[cfg(not(target_arch = "wasm32"))]
async fn bounded<T>(
	future: impl core::future::Future<Output = Result<T, HopCallError>>,
	timeout: core::time::Duration,
	what: &str,
) -> Result<T, HopCallError> {
	use futures::FutureExt;
	futures::select! {
		result = future.fuse() => result,
		_ = futures_timer::Delay::new(timeout).fuse() => {
			Err(HopCallError::Transport(format!("{what} took longer than {timeout:?}")))
		},
	}
}

/// Dials the node directly, over TLS for `wss://`. A browser cannot dial from
/// Rust, so wasm hosts have no connector and the HOP calls are `Unsupported`.
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeHopConnector;

#[cfg(not(target_arch = "wasm32"))]
struct NativeHopTransport(subxt_rpcs::RpcClient);

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl HopConnector for NativeHopConnector {
	async fn connect(&self, url: &str) -> Result<Arc<dyn HopTransport>, HopCallError> {
		let connect = async {
			if url.starts_with("wss://") {
				subxt_rpcs::client::RpcClient::from_url(url).await
			} else {
				subxt_rpcs::client::RpcClient::from_insecure_url(url).await
			}
			.map_err(|error| HopCallError::Transport(format!("cannot reach {url}: {error}")))
		};
		let client = bounded(connect, CONNECT_TIMEOUT, &format!("connecting to {url}")).await?;
		Ok(Arc::new(NativeHopTransport(client)))
	}
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl HopTransport for NativeHopTransport {
	async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value, HopCallError> {
		let mut rpc_params = subxt_rpcs::client::RpcParams::new();
		for param in params {
			rpc_params
				.push(param)
				.map_err(|error| HopCallError::Transport(error.to_string()))?;
		}
		let request = async {
			self.0.request::<Value>(method, rpc_params).await.map_err(|error| match error {
				subxt_rpcs::Error::User(user) => {
					HopCallError::Rpc { code: user.code, message: user.message }
				},
				other => HopCallError::Transport(other.to_string()),
			})
		};
		bounded(request, CALL_TIMEOUT, method).await
	}
}

fn multi_signer(public: &[u8; 32]) -> String {
	format!("0x01{}", hex::encode(public))
}

fn multi_signature(signature: &[u8; 64]) -> String {
	format!("0x01{}", hex::encode(signature))
}

fn hex_bytes(value: &Value) -> Result<Vec<u8>, HopCallError> {
	let text = value
		.as_str()
		.ok_or_else(|| HopCallError::Transport(format!("expected hex, got {value}")))?;
	hex::decode(text.strip_prefix("0x").unwrap_or(text))
		.map_err(|error| HopCallError::Transport(format!("expected hex: {error}")))
}

/// The HOP calls against one node.
pub struct HopClient {
	transport: Arc<dyn HopTransport>,
}

impl HopClient {
	/// A client over `transport`.
	pub fn new(transport: Arc<dyn HopTransport>) -> Self {
		Self { transport }
	}

	/// Submits `entry` for the holder of `recipient`, signed by `signer`.
	pub async fn submit(
		&self,
		entry: &[u8],
		recipient: &[u8; 32],
		signer: &Sr25519Signer,
		submit_timestamp_ms: u64,
	) -> Result<(), HopCallError> {
		let subxt::utils::MultiSignature::Sr25519(signature) =
			Signer::<SubstrateConfig>::sign(signer, &submit_payload(entry, submit_timestamp_ms))
		else {
			return Err(HopCallError::Transport("the allowance signer is not sr25519".into()));
		};
		let signer_public = Signer::<SubstrateConfig>::account_id(signer).0;
		self.transport
			.call(
				"hop_submit",
				vec![
					json!(format!("0x{}", hex::encode(entry))),
					json!([multi_signer(recipient)]),
					json!(multi_signature(&signature)),
					json!(multi_signer(&signer_public)),
					json!(submit_timestamp_ms),
				],
			)
			.await
			.map(|_| ())
	}

	/// The entry `identifier`, or `None` when the node no longer holds it.
	pub async fn claim(
		&self,
		identifier: &[u8; 32],
		keys: &TicketKeys,
	) -> Result<Option<Vec<u8>>, HopCallError> {
		let signature = keys
			.signer
			.sign_simple(SR25519_SIGNING_CONTEXT, &claim_payload(identifier))
			.to_bytes();
		match self
			.transport
			.call(
				"hop_claim",
				vec![
					json!(format!("0x{}", hex::encode(identifier))),
					json!(multi_signature(&signature)),
				],
			)
			.await
		{
			Ok(value) => hex_bytes(&value).map(Some),
			Err(HopCallError::Rpc { code: NOT_FOUND, .. }) => Ok(None),
			Err(error) => Err(error),
		}
	}

	/// Tells the node the entry was taken. A node that no longer holds it has nothing to drop.
	pub async fn ack(&self, identifier: &[u8; 32], keys: &TicketKeys) -> Result<(), HopCallError> {
		let signature = keys
			.signer
			.sign_simple(SR25519_SIGNING_CONTEXT, &ack_payload(identifier))
			.to_bytes();
		match self
			.transport
			.call(
				"hop_ack",
				vec![
					json!(format!("0x{}", hex::encode(identifier))),
					json!(multi_signature(&signature)),
				],
			)
			.await
		{
			Ok(_) | Err(HopCallError::Rpc { code: NOT_FOUND, .. }) => Ok(()),
			Err(error) => Err(error),
		}
	}

	/// The entry as Bulletin keeps it after promotion, checked against its identifier.
	pub async fn bitswap(&self, identifier: &[u8; 32]) -> Result<Option<Vec<u8>>, HopCallError> {
		match self
			.transport
			.call("bitswap_v1_get", vec![json!(hop::bitswap_cid(identifier))])
			.await
		{
			Ok(value) => {
				let bytes = hex_bytes(&value)?;
				Ok((entry_identifier(&bytes) == *identifier).then_some(bytes))
			},
			Err(HopCallError::Rpc { code: BITSWAP_NOT_FOUND | BITSWAP_INVALID_CID, .. }) => {
				Ok(None)
			},
			Err(error) => Err(error),
		}
	}

	/// The entry from the pool, or from Bulletin when the pool no longer holds it.
	/// The second value says whether it came from the pool, which is what an ack drops.
	async fn fetch(
		&self,
		identifier: &[u8; 32],
		keys: &TicketKeys,
	) -> Result<(Vec<u8>, bool), HopCallError> {
		if let Some(entry) = self.claim(identifier, keys).await? {
			return Ok((entry, true));
		}
		self.bitswap(identifier)
			.await?
			.map(|entry| (entry, false))
			.ok_or(HopCallError::Rpc {
				code: NOT_FOUND,
				message: "the entry is neither in the pool nor on Bulletin".into(),
			})
	}

	/// Uploads `data` as one file under `ticket`: chunk entries first, then the root.
	/// Returns the identifier of the root.
	pub async fn upload(
		&self,
		ticket: &[u8; 32],
		data: &[u8],
		signer: &Sr25519Signer,
		now_ms: u64,
	) -> Result<[u8; 32], HopCallError> {
		let prepared = hop::prepare_upload(ticket, data).map_err(HopCallError::Transport)?;
		let recipient = hop::ticket_keys(ticket).signer.public.to_bytes();
		for chunk in &prepared.chunks {
			self.submit(chunk, &recipient, signer, now_ms).await?;
		}
		self.submit(&prepared.root, &recipient, signer, now_ms).await?;
		Ok(prepared.identifier())
	}

	/// Downloads and opens the file `identifier` under `ticket`. Returns the
	/// file and the identifiers of the pool entries an ack should drop.
	pub async fn download(
		&self,
		identifier: &[u8; 32],
		ticket: &[u8; 32],
	) -> Result<(Vec<u8>, Vec<[u8; 32]>), HopCallError> {
		let keys = hop::ticket_keys(ticket);
		let (root, root_in_pool) = self.fetch(identifier, &keys).await?;
		let mut in_pool = Vec::new();
		if root_in_pool {
			in_pool.push(*identifier);
		}
		match hop::open_root(ticket, &root).map_err(HopCallError::Transport)? {
			HopPoolEntryV1::Inline(data) => Ok((data, in_pool)),
			HopPoolEntryV1::Chunked { total_size, chunks } => {
				let mut data = Vec::with_capacity(total_size as usize);
				for chunk in chunks {
					let chunk: [u8; 32] = chunk.try_into().map_err(|_| {
						HopCallError::Transport("a chunk identifier is not 32 bytes".into())
					})?;
					let (entry, chunk_in_pool) = self.fetch(&chunk, &keys).await?;
					if chunk_in_pool {
						in_pool.push(chunk);
					}
					data.extend(hop::open_chunk(ticket, &entry).map_err(HopCallError::Transport)?);
				}
				if data.len() as u64 != total_size {
					return Err(HopCallError::Transport(
						"the chunks do not add up to the file".into(),
					));
				}
				Ok((data, in_pool))
			},
		}
	}
}
