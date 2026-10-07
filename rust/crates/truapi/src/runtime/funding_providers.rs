//! The funding providers the host offers, kept current against dotNS.
//!
//! The host supplies the list once it knows it, each entry with the Worker
//! manifest it shipped. Candidates are answered from what dotNS last said
//! about a provider, and from that snapshot until dotNS has answered, so the
//! list renders with no chain read. Every query re-checks the providers in the
//! background through the manifest cache, so a provider whose publisher
//! changed or withdrew its manifest is corrected within the cache lifetime.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use truapi::latest::FundingDirection;

use super::product_manifest::worker_manifest;
use super::services::RuntimeServices;
use crate::host_logic::worker_manifest::WorkerManifest;
use crate::host_logic::funding::FundingSessionError;
use crate::host_logic::funding_providers::{FundingCandidate, FundingProviderEntry};
use crate::platform::{ProductContext, ProductExecutionKind};

/// The host's funding providers and what each one serves.
#[derive(Default)]
pub struct FundingProviders {
    /// The providers, in the host's order, with normalized product ids.
    entries: Mutex<Vec<FundingProviderEntry>>,
    /// What dotNS last said each provider publishes: its Worker manifest, or
    /// `None` for nothing usable.
    live: Mutex<HashMap<String, Option<WorkerManifest>>>,
    /// Providers whose manifest is being read.
    refreshing: Mutex<HashSet<String>>,
}

impl FundingProviders {
    /// Every provider serving `direction`, in the host's order.
    pub fn candidates(&self, direction: FundingDirection) -> Vec<FundingCandidate> {
        let entries = lock(&self.entries).clone();
        entries
            .iter()
            .filter_map(|entry| {
                FundingCandidate::for_direction(
                    &entry.product_id,
                    &self.manifest(entry)?,
                    direction,
                )
            })
            .collect()
    }

    /// Whether `provider_id` serves `direction`.
    pub fn is_candidate(&self, provider_id: &str, direction: FundingDirection) -> bool {
        self.candidates(direction)
            .iter()
            .any(|candidate| candidate.provider_id == provider_id)
    }

    /// The manifest the core goes by for `entry`: dotNS's answer once it has
    /// one, the host's snapshot until then.
    fn manifest(&self, entry: &FundingProviderEntry) -> Option<WorkerManifest> {
        if let Some(live) = lock(&self.live).get(&entry.product_id) {
            return live.clone();
        }
        WorkerManifest::parse(entry.worker_manifest.as_deref()?)
            .inspect_err(|reason| {
                tracing::warn!(provider = %entry.product_id, %reason, "the host's funding provider snapshot is unusable");
            })
            .ok()
    }
}

impl RuntimeServices {
    /// Replace the funding providers the host offers, and start reading their
    /// manifests. A product id that does not normalize is refused, and nothing
    /// changes.
    pub fn set_funding_providers(
        self: &Arc<Self>,
        entries: Vec<FundingProviderEntry>,
    ) -> Result<(), FundingSessionError> {
        let mut normalized: Vec<FundingProviderEntry> = Vec::with_capacity(entries.len());
        for entry in entries {
            let product_id =
                ProductContext::new_with_execution(entry.product_id, ProductExecutionKind::Worker)
                    .map_err(|error| FundingSessionError::InvalidProvider {
                        reason: error.to_string(),
                    })?
                    .product_id;
            if normalized.iter().all(|kept| kept.product_id != product_id) {
                normalized.push(FundingProviderEntry {
                    product_id,
                    worker_manifest: entry.worker_manifest,
                });
            }
        }
        let providers = &self.funding_providers;
        lock(&providers.live)
            .retain(|product_id, _| normalized.iter().any(|entry| &entry.product_id == product_id));
        *lock(&providers.entries) = normalized;
        self.refresh_funding_providers();
        Ok(())
    }

    /// The providers a session in `direction` can be handed to.
    pub fn funding_candidates(self: &Arc<Self>, direction: FundingDirection) -> Vec<FundingCandidate> {
        self.refresh_funding_providers();
        self.funding_providers.candidates(direction)
    }

    /// Read every provider's manifest that is not already being read. A read
    /// that fails keeps what the core had, since it says nothing about the
    /// provider.
    fn refresh_funding_providers(self: &Arc<Self>) {
        let entries = lock(&self.funding_providers.entries).clone();
        for entry in entries {
            if !lock(&self.funding_providers.refreshing).insert(entry.product_id.clone()) {
                continue;
            }
            let services = self.clone();
            (self.spawner)(Box::pin(async move {
                let product_id = entry.product_id;
                let read = worker_manifest(&services, services.platform.as_ref(), &product_id).await;
                let providers = &services.funding_providers;
                if let Ok(manifest) = read {
                    lock(&providers.live).insert(product_id.clone(), manifest);
                }
                lock(&providers.refreshing).remove(&product_id);
            }));
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::host_logic::worker_manifest::{FundingMode, FundingRoute, RouteDirection};

    fn manifest(routes: &str) -> String {
        format!(
            r#"{{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{{"funding":{{"routes":[{routes}],"quote":{{"via":"worker"}}}}}}}}"#
        )
    }

    const CARD_IN: &str = r#"{"mode":"CARD","directions":["In"],"assets":["EUR"]}"#;
    const CRYPTO_OUT: &str = r#"{"mode":"CRYPTO","directions":["Out"],"assets":["USDT"]}"#;

    fn providers(entries: &[(&str, Option<String>)]) -> FundingProviders {
        let providers = FundingProviders::default();
        *lock(&providers.entries) = entries
            .iter()
            .map(|(product_id, worker_manifest)| FundingProviderEntry {
                product_id: product_id.to_string(),
                worker_manifest: worker_manifest.clone(),
            })
            .collect();
        providers
    }

    fn ids(candidates: Vec<FundingCandidate>) -> Vec<String> {
        candidates.into_iter().map(|candidate| candidate.provider_id).collect()
    }

    // The list renders from what the host shipped before any chain read, in
    // the host's order, with only the routes that serve the direction.
    #[test]
    fn candidates_come_from_the_snapshot_until_dotns_answers() {
        let providers = providers(&[
            ("card.dot", Some(manifest(CARD_IN))),
            ("both.dot", Some(manifest(&format!("{CARD_IN},{CRYPTO_OUT}")))),
            ("unshipped.dot", None),
            ("broken.dot", Some("{".to_string())),
        ]);

        let outbound = providers.candidates(FundingDirection::Out);

        assert_eq!(
            (
                ids(providers.candidates(FundingDirection::In)),
                outbound.iter().map(|candidate| candidate.routes.clone()).collect::<Vec<_>>(),
            ),
            (
                vec!["card.dot".to_string(), "both.dot".to_string()],
                vec![vec![FundingRoute {
                    mode: FundingMode::Crypto,
                    directions: vec![RouteDirection::Out],
                    assets: vec!["USDT".to_string()],
                    countries: None,
                    requires_account: false,
                }]],
            )
        );
    }

    // A stale snapshot is corrected by what the provider publishes now,
    // including when it stopped serving Funding.
    #[test]
    fn dotns_answer_wins_over_the_snapshot() {
        let providers = providers(&[
            ("withdrawn.dot", Some(manifest(CARD_IN))),
            ("changed.dot", Some(manifest(CARD_IN))),
            ("unshipped.dot", None),
        ]);
        let live = |routes: &str| WorkerManifest::parse(&manifest(routes)).ok();
        lock(&providers.live).extend([
            ("withdrawn.dot".to_string(), None),
            ("changed.dot".to_string(), live(CRYPTO_OUT)),
            ("unshipped.dot".to_string(), live(CARD_IN)),
        ]);

        assert_eq!(
            (
                ids(providers.candidates(FundingDirection::In)),
                ids(providers.candidates(FundingDirection::Out)),
            ),
            (vec!["unshipped.dot".to_string()], vec!["changed.dot".to_string()])
        );
    }
}
