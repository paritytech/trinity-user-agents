use std::sync::Arc;

use futures::{StreamExt, channel::mpsc::unbounded, executor::block_on};
use tempfile::TempDir;

use super::*;
use crate::{
	store::open_temporary_core_db,
	test_support::{test_spawner, wait_until},
	time::ManualTimer,
	unix_time::current_unix_millis,
};

const LOYALTY: &str = "loyalty.dot";
const PERSONHOOD: &str = "personhood.dot";

fn product(id: &str) -> ProductId {
	ProductId::for_test(id)
}

fn humanity() -> PinnedPocketCard {
	PinnedPocketCard {
		product_id: product(PERSONHOOD),
		card_id: "humanity".into(),
		title: "Humanity".into(),
	}
}

fn face(text: &str) -> RendererNode {
	RendererNode::String { text: text.into() }
}

fn entry(product_id: &str, card_id: &str, privileged: bool) -> (String, String, bool) {
	(product_id.into(), card_id.into(), privileged)
}

fn open(pinned: Vec<PinnedPocketCard>) -> (TempDir, PocketCards) {
	let (dir, db) = open_temporary_core_db();
	(dir, block_on(PocketCards::open(db, pinned)).unwrap())
}

fn reopen(cards: &PocketCards, pinned: Vec<PinnedPocketCard>) -> PocketCards {
	block_on(PocketCards::open(cards.db.clone(), pinned)).unwrap()
}

fn add(cards: &PocketCards, product_id: &str, card_id: &str, text: &str) {
	block_on(cards.add(product(product_id), card_id.into(), card_id.into(), face(text))).unwrap();
}

fn listed(cards: &PocketCards) -> Vec<(String, String, bool)> {
	let current = block_on(cards.observe().next()).unwrap().unwrap();
	current
		.into_iter()
		.map(|card| (card.product_id, card.card_id, card.privileged))
		.collect()
}

fn keep(cards: &PocketCards, product_id: &str, card_id: &str, text: &str, drawn_at: u64) {
	block_on(cards.keep_face(product(product_id), card_id.into(), face(text), drawn_at)).unwrap();
}

fn kept_face(cards: &PocketCards, product_id: &str, card_id: &str) -> Option<RendererNode> {
	block_on(cards.face(product(product_id), card_id.into())).unwrap()
}

#[test]
fn pinned_cards_come_first_then_added_cards_in_the_order_they_were_added() {
	// The Wallet tab draws the list as given, so the order is the user's.
	let (_dir, cards) = open(vec![humanity()]);
	add(&cards, LOYALTY, "stamps", "stamps");
	add(&cards, LOYALTY, "coffee", "coffee");

	assert_eq!(
		listed(&cards),
		vec![
			entry(PERSONHOOD, "humanity", true),
			entry(LOYALTY, "stamps", false),
			entry(LOYALTY, "coffee", false),
		]
	);
}

#[test]
fn removal_answers_what_it_did_and_takes_the_face_with_the_card() {
	// A product is told whether its card left; a card added again must not
	// show the face the old one was approved by.
	let (_dir, cards) = open(vec![humanity()]);
	add(&cards, LOYALTY, "stamps", "old face");

	let remove = |card_id: &str, product_id: &str| {
		block_on(cards.remove(product(product_id), card_id.into())).unwrap()
	};
	assert_eq!(remove("stamps", LOYALTY), PocketCardRemoval::Removed);
	assert_eq!(remove("stamps", LOYALTY), PocketCardRemoval::Absent);
	assert_eq!(remove("humanity", PERSONHOOD), PocketCardRemoval::Privileged);
	assert_eq!(kept_face(&cards, LOYALTY, "stamps"), None);
	assert_eq!(listed(&cards), vec![entry(PERSONHOOD, "humanity", true)]);
}

#[test]
fn a_card_the_user_added_survives_being_pinned_and_unpinned() {
	// The host can start pinning a card the user already holds, and stop
	// again in a later release. The user's card must not go with the pin.
	let pinned_stamps = PinnedPocketCard {
		product_id: product(LOYALTY),
		card_id: "stamps".into(),
		title: "Stamps".into(),
	};
	let (_dir, cards) = open(Vec::new());
	add(&cards, LOYALTY, "stamps", "stamps");

	let cards = reopen(&cards, vec![pinned_stamps]);
	assert_eq!(listed(&cards), vec![entry(LOYALTY, "stamps", true)]);

	let cards = reopen(&cards, Vec::new());
	assert_eq!(listed(&cards), vec![entry(LOYALTY, "stamps", false)]);
	assert_eq!(kept_face(&cards, LOYALTY, "stamps"), Some(face("stamps")));
}

#[test]
fn a_pinned_card_the_user_never_added_leaves_with_its_face_once_unpinned() {
	let (_dir, cards) = open(vec![humanity()]);
	keep(&cards, PERSONHOOD, "humanity", "drawn", 1);

	let cards = reopen(&cards, Vec::new());

	assert_eq!(listed(&cards), Vec::new());
	assert_eq!(kept_face(&cards, PERSONHOOD, "humanity"), None);
}

#[test]
fn a_pinned_card_keeps_its_last_face_across_starts() {
	// The pins are written on every start; that must not cost Humanity the
	// face its product last drew.
	let (_dir, cards) = open(vec![humanity()]);
	keep(&cards, PERSONHOOD, "humanity", "drawn", 1);

	let cards = reopen(&cards, vec![humanity()]);

	assert_eq!(kept_face(&cards, PERSONHOOD, "humanity"), Some(face("drawn")));
}

#[test]
fn adding_a_pinned_card_keeps_it_pinned_under_the_host_title() {
	let (_dir, cards) = open(vec![humanity()]);
	add(&cards, PERSONHOOD, "humanity", "approved");

	assert_eq!(listed(&cards), vec![entry(PERSONHOOD, "humanity", true)]);
	let titles: Vec<String> = block_on(cards.observe().next())
		.unwrap()
		.unwrap()
		.into_iter()
		.map(|card| card.title)
		.collect();
	assert_eq!(titles, vec!["Humanity".to_string()]);
	assert_eq!(
		block_on(cards.remove(product(PERSONHOOD), "humanity".into())).unwrap(),
		PocketCardRemoval::Privileged
	);
}

#[test]
fn a_face_is_kept_only_for_a_card_in_the_pocket() {
	// A render can outlive its card, and nothing may bring a removed card's
	// face back.
	let (_dir, cards) = open(Vec::new());

	keep(&cards, LOYALTY, "stamps", "drawn", 1);

	assert_eq!(kept_face(&cards, LOYALTY, "stamps"), None);
}

#[test]
fn an_older_face_never_replaces_a_newer_one() {
	// The Pocket list and the opened card can render one card at once, and a
	// render that stops writes its last face late.
	let (_dir, cards) = open(Vec::new());
	add(&cards, LOYALTY, "stamps", "approved");
	let approved_at = current_unix_millis();

	keep(&cards, LOYALTY, "stamps", "stale", approved_at - 1_000);
	assert_eq!(kept_face(&cards, LOYALTY, "stamps"), Some(face("approved")));

	keep(&cards, LOYALTY, "stamps", "newer", approved_at + 1_000);
	assert_eq!(kept_face(&cards, LOYALTY, "stamps"), Some(face("newer")));
}

#[test]
fn a_kept_face_reads_back_as_the_product_drew_it() {
	// The card draws at a cold start exactly as its product last drew it.
	let json = std::fs::read_to_string(format!(
		"{}/tests/fixtures/pocket_faces/devicehood.json",
		env!("CARGO_MANIFEST_DIR")
	))
	.unwrap();
	let devicehood = crate::native::parse_renderer_node_json(json).unwrap();
	let (_dir, cards) = open(Vec::new());

	block_on(cards.add(product(LOYALTY), "stamps".into(), "Stamps".into(), devicehood.clone()))
		.unwrap();

	assert_eq!(kept_face(&cards, LOYALTY, "stamps"), Some(devicehood));
}

#[test]
fn a_kept_face_with_bytes_past_its_tree_reads_as_none() {
	// Drawing the prefix of a row that is not a face the core wrote would
	// show a card nobody drew.
	let (_dir, cards) = open(Vec::new());
	add(&cards, LOYALTY, "stamps", "stamps");
	block_on(cards.db.write(|tx| {
		tx.execute("UPDATE pocket_card_face SET face = CAST(face || x'00' AS BLOB)", [])?;
		Ok(())
	}))
	.unwrap();

	assert_eq!(kept_face(&cards, LOYALTY, "stamps"), None);
}

#[test]
fn a_face_that_no_longer_decodes_reads_as_none() {
	// The card keeps its place and waits for its product to draw again.
	let (_dir, cards) = open(Vec::new());
	add(&cards, LOYALTY, "stamps", "stamps");
	block_on(cards.db.write(|tx| {
		tx.execute("UPDATE pocket_card_face SET face = x'ff'", [])?;
		Ok(())
	}))
	.unwrap();

	assert_eq!(kept_face(&cards, LOYALTY, "stamps"), None);
}

#[test]
fn a_product_sees_only_its_own_cards_and_their_removal() {
	let (_dir, cards) = open(vec![humanity()]);
	add(&cards, LOYALTY, "stamps", "stamps");
	let loyalty = ProductContext::new(product(LOYALTY));
	let mut stream = cards.subscribe_pocket_cards(&loyalty);
	let card = |card_id: &str| PocketCard { card_id: card_id.into(), privileged: false };

	assert_eq!(block_on(stream.next()).unwrap().unwrap().cards, vec![card("stamps")]);

	let removal = HostPocketRemoveCardRequest { card_id: "stamps".into() };
	block_on(cards.remove_pocket_card(&loyalty, removal)).unwrap();

	assert_eq!(block_on(stream.next()).unwrap().unwrap().cards, Vec::new());
}

#[test]
fn a_product_cannot_remove_a_pinned_card() {
	let (_dir, cards) = open(vec![humanity()]);
	let personhood = ProductContext::new(product(PERSONHOOD));

	let removal = HostPocketRemoveCardRequest { card_id: "humanity".into() };

	assert_eq!(
		block_on(cards.remove_pocket_card(&personhood, removal)),
		Err(HostPocketRemoveCardError::Privileged)
	);
}

#[test]
fn the_newest_face_is_kept_at_most_every_interval_and_at_once_when_drawing_stops() {
	// Faces arrive at frame rate. Writing each one would keep the database
	// busy for nothing, and the last one must be kept as soon as the card
	// scrolls away, since the app may be stopped right after.
	let (_dir, cards) = open(Vec::new());
	add(&cards, LOYALTY, "stamps", "approved");
	let timer = Arc::new(ManualTimer::default());
	let (faces, drawn) = unbounded();
	let keeper = cards.clone();
	let keeper_timer = timer.clone();
	test_spawner()(Box::pin(async move {
		keeper.keep_faces(product(LOYALTY), "stamps".into(), drawn, keeper_timer).await;
	}));
	let kept = || kept_face(&cards, LOYALTY, "stamps");
	let drawn = |text: &str| (face(text), current_unix_millis());

	faces.unbounded_send(drawn("1")).unwrap();
	wait_until(|| kept() == Some(face("1")), "the first face is kept at once");
	wait_until(|| timer.sleeps() == 1, "the keeper waits after a write");

	faces.unbounded_send(drawn("2")).unwrap();
	faces.unbounded_send(drawn("3")).unwrap();
	assert_eq!(kept(), Some(face("1")), "nothing is written within the interval");
	timer.advance(FACE_WRITE_INTERVAL);
	wait_until(|| kept() == Some(face("3")), "the newest face is kept once the interval ends");
	wait_until(|| timer.sleeps() == 2, "the keeper waits again");

	faces.unbounded_send(drawn("4")).unwrap();
	drop(faces);
	wait_until(|| kept() == Some(face("4")), "the last face is kept as soon as drawing stops");
}
