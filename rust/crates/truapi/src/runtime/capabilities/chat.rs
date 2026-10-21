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
			HostChatActionSubscribeRequest, HostChatCapabilitiesError, HostChatCapabilitiesRequest,
			HostChatCapabilitiesResponse, HostChatCreateRoomError, HostChatCreateRoomRequest,
			HostChatCreateRoomResponse, HostChatDeleteRoomRequest, HostChatDeleteRoomResponse,
			HostChatListSubscribeError, HostChatListSubscribeItem, HostChatListSubscribeRequest,
			HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
			HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
			HostChatRoomError, HostChatRoomEventsSubscribeError, HostChatRoomEventsSubscribeItem,
			HostChatRoomEventsSubscribeRequest, HostChatSetRoomCallRequest,
			HostChatSetRoomCallResponse, HostChatSetRoomFeaturesRequest,
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
		CHAT_INLINE_MEDIA_MAX_BYTES, CHAT_PERSONA_PREFIX, CHAT_PERSONAS_MAX,
		CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES, CHAT_SEALED_ATTACHMENT_MAX_BYTES, ChatFieldError,
		ChatPlatform, ChatRateLimiter, chat_persona_bot_id, known_chat_feature_names,
		normalize_chat_identifier, validate_chat_feed_update, validate_chat_icon,
		validate_chat_message_content, validate_chat_name, validate_chat_room_call,
		validate_chat_room_config, validate_chat_room_footer, validate_chat_room_metadata,
		validate_chat_room_preview,
	},
	runtime::{ChatHandler, ProductRuntimeHost, renderer_access_for},
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
		request: HostChatActionSubscribeRequest,
	) -> Subscription<HostChatActionSubscribeItem, CallError<HostChatActionSubscribeError>> {
		let version = request.version();
		if let Err(error) = self.connection.chat_platform::<HostChatActionSubscribeError>() {
			return Subscription::interrupted(error);
		}
		// An event the subscriber's version cannot carry is skipped, never
		// sent in a version the subscriber cannot decode.
		let events =
			self.connection
				.chat
				.subscribe::<latest::GenericError>()
				.filter_map(move |item| {
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
					limits.feed.forget(&self.connection.product.product_id, &room_id);
					limits.preview.forget(&self.connection.product.product_id, &room_id);
					limits.call.forget(&self.connection.product.product_id, &room_id);
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

	#[instrument(skip_all, fields(runtime.method = "chat.capabilities"))]
	async fn capabilities(
		&self,
		_cx: &CallContext,
		request: HostChatCapabilitiesRequest,
	) -> Result<HostChatCapabilitiesResponse, CallError<HostChatCapabilitiesError>> {
		let version = request.version();
		self.chat_capabilities()
			.await
			.map(|response| HostChatCapabilitiesResponse::from_latest(response, version))
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
			if let Some(call) = &request.call {
				let feature = match call {
					latest::ChatRoomCall::Active { media_session: Some(_), .. } => "room_calls",
					_ => "call_buttons",
				};
				let served = self.chat_capabilities().await.map_err(|error| {
					error.map_domain(|error| latest::HostChatRoomError::Unknown {
						reason: error.reason,
					})
				})?;
				if !served.features.iter().any(|name| name == feature) {
					return Err(room_failure(format!("this host does not serve {feature}")));
				}
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

	#[instrument(skip_all, fields(runtime.method = "chat.room_events_subscribe"))]
	async fn room_events_subscribe(
		&self,
		_cx: &CallContext,
		request: HostChatRoomEventsSubscribeRequest,
	) -> Subscription<HostChatRoomEventsSubscribeItem, CallError<HostChatRoomEventsSubscribeError>>
	{
		let version = request.version();
		let platform = match self.connection.chat_platform::<HostChatRoomEventsSubscribeError>() {
			Ok(platform) => platform,
			Err(error) => return Subscription::interrupted(error),
		};
		let states = match platform.read_chat_room_user_states(&self.connection.product).await {
			Ok(states) => states,
			Err(error) => {
				warn!(reason = %error.reason, "chat room user states could not be read");
				return Subscription::interrupted(CallError::HostFailure { reason: error.reason });
			},
		};
		let now_ms = current_unix_millis();
		let snapshot = states
			.rooms
			.into_iter()
			.filter(|state| state.muted_at(now_ms) || state.archived)
			.map(|state| latest::HostChatRoomEventsSubscribeItem {
				event: latest::ChatRoomEvent::UserStateChanged {
					muted_until: state.muted_until.filter(|_| state.muted_at(now_ms)),
					archived: state.archived,
				},
				room_id: state.room_id,
			})
			.collect();
		let events = self
			.connection
			.services
			.chat_room_events
			.subscribe::<latest::GenericError>(&self.connection.product.product_id, snapshot)
			.map(move |item| match item {
				Ok(item) => Ok(HostChatRoomEventsSubscribeItem::from_latest(item, version)),
				Err(error) => Err(from_latest_error(error, version)),
			});
		Subscription::new(events)
	}
}

/// The room that a room link `polkadot://<product_id>/-/chat/<room_id>` of
/// `product_id` opens.
fn chat_room_of_link(product_id: &str, link: &str) -> Option<String> {
	use crate::host_logic::dotns::{NavigateDecision, parse_navigate};
	let (identifier, path) = match parse_navigate(link) {
		NavigateDecision::DotName { identifier, path, .. } => (identifier, path),
		NavigateDecision::Localhost { host, path, .. } => (host, path),
		_ => return None,
	};
	if identifier != product_id {
		return None;
	}
	let room = path.strip_prefix("-/chat/")?.split(['?', '#']).next()?;
	normalize_chat_identifier("roomId", &percent_decode(room)?).ok()
}

/// Decode the `%XX` escapes of one URL path segment.
fn percent_decode(segment: &str) -> Option<String> {
	let mut bytes = Vec::with_capacity(segment.len());
	let mut rest = segment.bytes();
	while let Some(byte) = rest.next() {
		if byte != b'%' {
			bytes.push(byte);
			continue;
		}
		let digits = [rest.next()?, rest.next()?];
		bytes.push(u8::from_str_radix(core::str::from_utf8(&digits).ok()?, 16).ok()?);
	}
	String::from_utf8(bytes).ok()
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

/// Whether the core can back a feature a host lists. The core cannot back a
/// product-drawn body without a renderer. It does not serve the Media
/// service yet, and no host draws the payment button yet.
fn core_backs_chat_feature(
	name: &str,
	execution_kind: crate::platform::ProductExecutionKind,
) -> bool {
	match name {
		"custom_bodies" | "custom_footer" => renderer_access_for(execution_kind).is_ok(),
		"room_calls" | "payment_button" => false,
		_ => true,
	}
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
		let config = platform
			.read_chat_room_config(&self.connection.product, room_id.clone())
			.await
			.map_err(|error| error.reason)?;
		match handler {
			ChatHandler::Config(declared) => Ok(declared(&config)),
			ChatHandler::Search if !config.product_source => Ok(false),
			ChatHandler::Search => platform
				.read_chat_room_features(&self.connection.product, room_id)
				.await
				.map(|features| features.search)
				.map_err(|error| error.reason),
		}
	}

	/// The Chat features the host lists that the core knows and can back,
	/// with the attachment bounds of the core.
	pub async fn chat_capabilities(
		&self,
	) -> Result<latest::HostChatCapabilitiesResponse, CallError<latest::GenericError>> {
		let platform = self.connection.chat_platform()?;
		let listed = platform
			.chat_capabilities(&self.connection.product)
			.await
			.map_err(CallError::Domain)?;
		let features: Vec<String> = known_chat_feature_names(&listed.names)
			.into_iter()
			.filter(|name| core_backs_chat_feature(name, self.connection.product.execution_kind))
			.filter(|name| *name != "hop_attachments" || self.backs_hop_attachments())
			.map(str::to_string)
			.collect();
		let stages = features.iter().any(|name| name == "preimage_attachments");
		let seals = features.iter().any(|name| name == "sealed_attachments");
		Ok(latest::HostChatCapabilitiesResponse {
			features,
			inline_attachment_max_bytes: CHAT_INLINE_MEDIA_MAX_BYTES as u32,
			preimage_attachment_max_bytes: if stages {
				CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES as u32
			} else {
				0
			},
			sealed_attachment_max_bytes: if seals {
				CHAT_SEALED_ATTACHMENT_MAX_BYTES as u32
			} else {
				0
			},
		})
	}

	/// Refuse a participant id `bot:<bot_id>` that names no persona the
	/// product registered. The host list of personas is read only when a
	/// persona id is present.
	pub async fn check_chat_personas<'a>(
		&self,
		platform: &Arc<dyn ChatPlatform>,
		participant_ids: impl IntoIterator<Item = &'a str>,
	) -> Result<(), String> {
		let bots: Vec<&str> = participant_ids.into_iter().filter_map(chat_persona_bot_id).collect();
		if bots.is_empty() {
			return Ok(());
		}
		let registered = platform
			.read_chat_bots(&self.connection.product)
			.await
			.map_err(|error| error.reason)?
			.bot_ids;
		match bots.into_iter().find(|bot| !registered.iter().any(|known| known == bot)) {
			Some(unknown) => Err(format!("unknown bot {unknown}")),
			None => Ok(()),
		}
	}

	/// Whether `deeplink` opens a room of this product that the user muted.
	/// A link that names no room of the product, or a host that cannot tell,
	/// gives `false`.
	pub async fn chat_room_muted(&self, deeplink: &str) -> bool {
		let Some(platform) = self.connection.chat_platform.as_ref() else { return false };
		let Some(room_id) = chat_room_of_link(&self.connection.product.product_id, deeplink) else {
			return false;
		};
		match platform.read_chat_room_user_states(&self.connection.product).await {
			Ok(states) => {
				let now_ms = current_unix_millis();
				states
					.rooms
					.iter()
					.any(|state| state.room_id == room_id && state.muted_at(now_ms))
			},
			Err(error) => {
				warn!(reason = %error.reason, "chat room user states could not be read");
				false
			},
		}
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
				&self.connection.product.product_id,
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
		icon: Option<String>,
	) -> Result<latest::HostChatCreateRoomResponse, CallError<latest::HostChatCreateRoomError>> {
		let platform = self.connection.chat_platform()?;
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
			.create_chat_room(&self.connection.product, request, icon_url)
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
		// The persona speaks as `bot:<bot_id>`, which must be an identifier too.
		normalize_chat_identifier("botId", &format!("{CHAT_PERSONA_PREFIX}{}", request.bot_id))
			.map_err(field_error)?;
		request.name = validate_chat_name("name", &request.name).map_err(field_error)?;
		request.icon = validate_chat_icon("icon", &request.icon).map_err(field_error)?;
		let failure = |reason: String| {
			CallError::Domain(latest::HostChatRegisterBotError::Unknown { reason })
		};
		let personas = platform
			.read_chat_bots(&self.connection.product)
			.await
			.map_err(|error| failure(error.reason))?
			.bot_ids;
		if personas.len() >= CHAT_PERSONAS_MAX && !personas.contains(&request.bot_id) {
			return Err(failure(format!("a product registers at most {CHAT_PERSONAS_MAX} bots")));
		}
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
		request.footer = validate_chat_room_footer(request.footer)
			.map_err(|error| failure(error.to_string()))?;
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
		if let latest::ChatFeedUpdate::New(message) = &request.update {
			self.check_chat_personas(&platform, [message.origin.participant_id.as_str()])
				.await
				.map_err(|reason| {
					CallError::Domain(latest::HostChatUpdateFeedError::Unknown { reason })
				})?;
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
		capabilities: Mutex<Vec<String>>,
		features: Mutex<BTreeMap<String, latest::ChatRoomFeatures>>,
		user_states: Mutex<Vec<crate::platform::ChatRoomUserState>>,
		bots: Mutex<Vec<String>>,
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

		async fn read_chat_room_user_states(
			&self,
			_product: &ProductContext,
		) -> Result<crate::platform::ChatRoomUserStates, latest::GenericError> {
			Ok(crate::platform::ChatRoomUserStates {
				rooms: self.user_states.lock().unwrap().clone(),
			})
		}

		async fn read_chat_bots(
			&self,
			_product: &ProductContext,
		) -> Result<crate::platform::ChatBotIds, latest::GenericError> {
			Ok(crate::platform::ChatBotIds { bot_ids: self.bots.lock().unwrap().clone() })
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
		let (host_config, _) = runtime_config("chat.dot");
		let product = ProductContext::new_with_execution(
			"chat.dot".to_string(),
			ProductExecutionKind::Worker,
		)
		.expect("test chat product context is valid");
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
			pairing_host,
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
			.connection
			.publish_chat_event(event(latest::ChatEvent::UserDeleted {
				message_id: "m1".to_string(),
			}))
			.expect("published");
		runtime
			.connection
			.publish_chat_event(event(command.clone()))
			.expect("published");

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
			.connection
			.publish_chat_event(event(latest::ChatEvent::UserDeleted {
				message_id: "m2".to_string(),
			}))
			.expect("published");
		assert_eq!(
			drain(&mut v2),
			[HostChatActionSubscribeItem::V2(event(latest::ChatEvent::UserDeleted {
				message_id: "m2".to_string()
			}))]
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
			runtime.connection.publish_chat_event(seen(index)).expect("published");
		}
		assert_eq!(drain(&mut events), [HostChatActionSubscribeItem::V2(seen(0))]);

		std::thread::sleep(core::time::Duration::from_millis(1_300));
		assert_eq!(drain(&mut events), [HostChatActionSubscribeItem::V2(seen(99))]);
	}

	fn capabilities(runtime: &ProductRuntimeHost) -> latest::HostChatCapabilitiesResponse {
		match block_on(Chat::capabilities(
			runtime,
			&CallContext::default(),
			HostChatCapabilitiesRequest::V1,
		)) {
			Ok(HostChatCapabilitiesResponse::V1(response)) => response,
			other => panic!("capabilities failed: {other:?}"),
		}
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

	// S49: the product reads the features the host serves end to end and the
	// core can back, in one order, and the attachment bounds of the core.
	#[test]
	fn capabilities_are_the_host_list_less_what_the_core_cannot_back() {
		let chat = RecordingHost::with_rooms(&[]);
		*chat.capabilities.lock().unwrap() = [
			"search",
			"call_buttons",
			"room_calls",
			"payment_button",
			"preimage_attachments",
			"sealed_attachments",
			"custom_footer",
			"a_name_from_a_later_rfc",
			"search",
		]
		.map(str::to_string)
		.to_vec();
		let runtime = chat_runtime(chat);

		assert_eq!(
			capabilities(&runtime),
			latest::HostChatCapabilitiesResponse {
				features: [
					"custom_footer",
					"preimage_attachments",
					"sealed_attachments",
					"search",
					"call_buttons"
				]
				.map(str::to_string)
				.to_vec(),
				inline_attachment_max_bytes: 1024 * 1024,
				preimage_attachment_max_bytes: 8 * 1024 * 1024,
				sealed_attachment_max_bytes: 32 * 1024 * 1024,
			}
		);
	}

	#[test]
	fn a_host_that_lists_nothing_serves_the_v2_features_only() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[]));

		assert_eq!(capabilities(&runtime).features, Vec::<String>::new());
	}

	// S51: the host keeps a declaration whole, a feature it does not serve
	// included, so that a later build applies it.
	#[test]
	fn the_features_of_a_room_are_stored_whole_even_when_unserved() {
		let chat = RecordingHost::with_rooms(&[hosted("support")]);
		let runtime = chat_runtime(chat.clone());
		let features = latest::ChatRoomFeatures {
			preimage_attachments: true,
			read_aloud: true,
			calls: Some(latest::ChatRoomCalls { audio: true, video: false }),
			..Default::default()
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

		*chat.capabilities.lock().unwrap() =
			["call_buttons", "room_calls"].map(str::to_string).to_vec();
		std::thread::sleep(core::time::Duration::from_millis(1_000));
		assert_eq!(
			set_call(&runtime, "main", Some(ringing())),
			Ok(HostChatSetRoomCallResponse::V1)
		);
		// The core serves no Media service, so it never backs `room_calls`.
		assert_eq!(
			set_call(
				&runtime,
				"main",
				Some(latest::ChatRoomCall::Active {
					call_id: "c1".to_string(),
					video: true,
					started_at: 1,
					media_session: Some([7; 32]),
				}),
			)
			.map(drop),
			room_unknown("this host does not serve room_calls")
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
		let references = || runtime.connection.services.worker_ledger.count("chat.dot");

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

	// S56: a worker that starts learns first which rooms the user muted or
	// archived, then the events the host kept for it, then live ones.
	#[test]
	fn room_events_start_with_the_muted_and_archived_rooms() {
		let chat = RecordingHost::with_rooms(&[hosted("main")]);
		let state = |room_id: &str, muted_until, archived| crate::platform::ChatRoomUserState {
			room_id: room_id.to_string(),
			muted_until,
			archived,
		};
		*chat.user_states.lock().unwrap() = vec![
			state("muted", Some(u64::MAX), false),
			state("archived", Some(1), true),
			state("mute-ended", Some(1), false),
		];
		let runtime = chat_runtime(chat);
		let requested = room_event("main", latest::ChatRoomEvent::CallRequested { video: true });
		runtime.connection.publish_chat_room_event(requested.clone()).expect("kept");

		let mut events = room_events(&runtime);
		let selected =
			room_event("main", latest::ChatRoomEvent::CallOpened { call_id: "c1".to_string() });
		runtime.connection.publish_chat_room_event(selected.clone()).expect("sent");

		assert_eq!(
			drain_room_events(&mut events),
			[
				room_event(
					"muted",
					latest::ChatRoomEvent::UserStateChanged {
						muted_until: Some(u64::MAX),
						archived: false
					}
				),
				room_event(
					"archived",
					latest::ChatRoomEvent::UserStateChanged { muted_until: None, archived: true }
				),
				requested,
				selected,
			]
		);
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
		let opened =
			room_event("main", latest::ChatRoomEvent::CallOpened { call_id: "c1".to_string() });
		runtime.connection.publish_chat_room_event(opened.clone()).expect("sent");

		assert_eq!(block_on(first.next()), None);
		assert_eq!(drain_room_events(&mut second), [opened]);
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
		assert!(drain_room_events(&mut room_events(&runtime)).is_empty());
	}

	#[test]
	fn a_product_registers_at_most_eight_personas() {
		let chat = RecordingHost::with_rooms(&[]);
		*chat.bots.lock().unwrap() =
			(0..CHAT_PERSONAS_MAX).map(|index| format!("b{index}")).collect();
		let runtime = chat_runtime(chat);
		let register = |bot_id: &str| {
			block_on(Chat::register_bot(
				&runtime,
				&CallContext::default(),
				HostChatRegisterBotRequest::V1(latest::HostChatRegisterBotRequest {
					bot_id: bot_id.to_string(),
					name: "Bot".to_string(),
					icon: String::new(),
				}),
			))
		};

		assert!(register("b0").is_ok(), "a repeat replaces a persona");
		assert!(matches!(
			register("b8"),
			Err(CallError::Domain(HostChatRegisterBotError::V1(
				latest::HostChatRegisterBotError::Unknown { .. }
			)))
		));
	}

	// D34: a link into a muted room of the product names the room; other
	// links do not.
	#[test]
	fn a_room_link_names_a_room_of_its_product() {
		assert_eq!(
			chat_room_of_link("chat.dot", "polkadot://chat.dot/-/chat/main"),
			Some("main".to_string())
		);
		assert_eq!(
			chat_room_of_link("chat.dot", "polkadot://chat.dot/-/chat/a%20b?x=1"),
			Some("a b".to_string())
		);
		assert_eq!(chat_room_of_link("chat.dot", "polkadot://other.dot/-/chat/main"), None);
		assert_eq!(chat_room_of_link("chat.dot", "polkadot://chat.dot/-/pocket"), None);
		assert_eq!(chat_room_of_link("chat.dot", "https://example.com/-/chat/main"), None);
	}

	#[test]
	fn a_notification_into_a_muted_room_is_silenced() {
		let chat = RecordingHost::with_rooms(&[hosted("main")]);
		*chat.user_states.lock().unwrap() = vec![crate::platform::ChatRoomUserState {
			room_id: "main".to_string(),
			muted_until: Some(u64::MAX),
			archived: false,
		}];
		let runtime = chat_runtime(chat.clone());

		assert!(block_on(runtime.chat_room_muted("polkadot://chat.dot/-/chat/main")));
		assert!(!block_on(runtime.chat_room_muted("polkadot://chat.dot/-/chat/other")));
		chat.user_states.lock().unwrap()[0].muted_until = Some(1);
		assert!(!block_on(runtime.chat_room_muted("polkadot://chat.dot/-/chat/main")));
	}

	// D34, S54: an immediate notification into a muted room gets an id and
	// shows nothing. A scheduled one, which the OS may fire, and one into
	// another room still reach the host.
	#[test]
	fn the_host_shows_no_immediate_notification_into_a_muted_room() {
		use truapi::{api::Notifications, versioned::notifications::HostPushNotificationRequest};

		let platform = stub_platform();
		let chat = RecordingHost::with_rooms(&[hosted("main"), hosted("other")]);
		*chat.user_states.lock().unwrap() = vec![crate::platform::ChatRoomUserState {
			room_id: "main".to_string(),
			muted_until: Some(u64::MAX),
			archived: true,
		}];
		let runtime = chat_runtime_on(platform.clone(), chat);
		let notification =
			|room_id: &str, scheduled_at: Option<u64>| latest::HostPushNotificationRequest {
				text: "Ping".to_string(),
				deeplink: Some(format!("polkadot://chat.dot/-/chat/{room_id}")),
				scheduled_at,
			};
		let send = |request: latest::HostPushNotificationRequest| {
			block_on(Notifications::send_push_notification(
				&runtime,
				&CallContext::default(),
				HostPushNotificationRequest::V1(request),
			))
			.expect("a notification gets an id")
		};

		send(notification("main", None));
		assert!(platform.pushed_notifications.lock().unwrap().is_empty());
		send(notification("main", Some(1_776_144_000_000)));
		send(notification("other", None));
		assert_eq!(
			*platform.pushed_notifications.lock().unwrap(),
			[notification("main", Some(1_776_144_000_000)), notification("other", None)]
		);
	}

	#[test]
	fn a_search_is_asked_only_of_a_product_source_room_that_declares_it() {
		let chat = RecordingHost::with_rooms(&[hosted("kept"), hosted("stored")]);
		let runtime = chat_runtime(chat.clone());
		configure(
			&chat,
			"kept",
			latest::ChatRoomConfig { product_source: true, ..Default::default() },
		);
		let declared = |room_id: &str| {
			block_on(runtime.chat_handler_declared(room_id.to_string(), ChatHandler::Search))
		};

		assert_eq!(declared("kept"), Ok(false));
		let search = latest::ChatRoomFeatures { search: true, ..Default::default() };
		set_features(&runtime, "kept", search.clone()).expect("declared");
		set_features(&runtime, "stored", search).expect("declared");
		assert_eq!(declared("kept"), Ok(true));
		assert_eq!(declared("stored"), Ok(false));
	}

	// D28: a notification wake holds the worker once and tells the room; the
	// thirteenth one in an hour holds nothing.
	#[test]
	fn a_notification_wake_holds_the_worker_and_tells_the_room() {
		let runtime = chat_runtime(RecordingHost::with_rooms(&[hosted("main")]));
		let notification = || crate::runtime::ChatWakeReason::Notification {
			room_id: "main".to_string(),
			watch_id: "dev".to_string(),
		};

		assert!(matches!(
			runtime.connection.wake(notification()),
			Ok(crate::runtime::ChatWakeOutcome::Held { .. })
		));
		assert_eq!(runtime.connection.services.worker_ledger.count("chat.dot"), 1);
		for _ in 1..crate::runtime::CHAT_NOTIFICATION_WAKES_PER_HOUR {
			runtime.connection.wake(notification()).expect("a valid reason");
		}
		assert_eq!(runtime.connection.services.worker_ledger.count("chat.dot"), 1);
		assert!(matches!(
			runtime.connection.wake(notification()),
			Ok(crate::runtime::ChatWakeOutcome::BudgetSpent)
		));
		assert!(matches!(
			runtime.connection.wake(crate::runtime::ChatWakeReason::ProductStart),
			Ok(crate::runtime::ChatWakeOutcome::Held { .. })
		));

		let woken = drain_room_events(&mut room_events(&runtime));
		assert_eq!(woken.len(), crate::runtime::CHAT_NOTIFICATION_WAKES_PER_HOUR);
		assert_eq!(
			woken[0],
			room_event("main", latest::ChatRoomEvent::Woken { watch_id: "dev".to_string() })
		);
	}
}
