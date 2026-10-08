//! Wallet resource approval and allowance issuance.

use crate::runtime::allowances::current_unix_secs;

use super::WalletAccountHolder;
use crate::chain_runtime::RuntimeFailure;
use crate::host_internal::sso_messages::OnExistingAllowancePolicy;
use crate::host_logic::product_account::ProductAccountError;
use crate::runtime::authority::{AccountHolder, AuthorityError, AuthoritySession};
use crate::runtime::statement_allowance::StatementAllowanceError;
use crate::runtime::statement_store_rpc::StatementStoreRpcClientError;
use tracing::{debug, warn};
use truapi::latest as api;

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
    /// The signing account is not in any personhood ring.
    #[error("signing account is not a personhood ring member; cannot grant {resource} allowance")]
    MissingPersonhoodMembership {
        /// Resource name.
        resource: &'static str,
    },
}

impl AllowanceAllocationError {
    /// Preserve authority failures while reporting unavailable allowance work.
    pub fn into_authority_error(self) -> AuthorityError {
        match self {
            Self::Authority(err)
            | Self::StatementAllowance(StatementAllowanceError::Authority(err)) => err,
            other => AuthorityError::Unavailable {
                reason: other.to_string(),
            },
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
        product_id: &str,
        policy: OnExistingAllowancePolicy,
    ) -> Result<StatementStoreAllocation, AllowanceAllocationError> {
        use super::allowance_renewal::StatementRenewalTarget;
        use crate::runtime::statement_allowance::collection::PersonhoodCollection;
        use crate::runtime::statement_allowance::{
            self, CollectionScanParams, PooledRegistrationParams, allocated_in,
            find_including_rings, register_statement_account_pooled, scan_collections,
        };

        // Test signatures use real keys without claiming on-chain registration.
        #[cfg(feature = "test-host")]
        if self.resource_controls.grants_allowances_unchecked() {
            return self.with_keys(session, |keys| {
                Ok(StatementStoreAllocation {
                    secret: keys
                        .statement_allowance_key(product_id)?
                        .secret
                        .to_bytes()
                        .to_vec(),
                    period: statement_allowance::slot::current_period(current_unix_secs()?),
                })
            });
        }
        let target = self.with_keys::<_, AllowanceAllocationError>(session, |keys| {
            Ok(keys.statement_allowance_key(product_id)?.public.to_bytes())
        })?;
        let client = self
            .services
            .statement_store
            .chain_client("statement-store allowance")
            .await?;
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
            return self.with_keys(session, |keys| {
                Ok(StatementStoreAllocation {
                    secret: keys
                        .statement_allowance_key(product_id)?
                        .secret
                        .to_bytes()
                        .to_vec(),
                    period,
                })
            });
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
            &chain.state,
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
            }
            statement_allowance::RegistrationOutcome::AlreadyAllocated { seq, collection } => {
                debug!(
                    %product_id,
                    seq,
                    %collection,
                    "statement-store allowance already allocated"
                );
            }
        }
        self.require_current_session(session)?;
        if let Err(reason) = self
            .track_statement_renewal_targets_for(
                session,
                vec![StatementRenewalTarget::ProductStatementAllowance {
                    product_id: product_id.to_string(),
                }],
            )
            .await
        {
            warn!(%product_id, %reason, "failed to record statement-store renewal target");
        }
        self.with_keys(session, |keys| {
            Ok(StatementStoreAllocation {
                secret: keys
                    .statement_allowance_key(product_id)?
                    .secret
                    .to_bytes()
                    .to_vec(),
                period,
            })
        })
    }

    /// Issue the product's Bulletin allowance key.
    pub async fn allocate_bulletin_allowance(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        policy: OnExistingAllowancePolicy,
    ) -> Result<Vec<u8>, AllowanceAllocationError> {
        use crate::runtime::statement_allowance::collection::PersonhoodCollection;
        use crate::runtime::statement_allowance::{
            self, claim_long_term_storage, fetch_bulletin_allowance, find_including_rings,
            wait_bulletin_authorization,
        };

        #[cfg(feature = "test-host")]
        if self.resource_controls.grants_allowances_unchecked() {
            return self.with_keys(session, |keys| {
                Ok(keys
                    .bulletin_allowance_key(product_id)?
                    .secret
                    .to_bytes()
                    .to_vec())
            });
        }
        let target = self.with_keys::<_, AllowanceAllocationError>(session, |keys| {
            Ok(keys.bulletin_allowance_key(product_id)?.public.to_bytes())
        })?;

        let bulletin_rpc = statement_allowance::rpc::RpcClient::new(
            self.services
                .bulletin
                .client("bulletin allowance")
                .await
                .map_err(|source| AllowanceAllocationError::ChainRpcClient {
                    context: "bulletin allowance client",
                    source,
                })?,
        );
        let current_allowance = fetch_bulletin_allowance(&bulletin_rpc, &target).await?;
        if matches!(policy, OnExistingAllowancePolicy::Ignore)
            && current_allowance.is_some_and(|allowance| allowance.available())
        {
            return self.with_keys(session, |keys| {
                Ok(keys
                    .bulletin_allowance_key(product_id)?
                    .secret
                    .to_bytes()
                    .to_vec())
            });
        }

        let people_client = self
            .services
            .statement_store
            .chain_client("bulletin allowance claim")
            .await?;
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
            chain_state: &chain.state,
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
            BULLETIN_AUTHORIZATION_WAIT,
        )
        .await?;
        debug!(
            %product_id,
            remained_size = authorization.remained_size,
            remained_transactions = authorization.remained_transactions,
            "Bulletin authorization visible"
        );
        self.with_keys(session, |keys| {
            Ok(keys
                .bulletin_allowance_key(product_id)?
                .secret
                .to_bytes()
                .to_vec())
        })
    }

    /// Fund the selected product account on the host's Asset Hub chain.
    pub async fn allocate_smart_contract_allowance(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        derivation_index: api::DerivationIndex,
        policy: OnExistingAllowancePolicy,
    ) -> Result<(), AllowanceAllocationError> {
        use truapi::latest::ChainIdentifier;

        use crate::host_logic::features;
        use crate::runtime::statement_allowance::collection::PersonhoodCollection;
        use crate::runtime::statement_allowance::{self, ChainClient, find_including_rings, pgas};

        let target = self.with_keys(session, |keys| {
            keys.product_keypair(&api::ProductAccountId {
                dot_ns_identifier: product_id.to_string(),
                derivation_index,
            })
            .map(|key| key.public.to_bytes())
        })?;

        let chains = features::supported_chains(self.services.platform.as_ref())
            .await
            .map_err(|err| AllowanceAllocationError::SupportedChains(err.reason))?;
        let asset_hub_genesis = features::genesis_for(&chains, ChainIdentifier::AssetHub)
            .ok_or(AllowanceAllocationError::ChainNotServed { chain: "Asset Hub" })?;
        let asset_hub_client = ChainClient::new(
            statement_allowance::rpc::RpcClient::new(subxt_rpcs::RpcClient::new(
                self.services
                    .chain
                    .rpc_client("PGAS allowance", &asset_hub_genesis)
                    .await
                    .map_err(|source| AllowanceAllocationError::ChainRpcClient {
                        context: "Asset Hub PGAS client",
                        source,
                    })?,
            )),
            asset_hub_genesis,
        );
        let asset_hub = self.services.chain_context.get(&asset_hub_client).await?;

        // Reusing a funded account preserves the person's daily claim slots.
        if matches!(policy, OnExistingAllowancePolicy::Ignore)
            && pgas::holds_a_full_claim(asset_hub_client.rpc(), &asset_hub.metadata, &target)
                .await?
        {
            debug!(%product_id, "PGAS allowance already funded; leaving it alone");
            self.require_current_session(session)?;
            return Ok(());
        }
        let network_suffix =
            statement_allowance::slot::read_network_suffix(asset_hub_client.rpc()).await?;

        let people_client = self
            .services
            .statement_store
            .chain_client("PGAS allowance ring")
            .await?;
        let people_rpc = people_client.rpc();
        let people = self.services.chain_context.get(&people_client).await?;

        let signer = self.personhood_signer(session).await?;
        // A PGAS claim uses the strongest available membership.
        let membership = find_including_rings(
            people_rpc,
            &people.metadata,
            &signer,
            &PersonhoodCollection::ALL,
            u32::MAX,
        )
        .await?
        .into_iter()
        .next()
        .ok_or(AllowanceAllocationError::MissingPersonhoodMembership { resource: "PGAS" })?;

        self.require_current_session(session)?;
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
        self.require_current_session(session)?;
        Ok(())
    }
}
