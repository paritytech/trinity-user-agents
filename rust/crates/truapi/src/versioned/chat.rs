//! Versioned wrappers for [`Chat`](crate::api::Chat) methods.
//!
//! v0.2 lets a product describe a posted message in one line with `alt`. A
//! v0.1 caller had no such text, so upgrading its request fills `alt` with
//! `None` and the host keeps its own placeholder.

use crate::versioned::{FromLatest, IntoLatest};
use crate::{v01, v02};

truapi_macros::versioned_type! {
    pub enum HostChatCreateRoomRequest { V1 => v01::HostChatCreateRoomRequest }
    pub enum HostChatCreateRoomResponse { V1 => v01::HostChatCreateRoomResponse }
    pub enum HostChatCreateRoomError { V1 => v01::HostChatCreateRoomError }
    pub enum HostChatRegisterBotRequest { V1 => v01::HostChatRegisterBotRequest }
    pub enum HostChatRegisterBotResponse { V1 => v01::HostChatRegisterBotResponse }
    pub enum HostChatRegisterBotError { V1 => v01::HostChatRegisterBotError }
    pub enum HostChatPostMessageRequest {
        V1 => v01::HostChatPostMessageRequest,
        V2 => v02::HostChatPostMessageRequest,
    }
    pub enum HostChatPostMessageResponse {
        V1 => v01::HostChatPostMessageResponse,
        V2 => v01::HostChatPostMessageResponse,
    }
    pub enum HostChatPostMessageError {
        V1 => v01::HostChatPostMessageError,
        V2 => v01::HostChatPostMessageError,
    }
    pub enum HostChatListSubscribeRequest { V1 }
    pub enum HostChatListSubscribeItem { V1 => v01::HostChatListSubscribeItem }
    pub enum HostChatListSubscribeError { V1 => v01::GenericError }
    pub enum HostChatActionSubscribeRequest { V1 }
    pub enum HostChatActionSubscribeItem { V1 => v01::HostChatActionSubscribeItem }
    pub enum HostChatActionSubscribeError { V1 => v01::GenericError }
    pub enum HostChatSetRoomFooterRequest { V1 => v01::HostChatSetRoomFooterRequest }
    pub enum HostChatSetRoomFooterResponse { V1 }
    pub enum HostChatSetRoomFooterError { V1 => v01::HostChatSetRoomFooterError }
}

impl IntoLatest for HostChatPostMessageRequest {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(v01::HostChatPostMessageRequest { room_id, payload }) => {
                v02::HostChatPostMessageRequest {
                    room_id,
                    payload,
                    alt: None,
                }
            }
            Self::V2(latest) => latest,
        }
    }
}

// The response and error keep their v0.1 shape. They still gain a V2 variant,
// because a method's version is uniform across its request, response and error.

impl IntoLatest for HostChatPostMessageResponse {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(payload) | Self::V2(payload) => payload,
        }
    }
}

impl FromLatest for HostChatPostMessageResponse {
    fn from_latest(latest: Self::Latest, target: u8) -> Self {
        if target >= 2 {
            Self::V2(latest)
        } else {
            Self::V1(latest)
        }
    }
}

impl IntoLatest for HostChatPostMessageError {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(payload) | Self::V2(payload) => payload,
        }
    }
}

impl FromLatest for HostChatPostMessageError {
    fn from_latest(latest: Self::Latest, target: u8) -> Self {
        if target >= 2 {
            Self::V2(latest)
        } else {
            Self::V1(latest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versioned::Versioned;
    use parity_scale_codec::{Decode, Encode};

    #[test]
    fn a_v01_post_upgrades_without_alt() {
        let payload = v01::ChatMessageContent::Text {
            text: "hi".to_string(),
        };
        let upgraded = HostChatPostMessageRequest::V1(v01::HostChatPostMessageRequest {
            room_id: "room".to_string(),
            payload: payload.clone(),
        })
        .into_latest();

        assert_eq!(
            upgraded,
            v02::HostChatPostMessageRequest {
                room_id: "room".to_string(),
                payload,
                alt: None,
            }
        );
    }

    #[test]
    fn a_post_answers_in_the_callers_version() {
        let response = v01::HostChatPostMessageResponse {
            message_id: "m".to_string(),
        };

        assert_eq!(
            HostChatPostMessageResponse::from_latest(response.clone(), 1).version(),
            1
        );
        assert_eq!(
            HostChatPostMessageResponse::from_latest(response, 2).version(),
            2
        );
    }

    // Fixture is `ChatRegisterBotV1_request` output from triangle-js-sdks
    // `packages/host-api`, under an `Enum({ v1: … })` envelope. Requests and
    // success responses match that host; error frames do not, because
    // `CallError` adds a tag and inner version (`TODO(shared-core-wire)`).
    #[test]
    fn register_bot_request_matches_reference_host_wire() {
        let request = HostChatRegisterBotRequest::V1(v01::HostChatRegisterBotRequest {
            bot_id: "flipper".into(),
            name: "Flipper".into(),
            icon: String::new(),
        });

        assert_eq!(
            hex::encode(request.encode()),
            "001c666c69707065721c466c697070657200"
        );

        let decoded = HostChatRegisterBotRequest::decode(&mut request.encode().as_slice()).unwrap();
        assert_eq!(decoded, request);
    }

    // Discriminant order matches the reference host's `Status('New','Exists')`.
    // The dispatcher adds the `Result` byte, so this pins the payload only.
    #[test]
    fn register_bot_response_status_matches_reference_host_wire() {
        let new = HostChatRegisterBotResponse::V1(v01::HostChatRegisterBotResponse {
            status: v01::ChatBotRegistrationStatus::New,
        });
        let exists = HostChatRegisterBotResponse::V1(v01::HostChatRegisterBotResponse {
            status: v01::ChatBotRegistrationStatus::Exists,
        });

        assert_eq!(hex::encode(new.encode()), "0000");
        assert_eq!(hex::encode(exists.encode()), "0001");

        assert_eq!(
            HostChatRegisterBotResponse::decode(&mut new.encode().as_slice()).unwrap(),
            new
        );
    }
}
