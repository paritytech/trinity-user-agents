//! Raw sign-message bytes, following the polkadot-app raw-signing conventions.
//!
//! A watermarked message carries the `<Bytes>…</Bytes>` envelope, which is what
//! keeps a raw signature from being replayable as a transaction signature.
//! String payloads follow polkadot-app's `isHex` rule, including its refusal to
//! silently sign a corrupt hex body as UTF-8.

use thiserror::Error;
use truapi::latest::RawPayload;

/// Opening half of the watermark a raw message is wrapped in.
const BYTES_WRAP_PREFIX: &[u8] = b"<Bytes>";
/// Closing half of the watermark a raw message is wrapped in.
const BYTES_WRAP_SUFFIX: &[u8] = b"</Bytes>";

/// Why a raw sign payload could not be turned into bytes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RawPayloadError {
	/// A `0x`-prefixed even-length string whose body is not hex. Signing it as
	/// UTF-8 instead would sign something the caller did not mean.
	#[error("raw sign payload is 0x-prefixed but not valid hex")]
	NotValidHex,
}

/// Decode raw sign-message bytes, adding the `<Bytes>…</Bytes>` envelope when
/// watermarked and not already wrapped.
pub fn raw_payload_bytes(
	payload: RawPayload,
	watermarked: bool,
) -> Result<Vec<u8>, RawPayloadError> {
	let raw = match payload {
		RawPayload::Bytes { bytes } => bytes,
		RawPayload::Payload { payload } => decode_payload_string(payload)?,
	};
	if !watermarked || (raw.starts_with(BYTES_WRAP_PREFIX) && raw.ends_with(BYTES_WRAP_SUFFIX)) {
		return Ok(raw);
	}
	let mut wrapped =
		Vec::with_capacity(BYTES_WRAP_PREFIX.len() + raw.len() + BYTES_WRAP_SUFFIX.len());
	wrapped.extend_from_slice(BYTES_WRAP_PREFIX);
	wrapped.extend_from_slice(&raw);
	wrapped.extend_from_slice(BYTES_WRAP_SUFFIX);
	Ok(wrapped)
}

fn decode_payload_string(payload: String) -> Result<Vec<u8>, RawPayloadError> {
	// `isHex`: `0x` prefix and even total length. Odd length is not hex and is
	// signed as UTF-8, matching polkadot-app.
	if let Some(body) = payload.strip_prefix("0x").filter(|_| payload.len().is_multiple_of(2)) {
		return hex::decode(body).map_err(|_| RawPayloadError::NotValidHex);
	}
	Ok(payload.into_bytes())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn raw_payload_bytes_wraps_and_decodes() {
		let ok = |p| raw_payload_bytes(p, true).expect("payload ok");
		// Bytes are <Bytes>-wrapped.
		assert_eq!(ok(RawPayload::Bytes { bytes: b"hi".to_vec() }), b"<Bytes>hi</Bytes>".to_vec(),);
		// A 0x-hex string payload decodes to bytes before wrapping.
		assert_eq!(
			ok(RawPayload::Payload { payload: "0xdeadbeef".to_string() }),
			[BYTES_WRAP_PREFIX, &[0xde, 0xad, 0xbe, 0xef], BYTES_WRAP_SUFFIX].concat(),
		);
		// A non-hex string payload is signed as UTF-8.
		assert_eq!(
			ok(RawPayload::Payload { payload: "hello".to_string() }),
			b"<Bytes>hello</Bytes>".to_vec(),
		);
		// An odd-length 0x string is not `isHex`, so it is signed as UTF-8.
		assert_eq!(
			ok(RawPayload::Payload { payload: "0xabc".to_string() }),
			b"<Bytes>0xabc</Bytes>".to_vec(),
		);
		// Already-wrapped input is left untouched (no double wrapping).
		assert_eq!(
			ok(RawPayload::Bytes { bytes: b"<Bytes>hi</Bytes>".to_vec() }),
			b"<Bytes>hi</Bytes>".to_vec(),
		);
		// An even-length 0x string that is not valid hex is a hard error,
		// never silently signed as UTF-8 (matches polkadot-app abort).
		assert_eq!(
			raw_payload_bytes(RawPayload::Payload { payload: "0xZZ".to_string() }, true),
			Err(RawPayloadError::NotValidHex),
		);
	}
}
