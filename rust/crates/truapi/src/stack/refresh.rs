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
use trinity_stack::{Descriptor, FetchRequest, Pointer, Served};
use truapi::latest::GenericError;

use super::{
	ReservedValuesPolicy, Stack, StackError, StackFetchRequest, StackFetchResponse, StackRecords,
	StackStatus, signed_pointer,
	source::{FirstFetch, Listing, StackSource},
};
use crate::{
	BackgroundTask, Spawner,
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
/// past this the core starts from the bundled copy, or for an imported stack
/// fetches the descriptor.
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
		let stored = read_record(records.as_ref(), &pointer.stack_id(), timer.as_ref()).await;
		let source = StackSource::load(
			pointer,
			bundled,
			stored.as_deref(),
			reserved_values,
			unix_now(clock.as_ref()),
		)?;
		Ok(Self::running(source, records, timer, clock))
	}

	/// Resolves a stack its build ships no copy of, such as one a person
	/// imported or a developer passes to a tool. `pointer` is the stack's link,
	/// signed or not: a link without a key trusts whoever serves the
	/// descriptor and has no rollback protection: a copy with a lower
	/// `sequence` is taken, as RFC 1 says. One with the same `sequence` and
	/// other content is refused, as for a signed stack.
	///
	/// Starts from the copy `records` holds. Without one, it first fetches the
	/// descriptor through `host`, stores it, and starts from that. The first
	/// copy the core holds pins what later copies must keep, as a bundled copy
	/// does. Otherwise like [`Self::load`].
	///
	/// Fails with [`StackError::Unreachable`] when there is no stored copy and
	/// the fetch fails, and with the rule a copy breaks when the core cannot
	/// run it. A fetched copy that could not be stored is logged, and the next
	/// start fetches again.
	pub async fn import(
		pointer: &str,
		reserved_values: ReservedValuesPolicy,
		records: Box<dyn StackRecords>,
		host: &dyn StackHost,
		timer: Arc<dyn Timer>,
		clock: Box<dyn Clock>,
	) -> Result<Self, StackError> {
		reserved_values.refuse_extras_that_reserve_nothing()?;
		let pointer = Pointer::from_link(pointer).map_err(StackError::Pointer)?;
		let stack_id = pointer.stack_id();
		let stored = read_record(records.as_ref(), &stack_id, timer.as_ref()).await;
		let now = unix_now(clock.as_ref());
		let source = match StackSource::import(&pointer, stored.as_deref(), &reserved_values, now)?
		{
			Some(source) => source,
			None => {
				let first = FirstFetch::new(pointer, reserved_values, now);
				let response = ask(host, first.request().clone(), timer.as_ref()).await;
				let (mut source, record) =
					first.fetched(served(&response), unix_now(clock.as_ref()))?;
				let check = source.replace_check(&record);
				let write = records.write_stack_record(&stack_id, record, check);
				if let Err(reason) = within_store_timeout(write, timer.as_ref()).await {
					tracing::warn!("stack descriptor could not be stored: {reason}");
					source.record_not_stored(reason);
				}
				source
			},
		};
		Ok(Self::running(source, records, timer, clock))
	}

	fn running(
		source: StackSource,
		records: Box<dyn StackRecords>,
		timer: Arc<dyn Timer>,
		clock: Box<dyn Clock>,
	) -> Self {
		Self {
			shared: Arc::new(Shared {
				source: Mutex::new(source),
				records,
				timer,
				clock,
				chain_answers: WakeSignal::default(),
			}),
			task: OnceLock::new(),
			refresh_requests: WakeSignal::default(),
		}
	}

	/// The descriptor of the copy the core runs, for a host's own features
	/// such as its chain endpoints and services. A newer copy of the same
	/// stack, such as one with other RPC URLs, replaces it while the core
	/// runs; one that changes the stack waits for the next start, as
	/// [`Self::stack`] does.
	pub fn descriptor(&self) -> Descriptor {
		self.shared.source().descriptor().clone()
	}

	/// Whether the copy is old enough that the refresh would fetch the
	/// descriptor now: an hour old, or never fetched, as a bundled copy is.
	///
	/// A tool that runs one command at a time may fetch before it starts
	/// rather than in the background, since it may exit before a background
	/// fetch is stored. It asks this, and when it is due awaits
	/// [`Self::fetch_now`] through a host that gives up soon.
	pub fn fetch_is_due(&self) -> bool {
		self.shared.source().next_fetch_in(self.shared.now()) == Some(Duration::ZERO)
	}

	/// Fetches the descriptor through `host` now, whatever the copy's age, and
	/// returns the status once the answer is taken and any newer copy stored.
	///
	/// A host whose chain gave a different genesis hash from the descriptor's
	/// asks before it refuses the stack, since the operator may have published
	/// the chain's new one. A newer copy that changes the stack sets
	/// [`StackStatus::restart_required`]: load the stack again to run it. Does
	/// nothing while another fetch is out.
	pub async fn fetch_now(&self, host: &dyn StackHost) -> StackStatus {
		let request = self.shared.source().forced_fetch(self.shared.now());
		if let Some(request) = request {
			self.shared.fetch(host, request).await;
		}
		self.status()
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
			spawner.create_task(self.shared.clone().run(host, requests))
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
			spawner.create_task(reports)
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
		let response = ask(host, request, self.timer.as_ref()).await;
		let (stack_id, write) = {
			let mut source = self.source();
			let write = source.fetched(served(&response), self.now()).map(|record| {
				let check = source.replace_check(&record);
				(record, check)
			});
			(source.stack_id(), write)
		};
		let Some((record, check)) = write else { return };
		let write = self.records.write_stack_record(&stack_id, record, check);
		match within_store_timeout(write, self.timer.as_ref()).await {
			Ok(()) => self.source().record_stored(),
			Err(reason) => {
				tracing::warn!("stack descriptor could not be stored: {reason}");
				self.source().record_not_stored(reason);
			},
		}
	}
}

/// The record `records` holds for `stack_id`, or `None` when there is none,
/// it cannot be read, or it is not read within [`RECORD_READ_TIMEOUT`].
async fn read_record(
	records: &dyn StackRecords,
	stack_id: &str,
	timer: &dyn Timer,
) -> Option<Vec<u8>> {
	let read = records.read_stack_record(stack_id);
	// Started only if the read is still out, so a read that answers at once
	// leaves no wait behind.
	let read_timeout = Box::pin(async { timer.sleep(RECORD_READ_TIMEOUT).await });
	match select(read, read_timeout).await {
		Either::Left((Ok(stored), _)) => stored,
		Either::Left((Err(error), _)) => {
			tracing::warn!("stored stack descriptor could not be read: {error}");
			None
		},
		Either::Right(_) => {
			tracing::warn!(
				"stored stack descriptor not read within {}s, starting without it",
				RECORD_READ_TIMEOUT.as_secs()
			);
			None
		},
	}
}

/// Asks `host` for the descriptor `request` names, within [`FETCH_TIMEOUT`].
/// Fails with the host's reason, or the limit of the core's the answer broke.
async fn ask(
	host: &dyn StackHost,
	request: FetchRequest,
	timer: &dyn Timer,
) -> Result<StackFetchResponse, String> {
	let FetchRequest { url, if_none_match } = request;
	let request = StackFetchRequest { url, if_none_match, max_bytes: MAX_DESCRIPTOR_BYTES };
	let answer = select(host.fetch_stack_descriptor(request), timer.sleep(FETCH_TIMEOUT));
	let response = match answer.await {
		Either::Left((response, _)) => response.map_err(|error| error.reason),
		Either::Right(_) => Err(format!("no answer within {}s", FETCH_TIMEOUT.as_secs())),
	};
	let response = response.and_then(|response| match &response {
		StackFetchResponse::Document { bytes, .. }
			if u32::try_from(bytes.len()).map_or(true, |length| length > MAX_DESCRIPTOR_BYTES) =>
		{
			Err(format!("{} bytes is larger than {MAX_DESCRIPTOR_BYTES}", bytes.len()))
		},
		_ => Ok(response),
	});
	if let Err(reason) = &response {
		tracing::debug!("stack descriptor fetch failed: {reason}");
	}
	response
}

/// What `response` served, for the reader.
fn served(response: &Result<StackFetchResponse, String>) -> Result<Served<'_>, String> {
	match response {
		Ok(StackFetchResponse::Document { bytes, etag }) => {
			Ok(Served::Document { bytes, etag: etag.as_deref() })
		},
		Ok(StackFetchResponse::NotModified) => Ok(Served::NotModified),
		Err(reason) => Err(reason.clone()),
	}
}

/// `write`'s outcome, or a failure once [`STORE_TIMEOUT`] passes without one.
async fn within_store_timeout(
	write: impl Future<Output = Result<(), GenericError>>,
	timer: &dyn Timer,
) -> Result<(), String> {
	match select(core::pin::pin!(write), timer.sleep(STORE_TIMEOUT)).await {
		Either::Left((stored, _)) => stored.map_err(|error| error.reason),
		Either::Right(_) => Err(format!("not stored within {}s", STORE_TIMEOUT.as_secs())),
	}
}

fn unix_now(clock: &dyn Clock) -> i64 {
	i64::try_from(clock.unix_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests;
