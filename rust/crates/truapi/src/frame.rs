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
use scale_info::TypeInfo;
use truapi::{
	CallError,
	versioned::{FromLatest, IntoLatest, Versioned},
};

use crate::{
	generated::wire_table::{MethodIds, WIRE_TABLE, WireKind},
	protocol_error::decode_protocol_error_payload,
};

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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum VersionedProtocolError {
	/// Initial protocol error shape.
	#[codec(index = 0)]
	V1(ProtocolErrorV1),
}

/// Protocol errors supported by codec version 1.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
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
	/// The host knows the method but not the version of the request, and
	/// rejected it without running it.
	#[codec(index = 1)]
	UnsupportedVersion {
		/// Trait discriminant of the rejected request.
		trait_id: u8,
		/// Method discriminant of the rejected request.
		method_id: u8,
		/// The newest version the host accepts for this method: `2` means `V2`.
		newest_version: u8,
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
	error.map_domain(|domain| E::from_latest(domain.into_latest(), version))
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

/// Encodes the answer to a payload that did not decode, as
/// `Err(CallError::MalformedFrame { reason })`.
///
/// The bytes do not depend on the method's types, so they work as any
/// request's response and any subscription's interrupt.
pub fn encode_malformed_frame_error(reason: String) -> Vec<u8> {
	Err::<(), CallError<()>>(CallError::MalformedFrame { reason }).encode()
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
			payload: Payload { trait_id, method_id, message_type, value },
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
	WIRE_TABLE.iter().find_map(|entry| match (&entry.kind, entry.method == method) {
		(WireKind::Request(ids), true) => Some(*ids),
		_ => None,
	})
}

/// Wire discriminants for a subscription method, by name. Walks the
/// generated [`WIRE_TABLE`].
pub fn subscription_ids(method: &str) -> Option<MethodIds> {
	WIRE_TABLE.iter().find_map(|entry| match (&entry.kind, entry.method == method) {
		(WireKind::Subscription(ids), true) => Some(*ids),
		_ => None,
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
		Self { prefix: prefix.into(), counter: 0 }
	}

	/// Return the next ID, monotonically increasing from 1.
	pub fn next_id(&mut self) -> String {
		self.counter += 1;
		format!("{}{}", self.prefix, self.counter)
	}
}

#[cfg(test)]
mod tests;
