//! The chat capability that products call.
//!
//! The core screens each chat call before a host sees it. It normalizes and
//! bounds identifiers and texts. A call that changes a room needs a room that
//! the product hosts. A delete refuses only a room that the product reaches as
//! a bot, because the host deletes a room that does not exist without an error.

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
			HostChatRegisterBotResponse, HostChatSetRoomFooterError, HostChatSetRoomFooterRequest,
			HostChatSetRoomFooterResponse, HostChatSetRoomMetadataError,
			HostChatSetRoomMetadataRequest, HostChatSetRoomMetadataResponse,
			HostChatSetRoomPreviewError, HostChatSetRoomPreviewRequest,
			HostChatSetRoomPreviewResponse, HostChatUpdateFeedError, HostChatUpdateFeedRequest,
			HostChatUpdateFeedResponse,
		},
	},
};

use crate::{
	platform::{
		ChatFieldError, ChatPlatform, ChatRoomChangeOutcome, normalize_chat_identifier,
		validate_chat_custom_message, validate_chat_feed_update, validate_chat_icon,
		validate_chat_message_content, validate_chat_name, validate_chat_resource,
		validate_chat_room_metadata, validate_chat_room_preview,
	},
	runtime::{AccountHolder, ProductRuntimeHost, versioning::version},
};

/// The participant id of the host's user, which a product cannot write as.
const USER_PARTICIPANT_ID: &str = "user";

/// The fields that a product makes valid when it sends less content.
/// `MessageTooLarge` names no field, so it can describe only these. `payload`
/// is the payload of a `Custom` message or message body.
const CHAT_SIZED_CONTENT_FIELDS: [&str; 2] = ["text", "payload"];

#[async_trait::async_trait]
impl<H: AccountHolder> Chat for ProductRuntimeHost<H> {
	#[instrument(skip_all, fields(runtime.method = "chat.create_room"))]
	async fn create_room(
		&self,
		_cx: &CallContext,
		request: HostChatCreateRoomRequest,
	) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
		// A v0.1 product names the room with its own `roomId` in every later
		// call, and its answer carries no other id. So its room keeps that id.
		// Only a v0.2 request gets the id that the host derives.
		let product_names_room = matches!(request, HostChatCreateRoomRequest::V1(_));
		version(request)
			.exec(|mut request| async move {
				let platform = self.connection.chat_platform()?;
				let field_error = |error: ChatFieldError| {
					CallError::Domain(latest::HostChatCreateRoomError::Unknown {
						reason: error.to_string(),
					})
				};
				let field = if product_names_room { "roomId" } else { "roomIdSource" };
				request.room_id_source = normalize_chat_identifier(field, &request.room_id_source)
					.map_err(field_error)?;
				request.name = validate_chat_name("name", &request.name).map_err(field_error)?;
				// RFC 0002 makes the icon of a v0.2 room a resource. A v0.1
				// product keeps the icon rules it was built against.
				let validate_icon =
					if product_names_room { validate_chat_icon } else { validate_chat_resource };
				request.icon = validate_icon("icon", &request.icon).map_err(field_error)?;
				let room_id = if product_names_room {
					request.room_id_source.clone()
				} else {
					self.connection.product.product_id.chat_room_id(&request.room_id_source)
				};
				// One host call: the host keeps the configuration with the
				// room, or refuses it before it creates anything.
				let status = platform
					.create_chat_room(&self.connection.product, room_id.clone(), request)
					.await
					.map_err(CallError::Domain)?;
				Ok(latest::HostChatCreateRoomResponse { status, room_id })
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
				// A docked card is screened as a posted custom message is.
				if let latest::ChatRoomFooter::Card(card) = request.footer {
					request.footer = latest::ChatRoomFooter::Card(
						validate_chat_custom_message(card).map_err(|error| {
							CallError::Domain(latest::HostChatSetRoomFooterError::Unknown {
								reason: error.to_string(),
							})
						})?,
					);
				}
				request.room_id = self
					.hosted_room(&platform, &request.room_id)
					.await
					.map_err(footer_refusal)?
					.room_id;
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
				request.room_id = self.hosted_room(&platform, &request.room_id).await?.room_id;
				room_change(
					platform.set_chat_room_metadata(&self.connection.product, request).await,
				)
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
				if preview_has_host_drawn_body(&request.preview) {
					return Err(CallError::Unsupported);
				}
				let room = self.hosted_room(&platform, &request.room_id).await?;
				request.room_id = room.room_id;
				// In a room that the host stores, the host shows the messages and
				// reactions of the user from its own store.
				if !room.config.product_source && preview_shows_the_user(&request.preview) {
					return Err(CallError::Domain(latest::HostChatSetRoomPreviewError::NotOwner));
				}
				room_change(platform.set_chat_room_preview(&self.connection.product, request).await)
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
				request.room_id = match self.hosted_room(&platform, &request.room_id).await {
					Ok(room) => room.room_id,
					// The host deletes a room that does not exist without an
					// error, so a product can send a delete again when it lost
					// the answer. A room that a stale list leaves out is still
					// deleted.
					Err(RoomRefusal::UnknownRoom(room_id)) => room_id,
					Err(RoomRefusal::NotRoomHost) => {
						return Err(CallError::Domain(
							latest::HostChatDeleteRoomError::NotRoomHost,
						));
					},
					Err(RoomRefusal::Malformed(reason)) => {
						return Err(CallError::MalformedFrame { reason });
					},
					Err(RoomRefusal::HostFailure(reason)) => {
						return Err(CallError::HostFailure { reason });
					},
				};
				room_change(platform.delete_chat_room(&self.connection.product, request).await)
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
				if feed_has_host_drawn_body(&request.update) {
					return Err(CallError::Unsupported);
				}
				let room = self.hosted_room(&platform, &request.room_id).await?;
				request.room_id = room.room_id;
				let persist = !room.config.product_source;
				// In a room that the host stores, the user writes and reacts
				// only through the host. A product cannot write or react as the
				// user.
				if persist && names_the_user(&request.update) {
					return Err(CallError::Domain(latest::HostChatUpdateFeedError::NotOwner));
				}
				room_change(
					platform.update_chat_feed(&self.connection.product, request, persist).await,
				)
			})
			.await
	}
}

/// Why the core refuses a room-changing call before the host sees it.
enum RoomRefusal {
	/// The product created no room with this normalized id.
	UnknownRoom(String),
	/// The product takes part in the room as a bot.
	NotRoomHost,
	/// The room id did not pass the core's checks.
	Malformed(String),
	/// The host could not read the room.
	HostFailure(String),
}

/// A room that the product hosts, as the host keeps it.
struct HostedRoom {
	/// The normalized id of the room.
	room_id: String,
	/// The configuration of the last `create_room` of the room.
	config: latest::ChatRoomConfig,
}

/// The error of a room-changing call that names a room the product does not
/// host.
trait RoomCallError {
	/// This product created no room with this id.
	const UNKNOWN_ROOM: Self;
	/// The product takes part in the room as a bot, not as its host.
	const NOT_ROOM_HOST: Self;
}

impl RoomCallError for latest::HostChatSetRoomMetadataError {
	const UNKNOWN_ROOM: Self = Self::UnknownRoom;
	const NOT_ROOM_HOST: Self = Self::NotRoomHost;
}

impl RoomCallError for latest::HostChatSetRoomPreviewError {
	const UNKNOWN_ROOM: Self = Self::UnknownRoom;
	const NOT_ROOM_HOST: Self = Self::NotRoomHost;
}

impl RoomCallError for latest::HostChatUpdateFeedError {
	const UNKNOWN_ROOM: Self = Self::UnknownRoom;
	const NOT_ROOM_HOST: Self = Self::NotRoomHost;
}

impl<E: RoomCallError> From<RoomRefusal> for CallError<E> {
	fn from(refusal: RoomRefusal) -> Self {
		match refusal {
			RoomRefusal::UnknownRoom(_) => CallError::Domain(E::UNKNOWN_ROOM),
			RoomRefusal::NotRoomHost => CallError::Domain(E::NOT_ROOM_HOST),
			RoomRefusal::Malformed(reason) => CallError::MalformedFrame { reason },
			RoomRefusal::HostFailure(reason) => CallError::HostFailure { reason },
		}
	}
}

/// The footer error has no bot variant: a room the product does not host is,
/// for the footer, a room it did not create.
fn footer_refusal(refusal: RoomRefusal) -> CallError<latest::HostChatSetRoomFooterError> {
	CallError::Domain(match refusal {
		RoomRefusal::UnknownRoom(_) | RoomRefusal::NotRoomHost => {
			latest::HostChatSetRoomFooterError::UnknownRoom
		},
		RoomRefusal::Malformed(reason) | RoomRefusal::HostFailure(reason) => {
			latest::HostChatSetRoomFooterError::Unknown { reason }
		},
	})
}

/// Whether `body` is a contact event, a call or a payment. The host draws these
/// with its own strings, and the core does not serve them yet: a product could
/// show a payment or a call that did not happen.
fn is_host_drawn(body: &latest::ChatMessageBody) -> bool {
	matches!(
		body,
		latest::ChatMessageBody::Event(_) |
			latest::ChatMessageBody::Call(_) |
			latest::ChatMessageBody::Payment(_)
	)
}

/// Whether `update` carries a body that the host draws itself: in the message,
/// in the message that a reply quotes, or in an edit.
fn feed_has_host_drawn_body(update: &latest::ChatFeedUpdate) -> bool {
	match update {
		latest::ChatFeedUpdate::New(message) | latest::ChatFeedUpdate::Replaced(message) => {
			let quoted = message.reply.as_ref().and_then(|reply| reply.preview.as_ref());
			is_host_drawn(&message.body) ||
				quoted.is_some_and(|preview| is_host_drawn(&preview.body))
		},
		latest::ChatFeedUpdate::Edited { body, .. } => is_host_drawn(body),
		latest::ChatFeedUpdate::ReactionPlaced { .. } |
		latest::ChatFeedUpdate::ReactionRemoved { .. } |
		latest::ChatFeedUpdate::StatusChanged { .. } |
		latest::ChatFeedUpdate::Deleted { .. } => false,
	}
}

/// Whether `preview` shows a body that the host draws itself.
fn preview_has_host_drawn_body(preview: &latest::ChatRoomPreview) -> bool {
	let host_drawn = |message: &latest::ChatPreviewMessage| is_host_drawn(&message.body);
	let reacted_to = preview.last_reaction.as_ref().and_then(|reaction| reaction.message.as_ref());
	preview.last_message.as_ref().is_some_and(host_drawn) || reacted_to.is_some_and(host_drawn)
}

/// Whether `update` names the host's user as the author of a message, of the
/// message that a reply quotes, of a reaction, or of a reaction that a
/// message's summaries count.
fn names_the_user(update: &latest::ChatFeedUpdate) -> bool {
	match update {
		latest::ChatFeedUpdate::New(message) | latest::ChatFeedUpdate::Replaced(message) => {
			shows_the_user(message)
		},
		latest::ChatFeedUpdate::ReactionPlaced { participant_id, .. } |
		latest::ChatFeedUpdate::ReactionRemoved { participant_id, .. } => {
			participant_id == USER_PARTICIPANT_ID
		},
		latest::ChatFeedUpdate::Edited { .. } |
		latest::ChatFeedUpdate::StatusChanged { .. } |
		latest::ChatFeedUpdate::Deleted { .. } => false,
	}
}

/// Whether the host's user wrote `message` or the message that it quotes, or
/// reacted to it as its reaction summaries say.
fn shows_the_user(message: &latest::ChatMessage) -> bool {
	let quotes_the_user = message
		.reply
		.as_ref()
		.and_then(|reply| reply.preview.as_ref())
		.is_some_and(|preview| preview.origin.participant_id == USER_PARTICIPANT_ID);
	let user_reacted = message.reactions.iter().any(|reaction| reaction.reacted_by_user);
	message.origin.participant_id == USER_PARTICIPANT_ID || quotes_the_user || user_reacted
}

/// Whether `preview` shows a message or a reaction of the host's user.
fn preview_shows_the_user(preview: &latest::ChatRoomPreview) -> bool {
	let by_user =
		|message: &latest::ChatPreviewMessage| message.origin.participant_id == USER_PARTICIPANT_ID;
	let last_message = preview.last_message.as_ref().is_some_and(by_user);
	let last_reaction = preview
		.last_reaction
		.as_ref()
		.is_some_and(|reaction| reaction.by_user || reaction.message.as_ref().is_some_and(by_user));
	last_message || last_reaction
}

/// Report a rejected field of a room-changing call.
fn room_field_error<E>(error: ChatFieldError) -> CallError<E> {
	CallError::MalformedFrame { reason: error.to_string() }
}

/// Report a host failure of a room-changing call.
fn room_host_error<E>(error: latest::GenericError) -> CallError<E> {
	CallError::HostFailure { reason: error.reason }
}

/// The answer to the product for what the host did with a change to a room. A
/// host that does not serve the request answers `Unsupported`, so a product
/// can tell it from a failure worth a retry.
fn room_change<E>(
	outcome: Result<ChatRoomChangeOutcome, latest::GenericError>,
) -> Result<(), CallError<E>> {
	match outcome {
		Ok(ChatRoomChangeOutcome::Applied) => Ok(()),
		Ok(ChatRoomChangeOutcome::Unsupported) => Err(CallError::Unsupported),
		Err(error) => Err(room_host_error(error)),
	}
}

/// Report a rejected feed field, a size rejection of the content as the size
/// variant the protocol declares for it.
fn feed_field_error(error: ChatFieldError) -> CallError<latest::HostChatUpdateFeedError> {
	match error {
		ChatFieldError::TooLong { field, .. } if CHAT_SIZED_CONTENT_FIELDS.contains(&field) => {
			CallError::Domain(latest::HostChatUpdateFeedError::MessageTooLarge)
		},
		error => room_field_error(error),
	}
}

/// Report a rejected `post_message` field, a size rejection of the content as
/// the size variant the protocol declares for it.
fn post_field_error(error: ChatFieldError) -> CallError<latest::HostChatPostMessageError> {
	CallError::Domain(match error {
		// `MessageTooLarge` names no field. Thus the core uses it only for the
		// content that a product can make smaller. Each other rejection uses
		// `Unknown`, and its reason names the field and its limit.
		ChatFieldError::TooLong { field, .. } if CHAT_SIZED_CONTENT_FIELDS.contains(&field) => {
			latest::HostChatPostMessageError::MessageTooLarge
		},
		error => latest::HostChatPostMessageError::Unknown { reason: error.to_string() },
	})
}

/// The normalized id of the room that a room-changing call names. The
/// normalization of `create_room`, so each spelling of a room id finds the
/// stored room.
fn normalized_room_id(room_id: &str) -> Result<String, RoomRefusal> {
	normalize_chat_identifier("roomId", room_id)
		.map_err(|error| RoomRefusal::Malformed(error.to_string()))
}

impl<H: AccountHolder> ProductRuntimeHost<H> {
	/// A room this product hosts, with its normalized id and configuration.
	///
	/// The core reads the room from the host once for each call, so every host
	/// answers a room the product never created, or joined only as a bot, the
	/// same way.
	async fn hosted_room(
		&self,
		platform: &Arc<dyn ChatPlatform>,
		room_id: &str,
	) -> Result<HostedRoom, RoomRefusal> {
		let room_id = normalized_room_id(room_id)?;
		let room = platform
			.chat_room(&self.connection.product, room_id.clone())
			.await
			.map_err(|error| RoomRefusal::HostFailure(error.reason))?;
		match room {
			None => Err(RoomRefusal::UnknownRoom(room_id)),
			Some(room) if room.participating_as != latest::ChatRoomParticipation::RoomHost => {
				Err(RoomRefusal::NotRoomHost)
			},
			Some(room) => Ok(HostedRoom { room_id, config: room.config }),
		}
	}
}

#[cfg(test)]
mod tests;
