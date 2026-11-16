use super::*;
use crate::versioned::Versioned;
use parity_scale_codec::{Decode, Encode};

#[test]
fn render_item_string_matches_the_wire_fixture() {
	let item = ProductRendererRenderItem::V1(v01::RendererNode::String { text: "Votes: 1".into() });
	assert_eq!(hex::encode(item.encode()), "000120566f7465733a2031");
}

#[test]
fn render_request_carries_the_chat_context() {
	let request = ProductRendererRenderRequest::V1(v01::ProductRendererRenderRequest {
		context: v01::RenderContext::ChatMessage {
			room_id: "room".into(),
			message_id: "message-1".into(),
			message_type: "vote".into(),
		},
		payload: vec![1, 2],
	});
	let bytes = request.encode();
	// V1 envelope, ChatMessage variant, then the three strings and payload.
	assert_eq!(hex::encode(&bytes), "000010726f6f6d246d6573736167652d3110766f7465080102");
	assert_eq!(ProductRendererRenderRequest::decode(&mut bytes.as_slice()).unwrap(), request);
}

#[test]
fn action_item_button_payload_is_empty() {
	let item = HostRendererActionSubscribeItem::V1(v01::HostRendererActionSubscribeItem {
		context: v01::RenderContext::PocketCard { card_id: "loyalty".into() },
		action_id: "vote".into(),
		payload: Vec::new(),
	});
	let bytes = item.encode();
	// V1 envelope, PocketCard variant, the card and action ids, then the
	// empty payload's length prefix.
	assert_eq!(hex::encode(&bytes), "00021c6c6f79616c747910766f746500");
	assert_eq!(HostRendererActionSubscribeItem::decode(&mut bytes.as_slice()).unwrap(), item);
}

#[test]
fn a_chat_message_context_still_renders_as_v1() {
	let request = v02::ProductRendererRenderRequest {
		context: v02::RenderContext::ChatMessage {
			room_id: "room".into(),
			message_id: "message-1".into(),
			message_type: "vote".into(),
		},
		payload: vec![1, 2],
	};

	assert_eq!(ProductRendererRenderRequest::from_latest(request, 1).version(), 1);
}

#[test]
fn footer_render_request_matches_the_wire_fixture() {
	let request = ProductRendererRenderRequest::V2(v02::ProductRendererRenderRequest {
		context: v02::RenderContext::ChatRoomFooter {
			room_id: "room".into(),
			message_type: "register".into(),
		},
		payload: vec![1],
	});
	let bytes = request.encode();
	assert_eq!(hex::encode(&bytes), "010410726f6f6d2072656769737465720401");
	assert_eq!(ProductRendererRenderRequest::decode(&mut bytes.as_slice()).unwrap(), request);
}

// Hosts and products built at different times must agree on this byte, so the
// back is appended after PocketCard and never renumbers it.
#[test]
fn pocket_card_back_is_the_variant_after_pocket_card() {
	let item = HostRendererActionSubscribeItem::V1(v01::HostRendererActionSubscribeItem {
		context: v01::RenderContext::PocketCardBack { card_id: "loyalty".into() },
		action_id: "vote".into(),
		payload: Vec::new(),
	});
	let bytes = item.encode();
	assert_eq!(hex::encode(&bytes), "00031c6c6f79616c747910766f746500");
	assert_eq!(HostRendererActionSubscribeItem::decode(&mut bytes.as_slice()).unwrap(), item);
}

#[test]
fn a_pocket_card_back_render_request_reaches_a_v1_product_as_v1() {
	let latest = v02::ProductRendererRenderRequest {
		context: v02::RenderContext::PocketCardBack { card_id: "loyalty".into() },
		payload: vec![1],
	};
	let sent = ProductRendererRenderRequest::from_latest(latest.clone(), 1);

	assert_eq!(
		sent,
		ProductRendererRenderRequest::V1(v01::ProductRendererRenderRequest {
			context: v01::RenderContext::PocketCardBack { card_id: "loyalty".into() },
			payload: vec![1],
		})
	);
	assert_eq!(sent.into_latest(), latest);
}

fn failure(context: v02::RenderContext) -> v02::HostRendererFailureSubscribeItem {
	v02::HostRendererFailureSubscribeItem {
		context,
		node: 2,
		material: Some([7; 32]),
		reason: v01::MaterialFailure::NotFound,
	}
}

/// A product that speaks v0.1 still hears which card node fell back, and
/// reading the report back gives the same report.
#[test]
fn a_card_failure_reaches_a_v1_product_as_v1() {
	let latest = failure(v02::RenderContext::PocketCard { card_id: "loyalty".into() });
	let sent = HostRendererFailureSubscribeItem::from_latest(latest.clone(), 1);

	assert_eq!(sent.version(), 1);
	assert_eq!(sent.into_latest(), latest);
}

/// v0.1 has no footer context, and only a product that spoke v0.2 docks a
/// footer, so a footer's failure stays v0.2.
#[test]
fn a_footer_failure_stays_v2() {
	let latest = failure(v02::RenderContext::ChatRoomFooter {
		room_id: "room".into(),
		message_type: "register".into(),
	});

	assert_eq!(HostRendererFailureSubscribeItem::from_latest(latest, 1).version(), 2);
}
