//! Public profile reads that share one Asset Hub follow.
//!
//! Every profile sits behind the same protocol registry and content resolver,
//! so only the last view of a read depends on the username. Reads wait in one
//! queue, and a single task serves it over one follow: it finds the resolver
//! once, reads every queued username's record in the same round, and takes the
//! next round on that follow until the queue is empty. A burst of avatars
//! holds one follow and reads side by side on it.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex, MutexGuard},
};

use futures::channel::oneshot;
use tracing::{Instrument, instrument::WithSubscriber};

use super::{ProfileDirectory, ProfileRecord, read_records};
use crate::{
	host_logic::dotns_gateway::DotnsError,
	runtime::{
		DotnsLookup, DotnsLookupKind,
		services::{READ_ENDED_UNANSWERED, RuntimeServices},
	},
};

/// Rounds one follow serves before the task opens a fresh one, so a queue
/// that never empties does not keep reading at one aging block.
const ROUNDS_PER_FOLLOW: usize = 8;

/// A username's record, or why it could not be read, shared by every caller
/// that asked for it in one round.
type Answer = Result<ProfileRecord, Arc<DotnsError>>;

/// Usernames waiting for a profile read, and whether a task is serving them.
#[derive(Default)]
pub struct ProfileReads {
	queue: Mutex<Queue>,
}

#[derive(Default)]
struct Queue {
	serving: bool,
	waiting: HashMap<String, Vec<oneshot::Sender<Answer>>>,
}

impl ProfileReads {
	fn lock(&self) -> MutexGuard<'_, Queue> {
		self.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// Everyone waiting now, leaving the queue empty.
	fn take(&self) -> HashMap<String, Vec<oneshot::Sender<Answer>>> {
		core::mem::take(&mut self.lock().waiting)
	}

	/// Stops serving when nobody waits. Decided under the same lock a new read
	/// queues under, so a read either finds the task serving or starts one.
	fn stop_when_idle(&self) -> bool {
		let mut queue = self.lock();
		if queue.waiting.is_empty() {
			queue.serving = false;
		}
		!queue.serving
	}
}

/// Queues `username` and answers once a round has read it, starting the task
/// that serves the queue when none is.
pub(super) async fn read(
	services: &Arc<RuntimeServices>,
	genesis: [u8; 32],
	username: &str,
) -> Answer {
	let (sender, answer) = oneshot::channel();
	let start = {
		let mut queue = services.dotns_reads.public_profiles.lock();
		queue.waiting.entry(username.to_owned()).or_default().push(sender);
		!core::mem::replace(&mut queue.serving, true)
	};
	if start {
		// The task logs as the read that started it would, under its span and
		// to its subscriber.
		let task = serve(services.clone(), genesis).in_current_span().with_current_subscriber();
		services.spawner.spawn(task);
	}
	answer
		.await
		.unwrap_or_else(|_| Err(Arc::new(DotnsError::Follow(READ_ENDED_UNANSWERED.to_string()))))
}

/// Serves the queue over one follow at a time until it is empty.
async fn serve(services: Arc<RuntimeServices>, genesis: [u8; 32]) {
	let reads = &services.dotns_reads.public_profiles;
	let mut serving = Serving { reads, stopped: false };
	loop {
		if reads.stop_when_idle() {
			serving.stopped = true;
			return;
		}
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
		let (mut lookup, directory) = match opened {
			Ok(opened) => opened,
			Err(error) => {
				let error = Arc::new(error);
				for sender in reads.take().into_values().flatten() {
					let _ = sender.send(Err(error.clone()));
				}
				continue;
			},
		};
		for _ in 0..ROUNDS_PER_FOLLOW {
			let round: Vec<_> = reads
				.take()
				.into_iter()
				.filter(|(_, senders)| senders.iter().any(|sender| !sender.is_canceled()))
				.collect();
			if round.is_empty() {
				break;
			}
			let usernames: Vec<String> =
				round.iter().map(|(username, _)| username.clone()).collect();
			let records = read_records(&mut lookup, directory.as_ref(), &usernames).await;
			for ((_, senders), record) in round.into_iter().zip(records) {
				let answer = record.map_err(Arc::new);
				for sender in senders {
					let _ = sender.send(answer.clone());
				}
			}
		}
	}
}

/// Stops serving if the task ends before it decided to, by a panic or by its
/// executor dropping it. The reads still waiting are answered as unanswered,
/// and the next read starts a new task instead of queueing behind none.
struct Serving<'a> {
	reads: &'a ProfileReads,
	stopped: bool,
}

impl Drop for Serving<'_> {
	fn drop(&mut self) {
		if self.stopped {
			return;
		}
		let mut queue = self.reads.lock();
		queue.serving = false;
		queue.waiting.clear();
	}
}
