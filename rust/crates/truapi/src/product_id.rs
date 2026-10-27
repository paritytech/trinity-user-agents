//! Normalized product identifiers.

use core::str::FromStr;

use parity_scale_codec::{Decode, Encode};

use crate::{
	CallError,
	dotns_name::{DOTNS_TLDS, has_dotns_tld, is_dns_label},
};

/// Largest accepted product identifier, in bytes. Manifest lookups are cached
/// per id and never evicted, so this keeps each cache entry small. Matches the
/// cap on chat identifiers.
pub const PRODUCT_ID_MAX_BYTES: usize = 256;

/// Trusted product labels across every network in [`DOTNS_TLDS`].
///
/// These products bypass recorded permissions and permission prompts. Device
/// permissions still need the OS grant.
///
/// [`DOTNS_TLDS`]: crate::dotns_name::DOTNS_TLDS
pub const REMOTE_PERMISSION_TRUSTED_LABELS: &[&str] = &["peopl", GAME_PRODUCT_LABEL, "stash"];

/// The bare label of the game product, Jollity, on every network.
const GAME_PRODUCT_LABEL: &str = "dim2";

/// The label a development product identifier is served under.
const LOCALHOST_PRODUCT_LABEL: &str = "localhost";

/// The start of a development product identifier that names a port.
const LOCALHOST_PORT_PREFIX: &str = "localhost:";

/// A product identifier in normalized form: trimmed, lowercased, and either a
/// name dotNS can register or a localhost development identifier.
///
/// Only [`FromStr`] builds one, so two values are equal exactly when they name
/// the same product executable. Encodes as the identifier string.
///
/// Accepted forms are defined in host-spec C.7:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L109-L128>
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, derive_more::Display)]
#[cfg_attr(feature = "runtime", derive(serde::Serialize))]
#[cfg_attr(feature = "runtime", serde(transparent))]
pub struct ProductId(String);

/// Why a string is not a product identifier.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Error))]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), uniffi(flat_error))]
pub enum ProductIdError {
	/// The identifier was empty or whitespace-only.
	#[display("product_id must not be empty")]
	Empty,
	/// The identifier was not a dotNS or localhost product identifier.
	#[display("product_id must be a dotNS or localhost product identifier, got {input:?}")]
	Invalid {
		/// The identifier as received.
		input: String,
	},
	/// The identifier was longer than [`PRODUCT_ID_MAX_BYTES`] after normalization.
	///
	/// Holds the lengths rather than the id, since the id can be huge.
	#[display("product_id must be at most {limit} bytes, got {actual}")]
	TooLong {
		/// Accepted maximum, in bytes.
		limit: usize,
		/// Normalized length, in bytes.
		actual: usize,
	},
}

/// A product-supplied id that does not parse is a malformed request, not a
/// host failure.
impl<D> From<ProductIdError> for CallError<D> {
	fn from(error: ProductIdError) -> Self {
		CallError::MalformedFrame { reason: error.to_string() }
	}
}

/// Normalizes a product identifier received in any accepted spelling.
impl FromStr for ProductId {
	type Err = ProductIdError;

	fn from_str(product_id: &str) -> Result<Self, Self::Err> {
		let trimmed = product_id.trim();
		if trimmed.is_empty() {
			return Err(ProductIdError::Empty);
		}
		let normalized = trimmed.to_ascii_lowercase();
		// Don't echo the id back: it can be any size.
		if normalized.len() > PRODUCT_ID_MAX_BYTES {
			return Err(ProductIdError::TooLong {
				limit: PRODUCT_ID_MAX_BYTES,
				actual: normalized.len(),
			});
		}
		if !has_well_formed_labels(&normalized) {
			return Err(ProductIdError::Invalid { input: product_id.to_string() });
		}
		if is_localhost_form(&normalized) {
			return Ok(Self(normalized));
		}
		// `localhost` is reserved as a label too, so a published
		// `localhost.<tld>` cannot pass for a development id.
		let reserves_localhost = normalized
			.rsplit_once('.')
			.is_some_and(|(label, _tld)| label == LOCALHOST_PRODUCT_LABEL);
		if has_dotns_tld(&normalized) && !reserves_localhost {
			Ok(Self(normalized))
		} else {
			Err(ProductIdError::Invalid { input: product_id.to_string() })
		}
	}
}

impl ProductId {
	/// Construct a dotNS identity from its full name and explicit network suffix.
	pub fn from_name(name: &str, network_suffix: &str) -> Result<Self, ProductIdError> {
		let input = format!("{name}.{network_suffix}");
		if !DOTNS_TLDS.contains(&network_suffix) {
			return Err(ProductIdError::Invalid { input });
		}
		input.parse()
	}

	/// A product id for a test fixture. Panics when `product_id` does not parse.
	#[cfg(any(test, feature = "mock"))]
	pub fn for_test(product_id: &str) -> Self {
		product_id.parse().expect("test product id is valid")
	}

	/// The normalized identifier.
	pub fn as_str(&self) -> &str {
		&self.0
	}

	/// The label just above the TLD, which is how `trustedProducts` names a
	/// product.
	///
	/// Executables are published under their product's name, so `app.dim2.dot`
	/// and `worker.dim2.dot` both yield `dim2` and get its grants. A subname of
	/// another domain yields that domain: `dim2.attacker.dot` yields `attacker`.
	/// A localhost identifier has no TLD and is returned unchanged.
	pub fn bare_label(&self) -> &str {
		self.0.rsplit_once('.').map_or(&self.0, |(above_tld, _tld)| {
			above_tld.rsplit_once('.').map_or(above_tld, |(_prefix, label)| label)
		})
	}

	/// Whether `other` is the same product on the same network. Unlike `==`,
	/// `app.dim2.dot` matches `dim2.dot`. `dim2.paseo` does not.
	pub fn same_product_on_one_network(&self, other: &Self) -> bool {
		self.bare_label() == other.bare_label() && self.on_one_network(other)
	}

	/// Whether `other` carries the same TLD. Two localhost identifiers carry
	/// none, so they share a network.
	pub fn on_one_network(&self, other: &Self) -> bool {
		self.network_suffix() == other.network_suffix()
	}

	/// The network suffix, or `None` for a localhost development identity.
	pub fn network_suffix(&self) -> Option<&str> {
		self.0.rsplit_once('.').map(|(_, tld)| tld)
	}

	/// Everything before the TLD: `app.dim2` for `app.dim2.dot`. `None` for a
	/// localhost identifier.
	pub fn dotns_name(&self) -> Option<&str> {
		if !has_dotns_tld(&self.0) {
			return None;
		}
		self.0.rsplit_once('.').map(|(label, _tld)| label)
	}

	/// Whether this dotNS identity belongs to the explicitly configured network.
	pub fn matches_network(&self, network_suffix: &str) -> bool {
		self.network_suffix() == Some(network_suffix)
	}

	/// Whether this is a development localhost identifier: exactly
	/// `localhost`, or `localhost:` followed by a port.
	pub fn is_localhost(&self) -> bool {
		is_localhost_form(&self.0)
	}

	/// Whether this product gets every [`RemotePermission`] without a prompt.
	/// Stored decisions don't change this.
	///
	/// Matches the whole label: `peopl.dot` and `peopl.paseo` are trusted,
	/// `app.peopl.dot` and localhost ids are not. The id also needs a dotNS TLD,
	/// so a bare `peopl` never qualifies.
	///
	/// [`RemotePermission`]: truapi::latest::RemotePermission
	pub fn has_trusted_remote_permissions(&self) -> bool {
		self.dotns_name()
			.is_some_and(|label| REMOTE_PERMISSION_TRUSTED_LABELS.contains(&label))
	}

	/// [`has_trusted_remote_permissions`](Self::has_trusted_remote_permissions)
	/// for an id in any spelling. An id that does not parse is not trusted.
	pub fn normalizes_to_trusted_remote_permissions(product_id: &str) -> bool {
		product_id.parse::<Self>().is_ok_and(|id| id.has_trusted_remote_permissions())
	}

	/// Whether this is the game product, Jollity, on any network. `app.dim2.dot`
	/// is not.
	pub fn is_game_product(&self) -> bool {
		self.dotns_name() == Some(GAME_PRODUCT_LABEL)
	}
}

impl From<ProductId> for String {
	fn from(product_id: ProductId) -> Self {
		product_id.0
	}
}

/// Compares against the normalized id, so another spelling is not equal.
impl PartialEq<&str> for ProductId {
	fn eq(&self, other: &&str) -> bool {
		self.0 == *other
	}
}

/// Decoding goes through [`FromStr`], so ids from the wire are always
/// normalized. Otherwise `DIM2.dot` and `dim2.dot` would get separate keys and
/// storage.
impl Decode for ProductId {
	fn decode<I: parity_scale_codec::Input>(
		input: &mut I,
	) -> Result<Self, parity_scale_codec::Error> {
		String::decode(input)?
			.parse::<Self>()
			.map_err(|_| "not an accepted product identifier".into())
	}
}

/// Deserializing also goes through [`FromStr`].
#[cfg(feature = "runtime")]
impl<'de> serde::Deserialize<'de> for ProductId {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		String::deserialize(deserializer)?.parse().map_err(serde::de::Error::custom)
	}
}

#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
uniffi::custom_type!(ProductId, String, {
	lower: |product_id| product_id.0,
	try_lift: |product_id| Ok(product_id.parse::<ProductId>()?),
});

/// Whether `normalized` is exactly `localhost` or `localhost:<port>`.
///
/// Hosts give these a development wildcard, so the whole id must match: a
/// prefix test would admit `localhost:8080.evil`.
fn is_localhost_form(normalized: &str) -> bool {
	if normalized == LOCALHOST_PRODUCT_LABEL {
		return true;
	}
	normalized
		.strip_prefix(LOCALHOST_PORT_PREFIX)
		.is_some_and(|port| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
}

/// Whether every label of `normalized` is one dotNS can register, or the id is
/// the localhost form.
///
/// Anything else can never resolve, and what it excludes is what a lookalike
/// id needs: other scripts, invisible characters, whitespace, path separators.
fn has_well_formed_labels(normalized: &str) -> bool {
	// Without this, `localhost:3000.dot` would pass as a dotNS name.
	if normalized == LOCALHOST_PRODUCT_LABEL || normalized.starts_with(LOCALHOST_PORT_PREFIX) {
		return is_localhost_form(normalized);
	}
	normalized.split('.').all(is_dns_label)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::dotns_name::{DOTNS_TLDS, MAX_DNS_LABEL_LEN};

	#[test]
	fn named_products_preserve_their_full_namespace() {
		let identifier = ProductId::from_name("App.Arena.Coinflip", "paseo").unwrap();
		assert_eq!(
			(identifier.clone(), identifier.dotns_name(), identifier.network_suffix()),
			(
				ProductId::for_test("app.arena.coinflip.paseo"),
				Some("app.arena.coinflip"),
				Some("paseo")
			)
		);
		assert!(identifier.matches_network("paseo"));
		assert!(!identifier.matches_network("dot"));
	}

	#[test]
	fn named_products_require_a_supported_network_suffix() {
		for suffix in ["", "com", "test", "dot.li", "other.dot"] {
			assert!(ProductId::from_name("browse", suffix).is_err(), "{suffix}");
		}
		let localhost = ProductId::for_test("localhost:3000");
		assert_eq!((localhost.dotns_name(), localhost.network_suffix()), (None, None));
		assert!(!localhost.matches_network("dot"));
	}

	#[test]
	fn trusted_remote_permission_labels_match_the_bare_product_label() {
		for product_id in [
			"peopl.dot",
			"peopl.paseo",
			"peopl.testnet",
			"dim2.dot",
			"dim2.paseo",
			"dim2.testnet",
			"stash.dot",
		] {
			assert!(
				ProductId::normalizes_to_trusted_remote_permissions(product_id),
				"{product_id} must hold remote permissions without a prompt"
			);
		}
		for product_id in [
			"app.peopl.dot",
			"sub.dim2.paseo",
			"app.dim2.testnet",
			"dim2.com",
			"notdim2.dot",
			"peopl",
			"peopl.com",
			"peoplx.dot",
			"my-peopl.dot",
			"localhost",
			"localhost:3000",
			"",
			"dot",
		] {
			assert!(
				!ProductId::normalizes_to_trusted_remote_permissions(product_id),
				"{product_id} is a separate product and must prompt"
			);
		}
	}

	#[test]
	fn a_published_product_cannot_take_the_localhost_label() {
		// Hosts grant a development wildcard to a localhost product, so the
		// label must not be reachable from dotNS.
		for product_id in ["localhost.dot", "localhost.paseo", "LOCALHOST.DOT"] {
			assert!(
				product_id.parse::<ProductId>().is_err(),
				"{product_id} must not be a valid product identifier"
			);
			assert!(
				!is_localhost_form(product_id),
				"{product_id} must not read as the development wildcard"
			);
		}
		for product_id in ["localhost", "localhost:3000"] {
			assert!(is_localhost_form(product_id), "{product_id}");
		}
		// A prefix test would admit these; the whole identifier has to match.
		for product_id in ["localhost:3000.evil.dot", "localhost:", "localhost:80a"] {
			assert!(
				!is_localhost_form(product_id),
				"{product_id} must not read as the development wildcard"
			);
			assert!(
				product_id.parse::<ProductId>().is_err(),
				"{product_id} must not pass as a dotNS name either"
			);
		}
		// Still a normal product, sharing only the prefix.
		assert!("localhosting.dot".parse::<ProductId>().is_ok());
	}

	#[test]
	fn trusted_remote_permissions_normalize_the_spellings_a_host_holds() {
		// Both the UniFFI and wasm exports parse before asking, and a host
		// holds an id in whatever spelling it received rather than the
		// normalized form the core passes internally.
		for product_id in
			["peopl.dot", "PEOPL.DOT", "  peopl.dot  ", "dim2.paseo", " DIM2.TESTNET ", "stash.dot"]
		{
			assert!(
				ProductId::normalizes_to_trusted_remote_permissions(product_id),
				"{product_id} is a first-party product in any accepted spelling"
			);
		}
		for product_id in ["app.peopl.dot", "peopl", "notpeopl.dot", "localhost:3000", "", "   "] {
			assert!(
				!ProductId::normalizes_to_trusted_remote_permissions(product_id),
				"{product_id} must not be read as trusted"
			);
		}
	}

	#[test]
	fn every_trusted_remote_permission_label_is_a_product_identifier() {
		// A label that product-id validation rejects would never reach the
		// permission engine, so the whitelist entry would be silently inert.
		for label in REMOTE_PERMISSION_TRUSTED_LABELS {
			for tld in DOTNS_TLDS {
				let product_id = format!("{label}.{tld}");
				assert!(
					product_id.parse::<ProductId>().is_ok(),
					"{product_id} must be an accepted product identifier"
				);
				assert!(
					ProductId::normalizes_to_trusted_remote_permissions(&product_id),
					"{product_id} must be recognized as trusted"
				);
			}
		}
	}

	#[test]
	fn trusted_remote_permission_labels_are_bare_lowercase_labels() {
		// The predicate compares against the label of an already-normalized id,
		// so an entry carrying a TLD or an uppercase letter can never match.
		for label in REMOTE_PERMISSION_TRUSTED_LABELS {
			assert!(!label.contains('.'), "{label} must not carry a TLD");
			assert_eq!(*label, label.to_lowercase(), "{label} must be lowercase");
		}
	}

	#[test]
	fn an_identifier_carrying_no_name_is_rejected() {
		// Each of these would reach the manifest grant key, the cache key, the
		// only consent prompt in this design and every log line.
		for id in [
			"dim2\u{0}.dot",        // NUL, into a prompt rendered verbatim
			"\u{202e}dim2.dot",     // right-to-left override
			"dim2\u{200b}.dot",     // zero width space
			"a b c.dot",            // whitespace
			"../../etc/passwd.dot", // path separators
			"dim2..dot",            // empty label: collapses onto other ids
			".dot",                 // same empty label
		] {
			assert!(
				id.parse::<ProductId>().is_err(),
				"{id:?} carries no product name and must not normalize"
			);
		}
	}

	#[test]
	fn only_a_name_dotns_can_register_is_an_identifier() {
		let longest_label = "a".repeat(MAX_DNS_LABEL_LEN);
		for id in ["Dim2.DOT", "dim2-two.dot", "0x.dot", &format!("{longest_label}.dot")] {
			assert!(id.parse::<ProductId>().is_ok(), "{id:?} is a name dotNS can register");
		}
		// dotNS refuses each of these at registration, so none can ever resolve.
		for id in [
			"tést.dot",
			"münchen.dot",
			"my_app.dot",
			"-dim2.dot",
			"dim2-.dot",
			"\u{ff44}\u{ff49}\u{ff4d}2.dot",
			"dim2\u{ad}.dot",
			&format!("{longest_label}a.dot"),
		] {
			assert!(id.parse::<ProductId>().is_err(), "{id:?} is not a name dotNS can register");
		}
	}

	#[test]
	fn an_overlong_product_id_is_not_an_identifier() {
		// Every label is well formed, so without the cap any length would be
		// a valid, distinct cache key taken from the wire.
		let full_label = "a".repeat(MAX_DNS_LABEL_LEN);
		let at_cap = format!("{full_label}.{full_label}.{full_label}.{}.dot", "a".repeat(60));
		assert_eq!(at_cap.len(), PRODUCT_ID_MAX_BYTES);
		assert!(at_cap.parse::<ProductId>().is_ok(), "an id exactly at the cap is still valid");

		let overlong = format!("{full_label}.{full_label}.{full_label}.{}.dot", "a".repeat(61));
		assert!(
			overlong.parse::<ProductId>().is_err(),
			"a product id past the cap must be rejected, not stored"
		);
	}

	#[cfg(feature = "runtime")]
	#[test]
	fn persisted_json_is_a_canonical_string_and_validates_on_restore() {
		let id = ProductId::for_test(" App.Peopl.DOT ");
		assert_eq!(serde_json::to_string(&id).unwrap(), r#""app.peopl.dot""#);
		assert_eq!(serde_json::from_str::<ProductId>(r#"" App.Peopl.DOT ""#).unwrap(), id);
		assert!(serde_json::from_str::<ProductId>(r#""not_a.dot""#).is_err());
	}

	#[test]
	fn the_bare_label_drops_the_tld() {
		assert_eq!(ProductId::for_test("dim2.dot").bare_label(), "dim2");
		assert_eq!(ProductId::for_test("dim2.paseo").bare_label(), "dim2");
		assert_eq!(ProductId::for_test("localhost").bare_label(), "localhost");
	}

	#[test]
	fn an_executable_carries_the_label_of_the_product_it_belongs_to() {
		assert_eq!(ProductId::for_test("app.dim2.dot").bare_label(), "dim2");
		assert_eq!(ProductId::for_test("widget.dim2.dot").bare_label(), "dim2");
		assert_eq!(ProductId::for_test("worker.dim2.paseo").bare_label(), "dim2");
		assert_eq!(ProductId::for_test("funding.dim2.dot").bare_label(), "dim2");
	}

	#[test]
	fn a_subname_of_another_domain_takes_that_domains_label() {
		assert_eq!(ProductId::for_test("dim2.attacker.dot").bare_label(), "attacker");
	}

	#[test]
	fn one_product_on_one_network_is_its_executables_under_one_tld() {
		assert!(
			ProductId::for_test("app.dim2.dot")
				.same_product_on_one_network(&ProductId::for_test("dim2.dot"))
		);
		assert!(
			!ProductId::for_test("dim2.paseo")
				.same_product_on_one_network(&ProductId::for_test("dim2.dot"))
		);
		assert!(
			!ProductId::for_test("dim2.attacker.dot")
				.same_product_on_one_network(&ProductId::for_test("dim2.dot"))
		);
	}

	#[test]
	fn decoding_normalizes_and_rejects_what_parsing_rejects() {
		let frame = "DIM2.Dot".to_string().encode();
		assert_eq!(ProductId::decode(&mut frame.as_slice()), Ok(ProductId::for_test("dim2.dot")));
		let frame = "evil.com".to_string().encode();
		assert!(ProductId::decode(&mut frame.as_slice()).is_err());
	}
}
