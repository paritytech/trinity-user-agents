//! Executable manifests, as the core reads them for every host.
//!
//! Parsing is pure: the JSON arrives from `crate::runtime::product_manifest`.
//! Only the fields this core reads are modelled, and an `includes` key it does
//! not recognise is ignored. The `includes.funding` configuration is read the
//! way the RFC requires: a value this core does not recognise is ignored, never
//! fatal, and routes are optional, so a configuration with no usable route
//! still serves Funding and leaves the quotes to say what it moves.
//!
//! Every executable needs `$v` 1, the `kind` of the subname it was read from
//! and an `appVersion` tuple. A Worker also needs a non-blank `entrypoint` and
//! at least one surface in `includes`: `chat` or `pocket` set to `true`, or a
//! `funding` section. A `null` reads as the key being absent.

use serde::Deserialize;
use truapi::latest::{FundingDirection, FundingRail};

/// The executable kind a Worker manifest must declare.
const WORKER_KIND: &str = "worker";
/// The executable kind an App manifest must declare.
const APP_KIND: &str = "app";
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

/// What every executable manifest carries.
#[derive(Deserialize)]
struct Common {
	#[serde(rename = "$v")]
	schema_version: u32,
	kind: String,
	/// Read only for its shape: an executable without one is unusable.
	#[serde(rename = "appVersion")]
	_app_version: AppVersion,
}

/// `[major, minor, patch]`, optionally followed by a build string. Only
/// its shape is checked, so the values are never read.
#[derive(Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum AppVersion {
	Release(u64, u64, u64),
	Build(u64, u64, u64, String),
}

/// The manifest as published, before the RFC's ignore rules are applied.
#[derive(Deserialize)]
struct Published {
	entrypoint: Option<String>,
	includes: Option<PublishedIncludes>,
}

#[derive(Default, Deserialize)]
struct PublishedIncludes {
	chat: Option<bool>,
	pocket: Option<bool>,
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

/// Why an executable manifest is unusable.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum ExecutableManifestError {
	/// The manifest is not the JSON an executable manifest of its kind is.
	#[display("executable manifest is not valid JSON: {reason}")]
	Json {
		/// The parser's message.
		reason: String,
	},
	/// The subname holds a manifest of another executable kind.
	#[display("manifest kind {kind:?} read from a {expected} subname")]
	Kind {
		/// The kind the manifest declares.
		kind: String,
		/// The kind of the subname it was read from.
		expected: &'static str,
	},
	/// The manifest is of a schema version this core cannot read.
	#[display("executable manifest schema version {version} is not supported")]
	SchemaVersion {
		/// The version the manifest declares.
		version: u32,
	},
	/// A Worker names no module to load.
	#[display("worker manifest names no entrypoint")]
	Entrypoint,
	/// A Worker serves no surface, so it has nothing to run for.
	#[display("worker manifest serves no modality")]
	NoModality,
}

fn json_error(err: serde_json::Error) -> ExecutableManifestError {
	ExecutableManifestError::Json { reason: err.to_string() }
}

/// The common fields of `json`, checked against `expected` kind.
fn common(json: &str, expected: &'static str) -> Result<(), ExecutableManifestError> {
	let common: Common = serde_json::from_str(json).map_err(json_error)?;
	if common.kind != expected {
		return Err(ExecutableManifestError::Kind { kind: common.kind, expected });
	}
	if common.schema_version != SUPPORTED_SCHEMA_VERSION {
		return Err(ExecutableManifestError::SchemaVersion { version: common.schema_version });
	}
	Ok(())
}

/// Checks an App manifest, which carries nothing beyond the common fields.
pub fn check_app_manifest(json: &str) -> Result<(), ExecutableManifestError> {
	common(json, APP_KIND)
}

impl WorkerManifest {
	/// Parses a Worker manifest under the rules in the module docs.
	pub fn parse(json: &str) -> Result<Self, ExecutableManifestError> {
		common(json, WORKER_KIND)?;
		let published: Published = serde_json::from_str(json).map_err(json_error)?;
		if published.entrypoint.is_none_or(|entrypoint| entrypoint.trim().is_empty()) {
			return Err(ExecutableManifestError::Entrypoint);
		}
		let includes = published.includes.unwrap_or_default();
		let serves_any = includes.chat == Some(true) ||
			includes.pocket == Some(true) ||
			includes.funding.is_some();
		if !serves_any {
			return Err(ExecutableManifestError::NoModality);
		}
		Ok(Self { funding: includes.funding.map(PublishedFunding::usable) })
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
