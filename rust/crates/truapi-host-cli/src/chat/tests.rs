use super::*;
use std::fs::read_to_string;
use truapi::{latest::ChatMessageContent, platform::ProductId};

fn product() -> ProductContext {
	ProductContext::new(ProductId::for_test("chat.dot"))
}

fn text(text: &str) -> ChatMessageContent {
	ChatMessageContent::Text { text: text.to_string() }
}

fn room(room_id_source: &str) -> HostChatCreateRoomRequest {
	HostChatCreateRoomRequest {
		room_id_source: room_id_source.to_string(),
		name: "Support".to_string(),
		icon: String::new(),
		config: Default::default(),
	}
}

#[test]
fn a_message_needs_a_room_this_host_created() {
	let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
	let host = CliChatHost::new(Transcript::new(Some(transcript.path().to_path_buf()), "chat"));

	let posted = futures::executor::block_on(host.post_chat_message(
		&product(),
		HostChatPostMessageRequest {
			room_id: "support".to_string(),
			payload: text("hello"),
			alt: None,
		},
	));

	assert!(matches!(posted, Err(HostChatPostMessageError::Unknown { .. })));
	// A refused message is not one this host was willing to store, so it
	// must not appear in the record a battery reads.
	assert_eq!(read_to_string(transcript.path()).expect("the transcript is readable"), "");
}

#[test]
fn a_stored_message_is_recorded_as_the_host_received_it() {
	let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
	let host = CliChatHost::new(Transcript::new(Some(transcript.path().to_path_buf()), "chat"));
	futures::executor::block_on(host.create_chat_room(
		&product(),
		"support".to_string(),
		room("support"),
	))
	.expect("a new room is created");

	let payload = text("line one\nline two");
	let posted = futures::executor::block_on(host.post_chat_message(
		&product(),
		HostChatPostMessageRequest {
			room_id: "support".to_string(),
			payload: payload.clone(),
			alt: Some("Two lines".to_string()),
		},
	))
	.expect("a message posts into a room this host created");

	assert_eq!(posted.message_id, "m1");
	let transcript_text = read_to_string(transcript.path()).expect("the transcript is readable");
	let recorded: serde_json::Value = serde_json::from_str(
		transcript_text
			.lines()
			.next_back()
			.expect("the message is the last line, after the room"),
	)
	.expect("each line is one JSON object");
	assert_eq!(recorded["kind"], "message");
	assert_eq!(recorded["messageId"], "m1");
	assert_eq!(recorded["roomId"], "support");
	assert_eq!(recorded["variant"], "Text");
	// The payload as bytes, so a difference between what a product sent
	// and what the host received cannot hide behind a rendering.
	assert_eq!(recorded["payload"], hex::encode(payload.encode()));
	assert_eq!(recorded["alt"], "Two lines");
}

#[test]
fn a_room_appears_in_the_list_a_subscriber_already_holds() {
	let host = CliChatHost::new(Transcript::new(None, "chat"));
	let mut rooms = host.subscribe_chat_rooms(&product());

	let snapshot = futures::executor::block_on(rooms.next())
		.expect("a subscription emits its snapshot")
		.expect("the snapshot is not an error");
	assert!(snapshot.rooms.is_empty());

	futures::executor::block_on(host.create_chat_room(
		&product(),
		"support".to_string(),
		room("support"),
	))
	.expect("a new room is created");

	let replacement = futures::executor::block_on(rooms.next())
		.expect("creating a room republishes the list")
		.expect("the replacement is not an error");
	assert_eq!(replacement.rooms.len(), 1);
	assert_eq!(replacement.rooms[0].room_id, "support");
}

// The core reads the room before each change to a room, and learns from it
// whether the host stores the messages. This host keeps only the default
// configuration, and a room it never created reads as `None`. A read through
// the subscription would leave one subscriber for each read, so the room is
// read without one.
#[test]
fn a_room_read_answers_the_room_and_leaves_no_subscriber() {
	let host = CliChatHost::new(Transcript::new(None, "chat"));
	futures::executor::block_on(host.create_chat_room(
		&product(),
		"support".to_string(),
		room("support"),
	))
	.expect("a new room is created");
	let read = |room_id: &str| {
		futures::executor::block_on(host.chat_room(&product(), room_id.to_string()))
			.expect("the host reads its rooms")
	};

	for _ in 0..3 {
		assert_eq!(
			read("support"),
			Some(ChatRoomState {
				participating_as: ChatRoomParticipation::RoomHost,
				config: ChatRoomConfig::default(),
			})
		);
	}
	assert_eq!(read("missing"), None);
	assert!(
		host.lock().subscribers.is_empty(),
		"a read that subscribes would leave one subscriber behind for each room change"
	);
}

// This host keeps only the default configuration. It refuses another one in
// the call that would create the room, so no room is left with the behaviour
// that the product did not ask for.
#[test]
fn a_room_with_a_configuration_this_host_cannot_keep_is_not_created() {
	let host = CliChatHost::new(Transcript::new(None, "chat"));
	let sourced = HostChatCreateRoomRequest {
		config: ChatRoomConfig { product_source: true, ..Default::default() },
		..room("support")
	};

	assert_eq!(
		futures::executor::block_on(host.create_chat_room(
			&product(),
			"support".to_string(),
			sourced
		)),
		Err(HostChatCreateRoomError::UnsupportedConfig)
	);
	assert!(
		host.lock().rooms.is_empty(),
		"a room left behind would have a behaviour that the product did not ask for"
	);
}
