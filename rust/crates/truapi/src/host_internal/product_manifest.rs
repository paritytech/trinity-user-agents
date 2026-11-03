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

/// Manifest JSON for tests. Only `$v` and `trustedProducts` are modelled, so a
/// fixture carrying the display name, description and icon a publisher also
/// writes would be exercising serde's tolerance rather than this parser.
#[cfg(test)]
pub fn test_manifest_json(trusted: &str) -> String {
	format!(r#"{{"$v":1,"trustedProducts":{trusted}}}"#)
}

/// The product-wide manifest published at a base name's `manifest` text record.
#[derive(Debug, Clone, Deserialize)]
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
mod tests {
	use super::*;

	fn manifest(trusted: &str) -> RootManifest {
		RootManifest::parse(&test_manifest_json(trusted)).expect("fixture parses")
	}

	#[test]
	fn a_named_scope_is_granted_only_to_the_product_named() {
		let root = manifest(r#"{"dim2":["storage"]}"#);
		assert!(root.grants(&ProductId::for_test("dim2.dot"), Granted::Storage));
		assert!(!root.grants(&ProductId::for_test("stash.dot"), Granted::Storage));
	}

	#[test]
	fn scopes_are_independent() {
		// `storage` must leave account interactions prompting as usual.
		let root = manifest(r#"{"dim2":["storage"]}"#);
		assert!(!root.grants(&ProductId::for_test("dim2.dot"), Granted::Context));
	}

	#[test]
	fn all_satisfies_every_narrower_scope() {
		let root = manifest(r#"{"dim2":["all"]}"#);
		assert!(root.grants(&ProductId::for_test("dim2.dot"), Granted::Storage));
		assert!(root.grants(&ProductId::for_test("dim2.dot"), Granted::Context));
	}

	#[test]
	fn an_unrecognised_grant_is_ignored_and_its_neighbours_still_apply() {
		// The RFC forbids failing validation over a value defined after this
		// core was built.
		let root = manifest(r#"{"dim2":["storage-write","storage"]}"#);
		assert!(root.grants(&ProductId::for_test("dim2.dot"), Granted::Storage));
		assert!(!root.grants(&ProductId::for_test("dim2.dot"), Granted::Context));
	}

	#[test]
	fn an_entry_of_only_unrecognised_grants_grants_nothing() {
		let root = manifest(r#"{"dim2":["storage-write"]}"#);
		assert!(!root.grants(&ProductId::for_test("dim2.dot"), Granted::Storage));
		assert!(!root.grants(&ProductId::for_test("dim2.dot"), Granted::Context));
	}

	#[test]
	fn an_unrecognised_scope_is_never_granted() {
		// The runtime asks with a `Granted`, so nothing in the type system stops
		// it asking for `Unrecognised`. A manifest full of values this core does
		// not know must still answer no.
		let root = manifest(r#"{"dim2":["storage-write"]}"#);
		assert!(!root.grants(&ProductId::for_test("dim2.dot"), Granted::Unrecognised));

		let wildcard = manifest(r#"{"dim2":["all"]}"#);
		assert!(!wildcard.grants(&ProductId::for_test("dim2.dot"), Granted::Unrecognised));
	}

	#[test]
	fn a_grant_of_the_wrong_shape_still_fails_the_document() {
		// Unrecognised means "a value this core does not know", not "anything at
		// all". A grant list holding a number is a malformed manifest, and the
		// publisher is told so rather than silently granted less than they wrote.
		assert!(
			RootManifest::parse(
				r#"{"$v":1,"displayName":"D","description":"d",
                    "icon":{"cid":"c","format":"png"},"trustedProducts":{"dim2":[17]}}"#
			)
			.is_err()
		);
	}

	#[test]
	fn a_key_written_with_a_tld_suffix_is_inert() {
		// It names `dim2.dot.<tld>`, which does not exist, so the caller `dim2`
		// matches nothing.
		let root = manifest(r#"{"dim2.dot":["storage"]}"#);
		assert!(!root.grants(&ProductId::for_test("dim2.dot"), Granted::Storage));
	}

	#[test]
	fn an_absent_trusted_products_field_grants_nothing() {
		let root = RootManifest::parse(
			r#"{"$v":1,"displayName":"D","description":"d","icon":{"cid":"c","format":"png"}}"#,
		)
		.expect("manifest without trustedProducts parses");
		assert!(!root.grants(&ProductId::for_test("dim2.dot"), Granted::Storage));
	}

	#[test]
	fn an_unknown_schema_version_is_refused() {
		assert!(RootManifest::parse(r#"{"$v":2,"trustedProducts":{}}"#).is_err());
	}

	#[test]
	fn malformed_json_is_refused() {
		assert!(RootManifest::parse("not json").is_err());
	}

	#[test]
	fn an_executable_inherits_the_grants_published_for_its_product() {
		let manifest = RootManifest::parse(r#"{"$v":1,"trustedProducts":{"dim2":["storage"]}}"#)
			.expect("parses");
		assert!(manifest.grants(&ProductId::for_test("dim2.dot"), Granted::Storage));
		assert!(manifest.grants(&ProductId::for_test("app.dim2.dot"), Granted::Storage));
		assert!(manifest.grants(&ProductId::for_test("worker.dim2.dot"), Granted::Storage));
	}

	#[test]
	fn a_subname_of_another_domain_collects_nothing_published_for_its_first_segment() {
		let manifest = RootManifest::parse(r#"{"$v":1,"trustedProducts":{"dim2":["storage"]}}"#)
			.expect("parses");
		assert!(!manifest.grants(&ProductId::for_test("dim2.attacker.dot"), Granted::Storage));
	}
}
