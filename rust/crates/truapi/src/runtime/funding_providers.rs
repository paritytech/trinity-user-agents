//! The funding providers a session can be handed to, kept current against
//! dotNS.
//!
//! The host supplies the providers it offers, each with the Worker manifest it
//! shipped; candidates are answered from what dotNS last said about a provider,
//! and from that snapshot until dotNS has answered, so the list renders with no
//! chain read. Only the providers the host lists are offered: the list is the
//! host's to control. Every query re-checks them in the background through
//! the manifest cache, so a provider whose publisher changed, withdrew or
//! unpublished it is corrected within the cache lifetime.

use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Mutex},
};

use truapi::latest::FundingDirection;

use futures::lock::Mutex as AsyncMutex;
use parity_scale_codec::{Decode, Encode};

use super::{funding::lock, product_manifest::worker_manifest, services::RuntimeServices};
use crate::{
	host_logic::{
		funding::FundingSessionError,
		funding_providers::{FundingCandidate, FundingProviderEntry, LearnedSupport},
		worker_manifest::WorkerManifest,
	},
	platform::{CoreStorageKey, ProductId},
	unix_time::current_unix_millis,
};

/// A provider the host offers, with the Worker manifest it shipped already
/// read.
#[derive(Clone)]
struct OfferedProvider {
	product_id: ProductId,
	snapshot: Option<WorkerManifest>,
}

/// The host's funding providers and what each one serves.
#[derive(Default)]
pub struct FundingProviders {
	/// The providers, in the host's order.
	entries: Mutex<Vec<OfferedProvider>>,
	/// What dotNS last said each provider publishes: its Worker manifest, or
	/// `None` for nothing usable.
	live: Mutex<HashMap<ProductId, Option<WorkerManifest>>>,
	/// Providers whose manifest is being read.
	refreshing: Mutex<HashSet<ProductId>>,
	/// What providers' quote answers showed about what they serve, newest
	/// per ask, persisted under [`CoreStorageKey::FundingSupport`].
	learned: Mutex<Vec<LearnedSupport>>,
	/// Held across every load and write of the records, so writes land in
	/// order and none lands before the stored records are in; the flag
	/// records whether they are.
	learned_store: AsyncMutex<bool>,
}

impl FundingProviders {
	/// Every provider serving `direction`, in the host's order. What their
	/// recent quote answers showed is folded into what their manifests say.
	pub fn candidates(&self, direction: FundingDirection) -> Vec<FundingCandidate> {
		let learned = lock(&self.learned).clone();
		let now_ms = current_unix_millis();
		self.providers()
			.into_iter()
			.filter_map(|(provider_id, manifest)| {
				FundingCandidate::for_direction(
					&provider_id,
					manifest.as_ref(),
					&learned,
					direction,
					now_ms,
				)
			})
			.collect()
	}

	/// Every provider a quote is asked of and a session can be handed to:
	/// each whose manifest serves Funding, in the host's order, whatever routes
	/// it declares, since a provider can serve more or less than it last
	/// published. One with no manifest yet, or whose manifest was withdrawn,
	/// is neither asked nor chosen.
	pub fn quote_targets(&self) -> Vec<ProductId> {
		self.providers()
			.into_iter()
			.filter(|(_, manifest)| {
				manifest.as_ref().is_some_and(|manifest| manifest.funding.is_some())
			})
			.map(|(provider_id, _)| provider_id)
			.collect()
	}

	/// The host's providers in its order, each with the manifest the core
	/// goes by.
	fn providers(&self) -> Vec<(ProductId, Option<WorkerManifest>)> {
		let entries = lock(&self.entries).clone();
		entries
			.iter()
			.map(|entry| (entry.product_id.clone(), self.manifest(entry)))
			.collect()
	}

	/// Record what an answer showed, replacing what was known for the same
	/// ask and dropping what is no longer trusted.
	fn learn(&self, support: LearnedSupport) {
		let mut learned = lock(&self.learned);
		let now_ms = support.learned_at_ms;
		learned.retain(|known| known.is_fresh(now_ms) && !known.same_ask(&support));
		learned.push(support);
	}

	/// Whether a session can be handed to `provider_id`: it is a quote target.
	pub fn is_quote_target(&self, provider_id: &ProductId) -> bool {
		self.quote_targets().contains(provider_id)
	}

	/// The manifest the core goes by for `entry`: dotNS's answer once it has
	/// one, the host's snapshot until then.
	fn manifest(&self, entry: &OfferedProvider) -> Option<WorkerManifest> {
		match lock(&self.live).get(&entry.product_id) {
			Some(live) => live.clone(),
			None => entry.snapshot.clone(),
		}
	}
}

impl RuntimeServices {
	/// Replace the funding providers the host offers, and start reading their
	/// manifests. A shipped manifest that does not parse is refused, and
	/// nothing changes.
	pub fn set_funding_providers(
		self: &Arc<Self>,
		entries: Vec<FundingProviderEntry>,
	) -> Result<(), FundingSessionError> {
		let offered = offered_providers(entries)?;
		let providers = &self.funding_providers();
		lock(&providers.live)
			.retain(|product_id, _| offered.iter().any(|entry| &entry.product_id == product_id));
		*lock(&providers.entries) = offered;
		self.refresh_funding_providers();
		Ok(())
	}

	/// The providers a session in `direction` can be handed to.
	pub fn funding_candidates(
		self: &Arc<Self>,
		direction: FundingDirection,
	) -> Vec<FundingCandidate> {
		self.refresh_funding_providers();
		self.funding_providers().candidates(direction)
	}

	/// Read every host provider's manifest that is not already being read. A
	/// read that fails keeps what the core had, since it says nothing about
	/// the provider.
	pub fn refresh_funding_providers(self: &Arc<Self>) {
		let services = self.clone();
		(self.spawner)(Box::pin(async move {
			let mut loaded = services.funding_providers().learned_store.lock().await;
			services.load_learned_support(&mut loaded).await;
		}));
		let entries = lock(&self.funding_providers().entries).clone();
		for entry in entries {
			if !lock(&self.funding_providers().refreshing).insert(entry.product_id.clone()) {
				continue;
			}
			let services = self.clone();
			(self.spawner)(Box::pin(async move {
				services.read_provider_manifest(&entry.product_id).await;
			}));
		}
	}

	/// Record what a provider's answer showed about what it serves, and
	/// persist the records once the stored ones are loaded, so a write never
	/// replaces records it has not read.
	pub fn learn_funding_support(self: &Arc<Self>, support: LearnedSupport) {
		self.funding_providers().learn(support);
		let services = self.clone();
		(self.spawner)(Box::pin(async move {
			// Each write takes the records as they are once it holds the
			// lock, so the last one to land is the newest.
			let mut loaded = services.funding_providers().learned_store.lock().await;
			if !services.load_learned_support(&mut loaded).await {
				return;
			}
			let records = lock(&services.funding_providers().learned).clone();
			if let Err(error) = services
				.platform
				.write_core_storage(CoreStorageKey::FundingSupport, records.encode())
				.await
			{
				tracing::warn!(reason = %error.reason, "storing what funding providers serve failed");
			}
		}));
	}

	/// Load what earlier answers showed unless `loaded` says it is in,
	/// keeping what is still trusted and anything learned since. Returns
	/// whether the records are loaded; a read that fails is tried again on
	/// the next call.
	async fn load_learned_support(&self, loaded: &mut bool) -> bool {
		if *loaded {
			return true;
		}
		let stored = match self.platform.read_core_storage(CoreStorageKey::FundingSupport).await {
			Ok(Some(bytes)) => {
				Vec::<LearnedSupport>::decode(&mut bytes.as_slice()).unwrap_or_else(|error| {
					tracing::warn!(%error, "dropping undecodable funding provider records");
					Vec::new()
				})
			},
			Ok(None) => Vec::new(),
			Err(error) => {
				tracing::warn!(reason = %error.reason, "reading what funding providers serve failed");
				return false;
			},
		};
		let now_ms = current_unix_millis();
		let mut learned = lock(&self.funding_providers().learned);
		for support in stored.into_iter().filter(|support| support.is_fresh(now_ms)) {
			if learned.iter().all(|known| !known.same_ask(&support)) {
				learned.push(support);
			}
		}
		*loaded = true;
		true
	}

	/// Read `product_id`'s Worker manifest into what the core goes by.
	async fn read_provider_manifest(&self, product_id: &ProductId) {
		let read = worker_manifest(self, self.platform.as_ref(), product_id).await;
		let providers = &self.funding_providers();
		if let Ok(manifest) = read {
			lock(&providers.live).insert(product_id.clone(), manifest);
		}
		lock(&providers.refreshing).remove(product_id);
	}
}

/// The host's `entries` with their shipped manifests read, in its order,
/// each product once.
fn offered_providers(
	entries: Vec<FundingProviderEntry>,
) -> Result<Vec<OfferedProvider>, FundingSessionError> {
	let mut offered: Vec<OfferedProvider> = Vec::with_capacity(entries.len());
	for entry in entries {
		let product_id = entry.product_id;
		let snapshot = entry
			.worker_manifest
			.as_deref()
			.map(WorkerManifest::parse)
			.transpose()
			.map_err(|reason| FundingSessionError::InvalidProvider {
				reason: format!("{product_id}: {reason}"),
			})?;
		if offered.iter().all(|kept| kept.product_id != product_id) {
			offered.push(OfferedProvider { product_id, snapshot });
		}
	}
	Ok(offered)
}

#[cfg(test)]
mod tests {
	use super::*;

	use truapi::latest::FundingRail;

	use crate::host_logic::worker_manifest::FundingRoute;

	fn manifest(routes: &str) -> String {
		format!(
			r#"{{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{{"funding":{{"routes":[{routes}]}}}}}}"#
		)
	}

	const CARD_IN: &str = r#"{"mode":"CARD","directions":["In"],"assets":["EUR"]}"#;
	const CRYPTO_OUT: &str = r#"{"mode":"CRYPTO","directions":["Out"],"assets":["USDT"]}"#;

	fn providers(entries: &[(&str, Option<String>)]) -> FundingProviders {
		let providers = FundingProviders::default();
		*lock(&providers.entries) = offered_providers(
			entries
				.iter()
				.map(|(product_id, worker_manifest)| FundingProviderEntry {
					product_id: ProductId::for_test(product_id),
					worker_manifest: worker_manifest.clone(),
				})
				.collect(),
		)
		.expect("offered");
		providers
	}

	fn ids(candidates: Vec<FundingCandidate>) -> Vec<ProductId> {
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
		]);

		let outbound = providers.candidates(FundingDirection::Out);

		assert_eq!(
			(
				ids(providers.candidates(FundingDirection::In)),
				outbound.iter().map(|candidate| candidate.routes.clone()).collect::<Vec<_>>(),
			),
			(
				vec![ProductId::for_test("card.dot"), ProductId::for_test("both.dot")],
				vec![vec![FundingRoute {
					rail: FundingRail::Crypto,
					directions: vec![FundingDirection::Out],
					assets: vec!["USDT".to_string()],
					networks: None,
					countries: None,
				}]],
			)
		);
	}

	// A snapshot the core cannot read is the host's mistake, so it is refused
	// when offered rather than leaving the provider silently missing.
	#[test]
	fn an_unreadable_snapshot_is_refused_when_offered() {
		let offered = |worker_manifest: &str| {
			offered_providers(vec![FundingProviderEntry {
				product_id: ProductId::for_test("card.dot"),
				worker_manifest: Some(worker_manifest.to_string()),
			}])
			.map(|offered| offered.len())
			.map_err(|error| matches!(error, FundingSessionError::InvalidProvider { .. }))
		};

		assert_eq!((offered(&manifest(CARD_IN)), offered("{")), (Ok(1), Err(true)));
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
			(ProductId::for_test("withdrawn.dot"), None),
			(ProductId::for_test("changed.dot"), live(CRYPTO_OUT)),
			(ProductId::for_test("unshipped.dot"), live(CARD_IN)),
		]);

		assert_eq!(
			(
				ids(providers.candidates(FundingDirection::In)),
				ids(providers.candidates(FundingDirection::Out)),
			),
			(vec![ProductId::for_test("unshipped.dot")], vec![ProductId::for_test("changed.dot")])
		);
	}

	// Quotes and selection cover the same providers: each whose manifest
	// serves Funding, whatever its routes say, and none still waiting for its
	// manifest or whose manifest was withdrawn.
	#[test]
	fn quotes_go_to_the_providers_a_session_can_be_handed_to() {
		let providers = providers(&[
			("card.dot", Some(manifest(CARD_IN))),
			("unshipped.dot", None),
			("withdrawn.dot", Some(manifest(CARD_IN))),
		]);
		lock(&providers.live).insert(ProductId::for_test("withdrawn.dot"), None);

		assert_eq!(
			(
				providers.quote_targets(),
				["card.dot", "unshipped.dot", "withdrawn.dot"]
					.map(|provider_id| providers.is_quote_target(&ProductId::for_test(provider_id))),
			),
			(vec![ProductId::for_test("card.dot")], [true, false, false])
		);
	}
}
