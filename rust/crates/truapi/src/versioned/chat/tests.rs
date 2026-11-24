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

// Products that use the first SDK set footers and post messages in these
// shapes. They call the client that the host injects, so the V1 envelope of
// these methods keeps its first wire.
#[test]
fn a_footer_and_a_post_keep_the_first_wire() {
	assert_eq!(HostChatSetRoomFooterRequest::LATEST, 2);
	assert_eq!(HostChatPostMessageRequest::LATEST, 2);

	let footer = HostChatSetRoomFooterRequest::V1(v01::HostChatSetRoomFooterRequest {
		room_id: "room".into(),
		footer: v01::ChatRoomFooter::TextInput,
	});
	// V1 envelope, the room id, then `TextInput` at index 0.
	assert_eq!(hex::encode(footer.encode()), "0010726f6f6d00");
}

// A first-SDK product still decodes as a room request. Its own id is only
// the source of the room id, so the host never stores a room under it.
#[test]
fn a_v01_room_id_becomes_the_source_of_the_host_id() {
	let room = HostChatCreateRoomRequest::V1(v01::HostChatCreateRoomRequest {
		room_id: "room".into(),
		name: "Room".into(),
		icon: String::new(),
	});
	// V1 envelope, the room id, the name, then the empty icon.
	assert_eq!(hex::encode(room.encode()), "0010726f6f6d10526f6f6d00");

	assert_eq!(
		room.into_latest(),
		v02::HostChatCreateRoomRequest {
			room_id_source: "room".into(),
			name: "Room".into(),
			icon: String::new(),
			config: Default::default(),
		}
	);
}

// A v0.1 caller has no field for the id, so only a v0.2 caller learns it
// from the answer.
#[test]
fn only_a_v02_room_answer_names_the_room_id() {
	let latest = v02::HostChatCreateRoomResponse {
		status: v02::ChatRoomRegistrationStatus::New,
		room_id: "0x01".into(),
	};

	assert_eq!(
		HostChatCreateRoomResponse::from_latest(latest.clone(), 1),
		HostChatCreateRoomResponse::V1(v01::HostChatCreateRoomResponse {
			status: v01::ChatRoomRegistrationStatus::New,
		})
	);
	assert_eq!(
		HostChatCreateRoomResponse::from_latest(latest.clone(), 2),
		HostChatCreateRoomResponse::V2(latest)
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
