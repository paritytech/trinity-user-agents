//! Worker executable manifests, as the core reads them for every host.
//!
//! Parsing is pure: the JSON arrives from `crate::runtime::product_manifest`.
//! Only the fields this core reads are modelled, and an `includes` key it does
//! not recognise is ignored. The `includes.funding` configuration is read the
//! way the RFC requires: a value this core does not recognise is ignored, never
//! fatal, and a configuration left with nothing usable serves no Funding.

use serde::Deserialize;

/// The executable kind a Worker manifest must declare.
const WORKER_KIND: &str = "worker";
/// Manifest schema version this core parses.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// What a product's `worker.<product_id>.<tld>` subname publishes, as far as
/// this core reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct WorkerManifest {
    /// Module the host loads inside the worker.
    pub entrypoint: String,
    /// Whether the worker serves Pocket.
    pub pocket: bool,
    /// Whether the worker serves Chat.
    pub chat: bool,
    /// Whether the worker serves Input.
    pub input: bool,
    /// How the worker serves Funding. `None` when it does not, including when
    /// its configuration has no usable route.
    pub funding: Option<FundingConfig>,
}

/// A provider's funding configuration, with every value the core does not
/// recognise already left out.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingConfig {
    /// What the provider moves and how; never empty.
    pub routes: Vec<FundingRoute>,
    /// Onramp adapter id for calls that need the provider's key.
    pub backend: Option<String>,
}

/// One payment mode a provider serves.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingRoute {
    /// The payment mode.
    pub mode: FundingMode,
    /// Directions served; never empty.
    pub directions: Vec<RouteDirection>,
    /// Symbols the user pays with or receives; never empty.
    pub assets: Vec<String>,
    /// Networks a crypto route's assets move on, such as `polkadot` (Polkadot
    /// Asset Hub), when declared. `None` for card and bank routes.
    pub networks: Option<Vec<String>>,
    /// ISO 3166-1 alpha-2 codes the route serves, when declared. The quote
    /// still decides.
    pub countries: Option<Vec<String>>,
    /// Whether the user needs an account with the provider.
    pub requires_account: bool,
}

/// How the user pays or is paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingMode {
    /// A card payment.
    Card,
    /// A bank transfer.
    Bank,
    /// A crypto transfer.
    Crypto,
}

/// Which way a route moves value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum RouteDirection {
    /// Into the user's balance.
    In,
    /// Out of the user's balance.
    Out,
}

/// The manifest as published, before the RFC's ignore rules are applied.
#[derive(Deserialize)]
struct Published {
    #[serde(rename = "$v")]
    schema_version: u32,
    kind: String,
    entrypoint: String,
    #[serde(default)]
    includes: PublishedIncludes,
}

#[derive(Default, Deserialize)]
struct PublishedIncludes {
    #[serde(default)]
    pocket: bool,
    #[serde(default)]
    chat: bool,
    #[serde(default)]
    input: bool,
    funding: Option<PublishedFunding>,
}

#[derive(Deserialize)]
struct PublishedFunding {
    routes: Vec<PublishedRoute>,
    backend: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedRoute {
    mode: String,
    directions: Vec<String>,
    assets: Vec<String>,
    networks: Option<Vec<String>>,
    countries: Option<Vec<String>>,
    #[serde(default)]
    requires_account: bool,
}

impl WorkerManifest {
    /// Parses a Worker manifest. A schema version this core cannot read, or a
    /// `kind` other than `worker`, makes the executable unusable.
    pub fn parse(json: &str) -> Result<Self, String> {
        let published: Published = serde_json::from_str(json)
            .map_err(|err| format!("worker manifest is not valid JSON: {err}"))?;
        if published.kind != WORKER_KIND {
            return Err(format!(
                "manifest kind {:?} read from a worker subname",
                published.kind
            ));
        }
        if published.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(format!(
                "worker manifest schema version {} is not supported",
                published.schema_version
            ));
        }
        Ok(Self {
            entrypoint: published.entrypoint,
            pocket: published.includes.pocket,
            chat: published.includes.chat,
            input: published.includes.input,
            funding: published
                .includes
                .funding
                .and_then(PublishedFunding::usable),
        })
    }
}

impl PublishedFunding {
    /// The configuration with what this core does not recognise left out, or
    /// `None` when no route is left.
    fn usable(self) -> Option<FundingConfig> {
        let routes: Vec<FundingRoute> = self
            .routes
            .into_iter()
            .filter_map(PublishedRoute::usable)
            .collect();
        (!routes.is_empty()).then_some(FundingConfig {
            routes,
            backend: self.backend,
        })
    }
}

impl PublishedRoute {
    fn usable(self) -> Option<FundingRoute> {
        let mode = match self.mode.as_str() {
            "CARD" => FundingMode::Card,
            "BANK" => FundingMode::Bank,
            "CRYPTO" => FundingMode::Crypto,
            _ => return None,
        };
        let mut directions = Vec::new();
        for direction in &self.directions {
            let direction = match direction.as_str() {
                "In" => RouteDirection::In,
                "Out" => RouteDirection::Out,
                _ => continue,
            };
            if !directions.contains(&direction) {
                directions.push(direction);
            }
        }
        let networks = self.networks.filter(|_| mode == FundingMode::Crypto);
        (!directions.is_empty() && !self.assets.is_empty()).then_some(FundingRoute {
            mode,
            directions,
            assets: self.assets,
            networks,
            countries: self.countries,
            requires_account: self.requires_account,
        })
    }
}

#[cfg(test)]
mod tests {
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
                    { "mode": "CARD", "directions": ["In"], "assets": ["EUR", "USD"], "countries": ["DE", "FR", "US"], "requiresAccount": true },
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
            mode: FundingMode::Card,
            directions: vec![RouteDirection::In],
            assets: vec!["EUR".to_string()],
            networks: None,
            countries: None,
            requires_account: false,
        }
    }

    const CARD_IN: &str = r#"{ "mode": "CARD", "directions": ["In"], "assets": ["EUR"] }"#;

    #[test]
    fn the_rfc_example_reads_as_published() {
        assert_eq!(
            WorkerManifest::parse(EXAMPLE),
            Ok(WorkerManifest {
                entrypoint: "index.js".to_string(),
                pocket: false,
                chat: false,
                input: false,
                funding: Some(FundingConfig {
                    routes: vec![
                        FundingRoute {
                            mode: FundingMode::Card,
                            directions: vec![RouteDirection::In],
                            assets: vec!["EUR".to_string(), "USD".to_string()],
                            networks: None,
                            countries: Some(vec![
                                "DE".to_string(),
                                "FR".to_string(),
                                "US".to_string()
                            ]),
                            requires_account: true,
                        },
                        FundingRoute {
                            mode: FundingMode::Crypto,
                            directions: vec![RouteDirection::In, RouteDirection::Out],
                            assets: vec!["USDT".to_string(), "DOT".to_string()],
                            networks: Some(vec!["polkadot".to_string(), "ethereum".to_string()]),
                            countries: None,
                            requires_account: false,
                        },
                    ],
                    backend: None,
                }),
            })
        );
    }

    // Networks only say where crypto moves, so a card route that names some
    // reads as one that does not.
    #[test]
    fn only_a_crypto_route_keeps_its_networks() {
        let card = r#"{ "mode": "CARD", "directions": ["In"], "assets": ["EUR"], "networks": ["polkadot"] }"#;
        let manifest = WorkerManifest::parse(&with_funding(&format!(r#"{{ "routes": [{card}] }}"#)))
            .expect("parsed");

        assert_eq!(manifest.funding.map(|funding| funding.routes), Some(vec![card_in()]));
    }

    // A worker that serves no Funding reads as it always did.
    #[test]
    fn a_manifest_without_funding_reads_its_flags() {
        let manifest = WorkerManifest::parse(
            r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"w.js","includes":{"pocket":true}}"#,
        );

        assert_eq!(
            manifest,
            Ok(WorkerManifest {
                entrypoint: "w.js".to_string(),
                pocket: true,
                chat: false,
                input: false,
                funding: None,
            })
        );
    }

    // Surfaces are added to `includes` without a new `$v`, so one this core
    // does not know must not cost the worker the surfaces it does.
    #[test]
    fn an_unrecognised_includes_key_is_ignored() {
        let manifest = WorkerManifest::parse(
            r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"w.js","includes":{"chat":true,"teleport":{"anything":1}}}"#,
        )
        .expect("parses");

        assert_eq!((manifest.chat, manifest.funding), (true, None));
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
            Some(vec![
                card_in(),
                FundingRoute {
                    mode: FundingMode::Crypto,
                    directions: vec![RouteDirection::Out],
                    assets: vec!["DOT".to_string()],
                    networks: None,
                    countries: None,
                    requires_account: false,
                },
            ])
        );
    }

    // A provider with nothing it can offer is not listed, but its other
    // surfaces still serve.
    #[test]
    fn no_usable_route_serves_no_funding() {
        let manifest = WorkerManifest::parse(&with_funding(
            r#"{"routes":[{ "mode": "CASH", "directions": ["In"], "assets": ["EUR"] }]}"#,
        ))
        .expect("parses");

        assert_eq!((manifest.chat, manifest.funding), (true, None));
    }

    #[test]
    fn a_backend_is_kept() {
        let manifest = WorkerManifest::parse(&with_funding(&format!(
            r#"{{"routes":[{CARD_IN}],"backend":"meld"}}"#
        )))
        .expect("parses");

        assert_eq!(
            manifest.funding,
            Some(FundingConfig {
                routes: vec![card_in()],
                backend: Some("meld".to_string()),
            })
        );
    }

    #[test]
    fn a_route_of_the_wrong_shape_fails_the_document() {
        let manifest = WorkerManifest::parse(&with_funding(
            r#"{"routes":[{ "mode": "CARD", "directions": "In", "assets": ["EUR"] }]}"#,
        ));

        assert!(manifest.is_err());
    }
}
