/// Leave the product runtime one minute to receive and process the SSO response
/// before its 300-second remote-authority deadline expires.
const BULLETIN_AUTHORIZATION_WAIT: std::time::Duration = std::time::Duration::from_secs(240);

/// Failure while deriving or allocating a Statement Store/Bulletin allowance.
#[derive(Debug, thiserror::Error)]
pub enum AllowanceAllocationError {
	/// Signing host session or authority state was unavailable.
	#[error("{0}")]
	Authority(#[from] AuthorityError),
	/// The host serves no chain for this role, so there is nothing to claim on.
	#[error("host serves no {chain} chain")]
	ChainNotServed {
		/// Role that could not be resolved.
		chain: &'static str,
	},
	/// Reading the host's chain set failed.
	#[error("supported chains: {0}")]
	SupportedChains(String),
	/// Product-account key derivation failed.
	#[error("{0}")]
	ProductAccount(#[from] ProductAccountError),
	/// Chain state, metadata, ring, slot, proof, or extrinsic allocation failed.
	#[error("{0}")]
	StatementAllowance(#[from] StatementAllowanceError),
	/// Runtime service could not open the required Statement Store RPC client.
	#[error("{0}")]
	StatementStoreRpcClient(#[from] StatementStoreRpcClientError),
	/// Runtime service could not open the required Bulletin RPC client.
	#[error("{context}: {source}")]
	ChainRpcClient {
		/// Client context, naming which chain failed.
		context: &'static str,
		/// Chain runtime failure.
		#[source]
		source: RuntimeFailure,
	},
	/// System time cannot be converted into a UNIX timestamp.
	#[error("system clock before UNIX epoch")]
	SystemClockBeforeUnixEpoch,
	/// The signing account is not in any personhood ring.
	#[error("signing account is not a personhood ring member; cannot grant {resource} allowance")]
	MissingPersonhoodMembership {
		/// Resource name.
		resource: &'static str,
	},
}

impl AllowanceAllocationError {
	pub fn into_authority_error(self) -> AuthorityError {
		match self {
			Self::Authority(err) => err,
			other => AuthorityError::Unavailable { reason: other.to_string() },
		}
	}
}

/// A product's statement-store allowance key, and the period it holds a slot in.
pub struct StatementStoreAllocation {
	/// sr25519 secret of the product's allowance account.
	pub secret: Vec<u8>,
	/// Allowance period the slot was found or claimed in.
	pub period: u32,
}

impl WalletAccountHolder {
	/// Issue the product's Bulletin allowance key.
	pub async fn allocate_bulletin_allowance(
		services: &RuntimeServices,
		signing_host: &SigningHost,
		session: &AuthoritySession,
		product_id: &str,
		policy: OnExistingAllowancePolicy,
	) -> Result<Vec<u8>, AllowanceAllocationError> {
		use crate::runtime::statement_allowance::{
			self, claim_long_term_storage, collection::PersonhoodCollection, fetch_bulletin_allowance,
			find_including_rings, wait_bulletin_authorization,
		};

		signing_host.require_current_session(session)?;
		let entropy = signing_host.root_entropy()?;
		let allowance = derive_sr25519_hard_path(&entropy, &["allowance", "bulletin", product_id])?;
		#[cfg(feature = "test-host")]
		if signing_host.grants_allowances_unchecked() {
			return Ok(allowance.secret.to_bytes().to_vec());
		}
		let target = allowance.public.to_bytes();

		let bulletin_rpc = statement_allowance::rpc::RpcClient::new(
			services.bulletin.client("bulletin allowance").await.map_err(|source| {
				AllowanceAllocationError::ChainRpcClient {
					context: "bulletin allowance client",
					source,
				}
			})?,
		);
		let current_allowance = fetch_bulletin_allowance(&bulletin_rpc, &target).await?;
		if matches!(policy, OnExistingAllowancePolicy::Ignore) &&
			current_allowance.is_some_and(|allowance| allowance.available())
		{
			signing_host.require_current_session(session)?;
			return Ok(allowance.secret.to_bytes().to_vec());
		}

		let people_client = services.statement_store.chain_client("bulletin allowance claim").await?;
		let people_rpc = people_client.rpc();
		let chain = services.chain_context.get(&people_client).await?;
		let network_suffix = statement_allowance::slot::read_network_suffix(people_rpc).await?;
		let candidates = signing_host.reserved_person_collection_candidates(session)?;
		// Prefer light membership so switching collections cannot reset the person's budget.
		let signer = statement_allowance::FixedPersonhoodSigner::new(&candidates).await?;
		let collections: Vec<_> = candidates.iter().map(|candidate| candidate.collection).collect();
		let memberships =
			find_including_rings(people_rpc, &chain.metadata, &signer, &collections, u32::MAX).await?;
		let membership = memberships
			.iter()
			.find(|membership| membership.collection == PersonhoodCollection::LitePeople)
			.or_else(|| memberships.first())
			.ok_or(AllowanceAllocationError::MissingPersonhoodMembership { resource: "Bulletin" })?;
		let period_duration =
			statement_allowance::slot::long_term_storage_period_duration(&chain.metadata)?;
		let period = statement_allowance::slot::current_long_term_storage_period(
			current_unix_secs()?,
			period_duration,
		)?;
		signing_host.require_current_session(session)?;
		let outcome = claim_long_term_storage(statement_allowance::LongTermStorageClaim {
			rpc: people_rpc,
			metadata: &chain.metadata,
			chain_state: &chain.state,
			signer: &signer,
			network_suffix: &network_suffix,
			target: &target,
			period,
			ring: membership,
		})
		.await?;
		let statement_allowance::LongTermStorageOutcome::Claimed { block_hash, counter, ring_index } =
			outcome;
		debug!(
			%product_id,
			%block_hash,
			counter,
			ring_index,
			"claimed Bulletin long-term storage allowance"
		);

		let authorization = wait_bulletin_authorization(
			&bulletin_rpc,
			&target,
			current_allowance,
			BULLETIN_AUTHORIZATION_WAIT,
		)
		.await?;
		debug!(
			%product_id,
			remained_size = authorization.remained_size,
			remained_transactions = authorization.remained_transactions,
			"Bulletin authorization visible"
		);
		signing_host.require_current_session(session)?;
		Ok(allowance.secret.to_bytes().to_vec())
	}

	/// Claim an Asset Hub PGAS allowance for the product account `derivation_index`
	/// selects.
	///
	/// Unlike the statement-store and Bulletin allowances, this credits the product
	/// account itself rather than a dedicated `//allowance//…` account, and returns
	/// nothing: PGAS pre-warms a balance on an account the host already controls, so
	/// there is no key to hand back.
	///
	/// Asset Hub is resolved through the host's chain set rather than a configured
	/// hash, so a host that does not serve it says so instead of claiming against
	/// whatever chain a stale hash happens to reach.
	pub async fn allocate_smart_contract_allowance(
		services: &RuntimeServices,
		signing_host: &SigningHost,
		session: &AuthoritySession,
		product_id: &str,
		derivation_index: api::DerivationIndex,
		policy: OnExistingAllowancePolicy,
	) -> Result<(), AllowanceAllocationError> {
		use truapi::latest::ChainIdentifier;

		use crate::{
			host_logic::features,
			runtime::statement_allowance::{self, ChainClient, find_including_rings, pgas},
		};

		signing_host.require_current_session(session)?;

		// PGAS credits the product account the caller named.
		let target = signing_host
			.product_keypair(&api::ProductAccountId {
				dot_ns_identifier: product_id.to_string(),
				derivation_index,
			})?
			.public
			.to_bytes();

		let chains = features::supported_chains(services.platform.as_ref())
			.await
			.map_err(|err| AllowanceAllocationError::SupportedChains(err.reason))?;
		let asset_hub_genesis = features::genesis_for(&chains, ChainIdentifier::AssetHub)
			.ok_or(AllowanceAllocationError::ChainNotServed { chain: "Asset Hub" })?;
		let asset_hub_client = ChainClient::new(
			statement_allowance::rpc::RpcClient::new(subxt_rpcs::RpcClient::new(
				services.chain.rpc_client("PGAS allowance", &asset_hub_genesis).await.map_err(
					|source| AllowanceAllocationError::ChainRpcClient {
						context: "Asset Hub PGAS client",
						source,
					},
				)?,
			)),
			asset_hub_genesis,
		);
		let asset_hub = services.chain_context.get(&asset_hub_client).await?;

		// Reusing a funded account preserves the person's daily claim slots.
		if matches!(policy, OnExistingAllowancePolicy::Ignore) &&
			pgas::holds_a_full_claim(asset_hub_client.rpc(), &asset_hub.metadata, &target).await?
		{
			debug!(%product_id, "PGAS allowance already funded; leaving it alone");
			signing_host.require_current_session(session)?;
			return Ok(());
		}
		let network_suffix =
			statement_allowance::slot::read_network_suffix(asset_hub_client.rpc()).await?;

		let people_client = services.statement_store.chain_client("PGAS allowance ring").await?;
		let people_rpc = people_client.rpc();
		let people = services.chain_context.get(&people_client).await?;

		let candidates = signing_host.reserved_person_collection_candidates(session)?;
		// A PGAS claim uses the strongest available membership.
		let signer = statement_allowance::FixedPersonhoodSigner::new(&candidates).await?;
		let collections: Vec<_> = candidates.iter().map(|candidate| candidate.collection).collect();
		let membership =
			find_including_rings(people_rpc, &people.metadata, &signer, &collections, u32::MAX)
				.await?
				.into_iter()
				.next()
				.ok_or(AllowanceAllocationError::MissingPersonhoodMembership { resource: "PGAS" })?;

		signing_host.require_current_session(session)?;
		let outcome = pgas::claim_pgas(pgas::PgasClaim {
			asset_hub_rpc: asset_hub_client.rpc(),
			asset_hub: &asset_hub,
			people_rpc,
			people_metadata: &people.metadata,
			signer: &signer,
			network_suffix: &network_suffix,
			target: &target,
			ring: &membership,
		})
		.await?;
		debug!(
			%product_id,
			day = outcome.day,
			slot_index = outcome.slot_index,
			ring_index = outcome.ring_index,
			block = %outcome.block_hash,
			"claimed PGAS allowance"
		);
		signing_host.require_current_session(session)?;
		Ok(())
	}
}
