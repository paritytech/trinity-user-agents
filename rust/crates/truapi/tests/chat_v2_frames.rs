//! Golden frames of the Chat Extension v2 methods.
//!
//! A host that knows only v2 must accept each v2 call of a product that uses
//! a newer `@parity/truapi`. The client always encodes the newest version of a
//! request, so a later method must not change the bytes of an earlier method.
//! `tests/snapshots/chat-v2-frames.txt` holds one frame for each leg of the
//! methods below, as the v2 wire encodes them. This test encodes the same values
//! again and compares the bytes. Each frame also decodes back to its value as
//! the type of its leg, so the snapshot holds only bytes that a peer reads.
//!
//! A failure means that the change moved a v2 frame. That change breaks each
//! product and host that exists: undo it and add a method. When the frames
//! differ, the test writes the frames it encoded to `chat-v2-frames.txt.actual`
//! beside the snapshot.
//!
//! `post_message` and `set_room_footer` keep the frames of the first Chat
//! methods. Products that use the first SDK call them through the client that
//! the host injects. `create_room` has a second version, in which the host
//! makes the room id, and its first frames still decode.

use std::{fmt::Debug, fs, path::PathBuf};

use parity_scale_codec::{Decode, DecodeAll, Encode};
use truapi::{
	CallError,
	frame::{
		MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE, MESSAGE_TYPE_START,
		Payload, ProtocolMessage,
	},
	generated::wire_table::{self, MethodIds},
	v01, v02,
	versioned::chat as versioned,
};

/// One line of the snapshot: a name, then the hex of the whole frame. The
/// value is the type of its leg, and the frame must decode back to it.
fn line<T: Encode + Decode + PartialEq + Debug>(
	name: &str,
	ids: MethodIds,
	message_type: u8,
	value: T,
) -> String {
	let frame = ProtocolMessage {
		request_id: "p:1".to_string(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type,
			value: value.encode(),
		},
	};
	let bytes = frame.encode();
	let decoded = ProtocolMessage::decode_all(&mut &bytes[..]).expect("decode the frame");
	assert_eq!(decoded, frame, "{name}");
	assert_eq!(T::decode_all(&mut &decoded.payload.value[..]), Ok(value), "{name}");
	format!("{name} {}", hex::encode(bytes))
}

fn message() -> v02::ChatMessage {
	v02::ChatMessage {
		message_id: "m1".into(),
		origin: v02::ChatMessageOrigin { participant_id: "contact".into() },
		timestamp: 1_700_000_000_000,
		body: v02::ChatMessageBody::RichText {
			text: Some("hi".into()),
			media: vec![v02::ChatMediaItem::Image {
				resource: v02::ChatResource::Hop {
					identifier: [7; 32],
					ticket: [8; 32],
					node: "wss://hop.example".into(),
					size: 3,
				},
				meta: v02::ChatMediaMeta {
					mime_type: "image/png".into(),
					size_bytes: Some(3),
					width: Some(4),
					height: Some(5),
					placeholder: None,
					alt: Some("a dot".into()),
					duration_ms: Some(6),
				},
			}],
		},
		status: v02::ChatMessageStatus::Sent,
		reply: Some(v02::ChatReply {
			message_id: "m0".into(),
			preview: Some(v02::ChatReplyPreview {
				origin: v02::ChatMessageOrigin { participant_id: "user".into() },
				body: v02::ChatMessageBody::Text { text: "yo".into() },
			}),
		}),
		edited_at: None,
		reactions: vec![v02::ChatReactionSummary {
			emoji: "👍".into(),
			count: 2,
			reacted_by_user: true,
			unseen: true,
		}],
		request: true,
	}
}

fn call() -> v02::ChatMessage {
	v02::ChatMessage {
		message_id: "c1".into(),
		origin: v02::ChatMessageOrigin { participant_id: "user".into() },
		timestamp: 1_700_000_000_000,
		body: v02::ChatMessageBody::Call(v02::ChatCallRecord {
			video: true,
			answered_at: Some(1_700_000_001_000),
			closed: Some(v02::ChatCallClose { at: 1_700_000_060_000, by_user: false }),
		}),
		status: v02::ChatMessageStatus::Delivered,
		reply: None,
		edited_at: None,
		reactions: Vec::new(),
		request: false,
	}
}

fn custom() -> v02::ChatMessage {
	v02::ChatMessage {
		message_id: "x1".into(),
		origin: v02::ChatMessageOrigin { participant_id: "contact".into() },
		timestamp: 1_700_000_000_000,
		body: v02::ChatMessageBody::Custom {
			message: v02::ChatCustomBody { message_type: "poll".into(), payload: vec![1, 2] },
			alt: Some("A poll".into()),
		},
		status: v02::ChatMessageStatus::Sent,
		reply: None,
		edited_at: None,
		reactions: Vec::new(),
		request: false,
	}
}

fn config() -> v02::ChatRoomConfig {
	v02::ChatRoomConfig { product_source: true, reaction_details: true, report_seen: true }
}

/// Every leg of the methods, at the versions a v2 client encodes.
fn frames() -> Vec<String> {
	use versioned::*;
	let room = || "room".to_string();
	let metadata = v02::ChatRoomMetadata {
		name: "Room".into(),
		description: Some("About".into()),
		icon: Some("https://icons.example/renamed.png".into()),
		placeholder: Some("No games yet".into()),
	};
	let preview = v02::ChatRoomPreview {
		timestamp: Some(5),
		badge: Some(v02::ChatRoomBadge::UnreadCount { count: 3 }),
		last_message: Some(v02::ChatPreviewMessage {
			message_id: "m1".into(),
			origin: v02::ChatMessageOrigin { participant_id: "contact".into() },
			timestamp: 1_700_000_000_000,
			body: v02::ChatMessageBody::Text { text: "Three new answers".into() },
			status: v02::ChatMessageStatus::Delivered,
			reply_to: Some("m0".into()),
			edited_at: Some(1_700_000_000_500),
			request: false,
		}),
		last_reaction: Some(v02::ChatPreviewReaction {
			emoji: "👍".into(),
			by_user: false,
			message: None,
			removed: true,
		}),
		unseen_reactions: 2,
		pinned: true,
		activity: Some(v02::ChatRoomActivity { text: "Ben is typing".into(), ongoing: true }),
	};
	vec![
		line(
			"0.create_room.request",
			wire_table::CHAT_CREATE_ROOM,
			MESSAGE_TYPE_REQUEST,
			HostChatCreateRoomRequest::V2(v02::HostChatCreateRoomRequest {
				room_id_source: "support".into(),
				name: "Room".into(),
				icon: "https://icons.example/room.png".into(),
				config: config(),
			}),
		),
		line(
			"0.create_room.response",
			wire_table::CHAT_CREATE_ROOM,
			MESSAGE_TYPE_RESPONSE,
			Ok::<_, CallError<HostChatCreateRoomError>>(HostChatCreateRoomResponse::V2(
				v02::HostChatCreateRoomResponse {
					status: v02::ChatRoomRegistrationStatus::Exists,
					room_id: room(),
				},
			)),
		),
		line(
			"0.create_room.v1_request",
			wire_table::CHAT_CREATE_ROOM,
			MESSAGE_TYPE_REQUEST,
			HostChatCreateRoomRequest::V1(v01::HostChatCreateRoomRequest {
				room_id: room(),
				name: "Room".into(),
				icon: "https://icons.example/room.png".into(),
			}),
		),
		line(
			"0.create_room.v1_response",
			wire_table::CHAT_CREATE_ROOM,
			MESSAGE_TYPE_RESPONSE,
			Ok::<_, CallError<HostChatCreateRoomError>>(HostChatCreateRoomResponse::V1(
				v01::HostChatCreateRoomResponse { status: v01::ChatRoomRegistrationStatus::Exists },
			)),
		),
		line(
			"1.register_bot.request",
			wire_table::CHAT_REGISTER_BOT,
			MESSAGE_TYPE_REQUEST,
			HostChatRegisterBotRequest::V1(v01::HostChatRegisterBotRequest {
				bot_id: "helper".into(),
				name: "Helper".into(),
				icon: String::new(),
			}),
		),
		line(
			"1.register_bot.response",
			wire_table::CHAT_REGISTER_BOT,
			MESSAGE_TYPE_RESPONSE,
			Ok::<_, CallError<HostChatRegisterBotError>>(HostChatRegisterBotResponse::V1(
				v01::HostChatRegisterBotResponse { status: v01::ChatBotRegistrationStatus::New },
			)),
		),
		line(
			"2.list_subscribe.start",
			wire_table::CHAT_LIST_SUBSCRIBE,
			MESSAGE_TYPE_START,
			HostChatListSubscribeRequest::V1,
		),
		line(
			"2.list_subscribe.receive",
			wire_table::CHAT_LIST_SUBSCRIBE,
			MESSAGE_TYPE_RESPONSE,
			HostChatListSubscribeItem::V1(v01::HostChatListSubscribeItem {
				rooms: vec![v01::ChatRoom {
					room_id: room(),
					participating_as: v01::ChatRoomParticipation::RoomHost,
				}],
			}),
		),
		line(
			"3.post_message.request",
			wire_table::CHAT_POST_MESSAGE,
			MESSAGE_TYPE_REQUEST,
			HostChatPostMessageRequest::V2(v02::HostChatPostMessageRequest {
				room_id: room(),
				payload: v01::ChatMessageContent::Text { text: "hi".into() },
				alt: Some("alt".into()),
			}),
		),
		line(
			"3.post_message.request_without_alt",
			wire_table::CHAT_POST_MESSAGE,
			MESSAGE_TYPE_REQUEST,
			HostChatPostMessageRequest::V2(v02::HostChatPostMessageRequest {
				room_id: room(),
				payload: v01::ChatMessageContent::Text { text: "hi".into() },
				alt: None,
			}),
		),
		line(
			"3.post_message.v1_request",
			wire_table::CHAT_POST_MESSAGE,
			MESSAGE_TYPE_REQUEST,
			HostChatPostMessageRequest::V1(v01::HostChatPostMessageRequest {
				room_id: room(),
				payload: v01::ChatMessageContent::Text { text: "hi".into() },
			}),
		),
		line(
			"3.post_message.response",
			wire_table::CHAT_POST_MESSAGE,
			MESSAGE_TYPE_RESPONSE,
			Ok::<_, CallError<HostChatPostMessageError>>(HostChatPostMessageResponse::V2(
				v01::HostChatPostMessageResponse { message_id: "m1".into() },
			)),
		),
		line(
			"4.action_subscribe.start",
			wire_table::CHAT_ACTION_SUBSCRIBE,
			MESSAGE_TYPE_START,
			HostChatActionSubscribeRequest::V1,
		),
		line(
			"4.action_subscribe.receive",
			wire_table::CHAT_ACTION_SUBSCRIBE,
			MESSAGE_TYPE_RESPONSE,
			HostChatActionSubscribeItem::V1(v01::HostChatActionSubscribeItem {
				room_id: room(),
				peer: "native".into(),
				payload: v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Text {
					text: "hi".into(),
				}),
			}),
		),
		line(
			"6.set_room_footer.request",
			wire_table::CHAT_SET_ROOM_FOOTER,
			MESSAGE_TYPE_REQUEST,
			HostChatSetRoomFooterRequest::V1(v01::HostChatSetRoomFooterRequest {
				room_id: room(),
				footer: v01::ChatRoomFooter::TextInput,
			}),
		),
		line(
			"6.set_room_footer.response",
			wire_table::CHAT_SET_ROOM_FOOTER,
			MESSAGE_TYPE_RESPONSE,
			Ok::<_, CallError<HostChatSetRoomFooterError>>(HostChatSetRoomFooterResponse::V1),
		),
		line(
			"8.set_room_metadata.request",
			wire_table::CHAT_SET_ROOM_METADATA,
			MESSAGE_TYPE_REQUEST,
			HostChatSetRoomMetadataRequest::V2(v02::HostChatSetRoomMetadataRequest {
				room_id: room(),
				metadata,
			}),
		),
		line(
			"8.set_room_metadata.response",
			wire_table::CHAT_SET_ROOM_METADATA,
			MESSAGE_TYPE_RESPONSE,
			Ok::<_, CallError<HostChatSetRoomMetadataError>>(HostChatSetRoomMetadataResponse::V2),
		),
		line(
			"8.set_room_metadata.error",
			wire_table::CHAT_SET_ROOM_METADATA,
			MESSAGE_TYPE_RESPONSE,
			Err::<HostChatSetRoomMetadataResponse, _>(CallError::Domain(
				HostChatSetRoomMetadataError::V2(v02::HostChatSetRoomMetadataError::NotRoomHost),
			)),
		),
		line(
			"9.set_room_preview.request",
			wire_table::CHAT_SET_ROOM_PREVIEW,
			MESSAGE_TYPE_REQUEST,
			HostChatSetRoomPreviewRequest::V2(v02::HostChatSetRoomPreviewRequest {
				room_id: room(),
				preview: preview.clone(),
			}),
		),
		line(
			"9.set_room_preview.image_badge",
			wire_table::CHAT_SET_ROOM_PREVIEW,
			MESSAGE_TYPE_REQUEST,
			HostChatSetRoomPreviewRequest::V2(v02::HostChatSetRoomPreviewRequest {
				room_id: room(),
				preview: v02::ChatRoomPreview {
					badge: Some(v02::ChatRoomBadge::Image {
						icon: "https://icons.example/live.png".into(),
					}),
					pinned: false,
					activity: None,
					..preview
				},
			}),
		),
		line(
			"9.set_room_preview.response",
			wire_table::CHAT_SET_ROOM_PREVIEW,
			MESSAGE_TYPE_RESPONSE,
			Ok::<_, CallError<HostChatSetRoomPreviewError>>(HostChatSetRoomPreviewResponse::V2),
		),
		line(
			"10.delete_room.request",
			wire_table::CHAT_DELETE_ROOM,
			MESSAGE_TYPE_REQUEST,
			HostChatDeleteRoomRequest::V2(v02::HostChatDeleteRoomRequest { room_id: room() }),
		),
		line(
			"10.delete_room.response",
			wire_table::CHAT_DELETE_ROOM,
			MESSAGE_TYPE_RESPONSE,
			Ok::<_, CallError<HostChatDeleteRoomError>>(HostChatDeleteRoomResponse::V2),
		),
		line(
			"11.update_feed.request",
			wire_table::CHAT_UPDATE_FEED,
			MESSAGE_TYPE_REQUEST,
			HostChatUpdateFeedRequest::V2(v02::HostChatUpdateFeedRequest {
				room_id: room(),
				update: v02::ChatFeedUpdate::New(message()),
			}),
		),
		line(
			"11.update_feed.replaced",
			wire_table::CHAT_UPDATE_FEED,
			MESSAGE_TYPE_REQUEST,
			HostChatUpdateFeedRequest::V2(v02::HostChatUpdateFeedRequest {
				room_id: room(),
				update: v02::ChatFeedUpdate::Replaced(call()),
			}),
		),
		line(
			"11.update_feed.deleted",
			wire_table::CHAT_UPDATE_FEED,
			MESSAGE_TYPE_REQUEST,
			HostChatUpdateFeedRequest::V2(v02::HostChatUpdateFeedRequest {
				room_id: room(),
				update: v02::ChatFeedUpdate::Deleted { message_id: "m1".into() },
			}),
		),
		line(
			"11.update_feed.custom",
			wire_table::CHAT_UPDATE_FEED,
			MESSAGE_TYPE_REQUEST,
			HostChatUpdateFeedRequest::V2(v02::HostChatUpdateFeedRequest {
				room_id: room(),
				update: v02::ChatFeedUpdate::New(custom()),
			}),
		),
		line(
			"11.update_feed.response",
			wire_table::CHAT_UPDATE_FEED,
			MESSAGE_TYPE_RESPONSE,
			Ok::<_, CallError<HostChatUpdateFeedError>>(HostChatUpdateFeedResponse::V2),
		),
		line(
			"11.update_feed.error",
			wire_table::CHAT_UPDATE_FEED,
			MESSAGE_TYPE_RESPONSE,
			Err::<HostChatUpdateFeedResponse, _>(CallError::Domain(HostChatUpdateFeedError::V2(
				v02::HostChatUpdateFeedError::NotOwner,
			))),
		),
		line(
			"12.messages_query.start",
			wire_table::CHAT_MESSAGES_QUERY,
			MESSAGE_TYPE_START,
			ProductChatMessagesQueryRequest::V2(v02::ProductChatMessagesQueryRequest {
				room_id: room(),
				anchor: v02::ChatPageAnchor::Before { cursor: Some("c1".into()) },
				page_size: 40,
			}),
		),
		line(
			"12.messages_query.receive",
			wire_table::CHAT_MESSAGES_QUERY,
			MESSAGE_TYPE_RESPONSE,
			ProductChatMessagesQueryResponse::V2(v02::ProductChatMessagesQueryResponse {
				messages: vec![message()],
				next: Some("c0".into()),
			}),
		),
		line(
			"12.messages_query.error",
			wire_table::CHAT_MESSAGES_QUERY,
			MESSAGE_TYPE_INTERRUPT,
			Err::<(), _>(CallError::Domain(ProductChatQueryError::V2(
				v02::ProductChatQueryError::NotAvailable,
			))),
		),
		line(
			"13.user_action.start",
			wire_table::CHAT_USER_ACTION,
			MESSAGE_TYPE_START,
			ProductChatUserActionRequest::V2(v02::ProductChatUserActionRequest {
				room_id: room(),
				action: v02::ChatUserAction::NewMessage {
					body: v02::ChatMessageBody::Text { text: "hi".into() },
					reply_to: None,
					message_id: Some("m1".into()),
					timestamp: Some(1_700_000_000_000),
				},
			}),
		),
		line(
			"13.user_action.accept",
			wire_table::CHAT_USER_ACTION,
			MESSAGE_TYPE_START,
			ProductChatUserActionRequest::V2(v02::ProductChatUserActionRequest {
				room_id: room(),
				action: v02::ChatUserAction::AcceptRequest,
			}),
		),
		line(
			"13.user_action.pay",
			wire_table::CHAT_USER_ACTION,
			MESSAGE_TYPE_START,
			ProductChatUserActionRequest::V2(v02::ProductChatUserActionRequest {
				room_id: room(),
				action: v02::ChatUserAction::SendPayment,
			}),
		),
		line(
			"13.user_action.delete",
			wire_table::CHAT_USER_ACTION,
			MESSAGE_TYPE_START,
			ProductChatUserActionRequest::V2(v02::ProductChatUserActionRequest {
				room_id: room(),
				action: v02::ChatUserAction::DeleteMessage { message_id: "m1".into() },
			}),
		),
		line(
			"13.user_action.receive",
			wire_table::CHAT_USER_ACTION,
			MESSAGE_TYPE_RESPONSE,
			ProductChatUserActionResponse::V2(v02::ProductChatUserActionResponse::MessageCreated(
				message(),
			)),
		),
		line(
			"13.user_action.acknowledged",
			wire_table::CHAT_USER_ACTION,
			MESSAGE_TYPE_RESPONSE,
			ProductChatUserActionResponse::V2(v02::ProductChatUserActionResponse::Acknowledged),
		),
		line(
			"13.user_action.error",
			wire_table::CHAT_USER_ACTION,
			MESSAGE_TYPE_INTERRUPT,
			Err::<(), _>(CallError::Domain(ProductChatUserActionError::V2(
				v02::ProductChatUserActionError::Rejected { reason: "no".into() },
			))),
		),
		line(
			"14.reactions_query.start",
			wire_table::CHAT_REACTIONS_QUERY,
			MESSAGE_TYPE_START,
			ProductChatReactionsQueryRequest::V2(v02::ProductChatReactionsQueryRequest {
				room_id: room(),
				message_id: "m1".into(),
			}),
		),
		line(
			"14.reactions_query.receive",
			wire_table::CHAT_REACTIONS_QUERY,
			MESSAGE_TYPE_RESPONSE,
			ProductChatReactionsQueryResponse::V2(v02::ProductChatReactionsQueryResponse {
				reactions: vec![v02::ChatReactionDetail {
					participant_id: "p1".into(),
					emoji: "👍".into(),
					timestamp: 1_700_000_000_000,
				}],
			}),
		),
	]
}

#[test]
fn the_frames_of_the_v2_methods_are_unchanged() {
	let snapshot =
		PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/chat-v2-frames.txt");
	let actual = frames().join("\n") + "\n";
	let expected = fs::read_to_string(&snapshot).unwrap_or_default();
	if expected != actual {
		let dump = snapshot.with_extension("txt.actual");
		fs::write(&dump, &actual).expect("write the encoded frames");
		panic!("a v2 chat frame changed; the encoded frames are in {}", dump.display());
	}
}
