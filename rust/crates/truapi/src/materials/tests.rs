use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

use futures::{
	FutureExt, StreamExt,
	channel::mpsc::{UnboundedSender, unbounded},
	executor::block_on,
	stream::BoxStream,
};
use truapi::latest::{
	BoxProps, GenericError, MaterialFailure, MaterialInstance, MaterialsProps, RendererNode,
};

use super::*;
use crate::{
	host_internal::bulletin::preimage_key,
	platform::{PreimageHost, ProductId},
	pocket_cards::PocketCards,
	store::{Db, open_temporary_core_db},
	test_support::{test_spawner, wait_until},
	time::ManualTimer,
};

type Answer = Result<Option<Vec<u8>>, GenericError>;

/// Bulletin as the test answers it: each lookup waits until the test pushes an
/// answer for its ID, as the real lookup waits on a miss.
#[derive(Default)]
struct FakeBulletin {
	lookups: Mutex<HashMap<[u8; 32], Vec<UnboundedSender<Answer>>>>,
}

impl FakeBulletin {
	fn answer(&self, id: [u8; 32], value: Vec<u8>) {
		for lookup in self.lookups.lock().unwrap().entry(id).or_default() {
			let _ = lookup.unbounded_send(Ok(Some(value.clone())));
		}
	}

	fn end(&self, id: [u8; 32]) {
		self.lookups.lock().unwrap().entry(id).or_default().clear();
	}

	fn lookups(&self, id: [u8; 32]) -> usize {
		self.lookups.lock().unwrap().get(&id).map_or(0, Vec::len)
	}

	fn open_lookups(&self, id: [u8; 32]) -> usize {
		let lookups = self.lookups.lock().unwrap();
		lookups
			.get(&id)
			.map_or(0, |open| open.iter().filter(|lookup| !lookup.is_closed()).count())
	}
}

impl PreimageHost for FakeBulletin {
	fn lookup_preimage(&self, key: [u8; 32]) -> BoxStream<'static, Answer> {
		let (answers, lookup) = unbounded();
		self.lookups.lock().unwrap().entry(key).or_default().push(answers);
		lookup.boxed()
	}
}

struct Device {
	materials: Materials,
	bulletin: Arc<FakeBulletin>,
	timer: Arc<ManualTimer>,
	db: Db,
}

impl Device {
	fn over(db: Db) -> Self {
		let bulletin = Arc::new(FakeBulletin::default());
		let timer = Arc::new(ManualTimer::default());
		let materials = Materials::new(db.clone(), bulletin.clone(), timer.clone(), test_spawner());
		Self { materials, bulletin, timer, db }
	}

	/// Fetches `text` through Bulletin and waits until it is kept.
	fn keep(&self, text: &[u8]) {
		let id = preimage_key(text);
		let mut answers = self.materials.observe(id);
		wait_until(|| self.bulletin.lookups(id) == 1, "the material is looked up");
		self.bulletin.answer(id, text.to_vec());
		assert_eq!(next(&mut answers), Some(Ok(text.to_vec())));
	}

	fn kept_ids(&self) -> Vec<[u8; 32]> {
		block_on(self.db.read(|conn| {
			let mut ids = conn.prepare("SELECT id FROM material ORDER BY rowid")?;
			let ids = ids.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
			Ok(ids)
		}))
		.unwrap()
	}

	/// Keeps a Pocket card whose saved face names `materials` inside a box.
	fn save_face(&self, card_id: &str, materials: &[[u8; 32]]) {
		let layers = materials
			.iter()
			.map(|material| MaterialInstance { material: *material, args: Vec::new() })
			.collect();
		let face = RendererNode::Box {
			modifiers: Vec::new(),
			props: BoxProps { content_alignment: None },
			children: vec![RendererNode::Materials {
				props: MaterialsProps { layers },
				children: Vec::new(),
			}],
		};
		let cards = block_on(PocketCards::open(self.db.clone(), Vec::new())).unwrap();
		let product = ProductId::for_test("loyalty.dot");
		block_on(cards.add(product, card_id.to_string(), "Card".to_string(), face)).unwrap();
	}

	fn pass_the_deadline(&self, sleeps_before: usize) {
		wait_until(|| self.timer.sleeps() > sleeps_before, "the deadline is set");
		self.timer.advance(FETCH_DEADLINE);
	}
}

/// The next answer, failing the test rather than hanging when none comes.
fn next(
	answers: &mut BoxStream<'static, Result<Vec<u8>, MaterialFailure>>,
) -> Option<Result<Vec<u8>, MaterialFailure>> {
	let timeout = futures_timer::Delay::new(std::time::Duration::from_secs(2));
	match block_on(futures::future::select(answers.next(), timeout)) {
		futures::future::Either::Left((answer, _)) => answer,
		futures::future::Either::Right(_) => panic!("no answer came"),
	}
}

const TEXT: &[u8] = b"// @material 1\nvec4 material(vec2 uv) { return vec4(uv, 0.0, 1.0); }\n";

/// A card keeps its look offline: once fetched, a material reads from the
/// device after a restart, with Bulletin never asked.
#[test]
fn a_material_fetched_once_is_ready_after_a_restart_without_bulletin() {
	let (_dir, db) = open_temporary_core_db();
	Device::over(db.clone()).keep(TEXT);

	let restarted = Device::over(db);
	let mut answers = restarted.materials.observe(preimage_key(TEXT));

	assert_eq!(next(&mut answers), Some(Ok(TEXT.to_vec())));
	assert_eq!(next(&mut answers), None);
	assert_eq!(restarted.bulletin.lookups(preimage_key(TEXT)), 0);
}

/// A caller that comes after the material is kept reads it from the device
/// rather than asking Bulletin again.
#[test]
fn a_kept_material_is_not_looked_up_again() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	device.keep(TEXT);

	let mut answers = device.materials.observe(preimage_key(TEXT));

	assert_eq!(next(&mut answers), Some(Ok(TEXT.to_vec())));
	assert_eq!(device.bulletin.lookups(preimage_key(TEXT)), 1);
}

/// A node waits while its material loads, falls back once the deadline
/// passes, and still draws when the material arrives late, so a card heals by
/// itself.
#[test]
fn nothing_by_the_deadline_is_not_found_and_a_late_arrival_still_draws() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let id = preimage_key(TEXT);
	let mut answers = device.materials.observe(id);
	wait_until(|| device.bulletin.lookups(id) == 1, "the material is looked up");

	assert_eq!(answers.next().now_or_never(), None, "nothing is reported while loading");

	device.pass_the_deadline(0);
	assert_eq!(next(&mut answers), Some(Err(MaterialFailure::NotFound)));
	let mut later = device.materials.observe(id);
	assert_eq!(
		next(&mut later),
		Some(Err(MaterialFailure::NotFound)),
		"a later node falls back too"
	);

	device.bulletin.answer(id, TEXT.to_vec());
	assert_eq!(next(&mut answers), Some(Ok(TEXT.to_vec())));
	assert_eq!(next(&mut later), Some(Ok(TEXT.to_vec())));
	assert_eq!(next(&mut answers), None);
}

/// Bytes that do not hash to the ID are never drawn, whatever the lookup
/// hands over.
#[test]
fn bytes_that_do_not_hash_to_the_id_are_never_drawn() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let id = preimage_key(TEXT);
	let mut answers = device.materials.observe(id);
	wait_until(|| device.bulletin.lookups(id) == 1, "the material is looked up");

	device.bulletin.answer(id, b"forged".to_vec());
	device.pass_the_deadline(0);
	assert_eq!(next(&mut answers), Some(Err(MaterialFailure::NotFound)));

	device.bulletin.answer(id, TEXT.to_vec());
	assert_eq!(next(&mut answers), Some(Ok(TEXT.to_vec())));
}

/// A node that fell back at the deadline is told once: a lookup that then
/// gives up for good adds no second report.
#[test]
fn a_lookup_that_ends_after_the_deadline_reports_once() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let id = preimage_key(TEXT);
	let mut answers = device.materials.observe(id);
	wait_until(|| device.bulletin.lookups(id) == 1, "the material is looked up");
	device.pass_the_deadline(0);
	assert_eq!(next(&mut answers), Some(Err(MaterialFailure::NotFound)));

	device.bulletin.end(id);

	assert_eq!(next(&mut answers), None);
}

/// A face naming one material in many places, or open in two places at once,
/// costs Bulletin one lookup.
#[test]
fn callers_of_one_material_share_one_lookup() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let id = preimage_key(TEXT);
	let mut first = device.materials.observe(id);
	let mut second = device.materials.observe(id);
	wait_until(|| device.bulletin.lookups(id) == 1, "the material is looked up");

	device.bulletin.answer(id, TEXT.to_vec());

	assert_eq!(next(&mut first), Some(Ok(TEXT.to_vec())));
	assert_eq!(next(&mut second), Some(Ok(TEXT.to_vec())));
	assert_eq!(device.bulletin.lookups(id), 1);
}

/// A material nobody waits for anymore is not asked for again and again for
/// the rest of the run. The next caller starts over, and is answered.
#[test]
fn the_lookup_stops_when_every_caller_is_gone() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let id = preimage_key(TEXT);
	let first = device.materials.observe(id);
	let second = device.materials.observe(id);
	wait_until(|| device.bulletin.open_lookups(id) == 1, "the material is looked up");

	drop(first);
	assert_eq!(device.bulletin.open_lookups(id), 1, "a caller still waits");
	drop(second);
	wait_until(|| device.bulletin.open_lookups(id) == 0, "the lookup is dropped");

	let mut answers = device.materials.observe(id);
	wait_until(|| device.bulletin.open_lookups(id) == 1, "a new lookup starts");
	device.bulletin.answer(id, TEXT.to_vec());
	assert_eq!(next(&mut answers), Some(Ok(TEXT.to_vec())));
}

/// An ID is product input, so a product could name any blob on Bulletin. One
/// larger than the format allows is refused and never fills the device.
#[test]
fn a_material_over_the_format_limit_is_invalid_and_not_kept() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let oversized = vec![b' '; MAX_MATERIAL_BYTES + 1];
	let id = preimage_key(&oversized);
	let mut answers = device.materials.observe(id);
	wait_until(|| device.bulletin.lookups(id) == 1, "the material is looked up");

	device.bulletin.answer(id, oversized);

	assert_eq!(next(&mut answers), Some(Err(MaterialFailure::InvalidMaterial)));
	assert_eq!(next(&mut answers), None);
	assert!(device.kept_ids().is_empty());
}

/// A lookup that can never answer, such as a node without the lookup method,
/// fails the node at once rather than after the deadline.
#[test]
fn a_lookup_that_ends_is_not_found_at_once() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let id = preimage_key(TEXT);
	let mut answers = device.materials.observe(id);
	wait_until(|| device.bulletin.lookups(id) == 1, "the material is looked up");

	device.bulletin.end(id);

	assert_eq!(next(&mut answers), Some(Err(MaterialFailure::NotFound)));
	assert_eq!(next(&mut answers), None);
}

/// A material of the largest size the format allows, distinct per `index`.
fn largest_material(index: usize) -> Vec<u8> {
	let mut text = format!("// {index}\n").into_bytes();
	text.resize(MAX_MATERIAL_BYTES, b' ');
	text
}

/// Materials only other screens drew make room for newer ones, oldest first,
/// but a material a saved card face draws stays, so the Pocket keeps its look
/// offline however much else was drawn. Pinned materials do not take room
/// from the others: 65 unpinned ones of the largest size are one over the cap.
#[test]
fn unpinned_materials_are_evicted_oldest_first_and_pinned_ones_stay() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let materials: Vec<_> = (0..67).map(largest_material).collect();
	let ids: Vec<_> = materials.iter().map(|text| preimage_key(text)).collect();
	device.save_face("stamps", &[ids[0], ids[66]]);

	for text in &materials {
		device.keep(text);
	}

	let mut expected = ids.clone();
	expected.remove(1);
	assert_eq!(device.kept_ids(), expected);
}

/// Materials the saved faces draw are fetched at start-up, before the Pocket
/// shows, and only those the device lacks.
#[test]
fn the_start_up_fetch_gets_what_saved_faces_name_and_the_device_lacks() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let kept = TEXT.to_vec();
	let missing = b"// @material 1\n// missing\n".to_vec();
	device.keep(&kept);
	device.save_face("stamps", &[preimage_key(&kept), preimage_key(&missing)]);

	(test_spawner())(device.materials.clone().fetch_saved().boxed());
	wait_until(
		|| device.bulletin.lookups(preimage_key(&missing)) == 1,
		"the missing one is looked up",
	);
	device.bulletin.answer(preimage_key(&missing), missing.clone());

	wait_until(|| device.kept_ids().contains(&preimage_key(&missing)), "it is kept");
	assert_eq!(
		device.bulletin.lookups(preimage_key(&kept)),
		1,
		"the kept one is not looked up again"
	);
}

/// A material that has expired from Bulletin is not asked for at every start
/// for the rest of the run: the start-up fetch gives up at the deadline.
#[test]
fn the_start_up_fetch_gives_up_at_the_deadline() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let id = preimage_key(TEXT);
	device.save_face("stamps", &[id]);

	(test_spawner())(device.materials.clone().fetch_saved().boxed());
	wait_until(|| device.bulletin.open_lookups(id) == 1, "the material is looked up");
	device.pass_the_deadline(0);

	wait_until(|| device.bulletin.open_lookups(id) == 0, "the lookup is dropped");
}
