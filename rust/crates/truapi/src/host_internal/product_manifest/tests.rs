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
	let manifest =
		RootManifest::parse(r#"{"$v":1,"trustedProducts":{"dim2":["storage"]}}"#).expect("parses");
	assert!(manifest.grants(&ProductId::for_test("dim2.dot"), Granted::Storage));
	assert!(manifest.grants(&ProductId::for_test("app.dim2.dot"), Granted::Storage));
	assert!(manifest.grants(&ProductId::for_test("worker.dim2.dot"), Granted::Storage));
}

#[test]
fn a_subname_of_another_domain_collects_nothing_published_for_its_first_segment() {
	let manifest =
		RootManifest::parse(r#"{"$v":1,"trustedProducts":{"dim2":["storage"]}}"#).expect("parses");
	assert!(!manifest.grants(&ProductId::for_test("dim2.attacker.dot"), Granted::Storage));
}

const ICON_CID: &str = "bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy";

fn brand(fields: &str) -> Result<ProductBrand, ManifestParseError> {
	ProductBrand::parse(&format!(r#"{{"$v":1,{fields}}}"#))
}

// Divergence "root manifest rules": a brand needs a non-blank name, a
// description and an icon with a CID and format; an icon the core cannot
// load costs only the icon.
#[test]
fn a_brand_needs_a_name_a_description_and_an_icon() {
	let icon = |cid: &str, format: &str| format!(r#""icon":{{"cid":"{cid}","format":"{format}"}}"#);
	let name = |name: &str| format!(r#""displayName":"{name}","description":"d""#);
	let named = |result: Result<ProductBrand, ManifestParseError>| {
		result.ok().map(|brand| (brand.display_name, brand.icon.is_some()))
	};

	assert_eq!(
		[
			named(brand(&format!("{},{}", name("Ramp"), icon(ICON_CID, "PNG")))),
			named(brand(&format!("{},{}", name("Ramp"), icon(ICON_CID, "svg")))),
			named(brand(&format!("{},{}", name("Ramp"), icon("not-a-cid", "png")))),
		],
		[Some(("Ramp".to_string(), true)), Some(("Ramp".to_string(), false)), Some(("Ramp".to_string(), false))]
	);
	for fields in [
		format!("{},{}", name(" "), icon(ICON_CID, "png")),
		format!(r#""displayName":"Ramp",{}"#, icon(ICON_CID, "png")),
		name("Ramp"),
		format!(r#"{},"icon":{{"cid":"{ICON_CID}"}}"#, name("Ramp")),
	] {
		assert!(brand(&fields).is_err(), "{fields}");
	}
}
