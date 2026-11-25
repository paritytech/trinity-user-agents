use super::*;
use std::fs::read_to_string;
use truapi::{
	latest::{
		ChatCallClose, ChatCustomBody, ChatMessageContent, ChatPaymentRecord, ChatPreviewMessage,
		ChatPreviewReaction, ChatRoomActivity, HostChatSetRoomFooterRequest,
	},
	platform::ProductId,
};

/// The reserved participant id of the product itself.
const PRODUCT: &str = "product";

/// An inline image, which the core passes on as a metadata icon and an image
/// badge.
const ICON: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

fn product() -> ProductContext {
	ProductContext::new(ProductId::for_test("chat.dot"))
}

fn text(text: &str) -> ChatMessageContent {
	ChatMessageContent::Text { text: text.to_string() }
}

fn body(text: &str) -> ChatMessageBody {
	ChatMessageBody::Text { text: text.to_string() }
}

fn room(room_id_source: &str) -> HostChatCreateRoomRequest {
	HostChatCreateRoomRequest {
		room_id_source: room_id_source.to_string(),
		name: "Support".to_string(),
		icon: String::new(),
		config: Default::default(),
	}
}

fn source_config() -> ChatRoomConfig {
	ChatRoomConfig { product_source: true, ..ChatRoomConfig::default() }
}

// The core derives the room id. These tests drive the host alone, so the
// source stands in for the id.
fn create(host: &CliChatHost, request: HostChatCreateRoomRequest) {
	let room_id = request.room_id_source.clone();
	futures::executor::block_on(host.create_chat_room(&product(), room_id, request))
		.expect("a room is created");
}

/// A host with the room `support` the host keeps and the room `kept`
/// the product keeps. The configuration arrives with the room.
fn host_with_rooms(transcript: Option<PathBuf>) -> Arc<CliChatHost> {
	let host = CliChatHost::new(transcript);
	create(&host, room("support"));
	create(&host, HostChatCreateRoomRequest { config: source_config(), ..room("kept") });
	host
}

fn message(message_id: &str, origin: &str, text: &str) -> ChatMessage {
	ChatMessage {
		message_id: message_id.to_string(),
		origin: ChatMessageOrigin { participant_id: origin.to_string() },
		timestamp: 1_767_225_600_000,
		body: body(text),
		status: ChatMessageStatus::Sent,
		reply: None,
		edited_at: None,
		reactions: Vec::new(),
		request: false,
	}
}

fn feed(
	host: &CliChatHost,
	room_id: &str,
	update: ChatFeedUpdate,
	persist: bool,
) -> Result<(), ChatFeedOutcome> {
	let outcome = futures::executor::block_on(host.update_chat_feed(
		&product(),
		HostChatUpdateFeedRequest { room_id: room_id.to_string(), update },
		persist,
	))
	.expect("the room exists");
	match outcome {
		ChatFeedOutcome::Applied => Ok(()),
		refusal => Err(refusal),
	}
}

fn room_view(host: &CliChatHost, room_id: &str) -> Value {
	host.observe()
		.0
		.rooms
		.into_iter()
		.find(|room| room["roomId"] == room_id)
		.expect("the room is in the snapshot")
}

fn drain(events: &mut mpsc::UnboundedReceiver<Value>) -> Vec<Value> {
	std::iter::from_fn(|| events.try_recv().ok()).collect()
}

fn transcript_lines(path: &std::path::Path) -> Vec<Value> {
	read_to_string(path)
		.expect("the transcript is readable")
		.lines()
		.map(|line| serde_json::from_str(line).expect("one JSON object per line"))
		.collect()
}

#[test]
fn a_message_needs_a_room_this_host_created() {
	let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
	let host = CliChatHost::new(Some(transcript.path().to_path_buf()));

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
	let host = CliChatHost::new(Some(transcript.path().to_path_buf()));
	create(&host, room("support"));

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
	let recorded = transcript_lines(transcript.path()).pop().expect("the message line");
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
	let host = CliChatHost::new(None);
	let mut rooms = host.subscribe_chat_rooms(&product());

	let snapshot = futures::executor::block_on(rooms.next())
		.expect("a subscription emits its snapshot")
		.expect("the snapshot is not an error");
	assert!(snapshot.rooms.is_empty());

	create(&host, room("support"));

	let replacement = futures::executor::block_on(rooms.next())
		.expect("creating a room republishes the list")
		.expect("the replacement is not an error");
	assert_eq!(replacement.rooms.len(), 1);
	assert_eq!(replacement.rooms[0].room_id, "support");
}

/// A surface that opens before the worker creates its room must still
/// learn about the room, and one that opens after must not be told twice.
#[test]
fn an_observer_sees_changes_after_its_snapshot_exactly_once() {
	let host = CliChatHost::new(None);
	let (before, mut early) = host.observe();
	assert!(before.rooms.is_empty());

	create(&host, room("support"));
	let (after, mut late) = host.observe();
	let posted = host.post_user_message("support", body("hi"), None).expect("the room exists");

	let room = early.try_recv().expect("the room event");
	assert_eq!(room["kind"], "room");
	assert_eq!(room["roomId"], "support");
	assert_eq!(room["name"], "Support");
	assert_eq!(room["participatingAs"], "RoomHost");
	let message = early.try_recv().expect("the message event");
	assert_eq!(message["kind"], "message");
	assert_eq!(message["message"]["messageId"], posted.message_id);

	assert_eq!(after.rooms.len(), 1);
	assert_eq!(after.rooms[0]["name"], "Support");
	let only = late.try_recv().expect("the message event");
	assert_eq!(only, message);
	assert!(late.try_recv().is_err(), "the room was in the snapshot");
}

/// A product correlates an action trigger by message id, so an id the
/// person's message took must never be handed to a product message too.
#[test]
fn user_and_product_messages_share_one_id_sequence() {
	let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
	let host = CliChatHost::new(Some(transcript.path().to_path_buf()));
	create(&host, room("support"));
	let post = |content| {
		futures::executor::block_on(host.post_chat_message(
			&product(),
			HostChatPostMessageRequest {
				room_id: "support".to_string(),
				payload: content,
				alt: None,
			},
		))
		.expect("the room exists")
		.message_id
	};

	let first = post(text("welcome"));
	let second = host.post_user_message("support", body("hello"), None).expect("the room exists");
	let third = post(text("hi there"));
	assert_eq!((first.as_str(), second.message_id.as_str(), third.as_str()), ("m1", "m2", "m3"));
	assert_eq!(second.origin.participant_id, USER);

	let recorded = transcript_lines(transcript.path());
	assert_eq!(recorded[2]["author"], "person");
	assert_eq!(recorded[2]["messageId"], "m2");
	assert_eq!(recorded[2]["payload"], hex::encode(body("hello").encode()));

	assert!(host.post_user_message("elsewhere", body("lost"), None).is_err());
	assert_eq!(host.observe().0.messages.len(), 1);
}

#[test]
fn a_room_the_product_keeps_takes_no_message_from_the_user_through_the_host() {
	let host = host_with_rooms(None);
	assert!(host.post_user_message("kept", body("hi"), None).is_err());
}

// A room that exists keeps its name and what it declared. Its configuration
// is the one of its last `create_room`, which the core sends each time.
#[test]
fn a_room_that_exists_keeps_what_it_declared() {
	let host = CliChatHost::new(None);
	create(&host, HostChatCreateRoomRequest { config: source_config(), ..room("kept") });

	let again = futures::executor::block_on(host.create_chat_room(
		&product(),
		"kept".to_string(),
		HostChatCreateRoomRequest {
			name: "Renamed".to_string(),
			config: source_config(),
			..room("kept")
		},
	))
	.expect("an existing room is found again");

	assert_eq!(again, ChatRoomRegistrationStatus::Exists);
	assert_eq!(host.room_config("kept"), Some(source_config()));
	assert_eq!(room_view(&host, "kept")["name"], "Support");
}

// A `create_room` that moves the messages of a room to the product discards
// the messages that the host stored for it.
#[test]
fn a_room_that_changes_its_store_discards_its_messages() {
	let host = CliChatHost::new(None);
	create(&host, room("support"));
	host.post_user_message("support", body("mine"), None).expect("the room exists");
	assert_eq!(host.observe().0.messages.len(), 1);

	create(&host, HostChatCreateRoomRequest { config: source_config(), ..room("support") });

	assert_eq!(host.room_config("support"), Some(source_config()));
	assert!(host.observe().0.messages.is_empty());
}

// The core reads the room to learn whether the host stores its messages, so
// a room reads back with the configuration of its last `create_room`. A room
// this host never created reads as `None`.
#[test]
fn a_room_reads_back_with_its_configuration() {
	let host = host_with_rooms(None);
	let read = |room_id: &str| {
		futures::executor::block_on(host.chat_room(&product(), room_id.to_string()))
			.expect("the host reads its rooms")
	};
	let hosted = |config| {
		Some(ChatRoomState { participating_as: ChatRoomParticipation::RoomHost, config })
	};
	assert_eq!(read("kept"), hosted(source_config()));
	assert_eq!(read("support"), hosted(ChatRoomConfig::default()));
	assert_eq!(read("never-created"), None);
}

// A metadata without an icon keeps the icon the room has.
#[test]
fn metadata_replaces_the_name_description_and_icon() {
	let host = host_with_rooms(None);
	let (_, mut events) = host.observe();
	let mut declare = |name: &str, icon: Option<&str>| {
		futures::executor::block_on(host.set_chat_room_metadata(
			&product(),
			HostChatSetRoomMetadataRequest {
				room_id: "support".to_string(),
				metadata: ChatRoomMetadata {
					name: name.to_string(),
					description: Some("We answer".to_string()),
					icon: icon.map(str::to_string),
					placeholder: Some("Ask us anything".to_string()),
				},
			},
		))
		.expect("the room exists");
		events.try_recv().expect("the room event")
	};

	let event = declare("Help desk", Some(ICON));
	assert_eq!(event["name"], "Help desk");
	assert_eq!(event["placeholder"], "Ask us anything");
	assert_eq!(event["description"], "We answer");
	assert_eq!(event["icon"], ICON);
	let event = declare("Front desk", None);
	assert_eq!((event["name"].clone(), event["icon"].clone()), (json!("Front desk"), json!(ICON)));
}

// In the host store a product deletes only its own messages, as it edits
// only its own. The page learns that the message left the room.
#[test]
fn a_deleted_message_leaves_the_room() {
	let host = host_with_rooms(None);
	let user = host.post_user_message("support", body("mine"), None).expect("the room exists");
	feed(&host, "support", ChatFeedUpdate::New(message("p-1", PRODUCT, "hi")), true)
		.expect("a new message is stored");
	let (_, mut events) = host.observe();
	let deleted = |id: &str| ChatFeedUpdate::Deleted { message_id: id.to_string() };

	assert_eq!(
		feed(&host, "support", deleted(&user.message_id), true),
		Err(ChatFeedOutcome::NotOwner)
	);
	assert_eq!(
		feed(&host, "support", deleted("p-9"), true),
		Err(ChatFeedOutcome::UnknownMessage)
	);
	feed(&host, "support", deleted("p-1"), true).expect("the product deletes its message");

	let events = drain(&mut events);
	assert_eq!(events.len(), 1);
	assert_eq!(
		(events[0]["kind"].clone(), events[0]["messageId"].clone()),
		(json!("messageDeleted"), json!("p-1"))
	);
	assert!(host.message("support", "p-1").is_none());
	assert!(host.message("support", &user.message_id).is_some());

	// A room the product keeps deletes what its cache holds, and an
	// update for a message off screen changes nothing.
	host.cache_page("kept", &[message("s-1", USER, "cached")]);
	feed(&host, "kept", deleted("s-0"), false).expect("a message off screen is fine");
	feed(&host, "kept", deleted("s-1"), false).expect("the screen drops the message");
	assert!(host.message("kept", "s-1").is_none());
}

#[test]
fn the_list_row_takes_the_preview_and_defaults_the_rest() {
	let host = host_with_rooms(None);
	feed(&host, "support", ChatFeedUpdate::New(message("p-1", PRODUCT, "newest")), true)
		.expect("the room exists");
	// With nothing declared: the newest text and the host's unread count.
	let list = &room_view(&host, "support")["list"];
	assert_eq!(list["text"], "newest");
	assert_eq!(list["origin"], PRODUCT);
	assert_eq!(list["badge"], json!({ "type": "UnreadCount", "count": 1 }));
	assert_eq!(list["unseenReactions"], 0);

	let preview = |preview: ChatRoomPreview| {
		let outcome = futures::executor::block_on(host.set_chat_room_preview(
			&product(),
			HostChatSetRoomPreviewRequest { room_id: "support".to_string(), preview },
		));
		assert_eq!(outcome, Ok(ChatRoomChangeOutcome::Applied));
		room_view(&host, "support")["list"].clone()
	};
	let from_the_product = ChatPreviewMessage {
		message_id: "c-1".to_string(),
		origin: ChatMessageOrigin { participant_id: "contact".to_string() },
		timestamp: 1_767_225_600_000,
		body: body("from the product"),
		status: ChatMessageStatus::Sent,
		reply_to: None,
		edited_at: None,
		request: false,
	};
	let list = preview(ChatRoomPreview {
		timestamp: Some(42),
		badge: Some(ChatRoomBadge::UnreadCount { count: 7 }),
		last_message: Some(from_the_product.clone()),
		last_reaction: Some(ChatPreviewReaction {
			emoji: "+1".to_string(),
			by_user: false,
			message: Some(from_the_product),
			removed: true,
		}),
		unseen_reactions: 2,
		pinned: true,
		activity: Some(ChatRoomActivity { text: "Ben is typing".to_string(), ongoing: true }),
	});
	assert_eq!(list["timestamp"], 42);
	assert_eq!(list["pinned"], true);
	assert_eq!(list["activity"], json!({ "text": "Ben is typing", "ongoing": true }));
	assert_eq!(list["badge"], json!({ "type": "UnreadCount", "count": 7 }));
	assert_eq!(list["text"], "from the product");
	assert_eq!(list["origin"], "contact");
	assert_eq!(
		list["lastReaction"],
		json!({ "emoji": "+1", "byUser": false, "messageId": "c-1", "removed": true })
	);
	assert_eq!(list["unseenReactions"], 2);

	let list = preview(ChatRoomPreview {
		timestamp: None,
		badge: Some(ChatRoomBadge::UnreadCount { count: 0 }),
		last_message: None,
		last_reaction: None,
		unseen_reactions: 0,
		pinned: false,
		activity: None,
	});
	assert_eq!(list["badge"], Value::Null, "a count of 0 shows nothing");
	assert_eq!(list["text"], "newest");
	assert_eq!((list["pinned"].clone(), list["activity"].clone()), (json!(false), Value::Null));
	let list = preview(ChatRoomPreview {
		timestamp: None,
		badge: Some(ChatRoomBadge::Image { icon: ICON.to_string() }),
		last_message: None,
		last_reaction: None,
		unseen_reactions: 0,
		pinned: false,
		activity: None,
	});
	assert_eq!(list["badge"], json!({ "type": "Image", "icon": ICON }));
}

#[test]
fn deleting_a_room_removes_it_and_is_idempotent() {
	let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
	let host = host_with_rooms(Some(transcript.path().to_path_buf()));
	let mut rooms = host.subscribe_chat_rooms(&product());
	futures::executor::block_on(rooms.next());
	let (_, mut events) = host.observe();
	let delete = |room_id: &str| {
		futures::executor::block_on(host.delete_chat_room(
			&product(),
			HostChatDeleteRoomRequest { room_id: room_id.to_string() },
		))
	};

	assert_eq!(
		[delete("support"), delete("support")],
		[Ok(ChatRoomChangeOutcome::Applied), Ok(ChatRoomChangeOutcome::Applied)],
		"a room that is gone is deleted too"
	);

	assert_eq!(drain(&mut events), vec![json!({ "kind": "roomDeleted", "roomId": "support" })]);
	let list = futures::executor::block_on(rooms.next())
		.expect("the list is republished")
		.expect("not an error");
	assert_eq!(list.rooms.iter().map(|room| room.room_id.as_str()).collect::<Vec<_>>(), ["kept"]);
	assert!(host.room_config("support").is_none());
	let kinds: Vec<_> = transcript_lines(transcript.path())
		.into_iter()
		.map(|line| line["kind"].clone())
		.collect();
	assert_eq!(kinds, ["room", "room", "config", "deleteRoom", "deleteRoom"]);
}

#[test]
fn the_store_of_a_room_takes_every_feed_update() {
	let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
	let host = host_with_rooms(Some(transcript.path().to_path_buf()));
	let user = host.post_user_message("support", body("mine"), None).expect("the room exists");
	let (_, mut events) = host.observe();

	feed(&host, "support", ChatFeedUpdate::New(message("p-1", PRODUCT, "first")), true)
		.expect("a new message is stored");
	assert_eq!(
		feed(&host, "support", ChatFeedUpdate::New(message("p-1", PRODUCT, "again")), true),
		Err(ChatFeedOutcome::DuplicateMessage)
	);
	feed(
		&host,
		"support",
		ChatFeedUpdate::Edited {
			message_id: "p-1".to_string(),
			body: body("first, edited"),
			edited_at: 99,
		},
		true,
	)
	.expect("the product edits its message");
	assert_eq!(
		feed(
			&host,
			"support",
			ChatFeedUpdate::Edited {
				message_id: user.message_id.clone(),
				body: body("not yours"),
				edited_at: 99,
			},
			true
		),
		Err(ChatFeedOutcome::NotOwner)
	);
	for participant_id in ["p1", "p1", "p2", "p3"] {
		let update = ChatFeedUpdate::ReactionPlaced {
			message_id: "p-1".to_string(),
			participant_id: participant_id.to_string(),
			emoji: "+1".to_string(),
		};
		feed(&host, "support", update, true).expect("a reaction is stored");
	}
	assert_eq!(
		feed(
			&host,
			"support",
			ChatFeedUpdate::ReactionPlaced {
				message_id: "p-1".to_string(),
				participant_id: USER.to_string(),
				emoji: "+1".to_string(),
			},
			true
		),
		Err(ChatFeedOutcome::NotOwner)
	);
	feed(
		&host,
		"support",
		ChatFeedUpdate::ReactionRemoved {
			message_id: "p-1".to_string(),
			participant_id: "p2".to_string(),
			emoji: "+1".to_string(),
		},
		true,
	)
	.expect("a reaction is removed");
	feed(
		&host,
		"support",
		ChatFeedUpdate::StatusChanged {
			message_id: "p-1".to_string(),
			status: ChatMessageStatus::Read,
		},
		true,
	)
	.expect("a status changes");

	let last = drain(&mut events).pop().expect("message events");
	assert_eq!(last["persisted"], true);
	let stored = &last["message"];
	assert_eq!(stored["body"]["text"], "first, edited");
	assert_eq!(stored["editedAt"], 99);
	assert_eq!(stored["status"], "Read");
	// One per participant, the repeat of p1 counted once.
	assert_eq!(stored["reactions"], json!([{ "emoji": "+1", "count": 2, "reactedByUser": false }]));

	// A call that ended is drawn again in place, without an edit mark.
	let call = ChatMessage {
		body: ChatMessageBody::Call(ChatCallRecord {
			video: false,
			answered_at: Some(5),
			closed: Some(ChatCallClose { at: 9, by_user: true }),
		}),
		..message("p-1", PRODUCT, "")
	};
	feed(&host, "support", ChatFeedUpdate::Replaced(call), true).expect("replaced");
	let replaced = drain(&mut events).pop().expect("the replace event");
	assert_eq!(replaced["message"]["body"]["type"], "Call");
	assert_eq!(replaced["message"]["body"]["text"], "Audio call, ended");
	assert_eq!(replaced["message"]["editedAt"], Value::Null);
	assert_eq!(replaced["message"]["reactions"], json!([]));
	// A replacement keeps the author of the message that it replaces. A
	// product cannot replace a message of the user.
	feed(&host, "support", ChatFeedUpdate::Replaced(message("p-1", "p2", "")), true)
		.expect("replaced");
	let replaced = drain(&mut events).pop().expect("the replace event");
	assert_eq!(replaced["message"]["origin"], PRODUCT);
	assert_eq!(
		feed(
			&host,
			"support",
			ChatFeedUpdate::Replaced(message(&user.message_id, PRODUCT, "not yours")),
			true
		),
		Err(ChatFeedOutcome::NotOwner)
	);
	assert_eq!(
		feed(&host, "support", ChatFeedUpdate::Replaced(message("p-9", PRODUCT, "")), true),
		Err(ChatFeedOutcome::UnknownMessage)
	);
	// The core checks the room first, so a room that is gone by now is a
	// failure of the host.
	assert_eq!(
		futures::executor::block_on(host.update_chat_feed(
			&product(),
			HostChatUpdateFeedRequest {
				room_id: "gone".to_string(),
				update: ChatFeedUpdate::Replaced(message("p-1", PRODUCT, "")),
			},
			true,
		)),
		Err(GenericError { reason: unknown_room("gone") })
	);

	let feed_lines: Vec<_> = transcript_lines(transcript.path())
		.into_iter()
		.filter(|line| line["kind"] == "feed")
		.collect();
	assert_eq!(feed_lines.len(), 10, "only the updates the host applied");
	assert_eq!(feed_lines[0]["update"], "New");
	assert_eq!(feed_lines[0]["messageId"], "p-1");
	assert_eq!(feed_lines[0]["persist"], true);
	assert_eq!(
		feed_lines[0]["payload"],
		hex::encode(ChatFeedUpdate::New(message("p-1", PRODUCT, "first")).encode())
	);
}

#[test]
fn a_feed_update_of_a_room_the_product_keeps_is_for_the_screen_only() {
	let host = host_with_rooms(None);
	let (_, mut events) = host.observe();

	// The page cache never saw it: nothing to change, and no error.
	feed(&host, "kept", ChatFeedUpdate::Replaced(message("old", PRODUCT, "")), false)
		.expect("an update for a message off screen is fine");
	assert!(drain(&mut events).is_empty());

	feed(&host, "kept", ChatFeedUpdate::New(message("s-1", PRODUCT, "hi")), false)
		.expect("a new message goes to the screen");
	let mut summary = message("s-2", USER, "with reactions");
	summary.reactions = vec![ChatReactionSummary {
		emoji: "+1".to_string(),
		count: 3,
		reacted_by_user: false,
		unseen: false,
	}];
	host.cache_page("kept", &[summary]);
	feed(
		&host,
		"kept",
		ChatFeedUpdate::ReactionPlaced {
			message_id: "s-2".to_string(),
			participant_id: USER.to_string(),
			emoji: "+1".to_string(),
		},
		false,
	)
	.expect("a reaction goes to the screen");

	let events = drain(&mut events);
	assert_eq!(events.len(), 2);
	assert_eq!(events[0]["persisted"], false);
	assert_eq!(events[0]["message"]["messageId"], "s-1");
	assert_eq!(
		events[1]["message"]["reactions"],
		json!([{ "emoji": "+1", "count": 4, "reactedByUser": true }])
	);
	// The product's own badge rules: nothing unless it set one.
	assert_eq!(events[1]["list"]["badge"], Value::Null);
	assert!(host.message("kept", "s-2").is_some());
	assert!(host.observe().0.messages.is_empty(), "the snapshot holds only the host's store");
}

#[test]
fn the_footer_is_kept() {
	let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
	let host = host_with_rooms(Some(transcript.path().to_path_buf()));
	futures::executor::block_on(host.set_chat_room_footer(
		&product(),
		HostChatSetRoomFooterRequest {
			room_id: "support".to_string(),
			footer: ChatRoomFooter::Empty,
		},
	))
	.expect("the room exists");

	assert_eq!(room_view(&host, "support")["footer"], json!({ "type": "Empty" }));
	// The default footer of a room that declared none.
	assert_eq!(room_view(&host, "kept")["footer"], json!({ "type": "TextInput" }));

	let kinds: Vec<_> = transcript_lines(transcript.path())
		.into_iter()
		.map(|line| line["kind"].as_str().unwrap_or_default().to_string())
		.filter(|kind| kind != "room")
		.collect();
	assert_eq!(kinds, ["config", "footer"]);
}

#[test]
fn the_newest_seen_message_clears_the_unread_count() {
	let host = host_with_rooms(None);
	for id in ["p-1", "p-2"] {
		feed(&host, "support", ChatFeedUpdate::New(message(id, PRODUCT, id)), true)
			.expect("stored");
	}
	assert_eq!(room_view(&host, "support")["list"]["badge"]["count"], 2);

	host.mark_seen("support", "p-1").expect("known");
	assert_eq!(room_view(&host, "support")["list"]["badge"]["count"], 1);
	host.mark_seen("support", "p-2").expect("known");
	assert_eq!(room_view(&host, "support")["list"]["badge"], Value::Null);
	assert_eq!(room_view(&host, "support")["lastSeen"], "p-2");
	assert!(host.mark_seen("support", "nope").is_err());
	assert!(host.mark_seen("kept", "nope").is_err());
}

#[test]
fn the_answer_to_a_user_action_shows_on_the_screen() {
	let host = host_with_rooms(None);
	host.cache_page("kept", &[message("s-1", USER, "first")]);
	let (_, mut events) = host.observe();
	let created = message("s-2", USER, "hello");

	host.apply_source_action(
		"kept",
		&ChatUserAction::NewMessage {
			body: body("hello"),
			reply_to: None,
			message_id: None,
			timestamp: None,
		},
		&ProductChatUserActionResponse::MessageCreated(created.clone()),
	);
	let acknowledged = ProductChatUserActionResponse::Acknowledged;
	host.apply_source_action(
		"kept",
		&ChatUserAction::EditMessage { message_id: "s-1".to_string(), body: body("edited") },
		&acknowledged,
	);
	host.apply_source_action(
		"kept",
		&ChatUserAction::ReactionPlaced { message_id: "s-1".to_string(), emoji: "+1".to_string() },
		&acknowledged,
	);
	host.apply_source_action("kept", &ChatUserAction::AcceptRequest, &acknowledged);
	host.apply_source_action(
		"kept",
		&ChatUserAction::Seen { message_id: "s-1".to_string() },
		&acknowledged,
	);

	let events = drain(&mut events);
	let messages: Vec<_> = events.iter().map(|event| event["message"].clone()).collect();
	assert_eq!(messages[0]["messageId"], "s-2");
	assert_eq!(messages[1]["body"]["text"], "edited");
	assert_eq!(messages[2]["reactions"][0]["reactedByUser"], true);
	assert_eq!(events.len(), 3, "a contact action or a seen report changes no message");
	assert_eq!(room_view(&host, "kept")["lastSeen"], "s-1");
}

#[test]
fn the_list_text_of_a_message_falls_back_in_order() {
	let meta = |alt: Option<&str>| ChatMediaMeta {
		mime_type: "image/png".to_string(),
		size_bytes: None,
		width: None,
		height: None,
		placeholder: None,
		alt: alt.map(str::to_string),
		duration_ms: None,
	};
	let image = |alt| ChatMediaItem::Image { resource: hop(), meta: meta(alt) };
	assert_eq!(preview_text(&body("hi")), "hi");
	assert_eq!(
		preview_text(&ChatMessageBody::RichText { text: None, media: vec![image(Some("A cat"))] }),
		"A cat"
	);
	assert_eq!(
		preview_text(&ChatMessageBody::RichText { text: None, media: vec![image(None)] }),
		SENT_A_MESSAGE
	);
	assert_eq!(
		preview_text(&ChatMessageBody::Event(ChatContactEvent::RequestAccepted)),
		"Request accepted"
	);
	assert_eq!(
		preview_text(&ChatMessageBody::Call(ChatCallRecord {
			video: true,
			answered_at: None,
			closed: Some(ChatCallClose { at: 1, by_user: false }),
		})),
		"Video call, not answered"
	);
	assert_eq!(
		preview_text(&ChatMessageBody::Payment(ChatPaymentRecord { amount: 1_500, legacy: false })),
		"Payment of 1500"
	);
	let custom = |alt: Option<&str>| ChatMessageBody::Custom {
		message: ChatCustomBody { message_type: "poll".to_string(), payload: vec![1] },
		alt: alt.map(str::to_string),
	};
	assert_eq!(preview_text(&custom(Some("A poll"))), "A poll");
	assert_eq!(preview_text(&custom(None)), SENT_A_MESSAGE);
}

fn hop() -> ChatResource {
	ChatResource::Hop {
		identifier: [1; 32],
		ticket: [2; 32],
		node: "wss://hop.example".to_string(),
		size: 1,
	}
}

#[test]
fn a_body_is_shown_with_every_field_the_product_sent() {
	let media = vec![
		ChatMediaItem::Image {
			resource: hop(),
			meta: ChatMediaMeta {
				mime_type: "image/png".to_string(),
				size_bytes: Some(1),
				width: Some(2),
				height: Some(3),
				placeholder: Some("LKO2".to_string()),
				alt: Some("dot".to_string()),
				duration_ms: None,
			},
		},
		ChatMediaItem::File {
			resource: hop(),
			meta: ChatMediaMeta {
				mime_type: "application/pdf".to_string(),
				size_bytes: None,
				width: None,
				height: None,
				placeholder: None,
				alt: None,
				duration_ms: None,
			},
			file_name: "a.pdf".to_string(),
		},
	];
	let view = body_json(&ChatMessageBody::RichText { text: Some("b".to_string()), media });
	assert_eq!(view["type"], "RichText");
	assert_eq!(view["text"], "b");
	// The ticket stays with the host: the page gets the node and the size.
	assert_eq!(
		view["media"][0],
		json!({
			"kind": "Image",
			"resource": {
				"type": "Hop",
				"size": 1,
				"node": "wss://hop.example",
				"identifier": format!("0x{}", "01".repeat(32)),
			},
			"fileName": null,
			"meta": { "mimeType": "image/png", "sizeBytes": 1, "width": 2, "height": 3, "placeholder": "LKO2", "alt": "dot" },
		})
	);
	assert_eq!(view["media"][1]["kind"], "File");
	assert_eq!(view["media"][1]["fileName"], "a.pdf");
	assert_eq!(
		body_json(&ChatMessageBody::Call(ChatCallRecord {
			video: false,
			answered_at: Some(3),
			closed: None,
		})),
		json!({
			"type": "Call",
			"video": false,
			"answeredAt": 3,
			"closed": null,
			"text": "Audio call, ongoing",
		})
	);
	assert_eq!(
		body_json(&ChatMessageBody::Payment(ChatPaymentRecord { amount: u128::MAX, legacy: true })),
		json!({ "type": "Payment", "amount": u128::MAX.to_string(), "legacy": true })
	);
	assert_eq!(
		body_json(&ChatMessageBody::Event(ChatContactEvent::LeftChat)),
		json!({ "type": "Event", "event": "LeftChat", "text": "Left the chat" })
	);
}

// The core reads the room before each change to a room. A read through the
// subscription would leave one subscriber for each read, so the room is read
// without one.
#[test]
fn a_room_read_answers_the_room_and_leaves_no_subscriber() {
	let host = CliChatHost::new(None);
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
	assert!(host.lock().subscribers.is_empty());
}
