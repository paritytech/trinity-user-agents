//! The local product config a developer authors, and the grants it declares.
//!
//! [RFC — Product Manifest Format] defines `LocalProductConfig` as the file a
//! publisher reads before it writes a product's root manifest to dotNS. Its
//! `trustedProducts` field has the same shape as the published manifest's, so
//! the grants a developer intends are already written down before anything is
//! deployed.
//!
//! A product under development has nothing on chain to resolve, so every
//! cross-product call is refused and the flows a partner integration exists for
//! cannot be exercised at all. This module reads the same field the publisher
//! will read, and seeds it into the manifest cache the core would otherwise
//! fill from dotNS.
//!
//! That makes it a local implementation of the manifest path rather than a
//! development relaxation: the core resolves the grant through the code it
//! always runs, and a scope the core does not honour is refused here exactly as
//! it would be on chain. What differs is only where the document came from, so
//! the host says so on startup — a grant that works locally and was never
//! published is the one mistake this must not help anyone ship.
//!
//! [RFC — Product Manifest Format]: ../../../docs/rfcs/product-manifest.md

use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;
use truapi::{
	encode_cached_root_manifest,
	platform::{CoreStorage, CoreStorageKey, ProductId},
};

/// The developer-authored config for one product.
///
/// Only the fields the host needs are read. The rest of the shape belongs to
/// the publisher, and an unknown field is not an error: this file is
/// source-controlled by the developer and shared with tooling that will grow
/// fields the host has no use for.
#[derive(Clone, Deserialize, Debug)]
pub struct LocalProductConfig {
	/// The product's dotNS base name, e.g. `peopl.paseo`. The id its grants and
	/// its storage are resolved under. A name that is not a product id fails
	/// the read.
	#[serde(rename = "productName")]
	pub product_name: ProductId,
	/// Human-readable name, carried into the seeded manifest so what the host
	/// resolves looks like what the publisher will write.
	#[serde(rename = "displayName", default)]
	pub display_name: Option<String>,
	/// Grants this product extends to others, keyed by bare product id.
	#[serde(rename = "trustedProducts", default)]
	pub trusted_products: BTreeMap<String, Vec<String>>,
}

impl LocalProductConfig {
	/// Read a config from a JSON file.
	pub fn read(path: &Path) -> Result<Self> {
		let bytes = std::fs::read(path)
			.with_context(|| format!("reading product config {}", path.display()))?;
		serde_json::from_slice(&bytes)
			.with_context(|| format!("parsing product config {}", path.display()))
	}

	/// The root manifest this config describes, as the publisher would write it.
	///
	/// Only the fields a grant lookup reads are filled. The icon is a
	/// placeholder: nothing resolves it locally, and the manifest parser keeps
	/// an unreadable icon non-fatal precisely so a document stays usable
	/// without one.
	fn root_manifest_json(&self) -> String {
		let trusted = serde_json::to_string(&self.trusted_products)
			.expect("a map of strings to strings always serializes");
		let display = self.display_name.as_deref().unwrap_or(self.product_name.as_str());
		let display = serde_json::to_string(display).expect("a string always serializes");
		format!(
			r#"{{"$v":1,"displayName":{display},"description":"Served locally by truapi-host.","icon":{{"cid":"","format":"png"}},"trustedProducts":{trusted}}}"#
		)
	}

	/// One line naming what this config grants, for the startup transcript.
	fn transcript_line(&self) -> String {
		if self.trusted_products.is_empty() {
			return format!("{}: grants nothing", self.product_name);
		}
		let grants = self
			.trusted_products
			.iter()
			.map(|(product, scopes)| format!("{product} -> [{}]", scopes.join(", ")))
			.collect::<Vec<_>>()
			.join(", ");
		format!("{}: {grants}", self.product_name)
	}
}

/// What [`apply`] seeded, so a caller can report it before serving anything.
#[derive(Clone)]
pub struct AppliedGrants {
	/// One line per config, naming the product and what it grants.
	pub lines: Vec<String>,
}

impl AppliedGrants {
	/// Whether any config was supplied at all.
	pub fn is_empty(&self) -> bool {
		self.lines.is_empty()
	}
}

/// Seed each config's manifest into the core's cache, so its grants resolve for
/// the life of this run.
///
/// `now_secs` is the time the manifests count as read from. The core honours a
/// cached manifest for a fixed lifetime and then reads through to the chain, so
/// a run that outlives it would start refusing grants a developer can see in
/// their own config file. Callers seed at startup and, for a long-lived host,
/// again on the same period.
pub async fn apply(
	platform: &dyn CoreStorage,
	configs: &[LocalProductConfig],
	now_secs: u64,
) -> Result<AppliedGrants> {
	let mut lines = Vec::with_capacity(configs.len());
	for config in configs {
		platform
			.write_core_storage(
				CoreStorageKey::product_manifest(&config.product_name),
				encode_cached_root_manifest(Some(&config.root_manifest_json()), now_secs),
			)
			.await
			.map_err(|error| anyhow::anyhow!("{error:?}"))
			.with_context(|| format!("seeding the local manifest for {}", config.product_name))?;
		lines.push(config.transcript_line());
	}
	Ok(AppliedGrants { lines })
}

/// Read every config named on the command line.
pub fn read_all(paths: &[PathBuf]) -> Result<Vec<LocalProductConfig>> {
	paths.iter().map(|path| LocalProductConfig::read(path)).collect()
}

#[cfg(test)]
mod tests;
