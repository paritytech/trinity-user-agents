//! [`DurableTxEngine`]: registration, status reads and the work that settles
//! every registered transaction.

mod pass;
mod recovery;
mod registration;
mod reporting;
mod tracker;
mod wake;

use std::sync::Arc;

use futures::future::AbortHandle;
use futures::stream::BoxStream;
use parking_lot::Mutex;
use subxt::utils::H256;

use super::dao;
use super::model::{
    DomainId, DurableTxEntry, DurableTxId, DurableTxState, DurableTxStatus, GroupId, Verdict,
};
use super::oracle::DurableRegistry;
use super::ownership::Ownership;
use super::time::Timer;
use crate::chain::{BlockBackend, ChainHeads, TxSubmitter, TxValidator};
use crate::store::{Db, DbError};
use crate::subscription::Spawner;
use wake::WakeSignal;

pub use recovery::RecoveryError;
pub use registration::{DurableRequest, EmptyRequest, RegistrationError};
pub use reporting::DurableWorkObserver;

/// What the engine runs on.
pub struct DurableDeps {
    /// The core database holding the ledger.
    pub db: Db,
    /// The oracle of every domain the engine serves.
    pub registry: DurableRegistry,
    /// Finalized and best heads.
    pub heads: Arc<dyn ChainHeads>,
    /// Block hashes, bodies and dispatch outcomes.
    pub blocks: Arc<dyn BlockBackend>,
    /// Pre-submission validation.
    pub validator: Arc<dyn TxValidator>,
    /// Submission and watching.
    pub submitter: Arc<dyn TxSubmitter>,
    /// Every wait the engine makes.
    pub timer: Arc<dyn Timer>,
    /// Runs registrations, submission watches and work reporting.
    pub spawner: Spawner,
}

/// Follows registered transactions to a verdict.
///
/// Registration writes ledger rows and hands each extrinsic to a submission
/// watch once they are committed. Whatever no watch owns, including everything
/// a previous process left live, is decided by recovery passes from the chain.
/// The engine never builds, signs or deletes anything.
pub struct DurableTxEngine {
    db: Db,
    registry: DurableRegistry,
    heads: Arc<dyn ChainHeads>,
    blocks: Arc<dyn BlockBackend>,
    validator: Arc<dyn TxValidator>,
    submitter: Arc<dyn TxSubmitter>,
    timer: Arc<dyn Timer>,
    spawner: Spawner,
    ownership: Arc<Ownership>,
    /// Wakes running recovery loops.
    recovery_wakes: WakeSignal,
    /// Asks the host for recovery when no loop is running to hear a wake.
    host_wakes: WakeSignal,
    pass_lock: futures::lock::Mutex<()>,
    reporter: Mutex<Option<AbortHandle>>,
}

impl DurableTxEngine {
    /// An engine over `deps`.
    pub fn new(deps: DurableDeps) -> Arc<Self> {
        Arc::new(Self {
            db: deps.db,
            registry: deps.registry,
            heads: deps.heads,
            blocks: deps.blocks,
            validator: deps.validator,
            submitter: deps.submitter,
            timer: deps.timer,
            spawner: deps.spawner,
            ownership: Arc::default(),
            recovery_wakes: WakeSignal::default(),
            host_wakes: WakeSignal::default(),
            pass_lock: futures::lock::Mutex::new(()),
            reporter: Mutex::new(None),
        })
    }

    /// The status of `id`, or `None` when no such transaction exists.
    pub async fn status(&self, id: DurableTxId) -> Result<Option<DurableTxStatus>, DbError> {
        self.db.read(move |conn| Ok(dao::status(conn, id)?)).await
    }

    /// [`Self::status`], again after every commit that changes it.
    pub fn observe_status(
        &self,
        id: DurableTxId,
    ) -> BoxStream<'static, Result<Option<DurableTxStatus>, DbError>> {
        dao::observe_status(&self.db, id)
    }

    /// The transactions of `group`, in registration order.
    pub async fn group(
        &self,
        domain: &DomainId,
        group: &GroupId,
    ) -> Result<Vec<DurableTxState>, DbError> {
        let (domain, group) = (domain.clone(), group.clone());
        self.db
            .read(move |conn| Ok(dao::group(conn, &domain, &group)?))
            .await
    }

    /// [`Self::group`], again after every commit that changes it.
    pub fn observe_group(
        &self,
        domain: &DomainId,
        group: &GroupId,
    ) -> BoxStream<'static, Result<Vec<DurableTxState>, DbError>> {
        dao::observe_group(&self.db, domain.clone(), group.clone())
    }

    /// Writes `verdict` only while `observed` is still the row's status and
    /// attempt. Returns whether it wrote.
    async fn write_verdict(
        &self,
        observed: &DurableTxEntry,
        verdict: Verdict,
    ) -> Result<bool, DbError> {
        let (observed, verdict) = (observed.clone(), verdict);
        self.db
            .write(move |tx| Ok(dao::compare_and_set(tx, &observed, &verdict)?))
            .await
    }

    /// Genesis hash of the chain `domain` lives on.
    fn chain_of(&self, domain: &DomainId) -> Option<H256> {
        self.registry.oracle(domain).map(|oracle| oracle.chain())
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use futures::executor::block_on;

    use super::*;
    use crate::chain::MortalExtrinsic;
    use crate::durable::testing::{FakeChain, extrinsic, test_engine};

    const DOMAIN: DomainId = DomainId::from_static("test");

    fn request(extrinsics: Vec<MortalExtrinsic>) -> DurableRequest {
        DurableRequest::presigned(DOMAIN, Some(GroupId::new("op")), extrinsics).unwrap()
    }

    #[test]
    fn status_reads_and_streams_follow_the_ledger() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);
        let id = block_on(engine.execute(request(vec![extrinsic(1, 100, 64)]))).unwrap()[0];
        let mut status = engine.observe_status(id);
        let mut group = engine.observe_group(&DOMAIN, &GroupId::new("op"));

        assert_eq!(
            (
                block_on(status.next()).unwrap().unwrap(),
                block_on(group.next()).unwrap().unwrap(),
                block_on(engine.status(id)).unwrap(),
                block_on(engine.status(DurableTxId(99))).unwrap(),
            ),
            (
                Some(DurableTxStatus::Pending),
                vec![DurableTxState {
                    id,
                    status: DurableTxStatus::Pending
                }],
                Some(DurableTxStatus::Pending),
                None
            )
        );
    }
}
