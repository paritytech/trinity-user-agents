//! The submission watch: submits one attempt to the node and follows it,
//! for latency. It proposes verdicts through the same compare-and-set a pass
//! uses, and hands the transaction back to recovery when it stops watching.

use core::{ops::ControlFlow, time::Duration};
use std::sync::Arc;

use futures::{
	StreamExt,
	future::{Either, select},
	stream::BoxStream,
};
use subxt::{tx::ValidationResult, utils::H256};
use tracing::{debug, warn};

use super::{DurableTxEngine, verdict::write_verdict};
use crate::{
	chain::{DispatchOutcome, EncodedExtrinsic, HashAndNumber, MortalExtrinsic, WatchEvent},
	durable::{
		dao,
		model::{DurableTxId, Verdict, VerdictFailureKind, VerdictStatus},
		ownership::OwnershipClaim,
	},
};

/// A watch that hears nothing for this long hands the transaction to
/// recovery, long before its era can end.
const SILENCE_TIMEOUT: Duration = Duration::from_secs(30);

/// One attempt of a transaction, as a watch follows it.
#[derive(Clone, Copy)]
pub struct Attempt {
	claim: OwnershipClaim,
	genesis: H256,
	tx_hash: H256,
}

impl Attempt {
	/// The attempt `extrinsic` makes at the transaction `claim` owns, on the
	/// chain with `genesis`.
	pub fn new(claim: OwnershipClaim, genesis: H256, extrinsic: &MortalExtrinsic) -> Self {
		Self { claim, genesis, tx_hash: extrinsic.extrinsic.hash() }
	}

	fn id(&self) -> DurableTxId {
		self.claim.id()
	}
}

/// Watches `extrinsic`, an attempt `engine` already owns, until it settles or
/// the watch ends, then hands it back.
pub fn spawn_watch(engine: &Arc<DurableTxEngine>, attempt: Attempt, extrinsic: EncodedExtrinsic) {
	let engine = engine.clone();
	(engine.spawner.clone())(Box::pin(async move {
		engine.watch(attempt, &extrinsic).await;
		engine.finish_watch(attempt).await;
	}));
}

impl DurableTxEngine {
	async fn watch(&self, attempt: Attempt, extrinsic: &EncodedExtrinsic) {
		if !self.admitted(attempt, extrinsic).await {
			return;
		}
		match self.submitter.submit_and_watch(attempt.genesis, extrinsic).await {
			Ok(events) => self.follow(attempt, events).await,
			Err(error) => {
				warn!(id = attempt.id().0, %error, "durable transaction could not be submitted")
			},
		}
	}

	/// Releases the attempt and, unless the watch decided it, hands the
	/// transaction to recovery: a running loop, or else the host.
	async fn finish_watch(&self, attempt: Attempt) {
		self.ownership.release(attempt.claim);
		if self.needs_recovery(attempt.id()).await && !self.recovery_wakes.wake() {
			self.host_wakes.wake();
		}
	}

	/// Whether the node may be sent `extrinsic`. A refusal fails the
	/// transaction at once: nothing can ever include these bytes. A
	/// validation that could not run lets it through, as Android does,
	/// because the bytes are held nowhere else.
	async fn admitted(&self, attempt: Attempt, extrinsic: &EncodedExtrinsic) -> bool {
		let validation = self.validator.validate(attempt.genesis, extrinsic).await;
		let Ok(ValidationResult::Invalid(reason)) = validation else {
			return true;
		};
		debug!(id = attempt.id().0, ?reason, "durable transaction refused before submission");
		self.propose(attempt, VerdictStatus::Failure(VerdictFailureKind::Rejected).into())
			.await;
		false
	}

	/// Follows `events` until a terminal one, the end of the stream, or
	/// [`SILENCE_TIMEOUT`] without one.
	async fn follow(&self, attempt: Attempt, mut events: BoxStream<'static, WatchEvent>) {
		let mut best = None;
		while let Some(event) = self.next_event(attempt, &mut events).await {
			if self.on_event(attempt, &mut best, event).await.is_break() {
				return;
			}
		}
	}

	/// The next event, or `None` when the stream ended or fell silent.
	async fn next_event(
		&self,
		attempt: Attempt,
		events: &mut BoxStream<'static, WatchEvent>,
	) -> Option<WatchEvent> {
		match select(events.next(), self.timer.sleep(SILENCE_TIMEOUT)).await {
			Either::Left((event, _)) => event,
			Either::Right(_) => {
				warn!(id = attempt.id().0, "durable submission watch fell silent");
				None
			},
		}
	}

	/// Acts on one event. `best` is the best block the extrinsic was last
	/// seen in, since a retraction does not name it.
	async fn on_event(
		&self,
		attempt: Attempt,
		best: &mut Option<H256>,
		event: WatchEvent,
	) -> ControlFlow<()> {
		debug!(id = attempt.id().0, ?event, "durable submission status");
		match event {
			WatchEvent::InBestBlock(hash) => self.on_best_block(attempt, best.insert(hash)).await,
			WatchEvent::NoLongerInBestBlock => self.on_retraction(attempt, best.take()).await,
			WatchEvent::InFinalizedBlock(hash) => {
				return self.on_finalized_block(attempt, hash).await;
			},
			WatchEvent::Invalid(_) | WatchEvent::Dropped(_) | WatchEvent::Error(_) => {
				return ControlFlow::Break(());
			},
		}
		ControlFlow::Continue(())
	}

	/// Records a successful dispatch in the best block. A failure there is
	/// not final, so it proposes nothing.
	async fn on_best_block(&self, attempt: Attempt, hash: &H256) {
		if let Some((block, DispatchOutcome::Succeeded)) = self.outcome_at(attempt, *hash).await {
			let success = VerdictStatus::PendingSuccess { at: block };
			self.propose(attempt, success.into()).await;
		}
	}

	/// Settles the transaction from its finalized block, when the outcome
	/// can be read there. The watch ends either way.
	async fn on_finalized_block(&self, attempt: Attempt, hash: H256) -> ControlFlow<()> {
		let status = match self.outcome_at(attempt, hash).await {
			Some((block, DispatchOutcome::Succeeded)) => {
				VerdictStatus::FinalizedSuccess { at: Some(block) }
			},
			Some((_, DispatchOutcome::Failed(_))) => {
				VerdictStatus::Failure(VerdictFailureKind::DispatchFailed)
			},
			None => return ControlFlow::Break(()),
		};
		self.propose(attempt, status.into()).await;
		ControlFlow::Break(())
	}

	/// The block with `hash` and how the attempt dispatched in it, when both
	/// can be read.
	async fn outcome_at(
		&self,
		attempt: Attempt,
		hash: H256,
	) -> Option<(HashAndNumber, DispatchOutcome)> {
		let number = self.blocks.block_number(attempt.genesis, hash).await.ok()??;
		let block = HashAndNumber { hash, number };
		let outcome = self
			.blocks
			.dispatch_outcome(attempt.genesis, block, attempt.tx_hash)
			.await
			.ok()??;
		Some((block, outcome))
	}

	/// Demotes the transaction when its record names the retracted block:
	/// a success resting on a block that is gone is no evidence.
	async fn on_retraction(&self, attempt: Attempt, retracted: Option<H256>) {
		let Some(retracted) = retracted else {
			return;
		};
		let id = attempt.id();
		let Ok(Some(entry)) = self.db.read(move |conn| dao::entry(conn, id)).await else {
			return;
		};
		let recorded = entry.status.success_block();
		if recorded.map(|block| block.hash) == Some(retracted) {
			self.propose(attempt, VerdictStatus::Pending.into()).await;
		}
	}

	/// Writes `verdict` while the row still awaits one for the watched
	/// attempt.
	async fn propose(&self, attempt: Attempt, verdict: Verdict) {
		let id = attempt.id();
		let observed = match self.db.read(move |conn| dao::entry(conn, id)).await {
			Ok(Some(observed)) => observed,
			Ok(None) => return,
			Err(error) => {
				return warn!(id = id.0, %error, "durable proposal could not read its row");
			},
		};
		if !observed.status.awaits_verdict() || observed.tx_hash != attempt.tx_hash {
			return;
		}
		if let Err(error) = write_verdict(self, &observed, verdict).await {
			warn!(id = id.0, %error, "durable proposal could not be written");
		}
	}

	/// Whether recovery still has to decide `id`. Unreadable counts as yes.
	async fn needs_recovery(&self, id: DurableTxId) -> bool {
		match self.db.read(move |conn| dao::status(conn, id)).await {
			Ok(Some(status)) => status.awaits_verdict(),
			Ok(None) => false,
			Err(_) => true,
		}
	}
}

#[cfg(test)]
mod tests;
