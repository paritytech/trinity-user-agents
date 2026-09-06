#![no_std]

//! Transport-neutral TrUAPI client codecs and generated method catalog.
//!
//! The crate owns no executor or transport. A product runtime sends the encoded
//! frames through its native transport, then decodes returned frames with the
//! same generated method marker.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode, Error as CodecError, Input};
use truapi::CallError;

mod generated;
pub use generated::*;

/// Product executable kinds used by TrUAPI authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionKind {
    /// Full application surface.
    App,
    /// Embedded widget surface.
    Widget,
    /// Background product worker.
    Worker,
}

/// Direction in which a method's initial frame travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Product starts the call and the host serves it.
    ProductToHost,
    /// Host starts the call and the product serves it.
    HostToProduct,
}

/// Wire interaction shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    /// One request followed by one response.
    Request,
    /// Start/receive stream with no typed start failure.
    Subscription,
    /// Start/receive stream with a typed start failure.
    ResultSubscription,
}

/// Request/response discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestFrameIds {
    /// Request frame discriminant.
    pub request_id: u8,
    /// Response frame discriminant.
    pub response_id: u8,
}

/// Subscription discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionFrameIds {
    /// Subscription start discriminant.
    pub start_id: u8,
    /// Subscription stop discriminant.
    pub stop_id: u8,
    /// Server-side interruption discriminant.
    pub interrupt_id: u8,
    /// Subscription item discriminant.
    pub receive_id: u8,
}

/// Wire ids for one method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodWire {
    /// Request/response pair.
    Request(RequestFrameIds),
    /// Subscription quartet.
    Subscription(SubscriptionFrameIds),
}

/// Generated metadata for one canonical method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodDescriptor {
    /// Canonical service trait name.
    pub service: &'static str,
    /// Rust method name.
    pub method: &'static str,
    /// Globally unique wire method name.
    pub wire_name: &'static str,
    /// Rust type of the versioned request envelope.
    pub request_type: &'static str,
    /// Rust type of the versioned success or stream-item envelope.
    pub response_type: &'static str,
    /// Rust type of the versioned domain-error envelope, when the method has one.
    pub error_type: Option<&'static str>,
    /// Interaction shape.
    pub kind: MethodKind,
    /// Initial-frame direction.
    pub direction: Direction,
    /// Required executable kind, or `None` when every kind may call it.
    pub required_execution: Option<ExecutionKind>,
    /// Whether diagnostics must redact this method's payload.
    pub sensitive: bool,
    /// Canonical frame discriminants.
    pub wire: MethodWire,
}

/// Generated marker for a product-initiated request method.
pub trait RequestMethod {
    /// Versioned request envelope.
    type Request: Encode;
    /// Versioned success envelope.
    type Response: Decode;
    /// Versioned domain-error envelope.
    type Error: Decode;
    /// Whether the success value is a versioned wrapper reconstructed from the outer version byte.
    const RESPONSE_VERSIONED: bool;
    /// Canonical method metadata.
    const DESCRIPTOR: MethodDescriptor;
}

/// Generated marker for a product-initiated subscription.
pub trait SubscriptionMethod {
    /// Versioned start request, or unit for an empty payload.
    type Request: Encode;
    /// Versioned stream item.
    type Item: Decode;
    /// Canonical method metadata.
    const DESCRIPTOR: MethodDescriptor;
}

/// Generated marker for a product-initiated subscription with typed start errors.
pub trait ResultSubscriptionMethod {
    /// Versioned start request.
    type Request: Encode;
    /// Versioned stream item.
    type Item: Decode;
    /// Versioned domain-error envelope.
    type Error: Decode;
    /// Canonical method metadata.
    const DESCRIPTOR: MethodDescriptor;
}

/// Generated marker for a host-initiated subscription served by a product.
pub trait HostSubscriptionMethod {
    /// Versioned host request.
    type Request: Decode;
    /// Versioned product stream item.
    type Item: Encode;
    /// Canonical method metadata.
    const DESCRIPTOR: MethodDescriptor;
}

/// Decoded frame value paired with its transport request id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded<T> {
    /// Request id from the frame envelope.
    pub request_id: String,
    /// Decoded typed payload.
    pub value: T,
}

/// Typed domain outcome carried by a product-initiated request response.
pub type RequestOutcome<M> =
    Result<<M as RequestMethod>::Response, CallError<<M as RequestMethod>::Error>>;

/// Result of decoding one product-initiated request response frame.
pub type DecodedResponse<M> = Result<Decoded<RequestOutcome<M>>, DecodeError>;

/// Structural failure while decoding a TrUAPI frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The frame or payload is not valid SCALE for the expected type.
    Malformed,
    /// The payload contains bytes after the expected value.
    TrailingBytes,
    /// The frame carries a discriminant other than the method leg expected.
    UnexpectedDiscriminant {
        /// Expected wire discriminant.
        expected: u8,
        /// Actual wire discriminant.
        actual: u8,
    },
    /// A versioned response contains neither the success nor failure tag.
    UnexpectedResultDiscriminant(u8),
    /// The generated descriptor has a wire shape incompatible with the operation.
    WrongMethodKind,
}

/// Encodes a product-initiated request frame without an intermediate payload allocation.
pub fn encode_request<M: RequestMethod>(request_id: &str, request: &M::Request) -> Vec<u8> {
    let ids = generated_request_ids(M::DESCRIPTOR);
    encode_value_frame(request_id, ids.request_id, request)
}

/// Decodes a product-initiated request's response frame.
pub fn decode_response<M: RequestMethod>(frame: &[u8]) -> DecodedResponse<M> {
    let ids = generated_request_ids(M::DESCRIPTOR);
    let frame = decode_frame(frame)?;
    expect_discriminant(ids.response_id, frame.discriminant)?;
    let payload = frame.payload;
    if payload.len() < 2 {
        return Err(DecodeError::Malformed);
    }
    let version = payload[0];
    let value = match payload[1] {
        0 if M::RESPONSE_VERSIONED => Ok(decode_prefixed_exact(version, &payload[2..])?),
        0 => Ok(decode_exact(&payload[2..])?),
        1 => Err(decode_exact(&payload[2..])?),
        other => return Err(DecodeError::UnexpectedResultDiscriminant(other)),
    };
    Ok(Decoded {
        request_id: frame.request_id,
        value,
    })
}

/// Encodes a product-initiated subscription start frame.
pub fn encode_subscription_start<M: SubscriptionMethod>(
    request_id: &str,
    request: &M::Request,
) -> Vec<u8> {
    let ids = generated_subscription_ids(M::DESCRIPTOR);
    encode_value_frame(request_id, ids.start_id, request)
}

/// Encodes a product-initiated result-subscription start frame.
pub fn encode_result_subscription_start<M: ResultSubscriptionMethod>(
    request_id: &str,
    request: &M::Request,
) -> Vec<u8> {
    let ids = generated_subscription_ids(M::DESCRIPTOR);
    encode_value_frame(request_id, ids.start_id, request)
}

/// Encodes a stop frame for a product-initiated subscription.
///
/// Returns [`DecodeError::WrongMethodKind`] when `descriptor` describes a request.
pub fn encode_subscription_stop(
    request_id: &str,
    descriptor: MethodDescriptor,
) -> Result<Vec<u8>, DecodeError> {
    let ids = subscription_ids(descriptor)?;
    Ok(encode_empty_frame(request_id, ids.stop_id))
}

/// Decodes a regular subscription item frame.
pub fn decode_subscription_item<M: SubscriptionMethod>(
    frame: &[u8],
) -> Result<Decoded<M::Item>, DecodeError> {
    decode_stream_item::<M::Item>(
        frame,
        generated_subscription_ids(M::DESCRIPTOR).receive_id,
    )
}

/// Decodes a result-subscription item frame.
pub fn decode_result_subscription_item<M: ResultSubscriptionMethod>(
    frame: &[u8],
) -> Result<Decoded<M::Item>, DecodeError> {
    decode_stream_item::<M::Item>(
        frame,
        generated_subscription_ids(M::DESCRIPTOR).receive_id,
    )
}

/// Decodes a typed result-subscription interruption.
pub fn decode_result_subscription_interrupt<M: ResultSubscriptionMethod>(
    frame: &[u8],
) -> Result<Decoded<CallError<M::Error>>, DecodeError> {
    let ids = generated_subscription_ids(M::DESCRIPTOR);
    let frame = decode_frame(frame)?;
    expect_discriminant(ids.interrupt_id, frame.discriminant)?;
    let (_, payload) = frame.payload.split_first().ok_or(DecodeError::Malformed)?;
    Ok(Decoded {
        request_id: frame.request_id,
        value: decode_exact(payload)?,
    })
}

/// Returns whether a frame is the interruption leg for a subscription descriptor.
///
/// Returns [`DecodeError::WrongMethodKind`] when `descriptor` describes a request.
pub fn is_subscription_interrupt(
    frame: &[u8],
    descriptor: MethodDescriptor,
) -> Result<bool, DecodeError> {
    let ids = subscription_ids(descriptor)?;
    Ok(decode_frame(frame)?.discriminant == ids.interrupt_id)
}

/// Decodes a host-initiated subscription start for a product worker.
pub fn decode_host_subscription_start<M: HostSubscriptionMethod>(
    frame: &[u8],
) -> Result<Decoded<M::Request>, DecodeError> {
    let ids = generated_subscription_ids(M::DESCRIPTOR);
    let frame = decode_frame(frame)?;
    expect_discriminant(ids.start_id, frame.discriminant)?;
    Ok(Decoded {
        request_id: frame.request_id,
        value: decode_exact(frame.payload)?,
    })
}

/// Encodes one product-served item for a host-initiated subscription.
pub fn encode_host_subscription_item<M: HostSubscriptionMethod>(
    request_id: &str,
    item: &M::Item,
) -> Vec<u8> {
    let ids = generated_subscription_ids(M::DESCRIPTOR);
    encode_value_frame(request_id, ids.receive_id, item)
}

/// Encodes product-side termination of a host-initiated subscription.
pub fn encode_host_subscription_interrupt<M: HostSubscriptionMethod>(request_id: &str) -> Vec<u8> {
    let ids = generated_subscription_ids(M::DESCRIPTOR);
    encode_empty_frame(request_id, ids.interrupt_id)
}

/// Returns whether a host frame stops a product-served subscription.
pub fn is_host_subscription_stop<M: HostSubscriptionMethod>(
    frame: &[u8],
) -> Result<bool, DecodeError> {
    let ids = generated_subscription_ids(M::DESCRIPTOR);
    Ok(decode_frame(frame)?.discriminant == ids.stop_id)
}

fn request_ids(descriptor: MethodDescriptor) -> Result<RequestFrameIds, DecodeError> {
    match descriptor.wire {
        MethodWire::Request(ids) => Ok(ids),
        MethodWire::Subscription(_) => Err(DecodeError::WrongMethodKind),
    }
}

fn subscription_ids(descriptor: MethodDescriptor) -> Result<SubscriptionFrameIds, DecodeError> {
    match descriptor.wire {
        MethodWire::Subscription(ids) => Ok(ids),
        MethodWire::Request(_) => Err(DecodeError::WrongMethodKind),
    }
}

fn generated_request_ids(descriptor: MethodDescriptor) -> RequestFrameIds {
    request_ids(descriptor).expect("generated request descriptor must use request wire ids")
}

fn generated_subscription_ids(descriptor: MethodDescriptor) -> SubscriptionFrameIds {
    subscription_ids(descriptor)
        .expect("generated subscription descriptor must use subscription wire ids")
}

fn encode_value_frame<T: Encode + ?Sized>(
    request_id: &str,
    discriminant: u8,
    value: &T,
) -> Vec<u8> {
    let mut frame = Vec::new();
    request_id.encode_to(&mut frame);
    frame.push(discriminant);
    value.encode_to(&mut frame);
    frame
}

fn encode_empty_frame(request_id: &str, discriminant: u8) -> Vec<u8> {
    let mut frame = Vec::new();
    request_id.encode_to(&mut frame);
    frame.push(discriminant);
    frame
}

struct BorrowedFrame<'a> {
    request_id: String,
    discriminant: u8,
    payload: &'a [u8],
}

fn decode_frame(mut frame: &[u8]) -> Result<BorrowedFrame<'_>, DecodeError> {
    let request_id = String::decode(&mut frame).map_err(|_| DecodeError::Malformed)?;
    let (&discriminant, payload) = frame.split_first().ok_or(DecodeError::Malformed)?;
    Ok(BorrowedFrame {
        request_id,
        discriminant,
        payload,
    })
}

fn expect_discriminant(expected: u8, actual: u8) -> Result<(), DecodeError> {
    if actual == expected {
        Ok(())
    } else {
        Err(DecodeError::UnexpectedDiscriminant { expected, actual })
    }
}

fn decode_stream_item<T: Decode>(frame: &[u8], receive_id: u8) -> Result<Decoded<T>, DecodeError> {
    let frame = decode_frame(frame)?;
    expect_discriminant(receive_id, frame.discriminant)?;
    Ok(Decoded {
        request_id: frame.request_id,
        value: decode_exact(frame.payload)?,
    })
}

fn decode_exact<T: Decode>(mut payload: &[u8]) -> Result<T, DecodeError> {
    let value = T::decode(&mut payload).map_err(|_| DecodeError::Malformed)?;
    if payload.is_empty() {
        Ok(value)
    } else {
        Err(DecodeError::TrailingBytes)
    }
}

fn decode_prefixed_exact<T: Decode>(prefix: u8, payload: &[u8]) -> Result<T, DecodeError> {
    let mut input = PrefixedInput {
        prefix: Some(prefix),
        payload,
    };
    let value = T::decode(&mut input).map_err(|_| DecodeError::Malformed)?;
    match input.remaining_len().map_err(|_| DecodeError::Malformed)? {
        Some(0) => Ok(value),
        _ => Err(DecodeError::TrailingBytes),
    }
}

struct PrefixedInput<'a> {
    prefix: Option<u8>,
    payload: &'a [u8],
}

impl Input for PrefixedInput<'_> {
    fn remaining_len(&mut self) -> Result<Option<usize>, CodecError> {
        Ok(Some(
            usize::from(self.prefix.is_some()) + self.payload.len(),
        ))
    }

    fn read(&mut self, into: &mut [u8]) -> Result<(), CodecError> {
        let mut written = 0;
        if let Some(prefix) = self.prefix.take() {
            let Some(first) = into.first_mut() else {
                self.prefix = Some(prefix);
                return Ok(());
            };
            *first = prefix;
            written = 1;
        }
        let remaining = &mut into[written..];
        if remaining.len() > self.payload.len() {
            return Err("not enough data to fill buffer".into());
        }
        remaining.copy_from_slice(&self.payload[..remaining.len()]);
        self.payload = &self.payload[remaining.len()..];
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use parity_scale_codec::Encode;
    use truapi::v01;
    use truapi::versioned::system::{HostHandshakeRequest, HostHandshakeResponse};

    #[test]
    fn catalogs_partition_worker_only_chat() {
        assert_eq!(APP_METHODS.len(), 66);
        assert_eq!(WIDGET_METHODS.len(), 66);
        assert_eq!(WORKER_METHODS.len(), 72);
        assert_eq!(WORKER_ONLY_METHODS.len(), 6);
        assert!(WORKER_ONLY_METHODS.iter().all(|method| {
            method.service == "Chat" && method.required_execution == Some(ExecutionKind::Worker)
        }));
    }

    #[test]
    fn request_and_response_use_canonical_envelopes() {
        let request = HostHandshakeRequest::V1(v01::HostHandshakeRequest { codec_version: 1 });
        assert_eq!(
            encode_request::<SystemHandshake>("p:1", &request),
            [12, b'p', b':', b'1', 0, 0, 1]
        );

        let decoded = decode_response::<SystemHandshake>(&[12, b'p', b':', b'1', 1, 0, 0])
            .expect("response frame");
        assert_eq!(decoded.request_id, "p:1");
        assert_eq!(decoded.value, Ok(HostHandshakeResponse::V1));
    }

    #[test]
    fn response_domain_error_preserves_versioned_payload() {
        let error = CallError::Domain(truapi::versioned::system::HostHandshakeError::V1(
            v01::HostHandshakeError::UnsupportedProtocolVersion,
        ));
        let mut frame = vec![12, b'p', b':', b'1', 1, 0, 1];
        error.encode_to(&mut frame);
        let decoded = decode_response::<SystemHandshake>(&frame).expect("error frame");
        assert_eq!(decoded.value, Err(error));
    }

    #[test]
    fn worker_serves_host_initiated_custom_chat_rendering() {
        let request = truapi::versioned::chat::ProductChatCustomMessageRenderRequest::V1(
            v01::ProductChatCustomMessageRenderRequest {
                message_id: "message-7".into(),
                message_type: "poll".into(),
                payload: vec![1, 2, 3],
            },
        );
        let ids = generated_subscription_ids(ChatCustomMessageRender::DESCRIPTOR);
        let start = encode_value_frame("host:4", ids.start_id, &request);
        let decoded =
            decode_host_subscription_start::<ChatCustomMessageRender>(&start).expect("start frame");
        assert_eq!(decoded.request_id, "host:4");
        assert_eq!(decoded.value, request);

        let item = truapi::versioned::chat::ProductChatCustomMessageRenderItem::V1(
            v01::CustomRendererNode::Nil,
        );
        let rendered = encode_host_subscription_item::<ChatCustomMessageRender>("host:4", &item);
        let rendered = decode_frame(&rendered).expect("rendered item frame");
        assert_eq!(rendered.discriminant, ids.receive_id);
        assert_eq!(
            decode_exact::<truapi::versioned::chat::ProductChatCustomMessageRenderItem>(
                rendered.payload
            ),
            Ok(item)
        );

        let stop = encode_empty_frame("host:4", ids.stop_id);
        assert_eq!(
            is_host_subscription_stop::<ChatCustomMessageRender>(&stop),
            Ok(true)
        );
        let interrupt = encode_host_subscription_interrupt::<ChatCustomMessageRender>("host:4");
        assert_eq!(
            decode_frame(&interrupt)
                .expect("interrupt frame")
                .discriminant,
            ids.interrupt_id
        );
    }

    #[test]
    fn descriptor_apis_validate_subscription_wire_ids() {
        let descriptor = AccountConnectionStatusSubscribe::DESCRIPTOR;
        let expected_ids = generated_subscription_ids(descriptor);
        let stop = encode_subscription_stop("p:1", descriptor).expect("subscription descriptor");
        assert_eq!(
            decode_frame(&stop).expect("stop frame").discriminant,
            expected_ids.stop_id
        );

        assert_eq!(
            encode_subscription_stop("p:1", SystemHandshake::DESCRIPTOR),
            Err(DecodeError::WrongMethodKind)
        );
        assert_eq!(
            is_subscription_interrupt(&stop, SystemHandshake::DESCRIPTOR),
            Err(DecodeError::WrongMethodKind)
        );
    }
}
