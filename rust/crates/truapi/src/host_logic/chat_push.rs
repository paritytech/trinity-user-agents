//! What a host shows for a push of the person-to-person chat while no chat
//! worker runs: the native `NotificationMessagePayload`, sealed with
//! ChaCha20-Poly1305 under the key of the contact's push route.

use parity_scale_codec::{Compact, Decode, Input};

use crate::host_logic::hop;

/// What a chat push shows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ChatPushPreview {
	/// A message, with the text of its notification.
	Message {
		/// The id of the message.
		message_id: String,
		/// The text of the notification.
		text: String,
	},
	/// A call offer, which the host rings for.
	Call {
		/// The id of the offer.
		message_id: String,
		/// The offer is for a video call.
		video: bool,
	},
}

/// The preview of a chat push `message` sealed under `key`, or `None` when it
/// does not open with the key or carries nothing to show.
pub fn decode_chat_push(key: &[u8; 32], message: &[u8]) -> Option<ChatPushPreview> {
	let payload = hop::open(key, message)?;
	let mut input = payload.as_slice();
	let message_id = String::decode(&mut input).ok()?;
	u64::decode(&mut input).ok()?;
	if u8::decode(&mut input).ok()? != 0 {
		return None;
	}
	// Stripped (0) and Full (1) keep the content's index for every variant shown.
	if u8::decode(&mut input).ok()? > 1 {
		return None;
	}
	let text = match u8::decode(&mut input).ok()? {
		0 => String::decode(&mut input).ok()?,
		4 => {
			String::decode(&mut input).ok()?;
			format!("reacted with {}", String::decode(&mut input).ok()?)
		},
		7 => {
			String::decode(&mut input).ok()?;
			rich_text(&mut input)?
		},
		8 => {
			Vec::<u8>::decode(&mut input).ok()?;
			let video = u8::decode(&mut input).ok()? == 1;
			return Some(ChatPushPreview::Call { message_id, video });
		},
		14 | 20 => "approved the chat request".to_string(),
		15 => rich_text(&mut input)?,
		_ => return None,
	};
	Some(ChatPushPreview::Message { message_id, text })
}

/// The text of rich content, or the kind of its first attachment.
fn rich_text(input: &mut &[u8]) -> Option<String> {
	if let Some(text) = Option::<String>::decode(input).ok()? {
		return Some(text);
	}
	if u8::decode(input).ok()? == 0 {
		return None;
	}
	if Compact::<u32>::decode(input).ok()?.0 == 0 || u8::decode(input).ok()? != 0 {
		return None;
	}
	Vec::<u8>::decode(input).ok()?;
	Vec::<u8>::decode(input).ok()?;
	u8::decode(input).ok()?;
	String::decode(input).ok()?;
	let kind = match input.read_byte().ok()? {
		1 => "Photo",
		2 => "Video",
		_ => "File",
	};
	Some(kind.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;

	// Payloads from the chat product's own encoder (`encodeNotificationPayload`).
	fn sealed(payload_hex: &str) -> Vec<u8> {
		hop::seal(&[9; 32], &hex::decode(payload_hex).unwrap()).unwrap()
	}

	fn preview(payload_hex: &str) -> Option<ChatPushPreview> {
		decode_chat_push(&[9; 32], &sealed(payload_hex))
	}

	fn message(text: &str) -> Option<ChatPushPreview> {
		Some(ChatPushPreview::Message { message_id: "id-1".to_string(), text: text.to_string() })
	}

	#[test]
	fn a_push_shows_what_the_native_chat_shows() {
		assert_eq!(
			preview("1069642d3107000000000000000001002c68656c6c6f207468657265"),
			message("hello there")
		);
		assert_eq!(
			preview("1069642d310700000000000000000104086d3110f09f918d"),
			message("reacted with 👍")
		);
		assert_eq!(
			preview("1069642d31070000000000000000010e087231"),
			message("approved the chat request")
		);
		assert_eq!(
			preview("1069642d3107000000000000000001080c763d3001"),
			Some(ChatPushPreview::Call { message_id: "id-1".to_string(), video: true })
		);
		let photo = "1069642d31070000000000000000010f00010400800101010101010101010101010101010101010101010101010101010101010101800202020202020202020202020202020202020202020202020202020202020202\
			00447773733a2f2f686f702e6578616d706c650128696d6167652f6a7065670a000000010000000100000000";
		assert_eq!(preview(photo), message("Photo"));
	}

	#[test]
	fn a_push_that_does_not_open_shows_nothing() {
		let sealed = sealed("1069642d3107000000000000000001002c68656c6c6f207468657265");
		assert_eq!(decode_chat_push(&[8; 32], &sealed), None);
		assert_eq!(decode_chat_push(&[9; 32], &sealed[..20]), None);
	}
}
