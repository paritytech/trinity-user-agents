//! Host methods a worker calls, and what a call answers.

use core::fmt::Debug;

use parity_scale_codec::{Decode, Encode};
use truapi::CallError;
use truapi::latest::LatestOf;
use truapi::versioned::{self, FromLatest, IntoLatest};

use crate::wire;

/// A host method a worker can call: its wire address and the versioned
/// envelopes of its three legs. The worker hands and receives the latest
/// payloads; the library wraps and unwraps them.
pub trait Call {
    /// Wire discriminants.
    const IDS: (u8, u8);
    /// The method as logs name it, spelled as the TypeScript client does.
    const NAME: &'static str;
    /// Request envelope.
    type Request: FromLatest + Encode;
    /// Success envelope.
    type Response: IntoLatest + Decode;
    /// Method-specific failure envelope.
    type Error: IntoLatest + Decode + Debug;
}

/// What a call answers: the latest success payload, or why there is none.
pub type Reply<C> =
    Result<LatestOf<<C as Call>::Response>, CallFailure<LatestOf<<C as Call>::Error>>>;

/// Why a call produced no result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallFailure<E> {
    /// The host answered with a failure.
    Host(CallError<E>),
    /// The answer did not decode as this method's result.
    Undecodable(String),
}

/// `chat.createRoom`.
pub enum ChatCreateRoom {}

impl Call for ChatCreateRoom {
    const IDS: (u8, u8) = wire::CHAT_CREATE_ROOM;
    const NAME: &'static str = "chat.createRoom";
    type Request = versioned::chat::HostChatCreateRoomRequest;
    type Response = versioned::chat::HostChatCreateRoomResponse;
    type Error = versioned::chat::HostChatCreateRoomError;
}

/// `chat.registerBot`.
pub enum ChatRegisterBot {}

impl Call for ChatRegisterBot {
    const IDS: (u8, u8) = wire::CHAT_REGISTER_BOT;
    const NAME: &'static str = "chat.registerBot";
    type Request = versioned::chat::HostChatRegisterBotRequest;
    type Response = versioned::chat::HostChatRegisterBotResponse;
    type Error = versioned::chat::HostChatRegisterBotError;
}

/// `chat.postMessage`.
pub enum ChatPostMessage {}

impl Call for ChatPostMessage {
    const IDS: (u8, u8) = wire::CHAT_POST_MESSAGE;
    const NAME: &'static str = "chat.postMessage";
    type Request = versioned::chat::HostChatPostMessageRequest;
    type Response = versioned::chat::HostChatPostMessageResponse;
    type Error = versioned::chat::HostChatPostMessageError;
}

/// `system.handshake`. The library sends it on start; a worker never does.
pub enum SystemHandshake {}

impl Call for SystemHandshake {
    const IDS: (u8, u8) = wire::SYSTEM_HANDSHAKE;
    const NAME: &'static str = "system.handshake";
    type Request = versioned::system::HostHandshakeRequest;
    type Response = versioned::system::HostHandshakeResponse;
    type Error = versioned::system::HostHandshakeError;
}

/// The same failure with its method-specific part at the latest version.
pub fn domain_into_latest<E: IntoLatest>(error: CallError<E>) -> CallError<E::Latest> {
    match error {
        CallError::Domain(domain) => CallError::Domain(domain.into_latest()),
        CallError::Denied => CallError::Denied,
        CallError::Unsupported => CallError::Unsupported,
        CallError::MalformedFrame { reason } => CallError::MalformedFrame { reason },
        CallError::HostFailure { reason } => CallError::HostFailure { reason },
        CallError::Cancelled => CallError::Cancelled,
    }
}

/// The same failure with its method-specific part wrapped for the wire.
pub fn domain_from_latest<E: FromLatest>(error: CallError<E::Latest>) -> CallError<E> {
    match error {
        CallError::Domain(domain) => CallError::Domain(E::from_latest(domain, E::LATEST)),
        CallError::Denied => CallError::Denied,
        CallError::Unsupported => CallError::Unsupported,
        CallError::MalformedFrame { reason } => CallError::MalformedFrame { reason },
        CallError::HostFailure { reason } => CallError::HostFailure { reason },
        CallError::Cancelled => CallError::Cancelled,
    }
}
