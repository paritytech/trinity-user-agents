/// Why a descriptor, envelope, pointer or spec was refused.
///
/// Each variant is one rule of the RFC. [`Error::code`] gives the stable name the conformance
/// vectors use, so a vector fails on the wrong reason as well as on the wrong verdict.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
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
	/// A known field is missing or has the wrong type.
	#[error("malformed: {0}")]
	Malformed(String),
	/// `sequence` is above 2^53 - 1.
	#[error("sequence {0} is above 2^53 - 1")]
	SequenceOutOfRange(u64),
	/// Another number is above 2^53 - 1, which I-JSON readers cannot hold exactly.
	#[error("{0} is above 2^53 - 1")]
	NumberOutOfRange(&'static str),
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
	/// A time is not RFC 3339 in UTC ending in `Z`.
	#[error("{0:?} is not an RFC 3339 UTC time")]
	InvalidTime(String),
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
	#[error("chainId is required on eth-rpc and absent on every other kind")]
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
	/// The text does not fit in a QR code.
	#[error("cannot draw a QR code: {0}")]
	QrCode(String),
	/// A secret key is not 32 bytes.
	#[error("a secret key is 32 bytes, got {0}")]
	SecretKeyLength(usize),
}

impl Error {
	/// The stable name of the rule, as the conformance vectors spell it.
	pub fn code(&self) -> &'static str {
		match self {
			Self::NotJson(_) => "not-json",
			Self::DuplicateMember(_) => "duplicate-member",
			Self::UnsupportedVersion(_) => "unsupported-version",
			Self::Malformed(_) => "malformed",
			Self::SequenceOutOfRange(_) => "sequence-out-of-range",
			Self::NumberOutOfRange(_) => "number-out-of-range",
			Self::ForkSignedWithSourceKey => "fork-signed-with-source-key",
			Self::InvalidTld(_) => "invalid-tld",
			Self::Empty(_) => "empty",
			Self::DuplicateChainRole(_) => "duplicate-chain-role",
			Self::DuplicateServiceKind(_) => "duplicate-service-kind",
			Self::DuplicateAssetRole(_) => "duplicate-asset-role",
			Self::RelayCount(_) => "relay-count",
			Self::ParaId(_) => "para-id",
			Self::UnknownChainRole(_) => "unknown-chain-role",
			Self::InvalidTime(_) => "invalid-time",
			Self::InvalidHex { .. } => "invalid-hex",
			Self::InvalidUrl(_) => "invalid-url",
			Self::ServiceUrlNotBase(_) => "service-url-not-base",
			Self::ChainId => "chain-id",
			Self::ForkedAt => "forked-at",
			Self::InvalidBase64 => "invalid-base64",
			Self::EnvelopeVersionMismatch { .. } => "envelope-version-mismatch",
			Self::NonCanonicalKey => "non-canonical-key",
			Self::SmallOrderKey => "small-order-key",
			Self::NonCanonicalR => "non-canonical-r",
			Self::SmallOrderR => "small-order-r",
			Self::NonCanonicalS => "non-canonical-s",
			Self::BadSignature => "bad-signature",
			Self::Unsigned => "unsigned",
			Self::InvalidLink(_) => "invalid-link",
			Self::Unreachable => "unreachable",
			Self::SpecDigest => "spec-digest",
			Self::SpecGenesis(_) => "spec-genesis",
			Self::MalformedSpec(_) => "malformed-spec",
			Self::QrCode(_) => "qr-code",
			Self::SecretKeyLength(_) => "secret-key-length",
		}
	}
}
