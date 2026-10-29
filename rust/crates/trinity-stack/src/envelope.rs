use crate::{
	Descriptor, Error, Pointer,
	descriptor::{self, VERSION},
	format, ijson,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use curve25519_dalek::{edwards::CompressedEdwardsY, scalar::Scalar};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::Serialize;

/// A document as served: a plain descriptor, or a descriptor inside a signed envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Document {
	/// A descriptor with no signature.
	Plain(Descriptor),
	/// A descriptor inside an envelope, not yet checked against any key.
	Envelope {
		/// The descriptor the payload carries.
		descriptor: Descriptor,
		/// The decoded payload: the exact bytes that were signed.
		payload: Vec<u8>,
		/// The signature over the signing prefix and the payload.
		signature: [u8; 64],
	},
}

/// A descriptor that passed a pointer's rules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
pub struct Verified {
	/// The descriptor.
	pub descriptor: Descriptor,
	/// Whether a signature was checked against the pointer's key.
	pub signed: bool,
	/// The descriptor's own bytes: the payload of an envelope, or the whole plain document.
	#[serde(skip)]
	content: Vec<u8>,
}

impl Verified {
	/// Whether two copies carry the same descriptor bytes, whatever envelope they came in.
	pub fn same_content(&self, other: &Self) -> bool {
		self.content == other.content
	}
}

impl Document {
	/// Parses a document as served. A document with a `payload` member is an envelope.
	pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
		let document = ijson::parse(bytes)?;
		if document.get("payload").is_none() {
			return Descriptor::from_json(document).map(Self::Plain);
		}
		let envelope_version = descriptor::version_of(&document)?;
		if envelope_version != VERSION {
			return Err(Error::UnsupportedVersion(envelope_version));
		}
		let field = |name: &str| {
			document
				.get(name)
				.and_then(|value| value.as_str())
				.ok_or_else(|| Error::Malformed(format!("envelope {name} must be a string")))
		};
		let payload = STANDARD.decode(field("payload")?).map_err(|_| Error::InvalidBase64)?;
		let signature = format::hex_bytes::<64>("signature", field("signature")?)?;
		let inner = ijson::parse(&payload)?;
		if inner.get("payload").is_some() {
			return Err(Error::Malformed("an envelope payload cannot be an envelope".into()));
		}
		let payload_version = descriptor::version_of(&inner)?;
		if payload_version != envelope_version {
			return Err(Error::EnvelopeVersionMismatch {
				envelope: envelope_version,
				payload: payload_version,
			});
		}
		let descriptor = Descriptor::from_json(inner)?;
		Ok(Self::Envelope { descriptor, payload, signature })
	}

	/// The descriptor, whatever the document's form.
	pub fn descriptor(&self) -> &Descriptor {
		match self {
			Self::Plain(descriptor) | Self::Envelope { descriptor, .. } => descriptor,
		}
	}
}

/// Parses a document and applies the pointer's rules: a pointer with a key accepts only an
/// envelope that verifies against it, and a pointer without one reads an envelope as unsigned.
pub fn open(bytes: &[u8], pointer: &Pointer) -> Result<Verified, Error> {
	match (Document::parse(bytes)?, pointer.key()) {
		(Document::Plain(_), Some(_)) => Err(Error::Unsigned),
		(Document::Plain(descriptor), None) => {
			Ok(Verified { descriptor, signed: false, content: bytes.to_vec() })
		},
		(Document::Envelope { descriptor, payload, .. }, None) => {
			Ok(Verified { descriptor, signed: false, content: payload })
		},
		(Document::Envelope { descriptor, payload, signature }, Some(key)) => {
			verify(key, descriptor.version, &payload, &signature)?;
			refuse_self_fork(&descriptor, key)?;
			Ok(Verified { descriptor, signed: true, content: payload })
		},
	}
}

/// A descriptor signed with a stack's key is that stack, so a fork signed with its source's key
/// would claim to be the source and a fork of it at once.
fn refuse_self_fork(descriptor: &Descriptor, key: &[u8; 32]) -> Result<(), Error> {
	let source_key = descriptor.fork.as_ref().and_then(|fork| fork.of.key.as_deref());
	match source_key {
		Some(source) if format::hex_bytes::<32>("fork.of.key", source)? == *key => {
			Err(Error::ForkSignedWithSourceKey)
		},
		_ => Ok(()),
	}
}

/// Wraps a descriptor's bytes in a signed envelope. The bytes are checked first, so an operator
/// cannot publish a signature over a descriptor readers would refuse.
pub fn sign(descriptor: &[u8], secret: &[u8; 32]) -> Result<Vec<u8>, Error> {
	let Document::Plain(parsed) = Document::parse(descriptor)? else {
		return Err(Error::Malformed("sign takes a plain descriptor, not an envelope".into()));
	};
	refuse_self_fork(&parsed, &public_key(secret))?;
	let signature =
		SigningKey::from_bytes(secret).sign(&signed_message(parsed.version, descriptor));
	let envelope = serde_json::json!({
		"$v": parsed.version,
		"payload": STANDARD.encode(descriptor),
		"signature": format::to_hex(&signature.to_bytes()),
	});
	Ok(serde_json::to_vec(&envelope).expect("a JSON value always serialises"))
}

/// The public key of a secret key, as a pointer names it.
pub fn public_key(secret: &[u8; 32]) -> [u8; 32] {
	SigningKey::from_bytes(secret).verifying_key().to_bytes()
}

/// The signing prefix `trinity-stack/descriptor/v<$v>` followed by the payload bytes.
fn signed_message(version: u64, payload: &[u8]) -> Vec<u8> {
	let mut message = format!("trinity-stack/descriptor/v{version}").into_bytes();
	message.extend_from_slice(payload);
	message
}

/// Strict RFC 8032 verification: canonical `A` and `R`, neither of small order, `S` below the
/// group order, and the cofactorless equation.
fn verify(key: &[u8; 32], version: u64, payload: &[u8], signature: &[u8; 64]) -> Result<(), Error> {
	check_point(key, Error::NonCanonicalKey, Error::SmallOrderKey)?;
	let (r, s) = signature.split_at(32);
	check_point(r.try_into().expect("32 bytes"), Error::NonCanonicalR, Error::SmallOrderR)?;
	let s: [u8; 32] = s.try_into().expect("32 bytes");
	if Scalar::from_canonical_bytes(s).is_none().into() {
		return Err(Error::NonCanonicalS);
	}
	let key = VerifyingKey::from_bytes(key).map_err(|_| Error::NonCanonicalKey)?;
	key.verify_strict(&signed_message(version, payload), &Signature::from_bytes(signature))
		.map_err(|_| Error::BadSignature)
}

fn check_point(bytes: &[u8; 32], non_canonical: Error, small_order: Error) -> Result<(), Error> {
	let point = CompressedEdwardsY(*bytes).decompress().ok_or(non_canonical.clone())?;
	if point.compress().as_bytes() != bytes {
		return Err(non_canonical);
	}
	if point.is_small_order() {
		return Err(small_order);
	}
	Ok(())
}
