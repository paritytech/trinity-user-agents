//! Public profile reads that share one Asset Hub follow.
//!
//! Every profile sits behind the same protocol registry and content resolver,
//! so only the last view of a read depends on the username. Reads wait in one
//! queue, oldest first, and a single task serves it one round at a time: it
//! opens a follow at the best block, finds the resolver, reads the records of
//! up to [`ROUND_SIZE`] queued usernames side by side, and lets that follow go.
//! Reads queued while the follow opens join its round, and later ones wait for
//! the next round on a fresh follow and block. A burst of avatars holds one
//! follow, and no follow is held longer than one bounded round.

use std::sync::{Arc, Mutex, MutexGuard};

use futures::channel::oneshot;
use tracing::{Instrument, instrument::WithSubscriber};

use super::{ProfileDirectory, ProfileRecord, VIEWS_AT_ONCE, read_records};
use crate::{
	host_logic::dotns_gateway::DotnsError,
	runtime::{
		DotnsLookup, DotnsLookupKind,
		services::{READ_ENDED_UNANSWERED, RuntimeServices},
	},
};

/// Usernames one round reads. Keeps the time a follow pins one block, and the
/// wait of a read queued behind a round, to a few batches of views.
const ROUND_SIZE: usize = 64;

/// A username's record, or why it could not be read, shared by every caller
/// that asked for it in one round.
type Answer = Result<ProfileRecord, Arc<DotnsError>>;

/// A queued username and the callers waiting for its record.
type Waiting = (String, Vec<oneshot::Sender<Answer>>);

/// Usernames waiting for a profile read, and whether a task is serving them.
#[derive(Default)]
pub struct ProfileReads {
	queue: Mutex<Queue>,
}

#[derive(Default)]
struct Queue {
	serving: bool,
	/// Oldest first, one entry per username.
	waiting: Vec<Waiting>,
}

impl ProfileReads {
	fn lock(&self) -> MutexGuard<'_, Queue> {
		self.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// The oldest [`ROUND_SIZE`] usernames someone still waits for, taken off
	/// the queue.
	fn take(&self) -> Vec<Waiting> {
		let mut queue = self.lock();
		queue.waiting.retain(|(_, senders)| awaited(senders));
		let count = queue.waiting.len().min(ROUND_SIZE);
		queue.waiting.drain(..count).collect()
	}

	/// Stops serving when nobody still waits. Decided under the same lock a new
	/// read queues under, so a read either finds the task serving or starts one.
	fn stop_when_idle(&self) -> bool {
		let mut queue = self.lock();
		queue.waiting.retain(|(_, senders)| awaited(senders));
		if queue.waiting.is_empty() {
			queue.serving = false;
		}
		!queue.serving
	}
}

/// Whether any caller still waits for this answer.
fn awaited(senders: &[oneshot::Sender<Answer>]) -> bool {
	senders.iter().any(|sender| !sender.is_canceled())
}

/// Queues `username` and answers once a round has read it, starting the task
/// that serves the queue when none is.
pub(super) async fn read(
	services: &Arc<RuntimeServices>,
	genesis: [u8; 32],
	username: &str,
) -> Answer {
	let reads = &services.dotns_reads.public_profiles;
	let (sender, answer) = oneshot::channel();
	let start = {
		let mut queue = reads.lock();
		match queue.waiting.iter_mut().find(|(queued, _)| queued == username) {
			Some((_, senders)) => senders.push(sender),
			None => queue.waiting.push((username.to_owned(), vec![sender])),
		}
		!core::mem::replace(&mut queue.serving, true)
	};
	if start {
		// Made before the spawn so it stops serving even if the executor drops
		// the task unpolled.
		let serving = Serving { reads: reads.clone(), stopped: false };
		// The task serves every caller, so it logs under a span of its own
		// rather than the one of the read that happened to start it.
		let span = tracing::info_span!(
			parent: None,
			"public_profile.reads",
			runtime.method = "public_profile.reads"
		);
		let task = serve(services.clone(), genesis, serving)
			.instrument(span)
			.with_current_subscriber();
		services.spawner.spawn(task);
	}
	answer
		.await
		.unwrap_or_else(|_| Err(Arc::new(DotnsError::Follow(READ_ENDED_UNANSWERED.to_string()))))
}

/// Serves the queue one round, and one follow, at a time until it is empty.
async fn serve(services: Arc<RuntimeServices>, genesis: [u8; 32], mut serving: Serving) {
	while !serving.reads.stop_when_idle() {
		let opened = match DotnsLookup::pinned_to_best_block(
			&services.chain,
			genesis,
			DotnsLookupKind::PublicProfile,
		)
		.await
		{
			Ok(mut lookup) => {
				ProfileDirectory::find(&mut lookup).await.map(|found| (lookup, found))
			},
			Err(error) => Err(error),
		};
		let mut round = serving.reads.take().into_iter();
		let (mut lookup, directory) = match opened {
			Ok(opened) => opened,
			Err(error) => {
				answer_all(round, &Arc::new(error));
				continue;
			},
		};
		loop {
			let batch: Vec<Waiting> = round
				.by_ref()
				.filter(|(_, senders)| awaited(senders))
				.take(VIEWS_AT_ONCE)
				.collect();
			if batch.is_empty() {
				break;
			}
			let usernames: Vec<String> =
				batch.iter().map(|(username, _)| username.clone()).collect();
			let records: Vec<Answer> = read_records(&mut lookup, directory.as_ref(), &usernames)
				.await
				.into_iter()
				.map(|record| record.map_err(Arc::new))
				.collect();
			// A batch whose every read failed says the follow did, so the rest of
			// the round fails with it instead of waiting out the same failure.
			let lost = records.iter().all(Result::is_err).then(|| records[0].clone());
			for ((_, senders), answer) in batch.into_iter().zip(records) {
				for sender in senders {
					let _ = sender.send(answer.clone());
				}
			}
			if let Some(Err(error)) = lost {
				answer_all(round.by_ref(), &error);
				break;
			}
		}
	}
	serving.stopped = true;
}

/// Answers every caller in `round` with `error`.
fn answer_all(round: impl Iterator<Item = Waiting>, error: &Arc<DotnsError>) {
	for sender in round.flat_map(|(_, senders)| senders) {
		let _ = sender.send(Err(error.clone()));
	}
}

/// Stops serving if the task ends before it decided to, because its executor
/// dropped it, polled or not, or it unwound from a panic. The reads still
/// waiting are answered as unanswered, and the next read starts a new task
/// instead of queueing behind none.
struct Serving {
	reads: Arc<ProfileReads>,
	stopped: bool,
}

impl Drop for Serving {
	fn drop(&mut self) {
		if self.stopped {
			return;
		}
		let mut queue = self.reads.lock();
		queue.serving = false;
		queue.waiting.clear();
	}
}
