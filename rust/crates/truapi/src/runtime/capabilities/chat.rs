//! Product-facing chat capability adapters.
//!
//! The core screens every chat call before a host sees it: identifiers and
//! texts are normalized and bounded, a room-changing call needs a room the
//! product hosts, and feed, preview and activity calls are paced per room.

use std::sync::Arc;

use futures::{StreamExt, future};
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
			HostChatCreateRoomResponse, HostChatDeleteRoomRequest, HostChatDeleteRoomResponse,
			HostChatListSubscribeError, HostChatListSubscribeItem, HostChatListSubscribeRequest,
			HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
			HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
			HostChatRoomError, HostChatSetRoomActivityRequest, HostChatSetRoomActivityResponse,
			HostChatSetRoomCommandsRequest, HostChatSetRoomCommandsResponse,
			HostChatSetRoomConfigRequest, HostChatSetRoomConfigResponse,
			HostChatSetRoomFooterError, HostChatSetRoomFooterRequest,
			HostChatSetRoomFooterResponse, HostChatSetRoomMetadataRequest,
			HostChatSetRoomMetadataResponse, HostChatSetRoomPreviewRequest,
			HostChatSetRoomPreviewResponse, HostChatSetRoomSuggestionsRequest,
			HostChatSetRoomSuggestionsResponse, HostChatSetRoomToolbarRequest,
			HostChatSetRoomToolbarResponse, HostChatUpdateFeedError, HostChatUpdateFeedRequest,
			HostChatUpdateFeedResponse,
		},
	},
};

use super::from_latest_error;
use crate::{
	platform::{
		ChatFieldError, ChatPlatform, ChatRateLimiter, normalize_chat_identifier,
		validate_chat_activity, validate_chat_commands, validate_chat_feed_update,
		validate_chat_icon, validate_chat_message_content, validate_chat_name,
		validate_chat_room_config, validate_chat_room_footer, validate_chat_room_metadata,
		validate_chat_room_preview, validate_chat_suggestions, validate_chat_toolbar,
	},
	runtime::ProductRuntimeHost,
	unix_time::current_unix_millis,
};

/// The participant id of the host's user, which a product cannot write as.
const USER_PARTICIPANT_ID: &str = "user";

/// Fields whose rejection a product resolves by sending less content, and the
/// only ones the fieldless `MessageTooLarge` can describe.
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
		let icon = request.v1_icon().map(str::to_string);
		self.create_chat_room(request.into_latest(), icon)
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
		let platform = match self.chat_platform::<HostChatListSubscribeError>() {
			Ok(platform) => platform,
			Err(error) => return Subscription::interrupted(error),
		};
		Subscription::new(platform.subscribe_chat_rooms(&self.product).map(
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
		request: HostChatActionSubscribeRequest,
	) -> Subscription<HostChatActionSubscribeItem, CallError<HostChatActionSubscribeError>> {
		let version = request.version();
		if let Err(error) = self.chat_platform::<HostChatActionSubscribeError>() {
			return Subscription::interrupted(error);
		}
		// An event the subscriber's version cannot carry is skipped, never
		// sent in a version the subscriber cannot decode.
		let events = self.chat.subscribe::<latest::GenericError>().filter_map(move |item| {
			future::ready(match item {
				Ok(item) => HostChatActionSubscribeItem::for_version(item, version).map(Ok),
				Err(error) => Some(Err(from_latest_error(error, version))),
			})
		});
		Subscription::new(events)
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
	) -> Result<HostChatSetRoomConfigResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.chat_platform()?;
			request.config = validate_chat_room_config(request.config).map_err(room_field_error)?;
			let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_config(&self.product, request)
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
	) -> Result<HostChatSetRoomMetadataResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.chat_platform()?;
			request.metadata =
				validate_chat_room_metadata(request.metadata).map_err(room_field_error)?;
			let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_metadata(&self.product, request)
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
	) -> Result<HostChatSetRoomPreviewResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.chat_platform()?;
			request.preview =
				validate_chat_room_preview(request.preview).map_err(room_field_error)?;
			let limiter = &self.services.chat_rate_limits.preview;
			let room_id = self.hosted_room(&platform, &request.room_id, Some(limiter)).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_preview(&self.product, request)
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
	) -> Result<HostChatDeleteRoomResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.chat_platform()?;
			match self.hosted_room(&platform, &request.room_id, None).await {
				// Deleting a room that is gone already succeeds, so a product
				// can retry a delete whose answer it lost.
				Err(RoomRefusal::UnknownRoom) => Ok(()),
				Err(refusal) => Err(CallError::Domain(refusal.into())),
				Ok(room_id) => {
					let limits = &self.services.chat_rate_limits;
					limits.feed.forget(&self.product.product_id, &room_id);
					limits.preview.forget(&self.product.product_id, &room_id);
					request.room_id = room_id;
					platform.delete_chat_room(&self.product, request).await.map_err(room_host_error)
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

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_toolbar"))]
	async fn set_room_toolbar(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomToolbarRequest,
	) -> Result<HostChatSetRoomToolbarResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.chat_platform()?;
			request.actions = validate_chat_toolbar(request.actions).map_err(room_field_error)?;
			let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_toolbar(&self.product, request)
				.await
				.map_err(room_host_error)
		};
		answer_room_change(result.await, version, HostChatSetRoomToolbarResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_suggestions"))]
	async fn set_room_suggestions(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomSuggestionsRequest,
	) -> Result<HostChatSetRoomSuggestionsResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.chat_platform()?;
			request.suggestions =
				validate_chat_suggestions(request.suggestions).map_err(room_field_error)?;
			let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_suggestions(&self.product, request)
				.await
				.map_err(room_host_error)
		};
		answer_room_change(result.await, version, HostChatSetRoomSuggestionsResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_commands"))]
	async fn set_room_commands(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomCommandsRequest,
	) -> Result<HostChatSetRoomCommandsResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.chat_platform()?;
			request.commands =
				validate_chat_commands(request.commands).map_err(room_field_error)?;
			let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_commands(&self.product, request)
				.await
				.map_err(room_host_error)
		};
		answer_room_change(result.await, version, HostChatSetRoomCommandsResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_activity"))]
	async fn set_room_activity(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomActivityRequest,
	) -> Result<HostChatSetRoomActivityResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.chat_platform()?;
			request.activity = request
				.activity
				.map(validate_chat_activity)
				.transpose()
				.map_err(room_field_error)?;
			let limiter = &self.services.chat_rate_limits.preview;
			let room_id = self.hosted_room(&platform, &request.room_id, Some(limiter)).await?;
			request.room_id = room_id;
			platform
				.set_chat_room_activity(&self.product, request)
				.await
				.map_err(room_host_error)
		};
		answer_room_change(result.await, version, HostChatSetRoomActivityResponse::from_latest)
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

impl From<RoomRefusal> for latest::HostChatRoomError {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom => Self::UnknownRoom,
			RoomRefusal::NotRoomHost => Self::NotRoomHost,
			RoomRefusal::RateLimited => Self::RateLimited,
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
fn answer_room_change<R>(
	result: Result<(), CallError<latest::HostChatRoomError>>,
	version: u8,
	response: fn((), u8) -> R,
) -> Result<R, CallError<HostChatRoomError>> {
	result
		.map(|()| response((), version))
		.map_err(|error| from_latest_error(error, version))
}

/// Report a rejected field of a room-changing call.
fn room_field_error(error: ChatFieldError) -> CallError<latest::HostChatRoomError> {
	CallError::Domain(latest::HostChatRoomError::Unknown { reason: error.to_string() })
}

/// Report a host failure of a room-changing call.
fn room_host_error(error: latest::GenericError) -> CallError<latest::HostChatRoomError> {
	CallError::Domain(latest::HostChatRoomError::Unknown { reason: error.reason })
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
		let rooms = match platform.subscribe_chat_rooms(&self.product).next().await {
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
			!limiter.try_acquire(&self.product.product_id, &room_id, current_unix_millis())
		{
			return Err(RoomRefusal::RateLimited);
		}
		Ok(room_id)
	}

	async fn create_chat_room(
		&self,
		mut request: latest::HostChatCreateRoomRequest,
		icon: Option<String>,
	) -> Result<latest::HostChatCreateRoomResponse, CallError<latest::HostChatCreateRoomError>> {
		let platform = self.chat_platform()?;
		let field_error = |error: ChatFieldError| {
			CallError::Domain(latest::HostChatCreateRoomError::Unknown {
				reason: error.to_string(),
			})
		};
		// A caller at the first version keeps its icon rules: the icon is
		// screened as before, and an `https` one, which a resource cannot name,
		// travels beside the request.
		let icon_url = icon
			.map(|icon| validate_chat_icon("icon", &icon))
			.transpose()
			.map_err(field_error)?
			.filter(|icon| icon.starts_with("https:"))
			.unwrap_or_default();
		request.room_id =
			normalize_chat_identifier("roomId", &request.room_id).map_err(field_error)?;
		request.metadata = validate_chat_room_metadata(request.metadata).map_err(field_error)?;
		request.config = validate_chat_room_config(request.config).map_err(field_error)?;
		request.preview = request
			.preview
			.map(validate_chat_room_preview)
			.transpose()
			.map_err(field_error)?;
		platform
			.create_chat_room(&self.product, request, icon_url)
			.await
			.map_err(CallError::Domain)
	}

	async fn register_chat_bot(
		&self,
		mut request: latest::HostChatRegisterBotRequest,
	) -> Result<latest::HostChatRegisterBotResponse, CallError<latest::HostChatRegisterBotError>> {
		let platform = self.chat_platform()?;
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
			.register_chat_bot(&self.product, request)
			.await
			.map_err(CallError::Domain)
	}

	async fn post_chat_message(
		&self,
		mut request: latest::HostChatPostMessageRequest,
	) -> Result<latest::HostChatPostMessageResponse, CallError<latest::HostChatPostMessageError>> {
		let platform = self.chat_platform()?;
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
			.read_chat_room_config(&self.product, request.room_id.clone())
			.await
			.map_err(|error| {
			CallError::Domain(latest::HostChatPostMessageError::Unknown { reason: error.reason })
		})?;
		if config.product_source {
			return Err(CallError::Domain(latest::HostChatPostMessageError::Unknown {
				reason: "the room keeps its messages in the product; use updateFeed".to_string(),
			}));
		}
		platform
			.post_chat_message(&self.product, request)
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
		let platform = self.chat_platform()?;
		request.footer = validate_chat_room_footer(request.footer)
			.map_err(|error| failure(error.to_string()))?;
		let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
		request.room_id = room_id;
		platform
			.set_chat_room_footer(&self.product, request)
			.await
			.map_err(|error| failure(error.reason))
	}

	async fn update_chat_feed(
		&self,
		mut request: latest::HostChatUpdateFeedRequest,
	) -> Result<(), CallError<latest::HostChatUpdateFeedError>> {
		let platform = self.chat_platform()?;
		request.update = validate_chat_feed_update(request.update).map_err(feed_field_error)?;
		let limiter = &self.services.chat_rate_limits.feed;
		let room_id = self.hosted_room(&platform, &request.room_id, Some(limiter)).await?;
		request.room_id = room_id;
		let config = platform
			.read_chat_room_config(&self.product, request.room_id.clone())
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
			.update_chat_feed(&self.product, request, persist)
			.await
			.map_err(CallError::Domain)
	}
}

#[cfg(test)]
mod tests {
	use std::{collections::BTreeMap, sync::Mutex};

	use futures::{executor::block_on, stream::BoxStream};
	use truapi::{v01, v02};

	use super::*;
	use crate::{
		platform::{CHAT_FEED_UPDATES_PER_SECOND, ProductContext, ProductExecutionKind},
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
			icon_url: String,
		) -> Result<latest::HostChatCreateRoomResponse, latest::HostChatCreateRoomError> {
			self.icons.lock().unwrap().push(icon_url);
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
		let (host_config, _) = runtime_config("chat.dot");
		let product = ProductContext::new_with_execution(
			"chat.dot".to_string(),
			ProductExecutionKind::Worker,
		)
		.expect("test chat product context is valid");
		let services = RuntimeServices::new(
			stub_platform(),
			host_config.host.host_info.clone(),
			host_config.people_chain_genesis_hash,
			host_config.bulletin_chain_genesis_hash,
			host_config.asset_hub_chain_genesis_hash,
			test_spawner(),
		);
		let pairing_host = PairingHostRole::new(services.clone(), host_config);
		let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
		adapters.chat_platform = Some(chat);
		let host = ProductRuntimeHost::from_services(services, adapters, pairing_host, product);
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
			edited_at: None,
			reactions: Vec::new(),
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

	fn set_config(
		runtime: &ProductRuntimeHost,
		room_id: &str,
		config: latest::ChatRoomConfig,
	) -> Result<HostChatSetRoomConfigResponse, CallError<HostChatRoomError>> {
		block_on(Chat::set_room_config(
			runtime,
			&CallContext::default(),
			HostChatSetRoomConfigRequest::V1(latest::HostChatSetRoomConfigRequest {
				room_id: room_id.to_string(),
				config,
			}),
		))
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

		assert_eq!(
			set_config(&runtime, "missing", Default::default()),
			Err(CallError::Domain(HostChatRoomError::V1(latest::HostChatRoomError::UnknownRoom)))
		);
		assert_eq!(
			set_config(&runtime, "lobby", Default::default()),
			Err(CallError::Domain(HostChatRoomError::V1(latest::HostChatRoomError::NotRoomHost)))
		);
		assert_eq!(
			set_config(&runtime, " support ", Default::default()),
			Ok(HostChatSetRoomConfigResponse::V1)
		);
		assert_eq!(chat.calls(), ["config support"]);
	}

	#[test]
	fn a_room_change_is_screened_before_the_host_sees_it() {
		let chat = RecordingHost::with_rooms(&[hosted("support")]);
		let runtime = chat_runtime(chat.clone());

		let refused = set_config(
			&runtime,
			"support",
			latest::ChatRoomConfig {
				placeholder: Some("hi\u{202e}".to_string()),
				..Default::default()
			},
		);

		assert!(matches!(
			refused,
			Err(CallError::Domain(HostChatRoomError::V1(
				latest::HostChatRoomError::Unknown { .. }
			)))
		));
		assert!(chat.calls().is_empty());
	}

	// A host-store room keeps what `update_feed` writes; a product-source room
	// only shows it, so the host must not store it.
	#[test]
	fn a_feed_update_is_stored_only_in_a_room_the_host_keeps() {
		let chat = RecordingHost::with_rooms(&[hosted("stored"), hosted("sourced")]);
		let runtime = chat_runtime(chat.clone());
		set_config(
			&runtime,
			"sourced",
			latest::ChatRoomConfig { product_source: true, ..Default::default() },
		)
		.expect("the product hosts the room");

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

	// S44: the 21st update in one second is refused, before the host sees it.
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
		set_config(
			&runtime,
			"sourced",
			latest::ChatRoomConfig { product_source: true, ..Default::default() },
		)
		.expect("the product hosts the room");

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
		assert_eq!(chat.calls(), ["config sourced"]);
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

	// The first version's `https` icon has no v0.2 shape, so it travels beside
	// the request, and a scheme the first version refused is still refused.
	#[test]
	fn a_first_version_room_keeps_its_icon_rules() {
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
		create("").expect("no icon is accepted");
		assert!(matches!(
			create("javascript:alert(1)"),
			Err(CallError::Domain(HostChatCreateRoomError::V1(
				latest::HostChatCreateRoomError::Unknown { .. }
			)))
		));
		assert_eq!(
			*chat.icons.lock().unwrap(),
			["https://icons.example/support.png".to_string(), String::new()]
		);
	}

	fn event(event: latest::ChatEvent) -> latest::HostChatActionSubscribeItem {
		v02::HostChatActionSubscribeItem {
			room_id: "support".to_string(),
			peer: "native".to_string(),
			event,
		}
	}

	fn drain(
		events: &mut Subscription<
			HostChatActionSubscribeItem,
			CallError<HostChatActionSubscribeError>,
		>,
	) -> Vec<HostChatActionSubscribeItem> {
		let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
		let mut items = Vec::new();
		while let core::task::Poll::Ready(Some(Ok(item))) = events.poll_next_unpin(&mut cx) {
			items.push(item);
		}
		items
	}

	// S42: a subscriber at the first version receives only the events its
	// item carries, and one at the second version receives every event.
	#[test]
	fn each_subscriber_receives_the_events_of_its_version() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[hosted("support")]));
		let subscribe =
			|request| block_on(Chat::action_subscribe(&runtime, &CallContext::default(), request));
		let mut v1 = subscribe(HostChatActionSubscribeRequest::V1);
		let command = latest::ChatEvent::Command(latest::ChatCommand {
			command: "help".to_string(),
			payload: String::new(),
		});

		runtime
			.publish_chat_event(event(latest::ChatEvent::RoomOpened))
			.expect("published");
		runtime.publish_chat_event(event(command.clone())).expect("published");

		assert_eq!(
			drain(&mut v1),
			[HostChatActionSubscribeItem::V1(v01::HostChatActionSubscribeItem {
				room_id: "support".to_string(),
				peer: "native".to_string(),
				payload: v01::ChatActionPayload::Command(latest::ChatCommand {
					command: "help".to_string(),
					payload: String::new(),
				}),
			})]
		);

		// The channel serves one subscriber at a time; a new one takes over.
		let mut v2 = subscribe(HostChatActionSubscribeRequest::V2);
		runtime
			.publish_chat_event(event(latest::ChatEvent::RoomClosed))
			.expect("published");
		assert_eq!(
			drain(&mut v2),
			[HostChatActionSubscribeItem::V2(event(latest::ChatEvent::RoomClosed))]
		);
	}

	// P2.5: a burst of `Seen` reaches the product at most once per room each
	// second, and the newest one follows when the second ends.
	#[test]
	fn a_burst_of_seen_events_is_paced_per_room() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[hosted("support")]));
		let mut events = block_on(Chat::action_subscribe(
			&runtime,
			&CallContext::default(),
			HostChatActionSubscribeRequest::V2,
		));
		let seen =
			|index: usize| event(latest::ChatEvent::Seen { message_id: format!("m{index}") });

		for index in 0..100 {
			runtime.publish_chat_event(seen(index)).expect("published");
		}
		assert_eq!(drain(&mut events), [HostChatActionSubscribeItem::V2(seen(0))]);

		std::thread::sleep(core::time::Duration::from_millis(1_300));
		assert_eq!(drain(&mut events), [HostChatActionSubscribeItem::V2(seen(99))]);
	}
}
