use std::sync::Mutex;

use futures::executor::block_on;
use truapi::latest::RendererNode;

use super::*;
use crate::{store::open_temporary_core_db, test_support::wait_until};

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
fn a_pocket_card_render_draws_each_face_and_keeps_the_newest() {
	// The host draws what the product drew, and the card keeps it to show
	// offline and at the next start.
	let (_dir, db) = open_temporary_core_db();
	let cards = block_on(PocketCards::open(db, Vec::new())).unwrap();
	let product = ProductId::for_test("loyalty.dot");
	let card = || "stamps".to_string();
	block_on(cards.add(product.clone(), card(), "Stamps".into(), RendererNode::Nil)).unwrap();
	let drawn = Arc::new(Drawn::default());
	let card_id = pocket_card(&RenderContext::PocketCard { card_id: card() })
		.expect("a Pocket card context names its card");
	let observer =
		keeping_faces(cards.clone(), product.clone(), card_id, drawn.clone(), &Spawner::for_test());
	let first = RendererNode::String { text: "Stamps: 3".into() };
	let newest = RendererNode::String { text: "Stamps: 4".into() };

	observer.on_update(first.clone());
	observer.on_update(newest.clone());

	assert_eq!(*drawn.0.lock().unwrap(), vec![first, newest.clone()]);
	wait_until(
		|| block_on(cards.face(product.clone(), card())).unwrap() == Some(newest.clone()),
		"the newest drawn face is kept",
	);
}

/// Only a card's face is kept. A context that draws anything else, the card
/// back no host sends any more included, names no card.
#[test]
fn only_a_pocket_card_face_names_a_card() {
	assert_eq!(pocket_card(&RenderContext::PocketCardBack { card_id: "stamps".into() }), None);
	assert_eq!(pocket_card(&RenderContext::InputWidget { candidate_id: "stamps".into() }), None);
}
