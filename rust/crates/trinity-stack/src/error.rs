/// Why a descriptor, envelope, pointer or spec was refused.
///
/// Each variant is one rule of the RFC. [`Error::code`] gives the stable name the conformance
/// vectors use, so a vector fails on the wrong reason as well as on the wrong verdict.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Error {
	/// The bytes are not valid UTF-8 JSON.
	#[error("not JSON: {0}")]
	NotJson(String),
	/// An object repeats a member name, which I-JSON forbids.
	#[error("duplicate member {0:?}")]
	DuplicateMember(String),
	/// `$v` names a format this reader does not know.
	#[error("unsupported format version {0}")]
	UnsupportedVersion(u64),
	/// A known field is missing, has the wrong type, or holds a number outside its range.
	#[error("malformed: {0}")]
	Malformed(String),
	/// A fork is signed with the key of the stack it was forked from.
	#[error("a fork must not be signed with its source's key")]
	ForkSignedWithSourceKey,
	/// `tld` is not one lowercase DNS label without the leading dot.
	#[error("tld {0:?} must be one lowercase label of a-z, 0-9 and -")]
	InvalidTld(String),
	/// A field that must not be empty is empty.
	#[error("{0} is empty")]
	Empty(&'static str),
	/// Two chains have the same role.
	#[error("chain role {0:?} appears more than once")]
	DuplicateChainRole(String),
	/// Two services have the same kind.
	#[error("service kind {0:?} appears more than once")]
	DuplicateServiceKind(String),
	/// Two assets have the same role.
	#[error("asset role {0:?} appears more than once")]
	DuplicateAssetRole(String),
	/// The descriptor does not have exactly one `Relay` chain.
	#[error("expected exactly one Relay chain, found {0}")]
	RelayCount(usize),
	/// A `paraId` is missing on a parachain, present on the relay, or repeated.
	#[error("paraId: {0}")]
	ParaId(String),
	/// `Asset.chain` or `coinage.chain` names a role no chain has.
	#[error("no chain has role {0:?}")]
	UnknownChainRole(String),
	/// A hex value is not lowercase, 0x-prefixed and of the right length.
	#[error("{field} is not {bytes} bytes of lowercase 0x-prefixed hex")]
	InvalidHex {
		/// Which field.
		field: &'static str,
		/// The length it must have.
		bytes: usize,
	},
	/// A URL uses a scheme the RFC does not allow where it appears.
	#[error("{0:?} is not an allowed URL")]
	InvalidUrl(String),
	/// An HTTP service URL is not a base URL ending in `/`.
	#[error("service URL {0:?} must end in /")]
	ServiceUrlNotBase(String),
	/// `chainId` is missing on `eth-rpc` or present on another kind.
	#[error("chainId is required on eth-rpc, up to 2^53 - 1, and absent on every other kind")]
	ChainId,
	/// `forkedAt` is missing on a chain of a fork, or set when the stack is no fork.
	#[error("forkedAt must be set on every chain exactly when fork is set")]
	ForkedAt,
	/// An envelope's `payload` is not standard padded base64.
	#[error("payload is not standard base64")]
	InvalidBase64,
	/// The payload's `$v` differs from the envelope's.
	#[error("envelope $v {envelope} differs from payload $v {payload}")]
	EnvelopeVersionMismatch {
		/// The envelope's `$v`.
		envelope: u64,
		/// The payload's `$v`.
		payload: u64,
	},
	/// The public key is not a canonical point encoding.
	#[error("public key is not canonically encoded")]
	NonCanonicalKey,
	/// The public key is a small-order point.
	#[error("public key has small order")]
	SmallOrderKey,
	/// The signature's `R` is not a canonical point encoding.
	#[error("signature R is not canonically encoded")]
	NonCanonicalR,
	/// The signature's `R` is a small-order point.
	#[error("signature R has small order")]
	SmallOrderR,
	/// The signature's `S` is not below the group order.
	#[error("signature S is not below the group order")]
	NonCanonicalS,
	/// The signature does not verify.
	#[error("signature does not verify")]
	BadSignature,
	/// A pointer with a key was given a plain descriptor.
	#[error("the pointer has a key but the document is not signed")]
	Unsigned,
	/// A link has no usable pointer in its query.
	#[error("invalid link: {0}")]
	InvalidLink(String),
	/// Nothing was fetched and nothing is stored.
	#[error("the descriptor could not be fetched and none is stored")]
	Unreachable,
	/// A spec's bytes do not match its `sha256`.
	#[error("spec bytes do not match sha256")]
	SpecDigest,
	/// A light spec implies a different genesis hash than its chain's.
	#[error("light spec implies genesis {0}")]
	SpecGenesis(String),
	/// A spec is not a chain spec this crate can read.
	#[error("malformed spec: {0}")]
	MalformedSpec(String),
	/// A secret key is not 32 bytes.
	#[error("a secret key is 32 bytes, got {0}")]
	SecretKeyLength(usize),
}

impl Error {
	/// The stable name of the rule, as the conformance vectors spell it: the variant in kebab
	/// case. Renaming a variant changes its code, so pin the old one with `#[strum(serialize)]`.
	pub fn code(&self) -> &'static str {
		self.into()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Codes are a published contract: readers in other languages match on them.
	#[test]
	fn codes_keep_their_published_spelling() {
		let codes = [
			(Error::NotJson(String::new()), "not-json"),
			(Error::InvalidBase64, "invalid-base64"),
			(Error::NonCanonicalR, "non-canonical-r"),
			(Error::ParaId(String::new()), "para-id"),
			(Error::InvalidHex { field: "f", bytes: 1 }, "invalid-hex"),
			(
				Error::EnvelopeVersionMismatch { envelope: 1, payload: 2 },
				"envelope-version-mismatch",
			),
			(Error::ForkSignedWithSourceKey, "fork-signed-with-source-key"),
			(Error::SecretKeyLength(3), "secret-key-length"),
		];
		for (error, code) in codes {
			assert_eq!(error.code(), code);
		}
	}
}
