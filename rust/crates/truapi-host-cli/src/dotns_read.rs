//! Asset Hub dotNS reads over plain RPC (`state_getStorage` / `state_call`).
//!
//! The transport half of username resolution. This module supplies the two RPC
//! primitives, over a connection from the CLI's chain provider.
//! `truapi::host_logic::dotns_gateway` walks the contract
//! chain over them. The CLI and the in-core `chainHead_v1` lookup therefore
//! resolve identically.

use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use subxt_rpcs::client::{RpcClient, RpcParams, rpc_params};
use truapi::{
	host_logic::dotns_gateway::{
		DotnsError, DotnsIdentity, DotnsTransport, DotnsViewError, VIEW_CALL_ORIGIN,
		account_alias_key, classify_labels, discover_pop_controller, encode_revive_call,
		label_available, lite_label_owner_key, resolve_labels, timestamp_now_key, view_output,
	},
	platform::async_trait,
};

use crate::{chain::CliChainProvider, network::NetworkConfig};

/// Env var overriding the `DotnsPopController` H160 (hex), skipping on-chain
/// discovery.
///
/// Discovery reads `DotnsGateway.DispatcherAddress`, which holds either a
/// dispatcher or the controller itself, and resolves whichever it is; the
/// override is for networks where that fails. The
/// controller is `0xCC932348606cc1f3318cADeC5A5Cd2CA447f8a4b` on paseo-next-v2
/// and previewnet; the polkadot preset carries its own from its deployment
/// manifest. `DEPLOYMENTS.md` in paritytech/dotns is the authority per network.
pub const DOTNS_POP_CONTROLLER_ENV: &str = "HOST_CLI_DOTNS_POP_CONTROLLER";

/// How long one read waits for its answer. Provider connections carry no
/// request timeout, and keepalive pings hold open a node that never answers.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// One Asset Hub RPC connection for a batch of dotNS reads.
pub struct AssetHubReader {
	rpc: RpcClient,
	/// `DotnsPopController` once resolved; it does not change within a run.
	pop_controller: Option<[u8; 20]>,
	/// The preset's controller, used only when discovery finds none.
	pop_controller_fallback: Option<[u8; 20]>,
}

impl AssetHubReader {
	/// Connects to the network's Asset Hub.
	pub async fn connect(network: &NetworkConfig) -> Result<Self> {
		let rpc = CliChainProvider::new(network)
			.rpc_client(network.asset_hub_genesis)
			.await
			.context("connect to Asset Hub")?;
		Ok(Self { rpc, pop_controller: None, pop_controller_fallback: None })
	}

	/// Connects to the network's Asset Hub, falling back to its known
	/// `DotnsPopController` when the chain does not name one.
	pub async fn for_network(network: &NetworkConfig) -> Result<Self> {
		let mut reader = Self::connect(network).await?;
		reader.pop_controller_fallback = network.dotns_pop_controller;
		Ok(reader)
	}

	/// Asset Hub chain time in Unix seconds. `Timestamp.Now` itself is
	/// milliseconds.
	pub async fn timestamp_secs(&self) -> Result<u64> {
		let value = self
			.raw_storage(&timestamp_now_key())
			.await?
			.context("Timestamp.Now is unset")?;
		let millis: [u8; 8] = value
			.as_slice()
			.try_into()
			.map_err(|_| anyhow::anyhow!("Timestamp.Now is not a u64"))?;
		Ok(u64::from_le_bytes(millis) / 1000)
	}

	/// Alias `account` registered with, per `DotnsGateway.AccountAlias`.
	pub async fn account_alias(&self, account: &[u8; 32]) -> Result<Option<[u8; 32]>> {
		let value = self.raw_storage(&account_alias_key(account)).await?;
		account_id_value("DotnsGateway.AccountAlias", value)
	}

	/// Account that reserved the dotted lite username `lite_label` through the
	/// gateway, per `DotnsGateway.LiteLabelOwner`.
	pub async fn lite_label_owner(&self, lite_label: &str) -> Result<Option<[u8; 32]>> {
		let value = self.raw_storage(&lite_label_owner_key(lite_label.as_bytes())).await?;
		account_id_value("DotnsGateway.LiteLabelOwner", value)
	}

	/// Usernames of `account` as recorded by the dotNS contracts, or `None`
	/// when none of its labels is a username.
	pub async fn dotns_identity(&mut self, account: &[u8; 32]) -> Result<Option<DotnsIdentity>> {
		let controller = self.pop_controller().await?;
		let labels = resolve_labels(self, &controller, account).await?;
		Ok(classify_labels(self, &controller, labels).await?)
	}

	/// Whether the registrar would still mint `label` under the network TLD.
	/// `false` once the name is registered, whichever flow minted it.
	pub async fn label_available(&mut self, label: &str) -> Result<bool> {
		let controller = self.pop_controller().await?;
		Ok(label_available(self, &controller, label).await?)
	}

	/// `DotnsPopController` address, resolved once per reader. The env override
	/// wins, then on-chain discovery from `DotnsGateway.DispatcherAddress`, then
	/// the preset's own controller.
	async fn pop_controller(&mut self) -> Result<[u8; 20]> {
		if let Some(controller) = self.pop_controller {
			return Ok(controller);
		}
		let controller = self.resolve_pop_controller().await?;
		self.pop_controller = Some(controller);
		Ok(controller)
	}

	async fn resolve_pop_controller(&mut self) -> Result<[u8; 20]> {
		if let Ok(value) = std::env::var(DOTNS_POP_CONTROLLER_ENV) {
			let trimmed = value.trim();
			if !trimmed.is_empty() {
				let bytes = hex::decode(trimmed.strip_prefix("0x").unwrap_or(trimmed))
					.with_context(|| format!("{DOTNS_POP_CONTROLLER_ENV} is not valid hex"))?;
				return bytes.try_into().map_err(|bytes: Vec<u8>| {
					anyhow::anyhow!(
						"{DOTNS_POP_CONTROLLER_ENV} must be 20 bytes, got {}",
						bytes.len()
					)
				});
			}
		}
		discover_pop_controller(self)
			.await?
			.or(self.pop_controller_fallback)
			.with_context(|| {
				format!(
					"dotNS gateway has no reachable controller; set \
                     {DOTNS_POP_CONTROLLER_ENV} to the DotnsPopController H160"
				)
			})
	}

	/// Dry-runs a contract view via the `ReviveApi_call` runtime API and returns
	/// its data.
	///
	/// Views originate from the synthetic always-mapped account. They work
	/// regardless of the queried account's revive mapping.
	async fn raw_view(&self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, DotnsViewError> {
		let args = encode_revive_call(&VIEW_CALL_ORIGIN, dest, &input);
		let output = self
			.request(
				"state_call",
				rpc_params!["ReviveApi_call", format!("0x{}", hex::encode(args))],
			)
			.await
			.map_err(|err| DotnsViewError::Failed(format!("ReviveApi_call: {err:#}")))?;
		let Some(output) = output.as_str() else {
			return Err(DotnsViewError::Failed(
				"state_call returned a non-string response".to_string(),
			));
		};
		let bytes = hex::decode(output.strip_prefix("0x").unwrap_or(output)).map_err(|err| {
			DotnsViewError::Failed(format!("state_call output is not valid hex: {err}"))
		})?;
		view_output(&bytes)
	}

	/// One `state_getStorage` read at the best block.
	async fn raw_storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
		let value = self
			.request("state_getStorage", rpc_params![format!("0x{}", hex::encode(key))])
			.await?;
		value
			.as_str()
			.map(|hex_value| {
				hex::decode(hex_value.strip_prefix("0x").unwrap_or(hex_value))
					.context("storage value is not valid hex")
			})
			.transpose()
	}

	async fn request(&self, method: &str, params: RpcParams) -> Result<Value> {
		tokio::time::timeout(REQUEST_TIMEOUT, self.rpc.request(method, params))
			.await
			.map_err(|_| {
				anyhow::anyhow!("rpc {method}: no answer within {}s", REQUEST_TIMEOUT.as_secs())
			})?
			.with_context(|| format!("rpc {method}"))
	}
}

/// Decodes an optional 32-byte account id storage value. A value of another
/// length is an error, not "absent": treating it as absent would let a
/// registration proceed that the gateway then rejects.
fn account_id_value(entry: &str, value: Option<Vec<u8>>) -> Result<Option<[u8; 32]>> {
	value
		.map(|bytes| {
			let len = bytes.len();
			<[u8; 32]>::try_from(bytes)
				.map_err(|_| anyhow::anyhow!("{entry} value is {len} bytes, expected 32"))
		})
		.transpose()
}

#[async_trait]
impl DotnsTransport for AssetHubReader {
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		self.raw_storage(&key)
			.await
			.map_err(|err| DotnsError::Storage(format!("{err:#}")))
	}

	async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, DotnsViewError> {
		self.raw_view(dest, input).await
	}
}

#[cfg(test)]
mod tests {
	use subxt_rpcs::client::{RawRpcFuture, RawRpcSubscription, RawValue, RpcClientT};

	use super::*;

	/// A node that holds every request open, as one that lost the request does.
	struct NeverAnswers;

	impl RpcClientT for NeverAnswers {
		fn request_raw<'a>(
			&'a self,
			_method: &'a str,
			_params: Option<Box<RawValue>>,
		) -> RawRpcFuture<'a, Box<RawValue>> {
			Box::pin(futures::future::pending())
		}

		fn subscribe_raw<'a>(
			&'a self,
			_sub: &'a str,
			_params: Option<Box<RawValue>>,
			_unsub: &'a str,
		) -> RawRpcFuture<'a, RawRpcSubscription> {
			Box::pin(futures::future::pending())
		}
	}

	/// Keepalive pings hold such a connection open, so without a request timeout
	/// a username lookup would wait forever rather than fail.
	#[tokio::test(start_paused = true)]
	async fn a_read_the_node_never_answers_fails() {
		let reader = AssetHubReader {
			rpc: RpcClient::new(NeverAnswers),
			pop_controller: None,
			pop_controller_fallback: None,
		};
		let read = tokio::time::timeout(REQUEST_TIMEOUT * 2, reader.raw_storage(&[0; 32]))
			.await
			.expect("the read gives up on its own");
		assert_eq!(read.unwrap_err().to_string(), "rpc state_getStorage: no answer within 60s");
	}
}
