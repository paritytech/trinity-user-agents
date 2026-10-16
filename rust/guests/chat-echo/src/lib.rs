//! Creates the room `echo`, a room the host stores, and answers each text
//! message the user posts there with the same text. A message that starts
//! with `/statement` is also submitted to the statement store with the
//! product's allowance, and the answer says whether the network took it. A
//! message `/wake <text>` makes the topic of `/statement <text>` the wake
//! watch of the room, and the answer says how a new statement there reaches
//! the worker. It runs until the host stops it, and writes a heartbeat line
//! to the host log every [`HEARTBEAT`], so a log shows when the worker runs.

use core::time::Duration;

use futures::StreamExt;
use truapi::latest::{
	ChatEvent, ChatMessageBody, ChatMessageContent, ChatRoomConfig, ChatRoomMetadata,
	HostChatCreateRoomRequest, HostChatPostMessageRequest, HostWorkerSetWakeWatchesRequest,
	SignedStatement, Statement, WorkerWakeWatch,
};

const ROOM: &str = "echo";

/// The time between two heartbeat lines.
const HEARTBEAT: Duration = Duration::from_secs(5);

/// 2027-12-28 in Unix seconds, in the high 32 bits as the statement store reads
/// an expiry. The worker has no clock, so the expiry is fixed.
const EXPIRY: u64 = 1_830_000_000 << 32;

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
	truapi::spawn(heartbeat());

	let mut actions = truapi::chat::action_subscribe(());
	while let Some(action) = actions.next().await {
		let action = action?;
		let ChatEvent::MessagePosted { body: ChatMessageBody::Text { text }, .. } = action.event
		else {
			continue;
		};
		let answer = if text.starts_with("/statement") {
			format!("echo: {text} -> {}", submit(&text).await)
		} else if let Some(rest) = text.strip_prefix("/wake") {
			format!("echo: {text} -> {}", watch(&format!("/statement{rest}")).await)
		} else {
			format!("echo: {text}")
		};
		truapi::chat::post_message(HostChatPostMessageRequest {
			room_id: action.room_id,
			payload: ChatMessageContent::Text { text: answer },
			alt: None,
		})
		.await?;
		truapi::log!("echoed {} bytes", text.len());
	}
	Err("the chat action subscription ended".into())
}

/// Write one line to the host log every [`HEARTBEAT`], with the count of lines
/// and the time since the worker loaded, until the host stops the worker.
async fn heartbeat() {
	let mut beat = 0u64;
	loop {
		truapi::time::sleep(HEARTBEAT).await;
		beat += 1;
		let seconds = truapi::time::monotonic_millis() / 1_000;
		truapi::log!("heartbeat {beat} after {seconds} s");
	}
}

/// The topic of `text`: the text, cut or padded to 32 bytes.
fn topic_of(text: &str) -> [u8; 32] {
	let mut topic = [0u8; 32];
	let bytes = text.as_bytes();
	let length = bytes.len().min(topic.len());
	topic[..length].copy_from_slice(&bytes[..length]);
	topic
}

/// Make the topic of `text` the one wake watch of the room.
async fn watch(text: &str) -> String {
	let request = HostWorkerSetWakeWatchesRequest {
		watches: vec![WorkerWakeWatch {
			id: ROOM.to_string(),
			room_id: ROOM.to_string(),
			topics: vec![topic_of(text)],
		}],
	};
	match truapi::worker::set_wake_watches(request).await {
		Ok(response) => format!("{:?}", response.delivery),
		Err(error) => format!("wake watch refused: {error:?}"),
	}
}

/// Prove `text` with the allowance of the product and submit it. The topic is
/// [`topic_of`] the text.
async fn submit(text: &str) -> String {
	let topic = topic_of(text);
	let bytes = text.as_bytes();
	let statement = Statement {
		proof: None,
		decryption_key: None,
		expiry: Some(EXPIRY),
		channel: None,
		topics: vec![topic],
		data: Some(bytes.to_vec()),
	};
	let proof = match truapi::statement_store::create_proof_authorized(statement.clone()).await {
		Ok(response) => response.proof,
		Err(error) => return format!("proof refused: {error:?}"),
	};
	let signed = SignedStatement {
		proof,
		decryption_key: statement.decryption_key,
		expiry: statement.expiry,
		channel: statement.channel,
		topics: statement.topics,
		data: statement.data,
	};
	match truapi::statement_store::submit(signed).await {
		Ok(_) => "submitted".to_string(),
		Err(error) => format!("submit refused: {error:?}"),
	}
}
