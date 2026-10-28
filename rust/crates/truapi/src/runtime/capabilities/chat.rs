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
		FromLatest, IntoLatest, Versioned,
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

use super::from_latest_error;
use crate::{
	platform::{
		ChatFieldError, ChatPlatform, ChatRateLimiter, normalize_chat_identifier,
		validate_chat_feed_update, validate_chat_icon, validate_chat_message_content,
		validate_chat_name, validate_chat_room_metadata, validate_chat_room_preview,
	},
	runtime::{ChatHandler, ProductRuntimeHost},
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
		let version = request.version();
		self.create_chat_room(request.into_latest())
			.await
			.map(|response| HostChatCreateRoomResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "chat.register_bot"))]
	async fn register_bot(
		&self,
		_cx: &CallContext,
		request: HostChatRegisterBotRequest,
	) -> Result<HostChatRegisterBotResponse, CallError<HostChatRegisterBotError>> {
		let version = request.version();
		self.register_chat_bot(request.into_latest())
			.await
			.map(|response| HostChatRegisterBotResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "chat.list_subscribe"))]
	async fn list_subscribe(
		&self,
		_cx: &CallContext,
		request: HostChatListSubscribeRequest,
	) -> Subscription<HostChatListSubscribeItem, CallError<HostChatListSubscribeError>> {
		let version = request.version();
		let platform = match self.connection.chat_platform::<HostChatListSubscribeError>() {
			Ok(platform) => platform,
			Err(error) => return Subscription::interrupted(error),
		};
		Subscription::new(platform.subscribe_chat_rooms(&self.connection.product).map(
			move |item| match item {
				Ok(item) => Ok(HostChatListSubscribeItem::from_latest(item, version)),
				Err(error) => {
					warn!(reason = %error.reason, "chat room list platform stream failed");
					Err(CallError::HostFailure { reason: error.reason })
				},
			},
		))
	}

	#[instrument(skip_all, fields(runtime.method = "chat.post_message"))]
	async fn post_message(
		&self,
		_cx: &CallContext,
		request: HostChatPostMessageRequest,
	) -> Result<HostChatPostMessageResponse, CallError<HostChatPostMessageError>> {
		let version = request.version();
		self.post_chat_message(request.into_latest())
			.await
			.map(|response| HostChatPostMessageResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "chat.action_subscribe"))]
	async fn action_subscribe(
		&self,
		_cx: &CallContext,
		_request: HostChatActionSubscribeRequest,
	) -> Subscription<HostChatActionSubscribeItem, CallError<HostChatActionSubscribeError>> {
		if let Err(error) = self.connection.chat_platform::<HostChatActionSubscribeError>() {
			return Subscription::interrupted(error);
		}
		self.connection.chat.subscribe()
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_footer"))]
	async fn set_room_footer(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomFooterRequest,
	) -> Result<HostChatSetRoomFooterResponse, CallError<HostChatSetRoomFooterError>> {
		let version = request.version();
		self.set_chat_room_footer(request.into_latest())
			.await
			.map(|()| HostChatSetRoomFooterResponse::from_latest((), version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_config"))]
	async fn set_room_config(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomConfigRequest,
	) -> Result<HostChatSetRoomConfigResponse, CallError<HostChatSetRoomConfigError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.connection.chat_platform()?;
			let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_config(&self.connection.product, request)
				.await
				.map_err(room_host_error)
		};
		answer_room_change(result.await, version, HostChatSetRoomConfigResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_metadata"))]
	async fn set_room_metadata(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomMetadataRequest,
	) -> Result<HostChatSetRoomMetadataResponse, CallError<HostChatSetRoomMetadataError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.connection.chat_platform()?;
			request.metadata =
				validate_chat_room_metadata(request.metadata).map_err(room_field_error)?;
			let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_metadata(&self.connection.product, request)
				.await
				.map_err(room_host_error)
		};
		answer_room_change(result.await, version, HostChatSetRoomMetadataResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_preview"))]
	async fn set_room_preview(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomPreviewRequest,
	) -> Result<HostChatSetRoomPreviewResponse, CallError<HostChatSetRoomPreviewError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.connection.chat_platform()?;
			request.preview =
				validate_chat_room_preview(request.preview).map_err(room_field_error)?;
			let limiter = &self.connection.services.chat_rate_limits.preview;
			let room_id = self.hosted_room(&platform, &request.room_id, Some(limiter)).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_preview(&self.connection.product, request)
				.await
				.map_err(room_host_error)
		};
		answer_room_change(result.await, version, HostChatSetRoomPreviewResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.delete_room"))]
	async fn delete_room(
		&self,
		_cx: &CallContext,
		request: HostChatDeleteRoomRequest,
	) -> Result<HostChatDeleteRoomResponse, CallError<HostChatDeleteRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.connection.chat_platform()?;
			match self.hosted_room(&platform, &request.room_id, None).await {
				// Deleting a room that is gone already succeeds, so a product
				// can retry a delete whose answer it lost.
				Err(RoomRefusal::UnknownRoom) => Ok(()),
				Err(refusal) => Err(CallError::Domain(refusal.into())),
				Ok(room_id) => {
					let limits = &self.connection.services.chat_rate_limits;
					limits.feed.forget(self.connection.product.product_id.as_str(), &room_id);
					limits.preview.forget(self.connection.product.product_id.as_str(), &room_id);
					request.room_id = room_id;
					platform
						.delete_chat_room(&self.connection.product, request)
						.await
						.map_err(room_host_error)
				},
			}
		};
		answer_room_change(result.await, version, HostChatDeleteRoomResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.update_feed"))]
	async fn update_feed(
		&self,
		_cx: &CallContext,
		request: HostChatUpdateFeedRequest,
	) -> Result<HostChatUpdateFeedResponse, CallError<HostChatUpdateFeedError>> {
		let version = request.version();
		self.update_chat_feed(request.into_latest())
			.await
			.map(|()| HostChatUpdateFeedResponse::from_latest((), version))
			.map_err(|error| from_latest_error(error, version))
	}
}

/// Why the core refuses a room-changing call before the host sees it.
#[derive(Debug)]
enum RoomRefusal {
	/// The product created no room with this id.
	UnknownRoom,
	/// The product takes part in the room as a bot.
	NotRoomHost,
	/// The room spent its call budget for now.
	RateLimited,
	/// The id was malformed, or the host could not list the rooms.
	Failed(String),
}

// Only `set_room_preview` and `update_feed` ask for a rate limit, and a
// delete of a room that does not exist succeeds before it reaches its error.
impl From<RoomRefusal> for latest::HostChatSetRoomConfigError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom => Self::UnknownRoom,
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::RateLimited => Self::Unknown { reason: "rate limited".to_string() },
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl From<RoomRefusal> for latest::HostChatSetRoomMetadataError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom => Self::UnknownRoom,
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::RateLimited => Self::Unknown { reason: "rate limited".to_string() },
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl From<RoomRefusal> for latest::HostChatSetRoomPreviewError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom => Self::UnknownRoom,
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::RateLimited => Self::RateLimited,
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl From<RoomRefusal> for latest::HostChatDeleteRoomError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::UnknownRoom => Self::Unknown { reason: "unknown room".to_string() },
			RoomRefusal::RateLimited => Self::Unknown { reason: "rate limited".to_string() },
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl From<RoomRefusal> for latest::HostChatUpdateFeedError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom => Self::UnknownRoom,
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::RateLimited => Self::RateLimited,
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
			RoomRefusal::RateLimited => Self::Unknown { reason: "rate limited".to_string() },
			RoomRefusal::Failed(reason) => Self::Unknown { reason },
		}
	}
}

impl<E: From<RoomRefusal>> From<RoomRefusal> for CallError<E> {
	fn from(refusal: RoomRefusal) -> Self {
		CallError::Domain(refusal.into())
	}
}

/// Answer a room-changing call in the caller's version.
fn answer_room_change<R, E: FromLatest>(
	result: Result<(), CallError<E::Latest>>,
	version: u8,
	response: fn((), u8) -> R,
) -> Result<R, CallError<E>> {
	result
		.map(|()| response((), version))
		.map_err(|error| from_latest_error(error, version))
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

	/// The normalized id of a room this product hosts, once the room passed
	/// the call budget `limiter` holds it to.
	///
	/// The room list is read once for each call, so every host answers a room
	/// the product never created, or joined only as a bot, the same way.
	async fn hosted_room(
		&self,
		platform: &Arc<dyn ChatPlatform>,
		room_id: &str,
		limiter: Option<&ChatRateLimiter>,
	) -> Result<String, RoomRefusal> {
		let room_id = normalize_chat_identifier("roomId", room_id)
			.map_err(|error| RoomRefusal::Failed(error.to_string()))?;
		let rooms = match platform.subscribe_chat_rooms(&self.connection.product).next().await {
			Some(Ok(item)) => item.rooms,
			Some(Err(error)) => return Err(RoomRefusal::Failed(error.reason)),
			None => return Err(RoomRefusal::Failed("the host published no room list".to_string())),
		};
		match rooms.iter().find(|room| room.room_id == room_id) {
			None => return Err(RoomRefusal::UnknownRoom),
			Some(room) if room.participating_as != latest::ChatRoomParticipation::RoomHost => {
				return Err(RoomRefusal::NotRoomHost);
			},
			Some(_) => {},
		}
		if let Some(limiter) = limiter &&
			!limiter.try_acquire(
				self.connection.product.product_id.as_str(),
				&room_id,
				current_unix_millis(),
			) {
			return Err(RoomRefusal::RateLimited);
		}
		Ok(room_id)
	}

	async fn create_chat_room(
		&self,
		mut request: latest::HostChatCreateRoomRequest,
	) -> Result<latest::HostChatCreateRoomResponse, CallError<latest::HostChatCreateRoomError>> {
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
	}

	async fn register_chat_bot(
		&self,
		mut request: latest::HostChatRegisterBotRequest,
	) -> Result<latest::HostChatRegisterBotResponse, CallError<latest::HostChatRegisterBotError>> {
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
	}

	async fn post_chat_message(
		&self,
		mut request: latest::HostChatPostMessageRequest,
	) -> Result<latest::HostChatPostMessageResponse, CallError<latest::HostChatPostMessageError>> {
		let platform = self.connection.chat_platform()?;
		// The same normalization create_room applied, so a product's own
		// spelling of a room id still resolves to the stored room.
		request.room_id =
			normalize_chat_identifier("roomId", &request.room_id).map_err(post_field_error)?;
		// Content is product-authored and host-rendered, so it gets the same
		// treatment the room fields get rather than reaching a host raw.
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
				reason: "the room keeps its messages in the product; use updateFeed".to_string(),
			}));
		}
		platform
			.post_chat_message(&self.connection.product, request)
			.await
			.map_err(CallError::Domain)
	}

	async fn set_chat_room_footer(
		&self,
		mut request: latest::HostChatSetRoomFooterRequest,
	) -> Result<(), CallError<latest::HostChatSetRoomFooterError>> {
		let failure = |reason: String| {
			CallError::Domain(latest::HostChatSetRoomFooterError::Unknown { reason })
		};
		let platform = self.connection.chat_platform()?;
		let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
		request.room_id = room_id;
		platform
			.set_chat_room_footer(&self.connection.product, request)
			.await
			.map_err(|error| failure(error.reason))
	}

	async fn update_chat_feed(
		&self,
		mut request: latest::HostChatUpdateFeedRequest,
	) -> Result<(), CallError<latest::HostChatUpdateFeedError>> {
		let platform = self.connection.chat_platform()?;
		request.update = validate_chat_feed_update(request.update).map_err(feed_field_error)?;
		let limiter = &self.connection.services.chat_rate_limits.feed;
		let room_id = self.hosted_room(&platform, &request.room_id, Some(limiter)).await?;
		request.room_id = room_id;
		let config = platform
			.read_chat_room_config(&self.connection.product, request.room_id.clone())
			.await
			.map_err(|error| {
				CallError::Domain(latest::HostChatUpdateFeedError::Unknown { reason: error.reason })
			})?;
		let persist = !config.product_source;
		// In a room the host stores, the host's user writes only through the
		// host, so a product cannot put words in the user's mouth.
		if persist &&
			let latest::ChatFeedUpdate::New(message) = &request.update &&
			message.origin.participant_id == USER_PARTICIPANT_ID
		{
			return Err(CallError::Domain(latest::HostChatUpdateFeedError::NotOwner));
		}
		platform
			.update_chat_feed(&self.connection.product, request, persist)
			.await
			.map_err(CallError::Domain)
	}
}

#[cfg(test)]
mod tests {
	use std::{collections::BTreeMap, sync::Mutex};

	use futures::{executor::block_on, stream::BoxStream};
	use truapi::v01;

	use super::*;
	use crate::{
		platform::{CHAT_FEED_UPDATES_PER_SECOND, ProductContext, ProductExecutionKind, ProductId},
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
			futures::stream::iter([Ok(latest::HostChatListSubscribeItem { rooms })]).boxed()
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

	#[test]
	fn a_product_cannot_write_as_the_user_in_a_room_the_host_keeps() {
		let chat = RecordingHost::with_rooms(&[hosted("stored")]);
		let runtime = chat_runtime(chat.clone());

		assert_eq!(
			update_feed(&runtime, "stored", text_message("user")),
			Err(CallError::Domain(HostChatUpdateFeedError::V1(
				latest::HostChatUpdateFeedError::NotOwner
			)))
		);
		assert!(chat.feed.lock().unwrap().is_empty());
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

	// The 21st update in one second is refused, before the host sees it.
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
