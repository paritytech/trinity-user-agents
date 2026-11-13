//! Tells the host whether the engine has work pending, so it keeps recovery
//! running while it does.

use futures::{
	future::{AbortHandle, Abortable, FutureExt, ready},
	stream::{self, BoxStream, StreamExt},
};
use tracing::warn;

use super::DurableTxEngine;
use crate::{durable::dao, store::DbError};

/// Told whether the engine has transactions still awaiting a verdict, so
/// the host can keep recovery running while it does.
pub trait DurableWorkObserver: Send + Sync {
	/// `true` while any transaction is live, `false` once none is. Reported
	/// on change, and `true` again whenever a transaction is handed back to
	/// recovery while none is running.
	fn durable_work_changed(&self, pending: bool);
}

impl DurableTxEngine {
	/// Whether any transaction of a registered domain still waits to be
	/// built or awaits a verdict, again whenever that changes. It also repeats `true` when a
	/// submission watch hands a transaction back while no recovery is running to take it: the
	/// ledger did not change, but somebody has to start recovery.
	pub fn live_work(&self) -> BoxStream<'static, Result<bool, DbError>> {
		let handed_back = self.host_wakes.subscribe().map(|()| Ok(true));
		let registry = self.registry.clone();
		let live = dao::observe_live_transaction_domains(&self.db)
			.map(move |domains| domains.map(|domains| registry.serves_any(&domains)));
		stream::select(live, handed_back).boxed()
	}

	/// Reports [`Self::live_work`] to `observer` until the engine is dropped.
	/// A second call replaces the first observer.
	pub fn report_work_to(&self, observer: impl Fn(bool) + Send + 'static) {
		let (abort, registration) = AbortHandle::new_pair();
		let reports = self.live_work().for_each(move |pending| {
			report(&observer, pending);
			ready(())
		});
		self.spawner.spawn(Abortable::new(reports, registration).map(|_| ()));
		if let Some(previous) = self.reporter.lock().replace(abort) {
			previous.abort();
		}
	}
}

impl Drop for DurableTxEngine {
	fn drop(&mut self) {
		if let Some(reporter) = self.reporter.get_mut().take() {
			reporter.abort();
		}
		self.executor.abort_all();
	}
}

fn report(observer: &impl Fn(bool), pending: Result<bool, DbError>) {
	match pending {
		Ok(pending) => observer(pending),
		Err(error) => warn!(%error, "durable work could not be read"),
	}
}

#[cfg(test)]
mod tests;
