//! The Pocket collection as a native host follows it, and the faces kept from
//! Pocket card renders.

use std::sync::Arc;

use futures::{
	StreamExt,
	channel::mpsc::{UnboundedSender, unbounded},
};
use truapi::latest::ProductRendererRenderItem;

use super::renderer::{NativeRendererObserver, NativeSubscription, spawn_observation};
use crate::{
	platform::ProductId,
	pocket_cards::{PocketCards, PocketEntry},
	subscription::Spawner,
	time::RealTimer,
};

/// Observer implemented by a native host to follow the Pocket collection.
#[uniffi::export(callback_interface)]
pub trait NativePocketObserver: Send + Sync {
	/// Deliver every card, pinned first, now and after each change.
	fn on_cards(&self, cards: Vec<PocketEntry>);

	/// Report that the collection could not be read. It is delivered again
	/// once a later change reads.
	fn on_error(&self, reason: String);
}

/// Delivers the collection to `observer` until the subscription is cancelled.
pub fn observe_pocket_cards(
	cards: &PocketCards,
	observer: Arc<dyn NativePocketObserver>,
	spawner: &Spawner,
) -> Arc<NativeSubscription> {
	let mut cards = cards.observe();
	spawn_observation(spawner, async move {
		while let Some(item) = cards.next().await {
			match item {
				Ok(cards) => observer.on_cards(cards),
				Err(error) => observer.on_error(error.to_string()),
			}
		}
	})
}

/// Wraps the observer of one card's render so every face it draws is also
/// kept as the card's newest. Keeping stops once the render's observer is
/// dropped, after writing the last face.
pub fn keeping_faces(
	cards: PocketCards,
	product_id: ProductId,
	card_id: String,
	observer: Arc<dyn NativeRendererObserver>,
	spawner: &Spawner,
) -> Arc<dyn NativeRendererObserver> {
	let (faces, drawn) = unbounded();
	(spawner)(Box::pin(async move {
		cards.keep_faces(product_id, card_id, drawn, Arc::new(RealTimer)).await;
	}));
	Arc::new(FaceKeepingObserver { observer, faces })
}

struct FaceKeepingObserver {
	observer: Arc<dyn NativeRendererObserver>,
	faces: UnboundedSender<ProductRendererRenderItem>,
}

impl NativeRendererObserver for FaceKeepingObserver {
	fn on_update(&self, node: ProductRendererRenderItem) {
		self.observer.on_update(node.clone());
		let _ = self.faces.unbounded_send(node);
	}

	fn on_complete(&self) {
		self.observer.on_complete();
	}

	fn on_error(&self, reason: String) {
		self.observer.on_error(reason);
	}
}
