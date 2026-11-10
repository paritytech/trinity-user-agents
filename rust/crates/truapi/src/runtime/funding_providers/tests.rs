use super::*;

use truapi::latest::FundingRail;

use crate::host_logic::worker_manifest::FundingRoute;

// The network picker lists each network a crypto route declares once, in
// the order the providers declare them: for the picked asset, or for any
// asset before one is picked.
#[test]
fn declared_networks_are_listed_once_in_order() {
	let route = |rail, assets: &[&str], networks: Option<&[&str]>| FundingRoute {
		rail,
		directions: vec![FundingDirection::In],
		assets: assets.iter().map(|asset| asset.to_string()).collect(),
		networks: networks.map(|n| n.iter().map(|network| network.to_string()).collect()),
		countries: None,
	};
	let candidate = |routes| FundingCandidate {
		provider_id: ProductId::for_test("ramp.dot"),
		routes,
		unsupported: Vec::new(),
		limits: Vec::new(),
	};
	let candidates = [
		candidate(vec![
			route(FundingRail::Crypto, &["USDT"], Some(&["polkadot", "ethereum"])),
			route(FundingRail::Crypto, &["USDC"], Some(&["base"])),
			route(FundingRail::Card, &["USDT"], None),
		]),
		candidate(vec![route(FundingRail::Crypto, &["USDT"], Some(&["ethereum", "tron"]))]),
	];

	let usdt = |networks: &[&str]| -> Vec<(String, Vec<String>)> {
		networks
			.iter()
			.map(|network| (network.to_string(), vec!["USDT".to_string()]))
			.collect()
	};
	assert_eq!(
		(
			declared_networks(&candidates, FundingDirection::In, Some("USDT")),
			declared_networks(&candidates, FundingDirection::In, None)
				.into_iter()
				.map(|(network, _)| network)
				.collect::<Vec<_>>(),
		),
		(
			usdt(&["polkadot", "ethereum", "tron"]),
			vec![
				"polkadot".to_string(),
				"ethereum".to_string(),
				"base".to_string(),
				"tron".to_string()
			]
		)
	);
}

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
				origin: FundingProviderOrigin::Published,
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

// A provider the host ships in the app is never published, so dotNS
// answering "nothing" must not drop it the way it drops a listed one.
#[test]
fn a_bundled_provider_keeps_the_hosts_manifest() {
	let providers = providers(&[("listed.dot", Some(manifest(CARD_IN)))]);
	lock(&providers.entries).extend(
		offered_providers(vec![FundingProviderEntry {
			product_id: ProductId::for_test("bundled.dot"),
			worker_manifest: Some(manifest(CARD_IN)),
			origin: FundingProviderOrigin::Bundled,
		}])
		.expect("offered"),
	);
	providers.answered(&ProductId::for_test("listed.dot"), None);
	providers.answered(&ProductId::for_test("bundled.dot"), None);

	assert_eq!(
		ids(providers.candidates(FundingDirection::In)),
		vec![ProductId::for_test("bundled.dot")]
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
			origin: FundingProviderOrigin::Published,
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
	providers.answered(&ProductId::for_test("withdrawn.dot"), None);
	providers.answered(&ProductId::for_test("changed.dot"), live(CRYPTO_OUT));
	providers.answered(&ProductId::for_test("unshipped.dot"), live(CARD_IN));

	assert_eq!(
		(
			ids(providers.candidates(FundingDirection::In)),
			ids(providers.candidates(FundingDirection::Out)),
		),
		(vec![ProductId::for_test("unshipped.dot")], vec![ProductId::for_test("changed.dot")])
	);
}

// What dotNS answers for a provider the host stopped listing is dropped,
// so listing it again starts from the host's snapshot, while a provider
// listed throughout keeps its answer.
#[test]
fn an_answer_for_a_provider_no_longer_listed_is_dropped() {
	let listed = |product_ids: &[&str]| {
		offered_providers(
			product_ids
				.iter()
				.map(|product_id| FundingProviderEntry {
					product_id: ProductId::for_test(product_id),
					worker_manifest: Some(manifest(CARD_IN)),
					origin: FundingProviderOrigin::Published,
				})
				.collect(),
		)
		.expect("offered")
	};
	let providers = FundingProviders::default();
	providers.offer(listed(&["kept.dot", "dropped.dot"]));
	providers.answered(&ProductId::for_test("kept.dot"), None);
	providers.offer(listed(&["kept.dot"]));
	let late = providers.answered(&ProductId::for_test("dropped.dot"), None);
	providers.offer(listed(&["kept.dot", "dropped.dot"]));

	assert_eq!(
		(late, ids(providers.candidates(FundingDirection::In))),
		(false, vec![ProductId::for_test("dropped.dot")])
	);
}

// A limit stands on its own clock: a later quote does not renew it, and a
// quote at an amount below a known minimum shows that minimum is gone.
#[test]
fn a_limit_ages_on_its_own_and_a_lower_quote_clears_it() {
	use crate::host_logic::funding_providers::{LEARNED_SUPPORT_TTL_MS, LearnedLimit};
	let ask = |asked, min: Option<u128>, at_ms| LearnedSupport {
		provider_id: ProductId::for_test("ramp.dot"),
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
	providers.answered(&ProductId::for_test("withdrawn.dot"), None);

	assert_eq!(
		(
			providers.quote_targets(),
			["card.dot", "unshipped.dot", "withdrawn.dot"]
				.map(|provider_id| providers.is_quote_target(&ProductId::for_test(provider_id))),
		),
		(vec![ProductId::for_test("card.dot")], [true, false, false])
	);
}
