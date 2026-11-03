use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use subxt::{tx::Signer, utils::H256};
use truapi::{
	ImageFormat, ProfileRecord, ProfileRecordLocation, SigningHostRuntime, Sr25519Signer,
	chain::{EvmContractCaller, ExtrinsicService, PreparedEvmTransaction},
	host_logic::{dotns_gateway::account_to_h160, product_account::derive_identity_keypair},
};

use super::{ProfileBackend, ProfilePublisher};
use crate::{
	chain::{PresetExtensionVersions, chain_runtime},
	dotns_read::AssetHubReader,
	network::NetworkConfig,
};

/// Publishes the active identity's profile through its network's contracts.
pub async fn publish(
	runtime: &SigningHostRuntime,
	network: NetworkConfig,
	entropy: &[u8],
	username: &str,
	avatar: (ImageFormat, Vec<u8>),
) -> Result<ProfileRecord> {
	let mut profile = ChainProfile::connect(runtime, network, entropy, username).await?;
	ProfilePublisher { backend: profile.backend() }.publish(avatar).await
}

struct ChainProfile<'a> {
	runtime: &'a SigningHostRuntime,
	reader: AssetHubReader,
	location: ProfileRecordLocation,
	signer: Sr25519Signer,
	contracts: EvmContractCaller,
}

impl<'a> ChainProfile<'a> {
	/// Resolves the profile and verifies ownership before allocating or uploading.
	async fn connect(
		runtime: &'a SigningHostRuntime,
		network: NetworkConfig,
		entropy: &[u8],
		username: &str,
	) -> Result<Self> {
		let identity = derive_identity_keypair(entropy, network.network_suffix)?;
		let signer = Sr25519Signer::from_keypair(&identity);
		let chain = chain_runtime(&network);
		let rpc = chain.rpc_client("public profile", &network.asset_hub_genesis).await?;
		let mut reader = AssetHubReader::from_rpc(subxt_rpcs::RpcClient::new(rpc), &network);
		let location = ProfileRecordLocation::resolve(&mut reader, username)
			.await?
			.context("dotNS has no content resolver for public profiles")?;
		ensure!(
			location.owner(&mut reader).await? == account_to_h160(&signer.account_id().0),
			"active identity does not own this username"
		);
		let extrinsics = ExtrinsicService::over_chain(
			chain.clone(),
			Arc::new(PresetExtensionVersions(network.extension_versions())),
		);
		let contracts = EvmContractCaller::new(chain, extrinsics, H256(network.asset_hub_genesis));
		Ok(Self { runtime, reader, location, signer, contracts })
	}

	/// Borrows the signing resources so a prepared write survives the upload and record check.
	fn backend(&mut self) -> impl ProfileBackend + '_ {
		ChainProfileBackend {
			runtime: self.runtime,
			reader: &mut self.reader,
			location: &self.location,
			signer: &self.signer,
			contracts: &self.contracts,
		}
	}
}

struct ChainProfileBackend<'a> {
	runtime: &'a SigningHostRuntime,
	reader: &'a mut AssetHubReader,
	location: &'a ProfileRecordLocation,
	signer: &'a Sr25519Signer,
	contracts: &'a EvmContractCaller,
}

#[async_trait::async_trait]
impl<'a> ProfileBackend for ChainProfileBackend<'a> {
	type Prepared = PreparedEvmTransaction<'a>;

	async fn read(&mut self) -> Result<Option<String>> {
		Ok(self.location.read(self.reader).await?)
	}

	async fn prepare(&mut self, record: &ProfileRecord) -> Result<Self::Prepared> {
		self.runtime
			.ensure_public_profile_allowance()
			.await
			.map_err(|error| anyhow::anyhow!(error.reason))?;
		let call = self
			.location
			.set_record_call(record)
			.context("profile record exceeds the 16 KiB limit")?;
		self.contracts
			.dry_run(self.signer, &self.location.resolver, &call)
			.await
			.context("dry-run the profile record update")
	}

	async fn upload(&mut self, bytes: Vec<u8>) -> Result<[u8; 32]> {
		self.runtime
			.submit_public_profile_avatar(bytes)
			.await
			.map_err(|error| anyhow::anyhow!(error.reason))
	}

	async fn publish(&mut self, prepared: Self::Prepared, record: &ProfileRecord) -> Result<()> {
		prepared.execute_until_finalized().await.context("publish the profile record")?;
		let published = self.location.read(self.reader).await?;
		let published = published.as_deref().and_then(ProfileRecord::from_text).unwrap_or_default();
		ensure!(published == *record, "published profile does not match the submitted record");
		Ok(())
	}
}
