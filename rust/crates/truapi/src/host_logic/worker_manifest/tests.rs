use super::*;

/// The RFC's example provider: card in, and crypto in and out.
const EXAMPLE: &str = r#"{
    "$v": 1,
    "appVersion": [1, 2, 0],
    "kind": "worker",
    "entrypoint": "index.js",
    "includes": {
        "funding": {
            "routes": [
                { "mode": "CARD", "directions": ["In"], "assets": ["EUR", "USD"], "countries": ["DE", "FR", "US"] },
                { "mode": "CRYPTO", "directions": ["In", "Out"], "assets": ["USDT", "DOT"], "networks": ["polkadot", "ethereum"] }
            ]
        }
    }
}"#;

fn with_funding(funding: &str) -> String {
	format!(
		r#"{{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{{"chat":true,"funding":{funding}}}}}"#
	)
}

fn card_in() -> FundingRoute {
	FundingRoute {
		rail: FundingRail::Card,
		directions: vec![FundingDirection::In],
		assets: vec!["EUR".to_string()],
		networks: None,
		countries: None,
	}
}

const CARD_IN: &str = r#"{ "mode": "CARD", "directions": ["In"], "assets": ["EUR"] }"#;

#[test]
fn the_rfc_example_reads_as_published() {
	assert_eq!(
		WorkerManifest::parse(EXAMPLE),
		Ok(WorkerManifest {
			funding: Some(FundingConfig {
				routes: Some(vec![
					FundingRoute {
						rail: FundingRail::Card,
						directions: vec![FundingDirection::In],
						assets: vec!["EUR".to_string(), "USD".to_string()],
						networks: None,
						countries: Some(vec!["DE".to_string(), "FR".to_string(), "US".to_string()]),
					},
					FundingRoute {
						rail: FundingRail::Crypto,
						directions: vec![FundingDirection::In, FundingDirection::Out],
						assets: vec!["USDT".to_string(), "DOT".to_string()],
						networks: Some(vec!["polkadot".to_string(), "ethereum".to_string()]),
						countries: None,
					},
				]),
			}),
		})
	);
}

// Networks only say where crypto moves, so a card route that names some
// reads as one that does not.
#[test]
fn only_a_crypto_route_keeps_its_networks() {
	let card =
		r#"{ "mode": "CARD", "directions": ["In"], "assets": ["EUR"], "networks": ["polkadot"] }"#;
	let manifest = WorkerManifest::parse(&with_funding(&format!(r#"{{ "routes": [{card}] }}"#)))
		.expect("parsed");

	assert_eq!(manifest.funding.map(|funding| funding.routes), Some(Some(vec![card_in()])));
}

// A worker that serves other surfaces but not Funding is no provider.
#[test]
fn a_manifest_without_funding_serves_none() {
	let manifest = WorkerManifest::parse(
		r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"w.js","includes":{"pocket":true}}"#,
	);

	assert_eq!(manifest, Ok(WorkerManifest { funding: None }));
}

// Surfaces are added to `includes` without a new `$v`, so one this core
// does not know must not cost the worker the Funding it serves.
#[test]
fn an_unrecognised_includes_key_is_ignored() {
	let manifest = WorkerManifest::parse(
        r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"w.js","includes":{"funding":{},"teleport":{"anything":1}}}"#,
    )
    .expect("parses");

	assert_eq!(manifest.funding, Some(FundingConfig { routes: None }));
}

#[test]
fn an_unknown_version_or_a_kind_other_than_worker_is_refused() {
	let unknown = WorkerManifest::parse(
		r#"{"$v":2,"appVersion":[1,0,0],"kind":"worker","entrypoint":"w.js"}"#,
	);
	let app =
		WorkerManifest::parse(r#"{"$v":1,"appVersion":[1,0,0],"kind":"app","entrypoint":"w.js"}"#);

	assert!(unknown.is_err() && app.is_err());
}

// Later revisions add modes and directions without a new `$v`, so an
// unrecognised one costs only the route or the value it appears in.
#[test]
fn unrecognised_routes_and_directions_are_ignored() {
	let manifest = WorkerManifest::parse(&with_funding(&format!(
		r#"{{"routes":[
            {CARD_IN},
            {{ "mode": "CASH", "directions": ["In"], "assets": ["EUR"] }},
            {{ "mode": "BANK", "directions": ["Sideways"], "assets": ["EUR"] }},
            {{ "mode": "BANK", "directions": ["Out", "Sideways"], "assets": [] }},
            {{ "mode": "CRYPTO", "directions": ["Sideways", "Out"], "assets": ["DOT"] }}
        ]}}"#
	)))
	.expect("parses");

	assert_eq!(
		manifest.funding.map(|funding| funding.routes),
		Some(Some(vec![
			card_in(),
			FundingRoute {
				rail: FundingRail::Crypto,
				directions: vec![FundingDirection::Out],
				assets: vec!["DOT".to_string()],
				networks: None,
				countries: None,
			},
		]))
	);
}

// Routes only narrow which providers a host asks for a quote, so a
// provider that declares none, or none this core can use, still serves
// Funding and its quotes say what it moves.
#[test]
fn a_provider_without_usable_routes_still_serves_funding() {
	let undeclared = FundingConfig { routes: None };
	for funding in [
		"{}",
		r#"{"routes":[]}"#,
		r#"{"routes":[{ "mode": "CASH", "directions": ["In"], "assets": ["EUR"] }]}"#,
	] {
		let manifest = WorkerManifest::parse(&with_funding(funding)).expect("parses");

		assert_eq!(manifest.funding, Some(undeclared.clone()), "{funding}");
	}
}

#[test]
fn a_route_of_the_wrong_shape_fails_the_document() {
	let manifest = WorkerManifest::parse(&with_funding(
		r#"{"routes":[{ "mode": "CARD", "directions": "In", "assets": ["EUR"] }]}"#,
	));

	assert!(manifest.is_err());
}

// A Worker serving no surface is refused, along with one without an
// entrypoint or an `appVersion`, and a `null` reads as the key being
// absent.
#[test]
fn a_worker_needs_an_entrypoint_a_version_and_a_surface() {
	let parse = |json: &str| WorkerManifest::parse(json).map(|manifest| manifest.funding.is_some());
	let cases = [
		(
			r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":" ","includes":{"chat":true}}"#,
			Err(ExecutableManifestError::Entrypoint),
		),
		(
			r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"w.js","includes":{}}"#,
			Err(ExecutableManifestError::NoModality),
		),
		(
			r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"w.js","includes":{"chat":false,"pocket":null}}"#,
			Err(ExecutableManifestError::NoModality),
		),
		(
			r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"w.js","includes":null}"#,
			Err(ExecutableManifestError::NoModality),
		),
		(
			r#"{"$v":1,"appVersion":[1,0,0,"rc1"],"kind":"worker","entrypoint":"w.js","includes":{"funding":{}}}"#,
			Ok(true),
		),
		(
			r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"w.js","includes":{"chat":true,"funding":null}}"#,
			Ok(false),
		),
	];
	for (json, expected) in cases {
		assert_eq!(parse(json), expected, "{json}");
	}
	assert!(matches!(
		parse(r#"{"$v":1,"kind":"worker","entrypoint":"w.js","includes":{"chat":true}}"#),
		Err(ExecutableManifestError::Json { .. })
	));
}

// An App manifest is checked against its own kind.
#[test]
fn an_app_manifest_needs_the_app_kind() {
	assert_eq!(
		(
			check_app_manifest(r#"{"$v":1,"appVersion":[1,0,0],"kind":"app"}"#),
			check_app_manifest(r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker"}"#),
		),
		(Ok(()), Err(ExecutableManifestError::Kind { kind: "worker".into(), expected: "app" }))
	);
}
