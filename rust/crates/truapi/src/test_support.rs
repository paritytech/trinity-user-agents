//! Shared fixtures for the runtime test modules: a stub platform, a
//! recording json-rpc connection, and SSO statement/frame builders.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::host_internal::sso_messages::{RemoteMessage, RemoteMessageData, v1};
use crate::host_logic::session::{SessionInfo, SsoSessionInfo};
use crate::host_logic::sso::pairing;
use crate::subscription::Spawner;
#[cfg(not(target_arch = "wasm32"))]
use crate::subscription::thread_per_subscription_spawner;

use crate::platform::{
    AccountAccessReview, AuthPresenter, AuthState, ChainProvider,
    CoreStorage as PlatformCoreStorage, CoreStorageKey, CreateTransactionReview,
    Features as PlatformFeatures, HostInfo, JsonRpcConnection, LocaleHost,
    Navigation as PlatformNavigation, Notifications as PlatformNotifications, PairingHostConfig,
    Permissions as PlatformPermissions, PlatformInfo, PreimageHost, ProductContext,
    ProductOperations as PlatformProductOperations, ProductStorage as PlatformProductStorage,
    ProductSubtreeReview, ProviderError, ResourceAllocationReview, SignPayloadReview,
    SignRawReview, SignVrfReview, StatementStoreProductSignReview, ThemeHost, UserConfirmation,
    UserConfirmationReview,
};
use futures::Stream;
use futures::stream::{self, BoxStream, StreamExt};
use parity_scale_codec::{Decode, Encode};
use schnorrkel::{ExpansionMode, MiniSecretKey};
use truapi::v01;
use truapi::versioned::account::{HostAccountCreateProofRequest, HostAccountGetAliasRequest};
use truapi::versioned::resource_allocation::HostRequestResourceAllocationRequest;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519SecretKey};

mod scripted_chain;

pub use scripted_chain::{ScriptedProvider, extract_id, notification_sender, wait_for_sent};

/// Block until `condition` holds, failing with `message` after two seconds.
/// Background runtime tasks run on their own threads, so a test that observes
/// their effects polls for them instead of assuming an ordering.
pub fn wait_until(mut condition: impl FnMut() -> bool, message: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !condition() {
        assert!(std::time::Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Test spawner that matches the current target.
pub fn test_spawner() -> Spawner {
    #[cfg(not(target_arch = "wasm32"))]
    {
        thread_per_subscription_spawner()
    }
    #[cfg(target_arch = "wasm32")]
    {
        immediate_spawner()
    }
}

/// Synchronous spawner for tests that should complete work immediately.
#[cfg(target_arch = "wasm32")]
pub fn immediate_spawner() -> Spawner {
    Arc::new(futures::executor::block_on)
}

/// Test hook invoked after each recorded auth state.
pub type AuthStateHook = Arc<dyn Fn(&AuthState) + Send + Sync>;
/// Test hook invoked after an auth-session write is recorded.
pub type StorageWriteHook = Arc<dyn Fn() + Send + Sync>;

/// Minimal Platform impl that only answers `feature_supported`. Every
/// other callback returns a unit value or empty stream, so the runtime
/// can exercise its delegation paths without pulling in a real backend.
#[derive(Default)]
pub struct StubPlatform {
    pub device_permission_decisions:
        Mutex<std::collections::VecDeque<crate::platform::PermissionDecision>>,
    pub device_permission_requests: Mutex<Vec<v01::HostDevicePermissionRequest>>,
    /// Product passed to each permission prompt, in order.
    pub permission_prompt_products: Mutex<Vec<ProductContext>>,
    pub remote_permission_denied: bool,
    pub remote_permission_decisions:
        Mutex<std::collections::VecDeque<crate::platform::PermissionDecision>>,
    /// Every `remote_permission` request, in order, so a test can assert which
    /// domains reached the prompt and that a stored grant suppresses a re-ask.
    pub remote_permission_requests: Arc<Mutex<Vec<v01::RemotePermissionRequest>>>,
    /// URLs handed to `navigate_to`. Empty means the gate blocked before the
    /// platform was ever reached.
    pub navigations: Arc<Mutex<Vec<String>>>,
    pub account_alias_confirmed: bool,
    pub account_alias_error: Option<&'static str>,
    pub create_proof_confirmed: bool,
    pub create_proof_error: Option<&'static str>,
    pub account_access_confirmed: bool,
    pub account_access_error: Option<&'static str>,
    pub account_access_reviews: Arc<Mutex<Vec<AccountAccessReview>>>,
    /// Permission answers retain their lifetime separately from action confirmations.
    pub permission_confirmation_decisions:
        Mutex<std::collections::VecDeque<crate::platform::PermissionDecision>>,
    /// Inverted so the derived default (`false`) approves, matching the
    /// pre-consent behavior where a cold own-account resolve was not gated.
    pub product_subtree_denied: bool,
    pub product_subtree_reviews: Arc<Mutex<Vec<ProductSubtreeReview>>>,
    pub identity_disclosure_confirmed: bool,
    pub identity_disclosure_error: Option<&'static str>,
    pub identity_disclosure_calls: Arc<AtomicUsize>,
    pub sign_payload_confirmed: bool,
    /// Every `SignPayload` review passed to `confirm_user_action`, in order.
    /// Empty proves an AutoSigning grant suppressed the prompt.
    pub sign_payload_reviews: Arc<Mutex<Vec<SignPayloadReview>>>,
    pub sign_payload_error: Option<&'static str>,
    pub sign_raw_confirmed: bool,
    pub sign_raw_error: Option<&'static str>,
    pub sign_raw_reviews: Arc<Mutex<Vec<SignRawReview>>>,
    pub sign_vrf_confirmed: bool,
    pub sign_vrf_error: Option<&'static str>,
    pub sign_vrf_reviews: Arc<Mutex<Vec<SignVrfReview>>>,
    /// Every `StatementStoreProductSign` review passed to `confirm_user_action`, in order.
    pub statement_store_product_sign_reviews: Arc<Mutex<Vec<StatementStoreProductSignReview>>>,
    pub create_transaction_confirmed: bool,
    /// Every `CreateTransaction` review passed to `confirm_user_action`, in
    /// order. Empty proves an AutoSigning grant suppressed the prompt.
    pub create_transaction_reviews: Arc<Mutex<Vec<CreateTransactionReview>>>,
    pub create_transaction_error: Option<&'static str>,
    /// Pause a transaction review until the test releases its confirmation.
    pub create_transaction_confirmation_gate:
        Mutex<Option<futures::channel::oneshot::Receiver<()>>>,
    pub resource_allocation_confirmed: bool,
    pub resource_allocation_error: Option<&'static str>,
    /// Pause a resource review until the test releases its confirmation.
    pub resource_allocation_confirmation_gate:
        Mutex<Option<futures::channel::oneshot::Receiver<()>>>,
    /// Every `ResourceAllocation` review passed to `confirm_user_action`, in order.
    pub resource_allocation_reviews: Arc<Mutex<Vec<ResourceAllocationReview>>>,
    pub session_blob: Option<Vec<u8>>,
    pub session_error: Option<&'static str>,
    pub session_clears: Arc<Mutex<usize>>,
    pub session_writes: Arc<Mutex<Vec<Vec<u8>>>>,
    pub on_auth_session_write: Arc<Mutex<Option<StorageWriteHook>>>,
    /// Every `auth_state_changed` emission in order.
    pub auth_states: Arc<Mutex<Vec<AuthState>>>,
    /// Invoked after each recorded auth state, outside any stub lock, so a
    /// test can react to a transition (e.g. cancel the login it observes).
    pub on_auth_state: Arc<Mutex<Option<AuthStateHook>>>,
    /// When true, `subscribe_theme` returns a never-ending stream.
    pub theme_stream_pending: bool,
    /// Set when the pending theme stream is dropped.
    pub theme_stream_dropped: Arc<AtomicBool>,
    pub pairing_success_response: bool,
    /// Deliver an authenticated wallet pending status and then stay open.
    pub pairing_pending_response: bool,
    /// Deliver a wallet failure status on the pairing subscription.
    pub pairing_failure_response: bool,
    /// Deliver the pairing success statement only through a snapshot
    /// query page; the live subscription stays silent.
    pub pairing_success_via_query: bool,
    /// Acknowledge the pairing subscription and then publish nothing, the
    /// shape a peer answering on a different SSO envelope presents: subscribed
    /// to the topic, with no statement this host can open ever arriving.
    pub pairing_silent_after_subscribe: bool,
    pub notification_id: u32,
    pub pushed_notifications: Arc<Mutex<Vec<v01::HostPushNotificationRequest>>>,
    pub cancelled_notifications: Arc<Mutex<Vec<u32>>>,
    pub sent_rpc: Arc<Mutex<Vec<String>>>,
    pub rpc_responses: Vec<String>,
    /// Responses keyed by JSON-RPC method, answered as each request arrives with
    /// that request's own id echoed back. A `state_call` is keyed by the runtime
    /// API it names instead, so metadata and view-function reads stay separable.
    ///
    /// Unlike `rpc_responses` this assumes nothing about request order and waits
    /// indefinitely for the next request, so a slow step between two requests
    /// cannot outrun the response pump. Prefer it whenever a test drives a path
    /// that decodes metadata or does other work between calls.
    pub rpc_method_responses: Vec<(&'static str, String)>,
    /// Hold the first connection's `rpc_method_responses` answers until the
    /// test releases them.
    pub rpc_method_responses_gate: Arc<Mutex<Option<futures::channel::oneshot::Receiver<()>>>>,
    pub sso_response_script: Option<SsoResponseScript>,
    /// Every genesis hash handed to `connect`, in order. Lets a test assert
    /// *which* chain a lookup reached, not merely that it reached one: the
    /// hashes a host is configured with are same-typed `[u8; 32]` passed
    /// positionally, so a transposed pair still connects and still answers.
    pub chain_connects: Arc<Mutex<Vec<[u8; 32]>>>,
    /// When set, `connect` fails with this reason.
    pub chain_connect_error: Option<&'static str>,
    /// When set, `connect` to this one chain fails, while every other chain
    /// still connects.
    pub unreachable_genesis: Option<[u8; 32]>,
    /// When true, the connection's response stream ends instead of staying
    /// pending. A follow opened over it then yields `None` rather than waiting
    /// out `OPERATION_TIMEOUT`, which is the difference between a test that
    /// asserts a lookup failed and a test that spends ten seconds proving it.
    pub chain_responses_end: bool,
    /// When true, `connect` stays pending forever.
    /// Hold every core-storage read pending forever, standing in for a host
    /// callback that is never answered.
    pub core_storage_pending: bool,
    pub chain_connect_pending: bool,
    /// Set when a `chain_connect_pending` connect future is dropped.
    pub pending_connect_dropped: Arc<AtomicBool>,
    /// Value returned by `lookup_preimage`, if any. Tests set this to a
    /// forged value to exercise the in-core integrity check.
    pub preimage_lookup_value: Option<Vec<u8>>,
    pub local_storage: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    /// Every product storage write that reached the platform, in order, so a
    /// test can see which writes the core skipped.
    pub local_storage_writes: Arc<Mutex<Vec<StorageWrite>>>,
    /// Open `subscribe_storage` streams by namespaced key; `write` and
    /// `clear` push each change to the matching ones.
    pub storage_subscribers: Arc<Mutex<Vec<(String, StorageChangeSender)>>>,
    /// Every `begin_operation` as `(product_id, label)`, in order. The
    /// returned id is the call's 1-based position.
    pub begun_operations: Arc<Mutex<Vec<(String, String)>>>,
    /// Every `end_operation` as `(product_id, id)`, in order.
    pub ended_operations: Arc<Mutex<Vec<(String, u32)>>>,
    /// Held open by a test so `begin_operation` is still in flight while the
    /// dispatch awaiting it goes away.
    pub begin_operation_gate: Mutex<Option<futures::channel::oneshot::Receiver<()>>>,
    /// When set, `end_operation` records the call and then fails with this
    /// reason, standing in for a host that drops the operation and still
    /// reports an error.
    pub end_operation_error: Option<&'static str>,
    /// When set, product/core storage reads fail with this reason.
    pub local_storage_error: Option<&'static str>,
    /// When set, only `PermissionAuthorization` reads fail. Narrower than
    /// `local_storage_error`, which fails every key: a test that needs the
    /// manifest cache to answer while the stored permission decision is
    /// unreadable cannot use the broad knob.
    pub permission_storage_error: Option<&'static str>,
}

/// One product storage write as the platform saw it: namespaced key and bytes.
pub type StorageWrite = (String, Vec<u8>);

/// Sender side of one stubbed storage subscription.
pub type StorageChangeSender = futures::channel::mpsc::UnboundedSender<
    Result<v01::HostLocalStorageChangeItem, v01::GenericError>,
>;

impl StubPlatform {
    /// Fail every open subscription on `key`, as a host whose store broke
    /// mid-stream would.
    pub fn fail_storage_subscriptions(&self, key: &str, reason: &str) {
        self.storage_subscribers
            .lock()
            .expect("storage subscribers mutex poisoned")
            .retain(|(subscribed, tx)| {
                subscribed != key
                    || tx
                        .unbounded_send(Err(v01::GenericError {
                            reason: reason.to_string(),
                        }))
                        .is_ok()
            });
    }

    fn push_storage_change(&self, key: &str, value: Option<Vec<u8>>) {
        self.storage_subscribers
            .lock()
            .expect("storage subscribers mutex poisoned")
            .retain(|(subscribed, tx)| {
                subscribed != key
                    || tx
                        .unbounded_send(Ok(v01::HostLocalStorageChangeItem {
                            value: value.clone(),
                        }))
                        .is_ok()
            });
    }
}

/// Scripted peer behavior for the recording connection's SSO exchange.
#[derive(Clone)]
pub enum SsoResponseScript {
    /// Peer acknowledges the request and replies with `response`.
    Success {
        session: SessionInfo,
        response: Box<RemoteMessage>,
    },
    /// Peer acknowledges the request and then sends `Disconnected`.
    PeerDisconnect { session: SessionInfo },
}

struct PendingThemeStream {
    dropped: Arc<AtomicBool>,
}

impl Drop for PendingThemeStream {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

impl Stream for PendingThemeStream {
    type Item = Result<v01::HostThemeSubscribeItem, v01::GenericError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}

/// First `Pairing` deeplink recorded on `auth_states`, if any.
pub fn first_pairing_deeplink(auth_states: &Mutex<Vec<AuthState>>) -> Option<String> {
    auth_states
        .lock()
        .expect("auth state list mutex poisoned")
        .iter()
        .find_map(|state| match state {
            AuthState::Pairing { deeplink } => Some(deeplink.clone()),
            _ => None,
        })
}

/// Default stub platform wrapped in an `Arc`.
pub fn stub_platform() -> Arc<StubPlatform> {
    Arc::new(StubPlatform::default())
}

/// Runtime configuration used by platform-backed runtime tests.
pub fn runtime_config(product_id: &str) -> (PairingHostConfig, ProductContext) {
    (
        PairingHostConfig::new(
            HostInfo {
                name: "Polkadot Web".to_string(),
                icon: Some("https://example.invalid/dotli.png".to_string()),
                version: None,
                platform: truapi::latest::HostPlatform::Web,
            },
            PlatformInfo::default(),
            [0; 32],
            [0xbb; 32],
            [0xcc; 32],
            "polkadotapp".to_string(),
        )
        .expect("test host runtime config is valid"),
        ProductContext::new(product_id.to_string()).expect("test product context is valid"),
    )
}

/// Basic connected session fixture without SSO channel material.
pub fn session_info() -> crate::host_logic::session::SessionInfo {
    crate::host_logic::session::SessionInfo {
        public_key: [
            0x80, 0x05, 0x28, 0xc9, 0x55, 0x87, 0x3e, 0x4c, 0x78, 0xb7, 0xdf, 0x24, 0xf7, 0x1d,
            0xb8, 0xf5, 0x81, 0xaa, 0x99, 0xe3, 0x49, 0x3b, 0xf4, 0x96, 0xed, 0xf1, 0x51, 0xab,
            0xc1, 0xd7, 0x20, 0x23,
        ],
        sso: None,
        root_entropy_source: Some([
            0x15, 0xcb, 0x94, 0x34, 0x84, 0x0b, 0x56, 0xbe, 0x1f, 0xdd, 0x91, 0xc4, 0x6a, 0x13,
            0xf5, 0x20, 0xf4, 0x91, 0x61, 0x2e, 0xa5, 0xd6, 0x06, 0x92, 0x0d, 0x91, 0x38, 0xe8,
            0xbd, 0xd6, 0x3c, 0xb0,
        ]),
        identity_account_id: Some([
            0x80, 0x05, 0x28, 0xc9, 0x55, 0x87, 0x3e, 0x4c, 0x78, 0xb7, 0xdf, 0x24, 0xf7, 0x1d,
            0xb8, 0xf5, 0x81, 0xaa, 0x99, 0xe3, 0x49, 0x3b, 0xf4, 0x96, 0xed, 0xf1, 0x51, 0xab,
            0xc1, 0xd7, 0x20, 0x23,
        ]),
        identity_chat_private_key: None,
        device_enc_public_key: None,
        lite_username: Some("alice".to_string()),
        full_username: Some("Alice Smith".to_string()),
    }
}

/// A pairing host and the signing host it paired with, each holding its own
/// side of one SSO session.
pub fn sso_host_and_responder_sessions() -> (SsoSessionInfo, SsoSessionInfo) {
    use crate::host_logic::sso::pairing::{
        ResponderIdentity, create_pairing_bootstrap, derive_x25519_keypair_from_entropy,
        establish_responder_session_info, establish_sso_session_info,
    };
    use crate::platform::{HostInfo, PairingHostConfig, PlatformInfo};

    let config = PairingHostConfig::new(
        HostInfo {
            name: "Test Host".to_string(),
            icon: None,
            version: None,
            platform: truapi::latest::HostPlatform::Unknown,
        },
        PlatformInfo::default(),
        [0; 32],
        [0xbb; 32],
        [0xcc; 32],
        "polkadotapp".to_string(),
    )
    .expect("test pairing config is valid");
    let bootstrap = create_pairing_bootstrap(&config).unwrap();
    let statement_keypair = MiniSecretKey::from_bytes(&[7; 32])
        .unwrap()
        .expand_to_keypair(ExpansionMode::Ed25519);
    let (encryption_secret_key, encryption_public_key) =
        derive_x25519_keypair_from_entropy(&[0xAB; 16], b"sso");
    let responder = ResponderIdentity {
        statement_secret: statement_keypair.secret.to_bytes(),
        statement_public_key: statement_keypair.public.to_bytes(),
        encryption_secret_key,
        encryption_public_key,
    };
    let responder_session = establish_responder_session_info(
        &responder,
        bootstrap.statement_store_public_key,
        bootstrap.encryption_public_key,
    )
    .unwrap();
    let host_session = establish_sso_session_info(
        &bootstrap,
        responder.statement_public_key,
        responder.encryption_public_key,
    )
    .unwrap();
    (host_session, responder_session)
}

/// Connected session fixture with deterministic SSO channel material.
pub fn sso_session_info() -> crate::host_logic::session::SessionInfo {
    let mut session = session_info();
    let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    let (_, peer_public_key) = peer_statement_keypair();
    let core_secret = X25519SecretKey::from([1; 32]);
    let peer_secret = X25519SecretKey::from([2; 32]);
    session.sso = Some(crate::host_logic::session::SsoSessionInfo {
        ss_secret: keypair.secret.to_bytes(),
        ss_public_key: keypair.public.to_bytes(),
        enc_secret: core_secret.to_bytes(),
        peer_enc_pubkey: X25519PublicKey::from(&peer_secret).to_bytes(),
        identity_account_id: peer_public_key,
        session_id_own: [4; 32],
        session_id_peer: [5; 32],
        request_channel: [6; 32],
        response_channel: [7; 32],
        peer_request_channel: [8; 32],
    });
    session.root_entropy_source = Some(keypair.secret.to_bytes()[..32].try_into().unwrap());
    session
}

/// Deterministic peer statement-store signing keypair.
pub fn peer_statement_keypair() -> ([u8; 64], [u8; 32]) {
    let mini_secret = MiniSecretKey::from_bytes(&[9; 32]).unwrap();
    let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
    (keypair.secret.to_bytes(), keypair.public.to_bytes())
}

/// SCALE-encoded statement signed by the deterministic peer keypair.
pub fn signed_test_statement(data: Vec<u8>) -> Vec<u8> {
    let (secret, public) = peer_statement_keypair();
    crate::host_logic::statement_store::sign_statement_fields(
        secret,
        public,
        vec![crate::host_logic::statement_store::StatementField::Data(
            data,
        )],
    )
    .unwrap()
    .encode()
}

/// Last submitted SSO remote message decoded from the stub RPC log.
pub fn submitted_remote_message(
    platform: &Arc<StubPlatform>,
    session: &SessionInfo,
) -> RemoteMessage {
    let submit = wait_for_statement_submit(&platform.sent_rpc);
    let (_, message) = submitted_sso_request_from_submit(&submit, session);
    message
}

/// Every SSO message this host has published, oldest first.
pub fn submitted_remote_messages(
    platform: &Arc<StubPlatform>,
    session: &SessionInfo,
) -> Vec<RemoteMessage> {
    platform
        .sent_rpc
        .lock()
        .expect("rpc list mutex poisoned")
        .iter()
        .filter(|request| request.contains("\"statement_submit\""))
        .map(|submit| submitted_sso_request_from_submit(submit, session).1)
        .collect()
}

fn submitted_sso_request_from_submit(
    submit: &str,
    session: &SessionInfo,
) -> (String, RemoteMessage) {
    let value: serde_json::Value = serde_json::from_str(submit).unwrap();
    let statement_hex = value["params"][0].as_str().unwrap();
    let statement = hex::decode(statement_hex.strip_prefix("0x").unwrap_or(statement_hex)).unwrap();
    let encrypted = crate::host_logic::statement_store::decode_statement_data(&statement)
        .expect("statement data should decode");
    let data = pairing::decrypt_session_statement_data(session.sso.as_ref().unwrap(), &encrypted)
        .expect("statement data should decrypt");
    let pairing::SsoStatementData::Request { request_id, data } = data else {
        panic!("expected request statement data");
    };
    let message =
        RemoteMessage::decode(&mut data[0].as_slice()).expect("remote message should decode");
    (request_id, message)
}

fn submitted_sso_request(
    sent: &Arc<Mutex<Vec<String>>>,
    session: &SessionInfo,
) -> (String, RemoteMessage) {
    let submit = wait_for_statement_submit(sent);
    submitted_sso_request_from_submit(&submit, session)
}

fn wait_for_statement_submit(sent: &Arc<Mutex<Vec<String>>>) -> String {
    for _ in 0..100 {
        if let Some(request) = sent
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .rev()
            .find(|request| request.contains("\"statement_submit\""))
            .cloned()
        {
            return request;
        }
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::sleep(Duration::from_millis(1));
        #[cfg(target_arch = "wasm32")]
        futures::executor::block_on(futures_timer::Delay::new(Duration::from_millis(1)));
    }
    panic!("statement_submit request should be sent");
}

/// JSON-RPC response sequence for a successful SSO request/response exchange.
pub fn sso_success_responses(
    session: &SessionInfo,
    message_id: &str,
    response: RemoteMessage,
) -> Vec<String> {
    let own_subscription_id = format!("own-sub-{message_id}");
    let peer_subscription_id = format!("peer-sub-{message_id}");
    vec![
        subscribe_ack_frame("truapi:1", &own_subscription_id),
        subscribe_ack_frame("truapi:2", &peer_subscription_id),
        statement_submit_ack_frame("truapi:3"),
        new_statements_frame(
            &own_subscription_id,
            vec![sso_statement(
                session,
                pairing::SsoStatementData::Response {
                    request_id: message_id.to_string(),
                    response_code: 0,
                },
                1,
            )],
        ),
        new_statements_frame(
            &peer_subscription_id,
            vec![sso_statement(
                session,
                pairing::SsoStatementData::Request {
                    request_id: format!("wallet-response-{message_id}"),
                    data: vec![response.encode()],
                },
                2,
            )],
        ),
    ]
}

/// Dynamic JSON-RPC response script for a successful SSO request/response exchange.
pub fn sso_success_response_script(
    session: &SessionInfo,
    response: RemoteMessage,
) -> SsoResponseScript {
    SsoResponseScript::Success {
        session: session.clone(),
        response: Box::new(response),
    }
}

/// Dynamic JSON-RPC response script where the SSO peer sends `Disconnected`.
pub fn sso_peer_disconnect_response_script(session: &SessionInfo) -> SsoResponseScript {
    SsoResponseScript::PeerDisconnect {
        session: session.clone(),
    }
}

/// JSON-RPC response sequence for the background peer-disconnect monitor.
pub fn sso_peer_disconnect_monitor_responses(
    session: &crate::host_logic::session::SessionInfo,
) -> Vec<String> {
    let subscription_id = "peer-disconnect-monitor-sub";
    vec![
        subscribe_ack_frame("truapi:1", subscription_id),
        new_statements_frame(
            subscription_id,
            vec![sso_statement(
                session,
                pairing::SsoStatementData::Request {
                    request_id: "wallet-disconnect-monitor".to_string(),
                    data: vec![
                        crate::host_internal::sso_messages::RemoteMessage {
                            message_id: "wallet-disconnect-monitor".to_string(),
                            data: crate::host_internal::sso_messages::RemoteMessageData::V1(
                                crate::host_internal::sso_messages::v1::RemoteMessage::Disconnected,
                            ),
                        }
                        .encode(),
                    ],
                },
                1,
            )],
        ),
    ]
}

/// JSON-RPC subscription acknowledgement frame.
pub fn subscribe_ack_frame(request_id: &str, subscription_id: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": subscription_id,
    })
    .to_string()
}

fn statement_submit_ack_frame(request_id: &str) -> String {
    // Mirror the real `statement_submit` result shape (`SubmitResult`); `submit`
    // treats only `new`/`known` as accepted.
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": { "status": "new" },
    })
    .to_string()
}

/// JSON-RPC `newStatements` notification carrying SCALE statements.
pub fn new_statements_frame(subscription_id: &str, statements: Vec<Vec<u8>>) -> String {
    let statements = statements
        .into_iter()
        .map(|statement| format!("0x{}", hex::encode(statement)))
        .collect::<Vec<_>>();
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "statement_subscribeStatement",
        "params": {
            "subscription": subscription_id,
            "result": {
                "event": "newStatements",
                "data": {
                    "statements": statements,
                    "remaining": 0,
                },
            },
        },
    })
    .to_string()
}

fn sso_statement(
    session: &crate::host_logic::session::SessionInfo,
    data: pairing::SsoStatementData,
    nonce_seed: u8,
) -> Vec<u8> {
    let mut nonce = [0; pairing::AEAD_NONCE_LEN];
    nonce[0] = nonce_seed;
    let encrypted = pairing::encrypt_session_statement_data_with_nonce(
        session.sso.as_ref().unwrap(),
        &data,
        nonce,
    )
    .unwrap();
    signed_test_statement(encrypted)
}

fn core_encryption_public_key_from_deeplink(deeplink: &str) -> [u8; 32] {
    pairing_device_from_deeplink(deeplink).1
}

/// Pairing device statement and encryption keys encoded in a deeplink.
pub fn pairing_device_from_deeplink(deeplink: &str) -> ([u8; 32], [u8; 32]) {
    let encoded = deeplink
        .split("handshake=")
        .nth(1)
        .expect("pairing deeplink should include handshake");
    let handshake = hex::decode(encoded).expect("handshake should be hex");
    let decoded = pairing::VersionedHandshakeProposal::decode(&mut handshake.as_slice())
        .expect("handshake should decode");
    let pairing::VersionedHandshakeProposal::V2(proposal) = decoded;
    (
        proposal.device.statement_account_id,
        proposal.device.encryption_public_key,
    )
}

/// Signed wallet handshake statement answering `deeplink` with pairing success.
pub fn wallet_handshake_statement(deeplink: &str) -> Vec<u8> {
    wallet_handshake_statement_with_response(
        deeplink,
        pairing::v2::EncryptedResponse::Success(Box::new(wallet_handshake_success())),
        0x44,
    )
}

fn pending_wallet_handshake_statement(deeplink: &str) -> Vec<u8> {
    wallet_handshake_statement_with_response(
        deeplink,
        pairing::v2::EncryptedResponse::Pending(pairing::v2::Status::AllowanceAllocation),
        0x43,
    )
}

/// Signed wallet handshake statement answering `deeplink` with failure `reason`.
pub fn failed_wallet_handshake_statement(deeplink: &str, reason: &str) -> Vec<u8> {
    wallet_handshake_statement_with_response(
        deeplink,
        pairing::v2::EncryptedResponse::Failed(reason.to_string()),
        0x45,
    )
}

/// Wallet device X25519 public key distinct from the persistent SSO key, so
/// tests catch a session that keys device-scoped material off the SSO channel
/// key by mistake.
pub fn wallet_device_encryption_public_key() -> [u8; 32] {
    pairing::x25519_public_key([9; 32])
}

fn wallet_handshake_success() -> pairing::v2::Success {
    let wallet_persistent_secret = X25519SecretKey::from([2; 32]);
    let wallet_persistent_public = X25519PublicKey::from(&wallet_persistent_secret).to_bytes();
    pairing::v2::Success {
        identity_account_id: peer_statement_keypair().1,
        root_account_id: session_info().public_key,
        identity_chat_private_key: [0x77; 32],
        sso_enc_pub_key: wallet_persistent_public,
        device_enc_pub_key: wallet_device_encryption_public_key(),
        root_entropy_source: [0x66; 32],
    }
}

fn wallet_handshake_statement_with_response(
    deeplink: &str,
    answer: pairing::v2::EncryptedResponse,
    _nonce_seed: u8,
) -> Vec<u8> {
    let core_public_key = core_encryption_public_key_from_deeplink(deeplink);
    let handshake = pairing::encrypt_v2_handshake_response(core_public_key, &answer)
        .expect("wallet handshake response should encrypt");

    signed_test_statement(handshake.encode())
}

/// SSO signing response message for the given request id.
pub fn sign_response_message(
    message_id: &str,
    signature: Vec<u8>,
    signed_transaction: Option<Vec<u8>>,
) -> crate::host_internal::sso_messages::RemoteMessage {
    crate::host_internal::sso_messages::RemoteMessage {
        message_id: format!("wallet-{message_id}"),
        data: crate::host_internal::sso_messages::RemoteMessageData::V1(
            crate::host_internal::sso_messages::v1::RemoteMessage::SignResponse(
                crate::host_internal::sso_messages::Response {
                    responding_to: message_id.to_string(),
                    payload: Ok(truapi::latest::HostSignPayloadResponse {
                        signature,
                        signed_transaction,
                    }),
                },
            ),
        ),
    }
}

/// SSO legacy-account raw signing response for the given request id.
pub fn sign_raw_legacy_response_message(
    message_id: &str,
    signature: Vec<u8>,
) -> crate::host_internal::sso_messages::RemoteMessage {
    crate::host_internal::sso_messages::RemoteMessage {
        message_id: format!("wallet-{message_id}"),
        data: crate::host_internal::sso_messages::RemoteMessageData::V1(
            crate::host_internal::sso_messages::v1::RemoteMessage::SignRawWithLegacyAccountResponse(
                crate::host_internal::sso_messages::Response {
                    responding_to: message_id.to_string(),
                    payload: Ok(signature),
                },
            ),
        ),
    }
}

/// Product account id fixture for `identifier`; the derivation suffix is the
/// canonical decimal form of `index`.
pub fn account_id(identifier: &str, index: u32) -> v01::ProductAccountId {
    v01::ProductAccountId {
        dot_ns_identifier: identifier.to_string(),
        derivation_index: v01::DerivationIndex::Index(index),
    }
}

/// Raw signing payload fixture.
pub fn raw_payload() -> v01::RawPayload {
    v01::RawPayload::Bytes {
        bytes: b"hello".to_vec(),
    }
}

/// Product-scoped proof context fixture for `product_id`.
pub fn product_proof_context(product_id: &str) -> v01::ProductProofContext {
    v01::ProductProofContext {
        product_id: product_id.to_string(),
        suffix: v01::DerivationIndex::Index(7),
    }
}

/// Ring-location fixture addressing a single pallet-instance ring.
pub fn ring_location_fixture() -> v01::RingLocation {
    v01::RingLocation {
        chain_id: [1; 32],
        junctions: vec![v01::RingLocationJunction::PalletInstance(42)],
    }
}

/// Contextual-alias request fixture for `product_id`.
pub fn account_alias_request(product_id: &str) -> HostAccountGetAliasRequest {
    HostAccountGetAliasRequest::V1(v01::HostAccountGetAliasRequest {
        key_handle: v01::ProductAccountId {
            dot_ns_identifier: product_id.to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
        context: product_proof_context(product_id),
        ring_location: ring_location_fixture(),
    })
}

/// Ring-VRF proof request fixture for `product_id`.
pub fn create_proof_request(product_id: &str) -> HostAccountCreateProofRequest {
    HostAccountCreateProofRequest::V1(v01::HostAccountCreateProofRequest {
        key_handle: v01::ProductAccountId {
            dot_ns_identifier: product_id.to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
        context: product_proof_context(product_id),
        ring_location: ring_location_fixture(),
        message: vec![4, 5, 6],
    })
}

/// Structured signing payload fixture.
pub fn sign_payload_data() -> v01::HostSignPayloadData {
    v01::HostSignPayloadData {
        block_hash: vec![0; 32],
        block_number: vec![0; 4],
        era: vec![0],
        genesis_hash: vec![1; 32],
        method: vec![0],
        nonce: vec![0],
        spec_version: vec![0],
        tip: vec![0],
        transaction_version: vec![0],
        signed_extensions: vec![],
        version: 4,
        asset_id: None,
        metadata_hash: None,
        mode: None,
        with_signed_transaction: parity_scale_codec::OptionBool(None),
    }
}

/// Product transaction payload fixture for `identifier`.
pub fn product_tx_payload(identifier: &str) -> v01::ProductAccountTxPayload {
    v01::ProductAccountTxPayload {
        signer: account_id(identifier, 0),
        genesis_hash: [1; 32],
        call_data: vec![0],
        extensions: vec![],
        tx_ext_version: 0,
        contacts: Vec::new(),
    }
}

/// Resource-allocation request fixture containing all supported resource kinds.
pub fn resource_allocation_request() -> HostRequestResourceAllocationRequest {
    HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
        resources: vec![
            v01::AllocatableResource::StatementStoreAllowance,
            v01::AllocatableResource::AutoSigning,
        ],
    })
}

/// Unsigned statement fixture with channel, topics, expiry, and data.
pub fn statement() -> v01::Statement {
    v01::Statement {
        proof: None,
        decryption_key: None,
        expiry: Some(99),
        channel: Some([1; 32]),
        topics: vec![[2; 32], [3; 32]],
        data: Some(vec![4, 5, 6]),
    }
}

/// Signed statement fixture scoped to `topic`.
pub fn signed_statement(topic: [u8; 32]) -> v01::SignedStatement {
    v01::SignedStatement {
        proof: v01::StatementProof::Sr25519 {
            signature: [9; 64],
            signer: [8; 32],
        },
        decryption_key: None,
        expiry: Some(99),
        channel: Some([1; 32]),
        topics: vec![topic],
        data: Some(vec![4, 5, 6]),
    }
}

#[crate::platform::async_trait]
impl PlatformProductStorage for StubPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        if let Some(reason) = self.local_storage_error {
            return Err(v01::HostLocalStorageReadError::Unknown {
                reason: reason.to_string(),
            });
        }
        Ok(self
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .get(&key)
            .cloned())
    }
    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        if let Some(reason) = self.local_storage_error {
            return Err(v01::HostLocalStorageReadError::Unknown {
                reason: reason.to_string(),
            });
        }
        self.local_storage_writes
            .lock()
            .expect("local storage writes mutex poisoned")
            .push((key.clone(), value.clone()));
        self.local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .insert(key.clone(), value.clone());
        self.push_storage_change(&key, Some(value));
        Ok(())
    }
    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        if let Some(reason) = self.local_storage_error {
            return Err(v01::HostLocalStorageReadError::Unknown {
                reason: reason.to_string(),
            });
        }
        self.local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .remove(&key);
        self.push_storage_change(&key, None);
        Ok(())
    }

    fn subscribe_storage(
        &self,
        key: String,
    ) -> BoxStream<'static, Result<v01::HostLocalStorageChangeItem, v01::GenericError>> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        self.storage_subscribers
            .lock()
            .expect("storage subscribers mutex poisoned")
            .push((key.clone(), tx));
        let value = self
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .get(&key)
            .cloned();
        Box::pin(
            stream::once(async move { Ok(v01::HostLocalStorageChangeItem { value }) }).chain(rx),
        )
    }
}

#[crate::platform::async_trait]
impl PlatformProductOperations for StubPlatform {
    async fn begin_operation(
        &self,
        product: &ProductContext,
        label: String,
    ) -> Result<v01::HostWorkerBeginOperationResponse, v01::HostWorkerOperationError> {
        let gate = self
            .begin_operation_gate
            .lock()
            .expect("begin operation gate mutex poisoned")
            .take();
        if let Some(gate) = gate {
            let _ = gate.await;
        }
        // The stub has no worker lifecycle to keep alive; it only records the
        // call and hands back its position as the id.
        let id = {
            let mut begun = self
                .begun_operations
                .lock()
                .expect("begun operations mutex poisoned");
            begun.push((product.product_id.clone(), label));
            begun.len() as u32
        };
        // The operation exists host-side from here on, so a test holding the
        // gate keeps only the answer in flight.
        let gate = self
            .begin_operation_gate
            .lock()
            .expect("begin operation gate mutex poisoned")
            .take();
        if let Some(gate) = gate {
            let _ = gate.await;
        }
        Ok(v01::HostWorkerBeginOperationResponse { id })
    }

    async fn end_operation(
        &self,
        product: &ProductContext,
        id: u32,
    ) -> Result<(), v01::HostWorkerOperationError> {
        self.ended_operations
            .lock()
            .expect("ended operations mutex poisoned")
            .push((product.product_id.clone(), id));
        match self.end_operation_error {
            Some(reason) => Err(v01::HostWorkerOperationError::Unknown {
                reason: reason.to_string(),
            }),
            None => Ok(()),
        }
    }
}

#[crate::platform::async_trait]
impl PlatformCoreStorage for StubPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        if self.core_storage_pending {
            futures::future::pending::<()>().await;
        }
        if let CoreStorageKey::AuthSession = key {
            if let Some(reason) = self.session_error {
                return Err(v01::GenericError {
                    reason: reason.to_string(),
                });
            }
            return Ok(self.session_blob.clone());
        }
        if let Some(reason) = self.local_storage_error {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        if let (CoreStorageKey::PermissionAuthorization { .. }, Some(reason)) =
            (&key, self.permission_storage_error)
        {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        Ok(self
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .get(&core_storage_test_key(key))
            .cloned())
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        if let CoreStorageKey::AuthSession = key {
            self.session_writes
                .lock()
                .expect("session write list mutex poisoned")
                .push(value);
            let hook = self
                .on_auth_session_write
                .lock()
                .expect("auth session write hook mutex poisoned")
                .clone();
            if let Some(hook) = hook {
                hook();
            }
            return Ok(());
        }
        if let Some(reason) = self.local_storage_error {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        self.local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .insert(core_storage_test_key(key), value);
        Ok(())
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        if let CoreStorageKey::AuthSession = key {
            *self
                .session_clears
                .lock()
                .expect("session clear counter mutex poisoned") += 1;
            return Ok(());
        }
        if let Some(reason) = self.local_storage_error {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        self.local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .remove(&core_storage_test_key(key));
        Ok(())
    }
}

/// Stable string key used by the stub core-storage map.
pub fn core_storage_test_key(key: CoreStorageKey) -> String {
    format!("core:{}", hex::encode(key.encode()))
}

#[crate::platform::async_trait]
impl PlatformNavigation for StubPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        self.navigations
            .lock()
            .expect("navigation list mutex poisoned")
            .push(url);
        Ok(())
    }
}

#[crate::platform::async_trait]
impl PlatformNotifications for StubPlatform {
    async fn push_notification(
        &self,
        notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        self.pushed_notifications
            .lock()
            .expect("notification list mutex poisoned")
            .push(notification);
        Ok(v01::HostPushNotificationResponse {
            id: self.notification_id,
        })
    }

    async fn cancel_notification(&self, id: u32) -> Result<(), v01::GenericError> {
        self.cancelled_notifications
            .lock()
            .expect("notification cancellation list mutex poisoned")
            .push(id);
        Ok(())
    }
}

#[crate::platform::async_trait]
impl PlatformPermissions for StubPlatform {
    async fn device_permission(
        &self,
        product: &ProductContext,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<crate::platform::PermissionDecision, v01::GenericError> {
        self.permission_prompt_products
            .lock()
            .expect("permission prompt products mutex poisoned")
            .push(product.clone());
        self.device_permission_requests
            .lock()
            .expect("device permission list mutex poisoned")
            .push(request);
        Ok(self
            .device_permission_decisions
            .lock()
            .expect("device permission decisions mutex poisoned")
            .pop_front()
            .unwrap_or(crate::platform::PermissionDecision::AllowAlways))
    }

    async fn remote_permission(
        &self,
        product: &ProductContext,
        request: v01::RemotePermissionRequest,
    ) -> Result<crate::platform::PermissionDecision, v01::GenericError> {
        self.permission_prompt_products
            .lock()
            .expect("permission prompt products mutex poisoned")
            .push(product.clone());
        self.remote_permission_requests
            .lock()
            .expect("remote permission list mutex poisoned")
            .push(request);
        if let Some(decision) = self
            .remote_permission_decisions
            .lock()
            .expect("remote permission decisions mutex poisoned")
            .pop_front()
        {
            return Ok(decision);
        }
        Ok(if self.remote_permission_denied {
            crate::platform::PermissionDecision::Deny
        } else {
            crate::platform::PermissionDecision::AllowAlways
        })
    }
}

#[crate::platform::async_trait]
impl PlatformFeatures for StubPlatform {
    async fn feature_supported(
        &self,
        _request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        Ok(v01::HostFeatureSupportedResponse { supported: true })
    }

    async fn supported_chains(&self) -> Result<crate::platform::HostChainSet, v01::GenericError> {
        Ok(crate::platform::HostChainSet {
            network: "paseo".to_string(),
            chains: vec![
                crate::platform::HostChainEntry {
                    identifier: v01::ChainIdentifier::AssetHub,
                    genesis_hash: [0xaa; 32],
                },
                crate::platform::HostChainEntry {
                    identifier: v01::ChainIdentifier::People,
                    genesis_hash: [0x22; 32],
                },
            ],
        })
    }
}

struct RecordingConnection {
    sent: Arc<Mutex<Vec<String>>>,
    responses: Vec<String>,
    method_responses: Vec<(&'static str, String)>,
    method_responses_gate: Arc<Mutex<Option<futures::channel::oneshot::Receiver<()>>>>,
    sso_response_script: Option<SsoResponseScript>,
    auth_states: Arc<Mutex<Vec<AuthState>>>,
    pairing_success_response: bool,
    pairing_pending_response: bool,
    pairing_failure_response: bool,
    pairing_success_via_query: bool,
    pairing_silent_after_subscribe: bool,
    chain_responses_end: bool,
}

async fn wait_for_statement_subscribe_id(sent: Arc<Mutex<Vec<String>>>, index: usize) -> String {
    wait_for_rpc_method_id(sent, "statement_subscribeStatement", index).await
}

async fn wait_for_rpc_method_id(
    sent: Arc<Mutex<Vec<String>>>,
    method: &str,
    index: usize,
) -> String {
    for _ in 0..100 {
        let ids = sent
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .filter_map(|request| {
                let value: serde_json::Value = serde_json::from_str(request).ok()?;
                (value.get("method")?.as_str()? == method)
                    .then(|| value.get("id")?.as_str().map(ToString::to_string))?
            })
            .collect::<Vec<_>>();
        if let Some(id) = ids.get(index) {
            return id.clone();
        }
        futures_timer::Delay::new(Duration::from_millis(1)).await;
    }
    panic!("{method} request {index} was not issued");
}

fn retarget_sso_response(mut response: RemoteMessage, message_id: &str) -> RemoteMessage {
    response.message_id = format!("wallet-{message_id}");
    let RemoteMessageData::V1(data) = response.data;
    response.data = RemoteMessageData::V1(data.with_responding_to(message_id.to_string()));
    response
}

fn sso_scripted_responses(
    sent: Arc<Mutex<Vec<String>>>,
    script: SsoResponseScript,
) -> BoxStream<'static, String> {
    Box::pin(stream::unfold(0, move |state| {
        let sent = sent.clone();
        let script = script.clone();
        async move {
            match state {
                0 => {
                    let id = wait_for_statement_subscribe_id(sent.clone(), 0).await;
                    Some((subscribe_ack_frame(&id, "own-sub"), 1))
                }
                1 => {
                    let id = wait_for_statement_subscribe_id(sent.clone(), 1).await;
                    Some((subscribe_ack_frame(&id, "peer-sub"), 2))
                }
                2 => {
                    let id = wait_for_rpc_method_id(sent.clone(), "statement_submit", 0).await;
                    Some((statement_submit_ack_frame(&id), 3))
                }
                3 => match &script {
                    SsoResponseScript::Success { session, .. }
                    | SsoResponseScript::PeerDisconnect { session } => {
                        let (statement_request_id, _) = submitted_sso_request(&sent, session);
                        Some((
                            new_statements_frame(
                                "own-sub",
                                vec![sso_statement(
                                    session,
                                    pairing::SsoStatementData::Response {
                                        request_id: statement_request_id,
                                        response_code: 0,
                                    },
                                    1,
                                )],
                            ),
                            4,
                        ))
                    }
                },
                4 => match script {
                    SsoResponseScript::Success { session, response } => {
                        let (_, request) = submitted_sso_request(&sent, &session);
                        let response = retarget_sso_response(*response, &request.message_id);
                        Some((
                            new_statements_frame(
                                "peer-sub",
                                vec![sso_statement(
                                    &session,
                                    pairing::SsoStatementData::Request {
                                        request_id: format!(
                                            "wallet-response-{}",
                                            request.message_id
                                        ),
                                        data: vec![response.encode()],
                                    },
                                    2,
                                )],
                            ),
                            5,
                        ))
                    }
                    SsoResponseScript::PeerDisconnect { session } => {
                        let (_, request) = submitted_sso_request(&sent, &session);
                        let message_id = format!("wallet-disconnect-{}", request.message_id);
                        Some((
                            new_statements_frame(
                                "peer-sub",
                                vec![sso_statement(
                                    &session,
                                    pairing::SsoStatementData::Request {
                                        request_id: message_id.clone(),
                                        data: vec![
                                            RemoteMessage {
                                                message_id,
                                                data: RemoteMessageData::V1(
                                                    v1::RemoteMessage::Disconnected,
                                                ),
                                            }
                                            .encode(),
                                        ],
                                    },
                                    2,
                                )],
                            ),
                            5,
                        ))
                    }
                },
                _ => futures::future::pending().await,
            }
        }
    }))
}

impl JsonRpcConnection for RecordingConnection {
    fn send(&self, request: String) {
        self.sent
            .lock()
            .expect("rpc list mutex poisoned")
            .push(request);
    }
    fn responses(&self) -> BoxStream<'static, String> {
        if self.pairing_silent_after_subscribe {
            let sent = self.sent.clone();
            return Box::pin(stream::unfold(0, move |state| {
                let sent = sent.clone();
                async move {
                    match state {
                        0 => {
                            let id = wait_for_statement_subscribe_id(sent.clone(), 0).await;
                            Some((subscribe_ack_frame(&id, "pairing-sub"), 1))
                        }
                        _ => futures::future::pending().await,
                    }
                }
            }));
        }
        if self.pairing_success_via_query {
            let auth_states = self.auth_states.clone();
            let sent = self.sent.clone();
            return Box::pin(stream::unfold(0, move |state| {
                let auth_states = auth_states.clone();
                let sent = sent.clone();
                async move {
                    match state {
                        0 => {
                            let id = wait_for_statement_subscribe_id(sent.clone(), 0).await;
                            Some((subscribe_ack_frame(&id, "pairing-sub"), 1))
                        }
                        1 => {
                            let query_id = wait_for_statement_subscribe_id(sent.clone(), 1).await;
                            Some((subscribe_ack_frame(&query_id, "query-sub"), 2))
                        }
                        2 => {
                            for _ in 0..100 {
                                if let Some(deeplink) = first_pairing_deeplink(&auth_states) {
                                    return Some((
                                        new_statements_frame(
                                            "query-sub",
                                            vec![wallet_handshake_statement(&deeplink)],
                                        ),
                                        3,
                                    ));
                                }
                                futures_timer::Delay::new(Duration::from_millis(1)).await;
                            }
                            panic!("pairing deeplink was not presented");
                        }
                        _ => futures::future::pending().await,
                    }
                }
            }));
        }
        if self.pairing_failure_response {
            let auth_states = self.auth_states.clone();
            let sent = self.sent.clone();
            return Box::pin(stream::unfold(0, move |state| {
                let auth_states = auth_states.clone();
                let sent = sent.clone();
                async move {
                    match state {
                        0 => {
                            let id = wait_for_statement_subscribe_id(sent.clone(), 0).await;
                            Some((subscribe_ack_frame(&id, "pairing-sub"), 1))
                        }
                        1 => {
                            for _ in 0..100 {
                                if let Some(deeplink) = first_pairing_deeplink(&auth_states) {
                                    return Some((
                                        new_statements_frame(
                                            "pairing-sub",
                                            vec![failed_wallet_handshake_statement(
                                                &deeplink,
                                                "The operation couldn't be completed. (SubstrateSdk.JSONRPCError error 1.)",
                                            )],
                                        ),
                                        2,
                                    ));
                                }
                                futures_timer::Delay::new(Duration::from_millis(1)).await;
                            }
                            panic!("pairing deeplink was not presented");
                        }
                        _ => futures::future::pending().await,
                    }
                }
            }));
        }
        if self.pairing_pending_response {
            let auth_states = self.auth_states.clone();
            let sent = self.sent.clone();
            return Box::pin(stream::unfold(0, move |state| {
                let auth_states = auth_states.clone();
                let sent = sent.clone();
                async move {
                    match state {
                        0 => {
                            let id = wait_for_statement_subscribe_id(sent.clone(), 0).await;
                            Some((subscribe_ack_frame(&id, "pairing-sub"), 1))
                        }
                        1 => {
                            for _ in 0..100 {
                                if let Some(deeplink) = first_pairing_deeplink(&auth_states) {
                                    return Some((
                                        new_statements_frame(
                                            "pairing-sub",
                                            vec![pending_wallet_handshake_statement(&deeplink)],
                                        ),
                                        2,
                                    ));
                                }
                                futures_timer::Delay::new(Duration::from_millis(1)).await;
                            }
                            panic!("pairing deeplink was not presented");
                        }
                        _ => futures::future::pending().await,
                    }
                }
            }));
        }
        if self.pairing_success_response {
            let auth_states = self.auth_states.clone();
            let sent = self.sent.clone();
            return Box::pin(stream::unfold(0, move |state| {
                let auth_states = auth_states.clone();
                let sent = sent.clone();
                async move {
                    match state {
                        0 => {
                            let id = wait_for_statement_subscribe_id(sent.clone(), 0).await;
                            Some((subscribe_ack_frame(&id, "pairing-sub"), 1))
                        }
                        1 => {
                            for _ in 0..100 {
                                if let Some(deeplink) = first_pairing_deeplink(&auth_states) {
                                    return Some((
                                        new_statements_frame(
                                            "pairing-sub",
                                            vec![wallet_handshake_statement(&deeplink)],
                                        ),
                                        2,
                                    ));
                                }
                                futures_timer::Delay::new(Duration::from_millis(1)).await;
                            }
                            panic!("pairing deeplink was not presented");
                        }
                        _ => futures::future::pending().await,
                    }
                }
            }));
        }
        if let Some(script) = self.sso_response_script.clone() {
            return sso_scripted_responses(self.sent.clone(), script);
        }
        if !self.method_responses.is_empty() {
            let answers = method_keyed_responses(self.sent.clone(), self.method_responses.clone());
            let gate = self
                .method_responses_gate
                .lock()
                .expect("method responses gate mutex poisoned")
                .take();
            return match gate {
                Some(gate) => Box::pin(
                    stream::once(async move {
                        gate.await.expect("method responses gate was released");
                        answers
                    })
                    .flatten(),
                ),
                None => answers,
            };
        }
        if self.responses.is_empty() {
            if self.chain_responses_end {
                // Ending immediately tears the connection down before the
                // request is recorded.
                let sent = self.sent.clone();
                return Box::pin(stream::unfold(sent, move |sent| async move {
                    for _ in 0..2000 {
                        if !sent.lock().expect("rpc list mutex poisoned").is_empty() {
                            return None;
                        }
                        futures_timer::Delay::new(Duration::from_millis(1)).await;
                    }
                    None
                }));
            }
            Box::pin(futures::stream::pending())
        } else {
            let responses = self.responses.clone();
            let sent = self.sent.clone();
            Box::pin(stream::unfold(0, move |index| {
                let responses = responses.clone();
                let sent = sent.clone();
                async move {
                    let Some(response) = responses.get(index).cloned() else {
                        return futures::future::pending().await;
                    };
                    wait_for_matching_request_id(sent, &response).await;
                    Some((response, index + 1))
                }
            }))
        }
    }

    fn close(&self) {}
}

/// The scripting key for one request: a `state_call` is keyed by the runtime API
/// it names, every other method by its own name.
///
/// A path that reads both metadata and a view function issues both through
/// `state_call`, so keying those two apart is what lets a script answer them
/// differently.
fn response_key(request: &serde_json::Value) -> Option<&str> {
    let method = request["method"].as_str()?;
    if method == "state_call" {
        return Some(request["params"][0].as_str().unwrap_or(method));
    }
    Some(method)
}

/// Answer each request as it arrives, keyed by [`response_key`], echoing its id.
///
/// Repeated entries for one key are answered in call order. Running past the
/// last one panics rather than replaying it: a script that answers fewer calls
/// than the code makes would otherwise hand a response meant for one read to a
/// different one, which decodes to a plausible wrong value instead of failing.
///
/// Exhausted method scripts panic so one read cannot reuse another's response.
///
/// Waits indefinitely for the next request rather than giving up after a fixed
/// number of polls, so work between requests cannot race the pump.
fn method_keyed_responses(
    sent: Arc<Mutex<Vec<String>>>,
    answers: Vec<(&'static str, String)>,
) -> BoxStream<'static, String> {
    Box::pin(stream::unfold(0usize, move |answered| {
        let sent = sent.clone();
        let answers = answers.clone();
        async move {
            loop {
                let request = sent
                    .lock()
                    .expect("rpc list mutex poisoned")
                    .get(answered)
                    .cloned();
                if let Some(request) = request {
                    let value: serde_json::Value =
                        serde_json::from_str(&request).expect("request is valid JSON");
                    let id = value["id"].as_str().expect("request carries a string id");
                    let key = response_key(&value).expect("request carries a method");
                    let occurrence = sent
                        .lock()
                        .expect("rpc list mutex poisoned")
                        .iter()
                        .take(answered)
                        .filter(|earlier| {
                            serde_json::from_str::<serde_json::Value>(earlier)
                                .ok()
                                .and_then(|earlier| response_key(&earlier).map(str::to_owned))
                                .is_some_and(|candidate| candidate == key)
                        })
                        .count();
                    let scripted = answers
                        .iter()
                        .filter(|(candidate, _)| *candidate == key)
                        .collect::<Vec<_>>();
                    let result = scripted
                        .get(occurrence)
                        .map(|(_, body)| body.clone())
                        .unwrap_or_else(|| {
                            panic!(
                                "`{key}` was called {} times, and the script has {} response(s) for it",
                                occurrence + 1,
                                scripted.len(),
                            )
                        });
                    return Some((
                        format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":{result}}}"#),
                        answered + 1,
                    ));
                }
                futures_timer::Delay::new(Duration::from_millis(1)).await;
            }
        }
    }))
}

#[test]
#[should_panic(
    expected = "`state_getStorage` was called 2 times, and the script has 1 response(s) for it"
)]
fn method_keyed_responses_do_not_replay_an_exhausted_answer() {
    use futures::StreamExt;

    let request =
        |id| format!(r#"{{"jsonrpc":"2.0","id":"{id}","method":"state_getStorage","params":[]}}"#);
    let sent = Arc::new(Mutex::new(vec![request(1), request(2)]));
    let mut responses =
        method_keyed_responses(sent, vec![("state_getStorage", "null".to_string())]);

    futures::executor::block_on(async {
        responses.next().await.expect("first scripted response");
        responses.next().await.expect("second scripted response");
    });
}

async fn wait_for_matching_request_id(sent: Arc<Mutex<Vec<String>>>, response: &str) {
    let Some(id) = json_rpc_id(response) else {
        return;
    };
    for _ in 0..100 {
        if sent
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .any(|request| json_rpc_id(request).as_deref() == Some(id.as_str()))
        {
            return;
        }
        futures_timer::Delay::new(Duration::from_millis(1)).await;
    }
    panic!("request {id} was not issued before scripted response");
}

fn json_rpc_id(frame: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(frame).ok()?;
    match value.get("id")? {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

/// Sets its flag when dropped, marking a cancelled pending operation.
struct DropFlagGuard(Arc<AtomicBool>);

impl Drop for DropFlagGuard {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[crate::platform::async_trait]
impl ChainProvider for StubPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
        // Recorded before the failure branches: a test asserting which chain
        // was dialled needs the attempt even when the connect never succeeds.
        self.chain_connects
            .lock()
            .expect("chain connect mutex poisoned")
            .push(genesis_hash);
        if let Some(reason) = self.chain_connect_error {
            return Err(ProviderError::Host {
                reason: reason.to_string(),
            });
        }
        if self.unreachable_genesis == Some(genesis_hash) {
            return Err(ProviderError::Host {
                reason: "fixture serves no such chain".to_string(),
            });
        }
        if self.chain_connect_pending {
            let _guard = DropFlagGuard(self.pending_connect_dropped.clone());
            futures::future::pending::<()>().await;
        }
        Ok(Box::new(RecordingConnection {
            sent: self.sent_rpc.clone(),
            responses: self.rpc_responses.clone(),
            method_responses: self.rpc_method_responses.clone(),
            method_responses_gate: self.rpc_method_responses_gate.clone(),
            sso_response_script: self.sso_response_script.clone(),
            auth_states: self.auth_states.clone(),
            pairing_success_response: self.pairing_success_response,
            pairing_pending_response: self.pairing_pending_response,
            pairing_failure_response: self.pairing_failure_response,
            pairing_success_via_query: self.pairing_success_via_query,
            pairing_silent_after_subscribe: self.pairing_silent_after_subscribe,
            chain_responses_end: self.chain_responses_end,
        }))
    }
}

impl AuthPresenter for StubPlatform {
    fn auth_state_changed(&self, state: AuthState) {
        self.auth_states
            .lock()
            .expect("auth state list mutex poisoned")
            .push(state.clone());
        let hook = self
            .on_auth_state
            .lock()
            .expect("auth state hook mutex poisoned")
            .clone();
        if let Some(hook) = hook {
            hook(&state);
        }
    }
}

#[crate::platform::async_trait]
impl UserConfirmation for StubPlatform {
    async fn confirm_permission(
        &self,
        review: UserConfirmationReview,
    ) -> Result<crate::platform::PermissionDecision, v01::GenericError> {
        let confirmed = self.confirm_user_action(review).await?;
        Ok(self
            .permission_confirmation_decisions
            .lock()
            .expect("permission confirmation mutex poisoned")
            .pop_front()
            .unwrap_or(if confirmed {
                crate::platform::PermissionDecision::AllowAlways
            } else {
                crate::platform::PermissionDecision::Deny
            }))
    }

    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        let (error, confirmed) = match review {
            UserConfirmationReview::SignPayload(review) => {
                self.sign_payload_reviews
                    .lock()
                    .expect("sign payload review list mutex poisoned")
                    .push(review);
                (self.sign_payload_error, self.sign_payload_confirmed)
            }
            UserConfirmationReview::SignRaw(review) => {
                self.sign_raw_reviews
                    .lock()
                    .expect("raw signing review list mutex poisoned")
                    .push(review);
                (self.sign_raw_error, self.sign_raw_confirmed)
            }
            UserConfirmationReview::SignVrf(review) => {
                self.sign_vrf_reviews
                    .lock()
                    .expect("VRF signing review list mutex poisoned")
                    .push(review);
                (self.sign_vrf_error, self.sign_vrf_confirmed)
            }
            UserConfirmationReview::StatementStoreProductSign(review) => {
                self.statement_store_product_sign_reviews
                    .lock()
                    .expect("statement store product sign review list mutex poisoned")
                    .push(review);
                (self.sign_raw_error, self.sign_raw_confirmed)
            }
            UserConfirmationReview::CreateTransaction(review) => {
                self.create_transaction_reviews
                    .lock()
                    .expect("create transaction review list mutex poisoned")
                    .push(review);
                let gate = self
                    .create_transaction_confirmation_gate
                    .lock()
                    .expect("transaction confirmation gate mutex poisoned")
                    .take();
                if let Some(gate) = gate {
                    gate.await
                        .expect("transaction confirmation gate was released");
                }
                (
                    self.create_transaction_error,
                    self.create_transaction_confirmed,
                )
            }
            UserConfirmationReview::AccountAlias(_) => {
                (self.account_alias_error, self.account_alias_confirmed)
            }
            UserConfirmationReview::CreateProof(_) => {
                (self.create_proof_error, self.create_proof_confirmed)
            }
            UserConfirmationReview::AccountAccess(review) => {
                self.account_access_reviews
                    .lock()
                    .expect("account access review list mutex poisoned")
                    .push(review);
                (self.account_access_error, self.account_access_confirmed)
            }
            UserConfirmationReview::IdentityDisclosure(_) => {
                self.identity_disclosure_calls
                    .fetch_add(1, Ordering::SeqCst);
                (
                    self.identity_disclosure_error,
                    self.identity_disclosure_confirmed,
                )
            }
            UserConfirmationReview::ResourceAllocation(review) => {
                self.resource_allocation_reviews
                    .lock()
                    .expect("resource allocation review list mutex poisoned")
                    .push(review);
                let gate = self
                    .resource_allocation_confirmation_gate
                    .lock()
                    .expect("resource confirmation gate mutex poisoned")
                    .take();
                if let Some(gate) = gate {
                    gate.await.expect("resource confirmation gate was released");
                }
                (
                    self.resource_allocation_error,
                    self.resource_allocation_confirmed,
                )
            }
            UserConfirmationReview::PreimageSubmit(_) => (None, true),
            UserConfirmationReview::ProductSubtree(review) => {
                self.product_subtree_reviews
                    .lock()
                    .expect("product subtree review list mutex poisoned")
                    .push(review);
                (None, !self.product_subtree_denied)
            }
        };
        if let Some(reason) = error {
            return Err(v01::GenericError {
                reason: reason.to_string(),
            });
        }
        Ok(confirmed)
    }
}

impl ThemeHost for StubPlatform {
    fn subscribe_theme(
        &self,
    ) -> BoxStream<'static, Result<v01::HostThemeSubscribeItem, v01::GenericError>> {
        if self.theme_stream_pending {
            return Box::pin(PendingThemeStream {
                dropped: self.theme_stream_dropped.clone(),
            });
        }
        // A custom name, not `Default`: the core must forward whatever the host
        // reports instead of substituting a name of its own.
        Box::pin(stream::once(async {
            Ok(v01::HostThemeSubscribeItem {
                name: v01::ThemeName::Custom("midnight".to_string()),
                variant: v01::ThemeVariant::Dark,
            })
        }))
    }
}

impl LocaleHost for StubPlatform {
    fn subscribe_locale(
        &self,
    ) -> BoxStream<'static, Result<v01::HostLocaleSubscribeItem, v01::GenericError>> {
        Box::pin(stream::once(async {
            Ok(v01::HostLocaleSubscribeItem {
                language_tag: "zh-Hans".to_string(),
            })
        }))
    }
}

#[crate::platform::async_trait]
impl PreimageHost for StubPlatform {
    fn lookup_preimage(
        &self,
        _key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        let value = self.preimage_lookup_value.clone();
        Box::pin(stream::once(async move { Ok(value) }))
    }
}
