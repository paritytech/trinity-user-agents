//! Worker executable manifests, as the core reads them for every host.
//!
//! Parsing is pure: the JSON arrives from `crate::runtime::product_manifest`.
//! Only the fields this core reads are modelled, and an `includes` key it does
//! not recognise is ignored. The `includes.funding` configuration is read the
//! way the RFC requires: a value this core does not recognise is ignored, never
//! fatal, and routes are optional, so a configuration with no usable route
//! still serves Funding and leaves the quotes to say what it moves.

use serde::Deserialize;
use truapi::latest::{FundingDirection, FundingRail};

/// The executable kind a Worker manifest must declare.
const WORKER_KIND: &str = "worker";
/// Manifest schema version this core parses.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// What a product's `worker.<product_id>.<tld>` subname publishes, as far as
/// this core reads it: whether, and how, the worker serves Funding. The host
/// loads the worker itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerManifest {
	/// How the worker serves Funding. `None` when it does not.
	pub funding: Option<FundingConfig>,
}

/// A provider's funding configuration, with every value the core does not
/// recognise already left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingConfig {
	/// What the provider moves and how, when it declares a usable route. With
	/// `None`, the provider's quotes say what it serves. Never empty.
	pub routes: Option<Vec<FundingRoute>>,
}

/// One payment mode a provider serves.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingRoute {
	/// The rail, published as the route's `mode`.
	pub rail: FundingRail,
	/// Directions served; never empty.
	pub directions: Vec<FundingDirection>,
	/// Symbols the user pays with or receives; never empty.
	pub assets: Vec<String>,
	/// Networks a crypto route's assets move on, such as `polkadot` (Polkadot
	/// Asset Hub), when declared. `None` for card and bank routes.
	pub networks: Option<Vec<String>>,
	/// ISO 3166-1 alpha-2 codes the route serves, when declared. The quote
	/// still decides.
	pub countries: Option<Vec<String>>,
}

/// The manifest as published, before the RFC's ignore rules are applied.
#[derive(Deserialize)]
struct Published {
	#[serde(rename = "$v")]
	schema_version: u32,
	kind: String,
	#[serde(default)]
	includes: PublishedIncludes,
}

#[derive(Default, Deserialize)]
struct PublishedIncludes {
	funding: Option<PublishedFunding>,
}

#[derive(Deserialize)]
struct PublishedFunding {
	routes: Option<Vec<PublishedRoute>>,
}

#[derive(Deserialize)]
struct PublishedRoute {
	mode: String,
	directions: Vec<String>,
	assets: Vec<String>,
	networks: Option<Vec<String>>,
	countries: Option<Vec<String>>,
}

/// Why a Worker manifest is unusable.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum WorkerManifestError {
	/// The manifest is not the JSON a Worker manifest is.
	#[display("worker manifest is not valid JSON: {reason}")]
	Json {
		/// The parser's message.
		reason: String,
	},
	/// The subname holds a manifest of another executable kind.
	#[display("manifest kind {kind:?} read from a worker subname")]
	Kind {
		/// The kind the manifest declares.
		kind: String,
	},
	/// The manifest is of a schema version this core cannot read.
	#[display("worker manifest schema version {version} is not supported")]
	SchemaVersion {
		/// The version the manifest declares.
		version: u32,
	},
}

impl WorkerManifest {
	/// Parses a Worker manifest. A schema version this core cannot read, or a
	/// `kind` other than `worker`, makes the executable unusable.
	pub fn parse(json: &str) -> Result<Self, WorkerManifestError> {
		let published: Published = serde_json::from_str(json)
			.map_err(|err| WorkerManifestError::Json { reason: err.to_string() })?;
		if published.kind != WORKER_KIND {
			return Err(WorkerManifestError::Kind { kind: published.kind });
		}
		if published.schema_version != SUPPORTED_SCHEMA_VERSION {
			return Err(WorkerManifestError::SchemaVersion { version: published.schema_version });
		}
		Ok(Self { funding: published.includes.funding.map(PublishedFunding::usable) })
	}
}

impl PublishedFunding {
	/// The configuration with what this core does not recognise left out. A
	/// declaration with no usable route reads as one declaring none.
	fn usable(self) -> FundingConfig {
		let routes: Vec<FundingRoute> = self
			.routes
			.unwrap_or_default()
			.into_iter()
			.filter_map(PublishedRoute::usable)
			.collect();
		FundingConfig { routes: (!routes.is_empty()).then_some(routes) }
	}
}

impl PublishedRoute {
	fn usable(self) -> Option<FundingRoute> {
		let rail: FundingRail = self.mode.parse().ok()?;
		let mut directions = Vec::new();
		for direction in self.directions.iter().filter_map(|name| name.parse().ok()) {
			if !directions.contains(&direction) {
				directions.push(direction);
			}
		}
		let networks = self.networks.filter(|_| rail == FundingRail::Crypto);
		(!directions.is_empty() && !self.assets.is_empty()).then_some(FundingRoute {
			rail,
			directions,
			assets: self.assets,
			networks,
			countries: self.countries,
		})
	}
}

#[cfg(test)]
mod tests;
