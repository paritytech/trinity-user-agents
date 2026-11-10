//! Materials on the device: the text behind a material ID, read from the core
//! database, or fetched from Bulletin and kept there.
//!
//! Every caller of one ID shares one fetch, which stops once no caller is
//! left. Only bytes that hash to the ID count: the core's Bulletin lookup
//! already reads others as a miss. Once [`FETCH_DEADLINE`] passes without
//! them, the fetch reports `NotFound` and goes on, so a material that arrives
//! late still draws. Only materials this core reads are kept.
//! A material a saved Pocket face draws is never evicted. The others share
//! [`UNPINNED_BYTES`], and the oldest kept goes first.

use core::{
	pin::Pin,
	task::{Context, Poll},
	time::Duration,
};
use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
};

use futures::{
	FutureExt, Stream, StreamExt,
	channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
	future::{AbortHandle, Abortable, join_all},
	select_biased,
	stream::BoxStream,
};
use parking_lot::Mutex;
use rusqlite::{OptionalExtension, Transaction};
use truapi::latest::{MaterialFailure, MaterialsProps, RendererNode};

use self::format::read_material;
use crate::{
	host_internal::bulletin::preimage_key,
	platform::PreimageHost,
	pocket_cards::saved_faces,
	spawner::Spawner,
	store::{Db, DbError},
	time::Timer,
};

/// Largest material the format allows.
const MAX_MATERIAL_BYTES: usize = 64 * 1024;

/// How long a node waits for its material before it falls back.
const FETCH_DEADLINE: Duration = Duration::from_secs(30);

/// Room for the materials no saved Pocket face draws.
const UNPINNED_BYTES: usize = 4 * 1024 * 1024;

/// The materials on the device, and the fetches of those that are not yet.
#[derive(Clone)]
pub struct Materials(Arc<Shared>);

struct Shared {
	db: Db,
	lookup: Arc<dyn PreimageHost>,
	timer: Arc<dyn Timer>,
	spawner: Spawner,
	fetches: Mutex<HashMap<[u8; 32], Fetch>>,
}

/// One fetch in flight, and the callers it answers.
struct Fetch {
	callers: Vec<UnboundedSender<Result<Vec<u8>, MaterialFailure>>>,
	/// What the deadline reported, for a caller who joins after it.
	failure: Option<MaterialFailure>,
	abort: AbortHandle,
}

impl Materials {
	/// Materials kept in `db`, fetched through `lookup`.
	pub fn new(
		db: Db,
		lookup: Arc<dyn PreimageHost>,
		timer: Arc<dyn Timer>,
		spawner: Spawner,
	) -> Self {
		Self(Arc::new(Shared { db, lookup, timer, spawner, fetches: Mutex::default() }))
	}

	/// The text of material `id` once it is on the device, then the end.
	/// Before that, the failure the deadline reported, after which the text
	/// can still come. A material this core cannot read (`InvalidMaterial` or
	/// `Unsupported`), or a lookup that can never answer, ends the stream.
	pub fn observe(&self, id: [u8; 32]) -> BoxStream<'static, Result<Vec<u8>, MaterialFailure>> {
		let (caller, answers) = unbounded();
		let started = {
			let mut fetches = self.0.fetches.lock();
			match fetches.get_mut(&id) {
				Some(fetch) => {
					if let Some(failure) = fetch.failure {
						let _ = caller.unbounded_send(Err(failure));
					}
					fetch.callers.push(caller);
					None
				},
				None => {
					let (abort, registration) = AbortHandle::new_pair();
					fetches.insert(
						id,
						Fetch { callers: vec![caller], failure: None, abort: abort.clone() },
					);
					Some((abort, registration))
				},
			}
		};
		if let Some((abort, registration)) = started {
			let fetch = Abortable::new(self.clone().fetch(id, abort), registration);
			self.0.spawner.spawn(fetch.map(drop));
		}
		Caller { answers, id, materials: self.clone() }.boxed()
	}

	/// Fetches what the saved Pocket faces draw and the device lacks, so the
	/// Pocket has it before it shows. Gives up on each at the deadline.
	pub async fn fetch_saved(self) {
		let faces = match self.0.db.read(saved_faces).await {
			Ok(faces) => faces,
			Err(error) => {
				tracing::warn!(%error, "saved faces were not read, so their materials are not fetched");
				return;
			},
		};
		let mut ids = HashSet::new();
		for face in &faces {
			collect_material_ids(face, &mut ids);
		}
		join_all(ids.into_iter().map(|id| {
			let mut answers = self.observe(id);
			async move {
				answers.next().await;
			}
		}))
		.await;
	}

	async fn fetch(self, id: [u8; 32], own: AbortHandle) {
		let answer = self.resolve(id, &own).await;
		let mut fetches = self.0.fetches.lock();
		// Every caller left as the answer came, so the entry may be a newer fetch's.
		if own.is_aborted() {
			return;
		}
		let Some(fetch) = fetches.remove(&id) else {
			return;
		};
		if fetch.failure.is_some() && answer.as_ref().err() == fetch.failure.as_ref() {
			return;
		}
		for caller in fetch.callers {
			let _ = caller.unbounded_send(answer.clone());
		}
	}

	async fn resolve(&self, id: [u8; 32], own: &AbortHandle) -> Result<Vec<u8>, MaterialFailure> {
		match self.kept(id).await {
			Ok(Some(text)) => return Ok(text),
			Ok(None) => {},
			Err(error) => tracing::warn!(%error, "a kept material was not read"),
		}
		let mut lookup = self.0.lookup.lookup_preimage(id).fuse();
		let mut deadline = self.0.timer.sleep(FETCH_DEADLINE).fuse();
		loop {
			// An answer already in hand beats the deadline.
			select_biased! {
				answer = lookup.next() => match answer {
					Some(Ok(Some(bytes))) if preimage_key(&bytes) == id => return self.keep(id, bytes).await,
					Some(Ok(_)) => {},
					Some(Err(error)) => {
						tracing::warn!(reason = %error.reason, "a material lookup failed for good");
						return Err(MaterialFailure::NotFound);
					},
					None => return Err(MaterialFailure::NotFound),
				},
				() = deadline => self.report(id, MaterialFailure::NotFound, own),
			}
		}
	}

	async fn kept(&self, id: [u8; 32]) -> Result<Option<Vec<u8>>, DbError> {
		self.0
			.db
			.read(move |conn| {
				Ok(conn.prepare_cached(KEPT)?.query_row([id], |row| row.get(0)).optional()?)
			})
			.await
	}

	async fn keep(&self, id: [u8; 32], text: Vec<u8>) -> Result<Vec<u8>, MaterialFailure> {
		read_material(&text)?;
		let row = text.clone();
		let kept = self
			.0
			.db
			.write(move |tx| {
				tx.prepare_cached(KEEP)?.execute(rusqlite::params![id, row])?;
				evict(tx)
			})
			.await;
		if let Err(error) = kept {
			tracing::warn!(%error, "a material was not kept, and is fetched again next time");
		}
		Ok(text)
	}

	fn report(&self, id: [u8; 32], failure: MaterialFailure, own: &AbortHandle) {
		let mut fetches = self.0.fetches.lock();
		if own.is_aborted() {
			return;
		}
		if let Some(fetch) = fetches.get_mut(&id) {
			fetch.failure = Some(failure);
			for caller in &fetch.callers {
				let _ = caller.unbounded_send(Err(failure));
			}
		}
	}

	fn leave(&self, id: [u8; 32]) {
		let mut fetches = self.0.fetches.lock();
		let Some(fetch) = fetches.get_mut(&id) else {
			return;
		};
		fetch.callers.retain(|caller| !caller.is_closed());
		if fetch.callers.is_empty() {
			fetch.abort.abort();
			fetches.remove(&id);
		}
	}
}

/// One caller's answers. Dropping it leaves the fetch, which stops once no
/// caller is left.
struct Caller {
	answers: UnboundedReceiver<Result<Vec<u8>, MaterialFailure>>,
	id: [u8; 32],
	materials: Materials,
}

impl Stream for Caller {
	type Item = Result<Vec<u8>, MaterialFailure>;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.answers.poll_next_unpin(cx)
	}
}

impl Drop for Caller {
	fn drop(&mut self) {
		self.answers.close();
		self.materials.leave(self.id);
	}
}

/// Deletes the oldest materials no saved face draws until the rest fit in
/// [`UNPINNED_BYTES`]. Runs in the write that keeps a material, so a face kept
/// meanwhile cannot lose its material.
fn evict(tx: &Transaction<'_>) -> Result<(), DbError> {
	let total: usize = tx.prepare_cached(TOTAL_BYTES)?.query_row([], |row| row.get(0))?;
	if total <= UNPINNED_BYTES {
		return Ok(());
	}
	let mut pinned = HashSet::new();
	for face in saved_faces(tx)? {
		collect_material_ids(&face, &mut pinned);
	}
	let mut unpinned_bytes = 0;
	let mut evicted = Vec::new();
	let mut newest_first = tx.prepare_cached(NEWEST_FIRST)?;
	for row in newest_first.query_map([], |row| Ok((row.get(0)?, row.get::<_, usize>(1)?)))? {
		let (id, size): ([u8; 32], usize) = row?;
		if pinned.contains(&id) {
			continue;
		}
		unpinned_bytes += size;
		if unpinned_bytes > UNPINNED_BYTES {
			evicted.push(id);
		}
	}
	let mut delete = tx.prepare_cached(DELETE)?;
	for id in evicted {
		delete.execute([id])?;
	}
	Ok(())
}

fn collect_material_ids(node: &RendererNode, ids: &mut HashSet<[u8; 32]>) {
	let layers = materials_nodes(node).into_iter().flat_map(|(props, _)| &props.layers);
	ids.extend(layers.map(|layer| layer.material));
}

/// Every `Materials` node in `tree`, depth first, each with whether another
/// `Materials` node encloses it.
fn materials_nodes(tree: &RendererNode) -> Vec<(&MaterialsProps, bool)> {
	let mut nodes = Vec::new();
	visit_materials(tree, false, &mut nodes);
	nodes
}

fn visit_materials<'tree>(
	node: &'tree RendererNode,
	nested: bool,
	nodes: &mut Vec<(&'tree MaterialsProps, bool)>,
) {
	let children = match node {
		RendererNode::Materials { props, children } => {
			nodes.push((props, nested));
			children
		},
		RendererNode::Box { children, .. } |
		RendererNode::Column { children, .. } |
		RendererNode::Row { children, .. } |
		RendererNode::Text { children, .. } |
		RendererNode::Button { children, .. } => children,
		RendererNode::Nil |
		RendererNode::String { .. } |
		RendererNode::Spacer { .. } |
		RendererNode::TextField { .. } |
		RendererNode::Image { .. } => return,
	};
	let nested = nested || matches!(node, RendererNode::Materials { .. });
	for child in children {
		visit_materials(child, nested, nodes);
	}
}

const KEPT: &str = "SELECT text FROM material WHERE id = ?1";

const KEEP: &str = "INSERT OR IGNORE INTO material (id, text) VALUES (?1, ?2)";

const TOTAL_BYTES: &str = "SELECT COALESCE(SUM(length(text)), 0) FROM material";

const NEWEST_FIRST: &str = "SELECT id, length(text) FROM material ORDER BY rowid DESC";

const DELETE: &str = "DELETE FROM material WHERE id = ?1";

#[cfg_attr(not(test), expect(dead_code, reason = "the plan per tree, still to come, calls these"))]
mod format;
#[cfg_attr(not(test), expect(dead_code, reason = "the plan per tree, still to come, calls these"))]
mod limits;
#[cfg_attr(not(test), expect(dead_code, reason = "the plan per tree, still to come, calls these"))]
mod noise;

#[cfg(test)]
mod tests;
