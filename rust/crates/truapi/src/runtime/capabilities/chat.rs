//! Product-facing chat capability adapters.
//!
//! The core screens every chat call before a host sees it: identifiers and
//! texts are normalized and bounded, a room-changing call needs a room the
//! product hosts, and feed and preview calls are paced per room.

use std::sync::Arc;

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
			HostChatCreateRoomResponse, HostChatDeleteRoomError, HostChatDeleteRoomRequest,
			HostChatDeleteRoomResponse, HostChatListSubscribeError, HostChatListSubscribeItem,
			HostChatListSubscribeRequest, HostChatPostMessageError, HostChatPostMessageRequest,
			HostChatPostMessageResponse, HostChatRegisterBotError, HostChatRegisterBotRequest,
			HostChatRegisterBotResponse, HostChatSetRoomConfigError, HostChatSetRoomConfigRequest,
			HostChatSetRoomConfigResponse, HostChatSetRoomFooterError,
			HostChatSetRoomFooterRequest, HostChatSetRoomFooterResponse,
			HostChatSetRoomMetadataError, HostChatSetRoomMetadataRequest,
			HostChatSetRoomMetadataResponse, HostChatSetRoomPreviewError,
			HostChatSetRoomPreviewRequest, HostChatSetRoomPreviewResponse, HostChatUpdateFeedError,
			HostChatUpdateFeedRequest, HostChatUpdateFeedResponse,
		},
	},
};

use crate::{
	platform::{
		ChatFieldError, ChatPlatform, normalize_chat_identifier, validate_chat_feed_update,
		validate_chat_icon, validate_chat_message_content, validate_chat_name,
		validate_chat_room_metadata, validate_chat_room_preview,
	},
	runtime::{ChatHandler, ProductRuntimeHost, chat::ChatRateLimiter, versioning::version},
	unix_time::current_unix_millis,
};

/// The participant id of the host's user, which a product cannot write as.
const USER_PARTICIPANT_ID: &str = "user";

/// Fields whose rejection a product resolves by sending less content, and the
/// only ones the fieldless `MessageTooLarge` can describe. `payload` is the
/// payload of a first-version `Custom` message.
const CHAT_SIZED_CONTENT_FIELDS: [&str; 2] = ["text", "payload"];

#[truapi::async_trait]
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
				let field_error = |error: ChatFieldError| {
					CallError::Domain(latest::HostChatCreateRoomError::Unknown {
						reason: error.to_string(),
					})
				};
				request.room_id =
					normalize_chat_identifier("roomId", &request.room_id).map_err(field_error)?;
				request.name = validate_chat_name("name", &request.name).map_err(field_error)?;
				request.icon = validate_chat_icon("icon", &request.icon).map_err(field_error)?;
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
				let field_error = |error: ChatFieldError| {
					CallError::Domain(latest::HostChatRegisterBotError::Unknown {
						reason: error.to_string(),
					})
				};
				request.bot_id =
					normalize_chat_identifier("botId", &request.bot_id).map_err(field_error)?;
				request.name = validate_chat_name("name", &request.name).map_err(field_error)?;
				request.icon = validate_chat_icon("icon", &request.icon).map_err(field_error)?;
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
							warn!(reason = %error.reason, "chat room list platform stream failed");
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
				// Apply the normalization of `create_room`, so each spelling of
				// a room id finds the stored room.
				request.room_id = normalize_chat_identifier("roomId", &request.room_id)
					.map_err(post_field_error)?;
				// The product writes the content and the host shows it. Thus
				// the core screens the content as it screens the room fields.
				request.payload =
					validate_chat_message_content(request.payload).map_err(post_field_error)?;
				request.alt = request
					.alt
					.map(|alt| validate_chat_name("alt", &alt))
					.transpose()
					.map_err(post_field_error)?
					.filter(|alt| !alt.is_empty())
					.filter(|_| matches!(request.payload, latest::ChatMessageContent::Custom(_)));
				let config = platform
					.read_chat_room_config(&self.connection.product, request.room_id.clone())
					.await
					.map_err(|error| {
						CallError::Domain(latest::HostChatPostMessageError::Unknown {
							reason: error.reason,
						})
					})?;
				if config.product_source {
					return Err(CallError::Domain(latest::HostChatPostMessageError::Unknown {
						reason: "the room keeps its messages in the product; use updateFeed"
							.to_string(),
					}));
				}
				platform
					.post_chat_message(&self.connection.product, request)
					.await
					.map_err(CallError::Domain)
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

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_footer"))]
	async fn set_room_footer(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomFooterRequest,
	) -> Result<HostChatSetRoomFooterResponse, CallError<HostChatSetRoomFooterError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				request.room_id = self.hosted_room(&platform, &request.room_id).await?;
				platform.set_chat_room_footer(&self.connection.product, request).await.map_err(
					|error| {
						CallError::Domain(latest::HostChatSetRoomFooterError::Unknown {
							reason: error.reason,
						})
					},
				)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_config"))]
	async fn set_room_config(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomConfigRequest,
	) -> Result<HostChatSetRoomConfigResponse, CallError<HostChatSetRoomConfigError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				request.room_id = self.hosted_room(&platform, &request.room_id).await?;
				platform
					.set_chat_room_config(&self.connection.product, request)
					.await
					.map_err(room_host_error)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_metadata"))]
	async fn set_room_metadata(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomMetadataRequest,
	) -> Result<HostChatSetRoomMetadataResponse, CallError<HostChatSetRoomMetadataError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				request.metadata =
					validate_chat_room_metadata(request.metadata).map_err(room_field_error)?;
				request.room_id = self.hosted_room(&platform, &request.room_id).await?;
				platform
					.set_chat_room_metadata(&self.connection.product, request)
					.await
					.map_err(room_host_error)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_preview"))]
	async fn set_room_preview(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomPreviewRequest,
	) -> Result<HostChatSetRoomPreviewResponse, CallError<HostChatSetRoomPreviewError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				request.preview =
					validate_chat_room_preview(request.preview).map_err(room_field_error)?;
				request.room_id = self.hosted_room(&platform, &request.room_id).await?;
				let limiter = &self.connection.services.chat_rate_limits.preview;
				if !self.take_room_budget(limiter, &request.room_id) {
					return Err(CallError::Domain(
						latest::HostChatSetRoomPreviewError::RateLimited,
					));
				}
				platform
					.set_chat_room_preview(&self.connection.product, request)
					.await
					.map_err(room_host_error)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.delete_room"))]
	async fn delete_room(
		&self,
		_cx: &CallContext,
		request: HostChatDeleteRoomRequest,
	) -> Result<HostChatDeleteRoomResponse, CallError<HostChatDeleteRoomError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				match self.hosted_room(&platform, &request.room_id).await {
					// A delete of a room that does not exist succeeds. Thus a
					// product can send a delete again when it lost the answer.
					Err(RoomRefusal::UnknownRoom) => Ok(()),
					Err(refusal) => Err(CallError::Domain(refusal.into())),
					Ok(room_id) => {
						request.room_id = room_id.clone();
						platform
							.delete_chat_room(&self.connection.product, request)
							.await
							.map_err(room_host_error)?;
						// Forget the budgets only after the host deletes the
						// room. A room that stays keeps the budget that it
						// spent.
						let limits = &self.connection.services.chat_rate_limits;
						let product_id = self.connection.product.product_id.as_str();
						limits.feed.forget(product_id, &room_id);
						limits.preview.forget(product_id, &room_id);
						Ok(())
					},
				}
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "chat.update_feed"))]
	async fn update_feed(
		&self,
		_cx: &CallContext,
		request: HostChatUpdateFeedRequest,
	) -> Result<HostChatUpdateFeedResponse, CallError<HostChatUpdateFeedError>> {
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				request.update =
					validate_chat_feed_update(request.update).map_err(feed_field_error)?;
				request.room_id = self.hosted_room(&platform, &request.room_id).await?;
				let limiter = &self.connection.services.chat_rate_limits.feed;
				if !self.take_room_budget(limiter, &request.room_id) {
					return Err(CallError::Domain(latest::HostChatUpdateFeedError::RateLimited));
				}
				let config = platform
					.read_chat_room_config(&self.connection.product, request.room_id.clone())
					.await
					.map_err(|error| {
						CallError::Domain(latest::HostChatUpdateFeedError::Unknown {
							reason: error.reason,
						})
					})?;
				let persist = !config.product_source;
				// In a room that the host stores, the user writes and reacts
				// only through the host. A product cannot write or react as the
				// user.
				if persist && names_the_user(&request.update) {
					return Err(CallError::Domain(latest::HostChatUpdateFeedError::NotOwner));
				}
				platform
					.update_chat_feed(&self.connection.product, request, persist)
					.await
					.map_err(CallError::Domain)
			})
			.await
	}
}

/// Why the core refuses a room-changing call before the host sees it.
#[derive(Debug)]
enum RoomRefusal {
	/// The product created no room with this id.
	UnknownRoom,
	/// The product takes part in the room as a bot.
	NotRoomHost,
	/// The id was malformed, or the host could not list the rooms.
	Failed(String),
}

impl From<RoomRefusal> for latest::HostChatSetRoomConfigError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom => Self::UnknownRoom,
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl From<RoomRefusal> for latest::HostChatSetRoomMetadataError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom => Self::UnknownRoom,
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl From<RoomRefusal> for latest::HostChatSetRoomPreviewError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom => Self::UnknownRoom,
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl From<RoomRefusal> for latest::HostChatDeleteRoomError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			// A delete of a room that does not exist succeeds, so the core does
			// not use this arm.
			RoomRefusal::UnknownRoom => Self::Unknown { reason: "unknown room".to_string() },
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl From<RoomRefusal> for latest::HostChatUpdateFeedError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom => Self::UnknownRoom,
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl From<RoomRefusal> for latest::HostChatSetRoomFooterError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			// The footer error has no bot variant: a room the product does not
			// host is, for the footer, a room it did not create.
			RoomRefusal::UnknownRoom | RoomRefusal::NotRoomHost => Self::UnknownRoom,
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl<E: From<RoomRefusal>> From<RoomRefusal> for CallError<E> {
	fn from(refusal: RoomRefusal) -> Self {
		CallError::Domain(refusal.into())
	}
}

/// Whether `update` names the host's user as the author of a message or a
/// reaction.
fn names_the_user(update: &latest::ChatFeedUpdate) -> bool {
	let participant_id = match update {
		latest::ChatFeedUpdate::New(message) | latest::ChatFeedUpdate::Replaced(message) => {
			&message.origin.participant_id
		},
		latest::ChatFeedUpdate::ReactionPlaced { participant_id, .. } |
		latest::ChatFeedUpdate::ReactionRemoved { participant_id, .. } => participant_id,
		latest::ChatFeedUpdate::Edited { .. } |
		latest::ChatFeedUpdate::StatusChanged { .. } |
		latest::ChatFeedUpdate::Deleted { .. } => return false,
	};
	participant_id == USER_PARTICIPANT_ID
}

/// Report a rejected field of a room-changing call.
fn room_field_error<E: From<RoomRefusal>>(error: ChatFieldError) -> CallError<E> {
	RoomRefusal::Failed(error.to_string()).into()
}

/// Report a host failure of a room-changing call.
fn room_host_error<E: From<RoomRefusal>>(error: latest::GenericError) -> CallError<E> {
	RoomRefusal::Failed(error.reason).into()
}

/// Report a rejected feed field, a size rejection of the content as the size
/// variant the protocol declares for it.
fn feed_field_error(error: ChatFieldError) -> CallError<latest::HostChatUpdateFeedError> {
	CallError::Domain(match error {
		ChatFieldError::TooLong { field, .. } if CHAT_SIZED_CONTENT_FIELDS.contains(&field) => {
			latest::HostChatUpdateFeedError::MessageTooLarge
		},
		error => latest::HostChatUpdateFeedError::Unknown { reason: error.to_string() },
	})
}

/// Report a rejected `post_message` field, a size rejection of the content as
/// the size variant the protocol declares for it.
fn post_field_error(error: ChatFieldError) -> CallError<latest::HostChatPostMessageError> {
	CallError::Domain(match error {
		// `MessageTooLarge` names no field, so it is reserved for the content
		// a product shrinks by sending less. Every other rejection reports
		// through `Unknown`, whose reason names the field and its limit.
		ChatFieldError::TooLong { field, .. } if CHAT_SIZED_CONTENT_FIELDS.contains(&field) => {
			latest::HostChatPostMessageError::MessageTooLarge
		},
		error => latest::HostChatPostMessageError::Unknown { reason: error.to_string() },
	})
}

impl ProductRuntimeHost {
	/// Whether the declarations the host keeps for `room_id` declare
	/// `handler`. The core asks a product a query only when they do.
	pub async fn chat_handler_declared(
		&self,
		room_id: String,
		handler: ChatHandler,
	) -> Result<bool, String> {
		let platform = self.connection.native_chat_platform().map_err(|error| error.to_string())?;
		let declared = match handler {
			ChatHandler::Config(declared) => declared,
			ChatHandler::Undeclared => return Ok(true),
		};
		let config = platform
			.read_chat_room_config(&self.connection.product, room_id.clone())
			.await
			.map_err(|error| error.reason)?;
		Ok(declared(&config))
	}

	/// The normalized id of a room this product hosts.
	///
	/// The room list is read once for each call, so every host answers a room
	/// the product never created, or joined only as a bot, the same way.
	async fn hosted_room(
		&self,
		platform: &Arc<dyn ChatPlatform>,
		room_id: &str,
	) -> Result<String, RoomRefusal> {
		let room_id = normalize_chat_identifier("roomId", room_id)
			.map_err(|error| RoomRefusal::Failed(error.to_string()))?;
		let rooms = platform
			.list_chat_rooms(&self.connection.product)
			.await
			.map_err(|error| RoomRefusal::Failed(error.reason))?
			.rooms;
		match rooms.iter().find(|room| room.room_id == room_id) {
			None => Err(RoomRefusal::UnknownRoom),
			Some(room) if room.participating_as != latest::ChatRoomParticipation::RoomHost => {
				Err(RoomRefusal::NotRoomHost)
			},
			Some(_) => Ok(room_id),
		}
	}

	/// Spend one call of the budget of `room_id` in `limiter`. Returns `false`
	/// when the room has no budget left.
	fn take_room_budget(&self, limiter: &ChatRateLimiter, room_id: &str) -> bool {
		limiter.try_acquire(
			self.connection.product.product_id.as_str(),
			room_id,
			current_unix_millis(),
		)
	}
}

#[cfg(test)]
mod tests {
	use std::{collections::BTreeMap, sync::Mutex};

	use futures::{executor::block_on, stream::BoxStream};
	use truapi::v01;

	use super::*;
	use crate::{
		platform::{
			CHAT_FEED_UPDATES_PER_SECOND, CHAT_PREVIEW_UPDATES_PER_SECOND, ProductContext,
			ProductExecutionKind, ProductId,
		},
		runtime::{PairingHostRole, RuntimeServices},
		test_support::{runtime_config, session_info, stub_platform, test_spawner},
	};

	/// A chat host that keeps rooms and configurations in memory and records
	/// what the core handed it.
	#[derive(Default)]
	struct RecordingHost {
		rooms: Mutex<BTreeMap<String, latest::ChatRoomParticipation>>,
		configs: Mutex<BTreeMap<String, latest::ChatRoomConfig>>,
		icons: Mutex<Vec<String>>,
		feed: Mutex<Vec<(latest::HostChatUpdateFeedRequest, bool)>>,
		calls: Mutex<Vec<String>>,
		/// The failure reason of `list_chat_rooms`, if any.
		room_list_failure: Mutex<Option<String>>,
		/// The failure reason of `delete_chat_room`, if any.
		delete_failure: Mutex<Option<String>>,
	}

	impl RecordingHost {
		fn with_rooms(rooms: &[(&str, latest::ChatRoomParticipation)]) -> Arc<Self> {
			let host = Self::default();
			*host.rooms.lock().unwrap() = rooms
				.iter()
				.map(|(id, participation)| (id.to_string(), *participation))
				.collect();
			Arc::new(host)
		}

		fn record(&self, call: &str, room_id: &str) -> Result<(), latest::GenericError> {
			self.calls.lock().unwrap().push(format!("{call} {room_id}"));
			Ok(())
		}

		fn calls(&self) -> Vec<String> {
			self.calls.lock().unwrap().clone()
		}

		fn room_list(&self) -> latest::HostChatListSubscribeItem {
			let rooms = self
				.rooms
				.lock()
				.unwrap()
				.iter()
				.map(|(room_id, participating_as)| latest::ChatRoom {
					room_id: room_id.clone(),
					participating_as: *participating_as,
				})
				.collect();
			latest::HostChatListSubscribeItem { rooms }
		}
	}

	#[truapi::async_trait]
	impl ChatPlatform for RecordingHost {
		async fn create_chat_room(
			&self,
			_product: &ProductContext,
			request: latest::HostChatCreateRoomRequest,
		) -> Result<latest::HostChatCreateRoomResponse, latest::HostChatCreateRoomError> {
			self.icons.lock().unwrap().push(request.icon);
			self.rooms
				.lock()
				.unwrap()
				.insert(request.room_id, latest::ChatRoomParticipation::RoomHost);
			Ok(latest::HostChatCreateRoomResponse {
				status: latest::ChatRoomRegistrationStatus::New,
			})
		}

		async fn register_chat_bot(
			&self,
			_product: &ProductContext,
			_request: latest::HostChatRegisterBotRequest,
		) -> Result<latest::HostChatRegisterBotResponse, latest::HostChatRegisterBotError> {
			Ok(latest::HostChatRegisterBotResponse {
				status: latest::ChatBotRegistrationStatus::New,
			})
		}

		async fn post_chat_message(
			&self,
			_product: &ProductContext,
			request: latest::HostChatPostMessageRequest,
		) -> Result<latest::HostChatPostMessageResponse, latest::HostChatPostMessageError> {
			let _ = self.record("post", &request.room_id);
			Ok(latest::HostChatPostMessageResponse { message_id: "m1".to_string() })
		}

		async fn set_chat_room_footer(
			&self,
			_product: &ProductContext,
			request: latest::HostChatSetRoomFooterRequest,
		) -> Result<(), latest::GenericError> {
			self.record("footer", &request.room_id)
		}

		fn subscribe_chat_rooms(
			&self,
			_product: &ProductContext,
		) -> BoxStream<'static, Result<latest::HostChatListSubscribeItem, latest::GenericError>> {
			futures::stream::iter([Ok(self.room_list())]).boxed()
		}

		async fn list_chat_rooms(
			&self,
			_product: &ProductContext,
		) -> Result<latest::HostChatListSubscribeItem, latest::GenericError> {
			match self.room_list_failure.lock().unwrap().clone() {
				Some(reason) => Err(latest::GenericError { reason }),
				None => Ok(self.room_list()),
			}
		}

		async fn read_chat_room_config(
			&self,
			_product: &ProductContext,
			room_id: String,
		) -> Result<latest::ChatRoomConfig, latest::GenericError> {
			Ok(self.configs.lock().unwrap().get(&room_id).cloned().unwrap_or_default())
		}

		async fn set_chat_room_config(
			&self,
			_product: &ProductContext,
			request: latest::HostChatSetRoomConfigRequest,
		) -> Result<(), latest::GenericError> {
			self.configs.lock().unwrap().insert(request.room_id.clone(), request.config);
			self.record("config", &request.room_id)
		}

		async fn set_chat_room_preview(
			&self,
			_product: &ProductContext,
			request: latest::HostChatSetRoomPreviewRequest,
		) -> Result<(), latest::GenericError> {
			self.record("preview", &request.room_id)
		}

		async fn delete_chat_room(
			&self,
			_product: &ProductContext,
			request: latest::HostChatDeleteRoomRequest,
		) -> Result<(), latest::GenericError> {
			if let Some(reason) = self.delete_failure.lock().unwrap().clone() {
				return Err(latest::GenericError { reason });
			}
			self.rooms.lock().unwrap().remove(&request.room_id);
			self.record("delete", &request.room_id)
		}

		async fn update_chat_feed(
			&self,
			_product: &ProductContext,
			request: latest::HostChatUpdateFeedRequest,
			persist: bool,
		) -> Result<(), latest::HostChatUpdateFeedError> {
			self.feed.lock().unwrap().push((request, persist));
			Ok(())
		}
	}

	fn chat_runtime(chat: Arc<RecordingHost>) -> ProductRuntimeHost {
		chat_runtime_on(stub_platform(), chat)
	}

	/// [`chat_runtime`] over `platform`, so that a test reads what the core
	/// handed the host platform.
	fn chat_runtime_on(
		platform: Arc<crate::test_support::StubPlatform>,
		chat: Arc<RecordingHost>,
	) -> ProductRuntimeHost {
		chat_runtime_of("chat.dot", platform, chat)
	}

	/// [`chat_runtime_on`] for the product `product_id`.
	fn chat_runtime_of(
		product_id: &str,
		platform: Arc<crate::test_support::StubPlatform>,
		chat: Arc<RecordingHost>,
	) -> ProductRuntimeHost {
		let (host_config, _) = runtime_config(product_id);
		let product = ProductContext::new_with_execution(
			ProductId::for_test(product_id),
			ProductExecutionKind::Worker,
		);
		let services = RuntimeServices::new(
			platform,
			host_config.host.host_info.clone(),
			host_config.people_chain_genesis_hash,
			host_config.bulletin_chain_genesis_hash,
			host_config.asset_hub_chain_genesis_hash,
			test_spawner(),
		);
		let pairing_host = PairingHostRole::new(services.clone(), host_config);
		let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
		adapters.chat_platform = Some(chat);
		let host = ProductRuntimeHost::from_services(
			services,
			adapters,
			pairing_host.clone(),
			pairing_host.session().clone(),
			product,
		);
		host.test_session_state().set_session(session_info());
		host
	}

	fn hosted(room_id: &str) -> (&str, latest::ChatRoomParticipation) {
		(room_id, latest::ChatRoomParticipation::RoomHost)
	}

	fn text_message(origin: &str) -> latest::ChatFeedUpdate {
		latest::ChatFeedUpdate::New(latest::ChatMessage {
			message_id: "m1".to_string(),
			origin: latest::ChatMessageOrigin { participant_id: origin.to_string() },
			timestamp: 1,
			body: latest::ChatMessageBody::Text { text: "hi".to_string() },
			status: latest::ChatMessageStatus::Sent,
			reply_to: None,
			reply_preview: None,
			edited_at: None,
			reactions: Vec::new(),
			request: false,
		})
	}

	fn update_feed(
		runtime: &ProductRuntimeHost,
		room_id: &str,
		update: latest::ChatFeedUpdate,
	) -> Result<HostChatUpdateFeedResponse, CallError<HostChatUpdateFeedError>> {
		block_on(Chat::update_feed(
			runtime,
			&CallContext::default(),
			HostChatUpdateFeedRequest::V1(latest::HostChatUpdateFeedRequest {
				room_id: room_id.to_string(),
				update,
			}),
		))
	}

	/// Stores `config` as the configuration the product declared for `room_id`.
	fn configure(chat: &RecordingHost, room_id: &str, config: latest::ChatRoomConfig) {
		chat.configs.lock().unwrap().insert(room_id.to_string(), config);
	}

	// The core answers a room the product does not host the same way for
	// every host, and the host never sees the call.
	#[test]
	fn a_room_change_needs_a_room_the_product_hosts() {
		let chat = RecordingHost::with_rooms(&[
			hosted("support"),
			("lobby", latest::ChatRoomParticipation::Bot),
		]);
		let runtime = chat_runtime(chat.clone());
		let set_config = |room_id: &str| {
			block_on(Chat::set_room_config(
				&runtime,
				&CallContext::default(),
				HostChatSetRoomConfigRequest::V1(latest::HostChatSetRoomConfigRequest {
					room_id: room_id.to_string(),
					config: Default::default(),
				}),
			))
		};

		assert_eq!(
			set_config("missing"),
			Err(CallError::Domain(HostChatSetRoomConfigError::V1(
				latest::HostChatSetRoomConfigError::UnknownRoom
			)))
		);
		assert_eq!(
			set_config("lobby"),
			Err(CallError::Domain(HostChatSetRoomConfigError::V1(
				latest::HostChatSetRoomConfigError::NotRoomHost
			)))
		);
		assert_eq!(set_config(" support "), Ok(HostChatSetRoomConfigResponse::V1));
		assert_eq!(chat.calls(), ["config support"]);
	}

	// A host-store room keeps what `update_feed` writes; a product-source room
	// only shows it, so the host must not store it.
	#[test]
	fn a_feed_update_is_stored_only_in_a_room_the_host_keeps() {
		let chat = RecordingHost::with_rooms(&[hosted("stored"), hosted("sourced")]);
		let runtime = chat_runtime(chat.clone());
		configure(
			&chat,
			"sourced",
			latest::ChatRoomConfig { product_source: true, ..Default::default() },
		);

		update_feed(&runtime, "stored", text_message("product")).expect("a stored update");
		update_feed(&runtime, "sourced", text_message("user")).expect("a shown update");

		let persisted = chat
			.feed
			.lock()
			.unwrap()
			.iter()
			.map(|(request, persist)| (request.room_id.clone(), *persist))
			.collect::<Vec<_>>();
		assert_eq!(persisted, [("stored".to_string(), true), ("sourced".to_string(), false)]);
	}

	/// Every update that names an author: a new message, a replaced message,
	/// and a reaction placed and removed.
	fn updates_by(participant_id: &str) -> [latest::ChatFeedUpdate; 4] {
		let latest::ChatFeedUpdate::New(message) = text_message(participant_id) else {
			unreachable!("text_message builds a new message")
		};
		let reaction = |placed: bool| {
			let message_id = "m1".to_string();
			let participant_id = participant_id.to_string();
			let emoji = "👍".to_string();
			if placed {
				latest::ChatFeedUpdate::ReactionPlaced { message_id, participant_id, emoji }
			} else {
				latest::ChatFeedUpdate::ReactionRemoved { message_id, participant_id, emoji }
			}
		};
		[
			latest::ChatFeedUpdate::New(message.clone()),
			latest::ChatFeedUpdate::Replaced(message),
			reaction(true),
			reaction(false),
		]
	}

	// In a room that the host stores, the user writes and reacts only through
	// the host. Thus no update from a product can name the user as its author.
	// Without this check, a host that trusts the core stores words that the
	// user did not write.
	#[test]
	fn a_product_cannot_write_as_the_user_in_a_room_the_host_keeps() {
		let chat = RecordingHost::with_rooms(&[hosted("stored")]);
		let runtime = chat_runtime(chat.clone());

		for update in updates_by("user") {
			assert_eq!(
				update_feed(&runtime, "stored", update),
				Err(CallError::Domain(HostChatUpdateFeedError::V1(
					latest::HostChatUpdateFeedError::NotOwner
				)))
			);
		}
		assert!(chat.feed.lock().unwrap().is_empty());
	}

	// In a room that the product keeps, the product reports all changes, also
	// the messages and reactions of the user. The host only shows them.
	// A deletion names no author, so the core sends it to the host. The host
	// refuses the deletion of a message that the product did not write.
	#[test]
	fn a_product_deletes_a_message_in_a_room_the_host_keeps() {
		let chat = RecordingHost::with_rooms(&[hosted("stored")]);
		let runtime = chat_runtime(chat.clone());

		let deleted = |id: &str| latest::ChatFeedUpdate::Deleted { message_id: id.to_string() };
		update_feed(&runtime, "stored", deleted(" m1 ")).expect("a stored deletion");

		let feed = chat.feed.lock().unwrap().clone();
		assert_eq!(
			feed.into_iter()
				.map(|(request, persist)| (request.update, persist))
				.collect::<Vec<_>>(),
			[(deleted("m1"), true)]
		);
	}

	#[test]
	fn a_room_the_product_keeps_shows_the_users_messages_and_reactions() {
		let chat = RecordingHost::with_rooms(&[hosted("sourced")]);
		let runtime = chat_runtime(chat.clone());
		configure(
			&chat,
			"sourced",
			latest::ChatRoomConfig { product_source: true, ..Default::default() },
		);

		for update in updates_by("user") {
			update_feed(&runtime, "sourced", update).expect("a shown update");
		}
		assert_eq!(chat.feed.lock().unwrap().len(), 4);
	}

	#[test]
	fn an_oversized_feed_update_is_too_large() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[hosted("stored")]));

		let update = latest::ChatFeedUpdate::Edited {
			message_id: "m1".to_string(),
			body: latest::ChatMessageBody::Text {
				text: "x".repeat(crate::platform::CHAT_BODY_MAX_BYTES + 1),
			},
			edited_at: 2,
		};

		assert_eq!(
			update_feed(&runtime, "stored", update),
			Err(CallError::Domain(HostChatUpdateFeedError::V1(
				latest::HostChatUpdateFeedError::MessageTooLarge
			)))
		);
	}

	// The core refuses the 21st update in one second, before the host sees it.
	#[test]
	fn the_feed_of_a_room_is_paced() {
		let chat = RecordingHost::with_rooms(&[hosted("stored")]);
		let runtime = chat_runtime(chat.clone());

		for _ in 0..CHAT_FEED_UPDATES_PER_SECOND {
			update_feed(&runtime, "stored", text_message("product")).expect("within budget");
		}
		assert_eq!(
			update_feed(&runtime, "stored", text_message("product")),
			Err(CallError::Domain(HostChatUpdateFeedError::V1(
				latest::HostChatUpdateFeedError::RateLimited
			)))
		);
		assert_eq!(chat.feed.lock().unwrap().len(), CHAT_FEED_UPDATES_PER_SECOND as usize);
	}

	// The core refuses the 6th preview in one second, before the host sees it.
	#[test]
	fn the_preview_of_a_room_is_paced() {
		let chat = RecordingHost::with_rooms(&[hosted("support")]);
		let runtime = chat_runtime(chat.clone());
		let set_preview = || {
			block_on(Chat::set_room_preview(
				&runtime,
				&CallContext::default(),
				HostChatSetRoomPreviewRequest::V1(latest::HostChatSetRoomPreviewRequest {
					room_id: "support".to_string(),
					preview: latest::ChatRoomPreview {
						timestamp: None,
						badge: None,
						last_message: None,
						last_reaction: None,
						unseen_reactions: 0,
						pinned: false,
						activity: None,
					},
				}),
			))
		};

		for _ in 0..CHAT_PREVIEW_UPDATES_PER_SECOND {
			assert_eq!(set_preview(), Ok(HostChatSetRoomPreviewResponse::V1));
		}
		assert_eq!(
			set_preview(),
			Err(CallError::Domain(HostChatSetRoomPreviewError::V1(
				latest::HostChatSetRoomPreviewError::RateLimited
			)))
		);
		assert_eq!(chat.calls().len(), CHAT_PREVIEW_UPDATES_PER_SECOND as usize);
	}

	#[test]
	fn a_room_the_product_keeps_takes_no_posted_message() {
		let chat = RecordingHost::with_rooms(&[hosted("sourced")]);
		let runtime = chat_runtime(chat.clone());
		configure(
			&chat,
			"sourced",
			latest::ChatRoomConfig { product_source: true, ..Default::default() },
		);

		let posted = block_on(Chat::post_message(
			&runtime,
			&CallContext::default(),
			HostChatPostMessageRequest::V1(v01::HostChatPostMessageRequest {
				room_id: "sourced".to_string(),
				payload: latest::ChatMessageContent::Text { text: "hi".to_string() },
			}),
		));

		assert!(matches!(
			posted,
			Err(CallError::Domain(HostChatPostMessageError::V1(
				latest::HostChatPostMessageError::Unknown { .. }
			)))
		));
		assert!(chat.calls().is_empty());
	}

	#[test]
	fn deleting_a_room_that_is_gone_succeeds() {
		let chat = RecordingHost::with_rooms(&[hosted("support")]);
		let runtime = chat_runtime(chat.clone());
		let delete = |room_id: &str| {
			block_on(Chat::delete_room(
				&runtime,
				&CallContext::default(),
				HostChatDeleteRoomRequest::V1(latest::HostChatDeleteRoomRequest {
					room_id: room_id.to_string(),
				}),
			))
		};

		assert_eq!(delete("support"), Ok(HostChatDeleteRoomResponse::V1));
		assert_eq!(delete("support"), Ok(HostChatDeleteRoomResponse::V1));
		assert_eq!(chat.calls(), ["delete support"]);
	}

	// If the host refuses a delete, the room stays and keeps the budget that it
	// spent. If not, a product can send a failed delete before each burst of
	// updates and send without limit.
	#[test]
	fn a_room_the_host_does_not_delete_keeps_its_spent_budget() {
		let chat = RecordingHost::with_rooms(&[hosted("stored")]);
		*chat.delete_failure.lock().unwrap() = Some("not served".to_string());
		let runtime = chat_runtime(chat.clone());

		for _ in 0..CHAT_FEED_UPDATES_PER_SECOND {
			update_feed(&runtime, "stored", text_message("product")).expect("within budget");
		}
		let deleted = block_on(Chat::delete_room(
			&runtime,
			&CallContext::default(),
			HostChatDeleteRoomRequest::V1(latest::HostChatDeleteRoomRequest {
				room_id: "stored".to_string(),
			}),
		));

		assert_eq!(
			deleted,
			Err(CallError::Domain(HostChatDeleteRoomError::V1(
				latest::HostChatDeleteRoomError::Unknown { reason: "not served".to_string() }
			)))
		);
		assert_eq!(
			update_feed(&runtime, "stored", text_message("product")),
			Err(CallError::Domain(HostChatUpdateFeedError::V1(
				latest::HostChatUpdateFeedError::RateLimited
			)))
		);
	}

	// A room list that the host cannot read is a host failure. If the core
	// reads it as an empty list, a delete succeeds and the room stays.
	#[test]
	fn a_delete_fails_when_the_host_cannot_list_its_rooms() {
		let chat = RecordingHost::with_rooms(&[hosted("support")]);
		*chat.room_list_failure.lock().unwrap() = Some("store locked".to_string());
		let runtime = chat_runtime(chat.clone());

		let deleted = block_on(Chat::delete_room(
			&runtime,
			&CallContext::default(),
			HostChatDeleteRoomRequest::V1(latest::HostChatDeleteRoomRequest {
				room_id: "support".to_string(),
			}),
		));

		assert_eq!(
			deleted,
			Err(CallError::Domain(HostChatDeleteRoomError::V1(
				latest::HostChatDeleteRoomError::Unknown { reason: "store locked".to_string() }
			)))
		);
		assert!(chat.calls().is_empty());
	}

	// A room keeps the default configuration, a room the host stores, until
	// the product declares one; a declaration replaces the previous one.
	#[test]
	fn a_room_takes_the_configuration_the_product_declares() {
		let chat = RecordingHost::with_rooms(&[
			hosted("support"),
			("lobby", latest::ChatRoomParticipation::Bot),
		]);
		let runtime = chat_runtime(chat.clone());
		let set = |room_id: &str, config: latest::ChatRoomConfig| {
			block_on(Chat::set_room_config(
				&runtime,
				&CallContext::default(),
				HostChatSetRoomConfigRequest::V1(latest::HostChatSetRoomConfigRequest {
					room_id: room_id.to_string(),
					config,
				}),
			))
		};
		let sourced = latest::ChatRoomConfig {
			product_source: true,
			reaction_details: true,
			report_seen: true,
		};

		update_feed(&runtime, "support", text_message("product")).expect("a stored update");
		assert_eq!(set(" support ", sourced.clone()), Ok(HostChatSetRoomConfigResponse::V1));
		assert_eq!(chat.configs.lock().unwrap().get("support"), Some(&sourced));
		update_feed(&runtime, "support", text_message("product")).expect("a shown update");
		assert_eq!(
			chat.feed
				.lock()
				.unwrap()
				.iter()
				.map(|(_, persist)| *persist)
				.collect::<Vec<_>>(),
			[true, false]
		);
		assert_eq!(
			set("lobby", sourced),
			Err(CallError::Domain(HostChatSetRoomConfigError::V1(
				latest::HostChatSetRoomConfigError::NotRoomHost
			)))
		);
		assert_eq!(chat.calls(), ["config support"]);
	}

	// The icon of a room keeps its rules: a scheme the host must not fetch is
	// refused.
	#[test]
	fn a_room_keeps_its_icon_rules() {
		let chat = RecordingHost::with_rooms(&[]);
		let runtime = chat_runtime(chat.clone());
		let create = |icon: &str| {
			block_on(Chat::create_room(
				&runtime,
				&CallContext::default(),
				HostChatCreateRoomRequest::V1(v01::HostChatCreateRoomRequest {
					room_id: "support".to_string(),
					name: "Support".to_string(),
					icon: icon.to_string(),
				}),
			))
		};

		create("https://icons.example/support.png").expect("an https icon is accepted");
		create("data:image/png;base64,AQID").expect("an inline icon is accepted");
		create("").expect("no icon is accepted");
		assert!(matches!(
			create("javascript:alert(1)"),
			Err(CallError::Domain(HostChatCreateRoomError::V1(
				latest::HostChatCreateRoomError::Unknown { .. }
			)))
		));
		assert_eq!(
			*chat.icons.lock().unwrap(),
			[
				"https://icons.example/support.png".to_string(),
				"data:image/png;base64,AQID".to_string(),
				String::new()
			]
		);
	}
}
