use super::*;

const NOW: u64 = 1_700_000_000_000;

fn learned(supported: bool, at_ms: u64) -> LearnedSupport {
	LearnedSupport {
		provider_id: ProductId::for_test("ramp.dot"),
		direction: FundingDirection::In,
		rail: FundingRail::Bank,
		asset: "EUR".to_string(),
		network: None,
		country: Some("DE".to_string()),
		asked: 1_000,
		supported,
		min: None,
		max: None,
		learned_at_ms: at_ms,
	}
}

fn limit(amount: u128) -> Option<LearnedLimit> {
	Some(LearnedLimit { amount, learned_at_ms: NOW })
}

fn manifest(funding: &str) -> WorkerManifest {
	WorkerManifest::parse(&format!(
		r#"{{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{{"funding":{funding}}}}}"#
	))
	.expect("parses")
}

/// Declares card in only, so bank in comes from what answers showed.
fn card_in() -> WorkerManifest {
	manifest(r#"{"routes":[{"mode":"CARD","directions":["In"],"assets":["EUR"]}]}"#)
}

// Routes only narrow which providers are asked, so a provider that
// declares none is a candidate either way and its quotes decide, while
// declared routes still leave out a direction they do not serve.
#[test]
fn a_provider_without_routes_is_a_candidate_for_either_direction() {
	let undeclared = manifest("{}");
	let card_in = card_in();
	let candidate = |manifest: &WorkerManifest, direction| {
		FundingCandidate::for_direction(
			&ProductId::for_test("ramp.dot"),
			Some(manifest),
			&[],
			direction,
			NOW,
		)
		.map(|candidate| candidate.routes)
	};

	assert_eq!(
		(
			candidate(&undeclared, FundingDirection::In),
			candidate(&undeclared, FundingDirection::Out),
			candidate(&card_in, FundingDirection::Out),
		),
		(Some(Vec::new()), Some(Vec::new()), None)
	);
}

// What an answer showed widens the manifest only while it is trusted;
// after that the manifest applies again. A rail learned this way lists no
// countries, as one answer does not say where else it is served.
#[test]
fn learned_support_applies_for_twelve_hours() {
	let card_in = card_in();
	let candidate = |at_ms| {
		FundingCandidate::for_direction(
			&ProductId::for_test("ramp.dot"),
			Some(&card_in),
			&[learned(true, at_ms)],
			FundingDirection::In,
			NOW,
		)
		.map(|candidate| candidate.routes.into_iter().map(|route| route.rail).collect::<Vec<_>>())
	};
	let bank = FundingCandidate::for_direction(
		&ProductId::for_test("ramp.dot"),
		Some(&card_in),
		&[learned(true, NOW)],
		FundingDirection::In,
		NOW,
	)
	.and_then(|candidate| {
		candidate.routes.into_iter().find(|route| route.rail == FundingRail::Bank)
	});

	assert_eq!(
		(
			candidate(NOW - LEARNED_SUPPORT_TTL_MS + 1),
			candidate(NOW - LEARNED_SUPPORT_TTL_MS),
			bank
		),
		(
			Some(vec![FundingRail::Card, FundingRail::Bank]),
			Some(vec![FundingRail::Card]),
			Some(FundingRoute {
				rail: FundingRail::Bank,
				directions: vec![FundingDirection::In],
				assets: vec!["EUR".to_string()],
				networks: None,
				countries: None,
			}),
		)
	);
}

// A refusal of only the amount shows what the provider takes, so the host
// can show the limit before quoting, and a crypto answer shows the network
// it serves, for the network picker.
#[test]
fn answers_show_the_limits_and_the_crypto_networks() {
	let usdt = LearnedSupport {
		rail: FundingRail::Crypto,
		asset: "USDT".to_string(),
		network: Some("polkadot".to_string()),
		country: None,
		min: limit(10_000_000),
		..learned(true, NOW)
	};

	let undeclared = manifest("{}");
	let candidate = FundingCandidate::for_direction(
		&ProductId::for_test("ramp.dot"),
		Some(&undeclared),
		&[usdt],
		FundingDirection::In,
		NOW,
	)
	.expect("candidate");

	assert_eq!(
		(candidate.routes, candidate.limits),
		(
			vec![FundingRoute {
				rail: FundingRail::Crypto,
				directions: vec![FundingDirection::In],
				assets: vec!["USDT".to_string()],
				networks: Some(vec!["polkadot".to_string()]),
				countries: None,
			}],
			vec![FundingLimit {
				rail: FundingRail::Crypto,
				asset: "USDT".to_string(),
				network: Some("polkadot".to_string()),
				country: None,
				min: Some(10_000_000),
				max: None,
			}],
		)
	);
}

// The country picker shows what the core knows: a country a manifest or
// an answer serves, with the lowest minimum shown there, and one that
// every answer naming it refused. A country nothing names is left out.
#[test]
fn countries_show_what_manifests_and_answers_say() {
	let declared = manifest(
		r#"{"routes":[{"mode":"CARD","directions":["In"],"assets":["EUR"],"countries":["PT","DE"]}]}"#,
	);
	let in_country = |country: &str, supported, min: Option<u128>| LearnedSupport {
		rail: FundingRail::Card,
		country: Some(country.to_string()),
		supported,
		min: min.and_then(limit),
		..learned(true, NOW)
	};
	let providers = [
		(ProductId::for_test("ramp.dot"), Some(declared)),
		(ProductId::for_test("other.dot"), Some(manifest("{}"))),
	];
	let learned = [
		in_country("DE", true, Some(2_000)),
		LearnedSupport {
			provider_id: ProductId::for_test("other.dot"),
			..in_country("DE", true, Some(1_500))
		},
		in_country("UY", false, None),
		LearnedSupport { rail: FundingRail::Bank, ..in_country("FR", true, None) },
	];

	assert_eq!(
		country_support(&providers, &learned, FundingDirection::In, FundingRail::Card, "EUR", NOW),
		vec![
			FundingCountry {
				country: "DE".to_string(),
				support: FundingCountrySupport::Supported { min: Some(1_500) },
			},
			FundingCountry {
				country: "PT".to_string(),
				support: FundingCountrySupport::Supported { min: None },
			},
			FundingCountry {
				country: "UY".to_string(),
				support: FundingCountrySupport::Unsupported
			},
		]
	);
}

// A provider that withdrew its manifest, or no longer serves Funding,
// stops being offered at once, whatever its earlier answers showed.
#[test]
fn learned_support_needs_a_manifest_serving_funding() {
	let without_funding = WorkerManifest::parse(
		r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{"chat":true}}"#,
	)
	.expect("parses");
	let candidate = |manifest: Option<&WorkerManifest>| {
		FundingCandidate::for_direction(
			&ProductId::for_test("ramp.dot"),
			manifest,
			&[learned(true, NOW)],
			FundingDirection::In,
			NOW,
		)
	};

	assert_eq!((candidate(None), candidate(Some(&without_funding))), (None, None));
}

// The range is the union of what providers take: the smallest minimum
// and the largest maximum, and no bound at all while some provider has
// not shown one, since it may take any amount.
#[test]
fn amount_limits_span_every_provider() {
	let scope = LimitScope {
		direction: FundingDirection::In,
		rail: FundingRail::Bank,
		asset: "EUR",
		network: None,
		country: Some("DE"),
	};
	let refused = |provider_id: &str, min: Option<u128>, max: Option<u128>| LearnedSupport {
		provider_id: ProductId::for_test(provider_id),
		min: min.and_then(limit),
		max: max.and_then(limit),
		..learned(true, NOW)
	};
	let ids = |ids: &[&str]| ids.iter().map(|id| ProductId::for_test(id)).collect::<Vec<_>>();
	let learned = [
		refused("a.dot", Some(20_000_000), Some(500_000_000)),
		refused("b.dot", Some(15_000_000), None),
		refused("d.dot", Some(2_000_000), None),
		LearnedSupport { country: Some("PT".to_string()), ..refused("c.dot", Some(10), None) },
	];

	assert_eq!(
		(
			amount_limits(&ids(&["a.dot", "b.dot"]), &learned, scope, NOW),
			amount_limits(&ids(&["a.dot"]), &learned, scope, NOW),
			amount_limits(&ids(&["a.dot", "c.dot"]), &learned, scope, NOW),
			amount_limits(&[], &learned, scope, NOW),
			amount_limits(&ids(&["d.dot"]), &learned, scope, NOW),
		),
		(
			FundingAmountLimits { min: Some(15_000_000), max: None },
			FundingAmountLimits { min: Some(20_000_000), max: Some(500_000_000) },
			FundingAmountLimits { min: Some(FUNDING_MIN_AMOUNT), max: None },
			FundingAmountLimits { min: Some(FUNDING_MIN_AMOUNT), max: None },
			FundingAmountLimits { min: Some(FUNDING_MIN_AMOUNT), max: None },
		)
	);
}

// The host applies every refresh; the list puts the configured providers in
// order, each once, and leaves out names that are no product.
#[test]
fn the_provider_config_lists_each_published_name_once() {
	let names = ["ramp.dot", "bank.dot", "ramp.dot", "not a product!"].map(str::to_string);
	let (entries, invalid) = provider_config(&names);
	assert_eq!(
		(
			entries
				.iter()
				.map(|entry| (entry.product_id.as_str().to_string(), entry.origin))
				.collect::<Vec<_>>(),
			invalid,
		),
		(
			vec![
				("ramp.dot".to_string(), FundingProviderOrigin::Published),
				("bank.dot".to_string(), FundingProviderOrigin::Published),
			],
			vec!["not a product!".to_string()],
		)
	);
}
