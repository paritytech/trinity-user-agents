//! Keeps the stack's descriptor current while the core runs.
//!
//! [`StackRefresh::load`] resolves the stack the core starts on without
//! touching the network. [`StackRefresh::start`] then fetches the descriptor
//! through the host, in the background, whenever the copy is old enough. A
//! host that does not fetch it is still told the stack's status, through
//! [`StackRefresh::start_reporting`].

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
	ReservedValuesPolicy, Stack, StackError, StackFetchRequest, StackFetchResponse, StackRecords,
	StackStatus, signed_pointer,
	source::{Listing, StackSource},
};
use crate::{
	Spawner,
	background_task::BackgroundTask,
	chain_runtime::{GenesisCheck, GenesisHashes},
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
	/// The refresh loop, or the loop that only reports, once started.
	task: OnceLock<BackgroundTask>,
	/// Asks the refresh loop for a fetch now. That loop ends once this drops.
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
/// A chain whose genesis hash is not the descriptor's is reported in
/// [`StackStatus::mismatches`], and the refresh fetches the descriptor sooner.
/// A People chain whose `NetworkSuffix` is not the descriptor's `tld` is
/// reported there too, once this runtime has read the suffix.
#[derive(Clone)]
pub struct StackChecks {
	shared: Arc<Shared>,
}

impl StackChecks {
	/// The check a `ChainRuntime` reports each connection's genesis hash to.
	pub fn genesis_check(&self) -> Arc<dyn GenesisCheck> {
		self.shared.clone()
	}

	/// Takes People's `NetworkSuffix.NetworkSuffix` as read from the chain.
	///
	/// A suffix that is not the stack's `tld` is reported in
	/// [`StackStatus::mismatches`] until a later read gives the `tld` or the
	/// refresh drops with its runtime. The suffix is kept in memory only, so
	/// a runtime built again reports none until it reads one.
	pub fn people_suffix_answered(&self, answered: &[u8]) {
		let listing = self.shared.source().people_suffix_answered(answered);
		self.shared.answer_recorded(listing);
	}
}

impl GenesisCheck for Shared {
	fn chain_answered(&self, hashes: GenesisHashes) {
		let listing = self.source().chain_answered(hashes);
		self.answer_recorded(listing);
	}
}

impl StackRefresh {
	/// Resolves the stack a host build runs. `pointer` is the stack's link
	/// (`polkadotapp://stack?key=0x…&url=…`), `bundled` the signed copy the
	/// build ships, and `reserved_values` the build's policy on reserved
	/// values. The bundled copy, a stored one and every fetched one are held
	/// to the same policy, so a newer copy that names a reserved value the
	/// build does not allow is refused and the core keeps the copy it holds.
	///
	/// Starts from the copy `records` holds when it is newer than `bundled`,
	/// and never waits on the network. Fails with a [`StackError`] naming the
	/// rule the policy or `bundled` breaks, the policy's first. A record that
	/// cannot be read, or is not read within a few seconds, is logged and
	/// ignored.
	///
	/// `clock` tells how old a copy is and `timer` waits for the next fetch.
	pub async fn load(
		pointer: &str,
		bundled: &[u8],
		reserved_values: ReservedValuesPolicy,
		records: Box<dyn StackRecords>,
		timer: Arc<dyn Timer>,
		clock: Box<dyn Clock>,
	) -> Result<Self, StackError> {
		reserved_values.refuse_extras_that_reserve_nothing()?;
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
			reserved_values,
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

	/// The stack the core runs, with the checks of its chains.
	///
	/// It stays the same for the life of the runtime: a newer copy that
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
	/// Set-once, together with [`Self::start_reporting`]: answers whether this
	/// call started the refresh.
	#[must_use = "`false` means it had been started already and `host` was dropped"]
	pub fn start(&self, host: Box<dyn StackHost>, spawner: &Spawner) -> bool {
		let mut started = false;
		self.task.get_or_init(|| {
			started = true;
			let requests = self.refresh_requests.subscribe();
			BackgroundTask::spawn(spawner, self.shared.clone().run(host, requests))
		});
		started
	}

	/// Tells `changed` the stack's status, for a host that does not fetch it.
	///
	/// `changed` is called once before this returns, then from a task on
	/// `spawner` each time a chain's answer changes the status, until this
	/// value drops. It has to return promptly and must not call this or
	/// [`Self::start`]. Nothing is fetched.
	///
	/// Set-once, together with [`Self::start`]: answers whether this call
	/// started the reports.
	#[must_use = "`false` means it had been started already and `changed` was dropped"]
	pub fn start_reporting(
		&self,
		changed: Box<dyn Fn(&StackStatus) + Send>,
		spawner: &Spawner,
	) -> bool {
		let mut started = false;
		self.task.get_or_init(|| {
			started = true;
			let chain_answers = self.shared.chain_answers.subscribe();
			let reported = self.shared.source().status();
			changed(&reported);
			let reports = self.shared.clone().report(changed, reported, chain_answers);
			BackgroundTask::spawn(spawner, reports)
		});
		started
	}

	/// Fetches the descriptor now instead of when the copy is old enough.
	///
	/// A host asks when its app returns to the foreground, because a sleeping
	/// device can stretch the wait for the next fetch. Returns at once.
	/// Requests made while a fetch is out lead to one more fetch, not one
	/// each. Does nothing unless [`Self::start`] started the refresh.
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

	/// Logs an entry the status lists now and wakes the loop, unless the
	/// answer changed nothing. Call it with the source unlocked: a native host
	/// is handed each warning as it is made, and may read the status from
	/// there.
	fn answer_recorded(&self, listing: Listing) {
		match listing {
			Listing::Unchanged => return,
			Listing::Cleared => {},
			Listing::Listed(mismatch) => tracing::warn!("{mismatch}"),
		}
		self.chain_answers.wake();
	}

	async fn run(self: Arc<Self>, host: Box<dyn StackHost>, mut forced: mpsc::Receiver<()>) {
		let mut chain_answers = self.chain_answers.subscribe();
		let mut reported = self.source().status();
		let tell = |status: &StackStatus| host.stack_changed(status.clone());
		tell(&reported);
		let mut force = false;
		loop {
			// A chain's answer is reported before the fetch it brings forward,
			// and while a fetch is out: a host may take half a minute to
			// answer one.
			self.report_change(&tell, &mut reported);
			let request = if force {
				self.source().forced_fetch(self.now())
			} else {
				self.source().due_fetch(self.now())
			};
			if let Some(request) = request {
				let mut fetch = core::pin::pin!(self.fetch(host.as_ref(), request));
				while let Either::Right(_) = select(fetch.as_mut(), chain_answers.next()).await {
					self.report_change(&tell, &mut reported);
				}
				self.report_change(&tell, &mut reported);
			}
			let next_fetch_in = self.source().next_fetch_in(self.now());
			let until_due = match next_fetch_in {
				Some(wait) => self.timer.sleep(wait),
				None => pending().boxed(),
			};
			force = futures::select_biased! {
				() = until_due.fuse() => false,
				request = forced.next() => match request {
					Some(()) => true,
					None => return,
				},
				_ = chain_answers.next() => false,
			};
		}
	}

	/// Tells `changed` the status after each chain answer that makes it differ
	/// from `reported`, the status it was told last.
	async fn report(
		self: Arc<Self>,
		changed: Box<dyn Fn(&StackStatus) + Send>,
		mut reported: StackStatus,
		mut chain_answers: mpsc::Receiver<()>,
	) {
		while chain_answers.next().await.is_some() {
			self.report_change(changed.as_ref(), &mut reported);
		}
	}

	/// Tells the host the stack's status when it differs from `reported`.
	fn report_change(&self, tell: &dyn Fn(&StackStatus), reported: &mut StackStatus) {
		let status = self.source().status();
		if status != *reported {
			*reported = status;
			tell(reported);
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
