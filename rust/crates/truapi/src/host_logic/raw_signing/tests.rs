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
