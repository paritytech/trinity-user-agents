//! [`DurableTxEngine`]: registration, status reads and the work that settles
//! every registered transaction.

mod pass;
mod recovery;
mod tracker;

use std::sync::Arc;

use futures::stream::BoxStream;
use subxt::utils::H256;

use super::dao;
use super::model::{
    DomainId, DurableTxEntry, DurableTxId, DurableTxState, DurableTxStatus, GroupId, Verdict,
};
use super::oracle::DurableRegistry;
use super::ownership::Ownership;
use super::time::Timer;
use crate::chain::{BlockBackend, ChainHeads, MortalExtrinsic, TxSubmitter, TxValidator};
use crate::store::{Db, DbError};
use crate::subscription::Spawner;
use recovery::Nudges;

pub use recovery::RecoveryError;

/// Told whether the engine has transactions still awaiting a verdict, so
/// the host can keep recovery running while it does.
pub trait DurableWorkObserver: Send + Sync {
    /// `true` while any transaction is live, `false` once none is. Reported
    /// only on change.
    fn durable_work_changed(&self, pending: bool);
}

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
    /// Runs submission watches.
    pub spawner: Spawner,
}

/// Follows registered transactions to a verdict.
///
/// Registration writes ledger rows and hands the bytes to a submission watch
/// once they are committed. Whatever no watch owns, including everything a
/// previous process left live, is decided by recovery passes from the chain.
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
    ownership: Ownership,
    nudges: Nudges,
    pass_lock: futures::lock::Mutex<()>,
}

/// What to register: presigned extrinsics of one domain, optionally under a
/// group.
pub struct DurableRequest {
    domain: DomainId,
    group: Option<GroupId>,
    extrinsics: Vec<MortalExtrinsic>,
}

/// A request named no extrinsic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a durable request needs at least one extrinsic")]
pub struct EmptyRequest;

impl DurableRequest {
    /// Presigned `extrinsics`, recorded as pending and broadcast once
    /// committed.
    pub fn presigned(
        domain: DomainId,
        group: Option<GroupId>,
        extrinsics: Vec<MortalExtrinsic>,
    ) -> Result<Self, EmptyRequest> {
        if extrinsics.is_empty() {
            return Err(EmptyRequest);
        }
        Ok(Self {
            domain,
            group,
            extrinsics,
        })
    }
}

/// Why a registration was not recorded. Nothing was written or broadcast.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    /// No oracle is registered for the domain, so nothing could decide its
    /// transactions.
    #[error("no durable domain {}", .0.as_str())]
    UnknownDomain(DomainId),
    /// The write failed or the caller's hook refused it.
    #[error(transparent)]
    Db(#[from] DbError),
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
            ownership: Ownership::default(),
            nudges: Nudges::default(),
            pass_lock: futures::lock::Mutex::new(()),
        })
    }

    /// Records `request` in its own write and broadcasts it once committed.
    pub async fn execute(
        self: &Arc<Self>,
        request: DurableRequest,
    ) -> Result<Vec<DurableTxId>, RegistrationError> {
        let (ids, ()) = self.execute_with(request, |_, _| Ok(())).await?;
        Ok(ids)
    }

    /// Records `request` and runs `on_register` with the new ids in the same
    /// write, so the domain's own rows commit or roll back with the ledger's.
    /// An error from `on_register` rolls everything back and broadcasts
    /// nothing.
    pub async fn execute_with<T, F>(
        self: &Arc<Self>,
        request: DurableRequest,
        on_register: F,
    ) -> Result<(Vec<DurableTxId>, T), RegistrationError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, DbError>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let DurableRequest {
            domain,
            group,
            extrinsics,
        } = request;
        let genesis = self
            .chain_of(&domain)
            .ok_or_else(|| RegistrationError::UnknownDomain(domain.clone()))?;
        let rows = extrinsics.clone();
        let (ids, value) = self
            .db
            .write(move |tx| {
                let ids = insert_all(tx, &domain, group.as_ref(), &rows)?;
                let value = on_register(tx, &ids)?;
                Ok((ids, value))
            })
            .await?;
        self.broadcast(genesis, &ids, extrinsics);
        Ok((ids, value))
    }

    /// Hands each committed extrinsic to its own submission watch.
    fn broadcast(
        self: &Arc<Self>,
        genesis: H256,
        ids: &[DurableTxId],
        extrinsics: Vec<MortalExtrinsic>,
    ) {
        for (id, extrinsic) in ids.iter().zip(extrinsics) {
            if self.ownership.acquire(*id, extrinsic.extrinsic.hash()) {
                self.spawn_watch(*id, genesis, extrinsic.extrinsic);
            }
        }
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

    /// Whether any transaction still awaits a verdict, again whenever that
    /// changes. While it is `true` something has to keep recovery running.
    pub fn live_work(&self) -> BoxStream<'static, Result<bool, DbError>> {
        dao::observe_has_live(&self.db)
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

/// Records every extrinsic as a pending row, in order.
fn insert_all(
    tx: &rusqlite::Transaction<'_>,
    domain: &DomainId,
    group: Option<&GroupId>,
    extrinsics: &[MortalExtrinsic],
) -> rusqlite::Result<Vec<DurableTxId>> {
    extrinsics
        .iter()
        .map(|extrinsic| dao::insert(tx, domain, group, extrinsic))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::StreamExt;
    use futures::executor::block_on;

    use super::*;
    use crate::durable::testing::{FakeChain, extrinsic, test_engine};
    use crate::test_support::wait_until;

    const DOMAIN: DomainId = DomainId::from_static("test");

    fn request(extrinsics: Vec<MortalExtrinsic>) -> DurableRequest {
        DurableRequest::presigned(DOMAIN, Some(GroupId::new("op")), extrinsics).unwrap()
    }

    #[test]
    fn a_request_with_no_extrinsic_cannot_be_built() {
        assert!(matches!(
            DurableRequest::presigned(DOMAIN, None, vec![]),
            Err(EmptyRequest)
        ));
    }

    /// iOS: `Ids come back in registration order and every one is owned by its submission`.
    #[test]
    fn registered_ids_come_back_in_order_and_are_owned_by_their_watch() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);

        let ids =
            block_on(engine.execute(request(vec![extrinsic(1, 100, 64), extrinsic(2, 100, 64)])))
                .unwrap();

        assert!(ids[0] < ids[1]);
        assert_eq!(
            block_on(engine.group(&DOMAIN, &GroupId::new("op"))).unwrap(),
            ids.iter()
                .map(|id| DurableTxState {
                    id: *id,
                    status: DurableTxStatus::Pending
                })
                .collect::<Vec<_>>()
        );
        assert!(ids.iter().all(|id| engine.ownership.is_owned(*id)));
    }

    /// The bytes go on the wire only after the rows that track them commit.
    #[test]
    fn committed_extrinsics_are_broadcast() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);

        block_on(engine.execute(request(vec![extrinsic(1, 100, 64)]))).unwrap();

        wait_until(
            || chain.state().submitted == vec![extrinsic(1, 100, 64).extrinsic],
            "the registered extrinsic is submitted",
        );
    }

    /// iOS: `The hook runs inside the transaction with the minted ids`.
    /// Android: LedgerAtomicityTest `aDomainsRowsJoinTheEnginesTransaction`.
    #[test]
    fn the_hook_writes_its_rows_in_the_same_transaction_with_the_new_ids() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);

        let (ids, hooked) = block_on(engine.execute_with(
            request(vec![extrinsic(1, 100, 64)]),
            |tx, ids| {
                tx.execute_batch("CREATE TABLE locks (tx_id INTEGER NOT NULL)")?;
                tx.execute("INSERT INTO locks (tx_id) VALUES (?1)", [ids[0].0])?;
                Ok(ids.to_vec())
            },
        ))
        .unwrap();

        let locked: i64 = block_on(
            engine
                .db
                .read(|conn| Ok(conn.query_row("SELECT tx_id FROM locks", [], |row| row.get(0))?)),
        )
        .unwrap();
        assert_eq!((hooked, locked), (ids.clone(), ids[0].0));
    }

    /// iOS: `A throwing hook rolls the whole batch back and takes no ownership`.
    /// Android: LedgerAtomicityTest `throwingAfterWritingDomainRowsRollsBackBoth`.
    #[test]
    fn a_failing_hook_rolls_back_every_row_and_broadcasts_nothing() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);

        let result = block_on(engine.execute_with(
            request(vec![extrinsic(1, 100, 64), extrinsic(2, 100, 64)]),
            |tx, _| -> Result<(), DbError> {
                tx.execute_batch("CREATE TABLE locks (tx_id INTEGER NOT NULL)")?;
                Err(DbError::Connection("domain refused".into()))
            },
        ));

        let tables: i64 = block_on(engine.db.read(|conn| {
            Ok(conn.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name = 'locks'",
                [],
                |row| row.get(0),
            )?)
        }))
        .unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(matches!(result, Err(RegistrationError::Db(_))));
        assert_eq!(
            (
                block_on(engine.group(&DOMAIN, &GroupId::new("op"))).unwrap(),
                tables,
                chain.state().submitted.len()
            ),
            (vec![], 0, 0)
        );
    }

    #[test]
    fn a_domain_without_an_oracle_cannot_register() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);
        let orphan = DurableRequest::presigned(
            DomainId::from_static("orphan"),
            None,
            vec![extrinsic(1, 100, 64)],
        )
        .unwrap();

        assert!(matches!(
            block_on(engine.execute(orphan)),
            Err(RegistrationError::UnknownDomain(domain)) if domain == DomainId::from_static("orphan")
        ));
    }

    #[test]
    fn status_reads_and_streams_follow_the_ledger() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);
        let mut live = engine.live_work();
        assert!(!block_on(live.next()).unwrap().unwrap());

        let id = block_on(engine.execute(request(vec![extrinsic(1, 100, 64)]))).unwrap()[0];
        let mut status = engine.observe_status(id);
        let mut group = engine.observe_group(&DOMAIN, &GroupId::new("op"));

        assert_eq!(
            (
                block_on(live.next()).unwrap().unwrap(),
                block_on(status.next()).unwrap().unwrap(),
                block_on(group.next()).unwrap().unwrap(),
                block_on(engine.status(id)).unwrap(),
                block_on(engine.status(DurableTxId(99))).unwrap(),
            ),
            (
                true,
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
