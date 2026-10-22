//! A chat host for the attachment staging tests, which keeps the features
//! of each room.

use std::{collections::BTreeMap, sync::Mutex};

use super::*;
use crate::platform::ChatPlatform;
use futures::{StreamExt, stream::BoxStream};
use truapi::latest;

/// A chat host that hosts every room it is asked about and keeps the
/// features of each.
#[derive(Default)]
pub struct FeatureHost {
	/// The features of each room.
	pub features: Mutex<BTreeMap<String, latest::ChatRoomFeatures>>,
}

#[crate::platform::async_trait]
impl ChatPlatform for FeatureHost {
	async fn create_chat_room(
		&self,
		_product: &ProductContext,
		_request: latest::HostChatCreateRoomRequest,
	) -> Result<latest::HostChatCreateRoomResponse, latest::HostChatCreateRoomError> {
		unreachable!("the staging tests create no room")
	}

	async fn register_chat_bot(
		&self,
		_product: &ProductContext,
		_request: latest::HostChatRegisterBotRequest,
	) -> Result<latest::HostChatRegisterBotResponse, latest::HostChatRegisterBotError> {
		unreachable!("the staging tests register no bot")
	}

	async fn post_chat_message(
		&self,
		_product: &ProductContext,
		_request: latest::HostChatPostMessageRequest,
	) -> Result<latest::HostChatPostMessageResponse, latest::HostChatPostMessageError> {
		unreachable!("the staging tests post no message")
	}

	async fn set_chat_room_footer(
		&self,
		_product: &ProductContext,
		_request: latest::HostChatSetRoomFooterRequest,
	) -> Result<(), latest::GenericError> {
		unreachable!("the staging tests set no footer")
	}

	fn subscribe_chat_rooms(
		&self,
		_product: &ProductContext,
	) -> BoxStream<'static, Result<latest::HostChatListSubscribeItem, latest::GenericError>> {
		let rooms = ["main", "other"]
			.map(|room_id| latest::ChatRoom {
				room_id: room_id.to_string(),
				participating_as: latest::ChatRoomParticipation::RoomHost,
			})
			.to_vec();
		futures::stream::iter([Ok(latest::HostChatListSubscribeItem { rooms })]).boxed()
	}

	async fn read_chat_room_features(
		&self,
		_product: &ProductContext,
		room_id: String,
	) -> Result<latest::ChatRoomFeatures, latest::GenericError> {
		Ok(self.features.lock().unwrap().get(&room_id).cloned().unwrap_or_default())
	}

	async fn set_chat_room_features(
		&self,
		_product: &ProductContext,
		request: latest::HostChatSetRoomFeaturesRequest,
	) -> Result<(), latest::GenericError> {
		self.features.lock().unwrap().insert(request.room_id, request.features);
		Ok(())
	}

	async fn delete_chat_room(
		&self,
		_product: &ProductContext,
		request: latest::HostChatDeleteRoomRequest,
	) -> Result<(), latest::GenericError> {
		self.features.lock().unwrap().remove(&request.room_id);
		Ok(())
	}
}
