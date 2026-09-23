#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use futures::stream::{self, BoxStream};
use truapi::v01;
use truapi_platform::{
    AuthPresenter, ChainProvider, CoreStorage, CoreStorageKey, Features, HostInfo,
    JsonRpcConnection, LocaleHost, NativeChatFileExportRequest, NativeChatFilePickRequest,
    NativeChatFilesHost, NativeChatPickedFile, Navigation, Notifications, PairingHostConfig,
    Permissions, PlatformInfo, PreimageHost, ProductContext, ProductOperations, ProductStorage,
    ThemeHost, UserConfirmation, UserConfirmationReview,
};
use truapi_server::frame::ProtocolMessage;
use truapi_server::transport::Transport;

/// Transport stub that records every frame sent through it, for asserting
/// what the core emits during a dispatch.
#[derive(Default)]
pub struct RecordingTransport {
    /// Frames captured in send order.
    pub sent: Mutex<Vec<ProtocolMessage>>,
    recorded: Condvar,
}

impl RecordingTransport {
    /// Wait until at least `count` frames have been recorded, or `timeout`
    /// elapses. A subscription's frames are produced by the spawner, so a
    /// dispatch that starts one returns before they are sent.
    pub fn wait_for(&self, count: usize, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mut sent = self.sent.lock().unwrap();
        while sent.len() < count {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let (guard, _) = self.recorded.wait_timeout(sent, deadline - now).unwrap();
            sent = guard;
        }
    }
}

impl Transport for RecordingTransport {
    fn send(&self, message: ProtocolMessage) {
        self.sent.lock().unwrap().push(message);
        self.recorded.notify_all();
    }
    fn on_message(
        &self,
        _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
    ) -> Box<dyn FnOnce()> {
        Box::new(|| {})
    }
}

/// Test spawner that matches the current target.
pub fn test_spawner() -> truapi_server::subscription::Spawner {
    #[cfg(not(target_arch = "wasm32"))]
    {
        truapi_server::subscription::thread_per_subscription_spawner()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Arc::new(futures::executor::block_on)
    }
}

/// Runtime configuration shared by integration tests.
pub fn test_runtime_config() -> (PairingHostConfig, ProductContext) {
    (
        PairingHostConfig::new(
            HostInfo {
                name: "Polkadot Web".to_string(),
                icon: Some("https://dot.li/dotli.png".to_string()),
                version: None,
                platform: truapi::latest::HostPlatform::Web,
            },
            PlatformInfo::default(),
            [0xa2; 32],
            [0xbb; 32],
            [0xcc; 32],
            "polkadotapp".to_string(),
        )
        .expect("test host runtime config is valid"),
        ProductContext::new("dotli.dot".to_string()).expect("test product context is valid"),
    )
}

/// Platform stub whose callbacks return fixed no-op values, enough for
/// wire-shape tests that only inspect emitted frames.
pub struct WireShapePlatform;

#[truapi_platform::async_trait]
impl ProductStorage for WireShapePlatform {
    async fn read(&self, _key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        Err(v01::HostLocalStorageReadError::Full)
    }
    async fn write(
        &self,
        _key: String,
        _value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        Ok(())
    }
    async fn clear(&self, _key: String) -> Result<(), v01::HostLocalStorageReadError> {
        Ok(())
    }
    fn subscribe_storage(
        &self,
        _key: String,
    ) -> BoxStream<'static, Result<v01::HostLocalStorageChangeItem, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}

#[truapi_platform::async_trait]
impl ProductOperations for WireShapePlatform {
    async fn begin_operation(
        &self,
        _product: &ProductContext,
        _label: String,
    ) -> Result<v01::HostWorkerBeginOperationResponse, v01::HostWorkerOperationError> {
        Ok(v01::HostWorkerBeginOperationResponse { id: 1 })
    }
    async fn end_operation(
        &self,
        _product: &ProductContext,
        _id: u32,
    ) -> Result<(), v01::HostWorkerOperationError> {
        Ok(())
    }
}

#[truapi_platform::async_trait]
impl Navigation for WireShapePlatform {
    async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
        Ok(())
    }
}

#[truapi_platform::async_trait]
impl Notifications for WireShapePlatform {
    async fn push_notification(
        &self,
        _notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        Ok(v01::HostPushNotificationResponse { id: 0 })
    }

    async fn cancel_notification(&self, _id: u32) -> Result<(), v01::GenericError> {
        Ok(())
    }
}

#[truapi_platform::async_trait]
impl Permissions for WireShapePlatform {
    async fn device_permission(
        &self,
        _product: &ProductContext,
        _request: v01::HostDevicePermissionRequest,
    ) -> Result<truapi_platform::PermissionDecision, v01::GenericError> {
        Ok(truapi_platform::PermissionDecision::AllowAlways)
    }
    async fn remote_permission(
        &self,
        _product: &ProductContext,
        _request: v01::RemotePermissionRequest,
    ) -> Result<truapi_platform::PermissionDecision, v01::GenericError> {
        Ok(truapi_platform::PermissionDecision::AllowAlways)
    }
}

#[truapi_platform::async_trait]
impl Features for WireShapePlatform {
    async fn feature_supported(
        &self,
        _request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        Ok(v01::HostFeatureSupportedResponse { supported: true })
    }

    async fn supported_chains(&self) -> Result<truapi_platform::HostChainSet, v01::GenericError> {
        Ok(truapi_platform::HostChainSet {
            network: "paseo".to_string(),
            chains: vec![truapi_platform::HostChainEntry {
                identifier: v01::ChainIdentifier::AssetHub,
                genesis_hash: [0xaa; 32],
            }],
        })
    }
}

struct DeadConnection;

impl JsonRpcConnection for DeadConnection {
    fn send(&self, _request: String) {}
    fn responses(&self) -> BoxStream<'static, String> {
        Box::pin(stream::empty())
    }
    fn close(&self) {}
}

#[truapi_platform::async_trait]
impl ChainProvider for WireShapePlatform {
    async fn connect(
        &self,
        _genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        Ok(Box::new(DeadConnection))
    }
}

#[truapi_platform::async_trait]
impl truapi_platform::HopProvider for WireShapePlatform {}

impl AuthPresenter for WireShapePlatform {}

#[truapi_platform::async_trait]
impl CoreStorage for WireShapePlatform {
    async fn read_core_storage(
        &self,
        _key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        Ok(None)
    }
    async fn write_core_storage(
        &self,
        _key: CoreStorageKey,
        _value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        Ok(())
    }
    async fn clear_core_storage(&self, _key: CoreStorageKey) -> Result<(), v01::GenericError> {
        Ok(())
    }
}

#[truapi_platform::async_trait]
impl UserConfirmation for WireShapePlatform {
    async fn confirm_user_action(
        &self,
        _review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        Ok(false)
    }
}

impl ThemeHost for WireShapePlatform {
    fn subscribe_theme(
        &self,
    ) -> BoxStream<'static, Result<v01::HostThemeSubscribeItem, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}

impl LocaleHost for WireShapePlatform {
    fn subscribe_locale(
        &self,
    ) -> BoxStream<'static, Result<v01::HostLocaleSubscribeItem, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}

impl PreimageHost for WireShapePlatform {
    fn lookup_preimage(
        &self,
        _key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}

fn unavailable_chat_files<T>() -> Result<T, v01::GenericError> {
    Err(v01::GenericError {
        reason: "native Chat files unavailable in this fixture".into(),
    })
}

#[truapi_platform::async_trait]
impl NativeChatFilesHost for WireShapePlatform {
    async fn pick_chat_files(
        &self,
        _: NativeChatFilePickRequest,
    ) -> Result<Vec<NativeChatPickedFile>, v01::GenericError> {
        unavailable_chat_files()
    }
    async fn read_chat_file(
        &self,
        _: String,
        _: u64,
        _: u32,
    ) -> Result<Vec<u8>, v01::GenericError> {
        unavailable_chat_files()
    }
    async fn release_chat_file(&self, _: String) -> Result<(), v01::GenericError> {
        unavailable_chat_files()
    }
    async fn begin_chat_file_export(
        &self,
        _: NativeChatFileExportRequest,
    ) -> Result<Option<String>, v01::GenericError> {
        unavailable_chat_files()
    }
    async fn write_chat_file_export(
        &self,
        _: String,
        _: u64,
        _: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        unavailable_chat_files()
    }
    async fn finish_chat_file_export(&self, _: String) -> Result<(), v01::GenericError> {
        unavailable_chat_files()
    }
    async fn cancel_chat_file_export(&self, _: String) -> Result<(), v01::GenericError> {
        unavailable_chat_files()
    }
}
