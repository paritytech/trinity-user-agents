//! Tells the host whether the engine has work pending, so it keeps recovery
//! running while it does.

use futures::future::{AbortHandle, Abortable, FutureExt, ready};
use futures::stream::{self, BoxStream, StreamExt};
use tracing::warn;

use super::DurableTxEngine;
use crate::durable::dao;
use crate::store::DbError;

/// Told whether the engine has transactions still awaiting a verdict, so
/// the host can keep recovery running while it does.
pub trait DurableWorkObserver: Send + Sync {
    /// `true` while any transaction is live, `false` once none is. Reported
    /// on change, and `true` again whenever a transaction is handed back to
    /// recovery while none is running.
    fn durable_work_changed(&self, pending: bool);
}

impl DurableTxEngine {
    /// Whether any transaction still awaits a verdict, again whenever that
    /// changes. It also repeats `true` when a submission watch hands a
    /// transaction back while no recovery is running to take it: the ledger
    /// did not change, but somebody has to start recovery.
    pub fn live_work(&self) -> BoxStream<'static, Result<bool, DbError>> {
        let handed_back = self.host_wakes.subscribe().map(|()| Ok(true));
        stream::select(dao::observe_has_live(&self.db), handed_back).boxed()
    }

    /// Reports [`Self::live_work`] to `observer` until the engine is dropped.
    /// A second call replaces the first observer.
    pub fn report_work_to(&self, observer: impl Fn(bool) + Send + 'static) {
        let (abort, registration) = AbortHandle::new_pair();
        let reports = self.live_work().for_each(move |pending| {
            report(&observer, pending);
            ready(())
        });
        (self.spawner)(Box::pin(Abortable::new(reports, registration).map(|_| ())));
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
    }
}

fn report(observer: &impl Fn(bool), pending: Result<bool, DbError>) {
    match pending {
        Ok(pending) => observer(pending),
        Err(error) => warn!(%error, "durable work could not be read"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::executor::block_on;
    use futures::{FutureExt, StreamExt};

    use crate::durable::engine::DurableRequest;
    use crate::durable::model::DomainId;
    use crate::durable::testing::{FakeChain, extrinsic, test_engine};
    use crate::test_support::wait_until;

    /// A watch that hands its transaction back while no recovery runs must
    /// ask the host for recovery again, although the ledger stayed live.
    #[test]
    fn a_watch_ending_with_no_recovery_running_asks_the_host_again() {
        let chain = FakeChain::new(130, 140);
        let events = chain.script_watch();
        let (_dir, engine, _timer) = test_engine(&chain);
        let mut live = engine.live_work();
        assert!(!block_on(live.next()).unwrap().unwrap());
        let request = DurableRequest::presigned(
            DomainId::from_static("test"),
            None,
            vec![extrinsic(1, 100, 64)],
        )
        .unwrap();
        block_on(engine.execute(request)).unwrap();
        assert!(block_on(live.next()).unwrap().unwrap());

        drop(events);

        wait_until(
            || matches!(live.next().now_or_never(), Some(Some(Ok(true)))),
            "the host is asked for recovery again",
        );
    }

    /// The reporter must not outlive the engine: it would keep the database
    /// and its connection threads alive after the runtime is gone.
    #[test]
    fn work_reporting_stops_when_the_engine_is_dropped() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);
        let reported = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let sink = reported.clone();
        engine.report_work_to(move |pending| sink.lock().push(pending));
        wait_until(
            || *reported.lock() == vec![false],
            "the current state is reported",
        );

        drop(engine);

        wait_until(
            || Arc::strong_count(&reported) == 1,
            "the reporter is dropped with the engine",
        );
    }
}
