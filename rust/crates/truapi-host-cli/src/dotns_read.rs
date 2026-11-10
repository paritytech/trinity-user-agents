//! Asset Hub dotNS reads over plain RPC (`state_getStorage` / `state_call`).
//!
//! The transport half of username resolution. This module supplies the two RPC
//! primitives, over a connection from the CLI's chain provider.
//! `truapi::host_logic::dotns_gateway` walks the contract chain over them,
//! starting from the stack's `dotns.popController`. The signing host's core
//! instead discovers its controller on chain, so the two resolve through
//! different controllers when the stack and the chain disagree.

use anyhow::{Context, Result};
use serde_json::Value;
use subxt_rpcs::client::{RpcClient, rpc_params};
use truapi::{
	chain::{EvmContractCaller, EvmContractError},
	host_logic::dotns_gateway::{
		ClaimBlocker, DotnsError, DotnsIdentity, DotnsTransport, account_alias_key, claim_blocker,
		classify_labels, label_available, lite_label_owner_key, resolve_labels, timestamp_now_key,
	},
};

use crate::{chain::CliChainProvider, network::Stack};

/// One Asset Hub RPC connection for a batch of dotNS reads.
pub struct AssetHubReader {
	rpc: RpcClient,
	/// The stack's `DotnsPopController`.
	pop_controller: [u8; 20],
}

impl AssetHubReader {
	/// Connects to the network's Asset Hub.
	pub async fn connect(network: &Stack) -> Result<Self> {
		let chains = CliChainProvider::new(network);
		let location = chains.location(network.genesis().asset_hub);
		let rpc = chains
			.rpc_client(network.genesis().asset_hub)
			.await
			.with_context(|| format!("connect to Asset Hub ({location})"))?;
		Ok(Self { rpc, pop_controller: network.pop_controller() })
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
		let controller = self.pop_controller;
		let labels = resolve_labels(self, &controller, account).await?;
		Ok(classify_labels(self, &controller, labels).await?)
	}

	/// Whether the registrar would still mint `label` under the network TLD.
	/// `false` once the name is registered, whichever flow minted it.
	pub async fn label_available(&mut self, label: &str) -> Result<bool> {
		let controller = self.pop_controller;
		Ok(label_available(self, &controller, label).await?)
	}

	/// Why a gateway claim of `label` by `claimant` would revert on the name's
	/// reservation queue, or `None` when it can go through.
	pub async fn claim_blocker(
		&mut self,
		label: &str,
		claimant: &[u8; 20],
	) -> Result<Option<ClaimBlocker>> {
		let controller = self.pop_controller;
		let now = self.timestamp_secs().await?;
		claim_blocker(self, &controller, label, claimant, now)
			.await
			.with_context(|| format!("read the reservation queue of {label:?}"))
	}

	/// The stack's `DotnsPopController`.
	pub fn pop_controller(&self) -> [u8; 20] {
		self.pop_controller
	}

	/// One `state_getStorage` read at the best block.
	async fn raw_storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
		let value: Value = self
			.rpc
			.request("state_getStorage", rpc_params![format!("0x{}", hex::encode(key))])
			.await
			.context("rpc state_getStorage")?;
		value
			.as_str()
			.map(|hex_value| {
				hex::decode(hex_value.strip_prefix("0x").unwrap_or(hex_value))
					.context("storage value is not valid hex")
			})
			.transpose()
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

#[async_trait::async_trait]
impl DotnsTransport for AssetHubReader {
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		self.raw_storage(&key)
			.await
			.map_err(|err| DotnsError::Storage(format!("{err:#}")))
	}

	async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, EvmContractError> {
		EvmContractCaller::read(&mut self.rpc, dest, &input).await
	}
}
