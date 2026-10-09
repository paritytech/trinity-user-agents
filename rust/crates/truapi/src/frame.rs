//! Wire protocol frame types.
//!
//! Every message on the wire is a `ProtocolMessage` containing a `requestId`
//! and a `payload`. On the wire the envelope is:
//!
//! ```text
//!   [requestId: SCALE str][trait: u8][method: u8][message_type: u8][payload bytes...]
//! ```
//!
//! The `(trait, method)` discriminant pair maps to a method/kind slot via the
//! auto-generated [`crate::generated::wire_table::WIRE_TABLE`]. Trait ids and
//! per-trait method ordering are part of the wire protocol; only ever append
//! within a trait. `message_type` (see the `MESSAGE_TYPE_*` constants) names
//! which leg of that method's exchange this frame carries — `Request`/
//! `Response`, or a subscription's `Start`/`Receive`/`Interrupt`/`Stop` —
//! generically, without decoding the payload: the framework dispatcher, the
//! subscription manager, and any tooling that taps the wire can all read it
//! directly off `Payload`. The payload bytes that follow are that leg's own
//! versioned wrapper (e.g. `{Method}Request`), SCALE-encoded and inlined
//! without a length prefix; nothing about the leg lives inside them.
//!
//! In-memory we keep the numeric pair directly so dispatch does not need to
//! reconstruct string action tags on every frame.

use parity_scale_codec::{Decode, Encode, Error as CodecError, Input, Output};
use truapi::CallError;
use truapi::versioned::{FromLatest, IntoLatest, Versioned};

use crate::generated::wire_table::{MethodIds, WIRE_TABLE, WireKind};
use crate::protocol_error::decode_protocol_error_payload;

/// Top-level wire message. Encoded as
/// `[requestId][trait][method][message_type][bytes]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolMessage {
    /// Per-message identifier carried by both halves of a request/response.
    pub request_id: String,
    /// Tagged payload describing the frame kind and SCALE bytes.
    pub payload: Payload,
}

/// Reserved trait discriminant for method-independent protocol errors. No API
/// trait may declare it (the codegen rejects `#[wire_trait(id = 255)]`), so no
/// method can ever be addressed here.
pub const PROTOCOL_ERROR_TRAIT_ID: u8 = 255;

/// Reserved method discriminant for method-independent protocol errors, within
/// [`PROTOCOL_ERROR_TRAIT_ID`].
pub const PROTOCOL_ERROR_METHOD_ID: u8 = 255;

/// The reserved `(trait, method)` address protocol errors travel on.
pub const PROTOCOL_ERROR_KEY: (u8, u8) = (PROTOCOL_ERROR_TRAIT_ID, PROTOCOL_ERROR_METHOD_ID);

/// Versioned payload carried by [`PROTOCOL_ERROR_KEY`] frames.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum VersionedProtocolError {
    /// Initial protocol error shape.
    #[codec(index = 0)]
    V1(ProtocolErrorV1),
}

/// Protocol errors supported by codec version 1.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ProtocolErrorV1 {
    /// The receiver does not support the incoming message address. Codec 2
    /// addresses a frame by `(trait, method)`, so one byte can no longer name
    /// it: a bare method id is ambiguous across traits.
    #[codec(index = 0)]
    UnsupportedMessage {
        /// Trait discriminant of the unsupported incoming frame.
        trait_id: u8,
        /// Method discriminant of the unsupported incoming frame.
        method_id: u8,
    },
}

/// Downgrade a call error's domain payload to the version its caller speaks.
///
/// A handler answers in latest terms, so without this a peer that asked in
/// v0.1 receives a v0.2-tagged error it cannot decode: the domain payload
/// carries its own variant tag and that tag has to name the version the caller
/// speaks. The framework variants carry no version and pass through.
pub fn downgrade_call_error<E>(error: CallError<E>, version: u8) -> CallError<E>
where
    E: Versioned + IntoLatest + FromLatest,
{
    match error {
        CallError::Domain(domain) => {
            CallError::Domain(E::from_latest(domain.into_latest(), version))
        }
        CallError::Denied => CallError::Denied,
        CallError::Unsupported => CallError::Unsupported,
        CallError::MalformedFrame { reason } => CallError::MalformedFrame { reason },
        CallError::HostFailure { reason } => CallError::HostFailure { reason },
        CallError::Cancelled => CallError::Cancelled,
    }
}

/// Which leg of a method's exchange a frame carries: `Request`/`Response` for
/// a plain request/response method, or a subscription's `Start`/`Receive`/
/// `Interrupt`/`Stop`. Wire-level, third byte of every frame (see the module
/// doc), so the dispatcher, the subscription manager and a debug tap can all
/// read it generically, without decoding the leg's own payload.
///
/// `Request` and `Start` share `0`, `Response` and `Receive` share `1`: a
/// subscription's first two legs occupy the same slots a plain method's two
/// legs would, so the byte alone plus the method's registered kind (never
/// both a request and a subscription) resolves unambiguously.
pub const MESSAGE_TYPE_REQUEST: u8 = 0;
/// See [`MESSAGE_TYPE_REQUEST`].
pub const MESSAGE_TYPE_START: u8 = 0;
/// See [`MESSAGE_TYPE_REQUEST`].
pub const MESSAGE_TYPE_RESPONSE: u8 = 1;
/// See [`MESSAGE_TYPE_REQUEST`].
pub const MESSAGE_TYPE_RECEIVE: u8 = 1;
/// A subscription's stream-ending frame, carrying `Result<(), Interrupt>`:
/// `Err(value)` for the method's own interrupt value, `Ok(())` for natural
/// completion. The `Result` framing is fixed; a domain error inside the
/// interrupt value carries its own version wrapper.
pub const MESSAGE_TYPE_INTERRUPT: u8 = 2;
/// A subscription's cancellation, product → host. Carries no payload at all.
pub const MESSAGE_TYPE_STOP: u8 = 3;
/// A request's withdrawal, travelling in the same direction as its `Request`
/// and correlated by the same `requestId`. Carries no payload at all. The
/// call still settles with exactly one `Response`; this only fires the
/// handler's [`truapi::CancellationToken`].
pub const MESSAGE_TYPE_CANCEL: u8 = 4;

/// Encode `Err(CallError::Cancelled)`, the terminal response of a call the
/// peer withdrew.
///
/// Valid for every request method without knowing either of its payload types:
/// the `Ok` arm is never reached, and `Cancelled` carries nothing, so the
/// bytes depend on neither `{Method}Response` nor `{Method}Error`. Built from
/// the enum rather than written as a literal so it tracks the variant's index.
/// Pair it with [`MESSAGE_TYPE_RESPONSE`].
pub fn encode_cancelled_response() -> Vec<u8> {
    Err::<(), CallError<()>>(CallError::Cancelled).encode()
}

/// Encode `Interrupt(Ok(()))`, a subscription's natural (error-free)
/// completion. `Ok(())` always encodes as a single `0` byte, so a clean
/// interrupt is one byte whatever the method's interrupt type is. Pair it
/// with [`MESSAGE_TYPE_INTERRUPT`].
pub fn encode_clean_interrupt() -> Vec<u8> {
    vec![0]
}

impl Encode for ProtocolMessage {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        self.request_id.encode_to(dest);
        self.payload.trait_id.encode_to(dest);
        self.payload.method_id.encode_to(dest);
        self.payload.message_type.encode_to(dest);
        // Payload bytes are inlined; the receiver reads "until end of frame"
        // because each transport frame is one ProtocolMessage. This matches
        // the public versioned enum transport shape (variant payload encoded
        // inline, no length prefix), and constrains us to slice-shaped
        // `Input`s on the decode side.
        dest.write(&self.payload.value);
    }
}

// Callers must hand `Decode` a slice-shaped `Input`; streaming inputs cannot
// decode this envelope because the payload has no length prefix.
impl Decode for ProtocolMessage {
    fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
        let request_id = String::decode(input)?;
        let trait_id = u8::decode(input)
            .map_err(|_| CodecError::from("frame is missing the trait discriminant byte"))?;
        let method_id = u8::decode(input)
            .map_err(|_| CodecError::from("frame is missing the method discriminant byte"))?;
        let message_type = u8::decode(input)
            .map_err(|_| CodecError::from("frame is missing the message-type byte"))?;
        // Unknown (trait, method) pairs are accepted here; routing is deferred
        // to dispatch, which reports frames with no registered handler.
        let remaining = input
            .remaining_len()?
            .ok_or_else(|| CodecError::from("frame input must report remaining length"))?;
        let mut value = vec![0u8; remaining];
        input.read(&mut value)?;
        if (trait_id, method_id) == PROTOCOL_ERROR_KEY {
            decode_protocol_error_payload(&value)?;
        }
        Ok(ProtocolMessage {
            request_id,
            payload: Payload {
                trait_id,
                method_id,
                message_type,
                value,
            },
        })
    }
}

/// Tagged payload. The `(trait_id, method_id)` pair is the wire discriminant
/// from [`crate::generated::wire_table::WIRE_TABLE`], identifying the frame's
/// trait and method; `message_type` (see the `MESSAGE_TYPE_*` constants)
/// names which leg of that method's exchange this frame carries.
///
/// Note: `Payload` does not derive `Encode`/`Decode` directly; the wire
/// representation lives on [`ProtocolMessage`]. `Payload` is kept as a plain
/// data type for in-memory dispatch (key on the pair, value bytes already
/// SCALE-encoded by the call site).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    /// Trait discriminant: first byte of the wire pair.
    pub trait_id: u8,
    /// Method discriminant within the trait: second byte of the wire pair.
    pub method_id: u8,
    /// Which leg of the method's exchange this frame carries. See the
    /// `MESSAGE_TYPE_*` constants.
    pub message_type: u8,
    /// SCALE-encoded inner value bytes: that leg's own versioned wrapper.
    pub value: Vec<u8>,
}

/// Wire discriminants for a request method, by name. Walks the generated
/// [`WIRE_TABLE`]; intended for tests and embedders that route by method
/// string rather than holding the generated const.
pub fn request_ids(method: &str) -> Option<MethodIds> {
    WIRE_TABLE.iter().find_map(|entry| {
        match (&entry.kind, entry.method == method) {
            (WireKind::Request(ids), true) => Some(*ids),
            _ => None,
        }
    })
}

/// Wire discriminants for a subscription method, by name. Walks the
/// generated [`WIRE_TABLE`].
pub fn subscription_ids(method: &str) -> Option<MethodIds> {
    WIRE_TABLE.iter().find_map(|entry| {
        match (&entry.kind, entry.method == method) {
            (WireKind::Subscription(ids), true) => Some(*ids),
            _ => None,
        }
    })
}

/// Unique ID generator with a prefix.
pub struct IdFactory {
    prefix: String,
    counter: u64,
}

impl IdFactory {
    /// Build a factory that mints IDs of the form `{prefix}{counter}`.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            counter: 0,
        }
    }

    /// Return the next ID, monotonically increasing from 1.
    pub fn next_id(&mut self) -> String {
        self.counter += 1;
        format!("{}{}", self.prefix, self.counter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(trait_id: u8, method_id: u8, message_type: u8, value: Vec<u8>) -> ProtocolMessage {
        ProtocolMessage {
            request_id: "p:1".to_string(),
            payload: Payload {
                trait_id,
                method_id,
                message_type,
                value,
            },
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
        assert_eq!(
            msg.encode(),
            expected_wire(1, 0, MESSAGE_TYPE_REQUEST, &inner)
        );
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
        assert_eq!(
            msg.encode(),
            expected_wire(2, 4, MESSAGE_TYPE_REQUEST, &inner)
        );
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
            vec![],                  // no payload at all
            vec![0],                 // version present, variant truncated
            vec![0, 0],              // no address at all
            vec![0, 0, 250],         // trait present, method truncated
            vec![0, 0, 250, 251, 0], // one trailing byte past a full pair
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

        let sub =
            subscription_ids("account_connection_status_subscribe").expect("known subscription");
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
        assert!(
            format!("{err}").contains("trait discriminant"),
            "unexpected error: {err}"
        );
        // RequestId plus the trait byte, no method byte.
        let mut missing_method = only_request_id.clone();
        missing_method.push(0);
        let err = ProtocolMessage::decode(&mut &missing_method[..])
            .expect_err("missing method byte must error");
        assert!(
            format!("{err}").contains("method discriminant"),
            "unexpected error: {err}"
        );
        // RequestId plus trait and method bytes, no message-type byte.
        let mut missing_message_type = missing_method.clone();
        missing_message_type.push(0);
        let err = ProtocolMessage::decode(&mut &missing_message_type[..])
            .expect_err("missing message-type byte must error");
        assert!(
            format!("{err}").contains("message-type"),
            "unexpected error: {err}"
        );
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
                    CallError::HostFailure {
                        reason: "boom".to_string()
                    },
                    version
                ),
                CallError::HostFailure {
                    reason: "boom".to_string()
                }
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
}
