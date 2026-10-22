//! What a host shows for a push of the person-to-person chat while no chat
//! worker runs: the native `NotificationMessagePayload`, sealed with
//! ChaCha20-Poly1305 under the key of the contact's push route.

use parity_scale_codec::{Compact, Decode};

use crate::host_logic::hop;

/// What a chat push shows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ChatPushPreview {
	/// A message, which the host words as its own chat words that kind.
	Message {
		/// The id of the message.
		message_id: String,
		/// What the message is.
		content: ChatPushContent,
	},
	/// A call offer, which the host rings for.
	Call {
		/// The id of the offer.
		message_id: String,
		/// The offer is for a video call.
		video: bool,
	},
}

/// The kind of a message a chat push carries, with what the host needs to
/// word its notification. The username of the contact comes from the host.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ChatPushContent {
	/// Plain text.
	Text {
		/// The text.
		text: String,
	},
	/// Text with attachments.
	RichText {
		/// The text or caption.
		text: Option<String>,
		/// The kind of each attachment, in order.
		media: Vec<ChatPushMediaKind>,
	},
	/// A reply to an earlier message.
	Reply {
		/// The text or caption of the reply.
		text: Option<String>,
		/// The kind of each attachment of the reply, in order.
		media: Vec<ChatPushMediaKind>,
	},
	/// A new text for an earlier message.
	Edited {
		/// The new text or caption.
		text: Option<String>,
	},
	/// The contact reacted to a message.
	Reaction {
		/// The emoji.
		emoji: String,
	},
	/// The contact removed a reaction.
	ReactionRemoved,
	/// The contact added the user.
	ContactAdded,
	/// The contact accepted the chat request of the user.
	RequestAccepted,
	/// The contact left the chat.
	LeftChat,
	/// The contact sent a payment.
	Payment {
		/// The amount, in the smallest unit.
		amount: u128,
		/// `true`: a transfer of the earlier chat, which the host words as its
		/// legacy transfer.
		legacy: bool,
	},
	/// A message this version of the core cannot read.
	Unsupported,
}

/// The kind of one attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ChatPushMediaKind {
	/// A still image.
	Image,
	/// A video.
	Video,
	/// Any other file.
	File,
}

/// The preview of a chat push `message` sealed under `key`, or `None` when it
/// does not open with the key or carries a message that the native chat shows
/// no notification for.
pub fn decode_chat_push(key: &[u8; 32], message: &[u8]) -> Option<ChatPushPreview> {
	let payload = hop::open(key, message)?;
	let mut input = payload.as_slice();
	let message_id = String::decode(&mut input).ok()?;
	u64::decode(&mut input).ok()?;
	if u8::decode(&mut input).ok()? != 0 {
		return None;
	}
	// Stripped (0) keeps the index of each content it carries, and drops the
	// coin keys of a payment, which come last.
	if u8::decode(&mut input).ok()? > 1 {
		return None;
	}
	let input = &mut input;
	let content = match u8::decode(input).ok()? {
		0 => ChatPushContent::Text { text: String::decode(input).ok()? },
		2 => ChatPushContent::Payment {
			amount: Compact::<u128>::decode(input).ok()?.0,
			legacy: true,
		},
		3 => ChatPushContent::ContactAdded,
		4 => {
			String::decode(input).ok()?;
			ChatPushContent::Reaction { emoji: String::decode(input).ok()? }
		},
		5 => ChatPushContent::ReactionRemoved,
		7 => {
			String::decode(input).ok()?;
			let (text, media) = rich_text(input)?;
			ChatPushContent::Reply { text, media }
		},
		8 => {
			Vec::<u8>::decode(input).ok()?;
			let video = u8::decode(input).ok()? == 1;
			return Some(ChatPushPreview::Call { message_id, video });
		},
		12 => {
			String::decode(input).ok()?;
			ChatPushContent::Edited { text: rich_text(input)?.0 }
		},
		13 => ChatPushContent::LeftChat,
		14 | 20 => ChatPushContent::RequestAccepted,
		15 => {
			let (text, media) = rich_text(input)?;
			ChatPushContent::RichText { text, media }
		},
		16 => ChatPushContent::Payment {
			amount: Compact::<u128>::decode(input).ok()?.0,
			legacy: false,
		},
		1 | 9 | 10 | 11 | 17 | 18 | 19 => return None,
		_ => ChatPushContent::Unsupported,
	};
	Some(ChatPushPreview::Message { message_id, content })
}

/// The text and the kinds of the attachments of rich content.
fn rich_text(input: &mut &[u8]) -> Option<(Option<String>, Vec<ChatPushMediaKind>)> {
	let text = Option::<String>::decode(input).ok()?;
	let count = match u8::decode(input).ok()? {
		0 => 0,
		1 => Compact::<u32>::decode(input).ok()?.0,
		_ => return None,
	};
	let media = (0..count).map(|_| attachment_kind(input)).collect::<Option<_>>()?;
	Some((text, media))
}

/// The kind of one `FileVariant::P2PMixnet` attachment, read whole.
fn attachment_kind(input: &mut &[u8]) -> Option<ChatPushMediaKind> {
	if u8::decode(input).ok()? != 0 {
		return None;
	}
	Vec::<u8>::decode(input).ok()?;
	Vec::<u8>::decode(input).ok()?;
	if u8::decode(input).ok()? != 0 {
		return None;
	}
	String::decode(input).ok()?;
	let kind = u8::decode(input).ok()?;
	String::decode(input).ok()?;
	u32::decode(input).ok()?;
	Some(match kind {
		0 => ChatPushMediaKind::File,
		1 => {
			<(u32, u32, Option<Vec<u8>>)>::decode(input).ok()?;
			ChatPushMediaKind::Image
		},
		2 => {
			<(u32, Option<Vec<u8>>)>::decode(input).ok()?;
			ChatPushMediaKind::Video
		},
		_ => return None,
	})
}

#[cfg(test)]
mod tests {
	use parity_scale_codec::Encode;

	use super::*;

	// Payloads from the chat product's own encoder (`encodeNotificationPayload`).
	fn sealed(payload_hex: &str) -> Vec<u8> {
		hop::seal(&[9; 32], &hex::decode(payload_hex).unwrap()).unwrap()
	}

	fn preview(payload_hex: &str) -> Option<ChatPushPreview> {
		decode_chat_push(&[9; 32], &sealed(payload_hex))
	}

	/// The preview of a Full payload of id `id-1` whose content is `content`,
	/// its index first.
	fn full(content: &[u8]) -> Option<ChatPushPreview> {
		let mut payload = ("id-1", 7u64, 0u8, 1u8).encode();
		payload.extend_from_slice(content);
		preview(&hex::encode(payload))
	}

	fn message(content: ChatPushContent) -> Option<ChatPushPreview> {
		Some(ChatPushPreview::Message { message_id: "id-1".to_string(), content })
	}

	/// A `FileVariant::P2PMixnet` with the meta `meta`, its index first.
	fn attachment(meta: &[u8]) -> Vec<u8> {
		let mut file = (0u8, vec![1u8; 32], vec![2u8; 32], 0u8, "wss://hop.example").encode();
		file.extend_from_slice(meta);
		file
	}

	fn rich(text: Option<&str>, files: &[Vec<u8>]) -> Vec<u8> {
		let mut rich = text.encode();
		if files.is_empty() {
			rich.push(0);
		} else {
			rich.push(1);
			rich.extend(Compact(files.len() as u32).encode());
			files.iter().for_each(|file| rich.extend_from_slice(file));
		}
		rich
	}

	#[test]
	fn a_push_carries_what_the_native_chat_words() {
		assert_eq!(
			preview("1069642d3107000000000000000001002c68656c6c6f207468657265"),
			message(ChatPushContent::Text { text: "hello there".to_string() })
		);
		assert_eq!(
			preview("1069642d310700000000000000000104086d3110f09f918d"),
			message(ChatPushContent::Reaction { emoji: "👍".to_string() })
		);
		assert_eq!(
			preview("1069642d31070000000000000000010e087231"),
			message(ChatPushContent::RequestAccepted)
		);
		assert_eq!(
			preview("1069642d3107000000000000000001080c763d3001"),
			Some(ChatPushPreview::Call { message_id: "id-1".to_string(), video: true })
		);
		let photo = "1069642d31070000000000000000010f00010400800101010101010101010101010101010101010101010101010101010101010101800202020202020202020202020202020202020202020202020202020202020202\
			00447773733a2f2f686f702e6578616d706c650128696d6167652f6a7065670a000000010000000100000000";
		assert_eq!(
			preview(photo),
			message(ChatPushContent::RichText {
				text: None,
				media: vec![ChatPushMediaKind::Image]
			})
		);
	}

	#[test]
	fn rich_content_names_the_kind_of_each_attachment() {
		let file = attachment(&(0u8, "application/pdf", 9u32).encode());
		let video = attachment(&(2u8, "video/mp4", 9u32, 4_000u32, None::<Vec<u8>>).encode());
		let image =
			attachment(&(1u8, "image/png", 9u32, 1u32, 1u32, Some(b"LKO2".to_vec())).encode());
		let mut reply = vec![7];
		reply.extend("m0".encode());
		reply.extend(rich(Some("look"), &[video.clone(), file.clone()]));
		let mut rich_text = vec![15];
		rich_text.extend(rich(None, &[image, video]));
		let mut edited = vec![12];
		edited.extend("m0".encode());
		edited.extend(rich(Some("fixed"), &[]));

		assert_eq!(
			full(&reply),
			message(ChatPushContent::Reply {
				text: Some("look".to_string()),
				media: vec![ChatPushMediaKind::Video, ChatPushMediaKind::File],
			})
		);
		assert_eq!(
			full(&rich_text),
			message(ChatPushContent::RichText {
				text: None,
				media: vec![ChatPushMediaKind::Image, ChatPushMediaKind::Video],
			})
		);
		assert_eq!(
			full(&edited),
			message(ChatPushContent::Edited { text: Some("fixed".to_string()) })
		);
	}

	// A native sender pushes these kinds too; the product pushes the payment.
	#[test]
	fn the_other_kinds_of_the_native_chat_come_through() {
		let mut payment = vec![16];
		payment.extend((Compact(1_500_000u128), vec![vec![3u8; 32]]).encode());
		let mut legacy = vec![2];
		legacy.extend((Compact(25u128), [4u8; 32], [5u8; 32]).encode());
		let mut removed = vec![5];
		removed.extend(("m0", "👍").encode());

		assert_eq!(full(&[3]), message(ChatPushContent::ContactAdded));
		assert_eq!(full(&[13]), message(ChatPushContent::LeftChat));
		assert_eq!(full(&removed), message(ChatPushContent::ReactionRemoved));
		assert_eq!(
			full(&payment),
			message(ChatPushContent::Payment { amount: 1_500_000, legacy: false })
		);
		assert_eq!(full(&legacy), message(ChatPushContent::Payment { amount: 25, legacy: true }));
		assert_eq!(full(&[21]), message(ChatPushContent::Unsupported));
	}

	// The Stripped payment has no coin keys.
	#[test]
	fn a_stripped_payment_carries_its_amount() {
		let mut payload = ("id-1", 7u64, 0u8, 0u8, 16u8).encode();
		payload.extend(Compact(42u128).encode());

		assert_eq!(
			preview(&hex::encode(payload)),
			message(ChatPushContent::Payment { amount: 42, legacy: false })
		);
	}

	// The native chat shows nothing for the messages that carry the protocol.
	#[test]
	fn a_protocol_message_shows_nothing() {
		let mut answer = vec![9];
		answer.extend(("offer", vec![1u8]).encode());

		assert_eq!(full(&answer), None);
		assert_eq!(full(&[1, 4, 1, 2, 0]), None);
	}

	#[test]
	fn a_push_that_does_not_open_shows_nothing() {
		let sealed = sealed("1069642d3107000000000000000001002c68656c6c6f207468657265");
		assert_eq!(decode_chat_push(&[8; 32], &sealed), None);
		assert_eq!(decode_chat_push(&[9; 32], &sealed[..20]), None);
	}
}
