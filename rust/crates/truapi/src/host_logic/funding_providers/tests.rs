use super::*;

const NOW: u64 = 1_700_000_000_000;

fn learned(supported: bool, at_ms: u64) -> LearnedSupport {
	LearnedSupport {
		provider_id: ProductId::for_test("ramp.dot"),
		direction: FundingDirection::In,
		rail: FundingRail::Bank,
		asset: "EUR".to_string(),
		country: Some("DE".to_string()),
		supported,
		learned_at_ms: at_ms,
	}
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
