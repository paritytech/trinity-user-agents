//! Normalized product identifiers.

use parity_scale_codec::{Decode, Encode};
use unicode_normalization::UnicodeNormalization;

use super::{CoreStorageKey, RuntimeConfigValidationError, has_dotns_tld, require_non_empty};

/// Largest accepted product identifier, in bytes.
///
/// Bounds the size of one identifier, not how many exist: a manifest miss
/// caches its answer keyed by the target and nothing evicts those entries.
/// Matches the cap on product-supplied chat identifiers.
pub const PRODUCT_ID_MAX_BYTES: usize = 256;

/// Blessed product labels across every network in [`DOTNS_TLDS`].
///
/// These products bypass recorded permissions and prompt only for device access.
///
/// [`DOTNS_TLDS`]: super::DOTNS_TLDS
pub const REMOTE_PERMISSION_TRUSTED_LABELS: &[&str] = &["peopl", GAME_PRODUCT_LABEL, "stash"];

/// The bare label of the game product, Jollity, on every network.
const GAME_PRODUCT_LABEL: &str = "dim2";

/// The label a development product identifier is served under.
const LOCALHOST_PRODUCT_LABEL: &str = "localhost";

/// The start of a development product identifier that names a port.
const LOCALHOST_PORT_PREFIX: &str = "localhost:";

/// A product identifier in normalized form: trimmed, NFC, lowercase, and
/// either a dotNS name or a localhost development identifier.
///
/// Only [`ProductId::parse`] builds one, so holding a `ProductId` means the
/// normalization already happened, and two values compare equal exactly when
/// they name the same product executable. Encodes as the identifier string.
///
/// Host-spec C.7 defines accepted product id forms:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L109-L128>
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, derive_more::Display)]
pub struct ProductId(String);

impl ProductId {
	/// Normalize a product identifier received in any accepted spelling.
	pub fn parse(product_id: &str) -> Result<Self, RuntimeConfigValidationError> {
		let trimmed = product_id.trim();
		require_non_empty("product_id", trimmed)?;
		let normalized = trimmed.nfc().collect::<String>().to_lowercase();
		// After normalizing, since NFC can change the length. Reported by length
		// rather than by value: an id that trips this can be arbitrarily large.
		if normalized.len() > PRODUCT_ID_MAX_BYTES {
			return Err(RuntimeConfigValidationError::ProductIdTooLong {
				limit: PRODUCT_ID_MAX_BYTES,
				actual: normalized.len(),
			});
		}
		if !has_well_formed_labels(&normalized) {
			return Err(RuntimeConfigValidationError::InvalidProductId {
				product_id: product_id.to_string(),
			});
		}
		if is_localhost_form(&normalized) {
			return Ok(Self(normalized));
		}
		// `localhost` is reserved as a label, not only as a bare identifier. A
		// product published as `localhost.<tld>` would otherwise be one string
		// comparison away from whatever a host grants the development wildcard.
		let reserves_localhost = normalized
			.rsplit_once('.')
			.is_some_and(|(label, _tld)| label == LOCALHOST_PRODUCT_LABEL);
		if has_dotns_tld(&normalized) && !reserves_localhost {
			Ok(Self(normalized))
		} else {
			Err(RuntimeConfigValidationError::InvalidProductId {
				product_id: product_id.to_string(),
			})
		}
	}

	/// The normalized identifier.
	pub fn as_str(&self) -> &str {
		&self.0
	}

	/// The segment above the TLD, which is the bare label a `trustedProducts`
	/// key is written with.
	///
	/// A product's executables are published beneath its own name, so
	/// `app.dim2.dot` and `worker.dim2.dot` both yield `dim2` and carry the grants
	/// published for it. They are that product, not neighbours of it. Reading the
	/// first segment instead would look for a key named after the executable, and
	/// reading everything below the TLD would make each executable its own product.
	///
	/// A subname under a different domain resolves to that domain: `dim2.attacker.dot`
	/// yields `attacker`, so it collects nothing published for `dim2`.
	///
	/// A localhost development identifier has no TLD and is returned unchanged.
	pub fn bare_label(&self) -> &str {
		self.0.rsplit_once('.').map_or(&self.0, |(above_tld, _tld)| {
			above_tld.rsplit_once('.').map_or(above_tld, |(_prefix, label)| label)
		})
	}

	/// Whether `other` names the same product on the same network.
	///
	/// [`bare_label`](Self::bare_label) alone answers "same product" across
	/// networks, which is right where a grant is read and wrong where a proof
	/// context is: the alias is what a product shows on the chain it is on, so a
	/// `.paseo` namesake's context is not the `.dot` grantor's.
	pub fn same_product_on_one_network(&self, other: &Self) -> bool {
		self.bare_label() == other.bare_label() && self.on_one_network(other)
	}

	/// Whether `other` carries the same TLD. Two localhost identifiers carry
	/// none, so they share a network.
	pub fn on_one_network(&self, other: &Self) -> bool {
		self.tld() == other.tld()
	}

	fn tld(&self) -> Option<&str> {
		self.0.rsplit_once('.').map(|(_, tld)| tld)
	}

	/// Everything before the TLD: `dim2` for `dim2.dot`, `app.dim2` for
	/// `app.dim2.dot`. `None` for a localhost identifier, which never yields
	/// a label.
	fn dotns_label(&self) -> Option<&str> {
		if !has_dotns_tld(&self.0) {
			return None;
		}
		self.0.rsplit_once('.').map(|(label, _tld)| label)
	}

	/// Whether this is a development localhost identifier: exactly
	/// `localhost`, or `localhost:` followed by a port.
	pub fn is_localhost(&self) -> bool {
		is_localhost_form(&self.0)
	}

	/// Whether this product holds every [`RemotePermission`] without prompting.
	///
	/// Matches the whole label and nothing else: `peopl.dot` and `peopl.paseo`
	/// are trusted, while `app.peopl.dot` and any `localhost` identifier are
	/// separate products and are not. The label is only read out of an id that
	/// [`has_dotns_tld`] accepts, so a widened product-id policy cannot promote
	/// an arbitrary single-label host.
	///
	/// Recorded permission decisions do not affect this policy.
	///
	/// [`RemotePermission`]: truapi::latest::RemotePermission
	pub fn has_trusted_remote_permissions(&self) -> bool {
		self.dotns_label()
			.is_some_and(|label| REMOTE_PERMISSION_TRUSTED_LABELS.contains(&label))
	}

	/// [`has_trusted_remote_permissions`](Self::has_trusted_remote_permissions)
	/// for an id in whatever spelling a host holds. An id that does not parse
	/// is not trusted.
	pub fn normalizes_to_trusted_remote_permissions(product_id: &str) -> bool {
		Self::parse(product_id).is_ok_and(|id| id.has_trusted_remote_permissions())
	}

	/// Whether this is the game product, Jollity, on any dotNS network. A
	/// subname such as `app.dim2.dot` is a different product.
	pub fn is_game_product(&self) -> bool {
		self.dotns_label() == Some(GAME_PRODUCT_LABEL)
	}

	/// Cache key for this product's root manifest.
	///
	/// Keyed by the bare label, which is what the document is actually resolved
	/// by: the manifest lookup walks [`bare_label`](Self::bare_label) under the
	/// host's TLD, so `peopl.dot`, `app.peopl.dot` and `worker.peopl.dot` are one
	/// dotNS node holding one document. Keying by the full id would give each
	/// spelling its own entry, so a caller naming subnames could force a fresh
	/// chain resolution and a fresh durable write per spelling for a document
	/// already held, without bound. The host resolves only against its own
	/// network, so the label alone identifies the node.
	///
	/// Anything seeding this cache must key it the same way or the entry is
	/// written where nothing reads it.
	pub fn manifest_cache_key(&self) -> CoreStorageKey {
		CoreStorageKey::ProductManifest { product_id: self.bare_label().to_string() }
	}
}

impl From<ProductId> for String {
	fn from(product_id: ProductId) -> Self {
		product_id.0
	}
}

/// Decoding parses, so a frame off the wire cannot produce an id
/// [`ProductId::parse`] rejects. Derivation and storage scopes are keyed by
/// the product id, so an unnormalized one would split one logical product
/// across two scopes.
impl Decode for ProductId {
	fn decode<I: parity_scale_codec::Input>(
		input: &mut I,
	) -> Result<Self, parity_scale_codec::Error> {
		Self::parse(&String::decode(input)?)
			.map_err(|_| "not an accepted product identifier".into())
	}
}

/// Whether `normalized` is a development localhost identifier.
///
/// Exactly `localhost`, or `localhost:` followed by a port and nothing else.
/// Hosts grant these a development wildcard, so the match is the whole
/// identifier: a prefix test would also admit `localhost:8080.evil`, and a
/// label test would admit a published `localhost.<tld>`, which
/// [`ProductId::parse`] rejects for the same reason.
fn is_localhost_form(normalized: &str) -> bool {
	if normalized == LOCALHOST_PRODUCT_LABEL {
		return true;
	}
	normalized
		.strip_prefix(LOCALHOST_PORT_PREFIX)
		.is_some_and(|port| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
}

/// Whether every dot-separated label of a normalized id is well formed.
///
/// A cross-product call takes this string from the wire, where it is
/// self-asserted, so every label is checked, not only the TLD. It rejects:
///
/// - An empty label. `dim2..dot` and `.dot` would both reduce to `""`, so distinct ids would
///   collapse onto one grant key and one manifest cache entry.
/// - Control and format characters. A NUL or a right-to-left override would reach
///   [`AccountAccessReview`], which renders the id verbatim in the only consent prompt this design
///   has, and every log line carrying it.
/// - Whitespace and path separators, which would let one product's id render like another's
///   anywhere the comparison is not byte-exact.
///
/// Deliberately not an ASCII allowlist: dotNS names are internationalized, so
/// `tést.dot` is a real id. What is rejected is the class of characters that
/// carries no name and only confuses a reader or a key.
///
/// [`AccountAccessReview`]: super::AccountAccessReview
fn has_well_formed_labels(normalized: &str) -> bool {
	/// Zero-width and bidirectional formatting characters. Invisible in every
	/// rendering, so they make two different ids look identical to a user.
	fn is_invisible_format(c: char) -> bool {
		matches!(c,
            '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}')
	}
	let label_ok = |label: &str| {
		!label.is_empty() &&
			!label.chars().any(|c| {
				c.is_control() ||
					c.is_whitespace() ||
					is_invisible_format(c) ||
					matches!(c, '/' | '\\')
			})
	};
	// The development form's namespace holds that form and nothing else, so
	// `localhost:3000.dot` cannot pass as a dotNS name.
	if normalized == LOCALHOST_PRODUCT_LABEL || normalized.starts_with(LOCALHOST_PORT_PREFIX) {
		return is_localhost_form(normalized);
	}
	normalized.split('.').all(label_ok)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::platform::DOTNS_TLDS;

	fn id(product_id: &str) -> ProductId {
		ProductId::parse(product_id).expect("test product identifier is valid")
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
				ProductId::parse(product_id).is_err(),
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
				ProductId::parse(product_id).is_err(),
				"{product_id} must not pass as a dotNS name either"
			);
		}
		// Still a normal product, sharing only the prefix.
		assert!(ProductId::parse("localhosting.dot").is_ok());
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
					ProductId::parse(&product_id).is_ok(),
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
				ProductId::parse(id).is_err(),
				"{id:?} carries no product name and must not normalize"
			);
		}
	}

	#[test]
	fn an_internationalized_identifier_is_still_an_identifier() {
		// The rejection above is a hazard list, not an ASCII allowlist: dotNS
		// names are internationalized and these are real ids.
		for id in ["Tést.DOT", "münchen.dot", "dim2-two.dot", "localhost:3000"] {
			assert!(ProductId::parse(id).is_ok(), "{id:?} is a legitimate product id");
		}
	}

	#[test]
	fn an_overlong_product_id_is_not_an_identifier() {
		// Every label here is well formed, so only the cap stops each length
		// of this from being a valid, distinct, wire-supplied cache key.
		let label = "a".repeat(PRODUCT_ID_MAX_BYTES);
		let overlong = format!("{label}.dot");
		assert!(overlong.len() > PRODUCT_ID_MAX_BYTES);
		assert!(
			ProductId::parse(&overlong).is_err(),
			"a product id past the cap must be rejected, not stored"
		);

		// A huge id alone would pass with the cap off by any amount.
		let at_cap = format!("{}.dot", "a".repeat(PRODUCT_ID_MAX_BYTES - 4));
		assert_eq!(at_cap.len(), PRODUCT_ID_MAX_BYTES);
		assert!(ProductId::parse(&at_cap).is_ok(), "an id exactly at the cap is still valid");
	}

	#[test]
	fn the_bare_label_drops_the_tld() {
		assert_eq!(id("dim2.dot").bare_label(), "dim2");
		assert_eq!(id("dim2.paseo").bare_label(), "dim2");
		assert_eq!(id("localhost").bare_label(), "localhost");
	}

	#[test]
	fn an_executable_carries_the_label_of_the_product_it_belongs_to() {
		assert_eq!(id("app.dim2.dot").bare_label(), "dim2");
		assert_eq!(id("widget.dim2.dot").bare_label(), "dim2");
		assert_eq!(id("worker.dim2.paseo").bare_label(), "dim2");
		assert_eq!(id("funding.dim2.dot").bare_label(), "dim2");
	}

	#[test]
	fn a_subname_of_another_domain_takes_that_domains_label() {
		assert_eq!(id("dim2.attacker.dot").bare_label(), "attacker");
	}

	#[test]
	fn one_product_on_one_network_is_its_executables_under_one_tld() {
		assert!(id("app.dim2.dot").same_product_on_one_network(&id("dim2.dot")));
		assert!(!id("dim2.paseo").same_product_on_one_network(&id("dim2.dot")));
		assert!(!id("dim2.attacker.dot").same_product_on_one_network(&id("dim2.dot")));
	}

	#[test]
	fn decoding_normalizes_and_rejects_what_parsing_rejects() {
		let frame = "DIM2.Dot".to_string().encode();
		assert_eq!(ProductId::decode(&mut frame.as_slice()), Ok(id("dim2.dot")));
		let frame = "evil.com".to_string().encode();
		assert!(ProductId::decode(&mut frame.as_slice()).is_err());
	}
}
