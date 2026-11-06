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
