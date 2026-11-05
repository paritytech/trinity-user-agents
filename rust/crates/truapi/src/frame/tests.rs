use super::*;

fn build(trait_id: u8, method_id: u8, message_type: u8, value: Vec<u8>) -> ProtocolMessage {
	ProtocolMessage {
		request_id: "p:1".to_string(),
		payload: Payload { trait_id, method_id, message_type, value },
	}
}

fn expected_wire(trait_id: u8, method_id: u8, message_type: u8, value: &[u8]) -> Vec<u8> {
	let mut out = Vec::new();
	"p:1".to_string().encode_to(&mut out);
	out.push(trait_id);
	out.push(method_id);
	out.push(message_type);
	out.extend_from_slice(value);
	out
}

#[test]
fn handshake_request_encodes_with_the_system_trait_pair() {
	// SCALE-encoded HostHandshakeRequest::V1(2u8) = [0u8 variant][2u8 codec_version]
	let inner: Vec<u8> = vec![0x00, 0x02];
	// system trait = 1, handshake request = 0, message_type = Request.
	let msg = build(1, 0, MESSAGE_TYPE_REQUEST, inner.clone());
	assert_eq!(msg.encode(), expected_wire(1, 0, MESSAGE_TYPE_REQUEST, &inner));
}

/// Pins where the pair and message type land for a multi-byte payload. The
/// payload here is an arbitrary blob, not a real `HostAccountGetRequest` —
/// this layer inlines payload bytes verbatim and never interprets them.
/// The typed layout of an `account_get_account` payload is asserted in
/// `tests/golden_frame.rs` against the golden fixture.
#[test]
fn get_account_request_encodes_with_discriminant_pair() {
	let mut inner = vec![0x00]; // V1 variant
	"foo".to_string().encode_to(&mut inner);
	0u32.encode_to(&mut inner);
	// account trait = 2, get_account request = 4.
	let msg = build(2, 4, MESSAGE_TYPE_REQUEST, inner.clone());
	assert_eq!(msg.encode(), expected_wire(2, 4, MESSAGE_TYPE_REQUEST, &inner));
}

#[test]
fn round_trip_preserves_ids_message_type_and_value() {
	let inner: Vec<u8> = vec![0x00, 0x42, 0xab, 0xcd];
	let msg = build(199, 0, MESSAGE_TYPE_RESPONSE, inner.clone());
	let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
	assert_eq!(decoded, msg);
}

/// An unknown discriminant pair is not rejected at decode; routing is
/// deferred to dispatch (which reports frames with no registered handler).
#[test]
fn unknown_discriminant_pair_decodes_ok() {
	let mut bytes = Vec::new();
	"p:1".to_string().encode_to(&mut bytes);
	bytes.push(250); // far outside the populated trait range
	bytes.push(123);
	bytes.push(MESSAGE_TYPE_REQUEST);
	bytes.extend_from_slice(&[0xaa, 0xbb]);
	let decoded = ProtocolMessage::decode(&mut &bytes[..]).expect("unknown pair must decode");
	assert_eq!(decoded.payload.trait_id, 250);
	assert_eq!(decoded.payload.method_id, 123);
	assert_eq!(decoded.payload.message_type, MESSAGE_TYPE_REQUEST);
	assert_eq!(decoded.payload.value, vec![0xaa, 0xbb]);
}

/// The reserved `(255, 255)` address is the one channel every peer must
/// answer on, so a build that rejected an unfamiliar payload here could
/// never be told anything new without the connection dying. An unknown
/// variant index therefore decodes to `None` and leaves the frame intact,
/// which is what lets a later protocol add a richer protocol error without
/// a wire break.
#[test]
fn an_unknown_protocol_error_variant_is_tolerated() {
	// A variant index this build has never heard of, with a payload whose
	// length it cannot know either.
	// An unknown version (byte 0) and an unknown `V1` variant (byte 1).
	for payload in [vec![7], vec![7, 1, 2, 3], vec![200, 0xff], vec![0, 9, 1]] {
		assert_eq!(
			decode_protocol_error_payload(&payload).expect("tolerated"),
			None,
			"unknown variant must not fail the payload decode"
		);

		let mut bytes = Vec::new();
		"p:1".to_string().encode_to(&mut bytes);
		bytes.push(PROTOCOL_ERROR_TRAIT_ID);
		bytes.push(PROTOCOL_ERROR_METHOD_ID);
		bytes.push(MESSAGE_TYPE_RESPONSE);
		bytes.extend_from_slice(&payload);
		let decoded = ProtocolMessage::decode(&mut &bytes[..])
			.expect("an unknown protocol error must still decode as a frame");
		assert_eq!(decoded.payload.value, payload);
	}
}

#[test]
fn protocol_error_payload_has_stable_versioned_shape() {
	let error = VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedMessage {
		trait_id: 250,
		method_id: 251,
	});
	let encoded = error.encode();
	let decoded = decode_protocol_error_payload(&encoded).expect("decode");
	// [0] versioned index, [0] variant index, [250] trait, [251] method.
	// Trait and method differ here on purpose, so transposing the two
	// fields cannot pass.
	assert_eq!((encoded, decoded), (vec![0, 0, 250, 251], Some(error)));

	let error = VersionedProtocolError::V1(ProtocolErrorV1::UnsupportedVersion {
		trait_id: 250,
		method_id: 251,
		newest_version: 2,
	});
	let encoded = error.encode();
	let decoded = decode_protocol_error_payload(&encoded).expect("decode");
	// [0] versioned index, [1] variant index, trait, method, newest version.
	assert_eq!((encoded, decoded), (vec![0, 1, 250, 251, 2], Some(error)));
}

#[test]
fn malformed_protocol_error_payloads_fail_frame_decoding() {
	// Re-derived for the 2-byte address: a valid payload is 4 bytes, so
	// `[0, 0, 250, 0]` - the old trailing-byte case - decodes cleanly as
	// the pair (250, 0) and would silently stop testing anything.
	//
	// An unknown version or variant index is deliberately absent from this
	// list: that is a newer peer rather than corruption, and is tolerated
	// so the protocol-error channel stays extensible. See
	// `an_unknown_protocol_error_variant_is_tolerated`.
	for payload in [
		vec![],                     // no payload at all
		vec![0],                    // version present, variant truncated
		vec![0, 0],                 // no address at all
		vec![0, 0, 250],            // trait present, method truncated
		vec![0, 0, 250, 251, 0],    // one trailing byte past a full pair
		vec![0, 1, 250, 251],       // newest version truncated
		vec![0, 1, 250, 251, 2, 0], // one trailing byte past a full version
	] {
		let message = ProtocolMessage {
			request_id: "p:1".into(),
			payload: Payload {
				trait_id: PROTOCOL_ERROR_TRAIT_ID,
				method_id: PROTOCOL_ERROR_METHOD_ID,
				message_type: MESSAGE_TYPE_REQUEST,
				value: payload,
			},
		};
		assert!(ProtocolMessage::decode(&mut message.encode().as_slice()).is_err());
	}
}

/// All four subscription phases share one `(trait, method)` address now
/// (message type is the wire's own third byte, no longer inside the
/// payload) and still round-trip through the codec. Catches a regression
/// where `Decode` mishandles an empty `Stop` payload but carries more for
/// `Start`/`Interrupt`/`Receive`. The address is
/// `account_connection_status_subscribe`'s (trait 2, method 0).
#[test]
fn subscription_phases_round_trip_through_codec() {
	let cases: &[(u8, Vec<u8>)] = &[
		(MESSAGE_TYPE_START, vec![0x00, 0xaa]), // start: [version, item bytes]
		(MESSAGE_TYPE_RECEIVE, vec![0x00, 0x01, 0x02, 0x03]), // receive: [version, item bytes]
		(MESSAGE_TYPE_INTERRUPT, vec![0x00]),   // interrupt: Ok(())
		(MESSAGE_TYPE_STOP, vec![]),            // stop: no payload at all
	];
	for (message_type, value) in cases {
		let msg = build(2, 0, *message_type, value.clone());
		let bytes = msg.encode();
		assert_eq!(
			bytes,
			expected_wire(2, 0, *message_type, value),
			"encode mismatch for message_type {message_type} payload {value:?}"
		);
		let decoded = ProtocolMessage::decode(&mut &bytes[..]).expect("decode");
		assert_eq!(
			decoded, msg,
			"round-trip mismatch for message_type {message_type} payload {value:?}"
		);
	}
}

/// `request_ids` / `subscription_ids` resolve a method name to its
/// generated discriminants without going through the codec.
#[test]
fn id_helpers_resolve_known_methods() {
	let handshake = request_ids("system_handshake").expect("known request method");
	assert_eq!(handshake.trait_id, 1);
	assert_eq!(handshake.method_id, 0);

	let get_account = request_ids("account_get_account").expect("known request method");
	assert_eq!(get_account.trait_id, 2);
	assert_eq!(get_account.method_id, 1);

	let sub = subscription_ids("account_connection_status_subscribe").expect("known subscription");
	assert_eq!(sub.trait_id, 2);
	assert_eq!(sub.method_id, 0);

	// A request method is not a subscription and vice versa.
	assert!(subscription_ids("system_handshake").is_none());
	assert!(request_ids("account_connection_status_subscribe").is_none());
	assert!(request_ids("not_a_method").is_none());
}

#[test]
fn encode_clean_interrupt_is_a_unit_ok() {
	assert_eq!(encode_clean_interrupt(), vec![0]);
}

/// Genuine zero-byte payload (e.g. unit-typed response, or a `Stop` frame).
/// `Decode` must handle `remaining_len == 0` without erroring or reading
/// past EOF.
#[test]
fn empty_payload_round_trips() {
	// local_storage_clear_response = (7, 5).
	let msg = build(7, 5, MESSAGE_TYPE_RESPONSE, Vec::new());
	let bytes = msg.encode();
	// [SCALE compact-len 0x0c][p][:][1][u8 7][u8 5][u8 message_type] = 4 + 3 = 7 bytes total
	assert_eq!(bytes.len(), 7);
	let decoded = ProtocolMessage::decode(&mut &bytes[..]).expect("decode");
	assert_eq!(decoded, msg);
}

/// Compact-len mode 1 kicks in for strings with length 64..=16383. Make
/// sure the codec handles a long requestId without truncation.
#[test]
fn long_request_id_round_trips() {
	let long_id: String = "x".repeat(200);
	let msg = ProtocolMessage {
		request_id: long_id,
		payload: Payload {
			trait_id: 194,
			method_id: 4,
			message_type: MESSAGE_TYPE_REQUEST,
			value: vec![0x00, 0xab, 0xcd],
		},
	};
	let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
	assert_eq!(decoded, msg);
}

/// Truncated frames must surface a `CodecError`, not panic, and the
/// trait-byte, method-byte, and message-type-byte truncations report
/// distinct errors.
#[test]
fn truncated_frames_error_cleanly() {
	// Empty buffer.
	assert!(ProtocolMessage::decode(&mut &[][..]).is_err());
	// Just the requestId, no trait byte.
	let mut only_request_id = Vec::new();
	"p:1".to_string().encode_to(&mut only_request_id);
	let err = ProtocolMessage::decode(&mut &only_request_id[..])
		.expect_err("missing trait byte must error");
	assert!(format!("{err}").contains("trait discriminant"), "unexpected error: {err}");
	// RequestId plus the trait byte, no method byte.
	let mut missing_method = only_request_id.clone();
	missing_method.push(0);
	let err = ProtocolMessage::decode(&mut &missing_method[..])
		.expect_err("missing method byte must error");
	assert!(format!("{err}").contains("method discriminant"), "unexpected error: {err}");
	// RequestId plus trait and method bytes, no message-type byte.
	let mut missing_message_type = missing_method.clone();
	missing_message_type.push(0);
	let err = ProtocolMessage::decode(&mut &missing_message_type[..])
		.expect_err("missing message-type byte must error");
	assert!(format!("{err}").contains("message-type"), "unexpected error: {err}");
	// RequestId header claims length=200 but the buffer is far shorter.
	let truncated_str_header = [200u8 << 2, 0x61, 0x62, 0x63];
	assert!(ProtocolMessage::decode(&mut &truncated_str_header[..]).is_err());
}

/// Empty requestId (zero-length string) is a valid SCALE-encoded `str`
/// (compact-len 0, no body). The codec must round-trip it without
/// confusing length-0 with EOF.
#[test]
fn empty_request_id_round_trips() {
	let msg = ProtocolMessage {
		request_id: String::new(),
		payload: Payload {
			trait_id: 194,
			method_id: 4,
			message_type: MESSAGE_TYPE_RESPONSE,
			value: vec![0x00, 0x01, 0x02],
		},
	};
	let bytes = msg.encode();
	// [SCALE compact-len 0 = 0x00][trait][method][message_type][payload]
	assert_eq!(bytes[0], 0x00);
	let decoded = ProtocolMessage::decode(&mut &bytes[..]).expect("decode");
	assert_eq!(decoded, msg);
}

/// Unicode characters round-trip through SCALE string encoding.
#[test]
fn unicode_request_id_round_trips() {
	let msg = ProtocolMessage {
		request_id: "héllo-世界-🦀".to_string(),
		payload: Payload {
			trait_id: 194,
			method_id: 4,
			message_type: MESSAGE_TYPE_REQUEST,
			value: vec![0x00, 0x01],
		},
	};
	let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
	assert_eq!(decoded, msg);
}

/// Large payload (>64KiB) round-trips. Catches buffer-size assumptions
/// in the inline-payload read path.
#[test]
fn large_payload_round_trips() {
	let big = vec![0xa5u8; 100 * 1024];
	let msg = build(194, 4, MESSAGE_TYPE_RESPONSE, big);
	let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
	assert_eq!(decoded, msg);
}

/// A two-version error envelope, standing in for any real one. `V2` adds a
/// variant `V1` has no room for, which is the case the downgrade exists for.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
enum ProbeError {
	V1(ProbeErrorV1),
	V2(ProbeErrorV2),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode)]
enum ProbeErrorV1 {
	Full,
	Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode)]
enum ProbeErrorV2 {
	Full,
	Refused,
	Unknown,
}

impl Versioned for ProbeError {
	type Latest = ProbeErrorV2;
	const LATEST: u8 = 2;
	fn version(&self) -> u8 {
		match self {
			Self::V1(_) => 1,
			Self::V2(_) => 2,
		}
	}
}

impl IntoLatest for ProbeError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(ProbeErrorV1::Full) => ProbeErrorV2::Full,
			Self::V1(ProbeErrorV1::Unknown) => ProbeErrorV2::Unknown,
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for ProbeError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 {
			return Self::V2(latest);
		}
		Self::V1(match latest {
			ProbeErrorV2::Full => ProbeErrorV1::Full,
			ProbeErrorV2::Refused | ProbeErrorV2::Unknown => ProbeErrorV1::Unknown,
		})
	}
}

#[test]
fn a_domain_error_is_downgraded_to_the_callers_version() {
	// Without this the frame says v0.1 while the payload inside is still
	// v0.2-tagged, and the caller cannot decode its own response.
	assert_eq!(
		downgrade_call_error(CallError::Domain(ProbeError::V2(ProbeErrorV2::Full)), 1),
		CallError::Domain(ProbeError::V1(ProbeErrorV1::Full))
	);
	assert_eq!(
		downgrade_call_error(CallError::Domain(ProbeError::V2(ProbeErrorV2::Full)), 2),
		CallError::Domain(ProbeError::V2(ProbeErrorV2::Full))
	);
}

#[test]
fn a_variant_the_caller_has_no_room_for_collapses_rather_than_leaking() {
	// `Refused` exists only in v0.2. A v0.1 caller must get something it can
	// decode, not a variant index past the end of its enum.
	assert_eq!(
		downgrade_call_error(CallError::Domain(ProbeError::V2(ProbeErrorV2::Refused)), 1),
		CallError::Domain(ProbeError::V1(ProbeErrorV1::Unknown))
	);
}

#[test]
fn framework_variants_pass_through_every_version() {
	// They carry no payload, so there is nothing to downgrade.
	for version in [1u8, 2u8] {
		assert_eq!(
			downgrade_call_error::<ProbeError>(CallError::Denied, version),
			CallError::Denied
		);
		assert_eq!(
			downgrade_call_error::<ProbeError>(CallError::Unsupported, version),
			CallError::Unsupported
		);
		assert_eq!(
			downgrade_call_error::<ProbeError>(
				CallError::HostFailure { reason: "boom".to_string() },
				version
			),
			CallError::HostFailure { reason: "boom".to_string() }
		);
	}
}

/// IdFactory mints monotonically increasing ids prefixed with the
/// configured string.
#[test]
fn id_factory_minted_ids_are_unique_and_monotonic() {
	let mut factory = IdFactory::new("p:");
	assert_eq!(factory.next_id(), "p:1");
	assert_eq!(factory.next_id(), "p:2");
	assert_eq!(factory.next_id(), "p:3");
}

/// Two distinct factories each maintain their own counter; minting from
/// one does not advance the other.
#[test]
fn two_factories_dont_share_state() {
	let mut a = IdFactory::new("a:");
	let mut b = IdFactory::new("b:");
	assert_eq!(a.next_id(), "a:1");
	assert_eq!(b.next_id(), "b:1");
	assert_eq!(a.next_id(), "a:2");
	assert_eq!(b.next_id(), "b:2");
}
