use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};
use std::collections::HashMap;
use std::sync::Mutex;
use truapi::v01;

/// Fans host notifications out to the product subscriptions and chain
/// connections that wait on them.
#[derive(Default)]
pub struct NativeEventBus {
    theme_changes:
        Mutex<Vec<mpsc::UnboundedSender<Result<v01::HostThemeSubscribeItem, v01::GenericError>>>>,
    locale_changes:
        Mutex<Vec<mpsc::UnboundedSender<Result<v01::HostLocaleSubscribeItem, v01::GenericError>>>>,
    preimage_changes: Mutex<Vec<PreimageSubscription>>,
    storage_changes: Mutex<Vec<StorageSubscription>>,
    chain_events: Mutex<NativeChainEvents>,
    chat_room_changes: Mutex<Vec<mpsc::UnboundedSender<v01::HostChatListSubscribeItem>>>,
    pocket_card_changes: Mutex<
        Vec<mpsc::UnboundedSender<Result<v01::HostPocketListSubscribeItem, v01::GenericError>>>,
    >,
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
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    /// Deliver a host theme change to every live theme subscriber.
    pub fn notify_theme_changed(&self, theme: v01::HostThemeSubscribeItem) {
        self.theme_changes
            .lock()
            .expect("native theme subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(Ok(theme.clone())).is_ok());
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
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    /// Deliver a host locale change to every live locale subscriber.
    pub fn notify_locale_changed(&self, locale: v01::HostLocaleSubscribeItem) {
        self.locale_changes
            .lock()
            .expect("native locale subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(Ok(locale.clone())).is_ok());
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
        let mut events = self
            .chain_events
            .lock()
            .expect("native chain subscribers mutex poisoned");
        if events.early_close_generation != close_generation {
            return None;
        }
        let (tx, rx) = mpsc::unbounded();
        events.responses.insert(connection_id, tx);
        Some(rx)
    }

    /// Deliver one JSON-RPC response to the connection it belongs to.
    pub fn notify_chain_response(&self, connection_id: u32, json: String) {
        let mut events = self
            .chain_events
            .lock()
            .expect("native chain subscribers mutex poisoned");
        let Some(tx) = events.responses.get(&connection_id) else {
            return;
        };
        if tx.unbounded_send(json).is_err() {
            events.responses.remove(&connection_id);
        }
    }

    /// End the response stream of a connection the host closed.
    pub fn notify_chain_closed(&self, connection_id: u32) {
        let mut events = self
            .chain_events
            .lock()
            .expect("native chain subscribers mutex poisoned");
        if events.responses.remove(&connection_id).is_none() {
            // Native callbacks can close a connection before returning its ID.
            events.early_close_generation = events.early_close_generation.wrapping_add(1);
        }
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
        let mut subscribers = self
            .chat_room_changes
            .lock()
            .expect("native Chat room subscribers mutex poisoned");
        subscribers.retain(|tx| !tx.is_closed());
        subscribers.push(tx);
        drop(subscribers);
        rx.boxed()
    }

    /// Deliver a full room-list replacement to every live Chat subscriber.
    pub fn notify_chat_rooms_changed(&self, rooms: Vec<v01::ChatRoom>) {
        let item = v01::HostChatListSubscribeItem { rooms };
        self.chat_room_changes
            .lock()
            .expect("native Chat room subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(item.clone()).is_ok());
    }

    /// Subscribe to the host's card collection. `snapshot` reads the host's
    /// cards while the subscriber mutex is held, so a replacement cannot land
    /// between the read and the registration: the snapshot is always the first
    /// item and every later change follows it in order. `snapshot` must not
    /// call back into [`super::NativeProductExecution::notify_pocket_cards_changed`],
    /// which takes the same mutex.
    pub fn subscribe_pocket_cards(
        &self,
        snapshot: impl FnOnce() -> Result<v01::HostPocketListSubscribeItem, v01::GenericError>,
    ) -> BoxStream<'static, Result<v01::HostPocketListSubscribeItem, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        let mut subscribers = self
            .pocket_card_changes
            .lock()
            .expect("native Pocket card subscribers mutex poisoned");
        let current = snapshot();
        // Subscribers are dropped when their product stops listening, and
        // nothing else prunes them between notifications.
        subscribers.retain(|tx| !tx.is_closed());
        subscribers.push(tx);
        drop(subscribers);
        stream::once(async move { current }).chain(rx).boxed()
    }

    /// Deliver a full card-list replacement to every live Pocket subscriber.
    pub fn notify_pocket_cards_changed(&self, cards: Vec<v01::PocketCard>) {
        self.send_pocket_cards(Ok(v01::HostPocketListSubscribeItem { cards }));
    }

    /// Report that the host can no longer say what the collection holds. The
    /// product reads it as a failed stream rather than as an empty collection.
    pub fn notify_pocket_cards_failed(&self, error: v01::GenericError) {
        self.send_pocket_cards(Err(error));
    }

    /// Send one Pocket stream item to every live subscriber, dropping closed ones.
    pub fn send_pocket_cards(
        &self,
        item: Result<v01::HostPocketListSubscribeItem, v01::GenericError>,
    ) {
        self.pocket_card_changes
            .lock()
            .expect("native Pocket card subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(item.clone()).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cancelled subscriber is pruned when the next one registers, so a
    /// product that subscribes and drops repeatedly cannot grow the list
    /// without bound between host notifications.
    #[test]
    fn a_cancelled_pocket_subscriber_is_pruned_on_the_next_registration() {
        let bus = NativeEventBus::default();
        let snapshot = || Ok(v01::HostPocketListSubscribeItem { cards: Vec::new() });
        for _ in 0..5 {
            drop(bus.subscribe_pocket_cards(snapshot));
        }
        let _live = bus.subscribe_pocket_cards(snapshot);

        assert_eq!(
            bus.pocket_card_changes
                .lock()
                .expect("native Pocket card subscribers mutex poisoned")
                .len(),
            1
        );
    }
}
