use std::sync::{Arc, Mutex};

use futures::{
	FutureExt, StreamExt,
	executor::{LocalPool, block_on},
	future::BoxFuture,
	task::SpawnExt,
};
use truapi::latest::{BoxProps, MaterialFailure, MaterialInstance, MaterialsProps, RendererNode};

use super::{testing::Device, *};
use crate::{
	host_internal::bulletin::preimage_key,
	platform::ProductId,
	pocket_cards::PocketCards,
	spawner::Spawner,
	store::open_temporary_core_db,
	test_support::{next, wait_until},
};

impl Device {
	fn kept_ids(&self) -> Vec<[u8; 32]> {
		block_on(self.db.read(|conn| {
			let mut ids = conn.prepare("SELECT id FROM material ORDER BY rowid")?;
			let ids = ids.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
			Ok(ids)
		}))
		.unwrap()
	}

	/// Keeps a Pocket card whose saved face names `materials` inside a box.
	fn save_face(&self, materials: &[[u8; 32]]) {
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
		block_on(cards.add(product, "stamps".to_string(), "Card".to_string(), face)).unwrap();
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

/// A node waits while its material loads, falls back once the deadline
/// passes, and still draws when the material arrives late, so a card heals by
/// itself. Bytes that do not hash to the ID never draw, whatever the lookup
/// hands over.
#[test]
fn nothing_by_the_deadline_is_not_found_and_a_late_arrival_still_draws() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let id = preimage_key(TEXT);
	let mut answers = device.materials.observe(id);
	wait_until(|| device.bulletin.lookups(id) == 1, "the material is looked up");

	assert_eq!(answers.next().now_or_never(), None, "nothing is reported while loading");

	device.bulletin.answer(id, b"forged".to_vec());
	device.pass_the_deadline();
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

/// A node that fell back at the deadline is told once: a lookup that then
/// gives up for good adds no second report.
#[test]
fn a_lookup_that_ends_after_the_deadline_reports_once() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let id = preimage_key(TEXT);
	let mut answers = device.materials.observe(id);
	wait_until(|| device.bulletin.lookups(id) == 1, "the material is looked up");
	device.pass_the_deadline();
	assert_eq!(next(&mut answers), Some(Err(MaterialFailure::NotFound)));

	device.bulletin.end(id);

	assert_eq!(next(&mut answers), None);
}

/// A material that arrives just as the deadline passes draws, with no
/// fallback first: an answer already in hand beats the deadline. The test runs
/// the fetch itself, so both are ready before the fetch looks at either.
#[test]
fn an_answer_ready_with_the_deadline_draws_without_falling_back() {
	let queued: Arc<Mutex<Vec<BoxFuture<'static, ()>>>> = Arc::default();
	let queue = Arc::clone(&queued);
	let spawner = Spawner::with_test_scheduler(move |task| queue.lock().unwrap().push(task));
	let (_dir, db) = open_temporary_core_db();
	let device = Device::with_spawner(db, spawner);
	let id = preimage_key(TEXT);
	let mut pool = LocalPool::new();
	let mut answers = device.materials.observe(id);
	for task in queued.lock().unwrap().drain(..) {
		pool.spawner().spawn(task).unwrap();
	}
	wait_until(
		|| {
			pool.run_until_stalled();
			device.bulletin.lookups(id) == 1 && device.timer.sleeps() == 1
		},
		"the fetch waits on the lookup and the deadline",
	);

	device.bulletin.answer(id, TEXT.to_vec());
	device.timer.advance(FETCH_DEADLINE);

	assert_eq!(pool.run_until(answers.next()), Some(Ok(TEXT.to_vec())));
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

/// An ID is product input, so a product could name any blob on Bulletin.
/// Only materials this core reads are kept: a broken one never fills the
/// device, and one of a newer format is reported as unsupported.
#[test]
fn a_material_this_core_cannot_read_is_refused_and_not_kept() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let newer = b"// @material 2\nvec4 material(vec2 uv) { return vec4(1.0); }\n".to_vec();
	for (text, failure) in [
		(vec![b' '; MAX_MATERIAL_BYTES + 1], MaterialFailure::InvalidMaterial),
		(newer, MaterialFailure::Unsupported),
	] {
		let id = preimage_key(&text);
		let mut answers = device.materials.observe(id);
		wait_until(|| device.bulletin.lookups(id) == 1, "the material is looked up");

		device.bulletin.answer(id, text);

		assert_eq!(next(&mut answers), Some(Err(failure)));
		assert_eq!(next(&mut answers), None);
	}
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

/// A material of the largest size the format allows, distinct per `index`,
/// padded through its label, which the format does not check.
fn largest_material(index: usize) -> Vec<u8> {
	let head = "// @material 1\n// @control {\"name\":\"k\",\"label\":\"";
	let tail = format!(
		"\",\"type\":\"float\",\"default\":{index}}}\nvec4 material(vec2 uv) {{ return vec4(p_k); }}\n"
	);
	let label = "x".repeat(MAX_MATERIAL_BYTES - head.len() - tail.len());
	format!("{head}{label}{tail}").into_bytes()
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
	device.save_face(&[ids[0], ids[66]]);

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
	let missing = b"// @material 1\nvec4 material(vec2 uv) { return vec4(0.0); }\n".to_vec();
	device.keep(&kept);
	device.save_face(&[preimage_key(&kept), preimage_key(&missing)]);

	Spawner::for_test().spawn(device.materials.clone().fetch_saved());
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
	device.save_face(&[id]);

	Spawner::for_test().spawn(device.materials.clone().fetch_saved());
	wait_until(|| device.bulletin.open_lookups(id) == 1, "the material is looked up");
	device.pass_the_deadline();

	wait_until(|| device.bulletin.open_lookups(id) == 0, "the lookup is dropped");
}

/// A product asks before sending materials, so the answer is yes only for what
/// this host reads, and never with materials off.
#[test]
fn a_host_draws_only_the_format_it_reads_and_only_when_on() {
	let features = |names: &[&str]| names.iter().map(|name| name.to_string()).collect::<Vec<_>>();

	assert!(draws(true, 1, &[]));
	assert!(!draws(false, 1, &[]), "materials off");
	assert!(!draws(true, 2, &[]), "a newer format version");
	assert!(!draws(true, 1, &features(&["sl-pressure"])), "a feature version 1 does not define");
}
