//! Kettle's events, as plain objects for the worker.
//!
//! Each has a `type`. Accounts and groups are `0x` hex. A text message is a
//! `message`; any other record is a `record` with its kind only, since the
//! worker shows text alone.

use js_sys::{Array, Object};
use kettle_chat::ChatEvent;
use kettle_proto::wire::v6::{
	Record,
	rfc2::{ChatMessageBody, ChatUserAction},
};
use wasm_bindgen::JsValue;

use crate::ports::{hex_of, set};

fn accounts(list: &[[u8; 32]]) -> Array {
	list.iter().map(|account| JsValue::from(hex_of(account))).collect()
}

/// The first word of a value's debug form: its variant's name.
fn variant(value: &impl core::fmt::Debug) -> String {
	let text = format!("{value:?}");
	text.split(|c: char| !c.is_alphanumeric())
		.next()
		.unwrap_or_default()
		.to_string()
}

pub fn event_to_js(event: &ChatEvent) -> JsValue {
	let object = Object::new();
	let kind = |name: &str| set(&object, "type", name);
	match event {
		ChatEvent::Created { group, direct } => {
			kind("created");
			set(&object, "group", hex_of(group));
			set(
				&object,
				"direct",
				direct.map(|peer| JsValue::from(hex_of(&peer))).unwrap_or(JsValue::NULL),
			);
		},
		ChatEvent::Joined { group, committer, members, epoch, direct } => {
			kind("joined");
			set(&object, "group", hex_of(group));
			set(&object, "committer", hex_of(committer));
			set(&object, "members", accounts(members));
			set(&object, "epoch", *epoch as f64);
			set(&object, "direct", *direct);
		},
		ChatEvent::Restored { group, committer, members, epoch, direct, active } => {
			kind("restored");
			set(&object, "group", hex_of(group));
			set(&object, "committer", hex_of(committer));
			set(&object, "members", accounts(members));
			set(&object, "epoch", *epoch as f64);
			set(
				&object,
				"direct",
				direct.map(|peer| JsValue::from(hex_of(&peer))).unwrap_or(JsValue::NULL),
			);
			set(&object, "active", *active);
		},
		ChatEvent::Record { group, sender, direct, record, .. } => {
			set(&object, "group", hex_of(group));
			set(&object, "sender", hex_of(sender));
			set(&object, "direct", *direct);
			match record {
				Record::Action(ChatUserAction::NewMessage {
					body: ChatMessageBody::Text { text },
					message_id,
					timestamp,
					..
				}) => {
					kind("message");
					set(&object, "text", text.as_str());
					if let Some(id) = message_id {
						set(&object, "id", id.as_str());
					}
					if let Some(at) = timestamp {
						set(&object, "at", *at as f64);
					}
				},
				Record::Action(action) => {
					kind("record");
					set(&object, "kind", variant(action));
				},
				Record::RoomMetadata(_) => {
					kind("record");
					set(&object, "kind", "RoomMetadata");
				},
			}
		},
		ChatEvent::Unknown { group, sender, kind: record_kind, newer, .. } => {
			kind("unknown");
			set(&object, "group", hex_of(group));
			set(&object, "sender", hex_of(sender));
			set(&object, "kind", *record_kind as f64);
			set(&object, "newer", *newer);
		},
		ChatEvent::Published { group, count } => {
			kind("published");
			set(&object, "group", hex_of(group));
			set(&object, "count", *count as f64);
		},
		ChatEvent::Acked { group, member, count } => {
			kind("acked");
			set(&object, "group", hex_of(group));
			set(&object, "member", hex_of(member));
			set(&object, "count", *count as f64);
		},
		ChatEvent::Delivered { group, count } => {
			kind("delivered");
			set(&object, "group", hex_of(group));
			set(&object, "count", *count as f64);
		},
		ChatEvent::WaitingForKeys { group, account } => {
			kind("waitingForKeys");
			set(&object, "group", hex_of(group));
			set(&object, "account", hex_of(account));
		},
		ChatEvent::KeysFound { account } => {
			kind("keysFound");
			set(&object, "account", hex_of(account));
		},
		ChatEvent::Rejected { group, rejection } => {
			kind("rejected");
			set(
				&object,
				"group",
				group.map(|group| JsValue::from(hex_of(&group))).unwrap_or(JsValue::NULL),
			);
			set(&object, "reason", format!("{rejection:?}"));
		},
		ChatEvent::MembersChanged { group, epoch, members } => {
			kind("membersChanged");
			set(&object, "group", hex_of(group));
			set(&object, "epoch", *epoch as f64);
			set(&object, "members", accounts(members));
		},
		ChatEvent::Removed { group } => {
			kind("removed");
			set(&object, "group", hex_of(group));
		},
		ChatEvent::Left { group } => {
			kind("left");
			set(&object, "group", hex_of(group));
		},
		ChatEvent::Moved { peer, from, to, requeued, .. } => {
			kind("moved");
			set(&object, "peer", hex_of(peer));
			set(&object, "from", hex_of(from));
			set(&object, "to", hex_of(to));
			set(&object, "requeued", *requeued as f64);
		},
		ChatEvent::Error { group, kind: error_kind, message } => {
			kind("error");
			set(
				&object,
				"group",
				group.map(|group| JsValue::from(hex_of(&group))).unwrap_or(JsValue::NULL),
			);
			set(&object, "kind", format!("{error_kind:?}"));
			set(&object, "message", message.as_str());
		},
		other => {
			kind("other");
			set(&object, "kind", variant(other));
		},
	}
	object.into()
}
