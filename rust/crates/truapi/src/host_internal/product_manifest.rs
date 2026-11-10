//! Root product manifest parsing and grant lookup.
//!
//! Pure: the bytes arrive from `crate::runtime::product_manifest`, and nothing
//! here reaches a chain. A manifest carries more than the trust grants, but only
//! the fields this core reads are modelled — everything else is skipped, so a
//! publisher extending the document does not break parsing.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{
	host_internal::bulletin::BulletinCid,
	image_loader::{ImageFormat, ImageReference},
	platform::ProductId,
};

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
	/// What each named product may do to this one, keyed by bare product label
	/// with no TLD suffix.
	#[serde(default, rename = "trustedProducts")]
	pub trusted_products: BTreeMap<String, Vec<Granted>>,
}

/// How a product is shown: the name and icon its root manifest gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ProductBrand {
	/// The product's name, never blank.
	pub display_name: String,
	/// Its icon, for `load_image`. `None` when the manifest's icon has a CID
	/// that does not parse or a format other than `png` or `jpeg`, which
	/// costs the product its icon and nothing else.
	pub icon: Option<ImageReference>,
}

/// The fields of a root manifest a brand is read from.
#[derive(Deserialize)]
struct PublishedBrand {
	#[serde(rename = "$v")]
	schema_version: u32,
	#[serde(rename = "displayName")]
	display_name: Option<String>,
	description: Option<String>,
	icon: Option<PublishedIcon>,
}

#[derive(Deserialize)]
struct PublishedIcon {
	cid: Option<String>,
	format: Option<String>,
}

impl ProductBrand {
	/// The brand of a root manifest, under the RFC's schema: `$v` 1, a
	/// non-blank `displayName`, a `description` and an `icon` with a `cid` and
	/// a `format`. A manifest that breaks any of these gives no brand. The
	/// icon itself is presentational, so one that cannot be loaded leaves
	/// only the icon out.
	pub fn parse(json: &str) -> Result<Self, ManifestParseError> {
		let published: PublishedBrand = serde_json::from_str(json)?;
		if published.schema_version != SUPPORTED_SCHEMA_VERSION {
			return Err(ManifestParseError::UnsupportedSchemaVersion(published.schema_version));
		}
		let display_name = published
			.display_name
			.filter(|name| !name.trim().is_empty())
			.ok_or(ManifestParseError::Missing("displayName"))?;
		published.description.ok_or(ManifestParseError::Missing("description"))?;
		let icon = published.icon.ok_or(ManifestParseError::Missing("icon"))?;
		let (Some(cid), Some(format)) = (icon.cid, icon.format) else {
			return Err(ManifestParseError::Missing("icon cid and format"));
		};
		let declared_format = match format.to_ascii_lowercase().as_str() {
			"png" => Some(ImageFormat::Png),
			"jpeg" => Some(ImageFormat::Jpeg),
			_ => None,
		};
		let icon = declared_format.zip(cid.parse::<BulletinCid>().ok()).map(|(format, cid)| {
			ImageReference::Bulletin { cid, declared_format: Some(format) }
		});
		Ok(Self { display_name, icon })
	}
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
	/// The manifest lacks a field its schema requires, or leaves it blank.
	#[display("manifest has no usable {_0}")]
	Missing(#[error(not(source))] &'static str),
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
