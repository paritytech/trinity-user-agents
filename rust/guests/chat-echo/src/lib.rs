//! Creates the room `echo`, a room the host stores, and answers each text
//! message the user posts there with the same text. It runs until the host
//! stops it.

use futures::StreamExt;
use truapi::latest::{
	ChatEvent, ChatMessageBody, ChatMessageContent, ChatRoomConfig, ChatRoomMetadata,
	HostChatCreateRoomRequest, HostChatPostMessageRequest,
};

const ROOM: &str = "echo";

#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
	truapi::chat::create_room(HostChatCreateRoomRequest {
		room_id: ROOM.to_string(),
		metadata: ChatRoomMetadata {
			name: "Echo".to_string(),
			avatar: None,
			description: Some("A wasm worker answers each message".to_string()),
		},
		config: ChatRoomConfig::default(),
		preview: None,
	})
	.await?;
	truapi::log!("room {ROOM} ready");

	let mut actions = truapi::chat::action_subscribe(());
	while let Some(action) = actions.next().await {
		let action = action?;
		let ChatEvent::MessagePosted { body: ChatMessageBody::Text { text }, .. } = action.event
		else {
			continue;
		};
		truapi::chat::post_message(HostChatPostMessageRequest {
			room_id: action.room_id,
			payload: ChatMessageContent::Text { text: format!("echo: {text}") },
			alt: None,
		})
		.await?;
		truapi::log!("echoed {} bytes", text.len());
	}
	Err("the chat action subscription ended".into())
}
