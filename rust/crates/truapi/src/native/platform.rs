use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::platform::{
    AuthPresenter, ChainProvider, CoreStorage, CoreStorageKey, DevicePermissionStatus, Features,
    JsonRpcConnection, LocaleHost, Navigation, Notifications, PermissionDecision, Permissions,
    PreimageHost, ProductContext, ProductOperations, ProductStorage, ProviderError, ThemeHost,
    UserConfirmation, UserConfirmationReview, async_trait,
};
use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};
use parity_scale_codec::Encode;
use truapi::v01;

use crate::host_logic::worker::WorkerTransition;
use crate::{DevicePairingObserver, PairedSsoPeer};

use super::callbacks::{
    HostCallbacks, NativeChatCallbacks, NativeContactsCallbacks, NativeGameCallbacks,
    NativePocketCallbacks, NativePocketRemoval,
};
use super::errors::HostRejection;
use super::events::NativeEventBus;

/// [`crate::platform::ContactsPlatform`] served by host-provided
/// [`NativeContactsCallbacks`]; constructed only when the host passed one.
pub struct ContactsCallbackPlatform {
    /// Host contact book and picker.
    pub contacts: Arc<dyn NativeContactsCallbacks>,
}

#[async_trait]
impl crate::platform::ContactsPlatform for ContactsCallbackPlatform {
    async fn contacts(
        &self,
        lookup: &crate::platform::HostContactLookup,
    ) -> Result<crate::platform::HostContactMatches, v01::GenericError> {
        self.contacts
            .contacts(lookup.clone())
            .map_err(|error| v01::GenericError {
                reason: error.to_string(),
            })
    }

    async fn pick_contact(
        &self,
        product: &ProductContext,
    ) -> Result<crate::platform::HostContactPick, v01::GenericError> {
        self.contacts
            .pick_contact(product.product_id.clone())
            .await
            .map_err(|error| v01::GenericError {
                reason: error.to_string(),
            })
    }
}

/// Every [`crate::platform::Platform`] trait served by one execution's
/// [`HostCallbacks`].
pub struct CallbackPlatform {
    /// Host callbacks this execution was opened with.
    pub callbacks: Arc<dyn HostCallbacks>,
    /// Events scoped to this execution.
    pub events: Arc<NativeEventBus>,
    /// Storage changes are product-wide rather than per-execution: a worker
    /// and the screen share one namespace, so they subscribe and publish on
    /// the runtime-wide bus instead of this execution's own.
    pub storage_events: Arc<NativeEventBus>,
}

impl crate::host_logic::worker::WorkerDemandObserver for CallbackPlatform {
    fn worker_demand_changed(&self, product_id: &str, transition: WorkerTransition) {
        self.callbacks
            .worker_demand_changed(product_id.to_string(), transition);
    }
}

impl DevicePairingObserver for CallbackPlatform {
    fn device_paired(&self, device: PairedSsoPeer) {
        self.callbacks.device_paired(device);
    }
}

#[async_trait]
impl Navigation for CallbackPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.navigate_to".to_string(),
            url.clone(),
        );
        self.callbacks.navigate_to(url).await
    }
}

#[async_trait]
impl Notifications for CallbackPlatform {
    async fn push_notification(
        &self,
        notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.push_notification".to_string(),
            notification.text.clone(),
        );

        let id = self
            .callbacks
            .push_notification(notification)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostPushNotificationResponse { id })
    }

    async fn cancel_notification(&self, id: u32) -> Result<(), v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.cancel_notification".to_string(),
            id.to_string(),
        );
        self.callbacks
            .cancel_notification(id)
            .map_err(v01::GenericError::from)
    }
}

#[async_trait]
impl crate::platform::PermissionStatusHost for CallbackPlatform {
    async fn device_permission_status(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<DevicePermissionStatus, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.device_permission_status".to_string(),
            format!("{request}"),
        );

        self.callbacks
            .device_permission_status(request)
            .await
            .map_err(v01::GenericError::from)
    }
}

#[async_trait]
impl Permissions for CallbackPlatform {
    async fn device_permission(
        &self,
        product: &ProductContext,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<PermissionDecision, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.device_permission".to_string(),
            format!("{request}"),
        );

        self.callbacks
            .device_permission(product.into(), request)
            .await
            .map_err(v01::GenericError::from)
    }

    async fn remote_permission(
        &self,
        product: &ProductContext,
        request: v01::RemotePermissionRequest,
    ) -> Result<PermissionDecision, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.remote_permission".to_string(),
            format!("{request}"),
        );

        self.callbacks
            .remote_permission(product.into(), request.permission)
            .await
            .map_err(v01::GenericError::from)
    }
}

#[async_trait]
impl Features for CallbackPlatform {
    async fn feature_supported(
        &self,
        request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.feature_supported".to_string(),
            format!("{request:?}"),
        );

        let supported = self
            .callbacks
            .feature_supported(request)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostFeatureSupportedResponse { supported })
    }

    async fn supported_chains(&self) -> Result<crate::platform::HostChainSet, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.supported_chains".to_string(),
            String::new(),
        );

        self.callbacks
            .supported_chains()
            .map_err(v01::GenericError::from)
    }
}

#[async_trait]
impl ProductStorage for CallbackPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        self.callbacks.local_storage_read(key).await
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        self.callbacks
            .local_storage_write(key.clone(), value.clone())
            .await?;
        self.storage_events
            .notify_storage_changed(&key, Some(value));
        Ok(())
    }

    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        self.callbacks.local_storage_clear(key.clone()).await?;
        self.storage_events.notify_storage_changed(&key, None);
        Ok(())
    }

    fn subscribe_storage(
        &self,
        key: String,
    ) -> BoxStream<'static, Result<v01::HostLocalStorageChangeItem, v01::GenericError>> {
        // Subscribe before reading, so a change landing between the two repeats
        // rather than being lost. The host pushes later changes via
        // `notify_storage_changed`.
        let rx = self.storage_events.subscribe_storage_changes(key.clone());
        let callbacks = self.callbacks.clone();
        let current = async move {
            callbacks
                .local_storage_read(key)
                .await
                .map(|value| v01::HostLocalStorageChangeItem { value })
                .map_err(|error| v01::GenericError {
                    reason: error.to_string(),
                })
        };
        stream::once(current).chain(rx).boxed()
    }
}

#[async_trait]
impl ProductOperations for CallbackPlatform {
    async fn begin_operation(
        &self,
        product: &ProductContext,
        label: String,
    ) -> Result<v01::HostWorkerBeginOperationResponse, v01::HostWorkerOperationError> {
        self.callbacks
            .begin_operation(product.product_id.clone(), label)
            .await
            .map(|id| v01::HostWorkerBeginOperationResponse { id })
            .map_err(|error| v01::HostWorkerOperationError::Unknown {
                reason: error.to_string(),
            })
    }

    async fn end_operation(
        &self,
        product: &ProductContext,
        id: u32,
    ) -> Result<(), v01::HostWorkerOperationError> {
        self.callbacks
            .end_operation(product.product_id.clone(), id)
            .await
            .map_err(|error| v01::HostWorkerOperationError::Unknown {
                reason: error.to_string(),
            })
    }
}

#[async_trait]
impl CoreStorage for CallbackPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        self.callbacks
            .core_storage_read(key.encode())
            .await
            .map_err(v01::GenericError::from)
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        self.callbacks
            .core_storage_write(key.encode(), value)
            .await
            .map_err(v01::GenericError::from)
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        self.callbacks
            .core_storage_clear(key.encode())
            .await
            .map_err(v01::GenericError::from)
    }
}

struct NativeJsonRpcConnection {
    id: u32,
    callbacks: Arc<dyn HostCallbacks>,
    events: Arc<NativeEventBus>,
    response_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    closed: AtomicBool,
}

impl JsonRpcConnection for NativeJsonRpcConnection {
    fn send(&self, request: String) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        if let Err(err) = self.callbacks.chain_send(self.id, request) {
            self.callbacks.on_core_log(
                "truapi.native.callback.chain_send_failed".to_string(),
                err.to_string(),
            );
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        let mut guard = self.response_rx.lock().unwrap();
        match guard.take() {
            Some(rx) => rx.boxed(),
            None => {
                self.callbacks.on_core_log(
                    "truapi.native.chain.responses_reused".to_string(),
                    "responses() called more than once".to_string(),
                );
                stream::empty().boxed()
            }
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        self.events.unregister_chain(self.id);
        if let Err(err) = self.callbacks.chain_close(self.id) {
            self.callbacks.on_core_log(
                "truapi.native.callback.chain_close_failed".to_string(),
                err.to_string(),
            );
        }
    }
}

impl Drop for NativeJsonRpcConnection {
    fn drop(&mut self) {
        self.close();
    }
}

#[async_trait]
impl ChainProvider for CallbackPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
        let close_generation = self.events.chain_close_generation();
        let Some(connection_id) = self
            .callbacks
            .chain_connect(genesis_hash.to_vec())
            .map_err(|rejection| ProviderError::Host {
                reason: v01::GenericError::from(rejection).reason,
            })?
        else {
            return Err(ProviderError::Host {
                reason: "chain provider unavailable".to_string(),
            });
        };
        let response_rx = self.events.register_chain(connection_id, close_generation);
        let registered = response_rx.is_some();
        let connection = NativeJsonRpcConnection {
            id: connection_id,
            callbacks: self.callbacks.clone(),
            events: self.events.clone(),
            response_rx: Mutex::new(response_rx),
            closed: AtomicBool::new(false),
        };
        if !registered {
            return Err(ProviderError::Host {
                reason: "chain connection closed during setup".to_string(),
            });
        }
        Ok(Box::new(connection))
    }
}

impl AuthPresenter for CallbackPlatform {
    fn auth_state_changed(&self, state: crate::platform::AuthState) {
        self.callbacks.on_core_log(
            "truapi.native.callback.auth_state_changed".to_string(),
            String::new(),
        );
        self.callbacks.auth_state_changed(state);
    }
}

#[async_trait]
impl UserConfirmation for CallbackPlatform {
    async fn confirm_permission(
        &self,
        review: UserConfirmationReview,
    ) -> Result<PermissionDecision, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_permission".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_permission(review)
            .await
            .map_err(v01::GenericError::from)
    }

    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_user_action".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_user_action(review)
            .await
            .map_err(v01::GenericError::from)
    }
}

impl ThemeHost for CallbackPlatform {
    fn subscribe_theme(
        &self,
    ) -> BoxStream<'static, Result<v01::HostThemeSubscribeItem, v01::GenericError>> {
        let current = self
            .callbacks
            .current_theme()
            .map_err(v01::GenericError::from);
        self.events.subscribe_theme(current)
    }
}

impl LocaleHost for CallbackPlatform {
    fn subscribe_locale(
        &self,
    ) -> BoxStream<'static, Result<v01::HostLocaleSubscribeItem, v01::GenericError>> {
        let current = self
            .callbacks
            .current_locale()
            .map_err(v01::GenericError::from);
        self.events.subscribe_locale(current)
    }
}

impl PreimageHost for CallbackPlatform {
    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        // Register the change receiver first so no event between the lookup
        // and the subscription is lost, then await the current value lazily.
        let rx = self.events.subscribe_preimage_changes(key.clone());
        let callbacks = self.callbacks.clone();
        let current = async move {
            callbacks
                .lookup_preimage(key)
                .await
                .map_err(v01::GenericError::from)
        };
        stream::once(current).chain(rx).boxed()
    }
}

/// [`crate::platform::ChatPlatform`] served by host-provided
/// [`NativeChatCallbacks`]; constructed only when the host passed one.
pub struct ChatCallbackPlatform {
    /// Host Chat storage and UI.
    pub chat: Arc<dyn NativeChatCallbacks>,
    /// Events scoped to the execution, carrying room-list changes.
    pub events: Arc<NativeEventBus>,
}

#[async_trait]
impl crate::platform::ChatPlatform for ChatCallbackPlatform {
    async fn create_chat_room(
        &self,
        _product: &ProductContext,
        request: v01::HostChatCreateRoomRequest,
    ) -> Result<v01::HostChatCreateRoomResponse, v01::HostChatCreateRoomError> {
        let status: v01::ChatRoomRegistrationStatus = self
            .chat
            .create_room(request.room_id, request.name, request.icon)
            .await
            .map_err(|error| v01::HostChatCreateRoomError::Unknown {
                reason: error.to_string(),
            })?;

        if status == v01::ChatRoomRegistrationStatus::New
            && let Ok(rooms) = self.chat.list_rooms().await
        {
            self.events.notify_chat_rooms_changed(rooms);
        }

        Ok(v01::HostChatCreateRoomResponse { status })
    }

    async fn register_chat_bot(
        &self,
        _product: &ProductContext,
        request: v01::HostChatRegisterBotRequest,
    ) -> Result<v01::HostChatRegisterBotResponse, v01::HostChatRegisterBotError> {
        let status = self
            .chat
            .register_bot(request.bot_id, request.name, request.icon)
            .await
            .map_err(|error| v01::HostChatRegisterBotError::Unknown {
                reason: error.to_string(),
            })?;

        // No room-list republish: a bot identity is not a room. A host that
        // joins the bot to one signals that via `notify_chat_rooms_changed`.
        Ok(v01::HostChatRegisterBotResponse { status })
    }

    async fn post_chat_message(
        &self,
        _product: &ProductContext,
        request: v01::HostChatPostMessageRequest,
    ) -> Result<v01::HostChatPostMessageResponse, v01::HostChatPostMessageError> {
        let message_id = self
            .chat
            .post_message(request.room_id, request.payload)
            .await
            .map_err(|error| v01::HostChatPostMessageError::Unknown {
                reason: error.to_string(),
            })?;
        Ok(v01::HostChatPostMessageResponse { message_id })
    }

    fn subscribe_chat_rooms(
        &self,
        _product: &ProductContext,
    ) -> BoxStream<'static, Result<v01::HostChatListSubscribeItem, v01::GenericError>> {
        // Registered before the snapshot: a change landing while the host answers
        // must be queued, not dropped.
        let changes = self.events.chat_room_changes();
        let chat = Arc::clone(&self.chat);
        stream::once(async move {
            v01::HostChatListSubscribeItem {
                rooms: chat.list_rooms().await.unwrap_or_default(),
            }
        })
        .chain(changes)
        .map(Ok)
        .boxed()
    }
}

/// [`crate::platform::PocketPlatform`] served by host-provided
/// [`NativePocketCallbacks`]; constructed only when the host passed one.
pub struct PocketCallbackPlatform {
    /// Host card collection.
    pub pocket: Arc<dyn NativePocketCallbacks>,
    /// Events scoped to the execution, carrying card-list changes.
    pub events: Arc<NativeEventBus>,
}

#[async_trait]
impl crate::platform::PocketPlatform for PocketCallbackPlatform {
    fn subscribe_pocket_cards(
        &self,
        _product: &ProductContext,
    ) -> BoxStream<'static, Result<v01::HostPocketListSubscribeItem, v01::GenericError>> {
        let pocket = self.pocket.clone();
        Box::pin(self.events.subscribe_pocket_cards(move || {
            pocket
                .list_cards()
                .map(|cards| v01::HostPocketListSubscribeItem { cards })
                .map_err(|error| v01::GenericError {
                    reason: error.to_string(),
                })
        }))
    }

    async fn remove_pocket_card(
        &self,
        _product: &ProductContext,
        request: v01::HostPocketRemoveCardRequest,
    ) -> Result<(), v01::HostPocketRemoveCardError> {
        let unknown = |error: HostRejection| v01::HostPocketRemoveCardError::Unknown {
            reason: error.to_string(),
        };
        match self
            .pocket
            .remove_card(request.card_id)
            .await
            .map_err(unknown)?
        {
            NativePocketRemoval::Privileged => Err(v01::HostPocketRemoveCardError::Privileged),
            // A card this host does not hold is already removed.
            NativePocketRemoval::Absent => Ok(()),
            NativePocketRemoval::Removed => {
                // The removal stands either way. A host that can no longer
                // list its cards says so on the stream rather than leaving the
                // product on a list that still holds the removed card.
                match self.pocket.list_cards() {
                    Ok(cards) => self.events.notify_pocket_cards_changed(cards),
                    Err(error) => self.events.notify_pocket_cards_failed(v01::GenericError {
                        reason: error.to_string(),
                    }),
                }
                Ok(())
            }
        }
    }
}

/// [`crate::platform::GamePlatform`] served by host-provided
/// [`NativeGameCallbacks`]; constructed only when the host passed one.
pub struct GameCallbackPlatform {
    /// Host game-reminder surface.
    pub game: Arc<dyn NativeGameCallbacks>,
}

#[async_trait]
impl crate::platform::GamePlatform for GameCallbackPlatform {
    async fn schedule_game_reminder(
        &self,
        _product: &ProductContext,
        starts_at: u64,
    ) -> Result<(), v01::GenericError> {
        self.game
            .schedule_reminder(starts_at)
            .await
            .map_err(v01::GenericError::from)
    }

    async fn cancel_game_reminder(
        &self,
        _product: &ProductContext,
    ) -> Result<(), v01::GenericError> {
        self.game
            .cancel_reminder()
            .await
            .map_err(v01::GenericError::from)
    }
}
