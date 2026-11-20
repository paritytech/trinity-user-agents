//! Keeps the stack's descriptor current while the core runs.
//!
//! [`StackRefresh::load`] resolves the stack the core starts on without
//! touching the network. [`StackRefresh::start`] then fetches the descriptor
//! through the host, in the background, whenever the copy is old enough.

use core::time::Duration;
use std::sync::{Arc, Mutex, OnceLock};

use futures::{
	FutureExt, StreamExt,
	channel::mpsc,
	future::{Either, pending, select},
};
use trinity_stack::{FetchRequest, Served};
use truapi::latest::GenericError;

use super::{
	Stack, StackError, StackFetchRequest, StackFetchResponse, StackRecords, StackStatus,
	signed_pointer, source::StackSource,
};
use crate::{
	Spawner,
	background_task::BackgroundTask,
	chain_runtime::{GenesisCheck, encode_hex},
	time::{Clock, Timer},
	wake::WakeSignal,
};

/// The largest descriptor the core reads.
const MAX_DESCRIPTOR_BYTES: u32 = 1024 * 1024;

/// How long the host has to answer a fetch before it counts as unreachable.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a record's store has to finish before it counts as failed. A
/// store that never finished would hold the loop, and no later fetch could
/// start.
const STORE_TIMEOUT: Duration = Duration::from_secs(30);

/// How long building a runtime waits for the stored record. A host whose
/// storage never answers would otherwise keep the runtime from starting, so
/// past this the core starts from the bundled copy.
const RECORD_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// What a host does for the stack the core runs: fetch its descriptor, and
/// show what the core reports about it.
#[async_trait::async_trait]
pub trait StackHost: Send + Sync {
	/// Fetches the stack's descriptor over HTTP, as [`StackFetchRequest`]
	/// describes. Fails when the server gave no usable answer. The core then
	/// keeps the copy it holds and tries again later.
	async fn fetch_stack_descriptor(
		&self,
		request: StackFetchRequest,
	) -> Result<StackFetchResponse, GenericError>;

	/// The stack's status when the refresh starts, and again whenever it
	/// changes. Return promptly: the refresh waits on it.
	fn stack_changed(&self, status: StackStatus);
}

/// The stack a host build runs and the background refresh of its descriptor.
pub struct StackRefresh {
	shared: Arc<Shared>,
	/// The refresh loop, once started.
	task: OnceLock<BackgroundTask>,
	/// Asks the loop for a fetch now. The loop ends once this drops.
	refresh_requests: WakeSignal,
}

struct Shared {
	source: Mutex<StackSource>,
	records: Box<dyn StackRecords>,
	timer: Arc<dyn Timer>,
	clock: Box<dyn Clock>,
	/// Wakes the loop when what a chain answered changed the status.
	chain_answers: WakeSignal,
}

/// Checks the chains of a [`StackRefresh`]'s stack against its descriptor.
///
/// A chain that gives a different genesis hash from the one the descriptor
/// names is reported in [`StackStatus::mismatches`], and the refresh fetches
/// the descriptor sooner.
#[derive(Clone)]
pub struct StackChecks {
	shared: Arc<Shared>,
}

impl StackChecks {
	/// Hears the genesis hash each connection's chain gives, for the
	/// `ChainRuntime` that opens them.
	pub fn genesis_check(&self) -> Arc<dyn GenesisCheck> {
		self.shared.clone()
	}
}

impl core::fmt::Debug for StackChecks {
	fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		formatter.debug_struct("StackChecks").finish_non_exhaustive()
	}
}

impl GenesisCheck for Shared {
	fn chain_answered(&self, named: [u8; 32], answered: [u8; 32]) {
		let changed = self.source().chain_answered(named, answered);
		if !changed {
			return;
		}
		// Logged with the source unlocked: a native host is handed each
		// warning as it is made, and may read the stack's status from there.
		if answered != named {
			tracing::warn!(
				"a chain of the stack gave genesis hash {}, not {}",
				encode_hex(&answered),
				encode_hex(&named)
			);
		}
		self.chain_answers.wake();
	}
}

impl StackRefresh {
	/// Resolves the stack a host build runs. `pointer` is the stack's link
	/// (`polkadotapp://stack?key=0x…&url=…`), `bundled` the signed copy the
	/// build ships, and `allows_reserved_values` whether it may use Polkadot's
	/// genesis hashes and the `dot` TLD.
	///
	/// Starts from the copy `records` holds when it is newer than `bundled`,
	/// and never waits on the network. Fails with a [`StackError`] naming the
	/// rule `bundled` breaks. A record that cannot be read, or is not read
	/// within a few seconds, is logged and ignored.
	///
	/// `clock` tells how old a copy is and `timer` waits for the next fetch.
	pub async fn load(
		pointer: &str,
		bundled: &[u8],
		allows_reserved_values: bool,
		records: Box<dyn StackRecords>,
		timer: Arc<dyn Timer>,
		clock: Box<dyn Clock>,
	) -> Result<Self, StackError> {
		let pointer = signed_pointer(pointer)?;
		let stack_id = pointer.stack_id();
		let read = records.read_stack_record(&stack_id);
		// Started only if the read is still out, so a read that answers at once
		// leaves no wait behind.
		let read_timeout = Box::pin(async { timer.sleep(RECORD_READ_TIMEOUT).await });
		let stored = match select(read, read_timeout).await {
			Either::Left((Ok(stored), _)) => stored,
			Either::Left((Err(error), _)) => {
				tracing::warn!("stored stack descriptor could not be read: {error}");
				None
			},
			Either::Right(_) => {
				tracing::warn!(
					"stored stack descriptor not read within {}s, starting from the bundled copy",
					RECORD_READ_TIMEOUT.as_secs()
				);
				None
			},
		};
		let source = StackSource::load(
			pointer,
			bundled,
			stored.as_deref(),
			allows_reserved_values,
			unix_now(clock.as_ref()),
		)?;
		Ok(Self {
			shared: Arc::new(Shared {
				source: Mutex::new(source),
				records,
				timer,
				clock,
				chain_answers: WakeSignal::default(),
			}),
			task: OnceLock::new(),
			refresh_requests: WakeSignal::default(),
		})
	}

	/// The stack the core runs, with the checks of its chains: a runtime
	/// built on it puts every chain connection it opens through them. The
	/// stack stays the same for the life of the runtime: a newer copy that
	/// changes it applies at the next start.
	pub fn stack(&self) -> Stack {
		let stack = self.shared.source().stack().clone();
		Stack { checks: Some(StackChecks { shared: self.shared.clone() }), ..stack }
	}

	/// What the host is told about the stack.
	pub fn status(&self) -> StackStatus {
		self.shared.source().status()
	}

	/// Starts refreshing through `host`, on `spawner`, until this value
	/// drops. Reports the current status to `host` first.
	///
	/// Set-once: answers whether this call started the refresh.
	#[must_use = "`false` means the refresh was already running and `host` was dropped"]
	pub fn start(&self, host: Box<dyn StackHost>, spawner: &Spawner) -> bool {
		let mut started = false;
		self.task.get_or_init(|| {
			started = true;
			let requests = self.refresh_requests.subscribe();
			BackgroundTask::spawn(spawner, self.shared.clone().run(host, requests))
		});
		started
	}

	/// Fetches the descriptor now instead of when the copy is old enough.
	///
	/// A host asks when its app returns to the foreground, because a sleeping
	/// device can stretch the wait for the next fetch. Returns at once.
	/// Requests made while a fetch is out lead to one more fetch, not one
	/// each. Does nothing before [`Self::start`].
	pub fn refresh_now(&self) {
		self.refresh_requests.wake();
	}
}

impl Shared {
	fn source(&self) -> std::sync::MutexGuard<'_, StackSource> {
		self.source.lock().expect("stack source mutex poisoned")
	}

	fn now(&self) -> i64 {
		unix_now(self.clock.as_ref())
	}

	async fn run(self: Arc<Self>, host: Box<dyn StackHost>, mut forced: mpsc::Receiver<()>) {
		let mut chain_answers = self.chain_answers.subscribe();
		let mut reported = self.source().status();
		host.stack_changed(reported.clone());
		let mut force = false;
		loop {
			// What a chain answered is told before the fetch it brings forward
			// and while a fetch is out, since a host may take half a minute to
			// answer one.
			self.report_change(host.as_ref(), &mut reported);
			let request = if force {
				self.source().forced_fetch(self.now())
			} else {
				self.source().due_fetch(self.now())
			};
			if let Some(request) = request {
				let mut fetch = core::pin::pin!(self.fetch(host.as_ref(), request));
				while let Either::Right(_) = select(fetch.as_mut(), chain_answers.next()).await {
					self.report_change(host.as_ref(), &mut reported);
				}
				self.report_change(host.as_ref(), &mut reported);
			}
			let next_fetch_in = self.source().next_fetch_in(self.now());
			let until_due = match next_fetch_in {
				Some(wait) => self.timer.sleep(wait),
				None => pending().boxed(),
			};
			let asked = select(forced.next(), chain_answers.next());
			force = match select(until_due, asked).await {
				Either::Left(_) | Either::Right((Either::Right(_), _)) => false,
				Either::Right((Either::Left((Some(()), _)), _)) => true,
				Either::Right((Either::Left((None, _)), _)) => return,
			};
		}
	}

	/// Tells `host` the stack's status when it differs from `reported`.
	fn report_change(&self, host: &dyn StackHost, reported: &mut StackStatus) {
		let status = self.source().status();
		if status != *reported {
			*reported = status;
			host.stack_changed(reported.clone());
		}
	}

	/// Makes one fetch through the host and stores the record it changed.
	async fn fetch(&self, host: &dyn StackHost, request: FetchRequest) {
		let FetchRequest { url, if_none_match } = request;
		let request = StackFetchRequest { url, if_none_match, max_bytes: MAX_DESCRIPTOR_BYTES };
		let answer = select(host.fetch_stack_descriptor(request), self.timer.sleep(FETCH_TIMEOUT));
		let response = match answer.await {
			Either::Left((response, _)) => response.map_err(|error| error.reason),
			Either::Right(_) => Err(format!("no answer within {}s", FETCH_TIMEOUT.as_secs())),
		};
		let served = match &response {
			Ok(StackFetchResponse::Document { bytes, etag }) => match u32::try_from(bytes.len()) {
				Ok(length) if length <= MAX_DESCRIPTOR_BYTES => {
					Ok(Served::Document { bytes, etag: etag.as_deref() })
				},
				_ => Err(format!("{} bytes is larger than {MAX_DESCRIPTOR_BYTES}", bytes.len())),
			},
			Ok(StackFetchResponse::NotModified) => Ok(Served::NotModified),
			Err(reason) => Err(reason.clone()),
		};
		if let Err(reason) = &served {
			tracing::debug!("stack descriptor fetch failed: {reason}");
		}
		let (stack_id, write) = {
			let mut source = self.source();
			let write = source.fetched(served, self.now()).map(|record| {
				let check = source.replace_check(&record);
				(record, check)
			});
			(source.stack_id(), write)
		};
		let Some((record, check)) = write else { return };
		let write = self.records.write_stack_record(&stack_id, record, check);
		let stored = match select(write, self.timer.sleep(STORE_TIMEOUT)).await {
			Either::Left((stored, _)) => stored.map_err(|error| error.reason),
			Either::Right(_) => Err(format!("not stored within {}s", STORE_TIMEOUT.as_secs())),
		};
		match stored {
			Ok(()) => self.source().record_stored(),
			Err(reason) => {
				tracing::warn!("stack descriptor could not be stored: {reason}");
				self.source().record_not_stored(reason);
			},
		}
	}
}

fn unix_now(clock: &dyn Clock) -> i64 {
	i64::try_from(clock.unix_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests;
