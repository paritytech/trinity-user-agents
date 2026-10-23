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

use std::{
	collections::{HashMap, HashSet},
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
	},
};

use truapi::latest::{FundingDirection, FundingRail};

use futures::lock::Mutex as AsyncMutex;
use parity_scale_codec::{Decode, Encode};

use super::{
	browse::published_products, funding::lock, product_manifest::worker_manifest,
	services::RuntimeServices,
};
use crate::{
	host_logic::{
		funding::FundingSessionError,
		funding_providers::{
			FundingAmountLimits, FundingCandidate, FundingCountry, FundingProviderEntry,
			FundingProviderOrigin, LearnedSupport, LimitScope, amount_limits, country_support,
		},
		worker_manifest::WorkerManifest,
	},
	platform::{CoreStorageKey, ProductId},
	unix_time::current_unix_millis,
};

/// A provider the host offers, with its product id normalized and the
/// Worker manifest it shipped already read.
#[derive(Clone)]
struct OfferedProvider {
	product_id: String,
	snapshot: Option<WorkerManifest>,
	origin: FundingProviderOrigin,
}

/// The host's funding providers and what each one serves.
#[derive(Default)]
pub struct FundingProviders {
	/// The providers, in the host's order, with normalized product ids.
	entries: Mutex<Vec<OfferedProvider>>,
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
	/// Held across every load and write of the records, so writes land in
	/// order and none lands before the stored records are in; the flag
	/// records whether they are.
	learned_store: AsyncMutex<bool>,
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
	/// each whose manifest serves Funding, the host's first, whatever routes
	/// it declares, since a provider can serve more or less than it last
	/// published. One with no manifest yet, or whose manifest was withdrawn,
	/// is neither asked nor chosen.
	pub fn quote_targets(&self) -> Vec<String> {
		self.providers()
			.into_iter()
			.filter(|(_, manifest)| {
				manifest.as_ref().is_some_and(|manifest| manifest.funding.is_some())
			})
			.map(|(provider_id, _)| provider_id)
			.collect()
	}

	/// The host's providers in its order, then the browse ones it does not
	/// list, each with the manifest the core goes by.
	fn providers(&self) -> Vec<(String, Option<WorkerManifest>)> {
		let entries = lock(&self.entries).clone();
		let discovered = lock(&self.discovered).clone();
		let offered = entries.iter().map(|entry| (entry.product_id.clone(), self.manifest(entry)));
		let published = discovered
			.into_iter()
			.filter(|product_id| entries.iter().all(|entry| &entry.product_id != product_id))
			.map(|product_id| {
				let manifest = lock(&self.live).get(&product_id).cloned().flatten();
				(product_id, manifest)
			});
		offered.chain(published).collect()
	}

	/// Keep `published` as the browse list, normalized like the host's
	/// entries so a product on both is one provider, and return it. An id that
	/// does not normalize is left out.
	fn set_discovered(&self, published: Vec<String>) -> Vec<String> {
		let mut normalized: Vec<String> = Vec::with_capacity(published.len());
		for product_id in published {
			match normalized_product_id(product_id) {
				Ok(product_id) if !normalized.contains(&product_id) => normalized.push(product_id),
				Ok(_) => {},
				Err(reason) => tracing::debug!(%reason, "skipping a browse product id"),
			}
		}
		*lock(&self.discovered) = normalized.clone();
		normalized
	}

	/// Record what an answer showed, replacing what was known for the same
	/// ask and dropping what is no longer trusted.
	fn learn(&self, mut support: LearnedSupport) {
		let mut learned = lock(&self.learned);
		let now_ms = support.learned_at_ms;
		if let Some(known) =
			learned.iter().find(|known| known.is_fresh(now_ms) && known.same_ask(&support))
		{
			support.keep_limits_of(known);
		}
		learned.retain(|known| known.is_fresh(now_ms) && !known.same_ask(&support));
		learned.push(support);
	}

	/// The amounts a session can take in `scope`, across every provider a
	/// quote is asked of.
	pub fn amount_limits(&self, scope: LimitScope<'_>) -> FundingAmountLimits {
		let learned = lock(&self.learned).clone();
		amount_limits(&self.quote_targets(), &learned, scope, current_unix_millis())
	}

	/// What the providers' manifests and recent quote answers say about each
	/// country for `direction`, `rail` and `asset`.
	pub fn countries(
		&self,
		direction: FundingDirection,
		rail: FundingRail,
		asset: &str,
	) -> Vec<FundingCountry> {
		let learned = lock(&self.learned).clone();
		country_support(&self.providers(), &learned, direction, rail, asset, current_unix_millis())
	}

	/// Whether a session can be handed to `provider_id`: it is a quote target.
	pub fn is_quote_target(&self, provider_id: &str) -> bool {
		self.quote_targets().iter().any(|target| target == provider_id)
	}

	/// The manifest the core goes by for `entry`: the host's for a bundled
	/// provider; otherwise dotNS's answer once it has one, the host's
	/// snapshot until then.
	fn manifest(&self, entry: &OfferedProvider) -> Option<WorkerManifest> {
		if entry.origin == FundingProviderOrigin::Bundled {
			return entry.snapshot.clone();
		}
		match lock(&self.live).get(&entry.product_id) {
			Some(live) => live.clone(),
			None => entry.snapshot.clone(),
		}
	}
}

impl RuntimeServices {
	/// Replace the funding providers the host offers, and start reading their
	/// manifests. A product id that does not normalize, or a shipped manifest
	/// that does not parse, is refused, and nothing changes.
	pub fn set_funding_providers(
		self: &Arc<Self>,
		entries: Vec<FundingProviderEntry>,
	) -> Result<(), FundingSessionError> {
		let normalized = offered_providers(entries)?;
		let providers = &self.funding_providers();
		let discovered = lock(&providers.discovered).clone();
		lock(&providers.live).retain(|product_id, _| {
			normalized.iter().any(|entry| &entry.product_id == product_id) ||
				discovered.contains(product_id)
		});
		*lock(&providers.entries) = normalized;
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

	/// Read every host provider's manifest that is not already being read,
	/// and the browse list with the manifests of what it lists. A read that
	/// fails keeps what the core had, since it says nothing about the
	/// provider.
	pub fn refresh_funding_providers(self: &Arc<Self>) {
		let services = self.clone();
		(self.spawner)(Box::pin(async move {
			let mut loaded = services.funding_providers().learned_store.lock().await;
			services.load_learned_support(&mut loaded).await;
		}));
		let entries = lock(&self.funding_providers().entries).clone();
		for entry in entries
			.into_iter()
			.filter(|entry| entry.origin == FundingProviderOrigin::Published)
		{
			if !lock(&self.funding_providers().refreshing).insert(entry.product_id.clone()) {
				continue;
			}
			let services = self.clone();
			(self.spawner)(Box::pin(async move {
				services.read_provider_manifest(&entry.product_id).await;
			}));
		}
		if self.funding_providers().discovering.swap(true, Ordering::AcqRel) {
			return;
		}
		let services = self.clone();
		(self.spawner)(Box::pin(async move {
			services.discover_funding_providers().await;
			services.funding_providers().discovering.store(false, Ordering::Release);
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
			},
		};
		tracing::debug!(count = published.len(), "products published to browse");
		let providers = &self.funding_providers();
		let published = providers.set_discovered(published);
		for product_id in published {
			if lock(&providers.refreshing).insert(product_id.clone()) {
				self.read_provider_manifest(&product_id).await;
			}
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
	async fn read_provider_manifest(&self, product_id: &str) {
		let read = match product_id.parse::<ProductId>() {
			Ok(provider) => worker_manifest(self, self.platform.as_ref(), &provider).await,
			Err(error) => Err(error.to_string()),
		};
		let providers = &self.funding_providers();
		if let Ok(manifest) = read {
			lock(&providers.live).insert(product_id.to_string(), manifest);
		}
		lock(&providers.refreshing).remove(product_id);
	}
}

/// The host's `entries` normalized and with their shipped manifests read, in
/// its order, each product once.
fn offered_providers(
	entries: Vec<FundingProviderEntry>,
) -> Result<Vec<OfferedProvider>, FundingSessionError> {
	let mut offered: Vec<OfferedProvider> = Vec::with_capacity(entries.len());
	for entry in entries {
		let product_id = normalized_product_id(entry.product_id)
			.map_err(|reason| FundingSessionError::InvalidProvider { reason })?;
		let snapshot = entry
			.worker_manifest
			.as_deref()
			.map(WorkerManifest::parse)
			.transpose()
			.map_err(|reason| FundingSessionError::InvalidProvider {
				reason: format!("{product_id}: {reason}"),
			})?;
		if entry.origin == FundingProviderOrigin::Bundled && snapshot.is_none() {
			return Err(FundingSessionError::InvalidProvider {
				reason: format!("{product_id}: a bundled provider needs its manifest"),
			});
		}
		if offered.iter().all(|kept| kept.product_id != product_id) {
			offered.push(OfferedProvider { product_id, snapshot, origin: entry.origin });
		}
	}
	Ok(offered)
}

/// `product_id` as the core names a provider's worker, so ids from the host
/// and from browse compare equal.
fn normalized_product_id(product_id: String) -> Result<String, String> {
	product_id
		.parse::<ProductId>()
		.map(|product_id| product_id.to_string())
		.map_err(|error| error.to_string())
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
					product_id: product_id.to_string(),
					worker_manifest: worker_manifest.clone(),
					origin: FundingProviderOrigin::Published,
				})
				.collect(),
		)
		.expect("offered");
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
					rail: FundingRail::Crypto,
					directions: vec![FundingDirection::Out],
					assets: vec!["USDT".to_string()],
					networks: None,
					countries: None,
				}]],
			)
		);
	}

	// A provider the host ships in the app is never published, so dotNS
	// answering "nothing" must not drop it the way it drops a listed one.
	#[test]
	fn a_bundled_provider_keeps_the_hosts_manifest() {
		let providers = providers(&[("listed.dot", Some(manifest(CARD_IN)))]);
		lock(&providers.entries).extend(
			offered_providers(vec![FundingProviderEntry {
				product_id: "bundled.dot".to_string(),
				worker_manifest: Some(manifest(CARD_IN)),
				origin: FundingProviderOrigin::Bundled,
			}])
			.expect("offered"),
		);
		lock(&providers.live)
			.extend([("listed.dot".to_string(), None), ("bundled.dot".to_string(), None)]);

		assert_eq!(
			ids(providers.candidates(FundingDirection::In)),
			vec!["bundled.dot".to_string()]
		);
	}

	// A snapshot the core cannot read is the host's mistake, so it is refused
	// when offered rather than leaving the provider silently missing.
	#[test]
	fn an_unreadable_snapshot_is_refused_when_offered() {
		let offered = |worker_manifest: &str| {
			offered_providers(vec![FundingProviderEntry {
				product_id: "card.dot".to_string(),
				worker_manifest: Some(worker_manifest.to_string()),
				origin: FundingProviderOrigin::Published,
			}])
			.map(|offered| offered.len())
			.map_err(|error| matches!(error, FundingSessionError::InvalidProvider { .. }))
		};

		assert_eq!((offered(&manifest(CARD_IN)), offered("{")), (Ok(1), Err(true)));
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

	// Browse may name a product the way the host does not, so its ids are
	// normalized like the host's: a product on both lists is one provider,
	// and a selection by the normalized id finds it.
	#[test]
	fn browse_ids_are_normalized_like_the_hosts() {
		let providers = providers(&[("card.dot", Some(manifest(CARD_IN)))]);
		let live = |routes: &str| WorkerManifest::parse(&manifest(routes)).ok();
		let discovered = providers.set_discovered(
			["CARD.dot", "Found.dot", "found.dot", "not a product id"]
				.map(str::to_string)
				.to_vec(),
		);
		lock(&providers.live).insert("found.dot".to_string(), live(CARD_IN));

		assert_eq!(
			(discovered, ids(providers.candidates(FundingDirection::In))),
			(
				vec!["card.dot".to_string(), "found.dot".to_string()],
				vec!["card.dot".to_string(), "found.dot".to_string()],
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

	// A limit stands on its own clock: a later quote does not renew it, and a
	// quote at an amount below a known minimum shows that minimum is gone.
	#[test]
	fn a_limit_ages_on_its_own_and_a_lower_quote_clears_it() {
		use crate::host_logic::funding_providers::{LEARNED_SUPPORT_TTL_MS, LearnedLimit};
		let ask = |asked, min: Option<u128>, at_ms| LearnedSupport {
			provider_id: "ramp.dot".to_string(),
			direction: FundingDirection::In,
			rail: truapi::latest::FundingRail::Card,
			asset: "EUR".to_string(),
			network: None,
			country: None,
			asked,
			supported: true,
			min: min.map(|amount| LearnedLimit { amount, learned_at_ms: at_ms }),
			max: None,
			learned_at_ms: at_ms,
		};
		let min_after = |answers: &[LearnedSupport]| {
			let providers = FundingProviders::default();
			for answer in answers {
				providers.learn(answer.clone());
			}
			lock(&providers.learned)
				.iter()
				.map(|known| known.min.map(|min| min.amount))
				.collect::<Vec<_>>()
		};
		let later = LEARNED_SUPPORT_TTL_MS - 1;

		assert_eq!(
			(
				min_after(&[ask(5, Some(10), 1_000), ask(20, None, 2_000)]),
				min_after(&[ask(5, Some(10), 1_000), ask(8, None, 2_000)]),
				min_after(&[ask(5, Some(10), 1_000), ask(20, None, 1_000 + later + 2)]),
			),
			(vec![Some(10)], vec![None], vec![None])
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
		lock(&providers.live).insert("withdrawn.dot".to_string(), None);

		assert_eq!(
			(
				providers.quote_targets(),
				["card.dot", "unshipped.dot", "withdrawn.dot"]
					.map(|provider_id| providers.is_quote_target(provider_id)),
			),
			(vec!["card.dot".to_string()], [true, false, false])
		);
	}
}
