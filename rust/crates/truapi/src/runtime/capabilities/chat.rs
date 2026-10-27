//! Product-facing chat capability adapter.

use futures::StreamExt;
use tracing::{instrument, warn};
use truapi::{
	CallContext, CallError, Subscription,
	api::Chat,
	latest,
	versioned::{
		IntoLatest,
		chat::{
			HostChatActionSubscribeError, HostChatActionSubscribeItem,
			HostChatActionSubscribeRequest, HostChatCreateRoomError, HostChatCreateRoomRequest,
			HostChatCreateRoomResponse, HostChatListSubscribeError, HostChatListSubscribeItem,
			HostChatListSubscribeRequest, HostChatPostMessageError, HostChatPostMessageRequest,
			HostChatPostMessageResponse, HostChatRegisterBotError, HostChatRegisterBotRequest,
			HostChatRegisterBotResponse, HostChatSetRoomFooterError, HostChatSetRoomFooterRequest,
			HostChatSetRoomFooterResponse,
		},
	},
};

use crate::{
	platform::{
		ChatFieldError, normalize_chat_identifier, validate_chat_icon,
		validate_chat_message_content, validate_chat_name,
	},
	runtime::{ProductRuntimeHost, versioning::version},
};

#[crate::platform::async_trait]
impl Chat for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "chat.create_room"))]
	async fn create_room(
		&self,
		_cx: &CallContext,
		request: HostChatCreateRoomRequest,
	) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				request.room_id = normalize_chat_identifier("roomId", &request.room_id)
					.map_err(chat_create_room_field_error)?;
				request.name = validate_chat_name("name", &request.name)
					.map_err(chat_create_room_field_error)?;
				request.icon = validate_chat_icon("icon", &request.icon)
					.map_err(chat_create_room_field_error)?;
				platform
					.create_chat_room(&self.connection.product, request)
					.await
					.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.register_bot"))]
	async fn register_bot(
		&self,
		_cx: &CallContext,
		request: HostChatRegisterBotRequest,
	) -> Result<HostChatRegisterBotResponse, CallError<HostChatRegisterBotError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				request.bot_id = normalize_chat_identifier("botId", &request.bot_id)
					.map_err(chat_register_bot_field_error)?;
				request.name = validate_chat_name("name", &request.name)
					.map_err(chat_register_bot_field_error)?;
				request.icon = validate_chat_icon("icon", &request.icon)
					.map_err(chat_register_bot_field_error)?;
				platform
					.register_chat_bot(&self.connection.product, request)
					.await
					.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.list_subscribe"))]
	async fn list_subscribe(
		&self,
		_cx: &CallContext,
		request: HostChatListSubscribeRequest,
	) -> Subscription<HostChatListSubscribeItem, CallError<HostChatListSubscribeError>> {
		version(request)
			.exec(|()| async move {
				let platform = match self.connection.chat_platform::<latest::GenericError>() {
					Ok(platform) => platform,
					Err(error) => return Subscription::interrupted(error),
				};
				Subscription::new(platform.subscribe_chat_rooms(&self.connection.product).map(
					|item| {
						item.map_err(|error| {
							warn!(
								reason = %error.reason,
								"chat room list platform stream failed"
							);
							CallError::HostFailure { reason: error.reason }
						})
					},
				))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.post_message"))]
	async fn post_message(
		&self,
		_cx: &CallContext,
		request: HostChatPostMessageRequest,
	) -> Result<HostChatPostMessageResponse, CallError<HostChatPostMessageError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				// The same normalization create_room applied, so a product's own
				// spelling of a room id still resolves to the stored room.
				request.room_id = normalize_chat_identifier("roomId", &request.room_id)
					.map_err(chat_post_field_error)?;
				// Content is product-authored and host-rendered, so it gets the same
				// treatment the room fields get rather than reaching a host raw.
				request.payload = validate_chat_message_content(request.payload)
					.map_err(chat_post_field_error)?;
				request.alt = request
					.alt
					.map(|alt| validate_chat_name("alt", &alt))
					.transpose()
					.map_err(chat_post_field_error)?
					.filter(|alt| !alt.is_empty())
					.filter(|_| {
						matches!(request.payload, truapi::latest::ChatMessageContent::Custom(_))
					});
				platform
					.post_chat_message(&self.connection.product, request)
					.await
					.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_footer"))]
	async fn set_room_footer(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomFooterRequest,
	) -> Result<HostChatSetRoomFooterResponse, CallError<HostChatSetRoomFooterError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				let failure = |reason: String| {
					CallError::Domain(latest::HostChatSetRoomFooterError::Unknown { reason })
				};
				request.room_id = normalize_chat_identifier("roomId", &request.room_id)
					.map_err(|error| failure(error.to_string()))?;

				// Checked here rather than by each host, so every host answers a room
				// the product never created the same way.
				let rooms =
					match platform.subscribe_chat_rooms(&self.connection.product).next().await {
						Some(Ok(item)) => item.rooms,
						Some(Err(error)) => return Err(failure(error.reason)),
						None => return Err(failure("the host published no room list".to_string())),
					};
				let created = rooms.iter().any(|room| {
					room.room_id == request.room_id &&
						room.participating_as == latest::ChatRoomParticipation::RoomHost
				});
				if !created {
					return Err(CallError::Domain(latest::HostChatSetRoomFooterError::UnknownRoom));
				}

				platform
					.set_chat_room_footer(&self.connection.product, request)
					.await
					.map_err(|error| failure(error.reason))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.action_subscribe"))]
	async fn action_subscribe(
		&self,
		_cx: &CallContext,
		request: HostChatActionSubscribeRequest,
	) -> Subscription<HostChatActionSubscribeItem, CallError<HostChatActionSubscribeError>> {
		version(request)
			.exec(|()| async move {
				if let Err(error) = self.connection.chat_platform::<latest::GenericError>() {
					return Subscription::interrupted(error);
				}
				Subscription::new(
					self.connection.chat.subscribe().map(|item| item.map(IntoLatest::into_latest)),
				)
			})
			.await
	}
}

/// Report a rejected chat bot field as a bot-registration domain error.
fn chat_register_bot_field_error(
	error: crate::platform::ChatFieldError,
) -> CallError<latest::HostChatRegisterBotError> {
	CallError::Domain(latest::HostChatRegisterBotError::Unknown { reason: error.to_string() })
}

/// Fields whose rejection a product resolves by sending less content, and the
/// only ones the fieldless `MessageTooLarge` can describe.
const CHAT_SIZED_CONTENT_FIELDS: [&str; 2] = ["text", "payload"];

/// Maps a rejected `post_message` field onto the wire error, reporting a size
/// rejection as the size variant the protocol declares for it.
fn chat_post_field_error(error: ChatFieldError) -> CallError<latest::HostChatPostMessageError> {
	let payload = match error {
		// `MessageTooLarge` names no field, so it is reserved for the content
		// a product shrinks by sending less. Every other rejection reports
		// through `Unknown`, whose reason names the field and its limit.
		ChatFieldError::TooLong { field, .. } if CHAT_SIZED_CONTENT_FIELDS.contains(&field) => {
			latest::HostChatPostMessageError::MessageTooLarge
		},
		error => latest::HostChatPostMessageError::Unknown { reason: error.to_string() },
	};
	CallError::Domain(payload)
}

/// Report a rejected chat room field as a room-creation domain error.
fn chat_create_room_field_error(
	error: crate::platform::ChatFieldError,
) -> CallError<latest::HostChatCreateRoomError> {
	CallError::Domain(latest::HostChatCreateRoomError::Unknown { reason: error.to_string() })
}
