//! Creates the room `actions`, a room that the host stores. The room has the
//! command `/ping` and a message with the buttons `yes` and `no`. The worker
//! answers each event of the room with one message:
//! - a text message: `echo: <text>`;
//! - a press on a button: `pressed <action> on <message>`;
//! - a command: `command /<name> <arguments>`.
//!
//! The worker runs until the host stops it.

use futures::StreamExt;
use truapi::latest::{
	ChatAction, ChatActionLayout, ChatActions, ChatCommandSpec, ChatEvent, ChatMessageBody,
	ChatMessageContent, ChatRoomConfig, ChatRoomMetadata, HostChatCreateRoomRequest,
	HostChatPostMessageRequest, HostChatSetRoomCommandsRequest,
};

const ROOM: &str = "actions";

#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
	match truapi::account::get_user_id(()).await {
		Ok(user) => truapi::log!("user id: {}", user.primary_username),
		Err(error) => truapi::log!("no user id: {error:?}"),
	}
	truapi::chat::create_room(HostChatCreateRoomRequest {
		room_id: ROOM.to_string(),
		metadata: ChatRoomMetadata {
			name: "Actions".to_string(),
			avatar: None,
			description: Some("A wasm worker answers each event".to_string()),
		},
		config: ChatRoomConfig::default(),
		preview: None,
	})
	.await?;
	truapi::chat::set_room_commands(HostChatSetRoomCommandsRequest {
		room_id: ROOM.to_string(),
		commands: vec![ChatCommandSpec {
			name: "ping".to_string(),
			description: "The worker answers with the arguments".to_string(),
			argument_hint: Some("<text>".to_string()),
		}],
	})
	.await?;
	let menu = post(ChatMessageContent::Actions(ChatActions {
		text: Some("Press a button".to_string()),
		actions: ["yes", "no"]
			.map(|id| ChatAction { action_id: id.to_string(), title: id.to_uppercase() })
			.to_vec(),
		layout: ChatActionLayout::Grid,
	}))
	.await?;
	truapi::log!("room {ROOM} ready, menu {menu}");

	let mut events = truapi::chat::action_subscribe(());
	while let Some(item) = events.next().await {
		let item = item?;
		let reply = match item.event {
			ChatEvent::MessagePosted { body: ChatMessageBody::Text { text }, .. } => {
				format!("echo: {text}")
			},
			ChatEvent::ActionTriggered(trigger) => {
				format!("pressed {} on {}", trigger.action_id, trigger.message_id)
			},
			ChatEvent::Command(command) => {
				format!("command /{} {}", command.command, command.payload)
			},
			_ => continue,
		};
		truapi::log!("{reply}");
		post(ChatMessageContent::Text { text: reply }).await?;
	}
	Err("the chat action subscription ended".into())
}

/// Post `payload` in the room, and give the id of the message.
async fn post(payload: ChatMessageContent) -> Result<String, truapi::Error> {
	let posted = truapi::chat::post_message(HostChatPostMessageRequest {
		room_id: ROOM.to_string(),
		payload,
		alt: None,
	})
	.await?;
	Ok(posted.message_id)
}
