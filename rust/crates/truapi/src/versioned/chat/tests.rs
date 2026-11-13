use super::*;
use crate::versioned::Versioned;
use parity_scale_codec::{Decode, Encode};

#[test]
fn a_v01_post_upgrades_without_alt() {
	let payload = v01::ChatMessageContent::Text { text: "hi".to_string() };
	let upgraded = HostChatPostMessageRequest::V1(v01::HostChatPostMessageRequest {
		room_id: "room".to_string(),
		payload: payload.clone(),
	})
	.into_latest();

	assert_eq!(
		upgraded,
		v02::HostChatPostMessageRequest { room_id: "room".to_string(), payload, alt: None }
	);
}

#[test]
fn a_post_answers_in_the_callers_version() {
	let response = v01::HostChatPostMessageResponse { message_id: "m".to_string() };

	assert_eq!(HostChatPostMessageResponse::from_latest(response.clone(), 1).version(), 1);
	assert_eq!(HostChatPostMessageResponse::from_latest(response, 2).version(), 2);
}

#[test]
fn a_v01_footer_upgrades_unchanged() {
	let upgraded = [v01::ChatRoomFooter::TextInput, v01::ChatRoomFooter::Empty].map(|footer| {
		HostChatSetRoomFooterRequest::V1(v01::HostChatSetRoomFooterRequest {
			room_id: "room".to_string(),
			footer,
		})
		.into_latest()
	});

	assert_eq!(
		upgraded,
		[v02::ChatRoomFooter::TextInput, v02::ChatRoomFooter::Empty].map(|footer| {
			v02::HostChatSetRoomFooterRequest { room_id: "room".to_string(), footer }
		})
	);
}

#[test]
fn a_footer_answers_in_the_callers_version() {
	assert_eq!(HostChatSetRoomFooterResponse::from_latest((), 1).version(), 1);
	assert_eq!(HostChatSetRoomFooterResponse::from_latest((), 2).version(), 2);
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

	assert_eq!(hex::encode(request.encode()), "001c666c69707065721c466c697070657200");

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

	assert_eq!(HostChatRegisterBotResponse::decode(&mut new.encode().as_slice()).unwrap(), new);
}
