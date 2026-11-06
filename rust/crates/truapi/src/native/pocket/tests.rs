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
fn a_pocket_card_render_draws_each_face_and_keeps_it_on_its_side() {
	// The host draws what the product drew, and the card keeps it to show
	// offline and at the next start, on the side the render was for.
	let (_dir, db) = open_temporary_core_db();
	let cards = block_on(PocketCards::open(db, Vec::new())).unwrap();
	let product = ProductId::for_test("loyalty.dot");
	let card = || "stamps".to_string();
	block_on(cards.add(product.clone(), card(), "Stamps".into(), RendererNode::Nil)).unwrap();
	let drawn = Arc::new(Drawn::default());
	let render = |context| {
		let (card_id, side) = card_side(&context).expect("a Pocket card context names a side");
		keeping_faces(cards.clone(), product.clone(), card_id, side, drawn.clone(), &test_spawner())
	};
	let front = RendererNode::String { text: "Stamps: 3".into() };
	let back = RendererNode::String { text: "Code".into() };

	render(RenderContext::PocketCard { card_id: card() }).on_update(front.clone());
	render(RenderContext::PocketCardBack { card_id: card() }).on_update(back.clone());

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
