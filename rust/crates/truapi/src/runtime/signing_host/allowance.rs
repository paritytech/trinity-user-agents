//! Statement Store, Bulletin and PGAS allowances the signing host allocates for products.

use tracing::debug;
use truapi::latest as api;

use super::SigningHost;
use crate::{
	chain_runtime::RuntimeFailure,
	host_internal::sso_messages::OnExistingAllowancePolicy,
	host_logic::product_account::{ProductAccountError, derive_sr25519_hard_path},
	platform::ProductId,
	runtime::{
		authority::{AuthorityError, AuthoritySession},
		services::RuntimeServices,
		statement_allowance::{PersonhoodSigner, StatementAllowanceError},
		statement_store_rpc::StatementStoreRpcClientError,
	},
};

/// The canonical prover is synchronous; keep its secret use inside the same
/// lifecycle lock that fences account replacement and product revocation.
struct SessionPersonhoodSigner<'a> {
	signing_host: &'a SigningHost,
	session: &'a AuthoritySession,
	product_id: &'a ProductId,
	fixed: crate::runtime::statement_allowance::FixedPersonhoodSigner<'a>,
}

impl<'a> SessionPersonhoodSigner<'a> {
	async fn new(
		signing_host: &'a SigningHost,
		session: &'a AuthoritySession,
		product_id: &'a ProductId,
		candidates: &'a [crate::runtime::statement_allowance::CollectionCandidate],
	) -> Result<Self, AllowanceAllocationError> {
		signing_host.require_current_product_session(session, product_id)?;
		let fixed =
			crate::runtime::statement_allowance::FixedPersonhoodSigner::new(candidates).await?;
		signing_host.require_current_product_session(session, product_id)?;
		Ok(Self { signing_host, session, product_id, fixed })
	}

	fn with_current<T>(
		&self,
		operation: impl FnOnce(
			&crate::runtime::statement_allowance::FixedPersonhoodSigner<'_>,
		) -> Result<T, StatementAllowanceError>,
	) -> Result<T, StatementAllowanceError> {
		let state = self.signing_host.local_grants.lock().expect("local grants mutex poisoned");
		let current = self
			.signing_host
			.session_state
			.current()
			.ok_or(StatementAllowanceError::SessionInvalidated)?;
		let generation = super::local_session_generation(&current, &self.session.validation_id)
			.map_err(|_| StatementAllowanceError::SessionInvalidated)?;
		state
			.require_product_generation(generation, self.product_id)
			.map_err(|_| StatementAllowanceError::SessionInvalidated)?;
		operation(&self.fixed)
	}
}

impl PersonhoodSigner for SessionPersonhoodSigner<'_> {
	fn member(
		&self,
		collection: crate::runtime::statement_allowance::collection::PersonhoodCollection,
	) -> Result<[u8; 32], StatementAllowanceError> {
		self.with_current(|fixed| fixed.member(collection))
	}

	fn alias(
		&self,
		collection: crate::runtime::statement_allowance::collection::PersonhoodCollection,
		context: &[u8],
	) -> Result<[u8; 32], StatementAllowanceError> {
		self.with_current(|fixed| fixed.alias(collection, context))
	}

	fn prove(
		&self,
		ring: &crate::runtime::statement_allowance::ring::RingParams,
		context: &[u8],
		message: &[u8],
	) -> Result<Vec<u8>, StatementAllowanceError> {
		self.with_current(|fixed| fixed.prove(ring, context, message))
	}
}

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

impl From<AllowanceAllocationError> for AuthorityError {
	fn from(error: AllowanceAllocationError) -> Self {
		match error {
			AllowanceAllocationError::Authority(err) => err,
			AllowanceAllocationError::StatementAllowance(
				StatementAllowanceError::SessionInvalidated,
			) => Self::Disconnected,
			other => Self::Unavailable { reason: other.to_string() },
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

/// Issue a statement allowance with the period actually found or registered.
pub async fn allocate_statement_store_allowance(
	services: &RuntimeServices,
	signing_host: &SigningHost,
	session: &AuthoritySession,
	product_id: &ProductId,
	policy: OnExistingAllowancePolicy,
) -> Result<StatementStoreAllocation, AllowanceAllocationError> {
	signing_host.require_current_product_session(session, product_id)?;
	let _allocation = signing_host.resource_allocation.lock().await;
	signing_host.require_current_product_session(session, product_id)?;
	let entropy = signing_host.root_entropy()?;
	let allowance =
		derive_sr25519_hard_path(&entropy, &["allowance", "statement-store", product_id.as_str()])?;
	// Test signatures use real keys without claiming on-chain registration.
	#[cfg(feature = "test-host")]
	if signing_host.grants_allowances_unchecked() {
		return Ok(StatementStoreAllocation {
			secret: allowance.secret.to_bytes().to_vec(),
			period: crate::runtime::statement_allowance::slot::current_period(current_unix_secs()?),
		});
	}
	let period = register_statement_store_target(
		services,
		signing_host,
		session,
		product_id,
		allowance.public.to_bytes(),
		policy,
	)
	.await?;
	signing_host.require_current_product_session(session, product_id)?;
	Ok(StatementStoreAllocation { secret: allowance.secret.to_bytes().to_vec(), period })
}

/// Allocate statement-store quota directly to the selected product account.
pub async fn allocate_product_statement_store_allowance(
	services: &RuntimeServices,
	signing_host: &SigningHost,
	session: &AuthoritySession,
	product_id: &ProductId,
	derivation_index: &api::DerivationIndex,
	policy: OnExistingAllowancePolicy,
) -> Result<(), AllowanceAllocationError> {
	signing_host.require_current_product_session(session, product_id)?;
	let _allocation = signing_host.resource_allocation.lock().await;
	signing_host.require_current_product_session(session, product_id)?;
	#[cfg(feature = "test-host")]
	if signing_host.grants_allowances_unchecked() {
		return Ok(());
	}
	let target = signing_host
		.product_keypair(&api::ProductAccountId {
			dot_ns_identifier: product_id.clone(),
			derivation_index: derivation_index.clone(),
		})?
		.public
		.to_bytes();
	register_statement_store_target(services, signing_host, session, product_id, target, policy)
		.await
		.map(|_| ())
}

async fn register_statement_store_target(
	services: &RuntimeServices,
	signing_host: &SigningHost,
	session: &AuthoritySession,
	product_id: &ProductId,
	target: [u8; 32],
	policy: OnExistingAllowancePolicy,
) -> Result<u32, AllowanceAllocationError> {
	use crate::runtime::statement_allowance::{
		self, PooledRegistrationParams, allocated_in, find_including_rings,
		register_statement_account_pooled_with_session_guard, scan_collections,
	};

	signing_host.require_current_product_session(session, product_id)?;
	let candidates = signing_host.reserved_person_collection_candidates(session)?;
	let client = services.statement_store.chain_client("statement-store allowance").await?;
	signing_host.require_current_product_session(session, product_id)?;
	let rpc = client.rpc();
	let chain = services.chain_context.get(&client).await?;
	signing_host.require_current_product_session(session, product_id)?;
	let network_suffix = statement_allowance::slot::read_network_suffix(rpc).await?;
	signing_host.require_current_product_session(session, product_id)?;
	let period = statement_allowance::slot::current_period(current_unix_secs()?);
	let reuse_existing = matches!(policy, OnExistingAllowancePolicy::Ignore);

	// Hold through scan and submission so renewal cannot select the same slot.
	let _registration = signing_host.renewal.registration_lock().lock().await;
	signing_host.require_current_product_session(session, product_id)?;

	// Existing allocations need no proof or costly ring snapshot.
	let signer =
		SessionPersonhoodSigner::new(signing_host, session, product_id, &candidates).await?;
	signing_host.require_current_product_session(session, product_id)?;
	let collections: Vec<_> = candidates.iter().map(|candidate| candidate.collection).collect();
	let scans = scan_collections(
		rpc,
		&chain.metadata,
		&signer,
		&collections,
		statement_allowance::CollectionScanParams {
			network_suffix: &network_suffix,
			period,
			target: &target,
			reuse_existing,
		},
	)
	.await?;
	signing_host.require_current_product_session(session, product_id)?;
	if let Some((collection, seq)) = allocated_in(&scans) {
		debug!(
			%product_id,
			period,
			seq,
			%collection,
			"statement-store allowance already allocated"
		);
		return Ok(period);
	}

	// Every ring back to index 0, because a membership that stopped being
	// re-included still proves against the ring that holds it.
	let memberships =
		find_including_rings(rpc, &chain.metadata, &signer, &collections, u32::MAX).await?;
	signing_host.require_current_product_session(session, product_id)?;
	if memberships.is_empty() {
		return Err(AllowanceAllocationError::MissingPersonhoodMembership {
			resource: "statement-store",
		});
	}
	signing_host.require_current_product_session(session, product_id)?;
	let outcome = register_statement_account_pooled_with_session_guard(
		rpc,
		&chain.metadata,
		client.extrinsic_target(&services.extrinsics),
		&signer,
		&scans,
		&memberships,
		PooledRegistrationParams {
			target: &target,
			period,
			network_suffix: &network_suffix,
			reuse_existing,
			// Only renewal may reclaim slots belonging to its own ledger.
			allow_eviction: false,
			protected: &[],
		},
		|| signing_host.require_current_product_session(session, product_id).is_ok(),
	)
	.await?;
	match outcome {
		statement_allowance::RegistrationOutcome::Registered {
			block_hash,
			seq,
			ring_index,
			collection,
		} => {
			debug!(
				%product_id,
				%block_hash,
				seq,
				ring_index,
				%collection,
				"registered statement-store allowance"
			);
		},
		statement_allowance::RegistrationOutcome::AlreadyAllocated { seq, collection } => {
			debug!(
				%product_id,
				seq,
				%collection,
				"statement-store allowance already allocated"
			);
		},
	}
	signing_host.require_current_product_session(session, product_id)?;
	Ok(period)
}

async fn bulletin_allowance_rpc(
	services: &RuntimeServices,
	signing_host: &SigningHost,
	session: &AuthoritySession,
	product_id: &ProductId,
) -> Result<crate::runtime::statement_allowance::rpc::RpcClient, AllowanceAllocationError> {
	signing_host.require_current_product_session(session, product_id)?;
	let client = services.bulletin.client("bulletin allowance").await.map_err(|source| {
		AllowanceAllocationError::ChainRpcClient { context: "bulletin allowance client", source }
	})?;
	signing_host.require_current_product_session(session, product_id)?;
	Ok(crate::runtime::statement_allowance::rpc::RpcClient::new(client))
}

async fn asset_hub_allowance_client(
	services: &RuntimeServices,
	signing_host: &SigningHost,
	session: &AuthoritySession,
	product_id: &ProductId,
) -> Result<crate::runtime::statement_allowance::ChainClient, AllowanceAllocationError> {
	use crate::{
		host_logic::features,
		runtime::statement_allowance::{ChainClient, rpc::RpcClient},
	};
	signing_host.require_current_product_session(session, product_id)?;
	let chains = features::supported_chains(services.platform.as_ref())
		.await
		.map_err(|err| AllowanceAllocationError::SupportedChains(err.reason))?;
	signing_host.require_current_product_session(session, product_id)?;
	let genesis = features::genesis_for(&chains, api::ChainIdentifier::AssetHub)
		.ok_or(AllowanceAllocationError::ChainNotServed { chain: "Asset Hub" })?;
	let rpc = services.chain.rpc_client("PGAS allowance", &genesis).await.map_err(|source| {
		AllowanceAllocationError::ChainRpcClient { context: "Asset Hub PGAS client", source }
	})?;
	signing_host.require_current_product_session(session, product_id)?;
	Ok(ChainClient::new(RpcClient::new(subxt_rpcs::RpcClient::new(rpc)), genesis))
}

/// Caller holds `resource_allocation`; only public keys leave this inspection.
pub(super) async fn inspect_product_resources(
	services: &RuntimeServices,
	signing_host: &SigningHost,
	session: &AuthoritySession,
	product_id: &ProductId,
) -> Result<crate::host_core::ProductResourceStatus, AllowanceAllocationError> {
	use crate::runtime::statement_allowance::{fetch_bulletin_allowance, pgas};
	signing_host.require_current_product_session(session, product_id)?;
	let bulletin_account = {
		let entropy = signing_host.root_entropy()?;
		derive_sr25519_hard_path(&entropy, &["allowance", "bulletin", product_id.as_str()])?
			.public
			.to_bytes()
	};
	let contract_account = signing_host
		.product_keypair(&api::ProductAccountId {
			dot_ns_identifier: product_id.clone(),
			derivation_index: api::DerivationIndex::Index(0),
		})?
		.public
		.to_bytes();
	signing_host.require_current_product_session(session, product_id)?;
	let bulletin_rpc = bulletin_allowance_rpc(services, signing_host, session, product_id).await?;
	let bulletin_available = fetch_bulletin_allowance(&bulletin_rpc, &bulletin_account)
		.await?
		.is_some_and(|allowance| allowance.available());
	signing_host.require_current_product_session(session, product_id)?;
	let client = asset_hub_allowance_client(services, signing_host, session, product_id).await?;
	let chain = services.chain_context.get(&client).await?;
	signing_host.require_current_product_session(session, product_id)?;
	let contract_funded =
		pgas::balance(client.rpc(), &chain.metadata, &contract_account).await? > 0;
	let (_, generation) = signing_host.require_current_product_session(session, product_id)?;
	let auto_signing =
		signing_host.has_auto_signing_grant(generation, session.public_key, product_id, product_id);
	signing_host.require_current_product_session(session, product_id)?;
	Ok(crate::host_core::ProductResourceStatus {
		bulletin_account,
		bulletin_available,
		contract_account,
		contract_funded,
		auto_signing,
	})
}

/// Issue the product's Bulletin allowance key.
pub async fn allocate_bulletin_allowance(
	services: &RuntimeServices,
	signing_host: &SigningHost,
	session: &AuthoritySession,
	product_id: &ProductId,
	policy: OnExistingAllowancePolicy,
) -> Result<Vec<u8>, AllowanceAllocationError> {
	signing_host.require_current_product_session(session, product_id)?;
	let _allocation = signing_host.resource_allocation.lock().await;
	allocate_bulletin_allowance_under_guard(services, signing_host, session, product_id, policy)
		.await
}

/// Caller holds `resource_allocation` across the read, claim and confirmation.
pub(super) async fn allocate_bulletin_allowance_under_guard(
	services: &RuntimeServices,
	signing_host: &SigningHost,
	session: &AuthoritySession,
	product_id: &ProductId,
	policy: OnExistingAllowancePolicy,
) -> Result<Vec<u8>, AllowanceAllocationError> {
	use crate::runtime::statement_allowance::{
		self, claim_long_term_storage, collection::PersonhoodCollection, fetch_bulletin_allowance,
		find_including_rings, wait_bulletin_authorization,
	};

	signing_host.require_current_product_session(session, product_id)?;
	let entropy = signing_host.root_entropy()?;
	let allowance =
		derive_sr25519_hard_path(&entropy, &["allowance", "bulletin", product_id.as_str()])?;
	#[cfg(feature = "test-host")]
	if signing_host.grants_allowances_unchecked() {
		return Ok(allowance.secret.to_bytes().to_vec());
	}
	let target = allowance.public.to_bytes();

	signing_host.require_current_product_session(session, product_id)?;
	let bulletin_rpc = bulletin_allowance_rpc(services, signing_host, session, product_id).await?;
	let current_allowance = fetch_bulletin_allowance(&bulletin_rpc, &target).await?;
	signing_host.require_current_product_session(session, product_id)?;
	if matches!(policy, OnExistingAllowancePolicy::Ignore) &&
		current_allowance.is_some_and(|allowance| allowance.available())
	{
		signing_host.require_current_product_session(session, product_id)?;
		return Ok(allowance.secret.to_bytes().to_vec());
	}

	let people_client = services.statement_store.chain_client("bulletin allowance claim").await?;
	signing_host.require_current_product_session(session, product_id)?;
	let people_rpc = people_client.rpc();
	let chain = services.chain_context.get(&people_client).await?;
	signing_host.require_current_product_session(session, product_id)?;
	let network_suffix = statement_allowance::slot::read_network_suffix(people_rpc).await?;
	signing_host.require_current_product_session(session, product_id)?;
	let candidates = signing_host.reserved_person_collection_candidates(session)?;
	// Prefer light membership so switching collections cannot reset the person's budget.
	let signer =
		SessionPersonhoodSigner::new(signing_host, session, product_id, &candidates).await?;
	signing_host.require_current_product_session(session, product_id)?;
	let collections: Vec<_> = candidates.iter().map(|candidate| candidate.collection).collect();
	let memberships =
		find_including_rings(people_rpc, &chain.metadata, &signer, &collections, u32::MAX).await?;
	signing_host.require_current_product_session(session, product_id)?;
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
	signing_host.require_current_product_session(session, product_id)?;
	let outcome = claim_long_term_storage(
		statement_allowance::LongTermStorageClaim {
			rpc: people_rpc,
			metadata: &chain.metadata,
			target_chain: people_client.extrinsic_target(&services.extrinsics),
			signer: &signer,
			network_suffix: &network_suffix,
			target: &target,
			period,
			ring: membership,
		},
		|| signing_host.require_current_product_session(session, product_id).is_ok(),
	)
	.await?;
	signing_host.require_current_product_session(session, product_id)?;
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
		1,
		BULLETIN_AUTHORIZATION_WAIT,
	)
	.await?;
	debug!(
		%product_id,
		remained_size = authorization.remained_size,
		remained_transactions = authorization.remained_transactions,
		"Bulletin authorization visible"
	);
	signing_host.require_current_product_session(session, product_id)?;
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
	product_id: &ProductId,
	derivation_index: api::DerivationIndex,
	policy: OnExistingAllowancePolicy,
) -> Result<(), AllowanceAllocationError> {
	signing_host.require_current_product_session(session, product_id)?;
	let _allocation = signing_host.resource_allocation.lock().await;
	allocate_smart_contract_allowance_under_guard(
		services,
		signing_host,
		session,
		product_id,
		derivation_index,
		policy,
	)
	.await
}

/// Caller holds `resource_allocation`, including when recovering both resources.
pub(super) async fn allocate_smart_contract_allowance_under_guard(
	services: &RuntimeServices,
	signing_host: &SigningHost,
	session: &AuthoritySession,
	product_id: &ProductId,
	derivation_index: api::DerivationIndex,
	policy: OnExistingAllowancePolicy,
) -> Result<(), AllowanceAllocationError> {
	use crate::runtime::statement_allowance::{self, find_including_rings, pgas};

	signing_host.require_current_product_session(session, product_id)?;

	// PGAS credits the product account the caller named.
	let target = signing_host
		.product_keypair(&api::ProductAccountId {
			dot_ns_identifier: product_id.clone(),
			derivation_index,
		})?
		.public
		.to_bytes();

	signing_host.require_current_product_session(session, product_id)?;
	let asset_hub_client =
		asset_hub_allowance_client(services, signing_host, session, product_id).await?;
	let asset_hub = services.chain_context.get(&asset_hub_client).await?;
	signing_host.require_current_product_session(session, product_id)?;

	// Even a partly spent claim is funding, not permission to replenish it.
	if matches!(policy, OnExistingAllowancePolicy::Ignore) &&
		pgas::balance(asset_hub_client.rpc(), &asset_hub.metadata, &target).await? > 0
	{
		debug!(%product_id, "PGAS allowance already funded; leaving it alone");
		signing_host.require_current_product_session(session, product_id)?;
		return Ok(());
	}
	signing_host.require_current_product_session(session, product_id)?;
	let network_suffix =
		statement_allowance::slot::read_network_suffix(asset_hub_client.rpc()).await?;
	signing_host.require_current_product_session(session, product_id)?;

	let people_client = services.statement_store.chain_client("PGAS allowance ring").await?;
	signing_host.require_current_product_session(session, product_id)?;
	let people_rpc = people_client.rpc();
	let people = services.chain_context.get(&people_client).await?;
	signing_host.require_current_product_session(session, product_id)?;

	let candidates = signing_host.reserved_person_collection_candidates(session)?;
	// A PGAS claim uses the strongest available membership.
	let signer =
		SessionPersonhoodSigner::new(signing_host, session, product_id, &candidates).await?;
	signing_host.require_current_product_session(session, product_id)?;
	let collections: Vec<_> = candidates.iter().map(|candidate| candidate.collection).collect();
	let membership =
		find_including_rings(people_rpc, &people.metadata, &signer, &collections, u32::MAX)
			.await?
			.into_iter()
			.next()
			.ok_or(AllowanceAllocationError::MissingPersonhoodMembership { resource: "PGAS" })?;

	signing_host.require_current_product_session(session, product_id)?;
	let outcome = pgas::claim_pgas_with_session_guard(
		pgas::PgasClaim {
			asset_hub_rpc: asset_hub_client.rpc(),
			asset_hub_metadata: &asset_hub.metadata,
			asset_hub_target: asset_hub_client.extrinsic_target(&services.extrinsics),
			people_rpc,
			people_metadata: &people.metadata,
			signer: &signer,
			network_suffix: &network_suffix,
			target: &target,
			ring: &membership,
		},
		|| signing_host.require_current_product_session(session, product_id).is_ok(),
	)
	.await?;
	debug!(
		%product_id,
		day = outcome.day,
		slot_index = outcome.slot_index,
		ring_index = outcome.ring_index,
		block = %outcome.block_hash,
		"claimed PGAS allowance"
	);
	signing_host.require_current_product_session(session, product_id)?;
	Ok(())
}

/// Allowance period clock, using web-time on browsers to avoid SystemTime panics.
pub fn current_unix_secs() -> Result<u64, AllowanceAllocationError> {
	#[cfg(not(target_arch = "wasm32"))]
	use std::time::{SystemTime, UNIX_EPOCH};
	#[cfg(target_arch = "wasm32")]
	use web_time::{SystemTime, UNIX_EPOCH};

	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|duration| duration.as_secs())
		.map_err(|_| AllowanceAllocationError::SystemClockBeforeUnixEpoch)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;
	use crate::{
		host_logic::product_account::derive_full_person_ring_vrf_entropy,
		platform::HostInfo,
		runtime::{
			authority::ProductAuthority, signing_host::LocalActivation, statement_allowance::slot,
		},
		test_support::{StubPlatform, test_spawner},
	};
	use futures::FutureExt;
	use parity_scale_codec::Encode;
	use std::sync::Arc;

	const ENTROPY: [u8; 16] = [0xab; 16];
	const NETWORK_SUFFIX: &str = "paseo";
	const PEOPLE_METADATA: &[u8] =
		include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

	#[test]
	fn repeated_implicit_provisioning_reuses_existing_allowance_without_submission() {
		let product_id = ProductId::for_test("myapp.dot");
		let allowance = derive_sr25519_hard_path(
			&ENTROPY,
			&["allowance", "statement-store", product_id.as_str()],
		)
		.expect("allowance derivation succeeds");
		let slot_entry = (allowance.public.to_bytes(), 0u32, 0u64).encode();
		let people_row = slot::testing::slot_row(
			derive_full_person_ring_vrf_entropy(&ENTROPY, NETWORK_SUFFIX),
			NETWORK_SUFFIX.as_bytes(),
			slot::current_period(current_unix_secs().unwrap()),
			&[Some(format!(r#""0x{}""#, hex::encode(&slot_entry)))],
		);
		let platform = Arc::new(StubPlatform {
			rpc_method_responses: vec![
				(
					"state_getRuntimeVersion",
					r#"{"specVersion":1000000,"transactionVersion":1}"#.to_string(),
				),
				("chain_getBlockHash", format!(r#""0x{}""#, hex::encode([0u8; 32]))),
				(
					"Metadata_metadata_at_version",
					format!(r#""0x{}""#, hex::encode(Some(PEOPLE_METADATA.to_vec()).encode())),
				),
				(
					"RuntimeViewFunction_execute_view_function",
					format!(r#""0x{}""#, hex::encode(Ok::<Vec<u8>, ()>(20u32.encode()).encode())),
				),
				(
					"RuntimeViewFunction_execute_view_function",
					format!(r#""0x{}""#, hex::encode(Ok::<Vec<u8>, ()>(10u32.encode()).encode())),
				),
				("state_getStorage", format!(r#""0x{}""#, hex::encode(b"paseo".to_vec().encode()))),
				("state_queryStorageAt", people_row),
				("state_queryStorageAt", r#"[{"block":"0xb10c","changes":[]}]"#.to_string()),
				("state_getStorage", format!(r#""0x{}""#, hex::encode(b"paseo".to_vec().encode()))),
				("state_getStorage", format!(r#""0x{}""#, hex::encode(&slot_entry))),
				(
					"state_getRuntimeVersion",
					r#"{"specVersion":1000000,"transactionVersion":1}"#.to_string(),
				),
				("chain_getBlockHash", format!(r#""0x{}""#, hex::encode([0u8; 32]))),
				(
					"RuntimeViewFunction_execute_view_function",
					format!(r#""0x{}""#, hex::encode(Ok::<Vec<u8>, ()>(20u32.encode()).encode())),
				),
			],
			..Default::default()
		});
		let services = RuntimeServices::new(
			platform.clone(),
			HostInfo {
				name: "Polkadot Mobile".to_string(),
				icon: None,
				version: None,
				platform: api::HostPlatform::Unknown,
			},
			[0; 32],
			[0xbb; 32],
			[0xcc; 32],
			test_spawner(),
		);
		let signing_host = SigningHost::new(
			services,
			NETWORK_SUFFIX.to_string(),
			None,
			crate::platform::ProductConsentPolicy::default(),
		);
		futures::executor::block_on(async {
			signing_host.activate_local_session(ENTROPY.to_vec()).await.unwrap();
			let session = signing_host.current_session().unwrap();
			let cx = truapi::CallContext::default();
			for _ in 0..2 {
				let allocation =
					signing_host.statement_store_allowance_key(&cx, &session, &product_id);
				futures::pin_mut!(allocation);
				let response = futures::select! {
					result = allocation.fuse() => result,
					_ = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse() => {
						panic!("allocation blocked on a chain read it should not have made")
					}
				}
				.expect("existing allowance succeeds");
				assert_eq!(response.public_key, allowance.public.to_bytes());
			}
		});
		let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
		let methods: Vec<String> = sent
			.iter()
			.filter_map(|request| {
				let value: serde_json::Value = serde_json::from_str(request).ok()?;
				value.get("method")?.as_str().map(ToString::to_string)
			})
			.collect();
		assert_eq!(
			methods.iter().filter(|method| *method == "chain_getFinalizedHead").count(),
			0,
			"a ring was resolved for an allowance already in place: {methods:?}"
		);
		assert!(
			!methods.iter().any(|method| method.starts_with("author_submit")),
			"an extrinsic was submitted for an allowance already in place: {methods:?}"
		);
	}
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "tests/resource_recovery.rs"]
mod recovery_tests;
