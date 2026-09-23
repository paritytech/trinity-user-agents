//! Typed SSO requests and their response variants.
//!
//! `#[sso_service]` generates each request's wire conversions from its handler
//! signature. Responses share the [`Response`] envelope; the request identifies
//! the response variant even when different operations have identical payload types.

use core::fmt::Display;

use truapi::{latest::HostAccountSignVrfError, versioned::account::HostProductDeviceChatError};

use super::messages::{
    PaymentTopUpError, RemoteMessage, RemoteMessageData, Response, RingVrfError, v1,
};

/// A request payload and the wire response selected by its handler declaration.
pub trait SsoRequest: Sized {
    /// Method name used for tracing.
    const NAME: &'static str;
    /// The handler's result payload, without correlation metadata.
    type Response;
    /// Wrap into the request variant.
    fn into_message(self) -> v1::RemoteMessage;
    /// Wrap an envelope into this request's response variant.
    fn response_into_message(response: Response<Self::Response>) -> v1::RemoteMessage;
    /// Unwrap this request's response variant; `None` for any other message.
    fn response_from_message(message: v1::RemoteMessage) -> Option<Response<Self::Response>>;
}

/// Failure payload that can express "no signing session".
pub trait SsoError: Display {
    /// The signing host has no active session to serve the request with.
    fn not_connected() -> Self;
}

impl SsoError for String {
    fn not_connected() -> Self {
        "signing host session is not active".to_string()
    }
}

impl SsoError for RingVrfError {
    fn not_connected() -> Self {
        Self::Unknown {
            reason: String::not_connected(),
        }
    }
}

impl SsoError for HostAccountSignVrfError {
    fn not_connected() -> Self {
        Self::NotConnected
    }
}

impl SsoError for HostProductDeviceChatError {
    fn not_connected() -> Self {
        Self::V1(truapi::latest::HostProductDeviceChatError::NotConnected)
    }
}

impl SsoError for PaymentTopUpError {
    fn not_connected() -> Self {
        Self(truapi::v01::HostPaymentTopUpError::Unknown {
            reason: "Wallet session is not active".to_string(),
        })
    }
}

/// Outcome code and reason recorded in the SSO transcript for one response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseOutcome {
    /// Stable outcome code such as `ok`, `error`, or `publish_failed`.
    pub outcome: &'static str,
    /// Single-line failure description, when any.
    pub reason: Option<String>,
}

impl ResponseOutcome {
    /// Classify a plain payload: `ok`, or `error` with the failure's reason.
    pub fn from_payload<T, E: Display>(payload: &Result<T, E>) -> Self {
        match payload {
            Ok(_) => Self {
                outcome: "ok",
                reason: None,
            },
            Err(err) => Self {
                outcome: "error",
                reason: Some(err.to_string()),
            },
        }
    }
}

impl RemoteMessage {
    /// Service method name for requests; variant name for other messages.
    pub(crate) fn name(&self) -> &'static str {
        let RemoteMessageData::V1(message) = &self.data;
        message.name()
    }

    /// Outgoing request carrying `request` under `message_id`.
    pub fn request<R: SsoRequest>(message_id: String, request: R) -> Self {
        Self {
            message_id,
            data: RemoteMessageData::V1(request.into_message()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_logic::sso::messages::{
        CreateTransactionRequest, CreateTransactionWithLegacyAccountRequest,
        SignRawWithLegacyAccountRequest,
    };

    #[test]
    fn identical_payloads_keep_their_request_specific_response_variants() {
        let response = Response {
            responding_to: "m-1".to_string(),
            payload: Ok(vec![7]),
        };
        let transaction = CreateTransactionRequest::response_into_message(response.clone());
        assert_eq!(transaction.name(), "CreateTransactionResponse");
        assert_eq!(
            CreateTransactionWithLegacyAccountRequest::response_from_message(transaction.clone()),
            Some(response.clone()),
        );
        assert!(SignRawWithLegacyAccountRequest::response_from_message(transaction).is_none());

        let signature = SignRawWithLegacyAccountRequest::response_into_message(response);
        assert_eq!(signature.name(), "SignRawWithLegacyAccountResponse");
        assert!(CreateTransactionRequest::response_from_message(signature).is_none());
    }
}
