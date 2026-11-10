use super::*;
use std::fs::read_to_string;
use truapi::platform::ProductId;

fn product() -> ProductContext {
	ProductContext::new(ProductId::for_test("chat.dot"))
}

fn text(text: &str) -> ChatMessageContent {
	ChatMessageContent::Text { text: text.to_string() }
}

fn room(room_id: &str) -> HostChatCreateRoomRequest {
	HostChatCreateRoomRequest {
		room_id: room_id.to_string(),
		name: "Support".to_string(),
		icon: String::new(),
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
	futures::executor::block_on(host.create_chat_room(&product(), room("support")))
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

	futures::executor::block_on(host.create_chat_room(&product(), room("support")))
		.expect("a new room is created");

	let replacement = futures::executor::block_on(rooms.next())
		.expect("creating a room republishes the list")
		.expect("the replacement is not an error");
	assert_eq!(replacement.rooms.len(), 1);
	assert_eq!(replacement.rooms[0].room_id, "support");
}
