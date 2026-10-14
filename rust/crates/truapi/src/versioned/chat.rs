//! Versioned wrappers for [`Chat`](crate::api::Chat) methods.
//!
//! v0.2 lets a product describe a posted message in one line with `alt`. A
//! v0.1 caller had no such text, so upgrading its request fills `alt` with
//! `None` and the host keeps its own placeholder.
//!
//! v0.2 of `create_room` carries the metadata, configuration and preview of a
//! room, v0.2 of `set_room_footer` carries the input configuration or a custom
//! footer, and v0.2 of the `action_subscribe` item carries a
//! [`v01::ChatEvent`]. A v0.1 subscriber receives only the events its item can
//! carry; see [`HostChatActionSubscribeItem::for_version`].

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum HostChatCreateRoomRequest {
		V1 => v01::HostChatCreateRoomRequest,
		V2 => v02::HostChatCreateRoomRequest,
	}
	pub enum HostChatCreateRoomResponse {
		V1 => v01::HostChatCreateRoomResponse,
		V2 => v01::HostChatCreateRoomResponse,
	}
	pub enum HostChatCreateRoomError {
		V1 => v01::HostChatCreateRoomError,
		V2 => v01::HostChatCreateRoomError,
	}
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
	pub enum HostChatActionSubscribeRequest { V1, V2 }
	pub enum HostChatActionSubscribeItem {
		V1 => v01::HostChatActionSubscribeItem,
		V2 => v02::HostChatActionSubscribeItem,
	}
	pub enum HostChatActionSubscribeError {
		V1 => v01::GenericError,
		V2 => v01::GenericError,
	}
	pub enum HostChatSetRoomFooterRequest {
		V1 => v01::HostChatSetRoomFooterRequest,
		V2 => v02::HostChatSetRoomFooterRequest,
	}
	pub enum HostChatSetRoomFooterResponse { V1, V2 }
	pub enum HostChatSetRoomFooterError {
		V1 => v01::HostChatSetRoomFooterError,
		V2 => v01::HostChatSetRoomFooterError,
	}
	pub enum HostChatRoomError { V1 => v01::HostChatRoomError }
	pub enum ProductChatQueryError { V1 => v01::ProductChatQueryError }
	pub enum HostChatSetRoomConfigRequest { V1 => v01::HostChatSetRoomConfigRequest }
	pub enum HostChatSetRoomConfigResponse { V1 }
	pub enum HostChatSetRoomMetadataRequest { V1 => v01::HostChatSetRoomMetadataRequest }
	pub enum HostChatSetRoomMetadataResponse { V1 }
	pub enum HostChatSetRoomPreviewRequest { V1 => v01::HostChatSetRoomPreviewRequest }
	pub enum HostChatSetRoomPreviewResponse { V1 }
	pub enum HostChatDeleteRoomRequest { V1 => v01::HostChatDeleteRoomRequest }
	pub enum HostChatDeleteRoomResponse { V1 }
	pub enum HostChatUpdateFeedRequest { V1 => v01::HostChatUpdateFeedRequest }
	pub enum HostChatUpdateFeedResponse { V1 }
	pub enum HostChatUpdateFeedError { V1 => v01::HostChatUpdateFeedError }
	pub enum HostChatSetRoomToolbarRequest { V1 => v01::HostChatSetRoomToolbarRequest }
	pub enum HostChatSetRoomToolbarResponse { V1 }
	pub enum HostChatSetRoomSuggestionsRequest { V1 => v01::HostChatSetRoomSuggestionsRequest }
	pub enum HostChatSetRoomSuggestionsResponse { V1 }
	pub enum HostChatSetRoomCommandsRequest { V1 => v01::HostChatSetRoomCommandsRequest }
	pub enum HostChatSetRoomCommandsResponse { V1 }
	pub enum HostChatSetRoomActivityRequest { V1 => v01::HostChatSetRoomActivityRequest }
	pub enum HostChatSetRoomActivityResponse { V1 }
	pub enum ProductChatMessagesQueryRequest { V1 => v01::ProductChatMessagesQueryRequest }
	pub enum ProductChatMessagesQueryItem { V1 => v01::ProductChatMessagesQueryItem }
	pub enum ProductChatUserActionRequest { V1 => v01::ProductChatUserActionRequest }
	pub enum ProductChatUserActionItem { V1 => v01::ProductChatUserActionItem }
	pub enum ProductChatUserActionError { V1 => v01::ProductChatUserActionError }
	pub enum ProductChatReactionsQueryRequest { V1 => v01::ProductChatReactionsQueryRequest }
	pub enum ProductChatReactionsQueryItem { V1 => v01::ProductChatReactionsQueryItem }
	pub enum ProductChatParticipantsQueryRequest { V1 => v01::ProductChatParticipantsQueryRequest }
	pub enum ProductChatParticipantsQueryItem { V1 => v01::ProductChatParticipantsQueryItem }
	pub enum ProductChatMessageActionsQueryRequest {
		V1 => v01::ProductChatMessageActionsQueryRequest,
	}
	pub enum ProductChatMessageActionsQueryItem { V1 => v01::ProductChatMessageActionsQueryItem }
	pub enum ProductChatParticipantActionsQueryRequest {
		V1 => v01::ProductChatParticipantActionsQueryRequest,
	}
	pub enum ProductChatParticipantActionsQueryItem {
		V1 => v01::ProductChatParticipantActionsQueryItem,
	}
}

/// Answer `latest` as `Self::V1` for a v0.1 peer and as `Self::V2` otherwise,
/// for a leg whose payload did not change between the two versions.
macro_rules! same_payload_in_both_versions {
	($($name:ident),* $(,)?) => {$(
		impl IntoLatest for $name {
			fn into_latest(self) -> Self::Latest {
				match self {
					Self::V1(payload) | Self::V2(payload) => payload,
				}
			}
		}

		impl FromLatest for $name {
			fn from_latest(latest: Self::Latest, target: u8) -> Self {
				if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
			}
		}
	)*};
}

// A response, item or error keeps its v0.1 shape where the request changed.
// It still gains a V2 variant, because a method's version is uniform across
// its request, response and error.
same_payload_in_both_versions!(
	HostChatCreateRoomResponse,
	HostChatCreateRoomError,
	HostChatPostMessageResponse,
	HostChatPostMessageError,
	HostChatActionSubscribeError,
	HostChatSetRoomFooterError,
);

/// Answer a unit leg in the version of the peer.
macro_rules! unit_in_both_versions {
	($($name:ident),* $(,)?) => {$(
		impl IntoLatest for $name {
			fn into_latest(self) -> Self::Latest {}
		}

		impl FromLatest for $name {
			fn from_latest(_latest: Self::Latest, target: u8) -> Self {
				if target >= 2 { Self::V2 } else { Self::V1 }
			}
		}
	)*};
}

unit_in_both_versions!(HostChatActionSubscribeRequest, HostChatSetRoomFooterResponse);

impl IntoLatest for HostChatPostMessageRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostChatPostMessageRequest { room_id, payload }) => {
				v02::HostChatPostMessageRequest { room_id, payload, alt: None }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl IntoLatest for HostChatCreateRoomRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostChatCreateRoomRequest { room_id, name, icon }) => {
				v02::HostChatCreateRoomRequest {
					room_id,
					metadata: v01::ChatRoomMetadata {
						name,
						avatar: inline_icon(&icon),
						description: None,
					},
					config: v01::ChatRoomConfig::default(),
					preview: None,
				}
			},
			Self::V2(latest) => latest,
		}
	}
}

impl HostChatCreateRoomRequest {
	/// The `icon` of a v0.1 request, which keeps the v0.1 rules: the host
	/// screens it as before, and draws an `https` icon, which a v0.2 resource
	/// cannot name. A v0.2 request gives `None`.
	pub fn v1_icon(&self) -> Option<&str> {
		match self {
			Self::V1(request) => Some(&request.icon),
			Self::V2(_) => None,
		}
	}
}

/// The avatar a v0.1 `icon` gives: the image of a base64 `data:` URL. An empty
/// icon, an `https` icon and a malformed one give no avatar, because a v0.2
/// resource never names a URL the host fetches. See
/// [`HostChatCreateRoomRequest::v1_icon`].
fn inline_icon(icon: &str) -> Option<v01::ChatResource> {
	let (header, data) = icon.trim().strip_prefix("data:")?.split_once(',')?;
	let mime_type = header.strip_suffix(";base64")?;
	let bytes = BASE64.decode(data).ok()?;
	Some(v01::ChatResource::Inline { mime_type: mime_type.to_string(), bytes })
}

impl IntoLatest for HostChatSetRoomFooterRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostChatSetRoomFooterRequest { room_id, footer }) => {
				let footer = match footer {
					v01::ChatRoomFooter::TextInput => {
						v02::ChatRoomFooter::Input(v01::ChatInputConfig::default())
					},
					v01::ChatRoomFooter::Empty => v02::ChatRoomFooter::Empty,
				};
				v02::HostChatSetRoomFooterRequest { room_id, footer }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl IntoLatest for HostChatActionSubscribeItem {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostChatActionSubscribeItem { room_id, peer, payload }) => {
				v02::HostChatActionSubscribeItem { room_id, peer, event: v1_payload_event(payload) }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for HostChatActionSubscribeItem {
	/// An event a v0.1 item cannot carry stays `V2`. The runtime never sends
	/// one to a v0.1 subscriber, because it skips each item for which
	/// [`Self::for_version`] gives `None`.
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target < 2 &&
			let Some(payload) = event_v1_payload(&latest.event)
		{
			return Self::V1(v01::HostChatActionSubscribeItem {
				room_id: latest.room_id,
				peer: latest.peer,
				payload,
			});
		}
		Self::V2(latest)
	}
}

impl HostChatActionSubscribeItem {
	/// The item a subscriber at version `target` receives, or `None` when its
	/// version cannot carry the event. A v0.1 subscriber receives posted
	/// messages, button presses and commands only. It receives a posted `Text`
	/// or `Custom` body as it is, any other body as `Text` of its text, and
	/// nothing for a body without text.
	pub fn for_version(latest: v02::HostChatActionSubscribeItem, target: u8) -> Option<Self> {
		if target < 2 && event_v1_payload(&latest.event).is_none() {
			return None;
		}
		Some(Self::from_latest(latest, target))
	}
}

/// The event of a v0.1 payload. v0.1 gave a posted message no id, so its
/// event has an empty `message_id`. A v0.1 reaction is a `UserReacted` event,
/// and URL media, which a [`v01::ChatMediaItem`] cannot carry, is left out.
fn v1_payload_event(payload: v01::ChatActionPayload) -> v01::ChatEvent {
	use v01::{ChatActionPayload as Payload, ChatMessageContent as Content};
	let posted =
		|body| v01::ChatEvent::MessagePosted { message_id: String::new(), body, reply_to: None };
	let reacted = |reaction: v01::ChatReaction, placed| v01::ChatEvent::UserReacted {
		message_id: reaction.message_id,
		emoji: reaction.emoji,
		placed,
	};
	match payload {
		Payload::MessagePosted(Content::Text { text }) => {
			posted(v01::ChatMessageBody::Text { text })
		},
		Payload::MessagePosted(Content::RichText(rich)) => {
			posted(v01::ChatMessageBody::RichText { text: rich.text, media: Vec::new() })
		},
		Payload::MessagePosted(Content::Actions(actions)) => {
			posted(v01::ChatMessageBody::Actions(actions))
		},
		Payload::MessagePosted(Content::File(file)) => {
			posted(v01::ChatMessageBody::RichText { text: file.text, media: Vec::new() })
		},
		Payload::MessagePosted(Content::Reaction(reaction)) => reacted(reaction, true),
		Payload::MessagePosted(Content::ReactionRemoved(reaction)) => reacted(reaction, false),
		Payload::MessagePosted(Content::Custom(custom)) => posted(v01::ChatMessageBody::Custom {
			message_type: custom.message_type,
			payload: custom.payload,
			preview_text: None,
		}),
		Payload::ActionTriggered(trigger) => v01::ChatEvent::ActionTriggered(trigger),
		Payload::Command(command) => v01::ChatEvent::Command(command),
	}
}

/// The v0.1 payload of an event, or `None` when v0.1 has no shape for it.
fn event_v1_payload(event: &v01::ChatEvent) -> Option<v01::ChatActionPayload> {
	use v01::{
		ChatActionPayload as Payload, ChatMessageBody as Body, ChatMessageContent as Content,
	};
	match event {
		v01::ChatEvent::MessagePosted { body, .. } => {
			let content = match body.clone() {
				Body::Text { text } => Content::Text { text },
				Body::Custom { message_type, payload, .. } => {
					Content::Custom(v01::ChatCustomMessage { message_type, payload })
				},
				Body::RichText { text, .. } => Content::Text { text: text? },
				Body::Actions(actions) => Content::Text { text: actions.text? },
			};
			Some(Payload::MessagePosted(content))
		},
		v01::ChatEvent::ActionTriggered(trigger) => Some(Payload::ActionTriggered(trigger.clone())),
		v01::ChatEvent::Command(command) => Some(Payload::Command(command.clone())),
		v01::ChatEvent::UserEdited { .. } |
		v01::ChatEvent::UserDeleted { .. } |
		v01::ChatEvent::UserReacted { .. } |
		v01::ChatEvent::Seen { .. } |
		v01::ChatEvent::MessageActionTriggered { .. } |
		v01::ChatEvent::ParticipantActionTriggered { .. } |
		v01::ChatEvent::ToolbarActionTriggered { .. } |
		v01::ChatEvent::SuggestionSelected { .. } |
		v01::ChatEvent::RoomOpened |
		v01::ChatEvent::RoomClosed => None,
	}
}

#[cfg(test)]
mod tests {
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

	// A V1 envelope encodes index 0 and V2 index 1, so an older peer's frames
	// still decode after the V2 legs land.
	#[test]
	fn the_v2_legs_take_the_next_positional_index() {
		let v1_room = HostChatCreateRoomRequest::V1(v01::HostChatCreateRoomRequest {
			room_id: "room".into(),
			name: "Room".into(),
			icon: String::new(),
		});
		let v2_room = HostChatCreateRoomRequest::V2(v1_room.clone().into_latest());
		assert_eq!((v1_room.encode()[0], v2_room.encode()[0]), (0, 1));
		assert_eq!(HostChatCreateRoomRequest::LATEST, 2);

		let footer = HostChatSetRoomFooterRequest::V2(v02::HostChatSetRoomFooterRequest {
			room_id: "room".into(),
			footer: v02::ChatRoomFooter::Custom,
		});
		// V2 envelope, the room id, then `Custom` at index 2.
		assert_eq!(hex::encode(footer.encode()), "0110726f6f6d02");
		assert_eq!(
			HostChatSetRoomFooterRequest::decode(&mut footer.encode().as_slice()).unwrap(),
			footer
		);

		assert_eq!(HostChatActionSubscribeRequest::V2.encode(), [1]);
		let item = HostChatActionSubscribeItem::V2(v02::HostChatActionSubscribeItem {
			room_id: "room".into(),
			peer: "native".into(),
			event: v01::ChatEvent::RoomClosed,
		});
		// V2 envelope, room, peer, then `RoomClosed`, the last event at index 12.
		assert_eq!(hex::encode(item.encode()), "0110726f6f6d186e61746976650c");
	}

	#[test]
	fn a_v01_room_upgrades_to_the_default_configuration() {
		let upgraded = HostChatCreateRoomRequest::V1(v01::HostChatCreateRoomRequest {
			room_id: "room".into(),
			name: "Room".into(),
			icon: "data:image/png;base64,AQID".into(),
		})
		.into_latest();

		assert_eq!(
			upgraded,
			v02::HostChatCreateRoomRequest {
				room_id: "room".into(),
				metadata: v01::ChatRoomMetadata {
					name: "Room".into(),
					avatar: Some(v01::ChatResource::Inline {
						mime_type: "image/png".into(),
						bytes: vec![1, 2, 3],
					}),
					description: None,
				},
				config: v01::ChatRoomConfig::default(),
				preview: None,
			}
		);
	}

	// A v0.2 resource never names a URL, so an `https` icon leaves the avatar
	// empty and travels beside the request under the v0.1 rules.
	#[test]
	fn a_v01_https_icon_keeps_the_v01_rules() {
		let request = HostChatCreateRoomRequest::V1(v01::HostChatCreateRoomRequest {
			room_id: "room".into(),
			name: "Room".into(),
			icon: "https://icons.example/room.png".into(),
		});

		assert_eq!(request.v1_icon(), Some("https://icons.example/room.png"));
		let upgraded = request.into_latest();
		assert_eq!(upgraded.metadata.avatar, None);
		assert_eq!(HostChatCreateRoomRequest::V2(upgraded).v1_icon(), None);
	}

	#[test]
	fn a_v01_footer_upgrades_to_the_default_input() {
		let footer = |footer| {
			HostChatSetRoomFooterRequest::V1(v01::HostChatSetRoomFooterRequest {
				room_id: "room".into(),
				footer,
			})
			.into_latest()
			.footer
		};

		assert_eq!(
			footer(v01::ChatRoomFooter::TextInput),
			v02::ChatRoomFooter::Input(v01::ChatInputConfig {
				placeholder: None,
				photo: true,
				video: true,
				file: false,
				payment: None,
			})
		);
		assert_eq!(footer(v01::ChatRoomFooter::Empty), v02::ChatRoomFooter::Empty);
	}

	#[test]
	fn a_room_and_a_footer_answer_in_the_callers_version() {
		let created =
			v01::HostChatCreateRoomResponse { status: v01::ChatRoomRegistrationStatus::New };
		assert_eq!(HostChatCreateRoomResponse::from_latest(created, 1).version(), 1);
		assert_eq!(HostChatCreateRoomResponse::from_latest(created, 2).version(), 2);
		assert_eq!(
			HostChatSetRoomFooterResponse::from_latest((), 1),
			HostChatSetRoomFooterResponse::V1
		);
		assert_eq!(
			HostChatSetRoomFooterResponse::from_latest((), 2),
			HostChatSetRoomFooterResponse::V2
		);
	}

	fn every_event() -> Vec<v01::ChatEvent> {
		use v01::ChatEvent::*;
		let id = || "m1".to_string();
		vec![
			MessagePosted {
				message_id: id(),
				body: v01::ChatMessageBody::Text { text: "hi".into() },
				reply_to: None,
			},
			ActionTriggered(v01::ActionTrigger {
				message_id: id(),
				action_id: "go".into(),
				payload: None,
			}),
			Command(v01::ChatCommand { command: "help".into(), payload: String::new() }),
			UserEdited { message_id: id(), body: v01::ChatMessageBody::Text { text: "hi".into() } },
			UserDeleted { message_id: id() },
			UserReacted { message_id: id(), emoji: "👍".into(), placed: true },
			Seen { message_id: id() },
			MessageActionTriggered { message_id: id(), action_id: "pin".into() },
			ParticipantActionTriggered { participant_id: "p".into(), action_id: "mute".into() },
			ToolbarActionTriggered { action_id: "settings".into() },
			SuggestionSelected { suggestion_id: "yes".into() },
			RoomOpened,
			RoomClosed,
		]
	}

	fn item(event: v01::ChatEvent) -> v02::HostChatActionSubscribeItem {
		v02::HostChatActionSubscribeItem { room_id: "room".into(), peer: "native".into(), event }
	}

	// S42: a V1 subscriber sees only the three events its item can carry, and
	// a V2 subscriber sees every event.
	#[test]
	fn a_v01_subscriber_receives_only_the_three_v01_events() {
		let delivered = every_event()
			.into_iter()
			.filter_map(|event| HostChatActionSubscribeItem::for_version(item(event), 1))
			.collect::<Vec<_>>();

		assert_eq!(
			delivered,
			[
				HostChatActionSubscribeItem::V1(v01::HostChatActionSubscribeItem {
					room_id: "room".into(),
					peer: "native".into(),
					payload: v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Text {
						text: "hi".into(),
					}),
				}),
				HostChatActionSubscribeItem::V1(v01::HostChatActionSubscribeItem {
					room_id: "room".into(),
					peer: "native".into(),
					payload: v01::ChatActionPayload::ActionTriggered(v01::ActionTrigger {
						message_id: "m1".into(),
						action_id: "go".into(),
						payload: None,
					}),
				}),
				HostChatActionSubscribeItem::V1(v01::HostChatActionSubscribeItem {
					room_id: "room".into(),
					peer: "native".into(),
					payload: v01::ChatActionPayload::Command(v01::ChatCommand {
						command: "help".into(),
						payload: String::new(),
					}),
				}),
			]
		);
		for event in every_event() {
			assert_eq!(
				HostChatActionSubscribeItem::for_version(item(event.clone()), 2),
				Some(HostChatActionSubscribeItem::V2(item(event)))
			);
		}
	}

	#[test]
	fn a_v01_subscriber_receives_a_posted_body_as_its_text() {
		let posted = |body| {
			HostChatActionSubscribeItem::for_version(
				item(v01::ChatEvent::MessagePosted {
					message_id: "m1".into(),
					body,
					reply_to: None,
				}),
				1,
			)
			.map(|item| match item {
				HostChatActionSubscribeItem::V1(item) => item.payload,
				HostChatActionSubscribeItem::V2(_) => panic!("a V1 subscriber gets a V1 item"),
			})
		};
		let text = |text: &str| {
			Some(v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Text {
				text: text.into(),
			}))
		};

		assert_eq!(
			posted(v01::ChatMessageBody::RichText {
				text: Some("**hi**".into()),
				media: Vec::new()
			}),
			text("**hi**")
		);
		assert_eq!(posted(v01::ChatMessageBody::RichText { text: None, media: Vec::new() }), None);
		assert_eq!(
			posted(v01::ChatMessageBody::Actions(v01::ChatActions {
				text: Some("Pick one".into()),
				actions: Vec::new(),
				layout: v01::ChatActionLayout::Column,
			})),
			text("Pick one")
		);
		assert_eq!(
			posted(v01::ChatMessageBody::Custom {
				message_type: "vote".into(),
				payload: vec![1],
				preview_text: Some("A vote".into()),
			}),
			Some(v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Custom(
				v01::ChatCustomMessage { message_type: "vote".into(), payload: vec![1] }
			)))
		);
	}

	#[test]
	fn a_v01_item_upgrades_to_its_event() {
		let upgrade = |payload| {
			HostChatActionSubscribeItem::V1(v01::HostChatActionSubscribeItem {
				room_id: "room".into(),
				peer: "native".into(),
				payload,
			})
			.into_latest()
			.event
		};

		assert_eq!(
			upgrade(v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Text {
				text: "hi".into(),
			})),
			v01::ChatEvent::MessagePosted {
				message_id: String::new(),
				body: v01::ChatMessageBody::Text { text: "hi".into() },
				reply_to: None,
			}
		);
		assert_eq!(
			upgrade(v01::ChatActionPayload::MessagePosted(
				v01::ChatMessageContent::ReactionRemoved(v01::ChatReaction {
					message_id: "m1".into(),
					emoji: "👍".into()
				})
			)),
			v01::ChatEvent::UserReacted {
				message_id: "m1".into(),
				emoji: "👍".into(),
				placed: false
			}
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
}
