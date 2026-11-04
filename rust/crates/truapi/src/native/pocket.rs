//! The Pocket collection as a native host follows it, and the faces kept from
//! Pocket card renders.

use std::{
	collections::HashSet,
	sync::{Arc, Mutex},
};

use futures::{
	StreamExt,
	channel::mpsc::{UnboundedSender, unbounded},
};
use truapi::latest::ProductRendererRenderItem;

use super::renderer::{NativeRendererObserver, NativeSubscription, spawn_observation};
use crate::{
	platform::ProductId,
	pocket_cards::{PocketCardSide, PocketCards, PocketEntry},
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
/// kept as the newest on `side`. Keeping stops once the render's observer is
/// dropped, after writing the last face.
pub fn keeping_faces(
	cards: PocketCards,
	product_id: ProductId,
	card_id: String,
	side: PocketCardSide,
	observer: Arc<dyn NativeRendererObserver>,
	spawner: &Spawner,
) -> Arc<dyn NativeRendererObserver> {
	let (faces, drawn) = unbounded();
	let keeper = cards.clone();
	(spawner)(Box::pin(async move {
		keeper.keep_faces(product_id, card_id, side, drawn, Arc::new(RealTimer)).await;
	}));
	Arc::new(FaceKeepingObserver { observer, faces, cards })
}

/// The cards whose back the current product connection has drawn. A press on
/// a back reaches the product only for these: a product whose SDK predates
/// `PocketCardBack` cannot decode one and would lose its whole action stream,
/// and a product that drew a back has shown it can.
#[derive(Clone, Default)]
pub struct LiveBacks(Arc<Mutex<HashSet<String>>>);

impl LiveBacks {
	/// Records that `card_id`'s back has been drawn on this connection.
	pub fn mark(&self, card_id: &str) {
		self.0.lock().expect("live backs mutex poisoned").insert(card_id.to_owned());
	}

	/// Whether `card_id`'s back has been drawn on this connection.
	pub fn contains(&self, card_id: &str) -> bool {
		self.0.lock().expect("live backs mutex poisoned").contains(card_id)
	}

	/// Forgets every card, for a new product connection.
	pub fn reset(&self) {
		self.0.lock().expect("live backs mutex poisoned").clear();
	}

	/// Wraps a back render's observer so the first tree it draws marks the card.
	pub fn marking(
		&self,
		card_id: String,
		observer: Arc<dyn NativeRendererObserver>,
	) -> Arc<dyn NativeRendererObserver> {
		Arc::new(MarkingObserver { live: self.clone(), card_id, observer })
	}
}

struct MarkingObserver {
	live: LiveBacks,
	card_id: String,
	observer: Arc<dyn NativeRendererObserver>,
}

impl NativeRendererObserver for MarkingObserver {
	fn on_update(&self, node: ProductRendererRenderItem) {
		self.live.mark(&self.card_id);
		self.observer.on_update(node);
	}

	fn on_complete(&self) {
		self.observer.on_complete();
	}

	fn on_error(&self, reason: String) {
		self.observer.on_error(reason);
	}
}

struct FaceKeepingObserver {
	observer: Arc<dyn NativeRendererObserver>,
	faces: UnboundedSender<(ProductRendererRenderItem, u64)>,
	cards: PocketCards,
}

impl NativeRendererObserver for FaceKeepingObserver {
	fn on_update(&self, node: ProductRendererRenderItem) {
		self.observer.on_update(node.clone());
		let _ = self.faces.unbounded_send((node, self.cards.next_draw()));
	}

	fn on_complete(&self) {
		self.observer.on_complete();
	}

	fn on_error(&self, reason: String) {
		self.observer.on_error(reason);
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Mutex;

	use futures::executor::block_on;
	use truapi::latest::RendererNode;

	use super::*;
	use crate::{
		store::open_temporary_core_db,
		test_support::{test_spawner, wait_until},
	};

	#[derive(Default)]
	struct Drawn(Mutex<Vec<ProductRendererRenderItem>>);

	impl NativeRendererObserver for Drawn {
		fn on_update(&self, node: ProductRendererRenderItem) {
			self.0.lock().unwrap().push(node);
		}

		fn on_complete(&self) {}

		fn on_error(&self, _reason: String) {}
	}

	#[test]
	fn a_drawn_back_marks_its_card_until_the_connection_resets() {
		// Only a back the product has drawn may receive presses, and a new
		// product connection has drawn none yet.
		let live = LiveBacks::default();
		let drawn = Arc::new(Drawn::default());

		live.marking("stamps".into(), drawn.clone()).on_update(RendererNode::Nil);
		let marked = (live.contains("stamps"), live.contains("other"));
		live.reset();

		assert_eq!((marked, live.contains("stamps")), ((true, false), false));
		assert_eq!(*drawn.0.lock().unwrap(), vec![RendererNode::Nil]);
	}

	#[test]
	fn a_pocket_card_render_draws_each_face_and_keeps_it_on_its_side() {
		// The host draws what the product drew, and the card keeps it to show
		// offline and at the next start, on the side the render was for.
		let (_dir, db) = open_temporary_core_db();
		let cards = block_on(PocketCards::open(db, Vec::new())).unwrap();
		let product = ProductId::for_test("loyalty.dot");
		let card = || "stamps".to_string();
		block_on(cards.add(product.clone(), card(), "Stamps".into(), RendererNode::Nil)).unwrap();
		let drawn = Arc::new(Drawn::default());
		let render = |side| {
			keeping_faces(
				cards.clone(),
				product.clone(),
				card(),
				side,
				drawn.clone(),
				&test_spawner(),
			)
		};
		let front = RendererNode::String { text: "Stamps: 3".into() };
		let back = RendererNode::String { text: "Code".into() };

		render(PocketCardSide::Front).on_update(front.clone());
		render(PocketCardSide::Back).on_update(back.clone());

		assert_eq!(*drawn.0.lock().unwrap(), vec![front.clone(), back.clone()]);
		let kept = |side| block_on(cards.face(product.clone(), card(), side)).unwrap();
		wait_until(
			|| {
				(kept(PocketCardSide::Front), kept(PocketCardSide::Back)) ==
					(Some(front.clone()), Some(back.clone()))
			},
			"each drawn face is kept on its side",
		);
	}
}
