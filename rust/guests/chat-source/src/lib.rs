//! Keeps the room `notes`, a room whose messages the product keeps
//! (`product_source`). The host asks the worker for them, and the worker
//! answers the calls that the host starts:
//! - `chat.messagesQuery`: the messages of the room, oldest first;
//! - `chat.userAction`: a new message of the user is stored, and the worker adds the note `noted:
//!   <text>` after it; another action is acknowledged;
//! - `renderer.render`: the first message has a `Custom` body, which the worker draws as the text
//!   `card: <payload>` and keeps on screen until the host stops the render;
//! - `chat.participantsQuery`, answered with no codec: the room has two participants.
//!
//! The worker answers no other host-initiated method, so the host gets
//! `Unsupported` for them. It runs until the host stops it.

use core::cell::{Cell, RefCell};

use truapi::{
	CallError,
	codec::{Decode, Encode},
	latest::{
		ChatMessage, ChatMessageBody, ChatMessageOrigin, ChatMessageStatus, ChatPageAnchor,
		ChatRoomConfig, ChatRoomMetadata, ChatUserAction, HostChatCreateRoomRequest,
		ProductChatMessagesQueryItem, ProductChatQueryError, ProductChatUserActionError,
		ProductChatUserActionItem, RenderContext, RendererNode,
	},
	raw::RawSink,
	versioned::{
		FromLatest, Versioned,
		chat::{
			ProductChatParticipantsQueryItem, ProductChatParticipantsQueryRequest,
			ProductChatQueryError as VersionedQueryError,
		},
	},
};

const ROOM: &str = "notes";

/// The message type of the card that the worker draws.
const CARD: &str = "card";

thread_local! {
	/// The number of the next message.
	static NEXT_MESSAGE: Cell<u32> = const { Cell::new(0) };
	/// The messages of the room, oldest first.
	static MESSAGES: RefCell<Vec<ChatMessage>> = RefCell::new(vec![message(
		"product",
		ChatMessageBody::Custom {
			message_type: CARD.to_string(),
			payload: b"welcome".to_vec(),
			preview_text: Some("Welcome card".to_string()),
		},
	)]);
}

#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
	truapi::chat::on_messages_query(|request, sink| async move {
		if request.room_id != ROOM {
			return Err(CallError::Domain(ProductChatQueryError::UnknownRoom));
		}
		let page_size = request.page_size as usize;
		let messages = MESSAGES.with_borrow(|messages| {
			let skip = match request.anchor {
				ChatPageAnchor::Before { .. } => messages.len().saturating_sub(page_size),
				ChatPageAnchor::After { .. } => 0,
			};
			messages.iter().skip(skip).take(page_size).cloned().collect()
		});
		sink.send(ProductChatMessagesQueryItem { messages, next: None });
		Ok(())
	});
	truapi::chat::on_user_action(|request, sink| async move {
		if request.room_id != ROOM {
			return Err(CallError::Domain(ProductChatUserActionError::UnknownRoom));
		}
		let ChatUserAction::NewMessage { body, reply_to } = request.action else {
			sink.send(ProductChatUserActionItem::Acknowledged);
			return Ok(());
		};
		let text = match &body {
			ChatMessageBody::Text { text } => text.clone(),
			_ => String::new(),
		};
		let stored = ChatMessage { reply_to, ..message("user", body) };
		let note = message("product", ChatMessageBody::Text { text: format!("noted: {text}") });
		MESSAGES.with_borrow_mut(|messages| messages.extend([stored.clone(), note]));
		truapi::log!("stored {}: {text}", stored.message_id);
		sink.send(ProductChatUserActionItem::MessageCreated(stored));
		Ok(())
	});
	truapi::renderer::on_render(|request, sink| async move {
		let RenderContext::ChatMessage { message_id, .. } = request.context else {
			return Err(CallError::Unsupported);
		};
		let _stopped = Stopped(format!("render {message_id} stopped"));
		let payload = String::from_utf8_lossy(&request.payload).into_owned();
		sink.send(RendererNode::String { text: format!("{CARD}: {payload}") });
		truapi::log!("render {message_id} drawn");
		futures::future::pending().await
	});
	truapi::raw::on_call("chat_participants_query", participants);

	truapi::chat::create_room(HostChatCreateRoomRequest {
		room_id: ROOM.to_string(),
		metadata: ChatRoomMetadata {
			name: "Notes".to_string(),
			avatar: None,
			description: Some("A wasm worker keeps the messages".to_string()),
		},
		config: ChatRoomConfig { product_source: true, ..ChatRoomConfig::default() },
		preview: None,
	})
	.await?;
	truapi::log!("room {ROOM} ready");
	futures::future::pending().await
}

/// Answer that the room has two participants: the user and the product.
async fn participants(request: Vec<u8>, sink: RawSink) -> Vec<u8> {
	type End = Result<(), CallError<VersionedQueryError>>;
	let request = match ProductChatParticipantsQueryRequest::decode(&mut &request[..]) {
		Ok(request) => request,
		Err(error) => {
			return End::Err(CallError::MalformedFrame { reason: error.to_string() }).encode();
		},
	};
	let count = truapi::latest::ProductChatParticipantsQueryItem::Count { count: 2 };
	sink.send(&ProductChatParticipantsQueryItem::from_latest(count, request.version()).encode());
	End::Ok(()).encode()
}

/// A new message of `participant` at the time of the host clock.
fn message(participant: &str, body: ChatMessageBody) -> ChatMessage {
	let number = NEXT_MESSAGE.replace(NEXT_MESSAGE.get() + 1);
	ChatMessage {
		message_id: format!("n{number}"),
		origin: ChatMessageOrigin { participant_id: participant.to_string() },
		timestamp: truapi::time::now_unix_millis(),
		body,
		status: ChatMessageStatus::Sent,
		reply_to: None,
		edited_at: None,
		reactions: Vec::new(),
	}
}

/// Logs its line when the task that holds it is dropped.
struct Stopped(String);

impl Drop for Stopped {
	fn drop(&mut self) {
		truapi::log(&self.0);
	}
}
