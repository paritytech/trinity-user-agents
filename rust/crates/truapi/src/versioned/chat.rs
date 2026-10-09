//! Versioned wrappers for [`Chat`](crate::api::Chat) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostChatCreateRoomRequest { V1 => v01::HostChatCreateRoomRequest }
    pub enum HostChatCreateRoomResponse { V1 => v01::HostChatCreateRoomResponse }
    pub enum HostChatCreateRoomError { V1 => v01::HostChatCreateRoomError }
    pub enum HostChatRegisterBotRequest { V1 => v01::HostChatRegisterBotRequest }
    pub enum HostChatRegisterBotResponse { V1 => v01::HostChatRegisterBotResponse }
    pub enum HostChatRegisterBotError { V1 => v01::HostChatRegisterBotError }
    pub enum HostChatPostMessageRequest { V1 => v01::HostChatPostMessageRequest }
    pub enum HostChatPostMessageResponse { V1 => v01::HostChatPostMessageResponse }
    pub enum HostChatPostMessageError { V1 => v01::HostChatPostMessageError }
    pub enum HostChatListSubscribeRequest { V1 }
    pub enum HostChatListSubscribeItem { V1 => v01::HostChatListSubscribeItem }
    pub enum HostChatListSubscribeError { V1 => v01::GenericError }
    pub enum HostChatActionSubscribeRequest { V1 }
    pub enum HostChatActionSubscribeItem { V1 => v01::HostChatActionSubscribeItem }
    pub enum HostChatActionSubscribeError { V1 => v01::GenericError }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use parity_scale_codec::{Decode, Encode};

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
