//! The funding providers a session can be handed to, kept current against
//! dotNS.
//!
//! The host supplies the providers it offers, each with the Worker manifest it
//! shipped; candidates are answered from what dotNS last said about a provider,
//! and from that snapshot until dotNS has answered, so the list renders with no
//! chain read. Products published to browse whose Worker manifest serves
//! Funding join after the host's own, once dotNS has answered for them. Every
//! query re-checks both in the background through the manifest cache, so a
//! provider whose publisher changed, withdrew or unpublished it is corrected
//! within the cache lifetime.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use truapi::latest::FundingDirection;

use futures::lock::Mutex as AsyncMutex;
use parity_scale_codec::{Decode, Encode};

use super::browse::published_products;
use super::product_manifest::worker_manifest;
use super::services::RuntimeServices;
use crate::host_logic::worker_manifest::WorkerManifest;
use crate::host_logic::funding::FundingSessionError;
use crate::host_logic::funding_providers::{FundingCandidate, FundingProviderEntry, LearnedSupport};
use crate::platform::{CoreStorageKey, ProductContext, ProductExecutionKind};
use crate::unix_time::current_unix_millis;

/// The host's funding providers and what each one serves.
#[derive(Default)]
pub struct FundingProviders {
    /// The providers, in the host's order, with normalized product ids.
    entries: Mutex<Vec<FundingProviderEntry>>,
    /// What dotNS last said each provider publishes: its Worker manifest, or
    /// `None` for nothing usable.
    live: Mutex<HashMap<String, Option<WorkerManifest>>>,
    /// Products published to browse, in the Publishers' order.
    discovered: Mutex<Vec<String>>,
    /// Providers whose manifest is being read.
    refreshing: Mutex<HashSet<String>>,
    /// Whether the browse list is being read.
    discovering: AtomicBool,
    /// What providers' quote answers showed about what they serve, newest
    /// per ask, persisted under [`CoreStorageKey::FundingSupport`].
    learned: Mutex<Vec<LearnedSupport>>,
    /// Whether the persisted records were loaded.
    learned_loaded: AtomicBool,
    /// Held while the records are written, so writes land in order.
    learned_writes: AsyncMutex<()>,
}

impl FundingProviders {
    /// Every provider serving `direction`: the host's in its order, then
    /// those published to browse that the host does not list. What their
    /// recent quote answers showed is folded into what their manifests say.
    pub fn candidates(&self, direction: FundingDirection) -> Vec<FundingCandidate> {
        let learned = lock(&self.learned).clone();
        let now_ms = current_unix_millis();
        self.providers()
            .into_iter()
            .filter_map(|(provider_id, manifest)| {
                FundingCandidate::for_direction(&provider_id, manifest.as_ref(), &learned, direction, now_ms)
            })
            .collect()
    }

    /// Every provider a quote is asked of, whatever its manifest says it
    /// serves, since a provider can serve more or less than it last
    /// published: the host's, and those published to browse whose manifest
    /// serves Funding.
    pub fn quote_targets(&self) -> Vec<String> {
        let entries = lock(&self.entries).len();
        self.providers()
            .into_iter()
            .enumerate()
            .filter(|(index, (_, manifest))| {
                *index < entries || manifest.as_ref().is_some_and(|manifest| manifest.funding.is_some())
            })
            .map(|(_, (provider_id, _))| provider_id)
            .collect()
    }

    /// The host's providers in its order, then the browse ones it does not
    /// list, each with the manifest the core goes by.
    fn providers(&self) -> Vec<(String, Option<WorkerManifest>)> {
        let entries = lock(&self.entries).clone();
        let discovered = lock(&self.discovered).clone();
        let offered = entries
            .iter()
            .map(|entry| (entry.product_id.clone(), self.manifest(entry)));
        let published = discovered
            .into_iter()
            .filter(|product_id| entries.iter().all(|entry| &entry.product_id != product_id))
            .map(|product_id| {
                let manifest = lock(&self.live).get(&product_id).cloned().flatten();
                (product_id, manifest)
            });
        offered.chain(published).collect()
    }

    /// Record what an answer showed, replacing what was known for the same
    /// ask and dropping what is no longer trusted.
    fn learn(&self, support: LearnedSupport) {
        let mut learned = lock(&self.learned);
        let now_ms = support.learned_at_ms;
        learned.retain(|known| known.is_fresh(now_ms) && !known.same_ask(&support));
        learned.push(support);
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
        let discovered = lock(&providers.discovered).clone();
        lock(&providers.live).retain(|product_id, _| {
            normalized.iter().any(|entry| &entry.product_id == product_id)
                || discovered.contains(product_id)
        });
        *lock(&providers.entries) = normalized;
        self.refresh_funding_providers();
        Ok(())
    }

    /// The providers a session in `direction` can be handed to.
    pub fn funding_candidates(self: &Arc<Self>, direction: FundingDirection) -> Vec<FundingCandidate> {
        self.refresh_funding_providers();
        self.funding_providers.candidates(direction)
    }

    /// Read every host provider's manifest that is not already being read,
    /// and the browse list with the manifests of what it lists. A read that
    /// fails keeps what the core had, since it says nothing about the
    /// provider.
    pub fn refresh_funding_providers(self: &Arc<Self>) {
        if !self.funding_providers.learned_loaded.swap(true, Ordering::AcqRel) {
            let services = self.clone();
            (self.spawner)(Box::pin(async move {
                services.load_learned_support().await;
            }));
        }
        let entries = lock(&self.funding_providers.entries).clone();
        for entry in entries {
            if !lock(&self.funding_providers.refreshing).insert(entry.product_id.clone()) {
                continue;
            }
            let services = self.clone();
            (self.spawner)(Box::pin(async move {
                services.read_provider_manifest(&entry.product_id).await;
            }));
        }
        if self.funding_providers.discovering.swap(true, Ordering::AcqRel) {
            return;
        }
        let services = self.clone();
        (self.spawner)(Box::pin(async move {
            services.discover_funding_providers().await;
            services
                .funding_providers
                .discovering
                .store(false, Ordering::Release);
        }));
    }

    /// Read the browse list, then the manifest of each product on it, one at
    /// a time so a long list does not open a burst of chain reads.
    async fn discover_funding_providers(&self) {
        let published = match published_products(self, self.platform.as_ref()).await {
            Ok(published) => published,
            Err(reason) => {
                tracing::warn!(%reason, "reading the browse list failed");
                return;
            }
        };
        tracing::debug!(count = published.len(), "products published to browse");
        let providers = &self.funding_providers;
        *lock(&providers.discovered) = published.clone();
        for product_id in published {
            if lock(&providers.refreshing).insert(product_id.clone()) {
                self.read_provider_manifest(&product_id).await;
            }
        }
    }

    /// Record what a provider's answer showed about what it serves, and
    /// persist the records.
    pub fn learn_funding_support(self: &Arc<Self>, support: LearnedSupport) {
        self.funding_providers.learn(support);
        let services = self.clone();
        (self.spawner)(Box::pin(async move {
            // Each write takes the records as they are once it holds the
            // lock, so the last one to land is the newest.
            let _write = services.funding_providers.learned_writes.lock().await;
            let records = lock(&services.funding_providers.learned).clone();
            if let Err(error) = services
                .platform
                .write_core_storage(CoreStorageKey::FundingSupport, records.encode())
                .await
            {
                tracing::warn!(reason = %error.reason, "storing what funding providers serve failed");
            }
        }));
    }

    /// Load what earlier answers showed, keeping what is still trusted and
    /// anything learned since the load began.
    async fn load_learned_support(&self) {
        let stored = match self.platform.read_core_storage(CoreStorageKey::FundingSupport).await {
            Ok(Some(bytes)) => Vec::<LearnedSupport>::decode(&mut bytes.as_slice()).unwrap_or_default(),
            Ok(None) => Vec::new(),
            Err(error) => {
                tracing::warn!(reason = %error.reason, "reading what funding providers serve failed");
                return;
            }
        };
        let now_ms = current_unix_millis();
        let mut learned = lock(&self.funding_providers.learned);
        for support in stored.into_iter().filter(|support| support.is_fresh(now_ms)) {
            if learned.iter().all(|known| !known.same_ask(&support)) {
                learned.push(support);
            }
        }
    }

    /// Read `product_id`'s Worker manifest into what the core goes by.
    async fn read_provider_manifest(&self, product_id: &str) {
        let read = worker_manifest(self, self.platform.as_ref(), product_id).await;
        let providers = &self.funding_providers;
        if let Ok(manifest) = read {
            lock(&providers.live).insert(product_id.to_string(), manifest);
        }
        lock(&providers.refreshing).remove(product_id);
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
            r#"{{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{{"funding":{{"routes":[{routes}]}}}}}}"#
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

    // Published providers join after the host's own, once dotNS has said what
    // they serve; one the host also lists appears once, in the host's place.
    #[test]
    fn published_providers_follow_the_hosts_own() {
        let providers = providers(&[("card.dot", Some(manifest(CARD_IN)))]);
        let live = |routes: &str| WorkerManifest::parse(&manifest(routes)).ok();
        *lock(&providers.discovered) = ["unread.dot", "found.dot", "card.dot", "nothing.dot"]
            .map(str::to_string)
            .to_vec();
        lock(&providers.live).extend([
            ("found.dot".to_string(), live(CARD_IN)),
            ("card.dot".to_string(), live(CARD_IN)),
            ("nothing.dot".to_string(), None),
        ]);

        assert_eq!(
            ids(providers.candidates(FundingDirection::In)),
            vec!["card.dot".to_string(), "found.dot".to_string()]
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
