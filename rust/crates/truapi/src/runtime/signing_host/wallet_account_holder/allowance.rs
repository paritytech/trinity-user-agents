//! Wallet resource approval and allowance issuance.

use tracing::{debug, warn};
use truapi::latest as api;

use super::WalletAccountHolder;
use crate::{
	chain_runtime::RuntimeFailure,
	host_internal::sso_messages::OnExistingAllowancePolicy,
	host_logic::product_account::ProductAccountError,
	platform::ProductId,
	runtime::{
		allowances::current_unix_secs,
		authority::{AccountHolder, AccountInvocation, AuthorityError, AuthoritySession},
		statement_allowance::StatementAllowanceError,
		statement_store_rpc::StatementStoreRpcClientError,
	},
};

/// Leave the product runtime one minute to receive and process the SSO response
/// before its 300-second remote-authority deadline expires.
const BULLETIN_AUTHORIZATION_WAIT: std::time::Duration = std::time::Duration::from_secs(240);

/// Failure while deriving or allocating a Statement Store/Bulletin allowance.
#[derive(Debug, thiserror::Error)]
pub enum AllowanceAllocationError {
	/// Signing host session or authority state was unavailable.
	#[error("{0}")]
	Authority(#[from] AuthorityError),
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
	/// The durable ledger did not register the PGAS claim.
	#[cfg(not(target_arch = "wasm32"))]
	#[error("PGAS claim not registered: {0}")]
	PgasClaimNotRegistered(
		#[from] crate::durable::RegistrationError<crate::pgas_claims::PgasClaimError>,
	),
	/// The request stopped waiting before its PGAS claim reached a block.
	#[cfg(not(target_arch = "wasm32"))]
	#[error("PGAS claim {claim:?} not delivered: {reason}")]
	PgasClaimNotDelivered {
		/// The claim's ledger id.
		claim: crate::durable::DurableTxId,
		/// Why the request stopped waiting.
		#[source]
		reason: crate::pgas_claims::UndeliveredClaim,
	},
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
			AllowanceAllocationError::Authority(err) |
			AllowanceAllocationError::StatementAllowance(StatementAllowanceError::Authority(
				err,
			)) => err,
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

impl WalletAccountHolder {
	/// Issue a statement allowance with the period actually found or registered.
	pub async fn allocate_statement_store_allowance(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		policy: OnExistingAllowancePolicy,
	) -> Result<StatementStoreAllocation, AllowanceAllocationError> {
		let target = self.with_keys::<_, AllowanceAllocationError>(session, |keys| {
			Ok(keys.statement_allowance_key(product_id)?.public.to_bytes())
		})?;
		let period = self.ensure_statement_slot(session, product_id, target, policy).await?;
		self.with_keys(session, |keys| {
			Ok(StatementStoreAllocation {
				secret: keys.statement_allowance_key(product_id)?.secret.to_bytes().to_vec(),
				period,
			})
		})
	}

	/// Find or register the statement slot `target` holds for this period.
	async fn ensure_statement_slot(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		target: [u8; 32],
		policy: OnExistingAllowancePolicy,
	) -> Result<u32, AllowanceAllocationError> {
		use super::allowance_renewal::StatementRenewalTarget;
		use crate::runtime::statement_allowance::{
			self, CollectionScanParams, PooledRegistrationParams, allocated_in,
			collection::PersonhoodCollection, find_including_rings,
			register_statement_account_pooled, scan_collections,
		};

		// Test signatures use real keys without claiming on-chain registration.
		#[cfg(feature = "test-host")]
		if self.resource_controls.grants_allowances_unchecked() {
			return Ok(statement_allowance::slot::current_period(current_unix_secs()?));
		}
		let client =
			self.services.statement_store.chain_client("statement-store allowance").await?;
		let rpc = client.rpc();
		let chain = self.services.chain_context.get(&client).await?;
		let network_suffix = statement_allowance::slot::read_network_suffix(rpc).await?;
		let period = statement_allowance::slot::current_period(current_unix_secs()?);
		let reuse_existing = matches!(policy, OnExistingAllowancePolicy::Ignore);

		// Hold through scan and submission so renewal cannot select the same slot.
		let _registration = self.renewal.registration_lock().lock().await;

		// Existing allocations need no proof or costly ring snapshot.
		let signer = self.personhood_signer(session).await?;
		let scans = scan_collections(
			rpc,
			&chain.metadata,
			&signer,
			&PersonhoodCollection::ALL,
			CollectionScanParams {
				network_suffix: &network_suffix,
				period,
				target: &target,
				reuse_existing,
			},
		)
		.await?;
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
		let memberships = find_including_rings(
			rpc,
			&chain.metadata,
			&signer,
			&PersonhoodCollection::ALL,
			u32::MAX,
		)
		.await?;
		if memberships.is_empty() {
			return Err(AllowanceAllocationError::MissingPersonhoodMembership {
				resource: "statement-store",
			});
		}
		self.require_current_session(session)?;
		let outcome = register_statement_account_pooled(
			rpc,
			&chain.metadata,
			client.extrinsic_target(&self.services.extrinsics),
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
		self.require_current_session(session)?;
		if let Err(reason) = self
			.track_statement_renewal_targets_for(
				session,
				vec![StatementRenewalTarget::ProductStatementAllowance {
					product_id: product_id.clone(),
				}],
			)
			.await
		{
			warn!(%product_id, %reason, "failed to record statement-store renewal target");
		}
		Ok(period)
	}

	/// Issue the product's Bulletin allowance key.
	pub async fn allocate_bulletin_allowance(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		policy: OnExistingAllowancePolicy,
	) -> Result<Vec<u8>, AllowanceAllocationError> {
		use crate::runtime::statement_allowance::{
			self, claim_long_term_storage, collection::PersonhoodCollection,
			fetch_bulletin_allowance, find_including_rings, wait_bulletin_authorization,
		};

		#[cfg(feature = "test-host")]
		if self.resource_controls.grants_allowances_unchecked() {
			return self.bulletin_secret(session, product_id);
		}
		let target = self.with_keys::<_, AllowanceAllocationError>(session, |keys| {
			Ok(keys.bulletin_allowance_key(product_id)?.public.to_bytes())
		})?;

		let bulletin_rpc = statement_allowance::rpc::RpcClient::new(
			self.services.bulletin.client("bulletin allowance").await.map_err(|source| {
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
			return self.bulletin_secret(session, product_id);
		}

		let people_client =
			self.services.statement_store.chain_client("bulletin allowance claim").await?;
		let people_rpc = people_client.rpc();
		let chain = self.services.chain_context.get(&people_client).await?;
		let network_suffix = statement_allowance::slot::read_network_suffix(people_rpc).await?;
		let signer = self.personhood_signer(session).await?;
		// Prefer light membership so switching collections cannot reset the person's budget.
		let memberships = find_including_rings(
			people_rpc,
			&chain.metadata,
			&signer,
			&PersonhoodCollection::ALL,
			u32::MAX,
		)
		.await?;
		let membership = memberships
			.iter()
			.find(|membership| membership.collection == PersonhoodCollection::LitePeople)
			.or_else(|| memberships.first())
			.ok_or(AllowanceAllocationError::MissingPersonhoodMembership {
				resource: "Bulletin",
			})?;
		let period_duration =
			statement_allowance::slot::long_term_storage_period_duration(&chain.metadata)?;
		let period = statement_allowance::slot::current_long_term_storage_period(
			current_unix_secs()?,
			period_duration,
		)?;
		self.require_current_session(session)?;
		let outcome = claim_long_term_storage(statement_allowance::LongTermStorageClaim {
			rpc: people_rpc,
			metadata: &chain.metadata,
			target_chain: people_client.extrinsic_target(&self.services.extrinsics),
			signer: &signer,
			network_suffix: &network_suffix,
			target: &target,
			period,
			ring: membership,
		})
		.await?;
		let statement_allowance::LongTermStorageOutcome::Claimed {
			block_hash,
			counter,
			ring_index,
		} = outcome;
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
		self.bulletin_secret(session, product_id)
	}

	fn bulletin_secret(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
	) -> Result<Vec<u8>, AllowanceAllocationError> {
		self.with_keys(session, |keys| {
			Ok(keys.bulletin_allowance_key(product_id)?.secret.to_bytes().to_vec())
		})
	}

	/// Claim an Asset Hub PGAS allowance for the product account `derivation_index`
	/// selects.
	///
	/// Unlike the statement-store and Bulletin allowances, this credits the product
	/// account itself rather than a dedicated `//allowance//…` account, and returns
	/// nothing: PGAS pre-warms a balance on an account the host already controls, so
	/// there is no key to hand back.
	pub async fn allocate_smart_contract_allowance(
		&self,
		invocation: &AccountInvocation<'_>,
		product_id: &ProductId,
		derivation_index: api::DerivationIndex,
		policy: OnExistingAllowancePolicy,
	) -> Result<(), AllowanceAllocationError> {
		let session = invocation.session;
		let target = self.product_account_public_key(session, product_id, derivation_index)?;
		let asset_hub = self.pgas_asset_hub().await?;
		// Reusing a funded account preserves the person's daily claim slots.
		if matches!(policy, OnExistingAllowancePolicy::Ignore) &&
			asset_hub.holds_a_full_claim(&target).await?
		{
			debug!(%product_id, "PGAS allowance already funded; leaving it alone");
			return Ok(());
		}
		let signer = self.personhood_signer(session).await?;
		// Both claim paths need the person in a ring, so the check runs first either way.
		let proof = self.claim_proof_context(&asset_hub, &signer).await?;
		self.require_current_session(session)?;
		#[cfg(not(target_arch = "wasm32"))]
		if let Some(claims) = self.services.pgas_claims() {
			let request = super::pgas_delivery::DurablePgasClaim::new(session, target);
			return request.deliver(&claims, invocation.call).await;
		}
		let claim = self.pgas_claim(&asset_hub, &proof, &signer, &target);
		self.claim_pgas_directly(product_id, claim).await
	}

	/// The public key of the product account PGAS credits: the account of
	/// `product_id` that `derivation_index` selects.
	fn product_account_public_key(
		&self,
		session: &AuthoritySession,
		product_id: &ProductId,
		derivation_index: api::DerivationIndex,
	) -> Result<[u8; 32], AuthorityError> {
		let account =
			api::ProductAccountId { dot_ns_identifier: product_id.clone(), derivation_index };
		self.with_keys(session, |keys| {
			keys.product_keypair(&account).map(|key| key.public.to_bytes())
		})
	}
}
