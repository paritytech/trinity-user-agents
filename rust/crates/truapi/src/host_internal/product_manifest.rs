//! Root product manifest parsing and grant lookup.
//!
//! Pure: the bytes arrive from `crate::runtime::product_manifest`, and nothing
//! here reaches a chain. A manifest carries more than the trust grants, but only
//! the fields this core reads are modelled — everything else is skipped, so a
//! publisher extending the document does not break parsing.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::platform::ProductId;

/// Manifest schema version this core parses.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// A scope a publisher pre-approves for another product in `trustedProducts`.
///
/// `All` is a superset rather than a peer: it satisfies every other variant,
/// present and future. A value this core does not recognise parses as
/// [`Granted::Unrecognised`] rather than failing the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Granted {
	/// Every mediated interaction, present and future.
	All,
	/// Reading the granting product's host-local storage.
	Storage,
	/// Using the granting product's account and the identity behind it.
	Context,
	/// A grant value defined after this core was built.
	#[serde(other)]
	Unrecognised,
}

/// Manifest JSON for tests, with only `$v` and `trustedProducts`.
#[cfg(test)]
pub fn test_manifest_json(trusted: &str) -> String {
	format!(r#"{{"$v":1,"trustedProducts":{trusted}}}"#)
}

/// The product-wide manifest published at a base name's `manifest` text record.
#[derive(Clone, Deserialize)]
pub struct RootManifest {
	/// Schema version. A version this core does not know makes the product
	/// undiscoverable rather than malformed.
	#[serde(rename = "$v")]
	pub schema_version: u32,
	/// The name the product is shown by, when it declares one.
	#[serde(default, rename = "displayName")]
	pub display_name: Option<String>,
	/// The product's icon, when it declares one.
	#[serde(default)]
	pub icon: Option<ManifestIcon>,
	/// What each named product may do to this one, keyed by bare product label
	/// with no TLD suffix.
	#[serde(default, rename = "trustedProducts")]
	pub trusted_products: BTreeMap<String, Vec<Granted>>,
}

/// A product icon stored on Bulletin.
#[derive(Clone, Deserialize)]
pub struct ManifestIcon {
	/// The Bulletin CID the icon bytes are fetched by.
	pub cid: String,
}

/// A manifest that cannot be decoded or uses an unsupported schema.
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum ManifestParseError {
	/// The manifest is not valid JSON for the expected schema.
	#[display("manifest is not valid JSON: {_0}")]
	#[from]
	Json(serde_json::Error),
	/// The manifest names a schema this core cannot read.
	#[display("manifest schema version {_0} is not supported")]
	UnsupportedSchemaVersion(#[error(not(source))] u32),
}

impl RootManifest {
	/// Parses a manifest, rejecting a schema version this core cannot read.
	///
	/// An unrecognised grant value is not a parse failure: it is dropped from
	/// the entry it appears in and the recognised values around it still apply.
	pub fn parse(json: &str) -> Result<Self, ManifestParseError> {
		let manifest: Self = serde_json::from_str(json)?;
		if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
			return Err(ManifestParseError::UnsupportedSchemaVersion(manifest.schema_version));
		}
		Ok(manifest)
	}

	/// Whether this product grants `caller` the `wanted` scope.
	///
	/// `caller` is looked up by its bare label, the shape of a `trustedProducts`
	/// key. A key written with a suffix names a product that does not resolve,
	/// so it grants nothing.
	pub fn grants(&self, caller: &ProductId, wanted: Granted) -> bool {
		// A value this core does not recognise is not a scope anyone can be
		// granted. Without this an unrecognised entry in the manifest would
		// satisfy a query for one, which is the opposite of ignoring it.
		if wanted == Granted::Unrecognised {
			return false;
		}
		self.trusted_products.get(caller.bare_label()).is_some_and(|granted| {
			granted.iter().any(|value| *value == Granted::All || *value == wanted)
		})
	}
}

#[cfg(test)]
mod tests;
