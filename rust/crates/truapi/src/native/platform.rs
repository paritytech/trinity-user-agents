use core::{iter, ops::ControlFlow, time::Duration};
use std::sync::{
	Arc, Mutex,
	atomic::{AtomicBool, Ordering},
};

use crate::{
	markers,
	platform::{
		AuthPresenter, ChainProvider, CoreStorage, CoreStorageKey, DevicePermissionStatus,
		ExtrinsicVersionHost, Features, JsonRpcConnection, LocaleHost, Navigation, Notifications,
		PermissionDecision, Permissions, PreimageHost, ProductContext, ProductId,
		ProductOperations, ProductStorage, ProviderError, StackStorageKey, ThemeHost,
		UserConfirmation, UserConfirmationReview,
	},
};
use futures::{
	FutureExt,
	channel::mpsc,
	future,
	stream::{self, BoxStream, StreamExt},
};
use parity_scale_codec::Encode;
use truapi::{latest, v01};
use zeroize::Zeroizing;

use crate::{
	DevicePairingObserver, PairedSsoPeer, Spawner,
	encrypted_storage::EncryptedStorage,
	host_internal::bulletin::preimage_key,
	host_logic::worker::WorkerTransition,
	installation::{InstallationId, InstallationIdProvider},
	time::{RealTimer, Timer, retry},
};

use super::{
	callbacks::{
		HostCallbacks, NativeBalanceCallbacks, NativeChatCallbacks, NativeContactsCallbacks,
		NativeFundingCallbacks, NativeGameCallbacks, NativePaymentCallbacks,
		NativeScannerCallbacks, NativeTopUpCallbacks,
	},
	engine_follow::{BalanceFollower, StatusFollower},
	errors::HostRejection,
	events::NativeEventBus,
	funding_feed::{FundingChangeKind, write_resume_marker},
};

/// [`crate::platform::ContactsPlatform`] served by host-provided
/// [`NativeContactsCallbacks`]; constructed only when the host passed one.
pub struct ContactsCallbackPlatform {
	/// Host contact book and picker.
	pub contacts: Arc<dyn NativeContactsCallbacks>,
}

#[async_trait::async_trait]
impl crate::platform::ContactsPlatform for ContactsCallbackPlatform {
	async fn contacts(
		&self,
		lookup: &crate::platform::HostContactLookup,
	) -> Result<crate::platform::HostContactMatches, v01::GenericError> {
		self.contacts
			.contacts(lookup.clone())
			.map_err(|error| v01::GenericError { reason: error.to_string() })
	}

	async fn pick_contact(
		&self,
		product: &ProductContext,
	) -> Result<crate::platform::HostContactPick, v01::GenericError> {
		self.contacts
			.pick_contact(product.product_id.clone())
			.await
			.map_err(|error| v01::GenericError { reason: error.to_string() })
	}
}

/// [`crate::platform::ScannerPlatform`] served by host-provided
/// [`NativeScannerCallbacks`]; constructed only when the host passed one.
pub struct ScannerCallbackPlatform {
	/// Host viewfinder.
	pub scanner: Arc<dyn NativeScannerCallbacks>,
}

#[async_trait::async_trait]
impl crate::platform::ScannerPlatform for ScannerCallbackPlatform {
	async fn scan_code(
		&self,
		product: &ProductContext,
		request: &truapi::latest::HostScannerScanRequest,
	) -> Result<crate::platform::HostScan, v01::GenericError> {
		self.scanner
			.scan_code(product.product_id.clone(), product.execution_kind, request.clone())
			.await
			.map_err(|error| v01::GenericError { reason: error.to_string() })
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

impl crate::host_logic::worker::WorkerController for CallbackPlatform {
	fn request_worker_transition(&self, product_id: &ProductId, transition: WorkerTransition) {
		self.callbacks.request_worker_transition(product_id.clone(), transition);
	}

	fn worker_crashing(&self, product_id: &ProductId, crashes: u32) {
		self.callbacks.worker_crashing(product_id.clone(), crashes);
	}
}

impl crate::durable::DurableWorkObserver for CallbackPlatform {
	fn durable_work_changed(&self, pending: bool) {
		self.callbacks.durable_work_changed(pending);
	}
}

impl DevicePairingObserver for CallbackPlatform {
	fn device_paired(&self, device: PairedSsoPeer) {
		self.callbacks.device_paired(device);
	}
}

#[async_trait::async_trait]
impl EncryptedStorage for CallbackPlatform {
	async fn read_encrypted(
		&self,
		key: &[u8],
	) -> Result<Option<Zeroizing<Vec<u8>>>, v01::GenericError> {
		let value = self.callbacks.encrypted_storage_read(key.to_vec()).await?;
		Ok(value.map(Zeroizing::new))
	}

	async fn write_encrypted(&self, key: &[u8], value: &[u8]) -> Result<(), v01::GenericError> {
		Ok(self.callbacks.encrypted_storage_write(key.to_vec(), value.to_vec()).await?)
	}

	async fn delete_encrypted(&self, key: &[u8]) -> Result<(), v01::GenericError> {
		Ok(self.callbacks.encrypted_storage_delete(key.to_vec()).await?)
	}
}

#[async_trait::async_trait]
impl InstallationIdProvider for CallbackPlatform {
	async fn current_installation(&self) -> Result<InstallationId, v01::GenericError> {
		let id = self.callbacks.current_installation().await?;
		InstallationId::try_from(id)
	}
}

#[async_trait::async_trait]
impl Navigation for CallbackPlatform {
	async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
		self.callbacks
			.on_core_log(markers::NATIVE_NAVIGATE_TO.target.to_string(), url.clone());
		self.callbacks.navigate_to(url).await
	}
}

#[async_trait::async_trait]
impl Notifications for CallbackPlatform {
	async fn push_notification(
		&self,
		notification: v01::HostPushNotificationRequest,
	) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
		self.callbacks.on_core_log(
			markers::NATIVE_PUSH_NOTIFICATION.target.to_string(),
			notification.text.clone(),
		);

		let id = self.callbacks.push_notification(notification).await?;
		Ok(v01::HostPushNotificationResponse { id })
	}

	async fn cancel_notification(&self, id: u32) -> Result<(), v01::GenericError> {
		self.callbacks
			.on_core_log(markers::NATIVE_CANCEL_NOTIFICATION.target.to_string(), id.to_string());
		self.callbacks.cancel_notification(id).map_err(v01::GenericError::from)
	}
}

#[async_trait::async_trait]
impl crate::platform::PermissionStatusHost for CallbackPlatform {
	async fn device_permission_status(
		&self,
		request: v01::HostDevicePermissionRequest,
	) -> Result<DevicePermissionStatus, v01::GenericError> {
		self.callbacks.on_core_log(
			markers::NATIVE_DEVICE_PERMISSION_STATUS.target.to_string(),
			format!("{request}"),
		);

		self.callbacks
			.device_permission_status(request)
			.await
			.map_err(v01::GenericError::from)
	}
}

#[async_trait::async_trait]
impl crate::platform::ExpandedCardHost for CallbackPlatform {
	async fn set_expanded_card_face_shown(
		&self,
		shown: bool,
	) -> Result<crate::platform::ExpandedCardFaceOutcome, v01::GenericError> {
		self.callbacks.on_core_log(
			markers::NATIVE_SET_EXPANDED_CARD_FACE_SHOWN.target.to_string(),
			format!("{shown}"),
		);

		self.callbacks
			.set_expanded_card_face_shown(shown)
			.await
			.map_err(v01::GenericError::from)
	}
}

#[async_trait::async_trait]
impl Permissions for CallbackPlatform {
	async fn device_permission(
		&self,
		product: &ProductContext,
		request: v01::HostDevicePermissionRequest,
	) -> Result<PermissionDecision, v01::GenericError> {
		self.callbacks.on_core_log(
			markers::NATIVE_DEVICE_PERMISSION.target.to_string(),
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
			markers::NATIVE_REMOTE_PERMISSION.target.to_string(),
			format!("{request}"),
		);

		self.callbacks
			.remote_permission(product.into(), request.permission)
			.await
			.map_err(v01::GenericError::from)
	}
}

#[async_trait::async_trait]
impl Features for CallbackPlatform {
	async fn feature_supported(
		&self,
		request: v01::HostFeatureSupportedRequest,
	) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
		self.callbacks.on_core_log(
			markers::NATIVE_FEATURE_SUPPORTED.target.to_string(),
			format!("{request:?}"),
		);

		let supported = self.callbacks.feature_supported(request).await?;
		Ok(v01::HostFeatureSupportedResponse { supported })
	}
}

#[async_trait::async_trait]
impl ExtrinsicVersionHost for CallbackPlatform {
	async fn transaction_extension_versions(
		&self,
	) -> Result<crate::platform::ExtensionVersionMap, v01::GenericError> {
		self.callbacks.on_core_log(
			markers::NATIVE_TRANSACTION_EXTENSION_VERSIONS.target.to_string(),
			String::new(),
		);
		self.callbacks
			.transaction_extension_versions()
			.await
			.map_err(v01::GenericError::from)
	}
}

#[async_trait::async_trait]
impl ProductStorage for CallbackPlatform {
	async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
		self.callbacks.local_storage_read(key).await
	}

	async fn write(
		&self,
		key: String,
		value: Vec<u8>,
	) -> Result<(), v01::HostLocalStorageReadError> {
		self.callbacks.local_storage_write(key.clone(), value.clone()).await?;
		self.storage_events.notify_storage_changed(&key, Some(value));
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
				.map_err(|error| v01::GenericError { reason: error.to_string() })
		};
		stream::once(current).chain(rx).boxed()
	}
}

#[async_trait::async_trait]
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
			.map_err(|error| v01::HostWorkerOperationError::Unknown { reason: error.to_string() })
	}

	async fn end_operation(
		&self,
		product: &ProductContext,
		id: u32,
	) -> Result<(), v01::HostWorkerOperationError> {
		self.callbacks
			.end_operation(product.product_id.clone(), id)
			.await
			.map_err(|error| v01::HostWorkerOperationError::Unknown { reason: error.to_string() })
	}
}

#[async_trait::async_trait]
impl CoreStorage for CallbackPlatform {
	async fn read_core_storage(
		&self,
		key: StackStorageKey,
	) -> Result<Option<Vec<u8>>, v01::GenericError> {
		self.callbacks
			.core_storage_read(key.encode())
			.await
			.map_err(v01::GenericError::from)
	}

	async fn write_core_storage(
		&self,
		key: StackStorageKey,
		value: Vec<u8>,
	) -> Result<(), v01::GenericError> {
		self.callbacks
			.core_storage_write(key.encode(), value)
			.await
			.map_err(v01::GenericError::from)
	}

	async fn clear_core_storage(&self, key: StackStorageKey) -> Result<(), v01::GenericError> {
		self.callbacks
			.core_storage_clear(key.encode())
			.await
			.map_err(v01::GenericError::from)
	}

	async fn read_unscoped_core_storage(
		&self,
		slot: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, v01::GenericError> {
		self.callbacks
			.core_storage_read(slot.encode())
			.await
			.map_err(v01::GenericError::from)
	}

	async fn clear_unscoped_core_storage(
		&self,
		slot: CoreStorageKey,
	) -> Result<(), v01::GenericError> {
		self.callbacks
			.core_storage_clear(slot.encode())
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
			self.callbacks
				.on_core_log(markers::NATIVE_CHAIN_SEND_FAILED.target.to_string(), err.to_string());
		}
	}

	fn responses(&self) -> BoxStream<'static, String> {
		let mut guard = self.response_rx.lock().unwrap();
		match guard.take() {
			Some(rx) => rx.boxed(),
			None => {
				self.callbacks.on_core_log(
					markers::NATIVE_CHAIN_RESPONSES_REUSED.target.to_string(),
					"responses() called more than once".to_string(),
				);
				stream::empty().boxed()
			},
		}
	}

	fn close(&self) {
		if self.closed.swap(true, Ordering::Relaxed) {
			return;
		}
		self.events.unregister_chain(self.id);
		self.callbacks.on_core_log(
			markers::NATIVE_CHAIN_CLOSING.target.to_string(),
			format!("connection {}", self.id),
		);
		if let Err(err) = self.callbacks.chain_close(self.id) {
			self.callbacks.on_core_log(
				markers::NATIVE_CHAIN_CLOSE_FAILED.target.to_string(),
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

/// Lowercase hex for a hash in a log line.
fn hex_lower(bytes: &[u8]) -> String {
	bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[async_trait::async_trait]
impl ChainProvider for CallbackPlatform {
	async fn connect(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
		let close_generation = self.events.chain_close_generation();
		let Some(connection_id) =
			self.callbacks.chain_connect(genesis_hash.to_vec()).map_err(|rejection| {
				ProviderError::Host { reason: v01::GenericError::from(rejection).reason }
			})?
		else {
			return Err(ProviderError::Host { reason: "chain provider unavailable".to_string() });
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
		// The socket closed before the connection was set up, so a later
		// attempt can still open it.
		if !registered {
			return Err(ProviderError::Transport {
				reason: "chain connection closed during setup".to_string(),
			});
		}
		// The core opens one connection per chain per caller scope, so the
		// count of these for one genesis hash is the count of live scopes
		// using that chain. Logged on the host's own channel because the
		// core's tracing level stays at ERROR unless a host raises it.
		self.callbacks.on_core_log(
			markers::NATIVE_CHAIN_CONNECTED.target.to_string(),
			format!("connection {connection_id} for chain {}", hex_lower(&genesis_hash)),
		);
		Ok(Box::new(connection))
	}
}

impl AuthPresenter for CallbackPlatform {
	fn auth_state_changed(&self, state: crate::platform::AuthState) {
		self.callbacks
			.on_core_log(markers::NATIVE_AUTH_STATE_CHANGED.target.to_string(), String::new());
		self.callbacks.auth_state_changed(state);
	}
}

#[async_trait::async_trait]
impl UserConfirmation for CallbackPlatform {
	async fn confirm_permission(
		&self,
		review: UserConfirmationReview,
	) -> Result<PermissionDecision, v01::GenericError> {
		self.callbacks
			.on_core_log(markers::NATIVE_CONFIRM_PERMISSION.target.to_string(), String::new());
		self.callbacks.confirm_permission(review).await.map_err(v01::GenericError::from)
	}

	async fn confirm_user_action(
		&self,
		review: UserConfirmationReview,
	) -> Result<bool, v01::GenericError> {
		self.callbacks
			.on_core_log(markers::NATIVE_CONFIRM_USER_ACTION.target.to_string(), String::new());
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
		let current = self.callbacks.current_theme().map_err(v01::GenericError::from);
		self.events.subscribe_theme(current)
	}
}

impl LocaleHost for CallbackPlatform {
	fn subscribe_locale(
		&self,
	) -> BoxStream<'static, Result<v01::HostLocaleSubscribeItem, v01::GenericError>> {
		let current = self.callbacks.current_locale().map_err(v01::GenericError::from);
		self.events.subscribe_locale(current)
	}
}

// Pauses between retries of a preimage lookup that missed. The first retries
// use the CLI host's poll interval and cover the 30 seconds the product SDK's
// lookup helpers wait. Later retries are slower, to limit requests for lookups
// a product keeps open.
#[cfg(not(test))]
const PREIMAGE_RETRY_PAUSE_FAST: Duration = Duration::from_secs(6);
#[cfg(not(test))]
const PREIMAGE_RETRY_PAUSE_SLOW: Duration = Duration::from_secs(30);
#[cfg(test)]
const PREIMAGE_RETRY_PAUSE_FAST: Duration = Duration::from_millis(1);
#[cfg(test)]
const PREIMAGE_RETRY_PAUSE_SLOW: Duration = Duration::from_millis(4);
const PREIMAGE_FAST_RETRIES: usize = 5;

impl PreimageHost for CallbackPlatform {
	fn lookup_preimage(
		&self,
		key: [u8; 32],
	) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
		// Register the change receiver first so no event between the lookup
		// and the subscription is lost, then await the current value lazily.
		let rx = self.events.subscribe_preimage_changes(key.to_vec());
		let callbacks = self.callbacks.clone();
		async move {
			let current = callbacks.lookup_preimage(key.to_vec()).await;
			// A miss can mean the fetch failed, and hosts are not required to
			// notify the value later, so retry until the bytes hash to the key.
			// An error is final: apps answer a failed fetch with `None`, and
			// `HostRejection` has no case for a failure worth retrying.
			let retry = match &current {
				Ok(Some(value)) => preimage_key(value) != key,
				Ok(None) => true,
				Err(HostRejection::Rejected { .. }) => false,
			};
			let found = stream::iter(retry.then_some(key))
				.then(move |key| lookup_until_found(callbacks.clone(), key))
				.map(|found| found.map(Some).map_err(v01::GenericError::from));
			let current = current.map_err(v01::GenericError::from);
			stream::once(future::ready(current)).chain(stream::select(rx.map(Ok), found))
		}
		.flatten_stream()
		.boxed()
	}
}

/// Retry a host lookup that missed until it returns bytes that hash to
/// `digest` or an error.
async fn lookup_until_found(
	callbacks: Arc<dyn HostCallbacks>,
	digest: [u8; 32],
) -> Result<Vec<u8>, HostRejection> {
	let key_hex = hex_lower(&digest);
	callbacks.on_core_log(markers::NATIVE_PREIMAGE_RETRYING.target.to_string(), key_hex.clone());
	let mut fast_pauses = iter::repeat_n(PREIMAGE_RETRY_PAUSE_FAST, PREIMAGE_FAST_RETRIES);
	let mut next_pause = move || fast_pauses.next().unwrap_or(PREIMAGE_RETRY_PAUSE_SLOW);
	// The first lookup already missed, so even the first retry waits.
	RealTimer.sleep(next_pause()).await;
	let mut retries = 0u32;
	let found = retry(
		|| {
			retries += 1;
			async {
				match callbacks.lookup_preimage(digest.to_vec()).await {
					Ok(Some(value)) if preimage_key(&value) == digest => {
						ControlFlow::Break(Ok(value))
					},
					Ok(_) => ControlFlow::Continue(()),
					Err(rejection) => ControlFlow::Break(Err(rejection)),
				}
			}
		},
		|()| next_pause(),
	)
	.await;
	if found.is_ok() {
		callbacks.on_core_log(
			markers::NATIVE_PREIMAGE_FOUND.target.to_string(),
			format!("{key_hex} on retry {retries}"),
		);
	}
	found
}

/// [`crate::platform::ChatPlatform`] served by host-provided
/// [`NativeChatCallbacks`]; constructed only when the host passed one.
pub struct ChatCallbackPlatform {
	/// Host Chat storage and UI.
	pub chat: Arc<dyn NativeChatCallbacks>,
	/// Events scoped to the execution, carrying room-list changes.
	pub events: Arc<NativeEventBus>,
}

#[async_trait::async_trait]
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
			.map_err(|error| v01::HostChatCreateRoomError::Unknown { reason: error.to_string() })?;

		if status == v01::ChatRoomRegistrationStatus::New &&
			let Ok(rooms) = self.chat.list_rooms().await
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
		request: truapi::latest::HostChatPostMessageRequest,
	) -> Result<truapi::latest::HostChatPostMessageResponse, truapi::latest::HostChatPostMessageError>
	{
		let message_id = self
			.chat
			.post_message(request.room_id, request.payload, request.alt)
			.await
			.map_err(|error| truapi::latest::HostChatPostMessageError::Unknown {
				reason: error.to_string(),
			})?;
		Ok(truapi::latest::HostChatPostMessageResponse { message_id })
	}

	async fn set_chat_room_footer(
		&self,
		_product: &ProductContext,
		request: truapi::latest::HostChatSetRoomFooterRequest,
	) -> Result<(), v01::GenericError> {
		self.chat
			.set_room_footer(request.room_id, request.footer)
			.await
			.map_err(v01::GenericError::from)
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
			v01::HostChatListSubscribeItem { rooms: chat.list_rooms().await.unwrap_or_default() }
		})
		.chain(changes)
		.map(Ok)
		.boxed()
	}
}

/// [`crate::platform::GamePlatform`] served by host-provided
/// [`NativeGameCallbacks`]; constructed only when the host passed one.
pub struct GameCallbackPlatform {
	/// Host game-reminder surface.
	pub game: Arc<dyn NativeGameCallbacks>,
}

#[async_trait::async_trait]
impl crate::platform::GamePlatform for GameCallbackPlatform {
	async fn schedule_game_reminder(
		&self,
		_product: &ProductContext,
		starts_at: u64,
	) -> Result<(), v01::GenericError> {
		self.game.schedule_reminder(starts_at).await.map_err(v01::GenericError::from)
	}

	async fn cancel_game_reminder(
		&self,
		_product: &ProductContext,
	) -> Result<(), v01::GenericError> {
		self.game.cancel_reminder().await.map_err(v01::GenericError::from)
	}
}

/// [`crate::platform::FundingPlatform`] served by host-provided
/// [`NativeFundingCallbacks`].
pub struct FundingCallbackPlatform {
	/// Host funding overlay.
	pub funding: Arc<dyn NativeFundingCallbacks>,
	/// The core database directory, which holds the resume marker.
	pub database_directory: std::path::PathBuf,
}

#[async_trait::async_trait]
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
		target: latest::FundingFrameTarget,
	) -> Result<latest::FundingFrameOutcome, latest::GenericError> {
		self.funding
			.present_provider_frame(provider.product_id.clone(), intent, target)
			.await
			.map_err(latest::GenericError::from)
	}

	fn close_provider_frame(&self, provider: &ProductContext, intent: String) {
		self.funding.close_provider_frame(provider.product_id.clone(), intent);
	}

	async fn detected_country(&self) -> Option<String> {
		self.funding.detected_country().await
	}

	fn funding_quotes_changed(
		&self,
		intent: String,
		_quotes: Option<crate::platform::FundingQuoteList>,
	) {
		self.funding.funding_changed(Some(intent), FundingChangeKind::Quotes);
	}
}

impl crate::runtime::FundingFeed for FundingCallbackPlatform {
	fn funding_session_changed(&self, intent: String) {
		self.funding.funding_changed(Some(intent), FundingChangeKind::Session);
	}

	fn funding_in_progress_changed(&self, in_progress: bool) {
		write_resume_marker(&self.database_directory, in_progress);
	}

	fn funding_options_changed(&self, intent: String) {
		self.funding.funding_changed(Some(intent), FundingChangeKind::Options);
	}

	fn funding_providers_changed(&self) {
		self.funding.funding_changed(None, FundingChangeKind::Providers);
	}
}

/// What a product is told when it names a purse: native hosts hold only the
/// main one.
const MAIN_PURSE_ONLY: &str = "only the main purse is supported";

/// [`crate::platform::TopUpPlatform`] served by host-provided
/// [`NativeTopUpCallbacks`], following each top-up through a
/// [`StatusFollower`] the host pushes to with [`Self::notify_status`].
pub struct TopUpCallbackPlatform {
	top_up: Arc<dyn NativeTopUpCallbacks>,
	statuses: StatusFollower<v01::HostPaymentTopUpStatusSubscribeItem>,
}

impl TopUpCallbackPlatform {
	/// Serve top-ups from `top_up`, following them on `spawner`.
	pub fn new(top_up: Arc<dyn NativeTopUpCallbacks>, spawner: Spawner) -> Self {
		let host = top_up.clone();
		let follow = Arc::new(move |product_id, id| {
			let host = host.clone();
			async move { host.follow_top_up(product_id, id).await }.boxed()
		});
		Self { top_up, statuses: StatusFollower::new(follow, spawner) }
	}

	/// Take a status the host's engine reported for `product_id`'s top-up
	/// `id`.
	pub fn notify_status(
		&self,
		product_id: ProductId,
		id: crate::Bytes32,
		status: v01::HostPaymentTopUpStatusSubscribeItem,
	) {
		self.statuses.notify(product_id, id, status);
	}
}

#[async_trait::async_trait]
impl crate::platform::TopUpPlatform for TopUpCallbackPlatform {
	async fn top_up(
		&self,
		product: &ProductContext,
		request: crate::latest::HostPaymentTopUpRequest,
	) -> Result<(), crate::latest::HostPaymentTopUpError> {
		if request.into.is_some() {
			return Err(crate::latest::HostPaymentTopUpError::Unknown {
				reason: MAIN_PURSE_ONLY.into(),
			});
		}
		let id = request.id;
		self.top_up.top_up(product.product_id.clone(), request).await?;
		self.statuses.follow(product.product_id.clone(), id);
		Ok(())
	}

	fn subscribe_top_up_status(
		&self,
		product: &ProductContext,
		id: crate::Bytes32,
	) -> BoxStream<
		'static,
		Result<v01::HostPaymentTopUpStatusSubscribeItem, v01::HostPaymentTopUpStatusSubscribeError>,
	> {
		self.statuses.subscribe(product.product_id.clone(), id)
	}
}

/// [`crate::platform::PaymentPlatform`] served by host-provided
/// [`NativePaymentCallbacks`], following each payment through a
/// [`StatusFollower`] the host pushes to with [`Self::notify_status`].
pub struct PaymentCallbackPlatform {
	payments: Arc<dyn NativePaymentCallbacks>,
	statuses: StatusFollower<v01::HostPaymentStatusSubscribeItem>,
}

impl PaymentCallbackPlatform {
	/// Serve payments from `payments`, following them on `spawner`.
	pub fn new(payments: Arc<dyn NativePaymentCallbacks>, spawner: Spawner) -> Self {
		let host = payments.clone();
		let follow = Arc::new(move |product_id, id| {
			let host = host.clone();
			async move { host.follow_payment(product_id, id).await }.boxed()
		});
		Self { payments, statuses: StatusFollower::new(follow, spawner) }
	}

	/// Take a status the host's engine reported for `product_id`'s payment
	/// `id`.
	pub fn notify_status(
		&self,
		product_id: ProductId,
		id: crate::Bytes32,
		status: v01::HostPaymentStatusSubscribeItem,
	) {
		self.statuses.notify(product_id, id, status);
	}
}

#[async_trait::async_trait]
impl crate::platform::PaymentPlatform for PaymentCallbackPlatform {
	async fn request_payment(
		&self,
		product: &ProductContext,
		request: crate::latest::HostPaymentRequest,
	) -> Result<(), crate::latest::HostPaymentError> {
		if request.from.is_some() {
			return Err(crate::latest::HostPaymentError::Unknown {
				reason: MAIN_PURSE_ONLY.into(),
			});
		}
		let id = request.id;
		self.payments.request_payment(product.product_id.clone(), request).await?;
		self.statuses.follow(product.product_id.clone(), id);
		Ok(())
	}

	fn subscribe_payment_status(
		&self,
		product: &ProductContext,
		id: crate::Bytes32,
	) -> BoxStream<
		'static,
		Result<v01::HostPaymentStatusSubscribeItem, v01::HostPaymentStatusSubscribeError>,
	> {
		self.statuses.subscribe(product.product_id.clone(), id)
	}
}

/// [`crate::platform::BalancePlatform`] served by host-provided
/// [`NativeBalanceCallbacks`], following the balance through a
/// [`BalanceFollower`] the host pushes to with [`Self::notify_balance`].
pub struct BalanceCallbackPlatform {
	follower: BalanceFollower,
}

impl BalanceCallbackPlatform {
	/// Serve balances from `balance`, following it on `spawner`.
	pub fn new(balance: Arc<dyn NativeBalanceCallbacks>, spawner: Spawner) -> Self {
		let follow = Arc::new(move || {
			let balance = balance.clone();
			async move { balance.follow_balance().await }.boxed()
		});
		Self { follower: BalanceFollower::new(follow, spawner) }
	}

	/// Take the balance the host's engine reported. The core drops it for a
	/// product whose balance access was revoked.
	pub fn notify_balance(&self, available: u128) {
		self.follower.notify(available);
	}
}

impl crate::platform::BalancePlatform for BalanceCallbackPlatform {
	fn subscribe_balance(
		&self,
		_product: &ProductContext,
		purse: Option<u32>,
	) -> BoxStream<
		'static,
		Result<v01::HostPaymentBalanceSubscribeItem, v01::HostPaymentBalanceSubscribeError>,
	> {
		if purse.is_some() {
			let error =
				v01::HostPaymentBalanceSubscribeError::Unknown { reason: MAIN_PURSE_ONLY.into() };
			return stream::iter([Err(error)]).boxed();
		}
		self.follower
			.subscribe()
			.map(|balance| {
				balance.map(|available| v01::HostPaymentBalanceSubscribeItem { available })
			})
			.boxed()
	}
}
