//! Registration: records presigned extrinsics and hands each to a
//! submission watch once its row is committed.

use std::sync::Arc;

use futures::channel::oneshot;
use parking_lot::Mutex;
use subxt::utils::H256;

use super::DurableTxEngine;
use super::tracker::{Attempt, spawn_watch};
use crate::chain::MortalExtrinsic;
use crate::durable::dao;
use crate::durable::model::{DomainId, DurableTxId, GroupId};
use crate::durable::ownership::Ownership;
use crate::store::DbError;

/// What to register: presigned extrinsics of one domain, optionally under a
/// group.
pub struct DurableRequest {
    domain: DomainId,
    group: Option<GroupId>,
    extrinsics: Vec<MortalExtrinsic>,
}

/// A request named no extrinsic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a durable request needs at least one extrinsic, got none")]
pub struct EmptyRequest;

/// Why a registration was not recorded. Nothing was written or broadcast.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    /// No oracle is registered for the domain, so nothing could decide its
    /// transactions.
    #[error("no durable domain {}: register an oracle for it first", .0.as_str())]
    UnknownDomain(DomainId),
    /// The write failed or the caller's hook refused it.
    #[error(transparent)]
    Db(#[from] DbError),
}

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

impl DurableTxEngine {
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
    /// nothing. The registration finishes even when the caller stops
    /// waiting, because committed rows are useless without their broadcast.
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
        let genesis = self
            .chain_of(&request.domain)
            .ok_or_else(|| RegistrationError::UnknownDomain(request.domain.clone()))?;
        let (done, outcome) = oneshot::channel();
        let engine = self.clone();
        (self.spawner)(Box::pin(async move {
            let _ = done.send(engine.register(genesis, request, on_register).await);
        }));
        outcome
            .await
            .unwrap_or(Err(RegistrationError::Db(DbError::Closed)))
    }

    /// Records the rows, then broadcasts what was committed.
    async fn register<T, F>(
        self: Arc<Self>,
        genesis: H256,
        request: DurableRequest,
        on_register: F,
    ) -> Result<(Vec<DurableTxId>, T), RegistrationError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, DbError>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let extrinsics = request.extrinsics.clone();
        let (ids, value) = self.record(request, on_register).await?;
        // A watch only runs for an attempt this engine owns, so it never
        // races a recovery pass deciding the same row.
        for (id, extrinsic) in ids.iter().zip(extrinsics) {
            if self.ownership.is_owned(*id) {
                spawn_watch(
                    &self,
                    Attempt::new(*id, genesis, &extrinsic),
                    extrinsic.extrinsic,
                );
            }
        }
        Ok((ids, value))
    }

    /// Writes the rows and the hook's rows in one transaction, taking
    /// ownership of each attempt inside it so a recovery pass never reaches
    /// a committed row before its watch. A rolled-back write gives the
    /// ownership up again.
    async fn record<T, F>(
        &self,
        request: DurableRequest,
        on_register: F,
    ) -> Result<(Vec<DurableTxId>, T), DbError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>, &[DurableTxId]) -> Result<T, DbError>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let taken = Arc::new(Mutex::new(Vec::new()));
        let (ownership, claims) = (self.ownership.clone(), taken.clone());
        let written = self
            .db
            .write(move |tx| {
                let ids = insert_all(tx, &request)?;
                claims
                    .lock()
                    .extend(own(&ownership, &ids, &request.extrinsics));
                Ok((ids.clone(), on_register(tx, &ids)?))
            })
            .await;
        if written.is_err() {
            self.ownership.abandon_all(&taken.lock());
        }
        written
    }
}

/// Records every extrinsic of `request` as a pending row, in order.
fn insert_all(
    tx: &rusqlite::Transaction<'_>,
    request: &DurableRequest,
) -> rusqlite::Result<Vec<DurableTxId>> {
    request
        .extrinsics
        .iter()
        .map(|extrinsic| dao::insert(tx, &request.domain, request.group.as_ref(), extrinsic))
        .collect()
}

/// Takes ownership of each new row's attempt. Returns what was taken.
fn own(
    ownership: &Ownership,
    ids: &[DurableTxId],
    extrinsics: &[MortalExtrinsic],
) -> Vec<(DurableTxId, H256)> {
    ids.iter()
        .zip(extrinsics)
        .map(|(id, extrinsic)| (*id, extrinsic.extrinsic.hash()))
        .filter(|(id, hash)| ownership.acquire(*id, *hash))
        .collect()
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use futures::executor::block_on;

    use super::*;
    use crate::durable::model::{DurableTxState, DurableTxStatus};
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

    /// Each extrinsic is submitted only after the row that tracks it commits.
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

    /// A caller can stop waiting once the write is under way, as a cancelled
    /// FFI call does. The rows still commit, so their extrinsics must still
    /// be broadcast: nothing else holds the signed bytes.
    #[test]
    fn a_registration_whose_caller_gives_up_still_broadcasts() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);

        let mut registering = Box::pin(engine.execute(request(vec![extrinsic(1, 100, 64)])));
        assert!(registering.as_mut().now_or_never().is_none());
        drop(registering);

        wait_until(
            || chain.state().submitted.len() == 1,
            "the committed extrinsic is submitted",
        );
    }

    /// Android: ownership is taken in the registering transaction, so a
    /// committed row always has an owner and a pass never reaches it first.
    #[test]
    fn a_registered_transaction_is_owned_before_its_rows_commit() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);
        let probe = engine.clone();

        let (_, owned_in_write) = block_on(
            engine.execute_with(request(vec![extrinsic(1, 100, 64)]), move |_, ids| {
                Ok(probe.ownership.is_owned(ids[0]))
            }),
        )
        .unwrap();

        assert!(owned_in_write);
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
        assert!(matches!(result, Err(RegistrationError::Db(_))));
        assert_eq!(
            (
                block_on(engine.group(&DOMAIN, &GroupId::new("op"))).unwrap(),
                tables,
                chain.state().submitted.len(),
                engine.ownership.is_owned(DurableTxId(1)),
            ),
            (vec![], 0, 0, false)
        );
    }

    /// A rolled-back id is handed out again, so its ownership must be given
    /// up rather than marked released: the same bytes registered again under
    /// the reused id are still watched.
    #[test]
    fn the_same_extrinsic_registered_again_after_a_rollback_is_broadcast() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = test_engine(&chain);
        let refused = block_on(
            engine.execute_with(request(vec![extrinsic(1, 100, 64)]), |_, _| {
                Err::<(), _>(DbError::Connection("domain refused".into()))
            }),
        );
        assert!(refused.is_err());

        block_on(engine.execute(request(vec![extrinsic(1, 100, 64)]))).unwrap();

        wait_until(
            || chain.state().submitted.len() == 1,
            "the registered extrinsic is submitted",
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
}
