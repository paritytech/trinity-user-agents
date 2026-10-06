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
    HostCallbacks, NativeChatCallbacks, NativeContactsCallbacks, NativeFundingCallbacks,
    NativePaymentCallbacks, NativePocketCallbacks, NativePocketRemoval, NativeTopUpCallbacks,
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

/// [`crate::platform::FundingPlatform`] served by host-provided
/// [`NativeFundingCallbacks`].
pub struct FundingCallbackPlatform {
    /// Host funding overlay.
    pub funding: Arc<dyn NativeFundingCallbacks>,
}

#[async_trait]
impl crate::platform::FundingPlatform for FundingCallbackPlatform {
    async fn present_funding(
        &self,
        product: Option<&ProductContext>,
        session: crate::platform::FundingPresentation,
    ) -> Result<crate::platform::FundingPresentOutcome, v01::GenericError> {
        self.funding
            .present_funding(
                product.map(|product| product.product_id.clone()),
                session.intent,
                session.direction,
                session.amount,
            )
            .await
            .map_err(v01::GenericError::from)
    }

    async fn present_provider_frame(
        &self,
        provider: &ProductContext,
        intent: String,
        route: String,
    ) -> Result<v01::FundingFrameOutcome, v01::GenericError> {
        self.funding
            .present_provider_frame(provider.product_id.clone(), intent, route)
            .await
            .map_err(v01::GenericError::from)
    }

    fn funding_session_changed(&self, intent: String, status: v01::HostFundingStatusSubscribeItem) {
        self.funding.funding_session_changed(intent, status);
    }
}

/// One subscription to a host-pushed status.
struct Follower<S> {
    /// Tells this subscription apart from others on the same id.
    subscription: u64,
    sender: mpsc::UnboundedSender<S>,
}

/// Who follows which status, per product and id.
struct Followers<S> {
    next: u64,
    by_id: std::collections::HashMap<(String, crate::Bytes32), Vec<Follower<S>>>,
}

impl<S> Default for Followers<S> {
    fn default() -> Self {
        Self {
            next: 0,
            by_id: std::collections::HashMap::new(),
        }
    }
}

impl<S> Followers<S> {
    /// Forget followers whose streams were dropped.
    fn prune(&mut self) {
        self.by_id.retain(|_, senders| {
            senders.retain(|follower| !follower.sender.is_closed());
            !senders.is_empty()
        });
    }
}

/// Rank of a terminal status.
const TERMINAL_RANK: u8 = 3;

/// Statuses a host answers once and pushes after, relayed to every stream
/// following an id: each starts at the host's current status and shows every
/// later one once, never stepping back, and ends at a terminal one.
struct StatusRelay<T, E> {
    followers: Mutex<Followers<Result<T, E>>>,
    /// How far along a status is; [`TERMINAL_RANK`] ends a stream.
    rank: fn(&T) -> u8,
}

impl<T: Clone + Send + 'static, E: Send + 'static> StatusRelay<T, E> {
    fn new(rank: fn(&T) -> u8) -> Self {
        Self {
            followers: Mutex::new(Followers::default()),
            rank,
        }
    }

    /// Deliver a later `status` of `product_id`'s `id` to everyone following
    /// it; a terminal one ends their streams.
    fn notify(&self, product_id: String, id: crate::Bytes32, status: T) {
        let terminal = (self.rank)(&status) == TERMINAL_RANK;
        let mut followers = self.lock_followers();
        let key = (product_id, id);
        if let Some(senders) = followers.by_id.get_mut(&key) {
            for follower in senders.iter() {
                let _ = follower.sender.unbounded_send(Ok(status.clone()));
            }
        }
        if terminal {
            followers.by_id.remove(&key);
        }
        followers.prune();
    }

    /// Follow `product_id`'s `id` from `current`, the host's answer for it
    /// now; an error ends the stream there.
    fn follow(
        &self,
        product_id: String,
        id: crate::Bytes32,
        current: impl FnOnce() -> Result<T, E>,
    ) -> BoxStream<'static, Result<T, E>> {
        let key = (product_id, id);
        // Registered before the current status is read, so a status the host
        // pushes in between is not lost; the stream below drops it if it
        // repeats or precedes the current one.
        let (sender, changes) = mpsc::unbounded();
        let follower = {
            let mut followers = self.lock_followers();
            followers.prune();
            followers.next += 1;
            let follower = followers.next;
            followers.by_id.entry(key.clone()).or_default().push(Follower {
                subscription: follower,
                sender,
            });
            follower
        };
        let rank = self.rank;
        let first = current();
        let last_rank = first.as_ref().map_or(TERMINAL_RANK, rank);
        if last_rank == TERMINAL_RANK {
            let mut followers = self.lock_followers();
            if let Some(senders) = followers.by_id.get_mut(&key) {
                senders.retain(|registered| registered.subscription != follower);
            }
            followers.prune();
            return stream::iter([first]).boxed();
        }
        let later = changes.scan(last_rank, move |shown, status: Result<T, E>| {
            let next = status.as_ref().map_or(TERMINAL_RANK, rank);
            let fresh = next > *shown || (next == *shown && next == TERMINAL_RANK);
            if fresh {
                *shown = next;
            }
            futures::future::ready(Some(fresh.then_some(status)))
        });
        stream::iter([first])
            .chain(later.filter_map(futures::future::ready))
            .boxed()
    }

    /// How many ids are followed.
    #[cfg(test)]
    fn followed(&self) -> usize {
        self.lock_followers().by_id.len()
    }

    fn lock_followers(&self) -> std::sync::MutexGuard<'_, Followers<Result<T, E>>> {
        self.followers
            .lock()
            .expect("status followers mutex poisoned")
    }
}

/// [`crate::platform::TopUpPlatform`] served by host-provided
/// [`NativeTopUpCallbacks`]: the host answers a top-up's current status, and
/// pushes each later one through [`Self::notify_status`].
pub struct TopUpCallbackPlatform {
    top_up: Arc<dyn NativeTopUpCallbacks>,
    statuses: StatusRelay<v01::HostPaymentTopUpStatusSubscribeItem, v01::HostPaymentTopUpStatusSubscribeError>,
}

impl TopUpCallbackPlatform {
    /// Serve top-ups from `top_up`.
    pub fn new(top_up: Arc<dyn NativeTopUpCallbacks>) -> Self {
        Self {
            top_up,
            statuses: StatusRelay::new(top_up_rank),
        }
    }

    /// Deliver a later status of `product_id`'s top-up `id` to everyone
    /// following it; a terminal one ends their streams.
    pub fn notify_status(
        &self,
        product_id: String,
        id: crate::Bytes32,
        status: v01::HostPaymentTopUpStatusSubscribeItem,
    ) {
        self.statuses.notify(product_id, id, status);
    }
}

/// How far along a top-up `status` is, so a stream never steps back.
fn top_up_rank(status: &v01::HostPaymentTopUpStatusSubscribeItem) -> u8 {
    match status {
        v01::HostPaymentTopUpStatusSubscribeItem::Detecting => 0,
        v01::HostPaymentTopUpStatusSubscribeItem::Claiming => 1,
        v01::HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false } => 2,
        v01::HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true }
        | v01::HostPaymentTopUpStatusSubscribeItem::ClaimedPartially { .. }
        | v01::HostPaymentTopUpStatusSubscribeItem::NotClaimed => TERMINAL_RANK,
    }
}

#[async_trait]
impl crate::platform::TopUpPlatform for TopUpCallbackPlatform {
    async fn top_up(
        &self,
        product: &ProductContext,
        request: v01::HostPaymentTopUpRequest,
    ) -> Result<(), v01::HostPaymentTopUpError> {
        self.top_up.top_up(product.product_id.clone(), request).await
    }

    fn subscribe_top_up_status(
        &self,
        product: &ProductContext,
        id: crate::Bytes32,
    ) -> BoxStream<'static, Result<v01::HostPaymentTopUpStatusSubscribeItem, v01::HostPaymentTopUpStatusSubscribeError>>
    {
        let product_id = product.product_id.clone();
        self.statuses.follow(product_id.clone(), id, || {
            match self.top_up.top_up_status(product_id, id) {
                Ok(Some(status)) => Ok(status),
                Ok(None) => Err(v01::HostPaymentTopUpStatusSubscribeError::NotFound),
                Err(error) => Err(v01::HostPaymentTopUpStatusSubscribeError::Unknown {
                    reason: error.to_string(),
                }),
            }
        })
    }
}

/// [`crate::platform::PaymentPlatform`] served by host-provided
/// [`NativePaymentCallbacks`]: the host answers a payment's current status,
/// and pushes each later one through [`Self::notify_status`].
pub struct PaymentCallbackPlatform {
    payments: Arc<dyn NativePaymentCallbacks>,
    statuses: StatusRelay<v01::HostPaymentStatusSubscribeItem, v01::HostPaymentStatusSubscribeError>,
}

impl PaymentCallbackPlatform {
    /// Serve payments from `payments`.
    pub fn new(payments: Arc<dyn NativePaymentCallbacks>) -> Self {
        Self {
            payments,
            statuses: StatusRelay::new(payment_rank),
        }
    }

    /// Deliver a later status of `product_id`'s payment `id` to everyone
    /// following it; a terminal one ends their streams.
    pub fn notify_status(&self, product_id: String, id: crate::Bytes32, status: v01::HostPaymentStatusSubscribeItem) {
        self.statuses.notify(product_id, id, status);
    }
}

/// How far along a payment `status` is, so a stream never steps back.
fn payment_rank(status: &v01::HostPaymentStatusSubscribeItem) -> u8 {
    match status {
        v01::HostPaymentStatusSubscribeItem::Processing => 0,
        v01::HostPaymentStatusSubscribeItem::Completed
        | v01::HostPaymentStatusSubscribeItem::Failed { .. }
        | v01::HostPaymentStatusSubscribeItem::PartiallyClaimed { .. } => TERMINAL_RANK,
    }
}

#[async_trait]
impl crate::platform::PaymentPlatform for PaymentCallbackPlatform {
    async fn request_payment(
        &self,
        product: &ProductContext,
        request: v01::HostPaymentRequest,
    ) -> Result<(), v01::HostPaymentError> {
        self.payments
            .request_payment(product.product_id.clone(), request)
            .await
    }

    fn subscribe_payment_status(
        &self,
        product: &ProductContext,
        id: crate::Bytes32,
    ) -> BoxStream<'static, Result<v01::HostPaymentStatusSubscribeItem, v01::HostPaymentStatusSubscribeError>> {
        let product_id = product.product_id.clone();
        self.statuses.follow(product_id.clone(), id, || {
            match self.payments.payment_status(product_id, id) {
                Ok(Some(status)) => Ok(status),
                Ok(None) => Err(v01::HostPaymentStatusSubscribeError::PaymentNotFound),
                Err(error) => Err(v01::HostPaymentStatusSubscribeError::Unknown {
                    reason: error.to_string(),
                }),
            }
        })
    }
}

#[cfg(test)]
mod status_relay_tests {
    use super::*;

    use futures::executor::block_on;

    use crate::platform::TopUpPlatform;

    /// A top-up engine holding one status per id, and nothing else.
    struct Engine(Mutex<std::collections::HashMap<crate::Bytes32, v01::HostPaymentTopUpStatusSubscribeItem>>);

    #[async_trait::async_trait]
    impl NativeTopUpCallbacks for Engine {
        async fn top_up(
            &self,
            _product_id: String,
            _request: v01::HostPaymentTopUpRequest,
        ) -> Result<(), v01::HostPaymentTopUpError> {
            Ok(())
        }

        fn top_up_status(
            &self,
            _product_id: String,
            id: crate::Bytes32,
        ) -> Result<Option<v01::HostPaymentTopUpStatusSubscribeItem>, HostRejection> {
            Ok(self.0.lock().expect("statuses").get(&id).cloned())
        }
    }

    fn product() -> ProductContext {
        ProductContext {
            product_id: "fund.dot".into(),
            execution_kind: Default::default(),
        }
    }

    // A follower sees the status the host holds now, then every status the
    // host pushes, and its stream ends with the claim's verdict, so core's
    // credit step and a product's subscription both learn how it ended.
    #[test]
    fn a_top_up_is_followed_from_its_current_status_to_its_verdict() {
        let engine = Arc::new(Engine(Mutex::new(std::collections::HashMap::from([(
            [1; 32],
            v01::HostPaymentTopUpStatusSubscribeItem::Detecting,
        )]))));
        let platform = TopUpCallbackPlatform::new(engine);
        let followed = platform.subscribe_top_up_status(&product(), [1; 32]);
        let missing = platform.subscribe_top_up_status(&product(), [2; 32]);

        platform.notify_status(
            "fund.dot".into(),
            [1; 32],
            v01::HostPaymentTopUpStatusSubscribeItem::Claiming,
        );
        platform.notify_status(
            "fund.dot".into(),
            [1; 32],
            v01::HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true },
        );

        assert_eq!(
            (
                block_on(followed.collect::<Vec<_>>()),
                block_on(missing.collect::<Vec<_>>()),
                platform.statuses.followed(),
            ),
            (
                vec![
                    Ok(v01::HostPaymentTopUpStatusSubscribeItem::Detecting),
                    Ok(v01::HostPaymentTopUpStatusSubscribeItem::Claiming),
                    Ok(v01::HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true }),
                ],
                vec![Err(v01::HostPaymentTopUpStatusSubscribeError::NotFound)],
                0,
            )
        );
    }

    // Core's credit step subscribes afresh on every pass. Its subscription
    // seeing a verdict in the host's store must not end a product's stream
    // that still waits for the host to push that verdict.
    #[test]
    fn a_subscriber_that_sees_the_verdict_leaves_other_followers_waiting() {
        let engine = Arc::new(Engine(Mutex::new(std::collections::HashMap::from([(
            [1; 32],
            v01::HostPaymentTopUpStatusSubscribeItem::Claiming,
        )]))));
        let platform = TopUpCallbackPlatform::new(engine.clone());
        let product_stream = platform.subscribe_top_up_status(&product(), [1; 32]);
        engine.0.lock().expect("statuses").insert(
            [1; 32],
            v01::HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true },
        );
        let core_pass = platform.subscribe_top_up_status(&product(), [1; 32]);
        let verdict = v01::HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true };
        platform.notify_status("fund.dot".into(), [1; 32], verdict.clone());

        assert_eq!(
            (
                block_on(core_pass.collect::<Vec<_>>()),
                block_on(product_stream.collect::<Vec<_>>()),
            ),
            (
                vec![Ok(verdict.clone())],
                vec![
                    Ok(v01::HostPaymentTopUpStatusSubscribeItem::Claiming),
                    Ok(verdict)
                ],
            )
        );
    }

    // A status pushed while the snapshot is read reaches the follower too;
    // a stream shows each status once and never steps back, and a follower
    // that stops listening is forgotten.
    #[test]
    fn a_stream_never_repeats_or_steps_back_and_dropped_followers_are_forgotten() {
        let engine = Arc::new(Engine(Mutex::new(std::collections::HashMap::from([(
            [1; 32],
            v01::HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false },
        )]))));
        let platform = TopUpCallbackPlatform::new(engine);
        let followed = platform.subscribe_top_up_status(&product(), [1; 32]);
        let dropped = platform.subscribe_top_up_status(&product(), [1; 32]);
        drop(dropped);
        for status in [
            v01::HostPaymentTopUpStatusSubscribeItem::Claiming,
            v01::HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false },
            v01::HostPaymentTopUpStatusSubscribeItem::NotClaimed,
        ] {
            platform.notify_status("fund.dot".into(), [1; 32], status);
        }
        let _ = platform.subscribe_top_up_status(&product(), [2; 32]);

        assert_eq!(
            (
                block_on(followed.collect::<Vec<_>>()),
                platform.statuses.followed(),
            ),
            (
                vec![
                    Ok(v01::HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false }),
                    Ok(v01::HostPaymentTopUpStatusSubscribeItem::NotClaimed),
                ],
                0,
            )
        );
    }

    /// A payment engine holding one status per id, and nothing else.
    struct Payments(Mutex<std::collections::HashMap<crate::Bytes32, v01::HostPaymentStatusSubscribeItem>>);

    #[async_trait::async_trait]
    impl NativePaymentCallbacks for Payments {
        async fn request_payment(
            &self,
            _product_id: String,
            _request: v01::HostPaymentRequest,
        ) -> Result<(), v01::HostPaymentError> {
            Ok(())
        }

        fn payment_status(
            &self,
            _product_id: String,
            id: crate::Bytes32,
        ) -> Result<Option<v01::HostPaymentStatusSubscribeItem>, HostRejection> {
            Ok(self.0.lock().expect("statuses").get(&id).cloned())
        }
    }

    // A payment is followed from the host's current status to the verdict
    // it pushes, and one the host does not hold is reported as not found,
    // so its caller knows to request it again.
    #[test]
    fn a_payment_is_followed_to_its_verdict_and_an_unknown_one_is_not_found() {
        use crate::platform::PaymentPlatform;

        let payments = Arc::new(Payments(Mutex::new(std::collections::HashMap::from([(
            [1; 32],
            v01::HostPaymentStatusSubscribeItem::Processing,
        )]))));
        let platform = PaymentCallbackPlatform::new(payments);
        let followed = platform.subscribe_payment_status(&product(), [1; 32]);
        let missing = platform.subscribe_payment_status(&product(), [2; 32]);
        platform.notify_status("fund.dot".into(), [1; 32], v01::HostPaymentStatusSubscribeItem::Completed);

        assert_eq!(
            (block_on(followed.collect::<Vec<_>>()), block_on(missing.collect::<Vec<_>>())),
            (
                vec![
                    Ok(v01::HostPaymentStatusSubscribeItem::Processing),
                    Ok(v01::HostPaymentStatusSubscribeItem::Completed),
                ],
                vec![Err(v01::HostPaymentStatusSubscribeError::PaymentNotFound)],
            )
        );
    }
}
