//! Golden frames of the Chat Extension v2 methods 0 to 21.
//!
//! A host that knows only v2 must accept each v2 call of a product that uses
//! a newer `@parity/truapi`. The client always encodes the newest version of a
//! request, so a later method must not change the bytes of an earlier method.
//! `tests/snapshots/chat-v2-frames.txt` holds one frame for each leg of methods
//! 0 to 21, as the v2 wire encodes them. This test encodes the same values
//! again and compares the bytes.
//!
//! A failure means that the change moved a v2 frame. That change breaks each
//! product and host that exists: undo it and add a method. When the frames
//! differ, the test writes the frames it encoded to `chat-v2-frames.txt.actual`
//! beside the snapshot.

use std::{fs, path::PathBuf};

use parity_scale_codec::Encode;
use truapi::{
	frame::{
		MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE, MESSAGE_TYPE_START, Payload, ProtocolMessage,
	},
	generated::wire_table::{self, MethodIds},
	v01, v02,
	versioned::chat as versioned,
};

/// One line of the snapshot: a name, then the hex of the whole frame.
fn line(name: &str, ids: MethodIds, message_type: u8, value: impl Encode) -> String {
	let frame = ProtocolMessage {
		request_id: "p:1".to_string(),
		payload: Payload {
			trait_id: ids.trait_id,
			method_id: ids.method_id,
			message_type,
			value: value.encode(),
		},
	};
	format!("{name} {}", hex::encode(frame.encode()))
}

/// A `Result::Ok` around a response payload, as a response leg carries it.
fn ok<T: Encode>(value: T) -> Result<T, ()> {
	Ok(value)
}

fn message() -> v01::ChatMessage {
	v01::ChatMessage {
		message_id: "m1".into(),
		origin: v01::ChatMessageOrigin { participant_id: "product".into() },
		timestamp: 1_700_000_000_000,
		body: v01::ChatMessageBody::RichText {
			text: Some("**hi**".into()),
			media: vec![v01::ChatMediaItem::Image {
				resource: v01::ChatResource::Preimage { key: vec![7; 32] },
				meta: v01::ChatMediaMeta {
					mime_type: "image/png".into(),
					size_bytes: Some(3),
					width: Some(4),
					height: Some(5),
					placeholder: None,
					alt: Some("a dot".into()),
				},
			}],
		},
		status: v01::ChatMessageStatus::Sent,
		reply_to: Some("m0".into()),
		edited_at: None,
		reactions: vec![v01::ChatReactionSummary {
			emoji: "👍".into(),
			count: 2,
			reacted_by_user: true,
		}],
	}
}

fn action() -> v01::ChatProductAction {
	v01::ChatProductAction {
		action_id: "pin".into(),
		label: "Pin".into(),
		icon: None,
		destructive: false,
		handler: v01::ChatActionHandler::Callback,
	}
}

fn entry() -> v01::ChatParticipantEntry {
	v01::ChatParticipantEntry {
		participant_id: "p1".into(),
		participant: v01::ChatParticipant::Custom { nickname: "Ada".into(), avatar: None },
	}
}

fn config() -> v01::ChatRoomConfig {
	v01::ChatRoomConfig {
		product_source: true,
		reaction_details: true,
		participants: false,
		message_actions: true,
		allowed_reactions: Some(vec!["👍".into()]),
		placeholder: Some("Nothing yet".into()),
		report_seen: true,
	}
}

/// Every leg of methods 0 to 21, at the versions a v2 client encodes.
fn frames() -> Vec<String> {
	use versioned::*;
	let room = || "room".to_string();
	let metadata = v01::ChatRoomMetadata {
		name: "Room".into(),
		avatar: Some(v01::ChatResource::Inline { mime_type: "image/png".into(), bytes: vec![1] }),
		description: Some("About".into()),
	};
	let preview = v01::ChatRoomPreview {
		order: v01::ChatRoomOrder::PinToTop,
		timestamp: Some(5),
		badge: Some(v01::ChatRoomBadge::UnreadCount { count: 3 }),
		text: Some(v01::ChatRoomPreviewText { text: "typing".into(), ongoing: true }),
	};
	vec![
		line(
			"0.create_room.request",
			wire_table::CHAT_CREATE_ROOM,
			MESSAGE_TYPE_REQUEST,
			HostChatCreateRoomRequest::V2(v02::HostChatCreateRoomRequest {
				room_id: room(),
				metadata: metadata.clone(),
				config: config(),
				preview: Some(preview.clone()),
			}),
		),
		line(
			"0.create_room.response",
			wire_table::CHAT_CREATE_ROOM,
			MESSAGE_TYPE_RESPONSE,
			ok(HostChatCreateRoomResponse::V2(v01::HostChatCreateRoomResponse {
				status: v01::ChatRoomRegistrationStatus::Exists,
			})),
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
			ok(HostChatRegisterBotResponse::V1(v01::HostChatRegisterBotResponse {
				status: v01::ChatBotRegistrationStatus::New,
			})),
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
			"3.post_message.response",
			wire_table::CHAT_POST_MESSAGE,
			MESSAGE_TYPE_RESPONSE,
			ok(HostChatPostMessageResponse::V2(v01::HostChatPostMessageResponse {
				message_id: "m1".into(),
			})),
		),
		line(
			"4.action_subscribe.start",
			wire_table::CHAT_ACTION_SUBSCRIBE,
			MESSAGE_TYPE_START,
			HostChatActionSubscribeRequest::V2,
		),
		line(
			"4.action_subscribe.receive",
			wire_table::CHAT_ACTION_SUBSCRIBE,
			MESSAGE_TYPE_RESPONSE,
			HostChatActionSubscribeItem::V2(v02::HostChatActionSubscribeItem {
				room_id: room(),
				peer: "native".into(),
				event: v01::ChatEvent::UserReacted {
					message_id: "m1".into(),
					emoji: "👍".into(),
					placed: true,
				},
			}),
		),
		line(
			"6.set_room_footer.request",
			wire_table::CHAT_SET_ROOM_FOOTER,
			MESSAGE_TYPE_REQUEST,
			HostChatSetRoomFooterRequest::V2(v02::HostChatSetRoomFooterRequest {
				room_id: room(),
				footer: v02::ChatRoomFooter::Input(v01::ChatInputConfig {
					placeholder: Some("Ask".into()),
					photo: true,
					video: false,
					file: true,
					payment: None,
				}),
			}),
		),
		line(
			"6.set_room_footer.response",
			wire_table::CHAT_SET_ROOM_FOOTER,
			MESSAGE_TYPE_RESPONSE,
			ok(HostChatSetRoomFooterResponse::V2),
		),
		line(
			"8.set_room_metadata.request",
			wire_table::CHAT_SET_ROOM_METADATA,
			MESSAGE_TYPE_REQUEST,
			HostChatSetRoomMetadataRequest::V1(v01::HostChatSetRoomMetadataRequest {
				room_id: room(),
				metadata,
			}),
		),
		line(
			"9.set_room_preview.request",
			wire_table::CHAT_SET_ROOM_PREVIEW,
			MESSAGE_TYPE_REQUEST,
			HostChatSetRoomPreviewRequest::V1(v01::HostChatSetRoomPreviewRequest {
				room_id: room(),
				preview,
			}),
		),
		line(
			"10.delete_room.request",
			wire_table::CHAT_DELETE_ROOM,
			MESSAGE_TYPE_REQUEST,
			HostChatDeleteRoomRequest::V1(v01::HostChatDeleteRoomRequest { room_id: room() }),
		),
		line(
			"11.update_feed.request",
			wire_table::CHAT_UPDATE_FEED,
			MESSAGE_TYPE_REQUEST,
			HostChatUpdateFeedRequest::V1(v01::HostChatUpdateFeedRequest {
				room_id: room(),
				update: v01::ChatFeedUpdate::New(message()),
			}),
		),
		line(
			"11.update_feed.error",
			wire_table::CHAT_UPDATE_FEED,
			MESSAGE_TYPE_RESPONSE,
			Err::<(), _>(HostChatUpdateFeedError::V1(v01::HostChatUpdateFeedError::NotOwner)),
		),
		line(
			"16.messages_query.start",
			wire_table::CHAT_MESSAGES_QUERY,
			MESSAGE_TYPE_START,
			ProductChatMessagesQueryRequest::V1(v01::ProductChatMessagesQueryRequest {
				room_id: room(),
				anchor: v01::ChatPageAnchor::Before { cursor: Some("c1".into()) },
				page_size: 40,
			}),
		),
		line(
			"16.messages_query.receive",
			wire_table::CHAT_MESSAGES_QUERY,
			MESSAGE_TYPE_RESPONSE,
			ProductChatMessagesQueryItem::V1(v01::ProductChatMessagesQueryItem {
				messages: vec![message()],
				next: Some("c0".into()),
			}),
		),
		line(
			"17.user_action.start",
			wire_table::CHAT_USER_ACTION,
			MESSAGE_TYPE_START,
			ProductChatUserActionRequest::V1(v01::ProductChatUserActionRequest {
				room_id: room(),
				action: v01::ChatUserAction::NewMessage {
					body: v01::ChatMessageBody::Text { text: "hi".into() },
					reply_to: None,
				},
			}),
		),
		line(
			"17.user_action.receive",
			wire_table::CHAT_USER_ACTION,
			MESSAGE_TYPE_RESPONSE,
			ProductChatUserActionItem::V1(
				v01::ProductChatUserActionItem::MessageCreated(message()),
			),
		),
		line(
			"18.reactions_query.start",
			wire_table::CHAT_REACTIONS_QUERY,
			MESSAGE_TYPE_START,
			ProductChatReactionsQueryRequest::V1(v01::ProductChatReactionsQueryRequest {
				room_id: room(),
				message_id: "m1".into(),
			}),
		),
		line(
			"18.reactions_query.receive",
			wire_table::CHAT_REACTIONS_QUERY,
			MESSAGE_TYPE_RESPONSE,
			ProductChatReactionsQueryItem::V1(v01::ProductChatReactionsQueryItem {
				reactions: vec![v01::ChatReactionDetail {
					participant_id: "p1".into(),
					emoji: "👍".into(),
				}],
			}),
		),
		line(
			"19.participants_query.start",
			wire_table::CHAT_PARTICIPANTS_QUERY,
			MESSAGE_TYPE_START,
			ProductChatParticipantsQueryRequest::V1(v01::ProductChatParticipantsQueryRequest {
				room_id: room(),
				query: v01::ChatParticipantsQuery::Page { cursor: None, page_size: 100 },
			}),
		),
		line(
			"19.participants_query.receive",
			wire_table::CHAT_PARTICIPANTS_QUERY,
			MESSAGE_TYPE_RESPONSE,
			ProductChatParticipantsQueryItem::V1(v01::ProductChatParticipantsQueryItem::Page {
				participants: vec![entry()],
				next: None,
			}),
		),
		line(
			"20.message_actions_query.start",
			wire_table::CHAT_MESSAGE_ACTIONS_QUERY,
			MESSAGE_TYPE_START,
			ProductChatMessageActionsQueryRequest::V1(v01::ProductChatMessageActionsQueryRequest {
				room_id: room(),
				message: message(),
			}),
		),
		line(
			"20.message_actions_query.receive",
			wire_table::CHAT_MESSAGE_ACTIONS_QUERY,
			MESSAGE_TYPE_RESPONSE,
			ProductChatMessageActionsQueryItem::V1(v01::ProductChatMessageActionsQueryItem {
				menu: v01::ChatMessageMenu::ExtendBuiltIn {
					actions: vec![action()],
					hidden: vec![v01::ChatBuiltInMessageAction::Edit],
				},
			}),
		),
		line(
			"20.message_actions_query.error",
			wire_table::CHAT_MESSAGE_ACTIONS_QUERY,
			MESSAGE_TYPE_RESPONSE,
			ProductChatQueryError::V1(v01::ProductChatQueryError::NotAvailable),
		),
	]
}

#[test]
fn the_frames_of_methods_0_to_21_are_unchanged() {
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
