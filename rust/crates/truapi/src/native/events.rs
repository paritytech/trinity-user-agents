use futures::{
	channel::mpsc,
	stream::{self, BoxStream, StreamExt},
};
use std::{collections::HashMap, sync::Mutex};
use truapi::v01;

use super::subscribers::Subscribers;

/// Fans host notifications out to the product subscriptions and chain
/// connections that wait on them.
#[derive(Default)]
pub struct NativeEventBus {
	theme_changes: Mutex<Subscribers<(), Result<v01::HostThemeSubscribeItem, v01::GenericError>>>,
	locale_changes: Mutex<Subscribers<(), Result<v01::HostLocaleSubscribeItem, v01::GenericError>>>,
	preimage_changes: Mutex<Vec<PreimageSubscription>>,
	storage_changes: Mutex<Vec<StorageSubscription>>,
	chain_events: Mutex<NativeChainEvents>,
	chat_room_changes: Mutex<Subscribers<(), v01::HostChatListSubscribeItem>>,
}

#[derive(Default)]
struct NativeChainEvents {
	responses: HashMap<u32, mpsc::UnboundedSender<String>>,
	early_close_generation: u64,
}

struct PreimageSubscription {
	key: Vec<u8>,
	tx: mpsc::UnboundedSender<Result<Option<Vec<u8>>, v01::GenericError>>,
}

struct StorageSubscription {
	key: String,
	tx: mpsc::UnboundedSender<Result<v01::HostLocalStorageChangeItem, v01::GenericError>>,
}

impl NativeEventBus {
	/// Stream the theme: `current` first, then every host change.
	pub fn subscribe_theme(
		&self,
		current: Result<v01::HostThemeSubscribeItem, v01::GenericError>,
	) -> BoxStream<'static, Result<v01::HostThemeSubscribeItem, v01::GenericError>> {
		let (tx, rx) = mpsc::unbounded();
		self.theme_changes
			.lock()
			.expect("native theme subscribers mutex poisoned")
			.register((), tx);
		stream::once(async move { current }).chain(rx).boxed()
	}

	/// Deliver a host theme change to every live theme subscriber.
	pub fn notify_theme_changed(&self, theme: v01::HostThemeSubscribeItem) {
		self.theme_changes
			.lock()
			.expect("native theme subscribers mutex poisoned")
			.send(&(), &Ok(theme));
	}

	/// Stream the locale: `current` first, then every host change.
	pub fn subscribe_locale(
		&self,
		current: Result<v01::HostLocaleSubscribeItem, v01::GenericError>,
	) -> BoxStream<'static, Result<v01::HostLocaleSubscribeItem, v01::GenericError>> {
		let (tx, rx) = mpsc::unbounded();
		self.locale_changes
			.lock()
			.expect("native locale subscribers mutex poisoned")
			.register((), tx);
		stream::once(async move { current }).chain(rx).boxed()
	}

	/// Deliver a host locale change to every live locale subscriber.
	pub fn notify_locale_changed(&self, locale: v01::HostLocaleSubscribeItem) {
		self.locale_changes
			.lock()
			.expect("native locale subscribers mutex poisoned")
			.send(&(), &Ok(locale));
	}

	/// Stream later host changes to the preimage stored under `key`.
	pub fn subscribe_preimage_changes(
		&self,
		key: Vec<u8>,
	) -> mpsc::UnboundedReceiver<Result<Option<Vec<u8>>, v01::GenericError>> {
		let (tx, rx) = mpsc::unbounded();
		self.preimage_changes
			.lock()
			.expect("native preimage subscribers mutex poisoned")
			.push(PreimageSubscription { key, tx });
		rx
	}

	/// Deliver a preimage change to the subscribers of `key`.
	pub fn notify_preimage_changed(&self, key: &[u8], value: Option<Vec<u8>>) {
		self.preimage_changes
			.lock()
			.expect("native preimage subscribers mutex poisoned")
			.retain(|sub| {
				if sub.key != key {
					return true;
				}
				sub.tx.unbounded_send(Ok(value.clone())).is_ok()
			});
	}

	/// Stream later changes to the product storage value under `key`.
	pub fn subscribe_storage_changes(
		&self,
		key: String,
	) -> mpsc::UnboundedReceiver<Result<v01::HostLocalStorageChangeItem, v01::GenericError>> {
		let (tx, rx) = mpsc::unbounded();
		self.storage_changes
			.lock()
			.expect("native storage subscribers mutex poisoned")
			.push(StorageSubscription { key, tx });
		rx
	}

	/// Deliver a product storage change to the subscribers of `key`.
	pub fn notify_storage_changed(&self, key: &str, value: Option<Vec<u8>>) {
		let item = v01::HostLocalStorageChangeItem { value };
		self.storage_changes
			.lock()
			.expect("native storage subscribers mutex poisoned")
			.retain(|sub| {
				if sub.key != key {
					return true;
				}
				sub.tx.unbounded_send(Ok(item.clone())).is_ok()
			});
	}

	/// Count of host-reported closes, taken before a connect so a close that
	/// lands during it is not missed.
	pub fn chain_close_generation(&self) -> u64 {
		self.chain_events
			.lock()
			.expect("native chain subscribers mutex poisoned")
			.early_close_generation
	}

	/// Route responses for `connection_id` to a new receiver, or `None` when the
	/// host closed the connection after `close_generation` was read.
	pub fn register_chain(
		&self,
		connection_id: u32,
		close_generation: u64,
	) -> Option<mpsc::UnboundedReceiver<String>> {
		let mut events = self.chain_events.lock().expect("native chain subscribers mutex poisoned");
		if events.early_close_generation != close_generation {
			return None;
		}
		let (tx, rx) = mpsc::unbounded();
		events.responses.insert(connection_id, tx);
		Some(rx)
	}

	/// Deliver one JSON-RPC response to the connection it belongs to.
	pub fn notify_chain_response(&self, connection_id: u32, json: String) {
		let mut events = self.chain_events.lock().expect("native chain subscribers mutex poisoned");
		let Some(tx) = events.responses.get(&connection_id) else {
			return;
		};
		if tx.unbounded_send(json).is_err() {
			events.responses.remove(&connection_id);
		}
	}

	/// End the response stream of a connection the host closed.
	pub fn notify_chain_closed(&self, connection_id: u32) {
		if !self.close_chain_if_registered(connection_id) {
			// Native callbacks can close a connection before returning its ID.
			self.note_early_chain_close();
		}
	}

	/// End the response stream of `connection_id` and report whether this bus
	/// was the one holding it.
	///
	/// A product execution reports a close to its own bus and to the shared
	/// one, because either may hold the connection. Exactly one of them does,
	/// so a miss here is routine and must not be mistaken for a close that
	/// raced a connect: see [`Self::note_early_chain_close`].
	pub fn close_chain_if_registered(&self, connection_id: u32) -> bool {
		self.chain_events
			.lock()
			.expect("native chain subscribers mutex poisoned")
			.responses
			.remove(&connection_id)
			.is_some()
	}

	/// Record that the host closed a connection whose id no bus recognised,
	/// which is what a close arriving before `chain_connect` returns looks
	/// like. Invalidates connects that read the generation before it.
	pub fn note_early_chain_close(&self) {
		let mut events = self.chain_events.lock().expect("native chain subscribers mutex poisoned");
		events.early_close_generation = events.early_close_generation.wrapping_add(1);
	}

	/// Stop routing responses for a connection the core closed.
	pub fn unregister_chain(&self, connection_id: u32) {
		self.chain_events
			.lock()
			.expect("native chain subscribers mutex poisoned")
			.responses
			.remove(&connection_id);
	}

	/// Register for later room changes, separately from the snapshot: a mutex
	/// cannot be held across the host's await the way `subscribe_pocket_cards` does.
	pub fn chat_room_changes(&self) -> BoxStream<'static, v01::HostChatListSubscribeItem> {
		let (tx, rx) = mpsc::unbounded();
		self.chat_room_changes
			.lock()
			.expect("native Chat room subscribers mutex poisoned")
			.register((), tx);
		rx.boxed()
	}

	/// Deliver a full room-list replacement to every live Chat subscriber.
	pub fn notify_chat_rooms_changed(&self, rooms: Vec<v01::ChatRoom>) {
		let item = v01::HostChatListSubscribeItem { rooms };
		self.chat_room_changes
			.lock()
			.expect("native Chat room subscribers mutex poisoned")
			.send(&(), &item);
	}
}
