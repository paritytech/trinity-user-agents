//! Normalized product identifiers.

use alloc::{
	format,
	string::{String, ToString},
};
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
/// These products receive default remote grants only when no recorded decision
/// overrides them. Device and feature-specific authority checks still apply.
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
mod tests;

#[cfg(feature = "runtime")]
mod runtime {
	use super::{ProductId, ProductIdError};
	use crate::{host_logic::dotns::PRODUCT_SHARE_SUFFIX, platform::ExecutableKind};

	impl ProductId {
		/// Parse only a supported share host, without a URL or network lookup.
		pub fn from_share_host(input: &str) -> Option<Self> {
			let hostname = crate::dotns_name::normalize_host(input);
			let identifier = hostname.strip_suffix(PRODUCT_SHARE_SUFFIX)?.parse::<Self>().ok()?;
			identifier.network_suffix()?;
			Some(identifier)
		}

		/// The public share host on this identity's network, excluding local development.
		pub fn to_share_host(&self) -> Option<String> {
			self.network_suffix()?;
			Some(format!("{self}{PRODUCT_SHARE_SUFFIX}"))
		}

		/// Name an executable without changing its product's nested namespace.
		pub fn executable(&self, kind: ExecutableKind) -> Result<Self, ProductIdError> {
			format!("{}.{self}", kind.dotns_label()).parse()
		}

		/// Remove one executable prefix while keeping a complete, valid product name.
		pub fn executable_owner(&self) -> Result<Self, ProductIdError> {
			if let Some((prefix, owner)) = self.as_str().split_once('.') {
				let is_executable =
					[ExecutableKind::App, ExecutableKind::Widget, ExecutableKind::Worker]
						.iter()
						.any(|kind| kind.dotns_label() == prefix);
				if is_executable && owner.contains('.') {
					return owner.parse();
				}
			}
			Ok(self.clone())
		}
	}

	#[cfg(test)]
	mod tests {
		use super::*;

		#[test]
		fn share_hosts_require_supported_dotns_identities() {
			assert_eq!(
				ProductId::from_share_host("BROWSE.TESTNET.LI"),
				Some(ProductId::for_test("browse.testnet"))
			);
			for input in ["browse.test.li", "browse.dot", "https://browse.dot.li", "localhost.li"] {
				assert_eq!(ProductId::from_share_host(input), None, "{input}");
			}
		}

		#[test]
		fn share_hosts_preserve_the_product_network() {
			for (input, expected) in [
				("browse.dot", Some("browse.dot.li")),
				("browse.paseo", Some("browse.paseo.li")),
				("widget.browse.testnet", Some("widget.browse.testnet.li")),
				("localhost", None),
				("localhost:3000", None),
			] {
				assert_eq!(ProductId::for_test(input).to_share_host().as_deref(), expected);
			}
		}

		#[test]
		fn executable_owners_preserve_nested_product_names() {
			let owner = ProductId::for_test("arena.coinflip.paseo");
			for (kind, expected) in [
				(ExecutableKind::App, "app.arena.coinflip.paseo"),
				(ExecutableKind::Widget, "widget.arena.coinflip.paseo"),
				(ExecutableKind::Worker, "worker.arena.coinflip.paseo"),
			] {
				let executable = owner.executable(kind).unwrap();
				assert_eq!(executable, ProductId::for_test(expected));
				assert_eq!(executable.executable_owner().unwrap(), owner);
			}
		}

		#[test]
		fn only_an_executable_prefix_with_a_complete_owner_is_removed() {
			for (input, expected) in [
				("app.dot", "app.dot"),
				("arena.coinflip.dot", "arena.coinflip.dot"),
				("app.worker.coinflip.dot", "worker.coinflip.dot"),
				("localhost:3000", "localhost:3000"),
			] {
				assert_eq!(
					ProductId::for_test(input).executable_owner().unwrap(),
					ProductId::for_test(expected)
				);
			}
		}

		#[test]
		fn executable_names_cannot_create_invalid_product_ids() {
			assert!(ProductId::for_test("app.localhost.dot").executable_owner().is_err());
			assert!(ProductId::for_test("localhost:3000").executable(ExecutableKind::App).is_err());
			let label = "a".repeat(63);
			let longest =
				ProductId::for_test(&format!("{label}.{label}.{label}.{}.dot", "b".repeat(60)));
			assert!(longest.executable(ExecutableKind::Worker).is_err());
		}
	}
}
