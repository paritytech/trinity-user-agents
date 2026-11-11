//! A device whose Bulletin the test answers, shared by the materials tests.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

use futures::{
	StreamExt,
	channel::mpsc::{UnboundedSender, unbounded},
	stream::BoxStream,
};
use truapi::latest::GenericError;

use super::{FETCH_DEADLINE, Materials};
use crate::{
	host_internal::bulletin::preimage_key,
	platform::PreimageHost,
	spawner::Spawner,
	store::Db,
	test_support::{next, wait_until},
	time::ManualTimer,
};

type Answer = Result<Option<Vec<u8>>, GenericError>;

/// Bulletin as the test answers it: each lookup waits until the test pushes an
/// answer for its ID, as the real lookup waits on a miss.
#[derive(Default)]
pub struct FakeBulletin {
	lookups: Mutex<HashMap<[u8; 32], Vec<UnboundedSender<Answer>>>>,
}

impl FakeBulletin {
	/// Answers every open lookup of `id` with `value`.
	pub fn answer(&self, id: [u8; 32], value: Vec<u8>) {
		for lookup in self.lookups.lock().unwrap().entry(id).or_default() {
			let _ = lookup.unbounded_send(Ok(Some(value.clone())));
		}
	}

	/// Ends every lookup of `id`, as a lookup that can never answer does.
	pub fn end(&self, id: [u8; 32]) {
		self.lookups.lock().unwrap().entry(id).or_default().clear();
	}

	/// Lookups of `id` ever started.
	pub fn lookups(&self, id: [u8; 32]) -> usize {
		self.lookups.lock().unwrap().get(&id).map_or(0, Vec::len)
	}

	/// Lookups of `id` still open.
	pub fn open_lookups(&self, id: [u8; 32]) -> usize {
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

/// The materials of a device over `db`, its Bulletin and its clock.
pub struct Device {
	pub materials: Materials,
	pub bulletin: Arc<FakeBulletin>,
	pub timer: Arc<ManualTimer>,
	pub db: Db,
}

impl Device {
	/// A device over `db`.
	pub fn over(db: Db) -> Self {
		Self::with_spawner(db, Spawner::for_test())
	}

	/// A device over `db` that runs its tasks on `spawner`.
	pub fn with_spawner(db: Db, spawner: Spawner) -> Self {
		let bulletin = Arc::new(FakeBulletin::default());
		let timer = Arc::new(ManualTimer::default());
		let materials = Materials::new(db.clone(), bulletin.clone(), timer.clone(), spawner);
		Self { materials, bulletin, timer, db }
	}

	/// Fetches `text` through Bulletin and waits until it is kept.
	pub fn keep(&self, text: &[u8]) {
		let id = preimage_key(text);
		let mut answers = self.materials.observe(id);
		wait_until(|| self.bulletin.lookups(id) == 1, "the material is looked up");
		self.bulletin.answer(id, text.to_vec());
		assert_eq!(next(&mut answers), Some(Ok(text.to_vec())));
	}

	/// Moves the clock past the fetch deadline, once a deadline is set.
	pub fn pass_the_deadline(&self) {
		wait_until(|| self.timer.sleeps() > 0, "the deadline is set");
		self.timer.advance(FETCH_DEADLINE);
	}
}
