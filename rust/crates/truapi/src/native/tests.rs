use super::*;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use crate::platform::{
    AuthPresenter, AuthState, ChainProvider, DevicePermissionStatus, LocaleHost,
    PermissionAuthorizationRequest, PermissionAuthorizationStatus, PermissionDecision,
    PreimageHost, ProductContext, ProductExecutionKind, ProductStorage, ProviderError, ThemeHost,
    UserConfirmation, UserConfirmationReview,
};
use futures::stream::{BoxStream, StreamExt};
use parity_scale_codec::Encode;
use truapi::v01;

use super::ws_bridge::WsBridgeStartError;
use crate::host_logic::worker::WorkerTransition;
use crate::{PairedSsoPeer, PairingProposal};

use super::callbacks::{
    HostCallbacks, NativeChatCallbacks, NativeGameCallbacks, NativePocketCallbacks,
    NativePocketRemoval,
};
use super::config::{
    HostRuntimeConfig, NativeResolvedHostRuntimeConfig, NativeRuntimeConfigError,
    ProductExecutionConfig, native_host_platform,
};
use super::errors::HostRejection;
use super::events::NativeEventBus;
use super::platform::{
    CallbackPlatform, ChatCallbackPlatform, GameCallbackPlatform, PocketCallbackPlatform,
};
use super::runtime::{NativePairingError, NativeProductExecution, NativeTrUApiHostRuntime};
use crate::platform::CreateTransactionReview;
use futures::FutureExt;
use truapi::Bytes32;
use truapi::v01::LegacyAccountTxPayload;

pub type PreimageFixtureEntries = Vec<(Vec<u8>, Option<Vec<u8>>)>;

pub fn pocket_card(card_id: &str, privileged: bool) -> v01::PocketCard {
    v01::PocketCard {
        card_id: card_id.to_string(),
        privileged,
    }
}

/// Everything a Pocket stream has already queued, so a missing item reads
/// as pending here rather than hanging the test.
pub fn drain_pocket(
    stream: &mut BoxStream<
        'static,
        Result<v01::HostPocketListSubscribeItem, v01::GenericError>,
    >,
) -> Vec<Result<v01::HostPocketListSubscribeItem, v01::GenericError>> {
    let mut seen = Vec::new();
    while let Some(Some(item)) = stream.next().now_or_never() {
        seen.push(item);
    }
    seen
}

/// The snapshot is read while the subscriber mutex is held, so it is the
/// first item and every later change follows it in order. Delivering a
/// queued change first would leave the product on the older list, with the
/// snapshot overwriting it.
#[test]
fn a_pocket_subscriber_sees_its_snapshot_before_later_changes() {
    let bus = NativeEventBus::default();
    let mut stream = bus.subscribe_pocket_cards(|| {
        Ok(v01::HostPocketListSubscribeItem {
            cards: vec![pocket_card("loyalty", false)],
        })
    });
    bus.notify_pocket_cards_changed(vec![
        pocket_card("loyalty", false),
        pocket_card("humanity", true),
    ]);

    assert_eq!(
        drain_pocket(&mut stream),
        vec![
            Ok(v01::HostPocketListSubscribeItem {
                cards: vec![pocket_card("loyalty", false)],
            }),
            Ok(v01::HostPocketListSubscribeItem {
                cards: vec![pocket_card("loyalty", false), pocket_card("humanity", true)],
            }),
        ]
    );
}

/// A host that cannot say what it holds reaches the product as a failed
/// stream. Reporting an empty list instead would read as "you own no
/// cards" and wipe the product's view of its own collection.
#[test]
fn a_pocket_host_failure_reaches_the_product_instead_of_an_empty_list() {
    let bus = NativeEventBus::default();
    let mut opening = bus.subscribe_pocket_cards(|| {
        Err(v01::GenericError {
            reason: "card store unavailable".to_string(),
        })
    });
    assert_eq!(
        drain_pocket(&mut opening),
        vec![Err(v01::GenericError {
            reason: "card store unavailable".to_string(),
        })]
    );

    let mut live = bus.subscribe_pocket_cards(|| {
        Ok(v01::HostPocketListSubscribeItem {
            cards: vec![pocket_card("loyalty", false)],
        })
    });
    bus.notify_pocket_cards_failed(v01::GenericError {
        reason: "card store went away".to_string(),
    });
    assert_eq!(
        drain_pocket(&mut live),
        vec![
            Ok(v01::HostPocketListSubscribeItem {
                cards: vec![pocket_card("loyalty", false)],
            }),
            Err(v01::GenericError {
                reason: "card store went away".to_string(),
            }),
        ]
    );
}

#[test]
fn an_unexpected_foreign_error_converts_instead_of_panicking() {
    // A host that throws an exception its trait does not declare lands in
    // `try_convert_unexpected_callback_error`. Without a `From` impl the
    // generic converter panics, and `panic = "abort"` turns that into a
    // process abort on the shipping build.
    let reason = "android.database.sqlite.SQLiteFullException";
    let rejection = <HostRejection as uniffi::ConvertError<crate::UniFfiTag>>::
        try_convert_unexpected_callback_error(
            uniffi::UnexpectedUniFFICallbackError::new(reason),
        )
        .expect("an unexpected foreign error must convert");
    let HostRejection::Rejected { reason: converted } = rejection;
    assert_eq!(converted, reason);

    let storage = <v01::HostLocalStorageReadError as uniffi::ConvertError<crate::UniFfiTag>>::
        try_convert_unexpected_callback_error(
            uniffi::UnexpectedUniFFICallbackError::new(reason),
        )
        .expect("an unexpected foreign error must convert");
    assert_eq!(
        storage,
        v01::HostLocalStorageReadError::Unknown {
            reason: reason.to_string(),
        }
    );

    let navigate = <v01::HostNavigateToError as uniffi::ConvertError<crate::UniFfiTag>>::
        try_convert_unexpected_callback_error(
            uniffi::UnexpectedUniFFICallbackError::new(reason),
        )
        .expect("an unexpected foreign error must convert");
    assert_eq!(
        navigate,
        v01::HostNavigateToError::Unknown {
            reason: reason.to_string(),
        }
    );
}

pub fn text_chat_action(text: &str) -> v01::HostChatActionSubscribeItem {
    v01::HostChatActionSubscribeItem {
        room_id: "room".to_string(),
        peer: "native".to_string(),
        payload: v01::ChatActionPayload::MessagePosted(v01::ChatMessageContent::Text {
            text: text.to_string(),
        }),
    }
}

pub struct EventCallbacks {
    pub logs: Mutex<Vec<String>>,
    pub chat_room_status: Mutex<v01::ChatRoomRegistrationStatus>,
    pub chat_created_rooms: Mutex<Vec<(String, String, String)>>,
    pub chat_bot_status: Mutex<v01::ChatBotRegistrationStatus>,
    pub chat_registered_bots: Mutex<Vec<(String, String, String)>>,
    pub chat_bot_rejection: Mutex<Option<String>>,
    pub chat_post_rejection: Mutex<Option<String>>,
    pub chat_posted: Mutex<Vec<(String, v01::ChatMessageContent)>>,
    pub pocket_cards: Mutex<Vec<v01::PocketCard>>,
    pub pocket_removed: Mutex<Vec<String>>,
    pub theme: Mutex<v01::HostThemeSubscribeItem>,
    pub locale: Mutex<v01::HostLocaleSubscribeItem>,
    pub preimages: Mutex<PreimageFixtureEntries>,
    pub auth_states: Mutex<Vec<AuthState>>,
    pub chain_id: Mutex<Option<u32>>,
    pub on_chain_connect: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    pub chain_connects: Mutex<Vec<Vec<u8>>>,
    pub chain_sends: Mutex<Vec<(u32, String)>>,
    pub chain_closes: Mutex<Vec<u32>>,
    /// Worker demand transitions, in arrival order.
    pub worker_demand: Mutex<Vec<(String, WorkerTransition)>>,
    /// Devices reported as paired, in arrival order.
    pub paired_devices: Mutex<Vec<PairedSsoPeer>>,
    /// Capability this host reports as refused by the OS, if any.
    pub os_refused: Option<v01::HostDevicePermissionRequest>,
    /// Configurable prompt outcome for grant, denial, and callback failure tests.
    pub remote_permission_result: Result<PermissionDecision, HostRejection>,
    pub remote_permission_reply: Mutex<
        Option<futures::channel::oneshot::Receiver<Result<PermissionDecision, HostRejection>>>,
    >,
    pub core_storage: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    /// Disclosure consent is distinct from boolean action confirmation.
    pub permission_confirmation_result: PermissionDecision,
    /// Counts prompts across the execution's separate connections.
    pub remote_permission_calls: std::sync::atomic::AtomicUsize,
    pub remote_permission_products: Mutex<Vec<(String, ProductExecutionKind)>>,
}

impl EventCallbacks {
    /// Same as [`Self::new`], reporting `capability` as refused by the OS.
    pub fn refusing(capability: v01::HostDevicePermissionRequest) -> Self {
        Self {
            os_refused: Some(capability),
            ..Self::new()
        }
    }

    pub fn new() -> Self {
        Self {
            logs: Mutex::new(Vec::new()),
            chat_room_status: Mutex::new(v01::ChatRoomRegistrationStatus::New),
            chat_created_rooms: Mutex::new(Vec::new()),
            chat_bot_status: Mutex::new(v01::ChatBotRegistrationStatus::New),
            chat_registered_bots: Mutex::new(Vec::new()),
            chat_bot_rejection: Mutex::new(None),
            chat_post_rejection: Mutex::new(None),
            chat_posted: Mutex::new(Vec::new()),
            pocket_cards: Mutex::new(Vec::new()),
            pocket_removed: Mutex::new(Vec::new()),
            theme: Mutex::new(v01::HostThemeSubscribeItem {
                name: v01::ThemeName::Default,
                variant: v01::ThemeVariant::Light,
            }),
            locale: Mutex::new(v01::HostLocaleSubscribeItem {
                language_tag: "en".to_string(),
            }),
            preimages: Mutex::new(Vec::new()),
            auth_states: Mutex::new(Vec::new()),
            chain_id: Mutex::new(None),
            on_chain_connect: Mutex::new(None),
            chain_connects: Mutex::new(Vec::new()),
            chain_sends: Mutex::new(Vec::new()),
            chain_closes: Mutex::new(Vec::new()),
            worker_demand: Mutex::new(Vec::new()),
            paired_devices: Mutex::new(Vec::new()),
            os_refused: None,
            remote_permission_result: Ok(PermissionDecision::Deny),
            remote_permission_reply: Mutex::new(None),
            core_storage: Mutex::default(),
            permission_confirmation_result: PermissionDecision::Deny,
            remote_permission_calls: std::sync::atomic::AtomicUsize::new(0),
            remote_permission_products: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl HostCallbacks for EventCallbacks {
    fn on_core_log(&self, marker: String, _detail: String) {
        self.logs.lock().expect("logs mutex poisoned").push(marker);
    }
    fn worker_demand_changed(&self, product_id: String, transition: WorkerTransition) {
        self.worker_demand
            .lock()
            .expect("worker demand mutex poisoned")
            .push((product_id, transition));
    }

    fn device_paired(&self, device: PairedSsoPeer) {
        self.paired_devices
            .lock()
            .expect("paired device mutex poisoned")
            .push(device);
    }
    async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
        Ok(())
    }
    async fn push_notification(
        &self,
        _request: v01::HostPushNotificationRequest,
    ) -> Result<u32, HostRejection> {
        Ok(0)
    }
    fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
        Ok(())
    }
    async fn device_permission(
        &self,
        _product: ProductExecutionConfig,
        _request: v01::HostDevicePermissionRequest,
    ) -> Result<PermissionDecision, HostRejection> {
        Ok(PermissionDecision::Deny)
    }
    async fn device_permission_status(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<DevicePermissionStatus, HostRejection> {
        Ok(if self.os_refused == Some(request) {
            DevicePermissionStatus::Denied
        } else {
            DevicePermissionStatus::NotApplicable
        })
    }
    async fn remote_permission(
        &self,
        product: ProductExecutionConfig,
        _request: v01::RemotePermission,
    ) -> Result<PermissionDecision, HostRejection> {
        self.remote_permission_calls.fetch_add(1, Ordering::SeqCst);
        self.remote_permission_products
            .lock()
            .unwrap()
            .push((product.product_id, product.execution_kind));
        let reply = self.remote_permission_reply.lock().unwrap().take();
        match reply {
            Some(reply) => reply.await.unwrap(),
            None => self.remote_permission_result.clone(),
        }
    }
    fn auth_state_changed(&self, state: AuthState) {
        self.auth_states
            .lock()
            .expect("auth state mutex poisoned")
            .push(state);
    }
    async fn core_storage_read(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
        Ok(self.core_storage.lock().unwrap().get(&key).cloned())
    }
    async fn core_storage_write(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), HostRejection> {
        self.core_storage.lock().unwrap().insert(key, value);
        Ok(())
    }
    async fn core_storage_clear(&self, key: Vec<u8>) -> Result<(), HostRejection> {
        self.core_storage.lock().unwrap().remove(&key);
        Ok(())
    }
    fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
        self.chain_connects
            .lock()
            .expect("chain connects mutex poisoned")
            .push(genesis_hash);
        let on_connect = self.on_chain_connect.lock().unwrap().take();
        if let Some(on_connect) = on_connect {
            on_connect();
        }
        Ok(*self.chain_id.lock().expect("chain id mutex poisoned"))
    }
    fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection> {
        self.chain_sends
            .lock()
            .expect("chain sends mutex poisoned")
            .push((connection_id, request));
        Ok(())
    }
    fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection> {
        self.chain_closes
            .lock()
            .expect("chain closes mutex poisoned")
            .push(connection_id);
        Ok(())
    }
    async fn confirm_user_action(
        &self,
        _review: UserConfirmationReview,
    ) -> Result<bool, HostRejection> {
        Ok(false)
    }
    async fn confirm_permission(
        &self,
        _review: UserConfirmationReview,
    ) -> Result<PermissionDecision, HostRejection> {
        Ok(self.permission_confirmation_result)
    }
    async fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
        Ok(self
            .preimages
            .lock()
            .expect("preimage map mutex poisoned")
            .iter()
            .find(|(stored_key, _)| stored_key == &key)
            .and_then(|(_, value)| value.clone()))
    }
    fn current_theme(&self) -> Result<v01::HostThemeSubscribeItem, HostRejection> {
        Ok(self.theme.lock().expect("theme mutex poisoned").clone())
    }
    fn current_locale(&self) -> Result<v01::HostLocaleSubscribeItem, HostRejection> {
        Ok(self.locale.lock().expect("locale mutex poisoned").clone())
    }
    async fn feature_supported(
        &self,
        _request: v01::HostFeatureSupportedRequest,
    ) -> Result<bool, HostRejection> {
        Ok(false)
    }
    fn supported_chains(&self) -> Result<crate::platform::HostChainSet, HostRejection> {
        Ok(crate::platform::HostChainSet {
            network: "paseo".to_string(),
            chains: Vec::new(),
        })
    }
    async fn local_storage_read(
        &self,
        _key: String,
    ) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        Ok(None)
    }
    async fn local_storage_write(
        &self,
        _key: String,
        _value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        Ok(())
    }
    async fn local_storage_clear(
        &self,
        _key: String,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        Ok(())
    }
    async fn begin_operation(
        &self,
        _product_id: String,
        _label: String,
    ) -> Result<u32, HostRejection> {
        Ok(1)
    }
    async fn end_operation(&self, _product_id: String, _id: u32) -> Result<(), HostRejection> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl NativePocketCallbacks for EventCallbacks {
    fn list_cards(&self) -> Result<Vec<v01::PocketCard>, HostRejection> {
        Ok(self
            .pocket_cards
            .lock()
            .expect("pocket cards mutex poisoned")
            .clone())
    }

    async fn remove_card(&self, card_id: String) -> Result<NativePocketRemoval, HostRejection> {
        let mut cards = self
            .pocket_cards
            .lock()
            .expect("pocket cards mutex poisoned");
        let outcome = match cards.iter().find(|card| card.card_id == card_id) {
            None => NativePocketRemoval::Absent,
            Some(card) if card.privileged => NativePocketRemoval::Privileged,
            Some(_) => {
                cards.retain(|card| card.card_id != card_id);
                NativePocketRemoval::Removed
            }
        };
        drop(cards);
        self.pocket_removed
            .lock()
            .expect("pocket removed mutex poisoned")
            .push(card_id);
        Ok(outcome)
    }
}

#[async_trait::async_trait]
impl NativeChatCallbacks for EventCallbacks {
    async fn create_room(
        &self,
        room_id: String,
        name: String,
        icon: String,
    ) -> Result<v01::ChatRoomRegistrationStatus, HostRejection> {
        self.chat_created_rooms
            .lock()
            .expect("created rooms mutex poisoned")
            .push((room_id, name, icon));
        Ok(*self
            .chat_room_status
            .lock()
            .expect("room status mutex poisoned"))
    }

    async fn register_bot(
        &self,
        bot_id: String,
        name: String,
        icon: String,
    ) -> Result<v01::ChatBotRegistrationStatus, HostRejection> {
        if let Some(reason) = self
            .chat_bot_rejection
            .lock()
            .expect("bot rejection mutex poisoned")
            .clone()
        {
            return Err(HostRejection::Rejected { reason });
        }
        self.chat_registered_bots
            .lock()
            .expect("registered bots mutex poisoned")
            .push((bot_id, name, icon));
        Ok(*self
            .chat_bot_status
            .lock()
            .expect("bot status mutex poisoned"))
    }

    async fn post_message(
        &self,
        room_id: String,
        content: v01::ChatMessageContent,
    ) -> Result<String, HostRejection> {
        if let Some(reason) = self
            .chat_post_rejection
            .lock()
            .expect("post rejection mutex poisoned")
            .clone()
        {
            return Err(HostRejection::Rejected { reason });
        }
        let mut posted = self
            .chat_posted
            .lock()
            .expect("posted messages mutex poisoned");
        posted.push((room_id, content));
        // Distinct per message: a correlation assertion must not pass on a
        // constant the host happens to return every time.
        Ok(format!("message-{}", posted.len()))
    }

    async fn list_rooms(&self) -> Result<Vec<v01::ChatRoom>, HostRejection> {
        let mut room_ids: Vec<String> = self
            .chat_created_rooms
            .lock()
            .expect("created rooms mutex poisoned")
            .iter()
            .map(|(room_id, _, _)| room_id.clone())
            .collect();
        room_ids.sort();
        room_ids.dedup();
        Ok(room_ids
            .into_iter()
            .map(|room_id| v01::ChatRoom {
                room_id,
                participating_as: v01::ChatRoomParticipation::RoomHost,
            })
            .collect())
    }
}

pub fn event_platform() -> (Arc<EventCallbacks>, Arc<NativeEventBus>, CallbackPlatform) {
    let callbacks = Arc::new(EventCallbacks::new());
    let events = Arc::new(NativeEventBus::default());
    let platform = CallbackPlatform {
        callbacks: callbacks.clone(),
        events: events.clone(),
        storage_events: events.clone(),
    };
    (callbacks, events, platform)
}

#[test]
fn a_product_write_reaches_a_storage_subscription_on_the_same_key() {
    let (_callbacks, _events, platform) = event_platform();
    let key = "myapp.dot/progress".to_string();
    let mut subscription = platform.subscribe_storage(key.clone());

    assert_eq!(
        futures::executor::block_on(futures::StreamExt::next(&mut subscription)),
        Some(Ok(v01::HostLocalStorageChangeItem { value: None })),
        "a subscription opens on the key's current value"
    );

    futures::executor::block_on(ProductStorage::write(&platform, key, vec![1, 2, 3]))
        .expect("write");

    assert_eq!(
        futures::FutureExt::now_or_never(futures::StreamExt::next(&mut subscription)),
        Some(Some(Ok(v01::HostLocalStorageChangeItem {
            value: Some(vec![1, 2, 3]),
        }))),
        "a write the product made is a change its own subscribers must see"
    );
}

pub fn native_host_runtime_config() -> HostRuntimeConfig {
    HostRuntimeConfig {
        host_name: "Polkadot Web".to_string(),
        host_icon: Some("https://example.invalid/dotli.png".to_string()),
        host_version: None,
        platform_type: None,
        platform_version: None,
        people_chain_genesis_hash: vec![0xa2; 32],
        bulletin_chain_genesis_hash: vec![0xbb; 32],
        asset_hub_chain_genesis_hash: vec![0xcc; 32],
        network_suffix: "paseo".to_string(),
        // Every runtime opens its own database; the directory is left for the
        // OS to clean so the fixture can stay a plain value.
        database_directory: tempfile::tempdir()
            .unwrap()
            .keep()
            .to_string_lossy()
            .into_owned(),
        local_session_secret: Some(vec![7; 32]),
        local_session_lite_username: Some("alice".to_string()),
    }
}

pub fn native_execution_config(
    product_id: &str,
    execution_kind: ProductExecutionKind,
) -> ProductExecutionConfig {
    ProductExecutionConfig {
        product_id: product_id.to_string(),
        execution_kind,
    }
}

pub fn native_product_execution(
    callbacks: Arc<dyn HostCallbacks>,
    product_id: &str,
) -> Arc<NativeProductExecution> {
    let mut config = native_host_runtime_config();
    config.local_session_secret = None;
    config.local_session_lite_username = None;
    let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config)
        .expect("host runtime config should be valid");
    host.open_product_execution(
        callbacks,
        None,
        None,
        None,
        native_execution_config(product_id, ProductExecutionKind::App),
    )
    .expect("product execution config should be valid")
}

/// A deeplink that is not hex at all, and one that is hex but not a
/// proposal. The two render differently, so asserting on either alone
/// passes for a decoder that never saw the second kind.
pub const UNDECODABLE_DEEPLINKS: [(&str, &str); 2] = [
    ("not-a-deeplink", "invalid pairing deeplink hex"),
    (
        "polkadotapp://pair?handshake=ff",
        "invalid pairing handshake proposal",
    ),
];

/// Reads the reason out of either refusal, so a test names the failure it
/// expected rather than the enum shape.
pub fn pairing_rejection(failure: NativePairingError) -> String {
    match failure {
        NativePairingError::UndecodableDeeplink { reason }
        | NativePairingError::Rejected { reason } => reason,
    }
}

/// Build a deeplink carrying `peer` and `metadata`, the way a pairing host
/// puts its QR together.
pub fn proposal_deeplink(
    peer: PairedSsoPeer,
    metadata: Vec<crate::host_logic::sso::pairing::v2::MetadataEntry>,
) -> String {
    let proposal = crate::host_logic::sso::pairing::VersionedHandshakeProposal::V2(
        crate::host_logic::sso::pairing::v2::Proposal {
            device: crate::host_logic::sso::pairing::v2::Device {
                statement_account_id: peer.statement_account_id,
                encryption_public_key: peer.encryption_public_key,
            },
            metadata,
        },
    );
    format!(
        "polkadotapp://pair?handshake={}",
        hex::encode(parity_scale_codec::Encode::encode(&proposal))
    )
}

/// The peer a host must register a renewal target for before it answers,
/// and the name it prompts with. Without this entry point that account is
/// unreachable from a native host, the answer goes out with no allowance
/// behind it, and the prompt can only name the peer by its raw key.
#[test]
fn a_pairing_deeplink_yields_the_proposal_it_carries() {
    use crate::PairingProposalMetadata;
    use crate::host_logic::sso::pairing::v2::{MetadataEntry, MetadataKey};

    let peer = PairedSsoPeer {
        statement_account_id: [0x31; 32],
        encryption_public_key: [0x42; 32],
    };
    let deeplink = proposal_deeplink(
        peer,
        vec![
            MetadataEntry(MetadataKey::HostName, "Polkadot Desktop Host".to_string()),
            MetadataEntry(MetadataKey::PlatformType, "macOS".to_string()),
            MetadataEntry(MetadataKey::Custom("seat".to_string()), "3".to_string()),
        ],
    );

    assert_eq!(
        parse_pairing_deeplink(deeplink).expect("a well-formed deeplink decodes"),
        PairingProposal {
            peer,
            metadata: PairingProposalMetadata {
                host_name: Some("Polkadot Desktop Host".to_string()),
                platform_type: Some("macOS".to_string()),
                ..PairingProposalMetadata::default()
            },
        }
    );

    for (deeplink, expected) in UNDECODABLE_DEEPLINKS {
        let reason = pairing_rejection(
            parse_pairing_deeplink(deeplink.to_string())
                .expect_err("an undecodable deeplink carries no peer"),
        );
        assert!(
            reason.contains(expected),
            "{deeplink} did not reach the core's decoder: {reason}"
        );
    }
}

/// Both deeplink entry points have to reach the core's own decoder. One
/// wired to nothing would answer the same way for every input.
///
/// The variant is what a host branches on: an undecodable deeplink
/// announced nothing and tracked nothing, so reading it as an ordinary
/// refusal would have the host untrack a target it never tracked and
/// notify a peer that never heard from it.
#[test]
fn the_deeplink_entry_points_reject_what_they_cannot_decode() {
    let host = native_host_runtime_no_session();

    for (deeplink, expected) in UNDECODABLE_DEEPLINKS {
        let answered =
            futures::executor::block_on(host.establish_pairing(deeplink.to_string()))
                .expect_err("an undecodable deeplink cannot be answered");
        let announced = futures::executor::block_on(
            host.notify_pairing_allowance_allocation(deeplink.to_string()),
        )
        .err()
        .expect("an undecodable deeplink cannot be announced");

        for failure in [answered, announced] {
            assert!(
                matches!(failure, NativePairingError::UndecodableDeeplink { .. }),
                "{deeplink} was not reported as undecodable: {failure:?}"
            );
            let reason = pairing_rejection(failure);
            assert!(
                reason.contains(expected),
                "{deeplink} did not reach the core's decoder: {reason}"
            );
        }
    }
}

/// A deeplink that decodes and then fails is the opposite case: the peer
/// may have been reached and its renewal target tracked, so the host owes
/// both an undo. Sharing one variant with the undecodable case would leave
/// that difference readable only by matching on the reason string.
#[test]
fn a_decodable_deeplink_that_fails_is_not_reported_as_undecodable() {
    let host = native_host_runtime_no_session();
    let deeplink = proposal_deeplink(
        PairedSsoPeer {
            statement_account_id: [0x31; 32],
            encryption_public_key: [0x42; 32],
        },
        Vec::new(),
    );

    let answered = futures::executor::block_on(host.establish_pairing(deeplink.clone()))
        .expect_err("no session means no handshake to answer");
    let announced =
        futures::executor::block_on(host.notify_pairing_allowance_allocation(deeplink))
            .err()
            .expect("no session means no notice to sign");

    for failure in [answered, announced] {
        assert!(
            matches!(failure, NativePairingError::Rejected { .. }),
            "a decodable deeplink was reported as undecodable: {failure:?}"
        );
        let reason = pairing_rejection(failure);
        assert!(
            reason.contains("no active local session"),
            "the core's session check did not reach the host: {reason}"
        );
    }
}

/// The two peer entry points have to reach the core's signing host. One
/// wired to nothing would answer without a session to answer from.
#[test]
fn the_peer_entry_points_need_the_core_signing_host() {
    let host = native_host_runtime_no_session();
    let peer = PairedSsoPeer {
        statement_account_id: [0x31; 32],
        encryption_public_key: [0x42; 32],
    };

    for failure in [
        pairing_rejection(
            futures::executor::block_on(host.resume_pairing(peer))
                .expect_err("no session means no pairing to serve"),
        ),
        pairing_rejection(
            futures::executor::block_on(host.disconnect_paired_host(peer))
                .expect_err("no session means no disconnect to sign"),
        ),
    ] {
        assert!(
            failure.contains("no active local session"),
            "the core's session check did not reach the host: {failure}"
        );
    }
}

/// Without this a paired device stops at the core and the host never hears
/// of it, so no contact is told a new device joined.
#[test]
fn a_paired_device_reaches_the_host_callbacks() {
    let (callbacks, _events, platform) = event_platform();
    let device = PairedSsoPeer {
        statement_account_id: [0x31; 32],
        encryption_public_key: [0x42; 32],
    };

    crate::DevicePairingObserver::device_paired(&platform, device);

    assert_eq!(
        *callbacks
            .paired_devices
            .lock()
            .expect("paired device mutex poisoned"),
        vec![device]
    );
}

#[test]
fn process_runtime_counts_worker_references_per_product() {
    let callbacks = Arc::new(EventCallbacks::new());
    let host = NativeTrUApiHostRuntime::with_runtime_config(
        callbacks.clone(),
        native_host_runtime_config(),
    )
    .expect("host runtime config should be valid");
    let product = || "shared.dot".to_string();

    host.acquire_worker(product());
    host.acquire_worker(product());
    host.release_worker(product());
    host.release_worker(product());

    assert_eq!(
        *callbacks
            .worker_demand
            .lock()
            .expect("worker demand mutex poisoned"),
        vec![
            (product(), WorkerTransition::Start),
            (product(), WorkerTransition::Stop),
        ]
    );
}

/// A host removes a card through its own storage, which can take a while,
/// so the core waits for the answer without holding the thread that asked.
#[test]
fn native_pocket_removal_waits_for_the_host_without_holding_the_core_thread() {
    struct DeferredRemoval {
        outcome: Mutex<Option<futures::channel::oneshot::Receiver<NativePocketRemoval>>>,
    }

    #[async_trait::async_trait]
    impl NativePocketCallbacks for DeferredRemoval {
        fn list_cards(&self) -> Result<Vec<v01::PocketCard>, HostRejection> {
            Ok(Vec::new())
        }

        async fn remove_card(
            &self,
            _card_id: String,
        ) -> Result<NativePocketRemoval, HostRejection> {
            let outcome = self.outcome.lock().unwrap().take().expect("one removal");
            Ok(outcome.await.expect("the host answers"))
        }
    }

    let (answer, outcome) = futures::channel::oneshot::channel();
    let platform = PocketCallbackPlatform {
        pocket: Arc::new(DeferredRemoval {
            outcome: Mutex::new(Some(outcome)),
        }),
        events: Arc::new(NativeEventBus::default()),
    };
    let product = ProductContext::new("pocket.dot".to_string()).expect("valid product id");
    let mut removal = crate::platform::PocketPlatform::remove_pocket_card(
        &platform,
        &product,
        v01::HostPocketRemoveCardRequest {
            card_id: "humanity".to_string(),
        },
    );

    let waker = futures::task::noop_waker();
    let mut context = core::task::Context::from_waker(&waker);
    assert!(removal.poll_unpin(&mut context).is_pending());

    answer.send(NativePocketRemoval::Privileged).unwrap();
    assert!(matches!(
        futures::executor::block_on(removal),
        Err(v01::HostPocketRemoveCardError::Privileged)
    ));
}

#[test]
fn native_pocket_removal_outcomes_are_decided_by_the_host() {
    let pocket_host = Arc::new(EventCallbacks::new());
    *pocket_host
        .pocket_cards
        .lock()
        .expect("pocket cards mutex poisoned") =
        vec![pocket_card("loyalty", false), pocket_card("humanity", true)];
    let events = Arc::new(NativeEventBus::default());
    let platform = PocketCallbackPlatform {
        pocket: pocket_host.clone(),
        events,
    };
    let product = ProductContext::new("pocket.dot".to_string()).expect("valid product id");
    let remove = |card_id: &str| {
        futures::executor::block_on(crate::platform::PocketPlatform::remove_pocket_card(
            &platform,
            &product,
            v01::HostPocketRemoveCardRequest {
                card_id: card_id.to_string(),
            },
        ))
    };
    let mut cards =
        crate::platform::PocketPlatform::subscribe_pocket_cards(&platform, &product);
    let first = futures::executor::block_on(cards.next())
        .expect("the current list arrives on subscribe")
        .expect("no stream error");
    assert_eq!(
        first.cards,
        vec![pocket_card("loyalty", false), pocket_card("humanity", true)]
    );

    assert!(matches!(
        remove("humanity"),
        Err(v01::HostPocketRemoveCardError::Privileged)
    ));
    assert!(
        remove("absent").is_ok(),
        "an absent card is already removed"
    );
    assert!(remove("loyalty").is_ok());
    assert_eq!(
        pocket_host
            .pocket_removed
            .lock()
            .expect("pocket removed mutex poisoned")
            .as_slice(),
        ["humanity", "absent", "loyalty"],
        "the host decides every removal, so every request reaches it"
    );

    let republished = futures::executor::block_on(cards.next())
        .expect("a removal republishes the list")
        .expect("no stream error");
    assert_eq!(
        republished.cards,
        vec![pocket_card("humanity", true)],
        "the pinned card stays and the removed one is gone"
    );
}

#[derive(Default)]
struct RecordingGameCallbacks {
    calls: Mutex<Vec<Option<u64>>>,
}

#[async_trait::async_trait]
impl NativeGameCallbacks for RecordingGameCallbacks {
    async fn schedule_reminder(&self, starts_at: u64) -> Result<(), HostRejection> {
        self.calls
            .lock()
            .expect("game calls mutex poisoned")
            .push(Some(starts_at));
        Ok(())
    }

    async fn cancel_reminder(&self) -> Result<(), HostRejection> {
        self.calls
            .lock()
            .expect("game calls mutex poisoned")
            .push(None);
        Ok(())
    }
}

#[test]
fn game_callbacks_receive_the_start_time_and_the_cancel() {
    let callbacks = Arc::new(RecordingGameCallbacks::default());
    let platform = GameCallbackPlatform {
        game: callbacks.clone(),
    };
    let product = ProductContext::new("dim2.dot".to_string()).expect("valid product id");

    futures::executor::block_on(async {
        crate::platform::GamePlatform::schedule_game_reminder(&platform, &product, 42)
            .await
            .expect("schedule succeeds");
        crate::platform::GamePlatform::cancel_game_reminder(&platform, &product)
            .await
            .expect("cancel succeeds");
    });

    assert_eq!(
        *callbacks.calls.lock().expect("game calls mutex poisoned"),
        vec![Some(42), None]
    );
}

#[test]
fn native_chat_entrypoint_is_unsupported_without_an_adapter() {
    let mut config = native_host_runtime_config();
    config.local_session_secret = Some(vec![7; 32]);
    let host =
        NativeTrUApiHostRuntime::with_runtime_config(Arc::new(EventCallbacks::new()), config)
            .expect("host runtime config should be valid");
    let execution = host
        .open_product_execution(
            Arc::new(EventCallbacks::new()),
            None,
            None,
            None,
            native_execution_config("chat-product.dot", ProductExecutionKind::Worker),
        )
        .expect("Chat execution should open");

    let result = execution.publish_chat_action(text_chat_action("hello"));

    assert!(matches!(
        result,
        Err(crate::ProductRuntimeError::Unsupported)
    ));
}

/// Hosts mediate product network access in their own code and ask this to
/// decide whether to prompt, so it has to answer for the spellings a host
/// actually holds, not only the normalized one the core passes internally.
#[test]
fn the_trusted_export_normalizes_before_matching() {
    for trusted in [
        "peopl.dot",
        "PEOPL.DOT",
        "  peopl.dot  ",
        "dim2.paseo",
        "stash.dot",
    ] {
        assert!(
            super::has_trusted_remote_permissions(trusted.to_string()),
            "{trusted} is a first-party product",
        );
    }
    for untrusted in [
        "app.peopl.dot",
        "peopl",
        "notpeopl.dot",
        "localhost:3000",
        "",
        "   ",
    ] {
        assert!(
            !super::has_trusted_remote_permissions(untrusted.to_string()),
            "{untrusted} is not",
        );
    }
}

#[test]
fn permission_authorization_request_mirror_round_trips() {
    let device_cases = [
        v01::HostDevicePermissionRequest::Notifications,
        v01::HostDevicePermissionRequest::Camera,
        v01::HostDevicePermissionRequest::Microphone,
        v01::HostDevicePermissionRequest::Bluetooth,
        v01::HostDevicePermissionRequest::NFC,
        v01::HostDevicePermissionRequest::Location,
        v01::HostDevicePermissionRequest::Clipboard,
        v01::HostDevicePermissionRequest::OpenUrl,
        v01::HostDevicePermissionRequest::Biometrics,
    ];
    let remote_cases = [
        v01::RemotePermission::Remote {
            domains: vec!["a.dot".to_string(), "b.dot".to_string()],
        },
        v01::RemotePermission::WebRtc,
        v01::RemotePermission::ChainSubmit,
        v01::RemotePermission::PreimageSubmit,
        v01::RemotePermission::StatementSubmit,
    ];

    let mut cases: Vec<PermissionAuthorizationRequest> = Vec::new();
    cases.extend(
        device_cases
            .into_iter()
            .map(PermissionAuthorizationRequest::Device),
    );
    cases.extend(remote_cases.into_iter().map(|permission| {
        PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest { permission })
    }));
    cases.push(PermissionAuthorizationRequest::IdentityDisclosure);
    cases.push(PermissionAuthorizationRequest::AccountAccess {
        target_product_id: "other.dot".to_string(),
    });

    for case in cases {
        let native = case.clone();
        assert_eq!(native, case);
    }
}

#[test]
fn native_auth_presenter_forwards_states_across_the_ffi_mirror() {
    let (callbacks, _events, platform) = event_platform();

    platform.auth_state_changed(crate::platform::AuthState::Pairing {
        deeplink: "polkadotapp://pair?handshake=00".to_string(),
    });
    platform.auth_state_changed(crate::platform::AuthState::Connected(
        crate::platform::SessionUiInfo {
            public_key: [7; 32],
            identity_account_id: None,
            chat_public_key: None,
            device_enc_public_key: None,
            peer_statement_account_id: None,
            device_statement_account_id: None,
            lite_username: Some("alice".to_string()),
            full_username: None,
        },
    ));
    platform.auth_state_changed(crate::platform::AuthState::Disconnected);

    assert_eq!(
        callbacks
            .auth_states
            .lock()
            .expect("auth state mutex poisoned")
            .as_slice(),
        &[
            AuthState::Pairing {
                deeplink: "polkadotapp://pair?handshake=00".to_string(),
            },
            AuthState::Connected(crate::platform::SessionUiInfo {
                public_key: [7; 32],
                identity_account_id: None,
                chat_public_key: None,
                device_enc_public_key: None,
                peer_statement_account_id: None,
                device_statement_account_id: None,
                lite_username: Some("alice".to_string()),
                full_username: None,
            }),
            AuthState::Disconnected,
        ]
    );
}

#[test]
fn native_locale_subscription_emits_current_then_notified_changes() {
    let (callbacks, events, platform) = event_platform();
    let mut stream = platform.subscribe_locale();
    let switched = v01::HostLocaleSubscribeItem {
        language_tag: "zh-Hans".to_string(),
    };

    let first = futures::executor::block_on(stream.next()).unwrap();
    *callbacks.locale.lock().expect("locale mutex poisoned") = switched.clone();
    events.notify_locale_changed(switched.clone());
    let second = futures::executor::block_on(stream.next()).unwrap();

    assert_eq!(
        first.unwrap(),
        v01::HostLocaleSubscribeItem {
            language_tag: "en".to_string(),
        }
    );
    assert_eq!(second.unwrap(), switched);
}

#[test]
fn native_theme_subscription_emits_current_then_notified_changes() {
    let (callbacks, events, platform) = event_platform();
    let mut stream = platform.subscribe_theme();
    let named = v01::HostThemeSubscribeItem {
        name: v01::ThemeName::Custom("midnight".to_string()),
        variant: v01::ThemeVariant::Dark,
    };

    let first = futures::executor::block_on(stream.next()).unwrap();
    *callbacks.theme.lock().expect("theme mutex poisoned") = named.clone();
    events.notify_theme_changed(named.clone());
    let second = futures::executor::block_on(stream.next()).unwrap();

    assert_eq!(
        first.unwrap(),
        v01::HostThemeSubscribeItem {
            name: v01::ThemeName::Default,
            variant: v01::ThemeVariant::Light,
        }
    );
    assert_eq!(second.unwrap(), named);
}

#[test]
fn native_preimage_subscription_emits_current_then_notified_value() {
    let (callbacks, events, platform) = event_platform();
    let key = vec![7; 32];
    callbacks
        .preimages
        .lock()
        .expect("preimage map mutex poisoned")
        .push((key.clone(), Some(vec![1, 2, 3])));
    let mut stream = platform.lookup_preimage(key.clone());

    let first = futures::executor::block_on(stream.next()).unwrap();
    events.notify_preimage_changed(&key, Some(vec![4, 5, 6]));
    let second = futures::executor::block_on(stream.next()).unwrap();

    assert_eq!(first.unwrap(), Some(vec![1, 2, 3]));
    assert_eq!(second.unwrap(), Some(vec![4, 5, 6]));
}

#[test]
fn native_chat_room_subscription_emits_current_then_notified_replacement() {
    let (callbacks, events, _platform) = event_platform();
    let platform = ChatCallbackPlatform {
        chat: callbacks,
        events: events.clone(),
    };
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        ProductExecutionKind::Worker,
    )
    .unwrap();
    let mut stream = crate::platform::ChatPlatform::subscribe_chat_rooms(&platform, &product);

    let first = ready_rooms(stream.as_mut(), "initial room list");
    events.notify_chat_rooms_changed(vec![v01::ChatRoom {
        room_id: "support".to_string(),
        participating_as: v01::ChatRoomParticipation::Bot,
    }]);
    let second = futures::executor::block_on(stream.next())
        .unwrap()
        .expect("replacement room list");

    assert!(first.rooms.is_empty());
    assert_eq!(second.rooms.len(), 1);
    assert_eq!(second.rooms[0].room_id, "support");
    assert_eq!(
        second.rooms[0].participating_as,
        v01::ChatRoomParticipation::Bot
    );
}

/// What a room-list subscription yields: a replacement, or the host's
/// failure to produce one.
pub type RoomListItem = Result<v01::HostChatListSubscribeItem, v01::GenericError>;

/// Reads a room list the subscription has already emitted.
///
/// Polled without blocking on purpose: both the eager snapshot and a
/// notified replacement are sent before this runs, so a subscription that
/// stopped emitting one fails here rather than parking on a live sender
/// and timing the job out.
pub fn ready_rooms(
    mut rooms: core::pin::Pin<&mut (dyn futures::Stream<Item = RoomListItem> + Send)>,
    what: &str,
) -> v01::HostChatListSubscribeItem {
    let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
    match rooms.as_mut().poll_next(&mut cx) {
        core::task::Poll::Ready(Some(Ok(item))) => item,
        other => panic!("{what} must be ready, got {other:?}"),
    }
}

#[test]
fn native_chat_adapter_forwards_every_message_variant() {
    let callbacks = Arc::new(EventCallbacks::new());
    let platform = ChatCallbackPlatform {
        chat: callbacks.clone(),
        events: Arc::new(NativeEventBus::default()),
    };
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        ProductExecutionKind::Worker,
    )
    .unwrap();
    let reaction = v01::ChatReaction {
        message_id: "message-1".to_string(),
        emoji: "\u{1f3b2}".to_string(),
    };
    // Every variant the protocol advertises reaches the host, so a product
    // can post the `Actions` that `action_subscribe` later reports back.
    // An added variant stops `validate_chat_message_content` compiling,
    // which is what forces this list to be revisited.
    let variants = [
        v01::ChatMessageContent::Text {
            text: "hello".to_string(),
        },
        v01::ChatMessageContent::RichText(v01::ChatRichText {
            text: None,
            media: Vec::new(),
        }),
        v01::ChatMessageContent::Actions(v01::ChatActions {
            text: None,
            actions: Vec::new(),
            layout: v01::ChatActionLayout::Column,
        }),
        v01::ChatMessageContent::File(v01::ChatFile {
            url: "https://example.invalid/f".to_string(),
            file_name: "f".to_string(),
            mime_type: "text/plain".to_string(),
            size_bytes: 1,
            text: None,
        }),
        v01::ChatMessageContent::Reaction(reaction.clone()),
        v01::ChatMessageContent::ReactionRemoved(reaction),
        v01::ChatMessageContent::Custom(v01::ChatCustomMessage {
            message_type: "vote".to_string(),
            payload: vec![1, 2],
        }),
    ];

    for payload in &variants {
        futures::executor::block_on(crate::platform::ChatPlatform::post_chat_message(
            &platform,
            &product,
            v01::HostChatPostMessageRequest {
                room_id: "support".to_string(),
                payload: payload.clone(),
            },
        ))
        .unwrap_or_else(|error| panic!("{payload:?} must reach the host: {error:?}"));
    }

    let posted = callbacks
        .chat_posted
        .lock()
        .expect("posted messages mutex poisoned")
        .clone();
    // Asserts the room alongside the content: routing the room correctly
    // for `Text` while misrouting the rest must not pass.
    assert_eq!(
        posted,
        variants
            .iter()
            .map(|content| ("support".to_string(), content.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_posted_action_set_round_trips_to_the_product_that_posted_it() {
    // The loop the widened variant set exists to enable: a product posts
    // `Actions`, a user triggers one, and the product reads the trigger
    // back. The halves travel different paths -- `post_message` through the
    // chat adapter, `ActionTriggered` through the connection -- so this
    // covers what the core owns: that an `Actions` set reaches the host,
    // and that the trigger naming the returned id arrives unaltered.
    // Reusing that id is the host's half of the contract, documented on
    // `NativeChatCallbacks::post_message` and not enforceable here.
    let callbacks = Arc::new(EventCallbacks::new());
    let platform = ChatCallbackPlatform {
        chat: callbacks.clone(),
        events: Arc::new(NativeEventBus::default()),
    };
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        ProductExecutionKind::Worker,
    )
    .unwrap();
    let connection = crate::runtime::ActionChannel::chat();

    let posted = futures::executor::block_on(crate::platform::ChatPlatform::post_chat_message(
        &platform,
        &product,
        v01::HostChatPostMessageRequest {
            room_id: "support".to_string(),
            payload: v01::ChatMessageContent::Actions(v01::ChatActions {
                text: Some("pick one".to_string()),
                actions: vec![v01::ChatAction {
                    action_id: "approve".to_string(),
                    title: "Approve".to_string(),
                }],
                layout: v01::ChatActionLayout::Column,
            }),
        },
    ))
    .expect("an action set must reach the host");

    let mut actions = connection.subscribe::<truapi::latest::GenericError>();
    connection
        .publish(truapi::versioned::chat::HostChatActionSubscribeItem::V1(
            v01::HostChatActionSubscribeItem {
                room_id: "support".to_string(),
                peer: "alice".to_string(),
                payload: v01::ChatActionPayload::ActionTriggered(v01::ActionTrigger {
                    message_id: posted.message_id.clone(),
                    action_id: "approve".to_string(),
                    payload: None,
                }),
            },
        ))
        .expect("a trigger on a live subscription must be delivered");

    let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
    let delivered = match actions.poll_next_unpin(&mut cx) {
        core::task::Poll::Ready(Some(item)) => item,
        other => panic!("a published trigger must be ready, got {other:?}"),
    };
    let Ok(truapi::versioned::chat::HostChatActionSubscribeItem::V1(delivered)) = delivered
    else {
        panic!("expected a chat action item")
    };
    let v01::ChatActionPayload::ActionTriggered(trigger) = delivered.payload else {
        panic!(
            "expected an ActionTriggered payload, got {:?}",
            delivered.payload
        );
    };

    // The action set really reached the host, in the room it named.
    assert_eq!(
        callbacks
            .chat_posted
            .lock()
            .expect("posted messages mutex poisoned")
            .len(),
        1
    );
    // The id the product must match on to find the message it posted.
    assert_eq!(trigger.message_id, posted.message_id);
    assert_eq!(trigger.action_id, "approve");
}

#[test]
fn native_chat_adapter_surfaces_a_message_rejection() {
    let callbacks = Arc::new(EventCallbacks::new());
    let platform = ChatCallbackPlatform {
        chat: callbacks.clone(),
        events: Arc::new(NativeEventBus::default()),
    };
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        ProductExecutionKind::Worker,
    )
    .unwrap();
    *callbacks
        .chat_post_rejection
        .lock()
        .expect("post rejection mutex poisoned") =
        Some("cannot render a file card".to_string());

    let error = futures::executor::block_on(crate::platform::ChatPlatform::post_chat_message(
        &platform,
        &product,
        v01::HostChatPostMessageRequest {
            room_id: "support".to_string(),
            payload: v01::ChatMessageContent::File(v01::ChatFile {
                url: "https://example.invalid/f".to_string(),
                file_name: "f".to_string(),
                mime_type: "text/plain".to_string(),
                size_bytes: 1,
                text: None,
            }),
        },
    ))
    .expect_err("a host rejection must not be reported as a stored message");

    // Declining a variant is how a host that cannot render one opts out,
    // so a swallowed rejection would hand the product a message id for
    // something that was never persisted.
    assert_eq!(
        error,
        v01::HostChatPostMessageError::Unknown {
            reason: "cannot render a file card".to_string(),
        }
    );
    assert!(
        callbacks
            .chat_posted
            .lock()
            .expect("posted messages mutex poisoned")
            .is_empty()
    );
}

#[test]
fn native_chat_adapter_surfaces_a_bot_registration_rejection() {
    let callbacks = Arc::new(EventCallbacks::new());
    let platform = ChatCallbackPlatform {
        chat: callbacks.clone(),
        events: Arc::new(NativeEventBus::default()),
    };
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        ProductExecutionKind::Worker,
    )
    .unwrap();
    *callbacks
        .chat_bot_rejection
        .lock()
        .expect("bot rejection mutex poisoned") = Some("keychain locked".to_string());

    let error = futures::executor::block_on(crate::platform::ChatPlatform::register_chat_bot(
        &platform,
        &product,
        v01::HostChatRegisterBotRequest {
            bot_id: "flipper".to_string(),
            name: "Flipper".to_string(),
            icon: String::new(),
        },
    ))
    .expect_err("a host rejection must not be reported as a successful registration");

    // A swallowed rejection would reach the product as `New` for a bot
    // that does not exist.
    assert_eq!(
        error,
        v01::HostChatRegisterBotError::Unknown {
            reason: "keychain locked".to_string(),
        }
    );
    assert!(
        callbacks
            .chat_registered_bots
            .lock()
            .expect("registered bots mutex poisoned")
            .is_empty()
    );
}

#[test]
fn native_chat_adapter_preserves_bot_status_and_leaves_rooms_alone() {
    let callbacks = Arc::new(EventCallbacks::new());
    let events = Arc::new(NativeEventBus::default());
    let platform = ChatCallbackPlatform {
        chat: callbacks.clone(),
        events: events.clone(),
    };
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        ProductExecutionKind::Worker,
    )
    .unwrap();
    let request = v01::HostChatRegisterBotRequest {
        bot_id: "flipper".to_string(),
        name: "Flipper".to_string(),
        icon: String::new(),
    };

    let mut rooms = crate::platform::ChatPlatform::subscribe_chat_rooms(&platform, &product);
    assert!(
        ready_rooms(rooms.as_mut(), "initial room list")
            .rooms
            .is_empty()
    );

    let registered = futures::executor::block_on(
        crate::platform::ChatPlatform::register_chat_bot(&platform, &product, request.clone()),
    )
    .unwrap();

    *callbacks
        .chat_bot_status
        .lock()
        .expect("bot status mutex poisoned") = v01::ChatBotRegistrationStatus::Exists;
    let existing = futures::executor::block_on(
        crate::platform::ChatPlatform::register_chat_bot(&platform, &product, request),
    )
    .unwrap();

    assert_eq!(registered.status, v01::ChatBotRegistrationStatus::New);
    assert_eq!(existing.status, v01::ChatBotRegistrationStatus::Exists);
    assert_eq!(
        callbacks
            .chat_registered_bots
            .lock()
            .expect("registered bots mutex poisoned")
            .as_slice(),
        &[
            ("flipper".to_string(), "Flipper".to_string(), String::new()),
            ("flipper".to_string(), "Flipper".to_string(), String::new()),
        ]
    );

    // Registering a bot is not a room change. Polled without blocking so an
    // unexpected replacement fails instead of parking on a live sender.
    let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
    assert!(matches!(
        rooms.as_mut().poll_next(&mut cx),
        core::task::Poll::Pending
    ));

    // Still live for genuine room changes.
    events.notify_chat_rooms_changed(vec![v01::ChatRoom {
        room_id: "support".to_string(),
        participating_as: v01::ChatRoomParticipation::Bot,
    }]);
    assert_eq!(
        ready_rooms(rooms.as_mut(), "a genuine room change")
            .rooms
            .len(),
        1
    );
}

#[test]
fn native_chat_adapter_preserves_room_status_and_message_room() {
    let callbacks = Arc::new(EventCallbacks::new());
    let platform = ChatCallbackPlatform {
        chat: callbacks.clone(),
        events: Arc::new(NativeEventBus::default()),
    };
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        ProductExecutionKind::Worker,
    )
    .unwrap();
    let request = v01::HostChatCreateRoomRequest {
        room_id: "support".to_string(),
        name: "Support".to_string(),
        icon: String::new(),
    };
    let mut rooms = crate::platform::ChatPlatform::subscribe_chat_rooms(&platform, &product);
    assert!(
        ready_rooms(rooms.as_mut(), "initial room list")
            .rooms
            .is_empty()
    );

    let created = futures::executor::block_on(crate::platform::ChatPlatform::create_chat_room(
        &platform,
        &product,
        request.clone(),
    ))
    .unwrap();
    let updated_rooms = ready_rooms(rooms.as_mut(), "the created room replacement");
    *callbacks
        .chat_room_status
        .lock()
        .expect("room status mutex poisoned") = v01::ChatRoomRegistrationStatus::Exists;
    let existing = futures::executor::block_on(
        crate::platform::ChatPlatform::create_chat_room(&platform, &product, request),
    )
    .unwrap();
    let posted = futures::executor::block_on(crate::platform::ChatPlatform::post_chat_message(
        &platform,
        &product,
        v01::HostChatPostMessageRequest {
            room_id: "second-room".to_string(),
            payload: v01::ChatMessageContent::Text {
                text: "Echo: hello".to_string(),
            },
        },
    ))
    .unwrap();

    assert_eq!(created.status, v01::ChatRoomRegistrationStatus::New);
    assert_eq!(updated_rooms.rooms.len(), 1);
    assert_eq!(updated_rooms.rooms[0].room_id, "support");
    assert_eq!(existing.status, v01::ChatRoomRegistrationStatus::Exists);
    assert_eq!(posted.message_id, "message-1");
    assert_eq!(
        callbacks
            .chat_created_rooms
            .lock()
            .expect("created rooms mutex poisoned")
            .as_slice(),
        &[
            ("support".to_string(), "Support".to_string(), String::new()),
            ("support".to_string(), "Support".to_string(), String::new()),
        ]
    );
    assert_eq!(
        callbacks
            .chat_posted
            .lock()
            .expect("posted messages mutex poisoned")
            .as_slice(),
        &[(
            "second-room".to_string(),
            v01::ChatMessageContent::Text {
                text: "Echo: hello".to_string(),
            },
        )]
    );
}

#[test]
fn native_chain_provider_rejects_an_early_close_and_allows_a_later_retry() {
    let (callbacks, events, platform) = event_platform();
    *callbacks.chain_id.lock().unwrap() = Some(42);
    let closing_events = events.clone();
    *callbacks.on_chain_connect.lock().unwrap() = Some(Box::new(move || {
        closing_events.notify_chain_closed(42);
    }));

    let closed = futures::executor::block_on(ChainProvider::connect(&platform, [9; 32]));
    assert_eq!(
        closed.map(|_| ()),
        Err(ProviderError::Host {
            reason: "chain connection closed during setup".to_string(),
        })
    );
    assert_eq!(*callbacks.chain_closes.lock().unwrap(), vec![42]);

    *callbacks.chain_id.lock().unwrap() = Some(43);
    let connection =
        futures::executor::block_on(ChainProvider::connect(&platform, [9; 32])).unwrap();
    let mut responses = connection.responses();
    events.notify_chain_response(43, "after retry".to_string());
    assert_eq!(
        futures::executor::block_on(responses.next()),
        Some("after retry".to_string())
    );
    events.notify_chain_closed(43);
    assert_eq!(futures::executor::block_on(responses.next()), None);
    drop(connection);
    assert_eq!(*callbacks.chain_closes.lock().unwrap(), vec![42, 43]);
}

#[test]
fn native_chain_provider_keeps_new_setup_when_an_active_connection_closes() {
    let (callbacks, events, platform) = event_platform();
    *callbacks.chain_id.lock().unwrap() = Some(41);
    let previous =
        futures::executor::block_on(ChainProvider::connect(&platform, [9; 32])).unwrap();
    let mut previous_responses = previous.responses();
    *callbacks.chain_id.lock().unwrap() = Some(42);
    let closing_events = events.clone();
    *callbacks.on_chain_connect.lock().unwrap() = Some(Box::new(move || {
        closing_events.notify_chain_closed(41);
        previous.close();
    }));

    let connection =
        futures::executor::block_on(ChainProvider::connect(&platform, [9; 32])).unwrap();
    let mut responses = connection.responses();
    events.notify_chain_response(42, "new connection".to_string());
    assert_eq!(
        (
            futures::executor::block_on(previous_responses.next()),
            futures::executor::block_on(responses.next()),
        ),
        (None, Some("new connection".to_string()))
    );
    drop(connection);
    assert_eq!(*callbacks.chain_closes.lock().unwrap(), vec![41, 42]);
}

#[test]
fn native_chain_provider_forwards_send_response_and_close() {
    let (callbacks, events, platform) = event_platform();
    *callbacks.chain_id.lock().expect("chain id mutex poisoned") = Some(42);
    let genesis = [9; 32];

    let connection = futures::executor::block_on(ChainProvider::connect(&platform, genesis))
        .expect("chain connection should open");
    connection.send(r#"{"jsonrpc":"2.0","id":1}"#.to_string());
    let mut responses = connection.responses();
    events.notify_chain_response(42, r#"{"jsonrpc":"2.0","id":1,"result":true}"#.to_string());
    let response = futures::executor::block_on(responses.next()).unwrap();
    drop(responses);
    drop(connection);

    assert_eq!(
        callbacks
            .chain_connects
            .lock()
            .expect("chain connects mutex poisoned")
            .as_slice(),
        &[genesis.to_vec()]
    );
    assert_eq!(
        callbacks
            .chain_sends
            .lock()
            .expect("chain sends mutex poisoned")
            .as_slice(),
        &[(42, r#"{"jsonrpc":"2.0","id":1}"#.to_string())]
    );
    assert_eq!(response, r#"{"jsonrpc":"2.0","id":1,"result":true}"#);
    assert_eq!(
        callbacks
            .chain_closes
            .lock()
            .expect("chain closes mutex poisoned")
            .as_slice(),
        &[42]
    );
}

#[test]
fn runtime_config_reports_the_platform_the_library_was_built_for() {
    let resolved = NativeResolvedHostRuntimeConfig::try_from(native_host_runtime_config())
        .expect("config is valid");

    assert_eq!(
        resolved.signing.host.host_info.platform,
        native_host_platform()
    );
}

#[test]
fn runtime_config_rejects_wrong_size_genesis_hash() {
    let err = NativeResolvedHostRuntimeConfig::try_from(HostRuntimeConfig {
        people_chain_genesis_hash: vec![0; 31],
        ..native_host_runtime_config()
    })
    .unwrap_err();

    assert_eq!(
        err,
        NativeRuntimeConfigError::Invalid {
            reason: "people_chain_genesis_hash must be exactly 32 bytes, got 31".to_string(),
        }
    );
}

#[test]
fn each_configured_genesis_hash_reaches_its_own_field() {
    // Three adjacent `Vec<u8>` feeding a positional constructor:
    // transposing any two compiles and, without this, passes.
    let resolved = NativeResolvedHostRuntimeConfig::try_from(HostRuntimeConfig {
        people_chain_genesis_hash: vec![0xa1; 32],
        bulletin_chain_genesis_hash: vec![0xb2; 32],
        asset_hub_chain_genesis_hash: vec![0xc3; 32],
        ..native_host_runtime_config()
    })
    .expect("config is valid");

    assert_eq!(
        (
            resolved.signing.people_chain_genesis_hash,
            resolved.signing.bulletin_chain_genesis_hash,
            resolved.signing.asset_hub_chain_genesis_hash,
        ),
        ([0xa1; 32], [0xb2; 32], [0xc3; 32]),
    );
}

#[test]
fn a_wrong_size_asset_hub_genesis_hash_is_rejected_as_its_own_field() {
    // An empty vec must be an error, never a silent all-zero "no Asset
    // Hub".
    for len in [0usize, 31, 33] {
        let err = NativeResolvedHostRuntimeConfig::try_from(HostRuntimeConfig {
            asset_hub_chain_genesis_hash: vec![0; len],
            ..native_host_runtime_config()
        })
        .unwrap_err();

        assert_eq!(
            err,
            NativeRuntimeConfigError::Invalid {
                reason: format!("asset_hub_chain_genesis_hash must be exactly 32 bytes, got {len}"),
            }
        );
    }
}

#[test]
fn runtime_config_rejects_a_network_suffix_that_is_not_a_bare_tld() {
    // The suffix ends every reserved derivation (`peopl.<suffix>`), so a
    // shell passing the dotted form would silently derive a stranger.
    let err = NativeResolvedHostRuntimeConfig::try_from(HostRuntimeConfig {
        network_suffix: ".paseo".to_string(),
        ..native_host_runtime_config()
    })
    .unwrap_err();
    assert_eq!(
        err,
        NativeRuntimeConfigError::Invalid {
            reason: r#"network_suffix must be a supported dotNS TLD, got ".paseo""#.to_string(),
        }
    );
}

#[test]
fn product_execution_config_rejects_empty_product_id() {
    let err = ProductContext::try_from(ProductExecutionConfig {
        product_id: " ".to_string(),
        ..native_execution_config("app.dot", ProductExecutionKind::App)
    })
    .unwrap_err();

    assert_eq!(
        err,
        NativeRuntimeConfigError::Invalid {
            reason: "product_id must not be empty".to_string(),
        }
    );
}

#[test]
fn runtime_config_rejects_relative_host_icon() {
    let err = NativeResolvedHostRuntimeConfig::try_from(HostRuntimeConfig {
        host_icon: Some("/dotli.png".to_string()),
        ..native_host_runtime_config()
    })
    .unwrap_err();

    assert_eq!(
        err,
        NativeRuntimeConfigError::Invalid {
            reason: "host_info.icon must be an absolute HTTPS URL: relative URL without a base"
                .to_string(),
        }
    );
}

#[test]
fn runtime_config_rejects_non_https_host_icon() {
    let err = NativeResolvedHostRuntimeConfig::try_from(HostRuntimeConfig {
        host_icon: Some("http://localhost:3000/dotli.png".to_string()),
        ..native_host_runtime_config()
    })
    .unwrap_err();

    assert_eq!(
        err,
        NativeRuntimeConfigError::Invalid {
            reason: r#"host_info.icon must use https scheme, got "http""#.to_string(),
        }
    );
}

/// Calling `start_ws_bridge` twice on the same product execution
/// without an intervening `stop_ws_bridge` is a hard error. The bridge
/// is single-instance per execution, so the second start must surface
/// `AlreadyRunning` rather than silently leaking a worker thread.
#[test]
fn start_ws_bridge_twice_returns_already_running() {
    struct Noop;
    #[async_trait::async_trait]
    impl HostCallbacks for Noop {
        async fn confirm_permission(
            &self,
            _review: UserConfirmationReview,
        ) -> Result<PermissionDecision, HostRejection> {
            Ok(PermissionDecision::Deny)
        }

        fn on_core_log(&self, _marker: String, _detail: String) {}
        fn worker_demand_changed(&self, _product_id: String, _transition: WorkerTransition) {}
        fn device_paired(&self, _device: PairedSsoPeer) {}
        async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
            Ok(())
        }
        async fn push_notification(
            &self,
            _request: v01::HostPushNotificationRequest,
        ) -> Result<u32, HostRejection> {
            Ok(0)
        }
        fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn device_permission(
            &self,
            _product: ProductExecutionConfig,
            _request: v01::HostDevicePermissionRequest,
        ) -> Result<PermissionDecision, HostRejection> {
            Ok(PermissionDecision::Deny)
        }
        async fn device_permission_status(
            &self,
            _request: v01::HostDevicePermissionRequest,
        ) -> Result<DevicePermissionStatus, HostRejection> {
            Ok(DevicePermissionStatus::NotApplicable)
        }
        async fn remote_permission(
            &self,
            _product: ProductExecutionConfig,
            _request: v01::RemotePermission,
        ) -> Result<PermissionDecision, HostRejection> {
            Ok(PermissionDecision::Deny)
        }
        fn auth_state_changed(&self, _state: AuthState) {}
        async fn core_storage_read(
            &self,
            _key: Vec<u8>,
        ) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(None)
        }
        async fn core_storage_write(
            &self,
            _key: Vec<u8>,
            _value: Vec<u8>,
        ) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn chain_connect(&self, _genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
            Ok(None)
        }
        fn chain_send(
            &self,
            _connection_id: u32,
            _request: String,
        ) -> Result<(), HostRejection> {
            Ok(())
        }
        fn chain_close(&self, _connection_id: u32) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn confirm_user_action(
            &self,
            _review: UserConfirmationReview,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        async fn lookup_preimage(
            &self,
            _key: Vec<u8>,
        ) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(None)
        }
        fn current_theme(&self) -> Result<v01::HostThemeSubscribeItem, HostRejection> {
            Ok(v01::HostThemeSubscribeItem {
                name: v01::ThemeName::Default,
                variant: v01::ThemeVariant::Light,
            })
        }
        fn current_locale(&self) -> Result<v01::HostLocaleSubscribeItem, HostRejection> {
            Ok(v01::HostLocaleSubscribeItem {
                language_tag: "en".to_string(),
            })
        }
        async fn feature_supported(
            &self,
            _request: v01::HostFeatureSupportedRequest,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn supported_chains(&self) -> Result<crate::platform::HostChainSet, HostRejection> {
            Ok(crate::platform::HostChainSet {
                network: "paseo".to_string(),
                chains: Vec::new(),
            })
        }
        async fn local_storage_read(
            &self,
            _key: String,
        ) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
            Ok(None)
        }
        async fn local_storage_write(
            &self,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<(), v01::HostLocalStorageReadError> {
            Ok(())
        }
        async fn local_storage_clear(
            &self,
            _key: String,
        ) -> Result<(), v01::HostLocalStorageReadError> {
            Ok(())
        }
        async fn begin_operation(
            &self,
            _product_id: String,
            _label: String,
        ) -> Result<u32, HostRejection> {
            Ok(1)
        }
        async fn end_operation(
            &self,
            _product_id: String,
            _id: u32,
        ) -> Result<(), HostRejection> {
            Ok(())
        }
    }

    let execution = native_product_execution(Arc::new(Noop), "dotli.dot");
    let _first = execution
        .start_ws_bridge(0)
        .expect("first start must succeed");
    let err = execution
        .start_ws_bridge(0)
        .expect_err("second start must error");
    assert!(matches!(err, WsBridgeStartError::AlreadyRunning));
    execution.stop_ws_bridge();
}

/// A permission callback suspends while awaiting the user's decision and
/// holds no executor worker, so an unrelated request on the same
/// connection still round-trips while the decision is pending.
#[test]
fn pending_permission_decision_does_not_stall_bridge() {
    use std::sync::atomic::{AtomicBool, Ordering};

    use futures::SinkExt;
    use parity_scale_codec::Decode;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    use crate::frame::{Payload, ProtocolMessage, request_ids};

    /// `device_permission` stays pending until the test sends on
    /// `release`; every other callback is a trivial success.
    struct GatedPermissionCallbacks {
        permission_entered: Arc<AtomicBool>,
        release: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<()>>,
    }

    #[async_trait::async_trait]
    impl HostCallbacks for GatedPermissionCallbacks {
        async fn confirm_permission(
            &self,
            _review: UserConfirmationReview,
        ) -> Result<PermissionDecision, HostRejection> {
            Ok(PermissionDecision::Deny)
        }

        fn on_core_log(&self, _marker: String, _detail: String) {}
        fn worker_demand_changed(&self, _product_id: String, _transition: WorkerTransition) {}
        fn device_paired(&self, _device: PairedSsoPeer) {}
        async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
            Ok(())
        }
        async fn push_notification(
            &self,
            _request: v01::HostPushNotificationRequest,
        ) -> Result<u32, HostRejection> {
            Ok(0)
        }
        fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn device_permission(
            &self,
            _product: ProductExecutionConfig,
            _request: v01::HostDevicePermissionRequest,
        ) -> Result<PermissionDecision, HostRejection> {
            self.permission_entered.store(true, Ordering::SeqCst);
            self.release
                .lock()
                .await
                .recv()
                .await
                .expect("release signal");
            Ok(PermissionDecision::AllowAlways)
        }
        async fn device_permission_status(
            &self,
            _request: v01::HostDevicePermissionRequest,
        ) -> Result<DevicePermissionStatus, HostRejection> {
            Ok(DevicePermissionStatus::NotApplicable)
        }
        async fn remote_permission(
            &self,
            _product: ProductExecutionConfig,
            _request: v01::RemotePermission,
        ) -> Result<PermissionDecision, HostRejection> {
            Ok(PermissionDecision::Deny)
        }
        fn auth_state_changed(&self, _state: AuthState) {}
        async fn core_storage_read(
            &self,
            _key: Vec<u8>,
        ) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(None)
        }
        async fn core_storage_write(
            &self,
            _key: Vec<u8>,
            _value: Vec<u8>,
        ) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn chain_connect(&self, _genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
            Ok(None)
        }
        fn chain_send(
            &self,
            _connection_id: u32,
            _request: String,
        ) -> Result<(), HostRejection> {
            Ok(())
        }
        fn chain_close(&self, _connection_id: u32) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn confirm_user_action(
            &self,
            _review: UserConfirmationReview,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        async fn lookup_preimage(
            &self,
            _key: Vec<u8>,
        ) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(None)
        }
        fn current_theme(&self) -> Result<v01::HostThemeSubscribeItem, HostRejection> {
            Ok(v01::HostThemeSubscribeItem {
                name: v01::ThemeName::Default,
                variant: v01::ThemeVariant::Light,
            })
        }
        fn current_locale(&self) -> Result<v01::HostLocaleSubscribeItem, HostRejection> {
            Ok(v01::HostLocaleSubscribeItem {
                language_tag: "en".to_string(),
            })
        }
        async fn feature_supported(
            &self,
            _request: v01::HostFeatureSupportedRequest,
        ) -> Result<bool, HostRejection> {
            Ok(true)
        }
        fn supported_chains(&self) -> Result<crate::platform::HostChainSet, HostRejection> {
            Ok(crate::platform::HostChainSet {
                network: "paseo".to_string(),
                chains: Vec::new(),
            })
        }
        async fn local_storage_read(
            &self,
            _key: String,
        ) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
            Ok(None)
        }
        async fn local_storage_write(
            &self,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<(), v01::HostLocalStorageReadError> {
            Ok(())
        }
        async fn local_storage_clear(
            &self,
            _key: String,
        ) -> Result<(), v01::HostLocalStorageReadError> {
            Ok(())
        }
        async fn begin_operation(
            &self,
            _product_id: String,
            _label: String,
        ) -> Result<u32, HostRejection> {
            Ok(1)
        }
        async fn end_operation(
            &self,
            _product_id: String,
            _id: u32,
        ) -> Result<(), HostRejection> {
            Ok(())
        }
    }

    let (release_tx, release_rx) = tokio::sync::mpsc::channel::<()>(1);
    let permission_entered = Arc::new(AtomicBool::new(false));
    let execution = native_product_execution(
        Arc::new(GatedPermissionCallbacks {
            permission_entered: permission_entered.clone(),
            release: tokio::sync::Mutex::new(release_rx),
        }),
        "dotli.dot",
    );
    let endpoint = execution.start_ws_bridge(0).expect("start bridge");
    let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    let permission_ids =
        request_ids("permissions_request_device_permission").expect("known request method");
    let feature_ids = request_ids("system_feature_supported").expect("known request method");
    let (feature_response, permission_response) = rt.block_on(async {
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");

        let permission_value = truapi::versioned::permissions::HostDevicePermissionRequest::V1(
            v01::HostDevicePermissionRequest::Camera,
        )
        .encode();
        let permission_frame = ProtocolMessage {
            request_id: "p:permission".into(),
            payload: Payload {
                trait_id: permission_ids.trait_id,
                method_id: permission_ids.method_id,
                message_type: crate::frame::MESSAGE_TYPE_REQUEST,
                value: permission_value,
            },
        };
        ws.send(WsMessage::Binary(permission_frame.encode()))
            .await
            .expect("send device permission");

        // Wait until the permission callback is blocked on the decision.
        for _ in 0..1000 {
            if permission_entered.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(
            permission_entered.load(Ordering::SeqCst),
            "permission callback was not invoked"
        );

        let feature_value = truapi::versioned::system::HostFeatureSupportedRequest::V1(
            v01::HostFeatureSupportedRequest::Chain {
                genesis_hash: vec![0u8; 32],
            },
        )
        .encode();
        let feature_frame = ProtocolMessage {
            request_id: "p:feature".into(),
            payload: Payload {
                trait_id: feature_ids.trait_id,
                method_id: feature_ids.method_id,
                message_type: crate::frame::MESSAGE_TYPE_REQUEST,
                value: feature_value,
            },
        };
        ws.send(WsMessage::Binary(feature_frame.encode()))
            .await
            .expect("send feature_supported");

        let feature_response =
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    match ws.next().await {
                        Some(Ok(WsMessage::Binary(bytes))) => {
                            break ProtocolMessage::decode(&mut &bytes[..])
                                .expect("decode response");
                        }
                        Some(Ok(_)) => continue,
                        Some(Err(err)) => panic!("ws error: {err}"),
                        None => panic!("connection closed before response"),
                    }
                }
            })
            .await
            .expect("feature_supported must answer while the permission decision is pending");

        release_tx
            .send(())
            .await
            .expect("release permission callback");
        let permission_response =
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    match ws.next().await {
                        Some(Ok(WsMessage::Binary(bytes))) => {
                            break ProtocolMessage::decode(&mut &bytes[..])
                                .expect("decode response");
                        }
                        Some(Ok(_)) => continue,
                        Some(Err(err)) => panic!("ws error: {err}"),
                        None => panic!("connection closed before response"),
                    }
                }
            })
            .await
            .expect("released permission must answer");

        (feature_response, permission_response)
    });

    assert_eq!(feature_response.request_id, "p:feature");
    assert_eq!(feature_response.payload.trait_id, feature_ids.trait_id);
    assert_eq!(feature_response.payload.method_id, feature_ids.method_id);

    assert_eq!(permission_response.request_id, "p:permission");
    assert_eq!(
        permission_response.payload.trait_id,
        permission_ids.trait_id
    );
    assert_eq!(
        permission_response.payload.method_id,
        permission_ids.method_id
    );
    assert_eq!(
        permission_response.payload.message_type,
        crate::frame::MESSAGE_TYPE_RESPONSE
    );
    let expected_permission: Result<
        truapi::versioned::permissions::HostDevicePermissionResponse,
        truapi::CallError<truapi::versioned::permissions::HostDevicePermissionError>,
    > = Ok(
        truapi::versioned::permissions::HostDevicePermissionResponse::V1(
            v01::HostDevicePermissionResponse { granted: true },
        ),
    );
    assert_eq!(
        permission_response.payload.value,
        expected_permission.encode()
    );

    execution.stop_ws_bridge();
}

#[test]
fn closing_an_execution_releases_its_callbacks_while_the_host_lives() {
    let host = native_host_runtime_no_session();
    let callbacks = Arc::new(EventCallbacks::new());
    let weak_callbacks = Arc::downgrade(&callbacks);
    let execution = host
        .open_product_execution(
            callbacks,
            None,
            None,
            None,
            native_execution_config("first.dot", ProductExecutionKind::App),
        )
        .expect("open execution");
    execution.start_ws_bridge(0).expect("start bridge");

    execution.shutdown();
    drop(execution);

    assert!(
        weak_callbacks.upgrade().is_none(),
        "the listener must not retain a closed execution's callbacks"
    );
    drop(host);
}

#[test]
fn bridge_logs_follow_the_host_and_authenticated_execution() {
    use futures::SinkExt;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    let callbacks = [
        Arc::new(EventCallbacks::new()),
        Arc::new(EventCallbacks::new()),
        Arc::new(EventCallbacks::new()),
    ];
    let host = NativeTrUApiHostRuntime::with_runtime_config(
        callbacks[0].clone(),
        native_host_runtime_config(),
    )
    .expect("create host");
    let executions = [(1, "first.dot"), (2, "second.dot")].map(|(index, product_id)| {
        host.open_product_execution(
            callbacks[index].clone(),
            None,
            None,
            None,
            native_execution_config(product_id, ProductExecutionKind::App),
        )
        .expect("open execution")
    });
    executions[0].start_ws_bridge(0).expect("start first");
    let endpoint = executions[1].start_ws_bridge(0).expect("start second");
    let client = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("client runtime");
    let socket = client.block_on(async {
        let (mut socket, _) = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{}/?t={}",
            endpoint.port, endpoint.token
        ))
        .await
        .expect("connect second");
        socket
            .send(WsMessage::Text("ignored".into()))
            .await
            .expect("send text");
        socket
    });
    crate::test_support::wait_until(
        || {
            callbacks.iter().any(|callbacks| {
                callbacks
                    .logs
                    .lock()
                    .expect("logs mutex poisoned")
                    .iter()
                    .any(|marker| marker == "truapi.ws_bridge.text_frame_ignored")
            })
        },
        "connection did not process the text frame",
    );
    let logs = callbacks.map(|callbacks| {
        callbacks
            .logs
            .lock()
            .expect("logs mutex poisoned")
            .iter()
            .filter(|marker| marker.starts_with("truapi.ws_bridge."))
            .cloned()
            .collect::<Vec<_>>()
    });
    assert_eq!(
        logs,
        [
            vec!["truapi.ws_bridge.started"],
            vec![],
            vec![
                "truapi.ws_bridge.connection_open",
                "truapi.ws_bridge.text_frame_ignored"
            ],
        ]
    );
    drop(socket);
}

#[test]
fn two_executions_share_one_bridge_through_the_native_api() {
    use futures::SinkExt;
    use parity_scale_codec::Decode;
    use tokio_tungstenite::tungstenite::Message as WsMessage;
    use truapi::versioned::system::HostFeatureSupportedRequest;

    use crate::frame::{Payload, ProtocolMessage, request_ids};

    let host = NativeTrUApiHostRuntime::with_runtime_config(
        Arc::new(EventCallbacks::new()),
        native_host_runtime_config(),
    )
    .expect("host runtime config should be valid");
    let app = host
        .open_product_execution(
            Arc::new(EventCallbacks::new()),
            None,
            None,
            None,
            native_execution_config("shared.dot", ProductExecutionKind::App),
        )
        .expect("App execution should open");
    let chat_host = Arc::new(EventCallbacks::new());
    let chat = host
        .open_product_execution(
            chat_host.clone(),
            Some(chat_host),
            None,
            None,
            native_execution_config("shared.dot", ProductExecutionKind::Worker),
        )
        .expect("Chat execution should open");

    let app_endpoint = app.start_ws_bridge(0).expect("start app bridge");
    let chat_endpoint = chat.start_ws_bridge(0).expect("start chat bridge");
    assert_eq!(
        app_endpoint.port, chat_endpoint.port,
        "both executions must share the one listener port"
    );
    assert_ne!(
        app_endpoint.token, chat_endpoint.token,
        "each execution must get its own token"
    );

    let feature_ids = request_ids("system_feature_supported").expect("known request method");
    let round_trip = |request_id: &str| ProtocolMessage {
        request_id: request_id.into(),
        payload: Payload {
            trait_id: feature_ids.trait_id,
            method_id: feature_ids.method_id,
            message_type: crate::frame::MESSAGE_TYPE_REQUEST,
            value: HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
                genesis_hash: vec![0u8; 32],
            })
            .encode(),
        },
    };
    async fn answer<S>(ws: &mut S) -> ProtocolMessage
    where
        S: futures::Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
            + Unpin,
    {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                match ws.next().await {
                    Some(Ok(WsMessage::Binary(bytes))) => {
                        break ProtocolMessage::decode(&mut &bytes[..])
                            .expect("decode response");
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(err)) => panic!("ws error: {err}"),
                    None => panic!("connection closed before response"),
                }
            }
        })
        .await
        .expect("must answer")
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    rt.block_on(async {
        let app_url = format!(
            "ws://127.0.0.1:{}/?t={}",
            app_endpoint.port, app_endpoint.token
        );
        let chat_url = format!(
            "ws://127.0.0.1:{}/?t={}",
            chat_endpoint.port, chat_endpoint.token
        );

        let (mut app_ws, _) = tokio_tungstenite::connect_async(&app_url)
            .await
            .expect("app dial");
        let (mut chat_ws, _) = tokio_tungstenite::connect_async(&chat_url)
            .await
            .expect("chat dial");

        app_ws
            .send(WsMessage::Binary(round_trip("app:1").encode()))
            .await
            .expect("send on app connection");
        assert_eq!(answer(&mut app_ws).await.request_id, "app:1");

        chat_ws
            .send(WsMessage::Binary(round_trip("chat:1").encode()))
            .await
            .expect("send on chat connection");
        assert_eq!(answer(&mut chat_ws).await.request_id, "chat:1");

        app.stop_ws_bridge();

        chat_ws
            .send(WsMessage::Binary(round_trip("chat:2").encode()))
            .await
            .expect("send on chat connection after App stops");
        assert_eq!(
            answer(&mut chat_ws).await.request_id,
            "chat:2",
            "Chat's connection must keep answering after a sibling execution stops"
        );

        assert!(
            tokio_tungstenite::connect_async(&app_url).await.is_err(),
            "a revoked token must not still be accepted"
        );

        chat.stop_ws_bridge();
    });
}

pub fn native_host_runtime_no_session() -> Arc<NativeTrUApiHostRuntime> {
    let mut config = native_host_runtime_config();
    config.local_session_secret = None;
    config.local_session_lite_username = None;
    NativeTrUApiHostRuntime::with_runtime_config(Arc::new(EventCallbacks::new()), config)
        .expect("host runtime config should be valid")
}

#[test]
fn handle_sso_request_rejects_undecodable_bytes() {
    let runtime = native_host_runtime_no_session();
    let result =
        futures::executor::block_on(runtime.handle_sso_request(vec![0xFF, 0xFF, 0xFF]));
    assert!(result.is_err(), "garbage bytes must be a decode error");
}

#[test]
fn prepare_disconnect_request_round_trips_with_fresh_ids() {
    use crate::host_internal::sso_messages::{RemoteMessage, RemoteMessageData, v1};
    use parity_scale_codec::Decode;
    let runtime = native_host_runtime_no_session();
    let bytes = runtime.prepare_disconnect_request();
    let message = RemoteMessage::decode(&mut bytes.as_slice()).expect("valid encoding");
    assert_eq!(message.message_id.len(), 8, "opaque nanoid message id");
    assert!(matches!(
        message.data,
        RemoteMessageData::V1(v1::RemoteMessage::Disconnected)
    ));
    let second = RemoteMessage::decode(&mut runtime.prepare_disconnect_request().as_slice())
        .expect("valid encoding");
    assert_ne!(
        message.message_id, second.message_id,
        "each disconnect message carries its own id"
    );
}

#[test]
fn bytes32_widens_to_plain_bytes_on_the_wire() {
    let mut buf = Vec::new();
    <Bytes32 as uniffi::Lower<truapi::UniFfiTag>>::write([7; 32], &mut buf);
    assert_eq!(buf[..4], 32i32.to_be_bytes());
    assert_eq!(buf[4..], [7; 32]);
}

#[test]
fn bytes32_lift_rejects_wrong_length() {
    let mut buf = Vec::new();
    <Vec<u8> as uniffi::Lower<crate::UniFfiTag>>::write(vec![7; 31], &mut buf);
    assert!(
        <Bytes32 as uniffi::Lift<truapi::UniFfiTag>>::try_read(&mut buf.as_slice()).is_err()
    );
}

#[test]
fn bytes32_fields_survive_the_ffi_roundtrip() {
    let review = UserConfirmationReview::CreateTransaction(
        CreateTransactionReview::LegacyAccount(LegacyAccountTxPayload {
            signer: [13; 32],
            genesis_hash: [14; 32],
            call_data: vec![15],
            extensions: vec![],
            tx_ext_version: 0,
        }),
    );

    let mut buf = Vec::new();
    <UserConfirmationReview as uniffi::Lower<crate::UniFfiTag>>::write(
        review.clone(),
        &mut buf,
    );
    let lifted = <UserConfirmationReview as uniffi::Lift<crate::UniFfiTag>>::try_read(
        &mut buf.as_slice(),
    )
    .expect("review must lift back");
    assert_eq!(lifted, review);
}

#[test]
fn native_remote_authorization_uses_the_execution_permission_callback() {
    for (answer, granted) in [
        (Ok(PermissionDecision::AllowAlways), true),
        (Ok(PermissionDecision::Deny), false),
        (
            Err(HostRejection::Rejected {
                reason: "permission UI unavailable".to_string(),
            }),
            false,
        ),
    ] {
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            Arc::new(EventCallbacks::new()),
            native_host_runtime_config(),
        )
        .unwrap();
        let callbacks = Arc::new(EventCallbacks {
            remote_permission_result: answer,
            ..EventCallbacks::new()
        });
        let execution = host
            .open_product_execution(
                callbacks.clone(),
                None,
                None,
                None,
                native_execution_config("fetch.dot", ProductExecutionKind::Worker),
            )
            .unwrap();
        let request = truapi::latest::RemotePermissionRequest {
            permission: truapi::latest::RemotePermission::Remote {
                domains: vec!["api.example.com".to_string()],
            },
        };
        let response =
            futures::executor::block_on(execution.authorize_remote_permission(request))
                .unwrap();
        assert_eq!(
            (
                response,
                callbacks.remote_permission_products.lock().unwrap().clone()
            ),
            (
                granted,
                vec![("fetch.dot".to_string(), ProductExecutionKind::Worker)]
            ),
        );
    }
}

#[test]
fn native_permission_confirmation_preserves_consent_lifetime() {
    futures::executor::block_on(async {
        for decision in [
            PermissionDecision::AllowOnce,
            PermissionDecision::AllowAlways,
            PermissionDecision::Deny,
        ] {
            let platform = CallbackPlatform {
                callbacks: Arc::new(EventCallbacks {
                    permission_confirmation_result: decision,
                    ..EventCallbacks::new()
                }),
                events: Arc::default(),
                storage_events: Arc::default(),
            };
            let review = UserConfirmationReview::IdentityDisclosure(
                crate::platform::IdentityDisclosureReview {
                    product_id: "product.dot".to_string(),
                },
            );
            assert_eq!(
                (
                    platform.confirm_permission(review.clone()).await.unwrap(),
                    platform.confirm_user_action(review).await.unwrap(),
                ),
                (decision, false),
            );
        }
    });
}

#[test]
fn native_remote_authorization_reuses_stored_product_decisions() {
    for (decision, granted) in [
        (PermissionDecision::AllowAlways, true),
        (PermissionDecision::Deny, false),
    ] {
        let callbacks = Arc::new(EventCallbacks {
            remote_permission_result: Ok(decision),
            ..EventCallbacks::new()
        });
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            callbacks.clone(),
            native_host_runtime_config(),
        )
        .unwrap();
        let open = |product_id| {
            host.open_product_execution(
                callbacks.clone(),
                None,
                None,
                None,
                native_execution_config(product_id, ProductExecutionKind::App),
            )
            .unwrap()
        };
        let first_execution = open("fetch.dot");
        let next_execution = open("fetch.dot");
        let other_product = open("other.dot");
        let request = truapi::latest::RemotePermissionRequest {
            permission: truapi::latest::RemotePermission::Remote {
                domains: vec!["api.example.com".to_string()],
            },
        };
        futures::executor::block_on(async {
            let first = first_execution
                .authorize_remote_permission(request.clone())
                .await
                .unwrap();
            let next = next_execution
                .authorize_remote_permission(request.clone())
                .await
                .unwrap();
            let prompts_for_product = callbacks.remote_permission_calls.load(Ordering::SeqCst);
            let other = other_product
                .authorize_remote_permission(request)
                .await
                .unwrap();
            assert_eq!(
                (
                    first,
                    next,
                    prompts_for_product,
                    other,
                    callbacks.remote_permission_calls.load(Ordering::SeqCst),
                ),
                (granted, granted, 1, granted, 2),
            );
        });
    }
}

#[test]
fn native_remote_authorization_rejects_closed_and_closing_executions() {
    for pending in [false, true] {
        let (reply, response) = futures::channel::oneshot::channel();
        let callbacks = Arc::new(EventCallbacks {
            remote_permission_reply: Mutex::new(Some(response)),
            ..EventCallbacks::new()
        });
        let host = NativeTrUApiHostRuntime::with_runtime_config(
            callbacks.clone(),
            native_host_runtime_config(),
        )
        .unwrap();
        let execution = host
            .open_product_execution(
                callbacks.clone(),
                None,
                None,
                None,
                native_execution_config("fetch.dot", ProductExecutionKind::App),
            )
            .unwrap();
        futures::executor::block_on(async {
            let request = execution.authorize_remote_permission(
                truapi::latest::RemotePermissionRequest {
                    permission: truapi::latest::RemotePermission::Remote {
                        domains: vec!["api.example.com".to_string()],
                    },
                },
            );
            futures::pin_mut!(request);
            if pending {
                assert!(futures::poll!(&mut request).is_pending());
            }
            execution.shutdown();
            reply.send(Ok(PermissionDecision::AllowOnce)).unwrap();
            assert_eq!(
                (
                    request.await.err().map(|error| error.to_string()),
                    callbacks.remote_permission_calls.load(Ordering::SeqCst),
                ),
                (
                    Some("product execution is closed".to_string()),
                    usize::from(pending)
                ),
            );
        });
    }
}

/// Drives the whole native chain for a status read: foreign callback,
/// `CallbackPlatform`, the per-execution connection adapters, and the
/// permission service. Nothing else covers that path, and the adapter is
/// per execution rather than per host runtime, so a host-level installer
/// would silently answer from the wrong object.
#[test]
fn a_native_status_read_follows_the_os_gate() {
    let host = NativeTrUApiHostRuntime::with_runtime_config(
        Arc::new(EventCallbacks::new()),
        native_host_runtime_config(),
    )
    .expect("host runtime config should be valid");
    let execution = host
        .open_product_execution(
            Arc::new(EventCallbacks::refusing(
                v01::HostDevicePermissionRequest::Camera,
            )),
            None,
            None,
            None,
            native_execution_config("gated.dot", ProductExecutionKind::App),
        )
        .expect("execution should open");

    let camera = futures::executor::block_on(execution.permission_authorization_status(
        PermissionAuthorizationRequest::Device(v01::HostDevicePermissionRequest::Camera),
    ))
    .expect("status read");
    let microphone = futures::executor::block_on(execution.permission_authorization_status(
        PermissionAuthorizationRequest::Device(v01::HostDevicePermissionRequest::Microphone),
    ))
    .expect("status read");

    // Camera is refused by the OS; the microphone has no OS gate and no
    // stored decision, so its question is still open.
    assert_eq!(
        (camera, microphone),
        (
            PermissionAuthorizationStatus::Denied,
            PermissionAuthorizationStatus::NotDetermined,
        )
    );
}

/// A face kept and read back is the same face, so a card draws at a cold
/// start exactly as its product last drew it.
#[test]
fn a_kept_face_reads_back_as_itself() {
    let json = std::fs::read_to_string(format!(
        "{}/tests/fixtures/pocket_faces/devicehood.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture");
    let face = parse_renderer_node_json(json).expect("fixture reads");

    let kept = encode_renderer_node(face.clone());

    assert_eq!(decode_renderer_node(kept).expect("reads back"), face);
}

/// A face a real product ships, kept here as well as in the iOS host so the
/// reader is measured against the protocol shape rather than against what
/// this code happens to accept. One is enough for that, and the hosts keep
/// the rest of the conformance set.
#[test]
fn reads_the_card_faces_the_hosts_conform_to() {
    let json = std::fs::read_to_string(format!(
        "{}/tests/fixtures/pocket_faces/devicehood.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture");

    let node = parse_renderer_node_json(json).expect("devicehood must read as a renderer tree");

    assert!(matches!(node, latest::RendererNode::Column { .. }));
}

/// `depth` boxes around an empty node, three brackets per box.
fn nested_boxes(depth: usize) -> String {
    let mut json = String::new();
    for _ in 0..depth {
        json.push_str(r#"{"tag":"Box","value":{"modifiers":[],"props":{},"children":["#);
    }
    json.push_str(r#"{"tag":"Nil"}"#);
    for _ in 0..depth {
        json.push_str("]}}");
    }
    json
}

/// A face deeper than the core will carry is refused rather than half-read,
/// so no host draws one it could not read back.
#[test]
fn refuses_a_face_deeper_than_the_core_carries() {
    assert!(parse_renderer_node_json(nested_boxes(MAX_FACE_DEPTH as usize - 1)).is_ok());
    assert!(matches!(
        parse_renderer_node_json(nested_boxes(MAX_FACE_DEPTH as usize + 1)),
        Err(NativeRendererError::TooDeep { .. })
    ));
}

/// A product chooses how deep its preview nests, and a host reads it before
/// the user approved anything, on whatever thread it happens to be on. The
/// deepest face the bracket bound lets through is refused on a thread with
/// less stack than any host gives one, rather than overflowing it and taking
/// the app down.
#[test]
fn a_face_at_the_nesting_bound_is_refused_on_a_small_host_stack() {
    let json = nested_boxes(MAX_FACE_JSON_NESTING as usize / 3);

    let read = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || parse_renderer_node_json(json))
        .expect("host thread starts")
        .join()
        .expect("host thread survives the read");

    assert!(matches!(read, Err(NativeRendererError::TooDeep { .. })));
}

/// A host that cannot start the reader's thread, short of memory or of
/// threads, gets an error it can show, not a panic, which a release build
/// turns into an abort of the whole app.
#[cfg(target_pointer_width = "64")]
#[test]
fn a_face_reader_that_cannot_start_is_an_error_not_a_crash() {
    let no_thread_has_this_much_stack = 1 << 47;

    assert!(matches!(
        read_face_on_stack(nested_boxes(1), no_thread_has_this_much_stack),
        Err(NativeRendererError::ReaderUnavailable { .. })
    ));
}

/// A kept face is read back only if it is exactly what was kept: bytes past
/// the tree mean the row is not a face this host wrote, and drawing its
/// prefix would show a card nobody drew.
#[test]
fn a_kept_face_with_bytes_past_its_tree_does_not_read_back() {
    let mut kept = encode_renderer_node(latest::RendererNode::Nil);
    kept.push(0);

    assert!(matches!(
        decode_renderer_node(kept),
        Err(NativeRendererError::Malformed { .. })
    ));
}

/// A title is drawn and an id is addressed, so they cannot share one rule:
/// the emoji below carries a variation selector, which an id may not.
#[test]
fn a_card_title_accepts_what_a_card_id_refuses() {
    assert_eq!(
        screen_pocket_card_title("\u{2615}\u{fe0f} Coffee".to_string()).unwrap(),
        "\u{2615}\u{fe0f} Coffee"
    );
    assert!(screen_pocket_card_id("\u{2615}\u{fe0f} Coffee".to_string()).is_err());
}

/// Both sides NFC-normalize, so a host that compares raw bytes against what
/// the core stored would miss a card it holds.
#[test]
fn screening_normalizes_and_trims() {
    assert_eq!(
        screen_pocket_card_id("  cafe\u{301}  ".to_string()).unwrap(),
        "caf\u{e9}"
    );
    assert!(screen_pocket_card_id("   ".to_string()).is_err());
}
