//! Product-facing chat capability adapters.
//!
//! The core screens every chat call before a host sees it: identifiers and
//! texts are normalized and bounded, a room-changing call needs a room the
//! product hosts, and feed, preview and activity calls are paced per room.

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
			HostChatCreateRoomResponse, HostChatDeleteRoomRequest, HostChatDeleteRoomResponse,
			HostChatListSubscribeError, HostChatListSubscribeItem, HostChatListSubscribeRequest,
			HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
			HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
			HostChatRoomError, HostChatRoomEventsSubscribeError, HostChatRoomEventsSubscribeItem,
			HostChatRoomEventsSubscribeRequest, HostChatSetRoomCallRequest,
			HostChatSetRoomCallResponse, HostChatSetRoomConfigRequest,
			HostChatSetRoomConfigResponse, HostChatSetRoomContactRequest,
			HostChatSetRoomContactResponse, HostChatSetRoomFeaturesRequest,
			HostChatSetRoomFeaturesResponse, HostChatSetRoomFooterError,
			HostChatSetRoomFooterRequest, HostChatSetRoomFooterResponse,
			HostChatSetRoomMetadataRequest, HostChatSetRoomMetadataResponse,
			HostChatSetRoomPreviewRequest, HostChatSetRoomPreviewResponse, HostChatUpdateFeedError,
			HostChatUpdateFeedRequest, HostChatUpdateFeedResponse,
		},
	},
};

use super::from_latest_error;
use crate::{
	platform::{
		ChatFieldError, ChatPlatform, ChatRateLimiter, known_chat_feature_names,
		normalize_chat_identifier, validate_chat_feed_update, validate_chat_icon,
		validate_chat_message_content, validate_chat_name, validate_chat_room_call,
		validate_chat_room_contact, validate_chat_room_metadata, validate_chat_room_preview,
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
	) -> Result<HostChatSetRoomConfigResponse, CallError<HostChatRoomError>> {
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
	) -> Result<HostChatSetRoomMetadataResponse, CallError<HostChatRoomError>> {
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
	) -> Result<HostChatSetRoomPreviewResponse, CallError<HostChatRoomError>> {
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
	) -> Result<HostChatDeleteRoomResponse, CallError<HostChatRoomError>> {
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
					limits.contact.forget(self.connection.product.product_id.as_str(), &room_id);
					limits.call.forget(self.connection.product.product_id.as_str(), &room_id);
					self.forget_chat_staging(&room_id);
					self.connection.services.hold_chat_call(
						&self.connection.product.product_id,
						&room_id,
						None,
					);
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

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_features"))]
	async fn set_room_features(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomFeaturesRequest,
	) -> Result<HostChatSetRoomFeaturesResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.connection.chat_platform()?;
			let room_id = self.hosted_room(&platform, &request.room_id, None).await?;
			request.room_id = room_id;
			let features = request.features.clone();
			let room_id = request.room_id.clone();
			platform
				.set_chat_room_features(&self.connection.product, request)
				.await
				.map_err(room_host_error)?;
			self.record_chat_staging(&room_id, &features);
			Ok(())
		};
		answer_room_change(result.await, version, HostChatSetRoomFeaturesResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_call"))]
	async fn set_room_call(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomCallRequest,
	) -> Result<HostChatSetRoomCallResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.connection.chat_platform()?;
			request.call = request
				.call
				.map(validate_chat_room_call)
				.transpose()
				.map_err(room_field_error)?;
			let limiter = &self.connection.services.chat_rate_limits.call;
			let room_id = self.hosted_room(&platform, &request.room_id, Some(limiter)).await?;
			if request.call.is_some() {
				self.require_chat_feature("call_buttons").await?;
			}
			request.room_id = room_id.clone();
			let call = request.call.clone();
			platform
				.set_chat_room_call(&self.connection.product, request)
				.await
				.map_err(room_host_error)?;
			self.connection.services.hold_chat_call(
				&self.connection.product.product_id,
				&room_id,
				call.as_ref(),
			);
			Ok(())
		};
		answer_room_change(result.await, version, HostChatSetRoomCallResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.set_room_contact"))]
	async fn set_room_contact(
		&self,
		_cx: &CallContext,
		request: HostChatSetRoomContactRequest,
	) -> Result<HostChatSetRoomContactResponse, CallError<HostChatRoomError>> {
		let version = request.version();
		let mut request = request.into_latest();
		let result = async {
			let platform = self.connection.chat_platform()?;
			request.contact =
				validate_chat_room_contact(request.contact).map_err(room_field_error)?;
			let limiter = &self.connection.services.chat_rate_limits.contact;
			let room_id = self.hosted_room(&platform, &request.room_id, Some(limiter)).await?;
			self.require_chat_feature("contact_rooms").await?;
			request.room_id = room_id;
			platform
				.set_chat_room_contact(&self.connection.product, request)
				.await
				.map_err(room_host_error)
		};
		answer_room_change(result.await, version, HostChatSetRoomContactResponse::from_latest)
	}

	#[instrument(skip_all, fields(runtime.method = "chat.room_events_subscribe"))]
	async fn room_events_subscribe(
		&self,
		_cx: &CallContext,
		request: HostChatRoomEventsSubscribeRequest,
	) -> Subscription<HostChatRoomEventsSubscribeItem, CallError<HostChatRoomEventsSubscribeError>>
	{
		let version = request.version();
		if let Err(error) = self.connection.chat_platform::<HostChatRoomEventsSubscribeError>() {
			return Subscription::interrupted(error);
		}
		let events = self
			.connection
			.services
			.chat_room_events
			.subscribe::<latest::GenericError>(self.connection.product.product_id.as_str())
			.map(move |item| match item {
				Ok(item) => Ok(HostChatRoomEventsSubscribeItem::from_latest(item, version)),
				Err(error) => Err(from_latest_error(error, version)),
			});
		Subscription::new(events)
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

/// Refuse a room-changing call with a reason.
fn room_failure(reason: String) -> CallError<latest::HostChatRoomError> {
	CallError::Domain(latest::HostChatRoomError::Unknown { reason })
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

	/// The Chat features the host lists that the core knows and can back.
	/// Only the chat product has rooms with a contact and attachments on HOP.
	pub async fn chat_capabilities(
		&self,
	) -> Result<Vec<&'static str>, CallError<latest::GenericError>> {
		let platform = self.connection.chat_platform()?;
		let listed = platform
			.chat_capabilities(&self.connection.product)
			.await
			.map_err(CallError::Domain)?;
		Ok(known_chat_feature_names(&listed.names)
			.into_iter()
			.filter(|name| *name != "hop_attachments" || self.backs_hop_attachments())
			.filter(|name| {
				*name != "contact_rooms" || self.connection.product.is_person_chat_product()
			})
			.collect())
	}

	/// Refuse a room-changing call that needs a feature the host does not
	/// serve.
	async fn require_chat_feature(
		&self,
		feature: &str,
	) -> Result<(), CallError<latest::HostChatRoomError>> {
		let served = self.chat_capabilities().await.map_err(|error| {
			error.map_domain(|error| latest::HostChatRoomError::Unknown { reason: error.reason })
		})?;
		if !served.contains(&feature) {
			return Err(room_failure(format!("this host does not serve {feature}")));
		}
		Ok(())
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
		capabilities: Mutex<Vec<String>>,
		features: Mutex<BTreeMap<String, latest::ChatRoomFeatures>>,
		contacts: Mutex<BTreeMap<String, latest::ChatRoomContact>>,
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

		async fn chat_capabilities(
			&self,
			_product: &ProductContext,
		) -> Result<crate::platform::ChatFeatureNames, latest::GenericError> {
			Ok(crate::platform::ChatFeatureNames {
				names: self.capabilities.lock().unwrap().clone(),
			})
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
			self.features.lock().unwrap().insert(request.room_id.clone(), request.features);
			self.record("features", &request.room_id)
		}

		async fn set_chat_room_call(
			&self,
			_product: &ProductContext,
			request: latest::HostChatSetRoomCallRequest,
		) -> Result<(), latest::GenericError> {
			self.record("call", &request.room_id)
		}

		async fn set_chat_room_contact(
			&self,
			_product: &ProductContext,
			request: latest::HostChatSetRoomContactRequest,
		) -> Result<(), latest::GenericError> {
			self.contacts.lock().unwrap().insert(request.room_id.clone(), request.contact);
			self.record("contact", &request.room_id)
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

	/// Gives `room_id` the configuration it was created with.
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

		assert_eq!(
			set_features(&runtime, "missing", Default::default()),
			Err(CallError::Domain(HostChatRoomError::V1(latest::HostChatRoomError::UnknownRoom)))
		);
		assert_eq!(
			set_features(&runtime, "lobby", Default::default()),
			Err(CallError::Domain(HostChatRoomError::V1(latest::HostChatRoomError::NotRoomHost)))
		);
		assert_eq!(
			set_features(&runtime, " support ", Default::default()),
			Ok(HostChatSetRoomFeaturesResponse::V1)
		);
		assert_eq!(chat.calls(), ["features support"]);
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
			Err(CallError::Domain(HostChatRoomError::V1(latest::HostChatRoomError::NotRoomHost)))
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

	fn capabilities(runtime: &ProductRuntimeHost) -> Vec<&'static str> {
		block_on(runtime.chat_capabilities()).expect("the host lists its features")
	}

	fn set_features(
		runtime: &ProductRuntimeHost,
		room_id: &str,
		features: latest::ChatRoomFeatures,
	) -> Result<HostChatSetRoomFeaturesResponse, CallError<HostChatRoomError>> {
		block_on(Chat::set_room_features(
			runtime,
			&CallContext::default(),
			HostChatSetRoomFeaturesRequest::V1(latest::HostChatSetRoomFeaturesRequest {
				room_id: room_id.to_string(),
				features,
			}),
		))
	}

	fn set_call(
		runtime: &ProductRuntimeHost,
		room_id: &str,
		call: Option<latest::ChatRoomCall>,
	) -> Result<HostChatSetRoomCallResponse, CallError<HostChatRoomError>> {
		block_on(Chat::set_room_call(
			runtime,
			&CallContext::default(),
			HostChatSetRoomCallRequest::V1(latest::HostChatSetRoomCallRequest {
				room_id: room_id.to_string(),
				call,
			}),
		))
	}

	fn room_unknown(reason: &str) -> Result<(), CallError<HostChatRoomError>> {
		Err(CallError::Domain(HostChatRoomError::V1(latest::HostChatRoomError::Unknown {
			reason: reason.to_string(),
		})))
	}

	fn ringing() -> latest::ChatRoomCall {
		latest::ChatRoomCall::Ringing { call_id: "c1".to_string(), video: false, caller: None }
	}

	// S49: the core backs the features the host serves end to end that it
	// knows, in one order.
	#[test]
	fn capabilities_are_the_known_names_of_the_host_list() {
		let chat = RecordingHost::with_rooms(&[]);
		*chat.capabilities.lock().unwrap() = [
			"contact_rooms",
			"call_buttons",
			"payment_button",
			"a_name_from_a_later_rfc",
			"reaction_details",
			"contact_rooms",
		]
		.map(str::to_string)
		.to_vec();
		let runtime = chat_runtime(chat);

		assert_eq!(capabilities(&runtime), ["reaction_details", "call_buttons", "contact_rooms"]);
	}

	#[test]
	fn a_host_that_lists_nothing_serves_the_v2_features_only() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[]));

		assert_eq!(capabilities(&runtime), Vec::<&str>::new());
	}

	// S51: the host keeps a declaration whole, a feature it does not serve
	// included, so that a later build applies it.
	#[test]
	fn the_features_of_a_room_are_stored_whole_even_when_unserved() {
		let chat = RecordingHost::with_rooms(&[hosted("support")]);
		let runtime = chat_runtime(chat.clone());
		let features = latest::ChatRoomFeatures {
			calls: Some(latest::ChatRoomCalls { audio: true, video: false }),
			hop_attachments: true,
		};

		assert_eq!(
			set_features(&runtime, "support", features.clone()),
			Ok(HostChatSetRoomFeaturesResponse::V1)
		);
		assert_eq!(chat.features.lock().unwrap().get("support"), Some(&features));
	}

	#[test]
	fn a_call_needs_the_feature_that_serves_it() {
		let chat = RecordingHost::with_rooms(&[hosted("main")]);
		let runtime = chat_runtime(chat.clone());

		assert_eq!(
			set_call(&runtime, "main", Some(ringing())).map(drop),
			room_unknown("this host does not serve call_buttons")
		);

		*chat.capabilities.lock().unwrap() = vec!["call_buttons".to_string()];
		std::thread::sleep(core::time::Duration::from_millis(1_000));
		assert_eq!(
			set_call(&runtime, "main", Some(ringing())),
			Ok(HostChatSetRoomCallResponse::V1)
		);
		assert_eq!(chat.calls(), ["call main"]);
	}

	#[test]
	fn the_call_of_a_room_is_paced() {
		let chat = RecordingHost::with_rooms(&[hosted("main")]);
		*chat.capabilities.lock().unwrap() = vec!["call_buttons".to_string()];
		let runtime = chat_runtime(chat);

		assert!(set_call(&runtime, "main", Some(ringing())).is_ok());
		assert!(set_call(&runtime, "main", None).is_ok());
		assert_eq!(
			set_call(&runtime, "main", Some(ringing())),
			Err(CallError::Domain(HostChatRoomError::V1(latest::HostChatRoomError::RateLimited)))
		);
	}

	// D31: a call the core accepted holds the worker; a cleared call and a
	// deleted room release it, and a refused call holds nothing.
	#[test]
	fn the_call_of_a_room_holds_the_worker() {
		let chat = RecordingHost::with_rooms(&[hosted("main"), hosted("other")]);
		let runtime = chat_runtime(chat.clone());
		let references = || {
			runtime
				.connection
				.services
				.worker_ledger
				.count(&ProductId::for_test("chat.dot"))
		};

		assert!(set_call(&runtime, "main", Some(ringing())).is_err());
		assert_eq!(references(), 0);
		*chat.capabilities.lock().unwrap() = vec!["call_buttons".to_string()];
		set_call(&runtime, "main", Some(ringing())).expect("served");
		assert_eq!(references(), 1);
		set_call(&runtime, "other", Some(ringing())).expect("served");
		assert_eq!(references(), 2);
		std::thread::sleep(core::time::Duration::from_millis(1_000));
		set_call(&runtime, "main", None).expect("cleared");
		assert_eq!(references(), 1);
		block_on(Chat::delete_room(
			&runtime,
			&CallContext::default(),
			HostChatDeleteRoomRequest::V1(latest::HostChatDeleteRoomRequest {
				room_id: "other".to_string(),
			}),
		))
		.expect("deleted");
		assert_eq!(references(), 0);
	}

	fn room_events(
		runtime: &ProductRuntimeHost,
	) -> Subscription<HostChatRoomEventsSubscribeItem, CallError<HostChatRoomEventsSubscribeError>>
	{
		block_on(Chat::room_events_subscribe(
			runtime,
			&CallContext::default(),
			HostChatRoomEventsSubscribeRequest::V1,
		))
	}

	fn drain_room_events(
		events: &mut Subscription<
			HostChatRoomEventsSubscribeItem,
			CallError<HostChatRoomEventsSubscribeError>,
		>,
	) -> Vec<latest::HostChatRoomEventsSubscribeItem> {
		let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
		let mut items = Vec::new();
		while let core::task::Poll::Ready(Some(Ok(HostChatRoomEventsSubscribeItem::V1(item)))) =
			events.poll_next_unpin(&mut cx)
		{
			items.push(item);
		}
		items
	}

	fn room_event(
		room_id: &str,
		event: latest::ChatRoomEvent,
	) -> latest::HostChatRoomEventsSubscribeItem {
		latest::HostChatRoomEventsSubscribeItem { room_id: room_id.to_string(), event }
	}

	// S56: a worker that starts learns first the events the host kept for it,
	// then live ones.
	#[test]
	fn room_events_start_with_the_kept_events() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[hosted("main")]));
		let requested = room_event(
			"main",
			latest::ChatRoomEvent::CallRequested { video: true, call_id: Some("c0".to_string()) },
		);
		runtime.connection.publish_chat_room_event(requested.clone()).expect("kept");

		let mut events = room_events(&runtime);
		let ended =
			room_event("main", latest::ChatRoomEvent::CallEnded { call_id: "c1".to_string() });
		runtime.connection.publish_chat_room_event(ended.clone()).expect("sent");

		assert_eq!(drain_room_events(&mut events), [requested, ended]);
	}

	#[test]
	fn the_core_keeps_the_newest_room_events_of_a_product_without_subscriber() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[]));
		let ended = |index: usize| {
			room_event("main", latest::ChatRoomEvent::CallEnded { call_id: format!("c{index}") })
		};
		for index in 0..crate::platform::CHAT_ROOM_EVENTS_QUEUED_MAX + 2 {
			runtime.connection.publish_chat_room_event(ended(index)).expect("kept");
		}

		let kept = drain_room_events(&mut room_events(&runtime));

		assert_eq!(kept.len(), crate::platform::CHAT_ROOM_EVENTS_QUEUED_MAX);
		assert_eq!(kept.first(), Some(&ended(2)));
	}

	// One connection has one subscriber, as for `action_subscribe`.
	#[test]
	fn a_second_room_event_subscription_ends_the_first() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[]));
		let mut first = room_events(&runtime);
		let mut second = room_events(&runtime);
		let ended =
			room_event("main", latest::ChatRoomEvent::CallEnded { call_id: "c1".to_string() });
		runtime.connection.publish_chat_room_event(ended.clone()).expect("sent");

		assert_eq!(block_on(first.next()), None);
		assert_eq!(drain_room_events(&mut second), [ended]);
	}

	#[test]
	fn a_room_event_is_screened_before_the_product_sees_it() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[]));

		assert!(matches!(
			runtime.connection.publish_chat_room_event(room_event(
				"main",
				latest::ChatRoomEvent::CallSignal { call_id: "c1".to_string(), signal: Vec::new() },
			)),
			Err(crate::host_core::ProductRuntimeError::Rejected { .. })
		));
		// The product uses the call id the host chose as a message id.
		assert!(matches!(
			runtime.connection.publish_chat_room_event(room_event(
				"main",
				latest::ChatRoomEvent::CallRequested {
					video: false,
					call_id: Some("c\u{202e}1".to_string()),
				},
			)),
			Err(crate::host_core::ProductRuntimeError::Rejected { .. })
		));
		assert!(drain_room_events(&mut room_events(&runtime)).is_empty());
	}

	fn set_contact(
		runtime: &ProductRuntimeHost,
		room_id: &str,
		contact: latest::ChatRoomContact,
	) -> Result<HostChatSetRoomContactResponse, CallError<HostChatRoomError>> {
		block_on(Chat::set_room_contact(
			runtime,
			&CallContext::default(),
			HostChatSetRoomContactRequest::V1(latest::HostChatSetRoomContactRequest {
				room_id: room_id.to_string(),
				contact,
			}),
		))
	}

	fn alice(state: latest::ChatContactState) -> latest::ChatRoomContact {
		latest::ChatRoomContact { account: [7; 32], username: Some(" alice ".to_string()), state }
	}

	// A room is drawn as a person-to-person chat only on a host that serves
	// it, and the host receives the username as the core screened it.
	#[test]
	fn a_room_contact_needs_the_feature_that_serves_it() {
		let chat = RecordingHost::with_rooms(&[hosted("main")]);
		let runtime = chat_runtime(chat.clone());

		assert_eq!(
			set_contact(&runtime, "main", alice(latest::ChatContactState::Incoming)).map(drop),
			room_unknown("this host does not serve contact_rooms")
		);
		*chat.capabilities.lock().unwrap() = vec!["contact_rooms".to_string()];
		assert_eq!(
			set_contact(&runtime, "main", alice(latest::ChatContactState::Incoming)),
			Ok(HostChatSetRoomContactResponse::V1)
		);
		assert_eq!(
			chat.contacts.lock().unwrap().get("main"),
			Some(&latest::ChatRoomContact {
				account: [7; 32],
				username: Some("alice".to_string()),
				state: latest::ChatContactState::Incoming,
			})
		);
		assert_eq!(
			set_contact(&runtime, "lobby", alice(latest::ChatContactState::Established)),
			Err(CallError::Domain(HostChatRoomError::V1(latest::HostChatRoomError::UnknownRoom)))
		);
		assert_eq!(chat.calls(), ["contact main"]);
	}

	// Only the chat product draws rooms as person-to-person chats, so a host
	// that serves `contact_rooms` serves it to no other product.
	#[test]
	fn only_the_chat_product_has_contact_rooms() {
		let chat = RecordingHost::with_rooms(&[]);
		*chat.capabilities.lock().unwrap() = vec!["contact_rooms".to_string()];

		assert_eq!(capabilities(&chat_runtime(chat.clone())), ["contact_rooms"]);
		assert_eq!(
			capabilities(&chat_runtime_of("games.dot", stub_platform(), chat)),
			Vec::<&str>::new()
		);
	}

	// D28: a notification wake holds the worker once; the thirteenth one in
	// an hour holds nothing.
	#[test]
	fn a_notification_wake_holds_the_worker_once() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[hosted("main")]));
		let notification =
			|| crate::runtime::ChatWakeReason::Notification { room_id: "main".to_string() };

		assert!(matches!(
			runtime.connection.wake(notification()),
			Ok(crate::runtime::ChatWakeOutcome::Held { .. })
		));
		assert_eq!(
			runtime
				.connection
				.services
				.worker_ledger
				.count(&ProductId::for_test("chat.dot")),
			1
		);
		for _ in 1..crate::runtime::CHAT_NOTIFICATION_WAKES_PER_HOUR {
			runtime.connection.wake(notification()).expect("a valid reason");
		}
		assert_eq!(
			runtime
				.connection
				.services
				.worker_ledger
				.count(&ProductId::for_test("chat.dot")),
			1
		);
		assert!(matches!(
			runtime.connection.wake(notification()),
			Ok(crate::runtime::ChatWakeOutcome::BudgetSpent)
		));
		assert!(matches!(
			runtime.connection.wake(crate::runtime::ChatWakeReason::ProductStart),
			Ok(crate::runtime::ChatWakeOutcome::Held { .. })
		));
	}
}
