//! Versioned wrappers for [`Renderer`](crate::api::Renderer) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum ProductRendererRenderRequest { V1 => v01::ProductRendererRenderRequest }
    pub enum ProductRendererRenderItem { V1 => v01::RendererNode }
    pub enum ProductRendererRenderError { V1 => v01::GenericError }
    pub enum HostRendererActionSubscribeRequest { V1 }
    pub enum HostRendererActionSubscribeItem { V1 => v01::HostRendererActionSubscribeItem }
    pub enum HostRendererActionSubscribeError { V1 => v01::GenericError }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};
    use parity_scale_codec::{Decode, Encode};

    #[test]
    fn render_item_string_matches_the_wire_fixture() {
        let item = ProductRendererRenderItem::V1(v01::RendererNode::String {
            text: "Votes: 1".into(),
        });
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
        assert_eq!(
            hex::encode(&bytes),
            "000010726f6f6d246d6573736167652d3110766f7465080102"
        );
        assert_eq!(
            ProductRendererRenderRequest::decode(&mut bytes.as_slice()).unwrap(),
            request
        );
    }

    #[test]
    fn action_item_button_payload_is_empty() {
        let item = HostRendererActionSubscribeItem::V1(v01::HostRendererActionSubscribeItem {
            context: v01::RenderContext::PocketCard {
                card_id: "loyalty".into(),
            },
            action_id: "vote".into(),
            payload: Vec::new(),
        });
        let bytes = item.encode();
        // V1 envelope, PocketCard variant, the card and action ids, then the
        // empty payload's length prefix.
        assert_eq!(hex::encode(&bytes), "00021c6c6f79616c747910766f746500");
        assert_eq!(
            HostRendererActionSubscribeItem::decode(&mut bytes.as_slice()).unwrap(),
            item
        );
    }
}
