//! Trusted wallet-admin inspection; intentionally outside the product protocol.

use std::collections::HashSet;

use truapi::latest::{ChainIdentifier, GenericError};
use truapi_platform::normalize_product_identifier;

use super::{SigningHost, allowance_renewal};
use crate::host_logic::{
    features,
    product_account::{
        derive_product_keypair, derive_root_keypair_from_entropy, derive_sr25519_hard_path,
        index_bytes,
    },
};
use crate::runtime::statement_allowance::{
    inspection::{
        AccountLabels, ActivationGuard, AllowanceSection, ProductAccounts, ReadOnlyChain,
        WalletAllowanceSnapshot,
    },
    rpc::RpcClient,
};

const MAX_PRODUCT_IDS: usize = 32;

fn error(reason: impl ToString) -> GenericError {
    GenericError {
        reason: reason.to_string(),
    }
}

fn validate_products(product_ids: &[String]) -> Result<(), GenericError> {
    if product_ids.len() > MAX_PRODUCT_IDS {
        return Err(error(
            "wallet allowance inspection accepts at most 32 product IDs",
        ));
    }
    let mut seen = HashSet::with_capacity(product_ids.len());
    for product in product_ids {
        let normalized = normalize_product_identifier(product).map_err(error)?;
        if normalized != *product || !seen.insert(product) {
            return Err(error(
                "wallet allowance product IDs must be unique and canonical",
            ));
        }
    }
    Ok(())
}

impl SigningHost {
    /// Read real capacity and current occupancy, without refresh, proof, claims,
    /// registration, renewal, signing, or a write to local storage.
    pub(crate) async fn get_wallet_allowance_snapshot(
        &self,
        activation_id: &str,
        product_ids: Vec<String>,
    ) -> Result<WalletAllowanceSnapshot, GenericError> {
        self.check_identity_activation(activation_id)?;
        validate_products(&product_ids)?;
        let context = self.local_identity_context()?;
        let session = self
            .current_local_session()
            .ok_or_else(|| error("no active local identity"))?;
        let candidates = self
            .reserved_person_collection_candidates(&session)
            .map_err(error)?;
        let entropy = self.root_entropy().map_err(error)?;
        let root = derive_root_keypair_from_entropy(&entropy).map_err(error)?;
        let owner = root.public.to_bytes();
        if owner != session.public_key {
            return Err(error("local identity activation changed"));
        }
        let mut known_labels = AccountLabels::new();
        let identity = self.identity_keypair().map_err(error)?.public.to_bytes();
        known_labels.insert(identity, (None, "Wallet identity / SSO".to_string()));
        let mut products = Vec::with_capacity(product_ids.len());
        for product_id in &product_ids {
            let pgas = derive_product_keypair(&root, product_id, index_bytes(0))
                .map_err(error)?
                .public
                .to_bytes();
            let bulletin =
                derive_sr25519_hard_path(&entropy, &["allowance", "bulletin", product_id])
                    .map_err(error)?
                    .public
                    .to_bytes();
            let statement =
                derive_sr25519_hard_path(&entropy, &["allowance", "statement-store", product_id])
                    .map_err(error)?
                    .public
                    .to_bytes();
            known_labels.insert(
                statement,
                (Some(product_id.clone()), format!("Product: {product_id}")),
            );
            products.push(ProductAccounts {
                product_id: product_id.clone(),
                pgas,
                bulletin,
            });
        }
        drop(root);
        self.check_identity_activation(activation_id)?;
        let guard = || {
            self.check_identity_activation(activation_id)
                .map_err(|error| error.reason)
        };
        // The ledger is read strictly. A broken ledger cannot be silently
        // represented as no known device/product attribution.
        guard().map_err(error)?;
        let labels = allowance_renewal::inspection_labels(self, &entropy, owner).await;
        guard().map_err(error)?;
        drop(entropy);
        let labels = labels.map(|mut labels| {
            labels.extend(known_labels);
            labels
        });

        // Each chain pins one independent finalized block and fetches metadata,
        // policies, storage and clock from that block. No shared best-head cache.
        let (people, asset_hub, bulletin) = futures::join!(
            self.allowance_people(&guard),
            self.allowance_asset_hub(&guard),
            self.allowance_bulletin(&guard),
        );
        guard().map_err(error)?;
        let memberships = match &people {
            Ok(chain) => chain.memberships(&candidates).await,
            Err(reason) => Err(reason.clone()),
        };
        guard().map_err(error)?;
        let suffix = self.network_suffix.as_bytes();
        let (statement_store, pgas_claims, pgas_balances, bulletin_claims, bulletin_quotas) = futures::join!(
            async {
                match (&people, &memberships, &labels) {
                    (Ok(chain), Ok(memberships), Ok(labels)) => chain.section(
                        chain
                            .statement(&candidates, memberships, suffix, labels)
                            .await,
                    ),
                    (Err(reason), _, _) | (_, Err(reason), _) | (_, _, Err(reason)) => {
                        AllowanceSection::unavailable(reason)
                    }
                }
            },
            async {
                match (&asset_hub, &people, &memberships) {
                    (Ok(chain), Ok(people), Ok(memberships)) => chain.section(
                        chain
                            .pgas_claims(&candidates, memberships, &people.observation, suffix)
                            .await,
                    ),
                    (Err(reason), _, _) | (_, Err(reason), _) | (_, _, Err(reason)) => {
                        AllowanceSection::unavailable(reason)
                    }
                }
            },
            async {
                match &asset_hub {
                    Ok(chain) => chain.section(chain.pgas_balances(&products).await),
                    Err(reason) => AllowanceSection::unavailable(reason),
                }
            },
            async {
                match (&people, &memberships) {
                    (Ok(chain), Ok(memberships)) => chain.section(
                        chain
                            .bulletin_claims(&candidates, memberships, suffix)
                            .await,
                    ),
                    (Err(reason), _) | (_, Err(reason)) => AllowanceSection::unavailable(reason),
                }
            },
            async {
                match &bulletin {
                    Ok(chain) => chain.section(chain.bulletin_quotas(&products).await),
                    Err(reason) => AllowanceSection::unavailable(reason),
                }
            },
        );
        // A stale activation is a failed request, never a partly-unavailable
        // response which might still reveal the previous identity's sections.
        guard().map_err(error)?;
        Ok(WalletAllowanceSnapshot {
            schema_version: 1,
            identity_account_id: context.identity_account_id,
            network_suffix: self.network_suffix.clone(),
            product_ids,
            statement_store,
            pgas_claims,
            pgas_balances,
            bulletin_claims,
            bulletin_quotas,
        })
    }

    async fn allowance_people<'a>(
        &self,
        guard: ActivationGuard<'a>,
    ) -> Result<ReadOnlyChain<'a>, String> {
        guard()?;
        let client = self
            .services
            .statement_store
            .chain_client("wallet allowance inspection")
            .await;
        guard()?;
        ReadOnlyChain::open_client(client.map_err(|error| error.to_string())?, guard).await
    }

    async fn allowance_asset_hub<'a>(
        &self,
        guard: ActivationGuard<'a>,
    ) -> Result<ReadOnlyChain<'a>, String> {
        guard()?;
        let chains = features::supported_chains(self.platform.as_ref()).await;
        guard()?;
        let genesis = features::genesis_for(
            &chains.map_err(|error| error.reason)?,
            ChainIdentifier::AssetHub,
        )
        .ok_or_else(|| "Asset Hub is not configured".to_string())?;
        let client = self
            .services
            .chain
            .rpc_client("wallet allowance inspection", &genesis)
            .await;
        guard()?;
        let rpc = RpcClient::new(subxt_rpcs::RpcClient::new(
            client.map_err(|error| error.to_string())?,
        ));
        ReadOnlyChain::open(rpc, genesis, guard).await
    }

    async fn allowance_bulletin<'a>(
        &self,
        guard: ActivationGuard<'a>,
    ) -> Result<ReadOnlyChain<'a>, String> {
        guard()?;
        let genesis = self.services.bulletin.genesis_hash();
        if genesis == [0; 32] {
            return Err("Bulletin is not configured".to_string());
        }
        let client = self
            .services
            .bulletin
            .client("wallet allowance inspection")
            .await;
        guard()?;
        ReadOnlyChain::open(
            RpcClient::new(client.map_err(|error| error.to_string())?),
            genesis,
            guard,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_hints_reject_duplicates_noncanonical_and_unbounded_input() {
        assert!(validate_products(&["app.dot".to_string(), "app.dot".to_string()]).is_err());
        assert!(validate_products(&[" App.DOT ".to_string()]).is_err());
        assert!(validate_products(&["".to_string()]).is_err());
        let too_many = (0..=MAX_PRODUCT_IDS)
            .map(|index| format!("app{index}.dot"))
            .collect::<Vec<_>>();
        assert!(validate_products(&too_many).is_err());
    }
}
