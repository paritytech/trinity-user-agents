//! Keeps the stack's descriptor current while the core runs.
//!
//! [`StackRefresh::load`] resolves the stack the core starts on without
//! touching the network. [`StackRefresh::start`] then fetches the descriptor
//! through the host, in the background, whenever the copy is old enough.

use core::time::Duration;
use std::sync::{Arc, Mutex, OnceLock};

use futures::{
	StreamExt,
	channel::mpsc,
	future::{Either, select},
};
use trinity_stack::{FetchRequest, Served};

use super::{
	Stack, StackError, StackFetchRequest, StackFetchResponse, StackRecords, StackStatus,
	signed_pointer, source::StackSource,
};
use crate::{
	Spawner, background_task::BackgroundTask, time::Timer, unix_time::current_unix_secs,
	wake::WakeSignal,
};

/// The largest descriptor the core reads.
const MAX_DESCRIPTOR_BYTES: u32 = 1024 * 1024;

/// How long the host has to answer a fetch before it counts as unreachable.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the loop waits before it asks again whether a fetch is due.
const DUE_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// What a host does for the stack the core runs: fetch its descriptor, and
/// show what the core reports about it.
#[async_trait::async_trait]
pub trait StackHost: Send + Sync {
	/// Fetches the stack's descriptor over HTTP, as [`StackFetchRequest`]
	/// describes. Never fails: an unusable answer is
	/// [`StackFetchResponse::Unreachable`].
	async fn fetch_stack_descriptor(&self, request: StackFetchRequest) -> StackFetchResponse;

	/// The stack's status when the refresh starts, and again whenever it
	/// changes. Return promptly: the refresh waits on it.
	fn stack_changed(&self, status: StackStatus);
}

/// The stack a host build runs and the background refresh of its descriptor.
pub struct StackRefresh {
	shared: Arc<Shared>,
	/// The refresh loop, once started.
	task: OnceLock<BackgroundTask>,
}

struct Shared {
	source: Mutex<StackSource>,
	records: Box<dyn StackRecords>,
	timer: Arc<dyn Timer>,
	/// Asks the loop for a fetch now.
	refresh_requests: WakeSignal,
}

impl StackRefresh {
	/// Resolves the stack a host build runs. `pointer` is the stack's link
	/// (`polkadotapp://stack?key=0x…&url=…`), `bundled` the signed copy the
	/// build ships, and `allows_reserved_values` whether it may use Polkadot's
	/// genesis hashes and the `dot` TLD.
	///
	/// Starts from the copy `records` holds when it is newer than `bundled`,
	/// and never waits on the network. Fails with a [`StackError`] naming the
	/// rule `bundled` breaks. A record that cannot be read is logged and
	/// ignored.
	pub fn load(
		pointer: &str,
		bundled: &[u8],
		allows_reserved_values: bool,
		records: Box<dyn StackRecords>,
		timer: Arc<dyn Timer>,
	) -> Result<Self, StackError> {
		let pointer = signed_pointer(pointer)?;
		let stored = records.read_stack_record(&pointer.stack_id()).unwrap_or_else(|error| {
			tracing::warn!("stored stack descriptor could not be read: {error}");
			None
		});
		let source =
			StackSource::load(pointer, bundled, stored.as_deref(), allows_reserved_values, now())?;
		Ok(Self {
			shared: Arc::new(Shared {
				source: Mutex::new(source),
				records,
				timer,
				refresh_requests: WakeSignal::default(),
			}),
			task: OnceLock::new(),
		})
	}

	/// The stack the core runs. It stays the same for the life of the
	/// runtime: a newer copy that changes it applies at the next start.
	pub fn stack(&self) -> Stack {
		self.shared.source().stack().clone()
	}

	/// What the host is told about the stack.
	pub fn status(&self) -> StackStatus {
		self.shared.source().status()
	}

	/// Starts refreshing through `host`, on `spawner`, until this value
	/// drops. Reports the current status to `host` first.
	///
	/// Set-once: answers whether this call started the refresh.
	pub fn start(&self, host: Box<dyn StackHost>, spawner: &Spawner) -> bool {
		let mut started = false;
		self.task.get_or_init(|| {
			started = true;
			let requests = self.shared.refresh_requests.subscribe();
			BackgroundTask::spawn(spawner, self.shared.clone().run(host, requests))
		});
		started
	}

	/// Fetches the descriptor now instead of when the copy is old enough, as
	/// after a chain answers with another genesis hash. Returns at once.
	/// Requests made while a fetch is out lead to one more fetch, not one
	/// each. Does nothing before [`Self::start`].
	pub fn refresh_now(&self) {
		self.shared.refresh_requests.wake();
	}
}

impl Shared {
	fn source(&self) -> std::sync::MutexGuard<'_, StackSource> {
		self.source.lock().expect("stack source mutex poisoned")
	}

	async fn run(self: Arc<Self>, host: Box<dyn StackHost>, mut forced: mpsc::Receiver<()>) {
		let mut reported = self.source().status();
		host.stack_changed(reported.clone());
		let mut force = false;
		loop {
			let request = match force {
				true => self.source().forced_fetch(now()),
				false => self.source().due_fetch(now()),
			};
			if let Some(request) = request {
				self.fetch(host.as_ref(), request).await;
				let status = self.source().status();
				if status != reported {
					reported = status;
					host.stack_changed(reported.clone());
				}
			}
			force = match select(self.timer.sleep(DUE_CHECK_INTERVAL), forced.next()).await {
				Either::Left(_) => false,
				Either::Right((Some(()), _)) => true,
				Either::Right((None, _)) => return,
			};
		}
	}

	/// Makes one fetch through the host and stores the record it changed.
	async fn fetch(&self, host: &dyn StackHost, request: FetchRequest) {
		let FetchRequest { url, if_none_match } = request;
		let request = StackFetchRequest { url, if_none_match, max_bytes: MAX_DESCRIPTOR_BYTES };
		let answer = select(host.fetch_stack_descriptor(request), self.timer.sleep(FETCH_TIMEOUT));
		let response = match answer.await {
			Either::Left((response, _)) => response,
			Either::Right(_) => StackFetchResponse::Unreachable {
				reason: format!("no answer within {}s", FETCH_TIMEOUT.as_secs()),
			},
		};
		let served = match &response {
			StackFetchResponse::Document { bytes, etag } => match u32::try_from(bytes.len()) {
				Ok(length) if length <= MAX_DESCRIPTOR_BYTES => {
					Served::Document { bytes, etag: etag.as_deref() }
				},
				_ => {
					tracing::warn!(
						"stack descriptor fetch ignored: {} bytes is larger than \
						 {MAX_DESCRIPTOR_BYTES}",
						bytes.len()
					);
					Served::Unreachable
				},
			},
			StackFetchResponse::NotModified => Served::NotModified,
			StackFetchResponse::Unreachable { reason } => {
				tracing::debug!("stack descriptor fetch failed: {reason}");
				Served::Unreachable
			},
		};
		let (stack_id, record) = {
			let mut source = self.source();
			(source.stack_id(), source.fetched(served, now()))
		};
		let Some(record) = record else { return };
		if let Err(error) = self.records.write_stack_record(&stack_id, record).await {
			tracing::warn!("stack descriptor could not be stored: {error}");
		}
	}
}

fn now() -> i64 {
	i64::try_from(current_unix_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests;
