#![no_std]

//! Transport-neutral TrUAPI client codecs and generated method catalog.
//!
//! Frames contain a SCALE request id, a `(trait, method)` address, a message
//! type byte, and the leg's inline SCALE payload. This crate owns no transport.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};
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
    /// Items followed by a normal or failed interruption.
    Subscription,
}

/// The address shared by every leg of one method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodIds {
    /// Trait discriminant.
    pub trait_id: u8,
    /// Method discriminant within the trait.
    pub method_id: u8,
}

/// Which leg of an exchange a frame carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Request, or subscription start.
    Request = 0,
    /// Response, or subscription item.
    Response = 1,
    /// Subscription completion carrying `Result<(), CallError<E>>`.
    Interrupt = 2,
    /// Subscription cancellation with no payload.
    Stop = 3,
}

/// Wire ids and interaction shape for one method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodWire {
    /// Request/response method.
    Request(MethodIds),
    /// Subscription method.
    Subscription(MethodIds),
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
    /// Rust type of the request envelope.
    pub request_type: &'static str,
    /// Rust type of the success or stream-item envelope.
    pub response_type: &'static str,
    /// Rust type of the domain error.
    pub error_type: Option<&'static str>,
    /// Interaction shape.
    pub kind: MethodKind,
    /// Initial-frame direction.
    pub direction: Direction,
    /// Required executable kind, or `None` for every kind.
    pub required_execution: Option<ExecutionKind>,
    /// Canonical address and shape.
    pub wire: MethodWire,
}

/// Generated marker for a product-initiated request method.
pub trait RequestMethod {
    /// Request envelope.
    type Request: Encode;
    /// Success envelope.
    type Response: Decode;
    /// Domain-error envelope.
    type Error: Decode;
    /// Canonical method metadata.
    const DESCRIPTOR: MethodDescriptor;
}

/// Generated marker for a product-initiated subscription.
pub trait SubscriptionMethod {
    /// Start request, or unit for an empty payload.
    type Request: Encode;
    /// Stream item.
    type Item: Decode;
    /// Domain error carried by the interrupt leg.
    type Error: Decode;
    /// Canonical method metadata.
    const DESCRIPTOR: MethodDescriptor;
}

/// Generated marker for a host-initiated subscription served by a product.
pub trait HostSubscriptionMethod {
    /// Host request.
    type Request: Decode;
    /// Product stream item.
    type Item: Encode;
    /// Domain error carried by the interrupt leg.
    type Error: Encode;
    /// Canonical method metadata.
    const DESCRIPTOR: MethodDescriptor;
}

/// Decoded value paired with its transport request id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded<T> {
    /// Request id from the envelope.
    pub request_id: String,
    /// Typed payload.
    pub value: T,
}

/// Typed outcome carried by a request response.
pub type RequestOutcome<M> =
    Result<<M as RequestMethod>::Response, CallError<<M as RequestMethod>::Error>>;
/// Result of decoding a request response.
pub type DecodedResponse<M> = Result<Decoded<RequestOutcome<M>>, DecodeError>;
/// Typed completion carried by a subscription interruption.
pub type SubscriptionOutcome<M> = Result<(), CallError<<M as SubscriptionMethod>::Error>>;

/// Reserved method-independent protocol error address.
pub const PROTOCOL_ERROR_IDS: MethodIds = MethodIds {
    trait_id: 255,
    method_id: 255,
};

/// A recognized protocol failure from a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    /// The peer cannot route this method address.
    UnsupportedMessage(MethodIds),
}

/// Structural or protocol failure while decoding a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Invalid SCALE or a truncated envelope.
    Malformed,
    /// Bytes remain after the expected payload.
    TrailingBytes,
    /// The frame belongs to another method.
    UnexpectedMethod {
        /// Expected method address.
        expected: MethodIds,
        /// Actual method address.
        actual: MethodIds,
    },
    /// The frame belongs to another leg of the exchange.
    UnexpectedMessageType {
        /// Expected leg.
        expected: MessageType,
        /// Actual message-type byte.
        actual: u8,
    },
    /// A descriptor has a shape incompatible with the operation.
    WrongMethodKind,
    /// A correlated protocol failure. An unknown future version or variant is
    /// retained as `None` rather than mistaken for malformed known data.
    Protocol {
        /// Request id of the call that failed.
        request_id: String,
        /// Recognized failure, if supported by this build.
        error: Option<ProtocolError>,
    },
}

/// Encodes a request without an intermediate payload allocation.
pub fn encode_request<M: RequestMethod>(request_id: &str, request: &M::Request) -> Vec<u8> {
    encode_value_frame(
        request_id,
        generated_request_ids(M::DESCRIPTOR),
        MessageType::Request,
        request,
    )
}

/// Decodes a response's `Result<Response, CallError<Error>>` payload.
pub fn decode_response<M: RequestMethod>(frame: &[u8]) -> DecodedResponse<M> {
    decode_value_frame(
        frame,
        generated_request_ids(M::DESCRIPTOR),
        MessageType::Response,
    )
}

/// Encodes a subscription start.
pub fn encode_subscription_start<M: SubscriptionMethod>(
    request_id: &str,
    request: &M::Request,
) -> Vec<u8> {
    encode_value_frame(
        request_id,
        generated_subscription_ids(M::DESCRIPTOR),
        MessageType::Request,
        request,
    )
}

/// Encodes subscription cancellation; rejects request descriptors.
pub fn encode_subscription_stop(
    request_id: &str,
    descriptor: MethodDescriptor,
) -> Result<Vec<u8>, DecodeError> {
    Ok(encode_value_frame(
        request_id,
        subscription_ids(descriptor)?,
        MessageType::Stop,
        &(),
    ))
}

/// Decodes a subscription item.
pub fn decode_subscription_item<M: SubscriptionMethod>(
    frame: &[u8],
) -> Result<Decoded<M::Item>, DecodeError> {
    decode_value_frame(
        frame,
        generated_subscription_ids(M::DESCRIPTOR),
        MessageType::Response,
    )
}

/// Decodes normal completion (`Ok(())`) or a typed subscription failure.
pub fn decode_subscription_interrupt<M: SubscriptionMethod>(
    frame: &[u8],
) -> Result<Decoded<SubscriptionOutcome<M>>, DecodeError> {
    decode_value_frame(
        frame,
        generated_subscription_ids(M::DESCRIPTOR),
        MessageType::Interrupt,
    )
}

/// Whether a frame interrupts this subscription; rejects request descriptors.
pub fn is_subscription_interrupt(
    frame: &[u8],
    descriptor: MethodDescriptor,
) -> Result<bool, DecodeError> {
    let ids = subscription_ids(descriptor)?;
    let frame = decode_frame(frame)?;
    Ok(frame.ids == ids && frame.message_type == MessageType::Interrupt as u8)
}

/// Decodes a host-initiated start for a product worker.
pub fn decode_host_subscription_start<M: HostSubscriptionMethod>(
    frame: &[u8],
) -> Result<Decoded<M::Request>, DecodeError> {
    decode_value_frame(
        frame,
        generated_subscription_ids(M::DESCRIPTOR),
        MessageType::Request,
    )
}

/// Encodes a product-served subscription item.
pub fn encode_host_subscription_item<M: HostSubscriptionMethod>(
    request_id: &str,
    item: &M::Item,
) -> Vec<u8> {
    encode_value_frame(
        request_id,
        generated_subscription_ids(M::DESCRIPTOR),
        MessageType::Response,
        item,
    )
}

/// Encodes normal or failed termination of a product-served subscription.
pub fn encode_host_subscription_interrupt<M: HostSubscriptionMethod>(
    request_id: &str,
    outcome: &Result<(), CallError<M::Error>>,
) -> Vec<u8> {
    encode_value_frame(
        request_id,
        generated_subscription_ids(M::DESCRIPTOR),
        MessageType::Interrupt,
        outcome,
    )
}

/// Whether a host frame cancels this product-served subscription.
pub fn is_host_subscription_stop<M: HostSubscriptionMethod>(
    frame: &[u8],
) -> Result<bool, DecodeError> {
    let frame = decode_frame(frame)?;
    let matches = frame.ids == generated_subscription_ids(M::DESCRIPTOR)
        && frame.message_type == MessageType::Stop as u8;
    if matches {
        decode_exact::<()>(frame.payload)?;
    }
    Ok(matches)
}

fn generated_request_ids(descriptor: MethodDescriptor) -> MethodIds {
    match descriptor.wire {
        MethodWire::Request(ids) => ids,
        MethodWire::Subscription(_) => {
            panic!("generated request descriptor must use request wire ids")
        }
    }
}

fn subscription_ids(descriptor: MethodDescriptor) -> Result<MethodIds, DecodeError> {
    match descriptor.wire {
        MethodWire::Subscription(ids) => Ok(ids),
        MethodWire::Request(_) => Err(DecodeError::WrongMethodKind),
    }
}

fn generated_subscription_ids(descriptor: MethodDescriptor) -> MethodIds {
    subscription_ids(descriptor)
        .expect("generated subscription descriptor must use subscription wire ids")
}

fn encode_value_frame<T: Encode + ?Sized>(
    request_id: &str,
    ids: MethodIds,
    message_type: MessageType,
    value: &T,
) -> Vec<u8> {
    let mut frame = Vec::new();
    request_id.encode_to(&mut frame);
    frame.extend_from_slice(&[ids.trait_id, ids.method_id, message_type as u8]);
    value.encode_to(&mut frame);
    frame
}

struct BorrowedFrame<'a> {
    request_id: String,
    ids: MethodIds,
    message_type: u8,
    payload: &'a [u8],
}

fn decode_frame(mut frame: &[u8]) -> Result<BorrowedFrame<'_>, DecodeError> {
    let request_id = String::decode(&mut frame).map_err(|_| DecodeError::Malformed)?;
    let [trait_id, method_id, message_type, payload @ ..] = frame else {
        return Err(DecodeError::Malformed);
    };
    let ids = MethodIds {
        trait_id: *trait_id,
        method_id: *method_id,
    };
    if ids == PROTOCOL_ERROR_IDS {
        if *message_type != MessageType::Response as u8 {
            return Err(DecodeError::UnexpectedMessageType {
                expected: MessageType::Response,
                actual: *message_type,
            });
        }
        let error = match (payload.first(), payload.get(1)) {
            (None, _) => return Err(DecodeError::Malformed),
            (Some(version), _) if *version != 0 => None,
            (Some(_), Some(variant)) if *variant != 0 => None,
            _ => {
                let (version, variant, trait_id, method_id): (u8, u8, u8, u8) =
                    decode_exact(payload)?;
                debug_assert_eq!((version, variant), (0, 0));
                Some(ProtocolError::UnsupportedMessage(MethodIds {
                    trait_id,
                    method_id,
                }))
            }
        };
        return Err(DecodeError::Protocol { request_id, error });
    }
    Ok(BorrowedFrame {
        request_id,
        ids,
        message_type: *message_type,
        payload,
    })
}

fn decode_value_frame<T: Decode>(
    frame: &[u8],
    ids: MethodIds,
    message_type: MessageType,
) -> Result<Decoded<T>, DecodeError> {
    let frame = decode_frame(frame)?;
    if frame.ids != ids {
        return Err(DecodeError::UnexpectedMethod {
            expected: ids,
            actual: frame.ids,
        });
    }
    if frame.message_type != message_type as u8 {
        return Err(DecodeError::UnexpectedMessageType {
            expected: message_type,
            actual: frame.message_type,
        });
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use truapi::v01;
    use truapi::versioned::system::{HostHandshakeRequest, HostHandshakeResponse};

    #[test]
    fn request_and_response_use_canonical_envelopes() {
        let request = HostHandshakeRequest::V1(v01::HostHandshakeRequest { codec_version: 2 });
        assert_eq!(
            encode_request::<SystemHandshake>("p:1", &request),
            [12, b'p', b':', b'1', 1, 0, 0, 0, 2]
        );
        let decoded = decode_response::<SystemHandshake>(&[12, b'p', b':', b'1', 1, 0, 1, 0, 0])
            .expect("response frame");
        assert_eq!(decoded.request_id, "p:1");
        assert_eq!(decoded.value, Ok(HostHandshakeResponse::V1));
    }

    #[test]
    fn response_domain_error_preserves_versioned_payload() {
        let error = CallError::Domain(truapi::versioned::system::HostHandshakeError::V1(
            v01::HostHandshakeError::UnsupportedProtocolVersion,
        ));
        let mut frame = vec![12, b'p', b':', b'1', 1, 0, 1, 1];
        error.encode_to(&mut frame);
        assert_eq!(
            decode_response::<SystemHandshake>(&frame)
                .expect("error frame")
                .value,
            Err(error)
        );
    }

    #[test]
    fn worker_serves_unified_renderer() {
        use truapi::versioned::renderer::{
            ProductRendererRenderItem, ProductRendererRenderRequest,
        };
        let request = ProductRendererRenderRequest::V1(v01::ProductRendererRenderRequest {
            context: v01::RenderContext::PocketCard {
                card_id: "loyalty".into(),
            },
            payload: vec![1, 2, 3],
        });
        let ids = generated_subscription_ids(RendererRender::DESCRIPTOR);
        let start = encode_value_frame("host:4", ids, MessageType::Request, &request);
        let decoded =
            decode_host_subscription_start::<RendererRender>(&start).expect("start frame");
        assert_eq!(decoded.request_id, "host:4");
        assert_eq!(decoded.value, request);
        let item = ProductRendererRenderItem::V1(v01::RendererNode::Nil);
        let rendered = encode_host_subscription_item::<RendererRender>("host:4", &item);
        assert_eq!(
            decode_value_frame::<ProductRendererRenderItem>(&rendered, ids, MessageType::Response)
                .expect("item frame")
                .value,
            item
        );
        let stop = encode_value_frame("host:4", ids, MessageType::Stop, &());
        assert_eq!(is_host_subscription_stop::<RendererRender>(&stop), Ok(true));
        let interrupt = encode_host_subscription_interrupt::<RendererRender>("host:4", &Ok(()));
        assert_eq!(
            decode_value_frame::<Result<(), CallError<v01::GenericError>>>(
                &interrupt,
                ids,
                MessageType::Interrupt
            )
            .expect("interrupt")
            .value,
            Ok(())
        );
    }

    #[test]
    fn subscription_completion_and_failure_are_typed_and_strict() {
        type Method = AccountConnectionStatusSubscribe;
        let ids = generated_subscription_ids(Method::DESCRIPTOR);
        let complete = encode_value_frame(
            "p:1",
            ids,
            MessageType::Interrupt,
            &Ok::<(), CallError<v01::GenericError>>(()),
        );
        assert_eq!(
            decode_subscription_interrupt::<Method>(&complete)
                .expect("completion")
                .value,
            Ok(())
        );
        let failed = encode_value_frame(
            "p:1",
            ids,
            MessageType::Interrupt,
            &Err::<(), _>(CallError::<v01::GenericError>::Denied),
        );
        assert_eq!(
            decode_subscription_interrupt::<Method>(&failed)
                .expect("failure")
                .value,
            Err(CallError::Denied)
        );
        let mut trailing = complete;
        trailing.push(0);
        assert_eq!(
            decode_subscription_interrupt::<Method>(&trailing),
            Err(DecodeError::TrailingBytes)
        );
        let empty = encode_value_frame("p:1", ids, MessageType::Interrupt, &());
        assert_eq!(
            decode_subscription_interrupt::<Method>(&empty),
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn frame_address_and_leg_both_must_match() {
        let ids = generated_request_ids(SystemHandshake::DESCRIPTOR);
        let other = MethodIds {
            trait_id: ids.trait_id + 1,
            method_id: ids.method_id,
        };
        let wrong_method = encode_value_frame(
            "p:1",
            other,
            MessageType::Response,
            &Ok::<_, CallError<()>>(HostHandshakeResponse::V1),
        );
        assert_eq!(
            decode_response::<SystemHandshake>(&wrong_method),
            Err(DecodeError::UnexpectedMethod {
                expected: ids,
                actual: other
            })
        );
        let wrong_leg = encode_value_frame("p:1", ids, MessageType::Request, &());
        assert_eq!(
            decode_response::<SystemHandshake>(&wrong_leg),
            Err(DecodeError::UnexpectedMessageType {
                expected: MessageType::Response,
                actual: 0
            })
        );
        assert_eq!(
            encode_subscription_stop("p:1", SystemHandshake::DESCRIPTOR),
            Err(DecodeError::WrongMethodKind)
        );
    }

    #[test]
    fn protocol_failures_keep_correlation_and_accept_future_variants() {
        let known = [12, b'p', b':', b'1', 255, 255, 1, 0, 0, 4, 7];
        assert_eq!(
            decode_response::<SystemHandshake>(&known),
            Err(DecodeError::Protocol {
                request_id: "p:1".into(),
                error: Some(ProtocolError::UnsupportedMessage(MethodIds {
                    trait_id: 4,
                    method_id: 7
                }))
            })
        );
        let future = [12, b'p', b':', b'1', 255, 255, 1, 0, 1, 42];
        assert_eq!(
            decode_response::<SystemHandshake>(&future),
            Err(DecodeError::Protocol {
                request_id: "p:1".into(),
                error: None
            })
        );
        assert_eq!(
            decode_response::<SystemHandshake>(&known[..known.len() - 1]),
            Err(DecodeError::Malformed)
        );
        let mut trailing = known.to_vec();
        trailing.push(0);
        assert_eq!(
            decode_response::<SystemHandshake>(&trailing),
            Err(DecodeError::TrailingBytes)
        );
    }
}
