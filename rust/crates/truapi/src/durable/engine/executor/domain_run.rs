//! [`DomainRun`]: one worker building the waiting rows of one domain, round
//! after round, until a round finds none.

use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Weak},
};

use futures::{
	FutureExt,
	channel::mpsc::{Receiver, UnboundedSender},
	future::pending,
	stream::{Stream, StreamExt},
};
use subxt::utils::H256;
use tracing::warn;

use super::{
	attempt::{give_up_submission, start_built_attempt},
	backoff::backoff,
	schedule::Schedule,
};
use crate::{
	durable::{
		dao,
		engine::DurableTxEngine,
		model::{DeferredDurableTx, DomainId, DurableTxId},
		ownership::Ownership,
		policy::{Preparation, SubmissionPolicy},
	},
	store::{Db, DbError},
	time::Timer,
};

/// One domain's worker. Holds the engine weakly, so a policy that never
/// answers cannot keep a dropped engine alive.
pub struct DomainRun {
	engine: Weak<DurableTxEngine>,
	db: Db,
	timer: Arc<dyn Timer>,
	domain: DomainId,
	resolved: ResolvedPolicy,
	schedule: Schedule,
	finished: UnboundedSender<DomainId>,
	ownership: Arc<Ownership>,
	/// Heard only while `held` is not empty.
	releases: Receiver<()>,
	/// Waiting rows a round skipped because a watch owned them, kept until
	/// a round finds them no longer owned or no longer waiting.
	held: Vec<DurableTxId>,
}

/// What builds a domain's rows: its policy, and the chain it lives on.
struct ResolvedPolicy {
	policy: Arc<dyn SubmissionPolicy>,
	genesis: H256,
}

impl DomainRun {
	/// The worker for `domain`, which sends `domain` on `finished` once it
	/// finds no waiting row, or `None` when the domain is not registered: its
	/// rows are then left waiting untouched.
	pub fn new(
		engine: &Arc<DurableTxEngine>,
		domain: DomainId,
		finished: UnboundedSender<DomainId>,
	) -> Option<Self> {
		let resolved = resolve_policy(engine, &domain)?;
		Some(Self {
			engine: Arc::downgrade(engine),
			db: engine.db.clone(),
			timer: engine.timer.clone(),
			domain,
			resolved,
			schedule: Schedule::default(),
			finished,
			ownership: engine.ownership.clone(),
			releases: engine.ownership.releases(),
			held: Vec::new(),
		})
	}

	/// Runs rounds until a read finds no waiting row, then tells the builder.
	/// Between rounds it sleeps until the next row is due or the domain's
	/// rows change, whichever comes first.
	pub async fn run_until_drained(mut self) {
		let mut changes = dao::observe_domain_changes(&self.db, self.domain.clone());
		// The first emission is the state the first round reads anyway.
		let _ = changes.next().await;
		while self.engine.strong_count() > 0 {
			let Some(waiting) = self.read_waiting().await else {
				break;
			};
			self.decide_round(waiting).await;
			self.wait_for_due_or_change(&mut changes).await;
		}
		// The builder may be gone with the engine; then nobody needs to know.
		let _ = self.finished.unbounded_send(self.domain.clone());
	}

	/// The domain's waiting rows, read again after a growing backoff while
	/// the read fails, or `None` once none is waiting.
	async fn read_waiting(&self) -> Option<Vec<DeferredDurableTx>> {
		let mut failures = 0;
		loop {
			let domain = self.domain.clone();
			match self.db.read(move |conn| dao::released_submissions(conn, &domain)).await {
				Ok(rows) if rows.is_empty() => return None,
				Ok(rows) => return Some(rows),
				Err(error) => {
					warn!(domain = self.domain.as_str(), %error, "durable submissions of a domain could not be read");
					failures += 1;
					self.timer.sleep(backoff(failures)).await;
				},
			}
		}
	}

	/// Hands the policy every due row of `waiting` that no follower holds,
	/// and applies its answers. Every due row still waiting afterwards is
	/// asked about again later.
	async fn decide_round(&mut self, waiting: Vec<DeferredDurableTx>) {
		let now = self.timer.now();
		self.schedule.track(&waiting, now);
		let due: HashSet<_> = waiting
			.iter()
			.map(|row| row.id)
			.filter(|id| self.schedule.is_due(*id, now))
			.collect();
		self.schedule.ask_later(due.iter().copied(), now);
		// A row whose previous attempt is still followed waits for that
		// follower to stop, so a transaction never has two. It stays held,
		// due or not, until the follower releases it.
		let held_before = core::mem::take(&mut self.held);
		self.held = waiting
			.iter()
			.map(|row| row.id)
			.filter(|id| due.contains(id) || held_before.contains(id))
			.filter(|id| self.ownership.is_owned(*id))
			.collect();
		let batch: Vec<_> = waiting
			.into_iter()
			.filter(|row| due.contains(&row.id) && !self.held.contains(&row.id))
			.collect();
		if !batch.is_empty() {
			self.prepare_and_apply(batch).await;
		}
	}

	/// Sleeps until the next row is due, the domain's rows change, or a row
	/// the last round held back is no longer owned. A change makes every row
	/// the policy did not answer due at once; a release makes only the held
	/// rows due.
	async fn wait_for_due_or_change(
		&mut self,
		changes: &mut (impl Stream<Item = Result<Vec<(i64, String)>, DbError>> + Unpin),
	) {
		let now = self.timer.now();
		let Some(until_next) = self.schedule.until_next(now) else {
			return;
		};
		let due = self.timer.sleep(until_next).fuse();
		let changed = async {
			if changes.next().await.is_none() {
				pending::<()>().await
			}
		}
		.fuse();
		let released = released(&mut self.releases, &self.held, &self.ownership).fuse();
		futures::pin_mut!(due, changed, released);
		futures::select! {
			() = due => {},
			() = changed => self.schedule.domain_changed(self.timer.now()),
			() = released => self.schedule.make_due(&self.held, self.timer.now()),
		}
	}

	/// Builds `batch` with the domain's policy.
	async fn prepare_and_apply(&self, batch: Vec<DeferredDurableTx>) {
		let resolved = &self.resolved;
		match resolved.policy.prepare(&batch).await {
			Ok(answers) => self.apply_all(resolved.genesis, batch, answers).await,
			Err(error) => {
				warn!(domain = self.domain.as_str(), rows = batch.len(), %error, "durable submissions not prepared");
			},
		}
	}

	/// Applies the answer for each row of `batch`. Answers about other rows
	/// are ignored.
	async fn apply_all(
		&self,
		genesis: H256,
		batch: Vec<DeferredDurableTx>,
		mut answers: HashMap<DurableTxId, Preparation>,
	) {
		for row in batch {
			if let Some(preparation) = answers.remove(&row.id) {
				self.apply_preparation(genesis, row.id, preparation).await;
			}
		}
	}

	/// Starts the attempt a ready row makes, or fails a row its policy gave
	/// up on.
	async fn apply_preparation(&self, genesis: H256, id: DurableTxId, preparation: Preparation) {
		let Some(engine) = self.engine.upgrade() else {
			return;
		};
		match preparation {
			Preparation::Ready { extrinsic, params } => {
				start_built_attempt(&engine, id, genesis, extrinsic, params).await;
			},
			Preparation::GiveUp => {
				give_up_submission(&engine, id).await;
			},
		}
	}
}

/// Returns once a row of `held` is no longer owned, and never while `held`
/// is empty. A release is a write to memory only, so no change of the
/// domain's rows reports it.
async fn released(releases: &mut Receiver<()>, held: &[DurableTxId], ownership: &Ownership) {
	loop {
		if held.iter().any(|id| !ownership.is_owned(*id)) {
			return;
		}
		if held.is_empty() || releases.next().await.is_none() {
			pending::<()>().await
		}
	}
}

/// The policy and chain that build `domain`'s rows, when it is registered.
fn resolve_policy(engine: &DurableTxEngine, domain: &DomainId) -> Option<ResolvedPolicy> {
	let genesis = engine.chain_of(domain)?;
	let policy = engine.registry.submission_policy(domain)?.clone();
	Some(ResolvedPolicy { policy, genesis })
}
