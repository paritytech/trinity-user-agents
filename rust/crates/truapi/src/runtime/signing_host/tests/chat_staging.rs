//! Chat attachment staging: one preimage that the product's own Bulletin
//! allowance pays for, only in a room that declares `preimage_attachments`,
//! only under `PreimageSubmit`, and never above the bounds.

use std::{collections::BTreeMap, sync::Mutex};

use super::*;
use crate::{
	ChatStageRejection,
	host_internal::bulletin::preimage_key,
	platform::{CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES, ChatPlatform, ProductExecutionKind},
};
use futures::{StreamExt, executor::block_on, stream::BoxStream};
use truapi::{
	api::{Chat, Preimage},
	latest,
	versioned::{
		chat::{HostChatDeleteRoomRequest, HostChatSetRoomFeaturesRequest},
		preimage::{RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest},
	},
};

/// A chat host that hosts every room it is asked about and keeps the
/// features of each.
#[derive(Default)]
pub struct FeatureHost {
	/// The features of each room.
	pub features: Mutex<BTreeMap<String, latest::ChatRoomFeatures>>,
}

impl FeatureHost {
	fn declaring(room_id: &str, preimage_attachments: bool) -> Arc<Self> {
		let host = Self::default();
		host.features.lock().unwrap().insert(
			room_id.to_string(),
			latest::ChatRoomFeatures { preimage_attachments, ..Default::default() },
		);
		Arc::new(host)
	}
}

#[crate::platform::async_trait]
impl ChatPlatform for FeatureHost {
	async fn create_chat_room(
		&self,
		_product: &ProductContext,
		_request: latest::HostChatCreateRoomRequest,
		_icon_url: String,
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

/// A signed-in signing host that keeps preimages in the core, with
/// `platform` for its prompts and `withheld` resources refused.
fn staging_host(
	platform: StubPlatform,
	withheld: &[&str],
) -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
	let (services, activation) = signing_runtime_with_platform(Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		..platform
	}));
	block_on(activation.activate_local_session(ENTROPY.to_vec())).expect("activation succeeds");
	activation.set_grant_allowances_unchecked(true);
	activation.set_withheld_resources(withheld.iter().map(|tag| tag.to_string()).collect());
	(services, activation)
}

/// One Worker connection of `notes.dot`, an untrusted product, over `chat` when the host gives one.
fn connection(
	services: &Arc<RuntimeServices>,
	authority: &Arc<SigningHostRole>,
	chat: Option<Arc<FeatureHost>>,
) -> ProductRuntimeHost {
	let mut adapters = crate::host_core::ConnectionAdapters::from_services(services);
	adapters.chat_platform = chat.map(|chat| chat as Arc<dyn ChatPlatform>);
	ProductRuntimeHost::from_services(
		services.clone(),
		adapters,
		authority.clone(),
		authority.clone(),
		ProductContext::new_with_execution("notes.dot".to_string(), ProductExecutionKind::Worker)
			.expect("valid product id"),
	)
}

fn stage(
	runtime: &ProductRuntimeHost,
	room_id: &str,
	bytes: &[u8],
) -> Result<Vec<u8>, ChatStageRejection> {
	block_on(runtime.stage_chat_attachment(room_id, "application/pdf", bytes.to_vec()))
}

fn lookup(runtime: &ProductRuntimeHost, key: &[u8]) -> Option<Vec<u8>> {
	let mut items = block_on(runtime.lookup_subscribe(
		&CallContext::default(),
		RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
			key: key.to_vec(),
		}),
	));
	match block_on(items.next()) {
		Some(Ok(RemotePreimageLookupSubscribeItem::V1(item))) => item.value,
		other => panic!("a lookup item, got {other:?}"),
	}
}

fn set_features(runtime: &ProductRuntimeHost, room_id: &str, preimage_attachments: bool) {
	block_on(Chat::set_room_features(
		runtime,
		&CallContext::default(),
		HostChatSetRoomFeaturesRequest::V1(latest::HostChatSetRoomFeaturesRequest {
			room_id: room_id.to_string(),
			features: latest::ChatRoomFeatures { preimage_attachments, ..Default::default() },
		}),
	))
	.expect("the room is hosted");
}

/// Three MiB, above the inline bound and below the staging bound.
fn attachment() -> Vec<u8> {
	(0..3 * 1024 * 1024).map(|index| (index % 251) as u8).collect()
}

// S57: a staged attachment is one preimage, which a product then reads
// back through `lookup_subscribe`.
#[test]
fn a_staged_attachment_resolves_through_lookup() {
	let (services, authority) = staging_host(StubPlatform::default(), &[]);
	let runtime = connection(&services, &authority, Some(FeatureHost::declaring("main", true)));
	let bytes = attachment();

	let key = stage(&runtime, "main", &bytes).expect("the room declares staging");

	assert_eq!(key, preimage_key(&bytes).to_vec());
	// Another connection of the product reads it, as the product does.
	assert_eq!(lookup(&connection(&services, &authority, None), &key), Some(bytes));
}

// S58: a room that does not declare staging uploads nothing.
#[test]
fn a_room_without_the_declaration_stages_nothing() {
	let (services, authority) = staging_host(StubPlatform::default(), &[]);
	let bytes = attachment();

	assert_eq!(
		stage(
			&connection(&services, &authority, Some(FeatureHost::declaring("main", false))),
			"main",
			&bytes
		),
		Err(ChatStageRejection::NotDeclared)
	);
	// A connection with no chat adapter, and no declaration the core saw,
	// cannot tell either: the room declares nothing.
	assert_eq!(
		stage(&connection(&services, &authority, None), "main", &bytes),
		Err(ChatStageRejection::NotDeclared)
	);
	assert_eq!(lookup(&connection(&services, &authority, None), &preimage_key(&bytes)), None);
}

// S58: the bound holds before the room, the permission or the allowance is
// asked.
#[test]
fn an_attachment_above_eight_mib_is_too_large() {
	let (services, authority) = staging_host(StubPlatform::default(), &[]);
	let runtime = connection(&services, &authority, Some(FeatureHost::declaring("main", true)));

	assert_eq!(
		stage(&runtime, "main", &vec![0; CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES + 1]),
		Err(ChatStageRejection::TooLarge { max_bytes: 8 * 1024 * 1024 })
	);
}

// S58: the user refused `PreimageSubmit` for the product.
#[test]
fn a_refused_permission_stages_nothing() {
	let (services, authority) = staging_host(
		StubPlatform { remote_permission_denied: true, ..StubPlatform::default() },
		&[],
	);
	let runtime = connection(&services, &authority, Some(FeatureHost::declaring("main", true)));
	let bytes = attachment();

	assert_eq!(stage(&runtime, "main", &bytes), Err(ChatStageRejection::PermissionDenied));
	assert_eq!(lookup(&runtime, &preimage_key(&bytes)), None);
}

// D26: the product's own allowance pays; without it nothing is uploaded.
#[test]
fn no_allowance_for_the_product_stages_nothing() {
	let (services, authority) = staging_host(StubPlatform::default(), &["BulletinAllowance"]);
	let runtime = connection(&services, &authority, Some(FeatureHost::declaring("main", true)));

	assert_eq!(
		stage(&runtime, "main", &attachment()),
		Err(ChatStageRejection::AllowanceUnavailable {
			reason: "Bulletin allowance allocation was rejected by the signing host".to_string(),
		})
	);
}

// A host that sends preimages to the chain reads the chain's own bound
// first; with no chain to read, the submission fails and uploads nothing.
#[test]
fn a_host_with_no_bulletin_chain_fails_the_submission() {
	let (services, authority) = staging_host(
		StubPlatform { chain_connect_error: Some("no Bulletin node"), ..StubPlatform::default() },
		&[],
	);
	authority.set_grant_allowances_unchecked(false);
	let runtime = connection(&services, &authority, Some(FeatureHost::declaring("main", true)));

	assert!(matches!(
		stage(&runtime, "main", &attachment()),
		Err(ChatStageRejection::SubmitFailed { reason }) if reason.contains("no Bulletin node")
	));
}

// The core stages for the declaration it accepted, also on a connection
// that has no chat adapter, as a native host that stages after its worker
// stopped; a later declaration and a deleted room change it.
#[test]
fn the_core_stages_for_the_declaration_it_accepted() {
	let (services, authority) = staging_host(StubPlatform::default(), &[]);
	let chat = Arc::new(FeatureHost::default());
	let worker = connection(&services, &authority, Some(chat.clone()));
	let later = connection(&services, &authority, None);
	let bytes = attachment();

	set_features(&worker, "main", true);
	assert_eq!(stage(&later, "main", &bytes), Ok(preimage_key(&bytes).to_vec()));
	set_features(&worker, "main", false);
	assert_eq!(stage(&later, "main", &bytes), Err(ChatStageRejection::NotDeclared));

	set_features(&worker, "main", true);
	block_on(Chat::delete_room(
		&worker,
		&CallContext::default(),
		HostChatDeleteRoomRequest::V1(latest::HostChatDeleteRoomRequest {
			room_id: "main".to_string(),
		}),
	))
	.expect("deleted");
	assert_eq!(stage(&later, "main", &bytes), Err(ChatStageRejection::NotDeclared));
}

/// A chat host whose room `room_id` declares only `sealed_attachments`.
fn sealing_host(room_id: &str) -> Arc<FeatureHost> {
	let host = FeatureHost::default();
	host.features.lock().unwrap().insert(
		room_id.to_string(),
		latest::ChatRoomFeatures { sealed_attachments: true, ..Default::default() },
	);
	Arc::new(host)
}

// A sealed attachment goes to the Bulletin chain as encrypted parts, so the
// chain never holds the bytes. Only a holder of the resource, which the
// product sends through its own encrypted channel, reads them back.
#[test]
fn a_sealed_attachment_is_encrypted_parts_that_its_resource_opens() {
	let (services, authority) = staging_host(StubPlatform::default(), &[]);
	let runtime = connection(&services, &authority, Some(sealing_host("main")));
	let bytes: Vec<u8> = (0..5 * 1024 * 1024).map(|index| (index % 251) as u8).collect();

	let resource = block_on(runtime.seal_chat_attachment("main", bytes.clone()))
		.expect("the room declares sealed attachments");

	let latest::ChatResource::Sealed { parts, size, .. } = &resource else {
		panic!("a sealed resource, got {resource:?}");
	};
	assert_eq!(parts.len(), 3);
	assert_eq!(*size, bytes.len() as u64 + 16);
	let reader = connection(&services, &authority, None);
	let stored: Vec<u8> = parts
		.iter()
		.flat_map(|part| lookup(&reader, part).expect("the part is stored"))
		.collect();
	assert_eq!(stored.len() as u64, *size);
	assert!(!stored.windows(64).any(|window| window == &bytes[..64]));
	assert_eq!(block_on(reader.open_sealed_chat_resource(&resource)), Ok(bytes));
}

// A room states which kind it takes: a sealed room never stores an
// attachment in plain form, and a plain room gets no sealed one.
#[test]
fn each_kind_of_attachment_needs_its_own_declaration() {
	let (services, authority) = staging_host(StubPlatform::default(), &[]);
	let sealed_room = connection(&services, &authority, Some(sealing_host("main")));
	assert_eq!(stage(&sealed_room, "main", b"plain"), Err(ChatStageRejection::NotDeclared));

	let plain_room = connection(&services, &authority, Some(FeatureHost::declaring("other", true)));
	assert_eq!(
		block_on(plain_room.seal_chat_attachment("other", b"secret".to_vec())),
		Err(ChatStageRejection::NotDeclared)
	);
}
