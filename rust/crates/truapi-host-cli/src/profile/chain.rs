//! The active identity's dotNS profile record, written as its `peopl.<tld>` product writes it.

use alloy_primitives::B256;
use alloy_sol_types::{SolCall, sol};
use anyhow::{Context, Result, anyhow, ensure};
use async_trait::async_trait;
use subxt::tx::Signer;
use truapi::{
	CallContext, HostAdmin, SigningHostRuntime, Sr25519Signer,
	api::{Preimage, ResourceAllocation},
	chain::{EvmContractCaller, PreparedEvmTransaction},
	host_logic::{
		dotns_gateway::{
			ContentResolver, DotnsPopController, DotnsTransport, ProtocolRegistry, account_to_h160,
			discover_pop_controller, namehash_under,
		},
		product_account::{derive_identity_keypair, personhood_product_id},
	},
	latest,
	platform::ProductContext,
	versioned::{
		FromLatest, IntoLatest, Versioned, preimage::RemotePreimageSubmitRequest,
		resource_allocation::HostRequestResourceAllocationRequest,
	},
};

use super::ProfileBackend;
use crate::{asset_hub_contracts, dotns_read::AssetHubReader, network::Stack};

/// Text record the profile is published at.
const PROFILE_RECORD_KEY: &str = "profile";

sol! {
	interface NameRegistry {
		function owner(bytes32 node) external view returns (address);
	}

	interface TextResolver {
		function setText(bytes32 node, string key, string value) external;
	}
}

/// The chain resources one profile publication uses.
pub struct ChainProfile {
	product: HostAdmin,
	reader: AssetHubReader,
	node: [u8; 32],
	resolver: [u8; 20],
	signer: Sr25519Signer,
	contracts: EvmContractCaller,
}

impl ChainProfile {
	/// Finds the profile record and verifies the identity owns `username` before anything is
	/// allocated or uploaded.
	pub async fn connect(
		runtime: &SigningHostRuntime,
		network: &Stack,
		entropy: &[u8],
		username: &str,
	) -> Result<Self> {
		let signer = Sr25519Signer::from_keypair(&derive_identity_keypair(entropy, network.tld())?);
		let mut reader = AssetHubReader::connect(network).await?;
		let controller = discover_pop_controller(&mut reader)
			.await?
			.context("dotNS is not deployed on Asset Hub")?;
		let protocol = view(&mut reader, &controller, DotnsPopController::protocolRegistryCall {})
			.await?
			.into_array();
		let registry = protocol_component(&mut reader, &protocol, "registry").await?;
		let resolver = protocol_component(&mut reader, &protocol, "contentResolver").await?;
		let node = username
			.rsplit('.')
			.fold(namehash_under(&[0; 32], network.tld()), |parent, label| {
				namehash_under(&parent, label)
			});
		let owner =
			view(&mut reader, &registry, NameRegistry::ownerCall { node: node.into() }).await?;
		ensure!(
			owner.into_array() == account_to_h160(&signer.account_id().0),
			"the active identity does not own {username}"
		);
		Ok(Self {
			product: runtime
				.product_admin(ProductContext::new(personhood_product_id(network.tld()))),
			reader,
			node,
			resolver,
			signer,
			contracts: asset_hub_contracts(network),
		})
	}

	/// Borrows the signer and contract caller separately from the reader, so a prepared write
	/// survives the upload and the record check.
	pub fn backend(&mut self) -> ChainProfileBackend<'_> {
		ChainProfileBackend {
			product: &self.product,
			reader: &mut self.reader,
			node: self.node,
			resolver: self.resolver,
			signer: &self.signer,
			contracts: &self.contracts,
		}
	}
}

/// A [`ChainProfile`] split into what a publication reads and what its prepared write keeps.
pub struct ChainProfileBackend<'a> {
	product: &'a HostAdmin,
	reader: &'a mut AssetHubReader,
	node: [u8; 32],
	resolver: [u8; 20],
	signer: &'a Sr25519Signer,
	contracts: &'a EvmContractCaller,
}

#[async_trait]
impl<'a> ProfileBackend for ChainProfileBackend<'a> {
	type Prepared = PreparedEvmTransaction<'a>;

	async fn read(&mut self) -> Result<Option<String>> {
		let call =
			ContentResolver::textCall { node: self.node.into(), key: PROFILE_RECORD_KEY.into() };
		let text = view(self.reader, &self.resolver, call).await?;
		Ok((!text.is_empty()).then_some(text))
	}

	async fn prepare(&mut self, text: &str) -> Result<Self::Prepared> {
		let request = latest::HostRequestResourceAllocationRequest {
			resources: vec![latest::AllocatableResource::SmartContractAllowance(
				latest::DerivationIndex::Index(0),
			)],
		};
		let response = self
			.product
			.product_runtime()
			.request(
				&CallContext::default(),
				HostRequestResourceAllocationRequest::from_latest(
					request,
					HostRequestResourceAllocationRequest::LATEST,
				),
			)
			.await
			.map_err(|error| anyhow!("smart contract allowance failed: {error:?}"))?
			.into_latest();
		ensure!(
			response.outcomes == [latest::AllocationOutcome::Allocated],
			"smart contract allowance was not allocated: {:?}",
			response.outcomes
		);
		let call = TextResolver::setTextCall {
			node: self.node.into(),
			key: PROFILE_RECORD_KEY.into(),
			value: text.into(),
		};
		self.contracts
			.dry_run(self.signer, &self.resolver, &call.abi_encode())
			.await
			.context("dry-run the profile record update")
	}

	async fn upload(&mut self, bytes: Vec<u8>) -> Result<[u8; 32]> {
		let key = self
			.product
			.product_runtime()
			.submit(
				&CallContext::default(),
				RemotePreimageSubmitRequest::from_latest(
					bytes,
					RemotePreimageSubmitRequest::LATEST,
				),
			)
			.await
			.map_err(|error| anyhow!("avatar upload failed: {error:?}"))?
			.into_latest();
		<[u8; 32]>::try_from(key).map_err(|key| anyhow!("upload returned a {}-byte key", key.len()))
	}

	async fn publish(&mut self, prepared: Self::Prepared) -> Result<()> {
		prepared.execute_until_finalized().await.context("publish the profile record")
	}
}

async fn view<Call: SolCall>(
	reader: &mut AssetHubReader,
	contract: &[u8; 20],
	call: Call,
) -> Result<Call::Return> {
	let output = reader.view(contract, call.abi_encode()).await?;
	Ok(Call::abi_decode_returns_validate(&output)?)
}

async fn protocol_component(
	reader: &mut AssetHubReader,
	protocol: &[u8; 20],
	name: &str,
) -> Result<[u8; 20]> {
	let call = ProtocolRegistry::getCall { key: B256::right_padding_from(name.as_bytes()) };
	let address = view(reader, protocol, call).await?;
	ensure!(!address.is_zero(), "dotNS has no {name} contract");
	Ok(address.into_array())
}
