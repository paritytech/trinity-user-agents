use std::{collections::BTreeMap, sync::Mutex};

use futures::{executor::block_on, stream::BoxStream};
use truapi::v01;

use super::*;
use crate::{
	platform::{ExecutableKind, ProductContext, ProductId},
	runtime::{
		HostRole, PairingHost, RemoteAccountHolder, RuntimeServices, tests::install_pairing_session,
	},
	test_support::{runtime_config, session_info, stub_platform},
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
	/// The failure reason of `read_chat_room_config`, if any.
	config_failure: Mutex<Option<String>>,
	/// The host serves no preview, delete or feed update, as a host that keeps
	/// the defaults of `ChatPlatform`.
	unserved: Mutex<bool>,
	/// The host keeps only the default configuration, as a host that does
	/// not serve the configurations yet.
	default_config_only: Mutex<bool>,
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

	/// Record a room change, or answer that the host does not serve it.
	fn change(
		&self,
		call: &str,
		room_id: &str,
	) -> Result<ChatRoomChangeOutcome, latest::GenericError> {
		if *self.unserved.lock().unwrap() {
			return Ok(ChatRoomChangeOutcome::Unsupported);
		}
		self.record(call, room_id)?;
		Ok(ChatRoomChangeOutcome::Applied)
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

#[async_trait::async_trait]
impl ChatPlatform for RecordingHost {
	async fn create_chat_room(
		&self,
		_product: &ProductContext,
		room_id: String,
		request: latest::HostChatCreateRoomRequest,
	) -> Result<latest::ChatRoomRegistrationStatus, latest::HostChatCreateRoomError> {
		if *self.default_config_only.lock().unwrap() &&
			request.config != latest::ChatRoomConfig::default()
		{
			return Err(latest::HostChatCreateRoomError::UnsupportedConfig);
		}
		self.icons.lock().unwrap().push(request.icon);
		self.rooms
			.lock()
			.unwrap()
			.insert(room_id.clone(), latest::ChatRoomParticipation::RoomHost);
		self.configs.lock().unwrap().insert(room_id.clone(), request.config);
		let _ = self.record("create", &room_id);
		Ok(latest::ChatRoomRegistrationStatus::New)
	}

	async fn register_chat_bot(
		&self,
		_product: &ProductContext,
		_request: latest::HostChatRegisterBotRequest,
	) -> Result<latest::HostChatRegisterBotResponse, latest::HostChatRegisterBotError> {
		Ok(latest::HostChatRegisterBotResponse { status: latest::ChatBotRegistrationStatus::New })
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
		if let Some(reason) = self.config_failure.lock().unwrap().clone() {
			return Err(latest::GenericError { reason });
		}
		Ok(self.configs.lock().unwrap().get(&room_id).cloned().unwrap_or_default())
	}

	async fn set_chat_room_metadata(
		&self,
		_product: &ProductContext,
		request: latest::HostChatSetRoomMetadataRequest,
	) -> Result<(), latest::GenericError> {
		self.record("metadata", &request.room_id)
	}

	async fn set_chat_room_preview(
		&self,
		_product: &ProductContext,
		request: latest::HostChatSetRoomPreviewRequest,
	) -> Result<ChatRoomChangeOutcome, latest::GenericError> {
		self.change("preview", &request.room_id)
	}

	async fn delete_chat_room(
		&self,
		_product: &ProductContext,
		request: latest::HostChatDeleteRoomRequest,
	) -> Result<ChatRoomChangeOutcome, latest::GenericError> {
		if let Some(reason) = self.delete_failure.lock().unwrap().clone() {
			return Err(latest::GenericError { reason });
		}
		let outcome = self.change("delete", &request.room_id)?;
		if outcome == ChatRoomChangeOutcome::Applied {
			self.rooms.lock().unwrap().remove(&request.room_id);
		}
		Ok(outcome)
	}

	async fn update_chat_feed(
		&self,
		_product: &ProductContext,
		request: latest::HostChatUpdateFeedRequest,
		persist: bool,
	) -> Result<ChatRoomChangeOutcome, latest::GenericError> {
		if *self.unserved.lock().unwrap() {
			return Ok(ChatRoomChangeOutcome::Unsupported);
		}
		self.feed.lock().unwrap().push((request, persist));
		Ok(ChatRoomChangeOutcome::Applied)
	}
}

fn chat_runtime(chat: Arc<RecordingHost>) -> ProductRuntimeHost<RemoteAccountHolder> {
	let (host_config, _) = runtime_config("chat.dot");
	let product =
		ProductContext::new_with_execution(ProductId::for_test("chat.dot"), ExecutableKind::Worker);
	let services = RuntimeServices::new(
		stub_platform(),
		host_config.host.host_info.clone(),
		host_config.stack.clone(),
		crate::Spawner::for_test(),
	);
	let pairing_host = PairingHost::new(services.clone(), host_config);
	let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
	adapters.chat_platform = Some(chat);
	let host = ProductRuntimeHost::from_services(
		services,
		adapters,
		pairing_host.accounts().clone(),
		pairing_host.session().clone(),
		product,
	);
	install_pairing_session(&host, session_info());
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
		reply: None,
		edited_at: None,
		reactions: Vec::new(),
		request: false,
	})
}

fn update_feed(
	runtime: &ProductRuntimeHost<RemoteAccountHolder>,
	room_id: &str,
	update: latest::ChatFeedUpdate,
) -> Result<HostChatUpdateFeedResponse, CallError<HostChatUpdateFeedError>> {
	block_on(Chat::update_feed(
		runtime,
		&CallContext::default(),
		HostChatUpdateFeedRequest::V2(latest::HostChatUpdateFeedRequest {
			room_id: room_id.to_string(),
			update,
		}),
	))
}

fn set_metadata(
	runtime: &ProductRuntimeHost<RemoteAccountHolder>,
	room_id: &str,
	name: &str,
) -> Result<HostChatSetRoomMetadataResponse, CallError<HostChatSetRoomMetadataError>> {
	block_on(Chat::set_room_metadata(
		runtime,
		&CallContext::default(),
		HostChatSetRoomMetadataRequest::V2(latest::HostChatSetRoomMetadataRequest {
			room_id: room_id.to_string(),
			metadata: latest::ChatRoomMetadata {
				name: name.to_string(),
				description: None,
				icon: None,
				placeholder: None,
			},
		}),
	))
}

fn delete_room(
	runtime: &ProductRuntimeHost<RemoteAccountHolder>,
	room_id: &str,
) -> Result<HostChatDeleteRoomResponse, CallError<HostChatDeleteRoomError>> {
	block_on(Chat::delete_room(
		runtime,
		&CallContext::default(),
		HostChatDeleteRoomRequest::V2(latest::HostChatDeleteRoomRequest {
			room_id: room_id.to_string(),
		}),
	))
}

fn empty_preview() -> latest::ChatRoomPreview {
	latest::ChatRoomPreview {
		timestamp: None,
		badge: None,
		last_message: None,
		last_reaction: None,
		unseen_reactions: 0,
		pinned: false,
		activity: None,
	}
}

fn set_preview(
	runtime: &ProductRuntimeHost<RemoteAccountHolder>,
	room_id: &str,
	preview: latest::ChatRoomPreview,
) -> Result<HostChatSetRoomPreviewResponse, CallError<HostChatSetRoomPreviewError>> {
	block_on(Chat::set_room_preview(
		runtime,
		&CallContext::default(),
		HostChatSetRoomPreviewRequest::V2(latest::HostChatSetRoomPreviewRequest {
			room_id: room_id.to_string(),
			preview,
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
	let preview = |room_id: &str| set_preview(&runtime, room_id, empty_preview());

	assert_eq!(
		preview("missing"),
		Err(CallError::Domain(HostChatSetRoomPreviewError::V2(
			latest::HostChatSetRoomPreviewError::UnknownRoom
		)))
	);
	assert_eq!(
		preview("lobby"),
		Err(CallError::Domain(HostChatSetRoomPreviewError::V2(
			latest::HostChatSetRoomPreviewError::NotRoomHost
		)))
	);
	assert_eq!(preview(" support "), Ok(HostChatSetRoomPreviewResponse::V2));
	assert_eq!(chat.calls(), ["preview support"]);
}

// Each room change answers a room the product does not host with the variants
// of its own error, so a product can tell a room it never created from one
// it reaches as a bot.
#[test]
fn metadata_and_feed_changes_need_a_room_the_product_hosts() {
	let chat = RecordingHost::with_rooms(&[
		hosted("support"),
		("lobby", latest::ChatRoomParticipation::Bot),
	]);
	let runtime = chat_runtime(chat.clone());
	let metadata = |room_id: &str| set_metadata(&runtime, room_id, "Support");
	let feed = |room_id: &str| update_feed(&runtime, room_id, text_message("product"));

	assert_eq!(
		metadata("missing"),
		Err(CallError::Domain(HostChatSetRoomMetadataError::V2(
			latest::HostChatSetRoomMetadataError::UnknownRoom
		)))
	);
	assert_eq!(
		metadata("lobby"),
		Err(CallError::Domain(HostChatSetRoomMetadataError::V2(
			latest::HostChatSetRoomMetadataError::NotRoomHost
		)))
	);
	assert_eq!(
		feed("missing"),
		Err(CallError::Domain(HostChatUpdateFeedError::V2(
			latest::HostChatUpdateFeedError::UnknownRoom
		)))
	);
	assert_eq!(
		feed("lobby"),
		Err(CallError::Domain(HostChatUpdateFeedError::V2(
			latest::HostChatUpdateFeedError::NotRoomHost
		)))
	);
	assert_eq!(metadata(" support "), Ok(HostChatSetRoomMetadataResponse::V2));
	assert_eq!(chat.calls(), ["metadata support"]);
	assert!(chat.feed.lock().unwrap().is_empty(), "a refused update must not reach the host");
}

// A field that fails the screen never reaches the host, and the product reads
// which field failed and why.
#[test]
fn a_field_that_fails_the_screen_is_a_malformed_frame() {
	let chat = RecordingHost::with_rooms(&[hosted("support")]);
	let runtime = chat_runtime(chat.clone());
	fn malformed<E>(error: ChatFieldError) -> CallError<E> {
		CallError::MalformedFrame { reason: error.to_string() }
	}
	let activity = latest::ChatRoomPreview {
		activity: Some(latest::ChatRoomActivity { text: " ".to_string(), ongoing: true }),
		..empty_preview()
	};
	let unnamed = latest::ChatFeedUpdate::Deleted { message_id: String::new() };

	assert_eq!(
		set_metadata(&runtime, "support", "Support\u{202e}"),
		Err(malformed(ChatFieldError::UnsafeCharacter { field: "name" }))
	);
	assert_eq!(
		set_preview(&runtime, "support", activity),
		Err(malformed(ChatFieldError::Empty { field: "activity" }))
	);
	assert_eq!(
		update_feed(&runtime, "support", unnamed),
		Err(malformed(ChatFieldError::Empty { field: "messageId" }))
	);
	assert!(chat.calls().is_empty(), "a field the screen refuses must not reach the host");
	assert!(
		chat.feed.lock().unwrap().is_empty(),
		"a field the screen refuses must not reach the host"
	);
}

// The core decides from the configuration whether the host stores the update.
// A configuration it cannot read fails the update: the default would make the
// host store the messages of a room whose messages the product keeps.
#[test]
fn a_feed_update_fails_when_the_host_cannot_read_the_configuration() {
	let chat = RecordingHost::with_rooms(&[hosted("sourced")]);
	configure(
		&chat,
		"sourced",
		latest::ChatRoomConfig { product_source: true, ..Default::default() },
	);
	*chat.config_failure.lock().unwrap() = Some("store locked".to_string());
	let runtime = chat_runtime(chat.clone());

	assert_eq!(
		update_feed(&runtime, "sourced", text_message("product")),
		Err(CallError::HostFailure { reason: "store locked".to_string() })
	);
	assert!(
		chat.feed.lock().unwrap().is_empty(),
		"without the configuration the host cannot know whether to store the update"
	);
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

/// A message of the product that quotes a message of `quoted_participant_id`.
fn quoting(quoted_participant_id: &str) -> latest::ChatMessage {
	let latest::ChatFeedUpdate::New(mut message) = text_message("product") else {
		unreachable!("text_message builds a new message")
	};
	message.reply = Some(latest::ChatReply {
		message_id: "m0".to_string(),
		preview: Some(latest::ChatReplyPreview {
			origin: latest::ChatMessageOrigin { participant_id: quoted_participant_id.to_string() },
			body: latest::ChatMessageBody::Text { text: "I agree to pay".to_string() },
		}),
	});
	message
}

/// A message of the product whose reaction summary counts the user.
fn reacted_by_the_user() -> latest::ChatMessage {
	let latest::ChatFeedUpdate::New(mut message) = text_message("product") else {
		unreachable!("text_message builds a new message")
	};
	message.reactions = vec![latest::ChatReactionSummary {
		emoji: "👍".to_string(),
		count: 1,
		reacted_by_user: true,
		unseen: false,
	}];
	message
}

/// A new and a replaced message of `participant_id`, and a reaction of
/// `participant_id` placed and removed.
fn updates_by(participant_id: &str) -> [latest::ChatFeedUpdate; 4] {
	let latest::ChatFeedUpdate::New(message) = text_message(participant_id) else {
		unreachable!("text_message builds a new message")
	};
	[
		latest::ChatFeedUpdate::New(message.clone()),
		latest::ChatFeedUpdate::Replaced(message),
		latest::ChatFeedUpdate::ReactionPlaced {
			message_id: "m1".to_string(),
			participant_id: participant_id.to_string(),
			emoji: "👍".to_string(),
		},
		latest::ChatFeedUpdate::ReactionRemoved {
			message_id: "m1".to_string(),
			participant_id: participant_id.to_string(),
			emoji: "👍".to_string(),
		},
	]
}

/// Previews that show a message or a reaction of the user.
fn previews_of_the_user() -> [latest::ChatRoomPreview; 3] {
	let user_message = latest::ChatPreviewMessage {
		message_id: "m1".to_string(),
		origin: latest::ChatMessageOrigin { participant_id: "user".to_string() },
		timestamp: 1,
		body: latest::ChatMessageBody::Text { text: "hi".to_string() },
		status: latest::ChatMessageStatus::Sent,
		reply_to: None,
		edited_at: None,
		request: false,
	};
	let reaction_of_the_user = latest::ChatPreviewReaction {
		emoji: "👍".to_string(),
		by_user: true,
		message: None,
		removed: false,
	};
	let reaction_to_the_user = latest::ChatPreviewReaction {
		by_user: false,
		message: Some(user_message.clone()),
		..reaction_of_the_user.clone()
	};
	[
		latest::ChatRoomPreview { last_message: Some(user_message), ..empty_preview() },
		latest::ChatRoomPreview { last_reaction: Some(reaction_of_the_user), ..empty_preview() },
		latest::ChatRoomPreview { last_reaction: Some(reaction_to_the_user), ..empty_preview() },
	]
}

// The host draws a contact event, a call and a payment with its own strings,
// so a product could show a payment or a call that did not happen. The core
// does not serve them yet, in any room, and the host never sees one.
#[test]
fn a_contact_event_call_or_payment_is_unsupported() {
	let chat = RecordingHost::with_rooms(&[hosted("stored"), hosted("sourced")]);
	configure(
		&chat,
		"sourced",
		latest::ChatRoomConfig { product_source: true, ..Default::default() },
	);
	let runtime = chat_runtime(chat.clone());
	let bodies = [
		latest::ChatMessageBody::Event(latest::ChatContactEvent::ContactAdded),
		latest::ChatMessageBody::Call(latest::ChatCallRecord {
			video: false,
			answered_at: None,
			closed: None,
		}),
		latest::ChatMessageBody::Payment(latest::ChatPaymentRecord { amount: 1, legacy: false }),
	];

	for room_id in ["stored", "sourced"] {
		for body in &bodies {
			let latest::ChatFeedUpdate::New(mut message) = text_message("product") else {
				unreachable!("text_message builds a new message")
			};
			message.body = body.clone();
			let mut quote = quoting("contact");
			if let Some(preview) = quote.reply.as_mut().and_then(|reply| reply.preview.as_mut()) {
				preview.body = body.clone();
			}
			let edit = latest::ChatFeedUpdate::Edited {
				message_id: "m1".to_string(),
				body: body.clone(),
				edited_at: 2,
			};
			for update in
				[latest::ChatFeedUpdate::New(message), latest::ChatFeedUpdate::New(quote), edit]
			{
				assert_eq!(update_feed(&runtime, room_id, update), Err(CallError::Unsupported));
			}

			let preview = latest::ChatRoomPreview {
				last_message: Some(latest::ChatPreviewMessage {
					message_id: "m1".to_string(),
					origin: latest::ChatMessageOrigin { participant_id: "product".to_string() },
					timestamp: 1,
					body: body.clone(),
					status: latest::ChatMessageStatus::Sent,
					reply_to: None,
					edited_at: None,
					request: false,
				}),
				..empty_preview()
			};
			assert_eq!(set_preview(&runtime, room_id, preview), Err(CallError::Unsupported));
		}
	}
	assert!(
		chat.feed.lock().unwrap().is_empty() && chat.calls().is_empty(),
		"a host that draws these with its own strings would show what the product claims"
	);
}

// In a room that the host stores, the user writes and reacts only through
// the host. Thus no update from a product can name the user as its author.
// Without this check, a host that trusts the core stores words that the
// user did not write.
#[test]
fn a_product_cannot_write_as_the_user_in_a_room_the_host_keeps() {
	let chat = RecordingHost::with_rooms(&[hosted("stored")]);
	let runtime = chat_runtime(chat.clone());
	let quotes = [
		latest::ChatFeedUpdate::New(quoting("user")),
		latest::ChatFeedUpdate::Replaced(quoting("user")),
	];
	let summaries = [
		latest::ChatFeedUpdate::New(reacted_by_the_user()),
		latest::ChatFeedUpdate::Replaced(reacted_by_the_user()),
	];

	for update in updates_by("user").into_iter().chain(quotes).chain(summaries) {
		assert_eq!(
			update_feed(&runtime, "stored", update),
			Err(CallError::Domain(HostChatUpdateFeedError::V2(
				latest::HostChatUpdateFeedError::NotOwner
			)))
		);
	}
	assert!(
		chat.feed.lock().unwrap().is_empty(),
		"a host that trusts the core would store words the user did not write"
	);

	// The product still quotes the contact and counts the reactions of others.
	let mut by_others = reacted_by_the_user();
	by_others.reactions[0].reacted_by_user = false;
	update_feed(&runtime, "stored", latest::ChatFeedUpdate::New(quoting("contact")))
		.expect("a quote of the contact");
	update_feed(&runtime, "stored", latest::ChatFeedUpdate::New(by_others))
		.expect("a summary of the reactions of others");
}

// In a room that the product keeps, the product reports all changes, also
// the messages and reactions of the user. The host only shows them.
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
	update_feed(&runtime, "sourced", latest::ChatFeedUpdate::New(reacted_by_the_user()))
		.expect("a summary that counts the user");
	assert_eq!(chat.feed.lock().unwrap().len(), 5);
}

// The chat list shows the newest message and reaction of a room. In a room
// that the host stores, the host takes those of the user from its own store,
// so a product cannot show the user there.
#[test]
fn a_preview_cannot_show_the_user_in_a_room_the_host_keeps() {
	let chat = RecordingHost::with_rooms(&[hosted("stored"), hosted("sourced")]);
	let runtime = chat_runtime(chat.clone());
	configure(
		&chat,
		"sourced",
		latest::ChatRoomConfig { product_source: true, ..Default::default() },
	);

	for preview in previews_of_the_user() {
		assert_eq!(
			set_preview(&runtime, "stored", preview.clone()),
			Err(CallError::Domain(HostChatSetRoomPreviewError::V2(
				latest::HostChatSetRoomPreviewError::NotOwner
			)))
		);
		set_preview(&runtime, "sourced", preview).expect("a preview of a product room");
	}
	assert_eq!(chat.calls(), ["preview sourced"; 3]);
}

// A deletion names its message as the core screens it, and the host of a
// room it keeps stores it.
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

// `MessageTooLarge` tells the product to send less. The text and the custom
// payload are the content it can shrink, so both answer it.
#[test]
fn an_oversized_feed_update_is_too_large() {
	let runtime = chat_runtime(RecordingHost::with_rooms(&[hosted("stored")]));
	let edit = |body: latest::ChatMessageBody| latest::ChatFeedUpdate::Edited {
		message_id: "m1".to_string(),
		body,
		edited_at: 2,
	};
	let text = latest::ChatMessageBody::Text {
		text: "x".repeat(crate::platform::CHAT_BODY_MAX_BYTES + 1),
	};
	let custom = latest::ChatMessageBody::Custom {
		message: latest::ChatCustomBody {
			message_type: "card".to_string(),
			payload: vec![0; crate::platform::CHAT_CUSTOM_PAYLOAD_MAX_BYTES + 1],
		},
		alt: None,
	};
	let too_large = Err(CallError::Domain(HostChatUpdateFeedError::V2(
		latest::HostChatUpdateFeedError::MessageTooLarge,
	)));

	assert_eq!(update_feed(&runtime, "stored", edit(text)), too_large);
	assert_eq!(update_feed(&runtime, "stored", edit(custom)), too_large);
}

// A product sends a delete again when it lost the answer, so a room that is
// gone succeeds. The host gets the delete even for a room that its list leaves
// out: a list that lags the store must not keep a room the product deleted.
#[test]
fn deleting_a_room_that_is_gone_succeeds() {
	let chat = RecordingHost::with_rooms(&[hosted("support")]);
	let runtime = chat_runtime(chat.clone());

	assert_eq!(delete_room(&runtime, "support"), Ok(HostChatDeleteRoomResponse::V2));
	assert_eq!(delete_room(&runtime, " support "), Ok(HostChatDeleteRoomResponse::V2));
	assert_eq!(chat.calls(), ["delete support", "delete support"]);
}

// A host that does not serve a room change answers `Unsupported`, so a
// product can tell it from a failure worth a retry.
#[test]
fn a_room_change_the_host_does_not_serve_is_unsupported() {
	let chat = RecordingHost::with_rooms(&[hosted("support")]);
	*chat.unserved.lock().unwrap() = true;
	let runtime = chat_runtime(chat.clone());

	assert_eq!(set_preview(&runtime, "support", empty_preview()), Err(CallError::Unsupported));
	assert_eq!(
		update_feed(&runtime, "support", text_message("product")),
		Err(CallError::Unsupported)
	);
	assert_eq!(delete_room(&runtime, "support"), Err(CallError::Unsupported));
	assert!(
		chat.calls().is_empty(),
		"a host that does not serve a change must not apply part of it"
	);
}

// A product that reaches a room as a bot does not own it, so it cannot delete
// it, and the host never sees the call.
#[test]
fn a_product_cannot_delete_a_room_it_reaches_as_a_bot() {
	let chat = RecordingHost::with_rooms(&[("lobby", latest::ChatRoomParticipation::Bot)]);
	let runtime = chat_runtime(chat.clone());

	assert_eq!(
		delete_room(&runtime, "lobby"),
		Err(CallError::Domain(HostChatDeleteRoomError::V2(
			latest::HostChatDeleteRoomError::NotRoomHost
		)))
	);
	assert!(chat.calls().is_empty(), "a product must not delete a room it does not own");
}

// A delete that the host refuses is a host failure, with the reason of the
// host.
#[test]
fn a_delete_the_host_refuses_is_a_host_failure() {
	let chat = RecordingHost::with_rooms(&[hosted("stored")]);
	*chat.delete_failure.lock().unwrap() = Some("not served".to_string());
	let runtime = chat_runtime(chat.clone());

	let deleted = block_on(Chat::delete_room(
		&runtime,
		&CallContext::default(),
		HostChatDeleteRoomRequest::V2(latest::HostChatDeleteRoomRequest {
			room_id: "stored".to_string(),
		}),
	));

	assert_eq!(deleted, Err(CallError::HostFailure { reason: "not served".to_string() }));
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
		HostChatDeleteRoomRequest::V2(latest::HostChatDeleteRoomRequest {
			room_id: "support".to_string(),
		}),
	));

	assert_eq!(deleted, Err(CallError::HostFailure { reason: "store locked".to_string() }));
	assert!(chat.calls().is_empty());
}

// A room keeps the configuration of its last `create_room`, also when the
// room exists, so a product that declares another one changes the room.
#[test]
fn a_room_takes_the_configuration_of_its_last_create_room() {
	let chat = RecordingHost::with_rooms(&[]);
	let runtime = chat_runtime(chat.clone());
	let create = |config: latest::ChatRoomConfig| {
		let created = block_on(Chat::create_room(
			&runtime,
			&CallContext::default(),
			HostChatCreateRoomRequest::V2(latest::HostChatCreateRoomRequest {
				room_id_source: "support".to_string(),
				name: "Support".to_string(),
				icon: String::new(),
				config,
			}),
		))
		.expect("create_room accepts the room");
		created.into_latest().room_id
	};
	let sourced = latest::ChatRoomConfig { product_source: true, reaction_details: true };

	let room_id = create(latest::ChatRoomConfig::default());
	update_feed(&runtime, &room_id, text_message("product")).expect("a stored update");
	assert_eq!(create(sourced.clone()), room_id);
	assert_eq!(chat.configs.lock().unwrap().get(&room_id), Some(&sourced));
	update_feed(&runtime, &room_id, text_message("product")).expect("a shown update");
	assert_eq!(
		chat.feed
			.lock()
			.unwrap()
			.iter()
			.map(|(_, persist)| *persist)
			.collect::<Vec<_>>(),
		[true, false]
	);
	// Each `create_room` is one host call, which carries the configuration.
	let create = format!("create {room_id}");
	assert_eq!(chat.calls(), [create.clone(), create]);
}

// A host that cannot keep a configuration refuses it in the same call that
// would create the room. Thus no room is left behind with the behaviour
// that the product did not ask for.
#[test]
fn a_host_that_cannot_keep_a_configuration_creates_no_room() {
	let chat = RecordingHost::with_rooms(&[]);
	*chat.default_config_only.lock().unwrap() = true;
	let runtime = chat_runtime(chat.clone());
	let create = |config: latest::ChatRoomConfig| {
		block_on(Chat::create_room(
			&runtime,
			&CallContext::default(),
			HostChatCreateRoomRequest::V2(latest::HostChatCreateRoomRequest {
				room_id_source: "support".to_string(),
				name: "Support".to_string(),
				icon: String::new(),
				config,
			}),
		))
	};

	assert_eq!(
		create(latest::ChatRoomConfig { reaction_details: true, ..Default::default() }),
		Err(CallError::Domain(HostChatCreateRoomError::V2(
			latest::HostChatCreateRoomError::UnsupportedConfig
		)))
	);
	assert!(chat.rooms.lock().unwrap().is_empty());
	assert!(chat.calls().is_empty());

	create(latest::ChatRoomConfig::default()).expect("the default configuration");
	assert_eq!(chat.rooms.lock().unwrap().len(), 1);
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
			v01::HostChatCreateRoomError::Unknown { .. }
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
